#!/usr/bin/env python3
"""Test the production input socket in an isolated macOS Dolphin profile."""
import argparse
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import uuid
import zlib

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("hbc_input", ROOT / "tools/hbc-input.py")
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)


class ReferenceComplete(Exception):
    """The reference-only checks completed; continue through common cleanup."""


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


def reborn_status(host):
    with socket.create_connection((host, 4299), timeout=5) as connection:
        connection.settimeout(5)
        connection.sendall(b"HBCS".ljust(16, b"\0"))

        def read_exact(length):
            data = bytearray()
            while len(data) < length:
                chunk = connection.recv(length - len(data))
                if not chunk:
                    raise RuntimeError("truncated Reborn status reply")
                data.extend(chunk)
            return bytes(data)

        reply = read_exact(8)
        status = int.from_bytes(reply[:4], "big", signed=True)
        length = int.from_bytes(reply[4:], "big")
        if status != 0 or length > 8192:
            raise RuntimeError(f"invalid Reborn status: {status}, {length}")
        return json.loads(read_exact(length))


def reborn_hardware(host, mask):
    with socket.create_connection((host, 4299), timeout=30) as connection:
        connection.settimeout(30)
        connection.sendall(b"HBCH" + struct.pack(">H", mask) + bytes(10))
        header = read_exact(connection, 8)
        status = int.from_bytes(header[:4], "big", signed=True)
        length = int.from_bytes(header[4:], "big")
        if status != 0 or length > 8192:
            raise RuntimeError(f"invalid HBCH reply: status={status}, length={length}")
        return json.loads(read_exact(connection, length))


def reborn_hardware_info(host):
    names = ("System", "Video", "Storage", "USB", "Network")
    all_pages = reborn_hardware(host, 0)
    if tuple(all_pages) != names:
        raise RuntimeError(f"HBCH zero mask returned pages {tuple(all_pages)!r}")
    if any(not isinstance(page, dict) for page in all_pages.values()):
        raise RuntimeError("HBCH page values must be JSON objects")
    if not all_pages["System"].get("IOS", "").startswith("IOS"):
        raise RuntimeError(f"HBCH System page omitted IOS: {all_pages['System']!r}")
    selected = {}
    for index, name in enumerate(names):
        page = reborn_hardware(host, 1 << index)
        if tuple(page) != (name,):
            raise RuntimeError(f"HBCH mask for {name} returned pages {tuple(page)!r}")
        selected[name] = page[name]
    combined = reborn_hardware(host, (1 << 1) | (1 << 4))
    if tuple(combined) != ("Video", "Network"):
        raise RuntimeError(f"HBCH combined mask returned pages {tuple(combined)!r}")
    unknown = reborn_hardware(host, 1 << 7)
    if unknown != {}:
        raise RuntimeError(f"HBCH unknown page mask returned {unknown!r}")
    return {"all_pages": all_pages, "selected_pages": selected,
            "combined_pages": list(combined), "unknown_mask": unknown}


def reborn_screen(host):
    with socket.create_connection((host, 4299), timeout=30) as connection:
        connection.settimeout(30)
        connection.sendall(b"HBCP".ljust(16, b"\0"))
        header = read_exact(connection, 8)
        status = int.from_bytes(header[:4], "big", signed=True)
        length = int.from_bytes(header[4:], "big")
        if status != 0 or not 8 <= length <= 4 * 1024 * 1024:
            raise RuntimeError(f"invalid Reborn screenshot reply: {status}, {length}")
        payload = read_exact(connection, length)
    width = int.from_bytes(payload[:4], "big")
    height = int.from_bytes(payload[4:8], "big")
    expected = width * height * 2
    if not width or not height or expected != len(payload) - 8:
        raise RuntimeError(
            f"invalid Reborn screenshot dimensions or payload: {width}x{height}, {len(payload)-8} bytes"
        )
    if not any(payload[8:]):
        raise RuntimeError("Reborn screenshot framebuffer is all zero")
    return {"width": width, "height": height, "framebuffer_bytes": expected}


