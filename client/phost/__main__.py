import logging
import traceback
import json
from typing import List

import npyscreen
import click
import requests
from requests.exceptions import ConnectionError as RequestsConnectionError
from terminaltables import SingleTable
import dateutil.parser

from .config import load_conf
from .upload import compress_dir
from .util import compose, slugify


def make_request(method: str, *args, json=None, multipart_data=None):
    (func, kwargs) = {
        "POST": (requests.post, {"json": json, "files": multipart_data}),
        "GET": (requests.get, {}),
        "PUT": (requests.put, {"json": json}),
        "PATCH": (requests.patch, {"json": json}),
        "DELETE": (requests.delete, {}),
    }[method.upper()]

    return func(*args, **kwargs)


class PhostServerError(Exception):
    pass


class GlobalAppState(object):
    """ What it says on the tin. """

    def __init__(self):
        self.conf = load_conf()

    def api_call(self, resource_path: str, method="GET", **kwargs):
        try:
            res = make_request(
                method, "{}/{}".format(self.conf["api_server_url"], resource_path), **kwargs
            )
            if res.status_code == 404:
                raise PhostServerError("Resource not found")
            elif res.status_code == 500:
                raise PhostServerError("Internal server error")
            elif res.status_code != 200:
                raise PhostServerError(
                    "Received {} response code when making request: {}".format(
                        res.status_code, res.text
                    )
                )

            return res.json()

        except Exception as e:
            show_stacktrace = True
            if isinstance(e, PhostServerError):
                show_stacktrace = False
            elif isinstance(e, RequestsConnectionError):
                e = Exception("Error while communicating with the server's API")
                show_stacktrace = False

            if show_stacktrace:
                traceback.print_exc()
            else:
                print("Error: {}".format(e))

            exit(1)


STATE = None


class PhostApp(npyscreen.NPSApp):
    def main(self):
        pass


def list_deployments():
    def process_versions(versions: List[dict]) -> (str, str):
        active_version = "None"
        versions_string = ""

        for v in sorted(versions, key=lambda version: version["created_on"]):
            if v["active"]:
                active_version = v["version"]

            if versions_string:
                versions_string += ", "
            versions_string += v["version"]

        return (active_version, versions_string)

    deployments = STATE.api_call("deployments/")
    table_headers = ["Name", "URL", "Creation Date", "Active Version", "All Versions"]
    table_data = map(
        lambda datum: [
            datum["name"],
            "{}://{}.{}/".format(
                STATE.conf["hosting_protocol"], datum["subdomain"], STATE.conf["hosting_base_url"]
            ),
            dateutil.parser.parse(datum["created_on"]).strftime("%Y-%m-%d"),
            *process_versions(datum["versions"]),
        ],
        deployments,
    )

    table = SingleTable([table_headers, *table_data])
    table.inner_column_border = False
    table.inner_footing_row_border = False
    table.inner_heading_row_border = False
    table.inner_row_border = False
    table.outer_border = False
    table.padding_left = 0
    table.padding_right = 3

    print(table.table)


def delete_deployment(query, lookup_field, version):
    if version is None:
        STATE.api_call(
            "deployments/{}/?lookupField={}".format(query, lookup_field), method="DELETE"
        )
    else:
        STATE.api_call(
            "deployments/{}/{}/?lookupField={}".format(query, version, lookup_field),
            method="DELETE",
        )

    print("Deployment {}successfully deleted".format("" if version is None else "version "))


@click.group()
def main():
    pass


@main.group("deployment")
def deployment():
    pass


with_query_lookup_decorators = compose(
    click.argument("query"),
    click.option(
        "--name", "lookup_field", flag_value="name", default=True, help="Look up deployment by name"
    ),
    click.option(
        "--id",
        "lookup_field",
        flag_value="id",
        default=False,
        help="Look up deployment by deployment UUID",
    ),
    click.option(
        "--subdomain",
        "lookup_field",
        flag_value="subdomain",
        default=False,
        help="Look up deployment by subdomain",
    ),
)


delete_deployment_decorators = compose(
    with_query_lookup_decorators,
    click.option(
        "--version",
        "-v",
        default=None,
        help=(
            "If supplied, only this version will be deleted.  "
            "If not supplied, all versions will be deleted."
        ),
    ),
)


@deployment.command("ls")
def list_deployments_deployment():
    list_deployments()


@main.command("ls")
def list_deployments_main():
    list_deployments()


@deployment.command("rm")
@delete_deployment_decorators
def delete_deployment_deployment(query, lookup_field, version):
    delete_deployment(query, lookup_field, version)


@main.command("rm")
@delete_deployment_decorators
def delete_deployment_main(query, lookup_field, version):
    delete_deployment(query, lookup_field, version)


def create_deployment(name, subdomain, directory, version):
    if not subdomain:
        subdomain = slugify(name)

    # Compress the target directory into a tempfile .tgz archive
    tgz_file = compress_dir(directory)

    multipart_data = {
        "name": ("", name),
        "subdomain": ("", subdomain),
        "file": ("directory.tgz", tgz_file),
        "version": ("", version),
    }

    res = STATE.api_call("deployments/", method="POST", multipart_data=multipart_data)
    print("Deployment successfully created: {}".format(res["url"]))


create_deployment_decorators = compose(
    click.argument("name"),
    click.argument("directory"),
    click.option(
        "--subdomain",
        "-s",
        default=None,
        help=(
            "The subdomain on which the deployment will be hosted.  If left off, the subdomain will be"
            " constructed from the deployment name."
        ),
    ),
    click.option("--version", "-v", default="0.1.0"),
)


@main.command("create")
@create_deployment_decorators
def create_deployment_main(name, subdomain, directory, version):
    create_deployment(name, subdomain, directory, version)


@deployment.command("create")
@create_deployment_decorators
def create_deployment_deployment(name, subdomain, directory, version):
    create_deployment(name, subdomain, directory, version)


with_update_deployment_decorators = compose(
    with_query_lookup_decorators, click.argument("version"), click.argument("directory")
)


def update_deployment(query, lookup_field, version, directory):
    """ Pushes a new version for an existing deployment """

    multipart_data = {"file": compress_dir(directory)}
    STATE.api_call(
        "deployments/{}/{}/?lookupField={}".format(query, version, lookup_field),
        multipart_data=multipart_data,
        method="POST",
    )


@deployment.command("update")
@with_update_deployment_decorators
def update_deployment_deployment(query, lookup_field, version, directory):
    update_deployment(query, lookup_field, version, directory)


@main.command("update")
@with_update_deployment_decorators
def update_deployment_main(query, lookup_field, version, directory):
    update_deployment(query, lookup_field, version, directory)


@with_query_lookup_decorators
@deployment.command("show")
def show_deployment(query, lookup_field):
    deployment = STATE.api_call("deployments/{}/?lookupField={}".format(query, lookup_field))
    print(json.dumps(deployment, indent=4))


logging.getLogger("requests").setLevel(logging.CRITICAL)
logging.getLogger("urllib3").setLevel(logging.CRITICAL)
STATE = GlobalAppState()

main()
