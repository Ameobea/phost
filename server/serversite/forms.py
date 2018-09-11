from django import forms


class StaticDeploymentForm(forms.Form):
    name = forms.CharField(max_length=255)
    subdomain = forms.CharField(max_length=255)
    version = forms.CharField(max_length=32)
    file = forms.FileField(allow_empty_file=False)


class StaticDeploymentVersionForm(forms.Form):
    version = forms.CharField(max_length=32)
    file = forms.FileField(allow_empty_file=False)