def reborn_netlog(host):
    for port, expected in ((4407, f"{host}:4407"), (0, None)):
        if exchange(host, (b"HBCN" + port.to_bytes(2, "big")).ljust(16, b"\0")) != bytes(8):
            raise RuntimeError(f"HBCN rejected port {port}")
        actual = reborn_status(host).get("log")
        if actual != expected:
            raise RuntimeError(f"HBCS log target for port {port} was {actual!r}")
    return {"set": f"{host}:4407", "cleared": True}


def reborn_lastlog_empty(host):
    status = reborn_status(host)
    if status.get("lastlog") is not None:
        raise RuntimeError(f"HBCS reported a stale retained log: {status['lastlog']!r}")
    with socket.create_connection((host, 4299), timeout=5) as connection:
        connection.settimeout(5)
        connection.sendall(b"HBCL".ljust(16, b"\0"))
        header = read_exact(connection, 8)
    error = int.from_bytes(header[:4], "big", signed=True)
    length = int.from_bytes(header[4:], "big")
    if error != -2 or length != 0:
        raise RuntimeError(f"empty HBCL reply was status={error}, length={length}")
    return {"error": "ENOENT", "length": 0}


def reborn_crash_empty(host):
    before = reborn_status(host).get("crash")
    if before is not None:
        raise RuntimeError(f"HBCS reported a stale crash record: {before!r}")
    if exchange(host, b"HBCC".ljust(16, b"\0")) != bytes(8):
        raise RuntimeError("HBCC crash clear did not return an empty success reply")
    after = reborn_status(host).get("crash")
    if after is not None:
        raise RuntimeError(f"HBCS still reported a crash after HBCC clear: {after!r}")
    return {"before": None, "cleared": True, "after": None}


def reborn_netlog_app(host, dol, log, process):
    port = 4407
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind((host, port))
        listener.listen(1)
        listener.settimeout(35)
        if exchange(host, (b"HBCN" + port.to_bytes(2, "big")).ljust(16, b"\0")) != bytes(8):
            raise RuntimeError("HBCN did not accept the log target")
        if reborn_status(host).get("log") != f"{host}:{port}":
            raise RuntimeError("HBCS did not report the HBCN target before launch")
        image = dol.read_bytes()
        header = b"HAXX\0\x05\0\0" + len(image).to_bytes(4, "big") + bytes(4)
        with socket.create_connection((host, 4299), timeout=5) as sender:
            sender.sendall(header + image)

        confirmed = False
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline and not confirmed:
            if process.poll() is not None:
                raise RuntimeError("Dolphin exited while the HBC received the log probe")
            if "to=WiiloadConfirm" in log.read_text(errors="replace"):
                packet = b"HBCI" + bytes([1, 0, 0, 0]) + struct.pack(">IH", 16, 500) + bytes(18)
                if len(packet) != 32 or exchange(host, packet) != b"HBCI\x01\0\0\0":
                    raise RuntimeError("could not accept the HBC Wiiload app confirmation")
                confirmed = True
                break
            time.sleep(.1)

        with listener.accept()[0] as connection:
            connection.settimeout(10)
            output = bytearray()
            while True:
                try:
                    chunk = connection.recv(4096)
                except socket.timeout:
                    break
                if not chunk:
                    break
                output.extend(chunk)
                if b"HBCN_HANDOFF_OK" in output:
                    break
        if b"HBCN_HANDOFF_OK" not in output:
            raise RuntimeError(f"app did not log the HBCN marker: {bytes(output)!r}")

    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Dolphin exited before HBC returned after the log probe")
        try:
            status = reborn_status(host)
        except (OSError, RuntimeError):
            time.sleep(.2)
            continue
        if status.get("log") == f"{host}:{port}":
            return {"target": status["log"], "marker": "HBCN_HANDOFF_OK", "returned_to_hbc": True}
        time.sleep(.2)
    raise RuntimeError("HBC did not restore the HBCN log target after the app returned")


def read_exact(connection, length):
    data = bytearray()
    while len(data) < length:
        chunk = connection.recv(length - len(data))
        if not chunk:
            raise RuntimeError("truncated HBCF reply")
        data.extend(chunk)
    return bytes(data)


