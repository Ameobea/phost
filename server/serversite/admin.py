from django.contrib import admin

from serversite.models import StaticDeployment, DeploymentVersion

admin.site.register(StaticDeployment)
admin.site.register(DeploymentVersion)
