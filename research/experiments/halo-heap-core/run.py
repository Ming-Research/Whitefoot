#!/usr/bin/env python3
"""Explicit, compiler-independent Lua trace comparison; never a gate dependency."""
import argparse
import hashlib
import re
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


def diagnostic_bundle():
    """Retain heap bodies verbatim; make private value payloads locally visible.

    This is an explicitly labelled diagnostic, not a module-acceptance result.
    Internal access to readonly fields has the same meaning in the declaring
    module and in this bundle. No copied compiler or heap implementation ships.
    """
    value = (ROOT / 'lib/halo/value/module.wfm').read_text()
    value = value[:value.index('public fn num_of')].replace('public ', '')
    interface = (ROOT / 'lib/halo/heap/module.wfm').read_text()
    interface = interface[:interface.index('public fn heap_new')]
    interface = interface.replace('public readonly ', '').replace('public ', '')
    parts = [value, interface, (ROOT / 'lib/halo/value/value.wf').read_text()]
    parts += [p.read_text() for p in sorted((ROOT / 'lib/halo/heap').glob('*.wf'))]
    parts += [(HERE / 'test/trace.wf').read_text()]
    source = '\n\n'.join(parts)
    aliases = [line for line in source.splitlines() if line.startswith('alias ')
               and not line.startswith(('alias Value ', 'alias Heap '))]
    source = '\n'.join(aliases) + '\n\n' + '\n'.join(
        line for line in source.splitlines() if not line.startswith('alias '))
    for prefix in ('pkg::value::', 'halo::value::', 'halo::heap::'):
        source = source.replace(prefix, '')
    return re.sub(r'\n{3,}', '\n\n', source).strip() + '\n'


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
    parser.add_argument('--diagnostic-bundle', action='store_true')
    parser.add_argument('--report', type=Path)
    parser.add_argument('--layout', action='store_true', help='diagnostic bundle only: print sizes at steps 185..225')
    args = parser.parse_args()
    checks, seconds = run([args.compiler, '--graph', 'lib/halo/modules.wfg', '--check-modules'])
    report = [f'Module check exit: {checks.returncode}; seconds: {seconds:.3f}', checks.stdout + checks.stderr]
    if checks.returncode and not args.diagnostic_bundle:
        print('\n'.join(report))
        return 2
    with tempfile.TemporaryDirectory(prefix='scratch-', dir=HERE) as temporary:
        scratch = Path(temporary)
        if args.diagnostic_bundle:
            source = scratch / 'bundle.wf'
            bundle = diagnostic_bundle()
            if args.layout:
                # Canonical render of the inserted fragment only; bodies stay verbatim.
                insertion = """    if step >= 185_u64 {
      if step <= 225_u64 {
        if heap.tables.inner.len > 0_u64 {
          let array_length = heap.tables.inner[0_u64].payload.array.inner.len;
          let node_length = heap.tables.inner[0_u64].payload.nodes.inner.len;
          let occupied = cvt::<u32, u64>(heap.tables.inner[0_u64].payload.node_count);
          if trace(factory: factory, output: output, tag: 9_u64, step: step, key: array_length, val: node_length, border: occupied, count: 0_u64, sum: 0_u64) {
          } else {
            return False();
          }
        }
      }
    }
"""
                needle = '    let border = table_border(heap: &heap, t: t);'
                assert bundle.count(needle) == 1
                bundle = bundle.replace(needle, insertion + needle)
            source.write_text(bundle)
            command = [args.compiler, str(source)]
            report += ['Mode: DIAGNOSTIC SOURCE BUNDLE; module acceptance is NOT established.']
        else:
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
        layout = [line for line in sample.stdout.splitlines() if line.startswith('9 ')]
        actual = '\n'.join(line for line in sample.stdout.splitlines() if not line.startswith('9 '))
        mismatches = compare(actual, reference.stdout)
        # Exercise both different-field and missing-row failure paths.
        assert compare('0 1\n', '0 2\n')
        assert compare('', '0 1\n')
        assert not compare('0 1\n', '0\t1\n')
        report += [f'Trace rows: WF {len(actual.splitlines())}; Lua {len(reference.stdout.splitlines())}; mismatches {len(mismatches)}',
                   '| Row | Halo | Lua |', '| --- | --- | --- |']
        report += [f'| {i} | `{a}` | `{b}` |' for i, a, b in mismatches]
        files = sorted((ROOT / 'lib/halo/heap').glob('*')) + sorted((HERE / 'test').glob('*')) + [HERE / 'reference.lua', HERE / 'run.py']
        report += ['Halo layout rows (tag step array-size nodes-size occupied 0 0):'] + layout
        report += ['Source SHA-256:']
        report += [f'{p.relative_to(ROOT)} {hashlib.sha256(p.read_bytes()).hexdigest()}' for p in files if p.is_file()]
        rendered = '\n'.join(report) + '\n'
        if args.report:
            args.report.write_text(rendered)
        print(rendered)
        return int(bool(checks.returncode or build.returncode or sample.returncode or reference.returncode or mismatches))


if __name__ == '__main__':
    raise SystemExit(main())
