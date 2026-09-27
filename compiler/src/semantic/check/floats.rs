use super::super::model::{CheckedValue, FloatType};

const MAX_F32_DECIMAL_DIGITS: usize = 9;
const MAX_F64_DECIMAL_DIGITS: usize = 17;
// At nine significant digits, one binary32 rounding interval spans fewer
// than 120 adjacent decimal significands; its farthest edge is under 60
// steps from the represented value. Binary64's corresponding bound is lower.
// The interval only narrows at fewer digits, so this covers every shortest
// candidate while keeping canonicalization bounded.
const CANDIDATE_RADIUS: i128 = 64;

pub(super) fn parse_float_literal(bytes: &[u8]) -> Option<CheckedValue> {
    let (number, ty) = if let Some(number) = bytes.strip_suffix(b"_f32") {
        (number, FloatType::F32)
    } else {
        (bytes.strip_suffix(b"_f64")?, FloatType::F64)
    };
    let number = std::str::from_utf8(number).ok()?;
    let bits = match ty {
        FloatType::F32 => {
            let value = number.parse::<f32>().ok()?;
            if !value.is_finite() || canonical_f32(value)? != number {
                return None;
            }
            u64::from(value.to_bits())
        }
        FloatType::F64 => {
            let value = number.parse::<f64>().ok()?;
            if !value.is_finite() || canonical_f64(value)? != number {
                return None;
            }
            value.to_bits()
        }
    };
    Some(CheckedValue::Float { ty, bits })
}

/// One float value in the source spelling that denotes it [FORM-5, OP-1].
///
/// A finite value is its unique canonical literal, suffix included, which is
/// the spelling [FORM-7] admits for it. No literal denotes a non-finite value
/// [FORM-5], so those render as the operation producing them: `finf` and its
/// `fneg`, `fnan` for the canonical quiet NaN, and the `reinterpret` of the
/// bits for any other NaN payload [OP-8].
pub(in crate::semantic) fn float_value_spelling(ty: FloatType, bits: u64) -> String {
    let (suffix, carrier, finite, infinite, negative, canonical_nan) = match ty {
        FloatType::F32 => {
            let value = f32::from_bits(u32::try_from(bits).unwrap_or(u32::MAX));
            (
                "f32",
                "u32",
                value.is_finite().then(|| canonical_f32(value)).flatten(),
                value.is_infinite(),
                value.is_sign_negative(),
                bits == 0x7fc0_0000,
            )
        }
        FloatType::F64 => {
            let value = f64::from_bits(bits);
            (
                "f64",
                "u64",
                value.is_finite().then(|| canonical_f64(value)).flatten(),
                value.is_infinite(),
                value.is_sign_negative(),
                bits == 0x7ff8_0000_0000_0000,
            )
        }
    };
    match finite {
        Some(literal) => format!("{literal}_{suffix}"),
        None if infinite && negative => format!("fneg(finf::<{suffix}>())"),
        None if infinite => format!("finf::<{suffix}>()"),
        None if canonical_nan => format!("fnan::<{suffix}>()"),
        None => format!("reinterpret::<{carrier}, {suffix}>({bits}_{carrier})"),
    }
}

fn canonical_f32(value: f32) -> Option<String> {
    canonical_float(
        value.is_sign_negative(),
        value == 0.0,
        &format!("{:e}", value.abs()),
        MAX_F32_DECIMAL_DIGITS,
        |candidate| {
            candidate
                .parse::<f32>()
                .is_ok_and(|parsed| parsed.to_bits() == value.abs().to_bits())
        },
    )
}

fn canonical_f64(value: f64) -> Option<String> {
    canonical_float(
        value.is_sign_negative(),
        value == 0.0,
        &format!("{:e}", value.abs()),
        MAX_F64_DECIMAL_DIGITS,
        |candidate| {
            candidate
                .parse::<f64>()
                .is_ok_and(|parsed| parsed.to_bits() == value.abs().to_bits())
        },
    )
}

