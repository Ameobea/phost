import logging

import npyscreen
import click
import requests
from requests.exceptions import ConnectionError as RequestsConnectionError
from terminaltables import SingleTable
import dateutil.parser

from .config import load_conf
from .upload import compress_dir
from .util import compose


def make_request(method: str, *args, json=None, multipart_data=None):
    (func, kwargs) = {
        "POST": (requests.post, {"json": json, "files": multipart_data}),
        "GET": (requests.get, {}),
        "PUT": (requests.put, {"json": json}),
        "PATCH": (requests.patch, {"json": json}),
        "DELETE": (requests.delete, {}),
    }[method.upper()]

    return func(*args, **kwargs)


class GlobalAppState(object):
    """ What it says on the tin. """

    def __init__(self):
        self.conf = load_conf()

    def api_call(self, resource_path: str, method="GET", **kwargs):
        try:
            res = make_request(
                method, "{}/{}".format(self.conf["api_server_url"], resource_path), **kwargs
            )
            if res.status_code != 200:
                raise Exception(
                    "Received {} response code when making request: {}".format(
                        res.status_code, res.text
                    )
                )

            return res.json()

        except RequestsConnectionError:
            raise Exception("Error while communicating with the server's API")


STATE = None


class PhostApp(npyscreen.NPSApp):
    def main(self):
        pass


def list_deployments():
    deployments = STATE.api_call("deployments/")
    table_headers = ["Name", "URL", "Creation Date"]
    table_data = map(
        lambda datum: [
            datum["name"],
            "{}://{}.{}/".format(
                STATE.conf["hosting_protocol"], datum["subdomain"], STATE.conf["hosting_base_url"]
            ),
            dateutil.parser.parse(datum["created_on"]).strftime("%Y-%m-%d"),
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


@click.group()
def main():
    pass


@main.group("deployment")
def deployment():
    pass


delete_deployment_decorators = compose(
    click.argument("name"),
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


@deployment.command("list")
def list_deployments_deployment():
    list_deployments()


@main.command("list")
def list_deployments_main():
    list_deployments()


@deployment.command("delete")
@delete_deployment_decorators
def delete_deployment_deployment(name, version):
    pass  # TODO


@main.command("delete")
@delete_deployment_decorators
def delete_deployment_main(name, version):
    pass  # TODO


@click.argument("name")
@click.argument("subdomain")
@click.argument("directory")
@click.option("--version", "-v", default="0.1.0")
def create(name, subdomain, directory, version):
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


logging.getLogger("requests").setLevel(logging.CRITICAL)
logging.getLogger("urllib3").setLevel(logging.CRITICAL)
STATE = GlobalAppState()

main()
