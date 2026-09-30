//! Postconditions over aggregate results: integer fields of struct and Box
//! results and of routed success payloads, and the `Some` route.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::postconditions::{assert_complete, assert_fn9_refuted, assert_fn9_unproved};
use super::{assert_rule, assert_rule_kind, with_semantics};

// [FN-9, CALL-4, MSR-3, ENT-2, ENT-5] v0.80: integer fields of struct and Box
// results and of routed success payloads, the `Some` route, widening
// conversions in relation terms, and the exit state of a written reference
// parameter's integer field.

const TABLE_PRELUDE: &str =
    "struct Atom {\n  index: u64;\n}\n\nstruct Table {\n  spans: Box<Slots<u32>>;\n}\n\n";

#[test]
fn a_struct_result_field_is_read_from_the_returned_construction_operand() {
    let source = format!(
        "{TABLE_PRELUDE}fn intern(table: &Table, key: u32) -> atom: Atom writes(table) contract {{
  requires table^.spans.inner.len < table^.spans.inner.cap;
  ensures atom.index < table^.spans.inner.len;
}} {{
  let at = table^.spans.inner.len;
  place_back(window: &table^.spans.inner, value: key);
  return Atom(index: at);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  let table = Table(spans: move spans);
  let atom = intern(table: &table, key: 9_u32);
  let span = table.spans.inner[atom.index];
  let code = cvt.wrap::<u32, u8>(span);
  return std::process::exit_status(code: code);
}}
"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn a_struct_result_field_without_its_relation_leaves_the_caller_subscript_unproved() {
    let source = format!(
        "{TABLE_PRELUDE}fn intern(table: &Table, key: u32) -> atom: Atom writes(table) contract {{
  requires table^.spans.inner.len < table^.spans.inner.cap;
}} {{
  let at = table^.spans.inner.len;
  place_back(window: &table^.spans.inner, value: key);
  return Atom(index: at);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  let table = Table(spans: move spans);
  let atom = intern(table: &table, key: 9_u32);
  let span = table.spans.inner[atom.index];
  let code = cvt.wrap::<u32, u8>(span);
  return std::process::exit_status(code: code);
}}
"
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |_| true);
}

#[test]
fn nested_and_returned_place_fields_are_selected_return_data() {
    let source = br#"struct Span {
  start: u64;
  end: u64;
}

struct Token {
  span: Span;
  kind: u64;
}

fn token(limit: u64) -> result: Token pure contract {
  ensures result.span.end == limit;
  ensures result.span.start == 0_u64;
  ensures result.kind == 7_u64;
} {
  let span = Span(start: 0_u64, end: limit);
  let made = Token(span: span, kind: 7_u64);
  return made;
}

fn main() -> status: std::process::ExitStatus pure {
  let made = token(limit: 12_u64);
  let code = cvt::<u64, u8>(made.span.end);
  return std::process::exit_status(code: code);
}
"#;
    assert_complete(source);
}

#[test]
fn a_box_content_integer_field_is_a_result_datum() {
    let source = br#"struct Counter {
  count: u8;
}

fn make_counter(start: u8) -> made: Box<Counter> pure contract {
  requires start <= 100_u8;
  ensures made.inner.count <= 100_u8;
} {
  let zero = Counter(count: 0_u8);
  let boxed = box_new::<Counter>(value: zero);
  set boxed.inner.count = start;
  return move boxed;
}

fn main() -> status: std::process::ExitStatus pure {
  let made = make_counter(start: 7_u8);
  let doubled = made.inner.count * 2_u8;
  return std::process::exit_status(code: doubled);
}
"#;
    assert_complete(source);
}

#[test]
fn a_bool_result_field_is_no_postcondition_datum() {
    let source = br#"struct Block {
  count: u64;
  sealed: Bool;
}

fn pass_block(block: Block) -> result: Block pure contract {
  ensures result.sealed == block.sealed;
} {
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::invalid_postcondition_selector(),
    );
}

#[test]
fn an_enum_payload_step_below_an_unrouted_result_is_no_datum() {
    let source = br#"enum Held {
  Full(count: u64);
  Empty();
}

struct Slot {
  held: Held;
  serial: u64;
}

fn pass_slot(slot: Slot) -> result: Slot pure contract {
  ensures result.held.Full.count == 3_u64;
} {
  return move slot;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::invalid_postcondition_selector(),
    );
}

