use super::{compile, compile_and_run};

#[test]
fn every_direct_float_operation_executes_for_both_widths() {
    let template = r#"fn main() -> status: std::process::ExitStatus pure {
  let sum = fadd.strict(1.5_$TYPE, 2.25_$TYPE);
  if feq(sum, 3.75_$TYPE) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let difference = fsub.strict(sum, 0.75_$TYPE);
  if feq(difference, 3.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let product = fmul.strict(difference, 2.0_$TYPE);
  if feq(product, 6.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let quotient = fdiv.strict(product, 4.0_$TYPE);
  if feq(quotient, 1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let negative = fneg(quotient);
  if feq(negative, -1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  let absolute = fabs(negative);
  if feq(absolute, 1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let signed = fcopysign(absolute, negative);
  if feq(signed, -1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 7_u8);
  }
  let minimum = fmin(negative, absolute);
  if feq(minimum, -1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 8_u8);
  }
  let maximum = fmax(negative, absolute);
  if feq(maximum, 1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 9_u8);
  }
  let floor = ffloor(1.75_$TYPE);
  if feq(floor, 1.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 10_u8);
  }
  let ceil = fceil(1.25_$TYPE);
  if feq(ceil, 2.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 11_u8);
  }
  let truncated = ftrunc(-1.75_$TYPE);
  if feq(truncated, -1.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 12_u8);
  }
  let rounded = froundeven(2.5_$TYPE);
  if feq(rounded, 2.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 13_u8);
  }
  let remainder = frem(5.5_$TYPE, 2.0_$TYPE);
  if feq(remainder, 1.5_$TYPE) {
  } else {
    return std::process::exit_status(code: 14_u8);
  }
  let root = fsqrt.strict(4.0_$TYPE);
  if feq(root, 2.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 15_u8);
  }
  let fused = ffma.strict(2.0_$TYPE, 3.0_$TYPE, 1.0_$TYPE);
  if feq(fused, 7.0_$TYPE) {
  } else {
    return std::process::exit_status(code: 16_u8);
  }
  let infinity = finf::<$TYPE>();
  if fgt(infinity, fused) {
  } else {
    return std::process::exit_status(code: 17_u8);
  }
  let negative_infinity = fneg(infinity);
  if flt(negative_infinity, negative) {
  } else {
    return std::process::exit_status(code: 18_u8);
  }
  let nan = fnan::<$TYPE>();
  if fne(nan, nan) {
  } else {
    return std::process::exit_status(code: 19_u8);
  }
  let minimum_nan = fmin(nan, fused);
  if fne(minimum_nan, minimum_nan) {
  } else {
    return std::process::exit_status(code: 20_u8);
  }
  let negative_zero = fneg(0.0_$TYPE);
  let minimum_zero = fmin(negative_zero, 0.0_$TYPE);
  let minimum_reciprocal = fdiv.strict(1.0_$TYPE, minimum_zero);
  if feq(minimum_reciprocal, negative_infinity) {
  } else {
    return std::process::exit_status(code: 21_u8);
  }
  let maximum_zero = fmax(negative_zero, 0.0_$TYPE);
  let maximum_reciprocal = fdiv.strict(1.0_$TYPE, maximum_zero);
  if feq(maximum_reciprocal, infinity) {
  } else {
    return std::process::exit_status(code: 22_u8);
  }
  if fle(absolute, sum) {
  } else {
    return std::process::exit_status(code: 23_u8);
  }
  if fge(sum, absolute) {
  } else {
    return std::process::exit_status(code: 24_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for ty in ["f32", "f64"] {
        let llvm = compile(template.replace("$TYPE", ty).as_bytes());
        for forbidden in ["fadd fast", "fsub fast", "fmul fast", "fdiv fast"] {
            assert!(!llvm.contains(forbidden));
        }
        for intrinsic in [
            "llvm.fabs.",
            "llvm.copysign.",
            "llvm.minimum.",
            "llvm.maximum.",
            "llvm.floor.",
            "llvm.ceil.",
            "llvm.trunc.",
            "llvm.roundeven.",
            "llvm.sqrt.",
            "llvm.fma.",
        ] {
            assert!(llvm.contains(intrinsic), "missing {intrinsic} for {ty}");
        }
        let output = compile_and_run(&llvm);
        assert!(
            output.status.success(),
            "float operations failed for {ty}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

/// [OP-3] the rounding float ops carry `.strict`: IEEE 754, no reassociation,
/// no contraction. The absent `fast` flags the test above asserts are a
/// statement about the emitted module; this one is a statement about the
/// executed program, because the link step optimizes at the same level every
/// Whitefoot executable uses.
///
/// The witness is an addition that is not associative in binary32. With
/// `half_ulp` below one half of 1.0's ulp, `(1.0 + h) + h` rounds each step
/// back to 1.0, while `1.0 + (h + h)` rounds once and lands one ulp above it.
/// A run that reassociated, contracted, or widened any of the four additions
/// would make the two results agree and trap.
///
/// A green run establishes exactly that: it does not establish that any other
/// float op resists reassociation, only these additions under this host and
/// optimization level.
#[test]
fn strict_float_addition_rounds_every_step_and_is_never_reassociated() {
    let source = br#"fn left(a: f32, b: f32, c: f32) -> result: f32 pure {
  let ab = fadd.strict(a, b);
  return fadd.strict(ab, c);
}

fn right(a: f32, b: f32, c: f32) -> result: f32 pure {
  let bc = fadd.strict(b, c);
  return fadd.strict(a, bc);
}

fn main() -> status: std::process::ExitStatus pure {
  let one = 1.0_f32;
  let half_ulp = 4.0e-8_f32;
  let stepwise = left(a: one, b: half_ulp, c: half_ulp);
  let regrouped = right(a: one, b: half_ulp, c: half_ulp);
  if feq(stepwise, one) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  if fne(stepwise, regrouped) {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    for forbidden in ["fadd fast", "reassoc", "contract"] {
        assert!(
            !llvm.contains(forbidden),
            "strict addition must not carry {forbidden}"
        );
    }
    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "strict float association failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
