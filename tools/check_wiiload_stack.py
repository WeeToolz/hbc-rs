#!/usr/bin/env python3
"""Check the measured PowerPC compressed-upload call path and worker stack.

This is a lower-bound audit of direct calls, not a worst-case interrupt or
indirect-call analysis. Missing symbols or changed call edges require review.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT_FUNCTION = 'hbc_rs::platform::wiiload::worker'
TARGET_FUNCTION = 'miniz_oxide::inflate::core::init_tree'
MINIMUM_MARGIN = 16 * 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('elf', type=Path)
    parser.add_argument('--tool-prefix', default='/opt/devkitpro/devkitPPC/bin/powerpc-eabi-')
    args = parser.parse_args()
    assembly = subprocess.check_output([args.tool_prefix + 'objdump', '-Cd', str(args.elf)], text=True)
    symbols = subprocess.check_output([args.tool_prefix + 'nm', '-SC', str(args.elf)], text=True)
    functions = {}
    current = None
    for line in assembly.splitlines():
        match = re.match(r'([0-9a-f]+) <(.*)>:', line)
        if match:
            current = int(match[1], 16)
            functions[current] = {'name': match[2], 'frame': None, 'calls': set()}
        elif current is not None:
            frame = re.search(r'stwu\s+r1,-(\d+)\(r1\)', line)
            if frame:
                functions[current]['frame'] = int(frame[1])
            call = re.search(r'\bbl\s+([0-9a-f]+) ', line)
            if call:
                functions[current]['calls'].add(int(call[1], 16))
    def find(name):
        candidates = [address for address, function in functions.items() if function['name'] == name]
        if len(candidates) != 1:
            raise RuntimeError(f'Cannot identify one function {name}; inspect the binary')
        return candidates[0]

    root, target, leaf = find(ROOT_FUNCTION), find(TARGET_FUNCTION), find('memset')
    # Restrict the graph to functions that can reach init_tree. This permits
    # compiler inlining while still requiring a real emitted call path.
    ancestors = {target}
    while True:
        enlarged = ancestors | {address for address, function in functions.items()
                                if function['calls'] & ancestors}
        if enlarged == ancestors:
            break
        ancestors = enlarged

    def longest_path(address, visited):
        if address == target:
            return [target]
        if address in visited:
            return None
        paths = [longest_path(callee, visited | {address})
                 for callee in functions[address]['calls'] & ancestors]
        paths = [[address] + path for path in paths if path is not None]
        return max(paths, key=lambda path: sum(functions[item]['frame'] or 0 for item in path), default=None)

    path = longest_path(root, set())
    if path is None or leaf not in functions[target]['calls']:
        raise RuntimeError('Compressed-upload direct call path changed; inspect the binary')
    chain = [(address, functions[address]) for address in path + [leaf]]
    if any(function['frame'] is None for _, function in chain):
        raise RuntimeError('A compressed-upload frame cannot be measured; inspect the binary')
    stack_symbols = [line.split() for line in symbols.splitlines() if line.endswith(' hbc_rs::platform::wiiload::THREAD_STACK')]
    if len(stack_symbols) != 1:
        raise RuntimeError('Cannot identify exactly one worker stack symbol')
    stack_address, stack_size = [int(value, 16) for value in stack_symbols[0][:2]]
    if stack_address % 32 or stack_size % 32:
        raise RuntimeError('Worker stack address or size is not aligned to 32 bytes')
    measured = sum(function['frame'] for _, function in chain)
    margin = stack_size - measured
    result = {'stack_bytes': stack_size, 'alignment': 32,
              'direct_call_lower_bound_bytes': measured, 'remaining_bytes': margin,
              'required_margin_bytes': MINIMUM_MARGIN,
              'frames': [{'function': function['name'], 'bytes': function['frame']} for _, function in chain],
              'success': margin >= MINIMUM_MARGIN}
    print(json.dumps(result, indent=2))
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
