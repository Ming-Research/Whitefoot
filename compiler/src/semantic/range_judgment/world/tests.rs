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

fn region_preserves_read(
    written: Vec<WriteIndex>,
    footprint: Vec<CheckedRangeProjection>,
    at: Vec<Linear>,
    projection: Vec<CheckedRangeProjection>,
    premises: Vec<Literal>,
) -> Verdict {
    let mut world = World::default();
    let mut state = State::default();
    let location = Location::root(Origin::Constructed(world.new_origin()));
    let container = state.container(&mut world, location, 1).unwrap();
    let entry = state.version(&mut world, container);
    state.havoc_elements(&mut world, container, written, footprint);
    let exit = state.version(&mut world, container);
    let old = world.read(
        entry,
        at.clone(),
        projection.clone(),
        Some(IntegerType::U64),
    );
    let new = world.read(exit, at, projection, Some(IntegerType::U64));
    let mut units = premises;
    units.push(Literal::new(old, Relation::NotEqual, new));
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
    )
    .unwrap()
}

#[test]
fn a_captured_range_forgets_inside_and_retains_both_outside_boundaries() {
    for (index, expected) in [
        (0, Verdict::Refuted),
        (1, Verdict::Open),
        (2, Verdict::Open),
        (3, Verdict::Refuted),
    ] {
        assert_eq!(
            region_preserves_read(
                vec![WriteIndex::Range {
                    start: Linear::constant(1),
                    end: Linear::constant(3)
                }],
                Vec::new(),
                vec![Linear::constant(index)],
                vec![CheckedRangeProjection::Measure(CheckedMeasure::Length)],
                Vec::new(),
            ),
            expected,
            "index {index}"
        );
    }
}

#[test]
fn region_writes_keep_ancestor_prefixes_and_descriptor_siblings_distinct() {
    let footprint = vec![
        CheckedRangeProjection::Field(0),
        CheckedRangeProjection::Index(1),
    ];
    for (outer, field, expected) in [
        (0, 0, Verdict::Open),
        (1, 0, Verdict::Refuted),
        (0, 1, Verdict::Refuted),
    ] {
        assert_eq!(
            region_preserves_read(
                vec![WriteIndex::Point(Linear::constant(0)), WriteIndex::Any],
                footprint.clone(),
                vec![Linear::constant(outer), Linear::constant(2)],
                vec![
                    CheckedRangeProjection::Field(field),
                    CheckedRangeProjection::Index(1)
                ],
                Vec::new(),
            ),
            expected
        );
    }
    assert_eq!(
        region_preserves_read(
            vec![WriteIndex::Point(Linear::constant(0)), WriteIndex::Any],
            footprint,
            vec![Linear::constant(0)],
            vec![
                CheckedRangeProjection::Field(0),
                CheckedRangeProjection::Measure(CheckedMeasure::Length)
            ],
            Vec::new(),
        ),
        Verdict::Refuted
    );
}

#[test]
fn an_empty_nested_range_preserves_its_ancestors_identity() {
    assert_eq!(
        region_preserves_read(
            vec![
                WriteIndex::Point(Linear::constant(0)),
                WriteIndex::Range {
                    start: Linear::constant(2),
                    end: Linear::constant(2)
                }
            ],
            vec![CheckedRangeProjection::Index(1)],
            vec![Linear::constant(0)],
            vec![CheckedRangeProjection::WholeElement],
            Vec::new(),
        ),
        Verdict::Refuted
    );
}

#[test]
fn free_slot_writes_preserve_only_the_entry_live_prefix() {
    for (index, expected) in [
        (1, Verdict::Refuted),
        (2, Verdict::Open),
        (3, Verdict::Open),
    ] {
        assert_eq!(
            region_preserves_read(
                vec![WriteIndex::From(Linear::constant(2))],
                Vec::new(),
                vec![Linear::constant(index)],
                vec![CheckedRangeProjection::Field(0)],
                Vec::new(),
            ),
            expected
        );
    }
}

