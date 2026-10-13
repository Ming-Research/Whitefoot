use super::*;

#[test]
fn disjoint_write_preservation_retains_replayable_proofs() {
    let source = include_str!(
        "../../../../../tests/conformance/cases/ent5-pos-disjoint-symbolic-index-write.wf"
    );
    let summary = accepted_entailment(source.as_bytes(), "probe");
    validate_derivations(&summary);
    assert!(summary.derivations.roots.iter().any(|root| matches!(
        root.kind,
        DerivationRootKind::FactPreservation { .. }
    )));
}

#[test]
fn literal_symbolic_write_preservation_retains_replayable_proofs() {
    let source = include_str!(
        "../../../../../tests/conformance/cases/ent5-pos-disjoint-literal-symbolic-index-write.wf"
    );
    let summary = accepted_entailment(source.as_bytes(), "probe");
    validate_derivations(&summary);
    assert!(summary.derivations.roots.iter().any(|root| matches!(
        root.kind,
        DerivationRootKind::FactPreservation { .. }
    )));
}
