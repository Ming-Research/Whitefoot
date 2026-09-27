//! The [ENT-3.S7] operation table: the result bounds and operand relations
//! one integer operation row establishes from its operands' intervals.
//!
//! This module is table data and nothing else. It reads no fact state and
//! interns no term: the caller reads each operand's interval from the closed
//! state where the operation was evaluated and establishes what this returns.
//! Every table value is a mathematical integer computed with checked `i128`
//! arithmetic, and a row whose computation is not representable establishes
//! nothing, which only under-derives [ENT-1].

use super::super::super::model::{CheckedIntegerOperation, IntegerType};

/// One integer type's width, signedness and closed interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Span {
    width: u32,
    signed: bool,
    minimum: i128,
    maximum: i128,
}

impl Span {
    /// A `width`-bit two's-complement or unsigned type, `width` in `1..=64`.
    pub(super) const fn new(width: u32, signed: bool) -> Self {
        let (minimum, maximum) = if signed {
            (-(1_i128 << (width - 1)), (1_i128 << (width - 1)) - 1)
        } else {
            (0, (1_i128 << width) - 1)
        };
        Self {
            width,
            signed,
            minimum,
            maximum,
        }
    }

    pub(super) const fn of(ty: IntegerType) -> Self {
        Self::new(ty.width() as u32, ty.signed())
    }

    /// This type's own interval.
    pub(super) const fn interval(self) -> Interval {
        Interval::new(self.minimum, self.maximum)
    }

    const fn holds(self, interval: Interval) -> bool {
        self.minimum <= interval.low && interval.high <= self.maximum
    }

    fn clamp(self, value: i128) -> i128 {
        value.clamp(self.minimum, self.maximum)
    }

    const fn mask(self) -> u128 {
        (1_u128 << self.width) - 1
    }

    /// The type's bit pattern of one of its values.
    fn bits(self, value: i128) -> u128 {
        value.cast_unsigned() & self.mask()
    }

    /// The value whose pattern is the low `width` bits of `bits`.
    fn value_of_bits(self, bits: u128) -> i128 {
        let bits = bits & self.mask();
        let value = bits.cast_signed();
        if self.signed && bits >> (self.width - 1) == 1 {
            value - (1_i128 << self.width)
        } else {
            value
        }
    }

    /// [OP-2] `wrap_T(z)`: the member of T congruent to z modulo `2^width`.
    fn wrap(self, value: i128) -> i128 {
        self.value_of_bits(value.cast_unsigned())
    }
}

/// One closed operand or result interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Interval {
    pub(super) low: i128,
    pub(super) high: i128,
}

impl Interval {
    pub(super) const fn new(low: i128, high: i128) -> Self {
        Self { low, high }
    }

    pub(super) const fn value(value: i128) -> Self {
        Self::new(value, value)
    }

    const fn single(self) -> Option<i128> {
        if self.low == self.high {
            Some(self.low)
        } else {
            None
        }
    }

    fn join(self, other: Self) -> Self {
        Self::new(self.low.min(other.low), self.high.max(other.high))
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let low = self.low.max(other.low);
        let high = self.high.min(other.high);
        (low <= high).then_some(Self::new(low, high))
    }
}

/// One row of the [ENT-3.S7] table. `wrap` distinguishes a `.wrap` form's
/// condition and single-value result from its exact row's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Row {
    Add { wrap: bool },
    Subtract { wrap: bool },
    Multiply { wrap: bool },
    AddSaturating,
    SubtractSaturating,
    MultiplySaturating,
    Divide,
    Remainder,
    Negate { wrap: bool },
    Absolute { wrap: bool },
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft { wrap: bool },
    ShiftRight { wrap: bool },
    Minimum,
    Maximum,
    MultiplyHigh,
    PopulationCount,
    LeadingZeros,
    TrailingZeros,
    RotateLeft,
    RotateRight,
    ByteSwap,
    Reinterpret,
}

