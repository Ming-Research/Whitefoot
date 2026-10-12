#!/usr/bin/env python3
"""Registered post-E6 research diagnostics, never an adoption/performance gate.

Native tooling builds and times individual images; this investigation needs
paired counter deltas, factorial controls and path/role crossover reduction.
Hosted --smoke executes only six unqualified pilot rounds. All native execution
is invoked by CI; construction failures and the 300-second deadline stop work.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
from statistics import median
import subprocess
import time

from identity import (ElfImage, IdentityError, assembly_edges, canonical_symbol,
                      direct_calls, first_difference, normalized, on_cycle,
                      parse_disassembly, target)
from measure import balanced_order, e5c_placement
from summarize import median_interval

HERE = Path(__file__).parent
ROOT = HERE.parents[2]
PILOTS = 6
DECISIVE = {'spine': 30, 'fir': 30, 'twin': 36}
PADDING = (64, 4160, 65664)
VOID_CELLS = (('large_helper', 1), ('large_helper', 8), ('records', 8),
              ('fir', 4), ('stencil', 4), ('prefix', 4), ('histogram', 1))
COUNTERS = ('acquire_attempts', 'acquire_refusals', 'publications', 'steals',
            'requests_attempted', 'requests_posted', 'parks', 'spin_ns', 'park_ns')


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.new')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def command(argv, *, timeout=None, env=None):
    return subprocess.run(list(map(str, argv)), check=True, text=True,
                          capture_output=True, timeout=timeout, env=env)


def bound_functions(path, objdump='objdump'):
    text = command([objdump, '-d', '--no-show-raw-insn', path]).stdout
    Path(str(path) + '.disassembly').write_text(text)
    image = ElfImage(path)
    return image, image.bound(parse_disassembly(text))


def require_twin(left, right):
    if digest(left) != digest(right):
        raise ValueError(f'nonidentical twin images: {left}, {right}')


def validate_sample(row, *, width, instrumented=False, trace=False):
    if row.get('width') != width or row.get('instrumented') is not instrumented:
        raise ValueError('wrong width or counter-image identity')
    if row.get('startup_trace_requested') is not trace or (not trace and row.get('startup_trace') is not None):
        raise ValueError('startup trace output differs from requested diagnostic phase')
    if not row.get('mappings') or not all(isinstance(line, str) and re.match(r'[0-9a-f]+-[0-9a-f]+ ', line)
                                          for line in row['mappings']):
        raise ValueError('missing runtime mappings')
    if row.get('process_start_ns', 0) <= 0 or row.get('process_end_ns', 0) < row['process_start_ns']:
        raise ValueError('missing process timestamps')
    samples = row.get('samples', [])
    if [sample.get('sample') for sample in samples] != [0, 1]:
        raise ValueError('missing first/second call')
    count = None
    for sample in samples:
        if sample.get('oracle') != 'pass' or min(sample.get(key, 0) for key in ('wall_ns', 'cpu_ns', 'count', 'start_ns')) <= 0:
            raise ValueError('missing oracle or timed-call data')
        if sample.get('end_ns', 0) < sample['start_ns'] or sample['end_ns'] - sample['start_ns'] != sample['wall_ns']:
            raise ValueError('inconsistent call timestamps')
        if count is not None and count != sample['count']:
            raise ValueError('comparison extent changed')
        count = sample['count']
        for endpoint in ('before', 'after'):
            observation = sample.get(endpoint, {})
            required = {'lane_count', 'ready_count', 'idle_window_us', 'owner_word_registered', 'owner_request'}
            if not required <= observation.keys() or any(not isinstance(observation[key], int) or observation[key] < 0
                                                        for key in required - {'owner_word_registered'}):
                raise ValueError('missing actual scheduler observation')
            if observation['lane_count'] > width or observation['ready_count'] > max(0, width - 1):
                raise ValueError('impossible actual lane count')
        if instrumented:
            before, after = sample.get('snapshot_before', {}), sample.get('snapshot_after', {})
            if not isinstance(before, dict) or not isinstance(after, dict):
                raise ValueError('missing per-call counter snapshots')
            if (min(before.get('capture_start_ns', 0), after.get('capture_start_ns', 0)) <= 0
                    or before.get('capture_end_ns', 0) < before['capture_start_ns']
                    or after.get('capture_end_ns', 0) < after['capture_start_ns']
                    or before['capture_end_ns'] > sample['start_ns']
                    or after['capture_start_ns'] < sample['end_ns']):
                raise ValueError('counter capture does not bracket the timed call')
            for key in COUNTERS:
                left, right = before.get('counters', {}).get(key), after.get('counters', {}).get(key)
                if not isinstance(left, int) or not isinstance(right, int) or left < 0 or right < left:
                    raise ValueError(f'missing/nonmonotonic counter {key}')
    if trace:
        events = row.get('startup_trace', {}).get('events', [])
        if not events or any(not {'lane', 'kind', 'monotonic_ns', 'value'} <= event.keys()
                             or event['monotonic_ns'] <= 0 for event in events):
            raise ValueError('missing startup trace')
        actual = samples[-1]['after']['lane_count']
        if row['startup_trace'].get('dropped', 0) != 0:
            raise ValueError('startup trace dropped critical events; causal interpretation unverified')
        if actual:
            kinds = {(event['lane'], event['kind']) for event in events}
            if (0, 'word-registered') not in kinds or (0, 'idle-window-selected') not in kinds:
                raise ValueError('missing owner registration/idle-window trace')
            for lane in range(1, actual):
                if (lane, 'helper-ready') not in kinds or (lane, 'scan') not in kinds:
                    raise ValueError('missing helper ready/scan trace')
    return row


def counter_delta(row, sample=1):
    call = row['samples'][sample]
    return {key: call['snapshot_after']['counters'][key] - call['snapshot_before']['counters'][key]
            for key in COUNTERS}


def complete_pool(row):
    width = row['width']
    observation = row['samples'][1]['after']
    return (observation['lane_count'] in (0, 1) and observation['ready_count'] == 0 if width == 1 else
            observation['lane_count'] == width and observation['ready_count'] == width - 1
            and observation['owner_word_registered'] is True)


def estimate(values):
    if not values:
        raise ValueError('empty paired diagnostic')
    return {'median': median(values), 'interval': list(median_interval(values))}


def directional(values, threshold=0):
    result = estimate(values)
    result['criterion'] = f'lower > {threshold}; upper <= {threshold} rejects; otherwise undecided'
    result['status'] = ('supported' if result['interval'][0] > threshold else
                        'rejected' if result['interval'][1] <= threshold else 'undecided')
    return result


def selected_call_shapes(functions, root):
    """The actual legacy tap SCC must survive, not just a suggestive name."""
    cycles = []
    pending, seen = [root], set()
    while pending:
        name = pending.pop()
        if name in seen:
            continue
        seen.add(name)
        if name not in functions:
            raise ValueError(f'missing selected FIR computation {name}')
        instructions = functions[name]
        pending.extend(callee for callee in direct_calls(instructions, functions=functions)
                       if callee.startswith(('wf__par_seq_', 'wf_diagnostic_')))
        if any('fma' in instruction.mnemonic for instruction in instructions):
            raise ValueError(f'FMA/contraction in FIR computation {name}')
        edges = assembly_edges(instructions)
        components = set()
        for at in edges:
            if on_cycle(edges, at):
                # SCC = mutual reachability. Keep each actual loop once.
                from identity import reachable
                component = tuple(sorted(node for node in reachable(edges, at) if at in reachable(edges, node)))
                if component in components:
                    continue
                components.add(component)
                floating = [instructions[node].mnemonic for node in component
                            if instructions[node].mnemonic in ('mulsd', 'addsd', 'vmulsd', 'vaddsd')]
                if floating == ['mulsd', 'addsd'] * 4 or floating == ['vmulsd', 'vaddsd'] * 4:
                    cycles.append({'function': name, 'instruction_addresses': [instructions[node].address for node in component],
                                   'ordered_operations': floating})
    if not cycles:
        raise ValueError('legacy ordered four-tap hot loop did not survive; diagnostic construction blocked')
    return cycles


def compare_fir_closure(common_image, common, seq_image, seq, metadata):
    mapping = metadata['canonical_symbol_map']
    def restore(value):
        for imported, original in sorted(mapping.items(), key=lambda item: -len(item[0])):
            value = re.sub(r'(?<![\w.$])' + re.escape(imported) + r'(?![\w.$])', lambda _: original, value)
        return value
    class MappedImage:
        def __getattr__(self, name): return getattr(common_image, name)
        def code_identity(self, address, spelling): return restore(common_image.code_identity(address, spelling))
        def reference_identity(self, *args): return restore(common_image.reference_identity(*args))
    pending, seen, comparisons = [metadata['canonical_actual_root']], set(), []
    while pending:
        function = pending.pop()
        if function in seen:
            continue
        seen.add(function)
        original = mapping.get(function, canonical_symbol(function))
        if not original.startswith(('wf_', 'wf.')):
            continue  # External binding remains in the compared caller.
        if function not in common or original not in seq:
            raise ValueError(f'missing corresponding actual FIR closure function {function}/{original}')
        if original == 'wf_resource_record_abort':
            if not metadata.get('shared_cold_boundary_proof'):
                raise ValueError('missing shared latched cold-writer boundary proof')
            if not any(instruction.mnemonic == 'lock' and instruction.operands.startswith('cmpxchg')
                       for instruction in common[function]):
                raise ValueError('actual shared FIR cold writer lacks atomic latch')
            comparisons.append({'function': function, 'seq_function': original, 'exception': 'explicit shared latched cold support'})
            continue
        difference = first_difference(normalized(seq[original], image=seq_image), normalized(common[function], image=MappedImage()))
        if difference:
            raise ValueError(f'actual noinline FIR computation differs: {function}/{original}; {difference}')
        comparisons.append({'function': function, 'seq_function': original, 'identical': True})
        pending.extend(direct_calls(common[function], functions=common))
    return comparisons


def fir_selector(root, query):
    parameters = 'ptr %input, i64 %input_len, ptr %taps, i64 %tap_len, i64 %first, i64 %end, i64 %last_tap'
    arguments = parameters
    adapter = 'wf_diagnostic_fir_query' if query else 'wf_diagnostic_fir_direct'
    return f'''declare ptr @{root}({parameters})
declare ptr @{adapter}({parameters})
define ptr @wf_diagnostic_filter({parameters}) noinline {{
  %r = tail call ptr @{root}({arguments})
  ret ptr %r
}}
define void @wf_bench_fir({parameters}, ptr %out, ptr %out_len, ptr %out_cell) noinline {{
  %r = call ptr @{adapter}({arguments})
  %n = load i64, ptr %r
  %p = getelementptr inbounds i8, ptr %r, i64 8
  store ptr %p, ptr %out
  store i64 %n, ptr %out_len
  store ptr %r, ptr %out_cell
  ret void
}}
'''


def prepare(args):
    """Hosted construction only. Every link input is retained and hashed."""
    build, mode = args.build, args.mode
    output = build / 'diagnostics' / mode
    output.mkdir(parents=True, exist_ok=True)
    native = [Path(path) for path in args.native_objects]
    cflags, ldflags = shlex.split(args.cflags), shlex.split(args.ldflags)
    runner = build / 'diagnostic_runner.o'
    base = [path for path in native if path.name not in ('demand-core.o', 'core.o', 'prim_host.o')]
    prim = build / 'native/sched/prim_host.o'
    cores = {key: build / f'diagnostic-{key}-core.o' for key in ('shipped', 'zero', 'counter')}
    proof = {'schema': 1, 'mode': mode, 'images': {}, 'link_inputs': {}, 'construction': 'research-only; no LTO',
             'runtime_source_sha256': digest(ROOT / 'compiler/src/backend/sched/core.c'),
             'native_configuration_sha256': digest(build / 'native/configuration.txt'),
             'runtime_flags': {'shipped': 'observations only', 'zero': 'observations; WF_PAR_IDLE_WINDOW_US=0',
                               'counter': 'WF_PAR_DIAGNOSTICS; no timing admission'}}

    def link(name, objects, *, counter=False, twin_of=None, **metadata):
        image = output / name
        image.parent.mkdir(parents=True, exist_ok=True)
        if twin_of:
            shutil.copyfile(output / twin_of, image)
            image.chmod((output / twin_of).stat().st_mode)
            require_twin(output / twin_of, image)
            proof['link_inputs'][name] = proof['link_inputs'][twin_of]
        else:
            command([args.clang, '-O2', *objects, *ldflags, '-o', image])
            proof['link_inputs'][name] = [{'path': str(path.relative_to(build)), 'sha256': digest(path)} for path in objects]
        elf, functions = bound_functions(image, args.objdump)
        proof['images'][name] = dict(path=str(image.relative_to(build)), sha256=digest(image), instrumented=counter,
                                    **metadata)
        return elf, functions

    if mode == 'spine':
        for policy, arm in [('eager', 'demand'), ('requested', 'previous')]:
            wf = build / arm / 'spine.o'
            oracle = build / 'micro-spine.o'
            for idle in ('shipped', 'zero'):
                name = f'{policy}-{idle}'
                link(name, [wf, oracle, runner, *base, prim, cores[idle]], policy=policy, idle=idle, wf_object=digest(wf))
                link(name + '-twin', [], twin_of=name, policy=policy, idle=idle, wf_object=digest(wf))
            link(policy + '-counter', [wf, oracle, runner, *base, build / 'diagnostic-prim.o', cores['counter']],
                 counter=True, policy=policy, idle='shipped', wf_object=digest(wf))
        link('seq', [build / 'seq/spine.o', build / 'micro-spine.o', runner, *base, prim, cores['shipped']],
             policy='seq', idle='shipped', wf_object=digest(build / 'seq/spine.o'))
    elif mode == 'twin':
        for workload in sorted({name for name, _ in VOID_CELLS}):
            wf = build / 'demand' / f'{workload}.o'
            oracle = build / (f'micro-{workload}.o' if workload == 'large_helper' else f'oracle-{workload}.o')
            link(workload + '/demand', [wf, oracle, runner, *base, prim, cores['shipped']], wf_object=digest(wf))
            link(workload + '/twin', [], twin_of=workload + '/demand', wf_object=digest(wf))
            if workload == 'large_helper':
                link(workload + '/trace', [wf, oracle, runner, *base, build / 'diagnostic-prim.o', cores['counter']],
                     counter=True, wf_object=digest(wf))
    else:
        from diagnostic_ir import build_fir_module, build_seq_noinline_module
        texts = [(build / arm / 'fir.ll.raw').read_text() for arm in ('seq', 'demand', 'par')]
        common, metadata = build_fir_module(*texts)
        proof['fir'] = metadata
        (output / 'compute.ll').write_text(common)
        command([args.clang, *cflags, '-ffp-contract=off', '-Wno-override-module', '-x', 'ir', '-c', output / 'compute.ll', '-o', output / 'compute.o'])
        (output / 'seq-noinline.ll').write_text(build_seq_noinline_module(texts[0]))
        command([args.clang, *cflags, '-ffp-contract=off', '-Wno-override-module', '-x', 'ir', '-c', output / 'seq-noinline.ll', '-o', output / 'seq-noinline.o'])
        reference_selector = output / 'seq-reference.ll'
        reference_selector.write_text(fir_selector(metadata['seq_actual_root'], False))
        command([args.clang, *cflags, '-Wno-override-module', '-c', '-x', 'ir', reference_selector, '-o', reference_selector.with_suffix('.o')])
        seq_image, seq_functions = link('seq-noinline-reference', [output / 'seq-noinline.o', build / 'diagnostic_fir_adapter.o',
            reference_selector.with_suffix('.o'), build / 'oracle-fir.o', runner, *base, prim, cores['shipped']], reference=True)
        for construction in ('canonical', 'legacy'):
            for query in (False, True):
                label = construction + ('-query' if query else '-direct')
                source = output / (label + '.ll')
                source.write_text(fir_selector('wf_diagnostic_' + construction + '_filter', query))
                selector = source.with_suffix('.o')
                command([args.clang, *cflags, '-Wno-override-module', '-c', '-x', 'ir', source, '-o', selector])
                for padding in PADDING:
                    objects = ([build / 'layout' / f'{padding}.o'] if padding else []) + [output / 'compute.o',
                        build / 'diagnostic_fir_adapter.o', selector, build / 'oracle-fir.o', runner, *base, prim, cores['shipped']]
                    link(f'{padding}/{label}', objects, construction=construction, query=query, padding=padding,
                         compute_object=digest(output / 'compute.o'))
                    link(f'{padding}/{label}-twin', [], twin_of=f'{padding}/{label}', construction=construction,
                         query=query, padding=padding, compute_object=digest(output / 'compute.o'))
        common_image, functions = bound_functions(output / '64/legacy-direct', args.objdump)
        proof['fir']['legacy_four_tap_loops'] = selected_call_shapes(functions, metadata['legacy_actual_root'])
        proof['fir']['actual_canonical_closure'] = compare_fir_closure(common_image, functions, seq_image, seq_functions, metadata)
        validate_fir_images(build, proof, args.objdump)
    write_json(output / 'construction.json', proof)


def validate_fir_dispatch(functions, chosen):
    for name in ('wf_diagnostic_fir_active', 'wf_diagnostic_fir_inactive'):
        if direct_calls(functions[name], functions=functions) != {'wf_diagnostic_filter'}:
            raise ValueError('FIR query leaves do not share the exact selected callee')
    query = functions['wf_diagnostic_fir_query']
    points = [at for at, instruction in enumerate(query) if instruction.mnemonic.startswith('call')
              and target(instruction, functions) and target(instruction, functions)[1] == 'wf__par_pool_active']
    if len(points) != 1 or on_cycle(assembly_edges(query), points[0]):
        raise ValueError('FIR query is not exactly once outside all loops')
    if direct_calls(query, functions=functions) != {'wf__par_pool_active', 'wf_diagnostic_fir_active', 'wf_diagnostic_fir_inactive'}:
        raise ValueError('FIR query no longer selects the two distinct same-callee leaves')
    if direct_calls(functions['wf_diagnostic_filter'], functions=functions) != {chosen}:
        raise ValueError('FIR selector calls a different actual kernel entrance')


def validate_fir_images(build, proof, objdump='objdump'):
    """All treatments share one computation object and fixed symbol placement."""
    objects = {row.get('compute_object') for row in proof['images'].values() if not row.get('reference')}
    if len(objects) != 1 or None in objects:
        raise ValueError('FIR treatments do not share one compiled computation object')
    roots = list(proof['fir']['exposed_roots'].values()) + [proof['fir']['canonical_actual_root'], proof['fir']['legacy_actual_root']]
    for padding in PADDING:
        reference = None
        for label in ('canonical-direct', 'canonical-query', 'legacy-direct', 'legacy-query'):
            image, functions = bound_functions(build / proof['images'][f'{padding}/{label}']['path'], objdump)
            positions = {name: (image.symbols[name][0], image.symbols[name][1]) for name in roots}
            if reference is None:
                reference = positions
            if positions != reference:
                raise ValueError('FIR compute placement differs across matched selector treatments')
            chosen = proof['fir']['exposed_roots'][label.split('-', 1)[0]]
            validate_fir_dispatch(functions, chosen)
            if any('fma' in instruction.mnemonic for name in roots for instruction in functions[name]):
                raise ValueError('FIR kernel contracted arithmetic')
    proof['fir']['matched_placement'] = 'same absolute exposed-root starts and sizes within each padding treatment'


def image_plan(mode, smoke=False):
    widths = (1, 2) if smoke else (1, 4, 8)
    if mode == 'spine':
        arms = ['seq'] + [f'{policy}-{idle}{twin}' for policy in ('eager', 'requested')
                         for idle in ('shipped', 'zero') for twin in ('', '-twin')]
        return [('spine', width, 0, arms) for width in widths]
    if mode == 'fir':
        arms = [f'{construction}-{adapter}{twin}' for construction in ('canonical', 'legacy')
                for adapter in ('direct', 'query') for twin in ('', '-twin')]
        return [('fir', 1, padding, arms) for padding in PADDING]
    arms = ['distinct-D', 'distinct-T', 'swapped-D', 'swapped-T', 'same-D', 'same-T']
    cells = [(name, width) for name, width in VOID_CELLS] if not smoke else [(name, width) for name in sorted({n for n, _ in VOID_CELLS}) for width in widths]
    return [(name, width, 0, arms) for name, width in cells]


def resolve_image(mode, name, padding, arm):
    if mode == 'spine':
        return arm
    if mode == 'fir':
        return f'{padding}/{arm}'
    pair, role = arm.split('-')
    physical = 'twin' if (pair == 'distinct' and role == 'T') or (pair == 'swapped' and role == 'D') else 'demand'
    return f'{name}/{physical}'


def validate_construction(build, mode):
    path = build / 'diagnostics' / mode / 'construction.json'
    proof = json.loads(path.read_text())
    if proof.get('mode') != mode or not proof.get('images') or not proof.get('link_inputs'):
        raise ValueError('missing diagnostic construction evidence')
    for name, row in proof['images'].items():
        image = build / row['path']
        if image.resolve().is_relative_to(build.resolve()) is False or digest(image) != row['sha256']:
            raise ValueError('diagnostic image changed')
        for obj in proof['link_inputs'][name]:
            source = build / obj['path']
            if not source.resolve().is_relative_to(build.resolve()) or digest(source) != obj['sha256']:
                raise ValueError('diagnostic object changed')
    return proof


def validate_hosted_smoke(build, mode, proof):
    smoke = json.loads((build / f'diagnostic-{mode}-smoke.json').read_text())
    if (smoke.get('status') != 'complete' or smoke.get('smoke') is not True
            or smoke.get('decisive') or smoke.get('counter_decisive')
            or smoke.get('construction') != proof):
        raise ValueError('missing/stale/incomplete hosted smoke evidence')
    expected = {(name, width, padding, arm, round_id)
                for name, width, padding, arms in image_plan(mode, True)
                for arm in arms for round_id in range(PILOTS)}
    actual = [(row['workload'], row['width'], row['padding'], row['label'], row['round'])
              for row in smoke.get('pilots', [])]
    if len(actual) != len(set(actual)) or set(actual) != expected:
        raise ValueError('hosted smoke pilot matrix incomplete')
    if mode == 'spine':
        wanted = {('spine', width, arm, round_id) for width in (1, 2)
                  for arm in ('eager-counter', 'requested-counter') for round_id in range(PILOTS)}
    elif mode == 'twin':
        wanted = {('large_helper', 2, arm, round_id) for arm in ('trace-D', 'trace-T') for round_id in range(PILOTS)}
    else:
        wanted = set()
    counters = [(row['workload'], row['width'], row['label'], row['round']) for row in smoke.get('counter_pilots', [])]
    if len(counters) != len(set(counters)) or set(counters) != wanted:
        raise ValueError('hosted smoke counter/trace matrix incomplete')
    for row in smoke.get('counter_pilots', []):
        if row.get('instrumented') is not True or row.get('startup_trace_requested') is not (mode == 'twin'):
            raise ValueError('hosted smoke counter/trace flags differ')
    return smoke


def run_diagnostic(args):
    start = time.monotonic()
    proof = validate_construction(args.build, args.mode)
    output = args.build / f'diagnostic-{args.mode}.json'
    result = {'schema': 1, 'mode': args.mode, 'status': 'incomplete', 'qualification': 'unqualified hosted smoke' if args.smoke else 'diagnostic only; no E6 adoption verdict',
              'deadline_seconds': 300, 'smoke': args.smoke, 'pilots': [], 'decisive': [], 'counter_pilots': [], 'counter_decisive': [],
              'construction': proof, 'bootstrap': {'draws': 10000, 'seed': 20261010, 'coverage': 0.95}}
    if args.smoke:
        cpus = sorted(os.sched_getaffinity(0))
        if len(cpus) < 2:
            raise ValueError('hosted smoke requires two available CPUs')
        placement = {1: cpus[:1], 2: cpus[:2]}
    else:
        hosted = validate_hosted_smoke(args.build, args.mode, proof)
        result['hosted_smoke_sha256'] = digest(args.build / f'diagnostic-{args.mode}-smoke.json')
        result['hosted_smoke_elapsed_seconds'] = hosted.get('elapsed_seconds')
        if '14900K' not in Path('/proc/cpuinfo').read_text():
            raise ValueError('decisive diagnostics require the registered 14900K runner')
        placement, topology = e5c_placement()
        result['topology'] = topology
    result['placement'] = placement
    if os.environ.get('WF_SPLIT_WORK'):
        raise ValueError('WF_SPLIT_WORK override is outside the registered diagnostic settings')
    result['settings'] = {'WF_PAR_DEMAND': 'on', 'WF_SPLIT_WORK': 'unset', 'WF_SCHED_REPORT': '0',
                          'pilots_same_input_as_decisive': not args.smoke}
    manifest = json.loads((HERE / 'manifest.json').read_text())
    plan = image_plan(args.mode, args.smoke)
    # An outer CI timeout must never leave the transferred hosted result
    # looking like a completed native run. Raw sample files are incremental.
    write_json(output, result)

    def invoke(name, width, padding, arm, round_id, phase, *, counter=False, trace=False):
        remaining = 300 - (time.monotonic() - start)
        if remaining <= 0:
            raise TimeoutError('registered 300-second diagnostic deadline')
        physical = (f'{name}/trace' if trace else arm) if counter else resolve_image(args.mode, name, padding, arm)
        image = proof['images'].get(physical)
        if image is None:
            raise ValueError(f'missing diagnostic image {physical}')
        env = dict(os.environ, WF_WORKERS=str(width), WF_PAR_DEMAND='on', WF_SCHED_REPORT='0')
        env.pop('WF_SPLIT_WORK', None)
        env.pop('WFD_STARTUP_TRACE', None)
        if trace:
            env['WFD_STARTUP_TRACE'] = '1'
        if 'repetitions' in manifest[name]:
            env.update(WFD_REPETITIONS=str(manifest[name]['sizing_repetitions'] if args.smoke else manifest[name]['repetitions']),
                       WFD_EXTENT=str(manifest[name]['extent']))
        command_line = ['taskset', '-c', ','.join(map(str, placement[width])), args.build / image['path'], 'diagnostic', arm, str(width)]
        begun = time.monotonic_ns()
        completed = command(command_line, timeout=remaining, env=env)
        row = json.loads(completed.stdout)
        raw_path = args.build / 'diagnostics' / args.mode / 'samples' / f'{phase}-{name}-{width}-{padding}-{arm}-{round_id}.json'
        write_json(raw_path, {'stdout': row, 'stderr': completed.stderr, 'physical_image': physical})
        result['last_raw_sample'] = str(raw_path.relative_to(args.build))
        validate_sample(row, width=width, instrumented=counter, trace=trace)
        if row.get('label') != arm or row.get('workload') != name:
            raise ValueError('diagnostic oracle/label identity differs from requested cell')
        row.update(round=round_id, padding=padding, phase=phase, physical_image=physical, launch_start_ns=begun,
                   launch_end_ns=time.monotonic_ns(), stderr=completed.stderr, inputs={key: env[key] for key in ('WF_WORKERS', 'WF_PAR_DEMAND', 'WFD_REPETITIONS', 'WFD_EXTENT', 'WFD_STARTUP_TRACE') if key in env})
        result[('counter_' if counter else '') + phase].append(row)

    try:
        for phase, rounds in [('pilots', PILOTS)] + ([] if args.smoke else [('decisive', DECISIVE[args.mode])]):
            if phase == 'decisive':
                pilot_seconds = time.monotonic() - start
                projection = pilot_seconds * (1 + DECISIVE[args.mode] / PILOTS)
                result['frozen_decisive_rounds'] = DECISIVE[args.mode]
                result['pilot_elapsed_seconds'] = pilot_seconds
                result['duration_projection_seconds'] = projection
                if projection > 300:
                    raise TimeoutError('pilot projects beyond registered deadline; no decisive round started')
            # Diagnostic counters precede a separate uninstrumented block;
            # no counter process is inserted among wall-time policy controls.
            if args.mode == 'spine':
                for round_id in range(rounds):
                    for offset, (name, width, padding, _) in enumerate(plan):
                        for arm in balanced_order(['eager-counter', 'requested-counter'], round_id, offset):
                            invoke(name, width, padding, arm, round_id, phase, counter=True)
            elif args.mode == 'twin':
                for round_id in range(rounds):
                    for offset, (name, width, padding, _) in enumerate(plan):
                        if name != 'large_helper' or width == 1:
                            continue
                        for label in balanced_order(['trace-D', 'trace-T'], round_id, offset):
                            invoke(name, width, padding, label, round_id, phase, counter=True, trace=True)
            for round_id in range(rounds):
                for offset, (name, width, padding, arms) in enumerate(plan):
                    for arm in balanced_order(arms, round_id, offset):
                        invoke(name, width, padding, arm, round_id, phase)
            # Raw evidence is persisted per process; checkpoint the aggregate
            # once per phase instead of rewriting its growing trace payload
            # after every process.
            write_json(output, result)
        result['status'] = 'complete'
        if not args.smoke:
            result['reports'] = reduce_diagnostic(args.mode, result['decisive'], result['counter_decisive'], manifest)
    except (ValueError, OSError, subprocess.SubprocessError, TimeoutError) as error:
        result['status'] = 'blocked'
        result['reason'] = str(error)
        write_json(output, result)
        raise
    result['elapsed_seconds'] = time.monotonic() - start
    if result['elapsed_seconds'] > 300:
        result.update(status='blocked', reason='registered 300-second deadline exceeded during validation/reduction')
        write_json(output, result)
        raise TimeoutError(result['reason'])
    result['counter_scope'] = 'nontransactional counter snapshots bracket each call; background spin/park spans may straddle captures; instrumented clocks never enter performance ratios'
    write_json(output, result)
    if args.smoke:
        write_json(args.build / f'diagnostic-{args.mode}-smoke.json', result)


def reduce_diagnostic(mode, rows, counters, manifest):
    groups = {}
    for row in rows:
        key = row['workload'], row['width'], row['padding']
        cell = groups.setdefault(key, {})
        marker = row['label'], row['round']
        if marker in cell:
            raise ValueError('duplicate diagnostic paired row')
        cell[marker] = row['samples'][1]
    reports = []
    for (name, width, padding), cell in groups.items():
        rounds = range(DECISIVE[mode])
        def ratio(a, b, key='wall_ns'):
            return [cell[a, r][key] / cell[b, r][key] for r in rounds]
        report = dict(workload=name, width=width, padding=padding, qualified=False)
        if mode == 'spine':
            eager, requested = ratio('eager-shipped', 'requested-shipped'), ratio('eager-zero', 'requested-zero')
            report['idle_interaction'] = directional([a / b for a, b in zip(eager, requested)], 1)
            report['zero_idle_eager_over_requested'] = estimate(requested)
            report['zero_idle_repair_rejected'] = report['zero_idle_eager_over_requested']['interval'][0] > 1.05
            selected_counters = [row for row in counters if row['width'] == width]
            count_rows = {(row['label'], row['round']): counter_delta(row) for row in selected_counters}
            repetitions = {int(row['inputs']['WFD_REPETITIONS']) for row in selected_counters}
            if repetitions != {manifest[name]['repetitions']}:
                raise ValueError('counter invocation denominator differs from registered inputs')
            report['counter_pool_complete'] = all(complete_pool(row) for row in selected_counters)
            report['counter_differences_per_completed_invocation'] = {
                key: directional([(count_rows['eager-counter', r][key] - count_rows['requested-counter', r][key]) / next(iter(repetitions)) for r in rounds])
                for key in COUNTERS}
            report['twins'] = {f'{policy}-{idle}': estimate(ratio(f'{policy}-{idle}-twin', f'{policy}-{idle}'))
                               for policy in ('eager', 'requested') for idle in ('shipped', 'zero')}
        elif mode == 'fir':
            report['canonical_over_legacy'] = {adapter: directional(ratio('canonical-' + adapter, 'legacy-' + adapter), 1) for adapter in ('direct', 'query')}
            report['query_minus_direct_ns'] = {construction: estimate([cell[construction + '-query', r]['wall_ns'] - cell[construction + '-direct', r]['wall_ns'] for r in rounds]) for construction in ('canonical', 'legacy')}
            report['query_over_direct'] = {construction: estimate(ratio(construction + '-query', construction + '-direct')) for construction in ('canonical', 'legacy')}
            report['query_residue_claim_rejected'] = {construction: delta['interval'][0] >= 32000 for construction, delta in report['query_minus_direct_ns'].items()}
            report['twins'] = {arm: estimate(ratio(arm + '-twin', arm)) for arm in ('canonical-direct', 'canonical-query', 'legacy-direct', 'legacy-query')}
        else:
            report['twins'] = {pair: estimate(ratio(pair + '-T', pair + '-D')) for pair in ('distinct', 'swapped', 'same')}
            report['order_only_rejected'] = any(not (value['interval'][0] <= 1 <= value['interval'][1]) for value in report['twins'].values())
        report['twin_valid'] = all(value['interval'][0] <= 1 <= value['interval'][1] for value in report['twins'].values())
        report['causal_status'] = 'reported' if report['twin_valid'] else 'void: twin interval excludes 1'
        if not report['twin_valid']:
            if 'zero_idle_repair_rejected' in report:
                report['raw_zero_idle_repair_rejected'] = report['zero_idle_repair_rejected']
                report['zero_idle_repair_rejected'] = None
            if 'query_residue_claim_rejected' in report:
                report['raw_query_residue_claim_rejected'] = report['query_residue_claim_rejected']
                report['query_residue_claim_rejected'] = {construction: None for construction in ('canonical', 'legacy')}
            for key in ('idle_interaction',):
                if key in report:
                    report[key]['raw_status'] = report[key]['status']
                    report[key]['status'] = 'void'
            for key in ('canonical_over_legacy', 'counter_differences_per_completed_invocation'):
                for value in report.get(key, {}).values():
                    value['raw_status'] = value['status']
                    value['status'] = 'void'
        if mode == 'spine' and not report['counter_pool_complete']:
            for value in report['counter_differences_per_completed_invocation'].values():
                value.setdefault('raw_status', value['status'])
                value['status'] = 'unverified: partial or unavailable pool'
        reports.append(report)
    if mode == 'fir':
        # Fixed kernel/adapter object, relinked at the same three treatments.
        # The first numeric treatment is the explicit local floor reference.
        # This diagnostic floor does not replace any frozen E6 floor/verdict.
        base_padding = PADDING[0]
        for report in reports:
            width = report['width']
            floor = {}
            for construction in ('canonical', 'legacy'):
                controls = {}
                for padding in PADDING[1:]:
                    shifted = groups['fir', width, padding]
                    base = groups['fir', width, base_padding]
                    controls[str(padding)] = estimate([shifted[construction + '-direct', r]['wall_ns'] / base[construction + '-direct', r]['wall_ns'] for r in range(DECISIVE[mode])])
                radius = max(abs(endpoint - 1) for control in controls.values() for endpoint in control['interval'])
                band = [1 - radius, 1 + radius]
                ratio = report['query_over_direct'][construction]['interval']
                floor_valid = all(candidate['twin_valid'] for candidate in reports if candidate['width'] == width)
                floor[construction] = {'reference_padding': base_padding, 'controls': controls, 'radius': radius, 'band': band,
                                       'status': 'void: floor control twin invalid' if not floor_valid else
                                                 'at the floor' if band[0] <= ratio[0] and ratio[1] <= band[1] else 'outside the floor'}
            report['same_job_layout_floor'] = floor
    if mode == 'twin':
        for report in reports:
            if report['workload'] != 'large_helper' or report['width'] == 1:
                continue
            traces = [row for row in counters if row['workload'] == 'large_helper' and row['width'] == report['width']]
            report['startup_observations'] = [{'round': row['round'], 'label': row['label'], 'pool_complete': complete_pool(row),
                'before_second_call': row['samples'][1]['before'], 'second_call_delta': counter_delta(row),
                'trace_complete': row['startup_trace']['dropped'] == 0} for row in traces]
            report['startup_interpretation'] = 'observations only; no automated causal attribution from counters or sampled startup scans'
    return reports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=('spine', 'fir', 'twin'), required=True)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--clang', default='clang')
    parser.add_argument('--objdump', default='objdump')
    parser.add_argument('--native-objects', nargs='*', default=[])
    parser.add_argument('--cflags', default='-O2 -falign-functions=64 -Wa,-L')
    parser.add_argument('--ldflags', default='-pthread -lm -Wl,--discard-none')
    args = parser.parse_args()
    if args.prepare:
        if not args.native_objects:
            parser.error('preparation requires explicit matched native objects')
        try:
            prepare(args)
        except (ValueError, IdentityError, OSError, subprocess.SubprocessError) as error:
            write_json(args.build / 'diagnostics' / args.mode / 'construction-blocked.json', {'status': 'blocked', 'reason': str(error)})
            raise
    else:
        try:
            run_diagnostic(args)
        except (ValueError, IdentityError, OSError, subprocess.SubprocessError, TimeoutError) as error:
            output = args.build / f'diagnostic-{args.mode}.json'
            if not output.exists() or json.loads(output.read_text()).get('smoke') is True:
                write_json(output, {'schema': 1, 'mode': args.mode, 'smoke': args.smoke, 'status': 'blocked',
                                    'qualification': 'diagnostic only', 'reason': str(error)})
            raise


if __name__ == '__main__':
    main()
