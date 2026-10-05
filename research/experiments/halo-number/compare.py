#!/usr/bin/env python3
"""Explicit local Lua oracle experiment; builds neither Whitefoot nor Lua."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import re
import struct
import subprocess
import tempfile
import time

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compiler_cache import add_arguments as cache_arguments, flags as cache_flags

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
MASK = (1 << 64) - 1
SIGN = 1 << 63
INF = 0x7ff0000000000000
SEED = 0x48414c4f0902


def bits(value):
    return struct.unpack('<Q', struct.pack('<d', value))[0]


def number(raw):
    return struct.unpack('<d', struct.pack('<Q', raw))[0]


def isnan(raw):
    return raw & (SIGN - 1) > INF


def ulps(a, b):
    if isnan(a) or isnan(b):
        return None
    if (a & (SIGN - 1)) == INF or (b & (SIGN - 1)) == INF:
        return None
    def ordered(raw):
        return (~raw & MASK) if raw & SIGN else raw | SIGN
    return abs(ordered(a) - ordered(b))


def strings():
    tricky = [b'', b' ', b'\t\r\n\v\f', b'0', b'-0', b'+0', b'  -0 \t',
        b'1', b'.5', b'1.', b'+.5', b'-.0', b'.', b'+', b'-', b'--1',
        b'1e2', b'1E+2', b'1e-2', b'1e', b'1e+', b'1e-', b'1e2x',
        b'1e9999', b'-1e9999', b'1e-9999', b'-1e-9999', b'0e9999',
        b'0x10', b'-0x10', b'+0XfF', b' 0x20 ', b'0x', b'0xx', b'12x',
        b'000xFF', b'0X ', b'0x1.2p3', b'0x.8', b'0x1.', b'0x1p-1074',
        b'0x1p-1075', b'0x1.0000000000001p-1075', b'0x1p9999', b'-0x1p-9999',
        b'0x1p', b'0x1p+', b'0x1g', b'0x.p1', b'0xffffffffffffffffffffffff',
        b'inf', b'INF', b'Infinity', b'-infinity', b'+iNf', b'infinite',
        b'infx', b'inf in', b'nan', b'-nan', b'+NAN', b'nan()', b'nan(123)', b'nan(0123)', b'nan(0x123)', b'nan(0X123)',
        b'nan(18446744073709551615)', b'nan(18446744073709551616)',
        b'nan(18446744073709551617)', b'nan(+12)', b'nan(-1)',
        b'nan( 12)', b'nan(12 )', b'nan(123abc)', b'-nan(123)',
        b'nan(foo)', b'nan(a-b)', b'nan(a b)', b'nan(()', b'nan(())',
        b'nan(', b'nan(x)x', b'NaN(payload) \t', b'  1\n', b'1 2',
        b'1\0garbage', b'\0', b'\0 1', b'nan\0suffix', b'0x1\0g', b'1\xff',
        b'1\x85', b'\xa01', b'1,2', b'0b10', b'0o77', b'01',
        b'9007199254740993', b'18446744073709551615', b'18446744073709551616',
        b'1.7976931348623157e308', b'1.7976931348623159e308',
        b'2.2250738585072014e-308', b'2.2250738585072011e-308',
        b'4.9406564584124654e-324', b'2.4703282292062327e-324',
        b'2.4703282292062328e-324',
        b'1.00000000000000011102230246251565404236316680908203125',
        b'1.000000000000000111022302462515654042363166809082031251',
        b'1.' + b'0'*850 + b'1', b'0.'+b'0'*800+b'1e801',
        b'1e' + b'9'*100, b'1e-' + b'9'*100]
    # Truncation's sticky bit must distinguish a tie from either neighbor.
    tie = b'1.00000000000000011102230246251565404236316680908203125'
    tricky += [tie + b'0'*850 + tail for tail in [b'', b'1']]
    for exp in [-1075, -1074, -1023, -1022, -53, -1, 0, 52, 1023, 1024]:
        for significand in ['1', '1.00000000000008', '1.fffffffffffff', '.8', '0']:
            tricky.append(f'0x{significand}p{exp}'.encode())
    return tricky


def corpus(samples):
    rng = random.Random(SEED)
    specials = [0, SIGN, 1, SIGN|1, 0x000fffffffffffff,
        0x0010000000000000, 0x0010000000000001, 0x7fefffffffffffff,
        INF, SIGN|INF, 0x7ff8000000000000, 0xfff8000000000000,
        0x7ff0000000000001, 0xfff0000000000001, 0x7ff8000000004321]
    values = list(specials)
    for exponent in range(-323, 309):
        x = bits(float('1e'+str(exponent)))
        values.extend([x, x|SIGN, x-1, x+1])
    for power in range(-1074, 1024, 7):
        x = bits(math.ldexp(1.0, power))
        values.extend([x, x|SIGN])
    for integer in [1, 2, 9, 10, 99, 100, 999, 10**13, 10**14, 2**52, 2**53]:
        values.extend([bits(float(integer)), bits(-float(integer))])
    for x in [1.23456789012345, 123456789012345.0, 99999999999999.5,
              0.0000999999999999995, 0.00001, 1.00000000000005, 1.00000000000015]:
        values.extend([bits(x)-1, bits(x), bits(x)+1, bits(-x)])
    # Exact binary halfway values for decimal significant-digit rounding.
    for n in [12345678901234, 12345678901235, 99999999999999]:
        values.extend([bits(n + 0.5), bits(-(n + 0.5))])
    values.extend(rng.getrandbits(64) for _ in range(samples))
    cases = [('F', x, 0, 'format') for x in values]
    cases += [('S', s, 0, 'parse') for s in strings()]
    # Generated decimal/hex inputs exercise rounding independently of format.
    for _ in range(min(samples, 2000)):
        x = number(rng.getrandbits(64))
        if math.isfinite(x):
            cases += [('S', repr(x).encode(), 0, 'parse-decimal'),
                      ('S', x.hex().encode(), 0, 'parse-hex')]
    for i in range(samples):
        mode = i % 5
        if mode == 0:
            x, y = rng.getrandbits(64), rng.getrandbits(64)
        elif mode == 1:
            x = bits(math.ldexp(rng.uniform(0.5, 1), rng.randint(-1000, 1000)))
            y = bits(rng.uniform(-3, 3))
        elif mode == 2:
            x, y = bits(rng.uniform(0.01, 20)), bits(rng.uniform(-100, 100))
        elif mode == 3:
            x = bits(1+rng.uniform(-1e-10, 1e-10))
            y = bits(rng.uniform(-1e12, 1e12))
        else:
            x, y = bits(rng.uniform(-20, 20)), bits(float(rng.randint(-32, 32)))
        cases.append(('P', x, y, 'pow-random'))
    power_edges = specials + [bits(v) for v in [-2., -1., -0.5, 0.5, 1., 2.,
        -1075., -1074., 1024., 2.**-65, 2.**-64, 2.**63, -2.**63, 2.**53-1]]
    cases += [('P', x, y, 'pow-special') for x in power_edges for y in power_edges]
    for x in specials:
        for y in specials + [bits(3.), bits(-3.)]:
            cases.append(('M', x, y, 'fmod'))
    for _ in range(min(samples, 2000)):
        cases.append(('M', rng.getrandbits(64), rng.getrandbits(64), 'fmod'))
    for x in values[:1000] + values[-min(samples, 1000):]:
        cases += [('L', x, 0, 'floor'), ('C', x, 0, 'ceil')]
    return cases


def inputs(cases):
    lua, native = [], bytearray()
    opcodes = dict(F=1, S=2, P=3, M=4, L=5, C=6)
    for op, x, y, _ in cases:
        if op == 'S':
            assert len(x) <= 2048
            native += struct.pack('<BQQ', opcodes[op], len(x), 0) + x
            lua.append('S '+x.hex())
        else:
            native += struct.pack('<BQQ', opcodes[op], x, y)
            lua.append(op+' '+f'{x:016x}'+(' '+f'{y:016x}' if op in 'PM' else ''))
    native += bytes(17)
    return ('\n'.join(lua)+'\n').encode(), bytes(native)


def run(command, **kwargs):
    started = time.monotonic()
    result = subprocess.run(command, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=60, **kwargs)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {command!r}\n{result.stderr.decode(errors='replace')}")
    return result.stdout, time.monotonic()-started


def compare(cases, expected, actual):
    expected, actual = expected.splitlines(), actual.splitlines()
    if len(expected) != len(cases) or len(actual) != len(cases):
        raise RuntimeError(f'record count: {len(cases)} expected, {len(expected)} Lua, {len(actual)} Halo')
    groups, examples = {}, []
    for case, want, got in zip(cases, expected, actual):
        op, x, y, group = case
        stat = groups.setdefault(group, dict(count=0, mismatches=0, nan_bit_mismatches=0,
                                             nonfinite_mismatches=0, max_ulp=0, ulp_histogram={}))
        stat['count'] += 1
        if want == got:
            continue
        stat['mismatches'] += 1
        distance = None
        if op != 'F' and want != b'nil' and got != b'nil':
            a, b = int(want, 16), int(got, 16)
            distance = ulps(a, b)
            if isnan(a) and isnan(b):
                stat['nan_bit_mismatches'] += 1
            elif distance is None:
                stat['nonfinite_mismatches'] += 1
            else:
                stat['max_ulp'] = max(stat['max_ulp'], distance)
                hist = stat['ulp_histogram']; key = str(distance)
                hist[key] = hist.get(key, 0) + 1
        if sum(e['group'] == group for e in examples) < 8:
            examples.append(dict(group=group, op=op, x=x.hex() if isinstance(x, bytes) else f'{x:016x}',
                y=f'{y:016x}', oracle=want.decode(), halo=got.decode(), ulp=distance))
    return groups, examples


def self_check():
    """Prove the comparison rejects text, verdict and bit mutations and missing rows."""
    cases = [('F', 0, 0, 'format'), ('S', b'1', 0, 'parse'), ('P', 0, 0, 'pow')]
    good = b'0\n3ff0000000000000\n3ff0000000000000\n'
    assert all(s['mismatches'] == 0 for s in compare(cases, good, good)[0].values())
    bad = b'-0\nnil\n3ff0000000000001\n'
    groups, _ = compare(cases, good, bad)
    assert all(s['mismatches'] == 1 for s in groups.values())
    assert groups['pow']['max_ulp'] == 1
    try:
        compare(cases, good, b'0\n')
    except RuntimeError:
        pass
    else:
        raise AssertionError('missing record was not detected')
    assert ulps(bits(-1.), bits(math.nextafter(-1., 0.))) == 1
    assert ulps(0, SIGN) == 1
    assert ulps(INF, bits(1.)) is None


def musl_host(scratch, source, lua):
    """Compile the original local musl algorithm, independently of the WF port."""
    names = ['pow_data.h', 'exp_data.h', 'pow_data.c', 'exp_data.c', 'pow.c']
    contents = [source.joinpath(name).read_text() for name in names]
    digest = hashlib.sha256()
    for name, content in zip(names, contents):
        digest.update(name.encode()+b'\0'+content.encode())
    prefix = r"""
