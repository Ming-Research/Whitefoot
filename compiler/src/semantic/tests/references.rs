//! [REF-1] through [REF-3]: a reference is a local name for a path, its
//! validity is a fact, and it never escapes.
//!
//! This module replaces `tests/borrows.rs` (33 tests), whose whole subject was
//! v0.59's borrow apparatus. Every rule it exercised is retired in v0.60 and
//! each retirement has a named successor:
//!
//! - [OWN-2] borrow modes and [OWN-5] the loan-conflict matrix retire: there
//!   is one reference kind, `&p`, with no permission marker, and whether a
//!   callee may write through a reference parameter is stated by its effect
//!   row alone. The successor is [EFF-5]'s pairwise comparison of substituted
//!   paths, exercised in `two_overlapping_substituted_writes_are_refused` below.
//! - [OWN-6] child reborrows and [OWN-14] the admitted reborrow forms retire:
//!   a reference names a resolved path, and a further step below it is another
//!   path, not a derived loan. The successor is [REF-1]'s path resolution.
//! - [OWN-9] holder suspension and [OWN-12] the suspended-parent rules retire.
//!   The successor is [REF-2]: validity is a fact, invalidated by the seven
//!   enumerated events and re-established only by forming the reference again.
//! - [OWN-3], [OWN-4], [OWN-10] and [FORM-8] retire with regions, which v0.60
//!   does not have at all; there is no successor and no replacement spelling.
//! - [VIEW-6]'s slice return ceiling retires; the successor is [REF-3]'s
//!   no-escape refusal at the `return_stmt`, exercised in
//!   `a_returned_reference_is_an_escape` below.
//!
//! The sources are the already-ported v0.60 conformance cases, which fix the
//! exact spelling of each judgment; these tests add the rule and issue kind the
//! corpus manifest does not pin.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_accepts, assert_rule_kind, with_semantics};

/// Stored formal roots describe the incoming storage, even after the formal
/// reference holder is rebound. The all-source inventory may widen an alias,
/// but must not turn two exchanged formals into an unresolved recursive graph.
#[test]
fn rebound_parameter_summaries_preserve_every_entry_root() {
    let source =
        br#"fn exchange(first: &u64, second: &u64, other: &u64, flag: Bool) -> result: unit pure {
  let saved = first;
  let selected = if flag {
    give first;
  } else {
    give other;
  }
  set first = &deref(second);
  set second = &deref(saved);
  set selected = &deref(first);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        use crate::semantic::model::CheckedStatement;
        use crate::semantic::places::{PlaceMap, PlaceRoot, ResolvedPlace};

        let SemanticOutcome::Complete(program) = outcome else {
            panic!("expected checked reference rebindings, got {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "exchange")
            .unwrap();
        let map = PlaceMap::for_function(function);
        let roots = function.parameters[..3]
            .iter()
            .map(|parameter| ResolvedPlace::binding(parameter.binding))
            .collect::<Vec<_>>();
        for parameter in &function.parameters[..2] {
            let paths = map.resolve(PlaceRoot::Binding(parameter.binding), &[]);
            assert_eq!(paths.len(), 2, "both exchanged entry roots: {paths:?}");
            assert!(roots[..2].iter().all(|root| paths.contains(root)));
        }
        let selected = function
            .body
            .as_ref()
            .unwrap()
            .iter()
            .find_map(|statement| match statement {
                CheckedStatement::ValueMatchLet { binding, .. } => Some(*binding),
                _ => None,
            })
            .unwrap();
        let paths = map.resolve(PlaceRoot::Binding(selected), &[]);
        assert_eq!(
            paths.len(),
            3,
            "the shallow sibling also remains: {paths:?}"
        );
        assert!(roots.iter().all(|root| paths.contains(root)));
    });
}

/// Source-backed coverage replaces the retired synthetic payload-origin
/// reconstruction tests: both roots, complete field/Box/payload suffixes,
/// nested matches, distinct payload fields and variant overlap are retained.
#[test]
fn nested_reference_payload_origins_keep_all_roots_and_suffixes() {
    let source = br#"enum Leaf {
  Pair(left: u64, right: u64);
  Single(value: u64);
}

enum Branch {
  Child(value: Box<Leaf>);
  Empty();
}

struct Envelope {
  payload: Branch;
}

struct Holder {
  value: Box<Envelope>;
}

struct Parent {
  left: Holder;
  right: Holder;
}

