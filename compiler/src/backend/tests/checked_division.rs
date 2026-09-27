use super::{compile, compile_and_run};

#[test]
fn guards_every_integer_error_before_llvm() {
    let template = r#"fn main() -> status: std::process::ExitStatus pure {
  let quotient = 84_$TYPE /checked 2_$TYPE;
  match quotient {
    Ok(value: quotient_value) => {
      if quotient_value == 42_$TYPE {
      } else {
        return std::process::exit_status(code: 1_u8);
      }
    }
    Err(error: quotient_error) => {
      return std::process::exit_status(code: 2_u8);
    }
  }
  let remainder = 85_$TYPE %checked 43_$TYPE;
  match remainder {
    Ok(value: remainder_value) => {
      if remainder_value == 42_$TYPE {
      } else {
        return std::process::exit_status(code: 3_u8);
      }
    }
    Err(error: remainder_error) => {
      return std::process::exit_status(code: 4_u8);
    }
  }
  let divide_zero = 42_$TYPE /checked 0_$TYPE;
  match divide_zero {
    Ok(value: divide_zero_value) => {
      return std::process::exit_status(code: 5_u8);
    }
    Err(error: divide_zero_error) => {
      match divide_zero_error {
        DivideByZero() => {
        }
        DivOverflow() => {
          return std::process::exit_status(code: 6_u8);
        }
      }
    }
  }
  let remainder_zero = 42_$TYPE %checked 0_$TYPE;
  match remainder_zero {
    Ok(value: remainder_zero_value) => {
      return std::process::exit_status(code: 7_u8);
    }
    Err(error: remainder_zero_error) => {
      match remainder_zero_error {
        DivideByZero() => {
        }
        DivOverflow() => {
          return std::process::exit_status(code: 8_u8);
        }
      }
    }
  }
$SIGNED_CASES  return std::process::exit_status(code: 0_u8);
}
"#;
    for (ty, width, signed, minimum) in [
        ("i8", 8, true, "-128"),
        ("i16", 16, true, "-32768"),
        ("i32", 32, true, "-2147483648"),
        ("i64", 64, true, "-9223372036854775808"),
        ("u8", 8, false, ""),
        ("u16", 16, false, ""),
        ("u32", 32, false, ""),
        ("u64", 64, false, ""),
    ] {
        let signed_cases = if signed {
            format!(
                r#"  let divide_overflow = {minimum}_{ty} /checked -1_{ty};
  match divide_overflow {{
    Ok(value: divide_overflow_value) => {{
      return std::process::exit_status(code: 9_u8);
    }}
    Err(error: divide_overflow_error) => {{
      match divide_overflow_error {{
        DivideByZero() => {{
          return std::process::exit_status(code: 10_u8);
        }}
        DivOverflow() => {{
        }}
      }}
    }}
  }}
  let remainder_overflow = {minimum}_{ty} %checked -1_{ty};
  match remainder_overflow {{
    Ok(value: remainder_overflow_value) => {{
      return std::process::exit_status(code: 11_u8);
    }}
    Err(error: remainder_overflow_error) => {{
      match remainder_overflow_error {{
        DivideByZero() => {{
          return std::process::exit_status(code: 12_u8);
        }}
        DivOverflow() => {{
        }}
      }}
    }}
  }}
"#
            )
        } else {
            String::new()
        };
        let source = template
            .replace("$TYPE", ty)
            .replace("$SIGNED_CASES", &signed_cases);
        let llvm = compile(source.as_bytes());
        let division = if signed { "sdiv" } else { "udiv" };
        let remainder = if signed { "srem" } else { "urem" };
        for opcode in [division, remainder] {
            let instruction = format!(" = {opcode} i{width} ");
            let operation = llvm
                .find(&instruction)
                .unwrap_or_else(|| panic!("missing {opcode} lowering for {ty}"));
            let guarded_prefix = &llvm[..operation];
            let safe = guarded_prefix
                .rfind("integer.safe.")
                .unwrap_or_else(|| panic!("{opcode} for {ty} is not in a safe block"));
            let branch = guarded_prefix[..safe]
                .rfind("br i1")
                .unwrap_or_else(|| panic!("{opcode} for {ty} has no preceding error branch"));
            assert!(branch < safe && safe < operation);
        }
        let output = compile_and_run(&llvm);
        assert!(
            output.status.success(),
            "checked division program failed for {ty}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}