impl Row {
    /// The row of one integer-valued operation. Defined, checked and
    /// comparison rows produce no integer and have none.
    pub(super) const fn of(operation: CheckedIntegerOperation) -> Option<Self> {
        use CheckedIntegerOperation as Operation;
        Some(match operation {
            Operation::AddWrap => Self::Add { wrap: true },
            Operation::AddExact => Self::Add { wrap: false },
            Operation::SubtractWrap => Self::Subtract { wrap: true },
            Operation::SubtractExact => Self::Subtract { wrap: false },
            Operation::MultiplyWrap => Self::Multiply { wrap: true },
            Operation::MultiplyExact => Self::Multiply { wrap: false },
            Operation::AddSaturating => Self::AddSaturating,
            Operation::SubtractSaturating => Self::SubtractSaturating,
            Operation::MultiplySaturating => Self::MultiplySaturating,
            Operation::DivideExact => Self::Divide,
            Operation::RemainderExact => Self::Remainder,
            Operation::NegateWrap => Self::Negate { wrap: true },
            Operation::NegateExact => Self::Negate { wrap: false },
            Operation::AbsoluteWrap => Self::Absolute { wrap: true },
            Operation::AbsoluteExact => Self::Absolute { wrap: false },
            Operation::BitNot => Self::BitNot,
            Operation::BitAnd => Self::BitAnd,
            Operation::BitOr => Self::BitOr,
            Operation::BitXor => Self::BitXor,
            Operation::ShiftLeftWrap => Self::ShiftLeft { wrap: true },
            Operation::ShiftLeftExact => Self::ShiftLeft { wrap: false },
            Operation::ShiftRightWrap => Self::ShiftRight { wrap: true },
            Operation::ShiftRightExact => Self::ShiftRight { wrap: false },
            Operation::Minimum => Self::Minimum,
            Operation::Maximum => Self::Maximum,
            Operation::MultiplyHigh => Self::MultiplyHigh,
            Operation::PopulationCount => Self::PopulationCount,
            Operation::LeadingZeros => Self::LeadingZeros,
            Operation::TrailingZeros => Self::TrailingZeros,
            Operation::RotateLeft => Self::RotateLeft,
            Operation::RotateRight => Self::RotateRight,
            Operation::ByteSwap => Self::ByteSwap,
            Operation::AddDefined
            | Operation::SubtractDefined
            | Operation::MultiplyDefined
            | Operation::DivideDefined
            | Operation::RemainderDefined
            | Operation::AbsoluteDefined
            | Operation::NegateDefined
            | Operation::ShiftLeftDefined
            | Operation::ShiftRightDefined
            | Operation::AddChecked
            | Operation::SubtractChecked
            | Operation::MultiplyChecked
            | Operation::DivideChecked
            | Operation::RemainderChecked
            | Operation::AbsoluteChecked
            | Operation::NegateChecked
            | Operation::Equal
            | Operation::NotEqual
            | Operation::Less
            | Operation::LessEqual
            | Operation::Greater
            | Operation::GreaterEqual => return None,
        })
    }

    /// The exact row whose facts a checked row's success payload receives
    /// [ENT-5].
    pub(super) const fn of_checked(operation: CheckedIntegerOperation) -> Option<Self> {
        use CheckedIntegerOperation as Operation;
        Some(match operation {
            Operation::AddChecked => Self::Add { wrap: false },
            Operation::SubtractChecked => Self::Subtract { wrap: false },
            Operation::MultiplyChecked => Self::Multiply { wrap: false },
            Operation::DivideChecked => Self::Divide,
            Operation::RemainderChecked => Self::Remainder,
            Operation::NegateChecked => Self::Negate { wrap: false },
            Operation::AbsoluteChecked => Self::Absolute { wrap: false },
            _ => return None,
        })
    }
}

/// One relation between the result r and one operand x: `r - x` lies in
/// `[low, high]`, where an absent side is unbounded. `r <= x` is
/// `high: Some(0)`, `r < x` is `high: Some(-1)`, `x <= r` is `low: Some(0)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OperandRelation {
    pub(super) operand: usize,
    pub(super) low: Option<i128>,
    pub(super) high: Option<i128>,
}

/// What one row establishes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Facts {
    /// The result bounds, already limited to the result type.
    pub(super) bounds: Option<Interval>,
    pub(super) relations: Vec<OperandRelation>,
}

/// [ENT-3.S7] the facts one row establishes over its operands' intervals.
///
/// `operand` is the operation's selected type T and `result` the row's result
/// type (T, `u32` for a count, the destination for `reinterpret`); for a shift
/// or rotation the second interval is the amount. `product` is the interval
/// [ENT-6]'s interval-product rule proved when it discharged an exact
/// multiplication's domain.
pub(super) fn facts(
    row: Row,
    operand: Span,
    result: Span,
    operands: &[Interval],
    product: Option<Interval>,
) -> Facts {
    let table = table(row, operand, result, operands, product);
    let single: Option<Vec<i128>> = operands.iter().map(|interval| interval.single()).collect();
    let bounds = match single {
        Some(values) => exact(row, operand, result, &values).map(Interval::value),
        None => table
            .as_ref()
            .and_then(|(bounds, _)| bounds.intersect(result.interval())),
    };
    Facts {
        bounds,
        relations: table.map(|(_, relations)| relations).unwrap_or_default(),
    }
}

const fn offset(operand: usize, low: i128, high: i128) -> OperandRelation {
    OperandRelation {
        operand,
        low: Some(low),
        high: Some(high),
    }
}