#[test]
fn a_step_below_a_type_parameter_value_is_no_datum() {
    let source = br#"struct Counter {
  count: u64;
}

struct Holder<T> {
  item: T;
  serial: u64;
}

fn hold<T>(item: T) -> result: Holder<T> pure contract {
  ensures result.serial == 0_u64;
  ensures result.item.count == 0_u64;
} {
  let held = Holder<T>(item: move item, serial: 0_u64);
  return move held;
}

fn main() -> status: std::process::ExitStatus pure {
  let counter = Counter(count: 0_u64);
  let held = hold::<Counter>(item: counter);
  let code = cvt.wrap::<u64, u8>(held.serial);
  return std::process::exit_status(code: code);
}
"#;
    assert_rule_kind(source, SemanticRule::Fn9, |kind| {
        matches!(
            kind,
            SemanticIssueKind::InvalidPostconditionSelector { .. }
                | SemanticIssueKind::InvalidPostconditionRelation
        )
    });
}

#[test]
fn a_scaled_result_field_is_outside_the_relation_fragment() {
    let source = br#"struct Row {
  width: u64;
  stride: u64;
}

fn row(width: u64) -> result: Row pure contract {
  requires width <= 16384_u64;
  ensures result.stride == 4_u64 * result.width;
} {
  let stride = width * 4_u64;
  return Row(width: width, stride: stride);
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

const HEADER_PRELUDE: &str = "struct Header {\n  width: u32;\n  height: u32;\n}\n\nenum PngError {\n  Malformed();\n}\n\nfn parse_header(width: u32, height: u32) -> result: Result<Header, PngError> pure contract {\n  ensures when Ok(value: header): header.width >= 1_u32;\n  ensures when Ok(value: header): header.width <= 16384_u32;\n  ensures when Ok(value: header): header.height <= 16384_u32;\n} {\n  let width_ok = width >= 1_u32;\n  let width_small = width <= 16384_u32;\n  let height_small = height <= 16384_u32;\n  let sizes = band(width_ok, width_small);\n  let valid = band(sizes, height_small);\n  if valid {\n    let header = Header(width: width, height: height);\n    return Ok<Header, PngError>(value: header);\n  }\n  let error = PngError::Malformed();\n  return Err<Header, PngError>(error: error);\n}\n\n";

#[test]
fn a_routed_ok_struct_payload_field_reaches_the_match_arm() {
    let source = format!(
        "{HEADER_PRELUDE}fn main() -> status: std::process::ExitStatus pure {{
  let parsed = parse_header(width: 3_u32, height: 2_u32);
  match parsed {{
    Ok(value: header) => {{
      let width = header.width;
      let height = header.height;
      let stride = width * 4_u32;
      let bytes = stride * height;
      let code = cvt.wrap::<u32, u8>(bytes);
      return std::process::exit_status(code: code);
    }}
    Err(error: problem) => {{
      return std::process::exit_status(code: 1_u8);
    }}
  }}
}}
"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn a_routed_ok_struct_payload_field_survives_propagate_and_rebinding() {
    let source = format!(
        "{HEADER_PRELUDE}fn area(width: u32, height: u32) -> result: Result<u32, PngError> pure {{
  let header = propagate parse_header(width: width, height: height);
  let kept = header;
  let columns = kept.width;
  let rows = kept.height;
  let stride = columns * 4_u32;
  let bytes = stride * rows;
  return Ok<u32, PngError>(value: bytes);
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn a_forwarded_result_without_a_context_leaves_a_payload_field_unproved() {
    let source = br#"struct Header {
  width: u32;
}

enum PngError {
  Malformed();
}

fn plain_header(width: u32) -> result: Result<Header, PngError> pure {
  let header = Header(width: width);
  return Ok<Header, PngError>(value: header);
}

fn checked_header(width: u32) -> result: Result<Header, PngError> pure contract {
  ensures when Ok(value: header): header.width >= 1_u32;
} {
  let parsed = plain_header(width: width);
  return parsed;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_unproved(source);
}

#[test]
fn a_forwarded_result_with_a_context_proves_its_payload_field() {
    let source = format!(
        "{HEADER_PRELUDE}fn forwarded(width: u32, height: u32) -> result: Result<Header, PngError> pure contract {{
  ensures when Ok(value: header): header.width <= 16384_u32;
}} {{
  let parsed = parse_header(width: width, height: height);
  let kept = parsed;
  return kept;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn placements_carry_integer_field_values_that_are_terms() {
    let construction = br#"struct Header {
  width: u32;
  height: u32;
}

fn main() -> status: std::process::ExitStatus pure {
  let w = 7_u32;
  let header = Header(width: w, height: 1_u32);
  let wide = header.width * 4_u32;
  let code = cvt.wrap::<u32, u8>(wide);
  return std::process::exit_status(code: code);
}
"#;
    assert_complete(construction);

    let rebind = br#"struct Header {
  width: u32;
  height: u32;
}

fn main() -> status: std::process::ExitStatus pure {
  let header = Header(width: 0_u32, height: 1_u32);
  set header.width = 7_u32;
  let second = header;
  let copied = second.width * 4_u32;
  let code = cvt.wrap::<u32, u8>(copied);
  return std::process::exit_status(code: code);
}
"#;
    assert_complete(rebind);
}

#[test]
fn a_field_that_is_no_term_carries_only_its_type_bounds() {
    let source = br#"struct Header {
  width: u32;
  height: u32;
}

fn pass(header: Header) -> result: Header pure {
  return header;
}

fn main() -> status: std::process::ExitStatus pure {
  let header = Header(width: 7_u32, height: 1_u32);
  let back = pass(header: header);
  let wide = back.width * 4_u32;
  let code = cvt.wrap::<u32, u8>(wide);
  return std::process::exit_status(code: code);
}
"#;
    assert_rule_kind(source, SemanticRule::Op2, |_| true);
}

const RESULT_FIELD_CALLER: &str = "struct Atom {\n  index: u64;\n  serial: u64;\n}\n\nfn first(length: u64) -> atom: Atom pure contract {\n  requires length >= 1_u64;\n  ensures atom.index < length;\n} {\n  return Atom(index: 0_u64, serial: 0_u64);\n}\n\n";

#[test]
fn a_destination_field_write_ends_the_result_relation_and_a_sibling_write_keeps_it() {
    let sibling = format!(
        "{RESULT_FIELD_CALLER}fn main() -> status: std::process::ExitStatus pure {{
  let table = box_array_filled::<u8>(count: 4_u64, value: 5_u8);
  let atom = first(length: table.inner.len);
  set atom.serial = 9_u64;
  let byte = table.inner[atom.index];
  return std::process::exit_status(code: byte);
}}
"
    );
    assert_complete(sibling.as_bytes());

    let written = format!(
        "{RESULT_FIELD_CALLER}fn main() -> status: std::process::ExitStatus pure {{
  let table = box_array_filled::<u8>(count: 4_u64, value: 5_u8);
  let atom = first(length: table.inner.len);
  set atom.index = atom.serial;
  let byte = table.inner[atom.index];
  return std::process::exit_status(code: byte);
}}
"
    );
    assert_rule_kind(written.as_bytes(), SemanticRule::Op4, |_| true);
}

