//! Overlap-group storage-boundary tests cover release/borrow ordering, owning
//! ranges and disjoint owning pairs, conditional calls, and call-rooted matches.
//! They check that storage conflicts preserve call-entry and reference-formation
//! order while proved separations and borrows without releases retain overlap.

use super::*;

// The recursive owning-element split that lost its offers when the storage
// boundary discarded the permission judgment's retained range proofs.
const OWNING_RANGE_VISIT: &str = r#"enum Frontier {
  doc "A sparse directory over stable element slots.";
  Vacant();
  Mark();
  Fork(left: Box<Frontier>, right: Box<Frontier>);
}

fn visit(frontier: &Frontier, values: &[Box<u64>], span: u64) -> ok: Bool reads(frontier), writes(values) {
  doc "Visits independent leaves and joins only their completion results.";
  let count = values^.len;
  match frontier^ {
    Vacant() => {
      return True();
    }
    Mark() => {
      if 0_u64 < count {
        let fresh = box_new::<u64>(value: 1_u64);
        set values^[0_u64] = move fresh;
        return True();
      }
      return False();
    }
    Fork(left: left_tree, right: right_tree) => {
      let half = span / 2_u64;
      let middle = imin(half, count);
      let left_ok = visit(frontier: &left_tree^.inner, values: &values^[0_u64..middle], span: half);
      let right_ok = visit(frontier: &right_tree^.inner, values: &values^[middle..count], span: half);
      let both_ok = band(left_ok, right_ok);
      return both_ok;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn proved_disjoint_owning_ranges_keep_recursive_overlap_and_lane_emission() {
    with_checked(OWNING_RANGE_VISIT.as_bytes(), |checked| {
        let permissions = checked
            .data
            .permission
            .named("visit")
            .expect("visit permissions");
        let pair = permissions
            .pairs
            .iter()
            .find(|pair| pair.first.callee_name == "visit" && pair.second.callee_name == "visit")
            .expect("recursive sibling pair");
        assert!(pair.verdict.is_eligible(), "{pair:?}");
        // The old boundary sees a conflict on these exact checked places.
        assert!(
            pair.first
                .storage_effects
                .conflict(
                    &crate::semantic::UnprovedSeparations,
                    &pair.second.storage_effects,
                )
                .is_some()
        );
        let boundary = permissions
            .storage_pairs
            .iter()
            .find(|boundary| {
                boundary.first == pair.first.statement && boundary.second == pair.second.statement
            })
            .expect("the same ordered pair has a storage answer");
        assert!(boundary.conflict.is_none(), "{boundary:?}");
        let program = lower_checked(checked, OverlapLowering::OnWithCallGrain)
            .expect("recursive owning ranges lower with the default call grain");
        let visit = function(&program, "visit");
        let calls = calls_to(&program, visit, &["visit"]);
        assert_eq!(calls.len(), 2);
        assert_eq!(visit.overlaps().len(), 1);
        assert_eq!(visit.overlaps()[0].members, calls);
        assert!(
            !program
                .actualization_ledger()
                .iter()
                .any(|line| line.contains("release/borrow conflict"))
        );
        let module = crate::emit_llvm(&program)
            .expect("recursive owning ranges emit")
            .into_string();
        let body = module
            .split("\ndefine ")
            .find(|body| {
                body.lines()
                    .next()
                    .is_some_and(|line| line.contains("@wf__par_budget_visit("))
            })
            .expect("the recursive offer keeps its budgeted body");
        let body = body.split("\n}").next().expect("function body");
        for operation in [
            "call ptr @wf__par_acquire_lane(",
            "call void @wf__par_publish(",
            "call void @wf__par_join(",
        ] {
            assert!(body.contains(operation), "missing {operation}:\n{body}");
        }
    });
}

#[test]
fn overlapping_owning_ranges_do_not_form_a_recursive_overlap_group() {
    let source = OWNING_RANGE_VISIT.replace(
        "values: &values^[middle..count]",
        "values: &values^[0_u64..count]",
    );
    with_checked(source.as_bytes(), |checked| {
        let permissions = checked
            .data
            .permission
            .named("visit")
            .expect("visit permissions");
        let pair = permissions
            .pairs
            .iter()
            .find(|pair| pair.first.callee_name == "visit" && pair.second.callee_name == "visit")
            .expect("recursive sibling pair");
        assert!(
            !pair.verdict.is_eligible(),
            "overlapping writes deny PAR-1: {pair:?}"
        );
        let boundary = permissions
            .storage_pairs
            .iter()
            .find(|boundary| {
                boundary.first == pair.first.statement && boundary.second == pair.second.statement
            })
            .expect("recursive storage pair");
        assert!(boundary.conflict.is_some(), "{boundary:?}");
        let program = lower_checked(checked, OverlapLowering::On)
            .expect("overlapping calls lower sequentially");
        let visit = function(&program, "visit");
        let calls = calls_to(&program, visit, &["visit"]);
        assert_eq!(calls.len(), 2);
        assert!(
            !visit
                .overlaps()
                .iter()
                .any(|group| calls.iter().all(|call| group.members.contains(call)))
        );
        assert!(
            !program
                .actualization_ledger()
                .iter()
                .any(|line| line.contains("release/borrow conflict")),
            "a source denial is not a lowering narrowing"
        );
    });
}

#[test]
fn nonadjacent_owning_range_members_use_their_own_pair_proofs() {
    let source = OWNING_RANGE_VISIT.replace(
        "      let both_ok = band(left_ok, right_ok);",
        "      let empty_ok = visit(frontier: &right_tree^.inner, values: &values^[count..count], span: half);\n      let both_ok = band(left_ok, right_ok);",
    );
    with_checked(source.as_bytes(), |checked| {
        let permissions = checked
            .data
            .permission
            .named("visit")
            .expect("visit permissions");
        let run = permissions
            .runs
            .iter()
            .find(|run| {
                run.sites
                    .iter()
                    .filter(|site| site.callee_name == "visit")
                    .count()
                    == 3
            })
            .expect("three mutually permitted recursive calls");
        let calls = run
            .sites
            .iter()
            .filter(|site| site.callee_name == "visit")
            .collect::<Vec<_>>();
        let first = &calls[0].statement;
        let third = &calls[2].statement;
        let boundary = permissions
            .storage_pairs
            .iter()
            .find(|pair| pair.first == *first && pair.second == *third)
            .expect("nonadjacent storage pair");
        assert!(boundary.conflict.is_none(), "{boundary:?}");
        assert!(
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == "visit")
                .expect("checked visit")
                .entailment
                .permission_separations
                .iter()
                .any(|proof| {
                    proof.query.first == *first && proof.query.second == *third && proof.discharged
                }),
            "nonadjacent members need their own retained proof"
        );
        let program =
            lower_checked(checked, OverlapLowering::On).expect("three recursive calls lower");
        let visit = function(&program, "visit");
        let calls = calls_to(&program, visit, &["visit"]);
        assert_eq!(calls.len(), 3);
        assert!(visit.overlaps().iter().any(|group| group.members == calls));
    });
}

// Ignored reference arguments have no read effect, but still need live
// storage at entry. This isolates lowering's boundary from PAR-1 denial.
const IGNORED_OWNING_RANGE: &str = r#"fn replace(values: &[Box<u64>]) -> result: unit writes(values) {
  let count = values^.len;
  if 0_u64 < count {
    let fresh = box_new::<u64>(value: 1_u64);
    set values^[0_u64] = move fresh;
  }
  return unit;
}

fn ignore(values: &[Box<u64>]) -> result: unit pure {
  return unit;
}

fn pair(values: &[Box<u64>], middle: u64) -> result: unit writes(values) contract {
  requires middle <= values^.len;
} {
  let count = values^.len;
  replace(values: &values^[0_u64..middle]);
  ignore(values: &values^[middle..count]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

const STORAGE_MATCH_CHECK: &str = r#"enum Arm {
  Active();
  Inactive();
}

fn check(a: &Bool) -> result: Arm pure {
  return Arm::Active();
}
"#;

#[test]
fn call_rooted_match_arm_borrow_preserves_owned_call_entry_order() {
    let source = format!(
        r#"{STORAGE_MATCH_CHECK}
fn ignore(part: &u64) -> result: unit pure {{
  return unit;
}}

fn consume(value: Box<u64>) -> result: unit pure {{
  return unit;
}}

fn pair(x: Bool) -> result: unit pure {{
  let p = box_new::<u64>(value: 0_u64);
  match check(a: &x) {{
    Active() => {{
      ignore(part: &p.inner);
    }}
    Inactive() => {{
    }}
  }}
  consume(value: move p);
  return unit;
}}

{PLAIN_ENTRY}"#
    );
    for source in [
        source.clone(),
        source.replace(
            "Active() => {\n      ignore(part: &p.inner);\n    }\n    Inactive() => {\n    }",
            "Active() => {\n    }\n    Inactive() => {\n      ignore(part: &p.inner);\n    }",
        ),
    ] {
        assert_call_rooted_match_storage_boundary(
            source.as_bytes(),
            "check",
            "consume",
            Some("s2 releases storage at move p overlapping storage at &p.inner borrowed by s1"),
        );
    }
}

#[test]
fn call_rooted_match_arm_release_preserves_borrowed_call_entry_order() {
    let source = format!(
        r#"{STORAGE_MATCH_CHECK}
fn ignore(part: &Box<u64>) -> result: unit pure {{
  return unit;
}}

fn replace(cell: &Box<u64>) -> result: unit writes(cell) {{
  let fresh = box_new::<u64>(value: 1_u64);
  set cell^ = move fresh;
  return unit;
}}

fn pair(x: Bool) -> result: unit pure {{
  let p = box_new::<u64>(value: 0_u64);
  match check(a: &x) {{
    Active() => {{
      replace(cell: &p);
    }}
    Inactive() => {{
    }}
  }}
  ignore(part: &p);
  return unit;
}}

{PLAIN_ENTRY}"#
    );
    assert_call_rooted_match_storage_boundary(
        source.as_bytes(),
        "check",
        "ignore",
        Some("s1 releases storage at &p overlapping storage at &p borrowed by s2"),
    );
    // A match can only finish an actualized group. Put it last as well to
    // observe the storage cut in the emitted actualization ledger.
    let match_last = source
        .replace("  match check", "  ignore(part: &p);\n  match check")
        .replace("  }\n  ignore(part: &p);", "  }");
    assert_call_rooted_match_storage_boundary(
        match_last.as_bytes(),
        "ignore",
        "check",
        Some("s2 releases storage at &p overlapping storage at &p borrowed by s1"),
    );
}

#[test]
fn call_rooted_match_arm_range_borrows_keep_storage_boundary_without_entry_proofs() {
    // Arm range formations are not planned; conditional calls reject inline ranges too.
    let source = IGNORED_OWNING_RANGE
        .replace(
            "fn pair(values: &[Box<u64>], middle: u64)",
            "fn pair(values: &[Box<u64>], middle: u64, x: Bool)",
        )
        .replace(
            "  ignore(values: &values^[middle..count]);",
            r#"  match check(a: &x) {
    Active() => {
      ignore(values: &values^[middle..count]);
    }
    Inactive() => {
    }
  }"#,
        );
    let source = format!("{STORAGE_MATCH_CHECK}\n{source}");
    // The ignored inline range does not read its referent; PAR-1 permits both variants.
    for endpoints in ["middle..count", "0_u64..count"] {
        let source = source.replace(
            "values: &values^[middle..count]",
            &format!("values: &values^[{endpoints}]"),
        );
        assert_call_rooted_match_storage_boundary(
            source.as_bytes(),
            "replace",
            "check",
            Some(&format!(
                "s1 releases storage at &values^[0_u64..middle] overlapping storage at &values^[{endpoints}] borrowed by s2"
            )),
        );
    }
}

fn assert_call_rooted_match_storage_boundary(
    source: &[u8],
    first: &str,
    second: &str,
    conflict_suffix: Option<&str>,
) {
    with_checked(source, |checked| {
        let permissions = checked
            .data
            .permission
            .named("pair")
            .expect("pair permissions");
        let pair = permissions
            .pairs
            .iter()
            .find(|pair| pair.first.callee_name == first && pair.second.callee_name == second)
            .expect("call-rooted match and its adjacent call");
        assert!(pair.first.call.is_some() && pair.second.call.is_some());
        assert!(
            pair.verdict.is_eligible(),
            "source permission stays intact: {pair:?}"
        );
        assert!(
            pair.first
                .storage_effects
                .conflict(
                    &crate::semantic::UnprovedSeparations,
                    &pair.second.storage_effects,
                )
                .is_some(),
            "arm effects must survive even when proof separates them: {pair:?}"
        );
        let boundary = permissions
            .storage_pairs
            .iter()
            .find(|boundary| {
                boundary.first == pair.first.statement && boundary.second == pair.second.statement
            })
            .expect("the match member's pair-local storage boundary");
        assert_eq!(
            boundary.conflict.is_some(),
            conflict_suffix.is_some(),
            "{boundary:?}"
        );
        if let Some(suffix) = conflict_suffix {
            assert!(
                boundary.ledger.starts_with("PAR actualization  test.wf:"),
                "{}",
                boundary.ledger
            );
            assert!(
                boundary
                    .ledger
                    .contains(&format!("pair({first}, {second}) through line ")),
                "{}",
                boundary.ledger
            );
            assert!(
                boundary
                    .ledger
                    .ends_with(&format!("narrowed: release/borrow conflict; {suffix}")),
                "{}",
                boundary.ledger
            );
        } else {
            assert!(
                checked
                    .data
                    .functions
                    .iter()
                    .find(|function| function.name == "pair")
                    .expect("checked pair")
                    .entailment
                    .permission_separations
                    .iter()
                    .any(|proof| {
                        proof.query.first == pair.first.statement
                            && proof.query.second == pair.second.statement
                            && proof.discharged
                    }),
                "the match member needs its own retained separation proof"
            );
        }
        let ledger = boundary.ledger.clone();
        let program =
            lower_checked(checked, OverlapLowering::On).expect("match storage boundary lowers");
        let pair = function(&program, "pair");
        let calls = calls_to(&program, pair, &[first, second]);
        assert_eq!(calls.len(), 2);
        assert_eq!(
            pair.overlaps()
                .iter()
                .any(|group| calls.iter().all(|call| group.members.contains(call))),
            conflict_suffix.is_none(),
            "{:?}",
            pair.overlaps()
        );
        let lines = program
            .actualization_ledger()
            .iter()
            .filter(|line| line.contains("release/borrow conflict"))
            .collect::<Vec<_>>();
        // A match dispatch ends its IR block; a following call cannot join
        // that group even without a storage conflict. Only a match-last pair
        // can be narrowed by this boundary during actualization.
        let emitted = conflict_suffix.is_some() && second == "check";
        assert_eq!(lines.len(), usize::from(emitted), "{lines:?}");
        if emitted {
            assert_eq!(lines[0], &ledger);
        }
    });
}

#[test]
fn conditional_borrow_preserves_owned_call_entry_order() {
    let source = format!(
        r#"fn ignore(part: &u64) -> result: unit pure {{
  return unit;
}}

fn consume(value: Box<u64>) -> result: unit pure {{
  return unit;
}}

fn pair(go: Bool) -> result: unit pure {{
  let p = box_new::<u64>(value: 0_u64);
  if go {{
    ignore(part: &p.inner);
  }}
  consume(value: move p);
  return unit;
}}

{PLAIN_ENTRY}"#
    );
    assert_conditional_storage_boundary(
        source.as_bytes(),
        "consume",
        Some("s2 releases storage at move p overlapping storage at &p.inner borrowed by s1"),
    );
}

