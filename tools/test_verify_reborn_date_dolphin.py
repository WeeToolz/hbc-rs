import unittest

from verify_reborn_date_dolphin import EXPECTED, compare


def normalized(expected):
    year, remainder = divmod(expected, 10_000_000_000)
    fields = []
    for divisor in (100_000_000, 1_000_000, 10_000, 100, 1):
        value, remainder = divmod(remainder, divisor)
        fields.append(value)
    return [year, *fields]


class RebornDateComparisonTests(unittest.TestCase):
    def test_rejects_incomplete_c_results(self):
        with self.assertRaisesRegex(ValueError, 'complete expected result set'):
            compare({'success': True, 'records': []}, {'success': True})

    def test_rejects_a_c_result_that_differs_from_reborn(self):
        records = []
        for value, expected in EXPECTED:
            if expected is None:
                records.append({'input': value, 'invalid': True})
            else:
                records.append({'input': value, 'epoch': 1, 'normalized': normalized(expected)})
        records[0]['normalized'][2] += 1
        with self.assertRaisesRegex(ValueError, 'expected Reborn date table'):
            compare(
                {'success': True, 'records': records},
                {'success': True, 'cases': [{'case': 'metadata-date', 'success': True, 'result': 0}]},
            )

    def test_accepts_complete_matching_c_and_rust_runs(self):
        records = []
        for value, expected in EXPECTED:
            if expected is None:
                records.append({'input': value, 'invalid': True})
            else:
                records.append({
                    'input': value,
                    'epoch': 1,
                    'normalized': normalized(expected),
                })
        result = compare(
            {'success': True, 'records': records, 'sha256': 'c'},
            {
                'success': True,
                'sha256': 'rust',
                'cases': [{'case': 'metadata-date', 'success': True, 'result': 0}],
            },
        )
        self.assertEqual(result['cases'], len(EXPECTED))


if __name__ == '__main__':
    unittest.main()