/// `r - x <= high`.
const fn at_most(operand: usize, high: i128) -> OperandRelation {
    OperandRelation {
        operand,
        low: None,
        high: Some(high),
    }
}

/// `r - x >= low`.
const fn at_least(operand: usize, low: i128) -> OperandRelation {
    OperandRelation {
        operand,
        low: Some(low),
        high: None,
    }
}

/// The least and greatest value of `f` at the four corners of `a` by `b`.
fn hull2(a: Interval, b: Interval, f: impl Fn(i128, i128) -> Option<i128>) -> Option<Interval> {
    let corners = [
        f(a.low, b.low)?,
        f(a.low, b.high)?,
        f(a.high, b.low)?,
        f(a.high, b.high)?,
    ];
    let low = corners.iter().copied().min()?;
    let high = corners.iter().copied().max()?;
    Some(Interval::new(low, high))
}

/// An interval divided into its negative part and its nonnegative part, the
/// nonnegative part starting at `start` (one for a divisor, zero otherwise).
fn split(interval: Interval, start: i128) -> impl Iterator<Item = Interval> {
    let negative = (interval.low <= -1).then(|| Interval::new(interval.low, interval.high.min(-1)));
    let nonnegative =
        (interval.high >= start).then(|| Interval::new(interval.low.max(start), interval.high));
    negative.into_iter().chain(nonnegative)
}

/// The number of binary digits of `value >= 0`, zero for zero.
fn bitlen(value: i128) -> i128 {
    i128::from(128 - value.leading_zeros())
}

/// `2^bitlen(value) - 1`, the greatest value with no more digits.
fn digits_ceiling(value: i128) -> Option<i128> {
    1_i128
        .checked_shl(u32::try_from(bitlen(value)).ok()?)?
        .checked_sub(1)
}

/// [OP-8] the amounts a shift by `amount` may apply: the amount itself when
/// every value is below the width, and otherwise any amount a `.wrap` mask
/// can produce. An exact shift's discharged domain already keeps the amount
/// below the width.
fn shift_amounts(amount: Interval, width: u32) -> Interval {
    let last = i128::from(width) - 1;
    if amount.low >= 0 && amount.high <= last {
        amount
    } else {
        Interval::new(0, last)
    }
}

fn power_of_two(exponent: i128) -> Option<i128> {
    1_i128.checked_shl(u32::try_from(exponent).ok()?)
}

