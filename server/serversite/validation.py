""" Functions for validating input from the user """


class BadInputException(Exception):
    pass


class NotFound(Exception):
    pass


DEPLOYMENT_NAME_RGX = r"^[^\\.]+$"

DEPLOYMENT_SUBDOMAIN_RGX = r"^[^\\.\\w]+$"


def validate_deployment_name(deployment_name: str):
    if not DEPLOYMENT_NAME_RGX.match(deployment_name):
        raise BadInputException("Supplied deployment name must not contain periods")


def validate_subdomain(subdomain: str):
    if not DEPLOYMENT_SUBDOMAIN_RGX.match(subdomain):
        raise BadInputException("Supplied subdomain must not contain periods or whitespace")


def get_validated_form(FormClass, request):
    form = FormClass(request.POST, request.FILES)
    if not form.is_valid():
        raise BadInputException("Invalid fields provided to the static deployment creation form")

    return form
