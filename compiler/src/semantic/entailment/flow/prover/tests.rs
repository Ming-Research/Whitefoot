//! Internal proof-route and aggregate-evidence checks. Source verdicts live
//! in conformance; these tests isolate finite normalization from the later
//! affine-result fallback so that bypassing the shared dispatcher fails.

use super::*;

fn with_analyzer(check: impl FnOnce(&mut Analyzer<'_, '_>)) {
    let constant_ids = HashMap::new();
    let const_parameter_types = HashMap::new();
    let context = EntailmentContext {
        declarations: &[],
        callees: &[],
        constants: &[],
        constant_ids: &constant_ids,
        const_parameter_types: &const_parameter_types,
        nominals: &[],
        elements: &[],
        contract_queries: &[],
        verified_postconditions: &[],
        verified_postcondition_proofs: &[],
        binding_names: &[],
    };
    let function = CheckedFunction {
        formal_hypothesis: false,
        id: crate::semantic::model::FunctionId(0),
        declaration: crate::DeclarationId::from_index(0).unwrap(),
        module: crate::ModuleId::BUNDLE_ROOT,
        name: String::new(),
        symbol: String::new(),
        function_actuals: Vec::new(),
        region_parameters: Vec::new(),
        parameters: Vec::new(),
        result_mode: CheckedMode::Own,
        result: CheckedType::Unit,
        declared_state_writes: Vec::new(),
        declared_state_reads: Vec::new(),
        requirements: Vec::new(),
        requirement_places: Vec::new(),
        postconditions: Vec::new(),
        range_facts: Default::default(),
        body: None,
        reference_origins: Vec::new(),
        body_disposition: Default::default(),
        allocates: false,
        call_separations: Vec::new(),
        permission_separation_queries: Vec::new(),
        waiting: crate::semantic::model::CheckedWaiting::default(),
        obligations: Vec::new(),
        entailment: FunctionEntailment::default(),
    };
    let mut analyzer = Analyzer::new(&context, &function);
    check(&mut analyzer);
}

#[test]
fn finite_arithmetic_components_use_current_images_in_component_order() {
    use CheckedIntegerOperation::{
        AddExact, MultiplyExact, ShiftLeftExact, ShiftRightExact, SubtractExact,
    };
    // Operation, operand type, constant, constant first, current value, bad value.
    let cases = [
        (AddExact, IntegerType::U64, 4, false, 8, u64::MAX as i128),
        (AddExact, IntegerType::U64, 4, true, 8, u64::MAX as i128),
        (SubtractExact, IntegerType::U64, 4, false, 8, 3),
        (SubtractExact, IntegerType::U64, 20, true, 8, 21),
        (
            MultiplyExact,
            IntegerType::U64,
            2,
            false,
            8,
            u64::MAX as i128,
        ),
        (
            MultiplyExact,
            IntegerType::I64,
            -2,
            true,
            8,
            i64::MIN as i128,
        ),
        (
            AddExact,
            IntegerType::I64,
            -4,
            false,
            i64::MIN as i128 + 4,
            i64::MIN as i128 + 3,
        ),
        (
            SubtractExact,
            IntegerType::I64,
            4,
            false,
            i64::MIN as i128 + 4,
            i64::MIN as i128 + 3,
        ),
        (ShiftLeftExact, IntegerType::U64, 1, true, 8, 64),
        (ShiftRightExact, IntegerType::U64, 1, true, 8, 64),
    ];
    for (operation, ty, constant, constant_first, value, bad_value) in cases {
        with_analyzer(|analyzer| {
            let binding = BindingId(0);
            let value_ty = if matches!(operation, ShiftLeftExact | ShiftRightExact) {
                IntegerType::U32
            } else {
                ty
            };
            let term = analyzer
                .vocabulary
                .terms
                .intern(TermKind::Place(ResolvedPlace::binding(binding), value_ty));
            let variable = IntegerDomainOperand {
                term: Some(term),
                constant: None,
            };
            let literal = IntegerDomainOperand {
                term: Some(
                    analyzer
                        .vocabulary
                        .terms
                        .intern(TermKind::Constant(constant)),
                ),
                constant: Some(constant),
            };
            let operands = if constant_first {
                [literal, variable]
            } else {
                [variable, literal]
            };
            let operand_type = CheckedType::Integer(ty);
            let plan = analyzer
                .vocabulary
                .integer_domain_plan(operation, operand_type, &operands)
                .unwrap();
            let goal = IntegerDomainGoal {
                canonical: None,
                operation,
                operand_type,
                components: &plan.components,
                affine_clauses: None,
                affine_product: None,
            };
            let facts = FactState::new();
            let mut affine = AffineFlowState::default();
            affine.values.insert(binding, AffineForm::constant(value));
            let result = analyzer
                .reasoning()
                .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
            assert_eq!(
                result.disposition,
                ProofDisposition::Proved,
                "{operation:?} {ty:?}"
            );
            assert_eq!(result.route, Some(ProofRoute::Affine));
            let DerivationNode::IntegerDomain { parents, .. } =
                &analyzer.vocabulary.derivations.nodes[result.derivation.unwrap().0 as usize]
            else {
                panic!("finite components must retain one aggregate root");
            };
            assert_eq!(parents.len(), plan.components.len());
            for (parent, request) in parents.iter().zip(&plan.components) {
                if let DerivationNode::AffineConsequence { relation, .. } =
                    &analyzer.vocabulary.derivations.nodes[parent.0 as usize]
                {
                    assert_eq!(relation.as_deref(), request_relation(request).as_ref());
                }
            }
            // A successful query publishes no fact usable with another image.
            affine
                .values
                .insert(binding, AffineForm::constant(bad_value));
            let failed = analyzer
                .reasoning()
                .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
            assert_ne!(
                failed.disposition,
                ProofDisposition::Proved,
                "{operation:?} {ty:?}"
            );
            assert!(failed.derivation.is_none());
        });
    }
}

