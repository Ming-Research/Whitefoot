//! TYPE-11 range sites, nested element identity and placed OP-10 writes.
use super::with_semantics;
use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

const MAIN: &str = "\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

const CONTEXT: &str = "struct Block {\n  entry_slot: u64;\n  owner: u64;\n  normal_y: i64;\n}\n\nenum Flow {\n  Open(block: u64);\n  Close(block: u64);\n}\n\nstruct Order {\n  payloads: Slots<Flow, 64>;\n}\n\nstruct Context {\n  orders: Slots<Order, 16>;\n  blocks: Slots<Block, 64>;\n  invariant(c): forall slot(b in 0_u64..c^.orders.len, k in 0_u64..c^.orders[b].payloads.len) when c^.orders[b].payloads[k].Open.block < c^.blocks.len: c^.blocks[c^.orders[b].payloads[k].Open.block].entry_slot == k, c^.blocks[c^.orders[b].payloads[k].Open.block].owner == b;\n}\n\n";

fn check(source: &str, failure: Option<(SemanticRule, &str)>) {
    let source = format!("{source}{MAIN}");
    with_semantics(source.as_bytes(), |outcome| match (outcome, failure) {
        (SemanticOutcome::Complete(_), None) => {}
        (SemanticOutcome::SourceIssue { issue, .. }, Some((rule, site))) => {
            assert_eq!(issue.rule(), rule, "{issue:?}");
            assert!(
                matches!(issue.kind(), SemanticIssueKind::UndischargedRangeFact { site: actual, .. } if *actual == site),
                "{issue:?}"
            );
        }
        (outcome, expected) => panic!("expected {expected:?}, got {outcome:?}"),
    });
}

#[test]
fn implicit_range_requirement_certifies_a_callee_and_reaches_its_caller() {
    let source = format!(
        "{CONTEXT}fn paint(c: &Context, owner: u64) -> result: unit reads(c.orders), writes(c.blocks) {{
  if owner < c^.orders.len {{
    let count = c^.orders[owner].payloads.len;
    for (
      k in 0_u64..count,
      apart(i, j) {{
      }}
    ) {{
      let item = c^.orders[owner].payloads[k];
      match item {{
        Open(block: target) => {{
          if target < c^.blocks.len {{
            set c^.blocks[target].normal_y = 1_i64;
          }}
        }}
        Close(block: closed) => {{
        }}
      }}
    }}
  }}
  return unit;
}}

fn forward(c: &Context, owner: u64) -> result: unit reads(c.orders), writes(c.blocks) {{
  paint(c: c, owner: owner);
  return unit;
}}
{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("{outcome:?}");
        };
        let paint = program
            .data
            .executable_functions()
            .find(|function| function.name == "paint")
            .unwrap();
        assert_eq!(paint.range_facts.requirements.len(), 1);
        assert_eq!(
            paint
                .range_facts
                .postconditions
                .iter()
                .filter(|post| post.owed)
                .count(),
            1
        );
        assert_eq!(paint.range_facts.certified.len(), 1);
        assert_eq!(paint.range_facts.certified[0].writes.len(), 1);
        let forward = program
            .data
            .executable_functions()
            .find(|function| function.name == "forward")
            .unwrap();
        assert_eq!(forward.range_facts.requirements.len(), 1);
    });
}

#[test]
fn sibling_field_writer_preserves_the_type_invariant() {
    check(
        &format!(
            "{CONTEXT}fn paint(c: &Context, target: u64) -> result: unit writes(c.blocks) {{
  if target < c^.blocks.len {{
    set c^.blocks[target].normal_y = 7_i64;
  }}
  return unit;
}}
"
        ),
        None,
    );
}

#[test]
fn unknown_entry_slot_write_owes_the_type_invariant_at_exit() {
    check(&format!("{CONTEXT}fn damage(c: &Context, target: u64, unknown: u64) -> result: unit writes(c.blocks) {{
  if target < c^.blocks.len {{
    set c^.blocks[target].entry_slot = unknown;
  }}
  return unit;
}}
"), Some((SemanticRule::Range3, "a return")));
}

