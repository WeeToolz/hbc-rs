#!/usr/bin/env python3
"""Validate or regenerate the linked HBC return stub. No device operations."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / 'RustPorts/hbc-rs/assets'
BASE = 0x80001800
LIMIT = 0x80002f00
TOOLCHAIN = Path('/opt/devkitpro/devkitPPC/bin')


def elf_image(path):
    """Read the actual ELF32 big-endian PPC sections and symbols."""
    data = path.read_bytes()
    if len(data) < 52 or data[:7] != b'\x7fELF\x01\x02\x01':
        raise ValueError('Expected an ELF32 big-endian image')
    header = struct.unpack_from('>16sHHIIIIIHHHHHH', data)
    if header[1:3] != (2, 20) or header[11] != 40 or not header[12]:
        raise ValueError('Expected a PowerPC executable with ELF32 section headers')
    sections = []
    for index in range(header[12]):
        section = struct.unpack_from('>10I', data, header[6] + index * 40)
        name, kind, flags, address, offset, size, link, info, align, entsize = section
        content = b'' if kind == 8 else data[offset:offset + size]
        if kind != 8 and len(content) != size:
            raise ValueError('Truncated ELF section')
        sections.append(dict(kind=kind, flags=flags, address=address, size=size,
                             link=link, entsize=entsize, data=content))
    symbols = {}
    for section in sections:
        if section['kind'] != 2:
            continue
        if section['entsize'] != 16 or section['size'] % 16:
            raise ValueError('Invalid symbol table')
        strings = sections[section['link']]['data']
        for offset in range(0, section['size'], 16):
            name, value, size, info, other, index = struct.unpack_from('>IIIBBH', section['data'], offset)
            end = strings.find(b'\0', name)
            if end < 0:
                raise ValueError('Invalid symbol name')
            symbols[strings[name:end].decode('ascii')] = value
    return header[4], sections, symbols


def absolute_pair(image, offset):
    high, low = struct.unpack_from('>II', image, offset)
    if high & 0xffff0000 != 0x3c600000 or low & 0xffff0000 != 0x60630000:
        raise ValueError(f'Unsupported address-load sequence at {offset:#x}')
    return ((high & 0xffff) << 16) | (low & 0xffff)


def validate(binary=ASSETS / 'return_stub.bin', elf=ASSETS / 'return_stub.elf'):
    image = binary.read_bytes()
    entry, sections, symbols = elf_image(elf)
    if not 0x180 <= len(image) <= LIMIT - BASE:
        raise ValueError('Stub image overlaps return-title configuration or is truncated')
    if image[:16] != bytes.fromhex('48000010535455424841585800000000'):
        raise ValueError('Unsupported stub entry/header')
    real = absolute_pair(image, 0x24) & 0x3fffffff
    if real != 0x1848:
        raise ValueError(f'Incorrect real-mode destination {real:#010x}; expected 0x00001848')
    required = ('_start', 'startup', 'real', 'virtual', '_main', '__sbss_start', '__bss_end')
    if any(name not in symbols for name in required):
        raise ValueError('Missing required stub symbols')
    if entry != BASE or symbols['_start'] != BASE or symbols['startup'] != BASE + 16:
        raise ValueError('Stub entry is not linked at 0x80001800')
    if symbols['real'] != BASE + 0x48 or absolute_pair(image, 0x16c) != symbols['virtual']:
        raise ValueError('Stub mode-transition destinations do not match the ELF')
    if image[0x44:0x48] != bytes.fromhex('4c000064') or image[0x184:0x188] != bytes.fromhex('4c000064'):
        raise ValueError('Missing mode-transition rfi instructions')
    rebuilt = bytearray(len(image))
    for section in sections:
        if not section['flags'] & 2 or not section['size']:
            continue
        start, size = section['address'], section['size']
        if section['kind'] == 8:
            if start < 0x80005000:
                raise ValueError('Stub BSS overlaps Wii low memory')
            continue
        if not BASE <= start or start + size > BASE + len(image):
            raise ValueError('Allocated ELF section is outside the stored stub')
        rebuilt[start - BASE:start - BASE + size] = section['data']
    if bytes(rebuilt) != image:
        raise ValueError('Stub binary does not match its ELF sections')
    if symbols['__sbss_start'] < 0x80005000 or symbols['__bss_end'] <= symbols['__sbss_start']:
        raise ValueError('Invalid BSS bounds')
    for name in ('real', 'virtual', '_main'):
        if not any(s['flags'] & 4 and s['address'] <= symbols[name] < s['address'] + s['size'] for s in sections):
            raise ValueError(f'{name} is outside executable code')
    return {'sha256': hashlib.sha256(image).hexdigest(), 'size': len(image),
            'elf_sha256': hashlib.sha256(elf.read_bytes()).hexdigest(),
            'symbols': {name: symbols[name] for name in required}, 'real_destination': real}


def bundled():
    identity = validate()
    provenance = json.loads((ASSETS / 'return_stub.manifest.json').read_text())
    if any(provenance[key] != identity[key] for key in identity):
        raise ValueError('Return stub does not match its build manifest')
    return identity


def regenerate():
    source = ROOT / 'C-sources/hbc-reborn'
    app = source / 'channel/channelapp'
    app.joinpath('build').mkdir(exist_ok=True)
    command = ['make', '-B', '-C', str(app), 'stub.bin']
    subprocess.run(command, check=True)
    identity = validate(app / 'stub.bin', app / 'stub.elf')
    files = sorted((app / 'stub').glob('*')) + [app / 'Makefile', app / 'config.h', app / 'title_id.h']
    identity.update(source_revision=subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip(),
                    source_changes=subprocess.check_output(['git', '-C', str(source), 'status', '--short'], text=True),
                    source_sha256={str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()},
                    build_command=command,
                    compiler=subprocess.check_output([str(TOOLCHAIN / 'powerpc-eabi-gcc'), '--version'], text=True).splitlines()[0],
                    linker=subprocess.check_output([str(TOOLCHAIN / 'powerpc-eabi-ld'), '--version'], text=True).splitlines()[0])
    # Publish only after the complete generated image passes validation.
    for suffix in ('bin', 'elf'):
        (ASSETS / ('return_stub.' + suffix)).write_bytes((app / ('stub.' + suffix)).read_bytes())
    (ASSETS / 'return_stub.manifest.json').write_text(json.dumps(identity, indent=2) + '\n')
    return bundled()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--regenerate', action='store_true')
    args = parser.parse_args()
    print(json.dumps(regenerate() if args.regenerate else bundled(), indent=2))
