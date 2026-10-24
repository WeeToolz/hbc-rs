#!/usr/bin/env python3
"""Prepare and verify HBC UI actions through Dolphin's emulated Wii Remote."""
import configparser
import os
import struct
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch


APP_FIXTURES = (
    ('cedar', 'Cedar', '20250304120000'),
    ('amber', 'Amber', '20240102120000'),
    ('birch', 'Birch', '20241203120000'),
)
NAME_ORDER = ('Amber', 'Birch', 'Cedar')
DATE_ORDER = ('Cedar', 'Birch', 'Amber')
EVENT_PREFIX = 'HBC_UI_EVENT v=1 '


def make_nonlaunching_dol():
    """Return a minimal DOL with one self-branching text instruction."""
    dol = bytearray(0x104)
    struct.pack_into('>I', dol, 0x00, 0x100)
    struct.pack_into('>I', dol, 0x48, 0x80010000)
    struct.pack_into('>I', dol, 0x90, 4)
    struct.pack_into('>I', dol, 0xE0, 0x80010000)
    dol[0x100:0x104] = bytes.fromhex('48000000')
    return bytes(dol)


def prepare_apps(profile):
    """Create three non-launching SD entries with separate name and date order."""
    root = profile / 'Load/WiiSDSync/apps'
    sample_dol = make_nonlaunching_dol()
    for directory, title, release_date in APP_FIXTURES:
        app = root / directory
        app.mkdir(parents=True, exist_ok=True)
        (app / 'boot.dol').write_bytes(sample_dol)
        (app / 'meta.xml').write_text(
            '<app><name>{}</name><release_date>{}</release_date></app>\n'.format(
                title, release_date), encoding='utf-8')


def parse_events(log_text):
    """Read stable production markers; reject duplicate or malformed fields."""
    events = []
    for line in log_text.splitlines():
        marker = line.find(EVENT_PREFIX)
        if marker < 0:
            continue
        fields = {}
        for item in line[marker + len(EVENT_PREFIX):].split():
            if '=' not in item:
                raise ValueError(f'Malformed HBC UI event token: {item!r}')
            key, value = item.split('=', 1)
            if key in fields:
                raise ValueError(f'Duplicate HBC UI event key: {key}')
            fields[key] = value
        if 'name' not in fields:
            raise ValueError('HBC UI event has no name')
        events.append(fields)
    return events


class ParseEventsTests(unittest.TestCase):
    def test_reads_markers_after_dolphin_log_prefix(self):
        log = (
            '45:27:561 Core/HW/EXI/EXI_DeviceIPL.cpp:306 N[OSREPORT]: '
            'HBC_UI_EVENT v=1 name=pointer_press valid=1 x=12 y=34 '
            'target=option index=4\n'
        )
        self.assertEqual(parse_events(log), [{
            'name': 'pointer_press', 'valid': '1', 'x': '12', 'y': '34',
            'target': 'option', 'index': '4',
        }])

    def test_ignores_lines_without_markers(self):
        self.assertEqual(parse_events('HBC: startup complete\n'), [])


def _titles(event):
    encoded = event.get('titles_hex')
    if encoded is None:
        raise RuntimeError('Sort event has no visible title order')
    try:
        return tuple(bytes.fromhex(item).decode('utf-8') for item in encoded.split(','))
    except (ValueError, UnicodeError) as error:
        raise RuntimeError('Sort event has invalid UTF-8 title data') from error


