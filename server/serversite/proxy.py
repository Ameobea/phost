"""Contains functions for interacting with the Rust proxy server child"""

import signal
import subprocess
from subprocess import Popen, PIPE, DEVNULL
import os


CHILD_PID = None


def spawn_proxy_server():
    """ Spawns the proxy server and saves its PID """

    handle = Popen(["phost-proxy"], stdin=DEVNULL, stdout=PIPE, stderr=PIPE)

    global CHILD_PID
    CHILD_PID = handle.pid


def trigger_proxy_server_update():
    if CHILD_PID is None:
        print("Error: tried to trigger update of child proxy server before it's been spawned")
        return

    os.kill(CHILD_PID, signal.SIGUSR1)