#[test]
fn conditional_release_preserves_borrowed_call_entry_order() {
    let source = format!(
        r#"fn ignore(part: &Box<u64>) -> result: unit pure {{
  return unit;
}}

fn replace(cell: &Box<u64>) -> result: unit writes(cell) {{
  let fresh = box_new::<u64>(value: 1_u64);
  set cell^ = move fresh;
  return unit;
}}

fn pair(go: Bool) -> result: unit pure {{
  let p = box_new::<u64>(value: 0_u64);
  if go {{
    replace(cell: &p);
  }}
  ignore(part: &p);
  return unit;
}}

{PLAIN_ENTRY}"#
    );
    assert_conditional_storage_boundary(
        source.as_bytes(),
        "ignore",
        Some("s1 releases storage at &p overlapping storage at &p borrowed by s2"),
    );
}

#[test]
fn conditional_borrow_of_proved_disjoint_owning_range_still_overlaps() {
    // Bind the ranges before the guard: the conditional-call shape admits
    // forwarding a reference, but not speculative range formation in its arm.
    let source = IGNORED_OWNING_RANGE
        .replace(
            "fn pair(values: &[Box<u64>], middle: u64)",
            "fn pair(values: &[Box<u64>], middle: u64, go: Bool)",
        )
        .replace(
            "  replace(values: &values^[0_u64..middle]);\n  ignore(values: &values^[middle..count]);",
            "  let left = &values^[0_u64..middle];\n  let right = &values^[middle..count];\n  if go {\n    ignore(values: right);\n  }\n  replace(values: left);",
        );
    assert_conditional_storage_boundary(source.as_bytes(), "replace", None);
}

