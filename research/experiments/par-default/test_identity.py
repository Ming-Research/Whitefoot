"""Hosted identity checks. No compiler fixture, timing or build runs here."""
import unittest
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import identity
from identity import (IdentityError, Instruction, normalized, first_difference,
                      parse_disassembly, project_pool, entry_shell, computation_root,
                      dispatch_proof, parse_ir, ElfImage, canonical_symbol,
                      classify_construction, latched_writer_evidence, entry_adapter_evidence,
                      validate_admission, instruction_snapshot)


def insns(rows):
    return [Instruction(0x100 + index * 4, *row) for index, row in enumerate(rows)]


class ConstructionAttributionTests(unittest.TestCase):
    header = 'define i64 @wf_bench_fixture(i64 %value) {'
    seq_adapter = '''define i64 @wf_bench_fixture(i64 %value) {
  %r = call i64 @wf_body(i64 %value)
  ret i64 %r
}
'''
    demand_adapter = '''define i64 @wf_bench_fixture(i64 %value) {
  %pool = call i32 @wf__par_pool_active()
  %active = icmp ne i32 %pool, 0
  br i1 %active, label %par, label %seq
par:
  %a = call i64 @wf_body(i64 %value)
  br label %join
seq:
  %b = call i64 @wf__par_seq_body(i64 %value)
  br label %join
join:
  %r = phi i64 [ %a, %par ], [ %b, %seq ]
  ret i64 %r
}
define i64 @wf__par_seq_body(i64 %value) {
  %r = add i64 %value, 7
  ret i64 %r
}
'''

    def adapter_provenance(self, demand=None):
        evidence = entry_adapter_evidence(parse_ir(self.seq_adapter),
            parse_ir(self.demand_adapter if demand is None else demand),
            'wf_bench_fixture', self.header, self.header)
        if evidence is not None:
            closure, declarations = identity.adapter_ir_inventory(parse_ir(self.demand_adapter),
                evidence['inactive_ir_root'], set())
            evidence['canonical_ir_closure'] = closure
            evidence['canonical_ir_declarations'] = sorted(declarations)
            machine = insns([('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'),
                ('jne', '114 <wf_bench_fixture+0x14>'), ('call', '600 <wf__par_seq_body>'),
                ('ret', ''), ('call', '700 <wf_body>'), ('ret', '')])
            selected, _, count, _ = project_pool(machine, False)
            evidence['optimized_pool_projection'] = {'pool_decisions_per_timed_call': count,
                'outside_loops': True, 'instructions': instruction_snapshot(machine),
                'selected_instruction_addresses': [machine[at].address for at in sorted(selected)],
                'state_preservation': 'unverified; CFG selection does not establish ABI preservation'}
        return evidence

    def record(self, identical=True):
        return {'status': 'fail', 'covered_widths': [1], 'comparisons': [
            {'function': 'wf__par_seq_body', 'seq_function': 'wf_body',
             'seq_instructions': ['lea 7(%rdi),%rax', 'ret'],
             'demand_instructions': ['lea 7(%rdi),%rax' if identical else 'lea 8(%rdi),%rax', 'ret'],
             'role': 'reachable sequential body/helper', 'identical': identical},
            {'function': 'wf_bench_fixture', 'seq_instructions': ['push %rax'],
             'demand_instructions': ['push %saved0.0'], 'identical': False,
             'role': 'conservative actual W1 wrapper CFG projection; all nondecision instructions retained'}],
            'dispatch': {'pool_decisions_per_timed_call': 1, 'outside_loops': True, 'regions': []},
            'failures': [{'workload': 'fixture', 'function': 'wf_bench_fixture',
                          'reason': 'normalized timed sequential wrapper differs',
                          'instruction': 1, 'seq': 'push %rax', 'demand': 'push %saved0.0'}]}

    def test_identical_body_closure_alone_does_not_admit_an_entry_adapter_difference(self):
        record = self.record()
        classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['status'], 'fail')
        failure = record['failures'][0]
        self.assertEqual(failure['category'], 'ordinary')
        self.assertEqual((failure['instruction'], failure['seq'], failure['demand']),
                         (1, 'push %rax', 'push %saved0.0'))
        with self.assertRaises(IdentityError):
            validate_admission(record)

    def test_changed_missing_or_unselected_body_cannot_qualify_adapter(self):
        for changed, roots, missing in [(True, {'wf__par_seq_body'}, False),
                                        (False, set(), False), (False, {'wf__par_seq_body'}, True)]:
            record = self.record(not changed)
            if missing:
                record['failures'].append({'function': 'wf_missing', 'reason': 'missing corresponding sequential disassembly function'})
            classify_construction(record, 'wf_bench_fixture', roots, {})
            self.assertNotEqual(record['computation_status'], 'pass')
            self.assertEqual(record['failures'][0]['category'], 'ordinary')
            self.assertEqual(record['status'], 'fail')

    def test_inventory_identity_does_not_prove_unknown_timed_selection(self):
        record = self.record()
        record['failures'][0]['reason'] = 'wrapper CFG projection: unsupported machine decision'
        classify_construction(record, 'wf_bench_fixture', set(), {}, {'wf__par_seq_body'})
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['timed_selection_status'], 'unverified')
        self.assertEqual(record['failures'][0]['category'], 'ordinary')
        self.assertEqual(record['status'], 'fail')
        self.assertEqual(record['computation_scope']['selected_sequential_roots'], [])
        self.assertEqual(record['computation_scope']['inventoried_sequential_roots'], ['wf__par_seq_body'])

    def test_complete_adapter_boundary_is_admitted_and_replayed(self):
        evidence = self.adapter_provenance()
        self.assertEqual(evidence['inactive_ir_root'], 'wf__par_seq_body')
        record = self.record()
        record['entry_adapter_provenance'] = evidence
        classify_construction(record, 'wf_bench_fixture', set(), {}, {'wf__par_seq_body'})
        self.assertEqual(record['failures'][0]['category'], 'entry-adapter')
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['timed_selection_status'], 'unverified')
        self.assertEqual(record['status'], 'pass')
        self.assertEqual(validate_admission(record)['category_counts'], {'latched-writer': 0, 'entry-adapter': 1, 'entry-version': 0})
        self.assertEqual(record['computation_scope']['selected_sequential_roots'], [])
        for projection, root in [({}, 'wf__par_seq_body'),
                                 ({'pool_decisions_per_timed_call': 0}, 'wf__par_seq_body'),
                                 ({'pool_decisions_per_timed_call': 1}, 'wf__par_seq_unrelated')]:
            wrong = self.record()
            wrong['entry_adapter_provenance'] = {**evidence, 'optimized_pool_projection': projection,
                                                  'inactive_ir_root': root}
            classify_construction(wrong, 'wf_bench_fixture', set(), {}, {'wf__par_seq_body'})
            self.assertEqual(wrong['failures'][0]['category'], 'ordinary')
            self.assertEqual(wrong['status'], 'fail')
        missing = self.record()
        missing['comparisons'] = []
        missing['entry_adapter_provenance'] = evidence
        classify_construction(missing, 'wf_bench_fixture', set(), {}, {'wf__par_seq_body'})
        self.assertEqual(missing['failures'][0]['category'], 'ordinary')

    def test_adapter_provenance_preserves_call_arguments_join_and_output(self):
        for changed in [self.demand_adapter.replace('@wf__par_seq_body(', '@wf__par_seq_other('),
                        self.demand_adapter.replace('@wf__par_seq_body(i64 %value)', '@wf__par_seq_body(i64 9)'),
                        self.demand_adapter.replace('[ %b, %seq ]', '[ %a, %seq ]'),
                        self.demand_adapter.replace('ret i64 %r', 'ret i64 9'),
                        self.demand_adapter.replace('label %par, label %seq', 'label %seq, label %par')]:
            self.assertIsNone(self.adapter_provenance(changed))
        pointer_header = 'define void @wf_bench_fixture(ptr %value, ptr %out) {'
        pointer_seq = self.seq_adapter.replace(self.header, pointer_header).replace('call i64', 'call ptr').replace('i64 %value', 'ptr %value').replace('ret i64 %r', 'store ptr %r, ptr %out\n  ret void')
        pointer_demand = self.demand_adapter.replace(self.header, pointer_header).replace('call i64', 'call ptr').replace('i64 %value', 'ptr %value').replace('phi i64', 'phi ptr').replace('ret i64 %r', 'store ptr %r, ptr %out\n  ret void')
        self.assertIsNotNone(entry_adapter_evidence(parse_ir(pointer_seq), parse_ir(pointer_demand),
            'wf_bench_fixture', pointer_header, pointer_header))
        self.assertIsNone(entry_adapter_evidence(parse_ir(pointer_seq),
            parse_ir(pointer_demand.replace('ptr %out', 'ptr %different_out')),
            'wf_bench_fixture', pointer_header, pointer_header))
        self.assertIsNone(entry_adapter_evidence(parse_ir(self.seq_adapter), parse_ir(self.demand_adapter),
            'wf_bench_fixture', self.header, self.header.replace('i64 %value', 'i32 %value')))

    def test_adapter_admission_replays_arguments_outputs_pool_and_root_closure(self):
        evidence = self.adapter_provenance()
        for mutate in [
            lambda row: row['demand_blocks']['seq'].__setitem__(0, '%b = call i64 @wf__par_seq_body(i64 9)'),
            lambda row: row['demand_blocks']['join'].__setitem__(1, 'ret i64 9'),
            lambda row: row['optimized_pool_projection'].__setitem__('pool_decisions_per_timed_call', 0),
            lambda row: row['optimized_pool_projection'].__setitem__('outside_loops', False),
            lambda row: row['optimized_pool_projection']['instructions'].__setitem__(0,
                {'address': 0x100, 'mnemonic': 'nop', 'operands': ''}),
            lambda row: row.pop('canonical_ir_closure'),
        ]:
            wrong = self.record()
            wrong['entry_adapter_provenance'] = copy.deepcopy(evidence)
            mutate(wrong['entry_adapter_provenance'])
            classify_construction(wrong, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
            self.assertEqual(wrong['status'], 'fail')
            self.assertEqual(wrong['failures'][0]['category'], 'ordinary')
            with self.assertRaises(IdentityError):
                validate_admission(wrong)
        for wrong_target in ('wf__par_seq_unrelated', 'wf_body'):
            wrong = self.record()
            wrong['comparisons'].append({**wrong['comparisons'][0], 'function': wrong_target,
                'seq_function': canonical_symbol(wrong_target)})
            wrong['entry_adapter_provenance'] = copy.deepcopy(evidence)
            wrong['entry_adapter_provenance']['optimized_pool_projection']['instructions'][3]['operands'] = f'600 <{wrong_target}>'
            classify_construction(wrong, 'wf_bench_fixture', {'wf__par_seq_body', wrong_target}, {})
            self.assertEqual(wrong['failures'][0]['category'], 'ordinary')
            self.assertEqual(wrong['status'], 'fail')

    def synthesized_fill(self):
        # Raw scalar stores may become a native memset when LLVM inlines the
        # canonical entrance; no memory intrinsic declaration is present.
        record = self.record()
        evidence = self.adapter_provenance()
        evidence['canonical_ir_closure'] = parse_ir('''define i64 @wf__par_seq_body(i64 %value) {
  %dst = inttoptr i64 %value to ptr
  br label %fill
fill:
  %i = phi i64 [ 0, %entry ], [ %next, %fill ]
  %p = getelementptr i8, ptr %dst, i64 %i
  store i8 0, ptr %p
  %next = add i64 %i, 1
  %done = icmp eq i64 %next, 8
  br i1 %done, label %exit, label %fill
exit:
  ret i64 %value
}
''')
        evidence['optimized_pool_projection']['instructions'][3]['operands'] = '600 <memset>'
        record['entry_adapter_provenance'] = evidence
        record['comparisons'][0].update(seq_instructions=['call memset', 'ret'],
                                         demand_instructions=['call memset', 'ret'])
        return record

    def test_synthesized_memory_call_requires_identical_exact_canonical_machine_closure(self):
        record = self.synthesized_fill()
        evidence = record['entry_adapter_provenance']
        roots = {'wf__par_seq_body'}
        row = record['comparisons'][0]
        self.assertFalse(identity.verified_adapter_evidence('wf_bench_fixture', evidence, roots))
        self.assertTrue(identity.verified_adapter_evidence('wf_bench_fixture', evidence, roots, [row]))
        classify_construction(record, 'wf_bench_fixture', roots, {})
        self.assertEqual(record['failures'][0]['category'], 'entry-adapter')
        self.assertEqual(validate_admission(record)['category_counts']['entry-adapter'], 1)
        for change in ('different', 'missing', 'unrelated', 'wrong_counterpart'):
            changed = copy.deepcopy(row)
            if change == 'different': changed['demand_instructions'] = ['call memcpy', 'ret']
            if change == 'missing': changed.update(seq_instructions=['ret'], demand_instructions=['ret'])
            if change == 'unrelated': changed.update(function='wf__par_seq_other', seq_function='wf_other')
            if change == 'wrong_counterpart': changed['seq_function'] = 'wf_other'
            with self.subTest(change=change):
                self.assertFalse(identity.verified_adapter_evidence('wf_bench_fixture', evidence, roots | {'wf__par_seq_other'}, [changed]))
        forged = copy.deepcopy(record)
        forged['comparisons'][0].update(seq_instructions=['ret'], demand_instructions=['ret'])
        with self.assertRaises(IdentityError): validate_admission(forged)

    def test_unchanged_computation_requires_concrete_dispatch_and_cheap_input_proof(self):
        record = self.record()
        record['failures'] = []
        record['actual_entry'] = {'W1': {'computation': 'wf__par_seq_body', 'outside_loops': True,
            'decision_trace': ['pool-active']}}
        classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        validate_admission(record)
        cheap = copy.deepcopy(record)
        cheap['covered_widths'] = [1, 2, 4, 8]
        cheap['dispatch']['regions'] = [{'function': 'wf_body', 'decisions_per_active_timed_call': 1,
            'assembly_decisions_per_cheap_timed_call': 1, 'outside_loops': True}]
        cheap['versioned_region_inventory'] = ['wf_body']
        cheap['actual_entry']['W2/W4/W8'] = {'computation': 'wf__par_seq_body', 'outside_loops': True,
            'decision_trace': ['pool-active', 'cheap-region'], 'inputs': {'rdi': 1, 'rsi': 3}}
        classify_construction(cheap, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        validate_admission(cheap)
        for mutate in [lambda row: row['actual_entry']['W2/W4/W8'].pop('inputs'),
                       lambda row: row['dispatch']['regions'][0].__setitem__('outside_loops', False),
                       lambda row: row['dispatch']['regions'][0].__setitem__('assembly_decisions_per_cheap_timed_call', 2),
                       lambda row: row['covered_widths'].remove(4),
                       lambda row: row.__setitem__('versioned_region_inventory', ['wf_body', 'wf_missing'])]:
            wrong = copy.deepcopy(cheap)
            mutate(wrong)
            classify_construction(wrong, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
            self.assertEqual(wrong['status'], 'fail')
            with self.assertRaises(IdentityError):
                validate_admission(wrong)

    def test_forged_status_category_counts_or_body_identity_never_admit(self):
        valid = self.record()
        valid['entry_adapter_provenance'] = self.adapter_provenance()
        classify_construction(valid, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        validate_admission(valid)
        for mutate in [
            lambda row: row['failures'][0].__setitem__('category', 'latched-writer'),
            lambda row: row['admission']['category_counts'].__setitem__('entry-adapter', 0),
            lambda row: row['comparisons'][0]['demand_instructions'].__setitem__(0, 'lea 9(%rdi),%rax'),
            lambda row: row['dispatch'].__setitem__('outside_loops', False),
            lambda row: row['failures'][0].pop('seq'),
            lambda row: row['admission_inputs'].__setitem__('inventory_roots', ['wf__par_seq_missing']),
        ]:
            forged = copy.deepcopy(valid)
            mutate(forged)
            with self.assertRaises(IdentityError):
                validate_admission(forged)
        ordinary = self.record()
        classify_construction(ordinary, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        ordinary['status'] = ordinary['admission']['status'] = 'pass'
        ordinary['failures'][0]['category'] = 'entry-adapter'
        with self.assertRaises(IdentityError):
            validate_admission(ordinary)

    def test_actual_inlined_wrapper_retains_difference_and_unproved_state(self):
        seq = '''0000000000000100 <wf_bench_fixture>:
 100: lea 7(%rdi),%rax
 104: ret
0000000000000300 <wf_body>:
 300: lea 7(%rdi),%rax
 304: ret
'''
        demand = '''0000000000000100 <wf_bench_fixture>:
 100: push %rbx
 104: mov %rdi,%rbx
 108: call 500 <wf__par_pool_active>
 10c: test %eax,%eax
 110: je 120 <wf_bench_fixture+0x20>
 114: mov $1,%eax
 118: jmp 124 <wf_bench_fixture+0x24>
 120: lea 7(%rbx),%rax
 124: pop %rbx
 128: ret
0000000000000600 <wf__par_seq_body>:
 600: lea 7(%rdi),%rax
 604: ret
'''
        class Image:
            def __init__(self, _): pass
            def bound(self, functions):
                self.functions = functions
                return functions
            def code_identity(self, address, spelling):
                return canonical_symbol(self.functions.resolve(address, spelling))
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            for arm, text, assembly in [('seq', self.seq_adapter, seq),
                                        ('demand', self.demand_adapter, demand)]:
                (build / arm).mkdir()
                (build / arm / 'spine.ll').write_text(text)
                (build / arm / 'spine.disassembly').write_text(assembly)
            with patch.object(identity, 'ElfImage', Image):
                record = identity.check_workload('spine', build)
            self.assertEqual((record['status'], record['computation_status']), ('pass', 'pass'))
            validate_admission(record)
            self.assertEqual(record['timed_selection_status'], 'unverified')
            self.assertIn('unsupported entry-shell instruction lea', record['entry_state_proof']['reason'])
            self.assertEqual(record['computation_scope']['selected_sequential_roots'], [])
            self.assertEqual(record['entry_adapter_provenance']['inactive_ir_root'], 'wf__par_seq_body')
            failure = record['failures'][0]
            self.assertEqual((failure['category'], failure['instruction']), ('entry-adapter', 1))

    def writer(self):
        common = {'write.loop': ['%written = call i64 @wf_resource_write(ptr %message, i64 %length)',
                                 'br label %abort'],
                  'abort': ['call void @abort()', 'unreachable']}
        seq = {'entry': ['br label %write.loop'], **common}
        demand = {'entry': ['%latch = call ptr @wf__floor_record_latch()',
                            '%acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst',
                            '%won = extractvalue { i32, i1 } %acquired, 1',
                            'br i1 %won, label %write.loop, label %park'], **common,
                  'park': ['%parked = load volatile i32, ptr %latch, align 4', 'br label %park']}
        machine = insns([('call', '500 <wf__floor_record_latch>'),
                         ('lock', 'cmpxchg %edx,(%rax)'),
                         ('call', '600 <wf_resource_write>'), ('call', '700 <abort@plt>'), ('ret', '')])
        original_machine = insns([('call', '600 <wf_resource_write>'), ('call', '700 <abort@plt>'), ('ret', '')])
        return seq, demand, machine, original_machine

    write_helper = '''declare i64 @write(i32, ptr, i64)
declare ptr @__errno_location()
define private i64 @wf_resource_write(ptr %bytes, i64 %length) #0 align 64 {
entry:
  br label %write
write:
  %written = call i64 @write(i32 2, ptr %bytes, i64 %length)
  %failed = icmp slt i64 %written, 0
  br i1 %failed, label %error, label %done
error:
  %errno = call ptr @__errno_location()
  %code = load i32, ptr %errno, align 4
  %interrupted = icmp eq i32 %code, 4
  br i1 %interrupted, label %write, label %done
done:
  ret i64 %written
}
'''

    def writer_evidence(self, seq, demand, machine, original_machine,
                        helper=None, function='wf_resource_record_abort'):
        changed_helper = self.write_helper if helper is None else helper
        return latched_writer_evidence(function,
            {'wf_resource_record_abort': seq, **parse_ir(self.write_helper)},
            {function: demand, **parse_ir(changed_helper)}, machine, None, original_machine, None,
            identity.ir_function_headers(self.write_helper), identity.ir_function_headers(changed_helper))

    def test_inlined_writer_requires_unchanged_helper_closure_and_native_write_frontier(self):
        seq, demand, _, _ = self.writer()
        original_machine = insns([('call', '800 <__errno_location@plt>'),
            ('call', '900 <write@plt>'), ('call', '700 <abort@plt>')])
        machine = insns([('call', '500 <wf__floor_record_latch>'), ('lock', 'cmpxchg %edx,(%rax)'),
            ('call', '800 <__errno_location@plt>'), ('call', '900 <write@plt>'), ('call', '700 <abort@plt>')])
        evidence = self.writer_evidence(seq, demand, machine, original_machine)
        self.assertIsNotNone(evidence)
        self.assertTrue(identity.verified_writer_evidence('wf_resource_record_abort', evidence))
        self.assertNotIn('wf_resource_write', evidence['machine']['original_calls'])
        self.assertEqual(evidence['machine']['original_calls'], ['__errno_location', 'abort', 'write'])
        for helper in [self.write_helper.replace('icmp eq i32 %code, 4', 'icmp eq i32 %code, 5'),
                       self.write_helper.replace('i32 2, ptr %bytes', 'i32 1, ptr %bytes'),
                       self.write_helper.replace('ptr %bytes, i64 %length) #0', 'ptr %bytes, i32 %length) #0'),
                       self.write_helper.replace('declare ptr @__errno_location()', '')]:
            self.assertIsNone(self.writer_evidence(seq, demand, machine, original_machine, helper))
        forged = copy.deepcopy(evidence)
        forged['helpers']['demand_blocks']['wf_resource_write']['error'][2] = '%interrupted = icmp eq i32 %code, 5'
        self.assertFalse(identity.verified_writer_evidence('wf_resource_record_abort', forged))
        for changed in [machine[:3] + machine[4:],
                        machine + [Instruction(0x200, 'call', '900 <write@plt>')],
                        machine[:3] + [Instruction(0x10c, 'call', 'a00 <wrong_write@plt>')] + machine[4:]]:
            self.assertIsNone(self.writer_evidence(seq, demand, changed, original_machine))
        # Matching inventories alone cannot excuse deleting the actual write
        # from both images after the private helper was inlined.
        self.assertIsNone(self.writer_evidence(seq, demand, machine[:3] + machine[4:],
                                              original_machine[:1] + original_machine[2:]))

    def test_latched_writer_requires_both_exact_ir_delta_and_actual_atomic(self):
        seq, demand, machine, original_machine = self.writer()
        evidence = self.writer_evidence(seq, demand, machine, original_machine)
        self.assertIsNotNone(evidence)
        record = self.record()
        record['entry_adapter_provenance'] = self.adapter_provenance()
        record['comparisons'].append({'function': 'wf_resource_record_abort',
                                     'seq_function': 'wf_resource_record_abort',
                                     'seq_instructions': ['push', 'mov', 'sub', 'call @wf_resource_write'],
                                     'demand_instructions': ['push', 'mov', 'sub', 'lock cmpxchg'],
                                     'role': 'reachable sequential body/helper', 'identical': False})
        record['failures'].append({'function': 'wf_resource_record_abort',
                                  'reason': 'normalized sequential body differs', 'instruction': 4,
                                  'seq': 'call @wf_resource_write', 'demand': 'lock cmpxchg'})
        classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'},
                              {'wf_resource_record_abort': evidence})
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['failures'][1]['category'], 'latched-writer')
        self.assertEqual(record['status'], 'pass')
        validate_admission(record)
        for mutate in [
            lambda row: row['demand_blocks']['write.loop'].__setitem__(0, 'call void @different_write()'),
            lambda row: row['demand_blocks']['entry'].__setitem__(1, '%acquired = cmpxchg ptr %latch, i32 0, i32 2 seq_cst seq_cst'),
            lambda row: row['machine']['demand_instructions'].__setitem__(1,
                {'address': 0x104, 'mnemonic': 'nop', 'operands': ''}),
            lambda row: row['machine']['demand_instructions'].__setitem__(0,
                {'address': 0x100, 'mnemonic': 'nop', 'operands': ''}),
            lambda row: row['machine']['demand_instructions'].append(
                {'address': 0x200, 'mnemonic': 'call', 'operands': '500 <wf__floor_record_latch>'}),
            lambda row: row['machine']['demand_instructions'].append(
                {'address': 0x200, 'mnemonic': 'lock', 'operands': 'cmpxchg %edx,(%rax)'}),
        ]:
            wrong = copy.deepcopy(record)
            changed_evidence = copy.deepcopy(evidence)
            mutate(changed_evidence)
            classify_construction(wrong, 'wf_bench_fixture', {'wf__par_seq_body'},
                                  {'wf_resource_record_abort': changed_evidence})
            self.assertEqual(wrong['status'], 'fail')
            self.assertEqual(wrong['failures'][1]['category'], 'ordinary')
            with self.assertRaises(IdentityError):
                validate_admission(wrong)
        changed = {**demand, 'write.loop': ['call void @different_write()', 'br label %abort']}
        for candidate, actual in [(changed, machine), (demand, machine[:1]), (demand, insns([('lock', 'cmpxchg %edx,(%rax)'), ('ret', '')]))]:
            self.assertIsNone(self.writer_evidence(seq, candidate, actual, original_machine))
        self.assertIsNone(self.writer_evidence(seq, demand, machine, original_machine, function='wf_unrelated_writer'))

    def test_cli_retains_known_difference_first_instruction_and_nonzero_exit(self):
        def checked(name, _):
            record = self.record()
            record['failures'][0]['workload'] = name
            classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
            # The main-level small_split coverage assertion remains satisfied;
            # this case isolates whether attribution could admit a failure.
            record['dispatch']['regions'] = [{'function': 'wf_body'}]
            return record
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            output = io.StringIO()
            with patch('sys.argv', ['identity.py', '--build', directory]), \
                 patch.object(identity, 'check_workload', side_effect=checked), \
                 contextlib.redirect_stdout(output), self.assertRaises(SystemExit) as exit_status:
                identity.main()
            self.assertEqual(exit_status.exception.code, 1)
            result = json.loads((build / 'sequential-identity.json').read_text())
            self.assertEqual((result['status'], result['computation_status']), ('fail', 'pass'))
            first = result['workloads']['stencil']['failures'][0]
            self.assertEqual((first['category'], first['instruction']), ('ordinary', 1))
            self.assertIn('[ordinary]', output.getvalue())
            self.assertIn("instruction 1: seq='push %rax'", output.getvalue())


class NormalizationTests(unittest.TestCase):
    def test_canonical_helper_namespace_restores_the_complete_original_symbol(self):
        self.assertEqual(canonical_symbol('wf__par_seq.wf.drop.run.cell'), 'wf.drop.run.cell')
        self.assertEqual(canonical_symbol('.Lwf__par_seq.wf.drop.run.cell'), 'wf.drop.run.cell')
        self.assertEqual(canonical_symbol('.Lwf.drop.run.cell'), 'wf.drop.run.cell')
        original = insns([('call', '200 <wf.drop.run.cell>')])
        clone = insns([('call', '300 <wf__par_seq.wf.drop.run.cell>')])
        unrelated = insns([('call', '300 <wf__par_seq.wf.drop.run.cell_long>')])
        self.assertIsNone(first_difference(normalized(original), normalized(clone)))
        self.assertIsNotNone(first_difference(normalized(original), normalized(unrelated)))

    def test_consistent_saved_register_renaming_and_local_addresses(self):
        a = insns([('push', '%rbx'), ('mov', '%rdi,%rbx'), ('add', '$0x7,%rbx'), ('mov', '%rbx,%rax'), ('pop', '%rbx'), ('ret', '')])
        b = insns([('push', '%r12'), ('mov', '%rdi,%r12'), ('add', '$0x7,%r12'), ('mov', '%r12,%rax'), ('pop', '%r12'), ('ret', '')])
        self.assertIsNone(first_difference(normalized(a), normalized(b)))

    def test_abi_argument_swap_is_not_normalized(self):
        a = insns([('mov', '%rdi,%rax'), ('ret', '')])
        b = insns([('mov', '%rsi,%rax'), ('ret', '')])
        self.assertEqual(first_difference(normalized(a), normalized(b))['instruction'], 1)

    def test_constant_offset_and_branch_topology_mutations(self):
        a = insns([('add', '$0x7,%rax'), ('mov', '8(%rdi),%rax'), ('jne', '100 <body>'), ('ret', '')])
        for index, row in [(0, ('add', '$0x9,%rax')), (1, ('mov', '16(%rdi),%rax')), (2, ('jne', '10c <body+0xc>'))]:
            b = list(a)
            b[index] = Instruction(a[index].address, *row)
            self.assertEqual(first_difference(normalized(a), normalized(b))['instruction'], index + 1)

    def test_rip_data_requires_bytes_and_detects_same_named_changed_constant(self):
        instruction = insns([('movsd', '0x100(%rip),%xmm0 # 208 <constant>')])
        with self.assertRaisesRegex(IdentityError, 'ELF data evidence'):
            normalized(instruction)
        class Image:
            def __init__(self, value): self.value = value
            def reference_identity(self, *_): return 'readonly:' + self.value
        self.assertIsNotNone(first_difference(normalized(instruction, image=Image('000000000000f03f')),
                                             normalized(instruction, image=Image('0000000000000040'))))

    def test_equal_readonly_contents_do_not_erase_address_alias_identity(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b'same' + b'same'
        image.sections = [(0, 1, 2, 0x200, 0, 8, 0, 0, 1, 0)]
        image.symbols = {'A': (0x200, 4, 1), 'B': (0x204, 4, 1)}
        first = insns([('lea', '0x100(%rip),%rax # 200 <A>')])
        second = insns([('lea', '0x104(%rip),%rax # 204 <B>')])
        self.assertIsNotNone(first_difference(normalized(first, image=image),
                                             normalized(second, image=image)))

    def test_function_sizes_exclude_interfunction_padding(self):
        image = ElfImage.__new__(ElfImage)
        image.symbols = {'f': (0x100, 4, 2)}
        rows = insns([('ret', ''), ('nop', ''), ('nop', '')])
        self.assertEqual(image.bound({'f': rows})['f'], rows[:1])

    def test_parser_accepts_objdump_bytes_and_no_bytes(self):
        with_bytes = '0000000000000100 <f>:\n 100: 48 89 f8 mov %rdi,%rax\n 103: c3 ret\n'
        without = '0000000000000100 <f>:\n 100: mov %rdi,%rax\n 103: ret\n'
        self.assertEqual(parse_disassembly(with_bytes), parse_disassembly(without))


class DispatchTests(unittest.TestCase):
    def wrapper(self):
        return insns([('push', '%rbx'), ('mov', '%rdi,%rbx'),
                      ('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'),
                      ('jne', '124 <wrapper+0x24>'), ('mov', '%rbx,%rdi'),
                      ('pop', '%rbx'), ('jmp', '600 <wf__par_seq_body>'),
                      ('nop', ''), ('mov', '%rbx,%rdi'), ('pop', '%rbx'),
                      ('jmp', '700 <wf_body>')])

    def test_realistic_tail_pool_shell_keeps_abi_and_selects_w1(self):
        # Preserve all caller-clobbered args for realistic scalar wrapper.
        rows = [('push', '%' + register) for register in ('rbx', 'r12', 'r13', 'r14', 'r15', 'rbp')]
        rows += [('mov', '%' + argument + ',%' + saved) for argument, saved in zip(('rdi','rsi','rdx','rcx','r8','r9'), ('rbx','r12','r13','r14','r15','rbp'))]
        rows += [('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'), ('jne', '190 <wrapper+0x90>')]
        restore = [('mov', '%' + saved + ',%' + argument) for argument, saved in zip(('rdi','rsi','rdx','rcx','r8','r9'), ('rbx','r12','r13','r14','r15','rbp'))]
        restore += [('pop', '%' + saved) for saved in ('rbp','r15','r14','r13','r12','rbx')]
        rows += restore + [('jmp', '600 <wf__par_seq_body>')]
        function = insns(rows)
        destination, trace, _ = entry_shell(function, False)
        self.assertEqual((destination, trace), ('wf__par_seq_body', ['pool-active']))

    def test_cheap_extent_three_selects_same_body_at_active_widths(self):
        function = insns([('cmp', '$0x53b4,%rsi'), ('ja', '10c <body+0xc>'), ('jmp', '600 <wf__par_seq_body>'), ('ret', '')])
        destination, trace, _ = entry_shell(function, True, {'rsi': 3})
        self.assertEqual((destination, trace), ('wf__par_seq_body', ['cheap-region']))
        with self.assertRaises(IdentityError):
            entry_shell(function, True, {'rsi': 30000})

    def test_actual_three_argument_wrapper_and_active_cheap_path(self):
        saved = ('rbx', 'r12', 'r13')
        arguments = ('rdi', 'rsi', 'rdx')
        save = [('push', '%' + register) for register in saved]
        save += [('mov', '%' + argument + ',%' + register) for argument, register in zip(arguments, saved)]
        restore = [('mov', '%' + register + ',%' + argument) for argument, register in zip(arguments, saved)]
        restore += [('pop', '%' + register) for register in reversed(saved)]
        wrapper = insns(save + [('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'), ('jne', '140 <wf_bench_micro+0x40>')] + restore + [('jmp', '600 <wf__par_seq_body>')] + restore + [('jmp', '700 <wf_body>')])
        computation = insns([('lea', '7(%rdx),%rax'), ('ret', '')])
        region = insns([('cmp', '$0x53b4,%rsi'), ('ja', '10c <wf_body+0xc>'), ('jmp', '600 <wf__par_seq_body>'), ('ret', '')])
        demand = {'wf_bench_micro': wrapper, 'wf_body': region, 'wf__par_seq_body': computation}
        for active, trace in [(False, ['pool-active']), (True, ['pool-active', 'cheap-region'])]:
            root, observed = computation_root(demand, 'wf_bench_micro', active, {'rdi': 100, 'rsi': 3}, 3)
            self.assertEqual((root, observed), ('wf__par_seq_body', trace))
            self.assertIsNone(first_difference(normalized(computation), normalized(demand[root])))

    def test_renamed_region_can_select_an_unrelated_sequential_clone(self):
        region = insns([('cmp', '$0x53b4,%rsi'),
                        ('ja', '10c <wf_renamed+0xc>'),
                        ('jmp', '600 <wf__par_seq_body>'), ('ret', '')])
        computation = insns([('lea', '7(%rdx),%rax'), ('ret', '')])
        functions = {'wf_renamed': region, 'wf__par_seq_body': computation}
        self.assertEqual(computation_root(functions, 'wf_renamed', True,
                                          {'rsi': 3}, 3),
                         ('wf__par_seq_body', ['cheap-region']))

    def test_sequential_recursive_tail_remains_computation(self):
        function = insns([('test', '%rdi,%rdi'),
                          ('je', '110 <wf__par_seq_body+0x10>'),
                          ('sub', '$1,%rdi'),
                          ('jmp', '100 <wf__par_seq_body>'), ('ret', '')])
        functions = {'wf__par_seq_body': function}
        self.assertEqual(computation_root(functions, 'wf__par_seq_body', False,
                                          {'rdi': 3}, 1),
                         ('wf__par_seq_body', []))

    def test_single_recursive_tail_is_not_an_adapter_shell(self):
        for name in ('wf_body', 'wf__par_seq_body'):
            function = insns([('jmp', '100 <' + name + '>')])
            self.assertEqual(computation_root({name: function}, name, False),
                             (name, []))

    def test_test_instruction_ja_and_jbe_use_zero_flag(self):
        for mnemonic, expected in [('ja', 'wf__par_seq_zero'), ('jbe', 'wf__par_seq_nonzero')]:
            function = insns([('test', '%rsi,%rsi'), (mnemonic, '110 <body+0x10>'), ('jmp', '600 <wf__par_seq_zero>'), ('nop', ''), ('jmp', '700 <wf__par_seq_nonzero>')])
            destination, _, _ = entry_shell(function, True, {'rsi': 0})
            self.assertEqual(destination, expected)

    def test_unrestored_arg_fails_shell(self):
        with self.assertRaisesRegex(IdentityError, 'does not restore'):
            entry_shell(insns([('call', '500 <wf__par_pool_active>'), ('jmp', '600 <wf__par_seq_body>')]), False)

    def test_deallocated_save_is_not_valid_after_pool_call(self):
        function = insns([('push', '%rdi'), ('add', '$8,%rsp'),
                          ('call', '500 <wf__par_pool_active>'),
                          ('sub', '$8,%rsp'), ('pop', '%rdi'),
                          ('jmp', '600 <wf__par_seq_body>')])
        with self.assertRaisesRegex(IdentityError, 'pop'):
            entry_shell(function, False, argument_count=1)

    def test_dispatch_shell_cannot_cross_caller_stack_boundary(self):
        function = insns([('add', '$8,%rsp'), ('push', '%rdi'),
                          ('pop', '%rdi'), ('sub', '$8,%rsp'),
                          ('jmp', '600 <wf__par_seq_body>')])
        with self.assertRaisesRegex(IdentityError, 'caller stack boundary'):
            entry_shell(function, False, argument_count=1)

    def test_stack_arithmetic_invalidates_flags(self):
        function = insns([('cmp', '$3,%rsi'), ('sub', '$8,%rsp'), ('je', '110 <body+0x10>'), ('add', '$8,%rsp'), ('jmp', '600 <wf__par_seq_body>')])
        with self.assertRaisesRegex(IdentityError, 'known flags'):
            entry_shell(function, True, {'rsi': 3})

    def test_eax_is_derived_from_rax_and_cannot_retain_stale_pool_value(self):
        function = insns([('call', '500 <wf__par_pool_active>'), ('mov', '%rdi,%rax'), ('test', '%eax,%eax'), ('je', '110 <body+0x10>'), ('jmp', '600 <wf__par_seq_body>')])
        with self.assertRaisesRegex(IdentityError, 'overwritten'):
            entry_shell(function, False)

    def test_signed_negative_immediate_is_unsigned_in_cmpq(self):
        function = insns([('cmpq', '$-1,%rsi'), ('ja', '10c <body+0xc>'), ('jmp', '600 <wf__par_seq_body>'), ('ret', '')])
        destination, _, _ = entry_shell(function, True, {'rsi': 3})
        self.assertEqual(destination, 'wf__par_seq_body')

    def test_dispatch_inside_machine_loop_fails(self):
        function = insns([('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'), ('jne', '100 <wrapper>'), ('ret', '')])
        with self.assertRaisesRegex(IdentityError, 'cycle'):
            project_pool(function, False)

    def test_path_bypassing_pool_fails_despite_one_static_call(self):
        function = insns([('test', '%rdi,%rdi'), ('je', '118 <wrapper+0x18>'), ('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'), ('jne', '118 <wrapper+0x18>'), ('ret', ''), ('ret', '')])
        with self.assertRaisesRegex(IdentityError, 'bypasses'):
            project_pool(function, False)

    def test_llvm_pool_at_entry_is_proved_not_just_counted(self):
        text = '''define i64 @wf_bench_micro(i64 %n) {
entry:
  %pool = call i32 @wf__par_pool_active()
  %active = icmp ne i32 %pool, 0
  br i1 %active, label %par, label %seq
par:
  %a = call i64 @wf_body(i64 %n)
  ret i64 %a
seq:
  %b = call i64 @wf__par_seq_body(i64 %n)
  ret i64 %b
}
'''
        proof = dispatch_proof(parse_ir(text), 'wf_bench_micro')
        self.assertEqual(proof['pool_decisions_per_timed_call'], 1)
        self.assertTrue(proof['outside_loops'])


if __name__ == '__main__':
    unittest.main()
