//! Native conversion boundaries use an integer-only binary-value reference.
//! This test-owned oracle checks domain and result bits independently of the
//! emitter's floating round trips; it has no research or host-cast dependency.

use std::fmt::Write;

use super::{NUMERIC_TYPES, NumericKind, NumericType};

#[derive(Clone, Copy)]
enum BinaryValue {
    Finite {
        negative: bool,
        significand: u64,
        exponent: i32,
    },
    Infinity(bool),
    Nan,
}

fn format(width: u8) -> (u32, i32, i32, i32) {
    match width {
        32 => (24, 127, -149, 127),
        64 => (53, 1023, -1074, 1023),
        _ => panic!("only binary32 and binary64 are numeric float types"),
    }
}

fn decode(bits: u64, width: u8) -> BinaryValue {
    let (precision, bias, minimum, _) = format(width);
    let negative = bits >> (width - 1) != 0;
    let fraction_mask = (1_u64 << (precision - 1)) - 1;
    let fraction = bits & fraction_mask;
    let exponent = ((bits >> (precision - 1)) & (2 * bias + 1) as u64) as i32;
    if exponent == 2 * bias + 1 {
        if fraction == 0 {
            BinaryValue::Infinity(negative)
        } else {
            BinaryValue::Nan
        }
    } else {
        BinaryValue::Finite {
            negative,
            significand: fraction
                | if exponent == 0 {
                    0
                } else {
                    1 << (precision - 1)
                },
            exponent: if exponent == 0 {
                minimum
            } else {
                exponent - bias - (precision - 1) as i32
            },
        }
    }
}

fn integer_value(value: BinaryValue, destination: NumericType) -> Option<i128> {
    let BinaryValue::Finite {
        negative,
        mut significand,
        mut exponent,
    } = value
    else {
        return None;
    };
    if significand == 0 {
        return Some(0);
    }
    let zeros = significand.trailing_zeros();
    significand >>= zeros;
    exponent += zeros as i32;
    if exponent < 0 || 64 - significand.leading_zeros() + exponent as u32 > 64 {
        return None;
    }
    let magnitude = i128::from(significand) << exponent;
    let signed = destination.kind == NumericKind::SignedInteger;
    if negative {
        (signed && magnitude <= 1_i128 << (destination.width - 1)).then_some(-magnitude)
    } else {
        (magnitude < 1_i128 << (destination.width - u8::from(signed))).then_some(magnitude)
    }
}

fn float_bits(value: BinaryValue, width: u8) -> Option<u64> {
    let (precision, bias, minimum, maximum) = format(width);
    let (negative, mut significand, mut exponent) = match value {
        BinaryValue::Finite {
            negative,
            significand,
            exponent,
        } => (negative, significand, exponent),
        BinaryValue::Infinity(negative) => {
            return Some(
                (u64::from(negative) << (width - 1)) | ((2 * bias + 1) as u64) << (precision - 1),
            );
        }
        BinaryValue::Nan => {
            return Some(((2 * bias + 1) as u64) << (precision - 1) | 1 << (precision - 2));
        }
    };
    let sign = u64::from(negative) << (width - 1);
    if significand == 0 {
        return Some(sign);
    }
    let zeros = significand.trailing_zeros();
    significand >>= zeros;
    exponent += zeros as i32;
    let significant_bits = 64 - significand.leading_zeros();
    let highest = exponent + significant_bits as i32 - 1;
    if significant_bits > precision || exponent < minimum || highest > maximum {
        return None;
    }
    let encoded = if highest < 1 - bias {
        significand << (exponent - minimum)
    } else {
        let fraction =
            (significand << (precision - significant_bits)) & ((1_u64 << (precision - 1)) - 1);
        ((highest + bias) as u64) << (precision - 1) | fraction
    };
    Some(sign | encoded)
}

fn power_bits(exponent: i32, width: u8) -> u64 {
    float_bits(
        BinaryValue::Finite {
            negative: false,
            significand: 1,
            exponent,
        },
        width,
    )
    .expect("the boundary power is representable in the input format")
}

fn special_bits(width: u8) -> Vec<u64> {
    let (precision, bias, _, _) = format(width);
    let sign = 1 << (width - 1);
    let infinity = ((2 * bias + 1) as u64) << (precision - 1);
    vec![
        0,
        sign,
        infinity,
        infinity | sign,
        infinity | 1, // signaling NaN with payload, both signs
        infinity | sign | 1,
        infinity | (1 << (precision - 2)) | 123,
        infinity | sign | (1 << (precision - 2)) | 123,
    ]
}

fn float_samples(source: NumericType, destination: NumericType) -> Vec<u64> {
    let mut bits = special_bits(source.width);
    if destination.kind == NumericKind::Float {
        for exponent in [-149, -126, 0, 127] {
            let power = power_bits(exponent, source.width);
            bits.extend([power - 1, power, power + 1]);
        }
        bits.push(1); // the input's least subnormal
        let (precision, bias, _, _) = format(source.width);
        bits.push((((2 * bias + 1) as u64) << (precision - 1)) - 1);
        if source.width == 64 {
            bits.extend([power_bits(-150, 64), power_bits(128, 64)]);
        }
    } else {
        let exponent = i32::from(destination.width)
            - i32::from(destination.kind == NumericKind::SignedInteger);
        let upper = power_bits(exponent, source.width);
        let sign = 1 << (source.width - 1);
        bits.extend([
            power_bits(-1, source.width),
            power_bits(-1, source.width) | sign,
            power_bits(0, source.width),
            power_bits(0, source.width) | sign,
            upper - 1,
            upper,
            upper + 1,
            (upper - 1) | sign,
            upper | sign,
            (upper + 1) | sign,
        ]);
    }
    bits
}

