#!/usr/bin/env python3
"""Build both native token dumpers in scratch and compare binary-safe output."""
import argparse
import difflib
import hashlib
import json
import platform
import random
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compiler_cache import add_arguments as cache_arguments, flags as cache_flags

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--compiler', type=Path, default=ROOT / 'compiler/target/gate/whitefootc')
p.add_argument('--lua-source', type=Path, default=Path('/private/tmp/wf-redis-7.0.15/deps/lua/src'))
p.add_argument('--sample', type=int, help='Only the first N cases; size the run before full comparison')
p.add_argument('--halo-dump', type=Path, help='Use an already-built Whitefoot dump executable')
p.add_argument('--output', type=Path, help='Write JSON results to a scratch path')
cache_arguments(p, 'halo-lex', timing=True)
a = p.parse_args()
if a.sample is not None and a.sample <= 0:
    p.error("--sample must be positive")


def run(command, **kw):
    return subprocess.run([str(x) for x in command], check=True, timeout=120, **kw)


def corpus_paths(directory):
    paths = sorted(directory.rglob('*.lua'))
    if not paths:
        raise RuntimeError(f'Lua corpus is empty or missing: {directory}')
    return paths


def implementation_digest():
    digest = hashlib.sha256()
    for path in sorted((ROOT / 'lib/halo/lex').glob('*')):
        if path.suffix in {'.wf', '.wfm'}:
            digest.update(path.name.encode() + b'\0' + path.read_bytes())
    return digest.hexdigest()


def compare_dumps(expected, actual):
    if expected == actual:
        return None
    return ''.join(difflib.unified_diff(expected.decode().splitlines(True), actual.decode().splitlines(True), fromfile='PUC', tofile='Halo'))


def tricky():
    cases = {
        'keywords': b'and break do else elseif end false for function if in local nil not or repeat return then true until while AND _end end_ end2',
        'symbols': b'.. ... == >= <= ~= + - * / % ^ # = < > ~ ( ) { } [ ] ; : , . .... .....',
        'numbers': b'3 3.0 3e2 0x10 .5 5. 3E-2 3e+2 0Xff 0x1p2 0x1e2 0 01 1e999 1e-999',
        'escapes': br'''"\a\b\f\n\r\t\v\\\"\'\q\z\0\00\000\001\099\255\12x\1234" '"\"\'\\' '' ""''',
        'escaped-newlines': b'"a\\\nb\\\rc\\\r\nd\\\n\re" name',
        'mixed-newlines': b'a\nb\rc\r\nd\n\re\n\nf\r\rg -- text\r\nh',
        'long-levels': b'[[\nfirst\r\nsecond\n\rthird]] [=[\r\nx ]] y ]==] z]=] [==[ [=[inside]=] ]==] [===[\n\n]===]',
        'long-mismatch': b'[=[a ]==] b ]] c ]=] [==[a ]=] ]===] ]==]',
        'comments': b'--line\r\nname --[[\nlong\r\n]] next --[==[a ]=] ]==] last --[=oops\nreturn -- eof',
        'long-nesting-eof': b'[[a [[',
        'long-comment-nesting-eof': b'--[[a [[',
        'long-comment-nesting': b'--[[a [[b ]] c ]]',
        'long-nonzero-nesting': b'[=[a [=[b ]=] c ]=]',
        'long-mismatch-nesting': b'[[a [=[b]=] c]]',
        'long-nesting': b'[[a [[b ]] c ]]',
        'binary-string': b'"a\x00\xffb" [[\x00\xff]]',
        'binary-tokens': b'\x00\x01\x7f\x80\xff',
        'nul-check-next': b'.\x00 .\x00\x00 3\x00+abc 5\x00\x00abc',
        'empty': b'',
        'grow-string': b'"' + b'a' * 8192 + b'"',
        'grow-error': b'"' + b'a' * 8192 + br'\256"',
        'grow-comment': b'--[=[' + b'x' * 8192 + b']=] return',
    }
    for text in ['3x', '3_', '3e', '3e+', '3e-', '3e2x', '3..4', '3...4', '0x', '0xg', '0x1p', '0x1p+2', '00x10', '.3x', '1e2_']:
        cases['malformed-' + text] = text.encode()
    for name, text in {
        'eof-double': b'"abc', 'eof-single': b"'abc", 'eof-escape': b'"abc\\',
        'newline-string': b'"ab\n', 'decoded-error': br'"a\n\t\000b' + b'\n',
        'escape-256': br'"abc\256"', 'escape-999': br'"\999"',
        'long-eof': b'[=[abc\r\n', 'comment-eof': b'--[==[abc\n\r',
        'invalid-delimiter': b'[==x', 'invalid-delimiter-eof': b'[=',
        'ordinary-bracket': b'[x', 'short-comment-bad-delimiter': b'--[==x\nreturn',
    }.items(): cases[name] = text
    # One-byte controls and high bytes independently, so an early error cannot hide later observations.
    for byte in range(256):
        cases[f'byte-{byte:02x}'] = bytes([byte])
    # Fixed-seed lexical mixtures broaden boundary coverage without relying on parser validity.
    rng = random.Random(5105)
    atoms = [b'and', b'and_', b'_name9', b'3', b'.5', b'5.', b'3e-2', b'0x10',
             b'..', b'...', b'==', b'>=', b'<=', b'~=', b'[', b']', b'-', b'~',
             br'"a\n\255\q"', br"'a\1234'", b'[=[a ]]\r\n b]=]',
             b'-- short\n', b'--[==[a ]=] \n]==]', b'[[\nlong]]']
    for i in range(64):
        cases[f'mixture-{i}'] = b''.join(rng.choice(atoms) + rng.choice([b' ', b'\n', b'\r\n', b'\n\r', b'\t']) for _ in range(32))
    for prefix in ['0', '1', '.1', '01', '0x1', '0Xff', '0x', '0x1p', '0x1e']:
        for suffix in ['', '.', '..', 'e', 'e2', 'e+2', 'E-2', 'x', '_', 'p2', 'P02', 'p-2']:
            text = prefix + suffix
            cases['numeral-boundary-' + text] = text.encode()
    return [('tricky/' + n, b) for n, b in cases.items()]


