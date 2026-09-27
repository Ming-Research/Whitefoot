use crate::{
    SemanticIssueKind, SemanticOutcome, SemanticRule, UnsupportedSemanticFeature, lower_checked,
    lowering::OverlapLowering,
};

use super::super::model::{
    CheckedConst, CheckedNominalKind, CheckedType, IntegerType, WindowShape,
};
use super::{assert_rule, assert_rule_kind, assert_unsupported, with_semantics};

// Retired with the region parameter of [FORM-8] and the arena of [STOR-4]:
// v0.59's `brand_parameters_do_not_change_single_loan_spelling_or_legacy_
// arena_support` refused `&'r u64` at FORM-8's canonical region spelling and
// stopped `arena<u64>` as the unimplemented `RegionsAndBorrows` capability.
// v0.60 has no region, no loan and no arena: a reference is written `&T` and
// carries nothing [REF-1], so neither spelling exists to judge.

// Retired with the nominal region default of [PROV-1, FORM-8]: v0.59's
// `brand_parameters_do_not_leak_nominal_defaults_into_nested_declarations`
// asked which store a nested nominal's `Vector` field took its brand from.
// v0.60 has one heap [STOR-8], `Box<T>` carries no brand, and a nominal has
// no region parameter to leak.

// Retired with the region argument of [FORM-8, TYPE-5]: v0.59's
// `brand_parameters_reject_different_actuals_inside_one_operand` compared two
// region actuals written inside one operand type. v0.60 has no region
// argument; the exact type agreement it rode on survives in
// `source_nominal_argument_arity_and_kinds_are_exact` below.

#[test]
fn explicit_type_arguments_keep_their_written_order_and_kind() {
    let source = br#"struct Mark {
  value: u64;
}

struct Wrap<T: drop> {
  payload: T;
}

fn pack<T: drop>(value: T) -> result: Wrap<T> pure {
  return Wrap<T>(payload: move value);
}

fn main() -> status: std::process::ExitStatus pure {
  let value = Mark(value: 9_u64);
  let wrapped = pack::<Mark>(value: value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        )
    });
    let trailing = std::str::from_utf8(source)
        .unwrap()
        .replace("pack::<Mark>", "pack::<Mark, 2>");
    assert_rule_kind(trailing.as_bytes(), SemanticRule::Fn2, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

// Retired with the loan lifetime of [OWN-4]: v0.59's
// `brand_parameters_cannot_be_shortened_by_a_mode_loan_in_either_order`
// refused a `&'s u64` argument whose loan was shorter than the region an
// `own Mark<'s>` operand fixed, citing `InvalidBorrowLifetime`. v0.60 has no
// loan and no region: reference validity is a fact [REF-2] and interference
// is decided by the effect rows at the call [EFF-5].

#[test]
fn a_generic_cycle_judgment_reads_the_complete_argument_vector() {
    let source = br#"struct Mark<T: drop> {
  value: T;
}

fn recur<T: drop, const n: u64>(value: T) -> result: Mark<T> pure {
  return recur::<T, n>(value: move value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        )
    });
    let expanded = std::str::from_utf8(source)
        .unwrap()
        .replace("recur::<T, n>", "recur::<Slots<T, n>, n>");
    assert_rule_kind(expanded.as_bytes(), SemanticRule::Fn6, |kind| {
        matches!(kind, SemanticIssueKind::PolymorphicRecursion { .. })
    });
}

// Retired with the `Box` store brand of [PROV-1]: v0.59's
// `a_captured_generic_box_brand_does_not_infer_a_different_store` required
// the checker to refuse a `Box<'b, u64>` argument at a `Box<'a, u64>`
// position. There is one heap in v0.60 and `Box<T>` carries no brand
// [STOR-8], so two boxes of one referent type are one type and there is no
// store to confuse.

#[test]
fn explicit_int_generic_function_builds_each_reachable_concrete_instance() {
    let source = br#"fn identity<T: Int>(value: T) -> result: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = identity::<u32>(value: 7_u32);
  let second = identity::<i64>(value: -9_i64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("explicit generic instances must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 3);
        assert_eq!(checked.entry_function_name(), "main");
    });
}

#[test]
fn int_bound_selects_the_same_operation_row_for_every_concrete_instance() {
    let source = br#"fn maximum<T: Int>(left: T, right: T) -> result: T pure {
  return imax(left, right);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = maximum::<u8>(left: 4_u8, right: 9_u8);
  let signed = maximum::<i64>(left: -7_i64, right: -2_i64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("Int-bound operation must check for each instance: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 3);
    });
}

#[test]
fn float_bound_selects_operations_and_identities_for_every_concrete_instance() {
    let source = br#"fn nudge<T: Float>(value: T) -> result: T pure {
  let zero = 0_T;
  let one = 1_T;
  let shifted = fadd.strict(value, one);
  return fadd.strict(zero, shifted);
}

fn main() -> status: std::process::ExitStatus pure {
  let single = nudge::<f32>(value: 2.0_f32);
  let double = nudge::<f64>(value: 4.0_f64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("Float-bound operations must check for each instance: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 3);
    });
}

#[test]
fn float_bound_rejects_a_non_float_explicit_argument_under_fn3() {
    let source = br#"fn identity<T: Float>(value: T) -> result: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let invalid = identity::<u32>(value: 7_u32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn3, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

#[test]
fn numeric_identity_requires_an_int_or_float_bound() {
    let source = br#"fn invalid<T: drop>() -> result: T pure {
  return 0_T;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Form5, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

#[test]
fn int_bound_identity_is_concretized_before_lowering() {
    let source = br#"fn one<T: Int>() -> result: T pure {
  return 1_T;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = one::<u16>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("Int-bound identity must check and concretize: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 2);
    });
}

#[test]
fn unused_generic_exact_conversion_requires_the_complete_bound_domain() {
    // Numeric generics now have ordinary OP-6 obligations. A safe concrete
    // call cannot authorize the unused or canonical symbolic body's cast.
    for call in ["", "  let result = convert::<u8>(value: 7_u8);\n"] {
        let source = format!(
            "fn convert<T: Int>(value: T) -> result: u64 pure {{
  return cvt::<T, u64>(value);
}}

fn main() -> status: std::process::ExitStatus pure {{
{call}  return std::process::exit_status(code: 0_u8);
}}
"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Op6, |kind| {
            matches!(
                kind,
                SemanticIssueKind::UndischargedConversionDomainObligation { .. }
            )
        });
    }
}

