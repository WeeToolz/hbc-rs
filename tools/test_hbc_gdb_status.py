import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('gdb_status', Path(__file__).with_name('hbc-gdb-status.py'))
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)
LISTENING = bytes.fromhex('48424344 01000201 00000000 00000000 00000000 0b0c0000 00000000 00000000')
FAILED = bytes.fromhex('48424344 0100030c 05000000 fffffffd ffffffe3 0b0c0000 00000000 00000000')


class DiagnosticTests(unittest.TestCase):
    def test_fixed_listening_reply(self):
        self.assertEqual(client.decode_reply(LISTENING), {
            'state': 'listening', 'ever_opened': True, 'listener_stopped': False,
            'operation': None, 'ios_error': None, 'cleanup_error': None, 'gdb_port': 2828})

    def test_fixed_failure_retains_signed_ios_codes(self):
        self.assertEqual(client.decode_reply(FAILED), {
            'state': 'startup-failed', 'ever_opened': False, 'listener_stopped': False,
            'operation': 'bind', 'ios_error': -3, 'cleanup_error': -29, 'gdb_port': 2828})

    def test_rejects_malformed_reply(self):
        for offset, value in [(0, 0), (4, 2), (5, 1), (6, 9), (7, 128), (8, 99), (9, 1), (22, 1)]:
            reply = bytearray(LISTENING)
            reply[offset] = value
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                client.decode_reply(reply)
        with self.assertRaises(ValueError):
            client.decode_reply(LISTENING[:-1])

    def test_query_handles_fragmented_wire_reply(self):
        with patch.object(client.socket, 'create_connection') as connect:
            connection = connect.return_value.__enter__.return_value
            connection.recv.side_effect = [LISTENING[:3], LISTENING[3:19], LISTENING[19:]]
            self.assertEqual(client.query('test-host')['state'], 'listening')
            connection.sendall.assert_called_once_with(b'HBCD\x01' + bytes(11))

    def test_query_returns_truncation_error(self):
        with patch.object(client.socket, 'create_connection') as connect:
            connection = connect.return_value.__enter__.return_value
            connection.recv.side_effect = [LISTENING[:4], b'']
            with self.assertRaisesRegex(OSError, '4/32'):
                client.query('test-host')


if __name__ == '__main__':
    unittest.main()
