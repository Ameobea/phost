""" Contains functions for compressing and uploaded directories to be served. """

import os
import tempfile
import subprocess


def compress_dir(dir_path: str):
    target_dir_path = os.path.join(os.getcwd(), dir_path)
    temp_file = tempfile.NamedTemporaryFile(suffix=".tgz")
    temp_filename = temp_file.name
    subprocess.run(["tar", "-czf", temp_filename, "-C", target_dir_path, "."]).check_returncode()
    return temp_file
