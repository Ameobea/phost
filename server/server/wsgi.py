"""
WSGI config for server project.

It exposes the WSGI callable as a module-level variable named ``application``.

For more information on this file, see
https://docs.djangoproject.com/en/2.1/howto/deployment/wsgi/
"""

import os

from django.core.wsgi import get_wsgi_application

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "server.settings")

inner_application = get_wsgi_application()


def application(environ, start_response):
    # Pass all environment variables from Apache into the WSGI application/Django
    for (k, v) in os.environ.items():
        environ[k] = v

    return inner_application(environ, start_response)
