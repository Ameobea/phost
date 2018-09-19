import logging
import traceback
import json
from typing import List
import os
from time import sleep

import click
import requests
from requests.exceptions import ConnectionError as RequestsConnectionError
from terminaltables import SingleTable
import dateutil.parser

from .config import load_conf, load_cookies, save_cookies
from .upload import compress_dir
from .util import compose, slugify, create_random_subdomain


class PhostServerError(Exception):
    pass


class GlobalAppState(object):
    """ What it says on the tin. """

    def __init__(self, config_file):
        self.conf = load_conf(config_file)

        # Load cookies from previous session and construct a new session with them
        self.session = requests.Session()
        self.session.cookies.update(load_cookies())

    def __del__(self):
        """ Before we exit, save the cookies from this session so that they can be re-used next
        time that the application is run. """

        save_cookies(self.session.cookies.get_dict())

    def make_request(self, method: str, *args, json_body=None, form_data=None, multipart_data=None):
        (func, kwargs) = {
            "POST": (
                self.session.post,
                {"json": json_body, "files": multipart_data, "data": form_data},
            ),
            "GET": (self.session.get, {}),
            "PUT": (self.session.put, {"json": json_body}),
            "PATCH": (self.session.patch, {"json": json_body}),
            "DELETE": (self.session.delete, {}),
        }[method.upper()]

        return func(*args, **kwargs)

    def api_call(self, resource_path: str, method="GET", **kwargs):
        try:
            res = self.make_request(
                method, "{}/{}".format(self.conf["api_server_url"], resource_path), **kwargs
            )

            if res.status_code == 404:
                raise PhostServerError("Resource not found")
            elif res.status_code == 500:
                raise PhostServerError("Internal server error")
            elif res.status_code == 403:
                # Try to login and repeat the request if this isn't the login route
                if resource_path != "login/":
                    self.login()
                    sleep(0.2)
                    return self.api_call(resource_path, method=method, **kwargs)

                raise PhostServerError("Error logging in; invalid username/password?")
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

    def login(self):
        res = self.api_call(
            "login/",
            method="POST",
            form_data={"username": self.conf["username"], "password": self.conf["password"]},
        )

        if not res["success"]:
            print("Error logging into the server; invalid username/password?")
            exit(1)


STATE = None


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
    table_headers = ["Name", "URL", "Creation Date", "Active Version", "All Versions", "Categories"]
    table_data = map(
        lambda datum: [
            datum["name"],
            "{}://{}.{}/".format(
                STATE.conf["hosting_protocol"], datum["subdomain"], STATE.conf["hosting_base_url"]
            ),
            dateutil.parser.parse(datum["created_on"]).strftime("%Y-%m-%d"),
            *process_versions(datum["versions"]),
            ", ".join(datum["categories"]),
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
    req_path = (
        "deployments/{}/?lookupField={}".format(query, lookup_field)
        if version is None
        else "deployments/{}/{}/?lookupField={}".format(query, version, lookup_field)
    )
    STATE.api_call(req_path, method="DELETE")

    print("Deployment {}successfully deleted".format("" if version is None else "version "))


@click.group()
@click.option(
    "--config",
    "-c",
    type=click.File(encoding="utf-8"),
    default=os.path.expanduser("~/.phost/conf.toml"),
)
def main(config):
    global STATE  # pylint: disable=W0603
    STATE = GlobalAppState(config)


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


def create_deployment(name, subdomain, directory, version, random_subdomain, categories):
    if random_subdomain:
        if subdomain is None:
            subdomain = create_random_subdomain()
        else:
            print("Can't supply both `--random-subdomain` and an explicit subdomain")
            exit(1)
    elif not subdomain:
        subdomain = slugify(name)

    # Compress the target directory into a tempfile .tgz archive
    tgz_file = compress_dir(directory)

    multipart_data = {
        "name": ("", name),
        "subdomain": ("", subdomain),
        "file": ("directory.tgz", tgz_file),
        "version": ("", version),
        "categories": ("", ",".join(categories)),
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
            "The subdomain on which the deployment will be hosted.  If left off, the subdomain"
            " will be constructed from the deployment name."
        ),
    ),
    click.option("--version", "-v", default="0.1.0"),
    click.option(
        "--private",
        "-p",
        default=False,
        help="Private deployments have a randomized subdomain",
        is_flag=True,
    ),
    click.option(
        "--category",
        "-c",
        multiple=True,
        help=(
            "A string representing a category that this deployment should be added to."
            "  (Multiple may be provided)"
        ),
    ),
)


@main.command("create")
@create_deployment_decorators
def create_deployment_main(name, subdomain, directory, version, private, category):
    create_deployment(name, subdomain, directory, version, private, category)


@deployment.command("create")
@create_deployment_decorators
def create_deployment_deployment(name, subdomain, directory, version, private, category):
    create_deployment(name, subdomain, directory, version, private, category)


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

    print("Deployment successfully updated")


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
    deployment_data = STATE.api_call("deployments/{}/?lookupField={}".format(query, lookup_field))
    print(json.dumps(deployment_data, indent=4))


logging.getLogger("requests").setLevel(logging.CRITICAL)
logging.getLogger("urllib3").setLevel(logging.CRITICAL)

main()  # pylint: disable=E1120

