//! A closed take-back/exchange/drain permutation can hand off owners directly
//! in forward order. Recognition is over checked identities, never names of
//! source helpers or element representations. Unmatched bodies lower normally.

use crate::semantic::{
    BindingId, CheckedContainerRoot, CheckedEffectStep, CheckedEnumType, CheckedExpression,
    CheckedFunction, CheckedIntegerOperation, CheckedLoopId, CheckedMeasure, CheckedMode,
    CheckedPlaceStep, CheckedStatement, CheckedType, CheckedValue,
};
use crate::{
    IrConstant, IrEnumType, IrIntegerOperation, IrMatchTarget, IrMeasure, IrOperation,
    IrTargetDomainObligation, IrTerminator, IrType, LoweringFailure,
};

use super::IrBuilder;
use crate::lowering::{TypeLowering, lower_type};

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

struct Consumption<'a> {
    window: &'a CheckedContainerRoot,
    retained: &'a CheckedExpression,
    value: BindingId,
    element: CheckedType,
    first_call: &'a CheckedExpression,
    last_call: &'a CheckedExpression,
}

fn runtime(statements: &[CheckedStatement]) -> Vec<&CheckedStatement> {
    statements
        .iter()
        .filter(|s| !matches!(s, CheckedStatement::Proof(_)))
        .collect()
}

fn binding(expression: &CheckedExpression) -> Option<BindingId> {
    match expression {
        CheckedExpression::Binding {
            binding,
            consume_root: false,
            ..
        } => Some(*binding),
        _ => None,
    }
}

fn integer(expression: &CheckedExpression, expected: u64) -> bool {
    matches!(expression, CheckedExpression::Constant(CheckedValue::Integer { bits, .. })
        if *bits == expected && lower_type(TypeLowering::EMPTY, expression.ty()).ok() == Some(U64))
}

fn binary(
    expression: &CheckedExpression,
    expected: CheckedIntegerOperation,
) -> Option<(&CheckedExpression, &CheckedExpression)> {
    let CheckedExpression::IntegerOperation {
        operation,
        operand_type,
        arguments,
        ..
    } = expression
    else {
        return None;
    };
    if *operation != expected || lower_type(TypeLowering::EMPTY, *operand_type).ok()? != U64 {
        return None;
    }
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    Some((left, right))
}

fn let_value(statement: &CheckedStatement) -> Option<(BindingId, &CheckedExpression)> {
    match statement {
        CheckedStatement::Let { binding, value, .. } => Some((*binding, value)),
        _ => None,
    }
}

fn evaluated(statement: &CheckedStatement) -> Option<&CheckedExpression> {
    match statement {
        CheckedStatement::Evaluate { value, .. } => Some(value),
        _ => None,
    }
}

fn length(expression: &CheckedExpression) -> Option<&CheckedContainerRoot> {
    match expression {
        CheckedExpression::ContainerMeasure {
            measure: CheckedMeasure::Length,
            root,
        } => Some(root),
        _ => None,
    }
}

fn borrowed(expression: &CheckedExpression) -> Option<&CheckedContainerRoot> {
    match expression {
        CheckedExpression::BorrowAddressed { root, .. } => Some(root),
        _ => None,
    }
}

/// Only the bodyless compiler-owned PRE-1 identity can supply an operation;
/// a same-spelled user helper is not an operation match.
fn row<'a>(
    expression: &'a CheckedExpression,
    functions: &[CheckedFunction],
    name: &str,
) -> Option<&'a [CheckedExpression]> {
    let CheckedExpression::UserCall {
        function,
        arguments,
        tail_transfer: false,
        ..
    } = expression
    else {
        return None;
    };
    let callee = functions.get(function.0 as usize)?;
    (callee.body.is_none()
        && super::prelude::compiler_owned_row(&callee.name)
        && callee.name == name)
        .then_some(arguments.as_slice())
}

fn take<'a>(
    statement: &'a CheckedStatement,
    window: &CheckedContainerRoot,
    functions: &[CheckedFunction],
) -> Option<(BindingId, &'a CheckedExpression)> {
    let (value, expression) = let_value(statement)?;
    let [argument] = row(expression, functions, "take_back")? else {
        return None;
    };
    (borrowed(argument)? == window).then_some((value, expression))
}

