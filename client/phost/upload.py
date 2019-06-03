""" Contains functions for compressing and uploaded directories to be served. """

import os
import tarfile
import tempfile


def compress_dir(dir_path: str):
    target_dir_path = os.path.join(os.getcwd(), dir_path)
    temp_file = tempfile.NamedTemporaryFile(suffix=".tgz")
    temp_filename = temp_file.name

    with tarfile.open(temp_filename, mode="w:bz2") as out:
        out.add(target_dir_path)

    return temp_file