fn inspect(first: &Parent, second: &Parent, flag: Bool) -> result: unit reads(first.left.value.inner.payload), reads(second.right.value.inner.payload) {
  let selected = if flag {
    give &deref(first).left;
  } else {
    give &deref(second).right;
  }
  match deref(selected).value.inner.payload {
    Child(value: child) => {
      match deref(child).inner {
        Pair(left: left_value, right: right_value) => {
        }
        Single(value: single) => {
        }
      }
    }
    Empty() => {
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        use crate::semantic::model::CheckedStatement;
        use crate::semantic::places::{
            PlaceMap, PlaceRoot, PlaceStep, ResolvedPlace, UnprovedSeparations,
        };

        let SemanticOutcome::Complete(program) = outcome else {
            panic!("expected checked nested payload aliases, got {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "inspect")
            .unwrap();
        let map = PlaceMap::for_function(function);
        let CheckedStatement::Match { arms: outer, .. } = &function.body.as_ref().unwrap()[1]
        else {
            panic!("expected the outer borrowed match");
        };
        let CheckedStatement::Match { arms: inner, .. } = &outer[0].body[0] else {
            panic!("expected the inner borrowed match");
        };
        let prefix = vec![
            PlaceStep::Field(0),
            PlaceStep::Deref,
            PlaceStep::Field(0),
            PlaceStep::Payload {
                variant: outer[0].tag,
                field: 0,
            },
        ];
        let child = outer[0].binders[0].binding;
        let paths = map.resolve(PlaceRoot::Binding(child), &[]);
        assert_eq!(paths.len(), 2);
        for (field, parameter) in function.parameters[..2].iter().enumerate() {
            let mut expected = vec![PlaceStep::Field(u32::try_from(field).unwrap())];
            expected.extend_from_slice(&prefix);
            assert!(paths.contains(&ResolvedPlace {
                root: PlaceRoot::Binding(parameter.binding),
                path: expected,
            }));
        }
        let mut payloads = Vec::new();
        for arm in inner {
            for binder in &arm.binders {
                let paths = map.resolve(PlaceRoot::Binding(binder.binding), &[]);
                assert_eq!(paths.len(), 2, "every payload keeps both scrutinee roots");
                let mut expected = prefix.clone();
                expected.extend([
                    PlaceStep::Deref,
                    PlaceStep::Payload {
                        variant: arm.tag,
                        field: binder.field,
                    },
                ]);
                for (field, parameter) in function.parameters[..2].iter().enumerate() {
                    let mut complete = vec![PlaceStep::Field(u32::try_from(field).unwrap())];
                    complete.extend_from_slice(&expected);
                    assert!(paths.contains(&ResolvedPlace {
                        root: PlaceRoot::Binding(parameter.binding),
                        path: complete,
                    }));
                }
                payloads.push(paths);
            }
        }
        assert_eq!(payloads.len(), 3);
        let first_root = PlaceRoot::Binding(function.parameters[0].binding);
        let same_root = payloads
            .iter()
            .map(|paths| paths.iter().find(|path| path.root == first_root).unwrap())
            .collect::<Vec<_>>();
        assert!(!map.overlaps(&UnprovedSeparations, same_root[0], same_root[1]));
        assert!(map.overlaps(&UnprovedSeparations, same_root[0], same_root[0]));
        assert!(map.overlaps(&UnprovedSeparations, same_root[0], same_root[2]));
    });
}

/// One source descent has two observed cursor targets; it must not become a
/// path equation whose fixed point invents infinitely many further descents.
#[test]
fn straight_line_recursive_reference_descent_has_finite_origins() {
    let source = br#"struct Node {
  next: Option<Box<Node>>;
}

fn descend(root: &Node) -> result: unit reads(root.next) {
  let cursor = root;
  match deref(cursor).next {
    Some(value: child) => {
      set cursor = &deref(child).inner;
    }
    None() => {
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        use crate::semantic::model::CheckedStatement;
        use crate::semantic::places::{PlaceMap, PlaceRoot, PlaceStep};

        let SemanticOutcome::Complete(program) = outcome else {
            panic!("expected checked one-step descent, got {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "descend")
            .unwrap();
        let CheckedStatement::Let { binding, .. } = &function.body.as_ref().unwrap()[0] else {
            panic!("expected the cursor binding");
        };
        let map = PlaceMap::for_function(function);
        let paths = map.resolve(PlaceRoot::Binding(*binding), &[]);
        assert_eq!(
            paths.len(),
            2,
            "only the entry and the one selected child: {paths:?}"
        );
        assert!(
            paths
                .iter()
                .all(|path| path.root == PlaceRoot::Binding(function.parameters[0].binding))
        );
        assert!(paths.iter().any(|path| path.path.is_empty()));
        assert!(paths.iter().any(|path| matches!(
            path.path.as_slice(),
            [
                PlaceStep::Field(0),
                PlaceStep::Payload { field: 0, .. },
                PlaceStep::Deref
            ]
        )));
    });
}
/// Acyclic source chains beyond the retired 32-expansion boundary remain
/// finite, and an earlier reference never follows a later holder rebind.
#[test]
fn long_parameter_rebindings_keep_captured_entry_targets() {
    use std::fmt::Write;

    let mut source = String::from("fn rebind(");
    for index in 0..40 {
        if index != 0 {
            source.push_str(", ");
        }
        write!(source, "p{index}: &u64").unwrap();
    }
    source.push_str(") -> result: unit pure {\n");
    for index in 0..39 {
        writeln!(source, "  set p{index} = &deref(p{});", index + 1).unwrap();
    }
    source.push_str("  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");
    with_semantics(source.as_bytes(), |outcome| {
        use crate::semantic::places::{PlaceMap, PlaceRoot, ResolvedPlace};

        let SemanticOutcome::Complete(program) = outcome else {
            panic!("expected checked parameter rebindings, got {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "rebind")
            .unwrap();
        let map = PlaceMap::for_function(function);
        for pair in function.parameters.windows(2) {
            let paths = map.resolve(PlaceRoot::Binding(pair[0].binding), &[]);
            assert_eq!(
                paths,
                vec![
                    ResolvedPlace::binding(pair[0].binding),
                    ResolvedPlace::binding(pair[1].binding),
                ]
            );
        }
    });
}

/// [REF-1] a reference variable names a path and is not storage of its own, so
/// `&p` where `p` is a reference variable has no path to name that `p` does not
/// already name.
#[test]
fn a_reference_to_a_reference_variable_is_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/ref1-neg-reference-to-reference-variable.wf"
    );
    assert_rule_kind(source, SemanticRule::Ref1, |kind| {
        matches!(kind, SemanticIssueKind::ReferenceToReferenceVariable { .. })
    });
}

/// [REF-2] a proper prefix of the path being written invalidates the
/// reference, and the use afterwards carries the invalidating event.
#[test]
fn a_prefix_write_invalidates_the_reference_at_its_next_use() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/ref2-neg-use-after-prefix-write.wf");
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(
            kind,
            SemanticIssueKind::InvalidReferenceUse { event, .. }
                if event.contains("proper prefix")
        )
    });
}

/// [REF-2] a move never re-roots an existing reference: after `let w = move v;`
/// the references formed from `v` are invalid and are not reinterpreted as
/// references into `w`.
#[test]
fn a_move_of_the_root_does_not_re_root_its_references() {
    let source = include_bytes!("../../../../tests/conformance/cases/ref2-neg-use-after-move.wf");
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// [REF-2] compares evaluated index steps. Replacing the same dynamically
/// indexed owner is a proper-prefix write even though the step came from a
/// binding rather than a literal.
#[test]
fn a_same_dynamic_index_prefix_replacement_invalidates_the_reference() {
    let source = br#"nocopy struct Record {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let table = slots_new::<Record, 1>();
  let record = Record(value: 7_u8);
  place_back(window: &table, value: move record);
  let index = 0_u64;
  let p = &table[index].value;
  set table[index] = Record(value: 9_u8);
  return std::process::exit_status(code: deref(p));
}
"#;
    assert_rule_kind(
        source,
        SemanticRule::Ref2,
        |kind| matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. } if event.contains("proper prefix")),
    );
}

/// Consuming a whole nocopy owner invalidates every reference rooted there.
#[test]
fn a_whole_nocopy_owner_move_invalidates_its_reference() {
    let source = br#"nocopy struct Record {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let record = Record(value: 7_u8);
  let p = &record;
  let moved = move record;
  return std::process::exit_status(code: deref(p).value);
}
"#;
    assert_rule_kind(
        source,
        SemanticRule::Ref2,
        |kind| matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. } if event.contains("moved out of")),
    );
}

/// Moving a Box's content consumes the Box root, so a reference to that root
/// cannot survive the unboxing operation.
#[test]
fn consuming_box_content_invalidates_a_reference_to_the_whole_box() {
    let source = br#"nocopy struct Record {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let record = Record(value: 7_u8);
  let boxed = box_new::<Record>(value: move record);
  let p = &boxed;
  let extracted = move boxed.inner;
  return std::process::exit_status(code: deref(p).inner.value);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// [REF-2] writing the storage at a reference's own path, or below it, is a
/// content write and invalidates nothing. This is the half of the lattice a
/// conservative implementation would get wrong by invalidating on every write.
#[test]
fn a_content_write_at_or_below_the_path_keeps_the_reference_valid() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/ref2-pos-content-write-keeps-validity.wf"
    ));
}

/// The invalidation boundary is precise: writing the referenced content and
/// replacing a statically distinct indexed owner both preserve the reference.
#[test]
fn exact_content_and_distinct_literal_index_writes_preserve_references() {
    assert_accepts(
        br#"nocopy struct Record {
  value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let table = slots_new::<Record, 2>();
  let first = Record(value: 7_u8);
  place_back(window: &table, value: move first);
  let second = Record(value: 8_u8);
  place_back(window: &table, value: move second);
  let p = &table[0_u64].value;
  set table[0_u64].value = 9_u8;
  set table[1_u64] = Record(value: 4_u8);
  return std::process::exit_status(code: deref(p));
}
"#,
    );
}

/// [REF-3] a `return_stmt` whose selected expression is a reference is the
/// escape violation, and [FN-1] forms no candidate there.
#[test]
fn a_returned_reference_is_an_escape() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/ref3-neg-returned-reference.wf");
    assert_rule_kind(source, SemanticRule::Ref3, |kind| {
        matches!(kind, SemanticIssueKind::EscapingReference { .. })
    });
}

/// [REF-3] the restructuring the rule states — return an index and let the
/// caller form the reference — is accepted.
#[test]
fn returning_an_index_instead_of_a_reference_is_admitted() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/ref3-pos-index-result.wf"
    ));
}

