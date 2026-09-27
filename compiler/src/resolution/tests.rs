#![allow(clippy::panic)]

use crate::lexer::{LexLimits, LexOutcome, lex};
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, CanonicalLimits, CanonicalOutcome, FinalizeLimits, FinalizeOutcome,
    ParseLimits, ParseOutcome, SourceBundle, SourceInput, SourceLimits, TerminalLimits,
    TerminalOutcome, audit_canonical, classify_terminals, finalize, parse,
};

use super::catalog::OPERATION_FAMILIES;
use super::{
    DeclarationClass, DeclarationDomain, DeclarationOrigin, DeclarationRole, DeferredUseRole,
    DependentDeclarationRole, LexicalUseRole, PostconditionSelectorClass, ReservedDeclarationRole,
    ResolutionIssue, ResolutionIssueKind, ResolutionOutcome, ResolutionRule, ResolvedTarget,
    ScopeKind, resolve,
};

const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 1_024,
    max_logical_path_bytes: 128,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_binding_bytes: 1_048_576,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 1_024,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_token_bytes: 16_384,
    max_tokens: 131_072,
    max_lexemes: 262_144,
};

const PARSE_LIMITS: ParseLimits = ParseLimits {
    max_work: 8_000_000,
    max_tasks: 131_072,
    max_frames: 8_192,
    max_elements: 262_144,
};

const FINALIZE_LIMITS: FinalizeLimits = FinalizeLimits {
    max_work: 8_000_000,
    max_roots: 131_072,
    max_shape_tasks: 131_072,
    max_nodes: 131_072,
    max_child_edges: 131_072,
    max_terminals: 131_072,
    max_sources: 1_024,
};

const CANONICAL_LIMITS: CanonicalLimits = CanonicalLimits {
    max_work: 8_000_000,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_gaps: 131_072,
    max_path_components: 8_192,
};

fn with_resolution<ResultValue>(
    inputs: &[SourceInput<'_>],
    run: impl FnOnce(ResolutionOutcome) -> ResultValue,
) -> ResultValue {
    with_resolution_sources(inputs, false, run)
}

fn with_resolution_sources<ResultValue>(
    inputs: &[SourceInput<'_>],
    include_prelude: bool,
    run: impl FnOnce(ResolutionOutcome) -> ResultValue,
) -> ResultValue {
    let bundle = if include_prelude {
        SourceBundle::with_prelude(inputs, SOURCE_LIMITS)
    } else {
        SourceBundle::with_limits(inputs, SOURCE_LIMITS)
    }
    .expect("resolver test bundle must be valid");
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("resolver test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("resolver test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("resolver test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("resolver test derivation must finalize");
    };
    let canonical = audit_canonical(*finalized, CANONICAL_LIMITS);
    let CanonicalOutcome::Complete(syntax) = canonical else {
        panic!("resolver test source must use exact FORM-2 formatting: {canonical:?}");
    };
    run(resolve(syntax))
}

fn with_one_resolution<ResultValue>(
    source: &[u8],
    run: impl FnOnce(ResolutionOutcome) -> ResultValue,
) -> ResultValue {
    with_resolution(&[SourceInput::new("test.wf", source)], run)
}

#[test]
fn minimal_function_publishes_the_closed_prelude_and_source_declaration() {
    with_one_resolution(b"fn probe() -> result: unit pure {\n}\n", |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("minimal canonical function must resolve: {outcome:?}");
        };
        assert_eq!(resolved.prelude_declarations().len(), 24);
        assert_eq!(resolved.declarations().len(), 1);
        assert_eq!(resolved.declarations()[0].role(), DeclarationRole::Function);
        assert_eq!(resolved.declarations()[0].spelling(), "probe");
        assert!(resolved.scopes().len() >= 3);
    });
}

#[test]
fn top_level_functions_are_visible_throughout_the_closed_unit() {
    let source = br#"fn probe() -> result: unit pure {
  helper();
}

fn helper() -> result: unit pure {
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("later function must be globally visible: {outcome:?}");
        };
        let helper = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.spelling() == "helper")
            .expect("helper declaration must exist");
        let call = resolved
            .lexical_uses()
            .iter()
            .find(|usage| usage.spelling() == "helper")
            .expect("helper call must exist");
        assert_eq!(call.role(), LexicalUseRole::IdentifierCallee);
        assert_eq!(
            call.target(),
            ResolvedTarget::Source {
                declaration: helper.id(),
                class: DeclarationClass::Function,
            }
        );
    });
}

/// [MOD-3, CONST-2] a module's named consts are visible throughout it; the
/// checker orders their evaluation by dependency and refuses a cycle, so a
/// use of a later const resolves.
#[test]
fn named_constants_are_visible_throughout_their_module() {
    let source = b"const first: i32 = second;\n\nconst second: i32 = 2_i32;\n";
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("a later named constant must be visible: {outcome:?}");
        };
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.role() == LexicalUseRole::ConstValue && usage.spelling() == "second"
        }));
    });
}

#[test]
fn decimal_array_sizes_need_no_lexical_target() {
    let source = br#"const values: Array<i32, 4> =[0_i32, 0_i32, 0_i32, 0_i32];

fn probe() -> result: unit pure {
  return unit;
}
"#;
    // x1 [TYPE-2, PRE-1]: `Array` is a prelude declaration now, so a unit
    // resolved without the prelude has no such nominal to name. The subject
    // here is the const expression's own lexical roles, so the prelude is
    // included and the fixture is otherwise unchanged.
    with_resolution_sources(&[SourceInput::new("test.wf", source)], true, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("a decimal const expression must resolve without a name role: {outcome:?}");
        };
        // The prelude's own declarations name their capacity parameters,
        // so the claim is read over this unit's source file alone, which is
        // the file the subject is written in.
        assert!(
            resolved
                .lexical_uses()
                .iter()
                .filter(|usage| usage.origin().coordinate().source().ordinal() == 0)
                .all(|usage| usage.role() != LexicalUseRole::Const)
        );
    });
}

/// [MOD-3, TYPE-6] a module's nominals are visible independently of item
/// order, exactly as its functions are.
#[test]
fn source_nominals_are_visible_before_their_declaration() {
    let source = br#"fn consume(value: Later) -> result: unit pure {
}

struct Later {
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("a later nominal must be visible: {outcome:?}");
        };
        assert!(
            resolved.lexical_uses().iter().any(|usage| {
                usage.role() == LexicalUseRole::Type && usage.spelling() == "Later"
            })
        );
    });
}

#[test]
fn requires_shape_is_checked_before_names_inside_the_invalid_block() {
    let source = br#"fn guarded() -> result: unit pure contract {
  define value = missing;
} {
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("invalid requires block must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Fn8);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::ContractShape(_)
        ));
    });
}

#[test]
fn contract_structural_admission_selects_the_earliest_source() {
    let first_empty = br#"fn first() -> result: i32 pure contract {
} {
  return 0_i32;
}
"#;
    let second_define_only = br#"fn second() -> result: unit pure contract {
  define unresolved = missing;
} {
  return unit;
}
"#;

    for (first, second) in [
        (first_empty.as_slice(), second_define_only.as_slice()),
        (second_define_only.as_slice(), first_empty.as_slice()),
    ] {
        with_resolution(
            &[
                SourceInput::new("first.wf", first),
                SourceInput::new("second.wf", second),
            ],
            |outcome| {
                let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("an invalid clause block must reject: {outcome:?}");
                };
                assert_eq!(issue.rule(), ResolutionRule::Fn8);
                assert_eq!(issue.origin().coordinate().source().ordinal(), 0);
            },
        );
    }
}

#[test]
fn plain_postcondition_selector_is_private_and_definitions_share_one_contract_scope() {
    let source = br#"fn relation(value: i32) -> result: i32 pure contract {
  define reflexive = value == value;
  requires reflexive;
  ensures result == value;
} {
  return value;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("plain postcondition surface must resolve: {outcome:?}");
        };
        let postcondition = resolved
            .postconditions()
            .first()
            .expect("one private postcondition record");
        assert_eq!(postcondition.class, PostconditionSelectorClass::Plain);
        let [candidate] = postcondition.result_binders.as_slice() else {
            panic!("one result ordinal binder is required");
        };
        assert_eq!(candidate.spelling, "result");
        assert_eq!(candidate.origin.role_ordinal(), 0);
        assert!(candidate.paired_field.is_none());
        assert!(candidate.live_conflicts.is_empty());
        assert!(candidate.later_local_collision.is_none());
        assert!(postcondition.fields.is_empty());
        assert!(postcondition.variant_target.is_none());
        assert!(postcondition.entry_inventory_issue.is_none());
        assert!(postcondition.entry_resolution_issue.is_none());
        assert_eq!(postcondition.selector_uses.len(), 1);
        assert_eq!(postcondition.selector_uses[0].spelling, "result");
        assert!(
            resolved
                .declarations()
                .iter()
                .all(|declaration| declaration.spelling() != "result")
        );
        assert!(
            resolved
                .lexical_uses()
                .iter()
                .all(|usage| usage.spelling() != "result")
        );

        let contracts = resolved
            .scopes()
            .iter()
            .filter(|scope| scope.kind() == ScopeKind::ContractBlock)
            .collect::<Vec<_>>();
        assert_eq!(contracts.len(), 1, "requires and ensures share one scope");
    });
}

