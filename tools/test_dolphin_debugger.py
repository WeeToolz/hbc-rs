"""Deterministic tests for the production Dolphin debugger packet parser."""
import unittest
import socket
from unittest.mock import patch

from dolphin_test_support import Debugger


class ScriptedSocket:
    def __init__(self, *chunks):
        self.chunks = list(chunks)
        self.sent = []
        self.current_timeout = 17
        self.timeout_history = []
        self.on_recv = None

    def recv(self, _size):
        if self.on_recv:
            self.on_recv()
        if not self.chunks:
            raise socket.timeout()
        return self.chunks.pop(0)

    def sendall(self, data):
        self.sent.append(data)

    def gettimeout(self):
        return self.current_timeout

    def settimeout(self, seconds):
        self.current_timeout = seconds
        self.timeout_history.append(seconds)


class DebuggerTests(unittest.TestCase):
    def test_valid_packet_is_acknowledged(self):
        connection = ScriptedSocket(b'+$OK#9a')
        debugger = Debugger(connection, {})

        self.assertEqual(debugger.packet(), 'OK')
        self.assertEqual(connection.sent, [b'+'])

    def test_invalid_checksum_is_rejected_without_ack(self):
        connection = ScriptedSocket(b'$OK#00')
        debugger = Debugger(connection, {})

        with self.assertRaisesRegex(RuntimeError, 'checksum'):
            debugger.packet()
        self.assertEqual(connection.sent, [])

    def test_empty_response_is_returned_without_waiting_for_another_packet(self):
        connection = ScriptedSocket(b'$#00')
        debugger = Debugger(connection, {})

        self.assertEqual(debugger.command('unsupported'), '')
        self.assertEqual(connection.sent[-1], b'+')

    def test_memory_requires_exact_response_length(self):
        connection = ScriptedSocket(b'$0102#c3')
        debugger = Debugger(connection, {})

        with self.assertRaisesRegex(RuntimeError, 'Truncated'):
            debugger.memory(0x1000, 3)
        self.assertTrue(connection.sent[0].startswith(b'$m1000,3#'))
        self.assertEqual(connection.sent[-1], b'+')

    def test_command_queues_async_stop_then_returns_command_reply(self):
        connection = ScriptedSocket(b'$T05#b9', b'$01020304#8a')
        debugger = Debugger(connection, {})

        self.assertEqual(debugger.memory(0x1000, 4), bytes.fromhex('01020304'))
        self.assertEqual(debugger.stopped(), 'T05')
        self.assertEqual(connection.sent[0], b'$m1000,4#8e')
        self.assertEqual(connection.sent.count(b'+'), 2)

    def test_packet_timeout_is_bounded_and_restored(self):
        connection = ScriptedSocket()
        debugger = Debugger(connection, {})
        clock = iter((10, 10, 12))

        with patch('dolphin_test_support.time.monotonic', side_effect=lambda: next(clock)):
            with self.assertRaises(TimeoutError):
                debugger.packet(2)

        self.assertEqual(connection.timeout_history[0], 2)
        self.assertEqual(connection.current_timeout, 17)

    def test_stop_wait_uses_one_deadline_for_non_stop_packets(self):
        connection = ScriptedSocket(b'$OK#9a', b'$OK#9a')
        debugger = Debugger(connection, {})
        now = [0]

        def advance_on_receive():
            now[0] += 0.6

        connection.on_recv = advance_on_receive
        with patch('dolphin_test_support.time.monotonic', side_effect=lambda: now[0]):
            with self.assertRaises(TimeoutError):
                debugger.stopped(1)

        receive_limits = [value for value in connection.timeout_history if value != 17]
        self.assertEqual(receive_limits[0], 1)
        self.assertLess(receive_limits[1], receive_limits[0])
        self.assertEqual(connection.current_timeout, 17)


if __name__ == '__main__':
    unittest.main()
