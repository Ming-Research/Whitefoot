//! [SHARE-3] The wake test a guard's watch registers for one object target
//! (compiler/waiting-contexts/state-locks): a condition over that object's
//! state which the guard implies, so a write that leaves it false cannot
//! have made the guard true and need not wake the watch.
//!
//! The derivation splits the guard over `and`, `or` and `not`, pushing each
//! `not` down to the comparisons, and keeps a comparison of a scalar field of
//! the object (an integer, a `Bool` or a tag-only enum, reached through
//! fields alone) against a constant, a local the registration captures, or
//! another such field of the same object. A helper whose checked body is a
//! single return is expanded into that derivation with scoped arguments.
//! Every other part, including a comparison over another target, becomes
//! `true`, which only weakens the atom test. If that leaves a helper guard
//! without a test, `callback` audits its complete call closure and lowers the
//! original guard over the live state and scalar captures. A guard reads no
//! state it may change and a local stays as it was
//! while the frame is parked, so a test false on the state a write leaves
//! implies the guard false there.

use super::{IrBuilder, lower_type};
mod callback;
use crate::semantic::{
    BindingId, CheckedBodyDisposition, CheckedBooleanOperation, CheckedExpression,
    CheckedIntegerOperation, CheckedMode, CheckedPlaceStep, CheckedStatement, CheckedValue,
    FunctionId,
};
use crate::{
    IrConstant, IrOperation, IrType, IrValueId, IrWakeAtom, IrWakeCode, IrWakeComparison,
    IrWakeOperand, IrWakeTest, LoweringFailure,
};

/// `WF_WAKE_ATOMS` in `completion/bridge.h`; a test of at most this many
/// atoms has at most `WF_WAKE_PROGRAM` postfix codes.
const ATOMS: usize = 4;

/// A guard's condition over one object, before its scalars are addressed.
#[derive(Debug)]
enum Condition<'guard> {
    True,
    Atom(Atom<'guard>),
    And(Vec<Condition<'guard>>),
    Or(Vec<Condition<'guard>>),
}

#[derive(Debug)]
struct Atom<'guard> {
    /// The fields from the object's state to the scalar.
    field: Vec<u32>,
    comparison: IrWakeComparison,
    operand: Operand<'guard>,
}

#[derive(Debug)]
enum Operand<'guard> {
    /// A constant or a captured local, evaluated where the watch registers.
    Value(&'guard CheckedExpression),
    /// The `false` a bare `Bool` field read is compared against.
    False,
    /// Another scalar of the same object.
    Field(Vec<u32>),
}

/// One side of a comparison, as the derivation for one object sees it.
enum Side<'guard> {
    Field(Vec<u32>, IrType),
    Value(&'guard CheckedExpression, IrType),
    Other,
}

/// Binding IDs are local to a function. Each helper's parameter maps to an
/// expression in the caller's scope, never to a bare binding ID in this one.
#[derive(Clone, Copy)]
struct ScopedExpression<'guard> {
    expression: &'guard CheckedExpression,
    scope: usize,
}

struct Argument<'guard> {
    binding: BindingId,
    mode: CheckedMode,
    actual: ScopedExpression<'guard>,
}

/// Scope zero is the guard's own function; every other scope holds exactly
/// one expanded helper's formal-to-actual substitution.
struct Scope<'guard> {
    arguments: Vec<Argument<'guard>>,
}

fn argument<'scope, 'guard>(
    scopes: &'scope [Scope<'guard>],
    scope: usize,
    binding: BindingId,
) -> Option<&'scope Argument<'guard>> {
    scopes
        .get(scope)?
        .arguments
        .iter()
        .find(|argument| argument.binding == binding)
}

