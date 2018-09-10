"""
Utilities for dealing with uploaded deployment archives
"""

import tempfile
import subprocess
import os
import pathlib

from django.conf import settings


def handle_uploaded_static_archive(file, name: str, version: str) -> str:
    """
    Writes the archive to a temporary file and attempts to extract it to the project directory.
    Raises an exception if the extraction process was unsuccessful.
    """

    tf = tempfile.NamedTemporaryFile()
    for chunk in file.chunks():
        tf.write(chunk)

    host_dir = settings.HOST_PATH
    dst_dir = os.path.join(host_dir, name, version)
    pathlib.Path(dst_dir).mkdir(parents=True, exist_ok=False)

    # Extract the archive into the hosting directory
    temp_filename = tf.name
    subprocess.run(["tar", "-xzf", temp_filename, "-C", dst_dir]).check_returncode()

    return dst_dir
