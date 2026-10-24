#!/usr/bin/env python3
"""Capture a C HBC control run with the Rust acceptance test's Pipe setup."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from dolphin_test_support import write_dolphin_config
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
    args = parser.parse_args()
    if not 5 <= args.post_launch_seconds <= 180:
        parser.error('--post-launch-seconds must be from 5 to 180')
    image = args.image.resolve(strict=True)
    app_dir = args.app_dir.resolve(strict=True) if args.app_dir else None
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
    remote_fd = hotkey_fd = None
    with tempfile.TemporaryDirectory(prefix='hbc-c-input-', dir='/Volumes/SSD/WiiDev/tmp') as temporary:
        profile = Path(temporary)
        try:
            shutil.copytree(Path.home() / 'Library/Application Support/Dolphin/Wii', profile / 'Wii')
            config = profile / 'Config'
            config.mkdir()
            write_dolphin_config(config,
                '[Core]\nWiiSDCard=False\n[Interface]\nConfirmStop=False\n'
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
                process = subprocess.Popen([
                    '/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-b',
                    '-u', str(profile), '-e', str(image)], stdout=console, stderr=console)
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
                    remote.button('A')
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
            raise
        finally:
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