#[test]
fn a_call_footprint_loss_belongs_to_its_query_and_live_version() {
    let mut world = World::default();
    let mut state = State::default();
    let location = Location::root(Origin::Constructed(world.new_origin()));
    let container = state.container(&mut world, location, 1).unwrap();
    use crate::semantic::places::{CaptureId, CapturedRange, CapturedTerm, CapturedValue};
    let frames = vec![RangeFrame {
        captured: CapturedRange {
            start: CapturedValue::new(CaptureId::ValueDetermined, CapturedTerm::Literal(1)),
            end: CapturedValue::new(CaptureId::ValueDetermined, CapturedTerm::Literal(2)),
        },
        prefix: Vec::new(),
        projection: Vec::new(),
        offset: Linear::constant(0),
        start: Linear::constant(1),
        end: Linear::constant(2),
    }];
    state.havoc_elements(
        &mut world,
        container,
        vec![WriteIndex::Range {
            start: Linear::constant(1),
            end: Linear::constant(2),
        }],
        Vec::new(),
    );
    let narrowed = state.version(&mut world, container);
    world.call_footprint_losses.insert(
        narrowed,
        CallFootprintLoss {
            frames: frames.clone(),
            indices: vec![WriteIndex::Point(Linear::constant(1))],
            projection: vec![CheckedRangeProjection::Field(1)],
        },
    );
    let read = world.read(
        narrowed,
        vec![Linear::constant(1)],
        vec![CheckedRangeProjection::Field(0)],
        Some(IntegerType::U64),
    );
    let owed = vec![Literal::new(
        read.clone(),
        Relation::LessEqual,
        Linear::constant(7),
    )];
    let provenance = vec![facts::ReadProvenance {
        atom: read.terms[0].0,
        frames: frames.clone(),
    }];
    let candidates = facts::call_footprint_losses(&mut world, &provenance, &owed);
    assert_eq!(candidates.len(), 1);
    assert!(
        candidates[0].guards.is_empty() && candidates[0].conditions.is_empty(),
        "the same range's sibling field was lost by canonicalizing the footprint"
    );
    assert_eq!(candidates[0].restore.left, read);
    for old_fact in [false, true] {
        let mut query = Query {
            units: vec![negated(&owed[0])],
            ..Query::default()
        };
        if old_fact {
            query.units.push(Literal::new(
                candidates[0].restore.right.clone(),
                Relation::LessEqual,
                Linear::constant(7),
            ));
        }
        assert_eq!(
            facts::judge(&mut world, &[], &[], &[], &[], query.clone()),
            Ok(Verdict::Open)
        );
        assert!(query.restore_call_footprints(&mut world, &provenance, &owed));
        assert_eq!(
            facts::judge(&mut world, &[], &[], &[], &[], query),
            Ok(if old_fact {
                Verdict::Refuted
            } else {
                Verdict::Open
            }),
            "restoring a sibling relation explains failure only when an old fact proves the bound"
        );
    }

    assert!(
        facts::call_footprint_losses(&mut world, &[], &owed).is_empty(),
        "a root-index query has no same-range precision loss",
    );
    let mut different = provenance.clone();
    different[0].frames.push(frames[0].clone());
    assert!(
        facts::call_footprint_losses(&mut world, &different, &owed).is_empty(),
        "different reslicing chains cannot share suffix separation",
    );
    let saved_loss = world.call_footprint_losses[&narrowed].clone();
    world
        .call_footprint_losses
        .get_mut(&narrowed)
        .unwrap()
        .frames[0]
        .captured
        .start = CapturedValue::new(CaptureId::Source(1), CapturedTerm::Binding(BindingId(9)));
    let mut independent_capture = provenance.clone();
    independent_capture[0].frames[0].captured.start =
        CapturedValue::new(CaptureId::Source(2), CapturedTerm::Binding(BindingId(9)));
    assert!(
        facts::call_footprint_losses(&mut world, &independent_capture, &owed).is_empty(),
        "equal linear endpoint values do not identify independent binding captures"
    );
    world.call_footprint_losses.insert(narrowed, saved_loss);
    state.write_element(
        &mut world,
        container,
        vec![Linear::constant(3)],
        vec![CheckedRangeProjection::Field(0)],
        BTreeMap::new(),
    );
    let disjoint = state.version(&mut world, container);
    let later_read = world.read(
        disjoint,
        vec![Linear::constant(1)],
        vec![CheckedRangeProjection::Field(0)],
        Some(IntegerType::U64),
    );
    let later_owed = [Literal::new(
        later_read.clone(),
        Relation::LessEqual,
        Linear::constant(7),
    )];
    let later_provenance = [facts::ReadProvenance {
        atom: later_read.terms[0].0,
        frames: frames.clone(),
    }];
    let candidates = facts::call_footprint_losses(&mut world, &later_provenance, &later_owed);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].conditions,
        vec![vec![Literal::new(
            Linear::constant(1),
            Relation::NotEqual,
            Linear::constant(3),
        )]],
        "a later indexed write keeps the old loss only under index separation"
    );
    let join = world.new_join(vec![Vec::new(), Vec::new()]);
    let untouched = world.new_version(VersionDef::Fresh);
    let joined = world.new_version(VersionDef::Join {
        join,
        versions: vec![disjoint, untouched],
    });
    let joined_read = world.read(
        joined,
        vec![Linear::constant(1)],
        vec![CheckedRangeProjection::Field(0)],
        Some(IntegerType::U64),
    );
    let joined_owed = [Literal::new(
        joined_read.clone(),
        Relation::LessEqual,
        Linear::constant(7),
    )];
    let joined_provenance = [facts::ReadProvenance {
        atom: joined_read.terms[0].0,
        frames: frames.clone(),
    }];
    let candidates = facts::call_footprint_losses(&mut world, &joined_provenance, &joined_owed);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].guards, world.joins[join as usize][0],
        "a loss on one joined arm is conditional on taking that arm"
    );
    state.havoc_elements(&mut world, container, vec![WriteIndex::Any], Vec::new());
    let overwritten = state.version(&mut world, container);
    let read = world.read(
        overwritten,
        vec![Linear::constant(1)],
        vec![CheckedRangeProjection::Field(0)],
        Some(IntegerType::U64),
    );
    let owed = vec![Literal::new(
        read.clone(),
        Relation::LessEqual,
        Linear::constant(7),
    )];
    let provenance = vec![facts::ReadProvenance {
        atom: read.terms[0].0,
        frames,
    }];
    assert!(
        facts::call_footprint_losses(&mut world, &provenance, &owed).is_empty(),
        "a subsequent whole write destroys the fact independently of the older loss",
    );
}