fn construction(slot: u64) -> String {
    format!(
        "{CONTEXT}fn build() -> result: Context pure {{
  let payloads = slots_new::<Flow, 64>();
  let opening = Flow::Open(block: 0_u64);
  place_back(window: &payloads, value: opening);
  let orders = slots_new::<Order, 16>();
  let order = Order(payloads: move payloads);
  place_back(window: &orders, value: move order);
  let blocks = slots_new::<Block, 64>();
  let block = Block(entry_slot: {slot}_u64, owner: 0_u64, normal_y: 0_i64);
  place_back(window: &blocks, value: block);
  return Context(orders: move orders, blocks: move blocks);
}}
"
    )
}

#[test]
fn construction_and_result_ordinal_carry_the_range_type_invariant() {
    check(&construction(0), None);
}

#[test]
fn inconsistent_stores_fail_type11_at_construction() {
    check(
        &construction(1),
        Some((SemanticRule::Type11, "a construction")),
    );
}

fn nested(before: &str, effect: &str) -> String {
    format!("struct Row {{
  payloads: Slots<u64, 4>;
  other: u64;
}}

fn need(rows: &[Row]) -> result: unit pure contract {{
  requires forall wanted(b in 0_u64..rows^.len, k in 0_u64..rows^[b].payloads.len): rows^[b].payloads[k] == 0_u64;
}} {{
  return unit;
}}

fn forward(rows: &[Row]) -> result: unit {effect} contract {{
  requires forall held(b in 0_u64..rows^.len, k in 0_u64..rows^[b].payloads.len): rows^[b].payloads[k] == 0_u64;
}} {{
{before}  need(rows: rows);
  return unit;
}}
")
}

#[test]
fn nested_subscript_forms_and_preserves_its_tuple() {
    check(&nested("", "pure"), None);
}

#[test]
fn nested_sibling_write_preserves_all_inner_elements() {
    check(
        &nested(
            "  if 0_u64 < rows^.len {\n    set rows^[0_u64].other = 1_u64;\n  }\n",
            "writes(rows)",
        ),
        None,
    );
}

#[test]
fn nested_write_breaks_only_the_selected_tuple() {
    check(
        &nested(
            "  if 0_u64 < rows^.len {\n    if 0_u64 < rows^[0_u64].payloads.len {\n      set rows^[0_u64].payloads[0_u64] = 1_u64;\n    }\n  }\n",
            "writes(rows)",
        ),
        Some((SemanticRule::Range3, "a call")),
    );
}

#[test]
fn nested_descriptor_growth_defines_the_new_element() {
    check(
        &nested(
            "  if 0_u64 < rows^.len {\n    if rows^[0_u64].payloads.len < rows^[0_u64].payloads.cap {\n      place_back(window: &rows^[0_u64].payloads, value: 0_u64);\n    }\n  }\n",
            "writes(rows)",
        ),
        None,
    );
}

#[test]
fn nested_descriptor_growth_cannot_keep_a_false_content_fact() {
    check(
        &nested(
            "  if 0_u64 < rows^.len {\n    if rows^[0_u64].payloads.len < rows^[0_u64].payloads.cap {\n      place_back(window: &rows^[0_u64].payloads, value: 1_u64);\n    }\n  }\n",
            "writes(rows)",
        ),
        Some((SemanticRule::Range3, "a call")),
    );
}

fn push(value: u64) -> String {
    format!(
        "fn push_zero(xs: &Slots<u64, 1>) -> result: unit writes(xs) contract {{
  requires xs^.len == 0_u64;
  ensures forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;
}} {{
  place_back(window: xs, value: {value}_u64);
  return unit;
}}
"
    )
}

#[test]
fn place_back_defines_the_placed_value_and_length() {
    check(&push(0), None);
}

#[test]
fn place_back_does_not_invent_the_required_value() {
    check(&push(1), Some((SemanticRule::Range3, "a return")));
}

#[test]
fn take_back_preserves_the_remaining_prefix() {
    check(
        "fn shrink(xs: &Slots<u64, 4>) -> result: unit writes(xs) contract {
  requires 0_u64 < xs^.len;
  requires forall before(k in 0_u64..xs^.len): xs^[k] == 0_u64;
  ensures forall after(k in 0_u64..xs^.len): xs^[k] == 0_u64;
} {
  let removed = take_back(window: xs);
  return unit;
}
",
        None,
    );
}