#[test]
fn variant_postcondition_selector_preserves_prelude_identity_without_match_roles() {
    for field in ["value", "alternate"] {
        let source = format!(
            "fn selected(value: i32) -> result: Result<i32, i32> pure contract {{\n  ensures when Ok({field}: result): result == value;\n}} {{\n  return Ok<i32, i32>(value: value);\n}}\n"
        );
        with_one_resolution(source.as_bytes(), |outcome| {
            let ResolutionOutcome::Complete(resolved) = outcome else {
                panic!("variant postcondition surface must resolve: {outcome:?}");
            };
            let [postcondition] = resolved.postconditions() else {
                panic!("one private postcondition record is required");
            };
            assert_eq!(postcondition.class, PostconditionSelectorClass::Variant);
            // [CALL-4] the declaration writes one result, so exactly one
            // ordinal binder is a candidate; a routed clause names it
            // through its route rather than as a plain selector.
            assert_eq!(postcondition.result_binders.len(), 1);
            assert!(postcondition.route_ordinal.is_none());
            assert!(matches!(
                postcondition.variant_target,
                Some(ResolvedTarget::Prelude(id)) if id.ordinal() == 11
            ));
            let [selector_field] = postcondition.fields.as_slice() else {
                panic!("one selector field is required");
            };
            assert_eq!(selector_field.spelling, field);
            assert_eq!(selector_field.origin.role_ordinal(), 0);
            assert_eq!(selector_field.candidate.spelling, "result");
            assert_eq!(selector_field.candidate.origin.role_ordinal(), 1);
            assert_eq!(
                selector_field.candidate.paired_field.as_deref(),
                Some(field)
            );
            assert!(
                resolved
                    .declarations()
                    .iter()
                    .all(|declaration| declaration.role() != DeclarationRole::MatchBinder)
            );
            assert!(
                resolved
                    .deferred_uses()
                    .iter()
                    .all(|usage| usage.role() != DeferredUseRole::MatchField)
            );
        });
    }
}

#[test]
fn selector_candidates_use_their_exact_form3_reservation_roles() {
    let plain = br#"fn plain(value: i32) -> cvt: i32 pure contract {
  ensures cvt == value;
} {
  return value;
}
"#;
    let variant = br#"fn variant(value: i32) -> result: Result<i32, i32> pure contract {
  ensures when Ok(value: cvt): cvt == value;
} {
  return Ok<i32, i32>(value: value);
}
"#;
    for (source, expected_role, role_ordinal) in [
        (
            plain.as_slice(),
            ReservedDeclarationRole::PlainResultSelector,
            0,
        ),
        (
            variant.as_slice(),
            ReservedDeclarationRole::VariantResultSelector,
            1,
        ),
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("reserved selector candidate must reject: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Form3);
            assert_eq!(issue.origin().role_ordinal(), role_ordinal);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::ReservedName {
                    spelling,
                    declaration_role,
                    inventory_ordinal: 44,
                    ..
                } if spelling == "cvt" && *declaration_role == expected_role
            ));
        });
    }
}

#[test]
fn postcondition_lookup_waits_for_selector_admission_and_live_conflicts_are_retained() {
    let unresolved = br#"fn unresolved() -> result: unit pure contract {
  ensures result == missing;
} {
  return unit;
}
"#;
    with_one_resolution(unresolved, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("entry lookup must be retained behind selector admission: {outcome:?}");
        };
        let [postcondition] = resolved.postconditions() else {
            panic!("one private postcondition record is required");
        };
        assert!(postcondition.entry_inventory_issue.is_none());
        assert!(postcondition.provisional_uses.is_empty());
        assert!(matches!(
            postcondition.entry_resolution_issue.as_ref(),
            Some(issue)
                if issue.rule() == ResolutionRule::Type5
                    && matches!(
                        issue.kind(),
                        ResolutionIssueKind::UnresolvedUse { spelling, .. }
                            if spelling == "missing"
                    )
        ));
    });

    let inventory_conflict = br#"fn conflict(result: i32) -> result: i32 pure contract {
  ensures result == result;
} {
  return result;
}
"#;
    with_one_resolution(inventory_conflict, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("entry inventory must be retained behind selector admission: {outcome:?}");
        };
        let [postcondition] = resolved.postconditions() else {
            panic!("one private postcondition record is required");
        };
        let [candidate] = postcondition.result_binders.as_slice() else {
            panic!("one result ordinal binder is required");
        };
        assert_eq!(candidate.live_conflicts.len(), 1);
        assert!(candidate.later_local_collision.is_none());
        // The comparison is an operator token since v0.41 and produces no
        // lexical use; only the two `result` operands remain as selector uses.
        assert_eq!(postcondition.provisional_uses.len(), 0);
        assert_eq!(postcondition.selector_uses.len(), 2);
        assert!(postcondition.entry_resolution_issue.is_none());
        assert!(postcondition.entry_inventory_issue.is_none());
    });
}

#[test]
fn invalid_ensures_local_cannot_poison_an_ordinary_body_lookup() {
    let source = br#"fn poisoned(value: i32) -> result: i32 pure contract {
  define cvt = value == value;
  ensures result == value;
} {
  return value;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an invalid contract definition must reject before body lookup: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::ReservedName {
                spelling,
                declaration_role: ReservedDeclarationRole::ContractDefinition,
                ..
            } if spelling == "cvt"
        ));
    });
}

#[test]
fn unresolved_variant_selector_keeps_its_lookup_verdict_before_entry_inventory() {
    let source = br#"fn unresolved(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Missing(value: result): result == value;
} {
  return Ok<i32, Overflow>(value: value);
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!(
                "leading selector lookup must reject before delayed entry inventory: {outcome:?}"
            );
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse {
                spelling,
                role: LexicalUseRole::EnsuresVariant,
                ..
            } if spelling == "Missing"
        ));
    });
}

#[test]
fn contract_definitions_are_shared_across_clauses_but_do_not_reach_the_body() {
    let requires_into_ensures = br#"fn isolated(value: i32) -> result: i32 pure contract {
  define pre = value;
  requires pre == value;
  ensures result == pre;
} {
  return value;
}
"#;
    with_one_resolution(requires_into_ensures, |outcome| {
        let ResolutionOutcome::Complete(_) = outcome else {
            panic!("one shared contract definition must reach both clause kinds: {outcome:?}");
        };
    });

    let ensures_into_body = br#"fn isolated(value: i32) -> result: i32 pure contract {
  define post = value;
  ensures result == post;
} {
  return post;
}
"#;
    with_one_resolution(ensures_into_body, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("ensures local must not reach the body: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::InvisibleUse { spelling, .. } if spelling == "post"
        ));
    });
}

/// [MSR-6] a `pbase` admits an in-scope const generic.
///
/// Until v0.45 this test asserted the opposite: `available: [ConstGeneric]`
/// with the class inadmissible, which is the rejection the containers design
/// recorded as probe `q10`. That rejection is what [MSR-6] removes, so the
/// test now pins the admission and the declaration it resolves to rather than
/// being deleted.
#[test]
fn a_const_generic_resolves_as_an_ordinary_place_base() {
    let ordinary = br#"fn value<const n: u64>() -> result: u64 pure {
  return n;
}

fn probe() -> result: unit pure {
  return unit;
}
"#;
    let postcondition = br#"fn value<const n: u64>() -> result: u64 pure contract {
  ensures result == result;
} {
  return n;
}

fn probe() -> result: unit pure {
  return unit;
}
"#;
    for source in [ordinary.as_slice(), postcondition.as_slice()] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::Complete(resolved) = outcome else {
                panic!("a const generic is an ordinary pbase: {outcome:?}");
            };
            let usage = resolved
                .lexical_uses()
                .iter()
                .find(|usage: &&crate::resolution::LexicalUseRecord| {
                    usage.role() == LexicalUseRole::PlaceBase && usage.spelling() == "n"
                })
                .expect("the place base `n` is a resolved use");
            assert!(matches!(
                usage.target(),
                ResolvedTarget::Source {
                    class: DeclarationClass::ConstGeneric,
                    ..
                }
            ));
        });
    }
}

#[test]
fn fn8_admission_precedes_declaration_inventory() {
    // DIAG-1 fixes the stage order: complete unit-wide FN-8 admission precedes
    // declaration inventory. The FN-8 rejection therefore wins before the
    // complete ordinary declaration collection is installed for lookup.
    let source = br#"fn guarded(value: i32) -> result: i32 pure contract {
  define unresolved = missing;
} {
  return value;
}

fn main() -> result: unit pure {
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the FN-8 defect must outrank declaration inventory: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Fn8);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::ContractShape(_)
        ));
    });

    // The same unit with an admitted internal requirement reaches declaration
    // inventory and resolves normally.
    let admitted = br#"fn guarded(value: i32) -> result: i32 pure contract {
  requires value == value;
} {
  return value;
}

fn main() -> result: unit pure {
  return unit;
}
"#;
    with_one_resolution(admitted, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("an admitted requires block must reach declaration inventory: {outcome:?}");
        };
        assert!(
            resolved
                .declarations()
                .iter()
                .any(|declaration| declaration.spelling() == "guarded")
        );
    });
}

#[test]
fn a_builtin_prelude_collision_reports_the_prelude_origin() {
    // [DIAG-1, PRE-1] a built-in nominal collision reports its prelude origin.
    let source = "fn main() -> result: unit pure {\n  return unit;\n}\n\nstruct Overflow {\n}\n";
    with_one_resolution(source.as_bytes(), |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the prelude collision must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        let ResolutionIssueKind::DeclarationCollision {
            spelling,
            conflicts,
            ..
        } = issue.kind()
        else {
            panic!("expected a declaration collision: {issue:?}");
        };
        assert_eq!(spelling, "Overflow");
        assert!(
            conflicts
                .iter()
                .all(|conflict| matches!(conflict.origin(), DeclarationOrigin::Prelude(_)))
        );
    });
}

#[test]
fn requires_locals_do_not_escape_into_the_function_body() {
    let source = br#"fn guarded() -> result: unit pure contract {
  define condition = 1_i32;
  requires condition == condition;
} {
  return condition;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("requires local must not reach the body: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::InvisibleUse { spelling, .. } if spelling == "condition"
        ));
    });
}

#[test]
fn root_identifier_collisions_are_rejected_in_inventory_order() {
    let source = br#"fn value() -> result: unit pure {
}

const value: i32 = 1_i32;
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("function and const must share the lexical namespace: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "value"
        ));
    });
}

