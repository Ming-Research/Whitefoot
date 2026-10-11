//! Internal measure identity across affine header formation [RANGE-2].

use super::*;
use crate::semantic::tests::with_semantics;

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
