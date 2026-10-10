//! Opt-in demand experiments: work-priced slices with demand-only hand-out.
//! Removed or replaced when research/investigations/par-demand concludes.
use super::loops::U64;
use super::{IrBuilder, split};
use crate::semantic::LoopActualization;
use crate::{
    IrConstant, IrFunction, IrInstruction, IrIntegerOperation as Op, IrOperation, IrOverlap,
    IrSynthesis, IrTerminator, IrType, IrValueId, LoweringFailure,
};

/// Prototype price calibration, deliberately not a runtime clock sample.
const NANOS_PER_WEIGHT_UNIT: u64 = 1;
// One offer unit: the owner widened the interval from 5,000 units after the
// fifth rerun found large_helper's remaining 3.8 percent in per-slice setup
// every 625 iterations (research/investigations/par-demand/DESIGN.md).
const SLICE_NANOS: u64 = 150_000;
const SLICE_WORK: u64 = SLICE_NANOS / NANOS_PER_WEIGHT_UNIT;
const WORK_UNIT: u64 = super::call_grain::CALL_OFFER_WORK_UNIT;

/// The fewest iterations of a site of this static weight worth handing out:
/// the driver tests a remaining range against it, and the call site against
/// the whole range before it enters the driver at all.
pub(crate) const fn demand_minimum_span(weight: u64) -> u64 {
    (WORK_UNIT - 1) / weight + 1
}

