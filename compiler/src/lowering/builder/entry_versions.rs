//! Function-entry selection of the ordinary sequential computation.
//!
//! A function whose every reachable range-split decision can be stated over
//! values available at its entry gets one all-cheap choice per entry: when
//! every reachable site is below its grain, the emitter enters the canonical
//! ordinary closure instead of the parallel body
//! (compiler/parallel-lowering/two-worlds). The choice selects machine code
//! only; PAR-1 and PAR-2 already permit either computation.
//!
//! Coverage: range sites in the function and, through ordinary calls and
//! through a split's own chunk, in everything it reaches; immutable `u64`
//! formals, range and runtime-Array lengths of formals, and read-only Box-array
//! lengths of original formals, which are the leaves split-site pricing
//! already reads; and indexed sites, whose far-half setup charge is used when
//! its family counts are such leaves. A wrapping endpoint `lo +wrap e` is
//! bounded by `e`. Predicates of compile-time constants are folded.
//!
//! Left to local decisions: a value that varies after entry or is not such a
//! leaf (including a chunk's own endpoints and seed), a reachable call group,
//! recursion, a waiting function or a started context, and a function whose
//! entry block is a jump target. No source guard, operation, capture or
//! cleanup is moved or rewritten.

use std::collections::{BTreeMap, HashSet};

use super::loops::U64;
use super::work::{Environment, difference};
use crate::{
    IrEntryPredicate as Predicate, IrFunction, IrInstruction, IrIntegerOperation as Op,
    IrOperation, IrTerminator, IrValueId, IrWorkEstimate as Work,
};

/// The grain the range driver compares a remaining span against.
const GRAIN_LESS_ONE: u64 = super::call_grain::CALL_OFFER_WORK_UNIT - 1;

/// Every leaf is an entry value of `function` and every divisor is positive.
fn available(work: &Work, environment: &Environment<'_>) -> bool {
    environment.available(work) && positive_divisors(work)
}

fn positive_divisors(work: &Work) -> bool {
    match work {
        Work::Constant(_) | Work::Value(_) | Work::Length(_) | Work::BoxArrayLength(_) => true,
        Work::Sum(parts) => parts.iter().all(positive_divisors),
        Work::Product(left, right) | Work::Difference(left, right) => {
            positive_divisors(left) && positive_divisors(right)
        }
        Work::Quotient(value, divisor) => *divisor != 0 && positive_divisors(value),
    }
}

/// `upper - lower`, saturating at zero. Where it is an upper bound (`bound`),
/// `(lower +wrap extent) - lower` is `extent`: equal without wrapping, and
/// zero, which `extent` bounds, with it.
fn span(environment: &mut Environment<'_>, upper: Work, lower: Work, bound: bool) -> Work {
    if bound
        && let Work::Value(value) = &upper
        && let Some(IrOperation::Integer {
            operation: Op::AddWrap,
            operand_type,
            arguments,
        }) = environment.definition(*value)
        && *operand_type == U64
        && let [left, right] = arguments.as_slice()
    {
        let (left, right) = (*left, *right);
        if environment.scalar(left) == lower {
            return environment.scalar(right);
        }
        if environment.scalar(right) == lower {
            return environment.scalar(left);
        }
    }
    difference(upper, lower)
}

/// A work estimate over this function's values, its leaves observed the way
/// split-site pricing observes them.
fn normalize(work: &Work, environment: &mut Environment<'_>, bound: bool) -> Work {
    match work {
        Work::Constant(value) => Work::Constant(*value),
        Work::Value(value) => environment.scalar(*value),
        Work::Length(value) => environment.length(*value),
        Work::BoxArrayLength(value) => environment.box_array_length(*value),
        Work::Sum(parts) => Work::Sum(
            parts
                .iter()
                .map(|part| normalize(part, environment, bound))
                .collect(),
        ),
        Work::Product(left, right) => Work::Product(
            Box::new(normalize(left, environment, bound)),
            Box::new(normalize(right, environment, bound)),
        ),
        Work::Difference(left, right) => {
            let upper = normalize(left, environment, bound);
            let lower = normalize(right, environment, !bound);
            span(environment, upper, lower, bound)
        }
        Work::Quotient(value, divisor) => {
            Work::Quotient(Box::new(normalize(value, environment, bound)), *divisor)
        }
    }
}

