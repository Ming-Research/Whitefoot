use std::collections::HashSet;

use crate::semantic::{
    BindingId, CheckedEnumType, CheckedExpression, CheckedIntegerOperation, CheckedLoopId,
    CheckedMatchArm, CheckedStatement, CheckedType,
};
use crate::{
    IrAddressed, IrConstant, IrEnumType, IrIntegerOperation, IrMatchTarget, IrOperation,
    IrTerminator, IrType, LoweringFailure,
};

use super::IrBuilder;
use crate::lowering::TypeLowering;

const NEEDLE_LIMIT: usize = 4;

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};
const U8: IrType = IrType::Integer {
    width: 8,
    signed: false,
};

/// One byte value a recognized walk tests the loaded byte against.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Needle {
    Literal(u8),
    Binding(BindingId),
}

/// The run a recognized walk reads, which is one contiguous extent of `u8`
/// elements [TYPE-9].
///
/// Both admitted shapes start at their own first element and run without a
/// gap: a runtime-capacity `Array<u8>` is the element array of the block its
/// `Box` points at, whose `len` word heads that block
/// (compiler/storage-representation), and a constant-capacity `Array<u8, N>`
/// is the whole of its inline storage, whose length is the type constant and
/// is stored nowhere. A `Slots` or a `Ring` is not admitted: a window is
/// `len` slots beginning at `head` modulo `cap` [WIN-1], so its first logical
/// element is not its base and its extent can wrap.
#[derive(Clone)]
enum WalkedRun {
    /// `b.inner[i]` over a boxed runtime-capacity `Array<u8>`.
    Boxed(crate::semantic::CheckedBufferRoot),
    /// `a[i]` over an inline `Array<u8, N>` a binding names directly.
    Inline(BindingId),
}

/// A loop body in the recognized byte-walk form: exit guard on the
/// induction binding, one guarded `u8` load at the induction binding, a
/// neutral middle whose only observable exits are dominated by equality
/// tests of the loaded byte, and the trailing single-step increment.
struct ByteWalk {
    induction: BindingId,
    bound: BindingId,
    run: WalkedRun,
    needles: Vec<Needle>,
}

/// Recognizes the byte-walk form on the checked statements of one loop
/// body. Only grammar and semantic structure participate: no name, source
/// file, or calling context is consulted.
fn recognize_byte_walk(
    loop_id: CheckedLoopId,
    body: &[CheckedStatement],
    declared_outside: &HashSet<BindingId>,
) -> Option<ByteWalk> {
    let (guard_let, guard_match, load, middle, increment) = match body {
        [guard_let, guard_match, load, middle @ .., increment] => {
            (guard_let, guard_match, load, middle, increment)
        }
        _ => return None,
    };
    let (induction, bound) = recognize_guard(loop_id, guard_let, guard_match, declared_outside)?;
    let (byte, run) = recognize_load(load, induction, declared_outside)?;
    if !recognize_increment(increment, induction) {
        return None;
    }
    let mut needles = Vec::new();
    if !statements_are_neutral(middle, byte, declared_outside, &mut needles) {
        return None;
    }
    if needles.is_empty() || needles.len() > NEEDLE_LIMIT {
        return None;
    }
    Some(ByteWalk {
        induction,
        bound,
        run,
        needles: needles.into_iter().map(|(_, needle)| needle).collect(),
    })
}

/// The exit guard: `let done = ige::<u64>(i, bound)` with `True => break L`,
/// or `let more = ilt::<u64>(i, bound)` with `False => break L`, the other
/// arm empty, both over bindings declared outside the loop.
fn recognize_guard(
    loop_id: CheckedLoopId,
    guard_let: &CheckedStatement,
    guard_match: &CheckedStatement,
    declared_outside: &HashSet<BindingId>,
) -> Option<(BindingId, BindingId)> {
    let CheckedStatement::Let { binding, value, .. } = guard_let else {
        return None;
    };
    let CheckedExpression::IntegerOperation {
        operation,
        operand_type,
        arguments,
        ..
    } = value
    else {
        return None;
    };
    if crate::lowering::lower_type(TypeLowering::EMPTY, *operand_type).ok()? != U64 {
        return None;
    }
    let breaks_on: u32 = match operation {
        CheckedIntegerOperation::GreaterEqual => 1,
        CheckedIntegerOperation::Less => 0,
        _ => return None,
    };
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    let induction = outside_binding(left, declared_outside)?;
    let bound = outside_binding(right, declared_outside)?;
    let CheckedStatement::Match {
        scrutinee: CheckedExpression::Binding {
            binding: scrutinee, ..
        },
        enum_type: CheckedEnumType::Bool,
        arms,
        ..
    } = guard_match
    else {
        return None;
    };
    if scrutinee != binding {
        return None;
    }
    let break_arm = bool_arm(arms, breaks_on)?;
    let other_arm = bool_arm(arms, 1 - breaks_on)?;
    let [CheckedStatement::Break { target, .. }] = break_arm.body.as_slice() else {
        return None;
    };
    if *target != loop_id || !other_arm.body.is_empty() || !other_arm.fallthrough_drops.is_empty() {
        return None;
    }
    Some((induction, bound))
}

