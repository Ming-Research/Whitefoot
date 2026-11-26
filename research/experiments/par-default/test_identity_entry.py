"""Entry diagnostic negatives run without a compiler or native execution."""
import unittest
import sys
from pathlib import Path
# entry_dispatch is part of the extracted prototype runner (see manifest_path).
# Appended, not prepended: the runner also carries older copies of this
# directory's identity checker and its tests, which must not shadow these.
sys.path.append(str(Path(__file__).resolve().parents[1] / 'par-demand'))
from identity import Instruction, IdentityError
from entry_dispatch import report, shape, validate_sample


def instructions(rows, base=0x100):
    return [Instruction(base + 4 * i, *row) for i, row in enumerate(rows)]


def sample(width, round_id=0, calls=1024):
    return dict(width=width, calls=calls, order=round_id % 2, active=int(width > 1), resolution_ns=1,
        first_query=dict(wall_ns=1000, cpu_ns=1000),
        direct=dict(wall_ns=1000000, cpu_ns=1000000, pool_queries=0),
        dispatch=dict(wall_ns=1001024, cpu_ns=1002048, pool_queries=calls), round=round_id)


class EntryIdentityTests(unittest.TestCase):
    def functions(self):
        return dict(wf_entry_direct=instructions([('jmp', '400 <wf_entry_inactive>')]),
            wf_entry_active=instructions([('lea', '0x1(%rdi),%rax'), ('ret', '')], 0x300),
            wf_entry_inactive=instructions([('lea', '0x1(%rdi),%rax'), ('ret', '')], 0x400),
            wf__par_demand_mode=instructions([('mov', '$0x1,%eax'), ('ret', '')], 0x600),
            wf__par_pool_active=instructions([('ret', '')], 0x500),
            wf_entry_dispatch=instructions([('push', '%rbx'), ('mov', '%rdi,%rbx'),
                ('call', '500 <wf__par_pool_active>'), ('test', '%eax,%eax'),
                ('je', '120 <wf_entry_dispatch+0x20>'), ('mov', '%rbx,%rdi'),
                ('pop', '%rbx'), ('jmp', '300 <wf_entry_active>'), ('mov', '%rbx,%rdi'),
                ('pop', '%rbx'), ('jmp', '400 <wf_entry_inactive>')]))

    def test_actual_query_and_distinct_equal_destinations_pass(self):
        proof = shape(self.functions())
        self.assertTrue(proof['outside_loops'])
        self.assertEqual(proof['pool_decisions_per_entry'], 1)

    def test_folded_query_loop_wrong_leaf_and_changed_computation_fail(self):
        variants = []
        folded = self.functions()
        folded['wf_entry_dispatch'] = instructions([('jmp', '400 <wf_entry_inactive>')])
        variants.append(folded)
        loop = self.functions()
        loop['wf_entry_dispatch'][4] = Instruction(0x110, 'je', '108 <wf_entry_dispatch+0x8>')
        variants.append(loop)
        swapped = self.functions()
        swapped['wf_entry_dispatch'][-1] = Instruction(0x128, 'jmp', '300 <wf_entry_active>')
        variants.append(swapped)
        changed = self.functions()
        changed['wf_entry_active'][0] = Instruction(0x300, 'lea', '0x2(%rdi),%rax')
        variants.append(changed)
        ordinary_runtime = self.functions()
        ordinary_runtime['wf__par_demand_mode'][0] = Instruction(0x600, 'mov', '$0,%eax')
        variants.append(ordinary_runtime)
        for functions in variants:
            with self.assertRaises(IdentityError): shape(functions)

    def test_direct_control_cannot_query_or_change_argument(self):
        functions = self.functions()
        functions['wf_entry_direct'] = functions['wf_entry_dispatch']
        with self.assertRaises(IdentityError): shape(functions)
        functions = self.functions()
        functions['wf_entry_direct'] = instructions([('mov', '$2,%rdi'), ('jmp', '400 <wf_entry_inactive>')])
        with self.assertRaises(IdentityError): shape(functions)

    def evidence(self):
        return dict(calibration={str(w): dict(calls=1024, pilots=[sample(w)]) for w in (1, 2, 4, 8)},
            sizing=[sample(w, r) for w in (1, 2, 4, 8) for r in range(6)],
            decisive=[sample(w, r) for w in (1, 2, 4, 8) for r in range(6)])

    def test_report_retains_separate_distribution_and_is_not_a_cost_gate(self):
        result = report(self.evidence(), 6)
        self.assertIn('no 1 ns/2%', result['policy'])
        self.assertEqual(result['widths'][0]['decisive']['wall_delta_ns_per_entry']['median'], 1)
        self.assertEqual(result['widths'][0]['decisive']['cpu_delta_ns_per_entry']['median'], 2)
        self.assertEqual(result['widths'][0]['decisive']['first_query_wall_ns']['median'], 1000)
        self.assertEqual(result['widths'][0]['decisive']['first_query_cpu_ns']['median'], 1000)

    def test_missing_changed_counts_duplicate_rounds_and_early_freeze_fail(self):
        variants = []
        for section in ('sizing', 'decisive'):
            missing = self.evidence()
            missing[section].pop()
            variants.append(missing)
        wrong = self.evidence()
        wrong['decisive'][0]['dispatch']['pool_queries'] = 0
        variants.append(wrong)
        duplicate = self.evidence()
        duplicate['decisive'][1]['round'] = 0
        variants.append(duplicate)
        frozen = self.evidence()
        frozen['calibration']['1']['pilots'][0]['direct']['wall_ns'] = 10
        variants.append(frozen)
        for evidence in variants:
            with self.assertRaises(ValueError): report(evidence, 6)
        wrong = sample(1)
        wrong['active'] = 1
        with self.assertRaises(ValueError): validate_sample(wrong, 1, 1024, 0)
