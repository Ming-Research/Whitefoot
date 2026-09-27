//! The permission judgment P over adjacent statements of one block [PAR-1].
//!
//! Each grant fixture is a shape a real program writes; each denial fixture
//! violates exactly one numbered condition and asserts *that* condition, so a
//! denial arriving for the wrong reason fails the test. Design:
//! `research/investigations/proof-derived-parallelism/DESIGN.md` section 3.
//!
//! v0.60 judges two *adjacent* statements. There is no interposed window and
//! no `PairSide::Between`: a statement written between two calls is judged
//! against each of its two neighbours on its own, and what lets all three
//! overlap together is the run. The fixtures that used to put a statement
//! between two calls therefore assert the adjacency that carries the hazard,
//! or the run the three statements form. Loans, arenas, and the
//! exclusive/shared distinction are gone with [CAP-1], so every interference
//! denial is one `Denial::Footprint` over `own`, `&`, path overlap and the
//! effect row; dataflow is a footprint conflict too, because "a `let`'s
//! defined binding is a write path".
//!
//! Because every adjacency of a block is judged, a fixture's trailing
//! `return` forms a pair of its own with the last call it follows. The
//! helpers below therefore name the two members of the adjacency under test
//! rather than assuming a function has exactly one pair.

use crate::{SemanticOutcome, SemanticRule};

use super::super::entailment::{DerivationNode, RangeSeparationOrdering};
use super::super::permission::{
    ConflictKind, Denial, ExitKind, FootprintHalf, FunctionPermissions, PairSide,
    PermissionMetadata, PermissionPair, PermissionRun, PermissionVerdict,
};
use super::super::places::ResolvedPlace;
use super::{assert_rule_kind, with_semantics};

#[test]
fn a_condition_call_cannot_hide_an_arm_read_of_the_previous_result() {
    let source = br#"fn predicate(value: u64) -> result: Bool pure {
  return value == 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = predicate(value: 1_u64);
  if predicate(value: 0_u64) {
    let observed = first;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    assert!(matches!(
        pair_of(&table, "main", "predicate", "predicate").verdict,
        PermissionVerdict::Denied(Denial::Footprint { .. })
    ));
}

// Scalar state keeps the ordinary adjacency tests independent of a library
// API. The shared-factory tests below exercise the linked declarations
// separately. `writes(p)` states every access at or below `p` [EFF-1], so
// the write row is written once and the read entry of v0.59's row is gone
// with the permission marker on `output`.
const MARKER: &str = "fn write_marker(output: &u64, source: &[u8], start: u64, end: u64) -> result: Result<u64, std::io::IoError> reads(source), writes(output) {\n  let previous = deref(output);\n  let length = deref(source).len;\n  set deref(output) = previous +wrap start;\n  return Ok<u64, std::io::IoError>(value: end);\n}\n\n";

fn permission_of(source: &[u8]) -> PermissionMetadata {
    permission_of_with_discharged_query(source, &[])
}

/// The permission table of one fixture, after asserting that each named
/// function retains a discharged range question with the named ordering.
fn permission_of_with_discharged_query(
    source: &[u8],
    expected: &[(&str, RangeSeparationOrdering)],
) -> PermissionMetadata {
    // The marker follows the fixture, whose alias header leads its record
    // [MOD-4].
    let combined = [
        source.trim_ascii_end(),
        b"\n\n",
        MARKER.trim_end().as_bytes(),
        b"\n",
    ]
    .concat();
    with_semantics(&combined, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("permission fixture must check: {outcome:?}");
        };
        for function in &program.data.functions {
            super::entailment::validate_derivations(&function.entailment);
        }
        for &(expected_function, expected_ordering) in expected {
            let function = program
                .data
                .functions
                .iter()
                .find(|function| function.name == expected_function)
                .unwrap_or_else(|| panic!("no checked function named {expected_function}"));
            assert!(
                function
                    .entailment
                    .permission_separations
                    .iter()
                    .any(|proof| {
                        proof.discharged && proof.derivations.iter().any(|derivation| {
                            matches!(
                                function.entailment.derivations.nodes.get(derivation.0 as usize),
                                Some(DerivationNode::RangeSeparation { detail })
                                    if detail.left == proof.query.left
                                        && detail.right == proof.query.right
                                        && detail.ordering == expected_ordering
                            )
                        })
                    }),
                "{expected_function}: the permitted range pair must retain its exact \
                 {expected_ordering:?} conclusion"
            );
        }
        program.data.permission.clone()
    })
}

fn function_table<'table>(
    table: &'table PermissionMetadata,
    name: &str,
) -> &'table FunctionPermissions {
    table
        .named(name)
        .unwrap_or_else(|| panic!("no permission table for {name}"))
}

/// The only analyzed pair of one function, for a fixture whose block really
/// does hold exactly one.
fn only_pair<'table>(table: &'table PermissionMetadata, name: &str) -> &'table PermissionPair {
    let permissions = function_table(table, name);
    assert_eq!(
        permissions.pairs.len(),
        1,
        "{name} must have exactly one analyzed pair: {:?}",
        permissions.pairs
    );
    &permissions.pairs[0]
}

/// Every analyzed pair of one function whose two members carry the given
/// ledger names. A call member is named by its callee; every other member is
/// named by its statement form, so `("bump", "a set statement")` picks the
/// adjacency a fixture is about without depending on how many other
/// adjacencies the block has.
fn pairs_of<'table>(
    table: &'table PermissionMetadata,
    function: &str,
    first: &str,
    second: &str,
) -> Vec<&'table PermissionPair> {
    function_table(table, function)
        .pairs
        .iter()
        .filter(|pair| pair.first.callee_name == first && pair.second.callee_name == second)
        .collect()
}

/// The one analyzed pair of `function` whose members are named `first` and
/// `second`.
fn pair_of<'table>(
    table: &'table PermissionMetadata,
    function: &str,
    first: &str,
    second: &str,
) -> &'table PermissionPair {
    let found = pairs_of(table, function, first, second);
    let [pair] = found.as_slice() else {
        panic!(
            "{function} must have exactly one ({first}, {second}) pair: {:?}",
            function_table(table, function).pairs
        );
    };
    pair
}

/// The one run of `function` whose members carry exactly these ledger names,
/// in order.
fn run_of<'table>(
    table: &'table PermissionMetadata,
    function: &str,
    names: &[&str],
) -> &'table PermissionRun {
    let permissions = function_table(table, function);
    let found = permissions
        .runs
        .iter()
        .filter(|run| {
            run.sites.len() == names.len()
                && run
                    .sites
                    .iter()
                    .zip(names)
                    .all(|(site, name)| site.callee_name == *name)
        })
        .collect::<Vec<_>>();
    let [run] = found.as_slice() else {
        panic!(
            "{function} must have exactly one run {names:?}: {:?}",
            permissions.runs
        );
    };
    run
}

fn denial(pair: &PermissionPair, condition: u8) -> &Denial {
    let PermissionVerdict::Denied(denial) = &pair.verdict else {
        panic!("expected a denial, got {:?}", pair.verdict);
    };
    assert_eq!(
        denial.condition(),
        condition,
        "denied by the wrong condition: {denial:?}"
    );
    denial
}

/// The scalar cell, the writing call, the reading call, and the by-value call
/// every ordinary adjacency fixture below is built from. A field row keeps
/// the write precise, which is what lets the disjoint-field grant and the
/// overlapping-place denial be told apart by the same relation.
const CELLS: &str = r#"struct Cell {
  value: u64;
}

fn bump(slot: &Cell) -> result: u64 writes(slot.value) {
  set deref(slot).value = 7_u64;
  return 1_u64;
}

fn peek(slot: &Cell) -> result: u64 reads(slot.value) {
  return deref(slot).value;
}

fn take(v: u64) -> result: u64 pure {
  return v;
}

"#;

/// The recursive tree the fold fixtures walk. A `Box` payload makes the enum
/// finite [TYPE-9] and its content is the field `inner`.
const TREE: &str = r#"enum Node {
  Leaf(w: u64);
  Branch(left: Box<Node>, right: Box<Node>, w: u64);
}

"#;

fn cells(body: &str) -> Vec<u8> {
    format!("{CELLS}{body}").into_bytes()
}

fn tree(body: &str) -> Vec<u8> {
    format!("{TREE}{body}").into_bytes()
}

/// A fixture body written as raw bytes, for the `cells`/`tree` prefixes.
fn text(source: &[u8]) -> &str {
    std::str::from_utf8(source).expect("every fixture is UTF-8")
}

// ----------------------------------------------------------------------
// Grants
// ----------------------------------------------------------------------

/// Distinct scalar places admit independent ordinary mutating calls.
#[test]
fn writes_to_independent_scalar_places_are_permitted() {
    let source = br#"fn main(out: u64, err: u64) -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 65_u8);
  let bytes = slots_from_array::<u8, 2>(values: values);
  let window = &bytes[0_u64..2_u64];
  let first = write_marker(output: &out, source: window, start: 0_u64, end: 1_u64);
  let second = write_marker(output: &err, source: window, start: 1_u64, end: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "write_marker", "write_marker");
    assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
}

/// Two calls writing one and the same scalar place. v0.59 refused this as two
/// exclusive loans of one region; [CAP-1] has no loan and no exclusive/shared
/// distinction, so the successor is the ordinary write/write footprint
/// conflict on the place both rows reach.
#[test]
fn two_writes_of_one_scalar_deny_overlap() {
    let source = br#"fn main(out: u64) -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 65_u8);
  let bytes = slots_from_array::<u8, 2>(values: values);
  let window = &bytes[0_u64..2_u64];
  let first = write_marker(output: &out, source: window, start: 0_u64, end: 1_u64);
  let second = write_marker(output: &out, source: window, start: 1_u64, end: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "write_marker", "write_marker");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "write"));
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