/// The probe load: `let b = run[i];` on a `u8` run whose root binding is
/// declared outside the loop, offset exactly the induction binding.
///
/// The two admitted roots are [TYPE-9]'s two `Array` placements: the boxed
/// runtime-capacity form, reached through its cell's content step, and the
/// inline constant-capacity form named by a binding. A path carrying its own
/// subscript is refused, because the storage the walk reads would then depend
/// on an offset this recognizer has not proved loop-invariant.
fn recognize_load(
    load: &CheckedStatement,
    induction: BindingId,
    declared_outside: &HashSet<BindingId>,
) -> Option<(BindingId, WalkedRun)> {
    let CheckedStatement::Let { binding, value, .. } = load else {
        return None;
    };
    let (root, offset) = match value {
        CheckedExpression::BufferIndex { root, offset, .. } => {
            if crate::lowering::lower_type(TypeLowering::EMPTY, root.element_type).ok()? != U8
                || root
                    .path
                    .iter()
                    .any(|step| matches!(step, crate::semantic::CheckedPlaceStep::Subscript(_)))
            {
                return None;
            }
            (WalkedRun::Boxed(root.clone()), offset.as_ref())
        }
        CheckedExpression::ArrayIndex {
            root,
            element_type,
            offset,
            ..
        } => {
            let crate::semantic::CheckedArrayRoot::Binding { binding, fields } = root else {
                return None;
            };
            if !fields.is_empty()
                || crate::lowering::lower_type(TypeLowering::EMPTY, *element_type).ok()? != U8
            {
                return None;
            }
            (WalkedRun::Inline(*binding), offset.as_ref())
        }
        // An inline `Array<u8, N>` named by a binding reaches lowering as the
        // measured storage place its subscript selects. The unified storage
        // path also carries a boxed runtime Array's final element subscript;
        // its preceding path is the buffer root and must itself contain no
        // subscript, preserving the recognizer's loop-invariant-root limit.
        CheckedExpression::ReadStorage { root, .. } => {
            let crate::semantic::CheckedPlaceRoot::Binding(binding) = root.root else {
                return None;
            };
            let (last, prefix) = root.path.split_last()?;
            let crate::semantic::CheckedPlaceStep::Subscript(subscript) = last else {
                return None;
            };
            if crate::lowering::lower_type(TypeLowering::EMPTY, root.ty).ok()? != U8 {
                return None;
            }
            let run = match subscript.base_type {
                CheckedType::Array { .. } if prefix.is_empty() => WalkedRun::Inline(binding),
                CheckedType::Buffer { element }
                    if !prefix.iter().any(|step| {
                        matches!(step, crate::semantic::CheckedPlaceStep::Subscript(_))
                    }) =>
                {
                    WalkedRun::Boxed(crate::semantic::CheckedBufferRoot {
                        binding,
                        path: prefix.to_vec(),
                        element,
                        element_type: root.ty,
                        proof_base: None,
                    })
                }
                _ => return None,
            };
            (run, &subscript.offset)
        }
        _ => return None,
    };
    if !declared_outside.contains(&run_root_binding(&root)) {
        return None;
    }
    let CheckedExpression::Binding {
        binding: offset_binding,
        ..
    } = offset
    else {
        return None;
    };
    if *offset_binding != induction {
        return None;
    }
    Some((*binding, root))
}

/// The binding one walked run is rooted at.
const fn run_root_binding(run: &WalkedRun) -> BindingId {
    match run {
        WalkedRun::Boxed(root) => root.binding,
        WalkedRun::Inline(binding) => *binding,
    }
}

/// The trailing step: `set i = iadd.wrap::<u64>(i, 1_u64)`.
fn recognize_increment(increment: &CheckedStatement, induction: BindingId) -> bool {
    let CheckedStatement::Set { target, value, .. } = increment else {
        return false;
    };
    let crate::semantic::CheckedSetTarget::Place(place) = target else {
        return false;
    };
    if place.binding != induction || !place.fields.is_empty() {
        return false;
    }
    let CheckedExpression::IntegerOperation {
        operation: CheckedIntegerOperation::AddWrap,
        operand_type,
        arguments,
        ..
    } = value
    else {
        return false;
    };
    if crate::lowering::lower_type(TypeLowering::EMPTY, *operand_type) != Ok(U64) {
        return false;
    }
    let [CheckedExpression::Binding { binding, .. }, one] = arguments.as_slice() else {
        return false;
    };
    *binding == induction && integer_literal(one) == Some(1)
}