#[test]
fn generic_conversion_interfaces_accept_each_numeric_bound_pair() {
    for (source_bound, destination_bound, source_type, destination_type, literal) in [
        ("Int", "Int", "u32", "u8", "7_u32"),
        ("Int", "Float", "u32", "f32", "7_u32"),
        ("Float", "Int", "f64", "i32", "7.0_f64"),
        ("Float", "Float", "f64", "f32", "7.0_f64"),
    ] {
        let source = format!(
            "fn attempt<S: {source_bound}, D: {destination_bound}>(value: S) -> result: Result<D, NarrowError> pure {{
  return cvt.checked::<S, D>(value);
}}

fn convert<S: {source_bound}, D: {destination_bound}>(value: S) -> result: D pure contract {{
  requires cvt.defined::<S, D>(value);
}} {{
  let local_attempt = cvt.checked::<S, D>(value);
  let permitted = cvt.defined::<S, D>(value);
  return cvt::<S, D>(value);
}}

fn forward<A: {source_bound}, B: {destination_bound}>(value: A) -> result: B pure contract {{
  requires cvt.defined::<A, B>(value);
}} {{
  return convert::<A, B>(value: value);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let attempted = attempt::<{source_type}, {destination_type}>(value: {literal});
  let result = forward::<{source_type}, {destination_type}>(value: {literal});
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("{source_bound} to {destination_bound} conversion must check: {outcome:?}");
            };
            lower_checked(*checked, OverlapLowering::Off)
                .expect("concrete conversion endpoints must lower");
        });
    }
}

#[test]
fn generic_total_conversions_preserve_repeated_type_identity() {
    let source = br#"fn same_integer<T: Int>(value: T) -> result: T pure {
  let attempt_result = cvt.checked::<T, T>(value);
  return cvt::<T, T>(value);
}

fn same_float<T: Float>(value: T) -> result: T pure {
  let attempt_result = cvt.checked::<T, T>(value);
  return cvt::<T, T>(value);
}

fn widen_float<T: Float>(value: T) -> result: f64 pure {
  return cvt::<T, f64>(value);
}

fn small_float<T: Float>(value: u8) -> result: T pure {
  return cvt::<u8, T>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  let integer = same_integer::<i64>(value: -1_i64);
  let floating = same_float::<f32>(value: -0.0_f32);
  let wide = widen_float::<f32>(value: floating);
  let same = widen_float::<f64>(value: wide);
  let small = small_float::<f32>(value: 255_u8);
  let large = small_float::<f64>(value: 255_u8);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("correlated and universally total numeric pairs must check: {outcome:?}");
        };
        lower_checked(*checked, OverlapLowering::Off)
            .expect("universally total conversion instances must lower");
    });
}

#[test]
fn generic_conversion_constants_use_exact_values_without_selecting_widths() {
    let source = br#"const small: u32 = 127_u32;

const exact_integer: u32 = 16777218_u32;

const half: f64 = 0.5_f64;

fn integer_identities<T: Int, D: Int>() -> result: D pure {
  let zero = cvt::<T, D>(0_T);
  return cvt::<T, D>(1_T);
}

fn integer_float_identities<T: Int, D: Float>() -> result: D pure {
  let zero = cvt::<T, D>(0_T);
  return cvt::<T, D>(1_T);
}

fn float_integer_identities<T: Float, D: Int>() -> result: D pure {
  let zero = cvt::<T, D>(0_T);
  return cvt::<T, D>(1_T);
}

fn float_identities<T: Float, D: Float>() -> result: D pure {
  let zero = cvt::<T, D>(0_T);
  return cvt::<T, D>(1_T);
}

fn small_integer<D: Int>() -> result: D pure {
  return cvt::<u32, D>(small);
}

fn sparse_float<D: Float>() -> result: D pure {
  return cvt::<u32, D>(exact_integer);
}

fn integral_float<D: Int>() -> result: D pure {
  return cvt::<f64, D>(1.0_f64);
}

fn fractional_float<D: Float>() -> result: D pure {
  return cvt::<f64, D>(half);
}

fn require_exact<D: Float>(value: u32) -> result: unit pure contract {
  requires cvt.defined::<u32, D>(value);
} {
  return unit;
}

fn forward_exact<D: Float>() -> result: unit pure {
  return require_exact::<D>(value: 16777218_u32);
}

fn main() -> status: std::process::ExitStatus pure {
  let integer = integer_identities::<u64, i8>();
  let integer_float = integer_float_identities::<i64, f32>();
  let float_integer = float_integer_identities::<f64, u8>();
  let floating = float_identities::<f64, f32>();
  let small_value = small_integer::<i8>();
  let sparse = sparse_float::<f32>();
  let integral = integral_float::<u8>();
  let half_value = fractional_float::<f32>();
  let forwarded = forward_exact::<f32>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("uniform symbolic constant domains must check: {outcome:?}");
        };
        lower_checked(*checked, OverlapLowering::Off)
            .expect("symbolic constant conversion instances must lower");
    });
}

#[test]
fn mixed_generic_constant_domains_prove_neither_truth_sign() {
    let exact = br#"fn invalid<D: Float>() -> result: D pure {
  return cvt::<u32, D>(16777217_u32);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(exact, SemanticRule::Op6, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedConversionDomainObligation {
                disposition: crate::StaticObligationDisposition::Unproved,
                ..
            }
        )
    });
    let positive_requirement = br#"fn invalid<D: Float>(value: f64) -> result: i32 pure contract {
  requires cvt.defined::<u32, D>(16777217_u32);
} {
  return cvt::<f64, i32>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(positive_requirement, SemanticRule::Op6, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedConversionDomainObligation { .. }
        )
    });
    let negative_requirement = br#"fn require_inexact<D: Float>() -> result: unit pure contract {
  define exact = cvt.defined::<u32, D>(16777217_u32);
  requires bnot(exact);
} {
  return unit;
}

fn invalid<D: Float>() -> result: unit pure {
  return require_inexact::<D>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(negative_requirement, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement { .. })
    });
}

#[test]
fn distinct_generic_numeric_parameters_keep_their_domain_identity() {
    for (bound, source_type, destination_type, literal) in [
        ("Int", "u8", "u16", "1_u8"),
        ("Float", "f32", "f64", "1.0_f32"),
    ] {
        let source = format!(
            "fn invalid<S: {bound}, D: {bound}>(value: S) -> result: D pure {{
  return cvt::<S, D>(value);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let result = invalid::<{source_type}, {destination_type}>(value: {literal});
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Op6, |kind| {
            matches!(
                kind,
                SemanticIssueKind::UndischargedConversionDomainObligation { residual, .. }
                    if residual == "cvt.defined::<S, D>(value)"
            )
        });
    }
}

