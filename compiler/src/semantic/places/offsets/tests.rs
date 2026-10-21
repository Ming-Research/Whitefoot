use super::*;
use crate::semantic::model::BindingId;
use crate::semantic::places::{CaptureId, CapturedTerm, CapturedValue, PlaceStep};

fn field(ordinal: u32) -> ResolvedPlace {
    let mut place = ResolvedPlace::binding(BindingId(0));
    place.path.push(PlaceStep::Field(ordinal));
    place
}

#[test]
fn projected_identities_keep_fields_distinct_and_intern_repeated_paths() {
    let table = CapturedPlaceTable::default();
    let first = table.intern(field(0));
    assert_eq!(first, table.intern(field(0)));
    assert_ne!(first, table.intern(field(1)));
    assert_eq!(table.get(first), field(0));
}

#[test]
fn current_projected_identity_and_immutable_captures_are_distinct() {
    let table = CapturedPlaceTable::default();
    let id = table.intern(field(0));
    let first = CapturedValue::new(CaptureId::source(1), CapturedTerm::Place(id));
    let second = CapturedValue::new(CaptureId::source(2), CapturedTerm::Place(id));
    assert_eq!(first.goal_identity(), second.goal_identity());
    assert!(!first.provably_same(second));
    let mut selected = ResolvedPlace::binding(BindingId(1));
    selected.path.push(PlaceStep::Index(first));
    assert!(!selected.is_exact_path());
    selected.supersede_term(CapturedTerm::Place(id));
    assert!(!selected.is_exact_path());
    let PlaceStep::Index(superseded) = selected.path[0] else {
        panic!("the selected path has one index");
    };
    assert_eq!(superseded.term, CapturedTerm::SupersededPlace(id));
    assert_eq!(superseded.goal_identity(), superseded);
    assert!(superseded.provably_same(first));
    assert_ne!(superseded.goal_identity(), second.goal_identity());
    assert!(selected.path[0].projected_offset_support().is_none());
}

#[test]
fn formal_projection_identity_has_no_body_binding_root() {
    let table = CapturedPlaceTable::default();
    let first = table.intern_formal(vec![PlaceStep::Field(0)]);
    assert_eq!(first, table.intern_formal(vec![PlaceStep::Field(0)]));
    assert_ne!(first, table.intern_formal(vec![PlaceStep::Field(1)]));
    assert_eq!(table.formal(first), [PlaceStep::Field(0)]);
}

#[test]
fn projected_paths_use_the_existing_current_offset_identity() {
    let table = CapturedPlaceTable::default();
    let path = |capture| {
        let mut place = ResolvedPlace::binding(BindingId(0));
        place.path.push(PlaceStep::Index(CapturedValue::new(
            CaptureId::source(capture),
            CapturedTerm::Binding(BindingId(1)),
        )));
        place.path.push(PlaceStep::Field(0));
        place
    };
    assert_eq!(table.intern(path(0)), table.intern(path(1)));
    let mut old = path(0);
    old.supersede_binding(BindingId(1));
    assert_ne!(table.intern(old), table.intern(path(1)));
}
