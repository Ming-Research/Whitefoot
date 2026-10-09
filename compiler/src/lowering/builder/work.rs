//! Runtime counted extents for the once-per-loop scheduling estimate.
//!
//! This pass runs after checking. Unavailable expressions keep the static
//! price; no estimate is an acceptance fact or a reason to omit source work.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{
    IrAddressed, IrConstant, IrFunction, IrInstruction, IrIntegerOperation, IrOperation,
    IrSynthesis, IrTerminator, IrType, IrValueId, IrWorkEstimate as Work,
};

use super::split::{LOOP_FACTOR, loop_depths};

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

fn sum(parts: impl IntoIterator<Item = Work>) -> Work {
    let mut pending: Vec<_> = parts.into_iter().collect();
    let mut constant = 0_u64;
    let mut terms = BTreeMap::<Work, u64>::new();
    while let Some(part) = pending.pop() {
        match part {
            Work::Constant(value) => constant = constant.saturating_add(value),
            Work::Sum(parts) => pending.extend(parts),
            Work::Product(left, right) if matches!(*left, Work::Constant(_)) => {
                let Work::Constant(factor) = *left else {
                    unreachable!()
                };
                let coefficient = terms.entry(*right).or_default();
                *coefficient = coefficient.saturating_add(factor);
            }
            term => {
                let coefficient = terms.entry(term).or_default();
                *coefficient = coefficient.saturating_add(1);
            }
        }
    }
    let mut result: Vec<_> = terms
        .into_iter()
        .map(|(term, factor)| product(Work::Constant(factor), term))
        .collect();
    if constant != 0 {
        result.push(Work::Constant(constant));
    }
    match result.len() {
        0 => Work::Constant(0),
        1 => result.remove(0),
        _ => Work::Sum(result),
    }
}

fn product(left: Work, right: Work) -> Work {
    match (left, right) {
        (Work::Constant(0), _) | (_, Work::Constant(0)) => Work::Constant(0),
        (Work::Constant(1), value) | (value, Work::Constant(1)) => value,
        (Work::Constant(a), Work::Constant(b)) => Work::Constant(a.saturating_mul(b)),
        (value, Work::Constant(factor)) => product(Work::Constant(factor), value),
        (Work::Constant(a), Work::Product(b, value)) if matches!(*b, Work::Constant(_)) => {
            let Work::Constant(b) = *b else {
                unreachable!()
            };
            product(Work::Constant(a.saturating_mul(b)), *value)
        }
        (left, right) => Work::Product(Box::new(left), Box::new(right)),
    }
}

fn difference(left: Work, right: Work) -> Work {
    if left == right {
        return Work::Constant(0);
    }
    match (left, right) {
        (Work::Constant(a), Work::Constant(b)) => Work::Constant(a.saturating_sub(b)),
        (value, Work::Constant(0)) => value,
        (Work::Sum(mut terms), right) => {
            if let Some(index) = terms.iter().position(|term| *term == right) {
                terms.remove(index);
                sum(terms)
            } else {
                Work::Difference(Box::new(Work::Sum(terms)), Box::new(right))
            }
        }
        (Work::Difference(value, subtracted), Work::Constant(next))
            if matches!(*subtracted, Work::Constant(_)) =>
        {
            let Work::Constant(first) = *subtracted else {
                unreachable!()
            };
            difference(*value, Work::Constant(first.saturating_add(next)))
        }
        (left, right) => Work::Difference(Box::new(left), Box::new(right)),
    }
}

