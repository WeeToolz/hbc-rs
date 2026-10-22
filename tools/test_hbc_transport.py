"""Tests for the exact-read engine used by all three terminal clients."""
import socket
import unittest

from hbc_transport import recv_exact


class Stream:
    """An explicit byte stream that can end or fail between fragments."""
    def __init__(self, fragments):
        self.fragments = list(fragments)
        self.requests = []

    def recv(self, size):
        self.requests.append(size)
        if not self.fragments:
            return b""
        fragment = self.fragments.pop(0)
        if isinstance(fragment, BaseException):
            raise fragment
        part = fragment[:size]
        if fragment[size:]:
            self.fragments.insert(0, fragment[size:])
        return part


class ExactReadTests(unittest.TestCase):
    def test_fragmented_read_leaves_the_next_message_in_the_stream(self):
        stream = Stream([b"H", b"BC", b"I\x01next"])
        self.assertEqual(recv_exact(stream, 5), b"HBCI\x01")
        self.assertEqual(stream.requests, [5, 4, 2])
        self.assertEqual(recv_exact(stream, 4), b"next")

    def test_truncation_reports_received_and_required_lengths(self):
        for fragments, count in [([], 0), ([b"HBC", b"I"], 4)]:
            with self.subTest(count=count), self.assertRaisesRegex(OSError, f"{count}/8"):
                recv_exact(Stream(fragments), 8)

    def test_timeout_and_transport_failures_are_returned(self):
        for error in [socket.timeout("deadline"), OSError("connection reset")]:
            with self.subTest(error=error), self.assertRaises(type(error)) as caught:
                recv_exact(Stream([b"HBC", error]), 8)
            self.assertIs(caught.exception, error)

    def test_empty_read_needs_no_socket_call_and_negative_size_is_invalid(self):
        stream = Stream([])
        self.assertEqual(recv_exact(stream, 0), b"")
        with self.assertRaises(ValueError):
            recv_exact(stream, -1)
        self.assertEqual(stream.requests, [])


if __name__ == "__main__":
    unittest.main()
