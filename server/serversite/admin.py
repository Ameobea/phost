from django.contrib import admin

from serversite.models import StaticDeployment, DeploymentVersion, DeploymentCategory

admin.site.register(StaticDeployment)
admin.site.register(DeploymentVersion)
admin.site.register(DeploymentCategory)
