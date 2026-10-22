#!/usr/bin/env python3
"""Dolphin-only HBC GDB cooperative recovery test. Never use against a real Wii.

Requires test_network_gdb_dolphin.py in the same directory. No exception GDB commands.
"""
import argparse
import importlib.util
from pathlib import Path
import socket
import time
from test_network_gdb_dolphin import command, status, wait_for
from test_remote_input_dolphin import client as input_client


_SPEC = importlib.util.spec_from_file_location('gdb_diagnostics', Path(__file__).with_name('hbc-gdb-status.py'))
_diagnostic_client = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_diagnostic_client)


def check_diagnostics(host, startup_failed=False):
    snapshot = _diagnostic_client.query(host)
    expected = {
        'state': 'startup-failed' if startup_failed else 'listening',
        'ever_opened': not startup_failed, 'listener_stopped': False,
        'operation': 'bind' if startup_failed else None,
        'ios_error': -3 if startup_failed else None,
        'cleanup_error': None, 'gdb_port': 2828,
    }
    if snapshot != expected:
        raise RuntimeError(f'Unexpected listener diagnostics: {snapshot}; expected={expected}')
    print(f'PASS: Wiiload read-only GDB diagnostics: {snapshot}', flush=True)
    return snapshot


def check_wiiload(host):
    # A release changes no buttons. Its reply proves main-loop completion on
    # the production Wiiload socket, rather than TCP connection acceptance.
    if input_client.main(['--wii', host, '--kind', 'wiimote', '--release']) != 0:
        raise RuntimeError('Wiiload input release did not complete in the main loop')
    print('PASS: Wiiload main-loop release completed after debugger reconnect', flush=True)


def connect(host, port):
    sock = socket.create_connection((host, port), timeout=5)
    sock.settimeout(5)
    return sock


def ready(sock):
    state = status(sock)
    if (state.get('cooperative') != 'enabled' or state.get('pause') != 'running'
            or state.get('authorization') != 'disarmed'):
        raise RuntimeError(f'Unsafe initial state: {state}')


def paused(sock):
    command(sock, 'qHbcPause')
    state = wait_for(sock, lambda s: s.get('pause') == 'paused' or s.get('result') == 'busy',
                     'pause reached or rejected as busy')
    if state.get('pause') != 'paused':
        raise RuntimeError('Pause was rejected as busy; retry when HBC is idle')


def arm(sock):
    command(sock, 'qHbcArm')
    wait_for(sock, lambda s: s.get('authorization') == 'armed', 'authorization armed')


def resume(sock):
    command(sock, 'qHbcResume')
    wait_for(sock, lambda s: s.get('pause') == 'running' and s.get('result') == 'resumed',
             'main thread resumed')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--host', default='192.168.1.52')
    p.add_argument('--port', type=int, default=2828)
    p.add_argument('--cycles', type=int, default=5)
    p.add_argument('--wiiload', action='store_true',
                   help='Verify main-loop completion through TCP 4299 after reconnect')
    p.add_argument('--dolphin-only', action='store_true', required=True,
                   help='Confirm that the destination is the running Dolphin emulator, never a real Wii')
    p.add_argument('--diagnostics', action='store_true', help='Check the independent Wiiload diagnostic channel')
    args = p.parse_args()
    if not 1 <= args.cycles <= 20:
        p.error('--cycles must be from 1 to 20')
    print('WARNING: Dolphin-only test. Do not point --host at real Wii hardware.', flush=True)

    if args.diagnostics:
        check_diagnostics(args.host)

    # Phase 1: intentional disconnect while paused. Do not send resume.
    sock = connect(args.host, args.port)
    try:
        ready(sock)
        arm(sock)
        paused(sock)
        if args.diagnostics:
            check_diagnostics(args.host)
            state = status(sock)
            if state.get('pause') != 'paused' or state.get('authorization') != 'armed':
                raise RuntimeError('Read-only diagnostic query changed cooperative state')
        print('ACTION: Close GDB socket while HBC is paused (NO resume packet)', flush=True)
    finally:
        sock.close()

    # On a new connection, the production worker must observe the old disconnect,
    # clear authorization, and allow the main thread to return to running.
    deadline = time.monotonic() + 10
    last = None
    while time.monotonic() < deadline:
        try:
            with connect(args.host, args.port) as check:
                state = status(check)
                last = state
                if state.get('pause') == 'running' and state.get('authorization') == 'disarmed':
                    print('PASS: Disconnect while paused resumed HBC and disarmed authorization', flush=True)
                    break
        except (OSError, RuntimeError) as exc:
            last = str(exc)
        time.sleep(0.25)
    else:
        raise RuntimeError(f'FAIL: Disconnect recovery not confirmed; last={last}')

    # Phase 2: retry while the previous client is being cleaned up.
    deadline = time.monotonic() + 10
    while True:
        sock = None
        try:
            sock = connect(args.host, args.port)
            ready(sock)
            break
        except (OSError, RuntimeError):
            if sock is not None:
                sock.close()
            if time.monotonic() >= deadline:
                raise RuntimeError("Timed out waiting for a fresh GDB client")
            time.sleep(0.25)

    with sock:
        arm(sock)
        for i in range(1, args.cycles + 1):
            print(f'CYCLE {i}/{args.cycles}', flush=True)
            paused(sock)
            resume(sock)
            now = status(sock)
            if now.get('authorization') != 'armed' or now.get('pause') != 'running':
                raise RuntimeError(f'Unexpected post-resume state on cycle {i}: {now}')
            print(f'PASS: Pause/resume cycle {i}', flush=True)
        # Client shutdown must remove authorization, with no queued pause.
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            with connect(args.host, args.port) as sock:
                ready(sock)
            break
        except (OSError, RuntimeError):
            time.sleep(0.25)
    else:
        raise RuntimeError('Final disarmed/running status was not confirmed')
    print(f'PASS: {args.cycles} cycles; final connection disarmed and running', flush=True)
    if args.diagnostics:
        check_diagnostics(args.host)
    if args.wiiload:
        check_wiiload(args.host)
    print('No Reset test, memory access, breakpoints, or exception handlers used.', flush=True)


if __name__ == '__main__':
    main()
