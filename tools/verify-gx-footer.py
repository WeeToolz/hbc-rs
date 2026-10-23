"""Capture the real HBC GX footer path in an isolated 640x480 Dolphin profile.

Invalid-opcode checks are automatic. PNG screen transitions require visual review.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import struct
import zlib

root = Path(__file__).resolve().parents[1]
from dolphin_capture_config import configure_capture_profile
from xfb_capture import summarize_yuyv_luma
from dolphin_test_support import Debugger
parser = argparse.ArgumentParser(description='Capture production HBC notice, footer, menu, and Options paths in Dolphin.')
parser.add_argument('--label', default='fixed')
parser.add_argument('--dol', type=Path, default=root / 'target/hbc-rs.dol')
parser.add_argument('--elf', type=Path, default=root / 'target/powerpc-nintendo-none/release/hbc-rs')
parser.add_argument('--dolphin', type=Path, default=Path('/Applications/Dolphin.app/Contents/MacOS/Dolphin'))
parser.add_argument('--idle-seconds', type=int, default=60)
parser.add_argument('--check-network-button', action='store_true', help='Select Network setup and capture the settings load result without saving.')
parser.add_argument('--expect-fifo-errors', action='store_true', help='Use only to confirm a failing baseline; errors remain logged.')
args = parser.parse_args()
if not args.label or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_' for c in args.label):
    parser.error('label must use letters, digits, underscores, or hyphens')
if args.idle_seconds < 0:
    parser.error('idle seconds must not be negative')
dol, elf = args.dol.resolve(), args.elf.resolve()
dol_hash = hashlib.sha256(dol.read_bytes()).hexdigest()
output = root / 'target' / ('gx-footer-' + args.label)
profile = output / 'profile'
with socket.socket() as port_check:
    port_check.bind(('127.0.0.1', 55268))
configure_capture_profile(profile)
config = profile / 'Config'
(config / 'Dolphin.ini').write_text('[Core]\nCPUThread = False\nSIDevice0 = 6\nSIDevice1 = 0\nSIDevice2 = 0\nSIDevice3 = 0\nWiiSDCard = False\n[Interface]\nConfirmStop = False\nUsePanicHandlers = False\n[Input]\nBackgroundInput = True\n')
(config / 'GCPadNew.ini').write_text('[GCPad1]\nDevice = Pipe/0/footer\nButtons/A = `Button A`\nButtons/B = `Button B`\nButtons/X = `Button X`\nButtons/Start = `Button START`\nD-Pad/Down = `Button D_DOWN`\nD-Pad/Up = `Button D_UP`\n')
(config / 'Logger.ini').write_text('[Options]\nWriteToFile = True\nWriteToConsole = True\nVerbosity = 3\n[Logs]\nVIDEO = True\nCOMMANDPROCESSOR = True\nOSREPORT = True\nCONTROLLERINTERFACE = True\n')
pipes = profile / 'Pipes'
pipes.mkdir(exist_ok=True)
pipe = pipes / 'footer'
if pipe.exists():
    pipe.unlink()
os.mkfifo(pipe)
pipe_fd = os.open(pipe, os.O_RDWR | os.O_NONBLOCK)
symbols = subprocess.check_output(['/opt/homebrew/opt/llvm/bin/llvm-nm', '-nC', str(elf)], text=True)
xfb_symbol = next(int(line.split()[0], 16) for line in symbols.splitlines() if 'ogc_video::LAST_NEXT_FRAMEBUFFER ' in line)
log_path = output / 'dolphin.log'
log = log_path.open('w')
proc = subprocess.Popen([str(args.dolphin), '-b', '-u', str(profile), '-e', str(dol), '-C', 'Dolphin.General.GDBPort=55268'], stdout=log, stderr=log)
sock = None
debugger = None

def button(name, pressed):
    if name in ("UP", "DOWN", "LEFT", "RIGHT"):
        name = "D_" + name
    payload = f"{'PRESS' if pressed else 'RELEASE'} {name}\n".encode('ascii')
    if os.write(pipe_fd, payload) != len(payload):
        raise RuntimeError('incomplete Dolphin controller command')

def run(seconds):
    debugger.send('c')
    time.sleep(seconds)
    sock.sendall(b'\x03')
    stop = debugger.stopped()
    print('stop', stop, flush=True)
    return stop

def capture(name):
    global capture_failure
    pointer = int.from_bytes(debugger.memory(xfb_symbol, 4), 'big')
    if not 0x80000000 <= pointer < 0x94000000:
        raise RuntimeError('invalid XFB pointer')
    address = (pointer & 0x3fffffff) | 0x80000000
    length = 640 * 480 * 2
    xfb = b''.join(debugger.memory(address + offset, min(4096, length - offset)) for offset in range(0, length, 4096))
    (output / (name + '.xfb')).write_bytes(xfb)
    try:
        summary = summarize_yuyv_luma(xfb, 640, 480)
    except ValueError as error:
        capture_failure = f'{name}: {error}'
        print('capture rejected:', capture_failure, flush=True)
        raise
    rgb = bytearray()
    for offset in range(0, len(xfb), 4):
        y0, u, y1, v = xfb[offset:offset + 4]
        for y in (y0, y1):
            c, d, e = y - 16, u - 128, v - 128
            channels = ((298*c + 409*e + 128) >> 8,
                        (298*c - 100*d - 208*e + 128) >> 8,
                        (298*c + 516*d + 128) >> 8)
            rgb.extend(max(0, min(255, value)) for value in channels)
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\x00' + rgb[y*1920:(y+1)*1920] for y in range(480))
    png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 640, 480, 8, 2, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))
    (output / (name + '.png')).write_bytes(png)
    print('capture', name, hex(pointer), summary, flush=True)

completed = False
capture_failure = None
started = time.monotonic()
try:
    for _ in range(100):
        try:
            sock = socket.create_connection(('127.0.0.1', 55268), timeout=.5)
            break
        except OSError:
            if proc.poll() is not None:
                raise RuntimeError('Dolphin exited at startup')
            time.sleep(.1)
    if sock is None:
        raise RuntimeError('Dolphin GDB port did not open')
    sock.settimeout(20)
    debugger = Debugger(sock, {})
    print('initial', debugger.command('?'), flush=True)
    run(12)
    capture('notice')
    button('A', True)
    run(1)
    button('A', False)
    run(3)
    capture('browser')
    if not args.expect_fifo_errors:
        button('START', True)
        run(1)
        button('START', False)
        run(2)
        capture('menu')
        button('B', True)
        run(1)
        button('B', False)
        run(5)
        capture('browser-return')
        button('X', True)
        run(1)
        button('X', False)
        run(2)
        capture('options')
        run(3)
        capture('options-idle')
        if args.check_network_button:
            # Options opens with Back (7) focused. Network setup (8) is
            # directly above it, so follow the rendered layout with Up.
            button('UP', True)
            run(1)
            button('UP', False)
            run(1)
            capture('network-focus')
            button('A', True)
            run(1)
            button('A', False)
            run(2)
            capture('network-load-result')
            button('B', True)
            run(1)
            button('B', False)
            run(1)
        button('B', True)
        run(1)
        button('B', False)
        run(3)
        capture('browser-after-options')
        remaining = args.idle_seconds
        while remaining:
            interval = min(20, remaining)
            run(interval)
            remaining -= interval
        capture('browser-idle')
    completed = True
finally:
    if sock:
        sock.close()
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    os.close(pipe_fd)
    log.close()
    data = log_path.read_text(errors='replace')
    print('Unknown Opcode occurrences:', data.count('Unknown Opcode'), flush=True)
    result = {
        'dol': str(dol),
        'fifo_error_count': data.count('Unknown Opcode'),
        'dolphin_exit': proc.returncode,
        'completed': completed,
        'dol_sha256': dol_hash,
        'elapsed_seconds': round(time.monotonic() - started, 2),
        'idle_seconds': args.idle_seconds,
        'network_button_checked': args.check_network_button,
        'expected_fifo_errors': args.expect_fifo_errors,
        'visual_review_required': True,
        'capture_failure': capture_failure,
    }
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')

if args.expect_fifo_errors:
    if 'Unknown Opcode' not in data:
        raise SystemExit('FAIL: baseline did not reproduce the FIFO error')
elif 'Unknown Opcode' in data:
    raise SystemExit('FAIL: Dolphin reported an invalid GX opcode')
print('Capture complete. Review the PNG files for the requested screen transitions.', flush=True)
