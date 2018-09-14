from django.http import (
    HttpResponse,
    JsonResponse,
    HttpResponseBadRequest,
    HttpResponseServerError,
    HttpResponseNotFound,
)
from django.http.request import HttpRequest
from django.db import transaction
from django.db.utils import IntegrityError
from django.views.decorators.http import require_GET
from django.views.generic import TemplateView

from .models import StaticDeployment, DeploymentVersion
from .forms import StaticDeploymentForm, StaticDeploymentVersionForm
from .upload import (
    handle_uploaded_static_archive,
    update_symlink,
    delete_hosted_deployment,
    delete_hosted_version,
)
from .serialize import serialize
from .validation import BadInputException, validate_deployment_name, get_validated_form, NotFound


def with_caught_exceptions(func):
    def wrapper(*args, **kwargs):
        try:
            return func(*args, **kwargs)
        except BadInputException as e:
            return HttpResponseBadRequest(str(e))
        except NotFound:
            return HttpResponseNotFound()
        except Exception as e:
            print("Uncaught error: {}".format(str(e)))
            return HttpResponseServerError("An unhandled error occured while handling the request")

    return wrapper


@require_GET
def index(_req: HttpRequest):
    return HttpResponse("Site is up and running!  Try `GET /deployments`.")


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


class Deployment(TemplateView):
    @with_caught_exceptions
    def get(self, _req: HttpRequest, deployment_id=None):
        deployment = get_or_none(StaticDeployment, id=deployment_id)
        versions = DeploymentVersion.objects.filter(deployment=deployment)

        deployment_data = serialize(deployment, json=False)
        versions_data = serialize(versions, json=False)
        versions_list = list(map(lambda version_datum: version_datum["version"], versions_data))
        deployment_data["versions"] = versions_list

        return JsonResponse(deployment_data, safe=False)

    @with_caught_exceptions
    def delete(self, request: HttpRequest, deployment_id=None):
        with transaction.atomic():
            deployment = get_or_none(StaticDeployment, id=deployment_id)
            deployment_data = serialize(deployment, json=False)
            # This will also recursively delete all attached versions
            deployment.delete()

            delete_hosted_deployment(deployment_data["name"])

        return HttpResponse("Deployment successfully deleted")


class Deployments(TemplateView):
    @with_caught_exceptions
    def get(self, request: HttpRequest):
        all_deployments = StaticDeployment.objects.all()
        return serialize(all_deployments)

    @with_caught_exceptions
    def post(self, request: HttpRequest):
        form = get_validated_form(StaticDeploymentForm, request)

        deployment_name = form.cleaned_data["name"]
        subdomain = form.cleaned_data["subdomain"]
        version = form.cleaned_data["version"]
        validate_deployment_name(deployment_name)

        deployment_descriptor = None
        try:
            with transaction.atomic():
                # Create the new deployment descriptor
                deployment_descriptor = StaticDeployment(name=deployment_name, subdomain=subdomain)
                deployment_descriptor.save()

                # Create the new version and set it as active
                version_model = DeploymentVersion(
                    version=version, deployment=deployment_descriptor, active=True
                )
                version_model.save()

                handle_uploaded_static_archive(request.FILES["file"], deployment_name, version)
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


class DeploymentVersionView(TemplateView):
    @with_caught_exceptions
    def get(
        self, request: HttpRequest, *args, deployment_id=None, version=None
    ):  # pylint: disable=W0221
        deployment = get_or_none(StaticDeployment, id=deployment_id)
        version = get_or_none(DeploymentVersion, deployment=deployment, version=version)
        return serialize(version)

    @with_caught_exceptions
    def post(self, request: HttpRequest, *args, deployment_id=None, version=None):
        get_validated_form(StaticDeploymentVersionForm, request)
        deployment = get_or_none(StaticDeployment, id=deployment_id)

        with transaction.atomic():
            # Set any old active deployment as inactive
            old_version = DeploymentVersion.objects.get(deployment=deployment, active=True)
            if old_version:
                old_version.update(active=False)

            # Create the new version and set it active
            DeploymentVersion(version=version, deployment=deployment, active=True).save()

            deployment_data = serialize(deployment, json=False)

            # Extract the supplied archive into the hosting directory
            handle_uploaded_static_archive(request.FILES["file"], deployment_data["name"], version)
            # Update the `latest` version to point to this new version
            update_symlink(deployment_data["name"], version)

        return serialize(version)

    @with_caught_exceptions
    def delete(self, request: HttpRequest, deployment_id=None, version=None):
        with transaction.atomic():
            deployment = get_or_none(StaticDeployment, id=deployment_id)
            # Delete the entry for the deployment version from the database
            DeploymentVersion.objects.filter(deployment=deployment, version=version).delete()
            # If no deployment versions remain for the owning deployment, delete the deployment
            delete_deployment = False
            if not DeploymentVersion.filter(deployment=deployment):
                delete_deployment = True
                deployment.delete()

            if delete_deployment:
                delete_hosted_deployment(deployment)
            else:
                deployment_data = serialize(deployment, json=False)
                delete_hosted_version(deployment_data["name"], version)

        return HttpResponse("Deployment version successfully deleted")
