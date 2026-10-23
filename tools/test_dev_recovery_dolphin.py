#!/usr/bin/env python3
"""Validate development entry and recovery on an exact DOL, in Dolphin only.

The emulator debugger is restricted to 127.0.0.1. The HBC transport remains
cooperative-only. This script performs no Wii upload or physical device action.
"""
import argparse
import json
import re
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
from dev_report_receiver import ReportReceiver
from dolphin_test_support import Debugger, installed_title, verified_artifact, write_dolphin_config
from return_stub import bundled as validated_stub


def verify_stub_publication(elf):
    """Check the linked restore paths, since Dolphin does not model this hazard."""
    assembly = subprocess.check_output(
        ['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objdump', '-d', '-C', str(elf)], text=True)
    function = None
    paths = {}
    for block in re.split(r'\n\n+', assembly):
        header = re.search(r'^[0-9a-f]+ <([^\n]+)>:', block, re.MULTILINE)
        if header is None:
            continue
        function = header[1]
        if function not in ('ogc_system_sys::dev::finish', 'ogc_system_sys::dev::restore_loader_stub'):
            continue
        instructions = [line.split('\t')[-1].split()[0] for line in block.splitlines()
                        if re.match(r'^\s*[0-9a-f]+:\s+[0-9a-f]{2} ', line)]
        for index, instruction in enumerate(instructions):
            if instruction != 'icbi':
                continue
            following = instructions[index + 1:index + 8]
            if not any(following[i:i + 2] in (['hwsync', 'isync'], ['sync', 'isync'])
                       for i in range(len(following) - 1)):
                raise RuntimeError(f'Caller stub invalidation lacks sync/isync in {function}')
            paths[function] = paths.get(function, 0) + 1
    if not paths:
        raise RuntimeError('No linked caller-stub instruction invalidation path was found')
    return paths


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', required=True, type=Path)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--host', required=True)
    parser.add_argument('--mode', choices=('probe', 'network', 'invalid-args', 'watchdog-error', 'watchdog-zero', 'arm-timeout', 'reporter-error', 'reporter-zero', 'return-zero', 'return-error', 'report-disconnect', 'report-stall', 'log-overflow', 'log-contention', 'finish-wait', 'reset-stages', 'reset-stub-corrupt', 'reset-stub-entry'), required=True)
    parser.add_argument('--deadline-ms', type=int, default=30000)
    parser.add_argument('--debug-port', type=int, default=55286)
    parser.add_argument('--return-title', help='Verify full return to this already installed title in the private NAND copy; reset-stub-entry only')
    args = parser.parse_args()
    if args.return_title and args.mode != 'reset-stub-entry':
        parser.error('--return-title requires reset-stub-entry')
    trace_modes = {'reset-stages', 'reset-stub-corrupt', 'reset-stub-entry'}
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
    names, build, manifest = verified_artifact(DOL, ELF, ROOT)
    stub_identity = validated_stub()
    required = ['main', 'OGC_DEV_STATE', 'OGC_DEV_REPORT_RESULT', 'OGC_DEV_ENTRY_STAGE', 'OGC_DEV_ENTRY_RESULT', 'ogc_system_sys::sys_exit_to_loader', 'ogc_system_sys::dev::STUB_SAVED']
    if args.mode != 'network':
        required.append('OGC_DEV_PROBE_ENTERED')
    if args.mode in trace_modes:
        required += ['dev_wrapper_probe::reset_progress', 'OGC_DEV_RESET_SEQUENCE',
                     'OGC_DEV_RESET_STAGE', 'OGC_DEV_RESET_PRIORITY', 'OGC_DEV_RESET_RESULT',
                     'OGC_DEV_RESET_DSP_CONTROL', 'OGC_DEV_RESET_DSP_MAILBOX',
                     'OGC_DEV_PROBE_RESET_SCREEN_RESULT',
                     'OGC_DEV_STUB_COMPARE_RESULT', 'OGC_DEV_STUB_ENTRY_WORD',
                     'OGC_DEV_STUB_CONFIG_HIGH', 'OGC_DEV_STUB_CONFIG_LOW',
                     'ogc_system_probe::probe_support::PROBE_FRAMEBUFFER_A']
    if args.mode == 'reset-stub-entry':
        required += ['OGC_DEV_PROBE_STUB_ENTRY_SEEN', 'OGC_DEV_PROBE_STUB_PATCH_RESULT',
                     'OGC_DEV_PROBE_STUB_RESTORE_RESULT', 'OGC_DEV_STUB_SHA1',
                     'OGC_DEV_STUB_HASH_RESULT', 'OGC_DEV_STUB_REAL_WORDS', 'OGC_DEV_STUB_REAL_TARGET',
                     'OGC_DEV_STUB_REAL_RESULT']
    for name in required:
        if name not in names:
            raise RuntimeError(f'Missing development symbol {name}')
    if manifest['development'] is not True:
        raise RuntimeError('DOL is not a development build')
    profile = Path(tempfile.mkdtemp(prefix='hbc-network-dev-recovery-', dir=ROOT / 'tmp'))
    source_nand = Path.home() / 'Library/Application Support/Dolphin/Wii'
    for relative in ('shared2/sys/SYSCONF', 'shared2/sys/net/02/config.dat'):
        if not (source_nand / relative).is_file():
            raise RuntimeError('Required source NAND configuration is absent')
    shutil.copytree(source_nand, profile / 'Wii')
    title = installed_title(profile, args.return_title) if args.return_title else None
    config = profile / 'Config'
    config.mkdir()
    write_dolphin_config(config, '[Core]\nEnableCheats=True\nCPUThread=False\nWiiSDCard=False\n[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n')
    log = (OUT / 'dolphin-console.log').open('w')
    result = {'dol': str(DOL), 'sha256': build, 'deadline_ms': DEADLINE_MS, 'profile': str(profile), 'success': False,
              'stub_identity': stub_identity, 'stub_execution_verified': False,
              'hbc_return_verified': False, 'validation_scope': 'caller-stub-restoration'}
    if args.mode in trace_modes:
        result['stub_publication_instruction_paths'] = verify_stub_publication(ELF)
    receiver = None
    report_modes = {'return-zero', 'return-error', 'report-disconnect', 'report-stall', 'log-overflow', 'log-contention'}
    startup_modes = {'watchdog-error', 'watchdog-zero', 'arm-timeout', 'reporter-error', 'reporter-zero'}
    if args.mode in report_modes:
        receiver = ReportReceiver(HOST, 4298, args.mode)
        receiver.start()
    process = None
    emulator = None
    hbc = None

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
        debugger = Debugger(emulator, names)
        command, send = debugger.command, debugger.send
        memory, write = debugger.memory, debugger.write
        breakpoint, stopped_at, number = debugger.breakpoint, debugger.stopped_at, debugger.number
        print('Initial emulator stop:', command('?'), flush=True)
        arguments = ('hbc-rs.dol\0--report=' + HOST + ':4298\0--run-id=network-dev-recovery\0--build-id=' + build
                + '\0--recovery-reset-ms=' + str(DEADLINE_MS) + '\0\0').encode('ascii')
        if args.mode in ('return-error',):
            arguments = arguments[:-1] + b'--dev-probe-fail\0\0'
        if args.mode in ('log-overflow', 'log-contention'):
            arguments = arguments[:-1] + b'--dev-probe-log-overflow\0\0' if args.mode == 'log-overflow' else arguments
        if args.mode in {'probe', 'report-disconnect', 'report-stall'} | trace_modes:
            arguments = arguments[:-1] + b'--dev-probe-stall\0--dev-probe-screen\0\0'
        if args.mode == 'reset-stub-entry':
            arguments = arguments[:-1] + b'--dev-probe-trace-stub-entry\0\0'
        if args.mode == 'invalid-args':
            arguments = arguments.replace(b'--recovery-reset-ms=', b'--invalid-deadline=')
        if len(arguments) > 1024:
            raise RuntimeError('Dolphin argument storage bound exceeded')
        write(0x8132fc00, arguments)
        write(0x80004008, struct.pack('>III', 0x5f617267, 0x8132fc00, len(arguments)))
        stub = (ROOT / 'RustPorts/hbc-rs/assets/return_stub.bin').read_bytes()
        for offset in range(0, len(stub), 1024):
            write(0x80001800 + offset, stub[offset:offset + 1024])
        if title is not None:
            # This controlled emulator caller selects an existing installed
            # title. Never change the launching stub on the physical Wii.
            write(0x80002f00, b'LOADKTHX' + bytes.fromhex(title['title']))
            result['installed_title'] = title
        if args.mode in report_modes:
            begin_address = names['__ogc_dev_begin']
            breakpoint(begin_address)
            send('c')
            stopped_at(begin_address, 30)
            breakpoint(begin_address, False)
            saved_entry = {name: command('p' + name) for name in ('3', '4', '43')}
            return_address = 0x8132f700
            write(return_address, bytes.fromhex('48000000'))
            breakpoint(return_address)
            for register, value in [('P43=', f'{return_address:08x}'), ('P40=', f"{names['__ogc_dev_probe_prepare_network']:08x}")]:
                if command(register + value) != 'OK':
                    raise RuntimeError('Cannot call network setup')
            send('c')
            stopped_at(return_address, 45)
            if int(command('p3'), 16) != 0:
                raise RuntimeError('Real network setup failed before report test: ' + command('p3'))
            breakpoint(return_address, False)
            for register, value in [('P3=', saved_entry['3']), ('P4=', saved_entry['4']),
                                    ('P43=', saved_entry['43']), ('P40=', f'{begin_address:08x}')]:
                if command(register + value) != 'OK':
                    raise RuntimeError(f'Cannot restore entry register {register}')
        saved_stub = memory(0x80001800, 0x1800)
        exit_address = names['ogc_system_sys::sys_exit_to_loader']
        if args.mode in startup_modes:
            create = names['ogc_lwp_sys::create_thread']
            target = names['ogc_system_sys::dev::report_worker' if args.mode.startswith('reporter') else 'ogc_system_sys::dev::watchdog_worker']
            breakpoint(create)
            send('c')
            for _ in range(128):
                stopped_at(create, 30)
                if int(command('p4'), 16) == target:
                    break
                send('c')
            else:
                raise RuntimeError('Target worker creation was not reached')
            # Inject the result at the production LWP boundary. Do not replace
            # entry validation, stage publication, timeout, finish, or exit.
            handle = int(command('p3'), 16)
            write(handle, struct.pack('>I', 1 if args.mode == 'arm-timeout' else 0))
            injected = -12 if args.mode.endswith('error') else 0
            if command('P3=' + struct.pack('>i', injected).hex()) != 'OK':
                raise RuntimeError('Cannot inject thread creation result')
            if command('P40=' + command('p43')) != 'OK':
                raise RuntimeError('Cannot return from thread creation boundary')
            breakpoint(create, False)
        saved_stub = memory(0x80001800, 0x1800) if args.mode not in startup_modes else saved_stub
        first_stop = exit_address if args.mode == 'invalid-args' or args.mode in startup_modes else names['main']
        breakpoint(first_stop)
        send('c')
        stopped_at(first_stop, 30)
        if args.mode == 'invalid-args' or args.mode in startup_modes:
            result['entry_stage'] = number('OGC_DEV_ENTRY_STAGE')
            result['entry_result'] = number('OGC_DEV_ENTRY_RESULT') - 2 ** 32
            result['recovery_state'] = number('OGC_DEV_STATE')
            result['probe_entered'] = memory(names['OGC_DEV_PROBE_ENTERED'], 1) == b'\x01'
            expected = {
                'invalid-args': (2, -22, 0, False),
                'watchdog-error': (3, -12, 0, False),
                'watchdog-zero': (3, -1, 0, False),
                'arm-timeout': (4, -110, 0, False),
                'reporter-error': (5, -12, 1, False),
                'reporter-zero': (5, -1, 1, False),
            }[args.mode]
            if (result['entry_stage'], result['entry_result'], result['recovery_state'], result['probe_entered']) != expected:
                raise RuntimeError('Entry failure did not preserve its stage, result, or application gate')
            for offset in range(0, 0x1800, 512):
                write(0x80001800 + offset, bytes([0xa5]) * 512)
        else:
            result['armed_before_main'] = number('OGC_DEV_STATE') == 1
            result['entry_stage'] = number('OGC_DEV_ENTRY_STAGE')
            result['entry_result'] = number('OGC_DEV_ENTRY_RESULT')
            if result['entry_stage'] != 6 or result['entry_result'] != 0:
                raise RuntimeError('Shared entry did not complete successfully before main')
            result['caller_stub_saved'] = memory(names['ogc_system_sys::dev::STUB_SAVED'], 1) == b'\x01'
            if not result['armed_before_main'] or not result['caller_stub_saved']:
                raise RuntimeError('Shared recovery was not armed before main with a saved caller stub')
            # Prove restoration: replace every saved byte after begin completed.
            # This mutation exists only in the emulator test, after stub capture.
            original_stub = memory(0x80001800, 0x1800)
            for offset in range(0, 0x1800, 512):
                write(0x80001800 + offset, bytes([0xa5]) * 512)
            if args.mode == 'log-contention':
                write(names['ogc_system_sys::dev::LOG_LOCK'], b'\1')
            breakpoint(names['main'], False)
            completion_stop = names['ogc_system_sys::dev::finish'] if args.mode in ('finish-wait', 'log-contention') else exit_address
            breakpoint(completion_stop)
            if args.mode in ('report-disconnect', 'report-stall'):
                net_send = names['ogc_network_sys::net_send']
                breakpoint(net_send)
            send('c')
            if args.mode in ('report-disconnect', 'report-stall'):
                stopped_at(net_send, 20)
                # Complete one real send before injecting a later boundary
                # failure. The host must observe actual partial protocol data.
                send_return = int(command('p43'), 16)
                breakpoint(net_send, False)
                breakpoint(send_return)
                send('c')
                stopped_at(send_return, 10)
                breakpoint(send_return, False)
                if args.mode == 'report-disconnect':
                    breakpoint(net_send)
                    send('c')
                    stopped_at(net_send, 10)
                    if command('P3=ffffffe0') != 'OK' or command('P40=' + command('p43')) != 'OK':
                        raise RuntimeError('Cannot inject send failure')
                    breakpoint(net_send, False)
                else:
                    poll_address = names['ogc_network_sys::net_poll']
                    poll_original = memory(poll_address, 8)
                    # A controlled non-writable transport exercises the real
                    # write_all deadline. The production clock still runs.
                    write(poll_address, bytes.fromhex('386000004e800020'))
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
            result['deadline_stop'] = stopped_at(completion_stop, DEADLINE_MS / 1000 + 15)
            if args.mode == 'log-contention':
                result['lost_contention'] = number('ogc_system_sys::dev::LOST_CONTENTION')
                if result['lost_contention'] == 0:
                    raise RuntimeError('Capture contention did not record lost content')
                write(names['ogc_system_sys::dev::LOG_LOCK'], b'\0')
                breakpoint(completion_stop, False)
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
            expected_exit = -124 if args.mode in ('report-disconnect', 'report-stall') else -7 if args.mode == 'return-error' else 0 if args.mode in report_modes or args.mode == 'finish-wait' else -124
            if result['exit_status'] != expected_exit or (args.mode not in report_modes and args.mode != 'finish-wait' and result['state_at_deadline'] != 2):
                raise RuntimeError('The independent deadline did not cause the loader exit')
            if args.mode not in report_modes and args.mode != 'finish-wait' and (result['report_result'] >= 0 or result['report_result'] == -(2 ** 31)):
                raise RuntimeError('The intended closed-report-endpoint failure was not observed')
        if args.mode == 'report-stall':
            write(poll_address, poll_original)
        if args.mode == 'finish-wait':
            breakpoint(completion_stop, False)
            breakpoint(exit_address)
            write(names['ogc_system_sys::dev::REPORT_DONE'], b'\0')
            sleep = names['ogc_lwp_sys::sleep_ticks']
            finish = names['ogc_system_sys::dev::finish']
            breakpoint(sleep)
            send('c')
            for _ in range(128):
                stopped_at(sleep, 10)
                caller = int(command('p43'), 16)
                if finish <= caller < names['__ogc_dev_begin'] and memory(names['ogc_system_sys::dev::FINISHING'], 1) == b'\1':
                    break
                send('c')
            else:
                raise RuntimeError('Normal finish did not enter the bounded wait')
            breakpoint(sleep, False)
            # Stall this main thread only, with interrupts and the independent
            # watchdog intact. No production instruction is replaced.
            write(0x8132f800, bytes.fromhex('48000000'))
            if command('P40=8132f800') != 'OK':
                raise RuntimeError('Cannot inject main-thread shutdown stall')
            send('c')
            stopped_at(exit_address, DEADLINE_MS / 1000 + 15)
            if int(command('p3'), 16) != 0xffffff84:
                raise RuntimeError('Deadline did not interrupt stalled normal finish')
            result['deadline_during_finish'] = True
        breakpoint(names['ogc_system_sys::sys_exit_to_loader'], False)
        breakpoint(0x80001800)
        if args.mode in trace_modes:
            observer = names['dev_wrapper_probe::reset_progress']
            breakpoint(observer)
            # This probe uses the preferred 640-pixel console, with a 32-pixel
            # margin and doubled 32-pixel glyphs. Read the event-number row from the real
            # framebuffer. This detects an observer that publishes memory but
            # no longer writes the screen, including after IRQ shutdown.
            framebuffer = number('ogc_system_probe::probe_support::PROBE_FRAMEBUFFER_A') & 0x9fffffff
            if not 0x80000000 <= framebuffer < 0x81800000:
                raise RuntimeError('Probe console framebuffer is outside MEM1')
            event_row = framebuffer + (32 + 3 * 32) * 640 * 2
            previous_pixels = memory(event_row, 16 * 640 * 2)
            events = []
            sequence = number('OGC_DEV_RESET_SEQUENCE')
            dsp_control = number('OGC_DEV_RESET_DSP_CONTROL')
            dsp_mailbox = number('OGC_DEV_RESET_DSP_MAILBOX')
            send('c')
            for _ in range(256):
                stopped_at(observer, 20)
                stage, priority, raw_result = (int(command(register), 16) for register in ('p3', 'p4', 'p5'))
                signed_result = raw_result - 2 ** 32 if raw_result >= 2 ** 31 else raw_result
                sequence += 1
                if (number('OGC_DEV_RESET_SEQUENCE'), number('OGC_DEV_RESET_STAGE'),
                    number('OGC_DEV_RESET_PRIORITY'), number('OGC_DEV_RESET_RESULT')) != (sequence, stage, priority, raw_result):
                    raise RuntimeError('Reset evidence did not retain the production event and arguments')
                if stage == 7:
                    dsp_control = raw_result
                elif stage == 12:
                    dsp_mailbox = raw_result
                if (number('OGC_DEV_RESET_DSP_CONTROL'), number('OGC_DEV_RESET_DSP_MAILBOX')) != (dsp_control, dsp_mailbox):
                    raise RuntimeError('Reset evidence lost a DSP register observation')
                event = {'sequence': sequence, 'stage': stage, 'priority': priority, 'result': signed_result}
                events.append(event)
                result['reset_events'] = events
                if stage == 24 and args.mode == 'reset-stub-corrupt':
                    # Inject one real uncached-memory mismatch after reset
                    # cleanup and before the production handoff comparison.
                    corrupt_address = 0xc000180c
                    corrupt_original = memory(corrupt_address, 1)
                    write(corrupt_address, bytes([corrupt_original[0] ^ 0xff]))
                if stage == 18:
                    if number('OGC_DEV_PROBE_RESET_SCREEN_RESULT') != 0:
                        raise RuntimeError('Reset screen update failed before final callbacks')
                    current_pixels = memory(event_row, 16 * 640 * 2)
                    result['screen_changed_before_final_callbacks'] = current_pixels != previous_pixels
                    if not result['screen_changed_before_final_callbacks']:
                        raise RuntimeError('Reset events did not change the console framebuffer')
                    previous_pixels = current_pixels
                    result['interrupts_disabled_before_final_callbacks'] = int(command('p41'), 16) & 0x8000 == 0
                    if not result['interrupts_disabled_before_final_callbacks']:
                        raise RuntimeError('Final-callback evidence was not tested after IRQ shutdown')
                if stage == 25:
                    comparison = number('OGC_DEV_STUB_COMPARE_RESULT')
                    expected_comparison = 0xffffffac if args.mode == 'reset-stub-corrupt' else 0
                    result['stub_comparison_at_handoff'] = comparison - 2 ** 32 if comparison >= 2 ** 31 else comparison
                    if comparison != expected_comparison:
                        raise RuntimeError('Handoff evidence did not detect the real stub byte state')
                    if args.mode == 'reset-stub-corrupt':
                        write(corrupt_address, corrupt_original)
                    breakpoint(observer, False)
                    break
                send('c')
            else:
                raise RuntimeError('Reset observer did not reach the HBC branch within 256 events')
            stages = [event['stage'] for event in events]
            required_stages = [13, 14, 15, 26, 7, 12, 27, 28, 6, 10, 11, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]
            position = 0
            for stage in stages:
                if position < len(required_stages) and stage == required_stages[position]:
                    position += 1
            if position != len(required_stages):
                raise RuntimeError(f'Reset stage boundaries are absent or out of order: {stages}')
            if events[0]['result'] != -124 or not any(e['stage'] == 15 and e['priority'] == 2 and e['result'] == 1 for e in events):
                raise RuntimeError('Reset evidence did not retain the deadline result or HBC route')
        send('c')
        result['stub_stop'] = stopped_at(0x80001800, 20)
        if args.mode == 'reset-stub-entry':
            if memory(names['OGC_DEV_PROBE_STUB_ENTRY_SEEN'], 1) != b'\0' or number('OGC_DEV_PROBE_STUB_PATCH_RESULT') != 0:
                raise RuntimeError('Stub entry observer did not arm before the real entry transfer')
            original_word = int.from_bytes(original_stub[:4], 'big')
            if memory(0x80001800, 4) == original_stub[:4]:
                raise RuntimeError('Stub entry observer did not install its temporary branch')
            trace_row = framebuffer + (32 + 10 * 32) * 640 * 2
            trace_pixels = memory(trace_row, 16 * 640 * 2)
            send('c')
            result['restored_stub_stop'] = stopped_at(0x80001800, 10)
            result['stub_entry_observed'] = memory(names['OGC_DEV_PROBE_STUB_ENTRY_SEEN'], 1) == b'\1'
            result['stub_entry_restore_result'] = number('OGC_DEV_PROBE_STUB_RESTORE_RESULT')
            if not result['stub_entry_observed'] or result['stub_entry_restore_result'] != 0:
                raise RuntimeError('Stub entry was not observed and restored before re-entry')
            if memory(trace_row, 16 * 640 * 2) == trace_pixels:
                raise RuntimeError('Stub entry observer did not update the screen')
            displacement = original_word & 0x03fffffc
            if displacement & 0x02000000:
                displacement -= 0x04000000
            body = 0x80001800 + displacement
            breakpoint(0x80001800, False)
            breakpoint(body)
            send('c')
            result['original_stub_body_stop'] = stopped_at(body, 10)
        if args.mode in trace_modes:
            result['reset_screen_result'] = number('OGC_DEV_PROBE_RESET_SCREEN_RESULT')
            result['screen_changed_after_irq_shutdown'] = memory(event_row, 16 * 640 * 2) != previous_pixels
            if result['reset_screen_result'] != 0 or not result['screen_changed_after_irq_shutdown']:
                raise RuntimeError('Reset screen did not update synchronously after IRQ shutdown')
            if number('OGC_DEV_RESET_STAGE') != 25:
                raise RuntimeError('Final reset event did not identify the HBC branch')
        if args.mode in trace_modes:
            # Check the pixels the DOL actually published, not only its result.
            frame = b"".join(memory(framebuffer + offset, 1024)
                             for offset in range(0, 640 * 480 * 2, 1024))
            ink = 0
            for y in range(32, 448, 2):
                upper = frame[(y * 640 + 32) * 2:(y * 640 + 608) * 2]
                lower = frame[((y + 1) * 640 + 32) * 2:((y + 1) * 640 + 608) * 2]
                if upper != lower:
                    raise RuntimeError('Large text rows are not doubled')
                luminance = upper[::2]
                if luminance[::2] != luminance[1::2]:
                    raise RuntimeError('Large text columns are not doubled')
                ink += sum(value > 128 for value in luminance)
            if ink == 0:
                raise RuntimeError('Large text framebuffer is blank')
            result['large_text_pixels_verified'] = True
            # Grayscale preview preserves the exact framebuffer luminance.
            (OUT / 'reset-screen.pgm').write_bytes(b'P5\n640 480\n255\n' + frame[::2])
        result['recovery_state_after_finish'] = number('OGC_DEV_STATE')
        expected_stub = original_stub if args.mode not in startup_modes and args.mode != 'invalid-args' else (stub + bytes(0x1800 - len(stub)))
        # The original stub image includes bytes beyond the bundled stub file.
        if args.mode in startup_modes or args.mode == 'invalid-args':
            expected_stub = saved_stub
        result['caller_stub_restored'] = memory(0x80001800, 0x1800) == expected_stub
        if args.mode in trace_modes:
            observed = (number('OGC_DEV_STUB_ENTRY_WORD'), number('OGC_DEV_STUB_CONFIG_HIGH'), number('OGC_DEV_STUB_CONFIG_LOW'))
            expected = tuple(int.from_bytes(expected_stub[offset:offset + 4], 'big') for offset in (0, 0x1708, 0x170c))
            if observed != expected:
                raise RuntimeError('Handoff evidence did not retain the actual caller stub words')
            result['caller_stub_entry_word'] = f'0x{observed[0]:08x}'
        if result['recovery_state_after_finish'] != 3 or not result['caller_stub_restored']:
            raise RuntimeError('Caller stub restoration did not complete')
        if args.mode == 'reset-stub-entry':
            # Check evidence while application memory still exists. The stub
            # later clears BSS over the application's code and evidence.
            import hashlib
            digest = memory(names['OGC_DEV_STUB_SHA1'], 20)
            if number('OGC_DEV_STUB_HASH_RESULT') != 0 or digest != hashlib.sha1(expected_stub).digest():
                raise RuntimeError('Saved caller-stub hash does not match the full 6 KiB image')
            if number('OGC_DEV_STUB_REAL_RESULT') != 0 or number('OGC_DEV_STUB_REAL_TARGET') != 0x1848:
                raise RuntimeError('Saved caller-stub address evidence is incorrect')
            result['caller_stub_sha1'] = digest.hex()
            result['caller_stub_real_destination'] = f"0x{number('OGC_DEV_STUB_REAL_TARGET'):08x}"
            real_words = struct.unpack('>II', memory(names['OGC_DEV_STUB_REAL_WORDS'], 8))
            result['caller_stub_real_address_words'] = [f'0x{word:08x}' for word in real_words]
            # Hardware breakpoints at both address aliases detect a wrong
            # real-mode destination. Do not inject or replace stub code.
            breakpoint(body, False)
            for symbol, address in [('real', stub_identity['real_destination']),
                                    ('virtual', stub_identity['symbols']['virtual']),
                                    ('_main', stub_identity['symbols']['_main'])]:
                breakpoint(address)
                send('c')
                result['stub_' + symbol + '_stop'] = stopped_at(address, 15)
                breakpoint(address, False)
            result['stub_execution_verified'] = True
            result['validation_scope'] = 'caller-stub-startup-through-main'
            if title is not None:
                breakpoint(title['entry'])
                send('c')
                result['installed_title_entry_stop'] = stopped_at(title['entry'], 45)
                if memory(title['entry'], 32).hex() != title['entry_bytes']:
                    raise RuntimeError('Relaunched title entry does not match its installed boot content')
                breakpoint(title['entry'], False)
                send('c')
                sender = ROOT / 'Devtools/Wiiload-rs/target/release/wiiload-rs'
                ready_check = subprocess.run([str(sender), 'check', '--wii-ip', HOST,
                                              '--host-ip', HOST, '--report-port', '4298'],
                                             capture_output=True, text=True, timeout=90)
                (OUT / 'installed-title-readiness.json').write_text(ready_check.stdout)
                result['installed_title_readiness_passed'] = ready_check.returncode == 0
                result['installed_title_readiness_stderr'] = ready_check.stderr
                # Stop only this emulator and retain the actual VI-selected
                # framebuffer. This is historical evidence for visual review.
                emulator.sendall(b'\x03')
                result['returned_screen_stop'] = debugger.packet(5)
                vi = struct.unpack('>40H', memory(0xcc002000, 80))
                scale = 5 if vi[14] & 0x1000 else 0
                top = (((vi[14] & 0xff) << 16) | vi[15]) << scale
                bottom = (((vi[18] & 0xff) << 16) | vi[19]) << scale
                width = (vi[36] >> 8) * 16
                address = min(top, bottom) | 0x80000000
                if width != 640 or (vi[36] & 0xff) not in (40, 80) or not 0x80000000 <= address < 0x81700000:
                    raise RuntimeError('Returned HBC has an unsupported VI framebuffer layout')
                frame = memory(address, width * 480 * 2)
                from xfb_capture import summarize_yuyv_luma
                result['returned_screen'] = summarize_yuyv_luma(frame, width, 480)
                (OUT / 'returned-hbc.pgm').write_bytes(b'P5\n640 480\n255\n' + frame[::2])
                result['returned_hbc_screen_verified'] = True
                result['hbc_return_verified'] = result['installed_title_readiness_passed']
                result['validation_scope'] = 'installed-title-entry-and-upload-readiness'
                if not result['hbc_return_verified']:
                    raise RuntimeError('Installed-title entry and screen returned, but the HBC upload-readiness check failed')
        if receiver is not None:
            receiver.finish()
            result['report'] = receiver.validate_failure() if args.mode in ('report-disconnect', 'report-stall') else receiver.validate(build, 'network-dev-recovery', args.mode)
            report = number('OGC_DEV_REPORT_RESULT')
            result['report_result_after_finish'] = report - 2 ** 32 if report >= 2 ** 31 else report
            if args.mode in ('report-disconnect', 'report-stall', 'finish-wait'):
                if result['report_result_after_finish'] >= 0 or result['report_result_after_finish'] == -(2 ** 31):
                    raise RuntimeError('Failed report did not publish a final failure')
            elif result['report_result_after_finish'] != 0:
                raise RuntimeError('Report delivery failed')
        result['success'] = True
        print(f"PASS: {args.mode} {result['validation_scope']}; HBC return verified={result['hbc_return_verified']}", flush=True)
    except Exception as error:
        result['error'] = str(error)
        if emulator is not None:
            try:
                emulator.sendall(b'\x03')
                result['failure_stop'] = debugger.packet(3)
                pc = int(debugger.command('p40'), 16)
                nearby = [(address, name) for name, address in names.items() if address <= pc and address > 0x80004000]
                if nearby:
                    address, symbol = max(nearby)
                    result['failure_symbol'] = f'{symbol}+{pc-address:#x}'
                result['failure_registers'] = debugger.command('g')
            except Exception as inspection_error:
                result['failure_inspection_error'] = str(inspection_error)
        print('FAIL:', error, flush=True)
    finally:
        if receiver is not None:
            try:
                receiver.close()
            except Exception as error:
                result['receiver_cleanup_error'] = str(error)
                result['success'] = False
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
