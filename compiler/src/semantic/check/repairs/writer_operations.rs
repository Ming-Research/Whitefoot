//! Operation-domain and Result-propagation repairs for the writer trial.

use crate::semantic::model::CheckedConversionMode;

pub(in crate::semantic::check) fn conversion_repair(
    mode: CheckedConversionMode,
    source: &str,
    destination: &str,
) -> String {
    match mode {
        CheckedConversionMode::Nearest => format!(
            "`cvt.nearest` needs a float destination (`f32`, `f64`, or a type parameter with the `Float` bound); choose a float destination, or replace this conversion with `cvt.checked::<{source}, {destination}>(x)`, using the same operand for `x`, and handle its `Result` with an exhaustive `match` (including for float-to-integer conversion)"
        ),
        CheckedConversionMode::Wrap => format!(
            "`cvt.wrap` needs integer source and destination types (integer primitives or type parameters with the `Int` bound); replace this conversion with `cvt.checked::<{source}, {destination}>(x)`, using the same operand for `x`, and handle its `Result` with an exhaustive `match`"
        ),
        CheckedConversionMode::Exact
        | CheckedConversionMode::Checked
        | CheckedConversionMode::Defined => unreachable!("only invalid endpoint modes carry this repair"),
    }
}

pub(in crate::semantic::check) fn float_ordering_repair(symbol: &str, operation: &str) -> String {
    format!(
        "`{symbol}` orders integers; replace this float comparison with `{operation}(left, right)`, using the same two operands as positional arguments; float comparisons are `flt(left, right)`, `fle(left, right)`, `fgt(left, right)`, `fge(left, right)`, `feq(left, right)`, and `fne(left, right)`"
    )
}

pub(in crate::semantic::check) fn propagation_repair(
    enclosing_return_type: &str,
    operand_type: &str,
    operand_is_result: bool,
) -> String {
    if operand_is_result {
        format!(
            "`propagate` needs the enclosing function to return `Result<U, E>` with the same `E` as its operand; this function returns `{enclosing_return_type}` and the operand has type `{operand_type}`; handle the `Result` with an exhaustive `match`, or move this code into a helper that returns `Result` with the operand's error type and handle the helper's result here"
        )
    } else {
        format!(
            "`propagate` needs an own `Result<T, E>` operand; this operand has type `{operand_type}`; remove `propagate` and bind this value with an ordinary `let`"
        )
    }
}
