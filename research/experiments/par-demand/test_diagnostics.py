"""Diagnostic contracts; synthetic data only, no compiler/native invocation."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import diagnostics as d
from identity import Instruction


def sample(width=2, instrumented=False, trace=False):
    observation = dict(lane_count=width if width > 1 else 0, ready_count=width - 1,
                       idle_window_us=1000, owner_word_registered=width > 1, owner_request=0)
    row = dict(workload='spine', label='eager-counter', width=width, instrumented=instrumented,
               mappings=['1000-2000 r-xp 00000000 00:00 1 /image'], process_start_ns=1, process_end_ns=1000,
               startup_trace=dict(dropped=0, events=[
                   dict(lane=0, kind='idle-window-selected', monotonic_ns=10, value=1000),
                   dict(lane=0, kind='word-registered', monotonic_ns=20, value=42),
                   *[dict(lane=lane, kind=kind, monotonic_ns=30, value=0)
                     for lane in range(1, width) for kind in ('helper-ready', 'scan')]]), samples=[])
    row['startup_trace_requested'] = trace
    if not trace:
        row['startup_trace'] = None
    for index, start in enumerate((100, 400)):
        before = dict(capture_start_ns=start - 10, capture_end_ns=start - 5,
                      observation=observation.copy(), counters={name: 10 for name in d.COUNTERS})
        after = dict(capture_start_ns=start + 105, capture_end_ns=start + 110,
                     observation=observation.copy(), counters={name: 20 for name in d.COUNTERS})
        row['samples'].append(dict(sample=index, start_ns=start, end_ns=start + 100, wall_ns=100,
                                   cpu_ns=100, count=1, oracle='pass', before=observation.copy(),
                                   after=observation.copy(), snapshot_before=before if instrumented else None,
                                   snapshot_after=after if instrumented else None))
    return row


class DiagnosticContracts(unittest.TestCase):
    def test_counter_capture_is_required_and_brackets_each_call(self):
        row = sample(instrumented=True, trace=True)
        self.assertIs(d.validate_sample(row, width=2, instrumented=True, trace=True), row)
        self.assertEqual(d.counter_delta(row)['acquire_attempts'], 10)
        for mutate in (lambda row: row['samples'][1].pop('snapshot_before'),
                       lambda row: row['samples'][1]['snapshot_before'].update(capture_end_ns=450),
                       lambda row: row['samples'][1]['snapshot_after']['counters'].update(publications=9),
                       lambda row: row.update(mappings=[]),
                       lambda row: row['samples'][1].update(oracle='fail')):
            wrong = copy.deepcopy(row)
            mutate(wrong)
            with self.assertRaises(ValueError): d.validate_sample(wrong, width=2, instrumented=True, trace=True)

    def test_missing_or_dropped_startup_events_never_support_a_cause(self):
        for kind in ('word-registered', 'helper-ready', 'scan', 'idle-window-selected'):
            row = sample(instrumented=True, trace=True)
            row['startup_trace']['events'] = [event for event in row['startup_trace']['events'] if event['kind'] != kind]
            with self.assertRaises(ValueError): d.validate_sample(row, width=2, instrumented=True, trace=True)
        row = sample(instrumented=True, trace=True)
        row['startup_trace']['dropped'] = 1
        with self.assertRaisesRegex(ValueError, 'dropped'): d.validate_sample(row, width=2, instrumented=True, trace=True)
        row = sample(instrumented=True)
        row['samples'][1]['after'].update(lane_count=1, ready_count=0)
        self.assertFalse(d.complete_pool(row))

    def test_counter_only_calls_do_not_dump_startup_traces(self):
        row = sample(instrumented=True)
        self.assertIs(d.validate_sample(row, width=2, instrumented=True), row)
        row['startup_trace_requested'] = True
        with self.assertRaisesRegex(ValueError, 'trace output'): d.validate_sample(row, width=2, instrumented=True)

    def test_fir_native_selection_rejects_wrong_callee_and_repeated_query(self):
        functions = {
            'wf_diagnostic_fir_active': [Instruction(0x100, 'jmp', '400 <wf_diagnostic_filter>')],
            'wf_diagnostic_fir_inactive': [Instruction(0x200, 'jmp', '400 <wf_diagnostic_filter>')],
            'wf_diagnostic_fir_query': [Instruction(0x300, 'call', '500 <wf__par_pool_active>'),
                Instruction(0x304, 'jne', '30c <wf_diagnostic_fir_query+0xc>'),
                Instruction(0x308, 'jmp', '100 <wf_diagnostic_fir_active>'),
                Instruction(0x30c, 'jmp', '200 <wf_diagnostic_fir_inactive>')],
            'wf_diagnostic_filter': [Instruction(0x400, 'jmp', '600 <canonical>')],
            'wf__par_pool_active': [Instruction(0x500, 'ret', '')],
            'canonical': [Instruction(0x600, 'ret', '')],
            'wrong': [Instruction(0x700, 'ret', '')]}
        d.validate_fir_dispatch(functions, 'canonical')
        wrong = copy.deepcopy(functions)
        wrong['wf_diagnostic_filter'] = [Instruction(0x400, 'jmp', '700 <wrong>')]
        with self.assertRaisesRegex(ValueError, 'different actual kernel'): d.validate_fir_dispatch(wrong, 'canonical')
        wrong = copy.deepcopy(functions)
        wrong['wf_diagnostic_fir_query'].append(Instruction(0x310, 'jmp', '300 <wf_diagnostic_fir_query>'))
        # Make the backedge reachable; the query itself then belongs to an SCC.
        wrong['wf_diagnostic_fir_query'][2] = Instruction(0x308, 'call', '100 <wf_diagnostic_fir_active>')
        wrong['wf_diagnostic_fir_query'][3] = Instruction(0x30c, 'call', '200 <wf_diagnostic_fir_inactive>')
        with self.assertRaisesRegex(ValueError, 'outside all loops'): d.validate_fir_dispatch(wrong, 'canonical')

    def test_fir_canonical_machine_body_is_compared_to_independent_seq(self):
        common = {'canonical': [Instruction(0x100, 'mov', '$1,%eax'), Instruction(0x104, 'ret', '')]}
        seq = {'wf_filter': [Instruction(0x200, 'mov', '$1,%eax'), Instruction(0x204, 'ret', '')]}
        metadata = dict(canonical_symbol_map={'canonical': 'wf_filter'}, canonical_actual_root='canonical')
        self.assertTrue(d.compare_fir_closure(object(), common, object(), seq, metadata))
        common['canonical'][0] = Instruction(0x100, 'mov', '$2,%eax')
        with self.assertRaisesRegex(ValueError, 'actual noinline FIR computation differs'):
            d.compare_fir_closure(object(), common, object(), seq, metadata)

    def test_path_role_crossover_has_no_label_selected_computation(self):
        self.assertEqual(d.resolve_image('twin', 'fir', 0, 'distinct-D'), 'fir/demand')
        self.assertEqual(d.resolve_image('twin', 'fir', 0, 'distinct-T'), 'fir/twin')
        self.assertEqual(d.resolve_image('twin', 'fir', 0, 'swapped-D'), 'fir/twin')
        self.assertEqual(d.resolve_image('twin', 'fir', 0, 'swapped-T'), 'fir/demand')
        self.assertEqual(d.resolve_image('twin', 'fir', 0, 'same-D'), d.resolve_image('twin', 'fir', 0, 'same-T'))
        for mode, count in d.DECISIVE.items():
            self.assertEqual(count, {'spine': 30, 'fir': 30, 'twin': 36}[mode])
        self.assertEqual({width for _, width, _, _ in d.image_plan('fir', True)}, {1})
        self.assertEqual({padding for _, _, padding, _ in d.image_plan('fir')}, {64, 4160, 65664})

    def test_changed_image_or_input_object_blocks_construction(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            (build / 'image').write_bytes(b'image')
            (build / 'input.o').write_bytes(b'object')
            proof = dict(mode='spine', images={'seq': dict(path='image', sha256=d.digest(build / 'image'))},
                         link_inputs={'seq': [dict(path='input.o', sha256=d.digest(build / 'input.o'))]})
            d.write_json(build / 'diagnostics/spine/construction.json', proof)
            self.assertEqual(d.validate_construction(build, 'spine'), proof)
            (build / 'input.o').write_bytes(b'mutated')
            with self.assertRaisesRegex(ValueError, 'object changed'): d.validate_construction(build, 'spine')
            (build / 'input.o').write_bytes(b'object')
            (build / 'image').write_bytes(b'mutated')
            with self.assertRaisesRegex(ValueError, 'image changed'): d.validate_construction(build, 'spine')

    def test_unrelated_four_tap_function_is_not_selected_kernel_evidence(self):
        functions = {'selected': [Instruction(0x100, 'ret', '')],
                     'wf__par_seq_unused': [Instruction(0x200 + at * 4, operation, '%xmm1,%xmm0')
                                           for at, operation in enumerate(['mulsd', 'addsd'] * 4)]
                                           + [Instruction(0x220, 'jmp', '200 <wf__par_seq_unused>')]}
        self.assertTrue(d.selected_call_shapes(functions, 'wf__par_seq_unused'))
        with self.assertRaisesRegex(ValueError, 'did not survive'): d.selected_call_shapes(functions, 'selected')
        functions['selected'] = [Instruction(0x100, 'vfmadd132sd', '%xmm0,%xmm1,%xmm2'), Instruction(0x104, 'ret', '')]
        with self.assertRaisesRegex(ValueError, 'FMA'): d.selected_call_shapes(functions, 'selected')

    def test_missing_smoke_matrix_blocks_native_use(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            proof = {'mode': 'fir', 'images': {}}
            pilots = [dict(workload=name, width=width, padding=padding, label=arm, round=round_id)
                      for name, width, padding, arms in d.image_plan('fir', True)
                      for arm in arms for round_id in range(d.PILOTS)]
            smoke = dict(status='complete', smoke=True, construction=proof, pilots=pilots, decisive=[], counter_decisive=[], counter_pilots=[])
            d.write_json(build / 'diagnostic-fir-smoke.json', smoke)
            self.assertEqual(d.validate_hosted_smoke(build, 'fir', proof), smoke)
            smoke['pilots'].pop()
            d.write_json(build / 'diagnostic-fir-smoke.json', smoke)
            with self.assertRaisesRegex(ValueError, 'matrix incomplete'): d.validate_hosted_smoke(build, 'fir', proof)

    def test_smoke_counter_coverage_and_trace_output_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            proof = {'mode': 'spine', 'images': {}}
            pilots = [dict(workload=name, width=width, padding=padding, label=arm, round=round_id)
                      for name, width, padding, arms in d.image_plan('spine', True)
                      for arm in arms for round_id in range(d.PILOTS)]
            counters = [dict(workload='spine', width=width, label=arm, round=round_id,
                             instrumented=True, startup_trace_requested=False)
                        for width in (1, 2) for arm in ('eager-counter', 'requested-counter')
                        for round_id in range(d.PILOTS)]
            smoke = dict(status='complete', smoke=True, construction=proof, pilots=pilots,
                         decisive=[], counter_decisive=[], counter_pilots=counters)
            d.write_json(build / 'diagnostic-spine-smoke.json', smoke)
            self.assertEqual(d.validate_hosted_smoke(build, 'spine', proof), smoke)
            for mutate, message in ((lambda row: row['counter_pilots'].pop(), 'matrix incomplete'),
                                    (lambda row: row['counter_pilots'][0].update(startup_trace_requested=True), 'flags differ')):
                wrong = copy.deepcopy(smoke)
                mutate(wrong)
                d.write_json(build / 'diagnostic-spine-smoke.json', wrong)
                with self.assertRaisesRegex(ValueError, message): d.validate_hosted_smoke(build, 'spine', proof)

    def test_void_twins_retain_raw_direction_without_claiming_support(self):
        rows = []
        for padding in d.PADDING:
            for _, width, _, arms in d.image_plan('fir')[:1]:
                for round_id in range(d.DECISIVE['fir']):
                    for arm in arms:
                        row = sample(width=width)
                        row.update(workload='fir', label=arm, round=round_id, padding=padding)
                        row['samples'][1]['wall_ns'] = (200 if arm.startswith('canonical') else 100) * (2 if arm.endswith('-twin') else 1)
                        rows.append(row)
        with patch.object(d, 'median_interval', side_effect=lambda values: (min(values), max(values))):
            reports = d.reduce_diagnostic('fir', rows, [], {})
        self.assertTrue(all(report['causal_status'].startswith('void') for report in reports))
        self.assertEqual(reports[0]['canonical_over_legacy']['direct']['raw_status'], 'supported')
        self.assertEqual(reports[0]['canonical_over_legacy']['direct']['status'], 'void')
        self.assertTrue(reports[0]['same_job_layout_floor']['canonical']['status'].startswith('void'))
        self.assertEqual(reports[0]['query_residue_claim_rejected'], {'canonical': None, 'legacy': None})
        self.assertEqual(set(reports[0]['raw_query_residue_claim_rejected']), {'canonical', 'legacy'})
