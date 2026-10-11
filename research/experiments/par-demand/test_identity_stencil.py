"""Stencil-shaped x86 fixtures; no compiler, executable or timing runs."""
import unittest

from identity import Instruction, IdentityError, normalized, first_difference, project_pool


def instructions(rows):
    return [Instruction(0x100 + 4 * index, *row) for index, row in enumerate(rows)]


class StencilProjectionTests(unittest.TestCase):
    def wrapper(self):
        # Preserve the hosted wrapper's non-tail result stores and extra saves.
        rows = [('push', '%' + register) for register in
                ('rbp', 'r15', 'r14', 'r13', 'r12', 'rbx', 'rax')]
        rows += [('mov', '%r9,%rbx'), ('mov', '%r8,%r14'), ('mov', '%rcx,%r15'),
                 ('mov', '%rdx,%r12'), ('mov', '%rsi,%r13'), ('mov', '%rdi,%rbp'),
                 ('call', '500 <wf__par_pool_active>'),
                 ('mov', '%rbp,%rdi'), ('mov', '%r13,%rsi'), ('mov', '%r12,%rdx'),
                 ('test', '%eax,%eax'), ('je', '154 <wrapper+0x54>'),
                 ('call', '700 <wf_body>'), ('jmp', '158 <wrapper+0x58>'),
                 ('call', '600 <wf__par_seq_body>'),
                 ('mov', '(%rax),%rcx'), ('lea', '0x8(%rax),%rdx'),
                 ('mov', '%rdx,(%r15)'), ('mov', '%rcx,(%r14)'), ('mov', '%rax,(%rbx)'),
                 ('add', '$0x8,%rsp')]
        rows += [('pop', '%' + register) for register in
                 ('rbx', 'r12', 'r13', 'r14', 'r15', 'rbp')]
        rows += [('ret', '')]
        return instructions(rows)

    def test_non_tail_pool_with_argument_restores_keeps_all_nondecision_work(self):
        wrapper = self.wrapper()
        selected, omitted, count = project_pool(wrapper, False)
        self.assertEqual((omitted, count), ({13, 17, 18}, 1))
        self.assertTrue({14, 15, 16, 22, 23, 24, 25, 26, 27}.issubset(selected))
        self.assertNotIn(19, selected)
        self.assertNotIn(20, selected)
        self.assertIn(21, selected)
        # An independent sequential wrapper with the same nondecision work
        # agrees after projection; no restores or result stores are discarded.
        seq = [row for index, row in enumerate(wrapper) if index not in {13, 17, 18, 19, 20}]
        self.assertIsNone(first_difference(normalized(seq), normalized(wrapper, selected, omitted)))
        active, _, active_count = project_pool(wrapper, True)
        self.assertEqual(active_count, 1)
        self.assertIn(19, active)
        self.assertNotIn(21, active)

    def test_mutated_restore_remains_a_computation_difference(self):
        wrapper = self.wrapper()
        selected, omitted, _ = project_pool(wrapper, False)
        changed = list(wrapper)
        changed[16] = Instruction(changed[16].address, 'mov', '%r13,%rdx')
        difference = first_difference(normalized(wrapper, selected, omitted),
                                      normalized(changed, selected, omitted))
        self.assertEqual(difference['instruction'], 16)

    def test_added_saves_do_not_disappear_with_pool_decision(self):
        wrapper = self.wrapper()
        selected, omitted, _ = project_pool(wrapper, False)
        seq = instructions([('push', '%r15'), ('push', '%r14'), ('push', '%rbx'),
                            ('mov', '%r9,%rbx'), ('mov', '%r8,%r14'), ('mov', '%rcx,%r15'),
                            ('call', '600 <wf_body>'), ('mov', '(%rax),%rcx'),
                            ('lea', '0x8(%rax),%rdx'), ('mov', '%rdx,(%r15)'),
                            ('mov', '%rcx,(%r14)'), ('mov', '%rax,(%rbx)'),
                            ('pop', '%rbx'), ('pop', '%r14'), ('pop', '%r15'), ('ret', '')])
        self.assertIsNotNone(first_difference(normalized(seq), normalized(wrapper, selected, omitted)))

    def test_pool_result_write_and_nonrestore_in_gap_fail_closed(self):
        for row in [('mov', '%rbp,%rax'), ('add', '$0x1,%rdi'),
                    ('mov', '(%rbp),%rdi'), ('mov', '%rsp,%rdi')]:
            with self.subTest(row=row):
                wrapper = self.wrapper()
                wrapper[14] = Instruction(wrapper[14].address, *row)
                with self.assertRaisesRegex(IdentityError, 'unsupported pool dispatch'):
                    project_pool(wrapper, False)

    def test_repeating_branch_after_single_pool_call_fails(self):
        wrapper = self.wrapper()
        wrapper[18] = Instruction(wrapper[18].address, 'je', '148 <wrapper+0x48>')
        with self.assertRaisesRegex(IdentityError, 'assembly cycle'):
            project_pool(wrapper, False)


class StencilRegisterTests(unittest.TestCase):
    def test_identical_save_order_precludes_later_rbx_rbp_permutation(self):
        # Hosted bodies have the same six saves, then choose rbx versus rbp
        # for the product. Even permitting rbp renaming could not make a
        # consistent whole-function permutation satisfy both observations.
        prefix = [('push', '%' + register) for register in
                  ('rbp', 'r15', 'r14', 'r13', 'r12', 'rbx')]
        prefix += [('sub', '$0xa8,%rsp'), ('mov', '%rdx,0x20(%rsp)')]
        seq = instructions(prefix + [('mov', '%rsi,%rbx')])
        demand = instructions(prefix + [('mov', '%rsi,%rbp')])
        self.assertEqual(first_difference(normalized(seq), normalized(demand))['instruction'], 9)

    def test_frame_pointer_and_stack_registers_stay_pinned(self):
        seq = instructions([('push', '%rbp'), ('mov', '%rsp,%rbp'),
                            ('mov', '8(%rbp),%rax'), ('leave', ''), ('ret', '')])
        changed = instructions([('push', '%rbx'), ('mov', '%rsp,%rbx'),
                                ('mov', '8(%rbx),%rax'), ('leave', ''), ('ret', '')])
        self.assertIsNotNone(first_difference(normalized(seq), normalized(changed)))


if __name__ == '__main__':
    unittest.main()
