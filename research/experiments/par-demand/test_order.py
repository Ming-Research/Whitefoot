"""Pair-balance regressions use only generated orders, never native timing."""
import itertools
import unittest
from collections import Counter
from measure import balanced_order, require_paired_rounds


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
