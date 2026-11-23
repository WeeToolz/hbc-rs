#!/usr/bin/env python3
"""Compare Reborn C date results with the production Rust API Dolphin run."""

import argparse
import json
from pathlib import Path


EXPECTED = [
    ('20241332', 20250304010000),
    ('20250000', 20241231010000),
    ('20250101999999', 20250205044039),
    ('2025-01-02', 20250101010000),
    ('20250101-1-1-1', 20250131225859),
    ('19691201000000', None),
    ('-0010101', -9798990000),
    ('2025010112', 20250201010000),
    ('2025 1 1', 20250201010000),
    ('2025x0101', None),
    ('20250101123456tail', 20250201123456),
    ('202501011234', 20250201010000),
    ('20240131', 20240302010000),
    ('20250131', 20250303010000),
    ('20241231', 20250131010000),
    ('20250131235960', 20250304000000),
    ('20250102', 20250202010000),
]
EXPECTED_UPDATE = [
    123,
    456,
    18446744073709551609,
    0,
    0,
    0,
    0,
    0,
]


def normalized_key(record: dict[str, object]) -> int | None:
    if record.get('invalid') or record.get('epoch') == 0:
        return None
    year, month, day, hour, minute, second = record['normalized']
    return (
        year * 10_000_000_000
        + month * 100_000_000
        + day * 1_000_000
        + hour * 10_000
        + minute * 100
        + second
    )


def compare(c_result: dict[str, object], rust_result: dict[str, object]) -> dict[str, object]:
    records = c_result.get('records')
    if not c_result.get('success') or not isinstance(records, list) or len(records) != len(EXPECTED):
        raise ValueError('C Dolphin run does not contain the complete expected result set')
    actual = [(record['input'], normalized_key(record)) for record in records]
    if actual != EXPECTED:
        raise ValueError(f'C results do not match the expected Reborn date table: {actual!r}')
    cases = rust_result.get('cases')
    update_records = c_result.get('update_records')
    if not isinstance(update_records, list) or [
        (record['index'], record['value']) for record in update_records
    ] != list(enumerate(EXPECTED_UPDATE)):
        raise ValueError('C update date results do not match Reborn sscanf behavior')
    expected_cases = [
        {'case': 'metadata-date', 'success': True, 'result': 0},
        {'case': 'metadata-update', 'success': True, 'result': 0},
    ]
    if not rust_result.get('success') or cases != expected_cases:
        raise ValueError('Rust Dolphin run did not pass both production metadata APIs')
    return {
        'success': True,
        'apis': [
            'hbc_rs::metadata::parse_app_xml',
            'hbc_rs::metadata::parse_update_xml',
        ],
        'comparison': '17 release dates and 8 update dates match independent Reborn C runtime results',
        'cases': len(EXPECTED),
        'update_cases': len(EXPECTED_UPDATE),
        'dolphin_c_sha256': c_result['sha256'],
        'dolphin_rust_sha256': rust_result['sha256'],
        'physical_wii_tested': False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--c-result', type=Path, required=True)
    parser.add_argument('--rust-result', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = compare(
        json.loads(args.c_result.read_text(encoding='utf-8')),
        json.loads(args.rust_result.read_text(encoding='utf-8')),
    )
    args.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
