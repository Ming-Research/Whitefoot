use super::{compile, compile_and_run, emitted_function};

/// Each absolute-value mode runs in its own helper on the operand its
/// parameter receives, so `main` observes every result and every domain
/// answer at run time instead of a value the checker already knows from a
/// literal.
#[test]
fn executes_every_absolute_mode_for_every_signed_width() {
    let template = r#"fn wrapped_absolute(operand: $TYPE) -> result: $TYPE pure {
  return iabs.wrap(operand);
}

fn exact_absolute(operand: $TYPE) -> result: $TYPE pure contract {
  requires operand != $MIN_$TYPE;
} {
  return iabs(operand);
}

fn absolute_defined(operand: $TYPE) -> result: Bool pure {
  return iabs.defined(operand);
}

fn checked_absolute(operand: $TYPE) -> result: Result<$TYPE, Overflow> pure {
  return iabs.checked(operand);
}

fn main() -> status: std::process::ExitStatus pure {
  let wrapped = wrapped_absolute(operand: $MIN_$TYPE);
  if wrapped == $MIN_$TYPE {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let exact = exact_absolute(operand: -42_$TYPE);
  if exact == 42_$TYPE {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let ordinary_defined = absolute_defined(operand: -42_$TYPE);
  if ordinary_defined {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let minimum_defined = absolute_defined(operand: $MIN_$TYPE);
  if bnot(minimum_defined) {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let safe_result = checked_absolute(operand: -42_$TYPE);
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
  let overflow_result = checked_absolute(operand: $MIN_$TYPE);
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
        let intrinsic = format!("@llvm.abs.i{width}");
        assert!(
            llvm.contains(&format!("{intrinsic}(i{width}")),
            "missing absolute-value intrinsic for {ty}"
        );
        assert!(
            llvm.matches(&format!("{intrinsic}(i{width}")).count() >= 4,
            "each {ty} mode must use the same defined-edge intrinsic"
        );
        assert!(llvm.contains("i1 false"));
        let output = compile_and_run(&llvm);
        assert!(
            output.status.success(),
            "absolute-value program failed for {ty}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

/// The domain query runs on its helper's parameter, so `main` observes the
/// false answer at run time, and the helper's body computes no absolute
/// value.
#[test]
fn defined_minimum_reports_false_without_executing_absolute_value() {
    let source = br#"fn absolute_defined(operand: i8) -> result: Bool pure {
  return iabs.defined(operand);
}

fn main() -> status: std::process::ExitStatus pure {
  let is_defined = absolute_defined(operand: -128_i8);
  if bnot(is_defined) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("icmp ne i8"));
    let query = emitted_function(&llvm, "absolute_defined");
    assert!(query.contains("icmp ne i8"), "{query}");
    assert!(!query.contains("@llvm.abs.i8("), "{query}");
    assert!(!llvm.contains("call i8 @llvm.abs.i8("));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}
