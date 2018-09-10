from django.db import models
from django.template.defaultfilters import slugify
from django.conf import settings


class StaticDeployment(models.Model):
    name = models.CharField(max_length=255, unique=True)
    subdomain = models.SlugField(unique=True, max_length=64)
    created_on = models.DateTimeField(auto_now_add=True)

    def get_url(self) -> str:
        "{}://{}.{}/".format(settings.PROTOCOL, self.subdomain, settings.ROOT_URL)

    def save(self, *args, **kwargs):
        if not self.subdomain:
            self.subdomain = slugify(self.name)
            super(StaticDeployment, self).save(*args, **kwargs)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.name


class DeploymentVersion(models.Model):
    version = models.CharField(max_length=32)
    created_on = models.DateTimeField(auto_now_add=True)
    deployment = models.ForeignKey(StaticDeployment, on_delete=models.CASCADE)

    class Meta:
        ordering = ["created_on"]

        def __unicode__(self):
            return self.version
