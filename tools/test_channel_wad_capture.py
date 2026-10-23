"""Regression checks for Dolphin screenshots stored below a title directory."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_channel_wad_dolphin import capture_screenshot


class ScreenshotTests(unittest.TestCase):
    def test_copies_new_nested_screenshot_and_excludes_previous_image(self):
        with tempfile.TemporaryDirectory(dir='/Volumes/SSD/WiiDev/tmp') as temporary:
            root = Path(temporary)
            screenshots = root / 'ScreenShots'
            title = screenshots / 'OHBC'
            title.mkdir(parents=True)
            (title / 'old.png').write_bytes(b'previous image')
            output = root / 'evidence'
            output.mkdir()

            def save_image(fd, button):
                self.assertEqual((fd, button), (12, 'Y'))
                (title / 'new.png').write_bytes(b'new screenshot bytes')

            with patch('test_channel_wad_dolphin.tap', side_effect=save_image), patch('test_channel_wad_dolphin.time.sleep'):
                result = capture_screenshot(12, screenshots, output)
            self.assertEqual(result.read_bytes(), b'new screenshot bytes')


if __name__ == '__main__':
    unittest.main()
