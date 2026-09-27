//! The rendered record, pinned byte for byte for one representative of each
//! family a writer meets, in both formats.
//!
//! The per-rule sentences are pinned by `driver::pinned_sentences`; these
//! pin the shape around them: the summary line, the marked source line,
//! related positions, escaping, and the complete JSON object.

use super::{DiagnosticFormat, render_driver_failure};
use crate::{CompilationFailure, CompilerLimits, SourceInput, check};

/// The failure one source-only compilation must stop with.
fn stop(name: &str, source: &[u8]) -> CompilationFailure {
    check(&[SourceInput::new(name, source)], CompilerLimits::default())
        .expect_err("this fixture exists to be rejected")
}

/// No record a writer is shown names a compiler-internal identity.
fn assert_no_internal_identity(rendered: &str) {
    for identity in ["SourceId", "NodePath", "ByteOffset", "SyntaxCoordinate"] {
        assert!(!rendered.contains(identity), "{identity} in {rendered}");
    }
}

const BOUNDS: &[u8] =
    br#"fn drop_spaces(out: &[u8], src: &[u8]) -> kept: u64 reads(src), writes(out) contract {
  requires deref(out).len >= deref(src).len;
} {
  doc "Copies every byte of src that is not a space to the front of out.";
  let kept = 0_u64;
  let count = deref(src).len;
  for (at in 0_u64..count) {
    let byte = deref(src)[at];
    if byte == 32_u8 {
    } else {
      set deref(out)[kept] = byte;
      set kept = kept +wrap 1_u64;
    }
  }
  return kept;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

/// `kept` is a local the loop writes, so the repair offers the proof and
/// guard routes rather than a requirement, and no call returns it, so no
/// callee's `ensures` either [DIAG-1].
const OP4_FIX: &str = "`kept < deref(out).len` is not proved here: when facts that reach the access imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the access with `if kept < deref(out).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare";

#[test]
fn an_undischarged_subscript_prints_its_residual_under_a_marked_line() {
    let failure = stop("bounds.wf", BOUNDS);
    let expected = format!(
        "bounds.wf:11:21: error[OP-4]: UndischargedBoundsObligation
  source:       set deref(out)[kept] = byte;
  marker:                     ^^^^^^
  residual: kept < deref(out).len
  disposition: Unproved
  mechanical_fix: {OP4_FIX}"
    );
    assert_eq!(failure.to_string(), expected);
    assert_no_internal_identity(&expected);
}

/// The text form is lean; JSON is complete, with the envelope the text
/// leaves in its summary line and the byte interval it omits.
#[test]
fn the_json_record_is_complete_on_one_line() {
    let failure = stop("bounds.wf", BOUNDS);
    let expected = format!(
        r#"{{"rule":"OP-4","kind":"UndischargedBoundsObligation","category":"Source","stage":"Semantics","at":{{"file":"bounds.wf","line":11,"column":21}},"bytes":{{"start":377,"end":383}},"source":"      set deref(out)[kept] = byte;","detail":{{"residual":"kept < deref(out).len","disposition":"Unproved","mechanical_fix":"{OP4_FIX}"}}}}"#
    );
    let json = failure.render(DiagnosticFormat::Json);
    assert_eq!(json, expected);
    assert!(!json.contains('\n'));
    // One compiler executable renders one stop the same way every time.
    assert_eq!(
        stop("bounds.wf", BOUNDS).render(DiagnosticFormat::Json),
        json
    );
}

#[test]
fn a_call_requirement_names_the_callee_clause_by_its_position_and_text() {
    let source =
        br#"fn drop_spaces(out: &[u8], src: &[u8]) -> kept: u64 reads(src), writes(out) contract {
  requires deref(out).len >= deref(src).len;
} {
  doc "Copies every byte of src that is not a space to the front of out.";
  let kept = 0_u64;
  let count = deref(src).len;
  for (
    at in 0_u64..count,
    invariant behind: kept <= at
  ) {
    let byte = deref(src)[at];
    if byte == 32_u8 {
    } else {
      set deref(out)[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}

fn main() -> status: std::process::ExitStatus pure {
  let text = array_filled::<u8, 6>(value: 32_u8);
  let buffer = array_filled::<u8, 5>(value: 0_u8);
  let kept = drop_spaces(out: &buffer[0_u64..5_u64], src: &text[0_u64..6_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = stop("caller.wf", source);
    let expected = format!(
        "caller.wf:24:14: error[FN-8]: UndischargedCallRequirement
  source:   let kept = drop_spaces(out: &buffer[0_u64..5_u64], src: &text[0_u64..6_u64]);
  marker:              {}
  concrete_callee: drop_spaces
  requires_clause: caller.wf:2:3 \"requires deref(out).len >= deref(src).len;\"
  instantiated_goal: buffer[0..5].len >= text[0..6].len
  disposition: Refuted
  mechanical_fix: `buffer[0..5].len >= text[0..6].len` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values",
        "^".repeat(65)
    );
    assert_eq!(failure.to_string(), expected);
    assert_no_internal_identity(&expected);
    // The related position is an object with `at`, `bytes` and its own text.
    assert!(
        failure.render(DiagnosticFormat::Json).contains(
            r#""requires_clause":{"at":{"file":"caller.wf","line":2,"column":3},"bytes":{"start":89,"end":131},"text":"requires deref(out).len >= deref(src).len;"}"#
        ),
        "{}",
        failure.render(DiagnosticFormat::Json)
    );
}

/// A requirement of a supplied declaration is quoted from the record that
/// declares it: a PRE-1 record under a name no writer will try to open as a
/// file, and a host module's function from the standard library record
/// [PRE-2, MOD-10] that states it.
#[test]
fn a_supplied_requirement_is_quoted_from_its_declaring_record() {
    let prelude = br#"fn fill() -> result: u64 pure {
  let slots = slots_new::<u64, 0>();
  place_back(window: &slots, value: 1_u64);
  return slots.len;
}
"#;
    let detail = stop("fill.wf", prelude).detail();
    assert!(
        detail.contains(
            "requires_clause: <prelude>/place_back.wf:2:3 \"requires deref(window).len < deref(window).cap;\"\n"
        ),
        "{detail}"
    );
    let host = br#"fn walk(factory: &std::io::HandleFactory, root: &std::fs::DirectoryRead, name: &[u8]) -> result: u8 reads(root), reads(name), writes(factory) {
  match std::fs::open_file(factory: factory, root: root, name: name, start: 0_u64, end: 1_u64) {
    Ok(value: handle) => {
      std::fs::close_read(factory: factory, file: move handle);
    }
    Err(error: problem) => {
    }
  }
  return 0_u8;
}
"#;
    let detail = stop("walk.wf", host).detail();
    assert!(
        detail.contains(
            "requires_clause: std/fs/module.wfm:54:3 \"requires end <= deref(name).len;\"\n"
        ),
        "{detail}"
    );
}

/// Each node an FN-9 payload names prints its own text: the clause and the
/// selector, not the header line they sit on.
#[test]
fn a_postcondition_names_its_clause_and_selector_by_their_own_text() {
    let source = br#"fn unproved(value: i32, choose: Bool) -> result: i32 pure contract {
  ensures result == value;
} {
  if choose {
    return 0_i32;
  } else {
    return value;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = stop("ensures.wf", source);
    assert_eq!(
        failure.to_string(),
        r#"ensures.wf:5:5: error[FN-9]: UndischargedPostcondition
  source:     return 0_i32;
  marker:     ^^^^^^^^^^^^^
  concrete_function: unproved
  postcondition: ensures.wf:2:3 "ensures result == value;"
  conjunct: 0
  selector: ensures.wf:1:42 "result: i32"
  relation: 0 = value
  disposition: Unproved
  mechanical_fix: the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition the body proves"#
    );
    assert_eq!(
        failure.render(DiagnosticFormat::Json),
        r#"{"rule":"FN-9","kind":"UndischargedPostcondition","category":"Source","stage":"Semantics","at":{"file":"ensures.wf","line":5,"column":5},"bytes":{"start":118,"end":131},"source":"    return 0_i32;","detail":{"concrete_function":"unproved","postcondition":{"at":{"file":"ensures.wf","line":2,"column":3},"bytes":{"start":71,"end":95},"text":"ensures result == value;"},"conjunct":0,"selector":{"at":{"file":"ensures.wf","line":1,"column":42},"bytes":{"start":41,"end":52},"text":"result: i32"},"relation":"0 = value","disposition":"Unproved","mechanical_fix":"the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition the body proves"}}"#
    );
}

#[test]
fn a_failed_loop_invariant_names_its_obligation_and_required_relation() {
    let source =
        br#"fn drop_spaces(out: &[u8], src: &[u8]) -> kept: u64 reads(src), writes(out) contract {
  requires deref(out).len >= deref(src).len;
} {
  doc "Copies every byte of src that is not a space to the front of out.";
  let kept = 0_u64;
  let count = deref(src).len;
  for (
    at in 0_u64..count,
    invariant behind: kept <= at
  ) {
    let byte = deref(src)[at];
    if byte == 32_u8 {
    } else {
      set deref(out)[kept] = byte;
      set kept = kept +wrap 1_u64;
    }
  }
  return kept;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_eq!(
        stop("invariant.wf", source).to_string(),
        "invariant.wf:9:5: error[INV-1]: UndischargedLoopInvariant
  source:     invariant behind: kept <= at
  marker:     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  name: behind
  obligation: Backedge
  required_relation: kept <= (at + 1_u64)
  disposition: Unproved
  mechanical_fix: `behind` is not proved preserved at the next loop header: strengthen the invariant prefix, weaken or correct it, or establish in the body the facts from which every reachable fallthrough preserves it"
    );
}

/// A certificate part that carries use ordinals prints them as its fields,
/// not as a `Debug` spelling.
#[test]
fn a_failed_certificate_part_prints_its_use_ordinal_as_a_field() {
    let source = br#"fn increment(x: u8, middle: u8) -> result: u8 pure contract {
  requires middle <= 254_u8;
} {
  invariant upper_bound: x <= 254_u8 {
    use (x <= middle);
    use (middle <= 254_u8);
  }
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = stop("premise.wf", source);
    assert!(
        failure
            .detail()
            .contains("\nobligation: Premise {use_index: 0}\n"),
        "{failure}"
    );
    assert!(
        failure
            .render(DiagnosticFormat::Json)
            .contains(r#""obligation":{"kind":"Premise","use_index":0}"#),
        "{}",
        failure.render(DiagnosticFormat::Json)
    );
}

#[test]
fn a_grammar_rejection_quotes_the_expected_terminals_and_the_token_it_found() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n  let a = 42;\n  return std::process::exit_status(code: 0_u8);\n}\n";
    // A fixed terminal is quoted as its spelling; a token class is named.
    assert_eq!(
        stop("suffix.wf", source).to_string(),
        r#"suffix.wf:2:11: error[FORM-5]: UnexpectedToken
  source:   let a = 42;
  marker:           ^^
  expected: [IDENT, TYPEID, "pkg", "std", "&", "entry", "move", "if", "propagate", "match", literal, "musttail", OPNAME, "deref"]
  found: "42""#
    );
}

#[test]
fn a_canonical_rejection_quotes_the_trivia_and_marks_the_line_it_ends_in() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n    let a = 1_i32;\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = stop("indent.wf", source);
    assert_eq!(
        failure.to_string(),
        r#"indent.wf:2:1: error[FORM-2]: NonCanonicalTrivia
  source:     let a = 1_i32;
  marker: ^^^^
  expected: "\n  "
  found: "\n    ""#
    );
    // JSON carries the same escaped spelling the text form quotes.
    assert_eq!(
        failure.render(DiagnosticFormat::Json),
        r#"{"rule":"FORM-2","kind":"NonCanonicalTrivia","category":"Source","stage":"CanonicalSource","at":{"file":"indent.wf","line":2,"column":1},"bytes":{"start":52,"end":57},"source":"    let a = 1_i32;","detail":{"expected":"\\n  ","found":"\\n    "}}"#
    );
}

/// Source text is ASCII up to its first defect [DIAG-1], so a multi-byte
/// scalar on the quoted line is the defect itself or lies after it. The
/// defect is marked by one caret per character, not per byte, the line is
/// shown as written, and the field that quotes the defect escapes it; `bytes`
/// stays the exact interval.
#[test]
fn a_multi_byte_scalar_is_marked_as_one_character_and_escaped_where_quoted() {
    let source = "fn main() -> status: std::process::ExitStatus pure {\n  let x = \u{2192} caf\u{e9};\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = stop("utf8.wf", source.as_bytes());
    assert_eq!(
        failure.to_string(),
        "utf8.wf:2:11: error[FORM-1]: UnexpectedByte
  source:   let x = \u{2192} caf\u{e9};
  marker:           ^
  found: \"\\u{2192}\""
    );
    assert_eq!(
        failure.render(DiagnosticFormat::Json),
        "{\"rule\":\"FORM-1\",\"kind\":\"UnexpectedByte\",\"category\":\"Source\",\"stage\":\"Lexing\",\"at\":{\"file\":\"utf8.wf\",\"line\":2,\"column\":11},\"bytes\":{\"start\":63,\"end\":66},\"source\":\"  let x = \u{2192} caf\u{e9};\",\"detail\":{\"found\":\"\\\\u{2192}\"}}"
    );
}

/// A byte that is not a scalar is escaped both where the line is shown and
/// where it is quoted, and JSON carries the same spelling rather than a
/// replacement character.
#[test]
fn an_invalid_byte_is_escaped_in_both_formats() {
    let failure = stop(
        "byte.wf",
        b"fn main() -> status: std::process::ExitStatus pure {\n  let x = \xff;\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
    assert_eq!(
        failure.to_string(),
        "byte.wf:2:11: error[FORM-2]: InvalidUtf8
  source:   let x = \\xff;
  marker:           ^^^^
  found: \"\\xff\""
    );
    assert_eq!(
        failure.render(DiagnosticFormat::Json),
        r#"{"rule":"FORM-2","kind":"InvalidUtf8","category":"Source","stage":"Lexing","at":{"file":"byte.wf","line":2,"column":11},"bytes":{"start":63,"end":64},"source":"  let x = \\xff;","detail":{"found":"\\xff"}}"#
    );
}

/// A control or bidirectional formatting character later on the quoted line
/// is escaped, so the line cannot read differently from its bytes and the
/// marker stays under the defect.
#[test]
fn a_quoted_line_escapes_control_and_reordering_characters() {
    let source = "fn main() -> status: std::process::ExitStatus pure {\n  let x = @ \t\u{202e}y;\n  return std::process::exit_status(code: 0_u8);\n}\n";
    assert_eq!(
        stop("bidi.wf", source.as_bytes()).to_string(),
        "bidi.wf:2:11: error[FORM-3]: MissingLabelName
  source:   let x = @ \\t\\u{202e}y;
  marker:           ^
  found: \"@\""
    );
}

/// Zero-width characters, the byte-order mark, and the line and paragraph
/// separators are escaped on the quoted line too: each is invisible or breaks
/// the line in some reader. The first is the defect, so the marker covers its
/// printed escape.
#[test]
fn a_quoted_line_escapes_zero_width_and_separator_characters() {
    let source = "fn main() -> status: std::process::ExitStatus pure {\n  let x = \u{200b}y\u{2028}z\u{feff}\u{2029}\u{200d};\n  return std::process::exit_status(code: 0_u8);\n}\n";
    assert_eq!(
        stop("zero-width.wf", source.as_bytes()).to_string(),
        "zero-width.wf:2:11: error[FORM-1]: UnexpectedByte
  source:   let x = \\u{200b}y\\u{2028}z\\u{feff}\\u{2029}\\u{200d};
  marker:           ^^^^^^^^
  found: \"\\u{200b}\""
    );
}

/// A declaration origin points at the declared name and quotes the
/// declaration it belongs to, so the record shows what the other declaration
/// is rather than repeating the spelling it already names.
#[test]
fn a_declaration_origin_quotes_its_declaration() {
    let source = b"fn scale(factor: u64) -> result: u64 pure {\n  let factor = 2_u64;\n  return factor;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = stop("shadow.wf", source);
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with("shadow.wf:2:7: error[TYPE-6]: DeclarationCollision\n"),
        "{rendered}"
    );
    assert!(rendered.contains("\n  spelling: factor\n"), "{rendered}");
    assert!(
        rendered.contains(r#"origin: shadow.wf:1:10 "factor: u64""#),
        "{rendered}"
    );
    // JSON keeps the name's own coordinate in `at` and `bytes`.
    assert!(
        failure.render(DiagnosticFormat::Json).contains(
            r#""origin":{"at":{"file":"shadow.wf","line":1,"column":10},"bytes":{"start":9,"end":15},"text":"factor: u64"}"#
        ),
        "{}",
        failure.render(DiagnosticFormat::Json)
    );
}

#[test]
fn a_capability_stop_names_its_stage_and_never_cites_a_rule() {
    let failure = stop(
        "arms.wf",
        b"enum Flag {\n  A();\n  B();\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let flag = Flag::A();\n  match flag {\n    A() => {\n    }\n    A() => {\n    }\n    B() => {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with(
            "arms.wf:11:5: unsupported capability in Semantics: DuplicateMatchArm\n  source:     A() => {\n"
        ),
        "{rendered}"
    );
    assert!(!rendered.contains("error["), "{rendered}");
    assert_no_internal_identity(&rendered);
    assert!(
        failure.render(DiagnosticFormat::Json).starts_with(
            r#"{"kind":"DuplicateMatchArm","category":"Unsupported","stage":"Semantics","at":"#
        ),
        "{}",
        failure.render(DiagnosticFormat::Json)
    );
}

#[test]
fn a_compiler_facing_stop_keeps_its_payload_under_its_category_and_stage() {
    let failure = check(&[], CompilerLimits::default()).expect_err("no source record");
    assert_eq!(
        failure.to_string(),
        "whitefootc: invocation failure in SourceEnvelope: EmptySourceSequence
  payload: EmptySourceSequence"
    );
    assert_eq!(
        failure.render(DiagnosticFormat::Json),
        r#"{"kind":"EmptySourceSequence","category":"Invocation","stage":"SourceEnvelope","detail":{"payload":"EmptySourceSequence"}}"#
    );
}

#[test]
fn a_driver_stop_is_one_sentence_in_text_and_an_envelope_in_json() {
    let message = "cannot read \"missing.wf\": No such file or directory (os error 2)";
    assert_eq!(
        render_driver_failure("Invocation", message, DiagnosticFormat::Text),
        format!("whitefootc: {message}")
    );
    assert_eq!(
        render_driver_failure("Invocation", message, DiagnosticFormat::Json),
        r#"{"category":"Invocation","stage":"Driver","detail":{"message":"cannot read \"missing.wf\": No such file or directory (os error 2)"}}"#
    );
}