#[test]
fn two_opens_through_one_factory_are_ordinary_conflicting_calls() {
    let table = permission_of(include_bytes!(
        "../../../../tests/conformance/cases/accept-sysfile-two-permits-shared-directory.wf"
    ));
    let pair = only_pair(&table, "open_two");
    assert_eq!(pair.first.callee_name, "open_directory_source");
    assert_eq!(pair.second.callee_name, "open_directory_source");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        kind.halves(),
        ("write", "write"),
        "both calls write the one factory the writer handed each of them"
    );
}

#[test]
fn independent_descendant_cursors_do_not_gain_sibling_field_separation() {
    let source = br#"struct Node {
  left: u64;
  right: u64;
  next: Option<Box<Node>>;
}

fn paint_left(node: &Node) -> result: unit writes(node.left) {
  set deref(node).left = 1_u64;
  return unit;
}

fn paint_right(node: &Node) -> result: unit writes(node.right) {
  set deref(node).right = 2_u64;
  return unit;
}

fn inspect(root: &Node) -> result: unit writes(root) {
  let first = root;
  let second = root;
  for (i in 0_u64..2_u64) {
    match deref(first).next {
      Some(value: child) => {
        set first = &deref(child).inner;
      }
      None() => {
      }
    }
    match deref(second).next {
      Some(value: child) => {
        set second = &deref(child).inner;
      }
      None() => {
      }
    }
  }
  let painted_left = paint_left(node: first);
  let painted_right = paint_right(node: second);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "inspect", "paint_left", "paint_right");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("independent covers must retain their footprint conflict");
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

/// Rebinding two incoming holders cannot make their entry-storage anchors
/// unresolved. A third destination remains independent, while either member
/// of the exchanged origin set still prevents parallel writes.
#[test]
fn rebound_parameter_summaries_distinguish_independent_and_overlapping_writes() {
    for (destination, effects, independent) in [
        ("other", "writes(second), writes(other)", true),
        ("first", "writes(second)", false),
        ("saved", "writes(first), writes(second)", false),
    ] {
        let source = format!(
            "fn write_first(value: &u64) -> result: unit writes(value) {{
  set deref(value) = 1_u64;
  return unit;
}}

fn write_second(value: &u64) -> result: unit writes(value) {{
  set deref(value) = 2_u64;
  return unit;
}}

fn exchange(first: &u64, second: &u64, other: &u64) -> result: unit {effects} {{
  let saved = first;
  set first = &deref(second);
  set second = &deref(saved);
  let left = write_first(value: first);
  let right = write_second(value: {destination});
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        let table = permission_of(source.as_bytes());
        let pair = pair_of(&table, "exchange", "write_first", "write_second");
        if independent {
            assert!(pair.verdict.is_eligible(), "{pair:?}");
        } else {
            let Denial::Footprint { kind, .. } = denial(pair, 1) else {
                panic!("known overlapping origins must retain their footprint conflict");
            };
            assert_eq!(kind.halves(), ("write", "write"));
        }
    }
}

/// Disjoint destinations do not save two calls that also write one shared
/// cursor: the substituted rows meet on that one path whatever else they
/// reach.
#[test]
fn writes_through_one_shared_cursor_conflict_despite_disjoint_destinations() {
    let source = br#"fn stamp(cursor: &Cell, destination: &Cell) -> result: u64 writes(cursor.value), writes(destination.value) {
  set deref(cursor).value = deref(cursor).value +wrap 1_u64;
  set deref(destination).value = 5_u64;
  return 1_u64;
}

fn probe(cursor: &Cell, left: &Cell, right: &Cell) -> result: u64 writes(cursor.value), writes(left.value), writes(right.value) {
  let first = stamp(cursor: cursor, destination: left);
  let second = stamp(cursor: cursor, destination: right);
  return 0_u64;
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "probe", "stamp", "stamp");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

#[test]
fn direct_prelude_calls_form_an_eligible_pair() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let first = std::process::exit_status(code: 0_u8);
  let second = std::process::exit_status(code: 1_u8);
  return move second;
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "exit_status", "exit_status");
    assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
}

/// The two-child tree fold: each recursive sibling reaches storage only
/// through its own payload step, and [OWN-7] separates two payload steps of
/// one variant that select different fields. This is the shape a parallel
/// fold is written in.
#[test]
fn two_child_sibling_calls_are_permitted_and_eligible() {
    let source = br#"fn fold(node: &Node) -> result: u64 writes(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      return deref(leaf);
    }
    Branch(left: l, right: r, w: slot) => {
      let a = fold(node: &deref(l).inner);
      let b = fold(node: &deref(r).inner);
      let total = imax(a, b);
      set deref(slot) = total;
      return total;
    }
  }
}
"#;
    let table = permission_of(&tree(text(source)));
    let pair = pair_of(&table, "fold", "fold", "fold");
    assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
    let run = run_of(&table, "fold", &["fold", "fold"]);
    assert_eq!(run.sites.len(), 2);
}

/// Two calls whose rows name two different fields of one object are
/// permitted.
///
/// v0.59 refused this pair: both actuals were `&uniq pair`, and a
/// whole-object exclusive loan was coarser than the rows it carried. [CAP-1]
/// leaves only `own`, `&`, path overlap and the effect row, so the rows now
/// decide it and the two disjoint field paths overlap nothing.
#[test]
fn disjoint_effect_fields_of_one_object_are_permitted() {
    let source = br#"struct Pair {
  left: u64;
  right: u64;
}

fn set_left(pair: &Pair) -> result: unit writes(pair.left) {
  set deref(pair).left = 1_u64;
  return unit;
}

fn set_right(pair: &Pair) -> result: unit writes(pair.right) {
  set deref(pair).right = 2_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 0_u64);
  let first = set_left(pair: &pair);
  let second = set_right(pair: &pair);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "set_left", "set_right");
    assert_eq!(
        pair.verdict,
        PermissionVerdict::PermittedEligible,
        "the two rows name two fields, and [OWN-7] separates two field steps"
    );
}

/// [GRAM-4] makes an expression statement one call whose result is discarded,
/// and [PAR-1] gives it no footprint of its own: it is the call's substituted
/// row [EFF-5], its operand reads, and its by-value consumptions, with no
/// binding write and no path for a discarded result's release [STOR-8]. Every
/// adjacency therefore receives the verdict the let-bound spelling receives,
/// whichever member is written which way: disjoint rows are permitted, one
/// shared row is the ordinary write/write conflict, and a releasing
/// expression statement (the discarded `Box`) is judged by its row alone.
///
/// Until this fixture, both expression-statement forms were refused as
/// unclassified, ending every run through them by their spelling.
#[test]
fn an_expression_statement_call_is_judged_as_its_let_bound_call() {
    const PREFIX: &str = r#"struct Pair {
  left: u64;
  right: u64;
}

fn set_left(pair: &Pair) -> result: unit writes(pair.left) {
  set deref(pair).left = 1_u64;
  return unit;
}

fn set_right(pair: &Pair) -> result: unit writes(pair.right) {
  set deref(pair).right = 2_u64;
  return unit;
}

fn fresh_left(pair: &Pair) -> result: Box<Array<u64>> writes(pair.left) {
  set deref(pair).left = 3_u64;
  let made = box_array_filled::<u64>(count: 2_u64, value: 0_u64);
  return move made;
}

"#;
    for (first_callee, second_callee, expected) in [
        ("set_left", "set_right", None),
        ("set_left", "set_left", Some(1)),
        ("fresh_left", "set_right", None),
        ("fresh_left", "set_left", Some(1)),
    ] {
        for (first_form, second_form) in [
            ("let first = ", "let second = "),
            ("", ""),
            ("let first = ", ""),
            ("", "let second = "),
        ] {
            let source = format!(
                "{PREFIX}fn main() -> status: std::process::ExitStatus pure {{
  let pair = Pair(left: 0_u64, right: 0_u64);
  {first_form}{first_callee}(pair: &pair);
  {second_form}{second_callee}(pair: &pair);
  return std::process::exit_status(code: 0_u8);
}}
"
            );
            let table = permission_of(source.as_bytes());
            let pair = pair_of(&table, "main", first_callee, second_callee);
            match expected {
                None => assert_eq!(
                    pair.verdict,
                    PermissionVerdict::PermittedEligible,
                    "the two rows name two fields:\n{source}"
                ),
                Some(condition) => {
                    let Denial::Footprint { kind, .. } = denial(pair, condition) else {
                        panic!("expected a footprint conflict:\n{source}");
                    };
                    assert_eq!(kind.halves(), ("write", "write"), "{source}");
                }
            }
            if expected.is_none() {
                run_of(&table, "main", &[first_callee, second_callee]);
            }
        }
    }
}