def devfile_packet(operation, path, size=0, flags=0):
    path_bytes = path.encode("ascii")
    if not 0 < len(path_bytes) < 256:
        raise ValueError("HBCF path length is outside the protocol bounds")
    return (b"HBCF" + operation + bytes([flags]) + len(path_bytes).to_bytes(2, "big")
            + size.to_bytes(4, "big") + bytes(4) + path_bytes)


def devfile_request(host, operation, path, body=b"", size=None, flags=0):
    if size is None:
        size = len(body)
    with socket.create_connection((host, 4299), timeout=5) as connection:
        connection.settimeout(5)
        connection.sendall(devfile_packet(operation, path, size, flags) + body)
        reply = read_exact(connection, 8)
        status = int.from_bytes(reply[:4], "big", signed=True)
        length = int.from_bytes(reply[4:], "big")
        if length > 256 * 1024:
            raise RuntimeError(f"HBCF reply is too large: {length}")
        return status, read_exact(connection, length)


def devfile_put_frame(raw):
    compressor = zlib.compressobj(level=1)
    compressed = compressor.compress(raw) + compressor.flush()
    wire = compressed if len(compressed) < len(raw) else raw
    return (len(raw).to_bytes(4, "big") + len(wire).to_bytes(4, "big")
            + (zlib.crc32(raw) & 0xffffffff).to_bytes(4, "big") + wire)


def devfile_get_framed(host, path, compress):
    flags = 1 if compress else 0
    with socket.create_connection((host, 4299), timeout=5) as connection:
        connection.settimeout(5)
        connection.sendall(devfile_packet(b"g", path, flags=flags))
        reply = read_exact(connection, 8)
        status = int.from_bytes(reply[:4], "big", signed=True)
        total = int.from_bytes(reply[4:], "big")
        if status:
            return status, b""
        output = bytearray()
        while True:
            frame = read_exact(connection, 12)
            raw_length = int.from_bytes(frame[:4], "big")
            wire_length = int.from_bytes(frame[4:8], "big")
            checksum = int.from_bytes(frame[8:], "big")
            if raw_length == 0:
                terminal_status = int.from_bytes(frame[8:], "big", signed=True)
                if terminal_status:
                    raise RuntimeError(f"HBCF framed download ended with {terminal_status}")
                break
            if raw_length > 64 * 1024 or wire_length > raw_length:
                raise RuntimeError("HBCF returned an invalid download frame size")
            wire = read_exact(connection, wire_length)
            raw = zlib.decompress(wire) if wire_length < raw_length else wire
            actual_crc = zlib.crc32(raw) & 0xffffffff
            if len(raw) != raw_length or actual_crc != checksum:
                raise RuntimeError(
                    "HBCF download frame failed size or CRC verification "
                    f"(raw={len(raw)}/{raw_length}, wire={wire_length}, "
                    f"crc={actual_crc:08x}/{checksum:08x})"
                )
            output.extend(raw)
        if len(output) != total:
            raise RuntimeError(f"HBCF download size was {len(output)}, expected {total}")
        return 0, bytes(output)


