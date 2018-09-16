"""
Utilities for dealing with uploaded deployment archives
"""

import tempfile
import subprocess
from subprocess import CalledProcessError
import os
import pathlib
import shutil

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
        tf = tempfile.NamedTemporaryFile(suffix=".tgz")
        for chunk in file.chunks():
            print(len(chunk))
            tf.write(chunk)

        dst_dir = os.path.join(HOST_DIR, subdomain, version)
        pathlib.Path(dst_dir).mkdir(parents=True, exist_ok=True)
        if init:
            if version != "latest":
                os.symlink(dst_dir, os.path.join(HOST_DIR, subdomain, "latest"))

        # Extract the archive into the hosting directory
        temp_filename = tf.name
        subprocess.run(["tar", "-xzf", temp_filename, "-C", dst_dir]).check_returncode()
        tf.close()

        return dst_dir
    except CalledProcessError:
        raise BadInputException("Error while decompressing the provided archive .tgz file")
    except Exception as e:
        shutil.rmtree(dst_dir)
        raise e