/// [EFF-5] clause 1: two substituted effects on overlapping paths where at
/// least one is a write must be proved disjoint; passing one place as both
/// actuals is the refusal at the complete `call`.
#[test]
fn two_overlapping_substituted_writes_are_refused() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/eff5-neg-overlapping-substituted-write.wf"
    );
    assert_rule_kind(source, SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::OverlappingCallEffects { .. })
    });
}

/// A pair-writing helper whose row is `ROW`, called once on a local pair.
const PAIR_ACT: &str = r#"struct Pair {
  first: u8;
  second: u8;
}

fn act(pair: &Pair) -> result: unit ROW {
  BODY
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(first: 1_u8, second: 2_u8);
  act(pair: &pair);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// [EFF-1] a `writes` entry states every access at or below its path and a
/// `reads` entry every observation at or below its path, so a row that also
/// lists an entry one of them covers states that access twice. The entry is
/// refused where it is written, naming the first entry in written order that
/// covers it, and no call is left to meet the pair [EFF-5].
#[test]
fn an_entry_another_entry_covers_is_refused_at_the_row() {
    let body = "let old = deref(pair).first;\n  set deref(pair).second = old;";
    for (row, entry, covering) in [
        (
            "reads(pair.first), writes(pair)",
            "reads(pair.first)",
            "writes(pair)",
        ),
        (
            "writes(pair), writes(pair.first)",
            "writes(pair.first)",
            "writes(pair)",
        ),
        (
            "writes(pair.first), writes(pair)",
            "writes(pair.first)",
            "writes(pair)",
        ),
        (
            "reads(pair), reads(pair.first)",
            "reads(pair.first)",
            "reads(pair)",
        ),
        (
            "reads(pair.first), reads(pair)",
            "reads(pair.first)",
            "reads(pair)",
        ),
        // Both later entries cover the first one; the earlier cover is named.
        (
            "reads(pair.first), reads(pair), writes(pair)",
            "reads(pair.first)",
            "reads(pair)",
        ),
    ] {
        let source = PAIR_ACT.replace("ROW", row).replace("BODY", body);
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected an EFF-1 rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Eff1);
            assert_eq!(
                issue.kind(),
                &SemanticIssueKind::SubsumedEffectEntry {
                    entry: entry.to_owned(),
                    covering: covering.to_owned(),
                }
            );
        });
        super::assert_rule_at(source.as_bytes(), SemanticRule::Eff1, entry);
    }
    // Two sibling reads cover nothing of each other.
    assert_accepts(
        PAIR_ACT
            .replace("ROW", "reads(pair.first), reads(pair.second)")
            .replace(
                "BODY",
                "let first = deref(pair).first;\n  let second = deref(pair).second;",
            )
            .as_bytes(),
    );
}

/// [EFF-5] two effects one argument supplies are compared only when the
/// values of their positions could separate them. A whole read beside a
/// write below it overlaps at every position: the callee reaches both
/// through its one parameter, so the call proves nothing about the pair and
/// is admitted, where every call used to refuse it.
#[test]
fn one_argument_entries_that_overlap_at_every_position_are_not_compared() {
    let body = "let whole = deref(pair);\n  set deref(pair).first = whole.second;";
    assert_accepts(
        PAIR_ACT
            .replace("ROW", "reads(pair), writes(pair.first)")
            .replace("BODY", body)
            .as_bytes(),
    );
    // One argument that is a joined reference supplies both entries for each
    // place it may name; the cross pairs need no `i != j` either, because the
    // parameter names one of those places on any one call.
    assert_accepts(
        br#"struct Cell {
  count: u64;
  total: u64;
}

fn record(cell: &Cell) -> result: unit reads(cell), writes(cell.count) {
  let whole = deref(cell);
  set deref(cell).count = whole.total;
  return unit;
}

fn pick(values: &Array<Cell, 4>, i: u64, j: u64, choose: Bool) -> result: unit reads(values[i]), reads(values[j]), writes(values[i].count), writes(values[j].count) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let selected = &deref(values)[i];
  if choose {
    set selected = &deref(values)[j];
  }
  record(cell: selected);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

/// [EFF-5] one argument's two effects whose overlap depends on position
/// values are still compared at the call, and so are two arguments' effects
/// however each argument's own entries relate.
#[test]
fn position_dependent_and_cross_argument_pairs_are_still_compared() {
    assert_rule_kind(
        br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  copy_within(values: &values, i: 1_u64, j: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Eff5,
        |kind| matches!(kind, SemanticIssueKind::OverlappingCallEffects { .. }),
    );
    // [WIN-2] a slot overlaps `r.last` whatever its value, because one
    // `writes(r.last)` covers every `take_back` a body makes, so the pair one
    // argument supplies overlaps at every position and no call compares it,
    // while the same pair from two arguments is compared and refused.
    let take_after_read = |slot: &str| {
        format!(
            "fn take_after_read(window: &Slots<u64, 4>, i: u64) -> result: u64 reads(window[i]), writes(window.last), writes(window.len) contract {{
  requires i < deref(window).len;
}} {{
  let observed = deref(window)[i];
  let taken = take_back(window: window);
  return observed +wrap taken;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let sum = take_after_read(window: &window, i: {slot});
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_accepts(take_after_read("1_u64").as_bytes());
    assert_accepts(take_after_read("0_u64").as_bytes());
    assert_rule_kind(
        br#"fn take_after_read(source: &Slots<u64, 4>, window: &Slots<u64, 4>, i: u64) -> result: u64 reads(source[i]), writes(window.last), writes(window.len) contract {
  requires i < deref(source).len;
  requires 0_u64 < deref(window).len;
} {
  let observed = deref(source)[i];
  let taken = take_back(window: window);
  return observed +wrap taken;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let sum = take_after_read(source: &window, window: &window, i: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Eff5,
        |kind| matches!(kind, SemanticIssueKind::OverlappingCallEffects { .. }),
    );
    // The written field passed again through a second parameter is a pair of
    // two arguments.
    assert_rule_kind(
        br#"struct Stats {
  count: u64;
  total: u64;
}

fn record(stats: &Stats, extra: &u64) -> result: unit reads(stats), reads(extra), writes(stats.count) {
  let whole = deref(stats);
  let added = deref(extra);
  set deref(stats).count = whole.total +wrap added;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let stats = Stats(count: 1_u64, total: 7_u64);
  record(stats: &stats, extra: &stats.count);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Eff5,
        |kind| {
            matches!(kind, SemanticIssueKind::OverlappingCallEffects { first, second, .. }
                if first == "stats.count" && second == "stats.count")
        },
    );
}

/// [EFF-5, FORM-2] the two substituted paths an EFF-5 rejection carries are
/// spelled as the caller writes the places: a local by its name, the storage
/// a reference parameter names under `deref`, and fields by their names —
/// never a checker binding number or field ordinal.
#[test]
fn overlapping_call_effects_carry_source_spelled_paths() {
    let callee = r#"struct Pair {
  first: u8;
  second: u8;
}

fn act(pair: &Pair, seen: &u8) -> result: unit reads(seen), writes(pair) {
  let old = deref(seen);
  set deref(pair).second = old;
  return unit;
}

"#;
    for (caller, first, second) in [
        (
            "fn main() -> status: std::process::ExitStatus pure {\n  let pair = Pair(first: 1_u8, second: 2_u8);\n  act(pair: &pair, seen: &pair.first);\n  return std::process::exit_status(code: 0_u8);\n}\n",
            "pair.first",
            "pair",
        ),
        (
            "fn relay(holder: &Pair) -> result: unit writes(holder) {\n  act(pair: holder, seen: &deref(holder).first);\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
            "deref(holder).first",
            "deref(holder)",
        ),
    ] {
        let source = format!("{callee}{caller}");
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected an EFF-5 rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Eff5);
            let SemanticIssueKind::OverlappingCallEffects {
                first: rendered_first,
                second: rendered_second,
                ..
            } = issue.kind()
            else {
                panic!("unexpected kind {:?}", issue.kind());
            };
            assert_eq!(
                (rendered_first.as_str(), rendered_second.as_str()),
                (first, second)
            );
        });
    }
}

/// [EFF-5, REF-1] an index position in a substituted path spells the value
/// its argument captured: here the two bindings the call passed.
#[test]
fn an_undischarged_call_separation_names_the_captured_indices() {
    let source = br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit reads(window.len), writes(window[first]), writes(window[second]) {
  let length = deref(window).len;
  if first < length {
    if second < length {
      set deref(window)[first] = 1_u8;
      set deref(window)[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  let i = 0_u64;
  let j = 0_u64;
  write_two(window: &window, first: i, second: j);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected an EFF-5 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff5);
        let SemanticIssueKind::UndischargedCallSeparation { residual, .. } = issue.kind() else {
            panic!("unexpected kind {:?}", issue.kind());
        };
        assert_eq!(
            residual,
            "window[i] and window[j] require their captured indices to be distinct"
        );
    });
}

/// [EFF-5, REF-4] a range formed at the call is the actual's path extended
/// by its own range step, and that step spells the endpoints it captured:
/// over a local array and, re-sliced, through a range parameter's `deref`.
#[test]
fn an_undischarged_call_separation_names_ranges_formed_at_the_call() {
    let callee = r#"fn fill_two(first: &[u8], second: &[u8]) -> result: unit writes(first), writes(second) {
  if 0_u64 < deref(first).len {
    set deref(first)[0_u64] = 1_u8;
  }
  if 0_u64 < deref(second).len {
    set deref(second)[0_u64] = 2_u8;
  }
  return unit;
}

"#;
    for (caller, residual) in [
        (
            "fn main() -> status: std::process::ExitStatus pure {\n  let values = array_filled::<u8, 4>(value: 0_u8);\n  let lo = 1_u64;\n  let hi = 3_u64;\n  fill_two(first: &values[0_u64..2_u64], second: &values[lo..hi]);\n  return std::process::exit_status(code: 0_u8);\n}\n",
            "values[0_u64..2_u64] and values[lo..hi] select different storage (one ends before the other starts, or one is empty)",
        ),
        (
            "fn relay(part: &[u8]) -> result: unit writes(part) {\n  if 2_u64 <= deref(part).len {\n    fill_two(first: &deref(part)[0_u64..2_u64], second: &deref(part)[1_u64..2_u64]);\n  }\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
            "deref(part)[0_u64..2_u64] and deref(part)[1_u64..2_u64] select different storage (one ends before the other starts, or one is empty)",
        ),
    ] {
        let source = format!("{callee}{caller}");
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
                panic!("expected an EFF-5 rejection, got {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Eff5);
            let SemanticIssueKind::UndischargedCallSeparation {
                residual: rendered, ..
            } = issue.kind()
            else {
                panic!("unexpected kind {:?}", issue.kind());
            };
            assert_eq!(rendered, residual);
        });
    }
}

