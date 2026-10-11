//! Function-body cheap regions for the opt-in demand lowering.
//!
//! Transport every reachable split predicate through ordinary calls. A body
//! with repeated scheduling and entry-invariant predicates can select its
//! existing sequential clone once. Unknown predicates, call groups, recursion
//! and memory observations retain ordinary demand lowering. No source guard,
//! operation, capture or cleanup is moved or rewritten.
use std::collections::{BTreeMap, HashSet};

use super::{loops::U64, work::Environment};
use crate::{
    DemandAblation, IrEnumType, IrFunction, IrInstruction, IrIntegerOperation as Op, IrOperation,
    IrSynthesis, IrTerminator, IrValueId, IrWorkEstimate as Work,
};

type Predicates = Vec<(Work, Work)>;

fn available(work: &Work, function: &IrFunction) -> bool {
    match work {
        Work::Constant(_) => true,
        Work::Value(value) => function.parameters().contains(&(*value, U64)),
        Work::Sum(parts) => parts.iter().all(|part| available(part, function)),
        Work::Product(a, b) | Work::Difference(a, b) => {
            available(a, function) && available(b, function)
        }
        Work::Quotient(value, divisor) => *divisor != 0 && available(value, function),
        Work::Length(_) | Work::BoxArrayLength(_) => false,
    }
}

/// Prove the existing unsigned ordering on every forward path to this site.
/// Backedges are boundaries: a guard from a previous iteration cannot justify
/// cancellation after loop-carried values have changed. No new check is emitted.
fn ordered(
    env: &mut Environment<'_>,
    function: &IrFunction,
    block: usize,
    lower: &Work,
    upper: &Work,
    seen: &mut HashSet<usize>,
) -> bool {
    if block == 0 || !seen.insert(block) {
        return false;
    }
    let mut found = false;
    for (index, predecessor) in function.blocks().iter().enumerate() {
        let mut edges = Vec::new();
        match predecessor.terminator() {
            IrTerminator::Jump { target, .. } if target.index() == block => {
                edges.push(None);
            }
            IrTerminator::Match {
                scrutinee,
                enum_type,
                targets,
            } => {
                for target in targets
                    .iter()
                    .filter(|target| target.block().index() == block)
                {
                    edges.push(
                        (*enum_type == IrEnumType::Bool).then_some((*scrutinee, target.tag())),
                    );
                }
            }
            _ => {}
        }
        for condition in edges {
            found = true;
            if index >= block {
                seen.remove(&block);
                return false;
            }
            let guarded = condition.is_some_and(|(value, tag)| {
                let Some(IrOperation::Integer {
                    operation,
                    operand_type,
                    arguments,
                }) = env.definition(value)
                else {
                    return false;
                };
                if *operand_type != U64 || arguments.len() != 2 {
                    return false;
                }
                let left = env.scalar(arguments[0]);
                let right = env.scalar(arguments[1]);
                match (*operation, tag) {
                    (Op::LessEqual | Op::Less, 1) | (Op::Greater, 0) => {
                        left == *lower && right == *upper
                    }
                    (Op::GreaterEqual | Op::Greater, 1) | (Op::Less, 0) => {
                        left == *upper && right == *lower
                    }
                    _ => false,
                }
            });
            if !guarded && !ordered(env, function, index, lower, upper, seen) {
                seen.remove(&block);
                return false;
            }
        }
    }
    seen.remove(&block);
    found
}