fn integer_samples(source: NumericType, destination: NumericType) -> Vec<i128> {
    let signed = source.kind == NumericKind::SignedInteger;
    let precision = if destination.width == 32 { 24 } else { 53 };
    let minimum = if signed {
        -(1_i128 << (source.width - 1))
    } else {
        0
    };
    let maximum = (1_i128 << (source.width - u8::from(signed))) - 1;
    let threshold = 1_i128 << precision;
    [
        minimum,
        minimum + 1,
        -1,
        0,
        1,
        threshold - 1,
        threshold,
        threshold + 1,
        threshold + 2,
        -threshold - 1,
        -threshold,
        maximum - 1,
        maximum,
    ]
    .into_iter()
    .filter(|value| (minimum..=maximum).contains(value))
    .collect()
}

/// Append helpers and native observations to the existing boundary program,
/// preserving its single compiler/native construction. Expected values below
/// are encoded from the binary relation, never cast by Rust or the host C ABI.
pub(super) fn extend_program(original: &str) -> String {
    let mut helpers = String::from(
        "fn copied_float<T: Float>(value: T) -> result: T pure {\n  return cvt::<T, T>(value);\n}\n\n",
    );
    let mut checks =
        String::from("  let expected_true = True();\n  let expected_false = False();\n");
    for source in NUMERIC_TYPES {
        for destination in NUMERIC_TYPES {
            if source.kind != NumericKind::Float && destination.kind != NumericKind::Float {
                continue;
            }
            let name = format!("boundary_{}_{}", source.spelling, destination.spelling);
            let input_type = if source.kind == NumericKind::Float {
                format!("u{}", source.width)
            } else {
                source.spelling.to_owned()
            };
            let expected_type = if destination.kind == NumericKind::Float {
                format!("u{}", destination.width)
            } else {
                destination.spelling.to_owned()
            };
            let input = if source.kind == NumericKind::Float {
                format!("reinterpret::<{input_type}, {}>(input)", source.spelling)
            } else {
                "input".to_owned()
            };
            let actual = if destination.kind == NumericKind::Float {
                format!(
                    "reinterpret::<{}, {expected_type}>(actual)",
                    destination.spelling
                )
            } else {
                "actual".to_owned()
            };
            let exact_observation = actual.replace("actual", "converted_exact");
            let identity = if source.spelling == destination.spelling {
                format!(
                    "  let copied = copied_float::<{source}>(value: value);\n  let copied_bits = reinterpret::<{source}, {expected_type}>(copied);\n  if copied_bits == expected {{\n  }} else {{\n    return False();\n  }}\n",
                    source = source.spelling,
                )
            } else {
                String::new()
            };
            writeln!(
                helpers,
                "fn {name}(input: {input_type}, expected: {expected_type}, wanted: Bool) -> result: Bool pure {{\n  let value = {input};\n{identity}  let permitted = cvt.defined::<{source}, {destination}>(value);\n  if permitted {{\n    if wanted {{\n    }} else {{\n      return False();\n    }}\n    let converted_exact = cvt::<{source}, {destination}>(value);\n    let exact_observed = {exact_observation};\n    if exact_observed == expected {{\n    }} else {{\n      return False();\n    }}\n  }} else if wanted {{\n    return False();\n  }}\n  match cvt.checked::<{source}, {destination}>(value) {{\n    Ok(value: actual) => {{\n      if wanted {{\n        let observed = {actual};\n        return observed == expected;\n      }} else {{\n        return False();\n      }}\n    }}\n    Err(error: refused) => {{\n      if wanted {{\n        return False();\n      }} else {{\n        return True();\n      }}\n    }}\n  }}\n}}\n",
                source = source.spelling,
                destination = destination.spelling,
            )
            .expect("write numeric boundary helper");
            let observations = if source.kind == NumericKind::Float {
                float_samples(source, destination)
                    .into_iter()
                    .map(|bits| {
                        let value = decode(bits, source.width);
                        let expected = if source.spelling == destination.spelling {
                            Some(i128::from(bits))
                        } else if destination.kind == NumericKind::Float {
                            float_bits(value, destination.width).map(i128::from)
                        } else {
                            integer_value(value, destination)
                        };
                        (i128::from(bits), expected)
                    })
                    .collect::<Vec<_>>()
            } else {
                integer_samples(source, destination)
                    .into_iter()
                    .map(|value| {
                        let expected = float_bits(
                            BinaryValue::Finite {
                                negative: value < 0,
                                significand: u64::try_from(value.unsigned_abs())
                                    .expect("all integer magnitudes fit u64"),
                                exponent: 0,
                            },
                            destination.width,
                        );
                        (value, expected.map(i128::from))
                    })
                    .collect()
            };
            for (input, expected) in observations {
                writeln!(
                    checks,
                    "  if {name}(input: {input}_{input_type}, expected: {expected}_{expected_type}, wanted: {wanted}) {{\n  }} else {{\n    return std::process::exit_status(code: 20_u8);\n  }}",
                    wanted = if expected.is_some() { "expected_true" } else { "expected_false" },
                    expected = expected.unwrap_or(0),
                )
                .expect("write independently expected boundary observation");
            }
        }
    }
    let main = original
        .strip_suffix("  return std::process::exit_status(code: 0_u8);\n}\n")
        .expect("the existing boundary program ends with its success status");
    format!("{helpers}{main}{checks}  return std::process::exit_status(code: 0_u8);\n}}\n")
}
