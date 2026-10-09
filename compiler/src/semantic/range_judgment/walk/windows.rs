//! Placed OP-10 writes: preserve old elements and define the appended value.
use super::*;

impl Walker<'_> {
    pub(super) fn place_window_call(
        &mut self,
        state: &mut State,
        node: &NodePath,
        name: &str,
        arguments: &[Value],
    ) -> bool {
        let Some(Value::Ref(view)) = arguments.first() else {
            return false;
        };
        let (container, indices, projection, length, generation) = match view {
            View::Place(location) => {
                let Some(container) = state.container(&mut self.world, location.clone(), 1) else {
                    return false;
                };
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
            _ => return false,
        };
        let Some(next) = length.plus(&Linear::constant(if name == "place_back" { 1 } else { -1 }))
        else {
            return false;
        };
        // Descriptor mutation is not an element-only access for RANGE-5.
        self.unplaced_view(view, true, node, state);
        if name == "place_back" {
            let Some(value) = arguments.get(1) else {
                return false;
            };
            let mut at = indices.clone();
            let mut path = projection.clone();
            if generation.is_none() {
                path.push(CheckedRangeProjection::Index(at.len() as u32));
            }
            at.push(length);
            self.write_element(state, container, at, Some(path), value, node);
        }
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
        true
    }
}
