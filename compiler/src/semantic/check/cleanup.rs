use crate::SemanticCompilerFailure;

use super::super::model::{
    CheckedDrop, CheckedExpression, CheckedNominalKind, CheckedSetTarget, CheckedStatement,
    CheckedType,
};
use super::{CheckStop, Checker};

impl<'unit> Checker<'unit> {
    /// [STOR-8, PROV-6] Validate every release graph reached by the checked
    /// cleanup traversal before [EFF-2] compares the body's ordinary effects.
    pub(super) fn validate_release_graphs(
        &self,
        statements: &[CheckedStatement],
    ) -> Result<(), CheckStop> {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. } => {
                    self.validate_expression_release_graphs(value)?;
                }
                CheckedStatement::PropagateLet {
                    scrutinee,
                    error_drops,
                    ..
                } => {
                    self.validate_expression_release_graphs(scrutinee)?;
                    self.validate_drop_release_graphs(error_drops)?;
                }
                CheckedStatement::Set { target, value, .. } => {
                    match target {
                        CheckedSetTarget::Place(_) => {}
                        CheckedSetTarget::RangeIndex(target) => {
                            for offset in target.offsets() {
                                self.validate_expression_release_graphs(offset)?;
                            }
                        }
                        CheckedSetTarget::Storage(target) => {
                            for offset in target.offsets() {
                                self.validate_expression_release_graphs(offset)?;
                            }
                        }
                    }
                    self.validate_expression_release_graphs(value)?;
                }
                CheckedStatement::Evaluate { value, .. } => {
                    self.validate_expression_release_graphs(value)?;
                }
                CheckedStatement::DropExpression { value, drops, .. } => {
                    self.validate_expression_release_graphs(value)?;
                    for drop in drops {
                        self.release_graph_nodes(drop.ty)?;
                    }
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Return { value, drops, .. } => {
                    self.validate_expression_release_graphs(value)?;
                    self.validate_drop_release_graphs(drops)?;
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    self.validate_expression_release_graphs(scrutinee)?;
                    for arm in arms {
                        self.validate_release_graphs(&arm.body)?;
                        self.validate_drop_release_graphs(&arm.fallthrough_drops)?;
                    }
                }
                CheckedStatement::Give { value, drops, .. } => {
                    self.validate_expression_release_graphs(value)?;
                    self.validate_drop_release_graphs(drops)?;
                }
                CheckedStatement::Loop {
                    body,
                    backedge_drops,
                    ..
                } => {
                    self.validate_release_graphs(body)?;
                    self.validate_drop_release_graphs(backedge_drops)?;
                }
                CheckedStatement::CountedRange {
                    lower,
                    upper,
                    body,
                    backedge_drops,
                    ..
                } => {
                    self.validate_expression_release_graphs(lower)?;
                    self.validate_expression_release_graphs(upper)?;
                    self.validate_release_graphs(body)?;
                    self.validate_drop_release_graphs(backedge_drops)?;
                }
                CheckedStatement::Break { drops, .. } => {
                    self.validate_drop_release_graphs(drops)?;
                }
            }
        }
        Ok(())
    }

    fn validate_drop_release_graphs(&self, drops: &[CheckedDrop]) -> Result<(), CheckStop> {
        for drop in drops {
            self.release_graph_nodes(drop.ty)?;
        }
        Ok(())
    }

    fn validate_expression_release_graphs(
        &self,
        expression: &CheckedExpression,
    ) -> Result<(), CheckStop> {
        match expression {
            CheckedExpression::Project { residual_drops, .. } => {
                for drop in residual_drops {
                    self.release_graph_nodes(drop.ty)?;
                }
            }
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
                    self.validate_expression_release_graphs(argument)?;
                }
            }
            CheckedExpression::BoxTake { cleanup, .. } => {
                for action in cleanup {
                    if let super::super::model::CheckedOwnedTakeCleanup::Drop { ty, .. } = action {
                        self.release_graph_nodes(*ty)?;
                    }
                }
            }
            CheckedExpression::NumericConversion { value, .. }
            | CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                self.validate_expression_release_graphs(value)?;
            }
            CheckedExpression::ReadStorage { root, .. } => {
                for offset in root.offsets() {
                    self.validate_expression_release_graphs(offset)?;
                }
            }
            CheckedExpression::ArrayIndex { offset, .. }
            | CheckedExpression::BufferIndex { offset, .. } => {
                self.validate_expression_release_graphs(offset)?;
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                for offset in place.offsets() {
                    self.validate_expression_release_graphs(offset)?;
                }
            }
            CheckedExpression::RangeOf {
                source, start, end, ..
            } => {
                if let crate::semantic::CheckedRangeSource::Storage(root) = source {
                    for offset in root.offsets() {
                        self.validate_expression_release_graphs(offset)?;
                    }
                }
                self.validate_expression_release_graphs(start)?;
                self.validate_expression_release_graphs(end)?;
            }
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::Binding { .. }
            | CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::DerefAddressed { .. } => {}
        }
        Ok(())
    }
    pub(super) fn drop_paths(
        &self,
        ty: CheckedType,
        fields: Vec<u32>,
    ) -> Result<Vec<(Vec<u32>, CheckedType)>, CheckStop> {
        let mut drops = Vec::new();
        let mut pending = vec![(ty, fields, false)];
        while let Some((current, path, postorder)) = pending.pop() {
            if postorder {
                drops.push((path, current));
                continue;
            }
            match current {
                CheckedType::Unit
                | CheckedType::Bool
                | CheckedType::Integer(_)
                | CheckedType::Float(_)
                | CheckedType::GenericInt(_)
                | CheckedType::GenericFloat(_)
                | CheckedType::Generic(_) => {}
                CheckedType::Array { .. }
                | CheckedType::Buffer { .. }
                | CheckedType::Window { .. } => {
                    // [OWN-1, STOR-3] an `Array` of copy elements is copy and
                    // a copy value has an empty release.
                    if !self.is_copy_type(current)? {
                        drops.push((path, current));
                    }
                }
                CheckedType::Nominal(id) => {
                    let nominal = self.nominal(id)?;
                    // [OWN-1, STOR-3] a nominal whose every owned part is
                    // copy, and whose declaration removes nothing, is copy
                    // and has an empty release. A tag-only enum declared
                    // `nocopy` is affine and owns nothing, so its release is
                    // empty as well.
                    if self.is_copy_type(current)? || nominal.is_tag_only_enum() {
                        continue;
                    }
                    match &nominal.kind {
                        CheckedNominalKind::Struct { fields } => {
                            pending.push((current, path.clone(), true));
                            // PROV-6 visits fields in declaration order; the
                            // explicit work stack is last-in, first-out.
                            for (index, field) in fields.iter().enumerate().rev() {
                                if self.is_copy_type(field.ty)? {
                                    continue;
                                }
                                let mut child = path.clone();
                                child.push(
                                    u32::try_from(index)
                                        .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                                );
                                pending.push((field.ty, child, false));
                            }
                        }
                        CheckedNominalKind::Enum { .. }
                        | CheckedNominalKind::Box { .. }
                        | CheckedNominalKind::Opaque => {
                            drops.push((path, current));
                        }
                    }
                }
            }
        }
        Ok(drops)
    }

    pub(super) fn residual_drop_paths(
        &self,
        ty: CheckedType,
        moved: &[u32],
    ) -> Result<Vec<(Vec<u32>, CheckedType)>, CheckStop> {
        let mut drops = Vec::new();
        let mut pending = vec![(ty, Vec::new(), true, 0_usize, false)];
        while let Some((current, path, selected, depth, postorder)) = pending.pop() {
            if selected && depth == moved.len() {
                continue;
            }
            if postorder {
                drops.push((path, current));
                continue;
            }
            match current {
                CheckedType::Unit
                | CheckedType::Bool
                | CheckedType::Integer(_)
                | CheckedType::Float(_)
                | CheckedType::GenericInt(_)
                | CheckedType::GenericFloat(_)
                | CheckedType::Generic(_)
                | CheckedType::Array { .. }
                | CheckedType::Buffer { .. }
                | CheckedType::Window { .. }
                    if selected =>
                {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                CheckedType::Unit
                | CheckedType::Bool
                | CheckedType::Integer(_)
                | CheckedType::Float(_)
                | CheckedType::GenericInt(_)
                | CheckedType::GenericFloat(_) => {}
                CheckedType::Generic(_)
                | CheckedType::Array { .. }
                | CheckedType::Buffer { .. }
                | CheckedType::Window { .. } => {
                    if !self.is_copy_type(current)? {
                        drops.push((path, current));
                    }
                }
                CheckedType::Nominal(id) => {
                    let nominal = self.nominal(id)?;
                    if self.is_copy_type(current)? {
                        if selected {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        continue;
                    }
                    let CheckedNominalKind::Struct { fields } = &nominal.kind else {
                        if selected {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        drops.push((path, current));
                        continue;
                    };
                    if !selected {
                        pending.push((current, path.clone(), false, depth, true));
                    }
                    let selected_field = if selected {
                        Some(
                            moved
                                .get(depth)
                                .copied()
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                        )
                    } else {
                        None
                    };
                    if let Some(selected_field) = selected_field {
                        let field = fields
                            .get(selected_field as usize)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        if self.is_copy_type(field.ty)? {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                    }
                    for (index, field) in fields.iter().enumerate().rev() {
                        if self.is_copy_type(field.ty)? {
                            continue;
                        }
                        let index = u32::try_from(index)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                        let mut child = path.clone();
                        child.push(index);
                        let child_selected = selected_field == Some(index);
                        pending.push((
                            field.ty,
                            child,
                            child_selected,
                            depth + usize::from(child_selected),
                            false,
                        ));
                    }
                }
            }
        }
        Ok(drops)
    }
}
