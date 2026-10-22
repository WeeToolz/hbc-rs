#!/usr/bin/env python3
"""Control Rust HBC from a terminal without uploading a DOL."""
import argparse
import importlib.util
import os
from pathlib import Path
import secrets
import socket
import struct
import sys
import time

COMMANDS = {"restart": 1, "reset": 2, "power-off": 3, "eject": 4, "sync": 5,
            "save": 6, "screenshot": 7, "find-remotes": 8, "disconnect-remotes": 9,
            "stop-sync": 10, "restart-network": 11, "restart-app": 12, "return-hbc": 13}
ALIASES = {"system-menu": "reset", "power": "power-off", "save-settings": "save"}
STATES = ("queued", "running", "completed", "failed", "busy", "invalid", "unknown")


def packet(command, request_id):
    if not 0 <= request_id <= 0xffffffff or (command and not request_id):
        raise ValueError("request ID is out of range")
    return struct.pack(">4sBBHII", b"HBCC" if command else b"HBCQ", 1, command, 0, request_id, 0)


def recv_exact(connection, size):
    data = bytearray()
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            raise OSError("Connection closed before the complete command reply")
        data.extend(chunk)
    return bytes(data)


def exchange(host, port, command, request_id, timeout=3):
    with socket.create_connection((host, port), timeout=timeout) as connection:
        connection.settimeout(timeout)
        connection.sendall(packet(command, request_id))
        magic, version, state, operation, reserved, identity, session, length, tail = struct.unpack(
            ">4sBBBBIIHH", recv_exact(connection, 20))
        if (magic != b"HBCC" or version != 1 or state >= len(STATES) or reserved or tail
                or length > 512 or operation not in (0, *COMMANDS.values())):
            raise OSError("Invalid command reply")
        message = recv_exact(connection, length).decode("utf-8", "strict")
    if identity != request_id and not (request_id == 0 or state == 5):
        raise OSError("Reply request ID does not match")
    if command and state not in (5, 6) and operation != command:
        raise OSError("Reply operation does not match")
    if not session:
        raise OSError("Missing HBC session identity")
    return {"state": STATES[state], "operation": operation, "id": identity,
            "session": session, "message": message}


def run_command(host, port, command, timeout, exchange_fn=exchange, clock=time.monotonic, sleep=time.sleep):
    request_id = secrets.randbelow(0xffffffff) + 1
    started = clock()
    # A failure here proves this invocation submitted no hardware operation.
    try:
        session = exchange_fn(host, port, 0, 0)["session"]
    except (OSError, UnicodeError) as error:
        raise OSError(f"Read-only preflight failed; no hardware command was sent: {error}") from error
    print(f"request={request_id} operation={command}", flush=True)
    last_state = None
    last_error = None
    try:
        reply = exchange_fn(host, port, command, request_id)
    except (OSError, UnicodeError) as error:
        # The request may have reached HBC even if its reply was lost.
        # Query its identity; never repeat a hardware action.
        reply = None
        last_error = error
        print("Request delivery is unconfirmed; querying its result without resending", flush=True)
    while True:
        if reply is not None:
            if reply["session"] != session:
                if command == COMMANDS["restart"]:
                    print("HBC answered in a new session after the restart request. The physical console boot sequence was not observed.")
                    return 0
                raise OSError("HBC session changed before the operation result was confirmed")
            if reply["operation"] not in (command, 0):
                raise OSError("The retained result is for a different operation")
            if reply["state"] != last_state:
                print(f'{reply["state"]}: {reply["message"]}', flush=True)
                last_state = reply["state"]
            if reply["state"] == "completed":
                return 0
            if reply["state"] in ("failed", "busy", "invalid", "unknown"):
                raise OSError(reply["message"])
        if clock() - started >= timeout:
            detail = f" Last connection error: {last_error}" if last_error else ""
            raise TimeoutError("Operation completion is unconfirmed. The command was sent once; it was not repeated. Use status to inspect the retained result." + detail)
        sleep(.2)
        try:
            reply = exchange_fn(host, port, 0, request_id)
        except (OSError, UnicodeError) as error:
            last_error = error
            reply = None


def input_sender(host, port, arguments):
    spec = importlib.util.spec_from_file_location("hbc_input", Path(__file__).with_name("hbc-input.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.main(["--wii", host, "--port", str(port), *arguments])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        "input passes the remaining arguments to hbc-input.py. Example: "
        "--wii 10.0.0.242 input --kind wiimote home. Commands operate only while this HBC build is running."))
    parser.add_argument("--wii", default=os.environ.get("HBC_WII", os.environ.get("WII_IP", "10.0.0.242")))
    parser.add_argument("--port", type=int, default=4299)
    parser.add_argument("--timeout", type=float, default=70)
    parser.add_argument("command", choices=[*COMMANDS, *ALIASES, "status", "wait", "input", "home", "close"])
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    if not 1 <= args.port <= 65535 or not 0 < args.timeout <= 600:
        parser.error("port must be 1..65535 and timeout must be greater than 0 and at most 600 seconds")
    try:
        if args.command == "input":
            return input_sender(args.wii, args.port, args.arguments)
        if args.arguments:
            parser.error("only input takes additional arguments")
        if args.command in ("home", "close"):
            return input_sender(args.wii, args.port, ["home" if args.command == "home" else "b"])
        if args.command == "wait":
            deadline = time.monotonic() + args.timeout
            last_error = None
            while time.monotonic() < deadline:
                try:
                    reply = exchange(args.wii, args.port, 0, 0)
                    print(f'Rust HBC command service is ready; session={reply["session"]}')
                    return 0
                except (OSError, UnicodeError) as error:
                    last_error = error
                    time.sleep(.25)
            raise TimeoutError(f"Rust HBC command service did not answer: {last_error}")
        if args.command == "status":
            reply = exchange(args.wii, args.port, 0, 0)
            print(f'session={reply["session"]} request={reply["id"]} operation={reply["operation"]} {reply["state"]}: {reply["message"]}')
            return 1 if reply["state"] in ("failed", "invalid", "busy") else 0
        command = COMMANDS[ALIASES.get(args.command, args.command)]
        return run_command(args.wii, args.port, command, args.timeout)
    except (OSError, ValueError, UnicodeError) as error:
        print(f"hbc-control: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
