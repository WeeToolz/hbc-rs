#!/usr/bin/env python3
"""Safe HBC cooperative network debugger check for an already-running Dolphin guest.

Default: read-only qHbcStatus. --cooperative explicitly tests arm/pause/resume.
Never installs exception handlers or sends standard GDB execution commands.
"""
import argparse
import socket
import time


def read_frame(sock):
    deadline = time.monotonic() + 5.0
    data = bytearray()
    while time.monotonic() < deadline and len(data) < 4096:
        block = sock.recv(1024)
        if not block:
            raise RuntimeError('GDB connection closed')
        data.extend(block)
        start = data.find(b'$')
        if start >= 0:
            marker = data.find(b'#', start + 1)
            if marker >= 0 and len(data) >= marker + 3:
                payload = bytes(data[start + 1:marker])
                received = bytes(data[marker + 1:marker + 3])
                try:
                    checksum = int(received, 16)
                except ValueError as exc:
                    raise RuntimeError('Invalid reply checksum encoding') from exc
                if sum(payload) & 255 != checksum:
                    raise RuntimeError('Invalid reply checksum')
                return payload
    raise RuntimeError('No complete GDB reply')


def exchange(sock, text):
    payload = text.encode('ascii')
    frame = b'$' + payload + b'#' + f'{sum(payload) & 255:02x}'.encode('ascii')
    sock.sendall(frame)
    return read_frame(sock).decode('ascii', 'replace')


def status(sock):
    response = exchange(sock, 'qHbcStatus')
    fields = dict(item.split('=', 1) for item in response.split(';') if '=' in item)
    if fields.get('transport') != 'connected' or fields.get('client') != 'present':
        raise RuntimeError('Transport is not connected: ' + response)
    if fields.get('exception') != 'disabled' or fields.get('execution') != 'disabled':
        raise RuntimeError('Unsafe exception/execution mode detected: ' + response)
    if fields.get('reset') != 'normal':
        raise RuntimeError('Unexpected Reset mode: ' + response)
    print('STATUS:', response, flush=True)
    return fields


def wait_for(sock, predicate, description, seconds=5.0):
    end = time.monotonic() + seconds
    latest = None
    while time.monotonic() < end:
        latest = status(sock)
        if predicate(latest):
            print('PASS:', description, flush=True)
            return latest
        time.sleep(0.15)
    raise RuntimeError(f'Timed out waiting for {description}; last={latest}')


def command(sock, packet):
    result = exchange(sock, packet)
    if result != 'OK':
        raise RuntimeError(f'{packet} not queued: {result!r}')
    print('QUEUED:', packet, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='192.168.1.52')
    parser.add_argument('--port', type=int, default=2828)
    parser.add_argument('--cooperative', action='store_true', help='explicitly arm, pause, and resume the HBC main loop')
    args = parser.parse_args()
    print('Connect to already-running Dolphin HBC, not real Wii hardware.', flush=True)
    with socket.create_connection((args.host, args.port), timeout=5.0) as sock:
        sock.settimeout(5.0)
        initial = status(sock)
        if initial.get('authorization') != 'disarmed':
            raise RuntimeError('Expected unarmed connection on entry')
        print('PASS: read-only live GDB status', flush=True)
        if not args.cooperative:
            print('No control packets sent. Add --cooperative to explicitly test pause/resume.', flush=True)
            return
        if initial.get('cooperative') != 'enabled' or initial.get('pause') != 'running':
            raise RuntimeError('Not eligible to start cooperative test')
        print('Beginning explicit cooperative arm/pause/resume test.', flush=True)
        try:
            command(sock, 'qHbcArm')
            wait_for(sock, lambda s: s.get('authorization') == 'armed', 'authorization armed')
            command(sock, 'qHbcPause')
            paused = wait_for(sock,
                lambda s: s.get('pause') == 'paused' or s.get('result') == 'busy',
                'pause accepted or safely rejected as busy')
            if paused.get('pause') == 'paused':
                command(sock, 'qHbcResume')
                wait_for(sock, lambda s: s.get('pause') == 'running' and s.get('result') == 'resumed',
                    'normal execution resumed')
            else:
                print('PASS: unsafe-point pause request rejected as busy', flush=True)
        finally:
            # Disconnect is a second safe resume route in the production design.
            # This block closes the connection even on a timeout or protocol error.
            try:
                if sock.fileno() >= 0:
                    sock.close()
            except OSError:
                pass
    print('PASS: cooperative test completed; client disconnected', flush=True)


if __name__ == '__main__':
    main()