#include <math.h>
#include <stdint.h>
#include <string.h>
#define hidden
#undef __FP_FAST_FMA
#define __FP_FAST_FMA 1
#define TOINT_INTRINSICS 0
#define WANT_ROUNDING 0
#define predict_false(x) (x)
#define fp_barrier(x) (x)
#define fp_force_eval(x) ((void)(x))
static uint64_t asuint64(double x) { uint64_t b; memcpy(&b,&x,8); return b; }
static double asdouble(uint64_t b) { double x; memcpy(&x,&b,8); return x; }
static double eval_as_double(double x) { return x; }
static int issignaling_inline(double x) { (void)x; return 0; }
static double __math_uflow(unsigned sign) { return sign ? -0.0 : 0.0; }
static double __math_oflow(unsigned sign) { return sign ? -INFINITY : INFINITY; }
static double __math_invalid(double x) { (void)x; return NAN; }
"""
    # Only the dependency includes and exported name change. All data,
    # range reduction and polynomial expressions are the original C source.
    code = prefix + '\n'.join(re.sub(r'^#include[^\n]*', '', content, flags=re.M)
                              for content in contents)
    code = code.replace('double pow(double x, double y)', 'double halo_musl_pow(double x, double y)')
    reference = scratch/'musl.c'; reference.write_text(code)
    host = scratch/'musl-oracle'
    run(['cc', '-std=c11', '-O2', '-ffp-contract=off', '-DHALO_MUSL_REF',
         '-I'+str(lua.parent), str(HERE/'bits.c'), str(reference),
         str(lua.parent/'liblua.a'), '-lm', '-o', str(host)])
    return host, digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', required=True, type=Path)
    parser.add_argument('--lua', required=True, type=Path)
    parser.add_argument('--samples', type=int, default=10000)
    parser.add_argument('--musl-source', type=Path, help='optional local musl src/math path for independent port comparison')
    parser.add_argument('--results', type=Path, help='write measured Markdown results')
    cache_arguments(parser, 'halo-number', timing=True)
    args = parser.parse_args()
    compiler, lua = args.compiler.resolve(), args.lua.resolve()
    if args.samples < 1:
        parser.error('--samples must be positive')
    self_check()
    cases = corpus(args.samples)
    oracle_input, halo_input = inputs(cases)
    with tempfile.TemporaryDirectory(prefix='_run-', dir=HERE) as temporary:
        scratch = Path(temporary)
        adapter = scratch/'check'
        _, build_seconds = run([str(compiler), '--graph', 'lib/halo/number/tests/modules.wfg',
                                '--entry', 'check', '-o', str(adapter)] + cache_flags(args), cwd=ROOT)
        host = scratch/'oracle'
        run(['cc', '-std=c11', '-O2', '-I'+str(lua.parent), str(HERE/'bits.c'),
             str(lua.parent/'liblua.a'), '-lm', '-o', str(host)])
        expected, lua_seconds = run([str(host), str(HERE/'oracle.lua')], input=oracle_input, cwd=ROOT)
        executable_output, executable_seconds = run([str(lua), str(HERE/'oracle.lua')], input=oracle_input, cwd=ROOT)
        host_lines, executable_lines = expected.splitlines(), executable_output.splitlines()
        assert len(host_lines) == len(executable_lines) == len(cases)
        for case, host_line, executable_line in zip(cases, host_lines, executable_lines):
            if host_line == executable_line:
                continue
            if case[0] != 'F' and host_line != b'nil' and executable_line != b'nil':
                host_bits, executable_bits = int(host_line,16), int(executable_line,16)
                if isnan(host_bits) and isnan(executable_bits):
                    continue  # Lua arithmetic cannot inspect a NaN payload.
            raise RuntimeError(f'reference executable/archive disagreement: {case!r}: {host_line!r} vs {executable_line!r}')
        actual, halo_seconds = run([str(adapter)], input=halo_input, cwd=ROOT)
        musl_report = None
        if args.musl_source:
            musl, digest = musl_host(scratch, args.musl_source.resolve(), lua)
            musl_output, musl_seconds = run([str(musl), str(HERE/'oracle.lua')], input=oracle_input, cwd=ROOT)
            musl_groups, musl_examples = compare(cases, musl_output, actual)
            musl_report = dict(source_sha256=digest, seconds=musl_seconds,
                               groups={g:s for g,s in musl_groups.items() if g.startswith('pow-')},
                               examples=[e for e in musl_examples if e['group'].startswith('pow-')])
            for stat in musl_report['groups'].values():
                if stat['mismatches'] != stat['nan_bit_mismatches']:
                    raise RuntimeError('Whitefoot power differs from original musl on a non-NaN result')
    groups, examples = compare(cases, expected, actual)
    print(json.dumps(dict(build_seconds=build_seconds, lua_seconds=lua_seconds,
                          halo_seconds=halo_seconds, executable_seconds=executable_seconds, musl=musl_report, groups=groups, examples=examples), indent=2))
    if args.results:
        source_files = sorted((ROOT/'lib/halo/number').rglob('*.wf')) + sorted((ROOT/'lib/halo/number').rglob('*.wfm')) + sorted((ROOT/'lib/halo/number/tests').rglob('*.wfg'))
        source_hash = hashlib.sha256()
        for file in source_files:
            source_hash.update(str(file.relative_to(ROOT)).encode()+b'\0'+file.read_bytes())
        base = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
        lines = ['# Halo number oracle results', '',
                 f'Run UTC: {datetime.now(timezone.utc).isoformat(timespec="seconds")}.', '',
                 f'Host: {platform.platform()}; Python {platform.python_version()}.', '',
                 f'Base revision at run: `{base}`. Number source/test SHA-256: `{source_hash.hexdigest()}`.', '',
                 f'Compiler SHA-256: `{hashlib.sha256(compiler.read_bytes()).hexdigest()}`.', '',
                 f'Lua SHA-256: `{hashlib.sha256(lua.read_bytes()).hexdigest()}`.', '',
                 f'Reference liblua.a SHA-256: `{hashlib.sha256((lua.parent/"liblua.a").read_bytes()).hexdigest()}`.', '',
                 f'Seed: `{SEED:#x}`; random samples per primary operation: {args.samples}.', '',
                 f'Corpus SHA-256 (Lua input): `{hashlib.sha256(oracle_input).hexdigest()}`.', '',
                 '| Group | Cases | Bit/text mismatches | NaN bit mismatches | Other nonfinite mismatches | Maximum finite ULP |',
                 '|---|---:|---:|---:|---:|---:|']
        for group, s in groups.items():
            lines.append(f"| {group} | {s['count']} | {s['mismatches']} | {s['nan_bit_mismatches']} | {s['nonfinite_mismatches']} | {s['max_ulp']} |")
        lines += ['', f'Build: {build_seconds:.3f} s; Lua execution: {lua_seconds:.3f} s; Halo execution: {halo_seconds:.3f} s. These are sizing observations, not a performance comparison.', '',
                  f'The reference executable also ran all {len(cases)} cases in {executable_seconds:.3f} s and agreed with the archive host on every finite bit, parse verdict and format byte, and every NaN classification. NaN signs and payloads are extracted only by the archive host.', '',
                  'Every returned line and all process exit codes were checked. Comparator controls detect a changed format byte, parse verdict, power bit and missing record.', '',
                  'Finite mismatch ULP histograms:', '', '```json', json.dumps({g:s['ulp_histogram'] for g,s in groups.items() if s['mismatches']},indent=2), '```', '',
                  'First mismatches per group (inputs are hexadecimal IEEE bits, except S inputs are hexadecimal bytes):', '', '```json',json.dumps(examples,indent=2),'```','']
        if musl_report:
            lines += ['Independent comparison with the original local musl C FMA path (same power inputs). NaN payload priority intentionally follows the macOS oracle; the musl C comparison additionally confirms every non-NaN result bit of the port:', '',
                      '```json', json.dumps(musl_report, indent=2), '```', '']
        args.results.write_text('\n'.join(lines))
    # Powers are measured, with differences reported rather than concealed.
    mandatory = [g for g in groups if not g.startswith('pow-')]
    if any(groups[g]['mismatches'] for g in mandatory):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
