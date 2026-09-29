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

/// The [OP-6] rounded value R, then C's special rows: the nearest candidate,
/// ties to the even encoding, with `2^(E+1)` standing for the signed infinity
/// and a rounded zero keeping the input's sign. Integer arithmetic only.
fn rounded_bits(value: BinaryValue, width: u8) -> u64 {
    let (precision, _, minimum, maximum) = format(width);
    let BinaryValue::Finite {
        negative,
        significand,
        exponent,
    } = value
    else {
        return float_bits(value, width).expect("infinity and NaN have one encoding");
    };
    let sign = u64::from(negative) << (width - 1);
    if significand == 0 {
        return sign;
    }
    let highest = exponent + (64 - significand.leading_zeros()) as i32 - 1;
    let mut quantum = (highest - (precision as i32 - 1)).max(minimum);
    let mut units = if exponent >= quantum {
        significand << (exponent - quantum)
    } else {
        let shift = (quantum - exponent) as u32;
        let kept = significand.checked_shr(shift).unwrap_or(0);
        let dropped = if shift >= 64 {
            significand
        } else {
            significand & ((1_u64 << shift) - 1)
        };
        // Compare the dropped part with half a unit without forming 2^shift.
        let half = 1_u128 << (shift - 1).min(100);
        let dropped = u128::from(dropped);
        if dropped > half || (dropped == half && kept % 2 == 1) {
            kept + 1
        } else {
            kept
        }
    };
    if units == 1 << precision {
        units >>= 1;
        quantum += 1;
    }
    if units == 0 {
        return sign;
    }
    if quantum + (64 - units.leading_zeros()) as i32 - 1 > maximum {
        return float_bits(BinaryValue::Infinity(negative), width).expect("infinity encodes");
    }
    float_bits(
        BinaryValue::Finite {
            negative,
            significand: units,
            exponent: quantum,
        },
        width,
    )
    .expect("a rounded finite value is representable")
}

/// Inputs that fall between destination values: halfway points with an even
/// and an odd lower neighbour, the overflow tie, and the subnormal and
/// underflow ties, together with their one-ulp neighbours.
fn rounding_float_samples(source: NumericType, destination: NumericType) -> Vec<u64> {
    let mut bits = float_samples(source, destination);
    if source.width == 64 && destination.width == 32 {
        let one = power_bits(0, 64);
        let (precision, _, _, _) = format(64);
        let half_unit = 1_u64 << (precision - 1 - 24);
        for halfway in [one + half_unit, one + 3 * half_unit] {
            bits.extend([halfway - 1, halfway, halfway + 1]);
        }
        let overflow_tie = float_bits(
            BinaryValue::Finite {
                negative: false,
                significand: (1 << 25) - 1,
                exponent: 103,
            },
            64,
        )
        .expect("the f32 overflow tie is a binary64 value");
        bits.extend([overflow_tie - 1, overflow_tie, overflow_tie + 1]);
        for (significand, exponent) in [(1, -150), (3, -150), (3, -151)] {
            let tie = float_bits(
                BinaryValue::Finite {
                    negative: false,
                    significand,
                    exponent,
                },
                64,
            )
            .expect("subnormal-range ties are binary64 values");
            let sign = 1 << 63;
            bits.extend([tie - 1, tie, tie + 1, tie | sign]);
        }
    }
    bits
}

fn rounding_integer_samples(source: NumericType, destination: NumericType) -> Vec<i128> {
    let signed = source.kind == NumericKind::SignedInteger;
    let minimum = if signed {
        -(1_i128 << (source.width - 1))
    } else {
        0
    };
    let maximum = (1_i128 << (source.width - u8::from(signed))) - 1;
    let precision = if destination.width == 32 { 24 } else { 53 };
    let threshold = 1_i128 << precision;
    let mut values = integer_samples(source, destination);
    // An odd tie, and a value that rounds wrongly through binary64 first.
    values.extend([threshold + 3, -threshold - 3, (1 << 60) + (1 << 36) + 1]);
    values.retain(|value| (minimum..=maximum).contains(value));
    values
}