fn assert_conditional_storage_boundary(source: &[u8], next: &str, conflict_suffix: Option<&str>) {
    with_checked(source, |checked| {
        let permissions = checked
            .data
            .permission
            .named("pair")
            .expect("pair permissions");
        let pair = permissions
            .pairs
            .iter()
            .find(|pair| {
                pair.first.callee_name == "a conditional call" && pair.second.callee_name == next
            })
            .expect("conditional call followed by an ordinary call");
        assert!(pair.first.call.is_some());
        assert!(
            pair.verdict.is_eligible(),
            "source permission stays intact: {pair:?}"
        );
        assert!(
            pair.first
                .storage_effects
                .conflict(
                    &crate::semantic::UnprovedSeparations,
                    &pair.second.storage_effects
                )
                .is_some(),
            "the guarded call must retain its storage effects even when proof separates them: {pair:?}"
        );
        let boundary = permissions
            .storage_pairs
            .iter()
            .find(|boundary| {
                boundary.first == pair.first.statement && boundary.second == pair.second.statement
            })
            .expect("conditional call's pair-local storage boundary");
        assert_eq!(
            boundary.conflict.is_some(),
            conflict_suffix.is_some(),
            "{boundary:?}"
        );
        if conflict_suffix.is_none() {
            assert!(
                checked
                    .data
                    .functions
                    .iter()
                    .find(|function| function.name == "pair")
                    .expect("checked pair")
                    .entailment
                    .permission_separations
                    .iter()
                    .any(|proof| {
                        proof.query.first == pair.first.statement
                            && proof.query.second == pair.second.statement
                            && proof.discharged
                    }),
                "the guard's pair needs its own retained separation proof"
            );
        }
        let program = lower_checked(checked, OverlapLowering::On)
            .expect("conditional storage boundary lowers");
        let guard = program
            .functions()
            .iter()
            .find(|function| function.name().starts_with("_par_cond_"))
            .expect("the permitted conditional member is outlined");
        let pair = function(&program, "pair");
        let calls = calls_to(&program, pair, &[guard.name(), next]);
        assert_eq!(calls.len(), 2, "guard and following call");
        assert_eq!(
            pair.overlaps()
                .iter()
                .any(|group| calls.iter().all(|call| group.members.contains(call))),
            conflict_suffix.is_none(),
            "{:?}",
            pair.overlaps()
        );
        let lines = program
            .actualization_ledger()
            .iter()
            .filter(|line| line.contains("release/borrow conflict"))
            .collect::<Vec<_>>();
        assert_eq!(
            lines.len(),
            usize::from(conflict_suffix.is_some()),
            "{lines:?}"
        );
        if let Some(suffix) = conflict_suffix {
            let line = lines[0];
            assert!(line.starts_with("PAR actualization  test.wf:"), "{line}");
            assert!(
                line.contains(&format!("pair(a conditional call, {next}) through line ")),
                "{line}"
            );
            assert!(
                line.ends_with(&format!("narrowed: release/borrow conflict; {suffix}")),
                "{line}"
            );
        }
    });
}