#[test]
fn numeric_conversion_does_not_grant_an_unbounded_type_numeric_capability() {
    let source = br#"fn invalid<T>(value: T) -> result: u64 pure {
  return cvt::<T, u64>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  let result = invalid::<u8>(value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
}

#[test]
fn generic_reinterpret_keeps_its_existing_capability_boundary() {
    let source = br#"fn reinterpret_value<T: Int>(value: T) -> result: u32 pure {
  return reinterpret::<T, u32>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_unsupported(source, UnsupportedSemanticFeature::Generics);
}

#[test]
fn generic_conversion_requirements_follow_forwarded_const_arguments() {
    let source = br#"fn convert<const value: u32>() -> result: u8 pure contract {
  requires cvt.defined::<u32, u8>(value);
} {
  return cvt::<u32, u8>(value);
}

fn forward<const value: u32>() -> result: u8 pure contract {
  requires cvt.defined::<u32, u8>(value);
} {
  return convert::<value>();
}

fn main() -> status: std::process::ExitStatus pure {
  let result = forward::<7>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
    let out_of_range = std::str::from_utf8(source)
        .unwrap()
        .replace("forward::<7>", "forward::<256>");
    assert_rule_kind(out_of_range.as_bytes(), SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

#[test]
fn conversion_clause_definitions_admit_only_universally_total_exact_pairs() {
    let source = br#"fn same<T: Float>(value: T) -> result: T pure contract {
  define copied = cvt::<T, T>(value);
  requires cvt.defined::<T, T>(copied);
} {
  return value;
}

fn small<T: Float>(value: u8) -> result: T pure contract {
  define widened = cvt::<u8, T>(value);
  requires cvt.defined::<T, f64>(widened);
} {
  return cvt::<u8, T>(value);
}

fn main() -> status: std::process::ExitStatus pure {
  let same_value = same::<f64>(value: 1.0_f64);
  let converted = small::<f32>(value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
    for (source_type, destination_type) in [("u32", "u8"), ("S", "D")] {
        let parameters = if source_type == "S" {
            "<S: Int, D: Int>"
        } else {
            ""
        };
        let source = format!(
            "fn invalid{parameters}(value: {source_type}, witness: {destination_type}) -> result: unit pure contract {{
  requires cvt.defined::<{source_type}, {destination_type}>(value);
  requires cvt::<{source_type}, {destination_type}>(value) == witness;
}} {{
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        assert_rule(
            source.as_bytes(),
            SemanticRule::Fn8,
            SemanticIssueKind::InvalidRequires,
        );
    }
}

#[test]
fn conversion_defined_does_not_extend_the_postcondition_relation_fragment() {
    let source = br#"fn invalid<T: Float>(value: T) -> result: u8 pure contract {
  ensures cvt.defined::<T, T>(value);
} {
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionRelation,
    );
}

#[test]
fn int_bound_rejects_a_non_integer_explicit_argument_under_fn3() {
    let source = br#"fn identity<T: Int>(value: T) -> result: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let input = True();
  let invalid = identity::<Bool>(value: input);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn3, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// The positive control for the three [FN-6] rejections below: a cycle whose
/// every call does instantiate the callee at exactly the caller's own
/// parameters is *permitted* by FN-6, and it is also finite — the call mints
/// no instance the caller is not already at — so it monomorphizes to the one
/// instance the program reaches rather than stopping as an unimplemented
/// capability.
#[test]
fn a_generic_call_cycle_at_the_callers_own_parameters_monomorphizes() {
    let source = br#"fn recursive<T: Int>(value: T) -> result: T pure {
  return recursive::<T>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let seen = recursive::<u16>(value: 1_u16);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a cycle at the caller's own parameters must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 2);
    });
}

/// D7's complete-vector FN-6 rule refuses const growth before enumerating
/// instances. The previous type-only rule left this as a compiler capability
/// gap; the changed verdict follows the explicit specification amendment.
#[test]
fn a_generic_cycle_varying_a_const_argument_stops_before_instance_enumeration() {
    let source = br#"fn expand_count<const n: u64>(at: u64) -> total: u64 pure {
  let done = at == 0_u64;
  if done {
    return 0_u64;
  }
  let next = at -wrap 1_u64;
  let rest = expand_count::<n + 1>(at: next);
  return rest +wrap 1_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let total = expand_count::<1>(at: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    // D7 extends FN-6 from type-only forwarding to the complete parameter
    // vector, so changing n is now a specified source rejection.
    assert_rule(
        source,
        SemanticRule::Fn6,
        SemanticIssueKind::PolymorphicRecursion {
            cycle: "expand_count -> expand_count".to_owned(),
            mechanical_fix: "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle",
        },
    );
}

/// FN-6 refuses changed, constructed and permuted arguments with the cycle
/// named. The separate same-vector control executes ordinary finite discovery.
#[test]
fn polymorphic_recursion_is_rejected_at_the_call_that_leaves_the_caller_parameters() {
    let fixed_type = SemanticIssueKind::PolymorphicRecursion {
        cycle: "poly -> poly".to_owned(),
        mechanical_fix: "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle",
    };
    // The conformance corpus's own case bytes: the recursive call instantiates
    // the callee at a fixed `i32` instead of the caller's `T`.
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn6-neg-polymorphic-recursion.wf"),
        SemanticRule::Fn6,
        fixed_type.clone(),
    );
    // A growing argument is the shape that would actually diverge: each
    // instance would demand a strictly larger one.
    assert_rule(
        br#"fn poly<T: drop>(x: T) -> result: T pure {
  let y = poly::<Slots<T, 2>>(x: x);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn6,
        fixed_type,
    );
    // A permutation cycle terminates, and FN-6 is deliberately stronger than
    // finiteness requires, so it is rejected all the same.
    assert_rule(
        br#"fn left<A: drop, B: drop>(first: A, second: B) -> result: A pure {
  let swapped = right::<B, A>(first: second, second: first);
  return first;
}

fn right<A: drop, B: drop>(first: A, second: B) -> result: A pure {
  let back = left::<A, B>(first: first, second: second);
  return first;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn6,
        SemanticIssueKind::PolymorphicRecursion {
            cycle: "left -> right -> left".to_owned(),
            mechanical_fix: "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle",
        },
    );
}

/// This finite cycle drops its parameter vector at a nongeneric participant.
/// D7 deliberately refuses it under FN-6's stronger unchanged-vector rule.
#[test]
fn a_cycle_cannot_drop_the_generic_vector_at_a_nongeneric_trampoline() {
    let source = br#"fn poly<T: drop>(x: T) -> result: T pure {
  let back = trampoline();
  return x;
}

fn trampoline() -> result: i32 pure {
  let forward = poly::<i32>(x: 0_i32);
  return forward;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // D7's whole-component rule includes the edge that drops T. No instance
    // enumeration or capability refusal substitutes for this FN-6 judgment.
    assert_rule(
        source,
        SemanticRule::Fn6,
        SemanticIssueKind::PolymorphicRecursion {
            cycle: "poly -> trampoline -> poly".to_owned(),
            mechanical_fix: "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle",
        },
    );
}

#[test]
fn unused_int_generic_body_is_checked_for_the_complete_bound_domain() {
    let source = br#"fn invalid<T: Int>(value: T) -> result: T pure {
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(source, SemanticRule::Fn1, SemanticIssueKind::ReturnMismatch);
}

/// [FN-2, OWN-1, S37] the template is the spelling authority.
///
/// This test asserted the opposite until the owner's 2026-09-05 ruling: an
/// `affine`-bounded body writing `move value` was an [OWN-1] `MoveOfCopy`
/// rejection at every copy instance, so no generic body could serve a copy
/// type and an affine type, and the library dodged it by instantiating only at
/// affine types. Under [S37] the body is checked once at the symbolic instance
/// under its written bound, and the concrete-instance recheck does not
/// re-judge the [OWN-1]/[FORM-1] spelling: `move` of a template-affine value
/// at a copy instance denotes a copy. The instance recheck still rejects what
/// is invalid *at the instance* — the conformance corpus keeps that in
/// `const1-neg-eval-overflow`, whose body is admitted symbolically and refused
/// at the instantiation whose value leaves the const domain.
#[test]
fn a_move_in_an_affine_bounded_body_denotes_a_copy_at_a_copy_instance() {
    let source = br#"nocopy struct Payload {
  value: u8;
}

fn transfer<T: drop>(value: T) -> result: T pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = transfer::<u8>(value: 7_u8);
  let payload = Payload(value: 3_u8);
  let held = transfer::<Payload>(value: move payload);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("one affine-bounded body must serve a copy and an affine instance: {outcome:?}");
        };
        // The template, its copy instance, its affine instance, and `main`.
        assert_eq!(checked.function_count(), 3);
    });
}