/// "The paths of both statements are interpreted in the state before the
/// first statement; the first statement's `ensures` maps the second's indices
/// into that state, so an index that is live only after an append is not
/// distinct from the append slot" [PAR-1, WIN-2].
///
/// `place_back` writes `r.next` and `r.len`, and the `r[n]` the next statement
/// reads is live only after it: `n` is the append slot. [WIN-2]'s fixed row
/// separating a live index from `r.next` holds within one state only, and this
/// judgment performs no `ensures` mapping, so a window whose length an earlier
/// statement writes has no part-relative separation. A read before the append
/// is live in the first statement's own state and stays independent of it.
///
/// Before expression statements were judged, every window operation (written
/// as one) ended its run, which hid this for them; the let-bound spelling was
/// permitted and a hand-out published the wrong value.
#[test]
fn an_index_live_only_after_an_append_is_not_distinct_from_the_append_slot() {
    for bind in ["", "let appended = "] {
        let source = format!(
            "fn peek(v: &u64) -> result: u64 reads(v) {{
  return deref(v);
}}

fn after_append() -> result: u64 pure {{
  let r = slots_new::<u64, 4>();
  let n = r.len;
  {bind}place_back(window: &r, value: 7_u64);
  let seen = peek(v: &r[n]);
  return seen;
}}

fn before_append() -> result: u64 pure {{
  let r = slots_new::<u64, 4>();
  place_back(window: &r, value: 5_u64);
  let seen = peek(v: &r[0_u64]);
  {bind}place_back(window: &r, value: 6_u64);
  return seen;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let appended = after_append();
  let kept = before_append();
  return std::process::exit_status(code: 0_u8);
}}
"
        );
        let table = permission_of(source.as_bytes());
        for (function, first, second) in [
            ("after_append", "place_back", "peek"),
            ("before_append", "place_back", "peek"),
        ] {
            let pair = pair_of(&table, function, first, second);
            let Denial::Footprint { kind, .. } = denial(pair, 1) else {
                panic!("the appended slot is the one read:\n{source}");
            };
            assert_eq!(kind.halves(), ("write", "read"), "{source}");
        }
        assert_eq!(
            pair_of(&table, "before_append", "peek", "place_back").verdict,
            PermissionVerdict::PermittedEligible,
            "a slot live before the append is not the append slot:\n{source}"
        );
    }
}

/// Read-only sibling recursion. Nothing is written at all, so the
/// disjointness clause is satisfied by an empty write footprint rather than
/// by separation.
#[test]
fn read_only_sibling_recursion_is_permitted_and_eligible() {
    let source = br#"fn depth(node: &Node) -> result: u64 reads(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      return 1_u64;
    }
    Branch(left: l, right: r, w: slot) => {
      let a = depth(node: &deref(l).inner);
      let b = depth(node: &deref(r).inner);
      return imax(a, b);
    }
  }
}
"#;
    let table = permission_of(&tree(text(source)));
    let pair = pair_of(&table, "depth", "depth", "depth");
    assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
}

/// Three `reads`-only calls over one and the same run. Read/read overlap is
/// admitted, so all three pairs hold and the adjacent statements form one
/// run — the bisection shape, where every lane views the same immutable
/// input.
#[test]
fn reads_only_siblings_over_one_place_form_one_eligible_chain() {
    let source = br#"fn width(data: &Slots<u64, 8>) -> result: u64 reads(data) {
  return deref(data).len;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 8>(value: 1_u64);
  let buf = slots_from_array::<u64, 8>(values: values);
  let lo = width(data: &buf);
  let mid = width(data: &buf);
  let hi = width(data: &buf);
  let part = imax(mid, hi);
  let total = imax(lo, part);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let judged = pairs_of(&table, "main", "width", "width");
    assert_eq!(judged.len(), 2, "lo/mid and mid/hi: {judged:?}");
    for pair in judged {
        assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
    }
    let run = run_of(&table, "main", &["width", "width", "width"]);
    assert_eq!(run.sites.len(), 3);
}

/// A run is not implied by its adjacent pairs. Here s1 and s2 are disjoint
/// and s2 and s3 are disjoint, but s1 and s3 write the same cell, so the run
/// stops at two members even though both adjacent pairs hold.
#[test]
fn a_run_stops_where_a_nonadjacent_pair_conflicts() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let first = Cell(value: 1_u64);
  let second = Cell(value: 2_u64);
  let a = bump(slot: &first);
  let b = bump(slot: &second);
  let c = bump(slot: &first);
  let part = imax(b, c);
  let total = imax(a, part);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let judged = pairs_of(&table, "main", "bump", "bump");
    assert_eq!(judged.len(), 2, "a/b and b/c: {judged:?}");
    for pair in judged {
        assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
    }
    let run = run_of(&table, "main", &["bump", "bump"]);
    assert_eq!(
        run.sites.len(),
        2,
        "the running union carries s1's write into the test s3 fails"
    );
}

// ----------------------------------------------------------------------
// Denials, each by its own condition
// ----------------------------------------------------------------------

/// The second statement reads the binding the first defines.
///
/// v0.59 called this a dataflow denial of its own. v0.60 states that "a
/// `let`'s defined binding is a write path", so the link is an ordinary
/// footprint conflict between s1's write of that binding and s2's operand
/// read of it, and `Denial::Dataflow` is gone.
#[test]
fn a_dataflow_link_between_siblings_is_a_footprint_conflict() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let left = Cell(value: 1_u64);
  let a = bump(slot: &left);
  let b = take(v: a);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "bump", "take");
    let Denial::Footprint {
        kind, left, sides, ..
    } = denial(pair, 1)
    else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
    assert_eq!(
        left.place,
        ResolvedPlace::binding(pair.first.binding.expect("s1 defines a binding")),
        "the cited write is s1's own defined binding"
    );
    assert_eq!(
        *sides,
        (PairSide::First, PairSide::Second),
        "the link runs from s1 to s2, with nothing between them"
    );
}

/// Two reference actuals resolve to one and the same place, so the two write
/// footprints overlap under [OWN-7].
#[test]
fn overlapping_reference_arguments_are_denied_by_their_footprints() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let lo = bump(slot: &cell);
  let hi = bump(slot: &cell);
  let total = imax(lo, hi);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "bump", "bump");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::Write
        },
        "there is one reference kind, so the two rows are the whole story"
    );
    assert_eq!(
        *sides,
        (PairSide::First, PairSide::Second),
        "the conflict is between the two members, not with anything between them"
    );
}

/// The caller-side half. `take`'s row is `pure` and reaches no caller storage
/// at all, but its own operand reads the cell `bump` writes, and the overlap
/// moves exactly that read across `bump`'s call.
#[test]
fn an_operand_read_of_written_storage_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let a = bump(slot: &cell);
  let b = take(v: cell.value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "bump", "take");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The same hazard through a subscript rather than a whole binding: the
/// element read is rooted at the storage the later call writes through.
#[test]
fn an_operand_element_read_of_a_written_run_is_denied() {
    let source = br#"fn fill(dst: &Slots<u64, 4>, mark: u64) -> result: u64 writes(dst) contract {
  requires 1_u64 <= deref(dst).len;
} {
  set deref(dst)[0_u64] = mark;
  return mark;
}

fn take(v: u64) -> result: u64 pure {
  return v;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  let buf = slots_from_array::<u64, 4>(values: values);
  let b = take(v: buf[0_u64]);
  let a = fill(dst: &buf, mark: 9_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "take", "fill");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::OperandRead,
            later: FootprintHalf::Write
        }
    );
}

/// The caller-side half in its other direction: s1's own operand reads the
/// cell s2 writes through. Which member takes a lane is the implementation's
/// choice, so permission may not depend on it and both directions are judged.
#[test]
fn an_operand_read_by_the_first_call_of_storage_the_second_writes_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let a = take(v: cell.value);
  let b = bump(slot: &cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "take", "bump");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::OperandRead,
            later: FootprintHalf::Write
        }
    );
}

/// The disjointness clause in its other direction: s1 only reads, s2 writes
/// the same place. The judgment's first conflict loop never sees this pair,
/// so the second one has to.
#[test]
fn a_write_by_the_second_call_over_a_read_by_the_first_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let a = peek(slot: &cell);
  let b = bump(slot: &cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "peek", "bump");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Read,
            later: FootprintHalf::Write
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// A propagating first statement carries an `Err` edge to the function-return
/// sink [ERR-3], so the statement after it need not execute at all.
///
/// v0.59 removed such a statement from the candidate enumeration, so the pair
/// did not exist and nothing was reported. v0.60 judges every adjacency, so
/// the pair exists and is refused with the edge that costs it.
#[test]
fn a_propagating_first_statement_is_denied_by_its_exit() {
    let source = br#"fn narrow(v: u32) -> result: Result<u8, NarrowError> pure {
  return cvt.checked::<u32, u8>(v);
}

fn probe(v: u32, slot: &Cell) -> result: Result<unit, NarrowError> writes(slot.value) {
  let narrowed = propagate narrow(v: v);
  let stamped = bump(slot: slot);
  return Ok<unit, NarrowError>(value: unit);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "probe", "a propagate statement", "bump");
    let Denial::SkippingExit { side, kind } = denial(pair, 2) else {
        panic!("expected a skipping-exit denial, got {:?}", pair.verdict);
    };
    assert_eq!(*kind, ExitKind::PropagateError);
    assert_eq!(*side, PairSide::First);
}

/// The same exclusion for a propagating second member. v0.59's exit condition
/// read only the first member's edge, so an ordinary call followed by a
/// `propagate` once reached `PermittedEligible` without checking its own
/// exit; the judgment refuses both sides.
#[test]
fn a_propagating_second_statement_is_denied_by_its_exit() {
    let source = br#"fn narrow(v: u32) -> result: Result<u8, NarrowError> pure {
  return cvt.checked::<u32, u8>(v);
}

