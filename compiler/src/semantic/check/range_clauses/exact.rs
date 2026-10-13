//! Exact normalization of range terms [RANGE-1].
//!
//! A range term denotes a mathematical integer, so folding its constants and
//! scaling its terms loses nothing. An intermediate value may pass `i128`
//! while the normalized side fits, as `a * (a * 1) - a * (b * 1)` does for
//! a = 2^64 - 1 and b = 2^63; only the final coefficients and constant are
//! narrowed to the `i128` of [`CheckedRangeTerm`].

use std::cmp::Ordering;

use super::CheckedRangeTerm;

/// A signed integer of any magnitude: little-endian 64-bit limbs with no
/// high zero limb, so zero has no limbs and is never negative.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Exact {
    negative: bool,
    limbs: Vec<u64>,
}

impl From<i128> for Exact {
    fn from(value: i128) -> Self {
        let magnitude = value.unsigned_abs();
        Self::signed(
            value < 0,
            trimmed(vec![magnitude as u64, (magnitude >> 64) as u64]),
        )
    }
}

impl Exact {
    fn signed(negative: bool, limbs: Vec<u64>) -> Self {
        Self {
            negative: negative && !limbs.is_empty(),
            limbs,
        }
    }

    pub(super) fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub(super) fn plus(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            return Self::signed(self.negative, magnitude_sum(&self.limbs, &other.limbs));
        }
        match magnitude_order(&self.limbs, &other.limbs) {
            Ordering::Equal => Self::default(),
            Ordering::Greater => Self::signed(
                self.negative,
                magnitude_difference(&self.limbs, &other.limbs),
            ),
            Ordering::Less => Self::signed(
                other.negative,
                magnitude_difference(&other.limbs, &self.limbs),
            ),
        }
    }

    pub(super) fn times(&self, other: &Self) -> Self {
        Self::signed(
            self.negative != other.negative,
            magnitude_product(&self.limbs, &other.limbs),
        )
    }

    /// The value as an `i128`, or `None` where it lies outside.
    pub(super) fn narrowed(&self) -> Option<i128> {
        let magnitude = match self.limbs.as_slice() {
            [] => 0,
            [low] => u128::from(*low),
            [low, high] => u128::from(*low) | (u128::from(*high) << 64),
            _ => return None,
        };
        if self.negative {
            0_i128.checked_sub_unsigned(magnitude)
        } else {
            i128::try_from(magnitude).ok()
        }
    }
}

fn trimmed(mut limbs: Vec<u64>) -> Vec<u64> {
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    limbs
}

fn limb(limbs: &[u64], position: usize) -> u64 {
    limbs.get(position).copied().unwrap_or(0)
}

fn magnitude_order(left: &[u64], right: &[u64]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        unequal => unequal,
    }
}

fn magnitude_sum(left: &[u64], right: &[u64]) -> Vec<u64> {
    let mut limbs = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0_u128;
    for position in 0..left.len().max(right.len()) {
        let sum = u128::from(limb(left, position)) + u128::from(limb(right, position)) + carry;
        limbs.push(sum as u64);
        carry = sum >> 64;
    }
    limbs.push(carry as u64);
    trimmed(limbs)
}

/// `larger - smaller`, where `larger` is at least `smaller` in magnitude.
fn magnitude_difference(larger: &[u64], smaller: &[u64]) -> Vec<u64> {
    let mut limbs = Vec::with_capacity(larger.len());
    let mut borrow = false;
    for (position, &high) in larger.iter().enumerate() {
        let (difference, under) = high.overflowing_sub(limb(smaller, position));
        let (difference, under_borrow) = difference.overflowing_sub(u64::from(borrow));
        limbs.push(difference);
        borrow = under || under_borrow;
    }
    trimmed(limbs)
}