#[test]
fn ignored_range_borrows_plan_storage_separations_and_report_actual_cuts() {
    for (endpoints, reverse) in [
        ("middle..count", false),
        ("0_u64..count", false),
        ("0_u64..count", true),
    ] {
        let disjoint = endpoints == "middle..count";
        let release = "  replace(values: &values^[0_u64..middle]);";
        let borrow = format!("  ignore(values: &values^[{endpoints}]);");
        let calls = if reverse {
            format!("{borrow}\n{release}")
        } else {
            format!("{release}\n{borrow}")
        };
        let source = IGNORED_OWNING_RANGE.replace(
            "  replace(values: &values^[0_u64..middle]);\n  ignore(values: &values^[middle..count]);",
            &calls,
        );
        with_checked(source.as_bytes(), |checked| {
            let permissions = checked
                .data
                .permission
                .named("pair")
                .expect("pair permissions");
            let pair = permissions
                .pairs
                .iter()
                .find(|pair| pair.first.call.is_some() && pair.second.call.is_some())
                .expect("two calls");
            assert!(
                pair.verdict.is_eligible(),
                "ignored borrows keep source permission: {pair:?}"
            );
            let program =
                lower_checked(checked, OverlapLowering::On).expect("ignored borrow lowers");
            let pair = function(&program, "pair");
            let calls = calls_to(&program, pair, &["replace", "ignore"]);
            assert_eq!(calls.len(), 2);
            assert_eq!(
                pair.overlaps().iter().any(|group| group.members == calls),
                disjoint
            );
            let lines = program
                .actualization_ledger()
                .iter()
                .filter(|line| line.contains("release/borrow conflict"))
                .collect::<Vec<_>>();
            assert_eq!(
                lines.len(),
                usize::from(!disjoint),
                "report only actual cuts, once: {lines:?}"
            );
            if !disjoint {
                let line = lines[0];
                let names = if reverse {
                    "pair(ignore, replace)"
                } else {
                    "pair(replace, ignore)"
                };
                let releasing = if reverse { "s2" } else { "s1" };
                let borrowing = if reverse { "s1" } else { "s2" };
                assert!(line.starts_with("PAR actualization  test.wf:"), "{line}");
                assert!(
                    line.contains(names) && line.contains(" through line "),
                    "{line}"
                );
                assert!(line.ends_with(&format!(
                    "narrowed: release/borrow conflict; {releasing} releases storage at &values^[0_u64..middle] overlapping storage at &values^[0_u64..count] borrowed by {borrowing}"
                )), "{line}");
            }
        });
    }
}

#[test]
fn a_nonadjacent_release_borrow_conflict_ends_the_group() {
    let source = IGNORED_OWNING_RANGE.replace(
        "  ignore(values: &values^[middle..count]);",
        "  ignore(values: &values^[middle..count]);\n  ignore(values: &values^[0_u64..middle]);",
    );
    with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
        let pair = function(program, "pair");
        let calls = calls_to(program, pair, &["replace", "ignore"]);
        assert_eq!(calls.len(), 3);
        assert_eq!(pair.overlaps().len(), 1);
        assert_eq!(pair.overlaps()[0].members, calls[..2]);
        let lines = program
            .actualization_ledger()
            .iter()
            .filter(|line| line.contains("release/borrow conflict"))
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 1, "the nonadjacent cut is reported: {lines:?}");
        assert!(
            lines[0].contains(
                "storage at &values^[0_u64..middle] overlapping storage at &values^[0_u64..middle]"
            ),
            "{lines:?}"
        );
    });
}

#[test]
fn paged_cell_growth_preserves_formation_and_borrow_entry_order() {
    for (actual, kind, reverse, wrapper) in [
        ("&p.inner[0_u64..0_u64]", "Run<u64>", false, false),
        ("&p.inner[0_u64]", "u64", false, false),
        ("&p.inner", "Paged<u64>", false, false),
        ("&p.inner", "Paged<u64>", true, false),
        ("&p.inner", "Paged<u64>", true, true),
    ] {
        let resize = if wrapper {
            r#"fn resize(cell: &Box<Paged<u64>>, capacity: u64) -> result: unit writes(cell) contract {
  requires capacity >= cell^.inner.cap;
} {
  grow_paged(cell: cell, capacity: capacity);
  return unit;
}

"#
        } else {
            ""
        };
        let growth_name = if wrapper { "resize" } else { "grow_paged" };
        let growth = format!("  {growth_name}(cell: &p, capacity: 1025_u64);\n");
        let ignore = format!("  ignore(part: {actual});\n");
        let calls = if reverse {
            format!("{ignore}{growth}")
        } else {
            format!("{growth}{ignore}")
        };
        let source = format!(
            "fn ignore(part: &{kind}) -> result: unit pure {{\n  return unit;\n}}\n\n{resize}fn main() -> status: std::process::ExitStatus pure {{\n  let p = box_paged_new::<u64>(capacity: 1_u64);\n  place_back(window: &p.inner, value: 0_u64);\n{calls}  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_checked(source.as_bytes(), |checked| {
            let permissions = checked
                .data
                .permission
                .named("main")
                .expect("main permissions");
            let pair = permissions
                .pairs
                .iter()
                .find(|pair| {
                    let (first, second) = if reverse {
                        ("ignore", growth_name)
                    } else {
                        (growth_name, "ignore")
                    };
                    pair.first.callee_name == first && pair.second.callee_name == second
                })
                .expect("growth/formation adjacency");
            assert!(
                pair.verdict.is_eligible(),
                "source permission stays intact: {pair:?}"
            );
            assert!(
                pair.first
                    .storage_effects
                    .conflict(
                        &crate::semantic::UnprovedSeparations,
                        &pair.second.storage_effects
                    )
                    .is_some(),
                "the checked places must order growth against its borrow: {pair:?}"
            );
        });
        with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
            let main = function(program, "main");
            let calls = main
                .blocks()
                .iter()
                .flat_map(|block| block.instructions())
                .filter_map(|instruction| {
                    let IrInstruction::Define {
                        result,
                        operation: IrOperation::Call { function, .. },
                        ..
                    } = instruction
                    else {
                        return None;
                    };
                    let callee = program
                        .functions()
                        .get(*function as usize)
                        .expect("call target");
                    Some((callee.name().to_owned(), *result))
                })
                .collect::<Vec<_>>();
            let names = calls
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            // A generic prelude row's instance carries its instance key after
            // `$instance$`.
            let is_growth = |name: &str| {
                if wrapper {
                    name == "resize"
                } else {
                    name.starts_with("grow_paged$")
                }
            };
            assert!(
                names.iter().any(|name| is_growth(name)) && names.contains(&"ignore"),
                "growth and formation calls: {names:?}"
            );
            let calls = calls
                .iter()
                .filter(|(name, _)| is_growth(name) || name == "ignore")
                .map(|(_, result)| *result)
                .collect::<Vec<_>>();
            assert!(
                !main
                    .overlaps()
                    .iter()
                    .any(|group| calls.iter().all(|call| group.members.contains(call))),
                "cell growth must preserve formation and borrowed-call entry order: {:?}",
                main.overlaps()
            );
        });
    }
}

