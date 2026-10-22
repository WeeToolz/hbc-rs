#!/usr/bin/env python3
"""Test Reset during cooperative pause in an isolated Dolphin profile.

Run through Terminal. The named pipe drives Dolphin hotkeys. It does not send
memory, register, breakpoint, or exception commands to either debugger.
Keep the profile under WiiDev/tmp. Never publish its NAND files.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

from test_network_gdb_recovery_dolphin import arm, connect, paused, ready, check_diagnostics, check_wiiload
from test_network_gdb_dolphin import status
from test_remote_input_dolphin import own_processes

WORKSPACE = Path('/Volumes/SSD/WiiDev')
ROOT = Path(__file__).resolve().parents[1]


def tap(fd, button):
    for action in ('PRESS', 'RELEASE'):
        packet = f'{action} {button}\n'.encode('ascii')
        if os.write(fd, packet) != len(packet):
            raise RuntimeError('Incomplete Dolphin pipe write')
        time.sleep(0.25)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--nand', type=Path,
                        default=WORKSPACE / 'dolphin-network-test-nand/Wii',
                        help='Source NAND. The test changes only a temporary copy.')
    parser.add_argument('--dolphin-only', action='store_true', required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--cooperative-only', action='store_true', help='Check cooperative state and listener diagnostics; skip Reset')
    modes.add_argument('--startup-failure', action='store_true', help='Hold port 2828 to force a real bind failure; verify diagnostics through 4299')
    args = parser.parse_args()
    # Require the current host address. Do not let this test reach a real Wii.
    addresses = subprocess.check_output(['/sbin/ifconfig'], text=True)
    if f'inet {args.host} ' not in addresses:
        parser.error('--host must be an address assigned to this Mac')
    dol = args.dol.resolve(strict=True)
    for port in (2828, 4299):
        with socket.socket() as check:
            check.bind((args.host, port))
    args.output.mkdir(parents=True, exist_ok=False)
    profile = Path(tempfile.mkdtemp(prefix='hbc-gdb-reset-', dir=WORKSPACE / 'tmp'))
    result = {'dol': str(dol), 'sha256': hashlib.sha256(dol.read_bytes()).hexdigest(),
              'size': dol.stat().st_size, 'profile': str(profile),
              'paused': False, 'gdb_closed_after_reset': False,
              'return_to_menu': 'unconfirmed', 'success': False}
    fd = None
    sock = None
    blocked_listener = None
    log = profile / 'Logs/dolphin.log'
    try:
        nand = args.nand.resolve(strict=True)
        for relative in ('shared2/sys/SYSCONF', 'shared2/sys/net/02/config.dat'):
            if not (nand / relative).is_file():
                raise RuntimeError('Required isolated NAND configuration is absent')
        result['system_menu_installed'] = (nand / 'title/00000001/00000002/content/title.tmd').is_file()
        shutil.copytree(nand, profile / 'Wii')
        config = profile / 'Config'
        config.mkdir()
        (config / 'Dolphin.ini').write_text(
            '[Core]\nWiiSDCard=False\nCPUThread=False\n'
            '[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n'
            '[Input]\nBackgroundInput=True\n'
            '[General]\nHotkeysRequireFocus=False\n')
        (config / 'Hotkeys.ini').write_text(
            '[Hotkeys]\nDevice = Pipe/0/network-gdb\n'
            'General/Reset = `Button X`\nGeneral/Take Screenshot = `Button Y`\n')
        (config / 'Logger.ini').write_text(
            '[Options]\nWriteToFile=True\nVerbosity=4\n'
            '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\n'
            'PI=True\nIOS_STM=True\nIOS_ES=True\nCI=True\n')
        pipes = profile / 'Pipes'
        pipes.mkdir()
        pipe = pipes / 'network-gdb'
        os.mkfifo(pipe, 0o600)
        fd = os.open(pipe, os.O_RDWR | os.O_NONBLOCK)
        if args.startup_failure:
            blocked_listener = socket.socket()
            blocked_listener.bind((args.host, 2828))
            blocked_listener.listen(1)
        subprocess.run(['/usr/bin/open', '-n', '-a', '/Applications/Dolphin.app', '--args',
                        '-b', '-u', str(profile), '-e', str(dol)], check=True)
        if args.startup_failure:
            deadline = time.monotonic() + 45
            while True:
                try:
                    result['startup_failure'] = check_diagnostics(args.host, startup_failed=True)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.25)
            blocked_listener.close()
            blocked_listener = None
            # Failure must remain readable after the GDB listener is absent.
            result['retained_startup_failure'] = check_diagnostics(args.host, startup_failed=True)
            check_wiiload(args.host)
            result['success'] = True
        else:
            deadline = time.monotonic() + 45
            last = None
            while time.monotonic() < deadline:
                try:
                    sock = connect(args.host, 2828)
                    ready(sock)
                    break
                except (OSError, RuntimeError) as error:
                    last = str(error)
                    if sock is not None:
                        sock.close()
                        sock = None
                    time.sleep(0.25)
            else:
                raise RuntimeError(f'Dolphin GDB startup failed: {last}')
            sock.close()
            sock = None
            subprocess.run([sys.executable, str(ROOT / 'tools/test_network_gdb_recovery_dolphin.py'),
                            '--host', args.host, '--dolphin-only', '--wiiload', '--diagnostics'], check=True)
            result['disconnect_cycles_wiiload'] = 'passed'
            if args.cooperative_only:
                result['success'] = True
            else:
                time.sleep(0.25)
                sock = connect(args.host, 2828)
                ready(sock)
                tap(fd, 'Y')
                time.sleep(1)
                arm(sock)
                paused(sock)
                result['paused'] = True
                tap(fd, 'Y')
                print('ACTION: Dolphin Reset while paused; keep GDB connected; no resume', flush=True)
                started = time.monotonic()
                tap(fd, 'X')
                sock.settimeout(0.5)
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    try:
                        block = sock.recv(4096)
                        if not block:
                            result['gdb_closed_after_reset'] = True
                            result['gdb_close_seconds'] = round(time.monotonic() - started, 3)
                            break
                    except ConnectionResetError:
                        result['gdb_closed_after_reset'] = True
                        result['gdb_close_seconds'] = round(time.monotonic() - started, 3)
                        break
                    except socket.timeout:
                        continue
                time.sleep(5)
                tap(fd, 'Y')
                if not result['gdb_closed_after_reset']:
                    sock.settimeout(5)
                    result['status_after_reset'] = status(sock)
                    raise RuntimeError('GDB remained connected after Dolphin Reset during pause')
                print('OBSERVED: GDB closed without resume or client disconnect', flush=True)
                # Socket closure is evidence of cleanup, not proof of the System Menu.
                if not result['system_menu_installed']:
                    raise RuntimeError('System Menu is absent from test NAND; return cannot be qualified')
                raise RuntimeError('System Menu screen requires review; return remains unconfirmed')
    except Exception as error:
        result['error'] = str(error)
        print(f'FAIL: {error}', flush=True)
    finally:
        if blocked_listener is not None:
            blocked_listener.close()
        if sock is not None:
            sock.close()
            # Disconnect releases a cooperative pause. Give queued screenshots
            # time to finish before process cleanup. These are post-test images.
            time.sleep(1)
        if fd is not None:
            os.close(fd)
        try:
            for pid in own_processes(profile):
                os.kill(pid, signal.SIGTERM)
            deadline = time.monotonic() + 5
            while own_processes(profile) and time.monotonic() < deadline:
                time.sleep(0.1)
            if own_processes(profile):
                raise RuntimeError('Isolated Dolphin did not stop; process retained')
        except Exception as error:
            result['cleanup_error'] = str(error)
            result['success'] = False
        if log.exists():
            shutil.copy2(log, args.output / 'dolphin.log')
        for screenshot in (profile / 'ScreenShots').rglob('*.png'):
            shutil.copy2(screenshot, args.output / screenshot.name)
        (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