/// Whether every statement is neutral: for a byte matching no registered
/// needle, execution reaches the increment with no effect. Registered
/// needle tests may guard arbitrary statements on their hit arm.
fn statements_are_neutral(
    statements: &[CheckedStatement],
    byte: BindingId,
    declared_outside: &HashSet<BindingId>,
    needles: &mut Vec<(BindingId, Needle)>,
) -> bool {
    statements
        .iter()
        .all(|statement| statement_is_neutral(statement, byte, declared_outside, needles))
}

fn statement_is_neutral(
    statement: &CheckedStatement,
    byte: BindingId,
    declared_outside: &HashSet<BindingId>,
    needles: &mut Vec<(BindingId, Needle)>,
) -> bool {
    match statement {
        CheckedStatement::Let { binding, value, .. } => {
            if !expression_is_pure(value) {
                return false;
            }
            if let Some(needle) = needle_test(value, byte, declared_outside) {
                needles.push((*binding, needle));
            }
            true
        }
        CheckedStatement::Match {
            scrutinee: CheckedExpression::Binding { binding, .. },
            enum_type: CheckedEnumType::Bool,
            arms,
            ..
        } => {
            let Some(true_arm) = bool_arm(arms, 1) else {
                return false;
            };
            let Some(false_arm) = bool_arm(arms, 0) else {
                return false;
            };
            if needles.iter().any(|(binder, _)| binder == binding) {
                false_arm.fallthrough_drops.is_empty()
                    && statements_are_neutral(&false_arm.body, byte, declared_outside, needles)
            } else {
                true_arm.fallthrough_drops.is_empty()
                    && false_arm.fallthrough_drops.is_empty()
                    && statements_are_neutral(&true_arm.body, byte, declared_outside, needles)
                    && statements_are_neutral(&false_arm.body, byte, declared_outside, needles)
            }
        }
        _ => false,
    }
}

/// Whether the expression is pure and statically total under the frozen
/// whitelist: constants, binding reads, checked integer operations, Boolean
/// operations, enum equality, numeric conversion, and reinterpretation.
fn expression_is_pure(expression: &CheckedExpression) -> bool {
    match expression {
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::Binding { .. } => true,
        CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. }
        | CheckedExpression::EnumEquality { arguments, .. } => {
            arguments.iter().all(expression_is_pure)
        }
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. } => expression_is_pure(value),
        _ => false,
    }
}

/// The needle-registering shape: `ieq::<u8>(b, k)` in either argument
/// order, with `k` a `u8` literal or a `u8` binding declared outside.
fn needle_test(
    expression: &CheckedExpression,
    byte: BindingId,
    declared_outside: &HashSet<BindingId>,
) -> Option<Needle> {
    let CheckedExpression::IntegerOperation {
        operation: CheckedIntegerOperation::Equal,
        operand_type,
        arguments,
        ..
    } = expression
    else {
        return None;
    };
    if crate::lowering::lower_type(TypeLowering::EMPTY, *operand_type).ok()? != U8 {
        return None;
    }
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    let byte_side = |side: &CheckedExpression| matches!(side, CheckedExpression::Binding { binding, .. } if *binding == byte);
    let needle_side = |side: &CheckedExpression| {
        if let Some(bits) = integer_literal(side) {
            return Some(Needle::Literal(u8::try_from(bits).ok()?));
        }
        let binding = outside_binding(side, declared_outside)?;
        Some(Needle::Binding(binding))
    };
    if byte_side(left) {
        needle_side(right)
    } else if byte_side(right) {
        needle_side(left)
    } else {
        None
    }
}

fn integer_literal(expression: &CheckedExpression) -> Option<u64> {
    match expression {
        CheckedExpression::Constant(crate::semantic::CheckedValue::Integer { bits, .. })
        | CheckedExpression::NamedConstant {
            value: crate::semantic::CheckedValue::Integer { bits, .. },
            ..
        } => Some(*bits),
        _ => None,
    }
}

fn outside_binding(
    expression: &CheckedExpression,
    declared_outside: &HashSet<BindingId>,
) -> Option<BindingId> {
    let CheckedExpression::Binding { binding, .. } = expression else {
        return None;
    };
    declared_outside.contains(binding).then_some(*binding)
}