fn probe(v: u32, slot: &Cell) -> result: Result<unit, NarrowError> writes(slot.value) {
  let stamped = bump(slot: slot);
  let narrowed = propagate narrow(v: v);
  return Ok<unit, NarrowError>(value: unit);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "probe", "bump", "a propagate statement");
    let Denial::SkippingExit { side, kind } = denial(pair, 2) else {
        panic!("expected a skipping-exit denial, got {:?}", pair.verdict);
    };
    assert_eq!(*kind, ExitKind::PropagateError);
    assert_eq!(*side, PairSide::Second);
}

// ----------------------------------------------------------------------
// Proof-complete call closures
// ----------------------------------------------------------------------

/// The first recursive closure contains an unproved helper subscript and is
/// rejected before permission. The second source keeps the same recursive
/// sibling pair and makes only that helper total with a dominating branch.
#[test]
fn a_recursive_closure_requires_source_proof_and_then_is_eligible() {
    let unproved = r#"fn scaled(values: Array<u8, 8>, index: u64) -> result: u8 pure {
  return values[index];
}

fn bubble(node: &Node) -> result: u64 writes(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      let w = deref(leaf);
      let values = array_filled::<u8, 8>(value: 0_u8);
      let touched = scaled(values: values, index: w);
      return w;
    }
    Branch(left: l, right: r, w: slot) => {
      let a = bubble(node: &deref(l).inner);
      let b = bubble(node: &deref(r).inner);
      let total = a +wrap b;
      set deref(slot) = total;
      return total;
    }
  }
}
"#;
    with_semantics(&tree(unproved), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("the unproved closure must reject before permission: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Op4);
    });

    let proved = r#"fn scaled(values: Array<u8, 8>, index: u64) -> result: u8 pure {
  let size = values.len;
  if index < size {
    return values[index];
  }
  return 0_u8;
}

fn bubble(node: &Node) -> result: u64 writes(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      let w = deref(leaf);
      let values = array_filled::<u8, 8>(value: 0_u8);
      let touched = scaled(values: values, index: w);
      return w;
    }
    Branch(left: l, right: r, w: slot) => {
      let a = bubble(node: &deref(l).inner);
      let b = bubble(node: &deref(r).inner);
      let total = a +wrap b;
      set deref(slot) = total;
      return total;
    }
  }
}
"#;
    let table = permission_of(&tree(proved));
    let pair = pair_of(&table, "bubble", "bubble", "bubble");
    assert_eq!(
        pair.verdict,
        PermissionVerdict::PermittedEligible,
        "the proof-complete recursive closure must remain eligible"
    );
    let run = run_of(&table, "bubble", &["bubble", "bubble"]);
    assert_eq!(
        run.sites.len(),
        2,
        "an eligible pair forms its run like any other"
    );
    assert!(
        proved.contains("if index < size"),
        "the fixture must keep the dominating source proof in its closure"
    );
}

// ----------------------------------------------------------------------
// Statements written between two calls
// ----------------------------------------------------------------------

/// The F3 shape. One pure builtin between the two recursive calls, reading a
/// local the calls do not reach and defining a binding neither of them reads.
///
/// v0.59 judged the two calls across an interposed window. v0.60 has no
/// window: the builtin forms an ordinary adjacency with each neighbour, both
/// hold, and the composition clause — "any run of adjacent statements that
/// pairwise may overlap may all overlap" — is what puts all three in one run.
#[test]
fn a_pure_builtin_between_two_calls_keeps_one_run() {
    let source = r#"fn fold(node: &Node, seed: u64) -> result: u64 writes(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      return deref(leaf);
    }
    Branch(left: l, right: r, w: slot) => {
      let a = fold(node: &deref(l).inner, seed: seed);
      let gap = seed +wrap 1_u64;
      let b = fold(node: &deref(r).inner, seed: seed);
      let kids = imax(a, b);
      let total = imax(kids, gap);
      set deref(slot) = total;
      return total;
    }
  }
}
"#;
    let table = permission_of(&tree(source));
    let pairs = &function_table(&table, "fold").pairs;
    assert_eq!(
        pairs.len(),
        3,
        "the arm's three reported adjacencies, in source order: {pairs:?}"
    );
    assert_eq!(pairs[0].verdict, PermissionVerdict::PermittedEligible);
    assert_eq!(pairs[1].verdict, PermissionVerdict::PermittedEligible);
    let run = run_of(&table, "fold", &["fold", "a let statement", "fold"]);
    assert_eq!(
        run.sites.len(),
        3,
        "the two calls and the builtin form one run"
    );
}

/// A local invariant between two calls is a compile-time statement, not a
/// runtime member. It contributes no footprint and no exit edge, so it joins
/// the run without changing it.
#[test]
fn a_local_invariant_between_two_calls_keeps_one_run() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let left = Cell(value: 1_u64);
  let right = Cell(value: 2_u64);
  let a = peek(slot: &left);
  invariant two_steps: 0_u64 <= 2_u64;
  let b = peek(slot: &right);
  let total = a +wrap b;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    assert_eq!(
        pair_of(&table, "main", "peek", "a proof statement").verdict,
        PermissionVerdict::PermittedEligible
    );
    assert_eq!(
        pair_of(&table, "main", "a proof statement", "peek").verdict,
        PermissionVerdict::PermittedEligible
    );
    let run = run_of(&table, "main", &["peek", "a proof statement", "peek"]);
    assert_eq!(run.sites.len(), 3);
}

/// A `set` writes the storage the next call's callee reads through its
/// actual. Under the schedule that hands that call to a lane, the read races
/// the store and takes the pre-`set` value where source order requires the
/// post-`set` one.
///
/// v0.59 reported this as the interposed side of a wider window; v0.60
/// reports the adjacency that carries it.
#[test]
fn a_write_into_the_next_callees_read_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let other = Cell(value: 2_u64);
  let a = peek(slot: &other);
  set cell.value = 5_u64;
  let b = peek(slot: &cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "a set statement", "peek");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::Read
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// A `set` writes the storage the previous call's callee writes through its
/// actual. Under the schedule that hands that call out this is a live
/// store/store race between the lane and the calling thread.
#[test]
fn a_write_over_the_previous_callees_write_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let other = Cell(value: 2_u64);
  let a = bump(slot: &cell);
  set cell.value = 5_u64;
  let b = peek(slot: &other);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "bump", "a set statement");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::Write
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The operand half of the same hazard. `take`'s row is `pure` and reaches no
/// caller storage at all, but the schedule that hands it to a lane evaluates
/// its operands at the hand-out point, above the `set`, so it reads 1 where
/// source order gives 15. No callee row is involved on either side.
#[test]
fn a_write_under_the_next_calls_operand_read_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let other = Cell(value: 2_u64);
  let a = peek(slot: &other);
  set cell.value = 15_u64;
  let b = take(v: cell.value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "a set statement", "take");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The mirror of the fixture above, and an obligation v0.59's window rule
/// deliberately did **not** carry: a write placed after a call's operand read
/// was permitted there, because neither realizable schedule of an interposed
/// window let the store move above the read.
///
/// v0.60 states the operand half symmetrically — "each statement's write
/// paths must also be disjoint from the places the other statement's argument
/// expressions read" — so the same program is denied, and the asymmetry this
/// fixture used to pin is gone.
#[test]
fn a_write_over_the_previous_calls_operand_read_is_denied() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let other = Cell(value: 2_u64);
  let a = take(v: cell.value);
  set cell.value = 15_u64;
  let b = peek(slot: &other);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "take", "a set statement");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::OperandRead,
            later: FootprintHalf::Write
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

#[test]
fn an_owned_box_path_take_has_a_complete_root_footprint_and_conflicts_with_an_alias() {
    let source = br#"nocopy struct Payload {
  value: u8;
}

nocopy struct Holder {
  cell: Box<Payload>;
}

fn observe(holder: &Holder) -> result: u8 reads(holder) {
  return deref(holder).cell.inner.value;
}

fn main() -> status: std::process::ExitStatus pure {
  let payload = Payload(value: 7_u8);
  let cell = box_new::<Payload>(value: move payload);
  let holder = Holder(cell: move cell);
  let seen = observe(holder: &holder);
  let taken = move holder.cell.inner;
  return std::process::exit_status(code: taken.value);
}
"#;
    let table = permission_of(source);
    let pair = pair_of(&table, "main", "observe", "a let statement");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!(
            "the known owner root must conflict, not fail unresolved: {:?}",
            pair.verdict
        );
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Read,
            later: FootprintHalf::Write,
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The statement after a call reads the binding that call defines. Under the
/// schedule that hands the call out that value does not exist until the join.
#[test]
fn a_read_of_the_previous_calls_result_is_a_footprint_conflict() {
    let source = r#"fn fold(node: &Node, seed: u64) -> result: u64 writes(node) {
  match deref(node) {
    Leaf(w: leaf) => {
      return deref(leaf);
    }
    Branch(left: l, right: r, w: slot) => {
      let a = fold(node: &deref(l).inner, seed: seed);
      let gap = a +wrap 1_u64;
      let b = fold(node: &deref(r).inner, seed: seed);
      let kids = imax(a, b);
      let total = imax(kids, gap);
      set deref(slot) = total;
      return total;
    }
  }
}
"#;
    let table = permission_of(&tree(source));
    let pairs = &function_table(&table, "fold").pairs;
    let pair = &pairs[0];
    let Denial::Footprint { kind, left, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
    assert_eq!(
        left.place,
        ResolvedPlace::binding(pair.first.binding.expect("s1 defines a binding")),
        "s1's own result is the link"
    );
}

