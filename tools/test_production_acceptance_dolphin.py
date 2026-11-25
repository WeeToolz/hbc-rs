#!/usr/bin/env python3
"""Test production APIs in a private Dolphin profile and NAND.

This runner has no upload path. All writes stay in its private profile.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import tempfile
import time
from dolphin_test_support import Debugger, copy_private_tree, verified_artifact, write_dolphin_config

ROOT = Path(__file__).resolve().parents[3]
REPO = Path(__file__).resolve().parents[1]
TEMP_ROOT = Path('/Volumes/SSD/WiiDev/tmp/hbc-production-acceptance')
EXPECTED_XML = (b'<?xml version="1.0" encoding="UTF-8"?><hbc><device>1</device>'
                b'<sort_order>2</sort_order><browse_mode>1</browse_mode>'
                b'<app_sel>sd:/apps/acceptance/boot.dol</app_sel></hbc>')


def run_case(dol, names, profile, output, case, port, dolphin_arch):
    result = {'case': case, 'success': False}
    connection = None
    process = None
    with (output / (case + '.log')).open('w') as log:
        try:
            command = ['/Applications/Dolphin.app/Contents/MacOS/Dolphin']
            if dolphin_arch:
                command = ['/usr/bin/arch', f'-{dolphin_arch}', *command]
            process = subprocess.Popen([*command, '-d', '-u', str(profile), '-e', str(dol), '-C', f'Dolphin.General.GDBPort={port}', '-C', 'Dolphin.Core.CPUThread=False', '-C', 'Dolphin.Core.PauseOnPanic=False'], stdout=log, stderr=log)
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    connection = socket.create_connection(('127.0.0.1', port), timeout=.5)
                    break
                except OSError:
                    if process.poll() is not None:
                        raise RuntimeError('Dolphin exited before debugger startup')
                    time.sleep(.1)
            if connection is None:
                raise TimeoutError('Dolphin debugger did not start')
            connection.settimeout(.5)
            debug = Debugger(connection, names)
            debug.command('?')
            arguments = b'acceptance.dol\0' + case.encode() + b'\0\0'
            debug.write(0x8132fc00, arguments)
            debug.write(0x80004008, struct.pack('>III', 0x5f617267, 0x8132fc00, len(arguments)))
            debug.breakpoint(names['main'])
            debug.send('c')
            debug.stopped_at(names['main'], 30)
            debug.write(names['HBC_ACCEPTANCE_ALLOW'], b'EMUL')
            debug.breakpoint(names['main'], False)
            target = 0x800100f0 if case.startswith('launch-') else names['hbc_acceptance_complete']
            debug.breakpoint(target)
            if case in ('settings-banner-error', 'settings-banner-retry'):
                create = names['ogc_filesystem::create_file']
                debug.breakpoint(create)
                debug.send('c')
                debug.stopped_at(create, 30)
                # The first file creation is banner.bin. Preserve production
                # settings ordering and error propagation after this boundary.
                path = int(debug.command('p3'), 16)
                if b'banner.bin' not in debug.memory(path, 64).split(b'\0', 1)[0]:
                    raise RuntimeError('Failure injection did not reach the banner')
                if debug.command('P3=fffffc02') != 'OK' or debug.command('P40=' + debug.command('p43')) != 'OK':
                    raise RuntimeError('Cannot inject ISFS permission failure')
                debug.breakpoint(create, False)
                result['injected_error'] = -1022
            debug.send('c')
            debug.stopped_at(target, 45)
            code = int(debug.command('p4' if case.startswith('launch-') else 'p3'), 16)
            if case == 'disc-stop':
                result.update(decode_disc_stop_result(code))
                result['result'] = 0
            else:
                result['result'] = code
                if code != 0:
                    raise RuntimeError(f'Production acceptance failed: {code:#x}')
            if case.startswith('launch-'):
                expected = {0x80012000: b'\x12\x34\x56\x78', 0x80012004: bytes(4), 0x80013000: bytes(4), 0x8132fc00: b'boot.dol\0\0'}
                for address, data in expected.items():
                    if debug.memory(address, len(data)) != data:
                        raise RuntimeError(f'Child memory mismatch at {address:#x}')
            result['success'] = True
        except Exception as error:
            result['error'] = str(error)
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
                    result['success'] = False
                    result['cleanup_error'] = 'Dolphin required forced termination'
    print(json.dumps(result), flush=True)
    return result


def decode_disc_stop_result(code):
    """Decode the production acceptance marker and reject incomplete chains."""
    if code & 0xff000000 != 0x44000000:
        raise RuntimeError(f'Disc acceptance failed: {code:#x}')
    callbacks = (code >> 16) & 0xff
    opened = bool(code & 0x100)
    stopped = bool(code & 0x1)
    if (opened and (callbacks != 7 or not stopped)) or (not opened and callbacks != 1):
        raise RuntimeError('Disc acceptance reported an incomplete callback chain')
    return {
        'encoded_result': code,
        'disc_callback_mask': callbacks,
        'disc_open_succeeded': opened,
        'disc_stop_succeeded': stopped,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--debug-port', type=int, default=55287)
    parser.add_argument('--dolphin-arch', choices=['arm64', 'x86_64'],
                        help='Select a Dolphin architecture on macOS; x86_64 uses Rosetta.')
    parser.add_argument('--cases', nargs='+', choices=['launch-dol', 'launch-elf', 'settings', 'theme-storage-api', 'fat-storage-api', 'wii-startup-theme-api', 'wii-startup-dol-load-api', 'playtime-reset-api', 'settings-state', 'wiiload-header', 'wiiload-policy', 'wiiload-zip', 'wiiload-controller', 'runtime-state-api', 'runtime-pointer-api', 'runtime-measured-memo-api', 'text-layout-api', 'render-frame-api', 'video-platform-api', 'renderer-draw-frame-api', 'wii-startup-state-api', 'background-animation-api', 'wiiload-state-api', 'wiiload-start-api', 'zip-install-error-api', 'renderer-layout-memos-api', 'lifecycle-api', 'metadata-date', 'metadata-update', 'metadata-settings', 'metadata-theme', 'metadata-app', 'catalog', 'theme-package', 'theme-textures-api', 'archive-api', 'png-api', 'management-api', 'font-api', 'localization-api', 'overlay-control-api', 'home-overlay-app-api', 'network-restart-api', 'network-restart-worker-api', 'input-platform-api', 'remote-input-api', 'app-notice-api', 'about-api', 'loader-helpers', 'app-controller', 'main-menu-api', 'executable-loading-api', 'detail-focus-api', 'catalog-reconcile-api', 'storage-volumes', 'device-availability-api', 'app-deletion', 'input-policy', 'input-mapping', 'render-hit-test', 'disc-stop'], default=['launch-dol', 'launch-elf', 'settings'])
    parser.add_argument('--sdk-root', type=Path, default=Path(__file__).resolve().parents[2] / 'libogc-rs')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    TEMP_ROOT.mkdir(parents=True, exist_ok=True)
    (TEMP_ROOT / 'tmp').mkdir(exist_ok=True)
    names, digest, manifest = verified_artifact(
        args.dol.resolve(), args.elf.resolve(), TEMP_ROOT, args.sdk_root.resolve()
    )
    if manifest['development'] or manifest['target'] != 'emulator-acceptance':
        raise RuntimeError('Use the normal emulator-acceptance test binary')
    profile = Path(tempfile.mkdtemp(prefix='hbc-production-acceptance-', dir=TEMP_ROOT))
    if any(case in ('settings', 'theme-storage-api', 'playtime-reset-api') for case in args.cases):
        copy_private_tree(Path.home() / 'Library/Application Support/Dolphin/Wii', profile / 'Wii')
    else:
        (profile / 'Wii').mkdir()
    config = profile / 'Config'
    config.mkdir()
    if any(case in ('fat-storage-api', 'wii-startup-theme-api', 'wii-startup-dol-load-api') for case in args.cases):
        (config / 'Logger.ini').write_text(
            '[Options]\nWriteToFile=True\nVerbosity=4\n'
            '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\nIOS_ES=True\n'
            'IOS_NET=True\n', encoding='utf-8')
    sd_fixture = None
    theme_fixture = None
    if 'fat-storage-api' in args.cases:
        sd_fixture = profile / 'Load/WiiSDSync/acceptance'
        sd_fixture.mkdir(parents=True)
        (sd_fixture / 'payload.bin').write_bytes(b'fat-payload')
    elif 'wii-startup-dol-load-api' in args.cases:
        sd_fixture = profile / 'Load/WiiSDSync/apps'
    if any(case in ('fat-storage-api', 'wii-startup-theme-api') for case in args.cases):
        theme_fixture = profile / 'Load/WiiSDSync/apps/ThemeProbe'
        theme_fixture.mkdir(parents=True)
        (theme_fixture / 'meta.xml').write_text(
            '<app><name>Theme Probe</name></app>\n', encoding='utf-8')
        shutil.copyfile(REPO / 'tests/fixtures/theme-valid.zip', theme_fixture / 'theme.zip')
    if 'wii-startup-theme-api' in args.cases:
        (profile / 'Wii/title/00010001/48424345/data').mkdir(parents=True, exist_ok=True)
    if 'wii-startup-dol-load-api' in args.cases:
        apps = profile / 'Load/WiiSDSync/apps'
        valid_app = apps / 'AValidDOL'
        bad_app = apps / 'ZBadDOL'
        valid_app.mkdir(parents=True)
        bad_app.mkdir(parents=True)
        (valid_app / 'meta.xml').write_text('<app><name>Valid DOL</name></app>\n', encoding='utf-8')
        (bad_app / 'meta.xml').write_text('<app><name>Bad DOL</name></app>\n', encoding='utf-8')
        valid_dol = bytearray(0x104)
        struct.pack_into('>I', valid_dol, 0x00, 0x100)
        struct.pack_into('>I', valid_dol, 0x48, 0x80004000)
        struct.pack_into('>I', valid_dol, 0x90, 4)
        struct.pack_into('>I', valid_dol, 0xe0, 0x80004000)
        valid_dol[0x100:0x104] = b'\x4e\x80\x00\x20'
        (valid_app / 'boot.dol').write_bytes(valid_dol)
        bad_dol = bytearray(0x100)
        struct.pack_into('>I', bad_dol, 0x00, 48)
        struct.pack_into('>I', bad_dol, 0x48, 0x80004000)
        struct.pack_into('>I', bad_dol, 0x90, 48)
        struct.pack_into('>I', bad_dol, 0xe0, 0x80004000)
        (bad_app / 'boot.dol').write_bytes(bad_dol)
    sd_enabled = any(case in ('fat-storage-api', 'wii-startup-theme-api', 'wii-startup-dol-load-api') for case in args.cases)
    sd_config = 'WiiSDCard=True\nWiiSDCardAllowWrites=True\nWiiSDCardEnableFolderSync=True' if sd_enabled else 'WiiSDCard=False'
    write_dolphin_config(config, f'[Core]\nCPUThread=False\n{sd_config}\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n')
    results = []
    for case in args.cases:
        if case == 'playtime-reset-api':
            record = profile / 'Wii/title/00000001/00000002/data/play_rec.dat'
            before = record.read_bytes()
            if len(before) < 4 or not any(before[:4]):
                raise RuntimeError('Playtime fixture must have a nonzero four-byte prefix')
            result = run_case(args.dol.resolve(), names, profile, args.output, case, args.debug_port,
                              args.dolphin_arch)
            if result['success']:
                after = record.read_bytes()
                if len(after) != len(before) or after[:4] != bytes(4) or after[4:] != before[4:]:
                    result.update(success=False, error='Reset did not clear only the committed four-byte prefix')
                else:
                    result['playtime_prefix_reset'] = True
            results.append(result)
            continue
        if case != 'settings':
            results.append(run_case(args.dol.resolve(), names, profile, args.output, case, args.debug_port,
                                    args.dolphin_arch))
            continue
        # Use a fresh direct-DOL data directory in the isolated copy only.
        data = profile / 'Wii/title/00010001/48424345/data'
        if data.exists():
            shutil.rmtree(data)
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-direct', args.debug_port,
                                args.dolphin_arch))
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-save', args.debug_port,
                                args.dolphin_arch))
        if not results[-1]['success']:
            continue
        settings = data / 'settings.xml'
        if settings.read_bytes() != EXPECTED_XML:
            raise RuntimeError('Persisted XML does not match independent expected bytes')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-load', args.debug_port,
                                args.dolphin_arch))
        # Recreate an interrupted replacement from committed serialized bytes.
        backup = data / '_bak'
        backup.mkdir(exist_ok=True)
        settings.rename(backup / 'settings.xml')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-load-recovery', args.debug_port,
                                args.dolphin_arch))
        if not settings.exists() or settings.read_bytes() != EXPECTED_XML:
            raise RuntimeError('Restart did not recover the persisted backup')
        banner = data / 'banner.bin'
        banner.unlink()
        # Missing banner forces the real create path. The debugger injects
        # a permission failure at ISFS create_file, rather than relying on
        # host filesystem permissions that Dolphin may not enforce.
        before = settings.read_bytes()
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-banner-error', args.debug_port,
                                args.dolphin_arch))
        if settings.read_bytes() != before:
            raise RuntimeError('Failed banner save changed settings')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-banner-retry', args.debug_port,
                                args.dolphin_arch))
        if banner.read_bytes() != (ROOT / 'RustPorts/hbc-rs/assets/save_banner.bin').read_bytes():
            raise RuntimeError('Banner retry did not commit the required asset bytes')
    report = {'sha256': digest, 'dolphin_arch': args.dolphin_arch or 'native',
              'profile': str(profile), 'sd_fixture': str(sd_fixture) if sd_fixture else None,
              'theme_fixture': str(theme_fixture) if theme_fixture else None, 'cases': results,
              'success': all(r['success'] for r in results)}
    (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
