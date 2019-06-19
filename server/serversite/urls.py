from django.urls import path
from . import views, proxy


urlpatterns = [
    path("", views.index, name="index"),
    path("deployments/", views.Deployments.as_view(), name="deployments"),
    path("deployments/<str:deployment_id>/", views.Deployment.as_view(), name="deployment"),
    path(
        "deployments/<str:deployment_id>/<str:version>/",
        views.DeploymentVersionView.as_view(),
        name="deployment_version",
    ),
    path("proxy/<str:deployment_id>/", views.ProxyDeploymentView.as_view(), name="proxy"),
    path("proxy/", views.ProxyDeployments.as_view(), name="proxies"),
    path("login/", views.login_user, name="login"),
    path("404/", views.not_found),
]

# Initialize child proxy server process
proxy.spawn_proxy_server()