#[test]
fn dotless_operation_names_are_reserved_from_source_declarations() {
    with_one_resolution(b"fn cvt() -> result: unit pure {\n}\n", |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("operation name declaration must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::ReservedName {
                spelling,
                inventory_ordinal: 44,
                ..
            } if spelling == "cvt"
        ));
    });
}

/// OP-1: reservation is derived from the op column, so the six retired
/// comparison names left `DotlessOperationNames` the moment v0.41 respelled
/// their rows as operators, and a program may declare them like any other
/// identifier. The operator spellings themselves are not identifiers and
/// cannot be declared at all, so nothing is reserved on their behalf.
#[test]
fn every_retired_comparison_name_is_a_free_identifier() {
    for spelling in ["ieq", "ine", "ilt", "ile", "igt", "ige"] {
        let source = format!("fn {spelling}() -> result: unit pure {{\n}}\n");
        with_one_resolution(source.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "a retired comparison name must be declarable: {outcome:?}"
            );
        });
    }
}

// OP-1 reserves operation and mode names only in its listed declaration
// roles. Proof-only invariant declarations select their own lookup domain.
#[test]
fn operation_and_mode_names_resolve_as_header_and_body_invariants() {
    for spelling in ["cvt", "wrap", "defined", "checked", "sat", "strict"] {
        for header in [false, true] {
            let declaration = if header {
                format!(
                    "  for (\n    index in 0_u64..limit,\n    invariant {spelling}: index <= limit\n  ) {{\n"
                )
            } else {
                format!("  invariant {spelling}: limit <= limit;\n")
            };
            let indent = if header { "    " } else { "  " };
            let close = if header { "    break;\n  }\n" } else { "" };
            let source = format!(
                "fn probe(limit: u64) -> result: unit pure {{\n{declaration}{indent}invariant scaled: 3_u64 * limit <= 3_u64 * limit {{\n{indent}  use 3 times {spelling};\n{indent}}}\n{close}  return unit;\n}}\n"
            );
            with_one_resolution(source.as_bytes(), |outcome| {
                let ResolutionOutcome::Complete(resolved) = outcome else {
                    panic!("proof-only spelling must resolve: {outcome:?}");
                };
                let declaration = resolved
                    .declarations()
                    .iter()
                    .find(|declaration| {
                        declaration.role() == DeclarationRole::Invariant
                            && declaration.spelling() == spelling
                    })
                    .expect("invariant declaration exists");
                let usage = resolved
                    .lexical_uses()
                    .iter()
                    .find(|usage| {
                        usage.role() == LexicalUseRole::InvariantFact
                            && usage.spelling() == spelling
                    })
                    .expect("named premise exists");
                assert_eq!(
                    usage.target(),
                    ResolvedTarget::Source {
                        declaration: declaration.id(),
                        class: DeclarationClass::Invariant,
                    }
                );
            });
        }
    }
}

// Retired with v0.60: `region_names_are_unique_across_the_complete_function`
// asserted [OWN-3]'s per-function region-name uniqueness through
// `ResolutionRule::Own3` and `ResolutionIssueKind::RepeatedRegion`. v0.60
// deletes [OWN-3] and the `region_stmt`, `region_params` and REGIONID
// productions with it, so the rejection it exercised has no subject, no rule
// to cite and no spelling to write. The obligation it guarded — that a
// declaration's own names do not silently share one unit-wide scope — is now
// carried by the generic-parameter half of [TYPE-6], covered by
// `sibling_member_signatures_do_not_share_parameter_names` below. No
// successor rule inherits region uniqueness, so nothing replaces this case.

/// x1 deletes the eight-name reservation. [FORM-3] no longer takes `len`,
/// `cap`, `head`, `next`, `last`, `filled` or `free` away from a declaration:
/// the first three are the readonly fields [PRE-1] declares on the storage
/// shapes and the last four are effect-row vocabulary selected by the window
/// type of the place they follow [TYPE-10], so a writer may spell a
/// parameter, a field or a binding any of them.
///
/// Retires `measure_and_window_part_names_are_reserved_from_source_declarations`,
/// whose successor is this acceptance over the same three declaration roles.
#[test]
fn measure_and_window_part_names_are_ordinary_source_declarations() {
    for source in [
        &b"fn probe(len: u64) -> result: unit pure {\n  return unit;\n}\n"[..],
        &b"struct Holder {\n  cap: u64;\n  next: u64;\n}\n"[..],
        &b"fn probe() -> result: unit pure {\n  let filled = 0_u64;\n  return unit;\n}\n"[..],
    ] {
        with_one_resolution(source, |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "a measure or part spelling is an ordinary declaration: {outcome:?}"
            );
        });
    }
}

/// [GRAM-2] admits a `heap_decl` "at most once in a compilation unit and only
/// as the first `item` of the first source record". The grammar itself admits
/// one at every item position, so resolution owns the unit-level judgment.
#[test]
fn a_heap_declaration_is_admitted_only_as_the_leading_item() {
    with_one_resolution(
        b"program no_heap;\n\nfn probe() -> result: unit pure {\n  return unit;\n}\n",
        |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "a leading heap declaration is admitted: {outcome:?}"
            );
        },
    );
    with_one_resolution(
        b"fn probe() -> result: unit pure {\n  return unit;\n}\n\nprogram no_heap;\n",
        |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("a later heap declaration must reject: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Gram2);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::MisplacedHeapDeclaration { admitted: None }
            ));
        },
    );
    with_one_resolution(b"program no_heap;\n\nprogram no_heap;\n", |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a second heap declaration must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Gram2);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::MisplacedHeapDeclaration { admitted: Some(_) }
        ));
    });
}

/// [SET-1] "A `set` whose target name resolves to nothing declares nothing and
/// is a hard error citing SET-1 at that `place`." v0.59's [LIV-2] promoted
/// exactly this target into a `let` declaration instead, which is what the
/// repair now tells the writer to do [DIAG-1].
#[test]
fn an_unresolved_bare_set_target_declares_nothing_and_cites_set1() {
    let source = br#"fn probe() -> result: unit pure {
  set missing = 0_u64;
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unresolvable set target must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Set1);
        assert_eq!(
            issue.kind(),
            &ResolutionIssueKind::UndeclaredSetTarget {
                spelling: "missing".to_owned(),
                mechanical_fix: "no binding `missing` is in scope, so this `set` declares nothing: write it as `let missing = ...;`, keeping its right-hand side, to declare the binding here, or name a binding that is in scope".to_owned(),
            }
        );
    });
}

#[test]
fn a_break_label_must_lexically_enclose_the_break() {
    let source = br#"fn probe() -> result: unit pure {
  loop @done {
    break @done;
  }
  break @done;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("out-of-scope label must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::NonEnclosingLabel { spelling, .. } if spelling == "@done"
        ));
    });
}

#[test]
fn counted_range_binder_and_label_are_visible_only_in_the_body() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..limit) {
    let copied = index;
    break @range;
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("counted binder and label must resolve in their body: {outcome:?}");
        };
        let binder = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.role() == DeclarationRole::CountedBinder)
            .expect("counted binder declaration must exist");
        assert_eq!(binder.spelling(), "index");
        let label = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::LoopLabel
                    && declaration.spelling() == "@range"
            })
            .expect("counted label declaration must exist");
        let binder_use = resolved
            .lexical_uses()
            .iter()
            .find(|usage| usage.spelling() == "index")
            .expect("body must use the counted binder");
        assert_eq!(binder_use.role(), LexicalUseRole::PlaceBase);
        assert_eq!(
            binder_use.target(),
            ResolvedTarget::Source {
                declaration: binder.id(),
                class: DeclarationClass::Value,
            }
        );
        let break_use = resolved
            .lexical_uses()
            .iter()
            .find(|usage| usage.spelling() == "@range")
            .expect("body break must use the counted label");
        assert_eq!(break_use.role(), LexicalUseRole::BreakLabel);
        assert_eq!(
            break_use.target(),
            ResolvedTarget::Source {
                declaration: label.id(),
                class: DeclarationClass::Label,
            }
        );
    });
}

#[test]
fn unlabeled_loops_keep_the_counted_binder_without_creating_label_records() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  loop {
    break;
  }
  for (
    index in 0_u64..limit,
    invariant ceiling: index <= limit
  ) {
    break;
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("unlabeled loop forms must resolve structurally: {outcome:?}");
        };
        assert!(
            resolved
                .declarations()
                .iter()
                .all(|declaration| declaration.role() != DeclarationRole::LoopLabel)
        );
        assert!(
            resolved
                .lexical_uses()
                .iter()
                .all(|usage| usage.role() != LexicalUseRole::BreakLabel)
        );

        let binder = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.role() == DeclarationRole::CountedBinder)
            .expect("an unlabeled counted loop still declares its binder");
        assert_eq!(binder.spelling(), "index");
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.role() == LexicalUseRole::InvariantValue
                && usage.spelling() == "index"
                && usage.target()
                    == ResolvedTarget::Source {
                        declaration: binder.id(),
                        class: DeclarationClass::Value,
                    }
        }));
    });
}

#[test]
fn invariant_names_declare_facts_and_affine_locals_resolve_as_invariant_values() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (
    index in 0_u64..limit,
    invariant ceiling: index <= limit
  ) {
    break @range;
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("invariant value uses must resolve in the counted body: {outcome:?}");
        };
        let binder = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.role() == DeclarationRole::CountedBinder)
            .expect("counted binder declaration exists");
        let parameter = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Parameter
                    && declaration.spelling() == "limit"
            })
            .expect("limit parameter declaration exists");
        let invariant_uses = resolved
            .lexical_uses()
            .iter()
            .filter(|usage| usage.role() == LexicalUseRole::InvariantValue)
            .collect::<Vec<_>>();
        assert_eq!(invariant_uses.len(), 2);
        assert_eq!(invariant_uses[0].spelling(), "index");
        assert_eq!(
            invariant_uses[0].target(),
            ResolvedTarget::Source {
                declaration: binder.id(),
                class: DeclarationClass::Value,
            }
        );
        assert_eq!(invariant_uses[1].spelling(), "limit");
        assert_eq!(
            invariant_uses[1].target(),
            ResolvedTarget::Source {
                declaration: parameter.id(),
                class: DeclarationClass::Value,
            }
        );
        assert!(resolved.declarations().iter().any(|declaration| {
            declaration.role() == DeclarationRole::Invariant && declaration.spelling() == "ceiling"
        }));
        assert!(
            resolved
                .lexical_uses()
                .iter()
                .all(|usage| usage.spelling() != "ile"),
            "the invariant relation carrier must not create a lexical use"
        );
    });
}

