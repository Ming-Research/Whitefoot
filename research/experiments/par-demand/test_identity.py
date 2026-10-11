"""Hosted identity checks. No compiler fixture, timing or build runs here."""
import unittest
import contextlib
import io
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import identity
from identity import (IdentityError, Instruction, normalized, first_difference,
                      parse_disassembly, project_pool, entry_shell, computation_root,
                      dispatch_proof, parse_ir, ElfImage, canonical_symbol,
                      classify_construction, latched_writer_evidence)


def insns(rows):
    return [Instruction(0x100 + index * 4, *row) for index, row in enumerate(rows)]


class ConstructionAttributionTests(unittest.TestCase):
    def record(self, identical=True):
        return {'status': 'fail', 'comparisons': [
            {'function': 'wf__par_seq_body', 'role': 'reachable sequential body/helper', 'identical': identical}],
            'dispatch': {'pool_decisions_per_timed_call': 1, 'outside_loops': True, 'regions': []},
            'failures': [{'workload': 'fixture', 'function': 'wf_bench_fixture',
                          'reason': 'normalized timed sequential wrapper differs',
                          'instruction': 1, 'seq': 'push %rax', 'demand': 'push %saved0.0'}]}

    def test_identical_body_closure_does_not_green_an_entry_adapter_difference(self):
        record = self.record()
        classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'}, {})
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['status'], 'fail')
        failure = record['failures'][0]
        self.assertEqual(failure['category'], 'entry-adapter')
        self.assertEqual((failure['instruction'], failure['seq'], failure['demand']),
                         (1, 'push %rax', 'push %saved0.0'))
        self.assertEqual(failure['evidence']['selected_sequential_roots'], ['wf__par_seq_body'])

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
                         ('lock', 'cmpxchg %edx,(%rax)'), ('ret', '')])
        return seq, demand, machine

    def test_latched_writer_requires_both_exact_ir_delta_and_actual_atomic(self):
        seq, demand, machine = self.writer()
        evidence = latched_writer_evidence('wf_resource_record_abort',
            {'wf_resource_record_abort': seq}, {'wf_resource_record_abort': demand}, machine, None)
        self.assertIsNotNone(evidence)
        record = self.record()
        record['comparisons'].append({'function': 'wf_resource_record_abort',
                                     'role': 'reachable sequential body/helper', 'identical': False})
        record['failures'].append({'function': 'wf_resource_record_abort',
                                  'reason': 'normalized sequential body differs', 'instruction': 4})
        classify_construction(record, 'wf_bench_fixture', {'wf__par_seq_body'},
                              {'wf_resource_record_abort': evidence})
        self.assertEqual(record['computation_status'], 'pass')
        self.assertEqual(record['failures'][1]['category'], 'latched-writer')
        self.assertEqual(record['status'], 'fail')
        changed = {**demand, 'write.loop': ['call void @different_write()', 'br label %abort']}
        for candidate, actual in [(changed, machine), (demand, machine[:1]), (demand, insns([('lock', 'cmpxchg %edx,(%rax)'), ('ret', '')]))]:
            self.assertIsNone(latched_writer_evidence('wf_resource_record_abort',
                {'wf_resource_record_abort': seq}, {'wf_resource_record_abort': candidate}, actual, None))
        self.assertIsNone(latched_writer_evidence('wf_unrelated_writer',
            {'wf_resource_record_abort': seq}, {'wf_unrelated_writer': demand}, machine, None))

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
            self.assertEqual((first['category'], first['instruction']), ('entry-adapter', 1))
            self.assertIn('[entry-adapter]', output.getvalue())
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
