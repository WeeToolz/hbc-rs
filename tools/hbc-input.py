#!/usr/bin/env python3
"""Send bounded controller input to Rust HBC. Uses Python's standard library."""
import argparse
import socket
import struct
import sys

KINDS = {"logical": 0, "gamecube": 1, "wiimote": 2, "classic": 3,
         "nunchuk": 4, "guitar": 5, "generic": 6, "joycon-left": 7,
         "joycon-right": 8, "joycon-pair": 9, "switch-pro": 10, "xbox": 11}
LOGICAL = {"up": 1, "down": 2, "left": 4, "right": 8, "a": 16,
           "b": 32, "home": 64, "2": 128, "minus": 512, "plus": 1024, "1": 2048}
NORMAL = dict(zip(("a", "b", "x", "y", "up", "down", "left", "right",
                   "start", "select", "home", "l", "r", "zl", "zr",
                   "l-stick", "r-stick", "sl", "sr", "capture", "z"),
                  (1 << n for n in range(21))))
PAD = {"left": 1, "right": 2, "down": 4, "up": 8, "z": 16,
       "r": 32, "l": 64, "a": 256, "b": 512, "x": 1024, "y": 2048, "start": 4096}
WII = {"2": 1, "1": 2, "b": 4, "a": 8, "minus": 16, "home": 128,
       "left": 256, "right": 512, "down": 1024, "up": 2048, "plus": 4096}
CLASSIC = {name: bit << 16 for name, bit in {
    "up": 1, "left": 2, "zr": 4, "x": 8, "a": 16, "y": 32,
    "b": 64, "zl": 128, "r": 512, "plus": 1024, "home": 2048,
    "minus": 4096, "l": 8192, "down": 16384, "right": 32768}.items()}
RESULTS = {1: "invalid command", 2: "input queue busy", 3: "command expired before the main loop read it",
           4: "no main-loop completion within timeout; command may still be pending",
           5: "receiver stopped"}


def make_packet(args):
    bindings = LOGICAL if args.kind == "logical" else NORMAL
    if args.kind == "gamecube": bindings = PAD
    elif args.kind in ("wiimote", "nunchuk", "guitar"): bindings = WII.copy()
    elif args.kind == "classic": bindings = CLASSIC.copy()
    if args.kind == "guitar":
        bindings.update({name: bit << 16 for name, bit in {
            "strum-up": 1, "strum-down": 0x4000, "green": 0x10, "red": 0x40,
            "yellow": 8, "blue": 0x20, "orange": 0x80,
            "guitar-plus": 0x400, "guitar-minus": 0x1000}.items()})
    if args.kind == "nunchuk": bindings.update({"z": 1 << 16, "c": 2 << 16})
    if args.kind in ("generic", "xbox", "switch-pro", "joycon-left", "joycon-right", "joycon-pair"):
        bindings = bindings.copy()
        bindings.update({"back": bindings["select"], "menu": bindings["start"],
                         "lb": bindings["l"], "rb": bindings["r"],
                         "lt": bindings["zl"], "rt": bindings["zr"],
                         "l3": bindings["l-stick"], "r3": bindings["r-stick"]})
    mask = args.mask
    for name in args.buttons:
        for button in name.lower().replace(",", "+").split("+"):
            if button not in bindings:
                raise ValueError(f"unknown {args.kind} button {button!r}; choose: {', '.join(bindings)}")
            mask |= bindings[button]
    duration = 0 if args.release else args.hold_ms
    if not 0 <= mask <= 0xffffffff: raise ValueError("mask must fit in 32 bits")
    if not 1 <= args.hold_ms <= 2000: raise ValueError("hold time must be 1..2000 ms")
    if not -512 <= args.scroll <= 512: raise ValueError("scroll must be -512..512")
    axes = args.left_stick + args.right_stick
    if any(not -32767 <= axis <= 32767 for axis in axes): raise ValueError("stick values must be -32767..32767")
    if any(not 0 <= value <= 255 for value in args.triggers): raise ValueError("triggers must be 0..255")
    if args.kind in ("logical", "wiimote", "nunchuk", "guitar") and any(args.triggers): raise ValueError("analog triggers require GameCube, Classic, or a normalized controller kind")
    if args.pointer and any(not 0 <= value <= 4095 for value in args.pointer): raise ValueError("pointer values must be 0..4095")
    flags = int(args.pointer is not None) | (int(args.screenshot) << 1)
    pointer = args.pointer or [0, 0]
    if args.release and (mask or flags or args.scroll or any(axes) or any(args.triggers)):
        raise ValueError("--release cannot be combined with active input")
    return struct.pack(">4sBBHIHhhhhhhhBBH", b"HBCI", 1, KINDS[args.kind], flags,
                       mask, duration, *pointer, args.scroll, *axes, *args.triggers, 0)


def send_packet(host, port, packet, timeout=5):
    with socket.create_connection((host, port), timeout=timeout) as connection:
        connection.settimeout(timeout)
        connection.sendall(packet)
        reply = bytearray()
        while len(reply) < 8:
            chunk = connection.recv(8 - len(reply))
            if not chunk: raise OSError("connection closed before the complete input reply")
            reply.extend(chunk)
    if reply[:5] != b"HBCI\x01" or reply[6:] != b"\0\0":
        raise OSError(f"invalid input reply: {reply.hex()}")
    if reply[5] != 0:
        raise OSError(RESULTS.get(reply[5], f"unknown reply status {reply[5]}"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wii", required=True, help="Wii IP address")
    parser.add_argument("--port", type=int, default=4299)
    parser.add_argument("--kind", choices=KINDS, default="logical")
    parser.add_argument("buttons", nargs="*", help="button names, separated by spaces or +")
    parser.add_argument("--mask", type=lambda value: int(value, 0), default=0, help="raw button mask; includes unnamed buttons")
    parser.add_argument("--hold-ms", type=int, default=120, help="hold duration, 1..2000 ms")
    parser.add_argument("--release", action="store_true", help="release all remote input now")
    parser.add_argument("--pointer", type=int, nargs=2, metavar=("X", "Y"))
    parser.add_argument("--left-stick", type=int, nargs=2, default=[0, 0], metavar=("X", "Y"))
    parser.add_argument("--right-stick", type=int, nargs=2, default=[0, 0], metavar=("X", "Y"))
    parser.add_argument("--triggers", type=int, nargs=2, default=[0, 0], metavar=("L", "R"))
    parser.add_argument("--scroll", type=int, default=0)
    parser.add_argument("--screenshot", action="store_true")
    args = parser.parse_args(argv)
    try:
        packet = make_packet(args)
        send_packet(args.wii, args.port, packet)
    except (ValueError, OSError, struct.error) as error:
        print(f"hbc-input: {error}", file=sys.stderr)
        return 1
    print("Input merged into the HBC main-loop frame. This does not confirm the result of a menu action.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