#[test]
fn an_unresolved_affine_local_is_reported_as_an_invariant_value() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (
    index in 0_u64..limit,
    invariant ceiling: index <= missing
  ) {
    break @range;
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unresolved invariant value must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Inv1);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse {
                spelling,
                role: LexicalUseRole::InvariantValue,
                ..
            } if spelling == "missing"
        ));
    });
}

#[test]
fn counted_range_binder_is_invisible_in_both_endpoints_and_after_the_loop() {
    for source in [
        br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in index..limit) {
    break @range;
  }
  return unit;
}
"#
        .as_slice(),
        br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..index) {
    break @range;
  }
  return unit;
}
"#
        .as_slice(),
        br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..limit) {
    break @range;
  }
  let after = index;
  return unit;
}
"#
        .as_slice(),
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("counted binder must be invisible outside its body: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Type5);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::InvisibleUse { spelling, .. } if spelling == "index"
            ));
        });
    }
}

#[test]
fn counted_range_endpoints_with_an_outer_same_name_still_enforce_no_shadowing() {
    for source in [
        br#"fn probe(limit: u64) -> result: unit pure {
  let index = 0_u64;
  for @range (index in index..limit) {
    break @range;
  }
  return unit;
}
"#
        .as_slice(),
        br#"fn probe(limit: u64) -> result: unit pure {
  let index = 0_u64;
  for @range (index in 0_u64..index) {
    break @range;
  }
  return unit;
}
"#
        .as_slice(),
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("the live outer declaration must still enforce no-shadowing: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Type6);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::DeclarationCollision { spelling, .. }
                    if spelling == "index"
            ));
        });
    }
}

#[test]
fn invariant_fact_names_resolve_only_after_their_complete_declaration() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  for (
    index in 0_u64..limit,
    invariant ceiling: index <= limit
  ) {
    invariant repeated: index <= limit {
      use ceiling;
      use (index <= limit);
    }
    invariant chained: index <= limit {
      use repeated;
    }
    break;
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("published invariant names must resolve in their dominance region: {outcome:?}");
        };
        let named = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Invariant
                    && declaration.spelling() == "ceiling"
            })
            .expect("header invariant declaration exists");
        let local = resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.role() == DeclarationRole::Invariant
                    && declaration.spelling() == "repeated"
            })
            .expect("local invariant declaration exists");
        for (spelling, declaration) in [("ceiling", named.id()), ("repeated", local.id())] {
            let usage = resolved
                .lexical_uses()
                .iter()
                .find(|usage| {
                    usage.role() == LexicalUseRole::InvariantFact && usage.spelling() == spelling
                })
                .expect("named use must become an invariant-fact use");
            assert_eq!(
                usage.target(),
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Invariant,
                }
            );
        }
        assert_eq!(
            resolved
                .lexical_uses()
                .iter()
                .filter(|usage| usage.role() == LexicalUseRole::InvariantValue)
                .count(),
            6
        );
        assert_eq!(
            resolved
                .lexical_uses()
                .iter()
                .filter(|usage| usage.role() == LexicalUseRole::ProofValue)
                .count(),
            2
        );
    });

    let self_reference = br#"fn probe(value: i32) -> result: unit pure {
  invariant same: value <= value {
    use same;
  }
  return unit;
}
"#;
    with_one_resolution(self_reference, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an invariant name must not resolve inside its own certificate: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Inv1);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::InvisibleUse {
                spelling,
                role: LexicalUseRole::InvariantFact,
                ..
            } if spelling == "same"
        ));
    });
}

#[test]
fn an_unresolved_relation_use_value_is_reported_by_prf1() {
    let source = br#"fn probe(value: u64, limit: u64) -> result: unit pure {
  invariant scaled: 3_u64 * value <= 3_u64 * limit {
    use (value <= missing);
  }
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an unresolved use relation value must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Prf1);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse {
                spelling,
                role: LexicalUseRole::ProofValue,
                ..
            } if spelling == "missing"
        ));
    });
}

#[test]
fn repeated_header_and_local_invariant_names_are_reported_by_inv1() {
    for source in [
        br#"fn probe(value: u64) -> result: unit pure {
  invariant same: value <= value;
  invariant same: value <= value;
  return unit;
}
"#
        .as_slice(),
        br#"fn probe(value: u64) -> result: unit pure {
  loop (
    invariant same: value <= value,
    invariant same: value <= value
  ) {
    break;
  }
  return unit;
}
"#
        .as_slice(),
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("a repeated invariant name must reject: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Inv1);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::DeclarationCollision { spelling, .. }
                    if spelling == "same"
            ));
        });
    }
}

#[test]
fn header_invariant_names_are_invisible_after_their_loop() {
    for source in [
        br#"fn probe(limit: u64) -> result: unit pure {
  for (
    index in 0_u64..limit,
    invariant ceiling: index <= limit
  ) {
  }
  invariant after: 0_u64 <= limit {
    use ceiling;
  }
  return unit;
}
"#
        .as_slice(),
        br#"fn probe(value: u64) -> result: unit pure {
  loop (
    invariant stable: value <= value
  ) {
    break;
  }
  invariant after: value <= value {
    use stable;
  }
  return unit;
}
"#
        .as_slice(),
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("a header invariant name must end with its loop: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Inv1);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::InvisibleUse {
                    role: LexicalUseRole::InvariantFact,
                    ..
                }
            ));
        });
    }
}

#[test]
fn counted_range_label_is_non_enclosing_after_the_loop() {
    let source = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..limit) {
    break @range;
  }
  break @range;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("counted label must not escape its loop: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::NonEnclosingLabel { spelling, .. } if spelling == "@range"
        ));
    });
}

#[test]
fn counted_range_binder_uses_the_for_binder_reservation_role() {
    for name in ["cvt", "wrap", "defined", "checked", "sat", "strict"] {
        for label in ["", " @range"] {
            let source = format!(
                "fn probe(limit: u64) -> result: unit pure {{\n  for{label} ({name} in 0_u64..limit) {{\n    break{label};\n  }}\n  return unit;\n}}\n"
            );
            with_one_resolution(source.as_bytes(), |outcome| {
                let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("reserved counted binder {name} must reject: {outcome:?}");
                };
                assert_eq!(issue.rule(), ResolutionRule::Form3);
                assert!(matches!(
                    issue.kind(),
                    ResolutionIssueKind::ReservedName {
                        spelling,
                        declaration_role: ReservedDeclarationRole::ForBinder,
                        ..
                    } if spelling == name
                ));
            });
        }
    }
}

#[test]
fn counted_range_scope_rejects_live_shadowing_and_allows_expired_reuse() {
    let live_outer = br#"fn probe(limit: u64) -> result: unit pure {
  let index = 0_u64;
  for @range (index in 0_u64..limit) {
    break @range;
  }
  return unit;
}
"#;
    with_one_resolution(live_outer, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("counted binder must not shadow a live outer binding: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "index"
        ));
    });

    let nested = br#"fn probe(limit: u64) -> result: unit pure {
  for @outer (index in 0_u64..limit) {
    for @inner (index in 0_u64..limit) {
      break @inner;
    }
    break @outer;
  }
  return unit;
}
"#;
    with_one_resolution(nested, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("nested counted binder must not shadow its live parent: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "index"
        ));
    });

    let nested_distinct = br#"fn probe(limit: u64) -> result: unit pure {
  for @outer (outer_index in 0_u64..limit) {
    for @inner (inner_index in outer_index..limit) {
      let copied = inner_index;
      break @inner;
    }
    break @outer;
  }
  return unit;
}
"#;
    with_one_resolution(nested_distinct, |outcome| {
        assert!(
            matches!(outcome, ResolutionOutcome::Complete(_)),
            "distinct nested counted declarations must resolve: {outcome:?}"
        );
    });

    let reused = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..limit) {
    break @range;
  }
  for @range (index in 0_u64..limit) {
    break @range;
  }
  let index = 7_u64;
  let copied = index;
  return unit;
}
"#;
    with_one_resolution(reused, |outcome| {
        assert!(
            matches!(outcome, ResolutionOutcome::Complete(_)),
            "expired counted binder and label spellings may be reused: {outcome:?}"
        );
    });
}

/// Whether a family is spelled as an operator rather than as a callee name.
///
/// [OP-7] respelled twenty rows, and an operator token is never a declaration,
/// callee IDENT, or OPNAME [OP-1], so the two halves of the inventory now
/// resolve by different mechanisms and cannot be asserted together.
fn is_operator_family(spelling: &str) -> bool {
    !spelling.as_bytes()[0].is_ascii_alphabetic()
}

#[test]
fn dotless_and_dotted_operations_resolve_by_exact_op1_spelling() {
    let source = br#"fn probe() -> result: unit pure {
  let negated = ineg(1_i32);
  let smaller = imin(negated, 2_i32);
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("closed operations must resolve: {outcome:?}");
        };
        for spelling in ["ineg", "imin"] {
            let usage = resolved
                .lexical_uses()
                .iter()
                .find(|usage| usage.spelling() == spelling)
                .unwrap_or_else(|| panic!("missing operation use {spelling}"));
            assert!(matches!(usage.target(), ResolvedTarget::Operation(_)));
        }
    });
}

