//! [SHARE-2, SHARE-3] an atomic statement, lowered to an acquire of what it
//! holds, its block with the binder naming that, and a release on every edge
//! that leaves the block.
//!
//! A statement reached through a handle takes a handle of its own before it
//! acquires, so the object or map stays live until the statement completes
//! whatever the block does with the target place [SHARE-2]; each edge leaving
//! the block gives up the hold and then releases that handle, which drops the
//! state when it was the last. A guard that reads false watches the object
//! and acquires it again once a statement that writes the object has ended
//! [SHARE-3]. A statement on an entry of a state an enclosing statement holds
//! reaches the map through that state and takes no handle.
//!
//! [SHARE-3] a statement holding a map's state whose keys are computed
//! before its block runs (`semantic::key_twins`) holds those keys' entries
//! instead of the map: its twin is lowered first and collects the keys, the
//! runtime holds their entries together, and each statement on an entry in
//! its block then reaches an entry already held.

use crate::NodePath;
use crate::semantic::{
    BindingId, CheckedAtomicForm, CheckedDrop, CheckedEnumType, CheckedExpression,
    CheckedStatement, CheckedType,
};
use crate::{
    IrAddressed, IrDrop, IrDropSubject, IrMatchTarget, IrNominalId, IrNominalKind, IrOperation,
    IrShared, IrTerminator, IrType, IrValueId, LoweringFailure,
};

use super::{GiveTarget, IrBuilder, lower_type};

/// One atomic statement whose block is being lowered.
#[derive(Clone, Copy)]
pub(super) struct AtomicRegion {
    /// The statement's own handle to the object or map, or the address of a
    /// map's state an enclosing statement holds.
    object: IrValueId,
    nominal: IrNominalId,
    hold: Hold,
    /// Whether the statement holds a handle of its own, which each edge
    /// leaving the block releases.
    owned: bool,
}

/// What a region holds, which decides how an edge leaving it gives it up.
#[derive(Clone, Copy)]
enum Hold {
    Object,
    Map,
    /// The entries under a map statement's collected keys.
    Keys,
    /// An entry under one of the keys an enclosing statement collected, at
    /// the address `entry`, which that statement holds until it ends.
    Held {
        entry: IrValueId,
    },
    /// An entry, at the address `entry`; `held` when the map's state is held
    /// by an enclosing statement, so no handle of its own is released;
    /// `reads` when the statement only reads it beside others that do.
    Entry {
        entry: IrValueId,
        held: bool,
        reads: bool,
    },
}

impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic(
        &mut self,
        node_path: &NodePath,
        target: &CheckedExpression,
        form: CheckedAtomicForm,
        borrowed: bool,
        key: Option<&CheckedExpression>,
        binding: BindingId,
        state: CheckedType,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        let state_type = lower_type(self.erasure, state)?;
        let referent = IrAddressed::of(state_type).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let held = matches!(form, CheckedAtomicForm::Entry { held: true, .. });
        if let Some(object) = self.collecting {
            // Inside a twin: the statement adds its key and runs no block.
            // Its target, the held state, is not bound yet.
            let key = key.ok_or(LoweringFailure::InvalidCheckedProgram)?;
            if !held || !body.is_empty() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            let key = self.expression(key)?;
            if !matches!(self.value_type(key)?, IrType::Range { .. }) {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            self.define(IrType::Unit, IrOperation::SharedMapKey { object, key })?;
            return Ok(());
        }
        let target_value = self.expression(target)?;
        let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(target_value)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let shape = match self
            .nominals
            .get(nominal.index())
            .map(|nominal| &nominal.kind)
        {
            Some(IrNominalKind::Shared { state, shape }) => Some((*state, *shape)),
            _ => None,
        };
        let expected = match (form, shape) {
            (CheckedAtomicForm::Object, Some((state, IrShared::Object))) => state == state_type,
            (CheckedAtomicForm::Map, Some((_, IrShared::Map { .. }))) => true,
            (CheckedAtomicForm::Entry { held: false, .. }, Some((_, IrShared::Map { entry })))
            | (CheckedAtomicForm::Entry { held: true, .. }, Some((_, IrShared::State { entry }))) => {
                entry == state_type
            }
            _ => false,
        };
        if !expected {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        // The statement's own handle, the handle a caller lends it, or the
        // held state's address.
        let object = if held {
            target_value
        } else if borrowed {
            self.define(
                IrType::Nominal(nominal),
                IrOperation::Load {
                    address: target_value,
                    referent: IrAddressed::Nominal(nominal),
                },
            )?
        } else {
            let loaded = self.define(
                IrType::Nominal(nominal),
                IrOperation::Load {
                    address: target_value,
                    referent: IrAddressed::Nominal(nominal),
                },
            )?;
            self.define(
                IrType::Nominal(nominal),
                IrOperation::SharedRetain {
                    nominal,
                    object: loaded,
                },
            )?
        };
        let enclosing = self.bindings.keys().copied().collect::<Vec<_>>();
        let hold = match form {
            CheckedAtomicForm::Object => {
                self.acquire_object(object, nominal, referent, binding, guard)?;
                Hold::Object
            }
            CheckedAtomicForm::Map => {
                let IrType::Nominal(state_nominal) = state_type else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let twin = self.key_twins.remove(node_path);
                let hold = match &twin {
                    Some(twin) => {
                        self.define(IrType::Unit, IrOperation::SharedMapKeys { object })?;
                        self.collect_keys(object, twin)?;
                        self.define(IrType::Unit, IrOperation::SharedMapHoldKeys { object })?;
                        Hold::Keys
                    }
                    None => {
                        self.define(IrType::Unit, IrOperation::SharedMapHold { object })?;
                        Hold::Map
                    }
                };
                let address = self.define(
                    IrType::Address(referent),
                    IrOperation::SharedMapState {
                        state: state_nominal,
                        object,
                    },
                )?;
                self.bind_atomic(binding, address)?;
                hold
            }
            CheckedAtomicForm::Entry { held, reads } => {
                // A statement inside a whole-map statement's block holds its
                // entry as that statement holds the map, alone.
                let reads = reads && !held;
                let key = key.ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let key = self.expression(key)?;
                if !matches!(self.value_type(key)?, IrType::Range { .. }) {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                // [SHARE-2] a statement on an entry of a held state lies
                // directly in the block of the statement holding that state.
                let collected = held
                    && matches!(
                        self.atomics.last().map(|region| region.hold),
                        Some(Hold::Keys)
                    );
                if collected {
                    let entry = self.define(
                        IrType::Address(referent),
                        IrOperation::SharedMapHeld {
                            nominal,
                            object,
                            key,
                        },
                    )?;
                    self.bind_atomic(binding, entry)?;
                    Hold::Held { entry }
                } else {
                    let entry = self.define(
                        IrType::Address(referent),
                        IrOperation::SharedMapLock {
                            nominal,
                            object,
                            key,
                            held,
                            reads,
                        },
                    )?;
                    self.bind_atomic(binding, entry)?;
                    Hold::Entry { entry, held, reads }
                }
            }
        };
        let region = AtomicRegion {
            object,
            nominal,
            hold,
            owned: !held && !borrowed,
        };
        self.atomics.push(region);
        let lowered = self.lower_statements(body, give_target);
        self.atomics.pop();
        lowered?;
        if self.current.is_some() {
            let drops = self.lower_drops(fallthrough_drops)?;
            self.append_drops(drops)?;
            self.leave_atomic(region)?;
        }
        self.bindings
            .retain(|binding, _| enclosing.contains(binding));
        Ok(())
    }

    /// Lowers a twin, the statements that compute the keys a statement
    /// holding the map `object` names will reach: each statement on an entry
    /// in it adds its key. What the twin bound and recorded for its own
    /// values is dropped again, since the block binds the same names when it
    /// runs.
    fn collect_keys(
        &mut self,
        object: IrValueId,
        twin: &[CheckedStatement],
    ) -> Result<(), LoweringFailure> {
        let enclosing = self.bindings.keys().copied().collect::<Vec<_>>();
        let call_results = self.call_results.clone();
        let addressed = self.addressed_bindings.clone();
        let outer = self.collecting.replace(object);
        let lowered = self.lower_statements(twin, None);
        self.collecting = outer;
        lowered?;
        if self.current.is_none() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.bindings
            .retain(|binding, _| enclosing.contains(binding));
        self.call_results = call_results;
        self.addressed_bindings = addressed;
        Ok(())
    }

    /// Binds a statement's binder to the address of what it holds. The
    /// binder carries the address itself, as a borrow parameter does, so a
    /// use of it as a value is that address, not a load through it.
    fn bind_atomic(
        &mut self,
        binding: BindingId,
        address: IrValueId,
    ) -> Result<(), LoweringFailure> {
        if self.bindings.insert(binding, address).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.promote_binding_if_needed(binding)
    }

    /// Acquires an object for a statement and binds its binder to the
    /// object's state, which the guard reads, waiting for the guard.
    fn acquire_object(
        &mut self,
        object: IrValueId,
        nominal: IrNominalId,
        referent: IrAddressed,
        binding: BindingId,
        guard: Option<&CheckedExpression>,
    ) -> Result<(), LoweringFailure> {
        let acquire = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(acquire);
        // [SHARE-2] a statement inside a map's or an entry's block has no
        // guard, and its block holds an entry its driver must keep running.
        if self.atomics.is_empty() {
            self.define(IrType::Unit, IrOperation::SharedAcquire { object })?;
        } else if guard.is_none() {
            self.define(IrType::Unit, IrOperation::SharedTake { object })?;
        } else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let state_address = self.define(
            IrType::Address(referent),
            IrOperation::SharedState { nominal, object },
        )?;
        self.bind_atomic(binding, state_address)?;
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
        let object = region.object;
        match region.hold {
            Hold::Object => self.define(IrType::Unit, IrOperation::SharedUnlock { object })?,
            Hold::Map => self.define(IrType::Unit, IrOperation::SharedMapUnhold { object })?,
            Hold::Keys => {
                self.define(IrType::Unit, IrOperation::SharedMapReleaseKeys { object })?
            }
            Hold::Held { entry } => self.define(
                IrType::Unit,
                IrOperation::SharedMapLeaveHeld { object, entry },
            )?,
            Hold::Entry { entry, held, reads } => self.define(
                IrType::Unit,
                IrOperation::SharedMapUnlock {
                    object,
                    entry,
                    held,
                    reads,
                },
            )?,
        };
        if !region.owned {
            return Ok(());
        }
        self.append_drops(vec![IrDrop {
            subject: IrDropSubject::Value(object),
            ty: IrType::Nominal(region.nominal),
        }])
    }

    /// How many atomic statements enclose the statement being lowered.
    pub(super) fn atomic_depth(&self) -> usize {
        self.atomics.len()
    }
}
