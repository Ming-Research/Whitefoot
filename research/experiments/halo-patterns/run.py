#!/usr/bin/env python3
"""Compare authored pattern cases with Lua 5.1 using the existing native Halo host."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import tempfile

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compiler_cache import add_arguments as cache_arguments, flags as cache_flags

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent


def run(command, **kwargs):
    start = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=900, **kwargs)
    return result, time.monotonic() - start


def compare(expected, actual):
    # Deliberately fail on a changed value, missing result or changed order.
    return [(i, a, b) for i, (a, b) in enumerate(zip(expected, actual), 1) if a != b] + [
        (i + 1, expected[i] if i < len(expected) else '<missing>',
         actual[i] if i < len(actual) else '<missing>')
        for i in range(min(len(expected), len(actual)), max(len(expected), len(actual)))]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--compiler', required=True)
    parser.add_argument('--lua', required=True)
    parser.add_argument('--binary', type=Path, help='reuse an existing halo-e2e test executable')
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--budgets', default='7')
    parser.add_argument('--limit', type=int, help='small calibration sample before the full batch')
    cache_arguments(parser, 'halo-patterns', timing=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='halo-patterns-') as temporary:
        if args.binary is None:
            args.binary = Path(temporary)/'test'
            build, seconds = run([args.compiler, '--graph', str(HERE.parent/'halo-e2e/modules.wfg'),
                                  '--entry', 'test', '-o', str(args.binary)] + cache_flags(args))
            print(f'Native build exit {build.returncode}, {seconds:.3f}s', flush=True)
            if build.returncode:
                raise RuntimeError((build.stdout + build.stderr).decode('utf8', 'replace'))
        assert compare(['a', 'b'], ['a', 'b']) == []
        for bad in (['x', 'b'], ['a'], ['b', 'a'], ['a', 'b', 'c']):
            assert compare(['a', 'b'], bad)
        source = (HERE / 'cases.lua').read_bytes()
        if args.limit is not None:
            source = source.replace(b'local function pack', f'for i=#cases,{args.limit + 1},-1 do cases[i]=nil end\nlocal function pack'.encode(), 1)
        assert b'\0' not in source
        # Load the identical bytes under the embedding's chunk name; no Redis/network.
        wrapper = b'local f=assert(loadstring(io.read("*a"), "@user_script")); for _,s in ipairs(f()) do io.write(s,"\\n") end'
        reference, ref_seconds = run([args.lua, '-e', wrapper.decode()], input=source)
        if reference.returncode:
            raise RuntimeError(reference.stderr.decode())
        expected = reference.stdout.decode('ascii').splitlines()
        revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
        report = ['# Authored pattern differential cases', '', f'Local parent: `{revision}`.',
                  f'Cases SHA-256: `{hashlib.sha256(source).hexdigest()}`.',
                  f'Executable SHA-256: `{hashlib.sha256(args.binary.read_bytes()).hexdigest()}`.',
                  f'Compiler SHA-256: `{hashlib.sha256(Path(args.compiler).read_bytes()).hexdigest()}`.',
                  f'Reference: {len(expected)} cases, exit {reference.returncode}, {ref_seconds:.3f} seconds.', '']
        failures = 0
        for budget in map(int, args.budgets.split(',')):
            extra = [] if budget == 1000 else ['one'] if budget == 1 else ['seven', 'seven'] if budget == 7 else None
            if extra is None:
                raise ValueError('budgets are 1, 7 or 1000')
            result, seconds = run([str(args.binary.resolve())] + extra, input=b'\0' + source)
            if result.returncode:
                raise RuntimeError(f'native exit {result.returncode}: {result.stdout!r} {result.stderr!r}')
            reply = json.loads(result.stdout)
            if reply.get('type') != 'array' or any(x.get('type') != 'bulk' for x in reply['items']):
                raise RuntimeError(f'expected array of byte strings, got {reply}')
            actual = [x['bytes'] for x in reply['items']]
            mismatches = compare(expected, actual)
            failures += len(mismatches)
            report += [f'Budget {budget}: {len(expected)} reference / {len(actual)} Halo cases; '
                       f'{len(mismatches)} mismatches, native exit {result.returncode}, {seconds:.3f} seconds.', '']
            for i, a, b in mismatches:
                report += [f'- Case {i}: Lua `{a}`; Halo `{b}`.']
            print(f'budget={budget}: {len(expected)} cases, {len(mismatches)} mismatches, {seconds:.3f}s', flush=True)
            if mismatches:
                print('\n'.join(report[-min(len(mismatches), 12):]))
        if args.limit is None:
            contract_source = (HERE / 'depth.lua').read_bytes()
            contract_expected = ['optional-199|true|199', 'optional-200|false|pattern too complex',
                                 'greedy-199|true|0', 'greedy-200|false|pattern too complex',
                                 'minimal-199|true|0', 'minimal-200|false|pattern too complex',
                                 'tail-1000|true|1000']
            for budget in map(int, args.budgets.split(',')):
                extra = [] if budget == 1000 else ['one'] if budget == 1 else ['seven', 'seven']
                result, seconds = run([str(args.binary.resolve())] + extra, input=b'\0' + contract_source)
                if result.returncode:
                    raise RuntimeError(f'depth native exit {result.returncode}: {result.stdout!r} {result.stderr!r}')
                reply = json.loads(result.stdout)
                if reply.get('type') != 'array' or any(x.get('type') != 'bulk' for x in reply['items']):
                    raise RuntimeError(f'depth result schema: {reply}')
                actual = [x['bytes'] for x in reply['items']]
                mismatches = compare(contract_expected, actual)
                failures += len(mismatches)
                report += [f'Depth contract budget {budget}: 7 cases, {len(mismatches)} mismatches, '
                           f'native exit {result.returncode}, {seconds:.3f} seconds.', '']
                for i, a, b in mismatches:
                    report += [f'- Depth case {i}: contract `{a}`; Halo `{b}`.']
                print(f'depth budget={budget}: 7 cases, {len(mismatches)} mismatches', flush=True)
        args.report.write_text('\n'.join(report) + '\n')
        return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
