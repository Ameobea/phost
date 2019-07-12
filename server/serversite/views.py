import mimetypes
import os
import re
import traceback

from django.http import (
    HttpResponse,
    JsonResponse,
    HttpResponseBadRequest,
    HttpResponseServerError,
    HttpResponseNotFound,
    HttpResponseForbidden,
)
from django.conf import settings
from django.contrib.auth import authenticate, login
from django.db import transaction
from django.db.utils import IntegrityError
from django.utils.datastructures import MultiValueDictKeyError
from django.http.request import HttpRequest
from django.views.decorators.http import require_GET, require_POST
from django.views.generic import TemplateView

from .models import StaticDeployment, DeploymentVersion, DeploymentCategory, ProxyDeployment
from .forms import StaticDeploymentForm, ProxyDeploymentForm
from .upload import (
    handle_uploaded_static_archive,
    update_symlink,
    delete_hosted_deployment,
    delete_hosted_version,
)
from .serialize import serialize
from .validation import (
    BadInputException,
    validate_deployment_name,
    get_validated_form,
    NotFound,
    NotAuthenticated,
    InvalidCredentials,
    validate_subdomain,
)
from .proxy import trigger_proxy_server_update

# Used to get the name of the deployment into which a given URL points
REDIRECT_URL_RGX = re.compile("^/__HOSTED/([^/]+)/.*$")

# Taken from https://djangosnippets.org/snippets/101/
def send_data(path, filename=None, mimetype=None):

    if filename is None:
        filename = os.path.basename(path)

    if mimetype is None:
        mimetype, encoding = mimetypes.guess_type(filename)

    response = HttpResponse(content_type=mimetype)
    response.write(open(path, "rb").read())
    return response


def with_caught_exceptions(func):
    def wrapper(*args, **kwargs):
        try:
            return func(*args, **kwargs)
        except BadInputException as e:
            return HttpResponseBadRequest(str(e))
        except NotFound:
            return HttpResponseNotFound()
        except NotAuthenticated:
            return HttpResponseForbidden("You must be logged into access this view")
        except InvalidCredentials:
            return HttpResponseForbidden("Invalid username or password provided")
        except Exception as e:
            print("Uncaught error: {}".format(str(e)))
            traceback.print_exc()

            return HttpResponseServerError(
                "An unhandled error occured while processing the request"
            )

    return wrapper


def with_default_success(func):
    """ Decorator that returns a JSON success message if no errors occur during the request. """

    def wrapper(*args, **kwargs):
        func(*args, **kwargs)
        return JsonResponse({"success": True, "error": False})

    return wrapper


def with_login_required(func):
    """ Decorator that verifies that a user is logged in and returns a Forbidden status code if
    the requester is not. """

    def wrapper(router: TemplateView, req: HttpRequest, *args, **kwargs):
        if not req.user.is_authenticated:
            raise NotAuthenticated()

        return func(router, req, *args, **kwargs)

    return wrapper


@require_GET
def index(_req: HttpRequest):
    return HttpResponse("Site is up and running!  Try `GET /deployments`.")


@require_POST
@with_caught_exceptions
@with_default_success
def login_user(req: HttpRequest):
    username = None
    password = None
    try:
        username = req.POST["username"]
        password = req.POST["password"]
    except MultiValueDictKeyError:
        raise BadInputException("You must supply both a username and password")

    user = authenticate(req, username=username, password=password)

    if user is not None:
        login(req, user)
    else:
        raise InvalidCredentials("Invalid username or password")


def get_or_none(Model, do_raise=True, **kwargs):
    """ Lookups a model given some query parameters.  If a match is found, it is returned.
    Otherwise, either `None` is returned or a `NotFound` exception is raised depending on
    the value of `do_raise`. """

    try:
        return Model.objects.get(**kwargs)
    except Model.DoesNotExist:
        if do_raise:
            raise NotFound()
        else:
            return None


