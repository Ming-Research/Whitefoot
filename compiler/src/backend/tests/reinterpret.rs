use super::{compile, compile_and_run};

/// Each integer pair reinterprets the operand its helper's parameter
/// receives, so `main` observes the preserved bits at run time instead of a
/// value the checker already knows from a literal.
#[test]
fn every_reinterpret_pair_preserves_its_source_bits() {
    let source = br#"fn u8_from_i8(bits: i8) -> result: u8 pure {
  return reinterpret::<i8, u8>(bits);
}

fn i8_from_u8(bits: u8) -> result: i8 pure {
  return reinterpret::<u8, i8>(bits);
}

fn u16_from_i16(bits: i16) -> result: u16 pure {
  return reinterpret::<i16, u16>(bits);
}

fn i16_from_u16(bits: u16) -> result: i16 pure {
  return reinterpret::<u16, i16>(bits);
}

fn u32_from_i32(bits: i32) -> result: u32 pure {
  return reinterpret::<i32, u32>(bits);
}

fn i32_from_u32(bits: u32) -> result: i32 pure {
  return reinterpret::<u32, i32>(bits);
}

fn u64_from_i64(bits: i64) -> result: u64 pure {
  return reinterpret::<i64, u64>(bits);
}

fn i64_from_u64(bits: u64) -> result: i64 pure {
  return reinterpret::<u64, i64>(bits);
}

fn main() -> status: std::process::ExitStatus pure {
  let u8_bits = u8_from_i8(bits: -1_i8);
  if u8_bits == 255_u8 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let i8_bits = i8_from_u8(bits: 255_u8);
  if i8_bits == -1_i8 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let u16_bits = u16_from_i16(bits: -1_i16);
  if u16_bits == 65535_u16 {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let i16_bits = i16_from_u16(bits: 65535_u16);
  if i16_bits == -1_i16 {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  let u32_bits = u32_from_i32(bits: -1_i32);
  if u32_bits == 4294967295_u32 {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  let i32_bits = i32_from_u32(bits: 4294967295_u32);
  if i32_bits == -1_i32 {
  } else {
    return std::process::exit_status(code: 6_u8);
  }
  let u64_bits = u64_from_i64(bits: -1_i64);
  if u64_bits == 18446744073709551615_u64 {
  } else {
    return std::process::exit_status(code: 7_u8);
  }
  let i64_bits = i64_from_u64(bits: 18446744073709551615_u64);
  if i64_bits == -1_i64 {
  } else {
    return std::process::exit_status(code: 8_u8);
  }
  let f32_from_i32 = reinterpret::<i32, f32>(2143289345_i32);
  if fne(f32_from_i32, f32_from_i32) {
  } else {
    return std::process::exit_status(code: 9_u8);
  }
  let i32_from_f32 = reinterpret::<f32, i32>(f32_from_i32);
  if i32_from_f32 == 2143289345_i32 {
  } else {
    return std::process::exit_status(code: 10_u8);
  }
  let f32_from_u32 = reinterpret::<u32, f32>(2143289346_u32);
  if fne(f32_from_u32, f32_from_u32) {
  } else {
    return std::process::exit_status(code: 11_u8);
  }
  let u32_from_f32 = reinterpret::<f32, u32>(f32_from_u32);
  if u32_from_f32 == 2143289346_u32 {
  } else {
    return std::process::exit_status(code: 12_u8);
  }
  let f64_from_i64 = reinterpret::<i64, f64>(9221120237041090561_i64);
  if fne(f64_from_i64, f64_from_i64) {
  } else {
    return std::process::exit_status(code: 13_u8);
  }
  let i64_from_f64 = reinterpret::<f64, i64>(f64_from_i64);
  if i64_from_f64 == 9221120237041090561_i64 {
  } else {
    return std::process::exit_status(code: 14_u8);
  }
  let f64_from_u64 = reinterpret::<u64, f64>(9221120237041090562_u64);
  if fne(f64_from_u64, f64_from_u64) {
  } else {
    return std::process::exit_status(code: 15_u8);
  }
  let u64_from_f64 = reinterpret::<f64, u64>(f64_from_u64);
  if u64_from_f64 == 9221120237041090562_u64 {
  } else {
    return std::process::exit_status(code: 16_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    for instruction in [
        "bitcast i32",
        "bitcast float",
        "bitcast i64",
        "bitcast double",
        "or i8",
        "or i16",
        "or i32",
        "or i64",
    ] {
        assert!(
            llvm.contains(instruction),
            "reinterpret family must exercise {instruction}"
        );
    }
    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "reinterpret family failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
