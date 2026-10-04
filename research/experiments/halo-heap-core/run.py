#!/usr/bin/env python3
"""Explicit, compiler-independent Lua trace comparison; never a gate dependency."""
import argparse
import hashlib
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def run(command):
    start = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    return result, time.monotonic() - start


def compare(actual, expected):
    left, right = actual.splitlines(), expected.splitlines()
    mismatches = []
    for i in range(max(len(left), len(right))):
        a = left[i].split() if i < len(left) else ['<missing>']
        b = right[i].split() if i < len(right) else ['<missing>']
        if a != b:
            mismatches.append((i + 1, ' '.join(a), ' '.join(b)))
    return mismatches


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--compiler', required=True)
    parser.add_argument('--lua', required=True)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    checks, seconds = run([args.compiler, '--graph', 'lib/halo/modules.wfg', '--check-modules'])
    report = [f'Module check exit: {checks.returncode}; seconds: {seconds:.3f}', checks.stdout + checks.stderr]
    if checks.returncode:
        print('\n'.join(report))
        return 2
    with tempfile.TemporaryDirectory(prefix='scratch-', dir=HERE) as temporary:
        scratch = Path(temporary)
        command = [args.compiler, '--graph', str(HERE / 'modules.wfg'), '--function', 'pkg::test::main']
        report += ['Mode: bound module program.']
        binary = scratch / 'trace'
        build, seconds = run(command + ['-o', str(binary)])
        report += [f'Build exit: {build.returncode}; seconds: {seconds:.3f}', build.stdout + build.stderr]
        if build.returncode:
            print('\n'.join(report))
            return 2
        sample, seconds = run([str(binary)])
        report += [f'Native exit: {sample.returncode}; seconds: {seconds:.3f}', sample.stderr]
        reference, seconds = run([args.lua, str(HERE / 'reference.lua')])
        report += [f'Lua exit: {reference.returncode}; seconds: {seconds:.3f}', reference.stderr]
        actual = sample.stdout
        mismatches = compare(actual, reference.stdout)
        # Different fields, missing rows, and changed traversal order must fail.
        assert compare('0 1\n', '0 2\n')
        assert compare('', '0 1\n')
        assert not compare('0 1\n', '0\t1\n')
        assert compare('3 0 127 0 12 1 0\n3 0 127 1 13 2 0\n',
                       '3 0 127 0 13 2 0\n3 0 127 1 12 1 0\n')
        rows = actual.splitlines()
        report += [f'Behavior rows: {sum(line.startswith(("0 ", "1 ", "2 ")) for line in rows)}',
                   f'Ordered pairs: {sum(line.startswith("3 ") for line in rows)}; snapshots: {sum(line.startswith("4 ") for line in rows)}']
        report += [f'Trace rows: WF {len(actual.splitlines())}; Lua {len(reference.stdout.splitlines())}; mismatches {len(mismatches)}',
                   '| Row | Halo | Lua |', '| --- | --- | --- |']
        report += [f'| {i} | `{a}` | `{b}` |' for i, a, b in mismatches]
        files = sorted((ROOT / 'lib/halo/heap').glob('*')) + sorted((HERE / 'test').glob('*')) + [HERE / 'reference.lua', HERE / 'run.py']
        report += ['Source SHA-256:']
        report += [f'{p.relative_to(ROOT)} {hashlib.sha256(p.read_bytes()).hexdigest()}' for p in files if p.is_file()]
        rendered = '\n'.join(report) + '\n'
        if args.report:
            args.report.write_text(rendered)
        print(rendered)
        return int(bool(checks.returncode or build.returncode or sample.returncode or reference.returncode or mismatches))


if __name__ == '__main__':
    raise SystemExit(main())