def test_reborn_devfile(host):
    folder = f"sd:/apps/__hbc_rs_parity_{uuid.uuid4().hex[:10]}"
    raw_path = folder + "/raw.bin"
    framed_path = folder + "/framed.bin"
    app_path = folder + "/boot.dol"
    app_data_dir = folder + "/data"
    app_data_path = app_data_dir + "/save.bin"
    raw = bytes(range(256)) * 3
    framed = b"HBCF framed transfer test\n" * 173
    app_data = b"Catalog refresh evidence from HBCF."
    deadline = time.monotonic() + 30
    initial_status = reborn_status(host)
    while initial_status.get("device") != "sd" and time.monotonic() < deadline:
        time.sleep(.1)
        initial_status = reborn_status(host)
    if initial_status.get("device") != "sd":
        raise RuntimeError(f"HBCF Dolphin test did not mount its SD image: {initial_status}")
    initial_app_count = initial_status["apps"]
    try:
        status, _ = devfile_request(host, b"M", folder)
        if status:
            raise RuntimeError(f"HBCF mkdir failed with {status}")
        status, _ = devfile_request(host, b"M", app_data_dir)
        if status:
            raise RuntimeError(f"HBCF app-data mkdir failed with {status}")
        status, stat = devfile_request(host, b"S", folder)
        if status or not stat.startswith(b"d 0 "):
            raise RuntimeError(f"HBCF stat directory mismatch: {status}, {stat!r}")
        status, _ = devfile_request(host, b"P", raw_path, raw)
        if status:
            raise RuntimeError(f"HBCF raw upload failed with {status}")
        encoded = b"".join(devfile_put_frame(framed[i:i + 64 * 1024])
                            for i in range(0, len(framed), 64 * 1024))
        status, _ = devfile_request(host, b"p", framed_path, encoded, len(framed))
        if status:
            raise RuntimeError(f"HBCF framed upload failed with {status}")
        status, _ = devfile_request(host, b"P", app_path, app_data)
        if status:
            raise RuntimeError(f"HBCF app upload failed with {status}")
        time.sleep(.7)
        status, _ = devfile_request(host, b"P", app_data_path, b"private save data")
        if status:
            raise RuntimeError(f"HBCF app-data upload failed with {status}")
        time.sleep(.4)
        if reborn_status(host).get("apps") != initial_app_count:
            raise RuntimeError("HBCF refreshed the app catalog before the last app-folder change was quiet")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if reborn_status(host).get("apps") == initial_app_count + 1:
                break
            time.sleep(.1)
        else:
            raise RuntimeError("HBCF app upload did not refresh the active catalog after one quiet second")
        status, checksum = devfile_request(host, b"C", raw_path)
        expected_checksum = len(raw).to_bytes(4, "big") + (zlib.crc32(raw) & 0xffffffff).to_bytes(4, "big")
        if status or checksum != expected_checksum:
            raise RuntimeError(f"HBCF checksum mismatch: {status}, {checksum!r}")
        status, downloaded = devfile_request(host, b"G", raw_path)
        if status or downloaded != raw:
            raise RuntimeError(f"HBCF raw download mismatch: {status}")
        status, downloaded = devfile_get_framed(host, framed_path, compress=True)
        if status or downloaded != framed:
            raise RuntimeError(f"HBCF framed download mismatch: {status}")
        status, listing = devfile_request(host, b"L", folder)
        if (status or b"raw.bin" not in listing or b"framed.bin" not in listing
                or b"boot.dol" not in listing):
            raise RuntimeError(f"HBCF list mismatch: {status}, {listing!r}")
        for path in (raw_path, framed_path, app_path, app_data_path):
            status, _ = devfile_request(host, b"D", path)
            if status:
                raise RuntimeError(f"HBCF delete failed with {status}: {path}")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if reborn_status(host).get("apps") == initial_app_count:
                break
            time.sleep(.1)
        else:
            raise RuntimeError("HBCF app delete did not refresh the active catalog")
        status, _ = devfile_request(host, b"D", app_data_dir)
        if status:
            raise RuntimeError(f"HBCF app-data directory delete failed with {status}")
        status, _ = devfile_request(host, b"D", folder)
        if status:
            raise RuntimeError(f"HBCF directory delete failed with {status}")
        if devfile_request(host, b"S", raw_path)[0] != -2:
            raise RuntimeError("HBCF stat did not report ENOENT after delete")
        return {"operations": ["M", "S", "P", "p", "C", "G", "g", "L", "D"],
                "bytes_checked": len(raw) + len(framed) + len(app_data) + len(b"private save data"),
                "catalog_refresh": "upload/delete passed; app data restarted the quiet timer",
                "cleanup": "complete"}
    finally:
        for path in (raw_path, framed_path, app_path, app_data_path, app_data_dir, folder):
            try:
                devfile_request(host, b"D", path)
            except OSError:
                pass


