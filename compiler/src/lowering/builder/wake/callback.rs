//! Full guards which cannot use the compact atom representation. Admission
//! here selects an optimization, never source acceptance: unknown operations
//! leave the ordinary unconditional watch in place.

use std::collections::HashSet;

use super::*;
use crate::semantic::model::expression_children;
use crate::semantic::{CheckedContainerRoot, CheckedSetTarget};
use crate::{IrTerminator, OverlapLowering};

fn contains_call(expression: &CheckedExpression) -> bool {
    matches!(expression, CheckedExpression::UserCall { .. })
        || expression_children(expression).into_iter().any(contains_call)
}

/// Audit the complete direct-call closure before a writer may execute it.
/// No loop or call cycle is admitted, and every executable operation is a
/// local computation or a read. In particular a bodyless function is not
/// trusted merely because its row is pure.
struct Audit<'builder, 'program> {
    builder: &'builder IrBuilder<'program>,
    target: BindingId,
    captures: Vec<(BindingId, IrType)>,
    reads_target: bool,
    active: HashSet<FunctionId>,
    checked: HashSet<FunctionId>,
}

impl Audit<'_, '_> {
    fn binding(&mut self, binding: BindingId, caller: bool, reference: bool) -> bool {
        if !caller {
            // Every helper input was audited in its caller and this body's
            // whitelist can introduce no other state root.
            return true;
        }
        if binding == self.target {
            self.reads_target = true;
            return true;
        }
        if reference {
            return false;
        }
        let Some(value) = self.builder.bindings.get(&binding).copied() else {
            return false;
        };
        let Ok(mut ty) = self.builder.value_type(value) else {
            return false;
        };
        if self.builder.addressed_bindings.contains(&binding) {
            let IrType::Address(referent) = ty else {
                return false;
            };
            ty = referent.ty();
        }
        if !self.builder.wake_scalar(ty) {
            return false;
        }
        if !self.captures.iter().any(|(captured, _)| *captured == binding) {
            self.captures.push((binding, ty));
        }
        true
    }

    fn place(&mut self, root: &CheckedContainerRoot, caller: bool) -> bool {
        // Field, Box-content and descriptor loads are fixed work and cannot
        // acquire another object. Subscripts are conservatively left out.
        root.path.iter().all(|step| {
            matches!(
                step,
                CheckedPlaceStep::Field(_)
                    | CheckedPlaceStep::BoxReferent(_)
                    | CheckedPlaceStep::Measure(_)
            )
        }) && root
            .binding()
            .is_some_and(|binding| self.binding(binding, caller, true))
    }

    fn expression(&mut self, expression: &CheckedExpression, caller: bool) -> bool {
        let permitted = match expression {
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::IntegerOperation { .. }
            | CheckedExpression::FloatOperation { .. }
            | CheckedExpression::NumericConversion { .. }
            | CheckedExpression::Reinterpret { .. }
            | CheckedExpression::BooleanOperation { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. }
            | CheckedExpression::ProjectValue { .. }
            | CheckedExpression::BoxDeref { .. } => true,
            CheckedExpression::ValueEquality { operand_type, .. } => {
                lower_type(self.builder.erasure, *operand_type)
                    .is_ok_and(|ty| self.builder.wake_scalar(ty))
            }
            CheckedExpression::Binding {
                binding,
                consume_root,
                ..
            } => !consume_root && self.binding(*binding, caller, false),
            CheckedExpression::DerefAddressed { binding, .. } => {
                self.binding(*binding, caller, true)
            }
            CheckedExpression::Project {
                binding,
                consume_root,
                residual_drops,
                ..
            } => {
                !consume_root
                    && residual_drops.is_empty()
                    && self.binding(*binding, caller, false)
            }
            CheckedExpression::BorrowAddressed { root, .. }
            | CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::ReadStorage { root, .. } => self.place(root, caller),
            CheckedExpression::UserCall { function, .. } => self.function(*function),
            _ => false,
        };
        permitted
            && expression_children(expression)
                .into_iter()
                .all(|child| self.expression(child, caller))
    }

