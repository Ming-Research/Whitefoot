use super::{DiagnosticFormat, assert_no_internal_identity, stop};

const MAIN: &str = "\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

fn assert_clause(source: &str, clause: &str) -> crate::CompilationFailure {
    let failure = stop("clause.wf", source.as_bytes());
    let text = failure.to_string();
    assert!(
        text.contains("error[FN-9]: UndischargedPostcondition\n"),
        "{text}"
    );
    assert!(text.contains(&format!("\n  clause: {clause}\n")), "{text}");
    let json = failure.render(DiagnosticFormat::Json);
    assert!(json.contains(&format!("\"clause\":\"{clause}\"")), "{json}");
    assert_no_internal_identity(&text);
    failure
}

#[test]
fn a_failed_clause_keeps_its_written_comparison_beside_the_normalized_relation() {
    let source = format!("fn small() -> result: u64 pure contract {{\n  ensures result < 10_u64;\n}} {{\n  return 20_u64;\n}}\n{MAIN}");
    let failure = assert_clause(&source, "20_u64 < 10_u64");
    assert!(
        failure.to_string().contains("\n  relation: 0_u64 - 0_u64 <= -11_i64\n"),
        "{failure}"
    );
}

#[test]
fn clause_expansion_keeps_operand_order_and_arithmetic_structure() {
    let source = format!("fn larger(value: u64) -> result: u64 pure contract {{\n  define limit = 2_u64 + 3_u64;\n  ensures limit > result + 1_u64;\n}} {{\n  return value;\n}}\n{MAIN}");
    assert_clause(&source, "(2_u64 + 3_u64) > (value + 1_u64)");
}

#[test]
fn a_generic_clause_and_return_use_the_concrete_literal_type() {
    let source = "fn zero<T: Int>() -> result: T pure contract {\n  ensures result == 0_T;\n} {\n  return 1_T;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let value = zero::<u64>();\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = assert_clause(source, "1_u64 == 0_u64");
    assert!(
        failure.to_string().contains("concrete_function: zero::<u64>"),
        "{failure}"
    );
}

#[test]
fn a_routed_clause_substitutes_the_selected_constructor_field() {
    let source = format!("enum Choice {{\n  Number(value: u64);\n  Empty();\n}}\n\nfn small() -> result: Choice pure contract {{\n  ensures when result is Number(value: payload): payload < 10_u64;\n}} {{\n  return Choice::Number(value: 20_u64);\n}}\n{MAIN}");
    assert_clause(&source, "20_u64 < 10_u64");
}

#[test]
fn a_forwarded_route_names_the_returned_value_and_its_selected_field() {
    let source = format!("enum Choice {{\n  Number(value: u64);\n  Empty();\n}}\n\nfn small(value: Choice) -> result: Choice pure contract {{\n  ensures when result is Number(value: payload): payload < 10_u64;\n}} {{\n  return value;\n}}\n{MAIN}");
    assert_clause(&source, "value.Number.value < 10_u64");
}

#[test]
fn clause_and_selected_return_keep_character_literal_spellings() {
    let source = format!("enum Letter {{\n  Value(byte: u8);\n}}\n\nfn letter() -> result: Letter pure contract {{\n  define limit = 'a'_u8;\n  ensures when Value(byte: payload): payload < limit;\n}} {{\n  return Letter::Value(byte: 'z'_u8);\n}}\n{MAIN}");
    assert_clause(&source, "'z'_u8 < 'a'_u8");
}

#[test]
fn a_result_measure_preserves_its_projection_and_written_operator() {
    let source = format!("struct Packet {{\n  bytes: Array<u8, 2>;\n}}\n\nfn small(packet: Packet, limit: u64) -> result: Packet pure contract {{\n  ensures result.bytes.len < limit;\n}} {{\n  return packet;\n}}\n{MAIN}");
    assert_clause(&source, "packet.bytes.len < limit");
}

#[test]
fn a_clause_substitutes_each_named_result_ordinal() {
    let source = format!("fn pair() -> (first: u8, second: u8) pure contract {{\n  ensures first < second;\n}} {{\n  return 'z'_u8, 'a'_u8;\n}}\n{MAIN}");
    assert_clause(&source, "'z'_u8 < 'a'_u8");
}

#[test]
fn a_propagated_error_clause_names_the_call_and_error_field() {
    let source = include_str!(
        "../../../../../tests/conformance/cases/fn9-neg-written-err-propagated-route.wf"
    );
    assert_clause(source, "(fail(value: value)).Err.error <= 6_u64");
}

#[test]
fn a_consumed_aggregate_result_names_the_value_without_the_transfer_marker() {
    let source = format!("fn small(packet: Box<Array<u8, 2>>, limit: u64) -> result: Box<Array<u8, 2>> pure contract {{\n  ensures result.inner.len < limit;\n}} {{\n  return move packet;\n}}\n{MAIN}");
    assert_clause(&source, "packet.inner.len < limit");
}