fn magnitude_product(left: &[u64], right: &[u64]) -> Vec<u64> {
    let mut limbs = vec![0_u64; left.len() + right.len()];
    for (at, &factor) in left.iter().enumerate() {
        let mut carry = 0_u128;
        for (offset, &other) in right.iter().enumerate() {
            // At most (2^64 - 1)^2 + 2 * (2^64 - 1) = 2^128 - 1: no overflow.
            let product =
                u128::from(factor) * u128::from(other) + u128::from(limbs[at + offset]) + carry;
            limbs[at + offset] = product as u64;
            carry = product >> 64;
        }
        limbs[at + right.len()] = carry as u64;
    }
    trimmed(limbs)
}

/// An exact weighted sum of range terms while one side is formed: equal
/// terms merge and a zero weight drops its term.
#[derive(Debug, Default)]
pub(super) struct ExactSum {
    terms: Vec<(Exact, CheckedRangeTerm)>,
    constant: Exact,
}

impl ExactSum {
    /// `term` as a sum: a constant folds, a sum flattens, and any other term
    /// stands with weight one.
    pub(super) fn of(term: CheckedRangeTerm) -> Self {
        let mut sum = Self::default();
        sum.add_term(&Exact::from(1_i128), term);
        sum
    }

    fn add_term(&mut self, weight: &Exact, term: CheckedRangeTerm) {
        match term {
            CheckedRangeTerm::Constant(value) => {
                self.constant = self.constant.plus(&weight.times(&Exact::from(value)));
            }
            CheckedRangeTerm::Sum { terms, constant } => {
                self.constant = self.constant.plus(&weight.times(&Exact::from(constant)));
                for (coefficient, nested) in terms {
                    self.add_term(&weight.times(&Exact::from(coefficient)), nested);
                }
            }
            other => self.add_weighted(weight.clone(), other),
        }
    }

    fn add_weighted(&mut self, weight: Exact, term: CheckedRangeTerm) {
        if let Some(position) = self.terms.iter().position(|(_, held)| *held == term) {
            let merged = self.terms[position].0.plus(&weight);
            if merged.is_zero() {
                self.terms.remove(position);
            } else {
                self.terms[position].0 = merged;
            }
        } else if !weight.is_zero() {
            self.terms.push((weight, term));
        }
    }

    /// Adds `weight` times `other`.
    pub(super) fn add(&mut self, weight: &Exact, other: Self) {
        self.constant = self.constant.plus(&weight.times(&other.constant));
        for (coefficient, term) in other.terms {
            self.add_weighted(weight.times(&coefficient), term);
        }
    }

    /// The sum's value where it names no term.
    pub(super) fn constant(&self) -> Option<&Exact> {
        self.terms.is_empty().then_some(&self.constant)
    }

