#!/usr/bin/env python3
"""Explicit research replay; no formal gate imports this orchestration.

Calls the ordinary compiler, never implements language judgments. Owns pinned
Snowghost extraction, scale inputs, verdict recording and paired cost samples.
Retire with the investigation when superseding evidence covers its claims.
"""
import argparse
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tarfile
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CONFIGS = {'base': (), 'off': (), 'a': ('WF_JOIN_A',), 'ap': ('WF_JOIN_AP',),
           'b': ('WF_JOIN_B',), 'rows': ('WF_MID_DIV', 'WF_MID_SUB'),
           'b_rows': ('WF_JOIN_B', 'WF_MID_DIV', 'WF_MID_SUB')}
SNAPSHOTS = {
    'dc0defad7f9e096c29c4fb9b4ace42eec652fbbb': [
        'base::geometry', 'base::static_atoms', 'base::atom', 'dom',
        'text::line_break', 'css::syntax', 'css::rules', 'css::selectors',
        'image::png', 'html::tokenizer', 'html::tree_builder'],
    '5edc2f73dcf5817b832916a5d18b48fc8ff9327b': ['proto::style'],
    '2377817586b7939e552f9ede6e01c5adc21fb0b9': ['text::normalization', 'text::idna', 'url'],
    '3773a1d8aec3a54e5c0c2135c0c518ed5befffca': ['font'],
}

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(scratch, snowghost):
    jobs = {}
    for revision, modules in SNAPSHOTS.items():
        target = scratch / revision[:7]
        target.mkdir()
        archive = subprocess.check_output(['git', '-C', str(snowghost), 'archive', revision, 'renderer'])
        with tarfile.open(fileobj=io.BytesIO(archive)) as source:
            source.extractall(target, filter='data')
        # The same canonical spelling repair the original census states.
        for p in (target / 'renderer/html/tokenizer').glob('*.wf'):
            text = p.read_text().replace("'\\u{9}'", "'\\t'").replace("'\\u{d}'", "'\\r'")
            p.write_text(text)
        for module in modules:
            jobs['snowghost/' + module] = ['--graph', str(target / 'renderer/modules.wfg'), '--check-module', 'pkg::' + module]
    for n in (4, 8, 16, 32, 64):
        # A grows-with-n family whose base remains accepted. Header relations
        # and guarded writes each touch n counters, and each local theorem is
        # checked as written. Explicit guards are required in the base.
        declarations = '\n'.join(f'  let x{i} = limit;' for i in range(n))
        invariants = ',\n'.join(f'    invariant bound{i}: x{i} <= limit' for i in range(n))
        body = '\n'.join(f'    if x{i} > 0_u64 {{ set x{i} = x{i} - 1_u64; }}\n    invariant local{i}: x{i} <= limit;' for i in range(n))
        text = f'fn chain(limit: u64) -> result: unit pure {{\n{declarations}\n  loop (\n{invariants}\n  ) {{\n{body}\n    return unit;\n  }}\n  return unit;\n}}\n'
        path = scratch / f'chain-{n}.wf'
        path.write_text(text)
        jobs[f'chain/{n}'] = ['--check', str(path)]
    for module in ('vector', 'deque', 'slab', 'hash_map', 'priority_queue', 'ordered_map'):
        jobs['collections/' + module] = ['--graph', str(ROOT / 'lib/std/modules.wfg'), '--check-module', 'pkg::collections::' + module]
    return jobs


def cases():
    result = {}
    for line in (ROOT / 'tests/conformance/manifest.jsonl').read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        case = json.loads(line)
        if 'id' not in case:
            assert {'covered_by', 'rule', 'reason'} <= case.keys(), case
            continue
        path = ROOT / 'tests/conformance/cases' / case['id']
        result['corpus/' + case['id']] = (['--graph', str(path / 'modules.wfg'), '--check-modules']
                                          if path.is_dir() else ['--check', str(path.with_suffix('.wf'))])
    return result


