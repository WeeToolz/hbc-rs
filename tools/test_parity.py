"""Check explicit reference roots in the existing parity ledger validator."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import parity


class ReferenceTests(unittest.TestCase):
    def test_reborn_references_use_reborn_and_cannot_escape_its_root(self):
        with tempfile.TemporaryDirectory(dir='/Volumes/SSD/WiiDev/tmp') as temporary:
            root = Path(temporary)
            original = root / 'original'
            reborn = root / 'reborn'
            original.mkdir()
            reborn.mkdir()
            (original / 'loader.c').write_text('original_loader')
            (reborn / 'loader.c').write_text('reborn_loader')
            with patch.multiple(parity, C_SOURCE=original, C_REBORN_SOURCE=reborn):
                parity.validate_ref('reborn:loader.c#reborn_loader', original, root / 'ledger', 1)
                parity.validate_ref('loader.c#original_loader', original, root / 'ledger', 1)
                for reference in ('reborn:loader.c#original_loader', 'reborn:../original/loader.c#original_loader'):
                    with self.assertRaises(ValueError):
                        parity.validate_ref(reference, original, root / 'ledger', 1)


if __name__ == '__main__':
    unittest.main()
