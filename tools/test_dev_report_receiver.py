"""Check that the acceptance receiver rejects incomplete or false evidence."""
import struct
import unittest
import threading
from dev_report_receiver import ReportReceiver

BUILD = 'a' * 64
RUN = 'receiver-test'


class Connection:
    def __init__(self, data):
        self.data = data

    def settimeout(self, value):
        self.timeout = value

    def recv(self, size):
        # Deliberately split every header and payload across small reads.
        count = min(size, 3)
        part, self.data = self.data[:count], self.data[count:]
        return part


class Listener:
    def __init__(self, data):
        self.connection = Connection(data)

    def accept(self):
        return self.connection, ('test', 1)


def final(text, run=RUN, flags=0):
    # Independent wire layout, not the production encoder.
    header = b'WIR2' + struct.pack('>HHI', 2, flags, len(text)) + bytes(12) + struct.pack('>I', 36)
    return header + run.encode().ljust(64, b'\0') + BUILD.encode() + text + bytes(36)


def receive(data):
    receiver = ReportReceiver.__new__(ReportReceiver)
    receiver.listener = Listener(data)
    receiver.connection = None
    receiver.stop = threading.Event()
    receiver.progress = []
    receiver.final = None
    receiver.error = None
    receiver.bytes_received = 0
    receiver._run()
    return receiver


class ReceiverTests(unittest.TestCase):
    def test_complete_report_survives_short_reads(self):
        receiver = receive(final(b'PASS summary passes=1 fails=0\n'))
        self.assertEqual(receiver.validate(BUILD, RUN, 'return-zero')['flags'], 0)

    def test_missing_operation_result_cannot_pass(self):
        receiver = receive(final(b'INFO worker alive\n'))
        with self.assertRaisesRegex(RuntimeError, 'preserve'):
            receiver.validate(BUILD, RUN, 'return-zero')

    def test_wrong_run_identity_is_rejected(self):
        receiver = receive(final(b'PASS summary passes=1 fails=0\n', run='other'))
        with self.assertRaisesRegex(RuntimeError, 'identity'):
            receiver.validate(BUILD, RUN, 'return-zero')

    def test_failure_must_preserve_original_application_error(self):
        receiver = receive(final(b'FAIL summary passes=0 fails=1\n'))
        with self.assertRaisesRegex(RuntimeError, 'Original'):
            receiver.validate(BUILD, RUN, 'return-error')
        receiver = receive(final(b'INFO dev main exit status=-7\nFAIL summary passes=0 fails=1\n'))
        receiver.validate(BUILD, RUN, 'return-error')

    def test_partial_report_is_failure_even_when_header_is_valid(self):
        receiver = receive(final(b'PASS summary passes=1 fails=0\n')[:-1])
        with self.assertRaises(RuntimeError):
            receiver.validate(BUILD, RUN, 'return-zero')
        receiver.validate_failure()

    def test_log_loss_requires_failure_and_truncation_flag(self):
        text = b'FAIL summary passes=0 fails=1\n'
        with self.assertRaisesRegex(RuntimeError, 'truncation'):
            receive(final(text)).validate(BUILD, RUN, 'log-overflow')
        receive(final(text, flags=1)).validate(BUILD, RUN, 'log-overflow')

    def test_empty_transport_is_not_partial_report_evidence(self):
        with self.assertRaises(RuntimeError):
            receive(b'').validate_failure()


if __name__ == '__main__':
    unittest.main()
