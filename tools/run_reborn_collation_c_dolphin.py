#!/usr/bin/env python3
"""Run the installed devkitPPC C locale probe in a private Dolphin profile."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from dolphin_test_support import write_dolphin_config

ROOT = Path(__file__).resolve().parents[1]
PROFILE_ROOT = Path('/private/tmp/hbc-rs')
DOLPHIN = Path('/Applications/Dolphin.app/Contents/MacOS/Dolphin')
MARKER = b'PASS HBC_COLLATE ASCII-only UTF-8 byte order'


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dol', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=20)
    args = parser.parse_args()
    dol = args.dol.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    PROFILE_ROOT.mkdir(parents=True, exist_ok=True)
    process = None
    result = {
        'sha256': hashlib.sha256(dol.read_bytes()).hexdigest(),
        'dolphin_arch': 'x86_64',
        'success': False,
    }
    try:
        with tempfile.TemporaryDirectory(prefix='hbc-collation-c-', dir=PROFILE_ROOT) as name:
            profile = Path(name)
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
                '[Logs]\nOSREPORT=True\nCORE=True\nBOOT=True\n',
                encoding='utf-8',
            )
            shutil.copytree(config, args.output / 'Config')
            log_path = args.output / 'catalog.log'
            command = [
                '/usr/bin/arch', '-x86_64', str(DOLPHIN), '-b', '-u', str(profile),
                '-e', str(dol),
            ]
            result['command'] = command
            result['profile'] = str(profile)
            with log_path.open('wb') as log:
                process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
                deadline = time.monotonic() + args.timeout
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError('Dolphin exited before the C probe reported')
                    log.flush()
                    if MARKER in log_path.read_bytes():
                        result['completion_marker'] = True
                        result['success'] = True
                        break
                    time.sleep(0.1)
                if not result['success']:
                    raise TimeoutError('C locale result did not appear in Dolphin')
    except Exception as error:
        result['error'] = str(error)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                if result.get('completion_marker'):
                    result['forced_termination_after_marker'] = True
                else:
                    result['success'] = False
                    result['cleanup_error'] = 'Dolphin required forced termination before the probe result'
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