/// A native program comparing every float-destination `cvt.nearest` pair
/// with `rounded_bits`, and checking that the oracle equals the exact value
/// wherever the exact domain holds.
pub(super) fn nearest_program() -> String {
    let mut helpers = String::new();
    let mut checks = String::new();
    let mut pairs = 0;
    for source in NUMERIC_TYPES {
        for destination in NUMERIC_TYPES {
            if destination.kind != NumericKind::Float {
                continue;
            }
            pairs += 1;
            let name = format!("nearest_{}_{}", source.spelling, destination.spelling);
            let expected_type = format!("u{}", destination.width);
            let (input_type, input) = if source.kind == NumericKind::Float {
                (
                    format!("u{}", source.width),
                    format!(
                        "reinterpret::<u{}, {}>(input)",
                        source.width, source.spelling
                    ),
                )
            } else {
                (source.spelling.to_owned(), "input".to_owned())
            };
            writeln!(
                helpers,
                "fn {name}(input: {input_type}, expected: {expected_type}) -> result: Bool pure {{\n  let value = {input};\n  let rounded = cvt.nearest::<{source}, {destination}>(value);\n  let observed = reinterpret::<{destination}, {expected_type}>(rounded);\n  return observed == expected;\n}}\n",
                source = source.spelling,
                destination = destination.spelling,
            )
            .expect("write rounding helper");
            let observations: Vec<(i128, u64)> = if source.kind == NumericKind::Float {
                rounding_float_samples(source, destination)
                    .into_iter()
                    .map(|bits| {
                        let value = decode(bits, source.width);
                        if source.spelling == destination.spelling {
                            return (i128::from(bits), bits);
                        }
                        let expected = rounded_bits(value, destination.width);
                        if let Some(exact) = float_bits(value, destination.width) {
                            assert_eq!(expected, exact, "R agrees with C on D: {bits:#x}");
                        }
                        (i128::from(bits), expected)
                    })
                    .collect()
            } else {
                rounding_integer_samples(source, destination)
                    .into_iter()
                    .map(|value| {
                        let finite = BinaryValue::Finite {
                            negative: value < 0,
                            significand: u64::try_from(value.unsigned_abs())
                                .expect("all integer magnitudes fit u64"),
                            exponent: 0,
                        };
                        let expected = rounded_bits(finite, destination.width);
                        if let Some(exact) = float_bits(finite, destination.width) {
                            assert_eq!(expected, exact, "R agrees with C on D: {value}");
                        }
                        (value, expected)
                    })
                    .collect()
            };
            for (input, expected) in observations {
                writeln!(
                    checks,
                    "  if {name}(input: {input}_{input_type}, expected: {expected}_{expected_type}) {{\n  }} else {{\n    return std::process::exit_status(code: 30_u8);\n  }}",
                )
                .expect("write independently expected rounding observation");
            }
        }
    }
    assert_eq!(pairs, 20, "OP-6 admits cvt.nearest for exactly 20 pairs");
    format!(
        "{helpers}fn main() -> status: std::process::ExitStatus pure {{\n{checks}  return std::process::exit_status(code: 0_u8);\n}}\n"
    )
}

#[test]
fn the_rounding_oracle_matches_known_binary_values() {
    let one = power_bits(0, 64);
    let half_unit = 1_u64 << 28;
    for (input, expected) in [
        (one + half_unit, 0x3f80_0000),
        (one + 3 * half_unit, 0x3f80_0002),
        (one + half_unit + 1, 0x3f80_0001),
        (0x47ef_ffff_f000_0000, 0x7f80_0000),
        (0x47ef_ffff_efff_ffff, 0x7f7f_ffff),
        (0x3690_0000_0000_0000, 0),
        (0xb690_0000_0000_0000, 0x8000_0000),
        (0x36a8_0000_0000_0000, 2),
        (0x7fef_ffff_ffff_ffff, 0x7f80_0000),
    ] {
        assert_eq!(rounded_bits(decode(input, 64), 32), expected, "{input:#x}");
    }
    let integer = |value: u64| BinaryValue::Finite {
        negative: false,
        significand: value,
        exponent: 0,
    };
    assert_eq!(
        rounded_bits(integer((1 << 53) + 1), 64),
        0x4340_0000_0000_0000
    );
    assert_eq!(rounded_bits(integer(u64::MAX), 64), 0x43f0_0000_0000_0000);
    assert_eq!(
        rounded_bits(integer((1 << 60) + (1 << 36) + 1), 32),
        0x5d80_0001
    );
}