/// [OP-1] "infix resolution consults no name domain, and an operator token is
/// never a declaration, callee IDENT, or OPNAME". The respelling therefore did
/// not move these rows to a different name — it took them out of the name
/// domain entirely, which is a property worth a gate of its own.
///
/// Since v0.41 the integer comparisons cross this line with the arithmetic:
/// `==` is a second respelled subject, and `imin` is the one named control.
#[test]
fn a_respelled_family_produces_no_lexical_use_at_all() {
    let source = br#"fn probe() -> result: unit pure {
  let sum = 1_i32 +wrap 2_i32;
  let equal = sum == 3_i32;
  let named = imin(sum, 3_i32);
  return unit;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("infix operations must resolve: {outcome:?}");
        };
        let mut operations: Vec<_> = resolved
            .lexical_uses()
            .iter()
            .filter(|usage| matches!(usage.target(), ResolvedTarget::Operation(_)))
            .map(|usage| usage.spelling())
            .collect();
        operations.sort_unstable();
        // The named row in the same function is the control: it proves the
        // filter finds operation uses at all, so the two infix rows being
        // absent is the property and not an empty search.
        assert_eq!(operations, ["imin"]);
        assert!(
            !resolved
                .lexical_uses()
                .iter()
                .any(|usage| usage.spelling() == "=="),
            "a respelled comparison must produce no lexical use"
        );
        assert!(
            !resolved
                .lexical_uses()
                .iter()
                .any(|usage| usage.spelling() == "+wrap"),
            "a respelled family must produce no lexical use"
        );
    });
}

#[test]
fn match_binder_cannot_equal_its_paired_field_name() {
    let source = br#"fn probe() -> result: unit pure {
  match unit {
    Some(value: value) => {
      return unit;
    }
  }
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("non-fresh match binder must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Gram10);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::MatchBinderFreshness { spelling, .. } if spelling == "value"
        ));
    });
}

/// [TYPE-6] an arm label resolves against its scrutinee's enum type, so
/// resolution defers it to the checker and never selects a constructor of
/// the same spelling.
#[test]
fn arm_labels_wait_for_their_scrutinee_type() {
    let source = br#"struct Boxed {
}

fn probe() -> result: unit pure {
  match unit {
    Boxed() => {
      return unit;
    }
  }
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("an arm label is deferred to the checker: {outcome:?}");
        };
        assert!(resolved.deferred_uses().iter().any(|usage| {
            usage.role() == DeferredUseRole::ArmVariant && usage.spelling() == "Boxed"
        }));
        assert!(
            !resolved
                .lexical_uses()
                .iter()
                .any(|usage| usage.spelling() == "Boxed")
        );
    });
}

/// The fixture follows the language rather than the assertions moving.
/// `OperationCallee` is the OPNAME form specifically (`roles.rs` keys it on
/// `TerminalPredicate::OperationName`), so it rides `iabs.checked`, a dotted
/// [OP-1] row that keeps its operation-name route; an operator token is never
/// a callee and produces no lexical use at all.
///
/// v0.60 deletes the five region use roles and the two region declaration
/// roles with [OWN-3], [OWN-2] and [FORM-8], and adds two: `EffectIndex` for
/// the index and range endpoints [EFF-1] now admits inside an effect path, and
/// `PayloadVariant` for the variant TYPEID of the enum-payload `psuffix`
/// [GRAM-5] adds. Both are materialized below.
///
/// The effect path's two endpoint parameters are spelled `lower` and `upper`
/// because [FORM-3] reserves the eight measure and part names from every
/// declaration role and `last` is one of them. The role under test is the
/// endpoint position, which any ordinary IDENT occupies, so the rename
/// changes nothing the fixture exists to materialize.
#[test]
fn complete_role_fixture_materializes_every_declaration_use_and_deferred_family() {
    let source = br#"interface Bound {
  fn member(value: &i32) -> result: i32 reads(value);
}

interface Numeric<T: Int> {
  fn zero() -> result: T pure;
}

struct Package<T: drop, const n: i32> {
  items: Array<T, n>;
}

struct Holder {
  output: i32;
  table: Array<i32, 4>;
}

enum Choice<T: drop> {
  Absent();
  Present(value: T);
}

const one: i32 = 1_i32;

const two: i32 = one;

fn implementation(value: i32) -> result: i32 pure {
  return value;
}

binding Implementation : Bound {
  member = implementation;
}

fn user<T: drop, const n: i32>(arg: &T) -> result: T reads(arg) {
  return arg;
}

fn grouped<interface Bound>() -> result: i32 pure {
  let called = Bound::member(value: 1_i32);
  return called;
}

fn adjust(holder: &Holder, lower: u64, upper: u64) -> result: unit reads(holder.table[lower..upper]), writes(holder.output) {
  return unit;
}

fn numeric<T: Int>() -> result: T pure {
  return 0_T;
}

fn probe() -> result: unit pure {
  let ordinary = 1_i32 +wrap two;
  let smaller = iabs.checked(ordinary);
  let made = Package<i32, one>(items: ordinary);
  set made.items = ordinary;
  let borrowed = &ordinary;
  let called = user::<i32, one>(arg: borrowed);
  let taken = move called;
  let comparison = ordinary == two;
  let chosen = Choice::Present(value: ordinary);
  let payload = chosen.Present.value;
  loop @done {
    break @done;
  }
  for @counted (index in 0_u64..2_u64) {
    let observed = index;
    break @counted;
  }
  match ordinary {
    Present(value: held) => {
      give held;
    }
    Absent() => {
      return unit;
    }
  }
}
"#;
    // x1 [TYPE-2, PRE-1]: the fixture names `Array`, which is a prelude
    // declaration now rather than a compiler-owned table row, so the unit is
    // resolved with the prelude. Every role the fixture exercises is a source
    // role and none of them is the prelude's.
    with_resolution_sources(&[SourceInput::new("test.wf", source)], true, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("complete role fixture must resolve: {outcome:?}");
        };

        let declaration_roles: Vec<_> = resolved
            .declarations()
            .iter()
            .map(|declaration| declaration.role())
            .collect();
        for role in [
            DeclarationRole::Function,
            DeclarationRole::Struct,
            DeclarationRole::Enum,
            DeclarationRole::Variant,
            DeclarationRole::Interface,
            DeclarationRole::Binding,
            DeclarationRole::FunctionParameter,
            DeclarationRole::NamedConst,
            DeclarationRole::GenericType,
            DeclarationRole::ConstGeneric,
            DeclarationRole::Parameter,
            DeclarationRole::Let,
            DeclarationRole::LoopLabel,
            DeclarationRole::MatchBinder,
            DeclarationRole::CountedBinder,
        ] {
            assert!(
                declaration_roles.contains(&role),
                "missing declaration role {role:?}"
            );
        }

        let dependent_roles: Vec<_> = resolved
            .dependent_declarations()
            .iter()
            .map(|declaration| declaration.role())
            .collect();
        for role in [
            DependentDeclarationRole::Field,
            DependentDeclarationRole::VariantField,
        ] {
            assert!(
                dependent_roles.contains(&role),
                "missing dependent role {role:?}"
            );
        }

        let lexical_roles: Vec<_> = resolved
            .lexical_uses()
            .iter()
            .map(|usage| usage.role())
            .collect();
        for role in [
            LexicalUseRole::Type,
            LexicalUseRole::GenericBound,
            LexicalUseRole::FormalGroup,
            LexicalUseRole::TypeArgument,
            LexicalUseRole::Construct,
            LexicalUseRole::VariantOwner,
            LexicalUseRole::EffectRoot,
            LexicalUseRole::EffectIndex,
            LexicalUseRole::BreakLabel,
            LexicalUseRole::Const,
            LexicalUseRole::ConstValue,
            LexicalUseRole::PlaceBase,
            LexicalUseRole::IdentifierCallee,
            LexicalUseRole::OperationCallee,
            LexicalUseRole::FunctionBinding,
            LexicalUseRole::GenericNumericSuffix,
        ] {
            assert!(
                lexical_roles.contains(&role),
                "missing lexical role {role:?}"
            );
        }

        let deferred_roles: Vec<_> = resolved
            .deferred_uses()
            .iter()
            .map(|usage| usage.role())
            .collect();
        for role in [
            DeferredUseRole::FieldInitializer,
            DeferredUseRole::MatchField,
            DeferredUseRole::ArmVariant,
            DeferredUseRole::ProjectedField,
            DeferredUseRole::PayloadVariant,
            DeferredUseRole::FunctionBinding,
            DeferredUseRole::FunctionMember,
            DeferredUseRole::EffectField,
        ] {
            assert!(
                deferred_roles.contains(&role),
                "missing deferred role {role:?}"
            );
        }

        // Numeric literals retain their own suffix use, and qualified calls
        // add the deferred member use above.
        let suffix = resolved
            .lexical_uses()
            .iter()
            .find(|usage| usage.role() == LexicalUseRole::GenericNumericSuffix)
            .expect("generic literal suffix must resolve");
        assert_eq!(suffix.origin().subtoken_ordinal(), 1);

        // [EFF-1] `reads(holder.table[lower..upper])` roots at the reference
        // parameter, selects one field below it, and supplies both endpoints
        // as value parameters of the same callable.
        let indices: Vec<_> = resolved
            .lexical_uses()
            .iter()
            .filter(|usage| usage.role() == LexicalUseRole::EffectIndex)
            .map(|usage| usage.spelling().to_owned())
            .collect();
        assert_eq!(indices, vec!["lower".to_owned(), "upper".to_owned()]);
    });
}

#[test]
fn effect_paths_resolve_the_exact_formal_parameter_and_retain_fields() {
    // [EFF-1] every `effect_path` is rooted at a reference parameter, so the
    // fixture's root is `&Holder` and not the v0.59 `own Holder`.
    let source = b"struct Holder {\n  output: i32;\n}\n\nfn publish(holder: &Holder) -> result: unit writes(holder.output) {\n  return unit;\n}\n";
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("a reference-parameter-rooted state path must resolve: {outcome:?}");
        };
        let parameter = resolved
            .declarations()
            .iter()
            .find(|declaration| declaration.role() == DeclarationRole::Parameter)
            .expect("the parameter declaration exists");
        let usage = resolved
            .lexical_uses()
            .iter()
            .find(|usage| usage.role() == LexicalUseRole::EffectRoot)
            .expect("the effect root use exists");
        assert_eq!(usage.spelling(), "holder");
        assert_eq!(
            usage.target(),
            ResolvedTarget::Source {
                declaration: parameter.id(),
                class: DeclarationClass::Value,
            }
        );
        let field = resolved
            .deferred_uses()
            .iter()
            .find(|usage| usage.role() == DeferredUseRole::EffectField)
            .expect("the static effect field is retained");
        assert_eq!(field.spelling(), "output");
    });
}

