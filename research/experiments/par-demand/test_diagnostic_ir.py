"""Pure printing-model controls for the manual common-object diagnostic."""
import unittest

from diagnostic_ir import (DiagnosticIrError, Module, build_fir_module,
                           build_seq_noinline_module, rewrite, closure)


ARGUMENTS = 'ptr %input, i64 %input_len, ptr %taps, i64 %tap_len, i64 %first, i64 %end, i64 %last_tap'
TARGET = 'target datalayout = "e-m:e-p:64:64-i64:64-n8:16:32:64-S128"\ntarget triple = "x86_64-unknown-linux-gnu"\n'
COLD = '''@.wf_resource.heap = private constant [1 x i8] c"x"
@.wf_resource_record.latch = private global i32 0, align 4
declare void @abort() noreturn
declare i64 @write(i32, ptr, i64)
define private i64 @wf_resource_write(ptr %message, i64 %length) #0 {
entry:
  %n = call i64 @write(i32 2, ptr %message, i64 %length)
  ret i64 %n
}
define weak ptr @wf__floor_record_latch() #0 {
entry:
  ret ptr @.wf_resource_record.latch
}
define private void @wf_resource_record_abort(ptr %message, i64 %length) noreturn #0 {
entry:
  %latch = call ptr @wf__floor_record_latch()
  %acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst
  %won = extractvalue { i32, i1 } %acquired, 1
  br i1 %won, label %write.loop, label %park
write.loop:
  %n = call i64 @wf_resource_write(ptr %message, i64 %length)
  br label %abort
abort:
  call void @abort()
  unreachable
park:
  %parked = load volatile i32, ptr %latch, align 4
  br label %park
}
define private void @wf_resource_abort() noreturn #0 {
entry:
  call void @wf_resource_record_abort(ptr @.wf_resource.heap, i64 1)
  unreachable
}
attributes #0 = { "probe-stack"="inline-asm" }
'''


def function(name, body, arguments=ARGUMENTS):
    return f'define ptr @{name}({arguments}) #0 {{\nentry:\n{body}\n}}\n'


def modules(extra='', helper_body='  ret ptr %input'):
    seq = TARGET + COLD + extra + function('wf_helper$instance$A', helper_body, 'ptr %input')
    seq += function('wf_filter', '  %r = call ptr @wf_helper$instance$A(ptr %input)\n  ret ptr %r')
    demand = TARGET + COLD + extra + function('wf__par_seq_helper$instance$A', helper_body, 'ptr %input')
    demand += function('wf__par_seq_filter', '  %r = call ptr @wf__par_seq_helper$instance$A(ptr %input)\n  ret ptr %r')
    par = TARGET + COLD + extra + function('wf__par_seq__par_chunk_filter.1', '  ret ptr %input', 'ptr %input')
    par += function('wf__par_seq_filter', '  %r = call ptr @wf__par_seq__par_chunk_filter.1(ptr %input)\n  ret ptr %r')
    return seq, demand, par


