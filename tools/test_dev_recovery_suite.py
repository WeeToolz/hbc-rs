#!/usr/bin/env python3
"""Run each development-wrapper regression in a separate Dolphin process."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

MODES = ('probe', 'invalid-args', 'watchdog-error', 'watchdog-zero', 'arm-timeout',
         'reporter-error', 'reporter-zero', 'return-zero', 'return-error',
         'report-disconnect', 'report-stall', 'log-overflow', 'log-contention', 'finish-wait', 'reset-stages', 'reset-stub-corrupt', 'reset-stub-entry')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--modes', nargs='+', choices=MODES, default=MODES)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for mode in args.modes:
        command = [sys.executable, str(Path(__file__).with_name('test_dev_recovery_dolphin.py')),
                   '--dol', str(args.dol.resolve()), '--elf', str(args.elf.resolve()),
                   '--output', str(args.output.resolve() / mode), '--host', args.host,
                   '--mode', mode, '--deadline-ms', '10000']
        with (args.output / (mode + '.log')).open('w') as log:
            try:
                completed = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120)
                case = {'mode': mode, 'exit_code': completed.returncode, 'success': completed.returncode == 0}
            except subprocess.TimeoutExpired:
                # The child owns Dolphin cleanup. A timeout is never a pass.
                case = {'mode': mode, 'success': False, 'error': 'Test exceeded 120 seconds'}
        results.append(case)
        print(json.dumps(case), flush=True)
        (args.output / 'result.json').write_text(json.dumps({'cases': results, 'success': all(r['success'] for r in results)}, indent=2) + '\n')
    return 0 if all(r['success'] for r in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