#[test]
fn paged_page_borrow_conflicts_with_cell_growth() {
    let source = br#"fn ignore(part: &[u64]) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<u64>(capacity: 1_u64);
  place_back(window: &p.inner, value: 0_u64);
  if p.inner.pages.len > 0_u64 {
    let page = &p.inner.pages[0_u64];
    ignore(part: page);
    grow_paged(cell: &p, capacity: 1025_u64);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let pair = checked
            .data
            .permission
            .named("main")
            .expect("main permissions")
            .pairs
            .iter()
            .find(|pair| {
                pair.first.callee_name == "ignore" && pair.second.callee_name == "grow_paged"
            })
            .expect("page borrow/growth adjacency");
        // Forwarding the page reloads no owner slot. Inspect storage effects
        // directly so source interference cannot conceal a missing borrowed
        // page origin in the lowering boundary.
        assert!(
            pair.first
                .storage_effects
                .conflict(
                    &crate::semantic::UnprovedSeparations,
                    &pair.second.storage_effects
                )
                .is_some(),
            "a page borrows storage below the released Paged owner: {pair:?}"
        );
    });
}

#[test]
fn paged_cell_and_run_borrows_without_release_still_overlap() {
    let source = br#"fn ignore_cell(cell: &Paged<u64>) -> result: unit pure {
  return unit;
}

fn ignore_run(part: &Run<u64>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = box_paged_new::<u64>(capacity: 1_u64);
  place_back(window: &p.inner, value: 0_u64);
  ignore_cell(cell: &p.inner);
  ignore_run(part: &p.inner[0_u64..1_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let main = function(program, "main");
        let borrows = calls_to(program, main, &["ignore_cell", "ignore_run"]);
        assert_eq!(borrows.len(), 2, "cell and run borrowing calls");
        assert!(
            main.overlaps()
                .iter()
                .any(|group| borrows.iter().all(|call| group.members.contains(call))),
            "Paged borrows without an overlapping release stay in one group: {:?}",
            main.overlaps()
        );
    });
}

/// [PAR-1, STOR-6] an ignored reference into a `Box<Slots<T>>` block still
/// promises dereferenceability at its callee's entry, and forming it loads
/// the owner slot, so a call that can relocate the block must not run before
/// the borrowing call enters, nor while a later member forms its borrow.
/// Source permission does not count the borrow as a content read and stays
/// intact; the overlap lowering must keep the two calls out of one group. Two
/// borrows of the block's own elements, which neither relocates, still
/// overlap.
#[test]
fn slots_cell_growth_preserves_borrowed_call_entry_order() {
    for (actual, kind, wrapper, borrow_first) in [
        ("&p.inner", "Slots<u64>", false, true),
        ("&p.inner", "Slots<u64>", true, true),
        ("&p.inner[0_u64]", "u64", false, true),
        ("&p.inner[0_u64..1_u64]", "[u64]", false, true),
        ("&p.inner", "Slots<u64>", false, false),
        ("&p.inner", "Slots<u64>", true, false),
        ("&p.inner[0_u64..1_u64]", "[u64]", false, false),
    ] {
        let resize = if wrapper {
            r#"fn resize(cell: &Box<Slots<u64>>, capacity: u64) -> result: unit writes(cell) contract {
  requires capacity >= cell^.inner.cap;
} {
  doc "Grows the borrowed owner.";
  grow(cell: cell, capacity: capacity);
  return unit;
}

"#
        } else {
            ""
        };
        let growth_name = if wrapper { "resize" } else { "grow" };
        let borrow = format!("  ignore(part: {actual});\n");
        let growth = format!("  {growth_name}(cell: &p, capacity: 1025_u64);\n");
        let calls = if borrow_first {
            format!("{borrow}{growth}")
        } else {
            format!("{growth}{borrow}")
        };
        let source = format!(
            "fn ignore(part: &{kind}) -> result: unit pure {{\n  doc \"Borrows without reading.\";\n  return unit;\n}}\n\n{resize}fn main() -> status: std::process::ExitStatus pure {{\n  doc \"Keeps growth ordered against borrowed call entry.\";\n  let p = box_slots_new::<u64>(capacity: 1_u64);\n  place_back(window: &p.inner, value: 0_u64);\n{calls}  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
            let main = function(program, "main");
            let is_growth = |name: &str| {
                if wrapper {
                    name == "resize"
                } else {
                    name.starts_with("grow$")
                }
            };
            let calls = main
                .blocks()
                .iter()
                .flat_map(|block| block.instructions())
                .filter_map(|instruction| {
                    let IrInstruction::Define {
                        result,
                        operation: IrOperation::Call { function, .. },
                        ..
                    } = instruction
                    else {
                        return None;
                    };
                    let name = program
                        .functions()
                        .get(*function as usize)
                        .expect("call target")
                        .name();
                    (is_growth(name) || name == "ignore").then_some(*result)
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 2, "{actual}: growth and borrowed call");
            assert!(
                !main
                    .overlaps()
                    .iter()
                    .any(|group| calls.iter().all(|call| group.members.contains(call))),
                "{actual}: block growth must not run before the borrowed call enters: {:?}",
                main.overlaps()
            );
        });
    }
}

