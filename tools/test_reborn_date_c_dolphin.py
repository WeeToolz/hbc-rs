import unittest

from run_reborn_date_c_dolphin import parse_records, parse_update_records


class RebornDateProbeTests(unittest.TestCase):
    def test_parses_valid_and_invalid_scan_results_in_order(self):
        console = (
            'HBC_DATE input=20250102 count=3 epoch=1738458000 '
            'normalized=2025,2,2,1,0,0\n'
            'HBC_DATE input=2025x0101 count=1 invalid\n'
        )
        self.assertEqual(
            parse_records(console),
            [
                {
                    'input': '20250102',
                    'scan_count': 3,
                    'epoch': 1738458000,
                    'normalized': [2025, 2, 2, 1, 0, 0],
                },
                {'input': '2025x0101', 'scan_count': 1, 'invalid': True},
            ],
        )

    def test_parses_update_unsigned_scan_results(self):
        console = (
            'HBC_UPDATE index=1 value=456\n'
            'HBC_UPDATE index=2 value=18446744073709551609\n'
            'HBC_UPDATE index=7 value=0\n'
        )
        self.assertEqual(
            parse_update_records(console),
            [
                {'index': 1, 'value': 456},
                {'index': 2, 'value': 18446744073709551609},
                {'index': 7, 'value': 0},
            ],
        )


if __name__ == '__main__':
    unittest.main()
