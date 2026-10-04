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
CORPUS = ('numeric', 'base', 'strings', 'tables', 'math', 'random', 'errors', 'boundaries')


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


def observation_controls(expected):
    assert expected, 'a corpus script produced no observations'
    changed = bytes([expected[0] ^ 1]) + expected[1:]
    assert not compare(expected, changed)
    lines = expected.splitlines(keepends=True)
    assert len(lines) >= 2
    assert not compare(expected, b''.join(lines[1:]))
    assert not compare(expected, b''.join(reversed(lines)))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--compiler', type=Path, required=True)
    p.add_argument('--lua', type=Path, required=True)
    p.add_argument('--redis-source', type=Path, required=True, help='Redis 7.0.15 source tree for rand.c and bundled Lua headers')
    p.add_argument('--cc', default='cc')
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
    revision = run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True).stdout.decode().strip()
    sources = {str(path.relative_to(ROOT)): digest(path) for path in sorted((ROOT / 'lib/halo/vm').glob('*')) if path.is_file()}
    sources.update({str(path.relative_to(ROOT)): digest(path) for path in sorted(HERE.rglob('*')) if path.suffix in ('.lua', '.c', '.py', '.wf', '.wfm', '.wfg')})
    sources['Reference lua.o'] = digest(a.lua.parent / 'lua.o')
    sources['Reference liblua.a'] = digest(a.lua.parent / 'liblua.a')
    sources['Redis deps/lua/src/linit.c'] = digest(a.redis_source / 'deps/lua/src/linit.c')
    sources['Redis src/rand.c'] = digest(a.redis_source / 'src/rand.c')
    sources['Redis src/script_lua.c'] = digest(a.redis_source / 'src/script_lua.c')
    with tempfile.TemporaryDirectory(prefix='build-', dir=HERE) as scratch:
        reference = Path(scratch) / 'redis-lua'
        initialization = Path(scratch) / 'linit.o'
        started = time.monotonic()
        run([a.cc, '-O2', '-DluaL_openlibs=halo_original_openlibs', '-I', a.redis_source / 'deps/lua/src', '-c', a.redis_source / 'deps/lua/src/linit.c', '-o', initialization])
        run([a.cc, '-O2', '-I', a.redis_source / 'deps/lua/src', '-I', a.redis_source / 'src', HERE / 'reference.c', a.redis_source / 'src/rand.c', initialization, a.lua.parent / 'lua.o', a.lua.parent / 'liblua.a', '-lm', '-o', reference])
        print(f'Reference bootstrap build: {time.monotonic() - started:.3f}s', flush=True)
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
            oracle = reference if name == 'random' else a.lua
            expected = run([oracle, HERE / 'reference.lua'], input=source, capture_output=True).stdout
            observation_controls(expected)
            if name != 'random':
                control = run([reference, HERE / 'reference.lua'], input=source, capture_output=True).stdout
                assert compare(expected, control), 'relinked Lua changed a non-random observation'
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
        record = {'source_revision': revision, 'source_sha256': sources, 'adapter_sha256': digest(adapter), 'compiler_sha256': digest(a.compiler), 'lua_sha256': digest(a.lua), 'rows': rows, 'mismatches': mismatches}
        if a.results:
            a.results.write_text('# Halo library comparison results\n\n' + f'{len(rows)} script/budget comparisons; {sum(x["equal"] for x in rows)} equal; {len(mismatches)} mismatched.\n\n' + 'Reference-only C bootstrap installs callbacks from Redis 7.0.15 script_lua.c and links its original rand.c. Each corpus source is identical.\n\n' + '```json\n' + json.dumps(record, indent=2) + '\n```\n')
    return bool(mismatches)


if __name__ == '__main__':
    raise SystemExit(main())
