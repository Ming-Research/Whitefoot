use super::{compile, compile_and_run};

#[test]
fn executes_the_remaining_integer_family_and_defined_edges() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let anded = iand(240_u8, 15_u8);
  if anded == 0_u8 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let ored = ior(240_u8, 15_u8);
  if ored == 255_u8 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let xored = ixor(240_u8, 15_u8);
  if xored == 255_u8 {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let inverted = inot(0_u8);
  if inverted == 255_u8 {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let shifted_wrap = ishl.wrap(1_u8, 9_u32);
  if shifted_wrap == 2_u8 {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  let right_signed = ishr.wrap(-4_i8, 1_u32);
  if right_signed == -2_i8 {
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let shifted_exact = ishl(1_u8, 7_u32);
  if shifted_exact == 128_u8 {
  } else {
    return std::process::exit_status(code: 7_u8);
  }
  let right_exact = ishr(128_u8, 7_u32);
  if right_exact == 1_u8 {
  } else {
    return std::process::exit_status(code: 8_u8);
  }
  let rotated_left = irotl(1_u8, 1_u32);
  if rotated_left == 2_u8 {
  } else {
    return std::process::exit_status(code: 9_u8);
  }
  let rotated_right = irotr(1_u8, 1_u32);
  if rotated_right == 128_u8 {
  } else {
    return std::process::exit_status(code: 10_u8);
  }
  let population = ipopcount(240_u8);
  if population == 4_u32 {
  } else {
    return std::process::exit_status(code: 11_u8);
  }
  let leading = iclz(1_u8);
  if leading == 7_u32 {
  } else {
    return std::process::exit_status(code: 12_u8);
  }
  let trailing = ictz(0_u8);
  if trailing == 8_u32 {
  } else {
    return std::process::exit_status(code: 13_u8);
  }
  let swapped = ibswap(4660_u16);
  if swapped == 13330_u16 {
  } else {
    return std::process::exit_status(code: 14_u8);
  }
  let high_unsigned = imulhi(255_u8, 2_u8);
  if high_unsigned == 1_u8 {
  } else {
    return std::process::exit_status(code: 15_u8);
  }
  let high_signed = imulhi(-128_i8, 2_i8);
  if high_signed == -1_i8 {
  } else {
    return std::process::exit_status(code: 16_u8);
  }
  let add_unsigned = 250_u8 +sat 10_u8;
  if add_unsigned == 255_u8 {
  } else {
    return std::process::exit_status(code: 17_u8);
  }
  let add_signed = 120_i8 +sat 20_i8;
  if add_signed == 127_i8 {
  } else {
    return std::process::exit_status(code: 18_u8);
  }
  let subtract_unsigned = 1_u8 -sat 2_u8;
  if subtract_unsigned == 0_u8 {
  } else {
    return std::process::exit_status(code: 19_u8);
  }
  let subtract_signed = -120_i8 -sat 20_i8;
  if subtract_signed == -128_i8 {
  } else {
    return std::process::exit_status(code: 20_u8);
  }
  let multiply_unsigned = 20_u8 *sat 20_u8;
  if multiply_unsigned == 255_u8 {
  } else {
    return std::process::exit_status(code: 21_u8);
  }
  let multiply_high = 20_i8 *sat 20_i8;
  if multiply_high == 127_i8 {
  } else {
    return std::process::exit_status(code: 22_u8);
  }
  let multiply_low = -20_i8 *sat 20_i8;
  if multiply_low == -128_i8 {
  } else {
    return std::process::exit_status(code: 23_u8);
  }
  let minimum = imin(-2_i8, 1_i8);
  if minimum == -2_i8 {
  } else {
    return std::process::exit_status(code: 24_u8);
  }
  let maximum = imax(254_u8, 1_u8);
  if maximum == 254_u8 {
  } else {
    return std::process::exit_status(code: 25_u8);
  }
  let quotient = 9_i32 / 2_i32;
  if quotient == 4_i32 {
  } else {
    return std::process::exit_status(code: 26_u8);
  }
  let remainder = 9_i32 % 2_i32;
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

#[test]
fn defined_shift_reports_false_without_executing_an_invalid_shift() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let is_defined = ishl.defined(1_u8, 8_u32);
  if bnot(is_defined) {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("icmp ult i32"));
    assert!(!llvm.contains("shl i8 1, 8"));
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
