import unittest

from verify_reborn_date_dolphin import EXPECTED, EXPECTED_UPDATE, compare


def normalized(expected):
    year, remainder = divmod(expected, 10_000_000_000)
    fields = []
    for divisor in (100_000_000, 1_000_000, 10_000, 100, 1):
        value, remainder = divmod(remainder, divisor)
        fields.append(value)
    return [year, *fields]


def c_result():
    records = []
    for value, expected in EXPECTED:
        if expected is None:
            records.append({'input': value, 'invalid': True})
        else:
            records.append({'input': value, 'epoch': 1, 'normalized': normalized(expected)})
    return {
        'success': True,
        'records': records,
        'update_records': [
            {'index': index, 'value': value}
            for index, value in enumerate(EXPECTED_UPDATE)
        ],
        'sha256': 'c',
    }


def rust_result():
    return {
        'success': True,
        'sha256': 'rust',
        'cases': [
            {'case': 'metadata-date', 'success': True, 'result': 0},
            {'case': 'metadata-update', 'success': True, 'result': 0},
        ],
    }


class RebornDateComparisonTests(unittest.TestCase):
    def test_rejects_incomplete_c_results(self):
        with self.assertRaisesRegex(ValueError, 'complete expected result set'):
            compare({'success': True, 'records': []}, rust_result())

    def test_rejects_a_c_result_that_differs_from_reborn(self):
        captured = c_result()
        captured['records'][0]['normalized'][2] += 1
        with self.assertRaisesRegex(ValueError, 'expected Reborn date table'):
            compare(captured, rust_result())

    def test_accepts_complete_matching_c_and_rust_runs(self):
        result = compare(c_result(), rust_result())
        self.assertEqual(result['cases'], len(EXPECTED))
        self.assertEqual(result['update_cases'], len(EXPECTED_UPDATE))


if __name__ == '__main__':
    unittest.main()
