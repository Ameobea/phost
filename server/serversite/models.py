from django.db import models
from django.conf import settings


class StaticDeployment(models.Model):
    id = models.AutoField(primary_key=True)
    name = models.CharField(max_length=255, unique=True)
    subdomain = models.SlugField(unique=True, max_length=255)
    created_on = models.DateTimeField(auto_now_add=True)

    def get_url(self) -> str:
        return "{}://{}.{}/".format(settings.PROTOCOL, self.subdomain, settings.ROOT_URL)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.name


class DeploymentVersion(models.Model):
    id = models.AutoField(primary_key=True)
    version = models.CharField(max_length=32)
    created_on = models.DateTimeField(auto_now_add=True)
    deployment = models.ForeignKey(StaticDeployment, on_delete=models.CASCADE)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.version