/// The row's table entry when its condition holds and each computation is
/// representable: its result bounds and its relations.
fn table(
    row: Row,
    t: Span,
    result: Span,
    operands: &[Interval],
    product: Option<Interval>,
) -> Option<(Interval, Vec<OperandRelation>)> {
    let a = *operands.first()?;
    let second = operands.get(1).copied();
    let width = i128::from(t.width);
    Some(match row {
        Row::Add { wrap } => {
            let b = second?;
            let hull = Interval::new(a.low.checked_add(b.low)?, a.high.checked_add(b.high)?);
            if wrap && !t.holds(hull) {
                return None;
            }
            (
                hull,
                vec![offset(0, b.low, b.high), offset(1, a.low, a.high)],
            )
        }
        Row::Subtract { wrap } => {
            let b = second?;
            let hull = Interval::new(a.low.checked_sub(b.high)?, a.high.checked_sub(b.low)?);
            if wrap && !t.holds(hull) {
                return None;
            }
            let relation = offset(0, b.high.checked_neg()?, b.low.checked_neg()?);
            (hull, vec![relation])
        }
        Row::Multiply { wrap } => {
            let b = second?;
            let hull = match product {
                Some(product) if !wrap => product,
                _ => hull2(a, b, i128::checked_mul)?,
            };
            if wrap && !t.holds(hull) {
                return None;
            }
            (hull, Vec::new())
        }
        Row::AddSaturating | Row::SubtractSaturating | Row::MultiplySaturating => {
            let b = second?;
            let hull = match row {
                Row::AddSaturating => {
                    Interval::new(a.low.checked_add(b.low)?, a.high.checked_add(b.high)?)
                }
                Row::SubtractSaturating => {
                    Interval::new(a.low.checked_sub(b.high)?, a.high.checked_sub(b.low)?)
                }
                _ => hull2(a, b, i128::checked_mul)?,
            };
            let bounds = Interval::new(t.clamp(hull.low), t.clamp(hull.high));
            let relations = match row {
                Row::AddSaturating if a.low >= 0 && b.low >= 0 => {
                    vec![at_least(0, 0), at_least(1, 0)]
                }
                Row::SubtractSaturating if a.low >= 0 && b.low >= 0 => vec![at_most(0, 0)],
                _ => Vec::new(),
            };
            (bounds, relations)
        }
        Row::Divide => {
            let b = second?;
            let mut hull: Option<Interval> = None;
            for divisor in split(b, 1) {
                let part = hull2(a, divisor, i128::checked_div)?;
                hull = Some(hull.map_or(part, |hull| hull.join(part)));
            }
            let relations = if a.low >= 0 && b.low >= 0 {
                vec![at_most(0, 0)]
            } else {
                Vec::new()
            };
            (hull?, relations)
        }
        Row::Remainder => {
            let b = second?;
            let divisor = b.low.checked_abs()?.max(b.high.checked_abs()?);
            if divisor == 0 {
                return None;
            }
            let bounds = Interval::new(
                a.low.max(1 - divisor).min(0),
                a.high.min(divisor - 1).max(0),
            );
            let relations = if a.low >= 0 && b.low >= 0 {
                vec![at_most(0, 0), at_most(1, -1)]
            } else {
                Vec::new()
            };
            (bounds, relations)
        }
        Row::Negate { wrap } => {
            if wrap && a.low <= t.minimum {
                return None;
            }
            (
                Interval::new(a.high.checked_neg()?, a.low.checked_neg()?),
                Vec::new(),
            )
        }
        Row::Absolute { wrap } => {
            if wrap && a.low <= t.minimum {
                return None;
            }
            let mut hull: Option<Interval> = None;
            for part in split(a, 0) {
                let magnitudes = [part.low.checked_abs()?, part.high.checked_abs()?];
                let part = Interval::new(
                    magnitudes.iter().copied().min()?,
                    magnitudes.iter().copied().max()?,
                );
                hull = Some(hull.map_or(part, |hull| hull.join(part)));
            }
            (hull?, vec![at_least(0, 0)])
        }
        Row::BitNot => {
            let sum = t.maximum + t.minimum;
            (
                Interval::new(sum.checked_sub(a.high)?, sum.checked_sub(a.low)?),
                Vec::new(),
            )
        }
        Row::BitAnd => {
            let b = second?;
            let nonnegative = [a, b]
                .into_iter()
                .enumerate()
                .filter(|(_, interval)| interval.low >= 0)
                .collect::<Vec<_>>();
            let high = nonnegative
                .iter()
                .map(|(_, interval)| interval.high)
                .min()?;
            let relations = nonnegative
                .iter()
                .map(|(operand, _)| at_most(*operand, 0))
                .collect();
            (Interval::new(0, high), relations)
        }
        Row::BitOr | Row::BitXor => {
            let b = second?;
            if a.low < 0 || b.low < 0 {
                return None;
            }
            let high = a
                .high
                .checked_add(b.high)?
                .min(digits_ceiling(a.high.max(b.high))?);
            if row == Row::BitOr {
                (
                    Interval::new(a.low.max(b.low), high),
                    vec![at_least(0, 0), at_least(1, 0)],
                )
            } else {
                (Interval::new(0, high), Vec::new())
            }
        }
        Row::ShiftLeft { .. } => {
            let amounts = shift_amounts(second?, t.width);
            if a.low < 0 {
                return None;
            }
            let hull = Interval::new(
                a.low.checked_mul(power_of_two(amounts.low)?)?,
                a.high.checked_mul(power_of_two(amounts.high)?)?,
            );
            if !t.holds(hull) {
                return None;
            }
            (hull, Vec::new())
        }
        Row::ShiftRight { .. } => {
            let amounts = shift_amounts(second?, t.width);
            let hull = hull2(a, amounts, |value, amount| {
                Some(value >> u32::try_from(amount).ok()?)
            })?;
            let relations = if a.low >= 0 {
                vec![at_most(0, 0)]
            } else {
                Vec::new()
            };
            (hull, relations)
        }
        Row::Minimum => {
            let b = second?;
            (
                Interval::new(a.low.min(b.low), a.high.min(b.high)),
                vec![at_most(0, 0), at_most(1, 0)],
            )
        }
        Row::Maximum => {
            let b = second?;
            (
                Interval::new(a.low.max(b.low), a.high.max(b.high)),
                vec![at_least(0, 0), at_least(1, 0)],
            )
        }
        Row::MultiplyHigh => {
            let b = second?;
            let hull = hull2(a, b, |left, right| {
                Some(left.checked_mul(right)? >> t.width)
            })?;
            let relations = if a.low >= 0 && b.low >= 0 {
                vec![at_most(0, 0), at_most(1, 0)]
            } else {
                Vec::new()
            };
            (hull, relations)
        }
        Row::PopulationCount => {
            let bounds = if a.low >= 0 {
                Interval::new(i128::from(a.low >= 1), bitlen(a.high))
            } else {
                Interval::new(0, width)
            };
            (bounds, Vec::new())
        }
        Row::LeadingZeros => {
            let bounds = if a.low >= 0 {
                Interval::new(width - bitlen(a.high), width - bitlen(a.low))
            } else {
                Interval::new(0, width)
            };
            (bounds, Vec::new())
        }
        Row::TrailingZeros => {
            let bounds = if a.low >= 1 {
                Interval::new(0, bitlen(a.high) - 1)
            } else {
                Interval::new(0, width)
            };
            (bounds, Vec::new())
        }
        Row::RotateLeft | Row::RotateRight | Row::ByteSwap => return None,
        Row::Reinterpret => {
            let modulus = 1_i128 << t.width;
            if result.holds(a) {
                (a, vec![offset(0, 0, 0)])
            } else if a.high < 0 && !result.signed {
                (Interval::new(a.low + modulus, a.high + modulus), Vec::new())
            } else if result.signed && a.low > result.maximum {
                (Interval::new(a.low - modulus, a.high - modulus), Vec::new())
            } else {
                return None;
            }
        }
    })
}