fn exchanged(
    statement: &CheckedStatement,
    window: &CheckedContainerRoot,
    left: BindingId,
    value: BindingId,
    functions: &[CheckedFunction],
) -> bool {
    let Some(expression) = evaluated(statement) else {
        return false;
    };
    let Some([first, second]) = row(expression, functions, "swap") else {
        return false;
    };
    let Some(first) = borrowed(first) else {
        return false;
    };
    let Some(second) = borrowed(second) else {
        return false;
    };
    let Some((CheckedPlaceStep::Subscript(index), prefix)) = first.path.split_last() else {
        return false;
    };
    first.root == window.root
        && prefix == window.path
        && index.base_type == window.ty
        && binding(&index.offset) == Some(left)
        && second.binding() == Some(value)
        && second.path.is_empty()
}

/// This boundary's complete write to W is the existing EFF-5 exclusion at
/// every caller. Only other incoming reference roots are admitted as callback
/// environments; no function-wide may-origin inventory supplies authority.
fn isolated_window(function: &CheckedFunction, window: &CheckedContainerRoot) -> bool {
    if !matches!(window.ty, CheckedType::Window { .. })
        || window
            .path
            .iter()
            .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
    {
        return false;
    }
    let Some(parameter) = function.parameters.iter().find(|parameter| {
        Some(parameter.binding) == window.binding() && parameter.mode == CheckedMode::Reference
    }) else {
        return false;
    };
    function.declared_state_writes.iter().any(|write| {
        write.root == parameter.declaration
            && write.steps.len() <= window.path.len()
            && write
                .steps
                .iter()
                .zip(&window.path)
                .all(|(effect, place)| match (effect, place) {
                    (CheckedEffectStep::Field(a), CheckedPlaceStep::Field(b)) => a == b,
                    (CheckedEffectStep::Deref, CheckedPlaceStep::BoxReferent(_)) => true,
                    _ => false,
                })
    })
}

fn same_consumers(
    first: &CheckedExpression,
    second: &CheckedExpression,
    first_value: BindingId,
    second_value: BindingId,
    function: &CheckedFunction,
    window: &CheckedContainerRoot,
    functions: &[CheckedFunction],
) -> bool {
    let CheckedExpression::UserCall {
        function: first_id,
        arguments: first_args,
        tail_transfer: false,
        result_borrow: None,
        ..
    } = first
    else {
        return false;
    };
    let CheckedExpression::UserCall {
        function: second_id,
        arguments: second_args,
        tail_transfer: false,
        result_borrow: None,
        ..
    } = second
    else {
        return false;
    };
    let Some(callee) = functions.get(first_id.0 as usize) else {
        return false;
    };
    if first_id != second_id
        || first_args.len() != second_args.len()
        || first_args.len() != callee.parameters.len()
        || callee.result != CheckedType::Unit
        || callee.result_mode != CheckedMode::Own
    {
        return false;
    }
    let mut owners = 0;
    for ((first, second), formal) in first_args.iter().zip(second_args).zip(&callee.parameters) {
        let (
            CheckedExpression::Binding {
                binding: a,
                ty: a_type,
                consume_root: a_consume,
                ..
            },
            CheckedExpression::Binding {
                binding: b,
                ty: b_type,
                consume_root: b_consume,
                ..
            },
        ) = (first, second)
        else {
            return false;
        };
        if a_type != b_type || a_consume != b_consume {
            return false;
        }
        if *a == first_value && *b == second_value && formal.mode == CheckedMode::Own {
            owners += 1;
            continue;
        }
        if a != b || *a_consume || Some(*a) == window.binding() {
            return false;
        }
        let Some(incoming) = function.parameters.iter().find(|p| p.binding == *a) else {
            return false;
        };
        if incoming.mode != formal.mode
            || !matches!(incoming.mode, CheckedMode::Own | CheckedMode::Reference)
        {
            return false;
        }
    }
    owners == 1
}