#[test]
fn unresolved_and_body_local_effect_targets_reject_under_eff1() {
    for source in [
        &b"fn probe() -> result: unit reads(missing) {\n  return unit;\n}\n"[..],
        &b"fn probe() -> result: unit reads(local) {\n  let local = 0_u64;\n  return unit;\n}\n"[..],
    ] {
        with_one_resolution(source, |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("a non-formal effect target must reject in resolution: {outcome:?}");
            };
            assert_eq!(issue.rule(), ResolutionRule::Eff1);
            assert!(matches!(
                issue.kind(),
                ResolutionIssueKind::UnresolvedUse {
                    role: LexicalUseRole::EffectRoot,
                    ..
                } | ResolutionIssueKind::InvisibleUse {
                    role: LexicalUseRole::EffectRoot,
                    ..
                }
            ));
        });
    }
}

#[test]
fn existing_positive_conformance_programs_resolve_without_fixture_rewrites() {
    for source in [
        include_bytes!("../../../tests/conformance/cases/fn3-pos-contract-conform.wf").as_slice(),
        include_bytes!("../../../tests/conformance/cases/ex1-pos-worked-example.wf").as_slice(),
        include_bytes!("../../../tests/conformance/cases/gram10-pos-named-binders.wf").as_slice(),
    ] {
        with_resolution_sources(
            &[SourceInput::new("conformance.wf", source)],
            true,
            |outcome| {
                assert!(
                    matches!(outcome, ResolutionOutcome::Complete(_)),
                    "positive compiler-independent conformance source must resolve: {outcome:?}"
                );
            },
        );
    }
}

#[test]
fn existing_requires_scope_conformance_case_reaches_type5_resolution() {
    let source =
        include_bytes!("../../../tests/conformance/cases/fn8-neg-requires-local-in-body.wf");
    // The case names the standard library's exit status, which a unit with
    // the prelude carries [MOD-10].
    with_resolution_sources(&[SourceInput::new("test.wf", source)], true, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("requires-scope conformance case must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
    });
}

#[test]
fn prelude_collision_payload_keeps_both_ordered_struct_domains() {
    with_one_resolution(b"struct Overflow {\n}\n", |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("prelude collision must reject: {outcome:?}");
        };
        let ResolutionIssueKind::DeclarationCollision { conflicts, .. } = issue.kind() else {
            panic!("expected a declaration collision: {issue:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].domain(), DeclarationDomain::NominalType);
        assert_eq!(conflicts[1].domain(), DeclarationDomain::Constructor);
        // This unit is resolved without the parsed prelude, so the inventory
        // is the built-in catalog alone and `Overflow`'s two records keep the
        // ordinals that catalog gives them [PRE-1, DIAG-1].
        assert!(
            matches!(conflicts[0].origin(), DeclarationOrigin::Prelude(id) if id.ordinal() == 15)
        );
        assert!(
            matches!(conflicts[1].origin(), DeclarationOrigin::Prelude(id) if id.ordinal() == 16)
        );
    });
}

#[test]
fn duplicate_main_conformance_case_is_type6() {
    let source = include_bytes!("../../../tests/conformance/cases/fn7-neg-two-mains.wf");
    with_resolution_sources(&[SourceInput::new("test.wf", source)], true, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the later main declaration must be rejected: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, conflicts, .. }
                if spelling == "main" && conflicts.len() == 1
        ));
    });
}

#[test]
fn nested_declarations_cannot_shadow_source_later_global_functions() {
    let source = br#"fn probe() -> result: unit pure {
  let future = 1_i32;
  return unit;
}

fn future() -> result: unit pure {
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("whole-unit function visibility must prevent shadowing: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "future"
        ));
    });
}

/// [TYPE-6] a `fn_sig` parameter "is not visible in a sibling member or the
/// receiving function's body". v0.59 exercised this with a region parameter of
/// a sibling member; regions are gone, so the same judgment is read off the
/// parameter itself, which is the only owner-local name a member signature
/// still declares.
#[test]
fn sibling_member_signatures_do_not_share_parameter_names() {
    let source = br#"interface Separate {
  fn first(value: &i32) -> result: unit reads(value);
  fn second() -> result: unit reads(value);
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("sibling member parameter must not participate: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Eff1);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse { spelling, .. } if spelling == "value"
        ));
    });
}

#[test]
fn interface_import_and_unbounded_type_parameter_have_distinct_roles() {
    let source = br#"interface Source {
}

fn grouped<interface Source, T>(value: T) -> result: T pure {
  return move value;
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("explicit interface import must resolve: {outcome:?}");
        };
        assert!(resolved.declarations().iter().any(|declaration| {
            declaration.spelling() == "T" && declaration.role() == DeclarationRole::GenericType
        }));
        assert!(!resolved.declarations().iter().any(|declaration| {
            declaration.spelling() == "Source" && declaration.role() == DeclarationRole::GenericType
        }));
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.spelling() == "Source"
                && usage.role() == LexicalUseRole::FormalGroup
                && matches!(
                    usage.target(),
                    ResolvedTarget::Source {
                        class: DeclarationClass::Interface,
                        ..
                    }
                )
        }));
    });
}

#[test]
fn bare_type_parameter_never_becomes_an_interface_import_by_lookup() {
    let source = br#"interface Source {
}

fn grouped<Source>() -> result: unit pure {
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("bare Source must declare a colliding type binder: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "Source"
        ));
    });
}

#[test]
fn explicit_interface_import_requires_a_declared_interface() {
    let source = br#"fn grouped<interface Missing>() -> result: unit pure {
}
"#;
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("an explicit import must not declare a type binder: {outcome:?}");
        };
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse { spelling, .. } if spelling == "Missing"
        ));
    });
}

/// A conditional's two blocks are two lexical blocks [TYPE-6], and they are
/// the one construct whose blocks hang off the same node [GRAM-4], so the
/// scope tree has to separate them by brace pair rather than by production.
///
/// The two rejecting sources are the controls that make the two accepting ones
/// mean something: `match` arms, which are separate productions, must reach the
/// same answer as the `if` branches; and a genuine shadow of a live enclosing
/// binder must still be rejected, since giving each branch its own scope would
/// otherwise hide it.
#[test]
fn conditional_branches_are_separate_lexical_scopes() {
    let sibling_branches = br#"fn get(pick: Bool) -> result: unit pure {
  if pick {
    let inside = 1_u64;
    let observed = inside;
  } else {
    let inside = 2_u64;
    let observed = inside;
  }
  return unit;
}
"#;
    with_one_resolution(sibling_branches, |outcome| {
        assert!(
            matches!(outcome, ResolutionOutcome::Complete(_)),
            "disjoint sibling branches may reuse a spelling: {outcome:?}"
        );
    });

    let arm_control = br#"enum Pick {
  Left();
  Right();
}

fn get(pick: Pick) -> result: unit pure {
  match pick {
    Left() => {
      let inside = 1_u64;
      let observed = inside;
    }
    Right() => {
      let inside = 2_u64;
      let observed = inside;
    }
  }
  return unit;
}
"#;
    with_one_resolution(arm_control, |outcome| {
        assert!(
            matches!(outcome, ResolutionOutcome::Complete(_)),
            "the arm spelling of the same program must resolve too: {outcome:?}"
        );
    });

    let expired_then_enclosing = br#"fn get(pick: Bool) -> result: unit pure {
  if pick {
    let offset = 0_u64;
    let inner = offset;
  }
  let offset = 1_u64;
  let outer = offset;
  return unit;
}
"#;
    with_one_resolution(expired_then_enclosing, |outcome| {
        assert!(
            matches!(outcome, ResolutionOutcome::Complete(_)),
            "an expired branch scope may not block a later enclosing binder: {outcome:?}"
        );
    });

    let live_shadow = br#"fn get(pick: Bool) -> result: unit pure {
  let offset = 0_u64;
  if pick {
    let offset = 1_u64;
    let inner = offset;
  }
  let outer = offset;
  return unit;
}
"#;
    with_one_resolution(live_shadow, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("a branch binder may not shadow a live enclosing one: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "offset"
        ));
    });
}

#[test]
fn semantic_stage_order_precedes_source_position_and_inventory_rank_is_event_local() {
    let later_inventory_error = br#"fn probe() -> result: unit pure {
  missing();
}

fn cvt() -> result: unit pure {
}
"#;
    with_one_resolution(later_inventory_error, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("inventory must reject before lookup: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
    });

    let later_fn8_error = br#"fn cvt() -> result: unit pure {
}

fn guarded() -> result: unit pure contract {
  define value = 1_i32;
} {
  return unit;
}
"#;
    with_one_resolution(later_fn8_error, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("FN-8 must reject before inventory: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Fn8);
    });

    let earlier_lower_rank = br#"fn value() -> result: unit pure {
}

const value: i32 = 1_i32;

fn cvt() -> result: unit pure {
}
"#;
    with_one_resolution(earlier_lower_rank, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("minimum declaration event must win before rank: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::DeclarationCollision { spelling, .. } if spelling == "value"
        ));
    });
}