fn canonical_float(
    negative: bool,
    zero: bool,
    shortest: &str,
    maximum_digits: usize,
    matches_value: impl Fn(&str) -> bool,
) -> Option<String> {
    if zero {
        return Some(if negative { "-0.0" } else { "0.0" }.to_owned());
    }
    let (base_significand, base_digits, scientific_exponent) = scientific_components(shortest)?;
    let mut best = None;
    for digits in 1..=maximum_digits {
        let center = rescale_significand(base_significand, base_digits, digits)?;
        let decimal_exponent = scientific_exponent - i32::try_from(digits).ok()? + 1;
        for delta in -CANDIDATE_RADIUS..=CANDIDATE_RADIUS {
            let candidate = center.checked_add(delta)?;
            let minimum = 10_i128.checked_pow(u32::try_from(digits - 1).ok()?)?;
            let maximum = 10_i128.checked_pow(u32::try_from(digits).ok()?)?;
            if candidate < minimum || candidate >= maximum {
                continue;
            }
            let significand = u64::try_from(candidate).ok()?;
            let scientific = scientific_spelling(significand, decimal_exponent)?;
            if !matches_value(&scientific) {
                continue;
            }
            for spelling in equivalent_spellings(significand, decimal_exponent, negative)? {
                if best.as_ref().is_none_or(|current: &String| {
                    (spelling.len(), spelling.as_bytes()) < (current.len(), current.as_bytes())
                }) {
                    best = Some(spelling);
                }
            }
        }
    }
    best
}

fn scientific_components(rendered: &str) -> Option<(u64, usize, i32)> {
    let (mantissa, exponent) = rendered.split_once('e')?;
    let digits = mantissa.replace('.', "");
    Some((digits.parse().ok()?, digits.len(), exponent.parse().ok()?))
}

fn rescale_significand(value: u64, from: usize, to: usize) -> Option<i128> {
    let value = i128::from(value);
    if to >= from {
        value.checked_mul(10_i128.checked_pow(u32::try_from(to - from).ok()?)?)
    } else {
        let divisor = 10_i128.checked_pow(u32::try_from(from - to).ok()?)?;
        let quotient = value / divisor;
        let remainder = value % divisor;
        Some(quotient + i128::from(remainder.saturating_mul(2) >= divisor))
    }
}

fn scientific_spelling(significand: u64, decimal_exponent: i32) -> Option<String> {
    let digits = significand.to_string();
    let exponent = decimal_exponent.checked_add(i32::try_from(digits.len()).ok()?)? - 1;
    let mut spelling = String::new();
    spelling.push(*digits.as_bytes().first()? as char);
    spelling.push('.');
    if digits.len() == 1 {
        spelling.push('0');
    } else {
        spelling.push_str(&digits[1..]);
    }
    if exponent != 0 {
        spelling.push('e');
        spelling.push_str(&exponent.to_string());
    }
    Some(spelling)
}

fn equivalent_spellings(
    significand: u64,
    decimal_exponent: i32,
    negative: bool,
) -> Option<Vec<String>> {
    let digits = significand.to_string();
    let mut spellings = Vec::with_capacity(digits.len() + 1);
    spellings.push(with_sign(
        fixed_spelling(&digits, decimal_exponent)?,
        negative,
    ));
    for point in 1..=digits.len() {
        let exponent = decimal_exponent.checked_add(i32::try_from(digits.len() - point).ok()?)?;
        if exponent == 0 {
            continue;
        }
        let mut spelling = String::new();
        spelling.push_str(&digits[..point]);
        spelling.push('.');
        if point == digits.len() {
            spelling.push('0');
        } else {
            spelling.push_str(&digits[point..]);
        }
        spelling.push('e');
        spelling.push_str(&exponent.to_string());
        spellings.push(with_sign(spelling, negative));
    }
    Some(spellings)
}

