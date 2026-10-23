#!/usr/bin/env python3
"""Trace a normal Dolphin DOL launch without injected arguments or low-memory writes."""
import json
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path('/Volumes/SSD/WiiDev')
REPO = ROOT / 'RustPorts/hbc-rs'
TOOLS = REPO / 'tools'
sys.path.insert(0, str(TOOLS))
from dolphin_test_support import Debugger, write_dolphin_config

DOL = ROOT / 'Devtools/reports/evidence/2026-10-22-return-stub-address-fix/fresh-build/dev-stub-address-fix-fresh.dol'
ELF = ROOT / 'Devtools/reports/evidence/2026-10-22-return-stub-address-fix/fresh-build/dev-stub-address-fix-fresh.elf'
OUT = Path(__file__).resolve().parent
PORT = 55321
SYMBOLS = {
    'dev_begin': 0x8000B200,
    'main': 0x80004E2C,
    'sys_exit_to_loader': 0x80007684,
}


def main():
    import hashlib
    import struct
    import shutil
    import tempfile

    digest = hashlib.sha256(DOL.read_bytes()).hexdigest()
    if digest != '102d0aae30b4d21dda938304920fdbaf994ce69688298db945943f055444a8c1':
        raise RuntimeError('Selected DOL does not match the reported large-text build')
    profile = Path(tempfile.mkdtemp(prefix='latest-normal-dol-launch-', dir=OUT))
    config = profile / 'Config'
    config.mkdir()
    write_dolphin_config(config, '[Core]\nCPUThread=False\nWiiSDCard=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n')
    log_path = OUT / 'dolphin-console.log'
    with log_path.open('w') as log:
        process = subprocess.Popen([
            '/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-d', '-u', str(profile),
            '-e', str(DOL), '-C', f'Dolphin.General.GDBPort={PORT}',
            '-C', 'Dolphin.Core.CPUThread=False', '-C', 'Dolphin.Core.PauseOnPanic=False',
        ], stdout=log, stderr=log)
        connection = None
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    connection = socket.create_connection(('127.0.0.1', PORT), timeout=.5)
                    break
                except OSError:
                    if process.poll() is not None:
                        raise RuntimeError('Dolphin exited before its debugger became ready')
                    time.sleep(.1)
            if connection is None:
                raise TimeoutError('Dolphin debugger did not become ready')
            connection.settimeout(.5)
            debugger = Debugger(connection, {})
            first_stop = debugger.command('?')
            for address in SYMBOLS.values():
                debugger.breakpoint(address)
            debugger.send('c')
            events = []
            for _ in range(8):
                stop = debugger.packet(20)
                pc = int(stop.split('40:')[1].split(';')[0], 16)
                label = next((name for name, address in SYMBOLS.items() if address == pc), 'other')
                event = {'symbol': label, 'pc': f'0x{pc:08x}', 'stop': stop}
                if label == 'dev_begin':
                    argc = int(debugger.command('p3'), 16)
                    argv = int(debugger.command('p4'), 16)
                    args = []
                    for index in range(min(argc, 16)):
                        pointer = int.from_bytes(debugger.memory(argv + index * 4, 4), 'big')
                        if not pointer:
                            args.append(None)
                            continue
                        chunk = bytearray()
                        for offset in range(0, 256, 32):
                            data = debugger.memory(pointer + offset, 32)
                            nul = data.find(b'\0')
                            chunk.extend(data if nul < 0 else data[:nul])
                            if nul >= 0:
                                break
                        args.append(bytes(chunk).decode('utf-8', errors='replace'))
                    event.update({'argc': argc, 'argv': args})
                    entry_stage = int.from_bytes(debugger.memory(0x80459888, 4), 'big')
                    entry_result = int.from_bytes(debugger.memory(0x80037FF4, 4), 'big')
                    event.update({'entry_stage': entry_stage, 'entry_result': entry_result})
                if label == 'sys_exit_to_loader':
                    event['r3_status'] = debugger.command('p3')
                    event['entry_stage'] = int.from_bytes(debugger.memory(0x80459888, 4), 'big')
                    event['entry_result'] = int.from_bytes(debugger.memory(0x80037FF4, 4), 'big')
                events.append(event)
                if label == 'sys_exit_to_loader':
                    break
                debugger.send('c')
            else:
                raise TimeoutError('Trace did not reach loader exit within eight stops')
            result = {
                'dol': str(DOL), 'sha256': digest, 'launch': 'Dolphin -e DOL; no app arguments',
                'first_stop': first_stop, 'events': events,
                'reached_main': any(event['symbol'] == 'main' for event in events),
                'reached_loader_exit': any(event['symbol'] == 'sys_exit_to_loader' for event in events),
                'result': 'normal launch rejected the development entry before application main' if not any(event['symbol'] == 'main' for event in events) else 'main reached',
                'log': str(log_path), 'profile': str(profile),
            }
            (OUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        finally:
            if connection is not None:
                connection.close()
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == '__main__':
    main()
