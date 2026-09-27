//! Stable storage for checked reads, writes and borrows of owned content.
//!
//! Checked storage paths are the semantic authority. Finding those explicit nodes
//! before CFG lowering lets an owner use one representation on every branch:
//! its binding carries a stable address, while ordinary value uses load from
//! that address. Lowering never infers a borrow from source shape or type alone.

use std::collections::HashSet;

use crate::semantic::{
    BindingId, CheckedExpression, CheckedFunction, CheckedSetTarget, CheckedStatement,
};

use super::*;

pub(super) fn collect_addressed_bindings(function: &CheckedFunction) -> HashSet<BindingId> {
    let mut bindings = HashSet::new();
    collect_statements(function.body.as_deref().unwrap_or_default(), &mut bindings);
    bindings
}

fn collect_statements(statements: &[CheckedStatement], bindings: &mut HashSet<BindingId>) {
    for statement in statements {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. } => collect_expression(value, bindings),
            CheckedStatement::PropagateLet { scrutinee, .. } => {
                collect_expression(scrutinee, bindings);
            }
            CheckedStatement::Set { target, value, .. } => {
                match target {
                    CheckedSetTarget::Place(_) => {}
                    CheckedSetTarget::RangeIndex(target) => {
                        for offset in target.offsets() {
                            collect_expression(offset, bindings);
                        }
                    }
                    CheckedSetTarget::Storage(root) => {
                        bindings.extend(root.binding());
                        collect_place(root, bindings);
                    }
                }
                collect_expression(value, bindings);
            }
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                collect_expression(scrutinee, bindings);
                if arms
                    .iter()
                    .flat_map(|arm| &arm.binders)
                    .any(|binder| binder.mode != crate::semantic::CheckedMode::Own)
                {
                    collect_borrowed_place_expression(scrutinee, bindings);
                }
                for arm in arms {
                    collect_statements(&arm.body, bindings);
                }
            }
            CheckedStatement::Loop { body, .. } => {
                collect_statements(body, bindings);
            }
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                collect_expression(lower, bindings);
                collect_expression(upper, bindings);
                collect_statements(body, bindings);
            }
            CheckedStatement::Proof(_) | CheckedStatement::Break { .. } => {}
        }
    }
}

/// Finds the owner whose actual storage a borrowed match must retain.
/// Ordinary expression collection deliberately does not promote mere reads;
/// this walk is selected by checked binder mode and only for the place forms
/// whose address lowering below preserves their source storage.
fn collect_borrowed_place_expression(
    expression: &CheckedExpression,
    bindings: &mut HashSet<BindingId>,
) {
    match expression {
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. } => {
            bindings.insert(*binding);
        }
        CheckedExpression::Project { binding, .. } => {
            bindings.insert(*binding);
        }
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            bindings.extend(root.binding());
            collect_place(root, bindings);
        }
        CheckedExpression::BorrowRangeIndex { place, .. } => {
            collect_expression(&place.offset, bindings);
            collect_steps(&place.path, None, bindings);
        }
        CheckedExpression::BoxDeref { value, .. }
        | CheckedExpression::ProjectValue { value, .. } => {
            collect_borrowed_place_expression(value, bindings);
        }
        _ => {}
    }
}

