//! Internal storage and measure identity, and retained range proof receipts.

use super::*;
use crate::semantic::tests::with_semantics;

#[test]
fn retaining_another_alias_case_unions_certificate_sites_for_that_loop_only() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let crate::SemanticOutcome::Complete(program) = outcome else {
            panic!("the fixture must check: {outcome:?}");
        };
        let data = &program.data;
        let functions: Vec<_> = data.functions.iter().collect();
        let main = data
            .executable_functions()
            .find(|function| function.name == "main")
            .unwrap();
        let prelude = BTreeSet::new();
        let mut walker = Walker::new(
            &functions,
            &data.nominals,
            &data.elements,
            main,
            Vec::new(),
            &data.constants,
            JudgmentScope::Concrete,
            &prelude,
        );
        let path = |component| NodePath {
            components: vec![component],
        };
        let first = CertifiedLoop {
            id: CheckedLoopId(0),
            node: path(0),
            writes: vec![path(1)],
            reads: vec![path(2)],
        };
        let other = CertifiedLoop {
            id: CheckedLoopId(1),
            node: path(3),
            writes: vec![path(4)],
            reads: vec![path(5)],
        };
        walker.retain_certificate(first);
        walker.retain_certificate(other.clone());
        walker.retain_certificate(CertifiedLoop {
            id: CheckedLoopId(0),
            node: path(0),
            writes: vec![path(6), path(1), path(6)],
            reads: vec![path(7), path(2), path(7)],
        });
        assert_eq!(
            walker.certified,
            vec![
                CertifiedLoop {
                    id: CheckedLoopId(0),
                    node: path(0),
                    writes: vec![path(1), path(6)],
                    reads: vec![path(2), path(7)],
                },
                other,
            ]
        );
    });
}

/// [RANGE-2] Replacing an Array with its copied value must change element
/// identity even when the copied container was already materialized.
#[test]
fn replacing_a_registered_container_with_its_copy_changes_identity_only() {
    let source = br#"fn probe(cell: &Array<u64, 1>) -> result: unit writes(cell) {
  let saved = cell^;
  set cell^ = saved;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let crate::SemanticOutcome::Complete(program) = outcome else {
            panic!("the ordinary whole-array copy must check: {outcome:?}");
        };
        let data = &program.data;
        let functions: Vec<_> = data.functions.iter().collect();
        let probe = data
            .executable_functions()
            .find(|function| function.name == "probe")
            .unwrap();
        let prelude = BTreeSet::new();
        let mut walker = Walker::new(
            &functions,
            &data.nominals,
            &data.elements,
            probe,
            Vec::new(),
            &data.constants,
            JudgmentScope::Concrete,
            &prelude,
        );
        let parameter = probe.parameters[0].binding;
        let location = Location::root(Origin::Parameter(parameter));
        let mut state = State::default();
        state
            .values
            .insert(parameter, Value::Ref(View::Place(location.clone())));
        let original = state
            .container(&mut walker.world, location.clone(), 1)
            .unwrap();
        let entry = state.version(&mut walker.world, original);
        for statement in &probe.body.as_ref().unwrap()[..2] {
            state = walker.statement(state, statement).unwrap();
        }
        let replacement = state.container(&mut walker.world, location, 1).unwrap();
        let exit = state.version(&mut walker.world, replacement);
        assert_ne!(entry, exit, "a copied replacement has a distinct version");
        for (projection, expected) in [
            (Vec::new(), Verdict::Refuted),
            (vec![CheckedRangeProjection::WholeElement], Verdict::Open),
        ] {
            let before = walker.world.read(
                entry,
                vec![Linear::constant(0)],
                projection.clone(),
                Some(IntegerType::U64),
            );
            let after = walker.world.read(
                exit,
                vec![Linear::constant(0)],
                projection,
                Some(IntegerType::U64),
            );
            let query = Query {
                units: vec![Literal::new(before, Relation::NotEqual, after)],
                ..Query::default()
            };
            assert_eq!(
                facts::judge(&mut walker.world, &[], &[], &[], &[], query),
                Ok(expected)
            );
        }
    });
}