fn span(
    env: &mut Environment<'_>,
    function: &IrFunction,
    block: usize,
    upper: Work,
    lower: Work,
) -> Work {
    if upper == lower {
        return Work::Constant(0);
    }
    if let (Work::Constant(hi), Work::Constant(lo)) = (&upper, &lower) {
        return Work::Constant(hi.saturating_sub(*lo));
    }
    // (lo +wrap extent) - lo is exactly extent on an existing lo <= hi
    // edge. Without that edge it can be zero after wrap, so do not version.
    if let Work::Value(value) = &upper
        && let Some(IrOperation::Integer {
            operation: Op::AddWrap,
            operand_type,
            arguments,
        }) = env.definition(*value)
        && *operand_type == U64
        && arguments.len() == 2
    {
        let left = env.scalar(arguments[0]);
        let right = env.scalar(arguments[1]);
        let extent = if left == lower {
            Some(right)
        } else if right == lower {
            Some(left)
        } else {
            None
        };
        if let Some(extent) = extent
            && ordered(env, function, block, &lower, &upper, &mut HashSet::new())
        {
            return extent;
        }
    }
    Work::Difference(Box::new(upper), Box::new(lower))
}

fn transport(
    work: &Work,
    callee: &IrFunction,
    arguments: &[IrValueId],
    env: &mut Environment<'_>,
    caller: &IrFunction,
    block: usize,
) -> Option<Work> {
    let mut child = |work: &Work| transport(work, callee, arguments, env, caller, block);
    Some(match work {
        Work::Constant(value) => Work::Constant(*value),
        Work::Value(value) => {
            let index = callee
                .parameters()
                .iter()
                .position(|(formal, _)| formal == value)?;
            env.scalar(*arguments.get(index)?)
        }
        Work::Sum(parts) => Work::Sum(parts.iter().map(&mut child).collect::<Option<_>>()?),
        Work::Product(a, b) => Work::Product(Box::new(child(a)?), Box::new(child(b)?)),
        Work::Difference(a, b) => {
            let upper = child(a)?;
            let lower = child(b)?;
            span(env, caller, block, upper, lower)
        }
        Work::Quotient(value, divisor) => Work::Quotient(Box::new(child(value)?), *divisor),
        Work::Length(_) | Work::BoxArrayLength(_) => return None,
    })
}

struct Summaries<'ir> {
    functions: &'ir [IrFunction],
    ablation: DemandAblation,
    memo: Vec<Option<Option<Predicates>>>,
    active: HashSet<usize>,
}

impl Summaries<'_> {
    fn get(&mut self, ordinal: usize) -> Option<Predicates> {
        if let Some(summary) = &self.memo[ordinal] {
            return summary.clone();
        }
        // Precision bounds only this optimization, never source acceptance.
        if self.active.len() >= 256 || !self.active.insert(ordinal) {
            return None;
        }
        let result = self.build(ordinal);
        self.active.remove(&ordinal);
        self.memo[ordinal] = Some(result.clone());
        result
    }

    fn build(&mut self, ordinal: usize) -> Option<Predicates> {
        let function = &self.functions[ordinal];
        if function.waits()
            || !function.overlaps().is_empty()
            || function.synthesis() == Some(IrSynthesis::Splitter)
        {
            return None;
        }
        let mut env = Environment::new(function);
        let mut result = Vec::new();
        for (block_index, block) in function.blocks().iter().enumerate() {
            for instruction in block.instructions() {
                let IrInstruction::Define { operation, .. } = instruction else {
                    continue;
                };
                let (callee, arguments) = match operation {
                    IrOperation::Call {
                        function,
                        arguments,
                    } => (*function as usize, arguments.clone()),
                    IrOperation::LoopSplit {
                        chunk,
                        lower,
                        upper,
                        seed,
                        captures,
                        weight,
                        work,
                        indexed,
                        ..
                    } => {
                        if !indexed.is_empty() {
                            return None;
                        }
                        if *weight != 0 {
                            let upper = env.scalar(*upper);
                            let lower = env.scalar(*lower);
                            let extent = span(&mut env, function, block_index, upper, lower);
                            let price = if self.ablation == DemandAblation::Static {
                                Work::Constant(*weight)
                            } else {
                                work.clone().unwrap_or(Work::Constant(*weight))
                            };
                            // Normalize the site's SSA captures through the same
                            // scalar observer used by runtime work estimation.
                            let price = normalize(&price, &mut env)?;
                            result.push((extent, price));
                        }
                        (
                            *chunk as usize,
                            [*seed, *lower, *upper]
                                .into_iter()
                                .chain(captures.iter().copied())
                                .collect(),
                        )
                    }
                    // A started context selects its wrapper's world too. Its
                    // waiting body is outside this scalar region analysis.
                    IrOperation::ContextStart { .. } | IrOperation::ContextStartBound { .. } => {
                        return None;
                    }
                    _ => continue,
                };
                let called = self.get(callee)?;
                for (extent, price) in called {
                    result.push((
                        transport(
                            &extent,
                            &self.functions[callee],
                            &arguments,
                            &mut env,
                            function,
                            block_index,
                        )?,
                        transport(
                            &price,
                            &self.functions[callee],
                            &arguments,
                            &mut env,
                            function,
                            block_index,
                        )?,
                    ));
                }
            }
        }
        result.sort();
        result.dedup();
        result
            .iter()
            .all(|(extent, price)| available(extent, function) && available(price, function))
            .then_some(result)
    }
}

