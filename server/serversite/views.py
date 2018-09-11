from subprocess import CalledProcessError
import shutil

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
from .upload import handle_uploaded_static_archive
from .serialize import serialize_model, serialize_models


def index(_req):
    return HttpResponse("Site is up and running!  Try `GET /deployments`.")


def get_or_none(Model, **kwargs):
    try:
        return Model.objects.get(**kwargs)
    except Model.DoesNotExist:
        return None


@require_GET
def get_deployment(_req: HttpRequest, deployment_id=None):
    deployment = StaticDeployment.objects.get(id=deployment_id)
    if deployment is None:
        return HttpResponseNotFound()

    return serialize_model(deployment)


class DeploymentVersionView(TemplateView):
    def get(
        self, request: HttpRequest, *args, deployment_id=None, version=None, **kwargs
    ):  # pylint: disable=W0221
        deployment = get_or_none(StaticDeployment, id=deployment_id)
        if deployment is None:
            return HttpResponseNotFound()

        version = get_or_none(DeploymentVersion, deployment=deployment, version=version)
        if version is None:
            return HttpResponseNotFound()
        print(version)

        return serialize_model(version)

    def post(self, request: HttpRequest, *args, deployment_id=None, version=None, **kwargs):
        form = StaticDeploymentVersionForm(request.POST, request.FILES)
        if not form.is_valid():
            return HttpResponseBadRequest(
                "Invalid fields provided to the static deployment version creation form"
            )

        deployment = get_or_none(StaticDeployment, id=deployment_id)
        if deployment is None:
            return HttpResponseNotFound()

        host_dir = None
        try:
            host_dir = handle_uploaded_static_archive(
                request.FILES["file"], deployment.name, version
            )
        except CalledProcessError:
            return HttpResponseBadRequest(
                "Error while decompressing the provided archive .tgz file"
            )
        except Exception as e:
            print(e)
            return HttpResponseBadRequest(
                "The provided archive was missing, invalid, or there was a problem extracting it."
            )

        try:
            with transaction.atomic():
                # Set any old active deployment as inactive
                old_version = DeploymentVersion.objects.get(deployment=deployment, active=True)
                if old_version:
                    old_version.update(active=False)
                # Create the new version and set it active
                DeploymentVersion(version=version, deployment=deployment, active=True).save()
        except IntegrityError:
            # Delete the created host directory and return an error
            shutil.rmtree(host_dir)

            return HttpResponseServerError(
                "There was an error while inserting the static deployment into the catalogue"
            )

        return serialize_model(version)


class Deployments(TemplateView):
    def get(self, request: HttpRequest, *args, **kwargs):
        all_deployments = StaticDeployment.objects.all()
        return serialize_models(all_deployments)

    def post(self, request: HttpRequest, *args, **kwargs):
        form = StaticDeploymentForm(request.POST, request.FILES)
        if not form.is_valid():
            return HttpResponseBadRequest(
                "Invalid fields provided to the static deployment creation form"
            )

        host_dir = None
        name = form.cleaned_data["name"]
        subdomain = form.cleaned_data["subdomain"]
        version = form.cleaned_data["version"]
        try:
            host_dir = handle_uploaded_static_archive(request.FILES["file"], name, version)
        except CalledProcessError:
            return HttpResponseBadRequest(
                "Error while decompressing the provided archive .tgz file"
            )
        except Exception as e:
            print(e)
            return HttpResponseBadRequest(
                "The provided archive was missing, invalid, or there was a problem extracting it."
            )

        deployment_descriptor = None
        try:
            with transaction.atomic():
                # Create the new deployment descriptor
                deployment_descriptor = StaticDeployment(name=name, subdomain=subdomain)
                deployment_descriptor.save()
                # Create the new version and set it as active
                version_model = DeploymentVersion(
                    version=version, deployment=deployment_descriptor, active=True
                )
                version_model.save()
        except IntegrityError as e:
            # Delete the created host directory and return an error
            shutil.rmtree(host_dir)

            if "Duplicate entry" in str(e):
                return HttpResponseBadRequest("`name` and `subdomain` must be unique!")

            return HttpResponseServerError(
                "There was an error while inserting the static deployment into the catalogue"
            )

        return JsonResponse(
            {
                "name": name,
                "subdomain": subdomain,
                "version": version,
                "url": deployment_descriptor.get_url(),
            }
        )