/// A call reading a binding the statement before it defines. Under the
/// schedule that hands that call to a lane its operands are evaluated before
/// the defining statement runs, so the value it would read does not exist
/// yet.
#[test]
fn a_call_reading_the_previous_statements_binding_is_a_footprint_conflict() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let other = Cell(value: 2_u64);
  let a = peek(slot: &other);
  let seed = 7_u64;
  let b = take(v: seed);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "a let statement", "take");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The exit clause beside an ordinary call. The `propagate` has an `Err` edge
/// to the function-return sink [ERR-3], so on that edge the function returns
/// while the previous statement's lane is still executing against a frame the
/// return is about to destroy. That is a use-after-return, not a value
/// difference.
#[test]
fn a_propagate_beside_a_call_is_denied_by_its_exit() {
    let source = br#"fn probe(outcome: Result<u8, NarrowError>, a: &Cell, b: &Cell) -> result: Result<unit, NarrowError> reads(b.value), writes(a.value) {
  let seen = peek(slot: b);
  let narrowed = propagate outcome;
  let stamped = bump(slot: a);
  return Ok<unit, NarrowError>(value: unit);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "probe", "peek", "a propagate statement");
    let Denial::SkippingExit { side, kind } = denial(pair, 2) else {
        panic!("expected a skipping-exit denial, got {:?}", pair.verdict);
    };
    assert_eq!(*kind, ExitKind::PropagateError);
    assert_eq!(*side, PairSide::Second);
}

#[test]
fn a_proved_subscript_between_two_calls_creates_no_exit() {
    let source = br#"fn probe(values: Array<u8, 8>, cell: &Cell, other: &Cell) -> result: u64 reads(cell.value), reads(other.value) {
  let a = peek(slot: other);
  let picked = values[3_u64];
  let b = peek(slot: cell);
  return imax(a, b);
}
"#;
    let table = permission_of(&cells(text(source)));
    assert_eq!(
        pair_of(&table, "probe", "peek", "a let statement").verdict,
        PermissionVerdict::PermittedEligible
    );
    assert_eq!(
        pair_of(&table, "probe", "a let statement", "peek").verdict,
        PermissionVerdict::PermittedEligible
    );
    let run = run_of(&table, "probe", &["peek", "a let statement", "peek"]);
    assert_eq!(
        run.sites.len(),
        3,
        "the proof-complete operation must retain the eligible run"
    );
}

/// A `match` statement carries its own control flow and its own arm drops,
/// and the checked model gives it no statement node of its own, so it is
/// never a member of an adjacency and it ends the run it interrupts.
///
/// v0.59 reported this as an interposed-form refusal of a wider window. v0.60
/// has no window, and the honest report is that the two calls the `match`
/// separates are not adjacent and never share a run.
#[test]
fn a_match_statement_is_no_member_of_any_adjacency() {
    let source = br#"enum Choice {
  Low(w: u64);
  High(w: u64);
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let other = Cell(value: 2_u64);
  let which = Choice::Low(w: 3_u64);
  let a = peek(slot: &other);
  match which {
    Low(w: lw) => {
      let seen = lw;
    }
    High(w: hw) => {
      let seen = hw;
    }
  }
  let b = peek(slot: &cell);
  let total = imax(a, b);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    assert!(
        pairs_of(&table, "main", "peek", "peek").is_empty(),
        "the two calls the match separates are not adjacent"
    );
    assert!(
        !function_table(&table, "main").runs.iter().any(|run| {
            run.sites
                .iter()
                .filter(|site| site.callee_name == "peek")
                .count()
                > 1
        }),
        "an unclassified statement ends the run before it"
    );
}

/// A form the judgment does not account for is refused **with a report**. A
/// counted `for` carries a node of its own, so the adjacency exists and the
/// refusal names the form rather than passing over it silently. Fail-closed
/// and silent are different defects, and only the second is fixed by denying.
#[test]
fn a_counted_loop_beside_a_call_is_an_unclassified_form() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let a = peek(slot: &cell);
  for @scan (i in 0_u64..4_u64) {
    let seen = i;
  }
  let b = peek(slot: &cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "peek", "a for loop");
    let Denial::UnclassifiedForm { side, form } = denial(pair, 1) else {
        panic!(
            "a loop beside a call must be reported, not silently unjudged: {:?}",
            pair.verdict
        );
    };
    assert_eq!(*side, PairSide::Second);
    assert_eq!(*form, "a for loop");

    let mirrored = pair_of(&table, "main", "a for loop", "peek");
    let Denial::UnclassifiedForm { side, .. } = denial(mirrored, 1) else {
        panic!("the other side is refused for the same reason");
    };
    assert_eq!(*side, PairSide::First);
}

// ----------------------------------------------------------------------
// What replaced the loans half [CAP-1]
// ----------------------------------------------------------------------

/// The pointed case of v0.59's loans half: both callees declare `reads` only,
/// and each actual was a `&uniq` borrow of one cell, which v0.59 refused
/// because two usable exclusive loans never coexist on one place.
///
/// v0.60 has one reference kind and no permission marker; whether a callee
/// may write through a reference is stated by its row alone [REF-1, EFF-1].
/// Two read rows over one place are read/read overlap, which [PAR-1] admits.
#[test]
fn two_references_to_one_place_with_read_only_rows_are_permitted() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 21_u64);
  let a = peek(slot: &cell);
  let b = peek(slot: &cell);
  let both = a +wrap b;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    let pair = pair_of(&table, "main", "peek", "peek");
    assert_eq!(pair.verdict, PermissionVerdict::PermittedEligible);
}

/// v0.59 refused a shared borrow beside a consuming `move` by its loan, with
/// the callee's row `pure` and contributing nothing.
///
/// v0.60 has no loan: a reference names a path and reads no content, and what
/// a callee reaches through one is exactly its declared row. So the `pure`
/// callee reaches nothing and the adjacency holds, while the same call with a
/// `reads` row meets the consumption's write of that place and denies. Both
/// halves are asserted here so the boundary is pinned in one fixture.
#[test]
fn a_pure_row_reaches_nothing_through_a_reference_and_a_reading_row_denies() {
    let source = br#"fn ignore_node(node: &Box<u64>) -> result: u64 pure {
  return 7_u64;
}

fn read_node(node: &Box<u64>) -> result: u64 reads(node) {
  return deref(node).inner;
}

fn eat_node(node: Box<u64>) -> result: u64 pure {
  return 9_u64;
}

fn quiet(node: Box<u64>) -> result: u64 pure {
  let a = ignore_node(node: &node);
  let b = eat_node(node: move node);
  return a +wrap b;
}

fn loud(node: Box<u64>) -> result: u64 pure {
  let a = read_node(node: &node);
  let b = eat_node(node: move node);
  return a +wrap b;
}
"#;
    let table = permission_of(source);
    assert_eq!(
        pair_of(&table, "quiet", "ignore_node", "eat_node").verdict,
        PermissionVerdict::PermittedEligible,
        "a pure row reaches nothing through the reference it was handed"
    );
    let pair = pair_of(&table, "loud", "read_node", "eat_node");
    let Denial::Footprint { kind, sides, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Read,
            later: FootprintHalf::Write
        },
        "a by-value consumption counts as a write of the argument's place"
    );
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// A `let`-bound reference is an ordinary member, and reading through it is a
/// read of the path it names.
///
/// v0.59 refused any statement that formed a borrow, because the checked tree
/// erased the borrow's shared-or-uniq mode and an unloaned borrow would widen
/// permission. v0.60 has no mode to erase: forming the reference reads no
/// content and is permitted beside a write of the same storage, while the
/// later `deref` resolves to that storage and conflicts with it.
#[test]
fn a_read_through_a_reference_is_a_read_of_the_path_it_names() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_u64);
  let g = &cell;
  let a = bump(slot: &cell);
  let seen = deref(g).value;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(&cells(text(source)));
    assert_eq!(
        pair_of(&table, "main", "a let statement", "bump").verdict,
        PermissionVerdict::PermittedEligible,
        "forming a reference names a path and reads no content"
    );
    let pair = pair_of(&table, "main", "bump", "a let statement");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(
        *kind,
        ConflictKind {
            earlier: FootprintHalf::Write,
            later: FootprintHalf::OperandRead
        }
    );
}

// Retired with v0.59's loans half: `an_unresolvable_loan_actual_denies_rather_
// than_dropping_the_loan` handed a returned `Slice<'r, u8>` to two calls, and
// a range reference [REF-4] is never a result, so the program has no v0.60
// spelling; the fail-closed successor is `Denial::UnresolvedFootprint`, which
// no source form now reaches because every footprint element the judgment
// builds resolves.

/// v0.59 kept a shared and an exclusive formal view apart by their loans.
/// v0.60 has one reference kind, so the rows decide: two `pure` calls over one
/// range reference are permitted, and two writing calls over the same one meet
/// on that path.
#[test]
fn the_row_not_the_reference_kind_decides_two_range_reference_calls() {
    let source = br#"fn read_only(view: &[u8]) -> result: u64 pure {
  return 0_u64;
}

fn write_through(view: &[u8]) -> result: u64 writes(view) contract {
  requires 1_u64 <= deref(view).len;
} {
  set deref(view)[0_u64] = 1_u8;
  return 0_u64;
}

fn shared(handed: &[u8]) -> result: u64 pure {
  let a = read_only(view: handed);
  let b = read_only(view: handed);
  return a +wrap b;
}

fn exclusive(handed: &[u8]) -> result: u64 writes(handed) contract {
  requires 1_u64 <= deref(handed).len;
} {
  let a = write_through(view: handed);
  let b = write_through(view: handed);
  return a +wrap b;
}
"#;
    let table = permission_of(source);
    assert_eq!(
        pair_of(&table, "shared", "read_only", "read_only").verdict,
        PermissionVerdict::PermittedEligible
    );
    let pair = pair_of(&table, "exclusive", "write_through", "write_through");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

