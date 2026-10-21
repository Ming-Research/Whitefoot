use super::{compile, compile_and_run, emitted_function};

/// Each operation runs in its own helper on the operands its parameters
/// receive, so `main` observes every result at run time instead of a value
/// the checker already knows from literals.
#[test]
fn executes_the_remaining_integer_family_and_defined_edges() {
    let source = br#"fn and_bits(left: u8, right: u8) -> result: u8 pure {
  return iand(left, right);
}

fn or_bits(left: u8, right: u8) -> result: u8 pure {
  return ior(left, right);
}

fn xor_bits(left: u8, right: u8) -> result: u8 pure {
  return ixor(left, right);
}

fn not_bits(value: u8) -> result: u8 pure {
  return inot(value);
}

fn shift_left_wrapping(value: u8, amount: u32) -> result: u8 pure {
  return ishl.wrap(value, amount);
}

fn shift_right_wrapping(value: i8, amount: u32) -> result: i8 pure {
  return ishr.wrap(value, amount);
}

fn shift_left_exact(value: u8, amount: u32) -> result: u8 pure contract {
  requires amount < 8_u32;
} {
  return ishl(value, amount);
}

fn shift_right_exact(value: u8, amount: u32) -> result: u8 pure contract {
  requires amount < 8_u32;
} {
  return ishr(value, amount);
}

fn rotate_left(value: u8, amount: u32) -> result: u8 pure {
  return irotl(value, amount);
}

fn rotate_right(value: u8, amount: u32) -> result: u8 pure {
  return irotr(value, amount);
}

fn population_count(value: u8) -> result: u32 pure {
  return ipopcount(value);
}

fn leading_zeros(value: u8) -> result: u32 pure {
  return iclz(value);
}

fn trailing_zeros(value: u8) -> result: u32 pure {
  return ictz(value);
}

fn byte_swap(value: u16) -> result: u16 pure {
  return ibswap(value);
}

fn high_product_unsigned(left: u8, right: u8) -> result: u8 pure {
  return imulhi(left, right);
}

fn high_product_signed(left: i8, right: i8) -> result: i8 pure {
  return imulhi(left, right);
}

fn saturating_sum_unsigned(left: u8, right: u8) -> result: u8 pure {
  return left +sat right;
}

fn saturating_sum_signed(left: i8, right: i8) -> result: i8 pure {
  return left +sat right;
}

fn saturating_difference_unsigned(left: u8, right: u8) -> result: u8 pure {
  return left -sat right;
}

fn saturating_difference_signed(left: i8, right: i8) -> result: i8 pure {
  return left -sat right;
}

fn saturating_product_unsigned(left: u8, right: u8) -> result: u8 pure {
  return left *sat right;
}

fn saturating_product_signed(left: i8, right: i8) -> result: i8 pure {
  return left *sat right;
}

fn smaller_of(left: i8, right: i8) -> result: i8 pure {
  return imin(left, right);
}

fn larger_of(left: u8, right: u8) -> result: u8 pure {
  return imax(left, right);
}

fn quotient_of(numerator: i32, denominator: i32) -> result: i32 pure contract {
  requires denominator != 0_i32;
  requires denominator != -1_i32;
} {
  return numerator / denominator;
}

fn remainder_of(numerator: i32, denominator: i32) -> result: i32 pure contract {
  requires denominator != 0_i32;
  requires denominator != -1_i32;
} {
  return numerator % denominator;
}