with tempfile.TemporaryDirectory(prefix='halo-lex-', dir='/private/tmp') as tmp:
    scratch = Path(tmp)
    implementation = implementation_digest()
    start = time.monotonic()
    # Copy the pinned implementation, rather than linking a possibly stale archive.
    lua = scratch / 'lua'
    lua.mkdir()
    for path in a.lua_source.glob('*'):
        if path.suffix in {'.c', '.h'}: shutil.copy2(path, lua / path.name)
    source = (lua / 'llex.c').read_text()
    needle = '  luaZ_resetbuffer(ls->buff);\n  for (;;) {'
    assert source.count(needle) == 1, 'Pinned PUC llex instrumentation site changed'
    source = source.replace(needle, needle + '\n    token_start = offset(ls); token_line = ls->linenumber;')
    (lua / 'oracle-llex.c').write_text(source)
    core = 'lapi lcode ldebug ldo ldump lfunc lgc lmem lobject lopcodes lparser lstate lstring ltable ltm lundump lvm lzio lauxlib strbuf fpconv'.split()
    oracle = scratch / 'puc-dump'
    run(['cc', '-O2', '-I', lua, HERE / 'puc-dump.c', *[lua / (x + '.c') for x in core], '-lm', '-o', oracle], cwd=scratch)
    print(f'PUC native build: {time.monotonic() - start:.3f}s', flush=True)
    halo = a.halo_dump or scratch / 'halo-dump'
    if not a.halo_dump:
        start = time.monotonic()
        run(['perl', ROOT / '.github/run-check.pl', 'halo-lex-build', a.compiler, '--graph', HERE / 'modules.wfg', '--entry', 'dump', '-o', halo] + cache_flags(a), cwd=ROOT)
        print(f'Whitefoot native build: {time.monotonic() - start:.3f}s', flush=True)
    cases = [(str(f.relative_to(ROOT)), f.read_bytes()) for f in corpus_paths(ROOT / 'research/experiments/halo-oracle/scripts')]
    cases += tricky()
    cases = [(name, data, b'=fixture', 1) for name, data in cases]
    for i, chunk in enumerate([b'=literal', b'=' + b'x' * 79, b'=' + b'x' * 80, b'@' + b'p' * 72, b'@' + b'p' * 73, b'@' + b'path/' * 30, b'', b'one line', b'first\nsecond', b'first\rsecond', b'x' * 63, b'x' * 64, b'x' * 100]):
        cases.append((f'tricky/chunk-{i}', b'3x', chunk, 1))
    for i, data in enumerate([b'\n', b'name\n\n', b'and\n\n', b'3\n\n', b'"ok"\n\n', b'\x01\n\n', b'[=[\n]=]', b'--[=[\n]=]', b'name [=[a\n', b'name --[=[a ]==] \n', b'name --[[\n\n', b'name "abc\\\n']):
        cases.append((f'tricky/line-limit-{i}', data, b'=fixture', 2147483644 if i in (0, 6, 7, 8, 9, 11) else 2147483643))
    if a.sample is not None: cases = cases[:a.sample]
    names = [name for name, _, _, _ in cases]
    assert len(names) == len(set(names)), 'Duplicate comparison labels'
    mismatches = []
    tokens = errors = 0
    durations = []
    corpus_files = 0
    fixture = scratch / 'fixture.lua'
    framed = b''.join(initial_line.to_bytes(4, 'big') + len(chunk).to_bytes(4, 'big') + chunk + len(data).to_bytes(4, 'big') + data for _, data, chunk, initial_line in cases)
    start = time.monotonic()
    batch = run([halo], input=framed, stdout=subprocess.PIPE, cwd=ROOT).stdout
    halo_seconds = time.monotonic() - start
    actual_cases = []
    lines = []
    for line in batch.splitlines(True):
        lines.append(line)
        if line.startswith(b'E\t') or line.startswith(b'T\t287\t'):
            actual_cases.append(b''.join(lines))
            lines = []
    if lines or len(actual_cases) != len(cases):
        raise RuntimeError(f'Incomplete dump: {len(actual_cases)} terminal records for {len(cases)} sources; trailing {lines!r}')
    for (name, data, chunk, initial_line), actual in zip(cases, actual_cases):
        fixture.write_bytes(data)
        start = time.monotonic()
        expected = run([oracle, fixture, chunk.decode('ascii'), str(initial_line)], stdout=subprocess.PIPE).stdout
        durations.append(time.monotonic() - start)
        tokens += sum(line.startswith(b'T\t') for line in expected.splitlines())
        errors += sum(line.startswith(b'E\t') for line in expected.splitlines())
        corpus_files += name.startswith('research/')
        diff = compare_dumps(expected, actual)
        if diff is not None:
            mismatches.append({'file': name, 'diff': diff})
            print(f'MISMATCH {name}\n{diff}', flush=True)
    report = {'files': len(cases), 'corpus_files': corpus_files, 'tricky_files': len(cases) - corpus_files, 'tokens_including_eof': tokens,
              'lexical_errors': errors, 'mismatches': mismatches, 'execution_seconds': sum(durations) + halo_seconds, 'halo_batch_seconds': halo_seconds,
              'case_seconds_min': min(durations), 'case_seconds_max': max(durations),
              'llex_sha256': hashlib.sha256((a.lua_source / 'llex.c').read_bytes()).hexdigest(),
              'host': platform.platform(),
              'implementation_sha256': implementation,
              'compiler_sha256': hashlib.sha256(a.compiler.read_bytes()).hexdigest()}
    # The comparator must distinguish a wrong kind, span, line, payload, missing token, or error text.
    controls = [b'T\t285\t0\t1\t1\t61\nT\t287\t1\t1\t1\t-\n', b'E\t666f6f\n']
    x = controls[0]
    mutations = [x.replace(b'285', b'284'), x.replace(b'\t0\t', b'\t2\t'), x.replace(b'\t1\t61', b'\t2\t61'), x.replace(b'61', b'62'), x.splitlines(True)[0]]
    mutations.append(controls[1].replace(b'66', b'67'))
    assert compare_dumps(controls[0], controls[0]) is None
    assert all(compare_dumps(controls[0], m) is not None for m in mutations[:5])
    assert compare_dumps(controls[1], mutations[5]) is not None
    report['comparator_mutations_detected'] = len(mutations)
    empty_corpus = scratch / 'empty-corpus'
    empty_corpus.mkdir()
    for directory in [empty_corpus, scratch / 'missing-corpus']:
        try:
            corpus_paths(directory)
        except RuntimeError:
            pass
        else:
            raise AssertionError('Missing corpus was accepted')
    report['missing_corpus_controls_detected'] = 2
    assert implementation_digest() == implementation, 'Lexer sources changed during comparison'
    print(json.dumps(report, indent=2), flush=True)
    if a.output: a.output.write_text(json.dumps(report, indent=2) + '\n')
    raise SystemExit(bool(mismatches))
