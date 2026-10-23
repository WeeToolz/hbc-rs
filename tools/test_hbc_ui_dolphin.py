#!/usr/bin/env python3
"""Prepare and verify HBC UI actions through Dolphin's emulated Wii Remote."""
import configparser
import os
import struct
import time
import unittest
from pathlib import Path


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
    events = parse_events(log_text)
    pointer = [event for event in events if event['name'] == 'pointer_press']
    targets = {(event.get('target'), event.get('index')) for event in pointer
               if event.get('valid') == '1'}
    required_targets = {('option', '4'), ('option', '5'), ('option', '6'),
                        ('option', '8'), ('network_option', '6')}
    if not required_targets.issubset(targets):
        raise RuntimeError(f'Wii Remote did not hit required UI targets: {targets}')
    edge_clipped = any(
        (event.get('valid') == '0') or
        (event.get('valid') == '1' and int(event.get('x', '-1')) >= 630
         and int(event.get('y', '-1')) >= 470)
        for event in pointer)
    if not edge_clipped:
        raise RuntimeError('Wii Remote edge input did not reach or clip at the screen boundary')
    distinct_positions = {(event.get('x'), event.get('y')) for event in pointer
                          if event.get('valid') == '1'}
    if len(distinct_positions) < 3:
        raise RuntimeError('Wii Remote test did not prove three pointer positions')

    bubble_targets = [event for event in events if event['name'] == 'bubble_target']
    if len(bubble_targets) != 1:
        raise RuntimeError(f'Expected one production bubble target, found {len(bubble_targets)}')
    x, y = int(bubble_targets[0]['x']), int(bubble_targets[0]['y'])
    if not (0 <= x < 640 and 0 <= y < 480):
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
        name = {'1': 'START'}.get(name, name)
        self.send(f'PRESS {name}')
        self.send(f'RELEASE {name}')

    def axes(self, x, y):
        """Set both Dolphin Pipe axes with one normalized command."""
        self.send(f'SET MAIN {x:.4f} {y:.4f}')

    def click(self):
        self.button('A')


def wait_for_events(log_path, predicate, timeout, description, after_count=0):
    deadline = time.monotonic() + timeout
    last_events = []
    while time.monotonic() < deadline:
        try:
            events = parse_events(log_path.read_text(errors='replace'))
        except FileNotFoundError:
            events = []
        last_events = events
        selected = [event for event in events[after_count:] if predicate(event)]
        if selected:
            return selected[-1], events
        time.sleep(0.2)
    raise TimeoutError(f'{description} was not observed; recent events={last_events[-8:]}')


def click_at(remote, log_path, x, y, target, index, timeout):
    before = len(parse_events(log_path.read_text(errors='replace')))
    remote.axes(x / 639, y / 479)
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

    initial_events = parse_events(log_text)
    bubble = next((event for event in initial_events if event['name'] == 'bubble_target'), None)
    if bubble is None:
        bubble, _ = wait_for_events(log_path, lambda event: event['name'] == 'bubble_target',
                                    args.startup_timeout, 'production bubble target')
    bubble_x, bubble_y = int(bubble['x']), int(bubble['y'])
    before_pops = sum(int(event.get('count', '0')) for event in parse_events(
        log_path.read_text(errors='replace')) if event['name'] == 'bubble_pop')
    remote.axes(bubble_x / 639, bubble_y / 479)
    pointer_before = len(initial_events)
    remote.click()
    pointer_event, _ = wait_for_events(
        log_path,
        lambda item: item['name'] == 'pointer_press' and item.get('valid') == '1',
        10, 'emulated Wii Remote click on bubble', after_count=pointer_before)
    if abs(int(pointer_event['x']) - bubble_x) > 12 or \
            abs(int(pointer_event['y']) - bubble_y) > 12:
        raise RuntimeError('Wii Remote pointer did not reach the production bubble target')
    event, events = wait_for_events(
        log_path,
        lambda item: item['name'] == 'bubble_pop' and int(item.get('count', '0')) > 0,
        args.startup_timeout,
        'real bubble pop after the emulated Wii Remote click')
    after_pops = sum(int(item.get('count', '0')) for item in events
                     if item['name'] == 'bubble_pop')
    if after_pops <= before_pops:
        raise RuntimeError('Bubble pop count did not increase after the Wii Remote input')
    remote.button('B')
    screenshot('ui-bubble-pop')

    # The Pipe clamps out-of-range axes to its maximum value. Check that the
    # Wii Remote reports the clipped bottom-right screen edge.
    edge_before = len(parse_events(log_path.read_text(errors='replace')))
    remote.send('SET MAIN 1.5 1.5')
    remote.click()
    edge_event, _ = wait_for_events(
        log_path,
        lambda item: item['name'] == 'pointer_press',
        5, 'clipped Wii Remote pointer sample', after_count=edge_before)
    if edge_event.get('valid') != '1' or (
            int(edge_event['x']) < 630 or int(edge_event['y']) < 470):
        raise RuntimeError(f'Wii Remote did not reach the clipped screen edge: {edge_event}')
    remote.button('B')
    remote.axes(0.5, 0.5)

    # Date then Name ensures both settings transitions produce actual orders.
    remote.button('1')
    click_at(remote, log_path, 400, 265, 'option', 5, 10)
    click_at(remote, log_path, 180, 360, 'option', 6, 10)
    wait_for_events(log_path,
                    lambda item: item['name'] == 'sort_changed' and item.get('key') == 'date'
                    and _titles(item) == DATE_ORDER,
                    10, 'Date sort and visible order')
    screenshot('ui-date-sort')
    remote.button('1')
    click_at(remote, log_path, 180, 265, 'option', 4, 10)
    click_at(remote, log_path, 180, 360, 'option', 6, 10)
    wait_for_events(log_path,
                    lambda item: item['name'] == 'sort_changed' and item.get('key') == 'name'
                    and _titles(item) == NAME_ORDER,
                    10, 'Name sort and visible order')
    screenshot('ui-name-sort')

    remote.button('1')
    click_at(remote, log_path, 300, 330, 'option', 8, 10)
    screenshot('ui-network-settings')
    click_at(remote, log_path, 120, 420, 'network_option', 6, 10)
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
        'Pipe START maps to HBC Options because Dolphin Pipe has no Wii Remote 1/2 controls'
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
        'Buttons/1 = `Pipe/0/hbc-ui-remote:Button START`\n'
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