#[test]
fn finite_lower_component_uses_the_original_right_term_for_step_six() {
    with_analyzer(|analyzer| {
        let mut affine = AffineFlowState::default();
        let old = analyzer.vocabulary.terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            ResolvedPlace::binding(BindingId(0)),
        ));
        let current = analyzer.vocabulary.terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            ResolvedPlace::binding(BindingId(1)),
        ));
        let old_image = analyzer.vocabulary.measure_atom(old, &affine);
        let _ = analyzer.vocabulary.measure_atom(current, &affine);
        let fp = analyzer.vocabulary.new_affine_atom(IntegerType::U64);
        affine.facts.push(ActiveAffineFact {
            inequality: AffineInequality::from_bounded_forms(
                &fp,
                &old_image,
                -4,
                &mut AffineCheckState::new(),
            )
            .unwrap(),
            evidence: AffineFactEvidence::Source(SourceAffineFactRef::LoopInvariant(
                SourceLoopInvariantRef {
                    loop_id: CheckedLoopId(0),
                    source_ordinal: 0,
                },
            )),
        });
        let mut facts = FactState::new();
        let event = analyzer
            .vocabulary
            .derivations
            .event(FlowEventKind::S1, None);
        facts.establish(
            &Relation::Equal {
                left: old,
                right: current,
                difference: 0,
            },
            &mut analyzer.vocabulary.derivations,
            event,
        );
        let four = analyzer.vocabulary.terms.intern(TermKind::Constant(4));
        let operation = CheckedIntegerOperation::SubtractExact;
        let operand_type = CheckedType::Integer(IntegerType::U64);
        let plan = analyzer
            .vocabulary
            .integer_domain_plan(
                operation,
                operand_type,
                &[
                    IntegerDomainOperand {
                        term: Some(current),
                        constant: None,
                    },
                    IntegerDomainOperand {
                        term: Some(four),
                        constant: Some(4),
                    },
                ],
            )
            .unwrap();
        let goal = IntegerDomainGoal {
            canonical: None,
            operation,
            operand_type,
            components: &plan.components,
            affine_clauses: None,
            affine_product: None,
        };
        let result = analyzer
            .reasoning()
            .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
        assert_eq!(result.disposition, ProofDisposition::Proved);
        let DerivationNode::IntegerDomain { parents, .. } =
            &analyzer.vocabulary.derivations.nodes[result.derivation.unwrap().0 as usize]
        else {
            panic!("one integer-domain root must retain both components");
        };
        assert_eq!(parents.len(), 2);
        assert!(matches!(
            analyzer.vocabulary.derivations.nodes[parents[1].0 as usize],
            DerivationNode::TransitiveBound { left: ZERO, middle, right, bound: -4, .. }
                if middle == old && right == current
        ));
        let mut unavailable = plan.components[1];
        unavailable.left = None;
        let unknown = analyzer
            .reasoning()
            .prove_finite_component(ProofContext::new(&facts, &affine), unavailable);
        assert_eq!(unknown.disposition, ProofDisposition::Unknown);
        assert!(unknown.derivation.is_none());
    });
}