/// [EFF-5] substitutes the scalar values of index actuals, not the storage
/// paths from which those values were read. Distinct array elements can both
/// contain zero and therefore select the same written window element.
#[test]
fn substituted_index_values_do_not_inherit_their_actual_storage_separation() {
    let source = br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit reads(window.len), writes(window[first]), writes(window[second]) {
  let length = deref(window).len;
  if first < length {
    if second < length {
      set deref(window)[first] = 1_u8;
      set deref(window)[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let indices = array_filled::<u64, 2>(value: 0_u64);
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  write_two(window: &window, first: indices[0_u64], second: indices[1_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Eff5, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedCallSeparation { residual, .. }
                if residual.contains("captured indices to be distinct")
        )
    });
}

/// [EFF-5] may discharge two indexed writes from the values captured for this
/// call. The strict guard dominates the call, so its disequality is available
/// for the two occurrence-specific actual captures.
#[test]
fn indexed_call_separation_accepts_strict_orderings_and_disequality() {
    let source = br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit reads(window.len), writes(window[first]), writes(window[second]) {
  let length = deref(window).len;
  if first < length {
    if second < length {
      set deref(window)[first] = 1_u8;
      set deref(window)[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  let i = 0_u64;
  let j = 1_u64;
  if i < j {
    write_two(window: &window, first: i, second: j);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

#[test]
fn indexed_call_separation_does_not_retarget_captured_bindings() {
    let source =
        br#"fn write_refs(first: &u8, second: &u8) -> result: unit writes(first), writes(second) {
  set deref(first) = 1_u8;
  set deref(second) = 2_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 0_u8);
  let i = 0_u64;
  let j = 0_u64;
  let first = &values[i];
  let second = &values[j];
  set j = 1_u64;
  if i < j {
    write_refs(first: first, second: second);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallSeparation { residual, .. }
            if residual.contains("captured indices to be distinct"))
    });
}

#[test]
fn indexed_call_separation_keeps_formation_proof_after_source_writes() {
    let source =
        br#"fn write_refs(first: &u8, second: &u8) -> result: unit writes(first), writes(second) {
  set deref(first) = 1_u8;
  set deref(second) = 2_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 0_u8);
  let i = 0_u64;
  let j = 1_u64;
  if i < j {
    let first = &values[i];
    let second = &values[j];
    set j = 0_u64;
    write_refs(first: first, second: second);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

#[test]
fn indexed_call_separation_is_unique_per_call_actual() {
    let source = br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit reads(window.len), writes(window[first]), writes(window[second]) {
  let length = deref(window).len;
  if first < length {
    if second < length {
      set deref(window)[first] = 1_u8;
      set deref(window)[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  let i = 0_u64;
  let j = 1_u64;
  if i < j {
    write_two(window: &window, first: i, second: j);
  }
  set j = 0_u64;
  write_two(window: &window, first: i, second: j);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallSeparation { .. })
    });
}

/// [EFF-5] one call may owe several separations: a write compared with two
/// reads of one window is two questions at one call, each its own obligation
/// record answered by its own judgment. Both proved accepts the call; both
/// unproved is the EFF-5 refusal, never a disagreement between the checker's
/// records and the engine's judgments.
#[test]
fn two_separations_at_one_call_are_judged_one_by_one() {
    const TWO: &str = r#"fn touch3(a: &Slots<u64, 4>, b: &Slots<u64, 4>, c: &Slots<u64, 4>, i: u64, j: u64, k: u64) -> result: unit reads(b[j]), reads(c[k]), writes(a[i]) contract {
  requires i < deref(a).len;
  requires j < deref(b).len;
  requires k < deref(c).len;
} {
  let x = deref(b)[j];
  let y = deref(c)[k];
  set deref(a)[i] = 9_u64;
  return unit;
}

fn run(r: &Slots<u64, 4>, i: u64, j: u64, k: u64) -> result: unit reads(r[j]), reads(r[k]), writes(r[i]) contract {
  requires i < deref(r).len;
  requires j < deref(r).len;
  requires k < deref(r).len;
  requires i != j;
  requires i != k;
} {
  touch3(a: r, b: r, c: r, i: i, j: j, k: k);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = slots_new::<u64, 4>();
  place_back(window: &r, value: 1_u64);
  place_back(window: &r, value: 2_u64);
  place_back(window: &r, value: 3_u64);
  run(r: &r, i: 0_u64, j: 1_u64, k: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(TWO.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("two proved separations at one call must be accepted: {outcome:?}");
        };
        let run = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("run is checked");
        let separations = run
            .entailment
            .obligations
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome.family,
                    super::super::entailment::ObligationFamily::CallSeparation(_)
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(separations.len(), 2);
        assert_eq!(separations[0].node_path, separations[1].node_path);
        assert_ne!(separations[0].family, separations[1].family);
        assert!(separations.iter().all(|outcome| outcome.discharged));
    });
    let unproved = TWO.replace("  requires i != j;\n  requires i != k;\n", "");
    assert_ne!(unproved, TWO);
    assert_rule_kind(unproved.as_bytes(), SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallSeparation { .. })
    });
}

/// [EFF-5] clause 3: a live reference outside the call whose path has a proper
/// prefix among the call's substituted write paths becomes invalid after the
/// call. The citation is [REF-2]'s, because the rejection is at the later use.
#[test]
fn a_call_write_invalidates_the_bystander_reference() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/eff5-neg-bystander-invalidated-by-call.wf"
    );
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// [EFF-5] substituted paths that are disjoint by different roots are
/// admitted, which is the acceptance half the pairwise comparison must keep.
#[test]
fn disjoint_substituted_paths_are_admitted() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/eff5-pos-substituted-paths-disjoint.wf"
    ));
}

/// [EFF-5] each IDENT index or range endpoint of a declared row takes the value
/// its own argument supplies, evaluated once at the call.
#[test]
fn an_index_endpoint_takes_its_own_arguments_value() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/eff5-pos-index-endpoint-substitution.wf"
    ));
}