// Range fixtures for the proof-carrying half of [OWN-7]. Each call writes
// the range it receives, while the source remains sequentially valid whether
// or not [PAR-1] can retain an overlap permission.
const RANGE_PERMISSION_HELPERS: &str = r#"fn stamp_range(part: &[u8]) -> result: u64 writes(part) contract {
  requires 0_u64 < deref(part).len;
} {
  set deref(part)[0_u64] = 9_u8;
  return 1_u64;
}

fn stamp_two_ranges(first: &[u8], second: &[u8]) -> result: u64 writes(first), writes(second) contract {
  requires 0_u64 < deref(first).len;
  requires 0_u64 < deref(second).len;
} {
  set deref(first)[0_u64] = 7_u8;
  set deref(second)[0_u64] = 8_u8;
  return 2_u64;
}
"#;

/// [REF-1, PAR-1] a source occurrence evaluated in a loop is not the runtime
/// generation a carried reference retained from the prior iteration. At
/// `i == 1`, `saved` is `[5..6]` from the preceding iteration and `other` is
/// the current `[5..6]`; the current formation at the same source occurrence
/// is `[4..5]`. `observed` snapshots that carried value before the rebinding,
/// so alias closure must retain the header alternative and deny the pair,
/// while using the freshly formed `current` in that same iteration keeps the
/// ordinary range-separation permission. The dominating length guards admit
/// each helper's real element store without supplying an affine image for the
/// carried range endpoints.
#[test]
fn loop_carried_range_generations_do_not_reuse_the_current_iteration_image() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn shifted(values: &Array<u8, 8>) -> result: u64 writes(values) {{
  let seed = &deref(values)[0_u64..1_u64];
  let saved = &deref(seed)[0_u64..deref(seed).len];
  for (i in 0_u64..2_u64) {{
    let current_start = 5_u64 - i;
    let current_end = 6_u64 - i;
    let other_start = 6_u64 - i;
    let other_end = 7_u64 - i;
    let current = &deref(values)[current_start..current_end];
    let other = &deref(values)[other_start..other_end];
    let observed = saved;
    if 0_u64 < deref(observed).len {{
      if 0_u64 < deref(other).len {{
        let a = stamp_range(part: observed);
        let b = stamp_range(part: other);
      }}
    }}
    set saved = &deref(current)[0_u64..deref(current).len];
  }}
  return 0_u64;
}}

fn current_iteration(values: &Array<u8, 8>) -> result: u64 writes(values) {{
  for (i in 0_u64..2_u64) {{
    let current_start = 5_u64 - i;
    let current_end = 6_u64 - i;
    let other_start = 6_u64 - i;
    let other_end = 7_u64 - i;
    let current = &deref(values)[current_start..current_end];
    let other = &deref(values)[other_start..other_end];
    if 0_u64 < deref(current).len {{
      if 0_u64 < deref(other).len {{
        let a = stamp_range(part: current);
        let b = stamp_range(part: other);
      }}
    }}
  }}
  return 0_u64;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let shifted = pair_of(&table, "shifted", "stamp_range", "stamp_range");
    let Denial::Footprint { kind, .. } = denial(shifted, 1) else {
        panic!(
            "the prior iteration overlaps the current other range: {:?}",
            shifted.verdict
        );
    };
    assert_eq!(kind.halves(), ("write", "write"));
    assert_eq!(
        pair_of(&table, "current_iteration", "stamp_range", "stamp_range").verdict,
        PermissionVerdict::PermittedEligible
    );
}

/// [REF-1, EFF-5] the same time shift is a source conflict when both writes
/// are effects of one call. The loop-header generation must fail closed in
/// the ordinary call-effect judgment as well as in optional [PAR-1].
#[test]
fn loop_carried_range_generations_do_not_discharge_overlapping_call_effects() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn shifted(values: &Array<u8, 8>) -> result: u64 writes(values) {{
  let seed = &deref(values)[0_u64..1_u64];
  let saved = &deref(seed)[0_u64..deref(seed).len];
  for (i in 0_u64..2_u64) {{
    let current_start = 5_u64 - i;
    let current_end = 6_u64 - i;
    let other_start = 6_u64 - i;
    let other_end = 7_u64 - i;
    let current = &deref(values)[current_start..current_end];
    let other = &deref(values)[other_start..other_end];
    let observed = saved;
    if 0_u64 < deref(observed).len {{
      if 0_u64 < deref(other).len {{
        let conflict = stamp_two_ranges(first: observed, second: other);
      }}
    }}
    set saved = &deref(current)[0_u64..deref(current).len];
  }}
  return 0_u64;
}}
"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Eff5, |_| true);
}

/// [PAR-1, OWN-7] both captured ranges exist before the first statement, and
/// its entering state proves the first range ends where the second starts.
/// The permission judgment must consume that exact proof instead of treating
/// every pair of range steps as overlapping.
#[test]
fn dynamic_ranges_separated_at_the_first_statement_are_permitted() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn separated(values: &Array<u8, 4>, split: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= split;
  requires split < 4_u64;
}} {{
  let left = &deref(values)[0_u64..split];
  let right = &deref(values)[split..4_u64];
  let a = stamp_range(part: left);
  let b = stamp_range(part: right);
  return a +wrap b;
}}
"
    );
    let table = permission_of_with_discharged_query(
        source.as_bytes(),
        &[("separated", RangeSeparationOrdering::LeftBeforeRight)],
    );
    assert_eq!(
        pair_of(&table, "separated", "stamp_range", "stamp_range").verdict,
        PermissionVerdict::PermittedEligible
    );
}

/// [PAR-1] a `let`'s defined binding is a write path, and a call that passes
/// a local reference variable reads that binding, which holds the endpoints
/// its formation captured. Resolving the argument only to the path the
/// reference names left the formation and its use unordered.
#[test]
fn a_call_passing_a_reference_reads_the_let_that_formed_it() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn front(values: &Array<u8, 4>, split: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= split;
  requires split < 4_u64;
}} {{
  let left = &deref(values)[0_u64..split];
  let a = stamp_range(part: left);
  return a;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let pair = pair_of(&table, "front", "a let statement", "stamp_range");
    let Denial::Footprint {
        kind,
        left,
        right,
        sides,
    } = denial(pair, 1)
    else {
        panic!("expected a footprint conflict, got {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "operand read"));
    let holder = ResolvedPlace::binding(pair.first.binding.expect("s1 defines a binding"));
    assert_eq!(left.place, holder, "the cited write is the formed binding");
    assert_eq!(right.place, holder, "the cited read is the same binding");
    assert_eq!(*sides, (PairSide::First, PairSide::Second));
}

/// The same holder read keeps a run from joining a reference's formation to
/// its use across another member: the quicksort shape forms both ranges
/// before either call, and the second call reads what the second `let`
/// formed. The two calls still form their own run.
#[test]
fn a_run_does_not_join_a_reference_formation_to_a_later_use() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn halves(values: &Array<u8, 4>, split: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= split;
  requires split < 4_u64;
}} {{
  let left = &deref(values)[0_u64..split];
  let right = &deref(values)[split..4_u64];
  let a = stamp_range(part: left);
  let b = stamp_range(part: right);
  return a +wrap b;
}}
"
    );
    let table = permission_of_with_discharged_query(
        source.as_bytes(),
        &[("halves", RangeSeparationOrdering::LeftBeforeRight)],
    );
    let permissions = function_table(&table, "halves");
    assert!(
        !permissions.runs.iter().any(|run| {
            run.sites
                .iter()
                .any(|site| site.callee_name == "a let statement")
                && run
                    .sites
                    .iter()
                    .any(|site| site.callee_name == "stamp_range")
        }),
        "no run may hold a range formation together with a call that uses it: {:?}",
        permissions.runs
    );
    run_of(&table, "halves", &["stamp_range", "stamp_range"]);
    assert_eq!(
        pair_of(&table, "halves", "stamp_range", "stamp_range").verdict,
        PermissionVerdict::PermittedEligible
    );
}

/// A `set` through a local reference variable reads that binding too, so it
/// does not join the `let` that formed the reference in a run.
#[test]
fn a_set_through_a_reference_reads_the_let_that_formed_it() {
    let source = br#"fn clear(values: &Array<u8, 4>) -> result: unit writes(values) {
  let head = &deref(values)[0_u64..2_u64];
  set deref(head)[0_u64] = 0_u8;
  return unit;
}
"#;
    let table = permission_of(source);
    let permissions = function_table(&table, "clear");
    assert!(
        !permissions.runs.iter().any(|run| {
            run.sites
                .iter()
                .any(|site| site.callee_name == "a let statement")
                && run
                    .sites
                    .iter()
                    .any(|site| site.callee_name == "a set statement")
        }),
        "the formation and the write through it must stay ordered: {:?}",
        permissions.runs
    );
}

/// A `set` that rebinds a reference variable writes it, so the rebinding
/// stays ordered against a later statement that forms a range from the
/// variable, as the `let` that first formed it is [REF-1, PAR-1].
#[test]
fn a_rebinding_set_writes_the_reference_it_rebinds() {
    let source = br#"fn rebind(values: &Array<u8, 4>) -> result: unit writes(values) {
  let whole = &deref(values)[0_u64..4_u64];
  set whole = &deref(values)[1_u64..3_u64];
  let head = &deref(whole)[0_u64..1_u64];
  set deref(head)[0_u64] = 0_u8;
  return unit;
}
"#;
    let table = permission_of(source);
    let permissions = function_table(&table, "rebind");
    assert!(
        !permissions.runs.iter().any(|run| {
            run.sites.windows(2).any(|sites| {
                sites[0].callee_name == "a set statement"
                    && sites[1].callee_name == "a let statement"
            })
        }),
        "the rebinding and the formation through it must stay ordered: {:?}",
        permissions.runs
    );
}

