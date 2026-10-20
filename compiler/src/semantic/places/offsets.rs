//! Interned proof paths of projected integer offsets [ENT-2, ENT-5].

use std::collections::HashMap;
use std::sync::Mutex;

use super::{CapturedTerm, PlaceStep, ResolvedPlace};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CapturedPlaceId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FormalOffsetId(pub(crate) u32);

#[derive(Clone, Debug, Default)]
pub(crate) struct CapturedPlaces {
    pub(crate) places: Vec<ResolvedPlace>,
    pub(crate) formal_offsets: Vec<Vec<PlaceStep>>,
}

#[derive(Default)]
struct Entries {
    retained: CapturedPlaces,
    ids: HashMap<ResolvedPlace, CapturedPlaceId>,
    formal_ids: HashMap<Vec<PlaceStep>, FormalOffsetId>,
}

/// Structural proof paths share IDs, as ENT-2 requires.
/// Parallel analyses may intern substitutions, so insertion holds a short lock.
#[derive(Default)]
pub(crate) struct CapturedPlaceTable {
    entries: Mutex<Entries>,
}

impl CapturedPlaceTable {
    pub(crate) fn intern_formal(&self, projections: Vec<PlaceStep>) -> FormalOffsetId {
        let mut entries = self.entries.lock().expect("offset-place table lock");
        if let Some(id) = entries.formal_ids.get(&projections) {
            return *id;
        }
        let id = FormalOffsetId(
            u32::try_from(entries.retained.formal_offsets.len()).expect("formal offset identity"),
        );
        entries.retained.formal_offsets.push(projections.clone());
        entries.formal_ids.insert(projections, id);
        id
    }

    pub(crate) fn formal(&self, id: FormalOffsetId) -> Vec<PlaceStep> {
        self.entries
            .lock()
            .expect("offset-place table lock")
            .retained
            .formal_offsets[id.0 as usize]
            .clone()
    }

    pub(crate) fn intern(&self, place: ResolvedPlace) -> CapturedPlaceId {
        let place = place.term_identity();
        let mut entries = self.entries.lock().expect("offset-place table lock");
        if let Some(id) = entries.ids.get(&place) {
            *id
        } else {
            let id = CapturedPlaceId(
                u32::try_from(entries.retained.places.len()).expect("offset-place identity"),
            );
            entries.retained.places.push(place.clone());
            entries.ids.insert(place, id);
            id
        }
    }

    pub(crate) fn get(&self, id: CapturedPlaceId) -> ResolvedPlace {
        self.entries
            .lock()
            .expect("offset-place table lock")
            .retained
            .places[id.0 as usize]
            .clone()
    }

    /// Complete current-value support of an offset, before resolving references.
    pub(crate) fn support(&self, term: CapturedTerm) -> Vec<ResolvedPlace> {
        let place = match term {
            CapturedTerm::Binding(binding) => ResolvedPlace::binding(binding),
            CapturedTerm::Place(id) => self.get(id),
            _ => return Vec::new(),
        };
        let mut support = Vec::new();
        for step in &place.path {
            if let Some(binding) = step.measure_offset_support() {
                support.push(ResolvedPlace::binding(binding));
            }
            if let Some(id) = step.projected_offset_support() {
                support.extend(self.support(CapturedTerm::Place(id)));
            }
        }
        support.push(place);
        support
    }

}

/// The written clause (a) place, before capture, identified by its proof path.
/// Exact aliases contribute their target path; other references retain their root.
pub(crate) fn tracked_offset_place(
    expression: &crate::semantic::model::CheckedExpression,
) -> Option<ResolvedPlace> {
    use super::PlaceStep;
    use crate::semantic::model::CheckedExpression;
    match expression {
        CheckedExpression::Binding {
            binding,
            consume_root: false,
            ..
        } => Some(ResolvedPlace::binding(*binding)),
        CheckedExpression::Project {
            binding,
            fields,
            consume_root: false,
            ..
        } => {
            let mut place = ResolvedPlace::binding(*binding);
            place
                .path
                .extend(fields.iter().copied().map(PlaceStep::Field));
            Some(place)
        }
        CheckedExpression::DerefAddressed { binding, proof, .. } => Some(
            proof
                .clone()
                .unwrap_or_else(|| ResolvedPlace::binding(*binding)),
        ),
        CheckedExpression::BoxDeref { value, .. } => {
            let mut place = tracked_offset_place(value)?;
            place.path.push(PlaceStep::Deref);
            Some(place)
        }
        CheckedExpression::ProjectValue { value, field, .. } => {
            let mut place = tracked_offset_place(value)?;
            place.path.push(PlaceStep::Field(*field));
            Some(place)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