def invoke(binary, flags, args):
    env = {key: value for key, value in os.environ.items() if key not in {x for ys in CONFIGS.values() for x in ys}}
    env.update({key: '1' for key in flags})
    # wait4 returns this child's peak RSS, unlike RUSAGE_CHILDREN's running
    # high-water mark. File-backed output prevents a blocked diagnostic pipe.
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        start = time.perf_counter()
        child = subprocess.Popen([str(binary), '--diagnostic-format', 'json', *args], stdout=out, stderr=err, env=env)
        _, status, usage = os.wait4(child.pid, 0)
        elapsed = time.perf_counter() - start
        child.returncode = os.waitstatus_to_exitcode(status)
        err.seek(0)
        detail = err.read().decode()
        if child.returncode == 0:
            verdict = 'accepted'
        else:
            try:
                diagnostic = json.loads(detail)
                verdict = ':'.join(str(diagnostic[k]) for k in ('category', 'rule', 'kind'))
            except (ValueError, KeyError) as error:
                raise RuntimeError(f'compiler stop {child.returncode}: {detail}') from error
        rss = usage.ru_maxrss if platform.system() == 'Darwin' else usage.ru_maxrss * 1024
        return verdict, child.returncode, elapsed, rss


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True, type=Path)
    parser.add_argument('--prototype', required=True, type=Path)
    parser.add_argument('--snowghost', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--mode', choices=('verdicts', 'cost'), required=True)
    parser.add_argument('--samples', type=int, default=5)
    options = parser.parse_args()
    if options.samples < 1:
        parser.error('--samples must be positive')
    options.output.mkdir(parents=True, exist_ok=True)
    metadata = {'compiler_source': subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'origin/main'], text=True).strip(),
                'base_sha256': digest(options.base), 'prototype_sha256': digest(options.prototype),
                'patch_sha256': digest(HERE / 'prototype.patch'), 'script_sha256': digest(__file__),
                'platform': platform.platform(), 'python': platform.python_version(),
                'mode': options.mode, 'samples': options.samples, 'snowghost_revisions': list(SNAPSHOTS)}
    (options.output / (options.mode + '-identity.json')).write_text(json.dumps(metadata, indent=2) + '\n')
    with tempfile.TemporaryDirectory(prefix='branch-join-') as temporary:
        jobs = prepare(Path(temporary), options.snowghost)
        corpus = cases()
        probes = {'probe/' + p.stem: ['--check', str(p)] for p in sorted((HERE / 'probes').glob('*.wf'))}
        if options.mode == 'verdicts':
            workloads = {**probes, **jobs, **corpus}
            with (options.output / 'verdicts.csv').open('w') as file:
                writer = csv.writer(file, lineterminator='\n')
                writer.writerow(['workload', 'configuration', 'verdict', 'exit'])
                for i, (name, args) in enumerate(workloads.items()):
                    for config, flags in CONFIGS.items():
                        verdict, code, _, _ = invoke(options.base if config == 'base' else options.prototype, flags, args)
                        writer.writerow([name, config, verdict, code])
                    if i % 100 == 0:
                        file.flush()
                        print(f'verdicts {i + 1}/{len(workloads)}: {name}', flush=True)
        else:
            # Cost measures unmodified inputs, not faster rejection of rewrites.
            workloads = {name: [args] for name, args in jobs.items()}
            workloads['corpus'] = list(corpus.values())
            with (options.output / 'cost.csv').open('w') as file:
                writer = csv.writer(file, lineterminator='\n')
                writer.writerow(['workload', 'round', 'configuration', 'wall_seconds', 'max_rss_bytes', 'accepted', 'rejected'])
                for name, commands in workloads.items():
                    for sample in range(options.samples + 1):
                        names = list(CONFIGS)
                        names = names[sample % len(names):] + names[:sample % len(names)]
                        for config in names:
                            elapsed = 0.0
                            rss = accepted = rejected = 0
                            for args in commands:
                                verdict, _, wall, peak = invoke(options.base if config == 'base' else options.prototype, CONFIGS[config], args)
                                elapsed += wall
                                rss = max(rss, peak)
                                accepted += verdict == 'accepted'
                                rejected += verdict != 'accepted'
                            writer.writerow([name, sample, config, f'{elapsed:.9f}', rss, accepted, rejected])
                            file.flush()
                        print(f'cost {name} round {sample}/{options.samples}', flush=True)


if __name__ == '__main__':
    main()