/// [FN-2, OWN-1] a callee's written body keeps its own symbolic spelling
/// authority when a generic caller fixes only some of its arguments. The two
/// declarations deliberately reuse `T`: symbolic identity is a declaration,
/// not the parameter's spelling.
#[test]
fn partial_type_instantiation_keeps_the_callees_move_spelling() {
    let source = br#"nocopy struct Payload {
  value: u64;
}

fn package_value<T: drop, R>(value: R) -> result: Result<R, unit> pure {
  return Ok<R, unit>(value: move value);
}

fn forward<T: drop>(value: T) -> result: T pure {
  let wrapped = package_value::<T, unit>(value: unit);
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = forward::<u64>(value: 7_u64);
  let payload = Payload(value: 3_u64);
  let held = forward::<Payload>(value: move payload);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn partial_const_instantiation_keeps_the_callees_move_spelling() {
    let source = br#"fn package_value<R, const n: u64>(value: R) -> result: Result<R, unit> pure {
  return Ok<R, unit>(value: move value);
}

fn forward<const n: u64>() -> result: Result<unit, unit> pure {
  return package_value::<unit, n>(value: unit);
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = forward::<3>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

/// This is the Slab visitor's failure without any container code: the result
/// type is already copy while the supplied callable is still symbolic.
#[test]
fn partial_function_instantiation_keeps_the_callees_move_spelling() {
    let source =
        br#"fn package_value<R, fn make() -> result: R pure>() -> result: Result<R, unit> pure {
  let observed = make();
  return Ok<R, unit>(value: move observed);
}

fn forward<fn make() -> result: unit pure>() -> result: Result<unit, unit> pure {
  return package_value::<unit, fn make>();
}

fn make_unit() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = forward::<fn make_unit>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn the_canonical_generic_body_still_rejects_copy_moves_and_repeated_consumes() {
    for (parameters, value_type) in [("T: copy", "T"), ("T: drop", "u64")] {
        let source = format!(
            "fn invalid<{parameters}>(value: {value_type}) -> result: {value_type} pure {{\n  return move value;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Own1, |kind| {
            matches!(kind, SemanticIssueKind::MoveOfCopy { .. })
        });
    }
    let source = br#"fn invalid<T: drop, R>(value: R) -> result: Result<R, unit> pure {
  let first = move value;
  return Ok<R, unit>(value: move value);
}

fn forward<T: drop>() -> result: Result<unit, unit> pure {
  return invalid::<T, unit>(value: unit);
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = forward::<u64>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::UseAfterMove { .. })
    });
}

#[test]
fn nested_generic_calls_discover_reachable_instances_after_template_checking() {
    let source = br#"fn select<T: Int>(value: T) -> result: T pure {
  return imax(value, value);
}

fn forward<T: Int>(value: T) -> result: T pure {
  return select::<T>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = forward::<u8>(value: 7_u8);
  let signed = forward::<i64>(value: -9_i64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("nested generic calls must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 5);
    });
}

#[test]
fn const_parameters_forward_symbolically_and_instantiate_at_reachable_sizes() {
    let source = br#"fn preserve<const n: u64>(value: Slots<u8, n>) -> result: Slots<u8, n> pure {
  let size = value.len;
  return move value;
}

fn forward<const n: u64>(value: Slots<u8, n>) -> result: Slots<u8, n> pure {
  return preserve::<n>(value: move value);
}

fn main() -> status: std::process::ExitStatus pure {
  let small_input = slots_new::<u8, 2>();
  let small = forward::<2>(value: move small_input);
  let large_input = slots_new::<u8, 5>();
  let large = forward::<5>(value: move large_input);
  let small_held = small.len;
  if 1_u64 < small_held {
    let first = small[1_u64];
  }
  let large_held = large.len;
  if 4_u64 < large_held {
    let second = large[4_u64];
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("forwarded const instances must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 5);
    });
}

const CONST_GENERIC_INTEGER_DOMAINS: [(&str, i128, i128); 8] = [
    ("u8", 0, 255),
    ("u16", 0, 65_535),
    ("u32", 0, 4_294_967_295),
    ("u64", 0, 18_446_744_073_709_551_615),
    ("i8", -128, 127),
    ("i16", -32_768, 32_767),
    ("i32", -2_147_483_648, 2_147_483_647),
    ("i64", -9_223_372_036_854_775_808, 9_223_372_036_854_775_807),
];

/// [ENT-2, MSR-6] the branch relates the runtime value to the const parameter,
/// whose own written type closes the arithmetic domain. An alias or an extra
/// invariant must not be needed to introduce that standing bound.
#[test]
fn const_generic_type_bounds_discharge_strictly_guarded_arithmetic() {
    for (ty, _, _) in CONST_GENERIC_INTEGER_DOMAINS {
        let source = format!(
            "fn increment<const limit: {ty}>(value: {ty}) -> result: {ty} pure {{
  if value < limit {{
    return value + 1_{ty};
  }}
  return value;
}}

fn decrement<const limit: {ty}>(value: {ty}) -> result: {ty} pure {{
  if value > limit {{
    return value - 1_{ty};
  }}
  return value;
}}

fn forward<const limit: {ty}>(value: {ty}) -> result: {ty} pure {{
  let increased = increment::<limit>(value: value);
  return decrement::<limit>(value: increased);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let value = forward::<0>(value: 0_{ty});
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        with_semantics(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{ty}: {outcome:?}"
            );
        });
    }
}

/// The source invariant uses the affine image of the same const term. Its
/// interval is the declared type's interval, including signed lower bounds.
#[test]
fn const_generic_affine_images_keep_every_declared_integer_domain() {
    for (ty, minimum, maximum) in CONST_GENERIC_INTEGER_DOMAINS {
        let source = format!(
            "fn bounded<const limit: {ty}>() -> result: unit pure {{
  invariant lower: limit >= {minimum}_{ty};
  invariant upper: limit <= {maximum}_{ty};
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  bounded::<0>();
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        with_semantics(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{ty}: {outcome:?}"
            );
        });
    }
}