fn normalize(work: &Work, env: &mut Environment<'_>) -> Option<Work> {
    Some(match work {
        Work::Constant(value) => Work::Constant(*value),
        Work::Value(value) => env.scalar(*value),
        Work::Sum(parts) => Work::Sum(
            parts
                .iter()
                .map(|part| normalize(part, env))
                .collect::<Option<_>>()?,
        ),
        Work::Product(a, b) => {
            Work::Product(Box::new(normalize(a, env)?), Box::new(normalize(b, env)?))
        }
        Work::Difference(a, b) => {
            Work::Difference(Box::new(normalize(a, env)?), Box::new(normalize(b, env)?))
        }
        Work::Quotient(value, divisor) => {
            Work::Quotient(Box::new(normalize(value, env)?), *divisor)
        }
        Work::Length(_) | Work::BoxArrayLength(_) => return None,
    })
}

/// Repeated scheduling requires a call or split in a control-flow cycle.
/// The work estimator's block-index intervals are only a cost approximation:
/// nested branch bodies can be allocated after the block carrying the loop's
/// backedge, while an exit block can lie inside that interval.
fn repeated_scheduling(function: &IrFunction) -> bool {
    let edges: Vec<Vec<usize>> = function
        .blocks()
        .iter()
        .map(|block| match block.terminator() {
            IrTerminator::Jump { target, .. } => vec![target.index()],
            IrTerminator::Match { targets, .. } => {
                targets.iter().map(|target| target.block().index()).collect()
            }
            IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
        })
        .collect();
    crate::cycles::components(&edges).into_iter().any(|component| {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        cyclic
            && component.iter().any(|index| {
                function.blocks()[*index]
                    .instructions()
                    .iter()
                    .any(|instruction| {
                        matches!(
                            instruction,
                            IrInstruction::Define {
                                operation: IrOperation::Call { .. } | IrOperation::LoopSplit { .. },
                                ..
                            }
                        )
                    })
            })
    })
}

pub(super) fn plan(
    functions: &[IrFunction],
    ablation: DemandAblation,
) -> BTreeMap<String, Predicates> {
    if ablation == DemandAblation::Unversioned {
        return BTreeMap::new();
    }
    let mut summaries = Summaries {
        functions,
        ablation,
        memo: vec![None; functions.len()],
        active: HashSet::new(),
    };
    let mut plans = BTreeMap::new();
    for (ordinal, function) in functions.iter().enumerate() {
        if function.synthesis().is_some() {
            continue;
        }
        if repeated_scheduling(function)
            && let Some(predicates) = summaries.get(ordinal)
            && !predicates.is_empty()
        {
            plans.insert(function.name().to_owned(), predicates);
        }
    }
    plans
}