    /// The canonical range term: terms in order, a constant alone where no
    /// term remains, and a single unit term standing alone. `None` where a
    /// final coefficient or the constant lies outside `i128`.
    pub(super) fn narrowed(mut self) -> Option<CheckedRangeTerm> {
        self.terms.sort_by(|left, right| left.1.cmp(&right.1));
        let constant = self.constant.narrowed()?;
        let mut terms = self
            .terms
            .into_iter()
            .map(|(weight, term)| Some((weight.narrowed()?, term)))
            .collect::<Option<Vec<_>>>()?;
        if terms.is_empty() {
            return Some(CheckedRangeTerm::Constant(constant));
        }
        if constant == 0 && terms.len() == 1 && terms[0].0 == 1 {
            return terms.pop().map(|(_, term)| term);
        }
        Some(CheckedRangeTerm::Sum { terms, constant })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_U64: i128 = u64::MAX as i128;

    fn constant(value: i128) -> ExactSum {
        ExactSum::of(CheckedRangeTerm::Constant(value))
    }

    /// `left * (right * 1)`, scaled as `range_product` scales a factor.
    fn product(left: i128, right: i128) -> ExactSum {
        let mut inner = ExactSum::default();
        inner.add(&Exact::from(right), constant(1));
        let mut outer = ExactSum::default();
        outer.add(&Exact::from(left), inner);
        outer
    }

    fn value(position: u32) -> CheckedRangeTerm {
        CheckedRangeTerm::Bound(position)
    }

    #[test]
    fn exact_arithmetic_matches_i128_where_both_are_defined() {
        let samples = [
            0,
            1,
            -1,
            MAX_U64,
            -MAX_U64,
            i128::MAX,
            i128::MIN,
            i128::MIN + 1,
            1 << 64,
            -(1 << 64),
        ];
        for left in samples {
            assert_eq!(Exact::from(left).narrowed(), Some(left));
            for right in samples {
                assert_eq!(
                    Exact::from(left).plus(&Exact::from(right)).narrowed(),
                    left.checked_add(right),
                    "{left} + {right}"
                );
                assert_eq!(
                    Exact::from(left).times(&Exact::from(right)).narrowed(),
                    left.checked_mul(right),
                    "{left} * {right}"
                );
            }
        }
    }

    #[test]
    fn a_side_whose_product_passes_i128_normalizes_to_its_exact_value() {
        // (2^64 - 1) * ((2^64 - 1) * 1) - (2^64 - 1) * (2^63 * 1), the bound
        // of the `range3-*-term-product-beyond-i128` conformance cases: the
        // first product is 2^128 - 2^65 + 1, past i128::MAX, and the side is
        // (2^64 - 1) * (2^63 - 1). Saturating the first product at i128::MAX
        // instead gave 2^127 - 1 - (2^64 - 1) * 2^63 = 2^63 - 1.
        let mut side = ExactSum::default();
        side.add(&Exact::from(1_i128), product(MAX_U64, MAX_U64));
        side.add(&Exact::from(-1_i128), product(MAX_U64, 1 << 63));
        assert_eq!(
            side.narrowed(),
            Some(CheckedRangeTerm::Constant(MAX_U64 * ((1 << 63) - 1)))
        );
    }

    #[test]
    fn a_coefficient_whose_partial_sum_passes_i128_normalizes_exactly() {
        // -2^126 * x - 2^126 * x + x + 2^126 * x + 2^126 * x: the term's
        // final weight is 1. The saturating sum this replaced popped its
        // terms last first, so its partial weight 2^126 + 2^126 clamped to
        // 2^127 - 1, and the side came out as -x.
        let mut side = ExactSum::default();
        for weight in [-(1_i128 << 126), -(1 << 126), 1, 1 << 126, 1 << 126] {
            side.add(&Exact::from(weight), ExactSum::of(value(0)));
        }
        assert_eq!(side.narrowed(), Some(value(0)));
    }

    #[test]
    fn a_final_value_outside_i128_narrows_to_nothing_rather_than_a_bound() {
        assert_eq!(product(MAX_U64, MAX_U64).narrowed(), None);
        let mut scaled = ExactSum::default();
        scaled.add(&Exact::from(i128::MAX), ExactSum::of(value(0)));
        scaled.add(&Exact::from(i128::MAX), ExactSum::of(value(0)));
        assert_eq!(scaled.narrowed(), None);
        // The lowest i128 is still one.
        let mut lowest = ExactSum::default();
        lowest.add(&Exact::from(i128::MIN + 1), constant(1));
        lowest.add(&Exact::from(-1_i128), constant(1));
        assert_eq!(
            lowest.narrowed(),
            Some(CheckedRangeTerm::Constant(i128::MIN))
        );
    }

    #[test]
    fn narrowing_keeps_the_canonical_shapes() {
        let mut cancelled = ExactSum::of(value(1));
        cancelled.add(&Exact::from(-1_i128), ExactSum::of(value(1)));
        assert_eq!(cancelled.narrowed(), Some(CheckedRangeTerm::Constant(0)));
        let mut sum = ExactSum::of(CheckedRangeTerm::Iteration(0));
        sum.add(&Exact::from(2_i128), ExactSum::of(value(0)));
        sum.add(&Exact::from(1_i128), constant(3));
        assert_eq!(
            sum.narrowed(),
            Some(CheckedRangeTerm::Sum {
                terms: vec![(2, value(0)), (1, CheckedRangeTerm::Iteration(0))],
                constant: 3,
            })
        );
    }
}
