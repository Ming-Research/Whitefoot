#!/usr/bin/env python3
"""Explicit P1 launches and checksum validation; no compiler or VM model."""
import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
NAMES = ('fib', 'loop', 'integer-table', 'string-key', 'concat', 'sort', 'binary-trees')
WRAPPER = 'local f=assert(loadstring(io.read("*a"), "@user_script")); f()'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(puc, halo, stats, realistic):
    pattern = rb'-?[0-9]+(?:\.[0-9]+)?(?:e[+-]?[0-9]+)?\n'
    if re.fullmatch(pattern, puc) is None or puc != halo:
        raise ValueError(f'checksum mismatch or malformed output: PUC={puc!r}, Halo={halo!r}')
    lines = stats.decode('ascii').splitlines()
    if len(lines) != 2 or any(not x.isdigit() for x in lines):
        raise ValueError(f'malformed Halo stats: {stats!r}')
    suspends, collections = map(int, lines)
    if not realistic and suspends:
        raise ValueError(f'large budget suspended {suspends} times')
    return suspends, collections


def self_check():
    assert profile_samples('    42 wf__main_body  (in halo) + 148\n') == 42
    for bad in ('', '    0 wf__main_body  (in halo) + 148\n',
                '    1 wf__main_body  (in halo)\n    2 wf__main_body  (in halo)\n'):
        try:
            profile_samples(bad)
        except ValueError:
            pass
        else:
            raise AssertionError('empty/ambiguous profile admitted')
    assert validate(b'42\n', b'42\n', b'0\n2\n', False) == (0, 2)
    assert validate(b'42\n', b'42\n', b'3\n2\n', True) == (3, 2)
    for puc, halo, stats in ((b'42\n', b'43\n', b'0\n2\n'),
                             (b'42\n', b'', b'0\n2\n'),
                             (b'42\n', b'42\n42\n', b'0\n2\n'),
                             (b'42\n', b'42\n', b'1\n2\n'),
                             (b'42\n', b'42\n', b'0\n'),
                             (b'42\n', b'42\n', b'0\nx\n')):
        try:
            validate(puc, halo, stats, False)
        except ValueError:
            pass
        else:
            raise AssertionError('bad checksum/stats were admitted')


def profile_samples(report):
    counts = re.findall(r"^\s+(\d+) wf__main_body\s", report, re.M)
    if len(counts) != 1 or int(counts[0]) == 0:
        raise ValueError('profile has no single sampled execution worker')
    return int(counts[0])


