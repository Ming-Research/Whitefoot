use std::fmt::Write;

use super::{compile, compile_and_run};

#[derive(Clone, Copy)]
struct IntegerType {
    spelling: &'static str,
    width: u8,
    signed: bool,
}

const I8: IntegerType = IntegerType {
    spelling: "i8",
    width: 8,
    signed: true,
};
const I16: IntegerType = IntegerType {
    spelling: "i16",
    width: 16,
    signed: true,
};
const I32: IntegerType = IntegerType {
    spelling: "i32",
    width: 32,
    signed: true,
};
const I64: IntegerType = IntegerType {
    spelling: "i64",
    width: 64,
    signed: true,
};
const U8: IntegerType = IntegerType {
    spelling: "u8",
    width: 8,
    signed: false,
};
const U16: IntegerType = IntegerType {
    spelling: "u16",
    width: 16,
    signed: false,
};
const U32: IntegerType = IntegerType {
    spelling: "u32",
    width: 32,
    signed: false,
};
const U64: IntegerType = IntegerType {
    spelling: "u64",
    width: 64,
    signed: false,
};

const INTEGER_TYPES: [IntegerType; 8] = [I8, I16, I32, I64, U8, U16, U32, U64];

/// One representative per conversion class rather than all 64 ordered pairs.
///
/// Both compiler sides are fully parametric in `(width, signedness)` and hold
/// no per-pair path. Identity adds a representation copy for either sign;
/// the other integer casts follow the width relation. The semantic side decides totality with
/// comparison `IntegerType::converts_totally_to`, reached from
/// `semantic/check/expressions/calls/conversions.rs`. The emitter in
/// `backend/emitter/conversion.rs` picks one of four integer cast arms
/// (`trunc`, `sext`, `zext`, and the same-width `or`) and, for a checked
/// conversion, one of five reachable validity arms — signed narrowing
/// (`sge`/`sle` pair), unsigned narrowing (`ule`), signed to unsigned without
/// narrowing (`sge 0`), signed to unsigned with narrowing (`sge 0` and
/// `sle`), and unsigned to signed (`ule`) — from the width relation and the
/// sign pair alone. Two pairs sharing a
/// `(source sign, destination sign, width relation)` class therefore emit
/// structurally identical code that differs only in its constants, so one
/// representative per class is complete evidence for the arm selection, and
/// the widest rows below cover the constant arithmetic those arms compute.
///
/// ADDING AN EMITTER ARM REQUIRES ADDING A ROW HERE.
const CONVERSION_CLASSES: [(IntegerType, IntegerType); 17] = [
    // Equivalence-class representatives, one per reachable arm combination.
    (I16, I8), // trunc, signed narrowing validity (sge/sle pair)
    (I8, I16), // sext, total
    (U16, U8), // trunc, unsigned narrowing validity (ule)
    (U8, U16), // zext, total
    (I16, U8), // trunc, signed to unsigned narrowing validity (sge 0 and sle)
    (I8, U8),  // same-width or, signed to unsigned validity (sge 0)
    (I8, U16), // sext under a checked conversion, validity (sge 0)
    (U16, I8), // trunc, unsigned to signed validity (ule)
    (U8, I8),  // same-width or, unsigned to signed validity (ule)
    (U8, I16), // zext, total
    // Widest-constant rows: the emitter computes its validity bounds as
    // `1 << (w - 1)` and `(1 << w) - 1` from the operand widths, so every arm
    // that computes a constant is exercised once at 64 bits.
    (I64, I32), // signed narrowing bounds at the widest destination
    (U64, U32), // unsigned narrowing maximum at the widest destination
    (I64, U64), // 64-bit same-width or, signed to unsigned validity
    (U64, I64), // 64-bit same-width or, unsigned to signed maximum
    (I64, U32), // signed to unsigned narrowing maximum at 64-bit width
    (I64, I64), // signed identity preserves all bits and is total
    (U64, U64), // unsigned identity preserves the maximum and is total
];