def own_processes(profile):
    result = subprocess.check_output(["/bin/ps","-axo","pid=,command="],text=True)
    return [int(line.strip().split(None,1)[0]) for line in result.splitlines()
            if str(profile) in line and "/Dolphin.app/Contents/MacOS/Dolphin" in line]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host",required=True,help="Mac LAN address used by Dolphin's emulated Wii sockets")
    parser.add_argument("--dol",type=Path,default=ROOT/"target/hbc-rs.dol")
    parser.add_argument("--control", action="store_true", help="also test the direct terminal command service")
    parser.add_argument("--reborn-keys", action="store_true", help="check Reborn HBCK replies")
    parser.add_argument("--reborn-key-transitions", action="store_true", help="check HBCK screen transitions in a diagnostic build")
    parser.add_argument("--reborn-key-eof", action="store_true", help="check an interrupted HBCK body")
    parser.add_argument("--wiiload-eof", action="store_true", help="check interrupted Wiiload reads")
    parser.add_argument("--wiiload-theme", action="store_true", help="check received theme application")
    parser.add_argument("--reborn-theme-control", action="store_true", help="check Reborn C theme upload status")
    parser.add_argument("--reborn-transfer-style", action="store_true", help="check HBCT style replies and HBCS state")
    parser.add_argument("--reborn-screen", action="store_true", help="check HBCP dimensions and live YUYV framebuffer reply")
    parser.add_argument("--reborn-hardware-info", action="store_true", help="check HBCH page masks and JSON replies")
    parser.add_argument("--reborn-netlog", action="store_true", help="check HBCN target set, HBCS state, and clear")
    parser.add_argument("--reborn-lastlog-empty", action="store_true", help="check HBCS and HBCL when no retained log exists")
    parser.add_argument("--reborn-crash-empty", action="store_true", help="check HBCS and HBCC clear when no crash exists")
    parser.add_argument("--reborn-netlog-app", action="store_true", help="check HBCN memory handoff to a log probe app and back")
    parser.add_argument("--app-dol", type=Path, help="small probe DOL used by --reborn-netlog-app")
    parser.add_argument("--reborn-file", action="store_true", help="check HBCF operations against an isolated SD image")
    parser.add_argument("--nand", type=Path, required=True, help="Source NAND; only a private copy is used")
    parser.add_argument("--sd-image", type=Path, help="Clone an existing private SD image into the test profile")
    parser.add_argument("--dolphin", type=Path, required=True, help="Dolphin executable")
    parser.add_argument("--output", type=Path, required=True, help="New result directory, including its private profile")
    args = parser.parse_args()
    if args.reborn_file and not args.sd_image:
        parser.error("--reborn-file requires --sd-image")
    if args.reborn_netlog_app and not args.app_dol:
        parser.error("--reborn-netlog-app requires --app-dol")
    addresses = subprocess.check_output(["/sbin/ifconfig"], text=True)
    if f"inet {args.host} " not in addresses:
        parser.error("--host must be an address assigned to this Mac")
    with socket.socket() as port_check:
        port_check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        port_check.bind((args.host, 4299))
    args.output.mkdir(parents=True, exist_ok=False)
    profile = args.output / "profile"
    profile.mkdir()
    result = {"dol_sha256": hashlib.sha256(args.dol.read_bytes()).hexdigest(),
              "success": False, "transport": "HBC TCP input protocol", "control": args.control,
              "reborn_keys": args.reborn_keys,
              "reborn_transfer_style": args.reborn_transfer_style,
              "reborn_screen": args.reborn_screen,
              "reborn_hardware_info": args.reborn_hardware_info,
              "reborn_netlog": args.reborn_netlog,
              "reborn_lastlog_empty": args.reborn_lastlog_empty,
              "reborn_crash_empty": args.reborn_crash_empty,
              "reborn_netlog_app": args.reborn_netlog_app,
              "reborn_file": args.reborn_file}
    log = profile/"Logs/dolphin.log"

    nand = args.nand.resolve(strict=True)
    if not (nand / "shared2/sys/SYSCONF").is_file():
        raise RuntimeError("Source NAND SYSCONF not found")
    if not (nand / "shared2/sys/net/02/config.dat").is_file():
        raise RuntimeError("Source NAND network configuration not found")

    subprocess.run(["/bin/cp", "-c", "-R", str(nand), str(profile / "Wii")], check=True)
    if args.sd_image:
        (profile / "Load").mkdir()
        subprocess.run(["/bin/cp", "-c", str(args.sd_image.resolve(strict=True)),
                        str(profile / "Load/WiiSD.raw")], check=True)
    if args.wiiload_theme or args.reborn_theme_control:
        (profile / "Wii/title/00010001/48424345/data").mkdir(parents=True, exist_ok=True)
    print(f"Isolated profile: {profile}", flush=True)
    print("PASS: Test NAND copied into Dolphin profile", flush=True)
    output_log = (args.output / "process.log").open("w")
    process = subprocess.Popen([str(args.dolphin.resolve(strict=True)),
                    "-b","-u",str(profile),"-e",str(args.dol.resolve()),
                    "-C","Logger.Options.WriteToFile=True","-C","Logger.Options.Verbosity=4",
                    "-C","Logger.Logs.OSREPORT=True","-C","Dolphin.Interface.UsePanicHandlers=False",
                    "-C","Dolphin.Interface.ConfirmStop=False",
                    "-C","Dolphin.Analytics.PermissionAsked=True",
                    "-C","Dolphin.Analytics.Enabled=False",
                    "-C",f"Dolphin.Core.WiiSDCard={'True' if args.sd_image else 'False'}"],
                    stdout=output_log, stderr=subprocess.STDOUT)
    try:
        deadline = time.monotonic()+40
        while time.monotonic()<deadline:
            if process.poll() is not None:
                raise RuntimeError("Dolphin exited before the HBC listener became ready")
            if args.reborn_theme_control or args.reborn_transfer_style or args.reborn_file or args.reborn_screen or args.reborn_hardware_info or args.reborn_netlog or args.reborn_lastlog_empty or args.reborn_crash_empty or args.reborn_netlog_app:
                try:
                    with socket.create_connection((args.host, 4299), timeout=.5) as probe:
                        probe.sendall(b"HBCS".ljust(16, b"\0"))
                        probe.settimeout(.5)
                        if probe.recv(8) == b"":
                            time.sleep(.25)
                            continue
                    break
                except OSError:
                    time.sleep(.25)
                    continue
            text = log.read_text(errors="replace") if log.exists() else ""
            if "Wiiload receiver state changed to Listening" in text: break
            if "panic" in text.lower(): raise RuntimeError("guest panic during startup")
            time.sleep(.25)
        else: raise RuntimeError("guest listener did not become ready")
        if args.reborn_theme_control:
            initial = reborn_status(args.host)
            initial_seq = (initial.get("upload") or {}).get("seq", 0)
            theme = (ROOT / "tests/fixtures/theme-valid.zip").read_bytes()
            header = b"HAXX\0\x05\0\0" + len(theme).to_bytes(4, "big") + bytes(4)
            with socket.create_connection((args.host, 4299), timeout=5) as connection:
                connection.sendall(header + theme)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                current = reborn_status(args.host)
                upload = current.get("upload") or {}
                if upload.get("seq", 0) > initial_seq:
                    if upload.get("result") != "theme":
                        raise RuntimeError(f"Reborn theme upload failed: {upload}")
                    result["reborn_upload"] = upload
                    result["success"] = True
                    print(f"Reborn reported theme upload: {upload}", flush=True)
                    raise ReferenceComplete
                if process.poll() is not None:
                    raise RuntimeError("Reborn exited while applying theme")
                time.sleep(.2)
            raise RuntimeError("Reborn did not report theme upload completion")
        if args.reborn_transfer_style:
            selected = []
            for style in (b"scroll", b"bounce", b"truncate"):
                packet = b"HBCT" + style.ljust(12, b"\0")
                if exchange(args.host, packet) != bytes(8):
                    raise RuntimeError(f"HBCT rejected valid style {style.decode()}")
                current = reborn_status(args.host)
                if current.get("transfer_style") != style.decode():
                    raise RuntimeError(f"HBCS did not retain {style.decode()}: {current}")
                selected.append(style.decode())
            invalid = exchange(args.host, b"HBCT" + b"invalid".ljust(12, b"\0"))
            expected_error = (-22).to_bytes(4, "big", signed=True) + bytes(4)
            if invalid != expected_error:
                raise RuntimeError(f"HBCT invalid-style reply was {invalid!r}")
            result["transfer_styles"] = selected
            result["invalid_style_reply"] = "EINVAL"
            result.setdefault("cases", []).append(
                {"case": "reborn-transfer-style", "success": True, "result": 0}
            )
            result["success"] = True
            print(f"HBCT accepted {', '.join(selected)} and rejected an invalid style", flush=True)
            if not args.reborn_file:
                raise ReferenceComplete
        if args.reborn_screen:
            result["reborn_screen_result"] = reborn_screen(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-screen", "success": True, "result": 0}
            )
            result["success"] = True
            print(f"HBCP framebuffer passed: {result['reborn_screen_result']}", flush=True)
            raise ReferenceComplete
        if args.reborn_hardware_info:
            result["reborn_hardware_info_result"] = reborn_hardware_info(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-hardware-info", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCH page masks and JSON replies passed", flush=True)
            raise ReferenceComplete
        if args.reborn_netlog:
            result["reborn_netlog_result"] = reborn_netlog(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-netlog", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCN set, HBCS target report, and clear passed", flush=True)
            raise ReferenceComplete
        if args.reborn_lastlog_empty:
            result["reborn_lastlog_empty_result"] = reborn_lastlog_empty(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-lastlog-empty", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCS null lastlog and HBCL ENOENT passed", flush=True)
            raise ReferenceComplete
        if args.reborn_crash_empty:
            result["reborn_crash_empty_result"] = reborn_crash_empty(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-crash-empty", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCS null crash and HBCC clear passed", flush=True)
            raise ReferenceComplete
        if args.reborn_netlog_app:
            result["reborn_netlog_app_result"] = reborn_netlog_app(
                args.host, args.app_dol.resolve(strict=True), log, process
            )
            result.setdefault("cases", []).append(
                {"case": "reborn-netlog-app", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCN app handoff, log connection, and return passed", flush=True)
            raise ReferenceComplete
        if args.reborn_file:
            result["reborn_file_result"] = test_reborn_devfile(args.host)
            result.setdefault("cases", []).append(
                {"case": "reborn-file", "success": True, "result": 0}
            )
            result["success"] = True
            print("HBCF stat/list/checksum/delete and raw/framed transfers passed", flush=True)
            raise ReferenceComplete
        if args.wiiload_theme:
            theme = (ROOT / "tests/fixtures/theme-valid.zip").read_bytes()
            header = b"HAXX\0\x05\0\0" + len(theme).to_bytes(4, "big") + bytes(4)
            with socket.create_connection((args.host, 4299), timeout=5) as connection:
                connection.sendall(header + theme)
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                content = log.read_text(errors="replace")
                if "HBC: received theme applied" in content:
                    break
                if "to=WiiloadConfirm" in content:
                    raise RuntimeError("received theme unexpectedly requested confirmation")
                if "HBC: received theme failed" in content:
                    raise RuntimeError("received theme failed to apply")
                if process.poll() is not None:
                    raise RuntimeError("Dolphin exited while applying received theme")
                time.sleep(.1)
            else:
                raise RuntimeError("received theme did not finish")
            saved_theme = profile / "Wii/title/00010001/48424345/data/theme.zip"
            if not saved_theme.is_file() or saved_theme.read_bytes() != theme:
                raise RuntimeError("received theme archive did not persist intact")
            print("Received theme applied without a confirmation prompt", flush=True)
        if args.wiiload_eof:
            header = b"HAXX\0\x05\0\x02\0\0\0\x04\0\0\0\0"
            compressed = zlib.compress(b"A" * 4096)
            compressed_header = (b"HAXX\0\x05\0\x02" + len(compressed).to_bytes(4, "big")
                                 + (4096).to_bytes(4, "big"))
            for name, packet in (("header", header[:8]), ("data", header + b"ab"),
                                 ("arguments", header + b"abcdx"),
                                 ("compressed data", compressed_header + compressed[:4]),
                                 ("compressed arguments", compressed_header + compressed + b"x")):
                with socket.create_connection((args.host, 4299), timeout=5) as connection:
                    connection.settimeout(3)
                    connection.sendall(packet)
                    connection.shutdown(socket.SHUT_WR)
                    if connection.recv(1) != b"":
                        raise RuntimeError(f"unexpected Wiiload {name} reply")
                if exchange(args.host, b"HBCK".ljust(16, b"\0")) != bytes(8):
                    raise RuntimeError(f"listener did not recover after Wiiload {name}")
                print(f"Interrupted Wiiload {name} closed; next client succeeded", flush=True)
        if args.reborn_key_eof:
            with socket.create_connection((args.host, 4299), timeout=5) as connection:
                connection.settimeout(3)
                connection.sendall(b"HBCK\0\x02".ljust(16, b"\0") + b"h")
                connection.shutdown(socket.SHUT_WR)
                reply = bytearray()
                while len(reply) < 8:
                    chunk = connection.recv(8 - len(reply))
                    if not chunk:
                        raise RuntimeError("truncated HBCK error reply")
                    reply.extend(chunk)
                if bytes(reply) != (-22).to_bytes(4, "big", signed=True) + bytes(4):
                    raise RuntimeError(f"unexpected HBCK error reply: {reply!r}")
            print("Interrupted HBCK body rejected without delaying the listener", flush=True)
        if args.reborn_key_transitions:
            def send_keys(payload):
                packet = (b"HBCK" + len(payload).to_bytes(2, "big")).ljust(16, b"\0") + payload
                if exchange(args.host, packet) != bytes(8):
                    raise RuntimeError("HBCK key reply failed")

            def await_transition(before, after):
                marker = f"name=screen_change from={before} to={after}"
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if marker in log.read_text(errors="replace"):
                        return
                    if process.poll() is not None:
                        raise RuntimeError("Dolphin exited while waiting for HBCK transition")
                    time.sleep(.1)
                raise RuntimeError(f"missing HBCK transition {before} -> {after}")

            send_keys(b"dh")
            await_transition("Browser", "MainMenu")
            send_keys(b"da")
            await_transition("MainMenu", "About")
            print("HBCK keys reached the application: Browser -> MainMenu -> About", flush=True)
            # Return to the browser for the existing HBCI checks.
            release = bytearray.fromhex("48424349 0100 0000 00000020 03e8 0000 0000 0000 0000 0000 0000 0000 0000 0000")
            if exchange(args.host, release) != b"HBCI\x01\0\0\0":
                raise RuntimeError("could not leave About")
            await_transition("About", "MainMenu")
            time.sleep(1.2)
            if exchange(args.host, release) != b"HBCI\x01\0\0\0":
                raise RuntimeError("could not leave MainMenu")
            await_transition("MainMenu", "Browser")
        if args.reborn_keys:
            for payload in (b"", b"x", b"h", b"b", b"x" * 64):
                packet = (b"HBCK" + len(payload).to_bytes(2, "big")).ljust(16, b"\0") + payload
                if exchange(args.host, packet) != bytes(8):
                    raise RuntimeError(f"HBCK reply failed for {len(payload)} bytes")
            packet = (b"HBCK" + (65).to_bytes(2, "big")).ljust(16, b"\0")
            if exchange(args.host, packet) != (-22).to_bytes(4, "big", signed=True) + bytes(4):
                raise RuntimeError("oversized HBCK request was not rejected")
            print("HBCK replies: empty, filtered, HOME, Back, full and oversized requests", flush=True)
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
        result["success"] = True
        print("PASS: production socket and main-loop input path in Dolphin; real Wii verification remains required",flush=True)
    except ReferenceComplete:
        pass
    except Exception as error:
        result["error"] = str(error)
        raise
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
            result["success"] = False
            result["cleanup_error"] = "Dolphin required forced termination"
        output_log.close()
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result["success"] else 1

if __name__ == "__main__": raise SystemExit(main())