#[test]
fn set_and_destructuring_destinations_receive_the_result_field_relation() {
    let set_target = format!(
        "{RESULT_FIELD_CALLER}fn main() -> status: std::process::ExitStatus pure {{
  let table = box_array_filled::<u8>(count: 4_u64, value: 5_u8);
  let atom = Atom(index: 3_u64, serial: 0_u64);
  set atom = first(length: table.inner.len);
  let byte = table.inner[atom.index];
  return std::process::exit_status(code: byte);
}}
"
    );
    assert_complete(set_target.as_bytes());

    let destructured = br#"struct Header {
  width: u32;
  height: u32;
}

fn split(width: u32) -> (header: Header, frames: u32) pure contract {
  requires width <= 16384_u32;
  ensures header.width <= 16384_u32;
} {
  let header = Header(width: width, height: 1_u32);
  return header, 2_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  let (h, frames) = split(width: 5_u32);
  let stride = h.width * 4_u32;
  let total = stride +wrap frames;
  let code = cvt.wrap::<u32, u8>(total);
  return std::process::exit_status(code: code);
}
"#;
    assert_complete(destructured);
}

#[test]
fn indexed_and_consumed_destinations_keep_no_result_field_relation() {
    let indexed = format!(
        "{RESULT_FIELD_CALLER}fn main() -> status: std::process::ExitStatus pure {{
  let table = box_array_filled::<u8>(count: 4_u64, value: 5_u8);
  let blank = Atom(index: 0_u64, serial: 0_u64);
  let atoms = array_filled::<Atom, 2>(value: blank);
  set atoms[0_u64] = first(length: table.inner.len);
  let back = atoms[0_u64];
  let byte = table.inner[back.index];
  return std::process::exit_status(code: byte);
}}
"
    );
    assert_rule_kind(indexed.as_bytes(), SemanticRule::Op4, |_| true);

    let consumed = format!(
        "{RESULT_FIELD_CALLER}fn pass(atom: Atom) -> result: Atom pure {{
  return atom;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let table = box_array_filled::<u8>(count: 4_u64, value: 5_u8);
  let atom = first(length: table.inner.len);
  let back = pass(atom: atom);
  let byte = table.inner[back.index];
  return std::process::exit_status(code: byte);
}}
"
    );
    assert_rule_kind(consumed.as_bytes(), SemanticRule::Op4, |_| true);
}

