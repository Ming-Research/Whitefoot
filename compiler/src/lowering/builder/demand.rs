//! Segmented range execution with request-driven hand-out of the far half.
use super::IrBuilder;
use super::loops::U64;
use crate::semantic::LoopActualization;
use crate::{
    IrConstant, IrFunction, IrIntegerOperation as Op, IrOperation, IrOverlap, IrSynthesis,
    IrTerminator, IrType, IrValueId, LoweringFailure,
};

/// Estimated work between eligible request polls; this is not elapsed time.
const SLICE_WORK: u64 = 150_000;
const WORK_UNIT: u64 = super::call_grain::CALL_OFFER_WORK_UNIT;

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
        // supplies a positive runtime-extent price, or the established static fallback.
        let weight = b.new_parameter(U64)?;
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
        // below the minimum span never touches the word idle workers write.
        // Its remaining computation is still the ordinary subrange body.
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
        // Publish the far half first; the owner retains the near half.
        // Seeds and near-before-far combination preserve source leaf order.
        let mut far = vec![right_seed, middle, upper];
        far.extend(captures.iter().copied());
        far.push(weight);
        let mut near = vec![accumulator, cursor, middle];
        near.extend(captures);
        near.push(weight);
        let first = b.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: far,
            },
        )?;
        let second = b.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: near,
            },
        )?;
        let (near, far) = (second, first);
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
        overlaps.push(IrOverlap {
            members: vec![first, second],
        });
        b.finish(
            format!("_par_split_{name}"),
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