/// A callee's entry estimate in the caller's values. `actuals[i]` is the
/// value bound to the callee's `i`th formal, or `None` where that formal
/// varies within one entry of the caller (a chunk's seed and endpoints).
/// `bound` says whether the result may exceed the exact value, never fall
/// below it; a difference's subtrahend is transported the other way.
fn transport(
    work: &Work,
    callee: &IrFunction,
    actuals: &[Option<IrValueId>],
    environment: &mut Environment<'_>,
    bound: bool,
) -> Option<Work> {
    let actual = |formal: &IrValueId| {
        let index = callee
            .parameters()
            .iter()
            .position(|(parameter, _)| parameter == formal)?;
        *actuals.get(index)?
    };
    Some(match work {
        Work::Constant(value) => Work::Constant(*value),
        Work::Value(formal) => environment.scalar(actual(formal)?),
        Work::Length(formal) => environment.length(actual(formal)?),
        Work::BoxArrayLength(formal) => environment.box_array_length(actual(formal)?),
        Work::Sum(parts) => Work::Sum(
            parts
                .iter()
                .map(|part| transport(part, callee, actuals, environment, bound))
                .collect::<Option<_>>()?,
        ),
        Work::Product(left, right) => Work::Product(
            Box::new(transport(left, callee, actuals, environment, bound)?),
            Box::new(transport(right, callee, actuals, environment, bound)?),
        ),
        Work::Difference(left, right) => {
            let upper = transport(left, callee, actuals, environment, bound)?;
            let lower = transport(right, callee, actuals, environment, !bound)?;
            span(environment, upper, lower, bound)
        }
        Work::Quotient(value, divisor) => Work::Quotient(
            Box::new(transport(value, callee, actuals, environment, bound)?),
            *divisor,
        ),
    })
}

/// The value of an estimate whose leaves are all constants, saturating as
/// the emitted estimate does.
fn constant(work: &Work) -> Option<u64> {
    Some(match work {
        Work::Constant(value) => *value,
        Work::Value(_) | Work::Length(_) | Work::BoxArrayLength(_) => return None,
        Work::Sum(parts) => parts
            .iter()
            .map(constant)
            .try_fold(0_u64, |total, part| Some(total.saturating_add(part?)))?,
        Work::Product(left, right) => constant(left)?.saturating_mul(constant(right)?),
        Work::Difference(left, right) => constant(left)?.saturating_sub(constant(right)?),
        Work::Quotient(value, divisor) => constant(value)?.checked_div(*divisor)?,
    })
}

/// Whether a constant predicate holds, by the arithmetic the emitter uses.
fn constant_cheap(predicate: &Predicate) -> Option<bool> {
    let span = constant(&predicate.span)?;
    let price = constant(&predicate.price)?.max(1);
    let minimum = (GRAIN_LESS_ONE / price + 1).max(2);
    if span < minimum {
        return Some(true);
    }
    let Some(cells) = &predicate.cells else {
        return Some(false);
    };
    let charge = constant(cells)?.saturating_mul(3) / price;
    Some(span - span / 2 <= charge)
}

struct Summaries<'ir> {
    functions: &'ir [IrFunction],
    memo: Vec<Option<Option<Vec<Predicate>>>>,
    active: HashSet<usize>,
}

impl Summaries<'_> {
    /// Every split decision `ordinal` reaches, over its own entry values, or
    /// `None` when one is unavailable there.
    fn get(&mut self, ordinal: usize) -> Option<Vec<Predicate>> {
        if let Some(summary) = &self.memo[ordinal] {
            return summary.clone();
        }
        // Recursion keeps local decisions and its existing budget. The depth
        // bounds only this optimization's precision, never acceptance.
        if self.active.len() >= 256 || !self.active.insert(ordinal) {
            return None;
        }
        let result = self.build(ordinal);
        self.active.remove(&ordinal);
        self.memo[ordinal] = Some(result.clone());
        result
    }

    fn build(&mut self, ordinal: usize) -> Option<Vec<Predicate>> {
        let functions = self.functions;
        let function = &functions[ordinal];
        // A remaining call group hands out calls whose static work already
        // reached the grain; a split driver's own group is its range split.
        if function.waits() || !function.overlaps().is_empty() {
            return None;
        }
        let mut environment = Environment::new(function);
        let mut result = Vec::new();
        for instruction in function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            let (callee, actuals) = match operation {
                IrOperation::Call {
                    function,
                    arguments,
                } => (
                    *function as usize,
                    arguments.iter().copied().map(Some).collect::<Vec<_>>(),
                ),
                IrOperation::LoopSplit {
                    chunk,
                    lower,
                    upper,
                    captures,
                    weight,
                    work,
                    indexed,
                    ..
                } => {
                    let upper = environment.scalar(*upper);
                    let lower = environment.scalar(*lower);
                    let span = span(&mut environment, upper, lower, true);
                    let price = work.as_ref().map_or(Work::Constant(*weight), |work| {
                        normalize(work, &mut environment, true)
                    });
                    // The driver adds every family's count, shared or not.
                    let cells = if indexed.is_empty() {
                        None
                    } else {
                        indexed
                            .iter()
                            .map(|family| {
                                captures
                                    .get(family.count)
                                    .map(|count| environment.scalar(*count))
                            })
                            .collect::<Option<Vec<_>>>()
                            .map(Work::Sum)
                            .filter(|cells| available(cells, &environment))
                    };
                    result.push(Predicate { span, price, cells });
                    // Seed and endpoints differ for every slice of one entry;
                    // captures are fixed for the whole site.
                    (
                        *chunk as usize,
                        [None, None, None]
                            .into_iter()
                            .chain(captures.iter().copied().map(Some))
                            .collect(),
                    )
                }
                IrOperation::ContextStart { .. } | IrOperation::ContextStartBound { .. } => {
                    return None;
                }
                _ => continue,
            };
            let callee_function = functions.get(callee)?;
            for predicate in self.get(callee)? {
                let span = transport(
                    &predicate.span,
                    callee_function,
                    &actuals,
                    &mut environment,
                    true,
                )?;
                let price = transport(
                    &predicate.price,
                    callee_function,
                    &actuals,
                    &mut environment,
                    true,
                )?;
                // A count the caller cannot state only drops the refinement.
                let cells = predicate.cells.as_ref().and_then(|cells| {
                    transport(cells, callee_function, &actuals, &mut environment, false)
                });
                result.push(Predicate { span, price, cells });
            }
        }
        for predicate in &mut result {
            if !available(&predicate.span, &environment)
                || !available(&predicate.price, &environment)
            {
                return None;
            }
            if predicate
                .cells
                .as_ref()
                .is_some_and(|cells| !available(cells, &environment))
            {
                predicate.cells = None;
            }
        }
        result.sort();
        result.dedup();
        Some(result)
    }
}