#[test]
fn boxed_affine_header_uses_the_container_measure_and_its_generation() {
    // No modified index or range-derived bound is needed here: the source
    // checks independently of the range walk's representation of its header.
    // The assertions below inspect that representation directly.
    let source = br#"fn probe(a: &Box<Array<u64>>) -> result: u64 reads(a) contract {
  requires 0_u64 < a^.inner.len;
} {
  let pc = 0_u64;
  loop (
    invariant slot: pc < a^.inner.len
  ) {
    return a^.inner[0_u64];
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let crate::SemanticOutcome::Complete(program) = outcome else {
            panic!("the fixed-index source must check: {outcome:?}");
        };
        let data = &program.data;
        let functions: Vec<_> = data.functions.iter().collect();
        let probe = data
            .executable_functions()
            .find(|function| function.name == "probe")
            .unwrap();
        let body = probe.body.as_ref().unwrap();
        let CheckedStatement::Loop { invariants, .. } = &body[1] else {
            panic!("the second statement must be the loop");
        };
        let affine_measure = invariants[0]
            .relation
            .left
            .postorder()
            .chain(invariants[0].relation.right.postorder())
            .find(|expression| matches!(expression.kind, CheckedAffineExpressionKind::Measure(_)))
            .expect("the header must contain its boxed length measure");
        let CheckedAffineExpressionKind::Measure(measure) = &affine_measure.kind else {
            unreachable!();
        };
        let CheckedExpression::BufferMeasure { root, .. } = measure.as_ref() else {
            panic!("an affine runtime Array length must be a BufferMeasure");
        };
        assert_eq!(root.binding, probe.parameters[0].binding);
        assert!(matches!(
            root.path.as_slice(),
            [CheckedPlaceStep::BoxReferent(_)]
        ));

        let prelude = BTreeSet::new();
        let mut walker = Walker::new(
            &functions,
            &data.nominals,
            &data.elements,
            probe,
            Vec::new(),
            &data.constants,
            JudgmentScope::Concrete,
            &prelude,
        );
        let mut state = State::default();
        let parameter = Location::root(Origin::Parameter(root.binding));
        state
            .values
            .insert(root.binding, Value::Ref(View::Place(parameter.clone())));
        let container = state
            .container(&mut walker.world, parameter.child(Step::BoxContent), 1)
            .unwrap();
        let original = walker.world.measure(
            container,
            state.generation(container),
            CheckedMeasure::Length,
        );
        assert_eq!(
            walker.affine(&mut state, affine_measure).ok(),
            Some(original.clone()),
            "the affine header must name the same length as the element-read container"
        );
        assert_eq!(
            walker.affine(&mut state, affine_measure).ok(),
            Some(original.clone()),
            "repeated formation must reuse the same measure atom"
        );

        state.havoc_container(&mut walker.world, container, true);
        let changed = walker.world.measure(
            container,
            state.generation(container),
            CheckedMeasure::Length,
        );
        assert_ne!(
            original, changed,
            "forgetting the descriptor changes its measure"
        );
        assert_eq!(
            walker.affine(&mut state, affine_measure).ok(),
            Some(changed),
            "measure evaluation must follow the current descriptor generation"
        );
    });
}

/// [RANGE-2] the branch captures the selected Bool's tag before its body
/// writes it. The implicit else captures False on the unchanged image.
#[test]
fn bool_branches_capture_the_selected_version_before_a_field_write() {
    let source = br#"struct Node {
  live: Bool;
}

fn probe(nodes: &Slots<Node, 1>) -> result: unit writes(nodes) contract {
  requires nodes^.len == 1_u64;
} {
  if nodes^[0_u64].live {
    set nodes^[0_u64].live = False();
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let crate::SemanticOutcome::Complete(program) = outcome else {
            panic!("the independent fixed-index source must check: {outcome:?}");
        };
        let data = &program.data;
        let functions: Vec<_> = data.functions.iter().collect();
        let probe = data
            .executable_functions()
            .find(|function| function.name == "probe")
            .unwrap();
        let CheckedStatement::Match {
            scrutinee,
            enum_type,
            arms,
            ..
        } = &probe.body.as_ref().unwrap()[0]
        else {
            panic!("if must use the common branch representation");
        };
        let prelude = BTreeSet::new();
        let mut walker = Walker::new(
            &functions,
            &data.nominals,
            &data.elements,
            probe,
            Vec::new(),
            &data.constants,
            JudgmentScope::Concrete,
            &prelude,
        );
        let mut state = State::default();
        let parameter = Location::root(Origin::Parameter(probe.parameters[0].binding));
        state.values.insert(
            probe.parameters[0].binding,
            Value::Ref(View::Place(parameter)),
        );
        let Value::Bool(_, before) = walker.eval(&mut state, scrutinee) else {
            panic!("the condition must be a Bool selection");
        };
        let branches = walker.arms(&mut state, scrutinee, *enum_type, arms);
        assert_eq!(branches.len(), 2);
        for (index, mut branch) in branches.into_iter().enumerate() {
            let expected_tag = i128::from(index != 0);
            assert!(branch.conds.contains(&Literal::new(
                before.clone(),
                Relation::Equal,
                Linear::constant(expected_tag),
            )));
            let Value::Bool(_, after) = walker.eval(&mut branch, scrutinee) else {
                panic!("the condition remains a Bool selection");
            };
            if index == 0 {
                assert_ne!(before, after, "writing the field creates a new image");
                let mut units = branch.conds;
                units.push(Literal::new(after, Relation::NotEqual, Linear::constant(1)));
                assert_eq!(
                    facts::judge(
                        &mut walker.world,
                        &[],
                        &[],
                        &[],
                        &[],
                        Query {
                            units,
                            ..Query::default()
                        },
                    ),
                    Ok(Verdict::Refuted),
                    "the new tag is False even though the old branch tag was True"
                );
            } else {
                assert_eq!(before, after, "the implicit else leaves the image intact");
            }
        }
    });
}
