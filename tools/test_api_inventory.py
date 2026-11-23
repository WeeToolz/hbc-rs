"""Check API inventory identity and evidence mapping."""
import unittest
import api_inventory


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.document = {
            'format_version': 61,
            'paths': {'1': {'crate_id': 0, 'path': ['hbc_rs', 'metadata', 'parse_app_xml'], 'kind': 'function'}},
            'index': {'1': {'id': 1, 'crate_id': 0, 'name': 'parse_app_xml',
                            'visibility': 'public', 'span': {'filename': 'src/metadata.rs', 'begin': [78, 1]},
                            'inner': {'function': {}}}},
        }
        self.ledger = [{'id': 'HBC-API-DATE-01', 'rust_ref': 'src/metadata.rs#pub fn parse_app_xml',
                        'coverage': 'partial', 'verification': 'dolphin_tested'}]

    def test_exact_api_evidence_is_partial_and_unmapped_api_is_unverified(self):
        self.assertEqual(api_inventory.collect(self.document, self.ledger)[0]['status'], 'partial')
        self.assertEqual(api_inventory.collect(self.document, [])[0]['status'], 'unverified')

    def test_public_const_function_references_resolve(self):
        self.ledger[0]['rust_ref'] = 'src/metadata.rs#pub const fn parse_app_xml'
        self.assertEqual(api_inventory.collect(self.document, self.ledger)[0]['status'], 'partial')

    def test_unknown_rustdoc_format_and_missing_api_are_rejected(self):
        self.document['format_version'] = 999
        with self.assertRaises(ValueError):
            api_inventory.collect(self.document, self.ledger)

    def test_public_inherent_and_trait_methods_get_owner_paths(self):
        self.document['paths']['2'] = {
            'crate_id': 0, 'path': ['hbc_rs', 'Metadata'], 'kind': 'struct'
        }
        self.document['paths']['3'] = {
            'crate_id': 0, 'path': ['hbc_rs', 'MetadataReader'], 'kind': 'trait'
        }
        self.document['index']['2'] = {
            'id': 2, 'crate_id': 0, 'name': 'Metadata', 'visibility': 'public',
            'inner': {'struct': {}}
        }
        self.document['index']['3'] = {
            'id': 3, 'crate_id': 0, 'name': 'MetadataReader', 'visibility': 'public',
            'inner': {'trait': {'items': [4]}}
        }
        self.document['index']['4'] = {
            'id': 4, 'crate_id': 0, 'name': 'read', 'visibility': 'default',
            'span': {'filename': 'src/metadata.rs', 'begin': [12, 1]},
            'inner': {'function': {}}
        }
        self.document['index']['5'] = {
            'id': 5, 'crate_id': 0, 'visibility': 'default',
            'inner': {'impl': {'for': {'resolved_path': {'id': 2}}, 'items': [6]}}
        }
        self.document['index']['6'] = {
            'id': 6, 'crate_id': 0, 'name': 'new', 'visibility': 'public',
            'span': {'filename': 'src/metadata.rs', 'begin': [20, 1]},
            'inner': {'function': {}}
        }

        apis = api_inventory.public_functions(self.document)
        paths = {api['api'] for api in apis}
        self.assertIn('hbc_rs::MetadataReader::read', paths)
        self.assertIn('hbc_rs::Metadata::new', paths)
        self.document['format_version'] = 61
        self.ledger[0]['rust_ref'] = 'src/metadata.rs#pub fn missing'
        with self.assertRaises(ValueError):
            api_inventory.collect(self.document, self.ledger)


if __name__ == '__main__':
    unittest.main()
