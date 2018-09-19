import uuid

from django.db import models
from django.conf import settings


class DeploymentCategory(models.Model):
    category = models.CharField(max_length=64, unique=True)


class StaticDeployment(models.Model):
    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    name = models.CharField(max_length=255, unique=True)
    subdomain = models.SlugField(unique=True, max_length=255)
    created_on = models.DateTimeField(auto_now_add=True)
    categories = models.ManyToManyField(DeploymentCategory)

    def get_url(self) -> str:
        return "{}://{}.{}/".format(settings.PROTOCOL, self.subdomain, settings.ROOT_URL)

    def save(self, *args, **kwargs):  # pylint: disable=W0221
        self.subdomain = self.subdomain.lower()
        return super(StaticDeployment, self).save(*args, **kwargs)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.name  # pylint: disable=E1101


class DeploymentVersion(models.Model):
    version = models.CharField(max_length=32)
    created_on = models.DateTimeField(auto_now_add=True)
    deployment = models.ForeignKey(StaticDeployment, on_delete=models.CASCADE)
    active = models.BooleanField(default=False)

    def save(self, *args, **kwargs):  # pylint: disable=W0221
        self.version = self.version.lower()
        return super(DeploymentVersion, self).save(*args, **kwargs)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.version  # pylint: disable=E1101
