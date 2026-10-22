#!/usr/bin/env python3
"""Test the production input socket in an isolated macOS Dolphin profile."""
import argparse
import importlib.util
import os
from pathlib import Path
import signal
import shutil
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("hbc_input", ROOT / "tools/hbc-input.py")
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)


def exchange(host, packet):
    with socket.create_connection((host,4299),timeout=5) as connection:
        connection.settimeout(5)
        # Deliberately split the header and body across writes.
        connection.sendall(packet[:7]); connection.sendall(packet[7:16]); connection.sendall(packet[16:])
        reply = bytearray()
        while len(reply) < 8:
            chunk = connection.recv(8-len(reply))
            if not chunk: raise RuntimeError("truncated input response")
            reply.extend(chunk)
        return bytes(reply)


def own_processes(profile):
    result = subprocess.check_output(["/bin/ps","-axo","pid=,command="],text=True)
    return [int(line.strip().split(None,1)[0]) for line in result.splitlines()
            if str(profile) in line and "/Dolphin.app/Contents/MacOS/Dolphin" in line]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host",required=True,help="Mac LAN address used by Dolphin's emulated Wii sockets")
    parser.add_argument("--dol",type=Path,default=ROOT/"target/hbc-rs.dol")
    parser.add_argument("--control", action="store_true", help="also test the direct terminal command service")
    args = parser.parse_args()
    profile = Path(tempfile.mkdtemp(prefix="hbc-remote-input-"))
    log = profile/"Logs/dolphin.log"

    nand = Path("/Volumes/SSD/WiiDev/dolphin-network-test-nand/Wii")
    if not (nand / "shared2/sys/SYSCONF").is_file():
        raise RuntimeError("Source NAND SYSCONF not found")
    if not (nand / "shared2/sys/net/02/config.dat").is_file():
        raise RuntimeError("Source NAND network configuration not found")

    shutil.copytree(nand, profile / "Wii")
    print(f"Isolated profile: {profile}", flush=True)
    print("PASS: Test NAND copied into Dolphin profile", flush=True)
    subprocess.run(["/usr/bin/open","-n","-a","/Applications/Dolphin.app","--args",
                    "-b","-u",str(profile),"-e",str(args.dol.resolve()),
                    "-C","Logger.Options.WriteToFile=True","-C","Logger.Options.Verbosity=4",
                    "-C","Logger.Logs.OSREPORT=True","-C","Dolphin.Interface.UsePanicHandlers=False",
                    "-C","Dolphin.Interface.ConfirmStop=False",
                    "-C","Dolphin.Analytics.PermissionAsked=True",
                    "-C","Dolphin.Analytics.Enabled=False",
                    "-C","Dolphin.Core.WiiSDCard=False"],check=True)
    try:
        deadline = time.monotonic()+40
        while time.monotonic()<deadline:
            text = log.read_text(errors="replace") if log.exists() else ""
            if "Wiiload receiver state changed to Listening" in text: break
            if "panic" in text.lower(): raise RuntimeError("guest panic during startup")
            time.sleep(.25)
        else: raise RuntimeError("guest listener did not become ready")
        packet = bytearray.fromhex("48424349 0102 0000 00000080 03e8 0000 0000 0000 0000 0000 0000 0000 0000 0000")
        applied = b"HBCI\x01\0\0\0"
        if exchange(args.host,packet)!=applied: raise RuntimeError("HOME was not consumed by the real guest main loop")
        print("HOME: main-loop input completion received",flush=True)
        invalid = bytearray(packet); invalid[4] = 2
        if exchange(args.host,invalid)!=b"HBCI\x01\x01\0\0": raise RuntimeError("invalid protocol version was not rejected")
        release = bytearray(packet); release[11] = 0; release[12:14] = b"\0\0"
        if exchange(args.host,release)!=applied: raise RuntimeError("release was not consumed")
        print("Invalid command rejected; release consumed",flush=True)
        # A pointer and both sticks, without confirm, cannot activate a menu item.
        analog = bytearray(release); analog[5] = 6; analog[7] = 1
        analog[12:18] = bytes.fromhex("03e8 0140 00f0")
        analog[20:28] = bytes.fromhex("4000 4000 c000 2000")
        if exchange(args.host,analog)!=applied: raise RuntimeError("analog/pointer input was not consumed")
        time.sleep(1.2) # Hold must expire even without another network command.
        if exchange(args.host,release)!=applied: raise RuntimeError("receiver failed after automatic input expiry")
        print("Analog/pointer input consumed; receiver still works after hold expiry",flush=True)
        if args.control:
            control_spec = importlib.util.spec_from_file_location("hbc_control", ROOT / "tools/hbc-control.py")
            control = importlib.util.module_from_spec(control_spec)
            control_spec.loader.exec_module(control)
            first = control.exchange(args.host,4299,0,0)
            if first["state"] != "unknown": raise RuntimeError(f"unexpected initial command result: {first}")
            for operation, identity, message in [(12,101,"Unsupported in the HBC browser"), (7,102,"NoStorage")]:
                result = control.exchange(args.host,4299,operation,identity)
                deadline = time.monotonic()+10
                while result["state"] in ("queued","running") and time.monotonic()<deadline:
                    time.sleep(.1)
                    result = control.exchange(args.host,4299,0,identity)
                if result["state"] != "failed" or message not in result["message"]:
                    raise RuntimeError(f"production operation did not return the required error: {result}")
                duplicate = control.exchange(args.host,4299,operation,identity)
                if duplicate != result: raise RuntimeError("duplicate command did not retain its completed result")
                print(f"Control {operation}: final backend error delivered, duplicate did not rerun: {result['message']}",flush=True)
            # Exercise every controller family and its complete defined button mask
            # with release after each frame. No destructive HBC menu confirmation.
            for kind, mask in [(1,0x1f7f),(2,0x1f9f),(3,0xffff1f9f),(4,0x31f9f),
                               (5,0xffff1f9f), *[(kind,0x1fffff) for kind in range(6,12)]]:
                buttons = bytearray(release); buttons[5]=kind; buttons[8:12]=mask.to_bytes(4,"big"); buttons[12:14]=(1000).to_bytes(2,"big")
                # Validate parsing without producing menu presses: a zero hold with
                # active buttons is invalid for every family.
                buttons[12:14]=b"\0\0"
                if exchange(args.host,buttons)!=b"HBCI\x01\x01\0\0": raise RuntimeError(f"invalid all-release accepted for kind {kind}")
            print("Control parser rejects active release packets for every controller family",flush=True)
        text = log.read_text(errors="replace")
        if any(word in text.lower() for word in ("panicked at", "program exception", "invalid read")):
            raise RuntimeError("guest fault recorded; inspect log")
        print("PASS: production socket and main-loop input path in Dolphin; real Wii verification remains required",flush=True)
    finally:
        processes = own_processes(profile)
        for pid in processes:
            try: os.kill(pid,signal.SIGTERM)
            except ProcessLookupError: pass
        time.sleep(.5)
        for pid in own_processes(profile):
            try: os.kill(pid,signal.SIGKILL)
            except ProcessLookupError: pass
        if own_processes(profile): raise RuntimeError("could not stop the isolated Dolphin process")

if __name__ == "__main__": main()