#[test]
fn identifier_renaming_preserves_general_resolution_structure() {
    for (helper, local) in [
        ("a", "x"),
        ("helper_name", "local_value"),
        ("function_27", "binding_42"),
    ] {
        let source = format!(
            "fn {helper}() -> result: unit pure {{\n}}\n\nfn probe() -> result: unit pure {{\n  let {local} = 1_i32;\n  {helper}();\n  return {local};\n}}\n"
        );
        with_one_resolution(source.as_bytes(), |outcome| {
            let ResolutionOutcome::Complete(resolved) = outcome else {
                panic!("ordinary renaming must preserve resolution: {outcome:?}");
            };
            assert_eq!(resolved.declarations().len(), 3);
            assert!(resolved.lexical_uses().iter().any(|usage| {
                usage.spelling() == helper
                    && matches!(
                        usage.target(),
                        ResolvedTarget::Source {
                            class: DeclarationClass::Function,
                            ..
                        }
                    )
            }));
            assert!(resolved.lexical_uses().iter().any(|usage| {
                usage.spelling() == local
                    && matches!(
                        usage.target(),
                        ResolvedTarget::Source {
                            class: DeclarationClass::Value,
                            ..
                        }
                    )
            }));
        });
    }
}

#[test]
fn one_name_mutation_changes_a_complete_call_into_an_op1_rejection() {
    let accepted = br#"fn helper() -> result: unit pure {
}

fn probe() -> result: unit pure {
  helper();
}
"#;
    with_one_resolution(accepted, |outcome| {
        assert!(matches!(outcome, ResolutionOutcome::Complete(_)));
    });

    let mutated = br#"fn helper() -> result: unit pure {
}

fn probe() -> result: unit pure {
  missing();
}
"#;
    with_one_resolution(mutated, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("mutated call must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Op1);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse { spelling, .. } if spelling == "missing"
        ));
    });
}

#[test]
fn diagnostics_ignore_logical_paths_and_repeat_byte_for_byte() {
    let source = b"fn probe() -> result: unit pure {\n  missing();\n}\n";
    let issue = |path: &str| -> ResolutionIssue {
        with_resolution(&[SourceInput::new(path, source)], |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("missing call must reject: {outcome:?}");
            };
            issue
        })
    };
    assert_eq!(issue("first.wf"), issue("renamed/location.wf"));
    assert_eq!(issue("first.wf"), issue("first.wf"));
}

/// [MOD-3] record order no longer controls visibility inside one module,
/// and logical paths still create no namespace.
#[test]
fn source_record_order_controls_no_visibility_and_paths_create_no_namespace() {
    let use_source = SourceInput::new("consumer/first.wf", b"const first: i32 = second;\n");
    let declaration_source = SourceInput::new("library/second.wf", b"const second: i32 = 2_i32;\n");
    for order in [
        [use_source, declaration_source],
        [declaration_source, use_source],
    ] {
        with_resolution(&order, |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "a module's const is visible in every record of it: {outcome:?}"
            );
        });
    }

    let first = SourceInput::new("left/name.wf", b"fn same() -> result: unit pure {\n}\n");
    let second = SourceInput::new("right/name.wf", b"fn same() -> result: unit pure {\n}\n");
    with_resolution(&[first, second], |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("logical paths must not create function namespaces: {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type6);
    });
}

#[test]
fn every_distinct_op1_family_resolves_through_the_normal_callee_path() {
    // The callee path now covers the families that keep a name; the
    // twenty-seven operator-spelled rows — twenty-one arithmetic since v0.23
    // and the six integer comparisons since v0.41 — reach their family by
    // operator token and are covered by
    // `a_respelled_family_produces_no_lexical_use_at_all`. The two halves are
    // counted here so that a family silently leaving one for the other cannot
    // pass unnoticed.
    let named: Vec<_> = OPERATION_FAMILIES
        .iter()
        .enumerate()
        .filter(|(_, spelling)| !is_operator_family(spelling))
        .collect();
    assert_eq!(named.len(), OPERATION_FAMILIES.len() - 27);

    let mut source = String::from("fn probe() -> result: unit pure {\n");
    for (_, operation) in &named {
        source.push_str("  ");
        source.push_str(operation);
        source.push_str("(1_i32);\n");
    }
    source.push_str("}\n");

    with_one_resolution(source.as_bytes(), |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("every named OP-1 family must resolve: {outcome:?}");
        };
        let operations: Vec<_> = resolved
            .lexical_uses()
            .iter()
            .filter(|usage| matches!(usage.target(), ResolvedTarget::Operation(_)))
            .collect();
        assert_eq!(operations.len(), named.len());
        // The identity check is against the family's own position in the
        // inventory, not against the order it happens to be written in, so
        // filtering the source cannot make the ordinals agree by accident.
        for (usage, (index, spelling)) in operations.into_iter().zip(named.iter()) {
            let ResolvedTarget::Operation(id) = usage.target() else {
                unreachable!();
            };
            assert_eq!(usize::from(id.ordinal()), *index);
            assert_eq!(usage.spelling(), **spelling);
        }
    });
}

// v0.58 retires SYS-1/SYS-2/SYS-3, QUAL-1 and SYS-5 ordinal/release-table
// assertions. PRE-1 now supplies ordinary declarations; these witnesses retain
// visibility, collision and callable-binding coverage without an external domain.
#[test]
fn parsed_prelude_declarations_are_ordinary_visible_targets() {
    let source = b"fn inspect(args: &std::text::Args) -> result: u64 reads(args) {\n  return std::text::args_count(args: args);\n}\n";
    with_resolution_sources(
        &[SourceInput::new("ordinary.wf", source)],
        true,
        |outcome| {
            let ResolutionOutcome::Complete(resolved) = outcome else {
                panic!("ordinary prelude declaration resolution: {outcome:?}");
            };
            for (spelling, class) in [
                ("Args", DeclarationClass::NominalType),
                ("args_count", DeclarationClass::Function),
            ] {
                assert!(resolved.lexical_uses().iter().any(|usage| {
                usage.origin().coordinate().source().ordinal() == 0 && usage.spelling() == spelling
                    && matches!(usage.target(), ResolvedTarget::Source { class: actual, .. } if actual == class)
            }));
            }
        },
    );
}

/// Renamed from `..._and_opaque_types_have_no_constructor`: an opaque struct
/// does have a constructor entry, and this test asserts that it resolves.
/// [TYPE-2]: "Its constructor entry [TYPE-6] exists to be refused: a
/// constructor `call` whose leading TYPEID names an opaque struct is a hard
/// error citing TYPE-2 at the complete `call`" -- the refusal is the checker's,
/// over a name resolution supplied here.
#[test]
fn ordinary_prelude_names_cannot_be_shadowed_and_an_opaque_constructor_entry_resolves() {
    for source in [
        "struct Slots {\n}\n",
        "struct DivideByZero {\n}\n",
        "fn helper() -> result: unit pure {\n  let box_new = 0_u64;\n  return unit;\n}\n",
    ] {
        with_resolution_sources(
            &[SourceInput::new("collision.wf", source.as_bytes())],
            true,
            |outcome| {
                let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("PRE-1 collision must reject: {outcome:?}");
                };
                let ResolutionIssueKind::DeclarationCollision { mechanical_fix, .. } = issue.kind()
                else {
                    panic!("PRE-1 collision must retain its ordinary diagnostic: {issue:?}");
                };
                assert!(mechanical_fix.contains("PRE-1 prelude declaration"));
                assert!(!mechanical_fix.contains("binding whose value was moved"));
            },
        );
    }
    // [TYPE-6] a source variant belongs to its enum and enters no
    // constructor domain, so it shares a PRE-1 variant's spelling freely.
    with_resolution_sources(
        &[SourceInput::new(
            "owned.wf",
            b"enum Collision {\n  NotFound();\n}\n",
        )],
        true,
        |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "a type-owned variant collides with no prelude constructor: {outcome:?}"
            );
        },
    );
    // [TYPE-2] an opaque struct's constructor entry "exists to be refused", so
    // resolution supplies it and the refusal is the checker's hard error at
    // the complete `call`. What is checked here is that the entry resolves:
    // the rejection that used to happen at this stage, as an unresolved name,
    // would have made [TYPE-2]'s judgment over a resolved declaration
    // unreachable.
    let source = b"fn fabricate() -> result: std::text::HostString pure {\n  return std::text::HostString();\n}\n";
    with_resolution_sources(&[SourceInput::new("opaque.wf", source)], true, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("an opaque nominal's constructor entry resolves: {outcome:?}");
        };
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.spelling() == "HostString"
                && matches!(
                    usage.target(),
                    ResolvedTarget::Source {
                        class: DeclarationClass::StructConstructor,
                        ..
                    }
                )
        }));
    });
    // The cell keeps the one compiler-owned identity every later stage reads
    // a written `Box` through [TYPE-2, TYPE-9, PRE-1], in the constructor
    // domain as in the nominal one.
    let source = b"fn hold(cell: Box<u64>) -> result: Box<u64> pure {\n  return move cell;\n}\n";
    with_resolution_sources(&[SourceInput::new("cell.wf", source)], true, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("the cell resolves as a nominal type: {outcome:?}");
        };
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.origin().coordinate().source().ordinal() == 0
                && usage.spelling() == "Box"
                && usage.target() == ResolvedTarget::Container(crate::CELL_NOMINAL_ID)
        }));
    });
}

#[test]
fn an_ordinary_prelude_signature_is_eligible_for_an_actual_member() {
    let source = b"interface Counter {\n  fn count(args: &std::text::Args) -> result: u64 reads(args);\n}\n\nbinding Selected : Counter {\n  count = std::text::args_count;\n}\n";
    with_resolution_sources(&[SourceInput::new("actual.wf", source)], true, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("FN-4 admits ordinary declarations: {outcome:?}");
        };
        assert!(resolved.lexical_uses().iter().any(|usage| {
            usage.origin().coordinate().source().ordinal() == 0
                && usage.spelling() == "args_count"
                && matches!(
                    usage.target(),
                    ResolvedTarget::Source {
                        class: DeclarationClass::Function,
                        ..
                    }
                )
        }));
    });
}