#[test]
fn deeper_subscripts_keep_all_selection_positions() {
    check("fn need(xs: &[Slots<Slots<u64, 2>, 2>]) -> result: unit pure contract {
  requires forall kept(b in 0_u64..xs^.len, k in 0_u64..xs^[b].len) when 0_u64 < xs^[b][k].len: xs^[b][k][0_u64] == 0_u64;
} {
  return unit;
}

fn deep(xs: &[Slots<Slots<u64, 2>, 2>]) -> result: unit pure contract {
  requires forall held(b in 0_u64..xs^.len, k in 0_u64..xs^[b].len) when 0_u64 < xs^[b][k].len: xs^[b][k][0_u64] == 0_u64;
} {
  need(xs: xs);
  return unit;
}
", None);
}

fn atomic(body: &str) -> String {
    format!(
        "{CONTEXT}fn update(state: Shared<Context>) -> result: unit pure waits {{
  atomic held = &state {{
    if 0_u64 < held^.blocks.len {{
{body}    }}
  }}
  return unit;
}}
"
    )
}

#[test]
fn atomic_sibling_write_keeps_the_entry_fact() {
    check(
        &atomic("      set held^.blocks[0_u64].normal_y = 9_i64;\n"),
        None,
    );
}

#[test]
fn atomic_end_owes_type11() {
    check(
        &atomic("      set held^.blocks[0_u64].entry_slot = 9_u64;\n"),
        Some((SemanticRule::Type11, "an atomic leaving edge")),
    );
}

#[test]
fn atomic_early_return_owes_type11() {
    check(
        &atomic("      set held^.blocks[0_u64].entry_slot = 9_u64;\n      return unit;\n"),
        Some((SemanticRule::Type11, "an atomic leaving edge")),
    );
}

#[test]
fn shared_new_takes_the_structs_range_requirement() {
    check(
        &format!(
            "{CONTEXT}fn publish(c: Context) -> result: Shared<Context> pure {{
  return shared_new::<Context>(value: move c);
}}
"
        ),
        None,
    );
}

// Freshness is essential: replacing a target already named by another slot
// cannot preserve a left inverse, even when every index is in bounds.
fn insert(slot: &str) -> String {
    format!("{CONTEXT}fn insert(c: &Context, owner: u64, target: u64) -> result: unit writes(c.orders), writes(c.blocks) contract {{
  requires owner < c^.orders.len;
  requires target < c^.blocks.len;
  requires c^.orders[owner].payloads.len < c^.orders[owner].payloads.cap;
  requires forall fresh(b in 0_u64..c^.orders.len, k in 0_u64..c^.orders[b].payloads.len): c^.orders[b].payloads[k].Open.block != target;
}} {{
  let next_slot = c^.orders[owner].payloads.len;
  let opening = Flow::Open(block: target);
  place_back(window: &c^.orders[owner].payloads, value: opening);
  set c^.blocks[target].entry_slot = {slot};
  set c^.blocks[target].owner = owner;
  return unit;
}}
")
}

#[test]
fn insert_of_a_fresh_target_preserves_the_inverse() {
    check(&insert("next_slot"), None);
}

#[test]
fn insert_of_a_wrong_slot_fails_at_exit() {
    check(
        &insert("18446744073709551615_u64"),
        Some((SemanticRule::Range3, "a return")),
    );
}

#[test]
fn finish_sequence_uses_one_range_invariant_over_the_processed_prefix() {
    check(&format!("{CONTEXT}fn finish_sequence(c: &Context, pending: &[u64]) -> result: unit reads(pending), writes(c.orders), writes(c.blocks) contract {{
  requires c^.orders.len == 1_u64;
  requires c^.orders[0_u64].payloads.len == 0_u64;
  requires pending^.len <= c^.orders[0_u64].payloads.cap;
  requires forall pending_bounds(k in 0_u64..pending^.len): pending^[k] < c^.blocks.len;
  requires forall pending_inverse(k in 0_u64..pending^.len) when pending^[k] < c^.blocks.len: c^.blocks[pending^[k]].entry_slot == k;
}} {{
  let count = pending^.len;
  for (
    k in 0_u64..count,
    invariant length: c^.orders[0_u64].payloads.len == k,
    invariant forall prefix(j in 0_u64..k): c^.orders[0_u64].payloads[j].Open.block == pending^[j], c^.blocks[c^.orders[0_u64].payloads[j].Open.block].entry_slot == j, c^.blocks[c^.orders[0_u64].payloads[j].Open.block].owner == 0_u64
  ) {{
    let target = pending^[k];
    let opening = Flow::Open(block: target);
    place_back(window: &c^.orders[0_u64].payloads, value: opening);
    set c^.blocks[target].entry_slot = k;
    set c^.blocks[target].owner = 0_u64;
  }}
  return unit;
}}
"), None);
}

