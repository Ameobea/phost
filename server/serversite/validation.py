""" Functions for validating input from the user """

import re


class BadInputException(Exception):
    pass


class NotFound(Exception):
    pass


DEPLOYMENT_NAME_RGX = re.compile("^[^\\.]+$")


def validate_deployment_name(deployment_name: str):
    if not DEPLOYMENT_NAME_RGX.match(deployment_name):
        raise BadInputException("Supplied deployment name must not contain periods")


def get_validated_form(FormClass, request):
    form = FormClass(request.POST, request.FILES)
    if not form.is_valid():
        raise BadInputException("Invalid fields provided to the static deployment creation form")

    return form