#[test]
fn supplied_signature_locals_do_not_capture_writer_global_names() {
    // PRE-1 is an outer ordinary declaration environment. Its local `f` and
    // `input` parameter names cannot reserve those names in the writer unit.
    let source = b"const input: u64 = 7_u64;\n\nfn f() -> result: u64 pure {\n  return input;\n}\n";
    with_resolution_sources(
        &[SourceInput::new("ordinary.wf", source)],
        true,
        |outcome| {
            assert!(
                matches!(outcome, ResolutionOutcome::Complete(_)),
                "{outcome:?}"
            );
        },
    );
}

#[test]
fn ordinary_prelude_diagnostic_origins_follow_the_complete_record_preorder() {
    // The opaque structs first, each with the nominal and the refused
    // constructor [TYPE-2] its collision names in both domains; `Bool` and
    // its variants follow. `Bool` collides on its nominal alone, because an
    // enum contributes its variants' spellings to the constructor domain and
    // not its own.
    for (name, origins) in [
        ("Slots", vec![5, 6]),
        ("Bool", vec![22]),
        ("Overflow", vec![37, 38]),
    ] {
        let source = format!("struct {name} {{\n}}\n");
        with_resolution_sources(
            &[SourceInput::new("prelude/Array.wf", source.as_bytes())],
            true,
            |outcome| {
                let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                    panic!("collision must reject: {outcome:?}");
                };
                let ResolutionIssueKind::DeclarationCollision { conflicts, .. } = issue.kind()
                else {
                    panic!("ordinary collision: {issue:?}");
                };
                let observed: Vec<_> = conflicts
                    .iter()
                    .map(|conflict| match conflict.origin() {
                        DeclarationOrigin::Prelude(id) => id.ordinal(),
                        other => {
                            panic!("PRE-1 records have no fabricated source origin: {other:?}")
                        }
                    })
                    .collect();
                assert_eq!(observed, origins, "{name}");
                assert_eq!(issue.origin().coordinate().source().ordinal(), 0);
            },
        );
    }
}

#[test]
fn ordinary_prelude_inventory_is_independent_of_writer_names_and_declaration_count() {
    let read_inventory = |source: &[u8]| {
        with_resolution_sources(
            &[SourceInput::new("prelude/Array.wf", source)],
            true,
            |outcome| {
                let ResolutionOutcome::Complete(resolved) = outcome else {
                    panic!("legal writer path: {outcome:?}");
                };
                resolved
                    .prelude_declarations()
                    .iter()
                    .map(|record| {
                        assert_eq!(resolved.prelude_declaration(record.id()), Some(record));
                        (
                            record.id().ordinal(),
                            record.spelling().to_owned(),
                            record.lookup_class(),
                        )
                    })
                    .collect::<Vec<_>>()
            },
        )
    };
    let first = read_inventory(b"fn helper() -> result: unit pure {\n  return unit;\n}\n");
    let second = read_inventory(b"struct Extra {\n  field: u64;\n}\n\nfn helper() -> result: unit pure {\n  let local = 0_u64;\n  return unit;\n}\n");
    assert_eq!(first, second);
    // [PRE-1]'s preorder: "each opaque struct above in written order with its
    // refused constructor and its fields in declaration order". x1 puts the
    // three storage shapes first, each with its nominal, the constructor
    // [TYPE-2] exists to refuse, its element and capacity parameters and its
    // readonly measure fields; the cell follows with four records of its own.
    // The host handles are the standard library's [PRE-2], not PRE-1's.
    assert_eq!(first[0].1, "Array");
    assert_eq!(first[0].2, Some(DeclarationClass::NominalType));
    assert_eq!(first[1].1, "Array");
    assert_eq!(first[1].2, Some(DeclarationClass::StructConstructor));
    assert_eq!(first[2].1, "T");
    assert_eq!(first[3].1, "n");
    assert_eq!(first[4].1, "len");
    assert_eq!(first[5].1, "Slots");
    assert_eq!(first[9].1, "len");
    assert_eq!(first[10].1, "cap");
    assert_eq!(first[11].1, "Ring");
    assert_eq!(first[15].1, "len");
    assert_eq!(first[16].1, "cap");
    assert_eq!(first[17].1, "head");
    assert_eq!(first[18].1, "Box");
    assert_eq!(first[18].2, Some(DeclarationClass::NominalType));
    assert_eq!(first[19].1, "Box");
    assert_eq!(first[19].2, Some(DeclarationClass::StructConstructor));
    assert_eq!(first[20].1, "T");
    assert_eq!(first[21].1, "inner");
    // Then each enum with its variants and their fields, then `Int` and
    // `Float`, then the construction functions [OP-13], then the window
    // operations [OP-10], then `swap` [OP-11] and `free_empty` [OP-14], each
    // with its type, const and value parameters in declared order.
    assert_eq!(first[22].1, "Bool");
    assert_eq!(first[44].1, "Int");
    assert_eq!(first[45].1, "Float");
    assert_eq!(first[46].1, "box_new");
    assert_eq!(first[77].1, "place_back");
    assert_eq!(first[121].1, "swap");
    assert_eq!(first[125].1, "free_empty");
    // The opaque phase holds the three storage shapes and the cell, 22
    // records: `Array` contributes five, `Slots` six, `Ring` seven and `Box`
    // four. The host declarations left PRE-1 for the standard library
    // [PRE-2], so the inventory holds 128 records where it held 397.
    assert_eq!(first.len(), 128);
    // `free_empty`'s own value parameter is the last record of the preorder.
    assert_eq!(first.last().map(|record| record.1.as_str()), Some("window"));
    assert!(
        first
            .iter()
            .enumerate()
            .all(|(index, record)| index == record.0 as usize)
    );
}

/// While the host declarations were PRE-1's the inventory held 397 records,
/// and this test showed that a late collision kept an ordinal above `u8`. The
/// host declarations are the standard library's now [PRE-2] and the
/// inventory holds 128, so no prelude ordinal exceeds `u8`; what remains to
/// show is that the last function's collision names its own preorder ordinal.
#[test]
fn a_late_prelude_function_collision_names_its_preorder_ordinal() {
    let source = b"fn free_empty() -> result: unit pure {\n  return unit;\n}\n";
    with_resolution_sources(
        &[SourceInput::new("collision.wf", source)],
        true,
        |outcome| {
            let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("the function collides with its PRE-1 declaration: {outcome:?}");
            };
            let ResolutionIssueKind::DeclarationCollision { conflicts, .. } = issue.kind() else {
                panic!("ordinary function collision: {issue:?}");
            };
            assert_eq!(conflicts.len(), 1);
            assert!(
                matches!(conflicts[0].origin(), DeclarationOrigin::Prelude(id) if id.ordinal() == 125)
            );
        },
    );
}

/// Each declaration by role and spelling, with its key.
type DeclarationKeys = Vec<(String, String)>;

/// Each lexical use by spelling, with its item-relative key and its
/// whole-unit node path.
type UseKeys = Vec<(String, String, Vec<u32>)>;

/// Every declaration of `source` and every lexical use in it, with their
/// keys.
fn keys_of(source: &[u8]) -> (DeclarationKeys, UseKeys) {
    with_one_resolution(source, |outcome| {
        let ResolutionOutcome::Complete(resolved) = outcome else {
            panic!("key probe source must resolve: {outcome:?}");
        };
        let declarations = resolved
            .declarations()
            .iter()
            .map(|declaration| {
                (
                    format!("{:?} {}", declaration.role(), declaration.spelling()),
                    declaration.key().to_string(),
                )
            })
            .collect();
        let uses = resolved
            .lexical_uses()
            .iter()
            .map(|usage| {
                (
                    usage.spelling().to_owned(),
                    resolved
                        .occurrence_key(usage.origin().node())
                        .expect("a use inside an item has an occurrence key")
                        .to_string(),
                    usage.origin().node().components().to_vec(),
                )
            })
            .collect();
        (declarations, uses)
    })
}

/// [MOD-3] an item added before the others moves every later item's node
/// paths but renames no declaration and no occurrence: keys are relative to
/// the item that holds them (`design/compiler/incremental-compilation.md`).
#[test]
fn an_unrelated_item_renames_no_declaration_or_occurrence() {
    let body = br#"fn probe(limit: u64) -> result: unit pure {
  for @range (index in 0_u64..limit) {
    let copied = index;
    helper();
    break @range;
  }
  return unit;
}

fn helper() -> result: unit pure {
}
"#;
    let mut widened = b"fn unrelated() -> result: unit pure {\n}\n\n".to_vec();
    widened.extend_from_slice(body);
    let (declarations, uses) = keys_of(body);
    let (wider_declarations, wider_uses) = keys_of(&widened);
    assert_eq!(wider_declarations.len(), declarations.len() + 1);
    for declaration in &declarations {
        assert!(
            wider_declarations.contains(declaration),
            "{declaration:?} must keep its key"
        );
    }
    assert_eq!(wider_uses.len(), uses.len());
    for ((spelling, key, path), (wider_spelling, wider_key, wider_path)) in
        uses.iter().zip(&wider_uses)
    {
        assert_eq!(spelling, wider_spelling);
        assert_eq!(key, wider_key, "`{spelling}` must keep its occurrence key");
        assert_ne!(path, wider_path, "`{spelling}` moves in the whole unit");
    }
}

/// [TYPE-6] two enums of one module may name a variant alike, since a
/// construction names its owner and an arm its scrutinee's type; each
/// variant's key is placed within its own enum.
#[test]
fn variants_of_two_enums_keep_distinct_keys() {
    let source = br#"enum Left {
  Same();
}

enum Right {
  Same();
}
"#;
    let (declarations, _) = keys_of(source);
    let variants = declarations
        .iter()
        .filter(|(declaration, _)| declaration == "Variant Same")
        .map(|(_, key)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(variants.len(), 2);
    assert_ne!(variants[0], variants[1]);
    assert!(
        variants[0].starts_with("pkg::Left#Enum/"),
        "{}",
        variants[0]
    );
    assert!(
        variants[1].starts_with("pkg::Right#Enum/"),
        "{}",
        variants[1]
    );
}
