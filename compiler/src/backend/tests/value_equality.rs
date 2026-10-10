//! Typed active-payload reads and bounded code for nested aggregate equality.

use super::system::with_ir;
use super::{compile, compile_and_run, emitted_function, nominal_type};
use crate::{IrType, target::is_union_enum};

#[test]
fn fieldless_opaque_equality_is_constant_and_compares_no_representation() {
    let source =
        include_str!("../../../../tests/conformance/cases/op16-pos-fieldless-opaque-struct.wf");
    let llvm = compile(source.as_bytes());
    for (name, predicate) in [("same", "eq"), ("different", "ne")] {
        let body = emitted_function(&llvm, name);
        assert!(
            body.contains(&format!("icmp {predicate} i1 false, false")),
            "fieldless equality is constant: {body}"
        );
        assert!(
            !body.contains("icmp eq i128") && !body.contains("icmp ne i128"),
            "the opaque representation is not compared: {body}"
        );
    }

    // An array reaches the recursive comparison of the same opaque leaf.
    let nested = source.replace(
        "left: EqEmpty, right: EqEmpty",
        "left: Array<EqEmpty, 2>, right: Array<EqEmpty, 2>",
    );
    assert_ne!(nested, source);
    let llvm = compile(nested.as_bytes());
    for (name, equal) in [("same", true), ("different", false)] {
        let body = emitted_function(&llvm, name);
        assert!(body.contains("icmp ult i64"), "array traversal: {body}");
        assert!(
            body.contains(&format!("phi i1 [{equal},")),
            "the complete comparison has the requested polarity: {body}"
        );
        assert!(
            !body.contains("icmp eq") && !body.contains("icmp ne"),
            "a fieldless element has no equality guard: {body}"
        );
    }
}

#[test]
fn fielded_opaque_equality_compares_its_typed_fields() {
    let source = include_bytes!("../../../../tests/conformance/cases/op16-pos-opaque-struct.wf");
    let llvm = compile(source);
    let body = emitted_function(&llvm, "compare");
    assert!(body.contains("load i32,"), "read the u32 field: {body}");
    assert!(
        body.contains("icmp eq i32"),
        "compare the u32 field: {body}"
    );
}

const SOURCE: &[u8] = br#"enum EqualityPayload {
  EqualityEmpty();
  EqualityWide(first: u64, second: u64, flag: Bool);
  EqualityNarrow(value: u8);
}

struct EqualityEnvelope {
  prefix: u8;
  payload: EqualityPayload;
}

fn equal_payload(left: EqualityPayload, right: EqualityPayload) -> result: Bool pure {
  return left == right;
}

fn unequal_payload(left: EqualityPayload, right: EqualityPayload) -> result: Bool pure {
  return left != right;
}

fn equal_envelopes(left: Array<EqualityEnvelope, 32>, right: Array<EqualityEnvelope, 32>) -> result: Bool pure {
  return left == right;
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let first = EqualityPayload::EqualityWide(first: 11_u64, second: 19_u64, flag: flag);
  let same = EqualityPayload::EqualityWide(first: 11_u64, second: 19_u64, flag: flag);
  let other = EqualityPayload::EqualityWide(first: 11_u64, second: 20_u64, flag: flag);
  let narrow = EqualityPayload::EqualityNarrow(value: 11_u8);
  let equal = equal_payload(left: first, right: same);
  let different = unequal_payload(left: first, right: other);
  let different_tag = unequal_payload(left: first, right: narrow);
  if equal {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  if different {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  if different_tag {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  let envelope = EqualityEnvelope(prefix: 5_u8, payload: first);
  let changed = EqualityEnvelope(prefix: 5_u8, payload: other);
  let left = array_filled::<EqualityEnvelope, 32>(value: envelope);
  let right = array_filled::<EqualityEnvelope, 32>(value: envelope);
  let arrays_equal = equal_envelopes(left: left, right: right);
  if arrays_equal {
  } else {
    return std::process::exit_status(code: 4_u8);
  }
  set right[31_u64] = changed;
  let arrays_differ = equal_envelopes(left: left, right: right);
  if arrays_differ {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn union_equality_uses_typed_variant_views_and_nested_array_loops() {
    with_ir(SOURCE, |program| {
        let nominal = program
            .nominals()
            .iter()
            .find(|nominal| nominal.name() == "EqualityPayload")
            .expect("payload nominal");
        assert!(
            is_union_enum(program.nominals(), program.elements(), nominal.id()).expect("layout")
        );
        assert!(
            crate::target::is_memory_only(
                program.nominals(),
                program.elements(),
                IrType::Nominal(nominal.id())
            )
            .expect("memory-only layout")
        );
    });
    let llvm = compile(SOURCE);
    let payload = nominal_type("EqualityPayload");
    for name in ["equal_payload", "unequal_payload", "equal_envelopes"] {
        let body = emitted_function(&llvm, name);
        assert!(body.contains("switch i32"), "{body}");
        assert!(
            body.contains(&format!("getelementptr inbounds {payload}.v1,")),
            "{body}"
        );
        assert!(
            body.contains(&format!("getelementptr inbounds {payload}.v2,")),
            "{body}"
        );
        assert!(
            body.contains("load i1,"),
            "the Bool payload uses its own type: {body}"
        );
        assert!(
            !body.contains(&format!("load {payload},")),
            "no union carrier load: {body}"
        );
        assert!(!body.contains("memcmp"), "no padding comparison: {body}");
        assert!(!body.contains("@llvm.trap"), "total operation: {body}");
        assert!(body.contains("phi i1"), "one complete result: {body}");
    }
    let array = emitted_function(&llvm, "equal_envelopes");
    assert!(
        array.contains("phi i64") && array.contains("icmp ult i64"),
        "{array}"
    );
    let larger = String::from_utf8(SOURCE.to_vec())
        .expect("UTF-8")
        .replace("Envelope, 32", "Envelope, 256");
    let larger = compile(larger.as_bytes());
    assert_eq!(
        array.lines().count(),
        emitted_function(&larger, "equal_envelopes").lines().count(),
        "array extent must not unroll aggregate comparisons"
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
