#!/usr/bin/env python3
"""Capture a C HBC control run with the Rust acceptance test's Pipe setup."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time

from dolphin_test_support import Debugger, write_dolphin_config
from dolphin_capture_config import configure_capture_profile
from test_channel_wad_dolphin import capture_screenshot
from test_hbc_ui_dolphin import RemotePipe, configure_profile, make_sd_fixture, set_initial_sort


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--app-dir', type=Path,
                        help='Load this real app directory from emulated SD instead of the three-app input fixture')
    parser.add_argument('--post-launch-seconds', type=int, default=30,
                        help='Seconds to allow the selected app to draw its first frame (5..180)')
    parser.add_argument('--load-focus-left', action='store_true',
                        help='Move left from Back to Load in Rust HBC app details')
    parser.add_argument('--verify-app-entry', action='store_true',
                        help='Check that execution reaches the selected DOL entry with its loaded bytes')
    parser.add_argument('--debug-port', type=int, default=55287)
    parser.add_argument('--mmu', action='store_true',
                        help='Enable Dolphin MMU emulation to match an MMU-enabled user profile')
    parser.add_argument('--xfb-pointer-address', type=lambda value: int(value, 0),
                        help='Matching ELF address of a 640x480 MEM1 framebuffer pointer; inspect on capture failure')
    args = parser.parse_args()
    if not 5 <= args.post_launch_seconds <= 180:
        parser.error('--post-launch-seconds must be from 5 to 180')
    image = args.image.resolve(strict=True)
    app_dir = args.app_dir.resolve(strict=True) if args.app_dir else None
    if args.verify_app_entry and app_dir is None:
        parser.error('--verify-app-entry requires --app-dir')
    if args.load_focus_left and app_dir is None:
        parser.error('--load-focus-left requires --app-dir')
    if args.xfb_pointer_address is not None and not args.verify_app_entry:
        parser.error('--xfb-pointer-address requires --verify-app-entry')
    app_identity = None
    if app_dir is not None:
        dol = app_dir / 'boot.dol'
        payload = dol.read_bytes()
        if len(payload) < 0xe4:
            parser.error('--app-dir must contain a complete boot.dol')
        app_identity = {
            'source': str(app_dir), 'path': f'sd:/apps/{app_dir.name}/boot.dol',
            'bytes': len(payload), 'sha256': hashlib.sha256(payload).hexdigest(),
        }
        manifest_path = app_dir / 'boot.manifest.json'
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text())
            if (manifest['dol_bytes'] != len(payload) or
                    manifest['dol_sha256'] != app_identity['sha256']):
                parser.error('The app DOL does not match its build manifest')
            app_identity['build_manifest'] = manifest
    args.output.mkdir(parents=True, exist_ok=False)
    result = {
        'image': str(image), 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(),
        'physical_wii_tested': False, 'capture_complete': False,
        'input_verification': 'requires screenshot review', 'stages': [],
    }
    process = None
    connection = None
    remote_fd = hotkey_fd = None
    with tempfile.TemporaryDirectory(prefix='hbc-c-input-', dir='/Volumes/SSD/WiiDev/tmp') as temporary:
        profile = Path(temporary)
        try:
            shutil.copytree(Path.home() / 'Library/Application Support/Dolphin/Wii', profile / 'Wii')
            config = profile / 'Config'
            config.mkdir()
            write_dolphin_config(config,
                f'[Core]\nWiiSDCard=False\nMMU={args.mmu}\n[Interface]\nConfirmStop=False\n'
                'UsePanicHandlers=False\n[Input]\nBackgroundInput=True\n'
                '[General]\nHotkeysRequireFocus=False\n')
            if app_dir is None:
                result['fixture'] = make_sd_fixture(profile)
            else:
                shutil.copytree(app_dir, profile / 'Load/WiiSDSync/apps' / app_dir.name)
                configure_capture_profile(profile)
                result['fixture'] = {'application': app_identity}
                result['expected_visible_result'] = 'The selected application starts and draws its screen'
            set_initial_sort(profile)
            remote_fd = configure_profile(profile, config / 'Dolphin.ini')
            if app_dir is not None:
                with (config / 'WiimoteNew.ini').open('a') as mapping:
                    mapping.write('IR/Hide = `Pipe/0/hbc-ui-remote:Button Z`\n')
            (config / 'Hotkeys.ini').write_text(
                '[Hotkeys]\nDevice = Pipe/0/priiloader-return\n'
                'General/Take Screenshot = `Button Y`\n')
            (config / 'Logger.ini').write_text(
                '[Options]\nWriteToFile=True\nVerbosity=5\n'
                '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\n'
                'PI=True\nIOS_STM=True\nIOS_ES=True\nCI=True\nIOS_NET=True\n'
                'IOS_WIIMOTE=True\nWiimote=True\n')
            fifo = profile / 'Pipes/priiloader-return'
            os.mkfifo(fifo, 0o600)
            hotkey_fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
            shutil.copytree(config, args.output / 'Config')
            with (args.output / 'console.log').open('w') as console:
                command = [
                    '/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-b',
                    '-u', str(profile), '-e', str(image)]
                if args.verify_app_entry:
                    command += ['-d', '-C', f'Dolphin.General.GDBPort={args.debug_port}',
                                '-C', 'Dolphin.Core.CPUThread=False']
                result['command'] = command
                result['load_focus_left'] = args.load_focus_left
                process = subprocess.Popen(command, stdout=console, stderr=console)
                if args.verify_app_entry:
                    deadline = time.monotonic() + 30
                    while time.monotonic() < deadline:
                        try:
                            connection = socket.create_connection(
                                ('127.0.0.1', args.debug_port), timeout=.5)
                            break
                        except OSError:
                            if process.poll() is not None:
                                raise RuntimeError('Dolphin exited before debugger startup')
                            time.sleep(.1)
                    if connection is None:
                        raise TimeoutError('Dolphin debugger did not start')
                    debug = Debugger(connection, {})
                    initial = debug.command('?')
                    parent_entry = int.from_bytes(image.read_bytes()[0xe0:0xe4], 'big')
                    entry = int.from_bytes(payload[0xe0:0xe4], 'big')
                    debug.breakpoint(parent_entry)
                    if f'40:{parent_entry:08x};' not in initial:
                        debug.send('c')
                        debug.stopped_at(parent_entry, 30)
                    debug.breakpoint(parent_entry, False)
                    debug.send('s')
                    debug.stopped(10)
                    debug.breakpoint(entry)
                    debug.send('c')
                remote = RemotePipe(remote_fd)

                def capture(stage):
                    if process.poll() is not None:
                        raise RuntimeError(f'Dolphin exited before {stage}: {process.returncode}')
                    path = capture_screenshot(hotkey_fd, profile / 'ScreenShots', args.output)
                    result['stages'].append({'name': stage, 'screenshot': str(path)})
                    print(json.dumps(result['stages'][-1]), flush=True)

                time.sleep(10)
                if app_dir is not None:
                    # Hide the pointer, then choose the sole entry with the
                    # D-pad. C Reborn focuses Load when details open.
                    remote.send('PRESS Z')
                    capture('app-browser')
                    remote.button('D_DOWN')
                    remote.button('A')
                    time.sleep(1)
                    capture('app-details')
                    if args.load_focus_left:
                        remote.button('D_LEFT')
                        capture('app-load-selected')
                    remote.button('A')
                    if args.verify_app_entry:
                        result['child_entry_stop'] = debug.stopped_at(entry, 180)
                        loaded = debug.memory(entry, 32)
                        expected = None
                        for index in range(7):
                            offset = int.from_bytes(payload[index*4:index*4+4], 'big')
                            address = int.from_bytes(payload[0x48+index*4:0x4c+index*4], 'big')
                            length = int.from_bytes(payload[0x90+index*4:0x94+index*4], 'big')
                            if address <= entry and entry + 32 <= address + length:
                                start = offset + entry - address
                                expected = payload[start:start+32]
                                break
                        if expected is None or len(expected) != 32:
                            raise RuntimeError('App entry is outside its DOL text sections')
                        if loaded[:8] != expected[:8] or loaded[20:] != expected[20:]:
                            raise RuntimeError('Loaded child entry does not match the selected DOL')
                        result['child_entry_verified'] = True
                        (args.output / 'child-entry.bin').write_bytes(loaded)
                        print('Selected DOL entry reached and bytes verified', flush=True)
                        debug.breakpoint(entry, False)
                        debug.send('c')
                    time.sleep(args.post_launch_seconds)
                    capture('app-after-load')
                    result['capture_complete'] = True
                    return
                remote.axes(0.5, 0.5)
                time.sleep(2)
                capture('pointer-center')
                remote.axes(0.25, 0.25)
                time.sleep(1)
                capture('pointer-upper-left')
                remote.axes(0.75, 0.75)
                time.sleep(1)
                capture('pointer-lower-right')
                remote.axes(0.5, 0.5)
                remote.button('1')
                time.sleep(1)
                capture('button-1')
                remote.button('B')
                time.sleep(1)
                remote.axes(0.5, 0.5)
                remote.click()
                time.sleep(1)
                capture('pointer-click-app-details')
                remote.button('B')
                time.sleep(1)
                remote.button('2')
                time.sleep(1)
                capture('button-2')
                result['capture_complete'] = True
        except Exception as error:
            result['error'] = str(error)
            if connection is not None and result.get('child_entry_verified'):
                try:
                    connection.sendall(b'\x03')
                    result['failure_stop'] = debug.stopped(10)
                    # Dolphin can send an empty packet after the interrupt.
                    debug.send('p40')
                    reply = debug.packet()
                    while not reply or reply.startswith(('T', 'S')):
                        reply = debug.packet()
                    result['failure_pc'] = int(reply, 16)
                    result['failure_lr'] = int(debug.command('p43'), 16)
                    stack = int(debug.command('p1'), 16)
                    result['failure_stack'] = []
                    for _ in range(24):
                        if not 0x80000000 <= stack <= 0x817ffff0:
                            break
                        frame = debug.memory(stack, 16)
                        result['failure_stack'].append(
                            {'sp': stack, 'saved_lr': int.from_bytes(frame[4:8], 'big')})
                        next_stack = int.from_bytes(frame[:4], 'big')
                        if next_stack <= stack:
                            break
                        stack = next_stack
                    if args.xfb_pointer_address is not None:
                        from xfb_capture import summarize_yuyv_luma, write_yuyv_png
                        pointer = int.from_bytes(debug.memory(args.xfb_pointer_address, 4), 'big')
                        address = (pointer & 0x3fffffff) | 0x80000000
                        if not 0x80000000 <= address <= 0x81800000 - 640*480*2:
                            raise RuntimeError(f'Invalid app framebuffer pointer: {pointer:#x}')
                        frame = debug.memory(address, 640*480*2)
                        result['failure_xfb'] = summarize_yuyv_luma(frame, 640, 480)
                        write_yuyv_png(args.output / 'failure-xfb.png', frame, 640, 480)
                except Exception as inspection:
                    result['failure_inspection_error'] = str(inspection)
            raise
        finally:
            if connection is not None:
                connection.close()
            if process is not None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                    result['cleanup_error'] = 'Dolphin required forced termination'
                result['dolphin_exit_code'] = process.returncode
            for fd in (remote_fd, hotkey_fd):
                if fd is not None:
                    os.close(fd)
            log = profile / 'Logs/dolphin.log'
            if log.is_file():
                shutil.copy2(log, args.output / 'dolphin.log')
            (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