/// [PAR-1] a run means every source-ordered pair, including nonadjacent
/// members. The benign scalar statement reaches neither range, so the two
/// calls and that statement form one run only when the first/third captured
/// ranges are proved apart in the first call's entering state.
#[test]
fn a_nonadjacent_dynamic_range_pair_keeps_the_complete_run() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn separated(values: &Array<u8, 4>, split: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= split;
  requires split < 4_u64;
}} {{
  let left = &deref(values)[0_u64..split];
  let right = &deref(values)[split..4_u64];
  let a = stamp_range(part: left);
  let gap = 7_u64 +wrap 1_u64;
  let b = stamp_range(part: right);
  return a +wrap b;
}}
"
    );
    let table = permission_of_with_discharged_query(
        source.as_bytes(),
        &[("separated", RangeSeparationOrdering::LeftBeforeRight)],
    );
    let permissions = function_table(&table, "separated");
    assert!(
        permissions.runs.iter().any(|run| {
            run.sites.windows(3).any(|sites| {
                sites[0].callee_name == "stamp_range"
                    && sites[1].callee_name == "a let statement"
                    && sites[2].callee_name == "stamp_range"
            })
        }),
        "the two calls and their benign interposed statement must remain in one all-pairs run: {:?}",
        permissions.runs
    );
}

/// [REF-4, PAR-1] endpoint values belong to their range-formation captures.
/// Rebinding the source scalar later must not retarget an earlier capture and
/// manufacture separation for two ranges that were identical when formed.
#[test]
fn rebinding_an_endpoint_does_not_separate_earlier_captured_ranges() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn stale(values: &Array<u8, 4>) -> result: u64 writes(values) {{
  let start = 0_u64;
  let stop = 2_u64;
  let left = &deref(values)[0_u64..2_u64];
  let right = &deref(values)[start..stop];
  set start = 2_u64;
  let a = stamp_range(part: left);
  let b = stamp_range(part: right);
  return a +wrap b;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let pair = pair_of(&table, "stale", "stamp_range", "stamp_range");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!(
            "expected the captured ranges to overlap: {:?}",
            pair.verdict
        );
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

/// [ENT-5, PAR-1] the guarded pair may use its arm-entry ordering, but that
/// proof is unavailable after the join. The exact same captured ranges at two
/// different statement pairs therefore receive different permission verdicts
/// rather than sharing one function-wide range oracle.
#[test]
fn a_guarded_range_permission_does_not_escape_to_another_statement_pair() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn guarded(values: &Array<u8, 4>, cut: u64, start: u64) -> result: u64 writes(values) contract {{
  requires 1_u64 <= cut;
  requires cut <= 4_u64;
  requires start < 4_u64;
}} {{
  let left = &deref(values)[0_u64..cut];
  let right = &deref(values)[start..4_u64];
  if cut <= start {{
    let guarded_left = stamp_range(part: left);
    let guarded_right = stamp_range(part: right);
  }}
  let outer_left = stamp_range(part: left);
  let outer_right = stamp_range(part: right);
  return outer_left +wrap outer_right;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let pairs = pairs_of(&table, "guarded", "stamp_range", "stamp_range");
    let [inside, after_join] = pairs.as_slice() else {
        panic!("guarded must contain the arm pair and the post-join pair: {pairs:?}");
    };
    assert_eq!(inside.verdict, PermissionVerdict::PermittedEligible);
    let Denial::Footprint { kind, .. } = denial(after_join, 1) else {
        panic!(
            "the post-join pair must not reuse the arm proof: {:?}",
            after_join.verdict
        );
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

/// [PAR-1] a statement pair with several range conflicts is permitted only
/// when every conflicting pair is apart. Three literal pairs here separate,
/// but `[4..6]` and `[5..7]` overlap, so one successful proof must not hide
/// the remaining write/write conflict.
#[test]
fn every_range_conflict_of_a_multi_target_pair_must_be_separated() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn conjunction(values: &Array<u8, 8>) -> result: u64 writes(values) {{
  let a0 = &deref(values)[0_u64..2_u64];
  let a1 = &deref(values)[4_u64..6_u64];
  let b0 = &deref(values)[2_u64..4_u64];
  let b1 = &deref(values)[5_u64..7_u64];
  let a = stamp_two_ranges(first: a0, second: a1);
  let b = stamp_two_ranges(first: b0, second: b1);
  return a +wrap b;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let pair = pair_of(
        &table,
        "conjunction",
        "stamp_two_ranges",
        "stamp_two_ranges",
    );
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("one overlapping range pair must deny: {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

/// [PAR-1] this is the conservative first-point control, not an assertion
/// about reference-holder dataflow: `right` is formed between the calls, so
/// its capture has no affine image in the state before the first call. The
/// bounded range handoff therefore cannot use it to justify a wider run.
#[test]
fn a_later_formed_range_does_not_justify_a_wider_run() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn later(values: &Array<u8, 4>) -> result: u64 writes(values) {{
  let left = &deref(values)[0_u64..2_u64];
  let a = stamp_range(part: left);
  let right = &deref(values)[2_u64..4_u64];
  let b = stamp_range(part: right);
  return a +wrap b;
}}
"
    );
    let table = permission_of(source.as_bytes());
    let has_wider_run = function_table(&table, "later").runs.iter().any(|run| {
        run.sites.iter().map(|site| site.callee_name.as_str()).eq([
            "stamp_range",
            "a let statement",
            "stamp_range",
        ])
    });
    assert!(!has_wider_run);
}

/// [PAR-1, EFF-5] a range formed at the call resolves to its source's path
/// extended by the formation's own range step, as a bound range reference
/// does. Two inline ranges over different roots are therefore disjoint, and
/// two over one root meet on their range steps as an ordinary footprint
/// conflict; neither is the fail-closed unresolved footprint.
#[test]
fn inline_range_actuals_resolve_to_their_formation_paths() {
    let source = format!(
        "{RANGE_PERMISSION_HELPERS}
fn independent(values: &Array<u8, 4>, others: &Array<u8, 4>) -> result: u64 writes(values), writes(others) {{
  let a = stamp_range(part: &deref(values)[0_u64..2_u64]);
  let b = stamp_range(part: &deref(others)[0_u64..2_u64]);
  return a +wrap b;
}}

fn shared(values: &Array<u8, 4>) -> result: u64 writes(values) {{
  let a = stamp_range(part: &deref(values)[0_u64..3_u64]);
  let b = stamp_range(part: &deref(values)[2_u64..4_u64]);
  return a +wrap b;
}}
"
    );
    let table = permission_of(source.as_bytes());
    assert_eq!(
        pair_of(&table, "independent", "stamp_range", "stamp_range").verdict,
        PermissionVerdict::PermittedEligible
    );
    let pair = pair_of(&table, "shared", "stamp_range", "stamp_range");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!(
            "overlapping inline ranges must meet on their resolved paths: {:?}",
            pair.verdict
        );
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

// A partition-based recursive subdivision over one range parameter. The
// split point is a call result, so only its `ensures` places it inside the
// range, and each caller below forms its two child ranges either as call
// actuals or as bound ranges first.
const PARTITION_HELPER: &str = r#"fn partition(v: &[u8]) -> pivot_at: u64 writes(v) contract {
  requires 2_u64 <= deref(v).len;
  ensures pivot_at < deref(v).len;
} {
  let first = deref(v)[0_u64];
  set deref(v)[0_u64] = first;
  return 0_u64;
}
"#;

/// The body of one recursive subdivision: `children` is the pair of
/// statements that forms and passes the two child ranges.
fn subdivision(name: &str, children: &str) -> String {
    format!(
        "fn {name}(v: &[u8]) -> result: unit writes(v) {{
  let n = deref(v).len;
  if n <= 1_u64 {{
    return unit;
  }}
  let p = partition(v: v);
  let after = p + 1_u64;
{children}  return unit;
}}
"
    )
}

/// [PAR-1, REF-4, OWN-7] a range formed at the call names exactly the storage
/// the same range bound first names, so the verdict cannot depend on the
/// spelling. Both spellings of the two splits — at `p` and `p + 1`, and at one
/// shared `p` — are permitted by the same retained ordering, proved in the
/// state before the first call. The recursive calls read `after` and `n` but
/// write only their child ranges, so nothing the first call writes changes
/// the second call's endpoints.
#[test]
fn a_range_formed_at_the_call_is_judged_as_the_same_range_bound_first() {
    let source = [
        PARTITION_HELPER.to_owned(),
        subdivision(
            "split_inline",
            "  split_inline(v: &deref(v)[0_u64..p]);\n  split_inline(v: &deref(v)[after..n]);\n",
        ),
        subdivision(
            "split_bound",
            "  let smaller = &deref(v)[0_u64..p];\n  let larger = &deref(v)[after..n];\n  \
             split_bound(v: smaller);\n  split_bound(v: larger);\n",
        ),
        subdivision(
            "shared_inline",
            "  shared_inline(v: &deref(v)[0_u64..p]);\n  shared_inline(v: &deref(v)[p..n]);\n",
        ),
        subdivision(
            "shared_bound",
            "  let smaller = &deref(v)[0_u64..p];\n  let larger = &deref(v)[p..n];\n  \
             shared_bound(v: smaller);\n  shared_bound(v: larger);\n",
        ),
    ]
    .join("\n");
    let spellings = [
        "split_inline",
        "split_bound",
        "shared_inline",
        "shared_bound",
    ];
    let table = permission_of_with_discharged_query(
        source.as_bytes(),
        &spellings.map(|name| (name, RangeSeparationOrdering::LeftBeforeRight)),
    );
    for name in spellings {
        assert_eq!(
            pair_of(&table, name, name, name).verdict,
            PermissionVerdict::PermittedEligible,
            "{name}"
        );
    }
}

/// [PAR-1, OWN-7] the negative control of the case above: `[0..p+1)` and
/// `[p..n)` share element `p`, and evaluating the inline formations before
/// the first call must not separate them in either spelling.
#[test]
fn overlapping_ranges_formed_at_the_call_are_denied_in_both_spellings() {
    let source = [
        PARTITION_HELPER.to_owned(),
        subdivision(
            "overlap_inline",
            "  overlap_inline(v: &deref(v)[0_u64..after]);\n  overlap_inline(v: &deref(v)[p..n]);\n",
        ),
        subdivision(
            "overlap_bound",
            "  let smaller = &deref(v)[0_u64..after];\n  let larger = &deref(v)[p..n];\n  \
             overlap_bound(v: smaller);\n  overlap_bound(v: larger);\n",
        ),
    ]
    .join("\n");
    let table = permission_of(source.as_bytes());
    for name in ["overlap_inline", "overlap_bound"] {
        let pair = pair_of(&table, name, name, name);
        let Denial::Footprint { kind, .. } = denial(pair, 1) else {
            panic!("{name}: the shared element must deny: {:?}", pair.verdict);
        };
        assert_eq!(kind.halves(), ("write", "write"), "{name}");
    }
}

/// [PAR-1] interprets both statements' paths in the state before the first,
/// so an endpoint the first statement writes has no value there. Here the
/// first statement both writes `[0..p)` and replaces `q`, which held `p`
/// before it; the callee's `ensures` makes the new `q` zero, so the second
/// statement's `[q..n)` covers `[0..p)`. Reading `q` as it was before the
/// first statement would prove `p <= q` and separate them. The retained
/// question must stay undischarged, and the denial is the overlap of the two
/// ranges themselves rather than the later read of `q`.
#[test]
fn an_endpoint_the_first_statement_writes_is_not_read_before_it() {
    let source = r#"fn stamp_at(part: &[u8]) -> result: u64 writes(part) contract {
  ensures result <= 0_u64;
} {
  if 0_u64 < deref(part).len {
    set deref(part)[0_u64] = 9_u8;
  }
  return 0_u64;
}

fn rebound(v: &[u8], p: u64) -> result: u64 writes(v) contract {
  requires p <= deref(v).len;
} {
  let n = deref(v).len;
  let q = p;
  set q = stamp_at(part: &deref(v)[0_u64..p]);
  let b = stamp_at(part: &deref(v)[q..n]);
  return b;
}
"#;
    let combined = [MARKER.as_bytes(), source.as_bytes()].concat();
    let table = with_semantics(&combined, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("permission fixture must check: {outcome:?}");
        };
        for function in &program.data.functions {
            super::entailment::validate_derivations(&function.entailment);
        }
        let function = program
            .data
            .functions
            .iter()
            .find(|function| function.name == "rebound")
            .expect("rebound is checked");
        let proofs = &function.entailment.permission_separations;
        assert!(
            !proofs.is_empty() && proofs.iter().all(|proof| !proof.discharged),
            "the range pair must be asked and left undischarged: {proofs:?}"
        );
        program.data.permission.clone()
    });
    let pair = pair_of(&table, "rebound", "a set statement", "stamp_at");
    let Denial::Footprint { kind, .. } = denial(pair, 1) else {
        panic!("the replaced endpoint must deny: {:?}", pair.verdict);
    };
    assert_eq!(kind.halves(), ("write", "write"));
}

/// [PAR-1, OWN-7, REF-1] a range's captured endpoints belong to its own
/// formation, whatever their form. `collide` writes two ranges of one
/// field-read spelling and `twice` writes one range twice, each through two
/// adjacent calls; the empty range formed before those calls captures only
/// its own endpoints, so it separates neither pair and both are denied.
/// `apart` meets at one measure, `deref(w).len`, so the state before its
/// first call proves the first range ends where the second starts, and that
/// retained ordering permits the pair although no endpoint is a literal or a
/// binding.
#[test]
fn range_endpoints_of_every_form_belong_to_their_own_formation() {
    let source = br#"struct Bounds {
  lo: u64;
}

fn mark(part: &[u8]) -> result: u64 writes(part) {
  if 0_u64 < deref(part).len {
    set deref(part)[0_u64] = 9_u8;
  }
  return 1_u64;
}

fn collide(v: &[u8], b: Bounds) -> result: u64 writes(v) contract {
  requires b.lo <= deref(v).len;
} {
  let left = &deref(v)[b.lo..deref(v).len];
  let right = &deref(v)[b.lo..deref(v).len];
  let empty = &deref(v)[deref(v).len..deref(v).len];
  let x = mark(part: left);
  let y = mark(part: right);
  return x +wrap y;
}

fn twice(v: &[u8], b: Bounds) -> result: u64 writes(v) contract {
  requires b.lo <= deref(v).len;
} {
  let part = &deref(v)[b.lo..deref(v).len];
  let empty = &deref(v)[deref(v).len..deref(v).len];
  let x = mark(part: part);
  let y = mark(part: part);
  return x +wrap y;
}

fn apart(v: &[u8], u: &[u8], w: &[u8]) -> result: u64 reads(u), reads(w), writes(v) contract {
  requires deref(u).len <= deref(w).len;
  requires deref(w).len <= deref(v).len;
} {
  let low = &deref(v)[deref(u).len..deref(w).len];
  let high = &deref(v)[deref(w).len..deref(v).len];
  let x = mark(part: low);
  let y = mark(part: high);
  return x +wrap y;
}
"#;
    let table = permission_of_with_discharged_query(
        source,
        &[("apart", RangeSeparationOrdering::LeftBeforeRight)],
    );
    for function in ["collide", "twice"] {
        let pair = pair_of(&table, function, "mark", "mark");
        let Denial::Footprint { kind, .. } = denial(pair, 1) else {
            panic!(
                "{function}: the overlapping writes must deny: {:?}",
                pair.verdict
            );
        };
        assert_eq!(kind.halves(), ("write", "write"), "{function}");
    }
    assert_eq!(
        pair_of(&table, "apart", "mark", "mark").verdict,
        PermissionVerdict::PermittedEligible
    );
}

