//! Snapshots of by-value aggregate operands in range clauses [RANGE-1].

use super::*;

impl Walker<'_> {
    /// Keep both explicit construction fields and lazy element/copy sources.
    /// A frame stores this immutable version, so later mutation cannot change
    /// a projected parameter value or an already active fact.
    pub(super) fn snapshot_aggregate(
        &mut self,
        state: &mut State,
        value: &Value,
    ) -> super::super::world::VersionId {
        if let Value::Owned(location) = value {
            let location = state.resolve(location);
            if state.read_source(&location).is_none()
                && matches!(state.slots.get(&location), None | Some(Slot::Unknown))
            {
                let version = self.world.new_version(VersionDef::Initial);
                state.slots.insert(
                    location,
                    Slot::Read(ReadSource {
                        version,
                        indices: Vec::new(),
                        projection: Vec::new(),
                    }),
                );
            }
        }
        let mut stored = BTreeMap::new();
        stored_projections(&mut self.world, state, value, &mut Vec::new(), &mut stored);
        super::super::world::snapshot_version(&mut self.world, stored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::tests::with_semantics;

    #[test]
    fn concrete_fill_keeps_each_link_from_construction_to_borrowed_requirement() {
        use crate::semantic::range_facts::CheckedRangeTerm as Term;
        use crate::semantic::range_judgment::world::Stored;

        let source = include_bytes!(
            "../../../../../tests/conformance/cases/range1-pos-aggregate-filled-struct.wf"
        );
        with_semantics(source, |outcome| {
            let crate::SemanticOutcome::Complete(program) = outcome else {
                panic!("the complete fill witness must check: {outcome:?}");
            };
            let data = &program.data;
            let functions: Vec<_> = data.functions.iter().collect();
            let main = data
                .executable_functions()
                .find(|f| f.name == "main")
                .unwrap();
            let body = main.body.as_ref().unwrap();
            let CheckedStatement::Let {
                value:
                    CheckedExpression::UserCall {
                        function,
                        arguments,
                        ..
                    },
                ..
            } = &body[1]
            else {
                panic!("the fill is an ordinary checked call")
            };
            let callee = functions[function.0 as usize];
            assert_eq!(callee.name, "array_filled");
            assert_eq!(callee.parameters[0].ty, arguments[0].ty());
            assert!(matches!(callee.parameters[0].ty, CheckedType::Nominal(_)));
            let filled = callee
                .range_facts
                .postconditions
                .iter()
                .find(|post| post.clause.name == "filled")
                .expect("concrete filled clause");
            assert_eq!(filled.clause.conclusions.len(), 1);
            let relation = &filled.clause.conclusions[0];
            assert!(
                matches!(&relation.left, Term::Read { projection, indices, .. }
                if projection == &[CheckedRangeProjection::Field(0)] && indices == &[Term::Bound(0)])
            );
            assert!(
                matches!(&relation.right, Term::ValueProjection { root, projection, .. }
                if *root == CheckedRangeRoot::Binding(callee.parameters[0].binding)
                    && projection == &[CheckedRangeProjection::Field(0)])
            );
            assert!(
                callee
                    .range_facts
                    .postconditions
                    .iter()
                    .any(|post| !post.owed
                        && post
                            .clause
                            .conclusions
                            .iter()
                            .any(|relation| matches!(relation.right, Term::Constant(1)))),
                "the const-generic length postcondition must be lifted too"
            );

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
            let mut state = State::default();
            for statement in &body[..2] {
                state = walker.statement(state, statement).unwrap();
            }
            assert!(walker.issues.is_empty());
            let fill_id = *state
                .facts
                .iter()
                .find(|id| walker.facts[**id as usize].clause.name == "filled")
                .unwrap();
            let active = walker.facts[fill_id as usize].clone();
            let snapshot =
                active.frame.aggregates[&CheckedRangeRoot::Binding(callee.parameters[0].binding)];
            let VersionDef::Write { values, .. } = &walker.world.versions[snapshot as usize].def
            else {
                panic!("the actual by-value argument must have an immutable snapshot")
            };
            assert!(
                matches!(values.get([CheckedRangeProjection::Field(0)].as_slice()),
                Some(Stored::Int(value)) if *value == Linear::constant(0)),
                "the let-bound Block's field must survive the argument copy"
            );

            let CheckedStatement::Evaluate {
                value:
                    CheckedExpression::UserCall {
                        function,
                        arguments,
                        ..
                    },
                ..
            } = &body[2]
            else {
                panic!("the consumer is an ordinary checked call")
            };
            let need = functions[function.0 as usize];
            let borrowed = walker.eval(&mut state, &arguments[0]);
            let required = &need.range_facts.requirements[0];
            let frame = walker.frame(&mut state, required, &|_| Some(borrowed.clone()));
            let version = |frame: &Frame| match frame.places.values().next().unwrap() {
                PlaceView::Run { version, .. } => *version,
                other => panic!("expected a run: {other:?}"),
            };
            assert_eq!(
                version(&active.frame),
                version(&frame),
                "result[k] and targets^[k] must use the same container version"
            );
            let k = walker.world.opaque(None);
            let formed_fill = facts::form(
                &mut walker.world,
                &active.clause,
                &active.frame,
                std::slice::from_ref(&k),
                &[],
            )
            .unwrap();
            let formed_need = facts::form(&mut walker.world, required, &frame, &[k], &[]).unwrap();
            assert_eq!(
                formed_fill.conclusions[0].conclusions[0].left,
                formed_need.conclusions[0].conclusions[0].left,
                "the requirement's field read must select the filled trigger"
            );
            assert_eq!(walker.holds(&state, required, &frame), Ok(None));
            let mut without_length = state.clone();
            without_length.facts.retain(|id| *id == fill_id);
            assert_eq!(
                walker.holds(&without_length, required, &frame),
                Ok(None),
                "a fixed Array's type supplies its length even without a published length fact"
            );
            state.facts.retain(|id| *id != fill_id);
            assert!(
                walker.holds(&state, required, &frame).unwrap().is_some(),
                "the consumer needs the activated filled fact"
            );
        });
    }
}
