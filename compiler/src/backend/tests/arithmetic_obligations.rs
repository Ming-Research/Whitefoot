//! Emission of exact arithmetic [OP-2, ENT-6]: an accepted site has a proof of
//! its integer domain and compiles to the plain exact operation with no
//! overflow branch. The shipped emission and the forced-on entry must agree
//! exactly.

use super::{emit, emit_arithmetic_obligations};

/// The normalizer publishes its verified result bound. The caller consumes
/// that exact summary directly to discharge the addition; the final value
/// check is an ordinary test oracle.
const PROVED_EXACT: &[u8] = br#"fn clamp_below_thousand(value: u64) -> result: u64 pure contract {
  ensures result < 1000_u64;
} {
  if value < 1000_u64 {
    return value;
  } else {
    return 999_u64;
  }
}

fn increment(x: u64) -> result: u64 pure {
  let bounded = clamp_below_thousand(value: x);
  let stepped = bounded + 1_u64;
  return stepped;
}

fn wrapping_increment(x: u64) -> result: u64 pure {
  let stepped = x +wrap 1_u64;
  return stepped;
}

fn main() -> status: std::process::ExitStatus pure {
  let total = increment(x: 6_u64);
  if total != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let around = wrapping_increment(x: 6_u64);
  if around != 7_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

fn overflow_call_count(module: &str) -> usize {
    module
        .lines()
        .filter(|line| line.contains("call") && line.contains(".with.overflow."))
        .count()
}

/// The proved exact site is one `add` and has no overflow carrier. The
/// shipped emission and the forced-on entry are byte-identical: there is one
/// acceptance and lowering path, not a switchable pair.
///
/// The `add` carries `nuw`, and the `+wrap` beside it carries nothing.
/// [DIAG-2]: "every fact the checker has proved may be supplied to the
/// backend, as target attributes, instruction flags, metadata, or
/// assumptions: ... and a discharged integer-domain obligation [OP-2], the
/// last being what licenses a no-wrap flag on an exact operation", bounded by
/// the same sentence's "only a fact the checker has actually discharged may be
/// supplied, never one a writer states". `+` is [OP-2]'s exact family and
/// carries that obligation; `+wrap` is total and carries none, so there is
/// nothing to supply at the second site. The flag is a statement about the
/// same one instruction, not a second branch.
#[test]
fn a_proved_exact_site_emits_no_overflow_branch() {
    let shipped = emit(PROVED_EXACT);
    assert_eq!(
        overflow_call_count(&shipped),
        0,
        "an accepted exact add has no runtime overflow branch",
    );
    let flagged = shipped
        .lines()
        .filter(|line| line.trim_start().starts_with('%') && line.contains("= add nuw i64"))
        .count();
    assert_eq!(
        flagged, 1,
        "the discharged site compiles to one exact add carrying its proved domain",
    );
    let wrapping = super::emitted_function(&shipped, "wrapping_increment");
    assert!(
        wrapping.contains("= add i64"),
        "the wrap-mode site is a plain add: {wrapping}"
    );
    assert!(
        !wrapping.contains(" nuw ") && !wrapping.contains(" nsw "),
        "and carries no no-wrap flag, because nothing was discharged there: {wrapping}"
    );
    assert_eq!(
        shipped,
        emit_arithmetic_obligations(PROVED_EXACT),
        "the shipped path and the forced-on entry are one judgment",
    );
}

/// Fallthrough and continue share one exact compiler-owned range update,
/// whether the immutable binder is a scalar or has storage for a shared
/// borrow. The source accumulation remains wrapping in both bodies.
#[test]
fn counted_range_updates_carry_no_wrap_without_strengthening_source_wrap() {
    let source = br#"fn scalar_range(lower: u64, upper: u64, skip: u64) -> result: u64 pure {
  let total = 0_u64;
  for (i in lower..upper) {
    if i == skip {
      continue;
    }
    set total = total +wrap i;
  }
  return total;
}

fn addressed_range(lower: u64, upper: u64, skip: u64) -> result: u64 pure {
  let total = 0_u64;
  for (i in lower..upper) {
    let observed = &i;
    let seen = observed^;
    if seen == skip {
      continue;
    }
    set total = total +wrap seen;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let scalar = scalar_range(lower: 0_u64, upper: 4_u64, skip: 1_u64);
  let addressed = addressed_range(lower: 0_u64, upper: 4_u64, skip: 1_u64);
  if scalar != addressed {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit(source);
    for name in ["scalar_range", "addressed_range"] {
        let function = super::emitted_function(&module, name);
        if name == "addressed_range" {
            assert!(
                function.contains("load i64, ptr"),
                "the shared borrow must exercise the addressed binder: {function}",
            );
        }
        assert_eq!(
            function.matches("= add nuw i64").count(),
            1,
            "{name}: the shared counted update carries its guarded domain: {function}",
        );
        assert_eq!(
            function.matches("= add i64").count(),
            1,
            "{name}: the user-written +wrap remains a plain add: {function}",
        );
        assert!(
            !function.contains(" nsw "),
            "{name}: neither unsigned update nor +wrap claims signed no-wrap: {function}",
        );
        assert_eq!(
            overflow_call_count(function),
            0,
            "{name}: publishing the update's proof adds no overflow check: {function}",
        );
    }
}
