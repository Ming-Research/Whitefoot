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
