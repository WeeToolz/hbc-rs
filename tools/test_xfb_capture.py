import unittest

from xfb_capture import summarize_yuyv_luma


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

    def test_rejects_wrong_capture_size(self):
        with self.assertRaisesRegex(ValueError, "expected 8"):
            summarize_yuyv_luma(bytes(4), 2, 2)


if __name__ == "__main__":
    unittest.main()
