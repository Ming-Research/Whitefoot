use super::{compile, compile_and_run, emitted_body, emitted_function};

/// Each negation mode runs in its own helper on the operand its parameter
/// receives, so `main` observes every result and every domain answer at run
/// time instead of a value the checker already knows from a literal.
#[test]
fn executes_every_negation_mode_for_every_signed_width() {
    let template = r#"fn wrapped_negation(operand: $TYPE) -> result: $TYPE pure {
  return ineg.wrap(operand);
}

fn exact_negation(operand: $TYPE) -> result: $TYPE pure contract {
  requires operand != $MIN_$TYPE;
} {
  return ineg(operand);
}

fn negation_defined(operand: $TYPE) -> result: Bool pure {
  return ineg.defined(operand);
}

fn checked_negation(operand: $TYPE) -> result: Result<$TYPE, Overflow> pure {
  return ineg.checked(operand);
}

fn main() -> status: std::process::ExitStatus pure {
  let wrapped = wrapped_negation(operand: $MIN_$TYPE);
  if wrapped == $MIN_$TYPE {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let exact = exact_negation(operand: -42_$TYPE);
  if exact == 42_$TYPE {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let ordinary_defined = negation_defined(operand: -42_$TYPE);
  if ordinary_defined {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let minimum_defined = negation_defined(operand: $MIN_$TYPE);
  if bnot(minimum_defined) {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let safe_result = checked_negation(operand: -42_$TYPE);
  match safe_result {
    Ok(value: safe_value) => {
      if safe_value == 42_$TYPE {
      } else {
        return std::process::exit_status(code: 5_u8);
      }
    }
    Err(error: safe_error) => {
      return std::process::exit_status(code: 6_u8);
    }
  }
  let overflow_result = checked_negation(operand: $MIN_$TYPE);
  match overflow_result {
    Ok(value: overflow_value) => {
      return std::process::exit_status(code: 7_u8);
    }
    Err(error: overflow_error) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (ty, width, minimum) in [
        ("i8", 8, "-128"),
        ("i16", 16, "-32768"),
        ("i32", 32, "-2147483648"),
        ("i64", 64, "-9223372036854775808"),
    ] {
        let source = template.replace("$TYPE", ty).replace("$MIN", minimum);
        let llvm = compile(source.as_bytes());
        let intrinsic = format!("@llvm.ssub.with.overflow.i{width}");
        assert!(
            llvm.contains(&format!("sub i{width} 0,")),
            "wrapping negation must be plain subtraction for {ty}"
        );
        assert!(
            llvm.matches(&format!("{intrinsic}(i{width}")).count() == 2,
            "only the declaration and the checked {ty} negation helper need the overflow intrinsic"
        );
        assert!(
            emitted_body(&llvm, "checked_negation").contains(&format!("{intrinsic}(i{width}")),
            "the checked {ty} negation is the one that calls the overflow intrinsic"
        );
        assert!(llvm.contains(&format!("icmp ne i{width}")));
        // [DIAG-2]: "every fact the checker has proved may be supplied to the
        // backend, as target attributes, instruction flags, metadata, or
        // assumptions: ... and a discharged integer-domain obligation [OP-2],
        // the last being what licenses a no-wrap flag on an exact operation."
        // The same sentence bounds it: "only a fact the checker has actually
        // discharged may be supplied". The `wrap` family carries no [OP-2]
        // obligation and is defined at every operand, including the minimum
        // this program hands it, so nothing has been discharged there and no
        // flag may be supplied. Both directions are read below.
        assert_eq!(
            llvm.matches(&format!("sub nsw i{width} 0,")).count(),
            1,
            "exactly the exact-mode {ty} negation carries nsw"
        );
        assert_eq!(
            llvm.matches(&format!("sub i{width} 0,")).count(),
            1,
            "and the wrap-mode {ty} negation carries no flag at all"
        );
        assert_eq!(
            llvm.matches(" nsw ").count(),
            1,
            "no other {ty} row in the module carries it"
        );
        // No negation is unsigned, so no row of this module states `nuw`.
        assert!(!llvm.contains(" nuw "));
        let output = compile_and_run(&llvm);
        assert!(
            output.status.success(),
            "negation program failed for {ty}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

/// The domain query runs on its helper's parameter, so `main` observes the
/// false answer at run time, and the helper's body holds no negation.
#[test]
fn defined_minimum_reports_false_without_executing_negation() {
    let source = br#"fn negation_defined(operand: i8) -> result: Bool pure {
  return ineg.defined(operand);
}

fn main() -> status: std::process::ExitStatus pure {
  let is_defined = negation_defined(operand: -128_i8);
  if bnot(is_defined) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("icmp ne i8"));
    let query = emitted_function(&llvm, "negation_defined");
    assert!(query.contains("icmp ne i8"), "{query}");
    assert!(!query.contains("sub i8 0,"), "{query}");
    assert!(!query.contains("@llvm.ssub.with.overflow"), "{query}");
    assert!(!llvm.contains("sub i8 0,"));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}
