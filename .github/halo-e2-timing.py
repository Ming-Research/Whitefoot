#!/usr/bin/env python3
# Temporary helper of .github/workflows/halo-branch-weights-e2.yml's timing job
# on the experiment branch claude/cold-branch-weights-e2 (never merges). It
# builds B (the merge base's compiler) and C2 (this checkout's compiler with
# WHITEFOOT_BRANCH_WEIGHTS=C2) images of the Halo interpreter and the v2h
# CoreMark interpreter, copies B's images byte for byte as the twin, and times
# the three engines interleaved on the 14900K. The build logic mirrors the
# workflow's inline e2.py build(); the timing criterion is quoted from
# research/investigations/halo-call-path/DESIGN.md.
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import time

OUT = Path(os.environ['E2T_OUT'])
TEMP = Path(os.environ['RUNNER_TEMP'])
WORK = Path(os.environ['GITHUB_WORKSPACE'])
CPUS = os.environ.get('E2T_CPUS', '2')
ENGINES = ('B', 'twin', 'C2')
COREMARK_ITERATIONS = 2000
FIB_N = 32
RUN_BOUND = 600
ROUNDS = 7
CRC = {'crclist': '0xe714', 'crcmatrix': '0x1fd7', 'crcstate': '0x8e3a'}
TIME_QUOTE = ("A change passing both then gets 14900K timing on fib and CoreMark, interleaved against B with a twin of B; "
              "it is rejected if it is not faster than B beyond the twin's spread on fib, or slower beyond it on CoreMark.")
COREMARK_DIR = TEMP / 'halo-e2-silverfir/benchmarks/wasi/coremark'
HALO_DIR = TEMP / 'halo-e2-halo'


def command(argv, label, cwd=WORK, env=None):
    with (OUT / 'logs' / (label + '.stdout')).open('wb') as stdout, \
         (OUT / 'logs' / (label + '.stderr')).open('wb') as stderr:
        print(label + ': ' + ' '.join(map(str, argv)), flush=True)
        subprocess.run(list(map(str, argv)), cwd=cwd, env=env, stdout=stdout, stderr=stderr, check=True)


def environment(weights):
    env = os.environ.copy()
    env.pop('WHITEFOOT_BRANCH_WEIGHTS', None)
    env.pop('CCC_OVERRIDE_OPTIONS', None)
    env.pop('CARGO_TARGET_DIR', None)
    if weights:
        env['WHITEFOOT_BRANCH_WEIGHTS'] = weights
    return env


