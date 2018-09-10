from django.shortcuts import render
from django.http import HttpResponse, JsonResponse
from .models import StaticDeployment


def index(_req):
    return HttpResponse("Site is up and running!  Try `GET /deployments`.")


def deployments(_req):
    deployments = StaticDeployment.objects.all()
    return JsonResponse(list(deployments), safe=False)