def validate_events(log_text):
    """Assert observable pointer, bubble, sorting, and scan results."""
    if 'ACL queue size reached' in log_text or 'failure: Some(QueueFull)' in log_text:
        raise RuntimeError('Bluetooth input queue overflowed during UI acceptance')
    events = parse_events(log_text)
    viewport = next((event for event in events if event['name'] == 'viewport'), None)
    if viewport is None:
        raise RuntimeError('Guest viewport dimensions were not reported')
    width = int(viewport['width'])
    pointer = [event for event in events if event['name'] == 'pointer_press']
    targets = {(event.get('target'), event.get('index')) for event in pointer
               if event.get('valid') == '1'}
    required_targets = {('option', '4'), ('option', '5'), ('option', '6'),
                        ('option', '8'), ('network_option', '6')}
    if not required_targets.issubset(targets):
        raise RuntimeError(f'Wii Remote did not hit required UI targets: {targets}')
    edge_clipped = any(
        (event.get('valid') == '1' and int(event.get('x', '-1')) >= width - 10
         and int(event.get('y', '-1')) >= 470)
        for event in pointer)
    if not edge_clipped:
        raise RuntimeError('Wii Remote edge input did not reach or clip at the screen boundary')
    distinct_positions = {(event.get('x'), event.get('y')) for event in pointer
                          if event.get('valid') == '1'}
    if len(distinct_positions) < 3:
        raise RuntimeError('Wii Remote test did not prove three pointer positions')

    bubble_targets = [event for event in events if event['name'] == 'bubble_target']
    if not bubble_targets:
        raise RuntimeError('No production bubble target was observed')
    x, y = int(bubble_targets[-1]['x']), int(bubble_targets[-1]['y'])
    if not (0 <= x < width and 0 <= y < 480):
        raise RuntimeError(f'Bubble target is outside the screen: {x},{y}')
    bubble_pops = [event for event in events if event['name'] == 'bubble_pop']
    if not any(int(event.get('count', '0')) > 0 for event in bubble_pops):
        raise RuntimeError('The emulated Wii Remote did not pop a bubble')

    sorts = [event for event in events if event['name'] == 'sort_changed']
    name_events = [event for event in sorts if event.get('key') == 'name']
    date_events = [event for event in sorts if event.get('key') == 'date']
    if not any(_titles(event) == NAME_ORDER for event in name_events):
        raise RuntimeError('Name sort did not show the expected app order')
    if not any(_titles(event) == DATE_ORDER for event in date_events):
        raise RuntimeError('Date sort did not show the expected app order')

    scans = [event for event in events if event['name'] == 'network_scan_complete']
    failed_scans = [event for event in scans if event.get('status') == 'error']
    if not any(event.get('restart') == 'ok' and event.get('ui') == 'notice'
               for event in failed_scans):
        raise RuntimeError('Network scan did not finish with a visible supported-driver error')
    recoveries = [event for event in events
                  if event['name'] == 'network_scan_receiver_recovery']
    if not recoveries or recoveries[-1].get('state') != 'Listening':
        raise RuntimeError('Wiiload receiver did not recover to Listening after the scan')
    return {
        'pointer_targets': sorted([list(target) for target in targets]),
        'pointer_positions': [
            {'x': int(event['x']), 'y': int(event['y']),
             'target': event.get('target'), 'index': event.get('index')}
            for event in pointer if event.get('valid') == '1'
        ],
        'pointer_edge': max(
            (event for event in pointer if event.get('valid') == '1'),
            key=lambda event: int(event['x']) + int(event['y'])),
        'bubble_target': {'x': x, 'y': y},
        'bubble_pop_counts': [int(event['count']) for event in bubble_pops],
        'name_order': list(NAME_ORDER),
        'date_order': list(DATE_ORDER),
        'network_scan': failed_scans[-1],
        'receiver_recovery': recoveries[-1],
    }


class RemotePipe:
    """Send controls to a Dolphin Pipe-bound emulated Wii Remote."""

    def __init__(self, fd):
        self.fd = fd

    def send(self, message):
        packet = (message + '\n').encode('ascii')
        if __import__('os').write(self.fd, packet) != len(packet):
            raise RuntimeError('Incomplete Dolphin Wii Remote pipe write')
        time.sleep(0.2)

    def button(self, name):
        name = {'1': 'X', '2': 'Y'}.get(name, name)
        self.send(f'PRESS {name}')
        self.send(f'RELEASE {name}')

    def axes(self, x, y):
        """Set both Dolphin Pipe axes with one normalized command."""
        self.send(f'SET MAIN {x:.4f} {y:.4f}')

    def click(self):
        self.button('A')


def pointer_axis_delta(matrix, dx, dy):
    """Invert the measured two-axis pointer response."""
    a, b, c, d = matrix
    determinant = a * d - b * c
    if abs(determinant) < 1:
        raise RuntimeError(f'Dolphin pointer calibration is singular: {matrix}')
    return ((d * dx - b * dy) / determinant,
            (a * dy - c * dx) / determinant)


