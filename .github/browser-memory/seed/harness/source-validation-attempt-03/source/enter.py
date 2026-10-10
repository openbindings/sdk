#!/usr/bin/python3
"""Unprivileged wrapper: request enrollment BEFORE exec of browser, deny privilege gain."""
import ctypes
import json
import os
import socket
import sys

# Linux PR_SET_NO_NEW_PRIVS. Inherited by every browser descendant; sudo/setuid
# cannot acquire privilege to migrate out of root-owned cgroups.
libc = ctypes.CDLL(None, use_errno=True)
if libc.prctl(38, 1, 0, 0, 0) != 0:
    raise OSError(ctypes.get_errno(), "PR_SET_NO_NEW_PRIVS")
with socket.socket(socket.AF_UNIX) as sock:
    sock.settimeout(10)
    sock.connect(os.environ["OB_MEMORY_SOCKET"])
    sock.sendall((json.dumps({"token": os.environ["OB_MEMORY_TOKEN"]}) + "\n").encode())
    reply = sock.makefile("rb").readline(4096)
    answer = json.loads(reply)
    if answer.get("status") != "enrolled":
        raise RuntimeError("cgroup enrollment refused")
    if open("/proc/self/cgroup").read() != answer["membership"]:
        raise RuntimeError("membership changed before browser exec")
executable = os.environ.pop("OB_MEMORY_EXECUTABLE")
for key in ("OB_MEMORY_SOCKET", "OB_MEMORY_TOKEN"):
    os.environ.pop(key)
os.execv(executable, [executable, *sys.argv[1:]])