/// A u8 parameter used as a u64 storage extent is still the same symbolic
/// constant; neither the measure nor a forwarded formal may widen its bound.
#[test]
fn const_generic_extent_and_forwarding_keep_one_declared_type_identity() {
    let source = br#"fn extent<const n: u8>() -> result: unit pure {
  let values = slots_new::<u8, n>();
  invariant same: values.cap == n;
  invariant bounded: values.cap <= 255_u64;
  return unit;
}

fn increment<const limit: u64>(value: u64) -> result: u64 pure {
  if value < limit {
    return value + 1_u64;
  }
  return value;
}

fn forward<const n: u8>(value: u64) -> result: u64 pure {
  extent::<n>();
  return increment::<n>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let value = forward::<3>(value: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn const_generic_bounds_do_not_prove_a_narrower_domain() {
    for (ty, minimum, maximum) in CONST_GENERIC_INTEGER_DOMAINS {
        for (comparison, boundary) in [(">=", minimum + 1), ("<=", maximum - 1)] {
            // Neither an unused template nor a safe concrete use grants a
            // stronger bound to the canonical symbolic instance [FN-2].
            for call in ["", "  invalid::<0>();\n"] {
                let source = format!(
                    "fn invalid<const limit: {ty}>() -> result: unit pure {{
  invariant narrowed: limit {comparison} {boundary}_{ty};
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
{call}  return std::process::exit_status(code: 0_u8);
}}
"
                );
                assert_rule_kind(source.as_bytes(), SemanticRule::Inv1, |kind| {
                    matches!(kind, SemanticIssueKind::UndischargedLocalInvariant { .. })
                });
            }
        }
    }
}

#[test]
fn const_generic_type_bounds_do_not_make_inclusive_arithmetic_guards_strict() {
    for (ty, _, _) in CONST_GENERIC_INTEGER_DOMAINS {
        for (comparison, operation) in [("<=", "+"), (">=", "-")] {
            let source = format!(
                "fn invalid<const limit: {ty}>(value: {ty}) -> result: {ty} pure {{
  if value {comparison} limit {{
    return value {operation} 1_{ty};
  }}
  return value;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
            );
            assert_rule_kind(source.as_bytes(), SemanticRule::Op2, |kind| {
                matches!(
                    kind,
                    SemanticIssueKind::UndischargedIntegerDomainObligation { .. }
                )
            });
        }
    }
}

#[test]
fn unbounded_type_parameters_build_only_explicit_reachable_instances() {
    let source = br#"fn marker<T: drop>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  marker::<u8>();
  marker::<Bool>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("unbounded marker instances must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 3);
    });
}

/// The first two are wrong-kind arguments on a **user-generic call**, so
/// [DIAG-1] gives them to FN-2: "the cited rule is the rule selected by the
/// callee's class". They recorded TYPE-5 while the compiler chose its rule
/// from the kind of argument problem instead of the callee, and they move with
/// the 2026-08-08 ruling that settled the question — TYPE-5 governs whether an
/// argument's type matches its parameter, not the argument list itself. The
/// third is CONST-1's own violation and is unaffected.
#[test]
fn generic_argument_kinds_and_const_parameter_types_are_checked() {
    assert_rule_kind(
        br#"fn marker<T: drop>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  marker::<4>();
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn2,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule_kind(
        br#"fn sized<const n: u64>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  sized::<u8>();
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn2,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule(
        br#"fn invalid<const n: Bool>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Const1,
        SemanticIssueKind::InvalidConstValue,
    );
}

#[test]
fn source_generic_structs_are_checked_symbolically_and_rechecked_per_instance() {
    let source = br#"struct Pair<T: Int> {
  left: T;
  right: T;
}

fn duplicate<T: Int>(value: T) -> result: Pair<T> pure {
  return Pair<T>(left: value, right: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = duplicate::<u8>(value: 7_u8);
  let wide = duplicate::<i64>(value: -9_i64);
  let small_left = small.left;
  let wide_right = wide.right;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("generic struct instances must check: {outcome:?}");
        };
        let pair_instances = checked
            .data
            .nominals
            .iter()
            .filter(|nominal| nominal.name.starts_with("Pair<"))
            .collect::<Vec<_>>();
        assert_eq!(pair_instances.len(), 2);
        assert_ne!(pair_instances[0].id, pair_instances[1].id);
        assert_eq!(checked.function_count(), 3);
    });
}

#[test]
fn source_generic_enums_use_the_concrete_instance_member_table() {
    let source = br#"enum Choice<T: Int> {
  Missing();
  Present(value: T);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = Choice<u8>::Present(value: 3_u8);
  match small {
    Missing() => {
      let ignored = unit;
    }
    Present(value: observed) => {
      let retained = observed;
    }
  }
  let wide = Choice<i64>::Present(value: -5_i64);
  match wide {
    Missing() => {
      let ignored = unit;
    }
    Present(value: observed) => {
      let retained = observed;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("generic enum instances must check: {outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .nominals
                .iter()
                .filter(|nominal| nominal.name.starts_with("Choice<"))
                .count(),
            2
        );
    });
}

#[test]
fn const_and_nested_source_nominal_instances_are_fully_substituted() {
    let source = br#"struct Packet<const n: u64> {
  bytes: Slots<u8, n>;
}

struct Holder<T: drop> {
  value: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let short_bytes = slots_new::<u8, 2>();
  let short = Packet<2>(bytes: move short_bytes);
  let long_bytes = slots_new::<u8, 5>();
  let long = Packet<5>(bytes: move long_bytes);
  let held = Holder<Packet<2>>(value: move short);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("const and nested nominal instances must check: {outcome:?}");
        };
        let mut packet_lengths = checked
            .data
            .nominals
            .iter()
            .filter(|nominal| nominal.name.starts_with("Packet<"))
            .map(|nominal| match &nominal.kind {
                CheckedNominalKind::Struct { fields } => match fields[0].ty {
                    CheckedType::Window {
                        shape: WindowShape::Slots,
                        capacity: Some(CheckedConst::Value(length)),
                        ..
                    } => length,
                    other => panic!("Packet field must be a concrete window: {other:?}"),
                },
                other => panic!("Packet must remain a struct: {other:?}"),
            })
            .collect::<Vec<_>>();
        packet_lengths.sort_unstable();
        assert_eq!(packet_lengths, [2, 5]);
        assert_eq!(
            checked
                .data
                .nominals
                .iter()
                .filter(|nominal| nominal.name.starts_with("Holder<"))
                .count(),
            1
        );
    });
}

// Retired with the region-parameter list of [FORM-8]: v0.59's
// `multi_result_region_spelling_keeps_first_occurrence_order` required a
// declaration's `['right, 'left]` list to follow first-use order. v0.60 has
// no region list on a declaration, so there is no spelling order to keep.

// Retired with the inferred call region of [FORM-8]: v0.59's
// `nested_nominal_parameter_regions_are_inferred_at_calls` refused a region
// written at a call position the argument already fixed. v0.60 writes every
// type, const and function argument explicitly [FN-2] and has no region
// argument at all.