class PointerDriver:
    """Position the real guest pointer using observed coordinates as feedback."""

    def __init__(self, remote, log_path, width):
        self.remote = remote
        self.log_path = log_path
        self.width = width
        self.axes = (0.5, 0.5)
        center = self.measure(0.5, 0.5)
        horizontal = self.measure(0.55, 0.5)
        vertical = self.measure(0.5, 0.55)
        self.matrix = ((horizontal[0] - center[0]) / 0.05,
                       (vertical[0] - center[0]) / 0.05,
                       (horizontal[1] - center[1]) / 0.05,
                       (vertical[1] - center[1]) / 0.05)
        pointer_axis_delta(self.matrix, 1, 1)

    def position(self):
        events = parse_events(self.log_path.read_text(errors='replace'))
        positions = [event for event in events if event['name'] == 'pointer_position']
        if not positions or positions[-1].get('valid') != '1':
            raise RuntimeError(f'Guest pointer is not visible at Pipe axes {self.axes}')
        return int(positions[-1]['x']), int(positions[-1]['y'])

    def measure(self, x, y):
        self.remote.axes(x, y)
        self.axes = (x, y)
        time.sleep(0.25)
        return self.position()

    def move_to(self, x, y):
        for _ in range(12):
            actual_x, actual_y = self.position()
            dx, dy = x - actual_x, y - actual_y
            if abs(dx) <= 4 and abs(dy) <= 4:
                return actual_x, actual_y
            ax, ay = pointer_axis_delta(self.matrix, dx, dy)
            step = max(1, abs(ax) / 0.08, abs(ay) / 0.08)
            self.measure(max(0, min(1, self.axes[0] + ax / step)),
                         max(0, min(1, self.axes[1] + ay / step)))
        raise RuntimeError(f'Guest pointer did not reach {x},{y}; actual={self.position()}')


def wait_for_events(log_path, predicate, timeout, description, after_count=0):
    deadline = time.monotonic() + timeout
    last_events = []
    while time.monotonic() < deadline:
        try:
            log_text = log_path.read_text(errors='replace')
        except FileNotFoundError:
            log_text = ''
        if 'HBC: Bluetooth startup: BLUETOOTH FAILED' in log_text:
            raise RuntimeError(
                f'{description}: guest Bluetooth failed; inspect IOS_WIIMOTE '
                'and HBC Bluetooth diagnostics in dolphin.log')
        events = parse_events(log_text)
        last_events = events
        selected = [event for event in events[after_count:] if predicate(event)]
        if selected:
            return selected[-1], events
        time.sleep(0.2)
    raise TimeoutError(f'{description} was not observed; recent events={last_events[-8:]}')