fn collect_expression(expression: &CheckedExpression, bindings: &mut HashSet<BindingId>) {
    match expression {
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            bindings.extend(root.binding());
            collect_place(root, bindings);
        }
        CheckedExpression::ContainerMeasure { root, .. } => collect_place(root, bindings),
        CheckedExpression::UserCall { arguments, .. }
        | CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::FloatOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. }
        | CheckedExpression::EnumEquality { arguments, .. }
        | CheckedExpression::ConstructStruct {
            fields: arguments, ..
        }
        | CheckedExpression::ConstructEnum {
            fields: arguments, ..
        } => {
            for argument in arguments {
                collect_expression(argument, bindings);
            }
        }
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. }
        | CheckedExpression::BoxDeref { value, .. }
        | CheckedExpression::ProjectValue { value, .. } => collect_expression(value, bindings),
        CheckedExpression::BoxTake {
            binding,
            path,
            cleanup,
            ..
        } => {
            bindings.insert(*binding);
            collect_steps(path, Some(*binding), bindings);
            for action in cleanup {
                let path = match action {
                    crate::semantic::CheckedOwnedTakeCleanup::Drop { path, .. }
                    | crate::semantic::CheckedOwnedTakeCleanup::BoxShell { path, .. } => path,
                };
                collect_steps(path, Some(*binding), bindings);
            }
        }
        CheckedExpression::ArrayIndex { offset, .. } => collect_expression(offset, bindings),
        CheckedExpression::RangeIndex { place, .. } => {
            collect_expression(&place.offset, bindings);
            collect_steps(&place.path, None, bindings);
        }
        CheckedExpression::RangeElementMeasure { place, .. } => {
            collect_expression(&place.offset, bindings);
            collect_steps(&place.path, None, bindings);
        }
        CheckedExpression::BorrowRangeIndex { place, .. } => {
            collect_expression(&place.offset, bindings);
            collect_steps(&place.path, None, bindings);
        }
        // [TYPE-9] a runtime-capacity `Array<T>` is only ever `Box` content,
        // and its block is reached through the pointer the owner's slot
        // holds, exactly as a boxed window's is
        // (compiler/storage-representation). The root binding therefore
        // carries a stable address wherever the block is read, measured,
        // borrowed, or written at an element.
        CheckedExpression::BufferIndex { root, offset, .. } => {
            bindings.insert(root.binding);
            collect_expression(offset, bindings);
        }
        // [REF-4] the formation reads the source place's own offsets and
        // evaluates both endpoints.
        CheckedExpression::RangeOf {
            source, start, end, ..
        } => {
            if let crate::semantic::CheckedRangeSource::Storage(root) = source {
                bindings.extend(root.binding());
                collect_place(root, bindings);
            }
            collect_expression(start, bindings);
            collect_expression(end, bindings);
        }
        CheckedExpression::BufferMeasure { root, .. } => {
            bindings.insert(root.binding);
        }
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::Binding { .. }
        | CheckedExpression::ArrayMeasure { .. }
        | CheckedExpression::RangeMeasure { .. }
        | CheckedExpression::DerefAddressed { .. }
        | CheckedExpression::Project { .. } => {}
    }
}

fn collect_place(root: &crate::semantic::CheckedContainerRoot, bindings: &mut HashSet<BindingId>) {
    collect_steps(&root.path, root.binding(), bindings);
}

fn collect_steps(
    steps: &[crate::semantic::CheckedPlaceStep],
    owner: Option<BindingId>,
    bindings: &mut HashSet<BindingId>,
) {
    for step in steps {
        match step {
            // A Box projection follows the pointer in its actual owner slot,
            // including when only a descriptor measure is read through it.
            crate::semantic::CheckedPlaceStep::BoxReferent(_) => {
                bindings.extend(owner);
            }
            crate::semantic::CheckedPlaceStep::Subscript(subscript) => {
                collect_expression(&subscript.offset, bindings);
            }
            crate::semantic::CheckedPlaceStep::Field(_) => {}
        }
    }
}