fn bool_arm(arms: &[CheckedMatchArm], tag: u32) -> Option<&CheckedMatchArm> {
    arms.iter().find(|arm| arm.tag == tag)
}

impl IrBuilder<'_> {
    /// Emits the wide-probe fast path at a loop header whose body has the
    /// recognized byte-walk form; any recognition or representation
    /// mismatch falls back to the ordinary lowering with zero change.
    ///
    /// The scalar body's observable work is untouched: the probe skips only
    /// iterations that are provably `i := i + 1`, so effect order and
    /// acceptance are preserved by construction.
    pub(super) fn emit_probe_skip_if_recognized(
        &mut self,
        loop_id: CheckedLoopId,
        body: &[CheckedStatement],
        header: crate::IrBlockId,
        carried_bindings: &[BindingId],
    ) -> Result<(), LoweringFailure> {
        let declared_outside: HashSet<BindingId> = self.bindings.keys().copied().collect();
        let Some(walk) = recognize_byte_walk(loop_id, body, &declared_outside) else {
            return Ok(());
        };
        // The induction binding, the exit bound and every needle binding are
        // read as scalar values here, so an addressed one could hold a value
        // the walk has since written through its address; the run itself is
        // read through its own place and carries no such hazard.
        let scalar_bindings = [walk.induction, walk.bound];
        let needle_bindings = walk.needles.iter().filter_map(|needle| match needle {
            Needle::Binding(binding) => Some(*binding),
            Needle::Literal(_) => None,
        });
        if scalar_bindings
            .iter()
            .copied()
            .chain(needle_bindings)
            .any(|binding| self.addressed_bindings.contains(&binding))
        {
            return Ok(());
        }
        let Some(&index) = self.bindings.get(&walk.induction) else {
            return Ok(());
        };
        let Some(&limit) = self.bindings.get(&walk.bound) else {
            return Ok(());
        };
        if self.value_type(index)? != U64 || self.value_type(limit)? != U64 {
            return Ok(());
        }
        let buffer = match &walk.run {
            // [TYPE-9] the boxed runtime-capacity block, reached through its
            // cell exactly as every other read of it is.
            WalkedRun::Boxed(root) => {
                let address = self.buffer_root(root)?;
                let IrType::Address(IrAddressed::Buffer { element }) = self.value_type(address)?
                else {
                    return Ok(());
                };
                if self.element_type(element)? != U8 {
                    return Ok(());
                }
                address
            }
            // The inline constant-capacity run, whose storage is its own and
            // whose length is the type constant [TYPE-9, WIN-1].
            WalkedRun::Inline(binding) => {
                let Some(&value) = self.bindings.get(binding) else {
                    return Ok(());
                };
                match self.value_type(value)? {
                    IrType::Array { element, .. }
                    | IrType::Address(IrAddressed::Array { element, .. })
                        if self.element_type(element)? == U8 => {}
                    _ => return Ok(()),
                }
                value
            }
        };
        let mut needles = Vec::with_capacity(walk.needles.len());
        for needle in &walk.needles {
            let value = match needle {
                Needle::Literal(bits) => self.define(
                    U8,
                    IrOperation::Constant(IrConstant::Integer {
                        ty: U8,
                        bits: u64::from(*bits),
                    }),
                )?,
                Needle::Binding(binding) => {
                    let Some(&value) = self.bindings.get(binding) else {
                        return Ok(());
                    };
                    if self.value_type(value)? != U8 {
                        return Ok(());
                    }
                    value
                }
            };
            needles.push(value);
        }
        let skip = self.define(
            U64,
            IrOperation::BufferProbeSkip {
                buffer,
                index,
                limit,
                needles,
            },
        )?;
        let zero = self.define(
            U64,
            IrOperation::Constant(IrConstant::Integer { ty: U64, bits: 0 }),
        )?;
        let advanced = self.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Greater,
                operand_type: U64,
                arguments: vec![skip, zero],
            },
        )?;
        let (advance, _) = self.new_block(&[])?;
        let (scalar, _) = self.new_block(&[])?;
        self.terminate(IrTerminator::Match {
            scrutinee: advanced,
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: advance,
                },
                IrMatchTarget {
                    tag: 0,
                    block: scalar,
                },
            ],
        })?;
        self.current = Some(advance);
        let next = self.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::AddWrap,
                operand_type: U64,
                arguments: vec![index, skip],
            },
        )?;
        if self.bindings.insert(walk.induction, next) != Some(index) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let arguments = self.binding_values(carried_bindings)?;
        if self.bindings.insert(walk.induction, index) != Some(next) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.terminate(IrTerminator::Jump {
            target: header,
            arguments,
            drops: Vec::new(),
        })?;
        self.current = Some(scalar);
        Ok(())
    }
}
