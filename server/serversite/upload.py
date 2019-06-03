"""
Utilities for dealing with uploaded deployment archives
"""

import tempfile
from subprocess import CalledProcessError
import os
import pathlib
import shutil
import tarfile

from django.conf import settings

from .validation import BadInputException

HOST_DIR = settings.HOST_PATH


def delete_dir_if_exists(dir_path: str):
    if os.path.exists(dir_path):
        shutil.rmtree(dir_path)


def delete_hosted_version(deployment_name: str, version: str):
    version_dir = os.path.join(HOST_DIR, deployment_name, version)
    delete_dir_if_exists(version_dir)


def delete_hosted_deployment(deployment_name: str):
    deployment_dir = os.path.join(HOST_DIR, deployment_name)
    delete_dir_if_exists(deployment_dir)


def update_symlink(deployment_name: str, new_version: str):
    """ Updates the directory that `latest` is symlinked to in the given host directory.  This
    should be called after a new version is pushed. """

    link_path = os.path.join(HOST_DIR, deployment_name, "latest")
    try:
        os.unlink(link_path)
    except FileNotFoundError:
        pass

    os.symlink(os.path.join(HOST_DIR, deployment_name, new_version), link_path)


def handle_uploaded_static_archive(file, subdomain: str, version: str, init=True) -> str:
    """
    Writes the archive to a temporary file and attempts to extract it to the project directory.
    Raises an exception if the extraction process was unsuccessful.
    """

    try:
        dst_dir = os.path.join(HOST_DIR, subdomain, version)
        pathlib.Path(dst_dir).mkdir(parents=True, exist_ok=True)
        if init:
            if version != "latest":
                os.symlink(dst_dir, os.path.join(HOST_DIR, subdomain, "latest"))

        # Extract the archive into the hosting directory
        t = tarfile.open(mode="r:*", fileobj=file)
        t.extractall(dst_dir)
        t.close()

        return dst_dir
    except Exception as e:
        print("Error while creating deployment from tar archive")
        print(e)

        directory_to_delete = os.path.join(HOST_DIR, subdomain) if init else dst_dir
        print(f"Deleting {directory_to_delete}...")
        shutil.rmtree(directory_to_delete)
        raise e
