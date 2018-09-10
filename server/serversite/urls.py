from django.urls import path
from . import views

urlpatterns = [
    path("", views.index, name="index"),
    path("deployments/", views.deployments, name="deployments"),
    path(
        "create_static_deployment/", views.create_static_deployment, name="create_static_deployment"
    ),
]
