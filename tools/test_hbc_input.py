"""Host tests for the production network-input sender."""
import argparse
import importlib.util
from pathlib import Path
import socket
import threading
import unittest

SPEC = importlib.util.spec_from_file_location("hbc_input", Path(__file__).with_name("hbc-input.py"))
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)

class SenderTests(unittest.TestCase):
    def args(self, **changes):
        args = argparse.Namespace(kind="wiimote", buttons=["home"], mask=0, release=False,
            hold_ms=120, scroll=0, left_stick=[0,0], right_stick=[0,0], triggers=[0,0], pointer=None, screenshot=False)
        for key,value in changes.items(): setattr(args,key,value)
        return args

    def test_independent_wire_fixture(self):
        self.assertEqual(client.make_packet(self.args()), bytes.fromhex(
            "48424349 0102 0000 00000080 0078 0000 0000 0000 0000 0000 0000 0000 0000 0000"))
        release = client.make_packet(self.args(buttons=[], release=True))
        self.assertEqual(release, b"HBCI\x01\x02" + bytes(26))

    def test_invalid_user_input_is_rejected_before_network(self):
        for args in [self.args(buttons=["bogus"]), self.args(hold_ms=2001),
                     self.args(release=True), self.args(left_stick=[-32768,0]),
                     self.args(pointer=[-1,0]), self.args(triggers=[1,0])]:
            with self.assertRaises(ValueError): client.make_packet(args)

    def exchange(self, reply, expect_error=False):
        # Explicit controlled peer tests the real client's byte stream and EOF handling.
        packet = client.make_packet(self.args())
        received = bytearray()
        failures = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1",0)); listener.listen(1); listener.settimeout(2)
            def peer():
                try:
                    with listener.accept()[0] as connection:
                        connection.settimeout(2)
                        while len(received) < 32:
                            chunk = connection.recv(32-len(received))
                            if not chunk: break
                            received.extend(chunk)
                        for byte in reply: connection.sendall(bytes([byte]))
                except BaseException as error: failures.append(error)
            worker = threading.Thread(target=peer); worker.start()
            try:
                if expect_error:
                    with self.assertRaises(OSError): client.send_packet("127.0.0.1", listener.getsockname()[1], packet, 2)
                else: client.send_packet("127.0.0.1", listener.getsockname()[1], packet, 2)
            finally: worker.join(3)
            self.assertFalse(worker.is_alive()); self.assertFalse(failures)
        self.assertEqual(received, packet)

    def test_all_named_controller_buttons_have_valid_wire_masks(self):
        bindings = [("logical",client.LOGICAL),("gamecube",client.PAD),("wiimote",client.WII),
                    ("classic",client.CLASSIC),("generic",client.NORMAL)]
        for kind, buttons in bindings:
            for name, expected in buttons.items():
                packet=client.make_packet(self.args(kind=kind,buttons=[name]))
                self.assertEqual(int.from_bytes(packet[8:12],"big"),expected,(kind,name))
        guitar=client.make_packet(self.args(kind="guitar",buttons=["green","strum-down"]))
        self.assertEqual(guitar[8:12],bytes.fromhex("40100000"))
        for kind in ("gamecube","classic","generic"):
            packet=client.make_packet(self.args(kind=kind,buttons=[],triggers=[127,255]))
            self.assertEqual(packet[28:30],b"\x7f\xff")

    def test_complete_reply_and_fragmented_tcp_stream(self):
        self.exchange(b"HBCI\x01\x00\x00\x00")

    def test_rejection_pending_eof_and_invalid_reply_are_not_success(self):
        for reply in [b"HBCI\x01\x01\0\0", b"HBCI\x01\x04\0\0", b"HBCI\x01", b"BAD!\x01\0\0\0"]:
            self.exchange(reply, expect_error=True)

if __name__ == "__main__": unittest.main()