/// [TYPE-7] there is no implicit read through a reference: a reference binding
/// used where a value of its referent type is expected is a hard error whose
/// mechanical fix is `deref(.)`.
#[test]
fn reading_through_a_reference_is_explicit() {
    let source = include_bytes!("../../../../tests/conformance/cases/type7-neg-implicit-read.wf");
    assert_rule_kind(source, SemanticRule::Type7, |kind| {
        matches!(kind, SemanticIssueKind::MissingDereference { .. })
    });
}

/// [TYPE-7] `deref` takes a reference and nothing else. A `Box` is not a
/// reference: its content is the field `inner` [TYPE-9], which is what the
/// restructuring says.
#[test]
fn deref_of_a_box_binding_names_no_referent() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/type7-neg-deref-box-binding.wf");
    assert_rule_kind(source, SemanticRule::Type7, |kind| {
        matches!(kind, SemanticIssueKind::MissingDereference { .. })
    });
}

/// [TYPE-7] `deref` of an owned place that is not a reference at all.
#[test]
fn deref_of_a_non_reference_is_refused() {
    let source = include_bytes!("../../../../tests/conformance/cases/type7-neg-deref-nonref.wf");
    assert_rule_kind(source, SemanticRule::Type7, |kind| {
        matches!(kind, SemanticIssueKind::MissingDereference { .. })
    });
}

/// [REF-1] an index expression inside a path is evaluated when the reference is
/// formed; the path records that value, and later assignments to the variables
/// the expression used do not change it.
#[test]
fn an_index_inside_a_path_is_captured_at_formation() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/ref1-pos-index-captured-at-formation.wf"
    ));
}

/// [REF-1] at a control-flow join a reference variable's target is the union of
/// the path sets its incoming edges may name, and every check on it must hold
/// for every member of that set.
#[test]
fn a_join_takes_the_union_of_the_path_sets() {
    assert_accepts(include_bytes!(
        "../../../../tests/conformance/cases/ref1-pos-join-path-set.wf"
    ));
}

/// A direct write through a joined holder must be writable at every possible
/// target. The runtime address is singular, but the constant alternative
/// cannot disappear behind the writable local alternative [REF-1, CONST-2].
#[test]
fn a_joined_dereference_cannot_write_a_possible_constant_target() {
    let source = br#"const permanent: u64 = 1_u64;

fn examine(flag: Bool) -> result: unit pure {
  let spare = 0_u64;
  let selected = if flag {
    give &spare;
  } else {
    give &permanent;
  }
  set deref(selected) = 9_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Const2, |kind| {
        matches!(kind, SemanticIssueKind::ImmutableSetTarget)
    });
}

/// [EFF-2] reads through a joined holder exhibit every formal-rooted member.
/// Declaring only one incoming path is narrower than the body access.
#[test]
fn a_joined_dereference_exhibits_every_possible_parameter_read() {
    let source = br#"fn choose(flag: Bool, a: &u64, b: &u64) -> result: u64 reads(a) {
  let selected = if flag {
    give a;
  } else {
    give b;
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Eff2, |kind| {
        matches!(kind, SemanticIssueKind::EffectMismatch { .. })
    });
}

/// [REF-1, OP-12] two holders with the same singleton resolved path name the
/// same target. The old affine value may therefore enter the updating call
/// through either holder while the commit uses the other holder's runtime
/// address.
#[test]
fn singleton_reference_aliases_name_one_atomic_update_target() {
    let source = br#"nocopy struct Token {
  value: u64;
}

fn retain(old: Token) -> result: Token pure {
  return move old;
}

fn main() -> status: std::process::ExitStatus pure {
  let token = Token(value: 7_u64);
  let target = &token;
  let aliased = &token;
  set deref(target) = retain(old: move deref(aliased));
  if deref(target).value == 7_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    assert_accepts(source);
}

/// Sharing one possible member does not make a joined target exact. Selecting
/// that member would make the update atomic only on one runtime branch, so the
/// move through the singleton alias remains [OWN-1]'s refusal.
#[test]
fn an_overlapping_join_is_not_one_atomic_update_target() {
    let source = br#"nocopy struct Token {
  value: u64;
}

fn retain(old: Token) -> result: Token pure {
  return move old;
}

fn examine(flag: Bool) -> result: unit pure {
  let first = Token(value: 1_u64);
  let second = Token(value: 2_u64);
  let target = if flag {
    give &first;
  } else {
    give &second;
  }
  let aliased = &first;
  set deref(target) = retain(old: move deref(aliased));
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::MoveThroughReference { .. })
    });
}

/// A call requirement over a joined reference must hold for every path the
/// reference may name. One target carrying the required value cannot hide the
/// other target's contradictory value.
#[test]
fn a_joined_reference_call_actual_checks_every_possible_target() {
    let source = br#"fn needs_one(value: &u64) -> result: unit reads(value) contract {
  requires deref(value) == 1_u64;
} {
  let observed = deref(value);
  return unit;
}

fn examine(flag: Bool) -> result: unit pure {
  let a = 1_u64;
  let b = 0_u64;
  let p = if flag {
    give &a;
  } else {
    give &b;
  }
  let completed = needs_one(value: p);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

/// Reborrowing a joined reference retains all of its possible write targets.
/// The call may zero `b`, so the earlier nonzero guard cannot authorize the
/// following exact division.
#[test]
fn a_reborrow_of_a_joined_reference_kills_facts_for_every_possible_target() {
    let source = br#"fn zero(target: &u64) -> result: unit writes(target) {
  set deref(target) = 0_u64;
  return unit;
}

fn examine(flag: Bool) -> result: u64 pure {
  let a = 1_u64;
  let b = 1_u64;
  let p = if flag {
    give &a;
  } else {
    give &b;
  }
  let q = &deref(p);
  if b != 0_u64 {
    zero(target: q);
    return 1_u64 / b;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = False();
  let value = examine(flag: flag);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op2, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedIntegerDomainObligation { .. }
        )
    });
}

