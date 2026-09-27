//! Retained structural bodies are compared with an independent fresh check
//! after inventory and source-coordinate changes.

use super::with_resolved_semantics;
use crate::SemanticOutcome;
use crate::semantic::model::CheckedFunction;
use crate::semantic::places::CaptureId;
use crate::semantic::products::identity::SourceIdentities;
use crate::semantic::products::{IdentityKind, IdentityMap, Reader, Record, Writer};
use crate::syntax::views::SyntaxView;

#[test]
fn retained_products_rebind_declarations_occurrences_and_captures() {
    let original = b"fn keep(value: u64) -> result: u64 pure contract {\n  ensures result == value;\n} {\n  return value;\n}\n\nfn run(value: u64) -> result: u64 pure {\n  let kept = keep(value: value);\n  return kept;\n}\n";
    let mut changed =
        b"fn earlier(value: u8) -> result: u8 pure {\n  return value;\n}\n\n".to_vec();
    changed.extend_from_slice(original);
    with_resolved_semantics(original, |old_resolved, old| {
        let SemanticOutcome::Complete(old) = old else {
            panic!("{old:?}");
        };
        let old_view = SyntaxView::new(old_resolved.syntax()).expect("old syntax view");
        let old_sources = SourceIdentities::new(old_resolved, &old_view).expect("old identities");
        let mut function = old
            .data
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("run")
            .clone();
        // An imported structural body grants no old entailment conclusion.
        function.entailment = Default::default();
        let source_origin = old_resolved
            .declaration(function.parameters[0].declaration)
            .expect("parameter declaration")
            .origin()
            .clone();
        let node = old_view
            .node_with_path(source_origin.node())
            .expect("source node");
        let capture = CaptureId::source(u32::try_from(node.index()).expect("node index"));
        let mut writer = Writer::default();
        function.write(&mut writer);
        source_origin.write(&mut writer);
        capture.write(&mut writer);

        with_resolved_semantics(&changed, |new_resolved, new| {
            let SemanticOutcome::Complete(new) = new else {
                panic!("{new:?}");
            };
            let new_view = SyntaxView::new(new_resolved.syntax()).expect("current syntax view");
            let new_sources =
                SourceIdentities::new(new_resolved, &new_view).expect("current identities");
            let mut mapping = IdentityMap::new();
            for &(kind, previous) in &writer.identities {
                let current = if let Some(name) = old_sources.name((kind, previous)) {
                    let (current_kind, current) =
                        new_sources.resolve(&name).expect("current source identity");
                    assert_eq!(kind, current_kind);
                    current
                } else {
                    assert_eq!(kind, IdentityKind::Function);
                    let symbol = &old.data.functions[previous as usize].symbol;
                    new.data
                        .functions
                        .iter()
                        .find(|function| &function.symbol == symbol)
                        .expect("current callable")
                        .id
                        .0
                };
                mapping.insert((kind, previous), current);
            }
            let origin =
                |path: &crate::NodePath, role, subtoken| new_sources.origin(path, role, subtoken);
            let mut reader = Reader::new(&writer.bytes, &mapping).with_origins(&origin);
            let restored = CheckedFunction::read(&mut reader).expect("retained structural body");
            let restored_origin = crate::SourceOrigin::read(&mut reader).expect("current origin");
            let restored_capture = CaptureId::read(&mut reader).expect("current capture");
            assert!(reader.finished());
            let mut fresh = new
                .data
                .functions
                .iter()
                .find(|function| function.name == "run")
                .expect("fresh run")
                .clone();
            fresh.entailment = Default::default();
            assert_ne!(function.id, fresh.id, "the fixture must renumber callables");
            assert_ne!(
                function.declaration, fresh.declaration,
                "the fixture must renumber declarations"
            );
            assert_eq!(
                restored, fresh,
                "retained structure must match independent fresh checking"
            );
            let fresh_origin = new_resolved
                .declaration(fresh.parameters[0].declaration)
                .expect("fresh parameter")
                .origin();
            assert_ne!(source_origin.coordinate(), fresh_origin.coordinate());
            assert_eq!(&restored_origin, fresh_origin);
            let fresh_node = new_view
                .node_with_path(fresh_origin.node())
                .expect("current source node");
            assert_eq!(
                restored_capture,
                CaptureId::source(u32::try_from(fresh_node.index()).expect("node index"))
            );
            assert_ne!(
                capture, restored_capture,
                "the fixture must renumber source captures"
            );

            let mut missing = mapping.clone();
            missing.remove(&(IdentityKind::Function, function.id.0));
            assert!(CheckedFunction::read(&mut Reader::new(&writer.bytes, &missing)).is_none());
            for end in [0, 1, writer.bytes.len() / 2] {
                assert!(
                    CheckedFunction::read(&mut Reader::new(&writer.bytes[..end], &mapping))
                        .is_none()
                );
            }
        });
    });
}

#[test]
fn retained_affine_products_keep_deep_grouping_off_the_call_stack() {
    std::thread::Builder::new().stack_size(128 * 1024).spawn(|| {
        use crate::semantic::model::{CheckedAffineExpression, CheckedAffineExpressionKind, IntegerType};
        let leaf = |value| CheckedAffineExpression {
            node_path: crate::NodePath { components: Vec::new() },
            kind: CheckedAffineExpressionKind::Constant { value, ty: IntegerType::U64 },
        };
        let mut expression = leaf(7);
        for value in 0..10_000 {
            expression = CheckedAffineExpression {
                node_path: crate::NodePath { components: Vec::new() },
                kind: CheckedAffineExpressionKind::Subtract(Box::new(expression), Box::new(leaf(value))),
            };
        }
        let mut writer = Writer::default();
        expression.write(&mut writer);
        let mapping = IdentityMap::new();
        let mut reader = Reader::new(&writer.bytes, &mapping);
        let restored = CheckedAffineExpression::read(&mut reader).expect("deep retained expression");
        assert!(reader.finished());
        let mut postorder = restored.postorder();
        assert!(matches!(postorder.next().unwrap().kind, CheckedAffineExpressionKind::Constant { value: 7, .. }));
        for expected in 0..10_000 {
            assert!(matches!(postorder.next().unwrap().kind, CheckedAffineExpressionKind::Constant { value, .. } if value == expected));
            assert!(matches!(postorder.next().unwrap().kind, CheckedAffineExpressionKind::Subtract(_, _)));
        }
        assert!(postorder.next().is_none());
    }).unwrap().join().unwrap();
}