/// [OP-8] the amount one shift applies: the written amount for an exact
/// shift, whose domain keeps it below the width, and its masked value for a
/// `.wrap` shift.
fn applied_shift(amount: i128, width: u32, wrap: bool) -> Option<u32> {
    let amount = u32::try_from(amount).ok()?;
    if wrap {
        Some(amount & (width - 1))
    } else {
        (amount < width).then_some(amount)
    }
}

fn rotate_left(t: Span, bits: u128, amount: u32) -> u128 {
    if amount == 0 {
        bits
    } else {
        ((bits << amount) | (bits >> (t.width - amount))) & t.mask()
    }
}

/// [OP-2, OP-8] the row's exact value at one value of each operand, or
/// `None` when the row has none there (a zero divisor, an exact result
/// outside T, an exact shift by the width or more).
fn exact(row: Row, t: Span, result: Span, values: &[i128]) -> Option<i128> {
    let a = *values.first()?;
    let second = values.get(1).copied();
    let value = match row {
        Row::Add { wrap } => {
            let sum = a.checked_add(second?)?;
            if wrap { t.wrap(sum) } else { sum }
        }
        Row::Subtract { wrap } => {
            let difference = a.checked_sub(second?)?;
            if wrap { t.wrap(difference) } else { difference }
        }
        Row::Multiply { wrap } => {
            let b = second?;
            if wrap {
                t.value_of_bits(t.bits(a).wrapping_mul(t.bits(b)))
            } else {
                a.checked_mul(b)?
            }
        }
        Row::AddSaturating => t.clamp(a.checked_add(second?)?),
        Row::SubtractSaturating => t.clamp(a.checked_sub(second?)?),
        Row::MultiplySaturating => {
            let b = second?;
            a.checked_mul(b).map_or_else(
                || {
                    if (a < 0) == (b < 0) {
                        t.maximum
                    } else {
                        t.minimum
                    }
                },
                |product| t.clamp(product),
            )
        }
        Row::Divide => a.checked_div(second?)?,
        Row::Remainder => a.checked_rem(second?)?,
        Row::Negate { wrap } => {
            let negated = a.checked_neg()?;
            if wrap { t.wrap(negated) } else { negated }
        }
        Row::Absolute { wrap } => {
            let magnitude = a.checked_abs()?;
            if wrap { t.wrap(magnitude) } else { magnitude }
        }
        Row::BitNot => t.maximum + t.minimum - a,
        Row::BitAnd => t.value_of_bits(t.bits(a) & t.bits(second?)),
        Row::BitOr => t.value_of_bits(t.bits(a) | t.bits(second?)),
        Row::BitXor => t.value_of_bits(t.bits(a) ^ t.bits(second?)),
        Row::ShiftLeft { wrap } => {
            let amount = applied_shift(second?, t.width, wrap)?;
            t.value_of_bits(t.bits(a) << amount)
        }
        Row::ShiftRight { wrap } => a >> applied_shift(second?, t.width, wrap)?,
        Row::Minimum => a.min(second?),
        Row::Maximum => a.max(second?),
        Row::MultiplyHigh => {
            let b = second?;
            if t.signed {
                a.checked_mul(b)? >> t.width
            } else {
                let product = u128::try_from(a)
                    .ok()?
                    .checked_mul(u128::try_from(b).ok()?)?
                    >> t.width;
                i128::try_from(product).ok()?
            }
        }
        Row::PopulationCount => i128::from(t.bits(a).count_ones()),
        Row::LeadingZeros => i128::from(t.bits(a).leading_zeros() - (128 - t.width)),
        Row::TrailingZeros => {
            let bits = t.bits(a);
            if bits == 0 {
                i128::from(t.width)
            } else {
                i128::from(bits.trailing_zeros())
            }
        }
        Row::RotateLeft | Row::RotateRight => {
            let amount = u32::try_from(second?.rem_euclid(i128::from(t.width))).ok()?;
            let left = if row == Row::RotateLeft {
                amount
            } else {
                (t.width - amount) % t.width
            };
            t.value_of_bits(rotate_left(t, t.bits(a), left))
        }
        Row::ByteSwap => {
            let bits = t.bits(a);
            let bytes = t.width / 8;
            let swapped = (0..bytes).fold(0_u128, |swapped, byte| {
                swapped | ((bits >> (8 * byte)) & 0xff) << (8 * (bytes - 1 - byte))
            });
            t.value_of_bits(swapped)
        }
        Row::Reinterpret => result.value_of_bits(t.bits(a)),
    };
    (result.minimum <= value && value <= result.maximum).then_some(value)
}