fn fixed_spelling(digits: &str, decimal_exponent: i32) -> Option<String> {
    if decimal_exponent >= 0 {
        let mut spelling = digits.to_owned();
        spelling.extend(std::iter::repeat_n(
            '0',
            usize::try_from(decimal_exponent).ok()?,
        ));
        spelling.push_str(".0");
        return Some(spelling);
    }
    let point = i32::try_from(digits.len())
        .ok()?
        .checked_add(decimal_exponent)?;
    if point > 0 {
        let point = usize::try_from(point).ok()?;
        return Some(format!("{}.{}", digits.get(..point)?, digits.get(point..)?));
    }
    let mut spelling = "0.".to_owned();
    spelling.extend(std::iter::repeat_n(
        '0',
        usize::try_from(point.checked_neg()?).ok()?,
    ));
    spelling.push_str(digits);
    Some(spelling)
}

fn with_sign(spelling: String, negative: bool) -> String {
    if negative {
        format!("-{spelling}")
    } else {
        spelling
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FloatType, canonical_f32, canonical_f64, float_value_spelling, parse_float_literal,
    };

    #[test]
    fn canonical_float_spelling_uses_the_shortest_grammar_form() {
        for spelling in [
            b"0.0_f32".as_slice(),
            b"-0.0_f64",
            b"1.5_f32",
            b"10.0_f64",
            b"1.0e2_f64",
            b"0.0001_f64",
            b"1.0e-5_f64",
            b"6.022e23_f64",
        ] {
            assert!(
                parse_float_literal(spelling).is_some(),
                "{}",
                String::from_utf8_lossy(spelling)
            );
        }
        for spelling in [
            b"0.00_f32".as_slice(),
            b"-0.0e2_f64",
            b"1.50_f32",
            b"100.0_f64",
            b"0.00001_f64",
            b"6.0220e23_f64",
            b"1.0e999_f64",
        ] {
            assert!(
                parse_float_literal(spelling).is_none(),
                "{}",
                String::from_utf8_lossy(spelling)
            );
        }
    }

    #[test]
    fn lexicographic_ties_are_not_delegated_to_the_host_formatter() {
        assert_eq!(canonical_f32(f32::MAX).as_deref(), Some("3.4028234e38"));
        assert_eq!(
            canonical_f32(f32::MIN_POSITIVE).as_deref(),
            Some("1.1754943e-38")
        );
        assert_eq!(canonical_f32(f32::from_bits(1)).as_deref(), Some("1.0e-45"));
        assert_eq!(
            canonical_f64(f64::MAX).as_deref(),
            Some("1.7976931348623157e308")
        );
        assert_eq!(
            canonical_f64(f64::MIN_POSITIVE).as_deref(),
            Some("2.2250738585072012e-308")
        );
        assert_eq!(
            canonical_f64(f64::from_bits(1)).as_deref(),
            Some("2.5e-324")
        );
    }

    /// [FORM-5] a finite value renders as the one literal the lexer admits
    /// for it, which a diagnostic once rendered as its bit pattern; a value no
    /// literal denotes renders as the operation producing it [OP-1, OP-8].
    #[test]
    fn a_float_value_renders_as_its_canonical_source_spelling() {
        for (ty, bits, spelling) in [
            (FloatType::F64, 1.0_f64.to_bits(), "1.0_f64"),
            (FloatType::F64, (-0.0_f64).to_bits(), "-0.0_f64"),
            (FloatType::F64, 6.022e23_f64.to_bits(), "6.022e23_f64"),
            (FloatType::F32, u64::from(1.5_f32.to_bits()), "1.5_f32"),
            (FloatType::F32, u64::from(0.1_f32.to_bits()), "0.1_f32"),
            (FloatType::F64, f64::INFINITY.to_bits(), "finf::<f64>()"),
            (
                FloatType::F32,
                u64::from(f32::NEG_INFINITY.to_bits()),
                "fneg(finf::<f32>())",
            ),
            (FloatType::F64, 0x7ff8_0000_0000_0000, "fnan::<f64>()"),
            (
                FloatType::F64,
                0x7ff8_0000_0000_0001,
                "reinterpret::<u64, f64>(9221120237041090561_u64)",
            ),
        ] {
            assert_eq!(float_value_spelling(ty, bits), spelling);
            if spelling.ends_with("_f64") || spelling.ends_with("_f32") {
                assert!(
                    parse_float_literal(spelling.as_bytes()).is_some(),
                    "{spelling} must be the admitted literal"
                );
            }
        }
    }
}
