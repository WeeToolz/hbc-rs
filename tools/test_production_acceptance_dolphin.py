#!/usr/bin/env python3
"""Test the production loader and settings adapter in a private Dolphin NAND.

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
from dolphin_test_support import Debugger, verified_artifact, write_dolphin_config

ROOT = Path(__file__).resolve().parents[3]
EXPECTED_XML = (b'<?xml version="1.0" encoding="UTF-8"?><hbc><device>1</device>'
                b'<sort_order>2</sort_order><browse_mode>1</browse_mode>'
                b'<app_sel>sd:/apps/acceptance/boot.dol</app_sel></hbc>')


def run_case(dol, names, profile, output, case, port):
    result = {'case': case, 'success': False}
    connection = None
    process = None
    with (output / (case + '.log')).open('w') as log:
        try:
            process = subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-d', '-u', str(profile), '-e', str(dol), '-C', f'Dolphin.General.GDBPort={port}', '-C', 'Dolphin.Core.CPUThread=False', '-C', 'Dolphin.Core.PauseOnPanic=False'], stdout=log, stderr=log)
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--debug-port', type=int, default=55287)
    parser.add_argument('--cases', nargs='+', choices=['launch-dol', 'launch-elf', 'settings'], default=['launch-dol', 'launch-elf', 'settings'])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    names, digest, manifest = verified_artifact(args.dol.resolve(), args.elf.resolve(), ROOT)
    if manifest['development'] or manifest['target'] != 'emulator-acceptance':
        raise RuntimeError('Use the normal emulator-acceptance test binary')
    profile = Path(tempfile.mkdtemp(prefix='hbc-production-acceptance-', dir=ROOT / 'tmp'))
    shutil.copytree(Path.home() / 'Library/Application Support/Dolphin/Wii', profile / 'Wii')
    config = profile / 'Config'
    config.mkdir()
    write_dolphin_config(config, '[Core]\nCPUThread=False\nWiiSDCard=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n')
    results = []
    for case in args.cases:
        if case != 'settings':
            results.append(run_case(args.dol.resolve(), names, profile, args.output, case, args.debug_port))
            continue
        # Use a fresh direct-DOL data directory in the isolated copy only.
        data = profile / 'Wii/title/00010001/48424345/data'
        if data.exists():
            shutil.rmtree(data)
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-direct', args.debug_port))
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-save', args.debug_port))
        if not results[-1]['success']:
            continue
        settings = data / 'settings.xml'
        if settings.read_bytes() != EXPECTED_XML:
            raise RuntimeError('Persisted XML does not match independent expected bytes')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-load', args.debug_port))
        # Recreate an interrupted replacement from committed serialized bytes.
        backup = data / '_bak'
        backup.mkdir(exist_ok=True)
        settings.rename(backup / 'settings.xml')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-load', args.debug_port))
        if not settings.exists() or settings.read_bytes() != EXPECTED_XML:
            raise RuntimeError('Restart did not recover the persisted backup')
        banner = data / 'banner.bin'
        banner.unlink()
        # Missing banner forces the real create path. The debugger injects
        # a permission failure at ISFS create_file, rather than relying on
        # host filesystem permissions that Dolphin may not enforce.
        before = settings.read_bytes()
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-banner-error', args.debug_port))
        if settings.read_bytes() != before:
            raise RuntimeError('Failed banner save changed settings')
        results.append(run_case(args.dol.resolve(), names, profile, args.output, 'settings-banner-retry', args.debug_port))
        if banner.read_bytes() != (ROOT / 'RustPorts/hbc-rs/assets/save_banner.bin').read_bytes():
            raise RuntimeError('Banner retry did not commit the required asset bytes')
    report = {'sha256': digest, 'profile': str(profile), 'cases': results, 'success': all(r['success'] for r in results)}
    (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
