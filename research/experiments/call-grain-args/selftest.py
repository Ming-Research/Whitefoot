#!/usr/bin/env python3
"""Checks of the experiment's own instruments: each must fail on the wrong input
it exists to catch (a swapped or identical pair of arms, a vacuous identity
comparison, a noisy twin). Run by the workflow's smoke step; no compiler needed."""
import random
import re
import tempfile
import unittest
from pathlib import Path

import generate
import identity
import run
import stats

LEDGER = "PAR actualization  workload  call grain: omitted offer of mix_words (static work 35791 below 150000, no recursion)"

OBJECT = """0000000000000000 <wf_workload>:
   0:\tpush   %rbp
   1:\tcall   5 <wf_mix_words>
\t\t\t2: R_X86_64_PLT32\twf_mix_words-0x4
   5:\tjne    1a <wf_workload+0x1a>
   a:\tmovsd  0x10(%rip),%xmm0        # 20 <.LCPI0_0>
   f:\tret
  10:\tnopw   0x0(%rax,%rax,1)

0000000000000040 <wf_mix_words>:
  40:\txor    %eax,%eax
  42:\tret
"""


def clone(text):
    return text.replace("wf_workload", "wf__par_seq_workload").replace("wf_mix_words", "wf__par_seq_mix_words") \
        .replace("LCPI0_0", "LCPI7_0")


class Stats(unittest.TestCase):
    def test_order_is_balanced(self):
        items = list("abcd")
        firsts = [stats.balanced_order(items, r)[0] for r in range(8)]
        lasts = [stats.balanced_order(items, r)[-1] for r in range(8)]
        self.assertEqual(sorted(set(firsts)), items)
        self.assertEqual(sorted(set(lasts)), items)
        self.assertEqual(stats.balanced_order(items, 1), stats.balanced_order(items, 0)[::-1])

    def test_interval_covers_the_true_ratio_and_is_deterministic(self):
        rng = random.Random(3)
        values = [rng.gauss(0.5, 0.01) for _ in range(20)]
        median, low, high = stats.median_ci(values)
        self.assertTrue(low <= 0.5 <= high)
        self.assertEqual(stats.median_ci(values), (median, low, high))

    def test_too_few_rounds_refused(self):
        with self.assertRaises(ValueError):
            stats.median_ci([1.0, 1.0, 1.0])

    def test_not_slower_rule(self):
        quiet = (1.0, 0.995, 1.004)
        self.assertEqual(stats.judge_upper((1.20, 1.18, 1.22), quiet, 1.0, True)[0], "fail")
        self.assertEqual(stats.judge_upper((1.00, 0.99, 1.005), quiet, 1.0, True)[0], "pass")
        self.assertEqual(stats.judge_upper((1.01, 0.99, 1.03), quiet, 1.0, True)[0], "pass")
        self.assertEqual(stats.judge_upper((1.01, 0.99, 1.03), quiet, 1.0, False)[0], "unresolved")
        self.assertEqual(stats.judge_upper((1.01, 1.00, 1.02), (1.0, 0.9, 1.1), 1.0, True)[0], "void")

    def test_a_twin_spread_widens_the_bound(self):
        wide = (1.0, 0.985, 1.02)
        self.assertEqual(stats.judge_upper((1.015, 1.0, 1.02), wide, 1.0, True)[0], "pass")
        self.assertEqual(stats.judge_upper((1.03, 1.021, 1.04), wide, 1.0, True)[0], "fail")
        self.assertEqual(stats.judge_upper((1.03, 1.019, 1.04), wide, 1.0, True)[0], "pass")
        self.assertEqual(stats.judge_upper((1.015, 1.0, 1.02), wide, 1.0, False)[0], "unresolved")

    def test_swapped_arms_do_not_pass_the_speedup_bound(self):
        quiet = (1.0, 0.99, 1.01)
        self.assertEqual(stats.judge_upper((0.97, 0.96, 0.99), quiet, 0.60)[0], "fail")
        self.assertEqual(stats.judge_upper((0.45, 0.44, 0.47), quiet, 0.60)[0], "pass")

    def test_rerun_rule(self):
        self.assertEqual(stats.combine("pass", None), "pass")
        self.assertEqual(stats.combine("fail", "pass"), "inconclusive")
        self.assertEqual(stats.combine("fail", "fail"), "fail")
        self.assertEqual(stats.combine("unresolved", "unresolved"), "unresolved")
        self.assertEqual(stats.combine("pass", "void"), "void")

    def test_projection_is_even_and_capped(self):
        self.assertEqual(stats.project_rounds(0.002, 4), 10)
        self.assertEqual(stats.project_rounds(0.5, 4), 30)
        self.assertEqual(stats.project_rounds(0.012, 10) % 2, 0)

    def test_table_pairs_by_round(self):
        rows = []
        for r in range(6):
            for arm, wall in (("seq", 100), ("chg", 50), ("seq-twin", 100)):
                for s in (0, 1):
                    rows.append(dict(attempt=1, round=r, cell="c", arm=arm, width=1, sample=s, wall=wall * (1 + 9 * (s == 0)), cpu=0, count=1, grants=0))
        table = stats.Table(rows)
        self.assertEqual(table.ratios(1, "c", ("chg", 1), ("seq", 1)), [0.5] * 6)
        self.assertEqual(table.ratios(1, "c", ("seq", 1), ("chg", 1)), [2.0] * 6)