#[test]
fn executes_exact_success_and_failure_edges_for_every_conversion_class() {
    // Extend this existing native construction with wrapping's previously
    // unobservable out-of-domain integer values, including negative widening.
    // Its expected values use mathematical modulo, not host numeric casts.
    let (mut source, wrapping_observations) = wrapping_sources();
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n");
    let mut total_count = 0;
    let mut checked_count = 0;
    for (source_type, destination_type) in CONVERSION_CLASSES {
        if converts_totally(source_type, destination_type) {
            let value = total_value(source_type);
            writeln!(
                source,
                "  let total{total_count} = cvt::<{source_type}, {destination}>({value}_{source_type});\n  if total{total_count} == {value}_{destination} {{\n  }} else {{\n    return std::process::exit_status(code: 1_u8);\n  }}",
                destination = destination_type.spelling,
                source_type = source_type.spelling,
            )
            .expect("write total conversion");
            writeln!(
                source,
                "  if cvt.defined::<{source_type}, {destination}>({value}_{source_type}) {{\n  }} else {{\n    return std::process::exit_status(code: 2_u8);\n  }}\n  match cvt.checked::<{source_type}, {destination}>({value}_{source_type}) {{\n    Ok(value: checked_total{total_count}) => {{\n      if checked_total{total_count} == total{total_count} {{\n      }} else {{\n        return std::process::exit_status(code: 3_u8);\n      }}\n    }}\n    Err(error: refused_total{total_count}) => {{\n      return std::process::exit_status(code: 4_u8);\n    }}\n  }}",
                destination = destination_type.spelling,
                source_type = source_type.spelling,
            )
            .expect("write uniform total interfaces");
            total_count += 1;
            continue;
        }

        let failure = failing_value(source_type, destination_type);
        writeln!(
            source,
            "  let success{checked_count} = cvt.checked::<{source_type}, {destination}>(1_{source_type});\n  match success{checked_count} {{\n    Ok(value: success_value{checked_count}) => {{\n      if success_value{checked_count} == 1_{destination} {{\n      }} else {{\n        return std::process::exit_status(code: 1_u8);\n      }}\n    }}\n    Err(error: success_error{checked_count}) => {{\n      return std::process::exit_status(code: 1_u8);\n    }}\n  }}\n  let failure{checked_count} = cvt.checked::<{source_type}, {destination}>({failure}_{source_type});\n  match failure{checked_count} {{\n    Ok(value: failure_value{checked_count}) => {{\n      return std::process::exit_status(code: 1_u8);\n    }}\n    Err(error: failure_error{checked_count}) => {{\n      match failure_error{checked_count} {{\n        NarrowError() => {{\n        }}\n      }}\n    }}\n  }}",
            destination = destination_type.spelling,
            source_type = source_type.spelling,
        )
        .expect("write checked conversion");
        writeln!(
            source,
            "  if cvt.defined::<{source_type}, {destination}>(1_{source_type}) {{\n    let exact{checked_count} = cvt::<{source_type}, {destination}>(1_{source_type});\n    if exact{checked_count} == 1_{destination} {{\n    }} else {{\n      return std::process::exit_status(code: 5_u8);\n    }}\n  }} else {{\n    return std::process::exit_status(code: 6_u8);\n  }}\n  if cvt.defined::<{source_type}, {destination}>({failure}_{source_type}) {{\n    return std::process::exit_status(code: 7_u8);\n  }}",
            destination = destination_type.spelling,
            source_type = source_type.spelling,
        )
        .expect("write conversion domain agreement");
        checked_count += 1;
    }
    source.push_str(&wrapping_observations);
    source.push_str("  return std::process::exit_status(code: 0_u8);\n}\n");
    assert_eq!(total_count, 5);
    assert_eq!(checked_count, 12);

    let llvm = compile(source.as_bytes());
    for instruction in [
        " = sext ",
        " = zext ",
        " = trunc ",
        " = icmp sge ",
        " = icmp ule ",
    ] {
        assert!(
            llvm.contains(instruction),
            "conversion matrix must exercise {instruction}"
        );
    }
    assert!(!llvm.contains(" nsw "));
    assert!(!llvm.contains(" nuw "));
    assert_wrapping_ir(&llvm);
    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "integer conversion matrix failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

fn wrapping_sources() -> (String, String) {
    let mut helpers = String::new();
    let mut observations = String::new();
    let mut pair_count = 0;
    let mut observation_count = 0;
    for source in INTEGER_TYPES {
        let minimum = if source.signed {
            -(1_i128 << (source.width - 1))
        } else {
            0
        };
        let maximum = (1_i128 << (source.width - u8::from(source.signed))) - 1;
        let mut inputs = [minimum, minimum + 1, -1, 0, 1, maximum - 1, maximum]
            .into_iter()
            .filter(|value| (minimum..=maximum).contains(value))
            .collect::<Vec<_>>();
        inputs.sort_unstable();
        inputs.dedup();
        for destination in INTEGER_TYPES {
            let name = format!("wrap_{}_{}", source.spelling, destination.spelling);
            writeln!(
                helpers,
                "fn {name}(value: {source}) -> result: {destination} pure {{\n  return cvt.wrap::<{source}, {destination}>(value);\n}}\n",
                source = source.spelling,
                destination = destination.spelling,
            )
            .expect("write parameterized wrapping conversion");
            pair_count += 1;
            // Keep each pair's observations in a bounded body so checking
            // later pairs does not accumulate earlier pairs' value facts.
            writeln!(helpers, "fn check_{name}() -> valid: Bool pure {{")
                .expect("write wrapping observation helper");
            for &value in &inputs {
                let modulus = 1_i128 << destination.width;
                let residue = value.rem_euclid(modulus);
                let expected = if destination.signed && residue >= modulus / 2 {
                    residue - modulus
                } else {
                    residue
                };
                writeln!(
                    helpers,
                    "  let wrapped{observation_count} = {name}(value: {value}_{source});\n  if wrapped{observation_count} == {expected}_{destination} {{\n  }} else {{\n    return False();\n  }}",
                    source = source.spelling,
                    destination = destination.spelling,
                )
                .expect("write independently expected wrapping result");
                observation_count += 1;
            }
            helpers.push_str("  return True();\n}\n\n");
            writeln!(
                observations,
                "  if check_{name}() {{\n  }} else {{\n    return std::process::exit_status(code: 8_u8);\n  }}",
            )
            .expect("write wrapping helper observation");
        }
    }
    assert_eq!(pair_count, 64);
    assert_eq!(observation_count, 352);
    (helpers, observations)
}

fn assert_wrapping_ir(llvm: &str) {
    for source in INTEGER_TYPES {
        for destination in INTEGER_TYPES {
            let symbol = format!("@wf_wrap_{}_{}", source.spelling, destination.spelling);
            let body = super::parallel::function_body(llvm, &symbol);
            let instruction = if source.width > destination.width {
                format!(" = trunc i{} ", source.width)
            } else if source.width < destination.width {
                let opcode = if source.signed { "sext" } else { "zext" };
                format!(" = {opcode} i{} ", source.width)
            } else if source.signed == destination.signed {
                " = select i1 true, ".to_owned()
            } else {
                format!(" = or i{} ", source.width)
            };
            assert!(body.contains(&instruction), "{body}");
            for residual in ["icmp ", "fcmp ", "insertvalue ", "br i1", "@llvm.assume"] {
                assert!(!body.contains(residual), "unexpected {residual} in {body}");
            }
        }
    }
}

const fn converts_totally(source: IntegerType, destination: IntegerType) -> bool {
    (source.width == destination.width && source.signed == destination.signed)
        || (source.width < destination.width
            && (source.signed == destination.signed || (!source.signed && destination.signed)))
}

fn total_value(source: IntegerType) -> String {
    if source.signed {
        "-1".to_owned()
    } else {
        ((1_u128 << source.width) - 1).to_string()
    }
}

fn failing_value(source: IntegerType, destination: IntegerType) -> String {
    match (source.signed, destination.signed) {
        (true, false) => "-1".to_owned(),
        (true, true) | (false, true) => (1_u128 << (destination.width - 1)).to_string(),
        (false, false) => (1_u128 << destination.width).to_string(),
    }
}
