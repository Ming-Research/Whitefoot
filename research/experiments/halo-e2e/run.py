#!/usr/bin/env python3
"""Binary fixture transport and independent typed RESP2 comparison, no Lua evaluator."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import random
from pathlib import Path
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ORACLE = ROOT / 'research/experiments/halo-oracle'


def metadata(source):
    result = {'KEYS': [], 'ARGV': [], 'setup': []}
    for line in source.decode('utf8').splitlines():
        if not line.startswith('-- '):
            break
        name, sep, value = line[3:].partition(':')
        if sep and name in result:
            value = json.loads(value)
            if name == 'setup':
                result[name].append(value)
            else:
                result[name] = value
    return result


def lua_string(value):
    return '"' + ''.join(chr(b) if 32 <= b < 127 and b not in (34, 92)
                         else '\\%03d' % b for b in value.encode('utf8')) + '"'


def fixture(source):
    header = metadata(source)
    setup = ['KEYS={' + ','.join(map(lua_string, header['KEYS'])) + '}',
             'ARGV={' + ','.join(map(lua_string, header['ARGV'])) + '}']
    setup += ['redis.call(' + ','.join(map(lua_string, command)) + ')'
              for command in header['setup']]
    assert b'\0' not in source, 'transport delimiter occurs in source'
    return '\n'.join(setup).encode('ascii') + b'\0' + source


def canonical(reply):
    # Validate the schema before comparing; Python bool must not pass as int.
    if not isinstance(reply, dict):
        raise ValueError('reply is not an object')
    kind = reply.get('type')
    if kind == 'array' and set(reply) == {'type', 'items'}:
        if not isinstance(reply['items'], list):
            raise ValueError('array items are not a list')
        for child in reply['items']:
            canonical(child)
    elif kind == 'integer' and set(reply) == {'type', 'value'}:
        if type(reply['value']) is not int or not -(1 << 63) <= reply['value'] < (1 << 63):
            raise ValueError('reply integer is not int64')
    elif kind in ('bulk', 'error', 'status') and set(reply) == {'type', 'bytes'}:
        if not isinstance(reply['bytes'], str) or any(ord(c) > 255 for c in reply['bytes']):
            raise ValueError('reply payload is not wire bytes')
    elif kind == 'nil' and set(reply) == {'type', 'kind'}:
        if reply['kind'] not in ('bulk', 'array'):
            raise ValueError('unknown nil kind')
    else:
        raise ValueError('unknown reply schema: ' + repr(reply))
    return json.dumps(reply, ensure_ascii=True, indent=2) + '\n'


def reason(actual, expected):
    if actual.get('type') == 'error':
        error = actual['bytes']
        if expected.get('type') == 'error':
            return 'error text/location difference: ' + error
        if 'nonexistent global variable' in error:
            return 'unavailable global/library: ' + error
        if 'unsupported command' in error:
            return 'unsupported test-host command: ' + error
        if 'attempt to call a nil value' in error:
            return 'missing library/host member: ' + error
        return 'runtime/compile error: ' + error
    return 'reply/conversion difference; inspect actual JSON'


def sensitivity():
    baseline = {'type': 'array', 'items': [{'type': 'bulk', 'bytes': '\x00\xff'},
                                       {'type': 'integer', 'value': 1}]}
    good = canonical(baseline)
    assert lua_string('é') == '"\\195\\169"'
    for bad in ({'type': 'array', 'items': [{'type': 'bulk', 'bytes': '\x00\xfe'}, baseline['items'][1]]},
                {'type': 'array', 'items': [baseline['items'][0], {'type': 'integer', 'value': 2}]},
                {'type': 'nil', 'kind': 'bulk'},
                {'type': 'array', 'items': list(reversed(baseline['items']))},
                {'type': 'array', 'items': baseline['items'][:1]},
                {'type': 'array', 'items': [{'type': 'status', 'bytes': '\x00\xff'},
                                          baseline['items'][1]]}):
        assert good != canonical(bad)
    for bad in ({'type': 'integer', 'value': True}, {'type': 'bulk', 'bytes': '\u0100'},
                {'type': 'nil', 'kind': 'unknown'}, {'type': 'array', 'items': None}):
        try:
            canonical(bad)
        except ValueError:
            pass
        else:
            raise AssertionError('schema accepted injected defect')


def run(command, **kwargs):
    start = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=900, **kwargs)
    return result, time.monotonic() - start


def verify_errors(binary):
    """Redis 7 source-grounded replies, independent script identities/locations."""
    wrong = 'ERR Wrong number of args calling Redis command from script'
    missing = "Script attempted to access nonexistent global variable 'missing_global'"
    readonly = 'Attempt to modify a readonly table'
    probes = [
        (b'local n=0\nfor i=1,3 do n=n+i end\nreturn redis.call("GET","key","extra")\n', wrong, 3),
        (b'local function fail()\n  return missing_global\nend\nreturn fail()\n', 'ERR user_script:2: ' + missing, 2),
        (b'local function fail()\n  missing_global=17\nend\nreturn fail()\n', 'ERR user_script:2: ' + readonly, 2),
        (b'return redis.sha1hex()\n', 'ERR wrong number of arguments', 1),
        (b'return redis.sha1hex("a","b")\n', 'ERR wrong number of arguments', 1),
        (b'error(nil,0)\n', 'ERR nil', 1),
        (b'error(true,0)\n', 'ERR true', 1),
        (b'error(false,0)\n', 'ERR false', 1),
        (b'error(17,0)\n', 'ERR 17', 1),
        (b'error("unlocated",0)\n', 'ERR unlocated', 1),
    ]
    for source, message, line in probes:
        expected = {'type': 'error', 'bytes': message + ' script: ' + hashlib.sha1(source).hexdigest()
                    + f', on @user_script:{line}.'}
        for extra in (['one'], ['seven', 'seven'], []):
            result, _ = run([str(binary)] + extra, input=fixture(source))
            if result.returncode or canonical(json.loads(result.stdout)) != canonical(expected):
                raise AssertionError(f'error probe {source!r}, args {extra}: {result.stdout!r}, expected {expected}')
    protected = b'return {pcall(function() return redis.call("GET","key","extra") end)}'
    expected = {'type': 'array', 'items': [{'type': 'nil', 'kind': 'bulk'},
                                         {'type': 'bulk', 'bytes': wrong}]}
    result, _ = run([str(binary)], input=fixture(protected))
    if result.returncode or canonical(json.loads(result.stdout)) != canonical(expected):
        raise AssertionError(f'protected command error: {result.stdout!r}')
    protected = b'return {pcall(function() return missing_global end)}'
    expected['items'][1]['bytes'] = 'user_script:1: ' + missing
    result, _ = run([str(binary)], input=fixture(protected))
    if result.returncode or canonical(json.loads(result.stdout)) != canonical(expected):
        raise AssertionError(f'protected global error: {result.stdout!r}')
    print('Redis errors: 10 source/location/value probes at each budget + 2 protected-error probes pass', flush=True)


def verify_sha1(binary):
    """Independent binary vectors; no expected digest comes from Halo."""
    rng = random.Random(0x51A115)
    samples = [b'', b'abc', b'a\0b']
    for i in range(1000):
        # Force every padding boundary, then multiple blocks and longer inputs.
        length = i if i < 130 else rng.randrange(0, 4097)
        samples.append(rng.randbytes(length))
    times = []
    for index, data in enumerate(samples):
        escaped = ''.join('\\%03d' % b for b in data).encode('ascii')
        source = b'return redis.sha1hex("' + escaped + b'")'
        result, seconds = run([str(binary)], input=fixture(source))
        times.append(seconds)
        if result.returncode:
            raise AssertionError(f'SHA-1 vector {index}: native exit {result.returncode}')
        actual = canonical(json.loads(result.stdout))
        expected = canonical({'type': 'bulk', 'bytes': hashlib.sha1(data).hexdigest()})
        if actual != expected:
            raise AssertionError(f'SHA-1 vector {index}, length {len(data)}: {actual} != {expected}')
    print(f'SHA-1: 3 fixed + 1000 seeded random binary vectors match hashlib; '
          f'total {sum(times):.3f}s, range {min(times):.4f}..{max(times):.4f}s', flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--compiler', required=True)
    parser.add_argument('--verify-errors', action='store_true', help='check source-grounded Redis error/location probes')
    parser.add_argument('--verify-sha1', action='store_true', help='compare 1000 random binary inputs with hashlib')
    parser.add_argument('--binary', type=Path, help='reuse an already built test executable')
    parser.add_argument('--gc-stress', action='store_true', help='force full collection at every collector safepoint')
    parser.add_argument('--cases', type=Path, default=ORACLE, help='case root with scripts/GROUP/*.lua and expected/GROUP/*.txt')
    parser.add_argument('--filter', default='')
    parser.add_argument('--report', type=Path)
    parser.add_argument('--actual', type=Path, help='scratch directory for typed replies')
    parser.add_argument('--budgets', default='1000', help='comma-separated: 1,7,1000')
    args = parser.parse_args()
    sensitivity()
    budgets = [int(x) for x in args.budgets.split(',')]
    if any(b not in (1, 7, 1000) for b in budgets):
        parser.error('supported budgets are 1,7,1000')
    case_root = args.cases.resolve()
    cases = sorted((case_root / 'scripts').glob('*/*.lua'))
    cases = [p for p in cases if str(p.relative_to(case_root / 'scripts').with_suffix('')).startswith(args.filter)]
    if not cases:
        parser.error('filter matched no scripts')
    required = {p.relative_to(case_root / 'scripts').with_suffix('.txt') for p in cases}
    if not args.filter:
        present = {p.relative_to(case_root / 'expected') for p in (case_root / 'expected').glob('*/*.txt')}
        if required != present:
            raise ValueError('missing or surplus baseline files')
    source_files = sorted((ROOT / 'lib/halo').rglob('*.wf')) + sorted((ROOT / 'lib/halo').rglob('*.wfm'))
    source_files += [ROOT / 'lib/halo/modules.wfg', HERE / 'modules.wfg', HERE / 'run.py']
    source_files += sorted((HERE / 'test').glob('*'))
    digest = hashlib.sha256()
    for p in source_files:
        digest.update(str(p.relative_to(ROOT)).encode() + b'\0' + p.read_bytes())
    revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True, capture_output=True, check=True).stdout.strip()
    report = ['# Halo end-to-end comparison', '',
              f'Local parent revision: `{revision}`. Source digest: `{digest.hexdigest()}`.',
              f'GC stress: {args.gc_stress}.',
              'The digest includes every Halo module, the graph, host and runner; it identifies uncommitted source bytes too.', '']
    with tempfile.TemporaryDirectory(prefix='halo-e2e-', dir='/private/tmp') as temporary:
        scratch = Path(temporary)
        binary = args.binary.resolve() if args.binary else scratch / 'test'
        if not args.binary:
            build, seconds = run([args.compiler, '--graph', str(HERE / 'modules.wfg'), '--entry', 'test', '-o', str(binary)])
            report += [f'Native build exit {build.returncode}, {seconds:.3f} seconds.', '']
            if build.returncode:
                print((build.stdout + build.stderr).decode('utf8', 'replace'))
                return 2
        report += [f'Executable SHA-256: `{hashlib.sha256(binary.read_bytes()).hexdigest()}`.',
                   f'Compiler SHA-256: `{hashlib.sha256(Path(args.compiler).read_bytes()).hexdigest()}`.', '']
        if args.verify_sha1:
            verify_sha1(binary)
        if args.verify_errors:
            verify_errors(binary)
        rows = []
        summary = defaultdict(Counter)
        observed = {}
        for path in cases:
            relative = path.relative_to(case_root / 'scripts').with_suffix('')
            expected_path = case_root / 'expected' / relative.with_suffix('.txt')
            expected_bytes = expected_path.read_text('ascii')
            expected = json.loads(expected_bytes)
            if canonical(expected) != expected_bytes:
                raise ValueError('noncanonical baseline: ' + str(expected_path))
            source = path.read_bytes()
            for budget in budgets:
                extra = [] if budget == 1000 else ['one'] if budget == 1 else ['seven', 'seven']
                if args.gc_stress:
                    extra += ['--gc-stress']
                collections = None
                try:
                    result, seconds = run([str(binary)] + extra, input=fixture(source))
                    if result.returncode:
                        status, why, actual = 'FAIL', f'native exit {result.returncode} (2 transport/I/O, 3 setup compile, 4 setup runtime, 5 budget limit, 6 host stop)', None
                    else:
                        stats = json.loads(result.stderr)
                        collections = stats['collections']
                        if type(collections) is not int or collections < 0:
                            raise ValueError('invalid collection count')
                        actual = json.loads(result.stdout)
                        actual_bytes = canonical(actual)
                        status = 'PASS' if actual_bytes == expected_bytes else 'FAIL'
                        why = '' if status == 'PASS' else reason(actual, expected)
                        if args.actual:
                            output = args.actual / str(budget) / relative.with_suffix('.txt')
                            output.parent.mkdir(parents=True, exist_ok=True)
                            output.write_text(actual_bytes, encoding='ascii')
                        if relative in observed and observed[relative] != actual_bytes:
                            status, why = 'FAIL', 'budget changes reply; ' + why
                        observed[relative] = actual_bytes
                except (subprocess.TimeoutExpired, ValueError) as error:
                    status, why, seconds = 'FAIL', str(error), 0.0
                summary[(relative.parts[0], budget)][status] += 1
                rows.append((str(relative), budget, status, seconds, collections, why))
                print(f'{status} {relative} budget={budget} collections={collections}: {why}', flush=True)
        report += ['| Group | Budget | Passed | Failed |', '| --- | --- | ---: | ---: |']
        for (group, budget), count in sorted(summary.items()):
            report += [f'| {group} | {budget} | {count["PASS"]} | {count["FAIL"]} |']
        report += ['', '| Case | Budget | Result | Seconds | Collections | Failure reason |', '| --- | ---: | --- | ---: | ---: | --- |']
        for case, budget, status, seconds, collections, why in rows:
            report += [f'| {case} | {budget} | {status} | {seconds:.4f} | {collections} | {why.replace("|", "&#124;").replace(chr(0), "<NUL>")} |']
        report += ['', 'No scripts or expected replies were changed. No network or Redis server was used.',
                   'The reference is the stored Redis 7.0.15 RESP2 corpus. All cases are executed, including unavailable library cases.',
                   'Budget comparisons use fresh stores. They compare replies; atomic kill/restart and host Stop store preservation require separate checks.', '']
        text = '\n'.join(report)
        if args.report:
            args.report.write_text(text)
        failed = sum(c['FAIL'] for c in summary.values())
        print(f'{len(rows) - failed}/{len(rows)} passed')
        return 1 if failed else 0


if __name__ == '__main__':
    raise SystemExit(main())
