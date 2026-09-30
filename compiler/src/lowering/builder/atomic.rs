//! [SHARE-2, SHARE-3] an atomic statement, lowered to an acquire of its
//! object, its block with the binder naming the object's state, and a release
//! of the object on every edge that leaves the block.
//!
//! The statement takes a handle of its own before it acquires, so the object
//! stays live until the statement completes whatever the block does with the
//! target place [SHARE-2]; each edge leaving the block unlocks the object and
//! then releases that handle, which drops the state when it was the last. A
//! guard that reads false watches the object and acquires it again once a
//! statement that writes the object has ended [SHARE-3].

use crate::semantic::{
    BindingId, CheckedDrop, CheckedEnumType, CheckedExpression, CheckedStatement, CheckedType,
};
use crate::{
    IrAddressed, IrDrop, IrDropSubject, IrMatchTarget, IrNominalId, IrNominalKind, IrOperation,
    IrTerminator, IrType, IrValueId, LoweringFailure,
};

use super::{GiveTarget, IrBuilder, lower_type};

/// One atomic statement whose block is being lowered.
#[derive(Clone, Copy)]
pub(super) struct AtomicRegion {
    /// The statement's own handle to the object.
    object: IrValueId,
    nominal: IrNominalId,
}

impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic(
        &mut self,
        target: &CheckedExpression,
        binding: BindingId,
        state: CheckedType,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        let handle = self.expression(target)?;
        let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(handle)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let state_type = lower_type(self.erasure, state)?;
        if !matches!(
            self.nominals.get(nominal.index()).map(|nominal| &nominal.kind),
            Some(IrNominalKind::Shared { state }) if *state == state_type
        ) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let referent = IrAddressed::of(state_type).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let loaded = self.define(
            IrType::Nominal(nominal),
            IrOperation::Load {
                address: handle,
                referent: IrAddressed::Nominal(nominal),
            },
        )?;
        let object = self.define(
            IrType::Nominal(nominal),
            IrOperation::SharedRetain {
                nominal,
                object: loaded,
            },
        )?;
        let acquire = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(acquire);
        self.define(IrType::Unit, IrOperation::SharedAcquire { object })?;
        let state_address = self.define(
            IrType::Address(referent),
            IrOperation::SharedState { nominal, object },
        )?;
        let enclosing = self.bindings.keys().copied().collect::<Vec<_>>();
        if self.bindings.insert(binding, state_address).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        if let Some(guard) = guard {
            let holds = self.expression(guard)?;
            let proceed = self.new_block(&[])?.0;
            let watch = self.new_block(&[])?.0;
            self.terminate(IrTerminator::Match {
                scrutinee: holds,
                enum_type: crate::lowering::lower_enum_type(self.erasure, CheckedEnumType::Bool)?,
                targets: vec![
                    IrMatchTarget {
                        tag: 1,
                        block: proceed,
                    },
                    IrMatchTarget {
                        tag: 0,
                        block: watch,
                    },
                ],
            })?;
            self.current = Some(watch);
            self.define(IrType::Unit, IrOperation::SharedWatch { object })?;
            self.terminate(IrTerminator::Jump {
                target: acquire,
                arguments: Vec::new(),
                drops: Vec::new(),
            })?;
            self.current = Some(proceed);
        }
        self.atomics.push(AtomicRegion { object, nominal });
        let lowered = self.lower_statements(body, give_target);
        self.atomics.pop();
        lowered?;
        if self.current.is_some() {
            let drops = self.lower_drops(fallthrough_drops)?;
            self.append_drops(drops)?;
            self.leave_atomic(AtomicRegion { object, nominal })?;
        }
        self.bindings
            .retain(|binding, _| enclosing.contains(binding));
        Ok(())
    }

    /// Before an edge that leaves the blocks of the atomic statements from
    /// `depth` inward: each, innermost first, unlocks its object and releases
    /// its own handle [SHARE-2].
    pub(super) fn leave_atomics(&mut self, depth: usize) -> Result<(), LoweringFailure> {
        let regions = self
            .atomics
            .get(depth..)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .to_vec();
        for region in regions.into_iter().rev() {
            self.leave_atomic(region)?;
        }
        Ok(())
    }

    fn leave_atomic(&mut self, region: AtomicRegion) -> Result<(), LoweringFailure> {
        self.define(
            IrType::Unit,
            IrOperation::SharedUnlock {
                object: region.object,
            },
        )?;
        self.append_drops(vec![IrDrop {
            subject: IrDropSubject::Value(region.object),
            ty: IrType::Nominal(region.nominal),
        }])
    }

    /// How many atomic statements enclose the statement being lowered.
    pub(super) fn atomic_depth(&self) -> usize {
        self.atomics.len()
    }
}
