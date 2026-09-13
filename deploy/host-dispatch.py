#!/usr/bin/env python3
import getpass
import os

roles = {
    "delivery-status": "status",
    "delivery-submit": "submit",
    "delivery-replica": "replica",
}
role = roles.get(getpass.getuser())
command = os.environ.get("SSH_ORIGINAL_COMMAND", "")
if not role or not command or "\x00" in command or len(command) > 4096:
    raise SystemExit(64)
os.execv("/usr/bin/sudo", ["sudo", "-n", "/usr/local/libexec/ronitnath-host-entry", role, command])