#[cfg(test)]
mod tests {
    //! Enumerative soundness of the table: for every operand interval box the
    //! test draws, each row's result bounds and relations must hold at every
    //! operand value the row admits there. The reference semantics below is
    //! written from [OP-2] and [OP-8] independently of `exact`, which the
    //! single-value case uses.

    use super::{Facts, Interval, Row, Span, facts};

    /// [OP-2, OP-8] reference semantics; `None` outside the row's domain.
    fn reference(row: Row, t: Span, result: Span, a: i128, b: i128) -> Option<i128> {
        let modulus = 1_i128 << t.width;
        let wrap = |z: i128| {
            let reduced = z.rem_euclid(modulus);
            if t.signed && reduced > t.maximum {
                reduced - modulus
            } else {
                reduced
            }
        };
        let in_t = |z: i128| (t.minimum..=t.maximum).contains(&z).then_some(z);
        let unsigned = |z: i128| z.rem_euclid(modulus);
        let signed_of = |bits: i128| {
            if t.signed && bits > t.maximum {
                bits - modulus
            } else {
                bits
            }
        };
        let value = match row {
            Row::Add { wrap: true } => wrap(a + b),
            Row::Add { wrap: false } => in_t(a + b)?,
            Row::Subtract { wrap: true } => wrap(a - b),
            Row::Subtract { wrap: false } => in_t(a - b)?,
            Row::Multiply { wrap: true } => wrap(a * b),
            Row::Multiply { wrap: false } => in_t(a * b)?,
            Row::AddSaturating => (a + b).clamp(t.minimum, t.maximum),
            Row::SubtractSaturating => (a - b).clamp(t.minimum, t.maximum),
            Row::MultiplySaturating => (a * b).clamp(t.minimum, t.maximum),
            Row::Divide => {
                if b == 0 {
                    return None;
                }
                let quotient = a.abs() / b.abs();
                in_t(if (a < 0) == (b < 0) {
                    quotient
                } else {
                    -quotient
                })?
            }
            Row::Remainder => {
                if b == 0 || in_t(a / b).is_none() {
                    return None;
                }
                a - b * (a / b)
            }
            Row::Negate { wrap: true } => wrap(-a),
            Row::Negate { wrap: false } => in_t(-a)?,
            Row::Absolute { wrap: true } => wrap(a.abs()),
            Row::Absolute { wrap: false } => in_t(a.abs())?,
            Row::BitNot => signed_of(unsigned(a) ^ (modulus - 1)),
            Row::BitAnd => signed_of(unsigned(a) & unsigned(b)),
            Row::BitOr => signed_of(unsigned(a) | unsigned(b)),
            Row::BitXor => signed_of(unsigned(a) ^ unsigned(b)),
            Row::ShiftLeft { wrap: shift_wrap } | Row::ShiftRight { wrap: shift_wrap } => {
                let width = i128::from(t.width);
                let amount = if shift_wrap {
                    b.rem_euclid(width)
                } else if b < width {
                    b
                } else {
                    return None;
                };
                if matches!(row, Row::ShiftLeft { .. }) {
                    wrap(a * (1_i128 << amount))
                } else {
                    a.div_euclid(1_i128 << amount)
                }
            }
            Row::Minimum => a.min(b),
            Row::Maximum => a.max(b),
            Row::MultiplyHigh => (a * b).div_euclid(modulus),
            Row::PopulationCount => i128::from(unsigned(a).count_ones()),
            Row::LeadingZeros => {
                let bits = unsigned(a);
                (0..i128::from(t.width))
                    .take_while(|index| bits >> (i128::from(t.width) - 1 - index) & 1 == 0)
                    .count() as i128
            }
            Row::TrailingZeros => {
                let bits = unsigned(a);
                (0..i128::from(t.width))
                    .take_while(|index| bits >> index & 1 == 0)
                    .count() as i128
            }
            Row::RotateLeft | Row::RotateRight | Row::ByteSwap => return None,
            Row::Reinterpret => {
                let bits = unsigned(a);
                if result.signed && bits > result.maximum {
                    bits - modulus
                } else {
                    bits
                }
            }
        };
        Some(value)
    }