/// A re-slice formed from a joined range keeps the union of its origins. A
/// proper-prefix replacement of either origin invalidates the derived range.
#[test]
fn a_reslice_of_a_joined_range_is_invalidated_by_either_origin_replacement() {
    let source = br#"fn examine(flag: Bool) -> result: u8 pure {
  let a = array_filled::<u8, 2>(value: 1_u8);
  let b = array_filled::<u8, 2>(value: 2_u8);
  let part = if flag {
    give &a[0_u64..2_u64];
  } else {
    give &b[0_u64..2_u64];
  }
  let first = &deref(part)[0_u64..1_u64];
  let replacement = array_filled::<u8, 2>(value: 3_u8);
  set b = replacement;
  return deref(first)[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = False();
  let value = examine(flag: flag);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// A reference delivered from an expression with a returning alternative has
/// the same write reachability as an ordinary reference. The write kills the
/// branch fact before the exact division is checked.
#[test]
fn a_delivered_reference_with_a_returning_alternative_kills_stale_facts() {
    let source = br#"fn zero(target: &u64) -> result: unit writes(target) {
  set deref(target) = 0_u64;
  return unit;
}

fn examine(flag: u64) -> result: u64 pure {
  let value = 1_u64;
  let p = if flag == 1_u64 {
    give &value;
  } else {
    return 0_u64;
  }
  if value != 0_u64 {
    zero(target: p);
    return 1_u64 / value;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = examine(flag: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op2, |_| true);
}
#[test]
fn a_written_reference_cannot_select_named_constant_storage() {
    let source = br#"const permanent: u64 = 1_u64;

fn overwrite(target: &u64) -> result: unit writes(target) {
  set deref(target) = 9_u64;
  return unit;
}

fn examine(flag: Bool) -> result: unit pure {
  let spare = 0_u64;
  let original = &permanent;
  let aliased = original;
  let selected = if flag {
    give &spare;
  } else {
    give aliased;
  }
  overwrite(target: selected);
  return unit;
}
"#;
    super::assert_rule_kind(source, SemanticRule::Const2, |kind| {
        matches!(kind, SemanticIssueKind::ImmutableWrittenArgument { binding, .. }
            if binding == "permanent")
    });
}

#[test]
fn a_constant_can_be_read_by_reference_and_copied_to_writable_storage() {
    let source = br#"const permanent: u64 = 1_u64;

fn observe(value: &u64) -> result: u64 reads(value) {
  return deref(value);
}

fn overwrite(target: &u64) -> result: unit writes(target) {
  set deref(target) = 9_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = observe(value: &permanent);
  overwrite(target: &copied);
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_accepts(source);
}

/// [REF-2] a selected payload remains in place after its selecting arm ends.
#[test]
fn a_selected_payload_reference_survives_arm_exit() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 reads(packet) {
  let fallback = 7_u64;
  let selected = &fallback;
  match deref(packet) {
    Data(value: payload) => {
      set selected = &deref(payload);
    }
    Idle() => {
    }
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [REF-2] an `if` branch is a lexical scope just as a loop body is. A local
/// root borrowed into an outer reference dies on the branch edge.
#[test]
fn a_reference_to_an_if_local_dies_at_branch_exit() {
    let source = br#"fn examine(flag: Bool) -> result: u64 pure {
  let fallback = 7_u64;
  let selected = &fallback;
  if flag {
    let local = 9_u64;
    set selected = &local;
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. }
            if event.contains("scope"))
    });
}

/// [REF-1] a reference that does not cross a loop backedge may be rebound to
/// another path shape. The new path, kind, and referent type are retained.
#[test]
fn a_non_loop_reference_may_change_path_shape() {
    let source = br#"fn examine() -> result: u64 pure {
  let first = 7_u64;
  let second = 9_u64;
  let selected = &first;
  set selected = &second;
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// Rebinding changes only the path. It cannot silently change the reference
/// variable's referent type while retaining the old checked type.
#[test]
fn a_reference_rebinding_keeps_its_referent_type() {
    let source = br#"fn examine() -> result: u64 pure {
  let first = 7_u64;
  let second = 9_u8;
  let selected = &first;
  set selected = &second;
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Type5, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// A single-place reference and a range reference are different reference
/// kinds even when both name the same element type.
#[test]
fn a_reference_rebinding_keeps_its_reference_kind() {
    let source = br#"fn examine() -> result: u64 pure {
  let values = array_filled::<u64, 2>(value: 7_u64);
  let selected = &values[0_u64];
  set selected = &values[0_u64..1_u64];
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Type5, |kind| {
        matches!(kind, SemanticIssueKind::TypeMismatch { .. })
    });
}

/// A direct value-match delivery retains the selected address, without
/// exporting the selecting arm's variant fact.
#[test]
fn a_selected_payload_reference_survives_value_match_delivery() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 reads(packet) {
  let fallback = 7_u64;
  let selected = match deref(packet) {
    Data(value: payload) => {
      give &deref(payload);
    }
    Idle() => {
      give &fallback;
    }
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// A `give` crossing a nested match retains an existing selected payload.
#[test]
fn a_selected_payload_reference_survives_nested_give() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet, choose: Bool) -> result: u64 reads(packet) {
  let fallback = 7_u64;
  let selected = if choose {
    match deref(packet) {
      Data(value: payload) => {
        give &deref(payload);
      }
      Idle() => {
        give &fallback;
      }
    }
  } else {
    give &fallback;
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// A reference leaving on a break edge can select another root and payload.
#[test]
fn a_reference_on_a_break_edge_may_change_its_root() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 reads(packet) {
  let fallback = 7_u64;
  let selected = &fallback;
  loop @done {
    match deref(packet) {
      Data(value: payload) => {
        set selected = &deref(payload);
        break @done;
      }
      Idle() => {
        break @done;
      }
    }
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [REF-1] a loop-carried reference keeps one static path shape while the
/// captured index may change on each backedge. The arbitrary header must
/// therefore admit the prior iteration's path instead of stopping at an
/// ownership-join capability limit.
#[test]
fn a_loop_carried_reference_may_change_its_captured_index() {
    let source = br#"fn inspect(values: &Array<u64, 3>) -> result: u64 reads(values) {
  let selected = &deref(values)[0_u64];
  let result = 0_u64;
  for (i in 0_u64..3_u64) {
    let current = deref(selected);
    set result = result +wrap current;
    set selected = &deref(values)[i];
  }
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// A counted header has both its zero-trip preheader edge and every possible
/// backedge. The reference after the loop may consequently name the initial
/// element or the element selected by the last completed iteration; neither
/// edge may be dropped merely because the body has one syntactic rebinding.
#[test]
fn a_counted_reference_continuation_includes_zero_trip_and_backedges() {
    let source =
        br#"fn select(values: &Array<u64, 3>, count: u64) -> result: u64 reads(values) contract {
  requires count <= 2_u64;
} {
  let selected = &deref(values)[0_u64];
  for (i in 0_u64..count) {
    let next = i + 1_u64;
    set selected = &deref(values)[next];
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// [REF-2] the normal backedge can carry an invalid reference to the next
/// iteration. A use before that iteration reforms the reference is therefore
/// invalid even though the preheader supplied a valid path on the first trip.
#[test]
fn a_loop_head_use_observes_a_prior_iteration_window_invalidation() {
    let source = br#"fn inspect(owner: &Box<Slots<u64>>) -> result: u64 writes(owner) contract {
  requires 0_u64 < deref(owner).inner.len;
  requires deref(owner).inner.cap <= 4_u64;
} {
  let selected = &deref(owner).inner[0_u64];
  let result = 0_u64;
  for (i in 0_u64..2_u64) {
    let current = deref(selected);
    set result = result +wrap current;
    let capacity = deref(owner).inner.cap;
    grow(cell: owner, capacity: capacity);
  }
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. }
            if event.contains("call wrote a proper prefix"))
    });
}

/// Reforming the reference before every use cuts the dependency on the
/// possibly invalid header value. The invalidation at the end of one
/// iteration may therefore flow to the next head without making this body
/// unsafe. A non-continuing invalidation likewise creates no future use.
#[test]
fn reforming_before_use_and_noncontinuing_invalidation_are_valid() {
    let source = br#"fn reform(owner: &Box<Slots<u64>>) -> result: u64 writes(owner) contract {
  requires 0_u64 < deref(owner).inner.len;
  requires deref(owner).inner.cap <= 4_u64;
} {
  let selected = &deref(owner).inner[0_u64];
  let result = 0_u64;
  for (i in 0_u64..2_u64) {
    let nonempty = 0_u64 < deref(owner).inner.len;
    if nonempty {
      set selected = &deref(owner).inner[0_u64];
      let current = deref(selected);
      set result = result +wrap current;
      let capacity = deref(owner).inner.cap;
      let allocation_fits = capacity <= 4_u64;
      if allocation_fits {
        grow(cell: owner, capacity: capacity);
      }
    }
  }
  return result;
}

fn one_trip(owner: &Box<Slots<u64>>) -> result: u64 writes(owner) contract {
  requires 0_u64 < deref(owner).inner.len;
  requires deref(owner).inner.cap <= 4_u64;
} {
  let selected = &deref(owner).inner[0_u64];
  loop @done {
    let current = deref(selected);
    let capacity = deref(owner).inner.cap;
    grow(cell: owner, capacity: capacity);
    break @done;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// A break join includes the branch which never reforms the invalidated
/// reference, even when another branch selects the replacement payload.
#[test]
fn a_break_edge_with_an_unrepaired_invalid_reference_is_rejected() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 writes(packet) {
  match deref(packet) {
    Data(value: outer_payload) => {
      let selected = &deref(outer_payload);
      set deref(packet) = Packet::Data(value: 2_u64);
      loop @done {
        match deref(packet) {
          Data(value: inner_payload) => {
            set selected = &deref(inner_payload);
            break @done;
          }
          Idle() => {
            break @done;
          }
        }
      }
      return deref(selected);
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. }
            if event.contains("proper prefix"))
    });
}

/// [REF-1] a dereferenced joined scrutinee retains every possible enum root.
/// Replacing the second root in one arm must invalidate its payload binder;
/// choosing the first access as a representative would wrongly accept it.
#[test]
fn a_joined_match_scrutinee_keeps_every_payload_origin() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(choose: Bool) -> result: u64 pure {
  let first = Packet::Data(value: 1_u64);
  let second = Packet::Data(value: 2_u64);
  let selected = if choose {
    give &first;
  } else {
    give &second;
  }
  match deref(selected) {
    Data(value: payload) => {
      set second = Packet::Idle();
      return deref(payload);
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// The index operand is evaluated to select the matched range element, but
/// the scalar storage read while doing so is not another enum referent. A
/// later write to that scalar therefore leaves the payload reference valid.
#[test]
fn an_indexed_match_does_not_treat_index_storage_as_an_enum_origin() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine() -> result: u64 pure {
  let seed = Packet::Data(value: 7_u64);
  let packets = array_filled::<Packet, 2>(value: seed);
  set packets[1_u64] = Packet::Data(value: 9_u64);
  let index = 1_u64;
  let part = &packets[0_u64..2_u64];
  if index < deref(part).len {
    match deref(part)[index] {
      Data(value: payload) => {
        set index = 0_u64;
        return deref(payload);
      }
      Idle() => {
        return 0_u64;
      }
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// The selected indexed member remains the borrowed-match origin. Replacing
/// that enum invalidates its payload reference even though the index operand
/// itself came from separate scalar storage.
#[test]
fn an_indexed_match_keeps_the_selected_element_as_its_enum_origin() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine() -> result: u64 pure {
  let seed = Packet::Data(value: 7_u64);
  let packets = array_filled::<Packet, 2>(value: seed);
  set packets[1_u64] = Packet::Data(value: 9_u64);
  let index = 1_u64;
  let part = &packets[0_u64..2_u64];
  if index < deref(part).len {
    match deref(part)[index] {
      Data(value: payload) => {
        set packets[1_u64] = Packet::Idle();
        return deref(payload);
      }
      Idle() => {
        return 0_u64;
      }
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { .. })
    });
}

/// Re-establishing an already active `(place, variant)` refinement in a
/// nested match does not end the enclosing arm's fact when the inner arm
/// exits.
#[test]
fn an_outer_payload_reference_survives_a_nested_identical_refinement() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 reads(packet) {
  match deref(packet) {
    Data(value: outer_payload) => {
      let saved = &deref(outer_payload);
      match deref(packet) {
        Data(value: inner_payload) => {
          let observed = deref(inner_payload);
        }
        Idle() => {
        }
      }
      return deref(saved);
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// Replacing an enum ends the old selection. Selecting the new payload
/// establishes a new witness which can survive the inner match.
#[test]
fn a_new_selection_survives_the_replaced_outer_refinement() {
    let source = br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn examine(packet: &Packet) -> result: u64 writes(packet) {
  match deref(packet) {
    Data(value: outer_payload) => {
      let selected = &deref(outer_payload);
      set deref(packet) = Packet::Data(value: 2_u64);
      match deref(packet) {
        Data(value: inner_payload) => {
          set selected = &deref(inner_payload);
        }
        Idle() => {
          return 0_u64;
        }
      }
      return deref(selected);
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

/// A loop body local leaves scope on a nested delivery edge just as it does
/// on the backedge and break edges. The enclosing value initializer cannot
/// publish a reference rooted at that local.
#[test]
fn a_loop_local_reference_cannot_escape_on_a_give_edge() {
    let source = br#"fn examine(flag: Bool) -> result: u64 pure {
  let fallback = 7_u64;
  let selected = if flag {
    loop @deliver {
      let local = 9_u64;
      give &local;
    }
    give &fallback;
  } else {
    give &fallback;
  }
  return deref(selected);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Ref2, |kind| {
        matches!(kind, SemanticIssueKind::InvalidReferenceUse { event, .. }
            if event.contains("scope"))
    });
}

const INDEXED_CALL_HELPER: &str = r#"fn write_two(values: &Array<u8, 4>, first: u64, second: u64) -> result: unit writes(values[first]), writes(values[second]) {
  if first < 4_u64 {
    if second < 4_u64 {
      set deref(values)[first] = 1_u8;
      set deref(values)[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn assert_indexed_call_proof(label: &str, source: &[u8], require_affine: bool) {
    super::with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("indexed call separation {label} must be accepted: {outcome:?}");
        };
        let mut found = false;
        for function in &program.data.functions {
            super::entailment::validate_derivations(&function.entailment);
            found |= function.entailment.obligations.iter().any(|outcome| {
                matches!(
                    outcome.family,
                    super::super::entailment::ObligationFamily::CallSeparation(_)
                ) && outcome.discharged
                    && outcome.derivation.is_some_and(|root| {
                        let Some(super::super::entailment::DerivationNode::IndexSeparation {
                            detail,
                        }) = function.entailment.derivations.nodes.get(root.0 as usize)
                        else {
                            return false;
                        };
                        !require_affine || detail.affine_target.is_some()
                    })
            });
        }
        assert!(
            found,
            "accepted source must retain one exact indexed call proof"
        );
    });
}

#[test]
fn indexed_call_separation_uses_runtime_order_and_disequality_facts() {
    for relation in ["i < j", "j < i", "i != j"] {
        let source = format!(
            "{INDEXED_CALL_HELPER}\nfn ordered(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit writes(values) {{\n  if {relation} {{\n    write_two(values: values, first: i, second: j);\n  }}\n  return unit;\n}}\n"
        );
        assert_indexed_call_proof(relation, source.as_bytes(), false);
    }
    let mixed = format!(
        "{INDEXED_CALL_HELPER}\nfn mixed(values: &Array<u8, 4>, j: u64) -> result: unit writes(values) {{\n  if 0_u64 < j {{\n    write_two(values: values, first: 0_u64, second: j);\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("mixed literal", mixed.as_bytes(), false);
    // [ENT-3.S7] the successor's offset relation `j - i >= 1` is an L0 fact.
    let successor = format!(
        "{INDEXED_CALL_HELPER}\nfn successor(values: &Array<u8, 4>, i: u64, k: u64) -> result: unit writes(values) {{\n  if i < 3_u64 {{\n    if 0_u64 < k {{\n      if k < 3_u64 {{\n        let j = i + k;\n        write_two(values: values, first: i, second: j);\n      }}\n    }}\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("offset successor", successor.as_bytes(), false);
    // The offsets `t - i` in [2, 4] and `j - t` in [-3, -1] leave `j - i` in
    // [-1, 3], so only the affine images `j = i + k - m` and the guard
    // `m < k` separate the two positions.
    let affine = format!(
        "{INDEXED_CALL_HELPER}\nfn affine(values: &Array<u8, 4>, i: u64, k: u64, m: u64) -> result: unit writes(values) {{\n  if i < 3_u64 {{\n    if 0_u64 < m {{\n      if m < k {{\n        if k < 5_u64 {{\n          let t = i + k;\n          let j = t - m;\n          write_two(values: values, first: i, second: j);\n        }}\n      }}\n    }}\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("affine difference", affine.as_bytes(), true);
    for body in [
        "  write_two(values: values, first: i, second: j);\n",
        "  if i == j {\n    write_two(values: values, first: i, second: j);\n  }\n",
    ] {
        let source = format!(
            "{INDEXED_CALL_HELPER}\nfn refused(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit writes(values) {{\n{body}  return unit;\n}}\n"
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Eff5, |kind| {
            matches!(kind, SemanticIssueKind::UndischargedCallSeparation { .. })
        });
    }
}

#[test]
fn indexed_call_separation_obeys_loop_backedges() {
    let first_visit_only = format!(
        "{INDEXED_CALL_HELPER}\nfn looped(values: &Array<u8, 4>, i: u64, j: u64, stop: Bool) -> result: unit writes(values) {{\n  if i < j {{\n  }} else {{\n    return unit;\n  }}\n  loop @again {{\n    write_two(values: values, first: i, second: j);\n    set j = i;\n    if stop {{\n      break @again;\n    }}\n  }}\n  return unit;\n}}\n"
    );
    assert_rule_kind(first_visit_only.as_bytes(), SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallSeparation { .. })
    });
    let every_visit = format!(
        "{INDEXED_CALL_HELPER}\nfn looped(values: &Array<u8, 4>, i: u64, j: u64, stop: Bool) -> result: unit writes(values) {{\n  loop @again {{\n    if i < j {{\n      write_two(values: values, first: i, second: j);\n    }}\n    set j = i;\n    if stop {{\n      break @again;\n    }}\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("every loop visit", every_visit.as_bytes(), false);
}

#[test]
fn indexed_call_separation_requires_every_join_predecessor() {
    let one_arm = format!(
        "{INDEXED_CALL_HELPER}\nfn joined(values: &Array<u8, 4>, i: u64, j: u64, choose: Bool) -> result: unit writes(values) {{\n  if choose {{\n    let branch_marker = i;\n    if i < j {{\n      let observed = i;\n    }} else {{\n      return unit;\n    }}\n  }} else {{\n    let observed = j;\n  }}\n  write_two(values: values, first: i, second: j);\n  return unit;\n}}\n"
    );
    assert_rule_kind(one_arm.as_bytes(), SemanticRule::Eff5, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallSeparation { .. })
    });
    let both_arms = format!(
        "{INDEXED_CALL_HELPER}\nfn joined(values: &Array<u8, 4>, i: u64, j: u64, choose: Bool) -> result: unit writes(values) {{\n  if choose {{\n    let branch_marker = i;\n    if i < j {{\n      let observed = i;\n    }} else {{\n      return unit;\n    }}\n  }} else {{\n    let branch_marker = j;\n    if i < j {{\n      let observed = j;\n    }} else {{\n      return unit;\n    }}\n  }}\n  write_two(values: values, first: i, second: j);\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("both join arms", both_arms.as_bytes(), false);
}

#[test]
fn indexed_call_separation_uses_ordered_nested_candidates() {
    let helper = r#"fn write_nested(values: &Array<Array<u8, 4>, 4>, ao: u64, ai: u64, bo: u64, bi: u64) -> result: unit writes(values[ao][ai]), writes(values[bo][bi]) {
  if ao < 4_u64 {
    if ai < 4_u64 {
      if bo < 4_u64 {
        if bi < 4_u64 {
          set deref(values)[ao][ai] = 1_u8;
          set deref(values)[bo][bi] = 2_u8;
        }
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let outer = format!(
        "{helper}\nfn outer(values: &Array<Array<u8, 4>, 4>, i: u64, j: u64, k: u64) -> result: unit writes(values) {{\n  if i < j {{\n    write_nested(values: values, ao: i, ai: k, bo: j, bi: k);\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("outer nested index", outer.as_bytes(), false);
    let inner = format!(
        "{helper}\nfn inner(values: &Array<Array<u8, 4>, 4>, i: u64, j: u64) -> result: unit writes(values) {{\n  if i < j {{\n    write_nested(values: values, ao: 0_u64, ai: i, bo: 0_u64, bi: j);\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("inner after equal prefix", inner.as_bytes(), false);
    let later = format!(
        "{helper}\nfn later(values: &Array<Array<u8, 4>, 4>, i: u64, j: u64, k: u64, l: u64) -> result: unit writes(values) {{\n  if k < l {{\n    write_nested(values: values, ao: i, ai: k, bo: j, bi: l);\n  }}\n  return unit;\n}}\n"
    );
    assert_indexed_call_proof("later nested candidate", later.as_bytes(), false);
}

/// Conformance owns the source verdicts. This shared fixture additionally
/// checks that each demanded preservation keeps its replayable proof root,
/// including captured indices, joined targets and loop-header dependencies.
#[test]
fn demanded_bystander_preservations_retain_derivations() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/ref2-pos-bystander-preservation.wf");
    super::with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("bystander proof fixture must check completely: {outcome:?}");
        };
        for name in [
            "inspect_requirement",
            "inspect_guard",
            "inspect_capture",
            "inspect_join",
            "inspect_loop",
            "inspect_direct_write",
            "inspect_rhs_capture",
            "inspect_unused",
        ] {
            let function = program
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("missing proof fixture function {name}"));
            super::entailment::validate_derivations(&function.entailment);
            let preservations: Vec<_> = function
                .entailment
                .obligations
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome.family,
                        super::super::entailment::ObligationFamily::ReferencePreservation(_)
                    )
                })
                .collect();
            if name == "inspect_unused" {
                assert!(
                    preservations.is_empty(),
                    "unused references demand no proof"
                );
            } else {
                assert!(!preservations.is_empty(), "{name} retains its preservation");
                if name == "inspect_join" {
                    assert!(
                        preservations.len() >= 2,
                        "both joined targets retain preservation obligations"
                    );
                }
                for preservation in preservations {
                    assert!(preservation.discharged, "{name} has a discharged proof");
                    assert!(preservation.derivation.is_some(), "{name} retains its root");
                }
            }
        }
    });
}
