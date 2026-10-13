//! Descriptor support and version invalidation implementation invariants.
use super::super::facts::{self, Query};
use super::super::solver::Verdict;
use super::*;

#[test]
fn descriptor_projection_overlap_uses_measure_support() {
    use CheckedMeasure::{Bytes, Capacity, Head, Length, Pages};
    for measure in [Length, Capacity, Head, Pages, Bytes] {
        let projection = [CheckedRangeProjection::Measure(measure)];
        let bytes = [CheckedRangeProjection::Measure(Bytes)];
        assert!(projections_overlap(&projection, &bytes));
        assert!(projections_overlap(&bytes, &projection));
    }
    assert!(projections_overlap(
        &[CheckedRangeProjection::Measure(Length)],
        &[CheckedRangeProjection::Measure(Pages)],
    ));
    assert!(!projections_overlap(
        &[CheckedRangeProjection::Measure(Length)],
        &[CheckedRangeProjection::Measure(Capacity)],
    ));
    assert!(!projections_overlap(
        &[CheckedRangeProjection::Measure(Bytes)],
        &[CheckedRangeProjection::Index(0)],
    ));
}

#[test]
fn a_descriptor_write_retains_only_disjoint_measure_images() {
    let mut world = World::default();
    let mut state = State::default();
    let location = Location::root(Origin::Constructed(world.new_origin()));
    let container = state.container(&mut world, location, 1).unwrap();
    let version = state.version(&mut world, container);
    let before = state.generation(container);
    world.log = Some(Modified::default());
    state.havoc_measure(&mut world, container, CheckedMeasure::Length);
    let after = state.generation(container);
    assert_eq!(state.version(&mut world, container), version);
    let log = world.log.take().unwrap();
    assert!(log.descriptors.is_empty());
    assert_eq!(
        log.measures.get(&container),
        Some(&BTreeSet::from([CheckedMeasure::Length])),
    );
    for (measure, expected) in [
        (CheckedMeasure::Length, Verdict::Open),
        (CheckedMeasure::Pages, Verdict::Open),
        (CheckedMeasure::Bytes, Verdict::Open),
        (CheckedMeasure::Capacity, Verdict::Refuted),
        (CheckedMeasure::Head, Verdict::Refuted),
    ] {
        let old = world.measure(container, before, measure);
        let new = world.measure(container, after, measure);
        let mut units = state.conds.clone();
        units.push(Literal::new(old, Relation::NotEqual, new));
        assert_eq!(
            facts::judge(
                &mut world,
                &[],
                &[],
                &[],
                &[],
                Query {
                    units,
                    ..Query::default()
                },
            ),
            Ok(expected),
            "descriptor support for {measure:?}",
        );
    }
}
