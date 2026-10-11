"""Nontail timed-wrapper identity without compiling or running an image."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import identity


SEQ = '''0000000000000100 <wf_bench_projection>:
 100: push %rbx
 104: mov %rdi,%rbx
 108: mov %rbx,%rdi
 10c: call 300 <wf_body>
 110: pop %rbx
 114: ret
0000000000000300 <wf_body>:
 300: lea 7(%rdi),%rax
 304: ret
'''
DEMAND = '''0000000000000100 <wf_bench_projection>:
 100: push %rbx
 104: mov %rdi,%rbx
 108: call 500 <wf__par_pool_active>
 10c: mov %rbx,%rdi
 110: test %eax,%eax
 114: je 120 <wf_bench_projection+0x20>
 118: call 300 <wf_body>
 11c: jmp 124 <wf_bench_projection+0x24>
 120: call 600 <wf__par_seq_body>
 124: pop %rbx
 128: ret
0000000000000600 <wf__par_seq_body>:
 600: lea 7(%rdi),%rax
 604: ret
'''
IR = '''define i64 @wf_bench_projection(i64 %value) {
entry:
  %pool = call i32 @wf__par_pool_active()
  %active = icmp ne i32 %pool, 0
  br i1 %active, label %par, label %seq
par:
  %a = call i64 @wf_body(i64 %value)
  ret i64 %a
seq:
  %b = call i64 @wf__par_seq_body(i64 %value)
  ret i64 %b
}
'''


class FixtureImage:
    def __init__(self, path):
        pass

    def bound(self, functions):
        self.functions = functions
        return functions

    def code_identity(self, address, spelling):
        return identity.canonical_symbol(self.functions.resolve(address, spelling))


class ProjectionIntegrationTests(unittest.TestCase):
    def check(self, demand=DEMAND, seq=SEQ):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            for arm, assembly in [('seq', seq), ('demand', demand)]:
                (build / arm).mkdir()
                (build / arm / 'stencil.disassembly').write_text(assembly)
                (build / arm / 'stencil.ll').write_text(IR)
            with patch.object(identity, 'ElfImage', FixtureImage):
                return identity.check_workload('stencil', build)

    def test_nontail_projection_diagnoses_identity_without_waiving_state_proof(self):
        result = self.check()
        self.assertEqual(result['status'], 'fail')
        self.assertTrue(any('lacks entry-state preservation proof' in row['reason']
                            for row in result['failures']))
        self.assertTrue(any(row['function'] == 'wf_bench_projection'
                            and row['identical'] for row in result['comparisons']))
        self.assertTrue(any(row['function'] == 'wf__par_seq_body'
                            and row['identical'] for row in result['comparisons']))

    def test_equal_text_cannot_hide_a_live_volatile_value_across_pool_call(self):
        seq = SEQ.replace('push %rbx', 'push %r10').replace('pop %rbx', 'pop %r10').replace('%rbx', '%r10')
        demand = DEMAND.replace('push %rbx', 'push %r10').replace('pop %rbx', 'pop %r10').replace('%rbx', '%r10')
        # Adjacent call/test/branch is supported independently of the restore
        # gap; the volatile restore occurs only on the inactive selected edge.
        demand = demand.replace(' 10c: mov %r10,%rdi\n', '')
        demand = demand.replace(' 120: call', ' 11e: mov %r10,%rdi\n 120: call')
        demand = demand.replace('je 120 <wf_bench_projection+0x20>',
                                'je 11e <wf_bench_projection+0x1e>')
        result = self.check(demand, seq)
        self.assertEqual(result['status'], 'fail')
        self.assertTrue(any('lacks entry-state preservation proof' in row['reason']
                            for row in result['failures']))
        self.assertTrue(any(row['function'] == 'wf_bench_projection'
                            and row['identical'] for row in result['comparisons']))

    def test_equal_text_cannot_hide_flags_overwritten_by_pool_test(self):
        seq = SEQ.replace(' 100: push %rbx', ' 100: push %rbx\n 102: cmp %rdi,%rsi')
        seq = seq.replace(' 110: pop', ' 10e: sete %al\n 110: pop')
        demand = DEMAND.replace(' 100: push %rbx', ' 100: push %rbx\n 102: cmp %rdi,%rsi')
        demand = demand.replace(' 124: pop', ' 122: sete %al\n 124: pop')
        result = self.check(demand, seq)
        self.assertEqual(result['status'], 'fail')
        self.assertTrue(any('lacks entry-state preservation proof' in row['reason']
                            for row in result['failures']))
        self.assertTrue(any(row['function'] == 'wf_bench_projection'
                            and row['identical'] for row in result['comparisons']))

    def test_wrong_argument_restore_remains_a_wrapper_difference(self):
        result = self.check(DEMAND.replace('mov %rbx,%rdi', 'mov %rbx,%rsi'))
        failure = next(row for row in result['failures']
                       if row['reason'] == 'normalized timed sequential wrapper differs')
        self.assertEqual(failure['instruction'], 3)
        self.assertEqual(result['status'], 'fail')

    def test_changed_computation_survives_supported_dispatch_projection(self):
        result = self.check(DEMAND.replace('lea 7(%rdi)', 'lea 9(%rdi)'))
        failure = next(row for row in result['failures']
                       if row['function'] == 'wf__par_seq_body')
        self.assertEqual(failure['reason'], 'normalized sequential body differs')
        self.assertEqual(failure['instruction'], 1)
        self.assertEqual(result['status'], 'fail')
