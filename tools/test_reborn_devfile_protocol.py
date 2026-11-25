"""Host checks for the HBCF Dolphin protocol test packet builders."""
import importlib.util
from pathlib import Path
import sys
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "remote_input_dolphin", ROOT / "tools/test_remote_input_dolphin.py"
)
remote = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(remote)


class HbcfPacketTests(unittest.TestCase):
    def test_request_header_matches_reborn_byte_layout(self):
        packet = remote.devfile_packet(b"p", "sd:/apps/demo/boot.dol", 1234, flags=1)
        self.assertEqual(packet[:4], b"HBCF")
        self.assertEqual(packet[4:8], b"p\x01\x00\x16")
        self.assertEqual(int.from_bytes(packet[8:12], "big"), 1234)
        self.assertEqual(packet[12:16], bytes(4))
        self.assertEqual(packet[16:], b"sd:/apps/demo/boot.dol")

    def test_framed_upload_builder_uses_zlib_only_when_it_saves_space(self):
        raw = b"repeated data" * 300
        frame = remote.devfile_put_frame(raw)
        raw_len = int.from_bytes(frame[:4], "big")
        wire_len = int.from_bytes(frame[4:8], "big")
        checksum = int.from_bytes(frame[8:12], "big")
        wire = frame[12:]
        self.assertEqual(raw_len, len(raw))
        self.assertEqual(wire_len, len(wire))
        self.assertLess(wire_len, raw_len)
        self.assertEqual(zlib.decompress(wire), raw)
        self.assertEqual(checksum, zlib.crc32(raw) & 0xFFFFFFFF)

        raw = bytes(range(256))
        frame = remote.devfile_put_frame(raw)
        self.assertEqual(int.from_bytes(frame[4:8], "big"), len(raw))
        self.assertEqual(frame[12:], raw)

    def test_path_bound_is_enforced_before_packet_construction(self):
        with self.assertRaises(ValueError):
            remote.devfile_packet(b"S", "x" * 256)


if __name__ == "__main__":
    unittest.main()
