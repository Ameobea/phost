""" Functions for validating input from the user """


import re


class BadInputException(Exception):
    pass


class NotFound(Exception):
    pass


# We could technically allow special characters, but that makes slugification much harder and
# just isn't worth it.
DEPLOYMENT_NAME_RGX = re.compile("^[a-zA-Z0-9-_ ]+$")

# Stolen from https://stackoverflow.com/a/7933253/3833068
DEPLOYMENT_SUBDOMAIN_RGX = re.compile("^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$")


def validate_deployment_name(deployment_name: str):
    if not DEPLOYMENT_NAME_RGX.match(deployment_name):
        raise BadInputException(
            "Deployment name must contain only alphanumeric characters, spaces, dashes, and underscores."
        )


def validate_subdomain(subdomain: str):
    if not DEPLOYMENT_SUBDOMAIN_RGX.match(subdomain):
        raise BadInputException("Supplied subdomain is invalid")


def get_validated_form(FormClass, request):
    form = FormClass(request.POST, request.FILES)
    if not form.is_valid():
        raise BadInputException("Invalid fields provided to the static deployment creation form")

    return form
