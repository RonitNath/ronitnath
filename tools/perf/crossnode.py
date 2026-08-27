#!/usr/bin/env python3
"""Finding 2, as one repeatable measurement.

Register on one node and command another with nothing in between, and count
how often the second node declines a request that is perfectly well formed. A
command reads its preconditions from the node's own state machine, so a node
that has not applied the `register` yet answers "declined" — to the cookie it
has never seen, and to the person the document would belong to.

Two held-open sockets rather than two `curl` invocations, because the race is
measured in milliseconds and spawning a process is not. The offset the
register landed at is read out of `x-rn-offset` and sent back as `x-rn-after`,
which a build that honours it waits on and a build that does not ignores — so
the same script measures both, and the difference is the finding.

    tools/perf/crossnode.py --rounds 50 --label 2026-08-27-f5
    tools/perf/crossnode.py --no-after      send no offset at all

The cluster must already be up (`tools/cluster.sh start`).
"""

import argparse
import os
import socket
import subprocess
import sys
import time
import urllib.parse
import uuid


class Conn:
    """One keep-alive HTTP/1.1 connection to one node."""

    def __init__(self, host):
        self.host = host
        self.sock = None

    def _open(self):
        self.sock = socket.create_connection(self.host.split(":")[0:1] + [int(self.host.split(":")[1])])
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.sock.settimeout(15)
        self.file = self.sock.makefile("rb")

    def request(self, method, path, headers, body=b""):
        for attempt in (0, 1):
            if self.sock is None:
                self._open()
            head = f"{method} {path} HTTP/1.1\r\nhost: {self.host}\r\n"
            head += "connection: keep-alive\r\nsec-fetch-site: same-origin\r\n"
            for name, value in headers.items():
                head += f"{name}: {value}\r\n"
            head += f"content-length: {len(body)}\r\n\r\n"
            try:
                self.sock.sendall(head.encode() + body)
                return self._read()
            except (OSError, EOFError):
                self.close()
                if attempt == 1:
                    raise

    def _read(self):
        line = self.file.readline()
        if not line:
            raise EOFError("the server closed the connection before answering")
        status = int(line.split()[1])
        headers = {}
        while True:
            raw = self.file.readline()
            if raw in (b"\r\n", b"\n", b""):
                break
            name, _, value = raw.decode(errors="replace").partition(":")
            headers.setdefault(name.strip().lower(), []).append(value.strip())
        length = int(headers.get("content-length", ["0"])[0])
        body = self.file.read(length) if length else b""
        if headers.get("transfer-encoding", [""])[0].lower() == "chunked":
            body = b""
            while True:
                size = int(self.file.readline().strip() or b"0", 16)
                if size == 0:
                    self.file.readline()
                    break
                body += self.file.read(size)
                self.file.read(2)
        return status, headers, body

    def close(self):
        if self.sock is not None:
            try:
                self.sock.close()
            except OSError:
                pass
        self.sock = None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rounds", type=int, default=50)
    parser.add_argument("--register", type=int, default=2, help="node to register on")
    parser.add_argument("--command", type=int, default=3, help="node to command")
    parser.add_argument("--no-after", action="store_true", help="send no offset back")
    parser.add_argument("--label", default=time.strftime("%F"))
    args = parser.parse_args()

    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    out = os.path.join(root, "docs", "perf", args.label)
    os.makedirs(out, exist_ok=True)
    report = os.path.join(out, "crossnode.txt")

    register_host = f"127.0.0.1:{3160 + args.register}"
    command_host = f"127.0.0.1:{3160 + args.command}"
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True
    ).stdout.strip()

    lines = [
        "cross-node register then command",
        f"started:  {time.strftime('%FT%T%z')}",
        f"revision: {revision}",
        f"register on node {args.register} ({register_host}), "
        f"command on node {args.command} ({command_host})",
        f"offset sent back as x-rn-after: {'no' if args.no_after else 'yes'}",
        "",
    ]

    registrar = Conn(register_host)
    commander = Conn(command_host)
    stamp = int(time.time())
    committed = declined = other = 0
    registers_declined = 0
    seen = 0
    waits = []

    for round_number in range(1, args.rounds + 1):
        form = urllib.parse.urlencode(
            {
                "display_name": f"Cross Node {round_number}",
                "email": f"crossnode-{stamp}-{round_number}@example.test",
                "password": "perf-harness-password-1",
            }
        ).encode()
        register_headers = {"content-type": "application/x-www-form-urlencoded"}
        if seen and not args.no_after:
            # The registrar waits too: two registrations in a row on a
            # follower race the same way a register and a command do.
            register_headers["x-rn-after"] = str(seen)
        status, headers, _ = registrar.request(
            "POST", "/auth/register", register_headers, form
        )
        cookie = ""
        for value in headers.get("set-cookie", []):
            if value.startswith("rn_session="):
                cookie = value[len("rn_session=") :].split(";")[0]
        offset = headers.get("x-rn-offset", [""])[0]
        if offset:
            seen = max(seen, int(offset))
        if not cookie:
            lines.append(f"round {round_number:<4} register answered {status}")
            registers_declined += status == 403
            other += 1
            continue

        extra = {"content-type": "application/json", "cookie": f"rn_session={cookie}"}
        if offset and not args.no_after:
            extra["x-rn-after"] = offset
        body = (
            '{"key":"%s","title":"cross node %d","body":"written by crossnode.py"}'
            % (uuid.uuid4(), round_number)
        ).encode()

        started = time.perf_counter()
        status, _, _ = commander.request("POST", "/api/cmd/create-document", extra, body)
        took = (time.perf_counter() - started) * 1000
        waits.append(took)
        if status == 200:
            committed += 1
        elif status == 403:
            declined += 1
        else:
            other += 1
        lines.append(
            f"round {round_number:<4} offset {offset or '-':<8} "
            f"create-document {status} in {took:.1f} ms"
        )

    registrar.close()
    commander.close()

    waits.sort()
    summary = (
        f"\n{args.rounds} rounds: {committed} committed, {declined} declined, {other} other"
        f" (of which {registers_declined} were the register itself)"
    )
    if waits:
        summary += (
            f"\ncreate-document on the other node: "
            f"p50 {waits[len(waits) // 2]:.1f} ms  "
            f"p99 {waits[min(len(waits) - 1, int(0.99 * len(waits)))]:.1f} ms  "
            f"max {waits[-1]:.1f} ms"
        )
    lines.append(summary)
    with open(report, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print(summary.strip())
    print(f"written to {report}")
    return 0 if declined == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
