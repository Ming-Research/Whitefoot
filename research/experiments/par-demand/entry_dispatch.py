#!/usr/bin/env python3
"""Isolated entry-query witness proof and report, not a workload cost model.

Native tooling does not prove the dynamic query count or retain paired
per-entry distributions. Reuse E6's ELF/CFG reader rather than another parser.
"""
import argparse
import hashlib
import json
from pathlib import Path
from identity import (ElfImage, IdentityError, assembly_edges, entry_shell,
                      first_difference, normalized, on_cycle, parse_disassembly, target)

MIN_CALLS, MAX_CALLS, MIN_BATCH_NS = 1024, 1048576, 1000000
WIDTHS = (1, 2, 4, 8)
INPUTS = ('entry-dispatch', 'entry-dispatch.disassembly', 'entry-dispatch.o',
          'entry-leaf-active.o', 'entry-leaf-inactive.o', 'demand-core.o', 'demand-entry.o',
          'native/configuration.txt')


def shape(functions, image=None):
    names = ('wf_entry_direct', 'wf_entry_dispatch', 'wf_entry_active', 'wf_entry_inactive',
             'wf__par_pool_active', 'wf__par_demand_mode')
    if any(name not in functions for name in names):
        raise IdentityError('entry witness missing real query, mode or destination')
    if first_difference(normalized(functions['wf_entry_active'], image=image),
                        normalized(functions['wf_entry_inactive'], image=image)):
        raise IdentityError('entry witness leaves differ')
    dispatch = functions['wf_entry_dispatch']
    polls = [at for at, insn in enumerate(dispatch) if insn.mnemonic.startswith('call')
             and target(insn) and target(insn)[1].removesuffix('@plt') == 'wf__par_pool_active']
    if len(polls) != 1 or on_cycle(assembly_edges(dispatch), polls[0]):
        raise IdentityError('entry witness query must execute once outside loops')
    selected = {}
    for active, leaf in ((False, 'wf_entry_inactive'), (True, 'wf_entry_active')):
        destination, decisions, _ = entry_shell(dispatch, active, argument_count=1, functions=functions)
        if destination != leaf or decisions != ['pool-active']:
            raise IdentityError('entry witness query branch or ABI restoration unproved')
        selected[str(active)] = destination
    destination, decisions, _ = entry_shell(functions['wf_entry_direct'], False,
                                            argument_count=1, functions=functions)
    if destination != 'wf_entry_inactive' or decisions:
        raise IdentityError('entry witness direct control must transfer once without a query')
    mode = normalized(functions['wf__par_demand_mode'], image=image)
    if mode != ['mov $0x1,%eax', 'ret']:
        raise IdentityError('entry witness real demand-mode marker unproved')
    return dict(status='pass', pool_decisions_per_entry=1, direct_pool_decisions_per_entry=0,
                outside_loops=True, selected_destinations=selected, identical_leaves=True,
                scope='isolated real pool query and conditional transfer; excludes actual timed-wrapper saves/spills')


def check(build):
    image = ElfImage(build / 'entry-dispatch')
    functions = image.bound(parse_disassembly((build / 'entry-dispatch.disassembly').read_text()))
    return dict(shape(functions, image), hashes={name: hashlib.sha256((build / name).read_bytes()).hexdigest()
                                               for name in INPUTS},
                protocol=dict(min_calls=MIN_CALLS, max_calls=MAX_CALLS, min_batch_ns=MIN_BATCH_NS,
                              resolution_multiplier=1000, widths=list(WIDTHS), report_only=True))


def validate_proof(build):
    saved = json.loads((build / 'entry-dispatch-proof.json').read_text())
    if saved != check(build):
        raise ValueError('entry dispatch construction proof or bound artifacts changed')
    return saved


def validate_sample(sample, width, calls, order):
    if (sample.get('width'), sample.get('calls'), sample.get('order'), sample.get('active')) != (width, calls, order, int(width > 1)):
        raise ValueError('entry dispatch sample identity mismatch')
    if (sample.get('dispatch', {}).get('pool_queries'), sample.get('direct', {}).get('pool_queries')) != (calls, 0):
        raise ValueError('entry dispatch sample query count mismatch')
    if not isinstance(sample.get('resolution_ns'), int) or sample['resolution_ns'] <= 0:
        raise ValueError('entry dispatch clock resolution missing')
    for arm in ('direct', 'dispatch', 'first_query'):
        if any(not isinstance(sample.get(arm, {}).get(field), int) or sample[arm][field] <= 0
               for field in ('wall_ns', 'cpu_ns')):
            raise ValueError('entry dispatch sample timing missing')
    return sample


def report(evidence, rounds):
    # Import lazily: summarize imports the admission/proof consumer above.
    from summarize import quantity
    if set(evidence.get('calibration', {})) != set(map(str, WIDTHS)):
        raise ValueError('entry dispatch calibration widths missing')
    if len(evidence.get('sizing', [])) != 6 * len(WIDTHS) or len(evidence.get('decisive', [])) != rounds * len(WIDTHS):
        raise ValueError('entry dispatch sizing or decisive rounds missing')
    result = []
    for width in WIDTHS:
        calibration = evidence['calibration'][str(width)]
        calls = calibration['calls']
        pilots = calibration['pilots']
        expected = MIN_CALLS
        for index, sample in enumerate(pilots):
            validate_sample(sample, width, expected, index % 2)
            threshold = max(MIN_BATCH_NS, 1000 * sample['resolution_ns'])
            enough = min(sample[arm]['wall_ns'] for arm in ('direct', 'dispatch')) >= threshold
            if index != len(pilots) - 1 and (enough or expected == MAX_CALLS):
                raise ValueError('entry dispatch calibration exceeded its fixed stopping rule')
            expected *= 2
        if not pilots or calls != pilots[-1]['calls'] or not (enough or calls == MAX_CALLS):
            raise ValueError('entry dispatch calibration not frozen by precision rule')
        sets = {}
        for phase, count in (('sizing', 6), ('decisive', rounds)):
            samples = [row for row in evidence[phase] if row.get('width') == width]
            if {row.get('round') for row in samples} != set(range(count)) or len(samples) != count:
                raise ValueError('entry dispatch unpaired or duplicate rounds')
            for row in samples:
                validate_sample(row, width, calls, row['round'] % 2)
            sets[phase] = dict(samples=samples,
                first_query_wall_ns=quantity([s['first_query']['wall_ns'] for s in samples]),
                first_query_cpu_ns=quantity([s['first_query']['cpu_ns'] for s in samples]),
                wall_delta_ns_per_entry=quantity([(s['dispatch']['wall_ns'] - s['direct']['wall_ns']) / calls for s in samples]),
                cpu_delta_ns_per_entry=quantity([(s['dispatch']['cpu_ns'] - s['direct']['cpu_ns']) / calls for s in samples]))
        result.append(dict(width=width, calls_per_batch=calls, query_count_per_dispatch_entry=1,
                           precision='sufficient' if enough else 'inconclusive at calibration cap', **sets))
    return dict(policy='report-only; no 1 ns/2% criterion applies to once-entry dispatch',
                scope='isolated real query plus conditional transfer; does not attribute actual wrapper spill/save differences', widths=result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', required=True, type=Path)
    args = parser.parse_args()
    proof = check(args.build)
    (args.build / 'entry-dispatch-proof.json').write_text(json.dumps(proof, indent=2) + '\n')
