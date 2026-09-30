import configparser
import tempfile
import unittest
from pathlib import Path

from dolphin_capture_config import configure_capture_profile


class CaptureProfileTests(unittest.TestCase):
    def test_new_profile_enables_immediate_xfb_ram_copies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            configure_capture_profile(root)
            config = configparser.ConfigParser()
            config.read(root / "Config" / "GFX.ini")
            self.assertFalse(config.getboolean("Hacks", "XFBToTextureEnable"))
            self.assertFalse(config.getboolean("Hacks", "DeferEFBCopies"))

    def test_existing_profile_preserves_other_options_and_is_repeatable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Config").mkdir()
            path = root / "Config" / "GFX.ini"
            path.write_text("[Hacks]\nXFBToTextureEnable=True\nDeferEFBCopies=True\nEFBToTextureEnable=True\n[Settings]\nBackend=Metal\n")
            configure_capture_profile(root)
            first = path.read_text()
            configure_capture_profile(root)
            self.assertEqual(path.read_text(), first)
            config = configparser.ConfigParser()
            config.read(path)
            self.assertFalse(config.getboolean("Hacks", "XFBToTextureEnable"))
            self.assertFalse(config.getboolean("Hacks", "DeferEFBCopies"))
            self.assertTrue(config.getboolean("Hacks", "EFBToTextureEnable"))
            self.assertEqual(config["Settings"]["Backend"], "Metal")


if __name__ == "__main__":
    unittest.main()
