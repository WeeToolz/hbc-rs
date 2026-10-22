"""Tests for the real terminal sender with explicit controlled transport peers."""
import importlib.util
from pathlib import Path
import socket
import struct
import threading
import unittest

SPEC = importlib.util.spec_from_file_location("hbc_control", Path(__file__).with_name("hbc-control.py"))
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)

class ControlTests(unittest.TestCase):
    def test_independent_request_bytes(self):
        self.assertEqual(client.packet(1,7), b"HBCC\x01\x01\0\0\0\0\0\x07\0\0\0\0")
        self.assertEqual(client.packet(0,7), b"HBCQ\x01\0\0\0\0\0\0\x07\0\0\0\0")
        with self.assertRaises(ValueError): client.packet(1,0)

    def peer(self, response, raises=False):
        received = bytearray()
        errors = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1",0)); listener.listen(1); listener.settimeout(2)
            def serve():
                try:
                    with listener.accept()[0] as connection:
                        connection.settimeout(2)
                        while len(received)<16:
                            part=connection.recv(16-len(received))
                            if not part: break
                            received.extend(part)
                        for byte in response: connection.sendall(bytes([byte]))
                except BaseException as error: errors.append(error)
            worker=threading.Thread(target=serve); worker.start()
            try:
                if raises:
                    with self.assertRaises((OSError,UnicodeError)):
                        client.exchange("127.0.0.1",listener.getsockname()[1],1,7,2)
                else:
                    result=client.exchange("127.0.0.1",listener.getsockname()[1],1,7,2)
                    self.assertEqual(result,{"state":"failed","operation":1,"id":7,"session":9,"message":"IOS -6"})
            finally: worker.join(3)
            self.assertFalse(worker.is_alive()); self.assertFalse(errors)
        self.assertEqual(bytes(received),b"HBCC\x01\x01\0\0\0\0\0\x07\0\0\0\0")

    def test_fragmented_real_socket_reply(self):
        self.peer(b"HBCC\x01\x03\x01\0\0\0\0\x07\0\0\0\x09\0\x06\0\0IOS -6")

    def test_eof_invalid_version_oversized_and_wrong_id_rejected(self):
        fixture=b"HBCC\x01\x03\x01\0\0\0\0\x07\0\0\0\x09\0\0\0\0"
        for response in [fixture[:10],fixture.replace(b"HBCC",b"BAD!"),
                         fixture[:4]+b"\x02"+fixture[5:],fixture[:11]+b"\x08"+fixture[12:],
                         fixture[:16]+b"\x03\x00"+fixture[18:]]:
            self.peer(response,True)

    def run_states(self, states, expected_error=False, command=1):
        calls=[]; ticks=[0.0]
        def exchange(host,port,operation,identity):
            calls.append((operation,identity))
            value=states.pop(0)
            if isinstance(value,BaseException): raise value
            state,session=value
            return {"id":identity,"operation":command if identity else 0,"session":session,"state":state,"message":state}
        def sleep(seconds): ticks[0]+=seconds
        if expected_error:
            with self.assertRaises(OSError): client.run_command("host",4299,command,.7,exchange,lambda:ticks[0],sleep)
        else: self.assertEqual(client.run_command("host",4299,command,.7,exchange,lambda:ticks[0],sleep),0)
        self.assertEqual(sum(operation!=0 for operation,_ in calls),1)
        return calls

    def test_preflight_failure_submits_no_hardware_command(self):
        calls=[]
        def transport(host,port,operation,identity):
            calls.append((operation,identity)); raise OSError("timeout")
        with self.assertRaisesRegex(OSError,"no hardware command was sent"):
            client.run_command("host",4299,1,1,exchange_fn=transport)
        self.assertEqual(calls,[(0,0)])

    def test_submission_is_not_completion_and_backend_failure_is_returned(self):
        self.run_states([("unknown",9),("queued",9),("running",9),("failed",9)],True)
        self.run_states([("unknown",9),("queued",9),("completed",9)],command=6)

    def test_lost_ack_never_repeats_operation_and_offline_is_not_success(self):
        self.run_states([("unknown",9),OSError("lost ack"),("running",9),("failed",9)],True)
        self.run_states([("unknown",9),("running",9)]+[OSError("offline")]*5,True)

    def test_restart_requires_a_new_session_and_other_commands_reject_it(self):
        self.run_states([("unknown",9),("queued",9),("unknown",10)])
        self.run_states([("unknown",9),("queued",9),("unknown",10)],True,command=3)

if __name__=="__main__": unittest.main()
