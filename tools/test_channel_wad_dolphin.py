#!/usr/bin/env python3
"""Boot a prepared WAD in a disposable Dolphin NAND and check production startup."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from dolphin_test_support import Debugger, installed_title, write_dolphin_config


def tap(fd, button):
    for action in ('PRESS', 'RELEASE'):
        packet = f'{action} {button}\n'.encode('ascii')
        if os.write(fd, packet) != len(packet):
            raise RuntimeError('Incomplete Dolphin pipe write')
        time.sleep(0.25)


def capture_screenshot(fd, directory, output):
    directory.mkdir(exist_ok=True)
    before = set(directory.rglob('*.png'))
    tap(fd, 'Y')
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        images = [path for path in directory.rglob('*.png') if path not in before]
        if images:
            image = max(images, key=lambda path: path.stat().st_mtime_ns)
            time.sleep(0.25)
            target = output / f'dolphin-{time.strftime("%Y%m%d-%H%M%S")}-{time.monotonic_ns()}.png'
            shutil.copy2(image, target)
            return target
        time.sleep(0.1)
    raise TimeoutError('Dolphin did not save a screenshot after the Y hotkey')


def validate_self_load_log(text):
    before, separator, child = text.partition('HBC: transferring executable control')
    checks = {
        'parent_startup_complete': 'HBC: startup complete; entering render loop' in before,
        'transfer_control': bool(separator),
        'child_startup_complete': 'HBC: startup complete; entering render loop' in child,
        'child_listener_listening': 'HBC: Wiiload receiver state changed to Listening' in child,
    }
    if not all(checks.values()):
        raise RuntimeError(f'Self-load log evidence is incomplete: {checks}')
    return checks


def validate_storage_launch_log(text, no_ios_reload):
    checks = validate_self_load_log(text)
    before, _, handoff = text.partition('HBC: transferring executable control')
    expected = f'HBC: launch IOS policy: reload={str(not no_ios_reload).lower()}, ahb_access=false'
    if expected not in handoff or 'HBC: selected executable read and validated' not in before:
        raise RuntimeError('SD launch did not prove the expected metadata policy')
    actual_launches = handoff.count('Launching title 000000010000003a...')
    if actual_launches != (0 if no_ios_reload else 1):
        raise RuntimeError('Dolphin IOS58 launch events disagree with metadata')
    checks['actual_ios58_launches_after_handoff'] = actual_launches
    return checks


def self_load(args, result, profile, hotkey_fd, ui_remote_fd, process, started):
    """Measure startup and the real Wiiload or SD application launch path."""
    spec = importlib.util.spec_from_file_location('hbc_control', Path(__file__).with_name('hbc-control.py'))
    control = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(control)
    result['screenshots'] = []

    def screenshot(stage):
        try:
            path = capture_screenshot(hotkey_fd, profile / 'ScreenShots', args.output)
            result['screenshots'].append({'stage': stage, 'path': str(path),
                                          'seconds_since_launch': round(time.monotonic() - started, 3)})
        except Exception as error:
            result.setdefault('screenshot_errors', []).append({'stage': stage, 'error': str(error)})

    def wait_ready(stage, origin, previous_session=None):
        deadline = origin + args.startup_timeout
        next_screenshot = origin + 5
        last_error = None
        last_reply = None
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f'Dolphin exited during {stage}: {process.returncode}')
            try:
                last_reply = control.exchange(args.host, 4299, 0, 0, timeout=1)
                if previous_session is None or last_reply['session'] != previous_session:
                    result[stage] = {'seconds': round(time.monotonic() - origin, 3),
                                     'reply': last_reply}
                    print(json.dumps({stage: result[stage]}), flush=True)
                    screenshot(stage + '-ready')
                    return last_reply['session']
            except (OSError, ValueError, UnicodeError) as error:
                last_error = str(error)
            if time.monotonic() >= next_screenshot:
                screenshot(stage + '-waiting')
                next_screenshot = time.monotonic() + 30
            time.sleep(0.25)
        result[stage] = {'ready': False, 'timeout_seconds': args.startup_timeout,
                         'last_reply': last_reply, 'last_error': last_error}
        raise TimeoutError(f'{stage} did not produce the required HBC session')

    first_session = wait_ready('wad_startup', started)
    if args.ui_acceptance:
        ui_test = importlib.util.spec_from_file_location(
            'hbc_ui_dolphin', Path(__file__).with_name('test_hbc_ui_dolphin.py'))
        ui = importlib.util.module_from_spec(ui_test)
        ui_test.loader.exec_module(ui)
        ui.run_acceptance(args, result, profile, ui_remote_fd, process, screenshot)
        required_screens = {
            'wad_startup-ready', 'ui-bubble-pop', 'ui-date-sort', 'ui-name-sort',
            'ui-network-settings', 'ui-network-scan-error',
        }
        if not required_screens.issubset({shot['stage'] for shot in result['screenshots']}):
            raise RuntimeError('The UI acceptance run is missing a required screenshot')
        return
    if args.storage_dol:
        # Wait for the real FAT mount and catalog scan before driving the UI.
        log_path = profile / 'Logs/dolphin.log'
        deadline = time.monotonic() + args.startup_timeout
        while 'HBC: storage scan applied: volume=Some(Sd), apps=1' not in log_path.read_text(errors='replace'):
            if time.monotonic() >= deadline or process.poll() is not None:
                raise TimeoutError('The real SD catalog did not contain the test app')
            time.sleep(0.25)
        input_spec = importlib.util.spec_from_file_location('hbc_input', Path(__file__).with_name('hbc-input.py'))
        inputs = importlib.util.module_from_spec(input_spec)
        input_spec.loader.exec_module(inputs)

        def button(name):
            if inputs.main(['--wii', args.host, '--kind', 'logical', '--hold-ms', '100', name]) != 0:
                raise RuntimeError(f'HBC did not apply the {name} input')
            time.sleep(0.15)
            if inputs.main(['--wii', args.host, '--kind', 'logical', '--release']) != 0:
                raise RuntimeError('HBC did not apply the input release')

        button('a')  # Open details. Initial focus is Back.
        button('left')  # Select Load.
        screenshot('sd-app-details')
        launch_started = time.monotonic()
        # The final A can close the receiver before its completion response is
        # delivered. Send once, record that error, and require child evidence.
        try:
            button('a')
        except (OSError, RuntimeError) as error:
            result['launch_input_reply_error'] = str(error)
        wait_ready('dol_startup_from_sd', launch_started, first_session)
        text = log_path.read_text(errors='replace')
        result['launch_log_checks'] = validate_storage_launch_log(text, args.no_ios_reload)
        result['ios_reloaded'] = result['launch_log_checks']['actual_ios58_launches_after_handoff'] != 0
        result['launch_source'] = 'sd:/apps/selfload/boot.dol'
        required_screens = {'wad_startup-ready', 'sd-app-details', 'dol_startup_from_sd-ready'}
        if args.capture_options:
            deadline = time.monotonic() + args.startup_timeout
            while log_path.read_text(errors='replace').count('HBC: storage scan applied: volume=Some(Sd), apps=1') < 2:
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise TimeoutError('Child SD scan did not finish before Options capture')
                time.sleep(0.25)
            button('1')
            time.sleep(3)
            screenshot('child-storage-options')
            required_screens.add('child-storage-options')
        if not required_screens.issubset({shot['stage'] for shot in result['screenshots']}):
            raise RuntimeError('The SD launch is missing a required screenshot')
        return
    upload_started = time.monotonic()
    result['upload_started_seconds_since_launch'] = round(upload_started - started, 3)
    with (args.output / 'upload.log').open('w') as log:
        upload = subprocess.run([str(args.sender.resolve()), 'send', str(args.self_load_dol.resolve()),
                                 '--destination', f'tcp:{args.host}', '--summary',
                                 str((args.output / 'upload.json').resolve())],
                                stdout=log, stderr=log, timeout=60)
    result['sender_exit_code'] = upload.returncode
    result['upload_seconds'] = round(time.monotonic() - upload_started, 3)
    if upload.returncode:
        raise RuntimeError(f'Wiiload sender failed: {upload.returncode}; see upload.log')
    result['upload_completed_seconds_since_launch'] = round(time.monotonic() - started, 3)
    wait_ready('dol_startup_after_upload', upload_started, first_session)
    result['launch_log_checks'] = validate_self_load_log((profile / 'Logs/dolphin.log').read_text(errors='replace'))
    if not result['screenshots']:
        raise RuntimeError('No Dolphin screenshot was saved')


def probe_wiiload(host):
    """Send Wiiload-RS's complete zero-length readiness request."""
    with socket.create_connection((host, 4299), timeout=1) as connection:
        connection.settimeout(2)
        connection.sendall(b'HAXX\0\x05' + bytes(10))
        connection.shutdown(socket.SHUT_WR)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wad', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--nand', type=Path,
                        help='Source NAND to copy into the disposable Dolphin profile')
    parser.add_argument('--host', help='Mac address used to check HBC Wiiload after Priiloader return')
    parser.add_argument('--priiloader-recovery', action='store_true',
                        help='Wait for SYS_RETURNTOMENU and verify Priiloader starts installed HBC')
    parser.add_argument('--recovery-timeout', type=int, default=100,
                        help='Seconds to wait for HBC after the recovery WAD returns to the menu')
    parser.add_argument('--self-load-dol', type=Path,
                        help='Measure normal WAD startup, then upload its matching DOL through Wiiload')
    parser.add_argument('--storage-dol', type=Path,
                        help='Put the matching DOL on an emulated SD card and launch it through the browser')
    parser.add_argument('--ui-acceptance', action='store_true',
                        help='Test the real emulated Wii Remote, sorting, bubbles and network scan UI')
    parser.add_argument('--capture-options', action='store_true',
                        help='Capture live storage status after the SD child starts')
    parser.add_argument('--no-ios-reload', action='store_true',
                        help='Add no_ios_reload to the SD app metadata')
    parser.add_argument('--sender', type=Path,
                        default=Path('/Volumes/SSD/WiiDev/Devtools/Wiiload-rs/target/release/wiiload-rs'))
    parser.add_argument('--startup-timeout', type=int, default=180)
    args = parser.parse_args()
    normal_launch = args.self_load_dol is not None or args.storage_dol is not None
    network_test = args.priiloader_recovery or normal_launch
    if network_test and not args.host:
        parser.error('Network tests require --host')
    if (args.priiloader_recovery and normal_launch) or (args.self_load_dol and args.storage_dol):
        parser.error('Recovery validation and normal self-load are separate tests')
    if args.capture_options and not args.storage_dol:
        parser.error('--capture-options requires --storage-dol')
    if args.ui_acceptance and not args.storage_dol:
        parser.error('--ui-acceptance requires --storage-dol')
    if args.no_ios_reload and not args.storage_dol:
        parser.error('--no-ios-reload requires --storage-dol')
    if not 10 <= args.startup_timeout <= 600:
        parser.error('--startup-timeout must be from 10 to 600 seconds')
    if not 10 <= args.recovery_timeout <= 300:
        parser.error('--recovery-timeout must be from 10 to 300 seconds')
    args.output.mkdir(parents=True, exist_ok=False)
    result = {'success': False, 'wad_sha256': hashlib.sha256(args.wad.read_bytes()).hexdigest(),
              'elf_sha256': hashlib.sha256(args.elf.read_bytes()).hexdigest(),
              'physical_wii_tested': False, 'milestones': []}
    selected_dol = args.self_load_dol or args.storage_dol
    if normal_launch:
        manifest = json.loads((args.wad.parent / 'manifest.json').read_text())
        for path in (args.wad, args.elf, selected_dol):
            expected = manifest['files'][path.name]
            if path.stat().st_size != expected['size'] or hashlib.sha256(path.read_bytes()).hexdigest() != expected['sha256']:
                raise RuntimeError(f'Package identity mismatch: {path.name}')
        if manifest['features'] != ['debugger-network']:
            raise RuntimeError('Use the normal debugger-network package for self-load')
        result['dol_sha256'] = manifest['files'][selected_dol.name]['sha256']
        result['build_manifest'] = manifest
    names = {}
    for line in subprocess.check_output(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-nm', '-C', str(args.elf)], text=True).splitlines():
        fields = line.split(maxsplit=2)
        if len(fields) == 3:
            names[fields[2]] = int(fields[0], 16)
    process = None
    connection = None
    hotkey_fd = None
    ui_remote_fd = None
    source_nand = (args.nand.resolve(strict=True) if args.nand else
                   Path.home() / 'Library/Application Support/Dolphin/Wii')
    if not source_nand.is_dir():
        raise RuntimeError(f'Source NAND is absent: {source_nand}')
    for relative in ('shared2/sys/SYSCONF', 'shared2/sys/net/02/config.dat'):
        if not (source_nand / relative).is_file():
            raise RuntimeError(f'Source NAND lacks required file: {relative}')
    if network_test:
        # Refuse to run when the destination address is not this Mac or the
        # readiness port is already owned by another process.
        addresses = subprocess.check_output(['/sbin/ifconfig'], text=True)
        if f'inet {args.host} ' not in addresses:
            raise RuntimeError('--host must be an address assigned to this Mac')
        with socket.socket() as check:
            check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            check.bind((args.host, 4299))
    with tempfile.TemporaryDirectory(prefix='wad-dolphin-', dir='/Volumes/SSD/WiiDev/tmp') as temporary:
        profile = Path(temporary)
        shutil.copytree(source_nand, profile / 'Wii')
        if args.priiloader_recovery:
            hbc = installed_title(profile, '000100014f484243')
            priiloader_settings = profile / 'Wii/title/00000001/00000002/data/loader.ini'
            priiloader_file = profile / 'Wii/title/00000001/00000002/data/main.bin'
            settings = priiloader_settings.read_bytes() if priiloader_settings.is_file() else b''
            if len(settings) != 32 or settings[0] != 1 or not priiloader_file.is_file():
                raise RuntimeError('NAND does not prove Priiloader autoboot is configured for HBC')
            result['installed_hbc'] = hbc
            result['priiloader_autoboot'] = 'HBC'
        config = profile / 'Config'
        config.mkdir()
        write_dolphin_config(config, '[Core]\nWiiSDCard=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n[Input]\nBackgroundInput=True\n[General]\nHotkeysRequireFocus=False\n')
        if args.storage_dol:
            import configparser
            settings = configparser.ConfigParser()
            settings.optionxform = str
            settings.read(config / 'Dolphin.ini')
            settings['Core']['WiiSDCard'] = 'True'
            settings['Core']['WiiSDCardAllowWrites'] = 'True'
            settings['Core']['WiiSDCardEnableFolderSync'] = 'True'
            with (config / 'Dolphin.ini').open('w') as target:
                settings.write(target)
            if args.ui_acceptance:
                ui_test = importlib.util.spec_from_file_location(
                    'hbc_ui_dolphin', Path(__file__).with_name('test_hbc_ui_dolphin.py'))
                ui = importlib.util.module_from_spec(ui_test)
                ui_test.loader.exec_module(ui)
                result['ui_fixture'] = ui.make_sd_fixture(profile)
                ui.set_initial_sort(profile)
                ui_remote_fd = ui.configure_profile(profile, config / 'Dolphin.ini')
            else:
                app = profile / 'Load/WiiSDSync/apps/selfload'
                app.mkdir(parents=True)
                shutil.copyfile(args.storage_dol, app / 'boot.dol')
                flags = '<no_ios_reload/>' if args.no_ios_reload else ''
                (app / 'meta.xml').write_text(f'<app><name>HBC-RS SD launch test</name>{flags}</app>')
        if network_test:
            (config / 'Hotkeys.ini').write_text(
                '[Hotkeys]\nDevice = Pipe/0/priiloader-return\n'
                'General/Take Screenshot = `Button Y`\n')
            (config / 'Logger.ini').write_text(
                '[Options]\nWriteToFile=True\nVerbosity=4\n'
                '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\n'
                'PI=True\nIOS_STM=True\nIOS_ES=True\nCI=True\nIOS_NET=True\n')
            pipes = profile / 'Pipes'
            pipes.mkdir(exist_ok=True)
            pipe = pipes / 'priiloader-return'
            os.mkfifo(pipe, 0o600)
            hotkey_fd = os.open(pipe, os.O_RDWR | os.O_NONBLOCK)
        with (args.output / 'dolphin.log').open('w') as log:
            try:
                if normal_launch:
                    started = time.monotonic()
                    process = subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin',
                                                '-b', '-u', str(profile), '-e', str(args.wad.resolve())],
                                               stdout=log, stderr=log)
                    self_load(args, result, profile, hotkey_fd, ui_remote_fd,
                              process, started)
                    result['success'] = True
                else:
                    port = 55287
                    process = subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-d', '-u', str(profile), '-e', str(args.wad.resolve()), '-C', f'Dolphin.General.GDBPort={port}', '-C', 'Dolphin.Core.CPUThread=False', '-C', 'Dolphin.Core.PauseOnPanic=False'], stdout=log, stderr=log)
                    deadline = time.monotonic() + 25
                    while time.monotonic() < deadline:
                        try:
                            connection = socket.create_connection(('127.0.0.1', port), timeout=.5)
                            break
                        except OSError:
                            if process.poll() is not None:
                                raise RuntimeError(f'Dolphin exited before debugger startup: {process.returncode}')
                            time.sleep(.1)
                    if connection is None:
                        raise TimeoutError('Dolphin debugger did not start')
                    connection.settimeout(.5)
                    debug = Debugger(connection, names)
                    result['initial_stop'] = debug.command('?')
                    milestones = ['_start', 'main']
                    if not args.priiloader_recovery:
                        milestones.append('hbc_rs::platform::renderer::draw_frame')
                    for name in milestones:
                        address = names[name]
                        debug.breakpoint(address)
                        debug.send('c')
                        debug.stopped_at(address, 45)
                        debug.breakpoint(address, False)
                        result['milestones'].append({'symbol': name, 'address': f'{address:08x}'})
                        if name == 'main':
                            initial_stub = debug.memory(0x80001800, 5216)
                            (args.output / 'stub-before.bin').write_bytes(initial_stub)
                    expected = (Path(__file__).resolve().parents[1] / 'assets/return_stub.bin').read_bytes()
                    actual = debug.memory(0x80001800, len(expected))
                    result['return_stub_sha256'] = hashlib.sha256(actual).hexdigest()
                    (args.output / 'stub-after.bin').write_bytes(actual)
                    caller_stub = initial_stub[4:12] == b'STUBHAXX'
                    result['caller_stub_present'] = caller_stub
                    if actual != (initial_stub if caller_stub else expected):
                        raise RuntimeError('Return stub does not match the production install or preserve rule')
                    if args.priiloader_recovery:
                        # The recovery-validation WAD holds before app loading.
                        # Its deadline must return through System Menu so
                        # Priiloader can start the HBC title already in the NAND.
                        debug.send('c')
                        deadline = time.monotonic() + args.recovery_timeout
                        last_error = None
                        while time.monotonic() < deadline:
                            try:
                                probe_wiiload(args.host)
                                result['hbc_wiiload_ready_after_priiloader_return'] = True
                                result['return_route'] = 'SYS_RETURNTOMENU'
                                break
                            except OSError as error:
                                last_error = str(error)
                                if process.poll() is not None:
                                    raise RuntimeError('Dolphin exited before HBC returned')
                                time.sleep(0.25)
                        else:
                            raise TimeoutError(
                                f'Priiloader did not make HBC Wiiload ready: {last_error}'
                            )
                    result['success'] = True
            except Exception as error:
                result['error'] = str(error)
            finally:
                if hotkey_fd is not None and process is not None and process.poll() is None:
                    try:
                        result['screenshot'] = str(capture_screenshot(
                            hotkey_fd, profile / 'ScreenShots', args.output))
                    except Exception as error:
                        result['screenshot_error'] = str(error)
                if hotkey_fd is not None:
                    os.close(hotkey_fd)
                if ui_remote_fd is not None:
                    os.close(ui_remote_fd)
                if connection is not None:
                    connection.close()
                if process is not None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                        result['success'] = False
                        result['cleanup_error'] = 'Dolphin required forced termination'
                    result['dolphin_exit_code'] = process.returncode
                log_path = profile / 'Logs/dolphin.log'
                if log_path.is_file():
                    shutil.copy2(log_path, args.output / 'dolphin.log')
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