fn main() -> status: std::process::ExitStatus pure {
  let anded = and_bits(left: 240_u8, right: 15_u8);
  if anded == 0_u8 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let ored = or_bits(left: 240_u8, right: 15_u8);
  if ored == 255_u8 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let xored = xor_bits(left: 240_u8, right: 15_u8);
  if xored == 255_u8 {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let inverted = not_bits(value: 0_u8);
  if inverted == 255_u8 {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let shifted_wrap = shift_left_wrapping(value: 1_u8, amount: 9_u32);
  if shifted_wrap == 2_u8 {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  let right_signed = shift_right_wrapping(value: -4_i8, amount: 1_u32);
  if right_signed == -2_i8 {
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let shifted_exact = shift_left_exact(value: 1_u8, amount: 7_u32);
  if shifted_exact == 128_u8 {
  } else {
    return std::process::exit_status(code: 7_u8);
  }
  let right_exact = shift_right_exact(value: 128_u8, amount: 7_u32);
  if right_exact == 1_u8 {
  } else {
    return std::process::exit_status(code: 8_u8);
  }
  let rotated_left = rotate_left(value: 1_u8, amount: 1_u32);
  if rotated_left == 2_u8 {
  } else {
    return std::process::exit_status(code: 9_u8);
  }
  let rotated_right = rotate_right(value: 1_u8, amount: 1_u32);
  if rotated_right == 128_u8 {
  } else {
    return std::process::exit_status(code: 10_u8);
  }
  let population = population_count(value: 240_u8);
  if population == 4_u32 {
  } else {
    return std::process::exit_status(code: 11_u8);
  }
  let leading = leading_zeros(value: 1_u8);
  if leading == 7_u32 {
  } else {
    return std::process::exit_status(code: 12_u8);
  }
  let trailing = trailing_zeros(value: 0_u8);
  if trailing == 8_u32 {
  } else {
    return std::process::exit_status(code: 13_u8);
  }
  let swapped = byte_swap(value: 4660_u16);
  if swapped == 13330_u16 {
  } else {
    return std::process::exit_status(code: 14_u8);
  }
  let high_unsigned = high_product_unsigned(left: 255_u8, right: 2_u8);
  if high_unsigned == 1_u8 {
  } else {
    return std::process::exit_status(code: 15_u8);
  }
  let high_signed = high_product_signed(left: -128_i8, right: 2_i8);
  if high_signed == -1_i8 {
  } else {
    return std::process::exit_status(code: 16_u8);
  }
  let add_unsigned = saturating_sum_unsigned(left: 250_u8, right: 10_u8);
  if add_unsigned == 255_u8 {
  } else {
    return std::process::exit_status(code: 17_u8);
  }
  let add_signed = saturating_sum_signed(left: 120_i8, right: 20_i8);
  if add_signed == 127_i8 {
  } else {
    return std::process::exit_status(code: 18_u8);
  }
  let subtract_unsigned = saturating_difference_unsigned(left: 1_u8, right: 2_u8);
  if subtract_unsigned == 0_u8 {
  } else {
    return std::process::exit_status(code: 19_u8);
  }
  let subtract_signed = saturating_difference_signed(left: -120_i8, right: 20_i8);
  if subtract_signed == -128_i8 {
  } else {
    return std::process::exit_status(code: 20_u8);
  }
  let multiply_unsigned = saturating_product_unsigned(left: 20_u8, right: 20_u8);
  if multiply_unsigned == 255_u8 {
  } else {
    return std::process::exit_status(code: 21_u8);
  }
  let multiply_high = saturating_product_signed(left: 20_i8, right: 20_i8);
  if multiply_high == 127_i8 {
  } else {
    return std::process::exit_status(code: 22_u8);
  }
  let multiply_low = saturating_product_signed(left: -20_i8, right: 20_i8);
  if multiply_low == -128_i8 {
  } else {
    return std::process::exit_status(code: 23_u8);
  }
  let minimum = smaller_of(left: -2_i8, right: 1_i8);
  if minimum == -2_i8 {
  } else {
    return std::process::exit_status(code: 24_u8);
  }
  let maximum = larger_of(left: 254_u8, right: 1_u8);
  if maximum == 254_u8 {
  } else {
    return std::process::exit_status(code: 25_u8);
  }
  let quotient = quotient_of(numerator: 9_i32, denominator: 2_i32);
  if quotient == 4_i32 {
  } else {
    return std::process::exit_status(code: 26_u8);
  }
  let remainder = remainder_of(numerator: 9_i32, denominator: 2_i32);
  if remainder == 1_i32 {
  } else {
    return std::process::exit_status(code: 27_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    for fragment in [
        "@llvm.fshl.i8",
        "@llvm.fshr.i8",
        "@llvm.ctpop.i8",
        "@llvm.ctlz.i8",
        "@llvm.cttz.i8",
        "@llvm.bswap.i16",
        "@llvm.uadd.sat.i8",
        "@llvm.sadd.sat.i8",
        "@llvm.usub.sat.i8",
        "@llvm.ssub.sat.i8",
        "@llvm.smin.i8",
        "@llvm.umax.i8",
    ] {
        assert!(llvm.contains(fragment), "missing lowering {fragment}");
    }
    assert!(!llvm.contains(" nsw "));
    assert!(!llvm.contains(" nuw "));
    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "integer-family program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// The domain query runs on its helper's parameters, so `main` observes the
/// false answer at run time, and the helper's body executes no shift.
#[test]
fn defined_shift_reports_false_without_executing_an_invalid_shift() {
    let source = br#"fn shift_defined(value: u8, amount: u32) -> result: Bool pure {
  return ishl.defined(value, amount);
}

fn main() -> status: std::process::ExitStatus pure {
  let is_defined = shift_defined(value: 1_u8, amount: 8_u32);
  if bnot(is_defined) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("icmp ult i32"));
    let query = emitted_function(&llvm, "shift_defined");
    assert!(query.contains("icmp ult i32"), "{query}");
    assert!(!query.contains("shl i8"), "{query}");
    assert!(!llvm.contains("shl i8"));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}

#[test]
fn defined_division_checks_zero_and_signed_overflow_without_dividing() {
    let source = br#"fn division_is_defined(n: i32, d: i32) -> result: Bool pure {
  return n /defined d;
}

fn main() -> status: std::process::ExitStatus pure {
  let zero = 0_i32;
  let one = 1_i32;
  let zero_defined = division_is_defined(n: one, d: zero);
  if bnot(zero_defined) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let minimum = -2147483648_i32;
  let minus_one = -1_i32;
  let overflow_defined = division_is_defined(n: minimum, d: minus_one);
  if bnot(overflow_defined) {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let ordinary_defined = division_is_defined(n: one, d: one);
  if ordinary_defined {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("icmp ne i32"));
    assert!(llvm.contains("icmp ne i32 %v0, -2147483648"));
    assert!(llvm.contains("icmp ne i32 %v1, -1"));
    assert!(!llvm.contains(" = sdiv i32"));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}
