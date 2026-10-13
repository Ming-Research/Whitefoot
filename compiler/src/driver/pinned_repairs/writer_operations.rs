//! Operation domains and propagation repairs from the agent-writer trial.

use super::RepairPair;

pub(super) const WRITER_OPERATIONS: &[RepairPair] = &[
    RepairPair {
        name: "writer-float-mixed-widths.wf",
        rejected: br#"fn compare(left: f64, right: f32) -> result: Bool pure {
  return left < right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operand_types: [f64, f32]\n",
            "\n  mechanical_fix: `<` orders integers and these operands have different float types `f64` and `f32`; when rounding the right operand to `f64` is intended, bind `cvt.nearest::<f32, f64>(right)` to a fresh local with a preceding `let`, using the original right operand for `right`; replace this comparison with `flt(left, converted)`, using the original left operand for `left` and that fresh binding for `converted`\n",
        ],
        repaired: &[br#"fn compare(left: f64, right: f32) -> result: Bool pure {
  let converted = cvt.nearest::<f32, f64>(right);
  return flt(left, converted);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-mixed-widths-reversed.wf",
        rejected: br#"fn compare(left: f32, right: f64) -> result: Bool pure {
  return left >= right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operand_types: [f32, f64]\n",
            "\n  mechanical_fix: `>=` orders integers and these operands have different float types `f32` and `f64`; when rounding the right operand to `f32` is intended, bind `cvt.nearest::<f64, f32>(right)` to a fresh local with a preceding `let`, using the original right operand for `right`; replace this comparison with `fge(left, converted)`, using the original left operand for `left` and that fresh binding for `converted`\n",
        ],
        repaired: &[br#"fn compare(left: f32, right: f64) -> result: Bool pure {
  let converted = cvt.nearest::<f64, f32>(right);
  return fge(left, converted);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-distinct-type-parameters.wf",
        rejected: br#"fn compare<L: Float, R: Float>(left: L, right: R) -> result: Bool pure {
  return left > right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operand_types: [L, R]\n",
            "\n  mechanical_fix: `>` orders integers and these operands have different float types `L` and `R`; when rounding the right operand to `L` is intended, bind `cvt.nearest::<R, L>(right)` to a fresh local with a preceding `let`, using the original right operand for `right`; replace this comparison with `fgt(left, converted)`, using the original left operand for `left` and that fresh binding for `converted`\n",
        ],
        repaired: &[br#"fn compare<L: Float, R: Float>(left: L, right: R) -> result: Bool pure {
  let converted = cvt.nearest::<R, L>(right);
  return fgt(left, converted);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-nearest-integer-destination.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.nearest::<f64, u8>(1.0_f64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "]: InvalidOperation\n",
            "\n  operation: cvt.nearest\n",
            "\n  operand_types: [f64, u8]\n",
            "\n  mechanical_fix: `cvt.nearest` needs a float destination (`f32`, `f64`, or a type parameter with the `Float` bound); choose a float destination, or replace this conversion with `cvt.checked::<f64, u8>(x)`, using the same operand for `x`, and handle its `Result` with an exhaustive `match` (including for float-to-integer conversion)\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.nearest::<f64, f32>(1.0_f64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.checked::<f64, u8>(1.0_f64);
  match code {
    Ok(value: found) => {
    }
    Err(error: failed) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "writer-wrap-float-source.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.wrap::<f64, u8>(1.0_f64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: cvt.wrap\n",
            "\n  operand_types: [f64, u8]\n",
            "\n  mechanical_fix: `cvt.wrap` needs integer source and destination types (integer primitives or type parameters with the `Int` bound); replace this conversion with `cvt.checked::<f64, u8>(x)`, using the same operand for `x`, and handle its `Result` with an exhaustive `match`\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.checked::<f64, u8>(1.0_f64);
  match code {
    Ok(value: found) => {
    }
    Err(error: failed) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-wrap-float-destination.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.wrap::<u8, f64>(1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: cvt.wrap\n",
            "\n  operand_types: [u8, f64]\n",
            "\n  mechanical_fix: `cvt.wrap` needs integer source and destination types (integer primitives or type parameters with the `Int` bound); replace this conversion with `cvt.checked::<u8, f64>(x)`, using the same operand for `x`, and handle its `Result` with an exhaustive `match`\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.checked::<u8, f64>(1_u8);
  match code {
    Ok(value: found) => {
    }
    Err(error: failed) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-less.wf",
        rejected: br#"fn compare(left: f64, right: f64) -> result: Bool pure {
  return left < right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: <\n",
            "\n  operand_types: [f64]\n",
            "\n  mechanical_fix: `<` orders integers; replace this float comparison with `flt(left, right)`, using the same two operands as positional arguments; float comparisons are `flt(left, right)`, `fle(left, right)`, `fgt(left, right)`, `fge(left, right)`, `feq(left, right)`, and `fne(left, right)`\n",
        ],
        repaired: &[br#"fn compare(left: f64, right: f64) -> result: Bool pure {
  return flt(left, right);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-less-equal.wf",
        rejected: br#"fn compare(left: f32, right: f32) -> result: Bool pure {
  return left <= right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: <=\n",
            "\n  operand_types: [f32]\n",
            "\n  mechanical_fix: `<=` orders integers; replace this float comparison with `fle(left, right)`, using the same two operands as positional arguments; float comparisons are `flt(left, right)`, `fle(left, right)`, `fgt(left, right)`, `fge(left, right)`, `feq(left, right)`, and `fne(left, right)`\n",
        ],
        repaired: &[br#"fn compare(left: f32, right: f32) -> result: Bool pure {
  return fle(left, right);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-greater.wf",
        rejected: br#"fn compare<T: Float>(left: T, right: T) -> result: Bool pure {
  return left > right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: >\n",
            "\n  operand_types: [T]\n",
            "\n  mechanical_fix: `>` orders integers; replace this float comparison with `fgt(left, right)`, using the same two operands as positional arguments; float comparisons are `flt(left, right)`, `fle(left, right)`, `fgt(left, right)`, `fge(left, right)`, `feq(left, right)`, and `fne(left, right)`\n",
        ],
        repaired: &[br#"fn compare<T: Float>(left: T, right: T) -> result: Bool pure {
  return fgt(left, right);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-float-greater-equal.wf",
        rejected: br#"fn compare(left: f64, right: f64) -> result: Bool pure {
  return left >= right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-1",
        sentences: &[
            "\n  operation: >=\n",
            "\n  operand_types: [f64]\n",
            "\n  mechanical_fix: `>=` orders integers; replace this float comparison with `fge(left, right)`, using the same two operands as positional arguments; float comparisons are `flt(left, right)`, `fle(left, right)`, `fgt(left, right)`, `fge(left, right)`, `feq(left, right)`, and `fne(left, right)`\n",
        ],
        repaired: &[br#"fn compare(left: f64, right: f64) -> result: Bool pure {
  return fge(left, right);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-propagate-unit-return.wf",
        rejected: br#"fn consume(outcome: Result<u8, unit>) -> result: unit pure {
  let value = propagate outcome;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "ERR-3",
        sentences: &[
            "]: InvalidPropagation\n",
            "\n  enclosing_return_type: unit\n",
            "\n  operand_type: Result<u8, unit>\n",
            "\n  mechanical_fix: `propagate` needs the enclosing function to return `Result<U, E>` with the same `E` as its operand; this function returns `unit` and the operand has type `Result<u8, unit>`; handle the `Result` with an exhaustive `match`, or move this code into a helper that returns `Result` with the operand's error type and handle the helper's result here\n",
        ],
        repaired: &[
            br#"fn consume(outcome: Result<u8, unit>) -> result: unit pure {
  match outcome {
    Ok(value: found) => {
      return unit;
    }
    Err(error: failed) => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn forward(outcome: Result<u8, unit>) -> result: Result<u8, unit> pure {
  let value = propagate outcome;
  return Ok<u8, unit>(value: value);
}

fn consume(outcome: Result<u8, unit>) -> result: unit pure {
  let forwarded = forward(outcome: outcome);
  match forwarded {
    Ok(value: found) => {
      return unit;
    }
    Err(error: failed) => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "writer-propagate-different-errors.wf",
        rejected: br#"enum FirstError {
  First();
}

enum SecondError {
  Second();
}

fn consume(outcome: Result<u8, FirstError>) -> result: Result<u8, SecondError> pure {
  let value = propagate outcome;
  return Ok<u8, SecondError>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "ERR-3",
        sentences: &[
            "\n  enclosing_return_type: Result<u8, SecondError>\n",
            "\n  operand_type: Result<u8, FirstError>\n",
            "\n  mechanical_fix: `propagate` needs the enclosing function to return `Result<U, E>` with the same `E` as its operand; this function returns `Result<u8, SecondError>` and the operand has type `Result<u8, FirstError>`; handle the `Result` with an exhaustive `match`, or move this code into a helper that returns `Result` with the operand's error type and handle the helper's result here\n",
        ],
        repaired: &[
            br#"enum FirstError {
  First();
}

enum SecondError {
  Second();
}

fn consume(outcome: Result<u8, FirstError>) -> result: Result<u8, SecondError> pure {
  match outcome {
    Ok(value: found) => {
      return Ok<u8, SecondError>(value: found);
    }
    Err(error: failed) => {
      let error = SecondError::Second();
      return Err<u8, SecondError>(error: error);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"enum FirstError {
  First();
}

enum SecondError {
  Second();
}

fn forward(outcome: Result<u8, FirstError>) -> result: Result<u8, FirstError> pure {
  let value = propagate outcome;
  return Ok<u8, FirstError>(value: value);
}

fn consume(outcome: Result<u8, FirstError>) -> result: Result<u8, SecondError> pure {
  let forwarded = forward(outcome: outcome);
  match forwarded {
    Ok(value: found) => {
      return Ok<u8, SecondError>(value: found);
    }
    Err(error: failed) => {
      let error = SecondError::Second();
      return Err<u8, SecondError>(error: error);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "writer-propagate-non-result.wf",
        rejected: br#"fn consume(holder: Box<Result<u8, unit>>) -> result: unit pure {
  let value = propagate move holder;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "ERR-3",
        sentences: &[
            "\n  enclosing_return_type: unit\n",
            "\n  operand_type: Box<Result<u8, unit>>\n",
            "\n  mechanical_fix: `propagate` needs an own `Result<T, E>` operand; this operand has type `Box<Result<u8, unit>>`; remove `propagate` and bind this value with an ordinary `let`\n",
        ],
        repaired: &[br#"fn consume(holder: Box<Result<u8, unit>>) -> result: unit pure {
  let value = move holder;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

#[test]
fn float_ordering_without_two_owned_float_operands_has_no_float_repair() {
    use super::{CompilationFailureKind, CompilerLimits, SourceInput, compile};

    for right_type in ["u64", "&f64"] {
        let source = format!(
            "fn compare(left: f64, right: {right_type}) -> result: Bool pure {{\n  return left < right;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let failure = compile(
            &[SourceInput::new("nonfloat-right.wf", source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("the integer comparison still rejects its first float operand");
        assert_eq!(failure.kind(), CompilationFailureKind::Source);
        assert_eq!(failure.rule_id(), Some("OP-1"));
        let rendered = failure.to_string();
        assert!(
            !rendered.contains("replace this float comparison"),
            "{rendered}"
        );
        assert!(!rendered.contains("cvt.nearest"), "{rendered}");
    }
}

#[test]
fn mixed_float_contract_ordering_has_no_runtime_conversion_repair() {
    use super::{CompilationFailureKind, CompilerLimits, SourceInput, compile};

    for (left_type, right_type, clause) in [
        ("f64", "f32", "requires left < right;"),
        ("f32", "f64", "requires left < right;"),
        (
            "f64",
            "f32",
            "define comparison = left < right;\n  requires comparison;",
        ),
    ] {
        let source = format!(
            "fn compare(left: {left_type}, right: {right_type}) -> result: unit pure contract {{\n  {clause}\n}} {{\n  return unit;\n}}\n"
        );
        let failure = compile(
            &[SourceInput::new("mixed-float-contract.wf", source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("the integer comparison still rejects its first float operand");
        assert_eq!(failure.kind(), CompilationFailureKind::Source);
        assert_eq!(failure.rule_id(), Some("OP-1"), "{failure}");
        let rendered = failure.to_string();
        assert!(!rendered.contains("mechanical_fix:"), "{rendered}");
    }
}