fn quotient(value: Work, divisor: u64) -> Work {
    match value {
        Work::Constant(value) => Work::Constant(value / divisor),
        value if divisor == 1 => value,
        value => Work::Quotient(Box::new(value), divisor),
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Observation {
    Scalar,
    Length,
    BoxArrayLength,
}

impl Observation {
    fn leaf(self, value: IrValueId) -> Work {
        match self {
            Self::Scalar => Work::Value(value),
            Self::Length => Work::Length(value),
            Self::BoxArrayLength => Work::BoxArrayLength(value),
        }
    }
}

struct Environment<'ir> {
    function: &'ir IrFunction,
    definitions: HashMap<IrValueId, &'ir IrOperation>,
    incoming: HashMap<IrValueId, Vec<IrValueId>>,
    memo: HashMap<(Observation, IrValueId), Work>,
}

impl<'ir> Environment<'ir> {
    fn new(function: &'ir IrFunction) -> Self {
        let mut definitions = HashMap::new();
        let mut incoming: HashMap<IrValueId, Vec<IrValueId>> = HashMap::new();
        for block in function.blocks() {
            for instruction in block.instructions() {
                if let IrInstruction::Define {
                    result, operation, ..
                } = instruction
                {
                    definitions.insert(*result, operation);
                }
            }
            if let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
                && let Some(target) = function.blocks().get(target.index())
            {
                for ((parameter, _), argument) in target.parameters().iter().zip(arguments) {
                    incoming.entry(*parameter).or_default().push(*argument);
                }
            }
        }
        Self {
            function,
            definitions,
            incoming,
            memo: HashMap::new(),
        }
    }

    fn scalar(&mut self, value: IrValueId) -> Work {
        self.observe(Observation::Scalar, value, &mut Vec::new())
    }

    fn observe(
        &mut self,
        kind: Observation,
        value: IrValueId,
        active: &mut Vec<(Observation, IrValueId)>,
    ) -> Work {
        let key = (kind, value);
        if let Some(known) = self.memo.get(&key) {
            return known.clone();
        }
        // Only optimization precision is bounded. A long or cyclic value walk
        // leaves an opaque SSA leaf and therefore keeps the static estimate.
        if active.len() == 256 || active.contains(&key) {
            return kind.leaf(value);
        }
        active.push(key);
        let result = self.observe_inner(kind, value, active);
        active.pop();
        if !has_active_leaf(&result, active) {
            self.memo.insert(key, result.clone());
        }
        result
    }

    fn observe_inner(
        &mut self,
        kind: Observation,
        value: IrValueId,
        active: &mut Vec<(Observation, IrValueId)>,
    ) -> Work {
        let operation = self.definitions.get(&value).copied();
        match (kind, operation) {
            (
                Observation::Scalar,
                Some(IrOperation::Constant(IrConstant::Integer { ty, bits })),
            ) if *ty == U64 => Work::Constant(*bits),
            (
                Observation::Scalar,
                Some(IrOperation::Integer {
                    operation,
                    operand_type,
                    arguments,
                }),
            ) if *operand_type == U64 && arguments.len() == 2 => {
                if !matches!(
                    operation,
                    IrIntegerOperation::AddExact
                        | IrIntegerOperation::SubtractExact
                        | IrIntegerOperation::MultiplyExact
                ) {
                    return kind.leaf(value);
                }
                let left = self.observe(kind, arguments[0], active);
                let right = self.observe(kind, arguments[1], active);
                match operation {
                    IrIntegerOperation::AddExact => sum([left, right]),
                    IrIntegerOperation::SubtractExact => difference(left, right),
                    IrIntegerOperation::MultiplyExact => product(left, right),
                    _ => unreachable!(),
                }
            }
            (Observation::Scalar, Some(IrOperation::SliceMeasure { slice })) => {
                self.observe(Observation::Length, *slice, active)
            }
            (Observation::Scalar, Some(IrOperation::BufferMeasure { buffer })) => {
                self.observe(Observation::Length, *buffer, active)
            }
            (
                Observation::Scalar,
                Some(IrOperation::ContainerMeasure {
                    measure: crate::IrMeasure::Length,
                    container,
                }),
            ) if matches!(
                self.function.value_type(*container),
                Some(IrType::Address(IrAddressed::Buffer { .. }))
            ) =>
            {
                self.observe(Observation::Length, *container, active)
            }
            (Observation::Length, Some(IrOperation::SliceRange { start, end, .. })) => {
                let end = self.observe(Observation::Scalar, *end, active);
                let start = self.observe(Observation::Scalar, *start, active);
                difference(end, start)
            }
            (Observation::Length, Some(IrOperation::SliceFromBuffer { buffer })) => {
                self.observe(kind, *buffer, active)
            }
            (Observation::Length, Some(IrOperation::BufferFill { length, .. })) => {
                self.observe(Observation::Scalar, *length, active)
            }
            (Observation::Length, Some(IrOperation::AddressOf { value, .. })) => {
                self.observe(kind, *value, active)
            }
            (Observation::Length, Some(IrOperation::Load { address, .. })) => {
                self.observe(kind, *address, active)
            }
            (
                Observation::Length,
                Some(IrOperation::ProjectAddress {
                    address,
                    projection: crate::IrPlaceStep::BoxReferent { .. },
                }),
            ) if matches!(
                self.function.value_type(value),
                Some(IrType::Address(IrAddressed::Buffer { .. }))
            ) =>
            {
                self.observe(Observation::BoxArrayLength, *address, active)
            }
            (_, None) => {
                let Some(incoming) = self.incoming.get(&value).cloned() else {
                    return kind.leaf(value);
                };
                // Follow the complete forwarding component before observing
                // definitions. A loop-carried copy can cycle through several
                // block parameters, but an update terminates at its defining
                // instruction and must agree with all other incoming values.
                let mut pending = incoming;
                let mut seen = BTreeSet::from([value]);
                let mut terminals = BTreeSet::new();
                while let Some(argument) = pending.pop() {
                    if !seen.insert(argument) {
                        continue;
                    }
                    if let Some(forwards) = self.incoming.get(&argument) {
                        pending.extend(forwards.iter().copied());
                    } else {
                        terminals.insert(argument);
                    }
                }
                let mut common = None;
                for argument in terminals {
                    let observed = self.observe(kind, argument, active);
                    if common.as_ref().is_some_and(|first| *first != observed) {
                        return kind.leaf(value);
                    }
                    common = Some(observed);
                }
                common.unwrap_or_else(|| kind.leaf(value))
            }
            _ => kind.leaf(value),
        }
    }

    fn available(&self, work: &Work) -> bool {
        match work {
            Work::Constant(_) => true,
            Work::Value(value) => self.function.parameters().contains(&(*value, U64)),
            Work::Length(value) => self.function.parameters().iter().any(|(parameter, ty)| {
                parameter == value
                    && matches!(
                        ty,
                        IrType::Buffer { .. } | IrType::Range { .. } | IrType::Run { .. }
                    )
            }),
            Work::BoxArrayLength(value) => {
                self.function.readonly_reference_parameters.contains(value)
            }
            Work::Sum(parts) => parts.iter().all(|part| self.available(part)),
            Work::Product(left, right) | Work::Difference(left, right) => {
                self.available(left) && self.available(right)
            }
            Work::Quotient(value, _) => self.available(value),
        }
    }

    fn instantiate(&mut self, work: &Work, callee: &IrFunction, arguments: &[IrValueId]) -> Work {
        let actual = |value| {
            callee
                .parameters()
                .iter()
                .position(|(parameter, _)| *parameter == value)
                .and_then(|index| arguments.get(index))
                .copied()
        };
        match work {
            Work::Constant(value) => Work::Constant(*value),
            Work::Value(value) => {
                actual(*value).map_or(Work::Constant(0), |value| self.scalar(value))
            }
            Work::Length(value) => actual(*value).map_or(Work::Constant(0), |value| {
                self.observe(Observation::Length, value, &mut Vec::new())
            }),
            Work::BoxArrayLength(value) => actual(*value).map_or(Work::Constant(0), |value| {
                self.observe(Observation::BoxArrayLength, value, &mut Vec::new())
            }),
            Work::Sum(parts) => sum(parts
                .iter()
                .map(|part| self.instantiate(part, callee, arguments))),
            Work::Product(left, right) => {
                let left = self.instantiate(left, callee, arguments);
                let right = self.instantiate(right, callee, arguments);
                product(left, right)
            }
            Work::Difference(left, right) => {
                let left = self.instantiate(left, callee, arguments);
                let right = self.instantiate(right, callee, arguments);
                difference(left, right)
            }
            Work::Quotient(value, divisor) => {
                quotient(self.instantiate(value, callee, arguments), *divisor)
            }
        }
    }
}

fn is_active_leaf(work: &Work, active: &[(Observation, IrValueId)]) -> bool {
    match work {
        Work::Value(value) => active.contains(&(Observation::Scalar, *value)),
        Work::Length(value) => active.contains(&(Observation::Length, *value)),
        Work::BoxArrayLength(value) => active.contains(&(Observation::BoxArrayLength, *value)),
        _ => false,
    }
}

fn has_active_leaf(work: &Work, active: &[(Observation, IrValueId)]) -> bool {
    match work {
        Work::Sum(parts) => parts.iter().any(|part| has_active_leaf(part, active)),
        Work::Product(left, right) | Work::Difference(left, right) => {
            has_active_leaf(left, active) || has_active_leaf(right, active)
        }
        Work::Quotient(value, _) => has_active_leaf(value, active),
        _ => is_active_leaf(work, active),
    }
}

struct Call {
    callee: u32,
    arguments: Vec<IrValueId>,
    span: Option<Work>,
    factor: Work,
    iteration_factor: Work,
}

struct Cost {
    own: Work,
    own_iteration: Work,
    calls: Vec<Call>,
}

#[derive(Clone)]
struct Summary {
    whole: Work,
    iteration: Work,
}

fn cost(environment: &mut Environment<'_>) -> Cost {
    let function = environment.function;
    let mut depths = loop_depths(function.blocks());
    // The static back-edge interval includes a counted range's continuation.
    // That block has left this loop: remove its level before capping depth
    // and replacing the remaining counted levels with their extents.
    for range in &function.counted_ranges {
        let depth = &mut depths[range.continuation.index()];
        *depth = depth.saturating_sub(1);
    }
    let outer = if function.synthesis() == Some(IrSynthesis::Chunk) {
        function
            .counted_ranges
            .iter()
            .find(|range| {
                function.parameters().get(1).map(|parameter| parameter.0) == Some(range.lower)
                    && function.parameters().get(2).map(|parameter| parameter.0)
                        == Some(range.upper)
            })
            .map(|range| range.blocks.start)
    } else {
        None
    };
    let extents: Vec<_> = function
        .counted_ranges
        .iter()
        .map(|range| {
            let upper = environment.scalar(range.upper);
            let lower = environment.scalar(range.lower);
            let span = difference(upper, lower);
            let span = if environment.available(&span) {
                span
            } else {
                Work::Constant(LOOP_FACTOR)
            };
            (range, span)
        })
        .collect();
    let mut own = Vec::new();
    let mut own_iteration = Vec::new();
    let mut calls = Vec::new();
    for (index, block) in function.blocks().iter().enumerate() {
        let known: Vec<_> = extents
            .iter()
            .filter(|(range, _)| {
                range.blocks.contains(&index) && range.continuation.index() != index
            })
            .take(4)
            .collect();
        let remaining = usize::from(depths[index])
            .min(4)
            .saturating_sub(known.len());
        let base = Work::Constant(LOOP_FACTOR.saturating_pow(remaining as u32));
        let mut factor = base.clone();
        let mut iteration_factor = base;
        for (range, span) in known {
            factor = product(factor, span.clone());
            iteration_factor = product(
                iteration_factor,
                if Some(range.blocks.start) == outer {
                    Work::Constant(LOOP_FACTOR)
                } else {
                    span.clone()
                },
            );
        }
        let count = Work::Constant(block.instructions().len() as u64);
        own.push(product(count.clone(), factor.clone()));
        own_iteration.push(product(count, iteration_factor.clone()));
        for instruction in block.instructions() {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            let (callee, arguments, span) = match operation {
                IrOperation::Call {
                    function,
                    arguments,
                } => (*function, arguments.clone(), None),
                IrOperation::LoopSplit {
                    chunk,
                    seed,
                    lower,
                    upper,
                    captures,
                    ..
                } => {
                    let arguments = [*seed, *lower, *upper]
                        .into_iter()
                        .chain(captures.iter().copied())
                        .collect();
                    let upper = environment.scalar(*upper);
                    let lower = environment.scalar(*lower);
                    let span = difference(upper, lower);
                    let span = if environment.available(&span) {
                        span
                    } else {
                        Work::Constant(LOOP_FACTOR)
                    };
                    (*chunk, arguments, Some(span))
                }
                _ => continue,
            };
            calls.push(Call {
                callee,
                arguments,
                span,
                factor: factor.clone(),
                iteration_factor: iteration_factor.clone(),
            });
        }
    }
    Cost {
        own: sum(own),
        own_iteration: sum(own_iteration),
        calls,
    }
}

pub(super) fn assign(functions: &mut [IrFunction], static_costs: &[u64]) {
    if !functions
        .iter()
        .any(|function| function.synthesis() == Some(IrSynthesis::Chunk))
    {
        return;
    }
    let plans = {
        let mut environments: Vec<_> = functions.iter().map(Environment::new).collect();
        let costs: Vec<_> = environments.iter_mut().map(cost).collect();
        let mut summaries: Vec<_> = costs
            .iter()
            .map(|cost| Summary {
                whole: cost.own.clone(),
                iteration: quotient(cost.own_iteration.clone(), LOOP_FACTOR),
            })
            .collect();
        // The same three call-summary substitutions as the static estimate.
        for _ in 0..3 {
            let previous = summaries.clone();
            for (ordinal, summary) in summaries.iter_mut().enumerate() {
                let mut whole = vec![costs[ordinal].own.clone()];
                let mut iteration = vec![costs[ordinal].own_iteration.clone()];
                for call in &costs[ordinal].calls {
                    let callee = &functions[call.callee as usize];
                    let called = &previous[call.callee as usize];
                    let estimate = if call.span.is_some() {
                        &called.iteration
                    } else {
                        &called.whole
                    };
                    let estimate =
                        environments[ordinal].instantiate(estimate, callee, &call.arguments);
                    let estimate = if environments[ordinal].available(&estimate) {
                        estimate
                    } else {
                        let raw = static_costs[call.callee as usize];
                        Work::Constant(if call.span.is_some() {
                            (raw / LOOP_FACTOR).max(1)
                        } else {
                            raw
                        })
                    };
                    let estimate = call
                        .span
                        .as_ref()
                        .map_or(estimate.clone(), |span| product(estimate, span.clone()));
                    whole.push(product(estimate.clone(), call.factor.clone()));
                    iteration.push(product(estimate, call.iteration_factor.clone()));
                }
                *summary = Summary {
                    whole: sum(whole),
                    iteration: quotient(sum(iteration), LOOP_FACTOR),
                };
            }
        }
        let mut plans = Vec::new();
        for (ordinal, environment) in environments.iter_mut().enumerate() {
            let function = environment.function;
            for (block_index, block) in function.blocks().iter().enumerate() {
                for (instruction_index, instruction) in block.instructions().iter().enumerate() {
                    let IrInstruction::Define {
                        operation:
                            IrOperation::LoopSplit {
                                chunk,
                                seed,
                                lower,
                                upper,
                                captures,
                                ..
                            },
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    let arguments: Vec<_> = [*seed, *lower, *upper]
                        .into_iter()
                        .chain(captures.iter().copied())
                        .collect();
                    // A site's operands are already captured SSA values. Do
                    // not substitute their defining expressions here: even a
                    // local length observation is available at this site.
                    let summary = &summaries[*chunk as usize].iteration;
                    let callee = &functions[*chunk as usize];
                    let work = bind_site(summary, callee, &arguments);
                    plans.push((ordinal, block_index, instruction_index, work));
                }
            }
        }
        plans
    };
    for (function, block, instruction, work) in plans {
        if let IrInstruction::Define {
            operation: IrOperation::LoopSplit {
                work: destination, ..
            },
            ..
        } = &mut functions[function].blocks[block].instructions[instruction]
        {
            *destination = Some(work);
        }
    }
}

fn bind_site(work: &Work, function: &IrFunction, arguments: &[IrValueId]) -> Work {
    let actual = |value| {
        let index = function
            .parameters()
            .iter()
            .position(|(parameter, _)| *parameter == value)
            .expect("a work summary contains only formal values");
        arguments[index]
    };
    match work {
        Work::Constant(value) => Work::Constant(*value),
        Work::Value(value) => Work::Value(actual(*value)),
        Work::Length(value) => Work::Length(actual(*value)),
        Work::BoxArrayLength(value) => Work::BoxArrayLength(actual(*value)),
        Work::Sum(parts) => sum(parts
            .iter()
            .map(|part| bind_site(part, function, arguments))),
        Work::Product(left, right) => product(
            bind_site(left, function, arguments),
            bind_site(right, function, arguments),
        ),
        Work::Difference(left, right) => difference(
            bind_site(left, function, arguments),
            bind_site(right, function, arguments),
        ),
        Work::Quotient(value, divisor) => quotient(bind_site(value, function, arguments), *divisor),
    }
}