// Retired with the crossed region handoff of [FORM-8, TYPE-5]: v0.59's
// `nested_results_reject_crossed_region_handoffs` relabelled two nested
// brands through an anchor parameter. v0.60 has no brand; the exact type
// agreement the test rode on survives in
// `source_nominal_argument_arity_and_kinds_are_exact` below.

#[test]
fn source_nominal_argument_arity_and_kinds_are_exact() {
    assert_rule_kind(
        br#"struct Pair<T: drop> {
  value: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let invalid = Pair<u8, u16>(value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule_kind(
        br#"struct Packet<const n: u64> {
  bytes: Slots<u8, n>;
}

fn main() -> status: std::process::ExitStatus pure {
  let bytes = slots_new::<u8, 1>();
  let invalid = Packet<u8>(bytes: move bytes);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn constructor_only_generic_instances_still_reach_normal_type_diagnostics() {
    assert_rule(
        br#"struct Holder<T: drop> {
  value: T;
}

fn main() -> status: std::process::ExitStatus pure {
  return Holder<u8>(value: 1_u8);
}
"#,
        SemanticRule::Fn1,
        SemanticIssueKind::ReturnMismatch,
    );
}

#[test]
fn recursive_generic_nominal_layouts_stop_before_concrete_enumeration() {
    assert_unsupported(
        br#"struct Recursive<T: drop> {
  next: Recursive<T>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        UnsupportedSemanticFeature::RecursiveNominalLayout,
    );
}

#[test]
fn checked_integer_results_are_available_during_template_and_concrete_rechecking() {
    let source =
        br#"fn checked_sum<T: Int>(left: T, right: T) -> result: Result<T, Overflow> pure {
  return left +checked right;
}

fn main() -> status: std::process::ExitStatus pure {
  let small = checked_sum::<u8>(left: 1_u8, right: 2_u8);
  let wide = checked_sum::<i64>(left: -3_i64, right: 5_i64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("checked generic results must check through both stages: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 3);
    });
}

#[test]
fn numeric_and_const_parameters_flow_through_window_operations() {
    // v0.59 also drove two store-resident `Vector` runs through `heap_vector`
    // under an `allocates(store)` row. There is one heap in v0.60, allocation
    // carries no effect entry [STOR-8], and the store provider is gone, so the
    // surviving subject is the constant-capacity window the [OP-10] rows move.
    let source =
        br#"fn filled_run<T: Int, const n: u64>(value: T) -> result: Slots<T, n> pure contract {
  ensures result.len >= n;
} {
  let built = slots_new::<T, n>();
  for @fill (
    at in 0_u64..n,
    invariant grown: built.len >= at,
    invariant spare: built.cap + at >= built.len + n
  ) {
    place_back(window: &built, value: value);
  }
  return move built;
}

fn filled_float_run<T: Float, const n: u64>(value: T) -> result: Slots<T, n> pure contract {
  ensures result.len >= n;
} {
  let built = slots_new::<T, n>();
  for @fill (
    at in 0_u64..n,
    invariant grown: built.len >= at,
    invariant spare: built.cap + at >= built.len + n
  ) {
    place_back(window: &built, value: value);
  }
  return move built;
}

fn main() -> status: std::process::ExitStatus pure {
  let bytes = filled_run::<u8, 2>(value: 7_u8);
  let words = filled_run::<i64, 3>(value: -5_i64);
  let byte = bytes[1_u64];
  let word = words[2_u64];
  let samples = filled_float_run::<f32, 2>(value: 1.5_f32);
  let sample = samples[1_u64];
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("generic window rows must check and concretize: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 4);
    });
}

// Retired with the region-bearing generic argument of [FN-2]: v0.59's
// `region_bearing_function_and_nominal_arguments_reject_under_fn2` refused
// `Slice<u8>` and `Arena<64, 8>` in a written `targ`. v0.60 has neither type,
// and a reference kind is not a type at all [TYPE-8]: `targ := type | const
// | function_arg` and `type` has no `&` alternative, so the generic-argument
// position can no longer name one.

/// A concrete nominal written inside an otherwise symbolic function remains a
/// concrete descendant of that source schema.  Rebuilding the concrete
/// inventory must therefore retain both the nominal and the callee instance.
#[test]
fn schema_written_concrete_nominal_arguments_are_rebuilt_after_the_symbolic_checkpoint() {
    let source = br#"struct Pair<T: Int> {
  value: T;
}

fn consume<T: drop>(value: T) -> result: unit pure {
  return unit;
}

fn wrapper<U: drop>() -> result: unit pure {
  let pair = Pair<u8>(value: 1_u8);
  consume::<Pair<u8>>(value: pair);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the concrete nominal substitution must be rebuilt: {outcome:?}");
        };
        assert!(
            program
                .data
                .functions
                .iter()
                .any(|function| function.name == "consume")
        );
        assert!(
            program
                .data
                .nominals
                .iter()
                .any(|nominal| nominal.name.starts_with("Pair<"))
        );
    });
}

/// A partially concrete call chain contributes only the nominal arguments
/// whose complete type is known at the symbolic checkpoint.
#[test]
fn partial_schema_rebuild_keeps_only_the_truly_concrete_nominal_instance() {
    let source = br#"struct Pair<T: Int> {
  left: T;
  right: T;
}

fn sink<T: drop>() -> result: unit pure {
  return unit;
}

fn middle<A: Int, B: drop>() -> result: unit pure {
  sink::<Pair<A>>();
  return unit;
}

fn wrapper<U: drop>() -> result: unit pure {
  middle::<u8, U>();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("partial rebuilding must retain concrete descendants: {outcome:?}");
        };
        let pairs = program
            .data
            .nominals
            .iter()
            .take(program.data.executable_nominal_count)
            .filter(|nominal| nominal.name.starts_with("Pair<"))
            .collect::<Vec<_>>();
        let [pair] = pairs.as_slice() else {
            panic!("exactly one concrete Pair<u8> must survive: {pairs:?}");
        };
        let CheckedNominalKind::Struct { fields } = &pair.kind else {
            panic!("Pair is a struct nominal")
        };
        assert_eq!(fields.len(), 2);
        assert!(
            fields
                .iter()
                .all(|field| field.ty == CheckedType::Integer(IntegerType::U8))
        );
        assert_eq!(
            program
                .data
                .functions
                .iter()
                .filter(|function| function.name == "sink")
                .count(),
            1
        );
        lower_checked(*program, OverlapLowering::Off)
            .expect("the concrete-only rebuilt inventory must lower");
    });
}

/// A symbolic argument in one position must not hide an independent concrete
/// descendant in another position of the same call.
#[test]
fn partial_schema_rebuild_still_discovers_an_independent_concrete_descendant() {
    let source = br#"struct Pair<T: Int> {
  left: T;
  right: T;
}

fn sink<T: drop>() -> result: unit pure {
  return unit;
}

fn next<X: drop, Y: drop>() -> result: unit pure {
  sink::<Y>();
  return unit;
}

