"""Shared Dolphin debugger transport for emulator acceptance tests."""
import configparser
import socket
import time


def installed_title(profile, title):
    """Validate a title already present in a disposable NAND copy. No install."""
    import hashlib
    import struct
    if len(title) != 16 or any(c not in '0123456789abcdef' for c in title):
        raise ValueError('Use a 16-digit lowercase hexadecimal title ID')
    content = profile / 'Wii/title' / title[:8] / title[8:] / 'content'
    tmd = (content / 'title.tmd').read_bytes()
    payload = {0x10000: 0x240, 0x10001: 0x140, 0x10002: 0x80}.get(int.from_bytes(tmd[:4], 'big'))
    if payload is None or len(tmd) < payload + 0xa4 or tmd[payload + 0x4c:payload + 0x54].hex() != title:
        raise ValueError('Invalid installed-title TMD')
    count, boot = struct.unpack_from('>HH', tmd, payload + 0x9e)
    if not count or len(tmd) != payload + 0xa4 + count * 36:
        raise ValueError('Invalid installed-title content table')
    if not (profile / 'Wii/ticket' / title[:8] / (title[8:] + '.tik')).is_file():
        raise ValueError('Installed title has no ticket')
    for index in range(count):
        offset = payload + 0xa4 + index * 36
        cid, number, kind, size = struct.unpack_from('>IHHQ', tmd, offset)
        if number != boot:
            continue
        dol = (content / f'{cid:08x}.app').read_bytes()
        if len(dol) != size or hashlib.sha1(dol).digest() != tmd[offset + 16:offset + 36]:
            raise ValueError('Installed boot content does not match the TMD')
        if len(dol) < 256:
            raise ValueError('Installed boot DOL is truncated')
        entry = int.from_bytes(dol[0xe0:0xe4], 'big')
        for section in range(7):
            start = int.from_bytes(dol[section * 4:section * 4 + 4], 'big')
            address = int.from_bytes(dol[0x48 + section * 4:0x4c + section * 4], 'big')
            length = int.from_bytes(dol[0x90 + section * 4:0x94 + section * 4], 'big')
            if address <= entry and entry + 32 <= address + length and start + length <= len(dol):
                first = start + entry - address
                return {'title': title, 'entry': entry, 'entry_bytes': dol[first:first + 32].hex(),
                        'boot_sha256': hashlib.sha256(dol).hexdigest()}
        raise ValueError('Installed DOL entry is outside a valid text section')
    raise ValueError('Installed-title boot content is absent')


def write_dolphin_config(directory, contents):
    """Write an isolated test profile with analytics disabled before startup."""
    config = configparser.ConfigParser(interpolation=None)
    config.optionxform = str
    config.read_string(contents)
    config['Analytics'] = {'PermissionAsked': 'True', 'Enabled': 'False'}
    with (directory / 'Dolphin.ini').open('w') as output:
        config.write(output)


class Debugger:
    def __init__(self, connection, names):
        self.socket = connection
        self.names = names
        self.buffer = bytearray()

    def packet(self, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            start = self.buffer.find(b'$')
            end = self.buffer.find(b'#', start + 1) if start >= 0 else -1
            if end >= 0 and len(self.buffer) >= end + 3:
                payload = bytes(self.buffer[start + 1:end])
                checksum = int(bytes(self.buffer[end + 1:end + 3]), 16)
                del self.buffer[:end + 3]
                if sum(payload) & 255 != checksum:
                    raise RuntimeError('Invalid emulator debugger checksum')
                self.socket.sendall(b'+')
                return payload.decode('ascii')
            try:
                data = self.socket.recv(65536)
            except socket.timeout:
                continue
            if not data:
                raise RuntimeError('Emulator debugger closed')
            self.buffer.extend(data)
        raise TimeoutError('Emulator debugger response timed out')

    def send(self, value):
        data = value.encode('ascii')
        self.socket.sendall(b'$' + data + b'#' + f'{sum(data) & 255:02x}'.encode('ascii'))

    def command(self, value):
        self.send(value)
        return self.packet()

    def write(self, address, data):
        reply = self.command(f'M{address:x},{len(data):x}:' + data.hex())
        if reply != 'OK':
            raise RuntimeError(f'Emulator memory write rejected: {reply}')

    def memory(self, address, length):
        if length > 512:
            return b''.join(self.memory(address + offset, min(512, length - offset)) for offset in range(0, length, 512))
        reply = self.command(f'm{address:x},{length:x}')
        if reply.startswith(('E', 'S', 'T')):
            raise RuntimeError(f'Emulator memory read failed: {reply}')
        data = bytes.fromhex(reply)
        if len(data) != length:
            raise RuntimeError('Truncated emulator memory read')
        return data

    def number(self, name):
        return int.from_bytes(self.memory(self.names[name], 4), 'big')

    def breakpoint(self, address, install=True):
        reply = self.command(f'{"Z" if install else "z"}0,{address:x},4')
        if reply != 'OK':
            raise RuntimeError(f'Emulator breakpoint rejected: {reply}')

    def stopped_at(self, address, seconds=10):
        reply = self.packet(seconds)
        if f'40:{address:08x};' not in reply:
            raise RuntimeError(f'Unexpected emulator stop: {reply}; expected {address:08x}')
        return reply


def verified_artifact(dol, elf, workspace):
    import hashlib
    import json
    import subprocess
    import tempfile
    from pathlib import Path
    names = {}
    for line in subprocess.check_output(['/opt/devkitpro/devkitPPC/bin/powerpc-eabi-nm', '-C', str(elf)], text=True).splitlines():
        fields = line.split(maxsplit=2)
        if len(fields) == 3:
            names[fields[2]] = int(fields[0], 16)
    digest = hashlib.sha256(dol.read_bytes()).hexdigest()
    manifest = json.loads(dol.with_suffix('.manifest.json').read_text())
    if manifest['dol_sha256'] != digest:
        raise RuntimeError('DOL does not match its manifest')
    with tempfile.TemporaryDirectory(prefix='verify-dolphin-elf-', dir=workspace / 'tmp') as temporary:
        converted = Path(temporary) / 'verified.dol'
        subprocess.run(['cargo', 'xtask', 'elf-to-dol', str(elf), str(converted)], cwd=workspace / 'Devtools/libogc-rs', check=True, capture_output=True)
        if converted.read_bytes() != dol.read_bytes():
            raise RuntimeError('ELF does not produce the selected DOL; use its matching archived ELF')
    return names, digest, manifest
