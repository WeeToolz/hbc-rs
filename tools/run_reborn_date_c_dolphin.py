#!/usr/bin/env python3
"""Run the independent Reborn date parsing probe in Dolphin."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from dolphin_test_support import write_dolphin_config


DOLPHIN = Path('/Applications/Dolphin.app/Contents/MacOS/Dolphin')
PROFILE_ROOT = Path('/private/tmp/hbc-rs')
DATE_LINE = re.compile(
    r'HBC_DATE input=(.*?) count=(\d+) epoch=(-?\d+) '
    r'normalized=(-?\d+),(-?\d+),(-?\d+),(-?\d+),(-?\d+),(-?\d+)'
)
INVALID_LINE = re.compile(r'HBC_DATE input=(.*?) count=(\d+) invalid')
UPDATE_LINE = re.compile(r'HBC_UPDATE index=(\d+) value=(\d+)')


def parse_records(console: str) -> list[dict[str, object]]:
    records = []
    matches = [*DATE_LINE.finditer(console), *INVALID_LINE.finditer(console)]
    for match in sorted(matches, key=lambda item: item.start()):
        groups = match.groups()
        if len(groups) == 2:
            records.append({'input': groups[0], 'scan_count': int(groups[1]), 'invalid': True})
        else:
            records.append({
                'input': groups[0],
                'scan_count': int(groups[1]),
                'epoch': int(groups[2]),
                'normalized': [int(value) for value in groups[3:]],
            })
    return records


def parse_update_records(console: str) -> list[dict[str, object]]:
    return [
        {'index': int(match.group(1)), 'value': int(match.group(2))}
        for match in UPDATE_LINE.finditer(console)
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--dolphin-arch', choices=['arm64', 'x86_64'])
    parser.add_argument('--expected-date-cases', type=int, default=17)
    parser.add_argument('--expected-update-cases', type=int, default=8)
    parser.add_argument('--timeout', type=float, default=30)
    args = parser.parse_args()
    dol = args.dol.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    result: dict[str, object] = {
        'image': str(dol),
        'sha256': hashlib.sha256(dol.read_bytes()).hexdigest(),
        'physical_wii_tested': False,
        'expected_date_cases': args.expected_date_cases,
        'expected_update_cases': args.expected_update_cases,
        'success': False,
    }
    process = None
    try:
        PROFILE_ROOT.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='hbc-date-c-', dir=PROFILE_ROOT) as name:
            profile = Path(name)
            # This standalone parser probe does not need NAND content.
            (profile / 'Wii').mkdir()
            config = profile / 'Config'
            config.mkdir()
            write_dolphin_config(
                config,
                '[Core]\nCPUThread=False\nWiiSDCard=False\n'
                '[Interface]\nConfirmStop=False\nUsePanicHandlers=False\n',
            )
            (config / 'Logger.ini').write_text(
                '[Options]\nWriteToFile=True\nVerbosity=5\n'
                '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\nPI=True\n',
                encoding='utf-8',
            )
            shutil.copytree(config, args.output / 'Config')
            console_path = args.output / 'console.log'
            with console_path.open('w', encoding='utf-8') as console:
                command = [str(DOLPHIN), '-b', '-u', str(profile), '-e', str(dol)]
                if args.dolphin_arch:
                    command = ['/usr/bin/arch', f'-{args.dolphin_arch}', *command]
                result['command'] = command
                process = subprocess.Popen(command, stdout=console, stderr=subprocess.STDOUT)
                deadline = time.monotonic() + args.timeout
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        break
                    console.flush()
                    content = console_path.read_text(encoding='utf-8', errors='replace')
                    marker = (
                        f'HBC_META done dates={args.expected_date_cases} '
                        f'updates={args.expected_update_cases}'
                    )
                    if marker in content:
                        result['completion_marker'] = True
                        break
                    time.sleep(0.1)
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                        result['forced_termination'] = True
                console.flush()
            result['dolphin_exit_code'] = process.returncode
            content = console_path.read_text(encoding='utf-8', errors='replace')
            result['records'] = parse_records(content)
            result['update_records'] = parse_update_records(content)
            result['record_count'] = len(result['records'])
            result['update_record_count'] = len(result['update_records'])
            result['success'] = (
                result.get('completion_marker') is True
                and result['record_count'] == args.expected_date_cases
                and result['update_record_count'] == args.expected_update_cases
            )
            if not result['success']:
                result['error'] = 'Dolphin did not emit the complete C probe result set'
    except Exception as error:
        result['error'] = str(error)
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
