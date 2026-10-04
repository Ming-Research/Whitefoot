#!/usr/bin/env python3
"""Compile and run the slice 1 Lua corpus against the supplied local oracle."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CORPUS = ('numeric', 'base', 'strings', 'tables', 'math', 'random', 'errors')


def run(command, **kwargs):
    result = subprocess.run(list(map(str, command)), **kwargs)
    if result.returncode:
        raise RuntimeError(f'{command}: exit {result.returncode}; {getattr(result, "stderr", b"")!r}')
    return result


def compare(expected, actual):
    return expected == actual


def controls():
    assert compare(b'one\ntwo\n', b'one\ntwo\n')
    assert not compare(b'one\ntwo\n', b'One\ntwo\n')
    assert not compare(b'one\ntwo\n', b'one\n')
    assert not compare(b'one\ntwo\n', b'two\none\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compiler', type=Path, required=True)
    p.add_argument('--lua', type=Path, required=True)
    p.add_argument('--cache', type=Path)
    p.add_argument('--adapter', type=Path, help='reuse an already built adapter')
    p.add_argument('--sample', action='store_true', help='size the run with numeric.lua only')
    p.add_argument('--results', type=Path)
    a = p.parse_args()
    controls()
    names = ('numeric',) if a.sample else CORPUS
    timings = []
    mismatches = []
    rows = []
    with tempfile.TemporaryDirectory(prefix='build-', dir=HERE) as scratch:
        adapter = a.adapter or Path(scratch) / 'run'
        if not a.adapter:
            command = [a.compiler, '--graph', HERE / 'modules.wfg', '--entry', 'run', '-o', adapter]
            if a.cache:
                command += ['--cache', a.cache]
            started = time.monotonic()
            run(command, cwd=ROOT)
            print(f'Native build: {time.monotonic() - started:.3f}s', flush=True)
        for name in names:
            source = (HERE / f'{name}.lua').read_bytes()
            expected = run([a.lua, HERE / 'reference.lua'], input=source, capture_output=True).stdout
            for budget in (18446744073709551615, 7, 1):
                started = time.monotonic()
                result = run([adapter], input=struct.pack('<Q', budget) + source, capture_output=True, timeout=60)
                elapsed = time.monotonic() - started
                ok = compare(expected, result.stdout)
                timings.append(elapsed)
                rows.append({'script': name, 'budget': budget, 'lines': len(expected.splitlines()), 'equal': ok, 'seconds': elapsed})
                print(f'{name}, budget {budget}: {"equal" if ok else "MISMATCH"}, {elapsed:.3f}s', flush=True)
                if not ok:
                    text = ''.join(difflib.unified_diff(expected.decode('utf8', 'backslashreplace').splitlines(True), result.stdout.decode('utf8', 'backslashreplace').splitlines(True), fromfile='PUC', tofile='Halo'))
                    print(text, flush=True)
                    mismatches.append({'script': name, 'budget': budget, 'diff': text})
        record = {'compiler_sha256': digest(a.compiler), 'lua_sha256': digest(a.lua), 'rows': rows, 'mismatches': mismatches}
        if a.results:
            a.results.write_text('# Halo library comparison results\n\n' + f'{len(rows)} script/budget comparisons; {sum(x["equal"] for x in rows)} equal; {len(mismatches)} mismatched.\n\n' + 'Reference-only bootstrap replaces the standalone Lua executable\'s libc random with Redis 7.0.15 rand.c/script_lua.c semantics. Each corpus source is otherwise identical.\n\n' + '```json\n' + json.dumps(record, indent=2) + '\n```\n')
    return bool(mismatches)


if __name__ == '__main__':
    raise SystemExit(main())