#[test]
fn constant_construction_is_judged_by_the_range_derivation() {
    for value in [0, 1] {
        let source = format!(
            "struct Zeroes {{
  cells: Array<u64, 1>;
  invariant(c): forall zero(k in 0_u64..c^.cells.len): c^.cells[k] == 0_u64;
}}

const zeroes: Zeroes = Zeroes(cells:[{value}_u64]);
"
        );
        check(
            &source,
            (value == 1).then_some((SemanticRule::Type11, "a construction")),
        );
    }
}

#[test]
fn nested_indices_do_not_separate_accesses_to_one_outer_element() {
    let source = format!(
        "struct Row {{
  payloads: Array<u64, 2>;
}}

fn paint(rows: &[Row]) -> result: unit writes(rows) {{
  if 0_u64 < rows^.len {{
    for (
      k in 0_u64..2_u64,
      apart(i, j) {{
      }}
    ) {{
      set rows^[0_u64].payloads[k] = 0_u64;
    }}
  }}
  return unit;
}}
{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected overlapping outer elements, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Range5, "{issue:?}");
        assert!(
            matches!(issue.kind(), SemanticIssueKind::UndischargedApart { .. }),
            "{issue:?}"
        );
    });
}

fn copied_collection(before: &str, after: &str, effect: &str) -> String {
    format!("struct Row {{
  payloads: Array<u64, 1>;
}}

fn need(xs: &Array<u64, 1>) -> result: unit pure contract {{
  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;
}} {{
  return unit;
}}

fn forward(rows: &[Row]) -> result: unit {effect} contract {{
  requires forall held(b in 0_u64..rows^.len, k in 0_u64..rows^[b].payloads.len): rows^[b].payloads[k] == 0_u64;
}} {{
  if 0_u64 < rows^.len {{
{before}    let saved = rows^[0_u64];
{after}    need(xs: &saved.payloads);
  }}
  return unit;
}}
")
}

#[test]
fn a_copied_elements_collection_retains_the_source_definition() {
    check(&copied_collection("", "", "reads(rows)"), None);
}

#[test]
fn a_copied_collection_retains_its_version_after_a_source_write() {
    check(
        &copied_collection(
            "",
            "    set rows^[0_u64].payloads[0_u64] = 1_u64;\n",
            "writes(rows)",
        ),
        None,
    );
}

#[test]
fn a_write_to_the_copied_collection_invalidates_its_old_contents() {
    check(
        &copied_collection(
            "",
            "    set saved.payloads[0_u64] = 1_u64;\n",
            "reads(rows)",
        ),
        Some((SemanticRule::Range3, "a call")),
    );
}

#[test]
fn copying_a_changed_collection_does_not_recover_its_old_contents() {
    check(
        &copied_collection(
            "    set rows^[0_u64].payloads[0_u64] = 1_u64;\n",
            "",
            "writes(rows)",
        ),
        Some((SemanticRule::Range3, "a call")),
    );
}

const ZEROES: &str = "struct Zeroes {\n  cells: Array<u64, 1>;\n  invariant(c): forall zero(k in 0_u64..c^.cells.len): c^.cells[k] == 0_u64;\n}\n\n";

#[test]
fn an_atomic_guard_is_a_premise_at_its_leaving_edge() {
    check(
        &format!(
            "{ZEROES}fn update(state: Shared<Zeroes>, proposed: u64) -> result: unit pure waits {{
  atomic held = &state when proposed == 0_u64 {{
    set held^.cells[0_u64] = proposed;
  }}
  return unit;
}}
"
        ),
        None,
    );
}

#[test]
fn possibly_aliased_atomic_targets_are_explicitly_unsupported() {
    let source = format!(
        "{ZEROES}fn need(z: &Zeroes) -> result: unit pure {{
  return unit;
}}

