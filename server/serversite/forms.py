from django import forms


class StaticDeploymentForm(forms.Form):
    name = forms.CharField(max_length=255)
    subdomain = forms.CharField(max_length=255)
    version = forms.CharField(max_length=32)
    file = forms.FileField(allow_empty_file=False)
    categories = forms.CharField(required=False)  # Comma-separated list of categories
    not_found_document = forms.CharField(required=False)

class ProxyDeploymentForm(forms.Form):
    name = forms.CharField(max_length=255)
    subdomain = forms.CharField(max_length=255)
    use_cors_headers = forms.BooleanField(required=False)
    destination_address = forms.CharField(strip=True)