class Deployments(TemplateView):
    @with_caught_exceptions
    @with_login_required
    def get(self, request: HttpRequest):
        all_deployments = StaticDeployment.objects.prefetch_related(
            "deploymentversion_set", "categories"
        ).all()
        deployments_data = serialize(all_deployments, json=False)
        deployments_data_with_versions = [
            {
                **datum,
                "versions": serialize(deployment_model.deploymentversion_set.all(), json=False),
                "categories": serialize(deployment_model.categories.all(), json=False),
            }
            for (datum, deployment_model) in zip(deployments_data, all_deployments)
        ]

        return JsonResponse(deployments_data_with_versions, safe=False)

    @with_caught_exceptions
    @with_login_required
    def post(self, request: HttpRequest):
        form = get_validated_form(StaticDeploymentForm, request)

        deployment_name = form.cleaned_data["name"]
        subdomain = form.cleaned_data["subdomain"]
        version = form.cleaned_data["version"]
        categories = form.cleaned_data["categories"].split(",")
        not_found_document = form.cleaned_data["not_found_document"]
        validate_deployment_name(deployment_name)
        validate_subdomain(subdomain)

        deployment_descriptor = None
        try:
            with transaction.atomic():
                # Create the new deployment descriptor
                deployment_descriptor = StaticDeployment(
                    name=deployment_name, subdomain=subdomain, not_found_document=not_found_document
                )
                deployment_descriptor.save()

                # Create categories
                for category in categories:
                    (category_model, _) = DeploymentCategory.objects.get_or_create(
                        category=category
                    )
                    deployment_descriptor.categories.add(category_model)

                # Create the new version and set it as active
                version_model = DeploymentVersion(
                    version=version, deployment=deployment_descriptor, active=True
                )
                version_model.save()

                handle_uploaded_static_archive(request.FILES["file"], subdomain, version)
        except IntegrityError as e:
            if "Duplicate entry" in str(e):
                raise BadInputException("`name` and `subdomain` must be unique!")
            else:
                raise e

        return JsonResponse(
            {
                "name": deployment_name,
                "subdomain": subdomain,
                "version": version,
                "url": deployment_descriptor.get_url(),
            }
        )


def get_query_dict(query_string: str, req: HttpRequest) -> dict:
    lookup_field = req.GET.get("lookupField", "id")
    if lookup_field not in ["id", "subdomain", "name"]:
        raise BadInputException("The supplied `lookupField` was invalid")

    return {lookup_field: query_string}


class Deployment(TemplateView):
    @with_caught_exceptions
    def get(self, req: HttpRequest, deployment_id=None):
        query_dict = get_query_dict(deployment_id, req)
        deployment = get_or_none(StaticDeployment, **query_dict)
        versions = DeploymentVersion.objects.filter(deployment=deployment)
        active_version = next(v for v in versions if v.active)

        deployment_data = serialize(deployment, json=False)
        versions_data = serialize(versions, json=False)
        versions_list = list(map(lambda version_datum: version_datum["version"], versions_data))

        deployment_data = {
            **deployment_data,
            "versions": versions_list,
            "active_version": serialize(active_version, json=False)["version"],
        }

        return JsonResponse(deployment_data, safe=False)

    @with_caught_exceptions
    @with_login_required
    @with_default_success
    def delete(self, req: HttpRequest, deployment_id=None):
        with transaction.atomic():
            query_dict = get_query_dict(deployment_id, req)
            deployment = get_or_none(StaticDeployment, **query_dict)
            deployment_data = serialize(deployment, json=False)
            # This will also recursively delete all attached versions
            deployment.delete()

            delete_hosted_deployment(deployment_data["subdomain"])


class DeploymentVersionView(TemplateView):
    @with_caught_exceptions
    def get(
        self, req: HttpRequest, *args, deployment_id=None, version=None
    ):  # pylint: disable=W0221
        query_dict = get_query_dict(deployment_id, req)
        deployment = get_or_none(StaticDeployment, **query_dict)
        version_model = get_or_none(DeploymentVersion, deployment=deployment, version=version)
        return serialize(version_model)

    @with_caught_exceptions
    @with_login_required
    def post(self, req: HttpRequest, deployment_id=None, version=None):
        query_dict = get_query_dict(deployment_id, req)
        deployment = get_or_none(StaticDeployment, **query_dict)

        # Assert that the new version is unique among other versions for the same deployment
        if DeploymentVersion.objects.filter(deployment=deployment, version=version):
            raise BadInputException("The new version name must be unique.")

        version_model = None
        with transaction.atomic():
            # Set any old active deployment as inactive
            old_version = DeploymentVersion.objects.get(deployment=deployment, active=True)
            if old_version:
                old_version.active = False
                old_version.save()

            # Create the new version and set it active
            version_model = DeploymentVersion(version=version, deployment=deployment, active=True)
            version_model.save()

            deployment_data = serialize(deployment, json=False)

            # Extract the supplied archive into the hosting directory
            handle_uploaded_static_archive(
                req.FILES["file"], deployment_data["subdomain"], version, init=False
            )
            # Update the `latest` version to point to this new version
            update_symlink(deployment_data["subdomain"], version)

        return serialize(version_model)

    @with_caught_exceptions
    @with_login_required
    @with_default_success
    def delete(self, req: HttpRequest, deployment_id=None, version=None):
        with transaction.atomic():
            query_dict = get_query_dict(deployment_id, req)
            deployment = get_or_none(StaticDeployment, **query_dict)
            deployment_data = serialize(deployment, json=False)
            # Delete the entry for the deployment version from the database
            DeploymentVersion.objects.filter(deployment=deployment, version=version).delete()
            # If no deployment versions remain for the owning deployment, delete the deployment
            delete_deployment = False
            if not DeploymentVersion.objects.filter(deployment=deployment):
                delete_deployment = True
                deployment.delete()

            if delete_deployment:
                delete_hosted_deployment(deployment_data["subdomain"])
            else:
                delete_hosted_version(deployment_data["subdomain"], version)


