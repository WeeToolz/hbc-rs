#!/usr/bin/env python3
"""Check the production asset validator with independent malformed inputs."""
from pathlib import Path
import tempfile
import unittest
from return_stub import ASSETS, ROOT, absolute_pair, bundled, validate


class ReturnStubTests(unittest.TestCase):
    def test_bundled_asset(self):
        result = bundled()
        self.assertEqual(result['real_destination'], 0x1848)
        self.assertEqual(result['symbols']['_start'], 0x80001800)

    def check_rejected(self, image, message):
        with tempfile.TemporaryDirectory(dir=ROOT / 'tmp') as directory:
            binary = Path(directory) / 'stub.bin'
            binary.write_bytes(image)
            with self.assertRaisesRegex(ValueError, message):
                validate(binary)

    def test_original_wrong_link_destination(self):
        # These are the defective asset's actual address-load words. The
        # expected error is independent of the address decoder under test.
        image = bytearray((ASSETS / 'return_stub.bin').read_bytes())
        image[0x24:0x2c] = bytes.fromhex('3c60800060633f48')
        self.check_rejected(image, 'Incorrect real-mode destination 0x00003f48')

    def test_wrong_virtual_destination(self):
        image = bytearray((ASSETS / 'return_stub.bin').read_bytes())
        image[0x16c:0x174] = bytes.fromhex('3c60800060634088')
        self.check_rejected(image, 'mode-transition destinations')

    def test_changed_code_and_bad_bounds(self):
        image = bytearray((ASSETS / 'return_stub.bin').read_bytes())
        image[0x200] ^= 1
        self.check_rejected(image, 'does not match its ELF')
        self.check_rejected(bytes(0x1701), 'overlaps return-title')
        self.check_rejected(bytes(16), 'truncated')

    def test_unknown_load_sequence(self):
        with self.assertRaisesRegex(ValueError, 'Unsupported address-load'):
            absolute_pair(bytes(8), 0)


if __name__ == '__main__':
    unittest.main()