impl IrBuilder<'_> {
    pub(super) fn build_demand_driver(
        &self,
        (ordinal, name): (u32, &str),
        chunk: u32,
        actualization: LoopActualization,
        result_type: IrType,
        capture_types: &[IrType],
    ) -> Result<IrFunction, LoweringFailure> {
        let mut b = IrBuilder::new(
            self.context(),
            result_type,
            Default::default(),
            None,
            self.overlap,
            self.function_name,
        )?;
        let seed = b.new_parameter(result_type)?;
        let lower = b.new_parameter(U64)?;
        let upper = b.new_parameter(U64)?;
        let captures = capture_types
            .iter()
            .map(|ty| b.new_parameter(*ty))
            .collect::<Result<Vec<_>, _>>()?;
        // Same transport as the splitter's trailing budget word. Every caller
        // supplies a positive runtime-extent price, or a static fallback/ablation.
        let weight = b.new_parameter(U64)?;
        let ablation = self.synthesis.borrow().demand_ablation;
        let frontier = if ablation == crate::DemandAblation::Seed {
            Some(b.new_parameter(U64)?)
        } else {
            None
        };
        let one = b.demand_constant(1)?;
        let slice_work = b.demand_constant(SLICE_WORK)?;
        let quotient = b.demand_binary(Op::DivideExact, slice_work, weight)?;
        let step = b.demand_binary(Op::Maximum, one, quotient)?;
        let work_less_one = b.demand_constant(WORK_UNIT - 1)?;
        let quotient = b.demand_binary(Op::DivideExact, work_less_one, weight)?;
        let minimum_span = b.demand_binary(Op::AddWrap, quotient, one)?;
        let (header, carried) = b.new_block(&[result_type, U64])?;
        let (done, _) = b.new_block(&[])?;
        let (check, _) = b.new_block(&[])?;
        let (ask, _) = b.new_block(&[])?;
        let (slice, _) = b.new_block(&[])?;
        let (halve, _) = b.new_block(&[])?;
        let mut overlaps = Vec::new();
        if let Some(frontier) = frontier {
            let (publish, _) = b.new_block(&[])?;
            let (refine, _) = b.new_block(&[])?;
            let seeded = b.demand_binary(Op::Greater, frontier, one)?;
            b.branch(seeded, publish, refine)?;
            b.current = Some(publish);
            // Entry supplies min(span / minimum_span, lanes * 16). Publish
            // one far share per descent before the owner's first slice. The
            // quotient leaves every share >= minimum_span, and near retains
            // the remainder. Published shares never seed another frontier.
            let span = b.demand_binary(Op::SubtractWrap, upper, lower)?;
            let count = b.demand_binary(Op::DivideExact, span, frontier)?;
            let middle = b.demand_binary(Op::SubtractWrap, upper, count)?;
            let remaining = b.demand_binary(Op::SubtractWrap, frontier, one)?;
            let right_seed = match actualization {
                LoopActualization::IndependentMap => seed,
                LoopActualization::Reduction { combine, .. } => {
                    b.identity_value(combine, result_type)?
                }
            };
            let mut far = vec![right_seed, middle, upper];
            far.extend(captures.iter().copied());
            far.extend([weight, one]);
            let far = b.define(result_type, IrOperation::Call { function: ordinal, arguments: far })?;
            let mut near = vec![seed, lower, middle];
            near.extend(captures.iter().copied());
            near.extend([weight, remaining]);
            let near = b.define(result_type, IrOperation::Call { function: ordinal, arguments: near })?;
            let result = match actualization {
                LoopActualization::IndependentMap => near,
                LoopActualization::Reduction { combine, .. } => {
                    b.combine_values(combine, result_type, near, far)?
                }
            };
            b.terminate(IrTerminator::Return { value: result, drops: vec![] })?;
            overlaps.push(IrOverlap { members: vec![far, near] });
            b.current = Some(refine);
        }
        b.terminate(IrTerminator::Jump {
            target: header,
            arguments: vec![seed, lower],
            drops: vec![],
        })?;
        b.current = Some(header);
        let accumulator = carried[0];
        let cursor = carried[1];
        let nonempty = b.demand_binary(Op::Less, cursor, upper)?;
        b.branch(nonempty, check, done)?;
        b.current = Some(done);
        b.terminate(IrTerminator::Return {
            value: accumulator,
            drops: vec![],
        })?;
        b.current = Some(check);
        let span = b.demand_binary(Op::SubtractWrap, upper, cursor)?;
        let worth = b.demand_binary(Op::GreaterEqual, span, minimum_span)?;
        let divisible = b.demand_binary(Op::Greater, span, one)?;
        let worth = b.define(
            IrType::Bool,
            IrOperation::Boolean {
                operation: crate::IrBooleanOperation::And,
                arguments: vec![worth, divisible],
            },
        )?;
        // Only a range worth handing out reads the request word: a range
        // below the minimum span never touches the word idle workers write,
        // so its slices cost what the sequential loop costs at any width.
        b.branch(worth, ask, slice)?;
        b.current = Some(ask);
        let requested = b.define(IrType::Bool, IrOperation::DemandRequested)?;
        b.branch(requested, halve, slice)?;
        b.current = Some(slice);
        let count = b.demand_binary(Op::Minimum, span, step)?;
        // count <= upper - cursor: the cursor addition cannot wrap, even at MAX.
        let end = b.demand_binary(Op::AddWrap, cursor, count)?;
        let mut arguments = vec![accumulator, cursor, end];
        arguments.extend(captures.iter().copied());
        let next = b.define(
            result_type,
            IrOperation::Call {
                function: chunk,
                arguments,
            },
        )?;
        b.terminate(IrTerminator::Jump {
            target: header,
            arguments: vec![next, end],
            drops: vec![],
        })?;
        b.current = Some(halve);
        let two = b.demand_constant(2)?;
        let half = b.demand_binary(Op::DivideExact, span, two)?;
        let middle = b.demand_binary(Op::AddWrap, cursor, half)?;
        let right_seed = match actualization {
            LoopActualization::IndependentMap => accumulator,
            LoopActualization::Reduction { combine, .. } => {
                b.identity_value(combine, result_type)?
            }
        };
        // Only order reverses publication. Seeds and the near-then-far
        // reduction combine retain source order in both schedules.
        let mut far = vec![right_seed, middle, upper];
        far.extend(captures.iter().copied());
        far.push(weight);
        let mut near = vec![accumulator, cursor, middle];
        near.extend(captures);
        near.push(weight);
        if frontier.is_some() {
            far.push(one);
            near.push(one);
        }
        let reverse = ablation == crate::DemandAblation::Order;
        let (first, second) = if reverse { (near, far) } else { (far, near) };
        let first = b.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: first,
            },
        )?;
        let second = b.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: second,
            },
        )?;
        let (near, far) = if reverse { (first, second) } else { (second, first) };
        let result = match actualization {
            LoopActualization::IndependentMap => near,
            LoopActualization::Reduction { combine, .. } => {
                b.combine_values(combine, result_type, near, far)?
            }
        };
        b.terminate(IrTerminator::Return {
            value: result,
            drops: vec![],
        })?;
        overlaps.push(IrOverlap { members: vec![first, second] });
        b.finish(
            format!("_par_slice_{name}"),
            overlaps,
            Some(IrSynthesis::Splitter),
        )
    }

    fn demand_constant(&mut self, bits: u64) -> Result<IrValueId, LoweringFailure> {
        self.define(
            U64,
            IrOperation::Constant(IrConstant::Integer { ty: U64, bits }),
        )
    }

    fn demand_binary(
        &mut self,
        operation: Op,
        left: IrValueId,
        right: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        let ty = if matches!(operation, Op::Less | Op::Greater | Op::GreaterEqual) {
            IrType::Bool
        } else {
            U64
        };
        self.define(
            ty,
            IrOperation::Integer {
                operation,
                operand_type: U64,
                arguments: vec![left, right],
            },
        )
    }
}