    fn function(&mut self, function: FunctionId) -> bool {
        if self.checked.contains(&function) {
            return true;
        }
        if !self.active.insert(function) {
            return false;
        }
        let functions = self.builder.checked_functions;
        let Some(callee) = functions.get(function.0 as usize) else {
            return false;
        };
        if callee.waiting.waits
            || callee.allocates
            || !callee.held_parameters.is_empty()
            || !callee.declared_state_writes.is_empty()
            || !matches!(callee.body_disposition, CheckedBodyDisposition::Inhabited)
        {
            return false;
        }
        let Some(body) = callee.body.as_deref() else {
            return false;
        };
        let valid = self.statements(body);
        self.active.remove(&function);
        if valid {
            self.checked.insert(function);
        }
        valid
    }

    fn statements(&mut self, body: &[CheckedStatement]) -> bool {
        body.iter().all(|statement| match statement {
            CheckedStatement::Proof(_) => true,
            CheckedStatement::Let { value, .. } | CheckedStatement::Evaluate { value, .. } => {
                self.expression(value, false)
            }
            CheckedStatement::DestructuringLet { value, covered, .. } => {
                covered.is_empty() && self.expression(value, false)
            }
            CheckedStatement::Return { value, drops, .. }
            | CheckedStatement::Give { value, drops, .. } => {
                drops.is_empty() && self.expression(value, false)
            }
            CheckedStatement::Set {
                target,
                value,
                releases_displaced_storage,
                ..
            } => {
                // A direct owned scalar local is private scratch. Reference
                // writes and all indirect storage targets stay excluded.
                matches!(target, CheckedSetTarget::Place(place)
                    if place.mode == CheckedMode::Own && place.fields.is_empty()
                    && lower_type(self.builder.erasure, place.ty)
                        .is_ok_and(|ty| self.builder.wake_scalar(ty)))
                    && !releases_displaced_storage
                    && self.expression(value, false)
            }
            CheckedStatement::Match { scrutinee, arms, .. }
            | CheckedStatement::ValueMatchLet { scrutinee, arms, .. } => {
                self.expression(scrutinee, false)
                    && arms.iter().all(|arm| {
                        arm.covered.is_empty()
                            && arm.fallthrough_drops.is_empty()
                            && self.statements(&arm.body)
                    })
            }
            _ => false,
        })
    }
}

impl IrBuilder<'_> {
    pub(super) fn wake_callback(
        &mut self,
        expression: &CheckedExpression,
        binding: BindingId,
        state: IrValueId,
    ) -> Result<Option<IrWakeTest>, LoweringFailure> {
        // Keep the original inline lowering (including its conservative
        // no-filter cases) unchanged. This path supplies full evaluation for
        // helper guards for which atom extraction found no useful condition.
        if !contains_call(expression) {
            return Ok(None);
        }
        let mut audit = Audit {
            builder: self,
            target: binding,
            captures: Vec::new(),
            reads_target: false,
            active: HashSet::new(),
            checked: HashSet::new(),
        };
        if !audit.expression(expression, true) || !audit.reads_target {
            return Ok(None);
        }
        let captures = audit.captures;
        let mut callback = Self::new(
            self.context(),
            IrType::Bool,
            HashSet::new(),
            None,
            OverlapLowering::Off,
            self.function_name,
        )?;
        let parameter = callback.new_parameter(self.value_type(state)?)?;
        callback.bindings.insert(binding, parameter);
        let mut values = vec![state];
        for (binding, ty) in captures {
            let stored = *self
                .bindings
                .get(&binding)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let value = if self.addressed_bindings.contains(&binding) {
                self.load_storage_value(stored)?
            } else {
                stored
            };
            values.push(value);
            let parameter = callback.new_parameter(ty)?;
            callback.bindings.insert(binding, parameter);
        }
        let result = callback.expression(expression)?;
        callback.terminate(IrTerminator::Return {
            value: result,
            drops: Vec::new(),
        })?;
        let (ordinal, name) = self.synthesis.borrow_mut().reserve(self.function_name)?;
        let function = callback.finish(format!("_wake_guard_{name}"), Vec::new(), None)?;
        self.synthesis.borrow_mut().file(ordinal, function)?;
        Ok(Some(IrWakeTest {
            values,
            atoms: Vec::new(),
            program: Vec::new(),
            callback: Some(ordinal),
        }))
    }
}