/// [PAR-1] two calls borrowing elements of one `Box<Slots<T>>` block, neither
/// of which can relocate it, stay in one overlap group under the releasing-call
/// boundary.
#[test]
fn borrows_inside_one_block_still_overlap() {
    let source = br#"fn ignore(part: &[u64]) -> result: unit pure {
  doc "Borrows without reading.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Keeps independent borrows eligible for overlap.";
  let p = box_slots_new::<u64>(capacity: 4_u64);
  place_back(window: &p.inner, value: 0_u64);
  place_back(window: &p.inner, value: 0_u64);
  ignore(part: &p.inner[0_u64..1_u64]);
  ignore(part: &p.inner[1_u64..2_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let main = function(program, "main");
        let borrows = main
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .filter_map(|instruction| {
                let IrInstruction::Define {
                    result,
                    operation: IrOperation::Call { function, .. },
                    ..
                } = instruction
                else {
                    return None;
                };
                let name = program
                    .functions()
                    .get(*function as usize)
                    .expect("call target")
                    .name();
                (name == "ignore").then_some(*result)
            })
            .collect::<Vec<_>>();
        assert_eq!(borrows.len(), 2, "two borrowed calls");
        assert!(
            main.overlaps()
                .iter()
                .any(|group| borrows.iter().all(|call| group.members.contains(call))),
            "disjoint borrows of one block overlap: {:?}",
            main.overlaps()
        );
    });
}

/// The review's join witness: the owner and reference have distinct block
/// parameters, so definition tracing cannot recover their common storage.
#[test]
fn joined_reference_preserves_borrowed_call_entry_order() {
    let source = br#"fn ignore(part: &u64) -> result: unit pure {
  doc "Borrows without reading.";
  return unit;
}

fn consume(value: Box<u64>) -> result: unit pure {
  doc "Releases the owned box on return.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Keeps an owner alive until a joined reference enters its call.";
  let p = box_new::<u64>(value: 0_u64);
  let pick = 1_u64;
  let q = if pick == 1_u64 {
    give &p.inner;
  } else {
    give &p.inner;
  }
  ignore(part: &q^);
  consume(value: move p);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_release_and_borrow_are_separate(source, "consume");
}