class CommonObjectTests(unittest.TestCase):
    def test_target_contract_must_match_in_every_input_and_be_complete(self):
        original = modules()
        for arm in range(3):
            for old, new in (('x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'),
                             ('e-p:64:64', 'e-p:32:32')):
                with self.subTest(arm=arm, changed=old):
                    inputs = list(original)
                    inputs[arm] = inputs[arm].replace(old, new)
                    with self.assertRaisesRegex(DiagnosticIrError, 'target triple/datalayout differs'):
                        build_fir_module(*inputs)
        with self.assertRaisesRegex(DiagnosticIrError, 'missing LLVM target'):
            build_fir_module(*(text.replace(TARGET, '') for text in original))
        with self.assertRaisesRegex(DiagnosticIrError, 'duplicate LLVM target'):
            build_fir_module(original[0] + TARGET, *original[1:])

    def test_bridges_refuse_nondefault_calling_conventions_in_every_root(self):
        original = modules()
        for arm, root in enumerate(('wf_filter', 'wf__par_seq_filter', 'wf__par_seq_filter')):
            for convention in ('fastcc', 'cc 8', 'x86_vectorcallcc'):
                with self.subTest(arm=arm, convention=convention):
                    inputs = list(original)
                    inputs[arm] = inputs[arm].replace(f'define ptr @{root}(', f'define {convention} ptr @{root}(')
                    with self.assertRaisesRegex(DiagnosticIrError, 'unsupported FIR boundary ABI'):
                        build_fir_module(*inputs)
        with self.assertRaisesRegex(DiagnosticIrError, 'unsupported FIR boundary ABI'):
            build_seq_noinline_module(original[0].replace('define ptr @wf_filter(', 'define fastcc ptr @wf_filter('))

    def test_bridges_refuse_argument_abi_attributes_but_accept_emitted_proof_attributes(self):
        original = modules()
        for arm, root in enumerate(('wf_filter', 'wf__par_seq_filter', 'wf__par_seq_filter')):
            for old, new in (('ptr %input', 'ptr byval(i64) %input'),
                             ('i64 %input_len', 'i64 signext %input_len'),
                             ('i64 %input_len', 'i64 zeroext %input_len'),
                             ('ptr %input', 'ptr inreg %input')):
                with self.subTest(arm=arm, attribute=new):
                    inputs = list(original)
                    inputs[arm] = inputs[arm].replace(f'@{root}({ARGUMENTS})',
                        f'@{root}({ARGUMENTS.replace(old, new)})')
                    with self.assertRaisesRegex(DiagnosticIrError, 'unsupported FIR boundary ABI'):
                        build_fir_module(*inputs)
        inputs = []
        for text, root in zip(original, ('wf_filter', 'wf__par_seq_filter', 'wf__par_seq_filter')):
            arguments = ARGUMENTS.replace('ptr %input', 'ptr noalias nonnull nocapture %input')
            inputs.append(text.replace(f'@{root}({ARGUMENTS})', f'@{root}({arguments})'))
        text, _ = build_fir_module(*inputs)
        self.assertIn('ptr noalias nonnull nocapture %input', text)
        self.assertIn(' noinline', build_seq_noinline_module(inputs[0]))

    def test_both_actual_roots_and_bridges_are_noinline_without_cloning_unused_entities(self):
        seq, demand, par = modules()
        demand += function('wf_unused', '  ret ptr %input', 'ptr %input')
        text, proof = build_fir_module(seq, demand, par)
        module = Module(text)
        self.assertEqual(proof['canonical_actual_root'], 'wf_diagnostic_canonical.wf__par_seq_filter')
        self.assertEqual(proof['legacy_actual_root'], 'wf__par_seq_filter')
        self.assertEqual(proof['seq_actual_root'], 'wf_filter')
        self.assertEqual(proof['exposed_roots'], {'canonical': 'wf_diagnostic_canonical_filter', 'legacy': 'wf_diagnostic_legacy_filter'})
        for name in proof['noinline_roots']:
            self.assertIn(' noinline', module.entities['@', name].header)
        self.assertNotIn('wf_diagnostic_canonical.wf_unused', text)
        self.assertEqual(proof['canonical_symbol_map']['wf_diagnostic_canonical.wf__par_seq_helper$instance$A'], 'wf_helper$instance$A')
        self.assertIn('call ptr @wf__par_seq__par_chunk_filter.1', module.entities['@', proof['legacy_actual_root']].text)
        self.assertIn('call ptr @wf_diagnostic_canonical.wf__par_seq_helper$instance$A', module.entities['@', proof['canonical_actual_root']].text)

    def test_exact_token_rewrite_preserves_similar_names_and_quoted_constants(self):
        text = '@wf_helper$A = constant [23 x i8] c"@wf_helper$A %wf.t.Cell"\n@wf_helper$AB = constant ptr @"wf_helper$A"\n'
        rewritten = rewrite(text, {('@', 'wf_helper$A'): 'new.helper', ('%', 'wf.t.Cell'): 'new.Cell'})
        self.assertIn('@new.helper = constant', rewritten)
        self.assertIn('c"@wf_helper$A %wf.t.Cell"', rewritten)
        self.assertIn('@wf_helper$AB = constant ptr @new.helper', rewritten)

    def test_mutual_helper_recursion_imports_complete_closure(self):
        seq, demand, par = modules()
        seq = seq.replace('  ret ptr %input\n}', '  %r = call ptr @wf_other(ptr %input)\n  ret ptr %r\n}', 1)
        demand = demand.replace('  ret ptr %input\n}', '  %r = call ptr @wf__par_seq_other(ptr %input)\n  ret ptr %r\n}', 1)
        seq += function('wf_other', '  %r = call ptr @wf_helper$instance$A(ptr %input)\n  ret ptr %r', 'ptr %input')
        demand += function('wf__par_seq_other', '  %r = call ptr @wf__par_seq_helper$instance$A(ptr %input)\n  ret ptr %r', 'ptr %input')
        text, proof = build_fir_module(seq, demand, par)
        self.assertIn('@wf_diagnostic_canonical.wf__par_seq_other', text)
        self.assertIn('@wf_diagnostic_canonical.wf__par_seq_helper$instance$A', text)
        self.assertIn('wf_diagnostic_canonical.wf__par_seq_other', proof['canonical_symbol_map'])

    def test_weak_native_abi_name_and_complete_fallback_closure_are_shared(self):
        weak = function('wf__runtime_probe', '  ret ptr %input', 'ptr %input').replace('define ptr', 'define weak ptr')
        seq, demand, par = modules(extra=weak, helper_body='  %r = call ptr @wf__runtime_probe(ptr %input)\n  ret ptr %r')
        text, proof = build_fir_module(seq, demand, par)
        self.assertIn('@wf__runtime_probe', text)
        self.assertNotIn('@wf_diagnostic_canonical.wf__runtime_probe', text)
        self.assertIn('wf__runtime_probe', proof['shared_symbols'])
        wrong = demand.replace('define weak ptr @wf__runtime_probe(ptr %input) #0 {\nentry:\n  ret ptr %input',
                               'define weak ptr @wf__runtime_probe(ptr %input) #0 {\nentry:\n  ret ptr null')
        wrong_seq = seq.replace('define weak ptr @wf__runtime_probe(ptr %input) #0 {\nentry:\n  ret ptr %input',
                               'define weak ptr @wf__runtime_probe(ptr %input) #0 {\nentry:\n  ret ptr null')
        with self.assertRaisesRegex(DiagnosticIrError, 'shared weak ABI'):
            build_fir_module(wrong_seq, wrong, par)

    def test_equal_mutable_state_is_shared_but_conflicting_identity_is_refused(self):
        global_ = '@state = private global i64 0\n'
        body = '  %n = load i64, ptr @state\n  ret ptr %input'
        seq, demand, par = modules(extra=global_, helper_body=body)
        text, proof = build_fir_module(seq, demand, par)
        self.assertEqual(text.count('@state = private global'), 1)
        self.assertEqual(proof['canonical_symbol_map']['state'], 'state')
        with self.assertRaisesRegex(DiagnosticIrError, 'mutable object identity conflict'):
            build_fir_module(seq.replace('@state = private global i64 0', '@state = private global i64 1'),
                demand.replace('@state = private global i64 0', '@state = private global i64 1'), par)

    def test_readonly_function_pointer_global_is_remapped_and_aliases_are_preserved(self):
        seq, demand, par = modules()
        seq += '@table = private constant ptr @wf_helper$instance$A\n'
        demand += '@table = private constant ptr @wf__par_seq_helper$instance$A\n'
        seq = seq.replace('  ret ptr %input\n}', '  %p = load ptr, ptr @table\n  ret ptr %input\n}', 1)
        demand = demand.replace('  ret ptr %input\n}', '  %p = load ptr, ptr @table\n  ret ptr %input\n}', 1)
        text, proof = build_fir_module(seq, demand, par)
        self.assertIn('@wf_diagnostic_canonical.table = private constant ptr @wf_diagnostic_canonical.wf__par_seq_helper$instance$A', text)
        self.assertEqual(proof['canonical_symbol_map']['wf_diagnostic_canonical.table'], 'table')

    def test_conflicting_named_types_are_isolated_without_rewriting_local_values(self):
        seq, demand, par = modules(extra='%wf.t.Cell = type { i64 }\n',
            helper_body='  %v0 = alloca %wf.t.Cell\n  ret ptr %input')
        par = par.replace('%wf.t.Cell = type { i64 }', '%wf.t.Cell = type { i32 }')
        text, proof = build_fir_module(seq, demand, par)
        self.assertIn('%wf.t.diagnostic_canonical.wf.t.Cell = type { i64 }', text)
        self.assertIn('%v0 = alloca %wf.t.diagnostic_canonical.wf.t.Cell', text)
        self.assertIn('%wf.t.Cell = type { i32 }', text)
        self.assertEqual(proof['canonical_type_map']['wf.t.diagnostic_canonical.wf.t.Cell'], 'wf.t.Cell')

    def test_missing_dependencies_and_ambiguous_local_type_names_fail_closed(self):
        seq, demand, par = modules()
        missing_seq = seq.replace('call ptr @wf_helper$instance$A(ptr %input)',
                                  'call ptr @wf_missing(ptr %input)')
        missing_demand = demand.replace('call ptr @wf__par_seq_helper$instance$A(ptr %input)',
                                        'call ptr @wf_missing(ptr %input)')
        with self.assertRaisesRegex(DiagnosticIrError, 'missing LLVM dependency'):
            build_fir_module(missing_seq, missing_demand, par)
        missing = Module(function('f', '  %v0 = alloca %Missing\n  ret ptr %input', 'ptr %input') + 'attributes #0 = { nounwind }\n')
        with self.assertRaisesRegex(DiagnosticIrError, 'missing LLVM dependency'):
            closure(missing, {('@', 'f')})
        ambiguous = Module('%T = type { i64 }\n' + function('f', '  %T = alloca i64\n  ret ptr %input', 'ptr %input') + 'attributes #0 = { nounwind }\n')
        with self.assertRaisesRegex(DiagnosticIrError, 'ambiguous named type/local value'):
            closure(ambiguous, {('@', 'f')})

    def test_changed_root_body_is_refused_with_complete_dependencies(self):
        seq, demand, par = modules()
        changed_root = demand.replace(
            '  %r = call ptr @wf__par_seq_helper$instance$A(ptr %input)\n  ret ptr %r',
            '  %r = call ptr @wf__par_seq_helper$instance$A(ptr %input)\n  ret ptr null')
        with self.assertRaisesRegex(DiagnosticIrError, 'canonical FIR root differs from independent sequential source IR'):
            build_fir_module(seq, changed_root, par)

    def test_shared_writer_must_already_be_latched_and_unchanged(self):
        seq, demand, par = modules()
        for wrong in (par.replace('i32 0, i32 1 seq_cst', 'i32 0, i32 2 seq_cst'),
                      par.replace('call i64 @write(i32 2', 'call i64 @write(i32 1')):
            with self.assertRaisesRegex(DiagnosticIrError, 'shared cold-support entity differs'):
                build_fir_module(seq, demand, wrong)
        both = COLD.replace('%parked = load volatile i32, ptr %latch, align 4', '%parked = load i32, ptr %latch, align 4')
        with self.assertRaisesRegex(DiagnosticIrError, 'not the evidenced shared latched writer'):
            build_fir_module(seq, demand.replace(COLD, both), par.replace(COLD, both))

    def test_attribute_indices_remap_by_content_and_missing_groups_are_refused(self):
        seq, demand, par = modules()
        text, _ = build_fir_module(seq, demand.replace('#0', '#7'), par)
        self.assertNotIn('#7', text)
        with self.assertRaisesRegex(DiagnosticIrError, 'missing attribute group'):
            build_fir_module(seq, demand.replace('attributes #0 = { "probe-stack"="inline-asm" }\n', ''), par)
        with self.assertRaisesRegex(DiagnosticIrError, 'alwaysinline'):
            build_fir_module(seq.replace('"probe-stack"="inline-asm"', 'alwaysinline'),
                demand.replace('"probe-stack"="inline-asm"', 'alwaysinline'),
                par.replace('"probe-stack"="inline-asm"', 'alwaysinline'))

    def test_fixture_exports_and_references_are_renamed_for_the_native_driver(self):
        seq, demand, par = modules()
        entries = '''define i32 @wf__main_body(i32 %argc, ptr %argv) {
entry:
  ret i32 0
}
define i32 @main(i32 %argc, ptr %argv) {
entry:
  %r = call i32 @wf__main_body(i32 %argc, ptr %argv)
  ret i32 %r
}
'''
        text, _ = build_fir_module(seq, demand, par + entries)
        self.assertIn('@wf_diagnostic_fixture_main(', text)
        self.assertIn('call i32 @wf_diagnostic_fixture_body(', text)
        self.assertNotIn('@main(', text)
        sequential = build_seq_noinline_module(seq + entries)
        self.assertIn(' noinline', Module(sequential).entities['@', 'wf_filter'].header)
        self.assertNotIn('@main(', sequential)


if __name__ == '__main__':
    unittest.main()
