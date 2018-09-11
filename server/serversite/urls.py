from django.urls import path
from . import views


urlpatterns = [
    path("", views.index, name="index"),
    path("deployments/", views.Deployments.as_view(), name="deployments"),
    path("deployments/<uuid:deployment_id>/", views.get_deployment, name="deployment"),
    path(
        "deployments/<uuid:deployment_id>/<str:version>/",
        views.DeploymentVersionView.as_view(),
        name="deployment_version",
    ),
]
