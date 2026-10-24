import unittest
import struct
import tempfile
import zlib
from pathlib import Path

from xfb_capture import summarize_yuyv_luma, write_yuyv_png


class YuyvCaptureTests(unittest.TestCase):
    def test_rejects_the_black_xfb_seen_in_dolphin(self):
        black = bytes((16, 128, 16, 128)) * (640 * 480 // 2)
        with self.assertRaisesRegex(ValueError, "blank or nearly blank"):
            summarize_yuyv_luma(black, 640, 480)

    def test_accepts_a_frame_with_visible_range_and_scene_detail(self):
        luma_values = [*range(16, 256, 16), 248]
        pixels = len(luma_values)
        frame = bytes(
            channel
            for pair in range(0, pixels, 2)
            for channel in (luma_values[pair], 128, luma_values[pair + 1], 128)
        )

        summary = summarize_yuyv_luma(frame, pixels, 1)

        self.assertEqual(summary["active_luma_pixels"], 14)
        self.assertEqual(summary["luma_levels"], 16)
        self.assertEqual(summary["minimum_luma"], 16)
        self.assertEqual(summary["maximum_luma"], 248)

    def test_png_contains_expected_black_and_white_pixels(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'frame.png'
            write_yuyv_png(path, bytes((16, 128, 235, 128)), 2, 1)
            png = path.read_bytes()
        self.assertEqual(png[:8], b'\x89PNG\r\n\x1a\n')
        offset = 8
        image_data = bytearray()
        while offset < len(png):
            size = struct.unpack_from('>I', png, offset)[0]
            kind = png[offset + 4:offset + 8]
            payload = png[offset + 8:offset + 8 + size]
            expected_crc = struct.unpack_from('>I', png, offset + 8 + size)[0]
            self.assertEqual(zlib.crc32(kind + payload), expected_crc)
            if kind == b'IHDR':
                self.assertEqual(payload, bytes.fromhex('00000002000000010802000000'))
            if kind == b'IDAT':
                image_data.extend(payload)
            offset += size + 12
        self.assertEqual(zlib.decompress(image_data), b'\x00\x00\x00\x00\xff\xff\xff')

    def test_rejects_wrong_capture_size(self):
        with self.assertRaisesRegex(ValueError, "expected 8"):
            summarize_yuyv_luma(bytes(4), 2, 2)


if __name__ == "__main__":
    unittest.main()