#[test]
fn a_widening_conversion_denotes_its_operand_in_a_relation() {
    let postcondition = format!(
        "{TABLE_PRELUDE}fn last_index(table: &Table) -> index: u32 reads(table) contract {{
  requires table^.spans.inner.len >= 1_u64;
  requires table^.spans.inner.len <= 4294967296_u64;
  ensures cvt::<u32, u64>(index) < table^.spans.inner.len;
}} {{
  let count = table^.spans.inner.len;
  let last = count - 1_u64;
  let index = cvt::<u64, u32>(last);
  return index;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  place_back(window: &spans.inner, value: 5_u32);
  let table = Table(spans: move spans);
  let index = last_index(table: &table);
  let wide = cvt::<u32, u64>(index);
  let span = table.spans.inner[wide];
  let code = cvt.wrap::<u32, u8>(span);
  return std::process::exit_status(code: code);
}}
"
    );
    assert_complete(postcondition.as_bytes());

    let requirement = br#"struct NodeId {
  index: u32;
}

fn kind_of(nodes: &Box<Slots<u8>>, node: NodeId) -> kind: u8 reads(nodes) contract {
  requires cvt::<u32, u64>(node.index) < nodes^.inner.len;
} {
  let at = cvt::<u32, u64>(node.index);
  let kind = nodes^.inner[at];
  return kind;
}

fn main() -> status: std::process::ExitStatus pure {
  let nodes = box_slots_new::<u8>(capacity: 4_u64);
  place_back(window: &nodes.inner, value: 3_u8);
  let root = NodeId(index: 0_u32);
  let kind = kind_of(nodes: &nodes, node: root);
  return std::process::exit_status(code: kind);
}
"#;
    assert_complete(requirement);
}

#[test]
fn a_narrowing_conversion_is_no_relation_term() {
    let source = br#"fn low_half(value: u64) -> result: u32 pure contract {
  ensures result == cvt::<u64, u32>(value);
} {
  let low = cvt.wrap::<u64, u32>(value);
  return low;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::SourceIssue { .. }),
            "a value-dependent conversion must not form a relation: {outcome:?}"
        );
    });
}

#[test]
fn a_written_parameter_integer_field_denotes_its_exit_state() {
    let source = br#"struct Runs {
  count: u64;
  classes: Box<Slots<u8>>;
}

fn push_run(runs: &Runs, class: u8) -> result: unit writes(runs) contract {
  requires runs^.count == runs^.classes.inner.len;
  requires runs^.classes.inner.len < runs^.classes.inner.cap;
  ensures runs^.count == runs^.classes.inner.len;
  ensures runs^.count == entry(runs)^.count + 1_u64;
  ensures runs^.classes.inner.cap == entry(runs)^.classes.inner.cap;
} {
  place_back(window: &runs^.classes.inner, value: class);
  set runs^.count = runs^.count + 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let classes = box_slots_new::<u8>(capacity: 4_u64);
  let runs = Runs(count: 0_u64, classes: move classes);
  push_run(runs: &runs, class: 7_u8);
  push_run(runs: &runs, class: 8_u8);
  let code = cvt.wrap::<u64, u8>(runs.count);
  return std::process::exit_status(code: code);
}
"#;
    assert_complete(source);
}

