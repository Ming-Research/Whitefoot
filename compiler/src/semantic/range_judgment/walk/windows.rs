//! Placed OP-10 writes and removed-element images [RANGE-2].
use super::*;

impl Walker<'_> {
    pub(super) fn place_window_call(
        &mut self,
        state: &mut State,
        node: &NodePath,
        name: &str,
        arguments: &[Value],
        result: CheckedType,
    ) -> Option<Value> {
        let Some(Value::Ref(view)) = arguments.first() else {
            return None;
        };
        let (container, indices, projection, length, generation) = match view {
            View::Place(location) => {
                let container = state.container(&mut self.world, location.clone(), 1)?;
                let generation = state.generation(container);
                let length = self
                    .world
                    .measure(container, generation, CheckedMeasure::Length);
                (container, Vec::new(), Vec::new(), length, Some(generation))
            }
            View::Element {
                container,
                indices,
                projection: Some(projection),
            } => {
                let version = state.version(&mut self.world, *container);
                let mut path = projection.clone();
                path.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
                let length =
                    self.world
                        .read(version, indices.clone(), path, Some(IntegerType::U64));
                (
                    *container,
                    indices.clone(),
                    projection.clone(),
                    length,
                    None,
                )
            }
            _ => return None,
        };
        let next =
            length.plus(&Linear::constant(if name == "place_back" { 1 } else { -1 }))?;
        // Descriptor mutation is not an element-only access for RANGE-5.
        self.unplaced_view(view, true, node, state);
        let value = if name == "place_back" {
            let value = arguments.get(1)?;
            let mut at = indices.clone();
            let mut path = projection.clone();
            if generation.is_none() {
                path.push(CheckedRangeProjection::Index(at.len() as u32));
            }
            at.push(length);
            self.write_element(state, container, at, Some(path), value, node);
            Value::Unknown
        } else {
            // [RANGE-2] consuming take_back reads the removed storage from
            // the window's old version at its new length. Capture it before
            // changing a nested window's length projection; the lazy source
            // also preserves tags and every still-unnamed descendant.
            let mut at = indices.clone();
            let mut path = projection.clone();
            if generation.is_none() {
                path.push(CheckedRangeProjection::Index(at.len() as u32));
            }
            at.push(next.clone());
            self.read_projection(state, container, at, Some(path), result, node)
        };
        if let Some(old_generation) = generation {
            let new_generation = self.world.new_generation();
            state.generations.insert(container, new_generation);
            if let Some(log) = &mut self.world.log {
                log.descriptors.insert(container);
            }
            let new_length = self
                .world
                .measure(container, new_generation, CheckedMeasure::Length);
            state.conds.push(literal(new_length, Relation::Equal, next));
            for measure in [CheckedMeasure::Capacity, CheckedMeasure::Head] {
                let old = self.world.measure(container, old_generation, measure);
                let new = self.world.measure(container, new_generation, measure);
                state.conds.push(literal(new, Relation::Equal, old));
            }
        } else {
            let mut path = projection;
            path.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
            state.write_element(
                &mut self.world,
                container,
                indices,
                path,
                BTreeMap::from([(Vec::new(), super::super::world::Stored::Int(next))]),
            );
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::tests::with_semantics;

    /// [RANGE-2] the consuming read keeps the old version, new-length index,
    /// tag and untouched fields even when the returned value is later written.
    #[test]
    fn take_back_captures_old_last_and_mutates_only_returned_storage() {
        let source = br#"struct Block {
  parent: u32;
  live: Bool;
}

fn probe(xs: &Slots<Block, 2>) -> result: unit writes(xs) contract {
  requires xs^.len == 2_u64;
} {
  let taken = take_back(window: xs);
  set taken.parent = 1_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
        with_semantics(source, |outcome| {
            let crate::SemanticOutcome::Complete(program) = outcome else {
                panic!("the probe needs no range proof: {outcome:?}");
            };
            let data = &program.data;
            let functions: Vec<_> = data.functions.iter().collect();
            let probe = data
                .executable_functions()
                .find(|function| function.name == "probe")
                .unwrap();
            let body = probe.body.as_ref().unwrap();
            let CheckedStatement::Let { binding, value, .. } = &body[0] else {
                panic!("the first statement takes the element");
            };
            let CheckedExpression::UserCall { function, .. } = value else {
                panic!("take_back is a checked prelude call");
            };
            let prelude = BTreeSet::from([functions[function.0 as usize].declaration]);
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
            let parameter = probe.parameters[0].binding;
            let window = Location::root(Origin::Parameter(parameter));
            state
                .values
                .insert(parameter, Value::Ref(View::Place(window.clone())));
            let container = state.container(&mut walker.world, window, 1).unwrap();
            let old_version = state.version(&mut walker.world, container);
            let old_length = walker.world.measure(
                container,
                state.generation(container),
                CheckedMeasure::Length,
            );
            state = walker.statement(state, &body[0]).unwrap();
            let Value::Owned(taken) = state.values.get(binding).unwrap().clone() else {
                panic!("the returned aggregate owns storage");
            };
            let source = state.read_source(&taken).expect("old element image");
            assert_eq!(source.version, old_version);
            assert_eq!(
                source.indices,
                vec![old_length.plus(&Linear::constant(-1)).unwrap()],
                "the removed index is the new length, not the old length"
            );
            assert!(source.projection.is_empty());
            let live = taken.child(Step::Field(1));
            let expected_tag = walker.world.read(
                old_version,
                source.indices.clone(),
                vec![CheckedRangeProjection::Field(1), CheckedRangeProjection::Tag(2)],
                Some(IntegerType::U64),
            );
            let Value::Bool(_, old_tag) = walker.read_location(&mut state, &live, CheckedType::Bool)
            else {
                panic!("the live projection is Bool");
            };
            assert_eq!(old_tag, expected_tag);
            state = walker.statement(state, &body[1]).unwrap();
            assert_eq!(
                walker.read_location(
                    &mut state,
                    &taken.child(Step::Field(0)),
                    CheckedType::Integer(IntegerType::U32),
                ),
                Value::Int(Linear::constant(1)),
                "the changed parent overrides its old image"
            );
            let Value::Bool(_, tag) = walker.read_location(&mut state, &live, CheckedType::Bool)
            else {
                panic!("the live projection is Bool");
            };
            assert_eq!(tag, expected_tag, "a sibling write preserves the old tag");
            assert_eq!(state.version(&mut walker.world, container), old_version);
        });
    }
}