    /// Whether the result `r` at operand values `a` and `b` satisfies every
    /// bound and relation the row established.
    fn holds(facts: &Facts, r: i128, a: i128, b: i128) -> bool {
        facts
            .bounds
            .is_none_or(|bounds| bounds.low <= r && r <= bounds.high)
            && facts.relations.iter().all(|relation| {
                let difference = r - if relation.operand == 0 { a } else { b };
                relation.low.is_none_or(|low| low <= difference)
                    && relation.high.is_none_or(|high| difference <= high)
            })
    }

    fn intervals(values: &[i128]) -> Vec<Interval> {
        let mut values = values.to_vec();
        values.sort_unstable();
        values.dedup();
        values
            .iter()
            .enumerate()
            .flat_map(|(index, low)| {
                values[index..]
                    .iter()
                    .map(move |high| Interval::new(*low, *high))
            })
            .collect()
    }

    const BINARY: [Row; 22] = [
        Row::Add { wrap: false },
        Row::Add { wrap: true },
        Row::Subtract { wrap: false },
        Row::Subtract { wrap: true },
        Row::Multiply { wrap: false },
        Row::Multiply { wrap: true },
        Row::AddSaturating,
        Row::SubtractSaturating,
        Row::MultiplySaturating,
        Row::Divide,
        Row::Remainder,
        Row::BitAnd,
        Row::BitOr,
        Row::BitXor,
        Row::Minimum,
        Row::Maximum,
        Row::MultiplyHigh,
        Row::ShiftLeft { wrap: false },
        Row::ShiftLeft { wrap: true },
        Row::ShiftRight { wrap: false },
        Row::ShiftRight { wrap: true },
        Row::RotateLeft,
    ];

    const UNARY: [Row; 9] = [
        Row::Negate { wrap: false },
        Row::Negate { wrap: true },
        Row::Absolute { wrap: false },
        Row::Absolute { wrap: true },
        Row::BitNot,
        Row::PopulationCount,
        Row::LeadingZeros,
        Row::TrailingZeros,
        Row::ByteSwap,
    ];

    const fn is_shift(row: Row) -> bool {
        matches!(
            row,
            Row::ShiftLeft { .. } | Row::ShiftRight { .. } | Row::RotateLeft | Row::RotateRight
        )
    }

    const fn signed_only(row: Row) -> bool {
        matches!(row, Row::Negate { .. } | Row::Absolute { .. })
    }

    const fn result_span(row: Row, t: Span) -> Span {
        match row {
            Row::PopulationCount | Row::LeadingZeros | Row::TrailingZeros => Span::new(32, false),
            _ => t,
        }
    }