class Identity(unittest.TestCase):
    def test_clone_of_identical_code_is_identical(self):
        seq, par = identity.parse(OBJECT), identity.parse(clone(OBJECT))
        pairs, failures = identity.compare_clones(seq, par, ["wf_workload"])
        self.assertEqual(failures, [])
        self.assertEqual(pairs, {"wf_mix_words": "identical", "wf_workload": "identical"})

    def test_one_changed_instruction_is_found(self):
        seq = identity.parse(OBJECT)
        par = identity.parse(clone(OBJECT).replace("push   %rbp", "push   %rbx"))
        pairs, failures = identity.compare_clones(seq, par, ["wf_workload"])
        self.assertEqual(pairs["wf_workload"], "different")
        self.assertTrue(failures)

    def test_a_changed_call_target_is_found(self):
        seq = identity.parse(OBJECT)
        par = identity.parse(clone(OBJECT).replace("wf__par_seq_mix_words-0x4", "wf_other-0x4"))
        self.assertEqual(identity.compare_clones(seq, par, [])[0]["wf_workload"], "different")

    def test_an_empty_object_is_not_a_pass(self):
        pairs, failures = identity.compare_clones(identity.parse(OBJECT), {}, ["wf_workload"])
        self.assertEqual(set(pairs.values()), {"missing"})
        self.assertTrue(failures)

    def test_a_required_symbol_must_exist(self):
        _, failures = identity.compare_clones({}, {}, ["wf_workload"])
        self.assertTrue(failures)

    def test_overlapped_world_control(self):
        seq = identity.parse(OBJECT)
        overlapped = identity.parse(OBJECT.replace("push   %rbp", "push   %r15"))
        self.assertNotEqual(seq["wf_workload"], overlapped["wf_workload"])

    def test_padding_and_addresses_do_not_matter(self):
        shifted = OBJECT.replace("0000000000000040", "0000000000000080").replace("  40:", "  80:").replace("  42:", "  82:")
        self.assertEqual(identity.parse(OBJECT), identity.parse(shifted))


class Program(unittest.TestCase):
    def test_generated_source_has_the_steps_and_the_pair(self):
        text = generate.source(5)
        self.assertEqual(len(re.findall(r"let a\d+ = irotl", text)), 5)
        self.assertIn("let a = mix_words(", text)
        self.assertIn("let b = mix_words(", text)
        self.assertNotIn("//", text)

    def test_steps_grow_the_source(self):
        self.assertGreater(len(generate.source(9)), len(generate.source(3)))

    def test_ledger_line_is_parsed(self):
        self.assertEqual(int(run.OMITTED.search(LEDGER).group(1)), 35791)
        self.assertIsNone(run.OMITTED.search("PAR actualization  workload  call grain: priced offer of mix_words"))

    def test_reference_formulas_match_the_header(self):
        header = Path(__file__).with_name("pair_reference.h").read_text()
        self.assertIn("1 + (k * 7) % 61", header)
        self.assertIn("6364136223846793005", header)
        self.assertIn("1442695040888963407", header)
        self.assertEqual(generate.rotation(3), 1 + (3 * 7) % 61)
        self.assertEqual(generate.multiplier(0) & 1, 1)
        self.assertLess(generate.multiplier(40), 1 << 63)


if __name__ == "__main__":
    unittest.main()
