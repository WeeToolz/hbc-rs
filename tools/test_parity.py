"""Check explicit reference roots in the existing parity ledger validator."""
from pathlib import Path
import tempfile
import json
import unittest
from unittest.mock import patch

import parity


class ReferenceTests(unittest.TestCase):
    def test_status_report_counts_dolphin_and_wii_pending_separately(self):
        row = {'id': 'DATE', 'area': 'date', 'c_ref': 'source#symbol',
               'rust_ref': 'source#symbol', 'coverage': 'partial',
               'review': 'needs_followup', 'verification': 'dolphin_tested',
               'next_step': 'Verify on Wii'}
        output = parity.render([row])
        self.assertIn('Dolphin tested: **1**', output)
        self.assertIn('Wii evidence explicitly pending: **1**', output)

    def test_dolphin_evidence_requires_successful_nonempty_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / 'result.json'
            for payload in ({'success': True, 'cases': []},
                            {'success': True, 'cases': [{'case': 'date', 'success': False, 'result': 0}]},
                            {'success': False, 'cases': [{'case': 'date', 'success': True, 'result': 0}]},
                            {'success': True, 'cases': [{'case': 'other', 'success': True, 'result': 0}]},
                            {'success': True, 'cases': [{'case': 'date', 'success': True, 'result': False}]},
                            {'success': True, 'cases': [{'case': 'date', 'success': True, 'result': 1}]}):
                report.write_text(json.dumps(payload))
                with self.assertRaises(ValueError):
                    parity.validate_dolphin_evidence(report, 'date')
            report.write_text(json.dumps({'success': True, 'cases': [{'case': 'date', 'success': True, 'result': 0}]}))
            parity.validate_dolphin_evidence(report, 'date')

    def test_reborn_references_use_reborn_and_cannot_escape_its_root(self):
        with tempfile.TemporaryDirectory() as temporary:
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

    def test_reborn_sdk_references_use_sdk_root_and_cannot_escape_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = root / 'channelapp'
            sdk = root / 'sdk'
            original.mkdir()
            sdk.mkdir()
            (sdk / 'agent.c').write_text('hbc_agent_home_pending')
            with patch.multiple(parity, C_SOURCE=original, C_REBORN_SDK_SOURCE=sdk):
                parity.validate_ref('reborn-sdk:agent.c#hbc_agent_home_pending', original,
                                    root / 'ledger', 1)
                with self.assertRaises(ValueError):
                    parity.validate_ref('reborn-sdk:../channelapp/agent.c#hbc_agent_home_pending',
                                        original, root / 'ledger', 1)

    def test_rust_module_function_reference_is_qualified_by_its_source_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'src' / 'platform' / 'theme.rs'
            source.parent.mkdir(parents=True)
            source.write_text('pub fn load() {}\nimpl ThemeTextures { pub fn load() {} }\n')
            with patch.object(parity, 'ROOT', root):
                parity.validate_ref('src/platform/theme.rs#platform::theme::load',
                                    root, root / 'ledger', 1)
                with self.assertRaises(ValueError):
                    parity.validate_ref('src/platform/theme.rs#other::theme::load',
                                        root, root / 'ledger', 1)


if __name__ == '__main__':
    unittest.main()