fn middle<A: Int>() -> result: unit pure {
  next::<Pair<A>, u8>();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("partial rebuilding must project its concrete descendant: {outcome:?}");
        };
        assert!(
            program
                .data
                .nominals
                .iter()
                .take(program.data.executable_nominal_count)
                .all(|nominal| !nominal.name.starts_with("Pair<"))
        );
        assert_eq!(
            program
                .data
                .functions
                .iter()
                .filter(|function| function.name == "sink")
                .count(),
            1
        );
        assert!(
            program
                .data
                .functions
                .iter()
                .all(|function| function.name != "next")
        );
        lower_checked(*program, OverlapLowering::Off)
            .expect("the concrete descendant inventory must lower");
    });
}

/// Source-order diagnostics stay source-stable even when an earlier generic
/// body receives a dense concrete instance identity during inventory build.
#[test]
fn ordinary_admission_diagnostics_prefer_source_order_over_instance_identity() {
    let source = br#"fn earlier<T: drop>(values: Slots<u8, 4>, index: u64) -> result: u8 pure {
  return values[index];
}

fn later(values: Slots<u8, 4>, index: u64) -> result: u8 pure {
  return values[index];
}

fn main() -> status: std::process::ExitStatus pure {
  let first_values = slots_new::<u8, 4>();
  let second_values = slots_new::<u8, 4>();
  earlier::<u8>(values: move first_values, index: 5_u64);
  later(values: move second_values, index: 5_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("both bodies contain an OP-4 rejection: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op4);
        let crate::SemanticLocation::SourceNode(path, _) = issue.location();
        assert_eq!(path.components().first(), Some(&0));
    });
}

/// A multi-result list is a nominal of its own, and its ordinal names are
/// part of that nominal's identity: two declarations that differ only in a
/// result binder spelling stay in two physical families.
#[test]
fn nominal_physical_families_keep_result_list_ordinal_names() {
    let source = br#"struct Wrapped<T> {
  value: T;
}

fn general(value: Wrapped<Box<u64>>, spare: Box<u64>) -> (back: Wrapped<Box<u64>>, spare: Box<u64>) pure {
  return move value, move spare;
}

fn renamed(value: Wrapped<Box<u64>>, spare: Box<u64>) -> (other: Wrapped<Box<u64>>, spare: Box<u64>) pure {
  return move value, move spare;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the wrapper and result declarations must check: {outcome:?}");
        };
        let function = |name: &str| {
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .expect("each declaration has a checked function")
        };
        let (CheckedType::Nominal(general), CheckedType::Nominal(renamed)) =
            (function("general").result, function("renamed").result)
        else {
            panic!("both multi-result types must be nominal");
        };
        assert_ne!(
            checked.data.nominal_physical_alias[general.0 as usize],
            checked.data.nominal_physical_alias[renamed.0 as usize],
            "result-list ordinal names remain part of the family"
        );
    });
}

/// Layout alone cannot erase source declaration identity or phantom generic
/// arguments. Each of these empty structs has the same storage layout.
#[test]
fn nominal_physical_families_preserve_declarations_and_phantom_arguments() {
    let source = br#"struct Marker<T: drop, const n: u64> {
}

struct AlternateMarker<T: drop, const n: u64> {
}

fn base(value: Marker<u8, 1>) -> back: Marker<u8, 1> pure {
  return value;
}

fn element(value: Marker<u16, 1>) -> back: Marker<u16, 1> pure {
  return value;
}

fn count(value: Marker<u8, 2>) -> back: Marker<u8, 2> pure {
  return value;
}

fn declaration(value: AlternateMarker<u8, 1>) -> back: AlternateMarker<u8, 1> pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the empty generic structs must check: {outcome:?}");
        };
        let mut aliases = Vec::new();
        for name in ["base", "element", "count", "declaration"] {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .expect("each declaration has a checked function");
            let CheckedType::Nominal(id) = function.parameters[0].ty else {
                panic!("the input must be an empty nominal");
            };
            let family = checked.data.nominal_physical_alias[id.0 as usize];
            assert!(
                !aliases.contains(&family),
                "{name} must keep a distinct family"
            );
            aliases.push(family);
        }
    });
}

/// This acyclic graph exceeds both former implementation depth cutoffs.
/// Accepted source depth must not change which declarations share a family.
#[test]
fn nominal_physical_families_complete_deep_finite_type_graphs() {
    let mut source = String::from("struct Layer0 {\n  cell: Box<u64>;\n}\n\n");
    for depth in 1..=80 {
        source.push_str(&format!(
            "struct Layer{depth} {{\n  inner: Layer{};\n}}\n\n",
            depth - 1
        ));
    }
    source.push_str(
        "fn first(value: Layer80) -> back: Layer80 pure {\n  return move value;\n}\n\n\
         fn second(value: Layer80) -> back: Layer80 pure {\n  return move value;\n}\n\n\
         fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the finite nominal graph must check: {outcome:?}");
        };
        let ids = ["first", "second"].map(|name| {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .expect("both declarations have checked functions");
            let CheckedType::Nominal(id) = function.parameters[0].ty else {
                panic!("the input must be a Layer80 nominal");
            };
            id
        });
        assert_eq!(ids[0], ids[1], "one written nominal is one instance");
        for aliases in [
            &checked.data.nominal_lowering_alias,
            &checked.data.nominal_physical_alias,
        ] {
            assert_eq!(aliases[ids[0].0 as usize], aliases[ids[1].0 as usize]);
        }
    });
}

/// A back edge is only one obligation: the release class on another field
/// must still be checked even after the recursive pair has been visited.
#[test]
fn nominal_physical_families_complete_cycles_and_check_remaining_fields() {
    let source = br#"enum Tree {
  Leaf();
  Branch(next: Box<Tree>, values: Slots<u64, 2>);
}

fn first(value: Tree) -> back: Tree pure {
  return move value;
}

fn second(value: Tree) -> back: Tree pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the recursive nominal graph must check: {outcome:?}");
        };
        let ids = ["first", "second"].map(|name| {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .expect("each declaration has a checked function");
            let CheckedType::Nominal(id) = function.parameters[0].ty else {
                panic!("the input must be a Tree nominal");
            };
            id
        });
        assert_eq!(ids[0], ids[1]);
        let CheckedNominalKind::Enum { variants } = &checked.data.nominals[ids[0].0 as usize].kind
        else {
            panic!("Tree must remain an enum nominal");
        };
        assert_eq!(variants.len(), 2, "the nonrecursive field is still checked");
        assert_eq!(
            checked.data.nominal_lowering_alias[ids[0].0 as usize],
            checked.data.nominal_lowering_alias[ids[1].0 as usize]
        );
        assert_eq!(
            checked.data.nominal_physical_alias[ids[0].0 as usize],
            checked.data.nominal_physical_alias[ids[1].0 as usize]
        );
    });
}