impl IrBuilder<'_> {
    /// Resolve a mutation's complete address once, before its right-hand side.
    pub(super) fn addressed_target(
        &mut self,
        target: &CheckedSetTarget,
        storage: IrValueId,
    ) -> Result<Option<IrValueId>, LoweringFailure> {
        let address = match target {
            CheckedSetTarget::Storage(root) => self.lower_place_address(root)?,
            CheckedSetTarget::Place(place)
                if matches!(self.value_type(storage)?, IrType::Address(_)) =>
            {
                let path: Vec<_> = place
                    .fields
                    .iter()
                    .copied()
                    .map(crate::semantic::CheckedPlaceStep::Field)
                    .collect();
                self.project_address_path(storage, &path)?
            }
            _ => return Ok(None),
        };
        Ok(Some(address))
    }

    pub(super) fn lower_place_address(
        &mut self,
        root: &crate::semantic::CheckedContainerRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let address = match root.root {
            crate::semantic::CheckedPlaceRoot::Binding(binding) => self
                .bindings
                .get(&binding)
                .copied()
                .ok_or(LoweringFailure::InvalidCheckedProgram)?,
            crate::semantic::CheckedPlaceRoot::Constant(id) => {
                let constant = self
                    .constants
                    .get(id.0 as usize)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let referent =
                    IrAddressed::of(constant.ty()).ok_or(LoweringFailure::InvalidCheckedProgram)?;
                self.define(
                    IrType::Address(referent),
                    IrOperation::ConstantAddress {
                        constant: constant.id(),
                    },
                )?
            }
        };
        let address = self.project_address_path(address, &root.path)?;
        let referent = IrAddressed::of(lower_type(self.erasure, root.ty)?)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        if self.value_type(address)? != IrType::Address(referent) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(address)
    }

    /// Lowers the address carried by a checked borrowed-place expression.
    ///
    /// In particular, dereferencing a Box owner slot follows the stored Box
    /// pointer to its allocation. It never takes the address of the value
    /// snapshot which an ordinary read materializes from that allocation.
    pub(super) fn lower_borrowed_place_address(
        &mut self,
        expression: &CheckedExpression,
    ) -> Result<IrValueId, LoweringFailure> {
        let checked_ty = expression.ty();
        let ty = lower_type(self.erasure, checked_ty)?;
        let address = match expression {
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => {
                self.lower_addressed_borrow(*binding, ty)?
            }
            CheckedExpression::BorrowAddressed { root, .. } => self.lower_place_address(root)?,
            CheckedExpression::BorrowRangeIndex { place, .. } => self.lower_range_address(
                &place.root,
                &place.offset,
                &place.path,
                place.target_domain,
            )?,
            CheckedExpression::ReadStorage { root, .. } => self.lower_place_address(root)?,
            CheckedExpression::BoxDeref {
                nominal,
                referent,
                value,
                ..
            } => {
                let owner = self.lower_borrowed_place_address(value)?;
                let nominal = self.erased(*nominal);
                let referent = lower_type(self.erasure, *referent)?;
                self.define(
                    IrType::Address(
                        IrAddressed::of(referent).ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    ),
                    IrOperation::ProjectAddress {
                        address: owner,
                        projection: IrPlaceStep::BoxReferent { nominal },
                    },
                )?
            }
            CheckedExpression::ProjectValue {
                value,
                nominal,
                field,
                ty,
                ..
            } => {
                let owner = self.lower_borrowed_place_address(value)?;
                let nominal = self.erased(*nominal);
                let field_ty = lower_type(self.erasure, *ty)?;
                self.define(
                    IrType::Address(
                        IrAddressed::of(field_ty).ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    ),
                    IrOperation::ProjectAddress {
                        address: owner,
                        projection: IrPlaceStep::Field {
                            nominal,
                            field: *field,
                        },
                    },
                )?
            }
            CheckedExpression::Project {
                binding, fields, ..
            } => {
                let root = self
                    .bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let path = fields
                    .iter()
                    .copied()
                    .map(crate::semantic::CheckedPlaceStep::Field)
                    .collect::<Vec<_>>();
                self.project_address_path(root, &path)?
            }
            _ => {
                let value = self.expression(expression)?;
                if self.value_type(value)?
                    != IrType::Address(
                        IrAddressed::of(ty).ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    )
                {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                value
            }
        };
        if self.value_type(address)?
            != IrType::Address(IrAddressed::of(ty).ok_or(LoweringFailure::InvalidCheckedProgram)?)
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(address)
    }

    pub(super) fn project_address_path(
        &mut self,
        mut address: IrValueId,
        path: &[crate::semantic::CheckedPlaceStep],
    ) -> Result<IrValueId, LoweringFailure> {
        for step in path {
            let IrType::Address(base) = self.value_type(address)? else {
                return Err(LoweringFailure::InvalidCheckedProgram);
            };
            let (projection, ty) = match step {
                crate::semantic::CheckedPlaceStep::Field(field) => {
                    let IrType::Nominal(nominal) = base.ty() else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    };
                    let IrNominalKind::Struct { fields } = &self.nominals[nominal.index()].kind
                    else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    };
                    let ty = fields
                        .get(*field as usize)
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?
                        .ty;
                    (
                        IrPlaceStep::Field {
                            nominal,
                            field: *field,
                        },
                        ty,
                    )
                }
                crate::semantic::CheckedPlaceStep::BoxReferent(checked_nominal) => {
                    let nominal = self.erased(*checked_nominal);
                    if base.ty() != IrType::Nominal(nominal) {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    let IrNominalKind::Box { referent, .. } = &self.nominals[nominal.index()].kind
                    else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    };
                    (IrPlaceStep::BoxReferent { nominal }, *referent)
                }
                crate::semantic::CheckedPlaceStep::Subscript(subscript) => {
                    let offset = self.expression(&subscript.offset)?;
                    let projection = match lower_type(self.erasure, subscript.base_type)? {
                        IrType::Array { .. } => IrPlaceStep::ArrayElement {
                            offset,
                            target_domain: subscript.target_domain.into(),
                        },
                        IrType::Buffer { .. } => IrPlaceStep::BufferElement {
                            offset,
                            target_domain: subscript.target_domain.into(),
                        },
                        IrType::Window { .. } => IrPlaceStep::RunElement {
                            offset,
                            target_domain: subscript.target_domain.into(),
                        },
                        _ => return Err(LoweringFailure::InvalidCheckedProgram),
                    };
                    (
                        projection,
                        lower_type(self.erasure, subscript.element_type)?,
                    )
                }
            };
            let referent = IrAddressed::of(ty).ok_or(LoweringFailure::InvalidCheckedProgram)?;
            address = self.define(
                IrType::Address(referent),
                IrOperation::ProjectAddress {
                    address,
                    projection,
                },
            )?;
        }
        Ok(address)
    }

    pub(super) fn promote_binding_if_needed(
        &mut self,
        binding: BindingId,
    ) -> Result<(), LoweringFailure> {
        if !self.addressed_bindings.contains(&binding) {
            return Ok(());
        }
        let value = self
            .bindings
            .get(&binding)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let ty = self.value_type(value)?;
        // Initialization can supply an address directly for a borrow parameter,
        // returned borrow, or borrowed match binder. That binding carries the
        // address itself: a later ordinary use must not perform the implicit
        // load used for an owner which this method promotes into storage.
        // Synthesized captures inherit already-classified bindings without
        // reinitializing them here.
        if matches!(ty, IrType::Address(_)) {
            self.addressed_bindings.remove(&binding);
            return Ok(());
        }
        let referent = self.addressed_referent(ty)?;
        let address = self.define(
            IrType::Address(referent),
            IrOperation::AddressOf { value, referent },
        )?;
        if self.bindings.insert(binding, address) != Some(value) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(())
    }

    /// The referent an address may point at.
    ///
    /// A Box borrow addresses its owner's pointer slot, just as aggregate
    /// borrows address their owner's inline storage.
    fn addressed_referent(&self, ty: IrType) -> Result<IrAddressed, LoweringFailure> {
        let referent = IrAddressed::of(ty).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        if let IrAddressed::Nominal(nominal) = referent
            && !matches!(
                self.nominals
                    .get(nominal.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .kind,
                IrNominalKind::Struct { .. }
                    | IrNominalKind::Enum { .. }
                    | IrNominalKind::Box { .. }
                    | IrNominalKind::Opaque
            )
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(referent)
    }

    pub(super) fn lower_addressed_borrow(
        &self,
        binding: BindingId,
        ty: IrType,
    ) -> Result<IrValueId, LoweringFailure> {
        let value = self
            .bindings
            .get(&binding)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        if self.value_type(value)? != IrType::Address(self.addressed_referent(ty)?) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(value)
    }

    pub(super) fn binding_value(
        &mut self,
        binding: BindingId,
    ) -> Result<IrValueId, LoweringFailure> {
        let storage = self
            .bindings
            .get(&binding)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        self.load_storage_value(storage)
    }

    pub(super) fn load_storage_value(
        &mut self,
        storage: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        let IrType::Address(referent) = self.value_type(storage)? else {
            return Ok(storage);
        };
        self.define(
            referent.ty(),
            IrOperation::Load {
                address: storage,
                referent,
            },
        )
    }

    pub(super) fn store_addressed(
        &mut self,
        address: IrValueId,
        value: IrValueId,
        referent: IrAddressed,
    ) -> Result<(), LoweringFailure> {
        if self.value_type(address)? != IrType::Address(referent)
            || self.value_type(value)? != referent.ty()
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.current_block_mut()?
            .instructions
            .push(IrInstruction::Store {
                address,
                value,
                referent,
            });
        Ok(())
    }
}
