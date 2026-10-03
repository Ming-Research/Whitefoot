use crate::semantic::{BindingId, CheckedDrop, CheckedExpression, CheckedLoopId, CheckedStatement};
use crate::{
    IrBlockId, IrConstant, IrEnumType, IrIntegerOperation, IrMatchTarget, IrOperation,
    IrTerminator, IrType, IrValueId, LoweringFailure, NodePath,
};

use super::{GiveTarget, IrBuilder};

pub(super) const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

#[derive(Clone)]
pub(super) struct LoopTarget {
    pub(super) id: CheckedLoopId,
    pub(super) block: IrBlockId,
    pub(super) carried_bindings: Vec<BindingId>,
    /// How many atomic statements enclose the loop; a `break` leaves every
    /// one deeper [SHARE-2].
    pub(super) atomic_depth: usize,
}

impl IrBuilder<'_> {
    pub(super) fn lower_loop(
        &mut self,
        id: CheckedLoopId,
        body: &[CheckedStatement],
        backedge_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        let base_bindings = self.bindings.clone();
        let mut carried_bindings = base_bindings.keys().copied().collect::<Vec<_>>();
        carried_bindings.sort_by_key(|binding| binding.0);
        let parameter_types = carried_bindings
            .iter()
            .map(|binding| {
                base_bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)
                    .and_then(|value| self.value_type(value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (header, header_parameters) = self.new_block(&parameter_types)?;
        let (exit, exit_parameters) = self.new_block(&parameter_types)?;
        let entry_arguments = self.binding_values(&carried_bindings)?;
        self.terminate(IrTerminator::Jump {
            target: header,
            arguments: entry_arguments,
            drops: Vec::new(),
        })?;

        self.current = Some(header);
        self.bindings = base_bindings.clone();
        self.bind_parameters(&carried_bindings, &header_parameters)?;
        self.loops.push(LoopTarget {
            id,
            block: exit,
            carried_bindings: carried_bindings.clone(),
            atomic_depth: self.atomic_depth(),
        });
        if backedge_drops.is_empty() {
            self.emit_probe_skip_if_recognized(id, body, header, &carried_bindings)?;
        }
        self.lower_statements(body, give_target)?;
        if self.current.is_some() {
            let arguments = self.binding_values(&carried_bindings)?;
            let drops = self.lower_drops(backedge_drops)?;
            self.terminate(IrTerminator::Jump {
                target: header,
                arguments,
                drops,
            })?;
        }
        let Some(target) = self.loops.pop() else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        if target.id != id || target.block != exit {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }

        self.current = Some(exit);
        self.bindings = base_bindings;
        self.bind_parameters(&carried_bindings, &exit_parameters)
    }

    /// Lowers one counted `for`, either as the block graph below or — when
    /// [PAR-2] permitted this loop and this compilation asked for
    /// actualization — as a recursive split of its index range.
    ///
    /// The split is not a second lowering of the loop: its chunk *is* this
    /// block graph, built by the same code into a synthesized function, and
    /// the sequential world calls that chunk. What the split adds is a
    /// splitter beside it and a two-way rendering at this site.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_counted_range(
        &mut self,
        id: CheckedLoopId,
        node_path: &NodePath,
        binder: BindingId,
        lower: &CheckedExpression,
        upper: &CheckedExpression,
        body: &[CheckedStatement],
        backedge_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        // [FN-1] fixes endpoint evaluation order and evaluates each exactly
        // once. These SSA values are the private immutable captures, and they
        // are captured before the split decision so both renderings evaluate
        // the endpoints in the same place, exactly once.
        let lower_capture = self.expression(lower)?;
        let upper_capture = self.expression(upper)?;
        if self.value_type(lower_capture)? != U64 || self.value_type(upper_capture)? != U64 {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        if self.split_counted_range(
            id,
            node_path,
            binder,
            body,
            backedge_drops,
            lower_capture,
            upper_capture,
        )? {
            return Ok(());
        }
        self.counted_range_graph(
            id,
            binder,
            body,
            backedge_drops,
            give_target,
            lower_capture,
            upper_capture,
        )
    }

    /// The compiler-owned [FN-1] counted-range block graph, without
    /// desugaring it into source operations.
    ///
    /// The header owns the true/false guard, normal body fallthrough runs its
    /// drops before reaching the update block, and source exits target the
    /// continuation directly without an update.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn counted_range_graph(
        &mut self,
        id: CheckedLoopId,
        binder: BindingId,
        body: &[CheckedStatement],
        backedge_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
        lower_capture: IrValueId,
        upper_capture: IrValueId,
    ) -> Result<(), LoweringFailure> {
        let base_bindings = self.bindings.clone();
        let mut carried_bindings = base_bindings.keys().copied().collect::<Vec<_>>();
        carried_bindings.sort_by_key(|binding| binding.0);
        let mut base_parameter_types = carried_bindings
            .iter()
            .map(|binding| {
                base_bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)
                    .and_then(|value| self.value_type(value))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // The binder may need stable storage when the checked body contains a
        // permitted body-local shared borrow. Allocate that storage once,
        // before the first header, and carry the address rather than creating
        // a new address each iteration.
        if self.bindings.insert(binder, lower_capture).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.promote_binding_if_needed(binder)?;
        let binder_storage = self
            .bindings
            .get(&binder)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let binder_storage_type = self.value_type(binder_storage)?;

        let base_count = base_parameter_types.len();
        base_parameter_types.extend([U64, U64, binder_storage_type]);
        let (header, header_parameters) = self.new_block(&base_parameter_types)?;
        let (body_block, _) = self.new_block(&[])?;
        let (exhaustion, _) = self.new_block(&[])?;
        let exit_types = &base_parameter_types[..base_count];
        let (exit, exit_parameters) = self.new_block(exit_types)?;

        let mut entry_arguments = self.binding_values(&carried_bindings)?;
        // `binding_values` above includes the newly inserted binder only when
        // it is listed; the carried list was frozen from the outer scope, so
        // append the three counted identities explicitly.
        entry_arguments.extend([lower_capture, upper_capture, binder_storage]);
        self.terminate(IrTerminator::Jump {
            target: header,
            arguments: entry_arguments,
            drops: Vec::new(),
        })?;

        let header_base = header_parameters
            .get(..base_count)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let header_lower = *header_parameters
            .get(base_count)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let header_upper = *header_parameters
            .get(base_count + 1)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let header_binder = *header_parameters
            .get(base_count + 2)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;

        self.current = Some(header);
        self.bindings = base_bindings.clone();
        self.bind_parameters(&carried_bindings, header_base)?;
        if self.bindings.insert(binder, header_binder).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let binder_value = self.binding_value(binder)?;
        let guard = self.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![binder_value, header_upper],
            },
        )?;
        self.terminate(IrTerminator::Match {
            scrutinee: guard,
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: body_block,
                },
                IrMatchTarget {
                    tag: 0,
                    block: exhaustion,
                },
            ],
        })?;

        // The false header edge never enters the body. It therefore performs
        // no body cleanup and drops the binder/captures from the continuation
        // interface by forwarding only the incoming bindings.
        self.current = Some(exhaustion);
        self.bindings = base_bindings.clone();
        self.bind_parameters(&carried_bindings, header_base)?;
        let exhaustion_arguments = self.binding_values(&carried_bindings)?;
        self.terminate(IrTerminator::Jump {
            target: exit,
            arguments: exhaustion_arguments,
            drops: Vec::new(),
        })?;

        // The true edge enters with the same header identities. A break uses
        // the exit interface directly, so it cannot execute the update.
        self.current = Some(body_block);
        self.bindings = base_bindings.clone();
        self.bind_parameters(&carried_bindings, header_base)?;
        if self.bindings.insert(binder, header_binder).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.loops.push(LoopTarget {
            id,
            block: exit,
            carried_bindings: carried_bindings.clone(),
            atomic_depth: self.atomic_depth(),
        });
        self.lower_statements(body, give_target)?;
        let update = if self.current.is_some() {
            let (update, parameters) = self.new_block(&base_parameter_types)?;
            let mut arguments = self.binding_values(&carried_bindings)?;
            arguments.extend([header_lower, header_upper, header_binder]);
            let drops = self.lower_drops(backedge_drops)?;
            self.terminate(IrTerminator::Jump {
                target: update,
                arguments,
                drops,
            })?;
            Some((update, parameters))
        } else {
            None
        };
        let Some(target) = self.loops.pop() else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        if target.id != id || target.block != exit {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }

        if let Some((update, update_parameters)) = update {
            // Cleanup has completed on the edge into this block. The hidden
            // update is exact because the true guard established binder <
            // upper; AddWrap is the target-independent, total modular
            // operation. An all-terminating body creates no update block at all.
            let update_base = update_parameters
                .get(..base_count)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let update_lower = *update_parameters
                .get(base_count)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let update_upper = *update_parameters
                .get(base_count + 1)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let update_binder = *update_parameters
                .get(base_count + 2)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            self.current = Some(update);
            self.bindings = base_bindings.clone();
            self.bind_parameters(&carried_bindings, update_base)?;
            if self.bindings.insert(binder, update_binder).is_some() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            let old = self.binding_value(binder)?;
            let one = self.define(
                U64,
                IrOperation::Constant(IrConstant::Integer { ty: U64, bits: 1 }),
            )?;
            let next = self.define(
                U64,
                IrOperation::Integer {
                    operation: IrIntegerOperation::AddWrap,
                    operand_type: U64,
                    arguments: vec![old, one],
                },
            )?;
            let next_storage = match binder_storage_type {
                IrType::Address(referent) => {
                    self.store_addressed(update_binder, next, referent)?;
                    update_binder
                }
                _ => {
                    if self.bindings.insert(binder, next) != Some(update_binder) {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    next
                }
            };
            let mut backedge_arguments = self.binding_values(&carried_bindings)?;
            backedge_arguments.extend([update_lower, update_upper, next_storage]);
            self.terminate(IrTerminator::Jump {
                target: header,
                arguments: backedge_arguments,
                drops: Vec::new(),
            })?;
            self.counted_ranges.push(crate::IrCountedRange {
                blocks: header.index()..self.blocks.len(),
                continuation: exit,
                lower: lower_capture,
                upper: upper_capture,
            });
        }

        self.current = Some(exit);
        self.bindings = base_bindings;
        self.bind_parameters(&carried_bindings, &exit_parameters)
    }

    pub(super) fn bind_parameters(
        &mut self,
        bindings: &[BindingId],
        parameters: &[IrValueId],
    ) -> Result<(), LoweringFailure> {
        if bindings.len() != parameters.len() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        for (binding, value) in bindings.iter().zip(parameters) {
            if self.bindings.insert(*binding, *value).is_none() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
        }
        Ok(())
    }
}
