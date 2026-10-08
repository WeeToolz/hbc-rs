import json, pathlib, socket, subprocess, time
from dolphin_capture_config import configure_capture_profile

root = pathlib.Path(__file__).resolve().parents[1]
user_dir = root / 'target/dolphin-startup-profile'
user_dir.mkdir(parents=True, exist_ok=True)
configure_capture_profile(user_dir)
port = 55264
log = open(root / 'target/hbc-startup-dolphin.log', 'w')
proc = subprocess.Popen(['/Applications/Dolphin.app/Contents/MacOS/Dolphin', '-b', '-u', str(user_dir), '-e', str(root / 'target/hbc-rs-startup.dol'), '-C', f'Dolphin.General.GDBPort={port}'], stdout=log, stderr=log)
s = None
buffer = bytearray()
def packet():
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        start = buffer.find(b'$')
        end = buffer.find(b'#', start + 1) if start >= 0 else -1
        if end >= 0 and len(buffer) >= end + 3:
            payload = bytes(buffer[start+1:end])
            del buffer[:end+3]
            s.sendall(b'+')
            return payload.decode()
        data = s.recv(65536)
        if not data:
            raise RuntimeError('Dolphin closed the debugger connection')
        buffer.extend(data)
    raise RuntimeError('no debugger response')
def command(value):
    raw = value.encode()
    s.sendall(b'$' + raw + b'#' + f'{sum(raw)&255:02x}'.encode())
    return packet()
def memory(address, count):
    value = command(f'm{address:x},{count:x}')
    while value.startswith(('T','S')) or not value:
        value = packet()
    if value.startswith('E') and len(value) == 3:
        raise RuntimeError('memory read failed: ' + value)
    return bytes.fromhex(value)

symbols = subprocess.check_output(
    ['/opt/homebrew/opt/llvm/bin/llvm-nm', '-nC', str(root/'target/powerpc-nintendo-none/release/hbc-rs')],
    text=True,
)
addresses = {}
for line in symbols.splitlines():
    for name in ('FRAMEBUFFER', 'GX_FRAMEBUFFER', 'WIDTH', 'HEIGHT'):
        symbol_name = name if name == 'GX_FRAMEBUFFER' else 'diagnostics::wii::' + name
        if symbol_name + ' ' in line or line.split()[-1] == symbol_name:
            addresses[name] = int(line.split()[0], 16)
if len(addresses) != 4:
    raise RuntimeError('Could not find diagnostic framebuffer symbols in the ELF')
try:
    for attempt in range(80):
        try:
            s = socket.create_connection(('127.0.0.1', port), timeout=.5)
            break
        except OSError:
            if proc.poll() is not None:
                raise RuntimeError('Dolphin exited: ' + str(proc.returncode))
            time.sleep(.1)
    if not s:
        raise RuntimeError('Dolphin debugger port did not open')
    s.settimeout(10)
    print('initial', command('?'), flush=True)
    s.sendall(b'$c#63')
    time.sleep(8)
    s.sendall(b'\x03')
    stop = packet()
    print('stop', stop, flush=True)
    pointer = int.from_bytes(memory(addresses['FRAMEBUFFER'], 4), 'big')
    gx_pointer = int.from_bytes(memory(addresses['GX_FRAMEBUFFER'], 4), 'big')
    width = int.from_bytes(memory(addresses['WIDTH'], 4), 'big')
    height = int.from_bytes(memory(addresses['HEIGHT'], 4), 'big')
    print('framebuffer', hex(pointer), width, height, flush=True)
    if pointer and 0 < width <= 1024 and 0 < height <= 1024:
        length = ((width+15)&~15) * height * 2
        content = bytearray()
        for offset in range(0, length, 4096):
            content.extend(memory(pointer+offset, min(4096,length-offset)))
        (root/'target/hbc-startup-framebuffer.xfb').write_bytes(content)
    if gx_pointer and 0 < width <= 1024 and 0 < height <= 1024:
        length = ((width+15)&~15) * height * 2
        content = bytearray()
        for offset in range(0, length, 4096):
            content.extend(memory(gx_pointer+offset, min(4096,length-offset)))
        (root/'target/hbc-gx-framebuffer.xfb').write_bytes(content)
    (root/'target/hbc-startup-capture.json').write_text(json.dumps({'stop':stop, 'framebuffer':pointer, 'gx_framebuffer':gx_pointer, 'width':width,'height':height},indent=2))
finally:
    if s:
        s.close()
    proc.terminate()
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
    log.close()