class RemotePipeTests(unittest.TestCase):
    def test_acceptance_rejects_real_bluetooth_queue_overflow(self):
        with self.assertRaisesRegex(RuntimeError, 'Bluetooth input queue overflowed'):
            validate_events('E[IOS_WIIMOTE]: ACL queue size reached 100 - current packet will be dropped!')

    def test_pointer_calibration_inverts_both_measured_axes(self):
        x, y = pointer_axis_delta((800, 20, 10, 600), 82, 61)
        self.assertAlmostEqual(x, 0.1)
        self.assertAlmostEqual(y, 0.1)

    def test_pointer_calibration_rejects_unresponsive_axes(self):
        with self.assertRaisesRegex(RuntimeError, 'singular'):
            pointer_axis_delta((800, 0, 400, 0), 82, 61)

    def test_wii_buttons_use_independent_pipe_controls(self):
        reader, writer = os.pipe()
        try:
            with patch.object(time, 'sleep'):
                remote = RemotePipe(writer)
                remote.button('1')
                remote.button('2')
            self.assertEqual(os.read(reader, 256),
                             b'PRESS X\nRELEASE X\nPRESS Y\nRELEASE Y\n')
        finally:
            os.close(writer)
            os.close(reader)

    def test_guest_bluetooth_failure_is_reported_before_pointer_timeout(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as directory:
            log_path = Path(directory) / 'dolphin.log'
            log_path.write_text(
                'N[OSREPORT]: HBC: Bluetooth startup: BLUETOOTH FAILED\n')
            with self.assertRaisesRegex(RuntimeError, 'guest Bluetooth failed'):
                wait_for_events(log_path, lambda event: False, 1, 'pointer click')


def click_at(remote, pointer, log_path, x, y, target, index, timeout):
    x += (pointer.width - 640) // 2
    before = len(parse_events(log_path.read_text(errors='replace')))
    pointer.move_to(x, y)
    remote.click()
    event, _ = wait_for_events(
        log_path,
        lambda item: item['name'] == 'pointer_press' and
        item.get('target') == target and item.get('index') == str(index),
        timeout,
        f'Wii Remote pointer click on {target}[{index}] at {x},{y}',
        after_count=before)
    if event.get('valid') != '1':
        raise RuntimeError('Wii Remote click produced an invalid screen position')
    actual_x, actual_y = int(event['x']), int(event['y'])
    if abs(actual_x - x) > 12 or abs(actual_y - y) > 12:
        raise RuntimeError(f'Wii Remote pointer reached {actual_x},{actual_y}; expected {x},{y}')
    presses = [item for item in parse_events(log_path.read_text(errors='replace'))[before:]
               if item['name'] == 'pointer_press']
    if len(presses) != 1:
        raise RuntimeError(f'One Pipe A press produced {len(presses)} guest pointer presses')
    return event


def run_acceptance(args, result, profile, remote_fd, process, screenshot):
    """Drive UI with a Dolphin Pipe Wii Remote and verify production results."""
    remote = RemotePipe(remote_fd)
    log_path = profile / 'Logs/dolphin.log'
    deadline = time.monotonic() + args.startup_timeout
    while time.monotonic() < deadline:
        log_text = log_path.read_text(errors='replace') if log_path.is_file() else ''
        if 'HBC: storage scan applied: volume=Some(Sd), apps=3' in log_text:
            break
        if process.poll() is not None:
            raise RuntimeError('Dolphin stopped before the three-app SD scan completed')
        time.sleep(0.2)
    else:
        raise TimeoutError('The three-app SD catalog did not finish scanning')

    viewport, _ = wait_for_events(log_path, lambda event: event['name'] == 'viewport',
                                  args.startup_timeout, 'guest viewport')
    width = int(viewport['width'])
    result['viewport'] = {'width': width, 'height': int(viewport['height'])}
    remote.axes(0.5, 0.5)
    wait_for_events(log_path, lambda event: event['name'] == 'pointer_ready',
                    args.startup_timeout, 'guest Wii Remote pointer readiness')
    pointer = PointerDriver(remote, log_path, width)
    result['pointer_calibration'] = {'matrix': pointer.matrix}
    # Production bubbles pop on pointer contact. Use current bounds and wait
    # ahead of the moving bubble instead of clicking its stale startup position.
    pop_before = len(parse_events(log_path.read_text(errors='replace')))
    for _ in range(4):
        events = parse_events(log_path.read_text(errors='replace'))
        if any(event['name'] == 'bubble_pop' for event in events[pop_before:]):
            break
        targets = [event for event in events if event['name'] == 'bubble_target']
        if not targets:
            raise RuntimeError('No current production bubble bounds are available')
        bubble = targets[-1]
        pointer.move_to(int(bubble['x']), max(20, int(bubble['y']) - 50))
        time.sleep(1)
    wait_for_events(log_path,
                    lambda event: event['name'] == 'bubble_pop' and int(event.get('count', '0')) > 0,
                    5, 'real bubble pop after Wii Remote pointer contact', after_count=pop_before)
    screenshot('ui-bubble-pop')

    # Verify an actual guest screen-edge click. Pipe axes describe a camera
    # pose, so a clamped axis maximum is not a screen-pixel coordinate.
    pointer.move_to(width - 3, 477)
    edge_before = len(parse_events(log_path.read_text(errors='replace')))
    remote.click()
    edge_event, _ = wait_for_events(
        log_path,
        lambda item: item['name'] == 'pointer_press',
        5, 'clipped Wii Remote pointer sample', after_count=edge_before)
    if edge_event.get('valid') != '1' or (
            int(edge_event['x']) < width - 10 or int(edge_event['y']) < 470):
        raise RuntimeError(f'Wii Remote did not reach the clipped screen edge: {edge_event}')
    remote.button('B')
    pointer.move_to(320, 240)

    # Date then Name ensures both settings transitions produce actual orders.
    remote.button('1')
    click_at(remote, pointer, log_path, 400, 265, 'option', 5, 10)
    click_at(remote, pointer, log_path, 180, 360, 'option', 6, 10)
    wait_for_events(log_path,
                    lambda item: item['name'] == 'sort_changed' and item.get('key') == 'date'
                    and _titles(item) == DATE_ORDER,
                    10, 'Date sort and visible order')
    screenshot('ui-date-sort')
    remote.button('1')
    click_at(remote, pointer, log_path, 180, 265, 'option', 4, 10)
    click_at(remote, pointer, log_path, 180, 360, 'option', 6, 10)
    wait_for_events(log_path,
                    lambda item: item['name'] == 'sort_changed' and item.get('key') == 'name'
                    and _titles(item) == NAME_ORDER,
                    10, 'Name sort and visible order')
    screenshot('ui-name-sort')

    remote.button('1')
    click_at(remote, pointer, log_path, 300, 330, 'option', 8, 10)
    screenshot('ui-network-settings')
    click_at(remote, pointer, log_path, 120, 420, 'network_option', 6, 10)
    wait_for_events(log_path,
                    lambda item: item['name'] == 'network_scan_complete' and
                    item.get('status') == 'error' and item.get('restart') == 'ok' and
                    item.get('ui') == 'notice',
                    args.startup_timeout, 'completed supported network-driver error')
    wait_for_events(log_path,
                    lambda item: item['name'] == 'network_scan_receiver_recovery' and
                    item.get('state') == 'Listening',
                    args.startup_timeout, 'Wiiload receiver recovery to Listening')
    screenshot('ui-network-scan-error')

    log_text = log_path.read_text(errors='replace')
    result['ui_acceptance'] = validate_events(log_text)
    result['ui_acceptance']['input_path'] = (
        'Dolphin emulated Wii Remote pointer via Pipe/0/hbc-ui-remote; '
        'Pipe X/Y map to Wii Remote 1/2'
    )
    result['ui_acceptance']['fixture'] = {
        'apps': [{'directory': folder, 'title': title, 'release_date': date}
                 for folder, title, date in APP_FIXTURES],
        'name_order': list(NAME_ORDER), 'date_order': list(DATE_ORDER),
    }


def configure_profile(profile, config_path):
    """Enable one emulated Remote and bind it to a private Dolphin Pipe."""
    settings = configparser.ConfigParser(interpolation=None)
    settings.optionxform = str
    settings.read(config_path)
    settings['Core']['Wiimote1'] = '1'
    settings['Core']['WiiSDCard'] = 'True'
    settings['Core']['WiiSDCardAllowWrites'] = 'True'
    settings['Core']['WiiSDCardEnableFolderSync'] = 'True'
    with config_path.open('w') as output:
        settings.write(output)

    (profile / 'Config/WiimoteNew.ini').write_text(
        '[Wiimote1]\n'
        'Device = Pipe/0/hbc-ui-remote\n'
        'Buttons/A = `Pipe/0/hbc-ui-remote:Button A`\n'
        'Buttons/B = `Pipe/0/hbc-ui-remote:Button B`\n'
        'Buttons/1 = `Pipe/0/hbc-ui-remote:Button X`\n'
        'Buttons/2 = `Pipe/0/hbc-ui-remote:Button Y`\n'
        'D-Pad/Up = `Pipe/0/hbc-ui-remote:Button D_UP`\n'
        'D-Pad/Down = `Pipe/0/hbc-ui-remote:Button D_DOWN`\n'
        'D-Pad/Left = `Pipe/0/hbc-ui-remote:Button D_LEFT`\n'
        'D-Pad/Right = `Pipe/0/hbc-ui-remote:Button D_RIGHT`\n'
        'IR/Up = `Pipe/0/hbc-ui-remote:Axis MAIN Y -`\n'
        'IR/Down = `Pipe/0/hbc-ui-remote:Axis MAIN Y +`\n'
        'IR/Left = `Pipe/0/hbc-ui-remote:Axis MAIN X -`\n'
        'IR/Right = `Pipe/0/hbc-ui-remote:Axis MAIN X +`\n'
        'IMUIR/Enabled = True\n'
        'Options/Always Connected = True\n', encoding='utf-8')
    pipes = profile / 'Pipes'
    pipes.mkdir(exist_ok=True)
    pipe = pipes / 'hbc-ui-remote'
    os.mkfifo(pipe, 0o600)
    return os.open(pipe, os.O_RDWR | os.O_NONBLOCK)


def make_sd_fixture(profile):
    prepare_apps(profile)
    return {
        'name_order': list(NAME_ORDER),
        'date_order': list(DATE_ORDER),
        'apps': [{'directory': folder, 'title': title, 'release_date': date}
                 for folder, title, date in APP_FIXTURES],
    }


def set_initial_sort(profile):
    """Keep the test's first real sort transition deterministic."""
    path = profile / 'Wii/title/00010001/4f484243/data/settings.xml'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<hbc><device>-1</device><sort_order>0</sort_order>'
        '<browse_mode>0</browse_mode></hbc>', encoding='utf-8')