/// Whether some edge enters block 0, which an entry choice must precede.
fn entry_is_a_target(function: &IrFunction) -> bool {
    function
        .blocks()
        .iter()
        .any(|block| match block.terminator() {
            IrTerminator::Jump { target, .. } => target.index() == 0,
            IrTerminator::Match { targets, .. } => {
                targets.iter().any(|target| target.block().index() == 0)
            }
            IrTerminator::IntegerMatch {
                targets, default, ..
            } => targets
                .iter()
                .map(|target| target.block())
                .chain(*default)
                .any(|target| target.index() == 0),
            IrTerminator::Return { .. } | IrTerminator::Unreachable => false,
        })
}

/// The entry choice of every source function that reaches a range split
/// whose decisions are all entry-invariant. A predicate of constants is
/// folded: a cheap one is dropped, and an expensive one leaves the function
/// to its local decisions.
pub(super) fn plan(functions: &[IrFunction]) -> BTreeMap<String, Vec<Predicate>> {
    let mut summaries = Summaries {
        functions,
        memo: vec![None; functions.len()],
        active: HashSet::new(),
    };
    let mut plans = BTreeMap::new();
    for (ordinal, function) in functions.iter().enumerate() {
        if function.synthesis().is_some()
            || function.blocks().is_empty()
            || entry_is_a_target(function)
        {
            continue;
        }
        let Some(predicates) = summaries.get(ordinal) else {
            continue;
        };
        if predicates.is_empty() {
            continue;
        }
        let mut kept = Vec::with_capacity(predicates.len());
        let mut expensive = false;
        for predicate in predicates {
            match constant_cheap(&predicate) {
                Some(true) => {}
                Some(false) => expensive = true,
                None => kept.push(predicate),
            }
        }
        if !expensive {
            plans.insert(function.name().to_owned(), kept);
        }
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::{Predicate, constant_cheap};
    use crate::IrWorkEstimate as Work;

    fn site(span: u64, price: u64, cells: Option<u64>) -> Predicate {
        Predicate {
            span: Work::Constant(span),
            price: Work::Constant(price),
            cells: cells.map(Work::Constant),
        }
    }

    /// The constant fold is the driver's own decision: the minimum span is
    /// floor(149999 / price) + 1, here 4055 for a price of 37, and a far half
    /// no larger than three cell traversals per iteration never pays.
    #[test]
    fn constant_decisions_match_the_driver() {
        assert_eq!(constant_cheap(&site(4054, 37, None)), Some(true));
        assert_eq!(constant_cheap(&site(4055, 37, None)), Some(false));
        assert_eq!(constant_cheap(&site(4096, 37, Some(32768))), Some(true));
        assert_eq!(constant_cheap(&site(4096, 37, Some(1000))), Some(false));
        assert_eq!(constant_cheap(&site(1, u64::MAX, None)), Some(true));
        assert_eq!(constant_cheap(&site(2, u64::MAX, None)), Some(false));
        assert_eq!(constant_cheap(&site(1_000_000, 0, None)), Some(false));
        assert_eq!(
            constant_cheap(&site(u64::MAX, 1, Some(u64::MAX))),
            Some(true)
        );
    }

    /// A decision that reads a runtime value is not folded.
    #[test]
    fn a_runtime_leaf_is_not_folded() {
        let runtime = Predicate {
            span: Work::Length(crate::IrValueId(0)),
            price: Work::Constant(1),
            cells: None,
        };
        assert_eq!(constant_cheap(&runtime), None);
    }
}