/// Prelude calls use the ordinary call permission judgment. This pure call
/// forms the two adjacent eligible pairs rather than becoming an opaque
/// statement the judgment passes over.
#[test]
fn an_inline_prelude_call_forms_ordinary_adjacent_pairs() {
    let source = br#"fn quiet(cell: &Cell) -> result: u64 pure {
  return 3_u64;
}

fn probe(x: u64, name: std::text::HostString) -> result: u64 pure {
  let p = Cell(value: x);
  let r = Cell(value: x);
  let a = quiet(cell: &p);
  let path = std::fs::relative_path(value: move name);
  let b = quiet(cell: &r);
  let s = a +wrap b;
  return s;
}
"#;
    let table = permission_of(&cells(text(source)));
    assert_eq!(
        pair_of(&table, "probe", "quiet", "relative_path").verdict,
        PermissionVerdict::PermittedEligible
    );
    assert_eq!(
        pair_of(&table, "probe", "relative_path", "quiet").verdict,
        PermissionVerdict::PermittedEligible
    );
}

// ----------------------------------------------------------------------
// Call position
// ----------------------------------------------------------------------

/// [PAR-1] includes the scrutinee and every possible arm in a match's
/// footprint. Both arms here are empty, so these independent calls remain
/// eligible. The conflicting-arm control above checks that a nonempty arm
/// cannot hide a read of the first statement's result.
#[test]
fn a_scrutinee_call_with_independent_arms_forms_a_pair() {
    let source = br#"fn main(out: u64, err: u64) -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 65_u8);
  let bytes = slots_from_array::<u8, 2>(values: values);
  let window = &bytes[0_u64..2_u64];
  let first = write_marker(output: &out, source: window, start: 0_u64, end: 1_u64);
  match write_marker(output: &err, source: window, start: 1_u64, end: 2_u64) {
    Ok(value: written) => {
    }
    Err(error: problem) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    assert_eq!(
        pair_of(&table, "main", "write_marker", "write_marker").verdict,
        PermissionVerdict::PermittedEligible
    );
}

/// [PAR-1] applies the same complete-footprint check when the match is the
/// first statement; source order does not exclude its scrutinee call.
#[test]
fn a_scrutinee_call_written_first_with_independent_arms_forms_a_pair() {
    let source = br#"fn main(out: u64, err: u64) -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 2>(value: 65_u8);
  let bytes = slots_from_array::<u8, 2>(values: values);
  let window = &bytes[0_u64..2_u64];
  match write_marker(output: &out, source: window, start: 0_u64, end: 1_u64) {
    Ok(value: written) => {
    }
    Err(error: problem) => {
    }
  }
  let second = write_marker(output: &err, source: window, start: 1_u64, end: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    assert_eq!(
        pair_of(&table, "main", "write_marker", "write_marker").verdict,
        PermissionVerdict::PermittedEligible
    );
}