fn drain_guard(
    statement: &CheckedStatement,
    condition: &CheckedExpression,
    loop_id: CheckedLoopId,
    window: &CheckedContainerRoot,
    retained: BindingId,
) -> bool {
    let Some((left, right)) = binary(condition, CheckedIntegerOperation::LessEqual) else {
        return false;
    };
    if length(left) != Some(window) || binding(right) != Some(retained) {
        return false;
    }
    let CheckedStatement::Match {
        enum_type: CheckedEnumType::Bool,
        arms,
        ..
    } = statement
    else {
        return false;
    };
    if arms.len() != 2 {
        return false;
    }
    for arm in arms {
        if !arm.binders.is_empty() || !arm.covered.is_empty() || !arm.fallthrough_drops.is_empty() {
            return false;
        }
        let body = runtime(&arm.body);
        match arm.tag {
            0 if body.is_empty() => (),
            1 if matches!(body.as_slice(), [CheckedStatement::Break { target, drops, .. }]
                if *target == loop_id && drops.is_empty()) => {}
            _ => return false,
        }
    }
    true
}

fn recognize<'a>(
    function: &'a CheckedFunction,
    functions: &[CheckedFunction],
) -> Option<Consumption<'a>> {
    // A marked waiting callback is a context start, not an ordinary call.
    // Keep statement lowering's argument capture and activation-exit join.
    if !function.waiting.context_starts.is_empty()
        || function.result != CheckedType::Unit
        || function.result_mode != CheckedMode::Own
    {
        return None;
    }
    let body = runtime(function.body.as_deref()?);
    let [count, removed, half, first_loop, last_loop, finish] = body.as_slice() else {
        return None;
    };
    if !matches!(finish, CheckedStatement::Return { value: CheckedExpression::Constant(CheckedValue::Unit), drops, .. } if drops.is_empty())
    {
        return None;
    }
    let (count, count_expression) = let_value(count)?;
    let window = length(count_expression)?;
    if !isolated_window(function, window) {
        return None;
    }
    let (removed, difference) = let_value(removed)?;
    let (entry, retained) = binary(difference, CheckedIntegerOperation::SubtractExact)?;
    let retained_binding = binding(retained)?;
    if binding(entry) != Some(count)
        || !function.parameters.iter().any(|parameter| {
            parameter.binding == retained_binding
                && parameter.mode == CheckedMode::Own
                && lower_type(TypeLowering::EMPTY, parameter.ty).ok() == Some(U64)
        })
    {
        return None;
    }
    let (half, division) = let_value(half)?;
    let (dividend, divisor) = binary(division, CheckedIntegerOperation::DivideExact)?;
    if binding(dividend) != Some(removed) || !integer(divisor, 2) {
        return None;
    }
    let CheckedStatement::CountedRange {
        binder,
        lower,
        upper,
        body,
        backedge_drops,
        ..
    } = first_loop
    else {
        return None;
    };
    if !integer(lower, 0) || binding(upper) != Some(half) || !backedge_drops.is_empty() {
        return None;
    }
    let body = runtime(body);
    let [left, first_take, exchange, first_call] = body.as_slice() else {
        return None;
    };
    let (left, sum) = let_value(left)?;
    let (base, offset) = binary(sum, CheckedIntegerOperation::AddExact)?;
    if binding(base) != Some(retained_binding) || binding(offset) != Some(*binder) {
        return None;
    }
    let (first_value, first_take) = take(first_take, window, functions)?;
    if !exchanged(exchange, window, left, first_value, functions) {
        return None;
    }
    let first_call = evaluated(first_call)?;
    let CheckedStatement::Loop {
        id,
        body,
        backedge_drops,
        ..
    } = last_loop
    else {
        return None;
    };
    if !backedge_drops.is_empty() {
        return None;
    }
    let body = runtime(body);
    let (guard, condition, last_take, last_call) = match body.as_slice() {
        [
            guard @ CheckedStatement::Match { scrutinee, .. },
            take,
            call,
        ] => (*guard, scrutinee, *take, *call),
        [
            condition,
            guard @ CheckedStatement::Match { scrutinee, .. },
            take,
            call,
        ] => {
            let (guard_binding, condition) = let_value(condition)?;
            if binding(scrutinee) != Some(guard_binding) {
                return None;
            }
            (*guard, condition, *take, *call)
        }
        _ => return None,
    };
    if !drain_guard(guard, condition, *id, window, retained_binding) {
        return None;
    }
    let (last_value, _) = take(last_take, window, functions)?;
    let last_call = evaluated(last_call)?;
    if !same_consumers(
        first_call,
        last_call,
        first_value,
        last_value,
        function,
        window,
        functions,
    ) {
        return None;
    }
    Some(Consumption {
        window,
        retained,
        value: first_value,
        element: first_take.ty(),
        first_call,
        last_call,
    })
}