    /// Checks every row over every box drawn from `values` (and `amounts` for
    /// a shift's second operand), at every operand value in the box.
    fn check_type(t: Span, values: &[i128], amounts: &[i128]) {
        let value_intervals = intervals(values);
        let amount_intervals = intervals(amounts);
        for row in BINARY {
            let result = result_span(row, t);
            let seconds = if is_shift(row) {
                &amount_intervals
            } else {
                &value_intervals
            };
            for a in &value_intervals {
                for b in seconds {
                    let facts = facts(row, t, result, &[*a, *b], None);
                    for x in a.low..=a.high {
                        for y in b.low..=b.high {
                            if let Some(r) = reference(row, t, result, x, y) {
                                assert!(
                                    holds(&facts, r, x, y),
                                    "{row:?} {t:?} a={a:?} b={b:?} x={x} y={y} r={r} {facts:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        for row in UNARY {
            if signed_only(row) && !t.signed {
                continue;
            }
            let result = result_span(row, t);
            for a in &value_intervals {
                let facts = facts(row, t, result, &[*a], None);
                for x in a.low..=a.high {
                    if let Some(r) = reference(row, t, result, x, 0) {
                        assert!(
                            holds(&facts, r, x, 0),
                            "{row:?} {t:?} a={a:?} x={x} r={r} {facts:?}"
                        );
                    }
                }
            }
        }
        let other = Span::new(t.width, !t.signed);
        for a in &value_intervals {
            let facts = facts(Row::Reinterpret, t, other, &[*a], None);
            for x in a.low..=a.high {
                let r = reference(Row::Reinterpret, t, other, x, 0).expect("total row");
                assert!(
                    holds(&facts, r, x, 0),
                    "Reinterpret {t:?} a={a:?} x={x} r={r} {facts:?}"
                );
            }
        }
    }

    #[test]
    fn every_four_bit_box_satisfies_the_table() {
        for signed in [false, true] {
            let t = Span::new(4, signed);
            let values = (t.minimum..=t.maximum).collect::<Vec<_>>();
            check_type(t, &values, &(0..=9).collect::<Vec<_>>());
        }
    }

    /// Boxes whose endpoints lie on each type's edges, around zero and at a
    /// power of two, checked at every operand value inside them.
    #[test]
    fn eight_bit_endpoint_boxes_satisfy_the_table() {
        check_type(
            Span::new(8, false),
            &[0, 1, 7, 128, 200, 255],
            &[0, 1, 7, 8, 40],
        );
        check_type(
            Span::new(8, true),
            &[-128, -65, -1, 0, 1, 127],
            &[0, 1, 7, 8, 40],
        );
    }

    /// Single values take the exact row value whether or not the row's
    /// interval condition holds, including the wrap and count edges.
    #[test]
    fn single_values_take_the_exact_row_value() {
        let u8_span = Span::new(8, false);
        let i8_span = Span::new(8, true);
        let u32_span = Span::new(32, false);
        let one = |value: i128| Interval::value(value);
        let cases: [(Row, Span, Span, &[Interval], i128); 12] = [
            (
                Row::Add { wrap: true },
                u8_span,
                u8_span,
                &[one(250), one(10)],
                4,
            ),
            (
                Row::ShiftLeft { wrap: true },
                u32_span,
                u32_span,
                &[one(1), one(40)],
                256,
            ),
            (
                Row::ShiftRight { wrap: false },
                i8_span,
                i8_span,
                &[one(-7), one(1)],
                -4,
            ),
            (
                Row::RotateLeft,
                u8_span,
                u8_span,
                &[one(0x81), one(1)],
                0x03,
            ),
            (
                Row::RotateRight,
                u8_span,
                u8_span,
                &[one(0x81), one(1)],
                0xc0,
            ),
            (
                Row::ByteSwap,
                Span::new(16, false),
                Span::new(16, false),
                &[one(0x1234)],
                0x3412,
            ),
            (Row::PopulationCount, i8_span, u32_span, &[one(-1)], 8),
            (Row::LeadingZeros, u8_span, u32_span, &[one(0)], 8),
            (Row::TrailingZeros, u8_span, u32_span, &[one(8)], 3),
            (
                Row::MultiplyHigh,
                Span::new(64, false),
                Span::new(64, false),
                &[one(u64::MAX.into()), one(u64::MAX.into())],
                i128::from(u64::MAX) - 1,
            ),
            (Row::Reinterpret, i8_span, u8_span, &[one(-1)], 255),
            (
                Row::Absolute { wrap: true },
                i8_span,
                i8_span,
                &[one(-128)],
                -128,
            ),
        ];
        for (row, t, result, operands, expected) in cases {
            assert_eq!(
                facts(row, t, result, operands, None).bounds,
                Some(Interval::value(expected)),
                "{row:?}"
            );
        }
    }

    /// The table cases the investigation's sweep and corpus sites need.
    #[test]
    fn the_reported_shift_and_its_neighbors_bound_their_results() {
        let u64_span = Span::new(64, false);
        let u32_amount = Span::new(32, false).interval();
        let shifted = facts(
            Row::ShiftRight { wrap: false },
            u64_span,
            u64_span,
            &[u64_span.interval(), Interval::value(32)],
            None,
        );
        assert_eq!(shifted.bounds, Some(Interval::new(0, 4_294_967_295)));
        let local_one = facts(
            Row::ShiftLeft { wrap: true },
            Span::new(32, false),
            Span::new(32, false),
            &[Interval::value(1), u32_amount],
            None,
        );
        assert_eq!(local_one.bounds, Some(Interval::new(1, 1 << 31)));
        let may_be_zero = facts(
            Row::ShiftLeft { wrap: true },
            Span::new(32, false),
            Span::new(32, false),
            &[Interval::new(0, 1), u32_amount],
            None,
        );
        assert_eq!(may_be_zero.bounds, Some(Interval::new(0, 1 << 31)));
        let wrapping_sum = facts(
            Row::Add { wrap: true },
            u64_span,
            u64_span,
            &[Interval::new(0, 4096), Interval::new(0, 65_535)],
            None,
        );
        assert_eq!(wrapping_sum.bounds, Some(Interval::new(0, 4096 + 65_535)));
        assert_eq!(wrapping_sum.relations.len(), 2);
    }
}
