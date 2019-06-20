"""Contains functions for interacting with the Rust proxy server child"""

import signal
from subprocess import Popen, DEVNULL
import os

from django.conf import settings


CHILD_PID = None


def spawn_proxy_server():
    """ Spawns the proxy server and saves its PID so that we can communicate with it later """

    output_file_handle = open(settings.PROXY_SERVER_LOG_FILE, "w")
    handle = Popen(
        ["phost-proxy"], stdin=DEVNULL, stdout=output_file_handle, stderr=output_file_handle
    )

    global CHILD_PID
    CHILD_PID = handle.pid


def trigger_proxy_server_update():
    if CHILD_PID is None:
        print("Error: tried to trigger update of child proxy server before it's been spawned")
        return

    os.kill(CHILD_PID, signal.SIGUSR1)

