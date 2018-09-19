from django import forms


class StaticDeploymentForm(forms.Form):
    name = forms.CharField(max_length=255)
    subdomain = forms.CharField(max_length=255)
    version = forms.CharField(max_length=32)
    file = forms.FileField(allow_empty_file=False)
    categories = forms.CharField()  # Comma-separated list of categories