/// Resolve only a forwarded reference or a field-only borrow. In particular,
/// another target's referent must not become a captured registration value.
fn reference_path(
    expression: ScopedExpression<'_>,
    binding: BindingId,
    scopes: &[Scope<'_>],
) -> Option<Vec<u32>> {
    match expression.expression {
        CheckedExpression::Binding { binding: read, .. } => {
            reference_binding(*read, expression.scope, binding, scopes)
        }
        CheckedExpression::BorrowAddressed { root, .. } => {
            let mut path = reference_binding(root.binding()?, expression.scope, binding, scopes)?;
            for step in &root.path {
                let CheckedPlaceStep::Field(field) = step else {
                    return None;
                };
                path.push(*field);
            }
            Some(path)
        }
        _ => None,
    }
}

fn reference_binding(
    read: BindingId,
    scope: usize,
    binding: BindingId,
    scopes: &[Scope<'_>],
) -> Option<Vec<u32>> {
    if scope == 0 {
        return (read == binding).then(Vec::new);
    }
    let argument = argument(scopes, scope, read)?;
    if argument.mode != CheckedMode::Reference {
        return None;
    }
    reference_path(argument.actual, binding, scopes)
}

const fn negated(comparison: IrWakeComparison) -> IrWakeComparison {
    match comparison {
        IrWakeComparison::Equal => IrWakeComparison::NotEqual,
        IrWakeComparison::NotEqual => IrWakeComparison::Equal,
        IrWakeComparison::Less => IrWakeComparison::GreaterEqual,
        IrWakeComparison::LessEqual => IrWakeComparison::Greater,
        IrWakeComparison::Greater => IrWakeComparison::LessEqual,
        IrWakeComparison::GreaterEqual => IrWakeComparison::Less,
    }
}

/// The comparison with its two sides exchanged.
const fn mirrored(comparison: IrWakeComparison) -> IrWakeComparison {
    match comparison {
        IrWakeComparison::Equal => IrWakeComparison::Equal,
        IrWakeComparison::NotEqual => IrWakeComparison::NotEqual,
        IrWakeComparison::Less => IrWakeComparison::Greater,
        IrWakeComparison::LessEqual => IrWakeComparison::GreaterEqual,
        IrWakeComparison::Greater => IrWakeComparison::Less,
        IrWakeComparison::GreaterEqual => IrWakeComparison::LessEqual,
    }
}

const fn comparison(operation: CheckedIntegerOperation) -> Option<IrWakeComparison> {
    Some(match operation {
        CheckedIntegerOperation::Equal => IrWakeComparison::Equal,
        CheckedIntegerOperation::NotEqual => IrWakeComparison::NotEqual,
        CheckedIntegerOperation::Less => IrWakeComparison::Less,
        CheckedIntegerOperation::LessEqual => IrWakeComparison::LessEqual,
        CheckedIntegerOperation::Greater => IrWakeComparison::Greater,
        CheckedIntegerOperation::GreaterEqual => IrWakeComparison::GreaterEqual,
        _ => return None,
    })
}

/// The fields a read `p^.f.g` of the target `binding` selects, or `None`
/// for any other expression.
fn field_path(
    expression: ScopedExpression<'_>,
    binding: BindingId,
    scopes: &[Scope<'_>],
) -> Option<Vec<u32>> {
    match expression.expression {
        CheckedExpression::Binding {
            binding: read,
            consume_root: false,
            ..
        } if expression.scope != 0 =>
        {
            let argument = argument(scopes, expression.scope, *read)?;
            if argument.mode != CheckedMode::Own {
                return None;
            }
            field_path(argument.actual, binding, scopes)
        }
        CheckedExpression::DerefAddressed { binding: read, .. } => {
            reference_binding(*read, expression.scope, binding, scopes)
        }
        CheckedExpression::ProjectValue { value, field, .. } => {
            let mut path = field_path(
                ScopedExpression {
                    expression: value,
                    ..expression
                },
                binding,
                scopes,
            )?;
            path.push(*field);
            Some(path)
        }
        _ => None,
    }
}

fn atoms(condition: &Condition<'_>) -> usize {
    match condition {
        Condition::True => 0,
        Condition::Atom(_) => 1,
        Condition::And(parts) | Condition::Or(parts) => parts.iter().map(atoms).sum(),
    }
}

/// Removes `true` from conjunctions, makes a disjunction with a `true` part
/// `true`, flattens nested ones and unwraps single parts.
fn simplified(condition: Condition<'_>) -> Condition<'_> {
    match condition {
        Condition::And(parts) => {
            let mut kept = Vec::new();
            for part in parts.into_iter().map(simplified) {
                match part {
                    Condition::True => {}
                    Condition::And(inner) => kept.extend(inner),
                    part => kept.push(part),
                }
            }
            match kept.len() {
                0 => Condition::True,
                1 => kept.remove(0),
                _ => Condition::And(kept),
            }
        }
        Condition::Or(parts) => {
            let mut kept = Vec::new();
            for part in parts.into_iter().map(simplified) {
                match part {
                    Condition::True => return Condition::True,
                    Condition::Or(inner) => kept.extend(inner),
                    part => kept.push(part),
                }
            }
            match kept.len() {
                0 => Condition::True,
                1 => kept.remove(0),
                _ => Condition::Or(kept),
            }
        }
        condition => condition,
    }
}

/// A simplified condition implied by `condition` with at most `budget`
/// atoms: a conjunction keeps its parts in written order while they fit, and
/// a disjunction that does not fit becomes `true`.
fn weakened(condition: Condition<'_>, budget: usize) -> Condition<'_> {
    if atoms(&condition) <= budget {
        return condition;
    }
    match condition {
        Condition::And(parts) => {
            let mut left = budget;
            let mut kept = Vec::new();
            for part in parts {
                let part = weakened(part, left);
                left -= atoms(&part);
                kept.push(part);
            }
            simplified(Condition::And(kept))
        }
        _ => Condition::True,
    }
}

impl<'program> IrBuilder<'program> {
    /// The scalar types an atom compares, which the runtime loads by width.
    fn wake_scalar(&self, ty: IrType) -> bool {
        match ty {
            IrType::Integer { width, .. } => matches!(width, 8 | 16 | 32 | 64),
            IrType::Bool => true,
            IrType::Nominal(nominal) => self
                .nominals
                .get(nominal.index())
                .is_some_and(crate::IrNominal::is_tag_only_enum),
            _ => false,
        }
    }

    /// Whether reading the local `binding` yields a scalar of type `ty`
    /// itself, not a reference holder's address.
    fn captured_scalar(&self, binding: BindingId, ty: IrType) -> Result<bool, LoweringFailure> {
        let Some(stored) = self.bindings.get(&binding).copied() else {
            return Ok(false);
        };
        let held = self.value_type(stored)?;
        Ok(if self.addressed_bindings.contains(&binding) {
            matches!(held, IrType::Address(referent) if referent.ty() == ty)
        } else {
            held == ty
        })
    }

    fn wake_side<'guard>(
        &self,
        expression: ScopedExpression<'guard>,
        binding: BindingId,
        targets: &[BindingId],
        scopes: &[Scope<'guard>],
    ) -> Result<Side<'guard>, LoweringFailure> {
        if expression.scope != 0
            && let CheckedExpression::Binding {
                binding: read,
                consume_root: false,
                ..
            } = expression.expression
        {
            let Some(argument) = argument(scopes, expression.scope, *read) else {
                return Ok(Side::Other);
            };
            return if argument.mode == CheckedMode::Own {
                self.wake_side(argument.actual, binding, targets, scopes)
            } else {
                Ok(Side::Other)
            };
        }
        let Ok(ty) = lower_type(self.erasure, expression.expression.ty()) else {
            return Ok(Side::Other);
        };
        if !self.wake_scalar(ty) {
            return Ok(Side::Other);
        }
        if let Some(path) = field_path(expression, binding, scopes) {
            return Ok(Side::Field(path, ty));
        }
        let captured = match expression.expression {
            CheckedExpression::Constant(value) | CheckedExpression::NamedConstant { value, .. } => {
                matches!(value, CheckedValue::Integer { .. } | CheckedValue::Bool(_))
            }
            CheckedExpression::ConstructEnum { fields, .. } => fields.is_empty(),
            CheckedExpression::Binding {
                binding: local,
                consume_root: false,
                ..
            } if expression.scope == 0 && !targets.contains(local) => {
                self.captured_scalar(*local, ty)?
            }
            _ => false,
        };
        Ok(if captured {
            Side::Value(expression.expression, ty)
        } else {
            Side::Other
        })
    }

    /// One comparison of `left` and `right` under `positive`.
    fn wake_comparison<'guard>(
        &self,
        compared: IrWakeComparison,
        sides: [ScopedExpression<'guard>; 2],
        positive: bool,
        binding: BindingId,
        targets: &[BindingId],
        scopes: &[Scope<'guard>],
    ) -> Result<Condition<'guard>, LoweringFailure> {
        let compared = if positive {
            compared
        } else {
            negated(compared)
        };
        let [left, right] = sides;
        let left = self.wake_side(left, binding, targets, scopes)?;
        let right = self.wake_side(right, binding, targets, scopes)?;
        Ok(match (left, right) {
            (Side::Field(field, ty), Side::Value(value, operand)) if ty == operand => {
                Condition::Atom(Atom {
                    field,
                    comparison: compared,
                    operand: Operand::Value(value),
                })
            }
            (Side::Value(value, operand), Side::Field(field, ty)) if ty == operand => {
                Condition::Atom(Atom {
                    field,
                    comparison: mirrored(compared),
                    operand: Operand::Value(value),
                })
            }
            (Side::Field(field, ty), Side::Field(other, other_ty)) if ty == other_ty => {
                Condition::Atom(Atom {
                    field,
                    comparison: compared,
                    operand: Operand::Field(other),
                })
            }
            _ => Condition::True,
        })
    }

    /// The condition over `binding`'s object that `guard`, or its negation
    /// when `positive` is false, implies.
    fn wake_condition<'guard>(
        &self,
        guard: ScopedExpression<'guard>,
        positive: bool,
        binding: BindingId,
        targets: &[BindingId],
        scopes: &mut Vec<Scope<'guard>>,
        active: &mut Vec<FunctionId>,
    ) -> Result<Condition<'guard>, LoweringFailure>
    where
        'program: 'guard,
    {
        match guard.expression {
            CheckedExpression::Binding {
                binding: read,
                consume_root: false,
                ..
            } if guard.scope != 0 => {
                let Some(argument) = argument(scopes, guard.scope, *read) else {
                    return Ok(Condition::True);
                };
                if argument.mode != CheckedMode::Own {
                    return Ok(Condition::True);
                }
                self.wake_condition(argument.actual, positive, binding, targets, scopes, active)
            }
            CheckedExpression::UserCall {
                function,
                arguments,
                ..
            } => {
                if active.contains(function) {
                    return Ok(Condition::True);
                }
                let functions = self.checked_functions;
                let Some(callee) = functions.get(function.0 as usize) else {
                    return Ok(Condition::True);
                };
                if callee.waiting.waits
                    || !matches!(callee.body_disposition, CheckedBodyDisposition::Inhabited)
                    || callee.parameters.len() != arguments.len()
                {
                    return Ok(Condition::True);
                }
                let Some([CheckedStatement::Return { value, drops, .. }]) = callee.body.as_deref()
                else {
                    return Ok(Condition::True);
                };
                if !drops.is_empty() {
                    return Ok(Condition::True);
                }
                let scope = scopes.len();
                scopes.push(Scope {
                    arguments: callee
                        .parameters
                        .iter()
                        .zip(arguments)
                        .map(|(formal, actual)| Argument {
                            binding: formal.binding,
                            mode: formal.mode,
                            actual: ScopedExpression {
                                expression: actual,
                                scope: guard.scope,
                            },
                        })
                        .collect(),
                });
                active.push(*function);
                let condition = self.wake_condition(
                    ScopedExpression {
                        expression: value,
                        scope,
                    },
                    positive,
                    binding,
                    targets,
                    scopes,
                    active,
                );
                active.pop();
                scopes.pop();
                condition
            }
            CheckedExpression::BooleanOperation {
                operation: CheckedBooleanOperation::Not,
                arguments,
                ..
            } => match arguments.as_slice() {
                [inner] => self.wake_condition(
                    ScopedExpression {
                        expression: inner,
                        ..guard
                    },
                    !positive,
                    binding,
                    targets,
                    scopes,
                    active,
                ),
                _ => Ok(Condition::True),
            },
            CheckedExpression::BooleanOperation {
                operation:
                    operation @ (CheckedBooleanOperation::And | CheckedBooleanOperation::Or),
                arguments,
                ..
            } => {
                let parts = arguments
                    .iter()
                    .map(|argument| {
                        self.wake_condition(
                            ScopedExpression {
                                expression: argument,
                                ..guard
                            },
                            positive,
                            binding,
                            targets,
                            scopes,
                            active,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let conjunction = matches!(operation, CheckedBooleanOperation::And) == positive;
                Ok(if conjunction {
                    Condition::And(parts)
                } else {
                    Condition::Or(parts)
                })
            }
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                ..
            } => match (comparison(*operation), arguments.as_slice()) {
                (Some(compared), [left, right]) => self.wake_comparison(
                    compared,
                    [
                        ScopedExpression {
                            expression: left,
                            ..guard
                        },
                        ScopedExpression {
                            expression: right,
                            ..guard
                        },
                    ],
                    positive,
                    binding,
                    targets,
                    scopes,
                ),
                _ => Ok(Condition::True),
            },
            CheckedExpression::ValueEquality {
                equal, arguments, ..
            } => match arguments.as_slice() {
                [left, right] => {
                    let compared = if *equal {
                        IrWakeComparison::Equal
                    } else {
                        IrWakeComparison::NotEqual
                    };
                    self.wake_comparison(
                        compared,
                        [
                            ScopedExpression {
                                expression: left,
                                ..guard
                            },
                            ScopedExpression {
                                expression: right,
                                ..guard
                            },
                        ],
                        positive,
                        binding,
                        targets,
                        scopes,
                    )
                }
                _ => Ok(Condition::True),
            },
            _ => Ok(match self.wake_side(guard, binding, targets, scopes)? {
                Side::Field(field, IrType::Bool) => Condition::Atom(Atom {
                    field,
                    comparison: if positive {
                        IrWakeComparison::NotEqual
                    } else {
                        IrWakeComparison::Equal
                    },
                    operand: Operand::False,
                }),
                _ => Condition::True,
            }),
        }
    }

    /// The wake test of `guard` for the object target `binding`, whose state
    /// is at `state`, among the statement's `targets`; `None` when the guard
    /// implies no condition the runtime can check on that state.
    pub(super) fn wake_test(
        &mut self,
        guard: &CheckedExpression,
        binding: BindingId,
        state: IrValueId,
        targets: &[BindingId],
    ) -> Result<Option<IrWakeTest>, LoweringFailure> {
        let condition = self.wake_condition(
            ScopedExpression {
                expression: guard,
                scope: 0,
            },
            true,
            binding,
            targets,
            &mut vec![Scope {
                arguments: Vec::new(),
            }],
            &mut Vec::new(),
        )?;
        let condition = weakened(simplified(condition), ATOMS);
        if matches!(condition, Condition::True) {
            return self.wake_callback(guard, binding, state, targets);
        }
        let mut test = IrWakeTest {
            values: vec![state],
            atoms: Vec::new(),
            program: Vec::new(),
            callback: None,
        };
        self.wake_program(&condition, state, &mut test)?;
        Ok(Some(test))
    }

    /// Appends `condition`'s atoms, values and postfix codes to `test`.
    fn wake_program(
        &mut self,
        condition: &Condition<'_>,
        state: IrValueId,
        test: &mut IrWakeTest,
    ) -> Result<(), LoweringFailure> {
        match condition {
            Condition::True => Err(LoweringFailure::InvalidCheckedProgram),
            Condition::Atom(atom) => {
                let field = self.wake_field(state, &atom.field, test)?;
                let operand = match &atom.operand {
                    Operand::Field(other) => {
                        IrWakeOperand::Field(self.wake_field(state, other, test)?)
                    }
                    Operand::Value(value) => {
                        let value = self.expression(value)?;
                        test.values.push(value);
                        IrWakeOperand::Value(test.values.len() - 1)
                    }
                    Operand::False => {
                        let value = self
                            .define(IrType::Bool, IrOperation::Constant(IrConstant::Bool(false)))?;
                        test.values.push(value);
                        IrWakeOperand::Value(test.values.len() - 1)
                    }
                };
                let index = u8::try_from(test.atoms.len())
                    .map_err(|_| LoweringFailure::CounterOverflow)?;
                test.atoms.push(IrWakeAtom {
                    field,
                    comparison: atom.comparison,
                    operand,
                });
                test.program.push(IrWakeCode::Atom(index));
                Ok(())
            }
            Condition::And(parts) | Condition::Or(parts) => {
                let code = if matches!(condition, Condition::And(_)) {
                    IrWakeCode::And
                } else {
                    IrWakeCode::Or
                };
                for (index, part) in parts.iter().enumerate() {
                    self.wake_program(part, state, test)?;
                    if index > 0 {
                        test.program.push(code);
                    }
                }
                Ok(())
            }
        }
    }

    /// The address of the scalar `path` selects in the state, as a value of
    /// `test`.
    fn wake_field(
        &mut self,
        state: IrValueId,
        path: &[u32],
        test: &mut IrWakeTest,
    ) -> Result<usize, LoweringFailure> {
        let steps = path
            .iter()
            .copied()
            .map(CheckedPlaceStep::Field)
            .collect::<Vec<_>>();
        let address = self.project_address_path(state, &steps)?;
        test.values.push(address);
        Ok(test.values.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    //! Weakening the conditions reached by expanding helpers, whose bodies
    //! may together contribute more atoms than the written guard [GRAM-5].

    use super::{Atom, Condition, Operand, atoms, simplified, weakened};
    use crate::IrWakeComparison;

    fn atom(field: u32) -> Condition<'static> {
        Condition::Atom(Atom {
            field: vec![field],
            comparison: IrWakeComparison::NotEqual,
            operand: Operand::False,
        })
    }

    fn fields(condition: &Condition<'_>) -> Vec<u32> {
        match condition {
            Condition::True => Vec::new(),
            Condition::Atom(atom) => atom.field.clone(),
            Condition::And(parts) | Condition::Or(parts) => parts.iter().flat_map(fields).collect(),
        }
    }

    #[test]
    fn a_conjunction_keeps_its_first_four_atoms() {
        let five = Condition::And((0..5).map(atom).collect());
        let kept = weakened(simplified(five), 4);
        assert!(matches!(kept, Condition::And(_)), "{kept:?}");
        assert_eq!(fields(&kept), [0, 1, 2, 3]);
    }

    #[test]
    fn an_oversized_disjunction_implies_nothing() {
        let five = Condition::Or((0..5).map(atom).collect());
        assert!(matches!(weakened(simplified(five), 4), Condition::True));
        // Inside a conjunction it is dropped, and the fitting part stays.
        let mixed = Condition::And(vec![atom(9), Condition::Or((0..5).map(atom).collect())]);
        assert_eq!(fields(&weakened(simplified(mixed), 4)), [9]);
    }

    #[test]
    fn true_parts_simplify_away_and_nested_forms_flatten() {
        let nested = Condition::And(vec![
            Condition::True,
            Condition::And(vec![atom(0), atom(1)]),
            Condition::Or(vec![atom(2), Condition::True]),
        ]);
        let flat = simplified(nested);
        assert!(matches!(&flat, Condition::And(parts) if parts.len() == 2), "{flat:?}");
        assert_eq!(atoms(&flat), 2);
        assert!(matches!(
            simplified(Condition::Or(vec![Condition::True, atom(0)])),
            Condition::True
        ));
        assert!(matches!(simplified(Condition::And(vec![atom(3)])), Condition::Atom(_)));
    }
}