fn update(first: Shared<Zeroes>, second: Shared<Zeroes>) -> result: unit pure waits {{
  atomic a = &first, b = &second {{
    set a^.cells[0_u64] = 1_u64;
    need(z: b);
    set a^.cells[0_u64] = 0_u64;
  }}
  return unit;
}}
{MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("expected unsupported alias tracking, got {outcome:?}");
        };
        assert_eq!(
            unsupported.feature(),
            crate::UnsupportedSemanticFeature::RangeAtomicAliases
        );
    });
}

fn conditional_push(value: u64) -> String {
    format!(
        "fn grow_zero(xs: &Slots<u64, 1>, flag: Bool) -> result: unit writes(xs) contract {{
  requires xs^.len == 0_u64;
  ensures forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;
}} {{
  if flag {{
    place_back(window: xs, value: {value}_u64);
  }}
  return unit;
}}
"
    )
}

#[test]
fn a_conditional_place_back_joins_length_with_its_contents() {
    check(&conditional_push(0), None);
}

#[test]
fn a_conditional_place_back_cannot_hide_a_wrong_element() {
    check(
        &conditional_push(1),
        Some((SemanticRule::Range3, "a return")),
    );
}

#[test]
fn a_conditional_take_back_preserves_the_selected_prefix() {
    check(
        "fn shrink(xs: &Slots<u64, 4>, flag: Bool) -> result: unit writes(xs) contract {
  requires 0_u64 < xs^.len;
  requires forall before(k in 0_u64..xs^.len): xs^[k] == 0_u64;
  ensures forall after(k in 0_u64..xs^.len): xs^[k] == 0_u64;
} {
  if flag {
    let removed = take_back(window: xs);
  }
  return unit;
}
",
        None,
    );
}

#[test]
fn a_branch_havoc_before_collection_discovery_does_not_revive_the_copy() {
    check("struct Row {
  payloads: Array<u64, 1>;
}

fn damage(xs: &Array<u64, 1>) -> result: unit writes(xs) {
  set xs^[0_u64] = 1_u64;
  return unit;
}

fn need(xs: &Array<u64, 1>) -> result: unit pure contract {
  requires forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;
} {
  return unit;
}

fn forward(rows: &[Row], flag: Bool) -> result: unit reads(rows) contract {
  requires forall held(b in 0_u64..rows^.len, k in 0_u64..rows^[b].payloads.len): rows^[b].payloads[k] == 0_u64;
} {
  if 0_u64 < rows^.len {
    let saved = rows^[0_u64];
    if flag {
      damage(xs: &saved.payloads);
    } else {
      need(xs: &saved.payloads);
    }
    need(xs: &saved.payloads);
  }
  return unit;
}
", Some((SemanticRule::Range3, "a call")));
}

fn processed_prefix(stored: &str) -> String {
    format!(
        "struct Block {{
  slot: u64;
}}

fn finish_sequence(order: &Slots<u64, 2>, blocks: &[Block], pending: &[u64]) -> result: unit reads(pending), writes(order), writes(blocks) contract {{
  requires order^.len == 0_u64;
  requires pending^.len <= order^.cap;
  requires forall pending_bounds(j in 0_u64..pending^.len): pending^[j] < blocks^.len;
  requires forall pending_inverse(j in 0_u64..pending^.len) when pending^[j] < blocks^.len: blocks^[pending^[j]].slot == j;
}} {{
  for (
    k in 0_u64..pending^.len,
    invariant length: order^.len == k,
    invariant forall prefix(j in 0_u64..k): order^[j] == pending^[j], blocks^[{stored}[j]].slot == j
  ) {{
    let target = pending^[k];
    place_back(window: order, value: target);
    set blocks^[target].slot = k;
  }}
  return unit;
}}
"
    )
}

// The prefix names the appended target through the store, so the target
// read pending[j] reaches the backedge problem only through the prefix
// instance, and RANGE-3 step 1 forms no pending_inverse instance from it.
#[test]
fn processed_prefix_through_the_store_lacks_the_second_inverse_instance() {
    check(
        &processed_prefix("order^"),
        Some((SemanticRule::Range3, "a loop back edge")),
    );
}

// Naming the target through pending puts pending[j] in the owed problem
// itself, so step 1 forms both inverse instances.
#[test]
fn processed_prefix_through_pending_closes_with_the_owed_reads() {
    check(&processed_prefix("pending^"), None);
}