/// A joined reference can name either owner. Keeping only one alternative
/// would allow the other owner's release to race the borrowed call's entry.
#[test]
fn joined_reference_retains_every_borrowed_origin() {
    for consumed in ["left", "right"] {
        let source = format!(
            r#"fn ignore(part: &u64) -> result: unit pure {{
  doc "Borrows without reading.";
  return unit;
}}

fn consume(value: Box<u64>) -> result: unit pure {{
  doc "Releases the selected owner.";
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  doc "Orders either origin's release after joined borrowed call entry.";
  let left = box_new::<u64>(value: 0_u64);
  let right = box_new::<u64>(value: 1_u64);
  let pick = 1_u64;
  let q = if pick == 1_u64 {{
    give &left.inner;
  }} else {{
    give &right.inner;
  }}
  ignore(part: &q^);
  consume(value: move {consumed});
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        assert_release_and_borrow_are_separate(source.as_bytes(), "consume");
    }
}

/// The review's range witness: range formation and direct indexing produce
/// different IR path depths for the same owned Box slot.
#[test]
fn range_replacement_preserves_borrowed_call_entry_order() {
    let source = br#"fn ignore(part: &u64) -> result: unit pure {
  doc "Borrows without reading.";
  return unit;
}

fn replace(cell: &Box<u64>) -> result: unit writes(cell) {
  doc "Replaces and releases the borrowed owner.";
  let next = box_new::<u64>(value: 1_u64);
  set cell^ = move next;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Orders replacement through a range after borrowed call entry.";
  let slots = slots_new::<Box<u64>, 1>();
  let child = box_new::<u64>(value: 0_u64);
  place_back(window: &slots, value: move child);
  let run = &slots[0_u64..1_u64];
  ignore(part: &slots[0_u64].inner);
  replace(cell: &run^[0_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_release_and_borrow_are_separate(source, "replace");
}

/// An Entries descriptor owns nothing, but writing through it can release
/// an entry's Box payload. Scalar entries retain the permitted overlap.
#[test]
fn entries_payload_release_preserves_borrowed_call_entry_order() {
    for (value_type, value, borrowed, releases) in [
        (
            "Box<u64>",
            "box_new::<u64>(value: 0_u64)",
            "&b^.inner",
            true,
        ),
        ("u64", "0_u64", "&b^", false),
    ] {
        let payload = if releases { "move value" } else { "value" };
        let source = format!(
            r#"const names: Array<u8, 1> =[97_u8];

fn ignore(part: &u64) -> result: unit pure {{
  doc "Borrows without reading.";
  return unit;
}}

fn clear(entries: &Entries<{value_type}>) -> result: unit writes(entries) {{
  doc "Replaces entries and releases their previous payloads.";
  for (i in 0_u64..entries^.len) {{
    set entries^[i] = None<{value_type}>();
  }}
  return unit;
}}

fn borrow_then_clear(entries: &Entries<{value_type}>) -> result: unit writes(entries) {{
  doc "Keeps entry payloads alive until the borrowed call enters.";
  if entries^.len > 0_u64 {{
    let first = &entries^[0_u64];
    match first^ {{
      Some(value: b) => {{
        ignore(part: {borrowed});
        clear(entries: entries);
      }}
      None() => {{
      }}
    }}
  }}
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  doc "Exercises a borrowed Entries view containing one payload.";
  let store = shared_map_new::<{value_type}>(capacity: 1_u64);
  let keys = key_set_new(capacity: 1_u64);
  let index = key_set_insert(keys: &keys, key: &names[0_u64..1_u64]);
  atomic entries = &store[keys] {{
    if entries^.len > 0_u64 {{
      let value = {value};
      set entries^[0_u64] = Some<{value_type}>(value: {payload});
    }}
    borrow_then_clear(entries: entries);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        with_checked(source.as_bytes(), |checked| {
            let permission = checked
                .data
                .permission
                .named("borrow_then_clear")
                .expect("the helper has permission metadata");
            let pairs = permission
                .pairs
                .iter()
                .filter(|pair| {
                    pair.first.callee_name == "ignore" && pair.second.callee_name == "clear"
                })
                .collect::<Vec<_>>();
            assert_eq!(pairs.len(), 1, "{value_type}: one adjacent call pair");
            assert!(
                pairs[0].verdict.is_eligible(),
                "{value_type}: source permission must remain eligible: {:?}",
                pairs[0].verdict
            );

            let program = lower_checked(checked, OverlapLowering::On)
                .expect("the Entries witness must lower");
            let helper = function(&program, "borrow_then_clear");
            let calls = helper
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| match instruction {
                    IrInstruction::Define {
                        result,
                        operation: IrOperation::Call { function, .. },
                        ..
                    } if matches!(
                        program.functions()[*function as usize].name(),
                        "ignore" | "clear"
                    ) =>
                    {
                        Some(*result)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 2, "{value_type}: both calls must be lowered");
            let grouped = helper
                .overlaps()
                .iter()
                .any(|group| calls.iter().all(|call| group.members.contains(call)));
            assert_eq!(
                grouped,
                !releases,
                "{value_type}: only storage-releasing entry writes cut the group: {:?}",
                helper.overlaps()
            );
        });
    }
}

/// The review's selected-field witness: ProjectStruct does not retain the
/// root reached by the borrowed field's address chain.
#[test]
fn field_consumption_preserves_borrowed_call_entry_order() {
    let source = br#"struct Holder {
  doc "Owns one heap cell.";
  cell: Box<u64>;
}

fn ignore(part: &u64) -> result: unit pure {
  doc "Borrows without reading.";
  return unit;
}

fn consume(value: Box<u64>) -> result: unit pure {
  doc "Releases the owned box on return.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Orders field consumption after borrowed call entry.";
  let cell = box_new::<u64>(value: 0_u64);
  let holder = Holder(cell: move cell);
  ignore(part: &holder.cell.inner);
  consume(value: move holder.cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_release_and_borrow_are_separate(source, "consume");
}

/// The review's sibling witness: selecting kept releases discarded while
/// forming the argument, before the consuming call itself begins.
#[test]
fn sibling_cleanup_preserves_borrowed_call_entry_order() {
    let source = br#"struct Holder {
  doc "Owns a selected cell and a sibling released with the holder.";
  kept: Box<u64>;
  discarded: Box<u64>;
}

fn ignore(part: &u64) -> result: unit pure {
  doc "Borrows without reading.";
  return unit;
}

fn consume(value: Box<u64>) -> result: unit pure {
  doc "Releases the selected box on return.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Orders sibling cleanup after borrowed call entry.";
  let kept = box_new::<u64>(value: 0_u64);
  let discarded = box_new::<u64>(value: 1_u64);
  let holder = Holder(kept: move kept, discarded: move discarded);
  ignore(part: &holder.discarded.inner);
  consume(value: move holder.kept);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_release_and_borrow_are_separate(source, "consume");
}

/// Isolate argument cleanup from ownership of heap storage by the callee:
/// Kept is affine but has an empty release. Both a residual sibling and an
/// enclosing Box shell must cut the group before argument formation.
#[test]
fn argument_cleanup_without_heap_argument_prevents_overlap() {
    for (owner, borrowed, selected) in [
        (
            "let holder = Holder(kept: move kept, discarded: move discarded);",
            "holder.discarded.inner",
            "holder.kept",
        ),
        (
            "let content = Holder(kept: move kept, discarded: move discarded);\n  let holder = box_new::<Holder>(value: move content);",
            "holder.inner.discarded.inner",
            "holder.inner.kept",
        ),
    ] {
        let source = format!(
            r#"nocopy struct Kept {{
  doc "Carries no heap storage but requires an explicit move.";
  value: u64;
}}

struct Holder {{
  doc "Owns a nonheap selection and a heap sibling.";
  kept: Kept;
  discarded: Box<u64>;
}}

fn ignore(part: &u64) -> result: unit pure {{
  doc "Borrows without reading.";
  return unit;
}}

fn consume(value: Kept) -> result: unit pure {{
  doc "Consumes a value whose release is empty.";
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  doc "Orders argument cleanup independently of the selected type.";
  let kept = Kept(value: 0_u64);
  let discarded = box_new::<u64>(value: 1_u64);
  {owner}
  ignore(part: &{borrowed});
  consume(value: move {selected});
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        assert_release_and_borrow_are_separate(source.as_bytes(), "consume");
    }
}

fn assert_release_and_borrow_are_separate(source: &[u8], releasing: &str) {
    with_ir_mode(source, OverlapLowering::On, |program| {
        let main = function(program, "main");
        let calls_named = |name: &str| {
            main.blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| match instruction {
                    IrInstruction::Define {
                        result,
                        operation: IrOperation::Call { function, .. },
                        ..
                    } if program.functions()[*function as usize].name() == name => Some(*result),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let borrows = calls_named("ignore");
        let releases = calls_named(releasing);
        assert_eq!(borrows.len(), 1, "one borrowing call");
        assert_eq!(releases.len(), 1, "one releasing call");
        assert!(
            main.overlaps()
                .iter()
                .all(|group| !(group.members.contains(&releases[0])
                    && group.members.contains(&borrows[0]))),
            "{releasing} and the borrowing call must share no group: {:?}",
            main.overlaps()
        );
    });
}

/// A pure owner borrow does not release anything. Neither a blanket ban on
/// Box arguments nor the old owner/content prefix comparison preserves this.
#[test]
fn pure_owner_and_content_borrows_still_overlap() {
    let source = br#"fn ignore_owner(cell: &Box<u64>) -> result: unit pure {
  doc "Borrows an owner without reading or replacing it.";
  return unit;
}

fn ignore_content(part: &u64) -> result: unit pure {
  doc "Borrows the content without reading it.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Keeps pure owner and content borrows eligible for overlap.";
  let p = box_new::<u64>(value: 0_u64);
  ignore_owner(cell: &p);
  ignore_content(part: &p.inner);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let main = function(program, "main");
        let borrows = main
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::Call { function, .. },
                    ..
                } if matches!(
                    program.functions()[*function as usize].name(),
                    "ignore_owner" | "ignore_content"
                ) =>
                {
                    Some(*result)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(borrows.len(), 2, "owner and content borrows");
        assert!(
            main.overlaps()
                .iter()
                .any(|group| borrows.iter().all(|call| group.members.contains(call))),
            "pure owner and content borrows must overlap: {:?}",
            main.overlaps()
        );
    });
}

/// Ownership writes for distinct Boxes are already separated by [PAR-1].
/// A blanket releasing-call cut would serialize these pure consumers.
#[test]
fn disjoint_owned_box_consumers_still_overlap() {
    let source = br#"fn fold(tree: Box<u64>) -> result: u64 pure {
  doc "Reads the owned tree and releases it on return.";
  return tree.inner;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Uses both results after disjoint ownership transfers.";
  let left = box_new::<u64>(value: 17_u64);
  let right = box_new::<u64>(value: 29_u64);
  let a = fold(tree: move left);
  let b = fold(tree: move right);
  if a != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if b != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let main = function(program, "main");
        let consumers = calls_to(program, main, &["fold"]);
        assert_eq!(consumers.len(), 2, "two owned Box consumers");
        assert!(
            main.overlaps()
                .iter()
                .any(|group| consumers.iter().all(|call| group.members.contains(call))),
            "disjoint owned Box consumers must overlap: {:?}",
            main.overlaps()
        );
    });
}

/// Written Box references both borrow and may release. Match-bound sibling
/// subtrees still share a group: their resolved payload paths are disjoint,
/// and forwarding their references reloads no common ancestor owner slot.
#[test]
fn written_box_references_to_disjoint_match_subtrees_still_overlap() {
    let source = br#"enum Node {
  doc "Owns either a leaf value or two disjoint child trees.";
  Leaf(w: u64);
  Branch(left: Box<Node>, right: Box<Node>, w: u64);
}

fn fold(node: &Box<Node>) -> result: u64 writes(node) {
  doc "Folds both children in place and records the branch total.";
  match node^.inner {
    Leaf(w: leaf_w) => {
      return leaf_w^;
    }
    Branch(left: l, right: r, w: slot) => {
      let a = fold(node: l);
      let b = fold(node: r);
      let total = a +wrap b;
      set slot^ = total;
      return total;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Observes a fold over two owned subtrees.";
  let left_node = Node::Leaf(w: 17_u64);
  let left = box_new::<Node>(value: move left_node);
  let right_node = Node::Leaf(w: 29_u64);
  let right = box_new::<Node>(value: move right_node);
  let branch = Node::Branch(left: move left, right: move right, w: 0_u64);
  let root = box_new::<Node>(value: move branch);
  let total = fold(node: &root);
  if total != 46_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let fold = function(program, "fold");
        let children = calls_to(program, fold, &["fold"]);
        assert_eq!(children.len(), 2, "left and right recursive folds");
        assert!(
            fold.overlaps()
                .iter()
                .any(|group| children.iter().all(|call| group.members.contains(call))),
            "written references to disjoint match-bound subtrees must overlap: {:?}",
            fold.overlaps()
        );
    });
}

/// A neutral call must neither hide an earlier conflict nor prevent a new
/// group after it. Both directions, including members that both release and
/// borrow, conflict only when their resolved storage overlaps.
#[test]
fn release_borrow_conflicts_check_every_member_and_restart_groups() {
    for (first, last, row) in [
        (
            "replace(cell: left)",
            "ignore(part: &left^.inner)",
            "writes(left)",
        ),
        (
            "ignore(part: &right^.inner)",
            "replace(cell: right)",
            "writes(right)",
        ),
        (
            "both(cell: left, part: &right^.inner)",
            "both(cell: right, part: &left^.inner)",
            "writes(left), writes(right)",
        ),
        (
            "both(cell: left, part: &right^.inner)",
            "replace(cell: right)",
            "writes(left), writes(right)",
        ),
    ] {
        let source = format!(
            r#"fn replace(cell: &Box<u64>) -> result: u64 writes(cell) {{
  doc "Replaces the owner and releases its old cell.";
  let next = box_new::<u64>(value: 0_u64);
  set cell^ = move next;
  return 0_u64;
}}

fn ignore(part: &u64) -> result: u64 pure {{
  doc "Borrows without reading.";
  return 0_u64;
}}

fn both(cell: &Box<u64>, part: &u64) -> result: u64 writes(cell) {{
  doc "Replaces an owner while borrowing another cell.";
  let replaced = replace(cell: cell);
  return replaced;
}}

fn plain(value: u64) -> result: u64 pure {{
  doc "Passes through a scalar without borrowing or releasing.";
  return value;
}}

fn grouped(left: &Box<u64>, right: &Box<u64>) -> result: u64 {row} {{
  doc "Keeps neutral members on each side of a storage conflict.";
  let a = {first};
  let b = plain(value: 0_u64);
  let c = {last};
  let d = plain(value: 0_u64);
  if a != b {{
    return 1_u64;
  }}
  if c != d {{
    return 1_u64;
  }}
  return 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure {{
  doc "Exercises both groups and observes their results.";
  let left = box_new::<u64>(value: 0_u64);
  let right = box_new::<u64>(value: 0_u64);
  let outcome = grouped(left: &left, right: &right);
  if outcome != 0_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
            let grouped = function(program, "grouped");
            let calls = calls_to(program, grouped, &["replace", "ignore", "both", "plain"]);
            assert_eq!(calls.len(), 4, "{first}; plain; {last}; plain");
            let groups = grouped
                .overlaps()
                .iter()
                .map(|group| group.members.as_slice())
                .collect::<Vec<_>>();
            assert_eq!(
                groups,
                vec![&calls[..2], &calls[2..]],
                "{first}; plain; {last}; plain must form two ordered groups"
            );
        });
    }
}

fn calls_to(program: &IrProgram, caller: &IrFunction, names: &[&str]) -> Vec<IrValueId> {
    caller
        .blocks()
        .iter()
        .flat_map(IrBlock::instructions)
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                result,
                operation: IrOperation::Call { function, .. },
                ..
            } if names.contains(&program.functions()[*function as usize].name()) => Some(*result),
            _ => None,
        })
        .collect()
}

/// Forming `&old^.cells.inner[i].b` loads the owner slot `old^.cells`; the
/// earlier swap releases only storage below `old^.cells.inner[i].a`, which
/// lies in the block that slot points to. Snowghost's `take_prepared` swaps
/// sibling fields of one element this way.
#[test]
fn a_release_below_a_loaded_owner_slot_keeps_the_overlap_group() {
    let source = br#"struct Pair {
  a: Box<u64>;
  b: Box<u64>;
}

struct Holder {
  cells: Box<Slots<Pair>>;
}

fn take(fresh: &Pair, old: &Holder, i: u64) -> result: unit writes(fresh), writes(old) contract {
  requires i < old^.cells.inner.len;
} {
  swap(first: &fresh^.a, second: &old^.cells.inner[i].a);
  swap(first: &fresh^.b, second: &old^.cells.inner[i].b);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let permissions = checked
            .data
            .permission
            .named("take")
            .expect("take permissions");
        let pair = permissions
            .pairs
            .iter()
            .find(|pair| {
                pair.first.callee_name.starts_with("swap")
                    && pair.second.callee_name.starts_with("swap")
            })
            .expect("the two swaps are adjacent");
        assert!(pair.verdict.is_eligible(), "{pair:?}");
        assert!(
            pair.second.storage_effects.has_owner_slot_borrow(),
            "the second formation loads the owner slot: {pair:?}"
        );
        assert_eq!(
            pair.first.storage_effects.conflict(
                &crate::semantic::UnprovedSeparations,
                &pair.second.storage_effects
            ),
            None,
            "{pair:?}"
        );
    });
    with_ir_mode(source, OverlapLowering::On, |program| {
        let take = function(program, "take");
        // A generic prelude row's instance carries its instance key after
        // `$instance$`.
        let calls = take
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::Call { function, .. },
                    ..
                } if program.functions()[*function as usize]
                    .name()
                    .starts_with("swap") =>
                {
                    Some(*result)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_eq!(take.overlaps().len(), 1);
        assert_eq!(take.overlaps()[0].members, calls);
        assert!(
            !program
                .actualization_ledger()
                .iter()
                .any(|line| line.contains("release/borrow conflict")),
            "{:?}",
            program.actualization_ledger()
        );
    });
}