@with_caught_exceptions
def not_found(req):
    # This environment variable is passed in from Apache
    redirect_url = req.META.get("REDIRECT_URL")

    if redirect_url is None:
        return HttpResponseNotFound()

    # Get the name of the deployment that this 404 applies to, if any
    match = REDIRECT_URL_RGX.match(redirect_url)
    if match is None:
        return HttpResponseNotFound()

    deployment_subdomain = match[1]

    # Check to see if there's a custom 404 handle for the given deployment
    deployment = get_or_none(StaticDeployment, subdomain=deployment_subdomain)
    not_found_document = deployment.not_found_document

    if not_found_document is None:
        return HttpResponseNotFound()

    if deployment is None:
        return HttpResponseNotFound()

    # Sandbox the retrieved pathname to be within the deployment's directory, preventing all kinds
    # of potentially nasty directory traversal stuff.
    deployment_dir_path = os.path.abspath(os.path.join(settings.HOST_PATH, deployment.subdomain))
    document_path = os.path.abspath(
        os.path.relpath(
            os.path.join(deployment_dir_path, "latest", not_found_document),
            start=not_found_document,
        )
    )
    common_prefix = os.path.commonprefix([deployment_dir_path, document_path])

    if common_prefix != deployment_dir_path:
        return HttpResponseBadRequest(
            (
                f"Invalid error document provided: {not_found_document}; "
                "must be relative to deployment."
            )
        )

    if not os.path.exists(document_path):
        return HttpResponseBadRequest(
            f"The specified 404 document {not_found_document} doesn't exist in this deployment."
        )

    # Since our way of serving this file loads it into memory, we block any files that are >128MB
    file_size = os.path.getsize(document_path)
    if file_size > 1024 * 1024 * 1024 * 128:
        return HttpResponseBadRequest(
            f"Custom not found document is {file_size} bytes, which is more than the 128MB limit."
        )

    return send_data(document_path)


class ProxyDeployments(TemplateView):
    @with_caught_exceptions
    @with_login_required
    def get(self, request: HttpRequest):
        all_proxy_deployments = ProxyDeployment.objects.all()
        return serialize(all_proxy_deployments)

    @with_caught_exceptions
    @with_login_required
    def post(self, request: HttpRequest):
        form = get_validated_form(ProxyDeploymentForm, request)

        name = form.cleaned_data["name"]
        subdomain = form.cleaned_data["subdomain"]
        use_cors_headers = form.cleaned_data["use_cors_headers"] or False
        validate_deployment_name(name)
        validate_subdomain(subdomain)

        proxy_deployment_descriptor = ProxyDeployment(
            name=name,
            subdomain=subdomain,
            destination_address=form.cleaned_data["destination_address"],
            use_cors_headers=use_cors_headers,
        )
        try:
            proxy_deployment_descriptor.save()
        except IntegrityError as e:
            if "Duplicate entry" in str(e):
                raise BadInputException("`name` and `subdomain` must be unique!")
            else:
                raise e

        trigger_proxy_server_update()

        return JsonResponse(
            {"name": name, "subdomain": subdomain, "url": proxy_deployment_descriptor.get_url()}
        )


class ProxyDeploymentView(TemplateView):
    @with_caught_exceptions
    def get(self, req: HttpRequest, deployment_id=None):
        query_dict = get_query_dict(deployment_id, req)
        deployment = get_or_none(StaticDeployment, **query_dict)

        return serialize(deployment)

    @with_caught_exceptions
    @with_login_required
    @with_default_success
    def delete(self, req: HttpRequest, deployment_id=None):
        query_dict = get_query_dict(deployment_id, req)
        proxy_deployment = get_or_none(ProxyDeployment, **query_dict)

        proxy_deployment.delete()

        trigger_proxy_server_update()