def summary(values):
    median = statistics.median(values)
    return dict(median=median, minimum=min(values), maximum=max(values),
                relative_range=(max(values) - min(values)) / median, runs=len(values))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lua', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=HERE/'target/halo')
    parser.add_argument('--kernels', required=True, help='comma-separated explicit selection')
    parser.add_argument('--runs', type=int, required=True)
    parser.add_argument('--scale', action='append', default=[], help='kernel=N; same replacement for both VMs')
    parser.add_argument('--budget', choices=('large', 'realistic'), default='large')
    parser.add_argument('--reference-only', action='store_true', help='size PUC before selecting a slower workload scale')
    parser.add_argument('--profile', action='store_true', help='sample Halo; exclude these timings from baseline')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    self_check()
    if args.runs < 1:
        parser.error('runs must be positive')
    names = args.kernels.split(',')
    if any(name not in NAMES for name in names) or len(set(names)) != len(names):
        parser.error('unknown or duplicate kernel')
    scales = {}
    for value in args.scale:
        name, count = value.split('=')
        if name not in names or int(count) < 1:
            parser.error('scale must name a selected kernel and a positive count')
        scales[name] = int(count)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    binary, lua = args.binary.resolve(), args.lua.resolve()
    data = dict(revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                host=platform.platform(), started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                binary_sha256=digest(binary), lua_sha256=digest(lua),
                compiler_sha256=digest(ROOT/'compiler/target/gate/whitefootc'),
                host_sources_sha256={str(path.relative_to(HERE)): digest(path) for path in
                                     (HERE/'modules.wfg', HERE/'host/module.wfm', HERE/'host/driver.wf')},
                budget=args.budget, profile=args.profile, reference_only=args.reference_only, scales=scales, kernels={}, launches=[])
    failure = None
    try:
        for name in names:
            source = (HERE/'kernels'/f'{name}.lua').read_bytes()
            if name in scales:
                source, replacements = re.subn(rb'\Alocal N = [0-9]+', f'local N = {scales[name]}'.encode(), source)
                assert replacements == 1
            record = dict(source_sha256=hashlib.sha256(source).hexdigest(), pairs=[])
            data['kernels'][name] = record
            for run in range(args.runs):
                pair = {}
                order = ('PUC',) if args.reference_only else (('PUC', 'Halo') if run % 2 == 0 else ('Halo', 'PUC'))
                for engine in order:
                    command = [str(lua), '-e', WRAPPER] if engine == 'PUC' else [str(binary)] + (['1000'] if args.budget == 'realistic' else [])
                    start = time.perf_counter()
                    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    profiler = None
                    if args.profile and engine == 'Halo':
                        report = args.out.with_name(f'{args.out.stem}-{name}-{run}.sample.txt').resolve()
                        profiler = subprocess.Popen(['/usr/bin/sample', str(process.pid), '10', '1', '-file', str(report)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    try:
                        stdout, stderr = process.communicate(source, timeout=180)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.communicate()
                        raise RuntimeError(f'{name}/{engine} exceeded 180 seconds')
                    elapsed = time.perf_counter() - start
                    item = dict(kernel=name, run=run+1, engine=engine, seconds=elapsed,
                                exit=process.returncode, stdout=stdout.decode('ascii', 'replace'),
                                stderr=stderr.decode('utf8', 'replace'))
                    if profiler is not None:
                        po, pe = profiler.communicate(timeout=30)
                        item.update(profiler_exit=profiler.returncode, profiler_report=report.name,
                                    profiler_output=(po+pe).decode('utf8', 'replace'))
                    data['launches'].append(item)
                    if profiler is not None and report.exists():
                        item['worker_samples'] = profile_samples(report.read_text())
                    pair[engine] = item
                    print(f'{name} {run+1}/{args.runs} {engine}: {elapsed:.6f}s exit={process.returncode} checksum={stdout.strip()!r}', flush=True)
                    if profiler is not None and (profiler.returncode or not report.exists()):
                        raise RuntimeError(f'{name}: profiler exit={profiler.returncode}, report exists={report.exists()}')
                    if process.returncode:
                        raise RuntimeError(f'{name}/{engine}: exit={process.returncode}, stderr={stderr!r}')
                if args.reference_only:
                    record['pairs'].append(dict(puc=pair['PUC']['seconds'], checksum=pair['PUC']['stdout'].strip()))
                    continue
                suspends, collections = validate(pair['PUC']['stdout'].encode(), pair['Halo']['stdout'].encode(), pair['Halo']['stderr'].encode(), args.budget == 'realistic')
                record['pairs'].append(dict(puc=pair['PUC']['seconds'], halo=pair['Halo']['seconds'],
                                            checksum=pair['PUC']['stdout'].strip(), suspends=suspends, collections=collections))
            record['puc'] = summary([x['puc'] for x in record['pairs']])
            if args.reference_only:
                print(f'{name}: PUC sizing={record["puc"]}', flush=True)
                continue
            record['halo'] = summary([x['halo'] for x in record['pairs']])
            record['ratio'] = record['halo']['median'] / record['puc']['median']
            print(f'{name}: ratio={record["ratio"]:.3f} PUC={record["puc"]} Halo={record["halo"]}', flush=True)
    except Exception as error:
        failure = str(error)
        data['failure'] = failure
    args.out.write_text(json.dumps(data, indent=2) + '\n')
    if failure:
        raise RuntimeError(failure)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
