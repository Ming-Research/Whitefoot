use crate::SemanticOutcome;

use super::with_semantics;

#[test]
fn concrete_options_reuse_the_nominal_path_for_supported_payloads() {
    let source = br#"struct Pair {
  left: u32;
  right: u32;
}

fn scalar(value: i32) -> result: Option<i32> pure {
  return Some<i32>(value: value);
}

fn aggregate(value: Pair) -> result: Option<Pair> pure {
  return Some<Pair>(value: value);
}

fn nested() -> result: Option<Option<u8>> pure {
  let inner = Some<u8>(value: 7_u8);
  return Some<Option<u8>>(value: inner);
}

fn absent() -> result: Option<Pair> pure {
  return None<Pair>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("resource-free Option payloads must check: {outcome:?}");
        };
        let names = checked
            .data
            .nominals
            .iter()
            .map(|nominal| nominal.name.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "Option<i32>",
            "Option<Pair>",
            "Option<u8>",
            "Option<Option<u8>>",
        ] {
            assert!(
                names.contains(&expected),
                "missing concrete prelude nominal {expected}: {names:?}"
            );
        }
    });
}

/// The payload is a `Box` cell rather than the retired store-backed
/// `Vector<'s, u8>`. There is one heap [STOR-8], so a cell carries no store
/// brand, no provider parameter and no `allocates` row; its release is the
/// compiler-derived free at owner scope exit [STOR-1, STOR-3]. The subject is
/// unchanged: one `Option` instance carrying a payload whose release is
/// variant-dependent, and one drop on the return edge.
#[test]
fn option_of_a_resource_bearing_payload_uses_variant_dependent_cleanup() {
    let source = b"fn abandon(value: Option<Box<u64>>) -> result: unit pure {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("Option<Box<u64>> must check: {outcome:?}");
        };
        let nominal = checked
            .data
            .nominals
            .iter()
            .find(|nominal| nominal.name.starts_with("Option<"))
            .expect("concrete Option instance must be interned");
        let super::super::model::CheckedStatement::Return { drops, .. } =
            &checked.data.functions[0].body.as_deref().expect("WF body")[0]
        else {
            panic!("abandon must end in return");
        };
        assert_eq!(drops.len(), 1);
        assert_eq!(
            drops[0].ty,
            super::super::model::CheckedType::Nominal(nominal.id)
        );
    });
}
