from subprocess import CalledProcessError
import shutil

from django.shortcuts import render
from django.http import (
    HttpResponse,
    JsonResponse,
    HttpResponseNotAllowed,
    HttpResponseBadRequest,
    HttpResponseServerError,
)
from django.http.request import HttpRequest
from django.db import transaction
from django.db.utils import IntegrityError

from .models import StaticDeployment, DeploymentVersion
from .forms import StaticDeploymentForm
from .upload import handle_uploaded_static_archive


def index(_req):
    return HttpResponse("Site is up and running!  Try `GET /deployments`.")


def deployments(_req):
    deployments = StaticDeployment.objects.all()
    return JsonResponse(list(deployments), safe=False)


def create_static_deployment(request: HttpRequest):
    if not request.method == "POST":
        return HttpResponseNotAllowed(["POST"])

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
        return HttpResponseBadRequest("Error while decompressing the provided archive .tgz file")
    except Exception as e:
        print(e)
        return HttpResponseBadRequest(
            "The provided archive was not present, invalid, or there was a problem extracting it."
        )

    deployment_descriptor = None
    try:
        with transaction.atomic():
            deployment_descriptor = StaticDeployment(name=name, subdomain=subdomain)
            deployment_descriptor.save()
            version_model = DeploymentVersion(version=version, deployment=deployment_descriptor)
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
