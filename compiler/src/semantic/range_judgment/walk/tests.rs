//! Internal measure identity across affine header formation [RANGE-2].

use super::*;
use crate::semantic::tests::with_semantics;

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
            match_type,
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
        let branches = walker.arms(&mut state, scrutinee, *match_type, arms);
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

/// Integer arms all start at the same captured element version. The else
/// path adds one exclusion, and writing the element cannot change that fact
/// into an exclusion of the new value.
#[test]
fn integer_else_captures_one_set_before_an_element_write() {
    let source = br#"fn probe(cells: &Array<u64, 1>) -> result: unit writes(cells) {
  match cells^[0_u64] {
    0_u64 => {
    }
    1_u64 => {
    }
    2_u64 => {
    }
    else => {
      set cells^[0_u64] = 0_u64;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let crate::SemanticOutcome::Complete(program) = outcome else {
            panic!("integer match source must check: {outcome:?}");
        };
        let data = &program.data;
        let functions: Vec<_> = data.functions.iter().collect();
        let probe = data
            .executable_functions()
            .find(|function| function.name == "probe")
            .unwrap();
        let CheckedStatement::Match {
            scrutinee,
            match_type,
            arms,
            ..
        } = &probe.body.as_ref().unwrap()[0]
        else {
            panic!("expected integer match");
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
        let Value::Int(before) = walker.eval(&mut state, scrutinee) else {
            panic!("integer element");
        };
        // `arms` runs each body as well as establishing its selection. The
        // write's target evaluation also records the Array's known length,
        // so count selection conditions before running the body.
        let mut entry_arms = arms.clone();
        for arm in &mut entry_arms {
            arm.body.clear();
        }
        let branches = walker.arms(&mut state, scrutinee, *match_type, &entry_arms);
        assert_eq!(branches.len(), 4);
        for branch in &branches {
            assert_eq!(
                branch.conds.len(),
                state.conds.len() + 1,
                "each arm adds one selection; else never adds one disequality per label"
            );
        }
        let exclusions = branches
            .iter()
            .flat_map(|branch| &branch.conds)
            .filter(|literal| literal.relation == Relation::Excluded)
            .collect::<Vec<_>>();
        assert_eq!(exclusions.len(), 1, "one whole-set condition is captured");
        assert_eq!(exclusions[0].left, before);
        assert_eq!(exclusions[0].members(), &[0, 1, 2]);
        let mut otherwise = walker
            .block(branches.last().unwrap().clone(), &arms.last().unwrap().body)
            .expect("the actual else body continues after its write");
        let Value::Int(after) = walker.eval(&mut otherwise, scrutinee) else {
            panic!("integer element");
        };
        assert_ne!(before, after, "a write selects a new version");
        let mut old_units = otherwise.conds.clone();
        old_units.push(Literal::new(before, Relation::Equal, Linear::constant(0)));
        assert_eq!(
            facts::judge(
                &mut walker.world,
                &[],
                &[],
                &[],
                &[],
                Query {
                    units: old_units,
                    ..Query::default()
                }
            ),
            Ok(Verdict::Refuted),
            "old selection still excludes zero"
        );
        let mut new_units = otherwise.conds;
        new_units.push(Literal::new(after, Relation::Equal, Linear::constant(0)));
        assert_eq!(
            facts::judge(
                &mut walker.world,
                &[],
                &[],
                &[],
                &[],
                Query {
                    units: new_units,
                    ..Query::default()
                }
            ),
            Ok(Verdict::Open),
            "the write permits zero in the new version"
        );
    });
}