def fresh(path):
    if path.exists():
        shutil.rmtree(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def build():
    images = OUT / 'images'
    for label, tree in [('base', TEMP / 'halo-e2-base'), ('candidate', WORK)]:
        env = environment(None)
        target = fresh(TEMP / 'halo-e2-targets' / label)
        env['CARGO_TARGET_DIR'] = str(target)
        command(['cargo', 'fetch', '--locked', '--manifest-path', tree / 'compiler/Cargo.toml'], label + '-fetch', env=env)
        command(['make', '-C', tree / 'compiler', 'build'], label + '-compiler', env=env)
        shutil.copy2(target / 'gate/whitefootc', images / ('whitefootc-' + label))
    command(['python3', WORK / 'research/experiments/match-dispatch/wasm/gen.py', OUT / 'inputs/v2h.wf'], 'generate-v2h')
    source = (HALO_DIR / 'research/experiments/halo-bench/kernels/fib.lua').read_text()
    text, changed = re.subn(r'^local N = [0-9]+$', 'local N = ' + str(FIB_N), source, flags=re.M)
    if changed != 1:
        raise ValueError(f'fib: expected exactly one local N binding, got {changed}')
    (OUT / 'inputs/fib.lua').write_text(text)
    for label, compiler, weights in [('B', 'base', None), ('C2', 'candidate', 'C2')]:
        env = environment(weights)
        exe = images / ('whitefootc-' + compiler)
        graph = ['--graph', 'research/experiments/halo-bench/modules.wfg', '--entry', 'bench']
        # --full-lto forbids --cache; v2h gets its own fresh cache.
        command([exe, *graph, '--full-lto', '-o', images / ('halo-' + label)], 'halo-' + label, cwd=HALO_DIR, env=env)
        command([exe, OUT / 'inputs/v2h.wf', '--cache', fresh(TEMP / 'halo-e2-caches' / ('v2h-' + label)),
                 '-o', images / ('v2h-' + label)], 'v2h-' + label, env=env)
    # The twin is a byte copy of B, the same noise control the check-time
    # investigation uses (research/investigations/check-time/DESIGN.md).
    for kind in ('halo', 'v2h'):
        shutil.copy2(images / (kind + '-B'), images / (kind + '-twin'))
    for kind in ('halo', 'v2h'):
        a, b = (images / (kind + '-B')).read_bytes(), (images / (kind + '-twin')).read_bytes()
        if a != b:
            raise ValueError(kind + ': twin differs from B')
    (OUT / 'images.sha256').write_text(subprocess.run(
        ['sha256sum', *sorted(map(str, images.iterdir()))], check=True, stdout=subprocess.PIPE, text=True).stdout)


def validate_fib(text):
    a, b = 0, 1
    for _ in range(FIB_N):
        a, b = b, a + b
    if text.strip() != str(a) or str(a) != '2178309':
        raise ValueError(f'fib({FIB_N}) expected 2178309, got {text!r}')


def validate_coremark(text):
    fields = re.findall(r'\[0\](crc\w+)\s*:\s*(0x[0-9a-fA-F]+)', text)
    got = dict(fields)
    if len(fields) != 4 or set(got) != set(CRC) | {'crcfinal'}:
        raise ValueError('missing, duplicate or malformed CoreMark CRC fields')
    if any(got[k].lower() != v for k, v in CRC.items()):
        raise ValueError('wrong CoreMark component CRCs: ' + str(got))
    # research/experiments/match-dispatch/RESULTS.md records 0x4983 for this
    # module, these seeds and 2000 iterations.
    if got['crcfinal'].lower() != '0x4983':
        raise ValueError('wrong 2000-iteration CoreMark final CRC: ' + str(got))


def run_once(workload, engine, phase, index):
    if workload == 'fib':
        argv = [OUT / 'images' / ('halo-' + engine)]
        cwd = WORK
        stdin = (OUT / 'inputs/fib.lua').open('rb')
    else:
        argv = [OUT / 'images' / ('v2h-' + engine), 'coremark.wasm', '0x0', '0x0', '0x66', str(COREMARK_ITERATIONS)]
        cwd = COREMARK_DIR
        stdin = None
    cmd = ['taskset', '-c', CPUS, 'timeout', '--foreground', '--signal=TERM', '--kill-after=10s', str(RUN_BOUND) + 's',
           *map(str, argv)]
    load = Path('/proc/loadavg').read_text().split()[0]
    try:
        start = time.perf_counter()
        result = subprocess.run(cmd, cwd=cwd, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        wall = time.perf_counter() - start
    finally:
        if stdin is not None:
            stdin.close()
    if result.returncode != 0:
        raise ValueError(f'{workload} {engine} {phase} {index}: exit {result.returncode}: {result.stderr[-500:]}')
    (validate_fib if workload == 'fib' else validate_coremark)(result.stdout)
    row = {'workload': workload, 'engine': engine, 'phase': phase, 'round': index, 'wall_seconds': wall, 'loadavg1_before': load}
    print(f'{phase} {index} {workload:8s} {engine:5s} wall={wall:.4f}s load1={load} output OK', flush=True)
    return row


def rotated(index):
    k = index % len(ENGINES)
    return ENGINES[k:] + ENGINES[:k]


def save(rows, name):
    (OUT / 'raw').mkdir(exist_ok=True)
    (OUT / 'raw' / (name + '.json')).write_text(json.dumps(rows, indent=2) + '\n')
    with (OUT / 'raw' / (name + '.tsv')).open('w') as stream:
        stream.write('phase\tround\tworkload\tengine\twall_seconds\tloadavg1_before\n')
        for r in rows:
            stream.write(f"{r['phase']}\t{r['round']}\t{r['workload']}\t{r['engine']}\t{r['wall_seconds']:.6f}\t{r['loadavg1_before']}\n")


def sample():
    # Smallest useful sample: one run of each engine per workload, order B, twin, C2.
    rows = [run_once(w, e, 'sample', 1) for w in ('fib', 'CoreMark') for e in ENGINES]
    save(rows, 'sample')
    lines = ['## Smallest sample (1 round, not a verdict)', '', '| workload | B s | twin s | C2 s |', '|---|---:|---:|---:|']
    for w in ('fib', 'CoreMark'):
        v = {r['engine']: r['wall_seconds'] for r in rows if r['workload'] == w}
        lines.append(f"| {w} | {v['B']:.4f} | {v['twin']:.4f} | {v['C2']:.4f} |")
    text = '\n'.join(lines) + '\n'
    (OUT / 'sample.md').write_text(text)
    print(text, flush=True)


def rounds():
    run_once_rows = [run_once(w, e, 'warmup', 0) for w in ('fib', 'CoreMark') for e in rotated(0)]
    rows = []
    for i in range(1, ROUNDS + 1):
        for w in ('fib', 'CoreMark'):
            for e in rotated(i):
                rows.append(run_once(w, e, 'measure', i))
    save(run_once_rows + rows, 'rounds')
    reduce(rows)


def reduce(rows):
    lines = ['# E2 branch weights: 14900K timing', '',
             f'Host CPUs pinned with taskset -c {CPUS}; {ROUNDS} interleaved rounds after 1 warm-up round, order B/twin/C2 rotated each round.',
             f'fib({FIB_N}) via the Halo interpreter (output 2178309); v2h CoreMark, {COREMARK_ITERATIONS} iterations, CRC validated, native.',
             'Twin = byte copy of B. Twin spread = |median twin - median B| / median B. Wall seconds.', '',
             '| workload | engine | median s | min s | max s | n |', '|---|---|---:|---:|---:|---:|']
    stats = {}
    for w in ('fib', 'CoreMark'):
        for e in ENGINES:
            v = [r['wall_seconds'] for r in rows if r['workload'] == w and r['engine'] == e]
            if len(v) != ROUNDS:
                raise ValueError(f'{w} {e}: expected {ROUNDS} runs, got {len(v)}')
            stats[(w, e)] = (statistics.median(v), min(v), max(v))
            lines.append(f'| {w} | {e} | {stats[(w, e)][0]:.4f} | {stats[(w, e)][1]:.4f} | {stats[(w, e)][2]:.4f} | {len(v)} |')
    lines += ['', '| workload | twin spread | B min-max range / median B | C2 / B (median ratio) | C2 vs B | verdict |',
              '|---|---:|---:|---:|---:|---|']
    verdicts = {}
    for w in ('fib', 'CoreMark'):
        mb, lo, hi = stats[(w, 'B')]
        mt, mc = stats[(w, 'twin')][0], stats[(w, 'C2')][0]
        spread = abs(mt - mb) / mb
        brange = (hi - lo) / mb
        gain = (mb - mc) / mb  # positive: C2 faster
        if w == 'fib':
            ok = gain > spread
        else:
            ok = -gain <= spread
        verdicts[w] = 'PASS' if ok else 'FAIL'
        lines.append(f'| {w} | {100 * spread:.3f}% | {100 * brange:.3f}% | {mc / mb:.5f} | {100 * gain:+.3f}% faster | {verdicts[w]} |')
    lines += ['', f'Criterion: "{TIME_QUOTE}"',
              'fib PASS iff (median B - median C2) / median B > twin spread (C2 faster beyond the spread). '
              'CoreMark PASS unless (median C2 - median B) / median B > twin spread.',
              '', 'E2 timing overall: ' + ('PASS' if all(v == 'PASS' for v in verdicts.values()) else 'FAIL (rejection)')]
    text = '\n'.join(lines) + '\n'
    (OUT / 'timing.md').write_text(text)
    print(text, flush=True)


if __name__ == '__main__':
    {'build': build, 'sample': sample, 'rounds': rounds}[sys.argv[1]]()