#[test]
fn an_entry_qualified_integer_field_is_the_old_value() {
    let source = br#"struct Counter {
  count: u64;
}

fn bump(counter: &Counter) -> result: unit writes(counter) contract {
  requires counter^.count < 100_u64;
  ensures counter^.count == entry(counter)^.count;
} {
  set counter^.count = counter^.count + 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_refuted(source);
}

const FIND_SPACE: &str = "fn find_space(data: &[u8]) -> result: Option<u64> reads(data) contract {\n  ensures when Some(value: found): found < data^.len;\n} {\n  let length = data^.len;\n  let pos = 0_u64;\n  loop (\n    invariant bounded: pos <= length\n  ) {\n    if pos >= length {\n      break;\n    }\n    let byte = data^[pos];\n    if byte == 32_u8 {\n      return Some<u64>(value: pos);\n    }\n    set pos = pos + 1_u64;\n  }\n  return None<u64>();\n}\n\n";

#[test]
fn a_some_route_reaches_a_caller_match_and_a_forwarded_option() {
    let matched = format!(
        "{FIND_SPACE}fn main() -> status: std::process::ExitStatus pure {{
  let backing = box_array_filled::<u8>(count: 3_u64, value: 32_u8);
  let text = &backing.inner[0_u64..3_u64];
  let outcome = find_space(data: text);
  match outcome {{
    Some(value: at) => {{
      let byte = text^[at];
      return std::process::exit_status(code: byte);
    }}
    None() => {{
      return std::process::exit_status(code: 1_u8);
    }}
  }}
}}
"
    );
    assert_complete(matched.as_bytes());

    let forwarded = format!(
        "{FIND_SPACE}fn find_again(data: &[u8]) -> result: Option<u64> reads(data) contract {{
  ensures when Some(value: found): found < data^.len;
}} {{
  let outcome = find_space(data: data);
  let kept = outcome;
  return kept;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    assert_complete(forwarded.as_bytes());
}

#[test]
fn some_route_refusals() {
    let none_route = br#"fn nothing(limit: u64) -> result: Option<u64> pure contract {
  ensures when None(value: gone): gone < limit;
} {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        none_route,
        SemanticRule::Fn9,
        SemanticIssueKind::invalid_postcondition_selector(),
    );

    let unproved = br#"fn find(limit: u64) -> result: Option<u64> pure contract {
  ensures when Some(value: found): found < limit;
} {
  return Some<u64>(value: limit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_refuted(unproved);

    let only_none = br#"fn never(limit: u64) -> result: Option<u64> pure contract {
  ensures when Some(value: found): found < limit;
} {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(only_none, SemanticRule::Fn9, |kind| {
        matches!(kind, SemanticIssueKind::NoSelectedNormalExit { .. })
    });
}

#[test]
fn a_some_route_carries_an_option_struct_payload_field() {
    let source = br#"struct Atom {
  index: u32;
}

fn lookup(spans: &Box<Slots<u32>>, wanted: u32) -> found: Option<Atom> reads(spans) contract {
  ensures when Some(value: atom): cvt::<u32, u64>(atom.index) < spans^.inner.len;
} {
  let count = spans^.inner.len;
  if count == 0_u64 {
    return None<Atom>();
  }
  let made = Atom(index: 0_u32);
  return Some<Atom>(value: made);
}

fn main() -> status: std::process::ExitStatus pure {
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  place_back(window: &spans.inner, value: 5_u32);
  match lookup(spans: &spans, wanted: 5_u32) {
    Some(value: atom) => {
      let at = cvt::<u32, u64>(atom.index);
      let span = spans.inner[at];
      let code = cvt.wrap::<u32, u8>(span);
      return std::process::exit_status(code: code);
    }
    None() => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#;
    assert_complete(source);
}

#[test]
fn a_struct_result_publishes_its_boxed_slots_field_length() {
    let source = |length: &str| {
        format!(
            "struct Slab {{
  cells: Box<Slots<u8>>;
}}

fn make() -> result: Slab pure contract {{
  ensures result.cells.inner.len == {length};
}} {{
  let cells = box_slots_new::<u8>(capacity: 4_u64);
  return Slab(cells: move cells);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let slab = make();
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_complete(source("0_u64").as_bytes());
    assert_fn9_refuted(source("1_u64").as_bytes());
}
