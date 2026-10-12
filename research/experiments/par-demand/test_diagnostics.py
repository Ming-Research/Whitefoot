"""Diagnostic contracts; synthetic data only, no compiler/native invocation."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import diagnostics as d
from identity import Instruction, assembly_edges


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


# Hosted 64/legacy-direct wf__par_seq_filter at 0x45c0..0x460d, as objdump printed it.
FOUR_TAPS = [
    (0x45c0, 'movsd', '-0x18(%r9),%xmm1'), (0x45c6, 'mulsd', '(%rsi,%r8,8),%xmm1'),
    (0x45cc, 'movsd', '-0x10(%r9),%xmm2'), (0x45d2, 'addsd', '%xmm0,%xmm1'),
    (0x45d6, 'mulsd', '-0x8(%rsi,%r8,8),%xmm2'), (0x45dd, 'addsd', '%xmm1,%xmm2'),
    (0x45e1, 'movsd', '-0x8(%r9),%xmm1'), (0x45e7, 'mulsd', '-0x10(%rsi,%r8,8),%xmm1'),
    (0x45ee, 'addsd', '%xmm2,%xmm1'), (0x45f2, 'movsd', '(%r9),%xmm0'),
    (0x45f7, 'mulsd', '-0x18(%rsi,%r8,8),%xmm0'), (0x45fe, 'addsd', '%xmm1,%xmm0'),
    (0x4602, 'add', '$0xfffffffffffffffc,%r8'), (0x4606, 'add', '$0x20,%r9'),
    (0x460a, 'cmp', '%r8,%rcx'), (0x460d, 'jne', '45c0 <wf__par_seq_filter+0x100>')]


def fir_loops(taps):
    """The output loop around one tap body and its one-tap remainder loop."""
    rows = [(0x4570, 'jmp', '4593 <f>'),
            (0x4580, 'movsd', '%xmm0,0x8(%rax,%r15,8)'), (0x4587, 'inc', '%r15'),
            (0x458e, 'cmp', '%rbp,%r15'), (0x4591, 'je', '4650 <f>'),
            (0x4593, 'xorpd', '%xmm0,%xmm0'), (0x4597, 'cmp', '$0x3,%r13'), (0x459b, 'jae', '45b0 <f>'),
            (0x459d, 'xor', '%r8d,%r8d'), (0x45a0, 'test', '%rdx,%rdx'), (0x45a3, 'jne', '461b <f>'),
            (0x45a5, 'jmp', '4580 <f>'), (0x45b0, 'mov', '%rdi,%r9'), *taps,
            (0x460f, 'neg', '%r8'), (0x4612, 'test', '%rdx,%rdx'), (0x4615, 'je', '4580 <f>'),
            (0x461b, 'lea', '(%rbx,%r8,8),%r9'),
            (0x4630, 'movsd', '(%r9,%r8,8),%xmm1'), (0x4636, 'mulsd', '(%r10),%xmm1'),
            (0x463b, 'addsd', '%xmm1,%xmm0'), (0x463f, 'inc', '%r8'), (0x4646, 'cmp', '%r8,%rdx'),
            (0x4649, 'jne', '4630 <f>'), (0x464b, 'jmp', '4580 <f>'), (0x4650, 'ret', '')]
    return [Instruction(*row) for row in rows]


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
        functions = {'selected': [Instruction(0x100, 'ret', '')], 'wf__par_seq_unused': fir_loops(FOUR_TAPS)}
        self.assertTrue(d.selected_call_shapes(functions, 'wf__par_seq_unused'))
        with self.assertRaisesRegex(ValueError, 'did not survive'): d.selected_call_shapes(functions, 'selected')
        functions['selected'] = [Instruction(0x100, 'vfmadd132sd', '%xmm0,%xmm1,%xmm2'), Instruction(0x104, 'ret', '')]
        with self.assertRaisesRegex(ValueError, 'FMA'): d.selected_call_shapes(functions, 'selected')

    def test_four_taps_nested_in_output_loop_with_remainder_are_recognized(self):
        # Hosted run 38198881733 (Ubuntu clang 18.1.3), 64/legacy-direct
        # wf__par_seq_filter: the four-tap loop sits inside the output loop
        # beside a one-tap remainder loop, so the whole-function SCC holds ten
        # multiply/add operations and the earlier check refused a present loop.
        [loop] = d.selected_call_shapes({'wf__par_seq_filter': fir_loops(FOUR_TAPS)}, 'wf__par_seq_filter')
        self.assertEqual((loop['instruction_addresses'][0], loop['instruction_addresses'][-1]), (0x45c0, 0x460d))
        self.assertEqual(len(loop['instruction_addresses']), 16)
        self.assertEqual(loop['accumulator'], '%xmm0')
        self.assertEqual([tap[1] for tap in loop['taps']],
                         ['(%rsi,%r8,8)', '-0x8(%rsi,%r8,8)', '-0x10(%rsi,%r8,8)', '-0x18(%rsi,%r8,8)'])

    def test_absent_or_unordered_taps_still_block_construction(self):
        one_tap = [line for line in FOUR_TAPS if line[0] < 0x45d6 or line[0] >= 0x4602]
        one_tap = [(0x45d2, 'addsd', '%xmm1,%xmm0') if line[0] == 0x45d2 else line for line in one_tap]
        reassociated = [(0x45c0, 'movsd', '-0x18(%r9),%xmm1'), (0x45c6, 'mulsd', '(%rsi,%r8,8),%xmm1'),
                        (0x45cc, 'movsd', '-0x10(%r9),%xmm2'), (0x45d2, 'mulsd', '-0x8(%rsi,%r8,8),%xmm2'),
                        (0x45d6, 'addsd', '%xmm2,%xmm1'), (0x45da, 'movsd', '-0x8(%r9),%xmm2'),
                        (0x45e0, 'mulsd', '-0x10(%rsi,%r8,8),%xmm2'), (0x45e6, 'movsd', '(%r9),%xmm3'),
                        (0x45ea, 'mulsd', '-0x18(%rsi,%r8,8),%xmm3'), (0x45f0, 'addsd', '%xmm3,%xmm2'),
                        (0x45f4, 'addsd', '%xmm2,%xmm1'), (0x45f8, 'addsd', '%xmm1,%xmm0'), *FOUR_TAPS[12:]]
        not_carried = [(0x45fe, 'addsd', '%xmm1,%xmm3') if line[0] == 0x45fe else line for line in FOUR_TAPS]
        mnemonics_only = [(0x45c0 + at * 4, operation, '%xmm1,%xmm0')
                          for at, operation in enumerate(['mulsd', 'addsd'] * 4)] + [FOUR_TAPS[-1]]
        for body in (one_tap, reassociated, not_carried, mnemonics_only):
            with self.subTest(body=body):
                instructions = fir_loops(body)
                # The body is still found as an innermost loop; only its dataflow refuses it.
                [loop] = [loop for loop in d.innermost_loops(assembly_edges(instructions))
                          if instructions[loop[0]].address == 0x45c0]
                self.assertIsNone(d.ordered_tap_chain(instructions, loop))
                with self.assertRaisesRegex(ValueError, 'did not survive'):
                    d.selected_call_shapes({'wf__par_seq_filter': instructions}, 'wf__par_seq_filter')

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