impl IrBuilder<'_> {
    pub(super) fn lower_terminal_consumption(
        &mut self,
        function: &CheckedFunction,
        functions: &[CheckedFunction],
    ) -> Result<bool, LoweringFailure> {
        let Some(region) = recognize(function, functions) else {
            return Ok(false);
        };
        let target = |call: &CheckedExpression| match call {
            CheckedExpression::UserCall { call, .. } => self
                .physical_calls
                .iter()
                .find_map(|(site, target)| (site == call).then_some(*target)),
            _ => None,
        };
        if target(region.first_call).is_none()
            || target(region.first_call) != target(region.last_call)
        {
            return Ok(false);
        }
        let run = self.lower_place_address(region.window)?;
        let retained = self.expression(region.retained)?;
        let length = self.define(
            U64,
            IrOperation::ContainerMeasure {
                measure: IrMeasure::Length,
                container: run,
            },
        )?;
        let nonempty = self.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![retained, length],
            },
        )?;
        let (start, _) = self.new_block(&[])?;
        let (walk, cursor) = self.new_block(&[U64])?;
        let (next_iteration, _) = self.new_block(&[])?;
        let (finish, _) = self.new_block(&[])?;
        let (done, _) = self.new_block(&[])?;
        self.terminate(IrTerminator::Match {
            scrutinee: nonempty,
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: start,
                },
                IrMatchTarget {
                    tag: 0,
                    block: done,
                },
            ],
        })?;
        self.current = Some(start);
        self.terminate(IrTerminator::Jump {
            target: walk,
            arguments: vec![retained],
            drops: Vec::new(),
        })?;
        self.current = Some(walk);
        // Exact entry subtraction gives r<=n, the window gives n<=cap, and
        // the guarded induction gives r<=cursor<n. Thus cursor+1<=n cannot
        // wrap. Qualified layout bounds positive-stride addressing; zero
        // stride changes only the physical GEP, never this logical count.
        // Back takes and exchanges leave Ring head fixed throughout.
        let owner = self.define(
            lower_type(self.erasure, region.element)?,
            IrOperation::RunIndex {
                run,
                offset: cursor[0],
                target_domain: IrTargetDomainObligation::ElementAddress,
            },
        )?;
        self.addressed_bindings.remove(&region.value);
        self.bindings.insert(region.value, owner);
        // Keep ordinary Call/source-argument metadata, but do not transfer a
        // source overlap group into this replacement loop. Consumer calls
        // remain sequential in logical owner order.
        self.expression(region.first_call)?;
        let one = self.lower_fixed_measure(1)?;
        let next = self.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::AddExact,
                operand_type: U64,
                arguments: vec![cursor[0], one],
            },
        )?;
        let more = self.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![next, length],
            },
        )?;
        self.terminate(IrTerminator::Match {
            scrutinee: more,
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: next_iteration,
                },
                IrMatchTarget {
                    tag: 0,
                    block: finish,
                },
            ],
        })?;
        self.current = Some(next_iteration);
        self.terminate(IrTerminator::Jump {
            target: walk,
            arguments: vec![next],
            drops: Vec::new(),
        })?;
        self.current = Some(finish);
        self.define(
            IrType::Unit,
            IrOperation::RunConsumeFinish { run, retained },
        )?;
        self.terminate(IrTerminator::Jump {
            target: done,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(done);
        let unit = self.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?;
        self.terminate(IrTerminator::Return {
            value: unit,
            drops: Vec::new(),
        })?;
        Ok(true)
    }
}