/// Prune only constant extents whose body has no calls or nested loops.
/// Static prices are advice, not upper bounds on data-dependent helper work.
/// Leave the LoopSplit instruction in place so the sequential clone path and
/// chunk ownership remain identical. Zero weight marks a pruned site only in
/// demand mode; ordinary assign_weights always produces a positive price.
pub(crate) fn prunable(
    function: &IrFunction,
    functions: &[IrFunction],
    lower: IrValueId,
    upper: IrValueId,
    chunk: u32,
    weight: u64,
) -> bool {
    let constant = |value| {
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::Constant(IrConstant::Integer { bits, .. }),
                    ..
                } if *result == value => Some(*bits),
                _ => None,
            })
    };
    let Some((lo, hi)) = constant(lower).zip(constant(upper)) else {
        return false;
    };
    let body = &functions[chunk as usize];
    hi.saturating_sub(lo).saturating_mul(weight) < WORK_UNIT
        && split::loop_depths(&body.blocks)
            .iter()
            .all(|depth| *depth <= 1)
        && !body.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i,
                IrInstruction::Define {
                    operation: IrOperation::Call { .. } | IrOperation::LoopSplit { .. },
                    ..
                }
            )
        })
}

pub(super) fn prune_and_report(functions: &mut [IrFunction], ledger: &mut Vec<String>) {
    let mut pruned = Vec::new();
    for (ordinal, function) in functions.iter().enumerate() {
        if function.synthesis != Some(IrSynthesis::Splitter) {
            for overlap in &function.overlaps {
                for site in overlap.handed_out() {
                    ledger.push(format!(
                        "PAR demand  {} v{}  checked group",
                        function.name,
                        site.ordinal()
                    ));
                }
            }
        }
        for i in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let IrInstruction::Define {
                result,
                operation:
                    IrOperation::LoopSplit {
                        lower,
                        upper,
                        chunk,
                        weight,
                        indexed,
                        ..
                    },
                ..
            } = i
            {
                let kind = if !indexed.is_empty() {
                    "legacy splitter for indexed"
                } else if prunable(function, functions, *lower, *upper, *chunk, *weight) {
                    pruned.push((ordinal, *result));
                    "pruned"
                } else {
                    "slice driver"
                };
                ledger.push(format!(
                    "PAR demand  {} v{}  {kind}",
                    function.name,
                    result.ordinal()
                ));
            }
        }
    }
    for (ordinal, site) in pruned {
        for instruction in functions[ordinal]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
        {
            if let IrInstruction::Define {
                result,
                operation: IrOperation::LoopSplit { weight, .. },
                ..
            } = instruction
                && *result == site
            {
                *weight = 0;
            }
        }
    }
}
