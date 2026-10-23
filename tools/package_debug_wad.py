#!/usr/bin/env python3
"""Package the HBC debug channel with the native libogc-rs WAD tool.

Build the ELF first with debugger-network and CARGO_PROFILE_RELEASE_DEBUG=2.
This command checks ELF-to-DOL conversion. It does not install a title.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def run(arguments):
    return subprocess.check_output([str(value) for value in arguments], text=True)


def identity(path):
    return {'size': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('elf', 'dol', 'reborn-root', 'wad-tool', 'xtask', 'key-file', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[1]
    elf = args.output / 'hbc-rs-debug.elf'
    dol = args.output / 'hbc-rs-debug.dol'
    channel = args.output / 'hbc-rs-debug-channel.dol'
    wad = args.output / 'hbc-rs-debug-OHBC.wad'
    with tempfile.TemporaryDirectory(prefix='hbc-wad-', dir='/Volumes/SSD/WiiDev/tmp') as temporary:
        stage = Path(temporary)
        converted = stage / 'from-elf.dol'
        run([args.xtask.resolve(), 'elf-to-dol', args.elf.resolve(), converted])
        if converted.read_bytes() != args.dol.read_bytes():
            raise RuntimeError('The selected ELF does not produce the selected DOL')
        sections = run(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-readelf', '-S', args.elf.resolve()])
        if '.debug_info' not in sections or '.debug_line' not in sections:
            raise RuntimeError('ELF lacks source debug information')
        symbols = run(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-nm', '-C', args.elf.resolve()])
        if 'libdb_sys::wii_network::WiiNetworkTransport' not in symbols:
            raise RuntimeError('ELF lacks the network debugger transport')
        shutil.copyfile(args.elf, elf)
        shutil.copyfile(args.dol, dol)
        run([args.wad_tool.resolve(), 'channel-dol', dol.resolve(), channel.resolve()])
        title = args.reborn_root / 'channel/title'
        for name in ('00000000', 'footer'):
            shutil.copyfile(title / name, stage / name)
        shutil.copyfile(channel, stage / '00000001')
        print(run([args.wad_tool.resolve(), 'pack', (title / 'retail').resolve(), stage,
                   args.key_file.resolve(), wad.resolve(), '--legacy-fakesign']), end='')
        summary = run([args.wad_tool.resolve(), 'verify', wad.resolve(), args.key_file.resolve()])
        if 'Title 000100014F484243;' not in summary:
            raise RuntimeError('The template does not use the expected OHBC title ID')
        (args.output / 'verification.txt').write_text(summary)
    manifest = {
        'format': 1, 'title_id': '000100014f484243', 'channel': 'OHBC',
        'ios': 58, 'signature_policy': 'legacy-fakesign', 'certificate_root_trust': 'unchecked',
        'debug_profile': 'release with debug=2', 'features': ['debugger-network'],
        'feature_evidence': 'ELF source debug sections and WiiNetworkTransport symbol checked by packager',
        'hbc_revision': run(['git', '-C', root, 'rev-parse', 'HEAD']).strip(),
        'hbc_changes': run(['git', '-C', root, 'status', '--short']).splitlines(),
        'libogc_revision': run(['git', '-C', root / '../libogc-rs', 'rev-parse', 'HEAD']).strip(),
        'libogc_changes': run(['git', '-C', root / '../libogc-rs', 'status', '--short']).splitlines(),
        'files': {p.name: identity(p) for p in (elf, dol, channel, wad)},
        'return_stub': identity(root / 'assets/return_stub.bin'),
        'physical_wii_tested': False,
        'limitations': ['No physical return fix is claimed', 'No private-key signing or EC verification',
                        'No certificate root trust check', 'No automatic recovery qualification',
                        'Installation would use the existing OHBC title ID'],
    }
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Package and manifest written: {args.output}')


if __name__ == '__main__':
    main()