/// Concrete generic calls are rebuilt after the symbolic inventory is rolled
/// back. A `Box` type argument must retain its referent through that rebuild,
/// including when the call is inside an uncalled ordinary helper.
#[test]
fn generic_replay_preserves_a_box_type_argument() {
    let source = br#"fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn relay(cell: Box<u64>) -> result: Box<u64> pure {
  return pass::<Box<u64>>(value: move cell);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("generic replay must preserve each Box type argument: {outcome:?}");
        };
        let relay = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "relay")
            .expect("the written relay is checked even though main does not call it");
        let box_type = relay.parameters[0].ty;
        let CheckedType::Nominal(id) = box_type else {
            panic!("relay must take a Box nominal");
        };
        let CheckedNominalKind::Box { referent, .. } = checked.data.nominals[id.0 as usize].kind
        else {
            panic!("relay must retain its Box kind");
        };
        assert_eq!(referent, CheckedType::Integer(IntegerType::U64));
        assert_eq!(relay.result, box_type);
        assert!(
            checked.data.functions.iter().any(|function| {
                function.name == "pass"
                    && function.parameters[0].ty == box_type
                    && function.result == box_type
            }),
            "relay's explicit generic argument must produce an exact pass instance"
        );
    });
}

#[test]
fn general_elements_retain_deep_windows_through_generic_replay_and_nominal_fields() {
    let source = br#"struct Wrapped {
  values: Slots<Slots<Slots<u64, 2>, 2>, 2>;
}

fn pass<T: drop>(value: T) -> result: T pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let empty_leaf = slots_new::<u64, 2>();
  place_back(window: &empty_leaf, value: 7_u64);
  let leaf = move empty_leaf;
  let empty_middle = slots_new::<Slots<u64, 2>, 2>();
  place_back(window: &empty_middle, value: move leaf);
  let middle = move empty_middle;
  let empty_outer = slots_new::<Slots<Slots<u64, 2>, 2>, 2>();
  place_back(window: &empty_outer, value: move middle);
  let outer = move empty_outer;
  let returned = pass::<Slots<Slots<Slots<u64, 2>, 2>, 2>>(value: move outer);
  let wrapped = Wrapped(values: move returned);
  let retained = pass::<Wrapped>(value: move wrapped);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("deep window elements and the nominal control must check: {outcome:?}");
        };
        let wrapper = checked
            .data
            .nominals
            .iter()
            .find(|nominal| nominal.name == "Wrapped")
            .expect("the source nominal remains in the ordinary inventory");
        let CheckedNominalKind::Struct { fields } = &wrapper.kind else {
            panic!("the wrapper remains a source struct");
        };
        let mut ty = fields[0].ty;
        for _ in 0..3 {
            let CheckedType::Window {
                shape: WindowShape::Slots,
                element,
                capacity,
            } = ty
            else {
                panic!("every declared nested window must survive the checked graph");
            };
            assert_eq!(capacity, Some(CheckedConst::Value(2)));
            ty = checked
                .element_type(element)
                .expect("a complete checked element");
        }
        assert_eq!(ty, CheckedType::Integer(IntegerType::U64));
    });
}

#[test]
fn general_elements_reify_nominal_children_after_the_schema_checkpoint() {
    let source = br#"struct Pair<T: Int> {
  value: T;
}

fn consume<T: drop>(value: T) -> result: unit pure {
  return unit;
}

fn wrapper<U: drop>() -> result: unit pure {
  let pair = Pair<u8>(value: 7_u8);
  let empty_inner = slots_new::<Pair<u8>, 1>();
  place_back(window: &empty_inner, value: pair);
  let inner = move empty_inner;
  let empty_outer = slots_new::<Slots<Pair<u8>, 1>, 1>();
  place_back(window: &empty_outer, value: move inner);
  let outer = move empty_outer;
  consume::<Slots<Slots<Pair<u8>, 1>, 1>>(value: move outer);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("schema-created nominal element children must be reified: {outcome:?}");
        };
        let consume = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "consume")
            .expect("the schema-written concrete call is retained");
        let mut ty = consume.parameters[0].ty;
        for _ in 0..2 {
            let CheckedType::Window {
                shape: WindowShape::Slots,
                element,
                capacity,
            } = ty
            else {
                panic!("the concrete argument retains both structural layers");
            };
            assert_eq!(capacity, Some(CheckedConst::Value(1)));
            ty = checked
                .element_type(element)
                .expect("a reified element handle");
        }
        let CheckedType::Nominal(id) = ty else {
            panic!("the terminal element remains a source nominal");
        };
        assert!((id.0 as usize) < checked.data.executable_nominal_count);
        let nominal = &checked.data.nominals[id.0 as usize];
        assert!(nominal.name.starts_with("Pair<"));
        let CheckedNominalKind::Struct { fields } = &nominal.kind else {
            panic!("the terminal nominal must retain its original family");
        };
        assert_eq!(fields[0].ty, CheckedType::Integer(IntegerType::U8));
    });
}

#[test]
fn constructor_fields_materialize_the_nominal_instance() {
    let source = br#"struct Wrapped {
  values: Slots<u8, 4>;
}

fn rebuild(values: Slots<u8, 4>) -> result: Slots<u8, 4> pure {
  let wrapped = Wrapped(values: move values);
  let Wrapped(values: restored) = move wrapped;
  return move restored;
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!(
                "the constructor's ordinary field arguments determine its instance: {outcome:?}"
            );
        };
        assert!(
            checked
                .data
                .nominals
                .iter()
                .any(|nominal| nominal.name.starts_with("Wrapped"))
        );
    });
}

/// The constructor's S12 result relation and a header invariant must name
/// the same boxed content descriptor, including in the symbolic instance.
#[test]
fn a_generic_boxed_window_constructor_establishes_its_loop_preheader() {
    let source =
        br#"fn build<T: Int>(count: u64, value: T) -> result: Box<Slots<T>> pure contract {
  requires count <= 4_u64;
  ensures result.inner.len >= count;
} {
  let built = box_slots_new::<T>(capacity: count);
  for (
    at in 0_u64..count,
    invariant grown: built.inner.len >= at,
    invariant bounded: built.inner.len <= at,
    invariant spare: built.inner.cap - built.inner.len + at >= count
  ) {
    place_back(window: &built.inner, value: value);
  }
  return move built;
}

fn main() -> status: std::process::ExitStatus pure {
  let built = build::<u8>(count: 4_u64, value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#;
    // The ordinary path checks the symbolic body as well as the concrete
    // instance; the dark hook below exposes only the latter's retained proof.
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
    super::with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("boxed window source must form: {outcome:?}");
        };
        let functions = checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "build")
            .collect::<Vec<_>>();
        assert!(!functions.is_empty());
        for function in functions {
            assert_eq!(function.entailment.loop_invariants.len(), 3);
            assert!(
                function
                    .entailment
                    .loop_invariants
                    .iter()
                    .all(|invariant| invariant.proof.base && invariant.proof.step == Some(true)),
                "constructor and invariant descriptor identities must agree: {:?}",
                function.entailment
            );
        }
    });
}
