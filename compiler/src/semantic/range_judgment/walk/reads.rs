//! Projected reads and their diagnostic call-path provenance.
use super::*;

impl Walker<'_> {
    pub(super) fn read_element(
        &mut self,
        state: &mut State,
        container: ContainerId,
        indices: Vec<Linear>,
        ty: CheckedType,
        carrier: &NodePath,
    ) -> Value {
        self.read_projection(
            state,
            container,
            indices,
            Some(Vec::new()),
            ty,
            carrier,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn read_projection(
        &mut self,
        state: &mut State,
        container: ContainerId,
        indices: Vec<Linear>,
        projection: Option<Vec<CheckedRangeProjection>>,
        ty: CheckedType,
        carrier: &NodePath,
        barrier: Option<&RangeBarrier>,
    ) -> Value {
        self.access(container, &indices, false, carrier, state);
        match (ty, projection) {
            (CheckedType::Integer(integer), Some(projection)) => {
                let version = state.version(&mut self.world, container);
                let value = self.world.read(version, indices, projection, Some(integer));
                self.record_read_provenance(&value, barrier);
                Value::Int(value)
            }
            (CheckedType::Bool, Some(mut projection)) => {
                let version = state.version(&mut self.world, container);
                projection.push(CheckedRangeProjection::Tag(2));
                let tag = self
                    .world
                    .read(version, indices, projection, Some(IntegerType::U64));
                self.record_read_provenance(&tag, barrier);
                Value::Bool(Cond::Unknown, tag)
            }
            (other, Some(projection))
                if !matches!(
                    other,
                    CheckedType::Bool | CheckedType::Unit | CheckedType::Float(_)
                ) =>
            {
                let version = state.version(&mut self.world, container);
                let location = Location::root(Origin::Constructed(self.world.new_origin()));
                state.set_slot(
                    &mut self.world,
                    location.clone(),
                    Slot::Read(ReadSource {
                        version,
                        indices,
                        projection,
                    }),
                );
                Value::Owned(location)
            }
            (other, _) => self.opaque_of(other),
        }
    }
}
