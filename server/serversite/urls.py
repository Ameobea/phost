from django.urls import path
from . import views


urlpatterns = [
    path("", views.index, name="index"),
    path("deployments/", views.Deployments.as_view(), name="deployments"),
    path("deployments/<str:deployment_id>/", views.Deployment.as_view(), name="deployment"),
    path(
        "deployments/<str:deployment_id>/<str:version>/",
        views.DeploymentVersionView.as_view(),
        name="deployment_version",
    ),
    path("login/", views.login_user, name="login"),
    path("404/", views.not_found),
]
