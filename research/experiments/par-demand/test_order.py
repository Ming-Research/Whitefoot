"""Pair-balance regressions use only generated orders, never native timing."""
import itertools
import unittest
from collections import Counter
from measure import balanced_order, require_paired_rounds, ordered_cells
from summarize import parse_cells, cell_names, experiment_matrix, cell_widths, cell_arms, E6B


class OrderTests(unittest.TestCase):
    def test_six_and_nine_arms_balance_every_pair_at_all_width_offsets(self):
        for count, rounds, offset in itertools.product((6, 9), (30, 36), range(4)):
            items = tuple(range(count))
            orders = [balanced_order(items, r, offset) for r in range(rounds)]
            for first, second in itertools.combinations(items, 2):
                before = sum(order.index(first) < order.index(second) for order in orders)
                with self.subTest(count=count, rounds=rounds, offset=offset, pair=(first, second)):
                    self.assertEqual(before, rounds // 2)
            for r in range(0, rounds, 2):
                self.assertEqual(orders[r], orders[r + 1][::-1])

    def test_complete_rotation_blocks_balance_every_position(self):
        for count, offset in itertools.product((6, 9), range(4)):
            orders = [balanced_order(range(count), r, offset) for r in range(2 * count)]
            for item in range(count):
                self.assertEqual(Counter(order.index(item) for order in orders),
                                 Counter({position: 2 for position in range(count)}))

    def test_odd_decisive_count_refuses_without_rounding_but_sizing_is_unqualified(self):
        for rounds in (7, 29):
            with self.assertRaisesRegex(ValueError, f'protocol selected {rounds}; no rounding applied'):
                require_paired_rounds(rounds)
            require_paired_rounds(rounds, sizing=True)
        for rounds in (6, 30, 36): require_paired_rounds(rounds)
        for rounds in (0, -1, True):
            with self.assertRaises(ValueError): require_paired_rounds(rounds, sizing=True)

    def test_invalid_or_duplicate_arm_input_is_visible(self):
        for items, round_id in (([], 0), (['same', 'same'], 0), (['one'], -1)):
            with self.assertRaises(ValueError): balanced_order(items, round_id)

    def test_cells_parse_empty_known_and_reject_malformed_unknown_and_duplicate(self):
        _, _, manifest = experiment_matrix(E6B)
        complete = frozenset((name, width) for name in manifest for width in cell_widths(E6B, name))
        self.assertEqual(parse_cells("", E6B), complete)
        text = "recursion:1,mandelbrot:4,records:4,stencil:4,stencil:8"
        selected = parse_cells(text, E6B)
        self.assertEqual(cell_names(selected, E6B), text.split(","))
        self.assertEqual(parse_cells(text, 6), selected)
        for text in ("recursion", "recursion:1:4", ",", "recursion:-1", "recursion:1,", "recursion: 1"):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "malformed cell"):
                parse_cells(text, E6B)
        for text in ("missing:1", "recursion:2", "stencil:16"):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "unknown.*cell"):
                parse_cells(text, E6B)
        with self.assertRaisesRegex(ValueError, "duplicate cell"):
            parse_cells("stencil:4,stencil:04", E6B)
        with self.assertRaisesRegex(ValueError, "requires experiment 6 or 6b"):
            parse_cells("recursion:1", 3)

    def test_selected_cells_keep_full_matrix_round_and_arm_order(self):
        _, widths, manifest = experiment_matrix(E6B)
        selected = parse_cells("recursion:1,mandelbrot:4,records:4,stencil:4,stencil:8", E6B)
        for round_id in range(30):
            # Independent transcription of the unfiltered pre-selection loops.
            expected = []
            for name in balanced_order(manifest, round_id):
                for width in balanced_order(cell_widths(E6B, name), round_id):
                    arms = balanced_order(cell_arms(E6B, width, name), round_id, widths.index(width))
                    if (name, width) in selected:
                        expected.append((name, width, arms))
            self.assertEqual(list(ordered_cells(manifest, widths, E6B, round_id, selected)), expected)
