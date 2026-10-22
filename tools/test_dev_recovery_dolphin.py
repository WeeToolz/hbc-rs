#!/usr/bin/env python3
"""Validate development entry and recovery on an exact DOL, in Dolphin only.

The emulator debugger is restricted to 127.0.0.1. The HBC transport remains
cooperative-only. This script performs no Wii upload or physical device action.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path('/Volumes/SSD/WiiDev')
TOOLS = ROOT / 'RustPorts/hbc-rs/tools'
sys.path.insert(0, str(TOOLS))
from test_network_gdb_recovery_dolphin import connect, ready, arm, paused, check_diagnostics

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', required=True, type=Path)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--host', required=True)
    parser.add_argument('--mode', choices=('probe', 'network', 'invalid-args'), required=True)
    parser.add_argument('--deadline-ms', type=int, default=30000)
    parser.add_argument('--debug-port', type=int, default=55286)
    args = parser.parse_args()
    if not 10000 <= args.deadline_ms <= 120000:
        parser.error('--deadline-ms must be between 10000 and 120000')
    if not 1024 <= args.debug_port <= 65535:
        parser.error('--debug-port must be between 1024 and 65535')
    DOL, ELF, OUT = args.dol.resolve(), args.elf.resolve(), args.output.resolve()
    HOST, DEBUG_PORT, DEADLINE_MS = args.host, args.debug_port, args.deadline_ms
    OUT.mkdir(parents=True, exist_ok=False)
    addresses = subprocess.check_output(['/sbin/ifconfig'], text=True)
    if f'inet {HOST} ' not in addresses:
        raise RuntimeError('Dolphin network host must be a current Mac address')
    ports = (2828, 4299, 4298, DEBUG_PORT) if args.mode == 'network' else (4298, DEBUG_PORT)
    for port in ports:
        with socket.socket() as check:
            check.bind(('127.0.0.1' if port == DEBUG_PORT else HOST, port))
    names = {}
    for line in subprocess.check_output(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-nm', '-C', str(ELF)], text=True).splitlines():
        fields = line.split(maxsplit=2)
        if len(fields) == 3:
            names[fields[2]] = int(fields[0], 16)
    required = ['main', 'OGC_DEV_STATE', 'OGC_DEV_REPORT_RESULT', 'OGC_DEV_ENTRY_STAGE', 'OGC_DEV_ENTRY_RESULT', 'ogc_system_sys::sys_exit_to_loader', 'ogc_system_sys::dev::STUB_SAVED']
    if args.mode != 'network':
        required.append('OGC_DEV_PROBE_ENTERED')
    for name in required:
        if name not in names:
            raise RuntimeError(f'Missing development symbol {name}')
    build = hashlib.sha256(DOL.read_bytes()).hexdigest()
    manifest = json.loads(DOL.with_suffix('.manifest.json').read_text())
    if manifest['dol_sha256'] != build or manifest['development'] is not True:
        raise RuntimeError('DOL does not match development manifest')
    profile = Path(tempfile.mkdtemp(prefix='hbc-network-dev-recovery-', dir=ROOT / 'tmp'))
    # Cargo can replace an ELF at the same path after a later build. Check its
    # actual bytes before trusting addresses from nm against the selected DOL.
    converted = profile / 'verified-elf.dol'
    subprocess.run(['cargo', 'xtask', 'elf-to-dol', str(ELF), str(converted)],
                   cwd=ROOT / 'Devtools/libogc-rs', check=True,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if converted.read_bytes() != DOL.read_bytes():
        raise RuntimeError('ELF does not produce the selected DOL; use its matching archived ELF')
    source_nand = Path.home() / 'Library/Application Support/Dolphin/Wii'
    for relative in ('shared2/sys/SYSCONF', 'shared2/sys/net/02/config.dat'):
        if not (source_nand / relative).is_file():
            raise RuntimeError('Required source NAND configuration is absent')
    shutil.copytree(source_nand, profile / 'Wii')
    config = profile / 'Config'
    config.mkdir()
    (config / 'Dolphin.ini').write_text('[Core]\nEnableCheats=True\nCPUThread=False\nWiiSDCard=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n')
    log = (OUT / 'dolphin-console.log').open('w')
    result = {'dol': str(DOL), 'sha256': build, 'deadline_ms': DEADLINE_MS, 'profile': str(profile), 'success': False}
    process = None
    emulator = None
    hbc = None
    buffer = bytearray()

    def packet(seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            start = buffer.find(b'$')
            end = buffer.find(b'#', start + 1) if start >= 0 else -1
            if end >= 0 and len(buffer) >= end + 3:
                payload = bytes(buffer[start + 1:end])
                checksum = int(bytes(buffer[end + 1:end + 3]), 16)
                del buffer[:end + 3]
                if sum(payload) & 255 != checksum:
                    raise RuntimeError('Invalid emulator debugger checksum')
                emulator.sendall(b'+')
                return payload.decode('ascii')
            try:
                data = emulator.recv(65536)
            except socket.timeout:
                continue
            if not data:
                raise RuntimeError('Emulator debugger closed')
            buffer.extend(data)
        raise TimeoutError('Emulator debugger response timed out')

    def send(value):
        data = value.encode('ascii')
        emulator.sendall(b'$' + data + b'#' + f'{sum(data) & 255:02x}'.encode('ascii'))

    def command(value):
        send(value)
        return packet()

    def write(address, data):
        reply = command(f'M{address:x},{len(data):x}:' + data.hex())
        if reply != 'OK':
            raise RuntimeError(f'Emulator memory write rejected: {reply}')

    def memory(address, length):
        reply = command(f'm{address:x},{length:x}')
        if reply.startswith(('E', 'S', 'T')):
            raise RuntimeError(f'Emulator memory read failed: {reply}')
        data = bytes.fromhex(reply)
        if len(data) != length:
            raise RuntimeError('Truncated emulator memory read')
        return data

    def number(name):
        return int.from_bytes(memory(names[name], 4), 'big')

    def breakpoint(address, install=True):
        reply = command(f'{"Z" if install else "z"}0,{address:x},4')
        if reply != 'OK':
            raise RuntimeError(f'Emulator breakpoint rejected: {reply}')

    def stopped_at(address, seconds=10):
        reply = packet(seconds)
        if f'40:{address:08x};' not in reply:
            raise RuntimeError(f'Unexpected emulator stop: {reply}; expected {address:08x}')
        return reply

    try:
        process = subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-d', '-u', str(profile), '-e', str(DOL),
            '-C', f'Dolphin.General.GDBPort={DEBUG_PORT}', '-C', 'Dolphin.Core.EnableCheats=True',
            '-C', 'Dolphin.Core.PauseOnPanic=False', '-C', 'Logger.Options.WriteToFile=True',
            '-C', 'Logger.Options.Verbosity=4', '-C', 'Logger.Logs.OSREPORT=True'], stdout=log, stderr=log)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            try:
                emulator = socket.create_connection(('127.0.0.1', DEBUG_PORT), timeout=.5)
                break
            except OSError:
                if process.poll() is not None:
                    raise RuntimeError('Dolphin exited before debugger startup')
                time.sleep(.1)
        if emulator is None:
            raise RuntimeError('Dolphin debugger did not start')
        emulator.settimeout(.5)
        print('Initial emulator stop:', command('?'), flush=True)
        arguments = ('hbc-rs.dol\0--report=' + HOST + ':4298\0--run-id=network-dev-recovery\0--build-id=' + build
                + '\0--recovery-reset-ms=' + str(DEADLINE_MS) + '\0\0').encode('ascii')
        if args.mode == 'probe':
            arguments = arguments[:-1] + b'--dev-probe-stall\0--dev-probe-screen\0\0'
        if args.mode == 'invalid-args':
            arguments = arguments.replace(b'--recovery-reset-ms=', b'--invalid-deadline=')
        if len(arguments) > 1024:
            raise RuntimeError('Dolphin argument storage bound exceeded')
        write(0x8132fc00, arguments)
        write(0x80004008, struct.pack('>III', 0x5f617267, 0x8132fc00, len(arguments)))
        stub = (ROOT / 'RustPorts/hbc-rs/assets/return_stub.bin').read_bytes()
        for offset in range(0, len(stub), 1024):
            write(0x80001800 + offset, stub[offset:offset + 1024])
        first_stop = names['ogc_system_sys::sys_exit_to_loader'] if args.mode == 'invalid-args' else names['main']
        breakpoint(first_stop)
        send('c')
        stopped_at(first_stop, 30)
        if args.mode == 'invalid-args':
            result['entry_stage'] = number('OGC_DEV_ENTRY_STAGE')
            result['entry_result'] = number('OGC_DEV_ENTRY_RESULT') - 2 ** 32
            result['recovery_state'] = number('OGC_DEV_STATE')
            result['probe_entered'] = memory(names['OGC_DEV_PROBE_ENTERED'], 1) == b'\x01'
            if (result['entry_stage'], result['entry_result'], result['recovery_state'], result['probe_entered']) != (2, -22, 0, False):
                raise RuntimeError('Malformed arguments did not fail before worker and application startup')
        else:
            result['armed_before_main'] = number('OGC_DEV_STATE') == 1
            result['entry_stage'] = number('OGC_DEV_ENTRY_STAGE')
            result['entry_result'] = number('OGC_DEV_ENTRY_RESULT')
            if result['entry_stage'] != 6 or result['entry_result'] != 0:
                raise RuntimeError('Shared entry did not complete successfully before main')
            result['caller_stub_saved'] = memory(names['ogc_system_sys::dev::STUB_SAVED'], 1) == b'\x01'
            if not result['armed_before_main'] or not result['caller_stub_saved']:
                raise RuntimeError('Shared recovery was not armed before main with a saved caller stub')
            breakpoint(names['main'], False)
            breakpoint(names['ogc_system_sys::sys_exit_to_loader'])
            send('c')
            deadline = time.monotonic() + 15 if args.mode == 'network' else 0
            last = None
            while time.monotonic() < deadline:
                try:
                    hbc = connect(HOST, 2828)
                    ready(hbc)
                    break
                except (OSError, RuntimeError) as error:
                    last = str(error)
                    if hbc is not None:
                        hbc.close()
                        hbc = None
                    time.sleep(.25)
            if args.mode == 'network' and hbc is None:
                raise RuntimeError(f'GDB listener did not become ready: {last}')
            if args.mode == 'network':
                result['diagnostics_before_pause'] = check_diagnostics(HOST)
                arm(hbc)
                paused(hbc)
                result['paused_with_active_network_worker'] = True
                result['diagnostics_while_paused'] = check_diagnostics(HOST)
            print('Waiting for the independent deadline; no resume or client disconnect', flush=True)
            result['deadline_stop'] = stopped_at(names['ogc_system_sys::sys_exit_to_loader'], DEADLINE_MS / 1000 + 15)
            if args.mode == 'probe':
                result['probe_entered'] = memory(names['OGC_DEV_PROBE_ENTERED'], 1) == b'\x01'
                if not result['probe_entered']:
                    raise RuntimeError('Recovery occurred before the probe entered main')
            result['exit_status'] = int(command('p3'), 16)
            if result['exit_status'] >= 2 ** 31:
                result['exit_status'] -= 2 ** 32
            result['state_at_deadline'] = number('OGC_DEV_STATE')
            report = number('OGC_DEV_REPORT_RESULT')
            result['report_result'] = report - 2 ** 32 if report >= 2 ** 31 else report
            if result['exit_status'] != -124 or result['state_at_deadline'] != 2:
                raise RuntimeError('The independent deadline did not cause the loader exit')
            if result['report_result'] >= 0 or result['report_result'] == -(2 ** 31):
                raise RuntimeError('The intended closed-report-endpoint failure was not observed')
        breakpoint(names['ogc_system_sys::sys_exit_to_loader'], False)
        breakpoint(0x80001800)
        send('c')
        result['stub_stop'] = stopped_at(0x80001800, 20)
        result['recovery_state_after_finish'] = number('OGC_DEV_STATE')
        result['caller_stub_restored'] = memory(0x80001804, 8) == stub[4:12]
        if result['recovery_state_after_finish'] != 3 or not result['caller_stub_restored']:
            raise RuntimeError('Return did not complete through the restored caller stub')
        result['success'] = True
        print(f'PASS: {args.mode} entry/recovery checks and restored caller stub', flush=True)
    except Exception as error:
        result['error'] = str(error)
        print('FAIL:', error, flush=True)
    finally:
        if hbc is not None:
            hbc.close()
        if emulator is not None:
            emulator.close()
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=5)
                result['cleanup'] = 'terminated'
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                result['cleanup'] = 'forced-kill'
                result['success'] = False
        log.close()
        (OUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
