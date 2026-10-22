#!/usr/bin/env python3
"""Read retained GDB listener diagnostics through Wiiload (TCP 4299)."""
import argparse
import json
import math
import socket
import struct
import sys
from hbc_transport import recv_exact

QUERY = b'HBCD\x01' + bytes(11)
REPLY_LEN = 32
STATES = {1: 'starting', 2: 'listening', 3: 'startup-failed', 4: 'poll-failed', 5: 'stopped', 6: 'cleanup-failed'}
OPERATIONS = {0: None, 1: 'host-ip', 2: 'socket', 3: 'fcntl-get', 4: 'fcntl-set',
              5: 'bind', 6: 'listen', 7: 'accept', 8: 'read', 9: 'write',
              10: 'close', 11: 'transport'}


def decode_reply(reply):
    if len(reply) != REPLY_LEN:
        raise ValueError(f'Expected 32 diagnostic bytes; received {len(reply)}')
    if reply[:5] != b'HBCD\x01':
        raise ValueError('Invalid diagnostic reply magic or version')
    if reply[5] != 0:
        raise ValueError(f'Diagnostic query rejected (status {reply[5]})')
    state, flags, operation = reply[6:9]
    if (state not in STATES or operation not in OPERATIONS or flags & ~15
            or any(reply[9:12]) or any(reply[22:])):
        raise ValueError('Invalid diagnostic reply fields')
    code, cleanup, port = struct.unpack('>iiH', reply[12:22])
    if port != 2828:
        raise ValueError(f'Unexpected GDB port {port}')
    if (not flags & 4 and code != 0) or (not flags & 8 and cleanup != 0):
        raise ValueError('Error bytes are present without validity flags')
    if ((flags & 4 and code >= 0) or (flags & 8 and cleanup >= 0)
            or (flags & 2 and not flags & 1)
            or ((state in (3, 4, 6)) != (operation != 0))
            or (state == 2 and flags != 1)
            or (state == 3 and flags & 3)):
        raise ValueError('Inconsistent diagnostic state or error fields')
    return {'state': STATES[state], 'ever_opened': bool(flags & 1),
            'listener_stopped': bool(flags & 2), 'operation': OPERATIONS[operation],
            'ios_error': code if flags & 4 else None,
            'cleanup_error': cleanup if flags & 8 else None, 'gdb_port': port}


def query(host, port=4299, timeout=5.0):
    with socket.create_connection((host, port), timeout=timeout) as connection:
        connection.settimeout(timeout)
        connection.sendall(QUERY)
        reply = recv_exact(connection, REPLY_LEN)
    return decode_reply(bytes(reply))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--port', type=int, default=4299)
    parser.add_argument('--timeout', type=float, default=5.0)
    args = parser.parse_args()
    if not 1 <= args.port <= 65535 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error('port must be 1..65535 and timeout must be positive')
    try:
        result = query(args.host, args.port, args.timeout)
    except (OSError, ValueError) as error:
        print(f'hbc-gdb-status: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
