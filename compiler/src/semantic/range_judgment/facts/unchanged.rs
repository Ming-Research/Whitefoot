//! Demand-driven projection definitions for imported frames [RANGE-2, RANGE-3].
use super::*;

/// Instantiated frame definitions and the existing reads already expanded
/// through them. A definition is retained once per fact instance [RANGE-3].
#[derive(Default)]
pub(in crate::semantic::range_judgment) struct Definitions {
    instances: Vec<(Rule, Vec<Rule>)>,
    seen: BTreeSet<(usize, AtomId)>,
}

impl Definitions {
    pub(super) fn record(&mut self, world: &World, rule: &Rule, conditions: &[Rule]) {
        let [relation] = rule.conclusions.as_slice() else {
            return;
        };
        if relation.relation != Relation::Equal {
            return;
        }
        let Some((_, _, left)) = read(world, &relation.left) else {
            return;
        };
        let Some((_, _, right)) = read(world, &relation.right) else {
            return;
        };
        if left.last() == Some(&CheckedRangeProjection::WholeElement)
            && right.last() == Some(&CheckedRangeProjection::WholeElement)
        {
            self.instances.push((rule.clone(), conditions.to_vec()));
        }
    }
}

/// Close instantiated unchanged definitions over reads the problem already
/// holds, including reads introduced by another definition. This is step 2,
/// not an extra automatic-instantiation round [RANGE-3].
pub(super) fn expand(
    world: &mut World,
    atoms: &mut BTreeSet<AtomId>,
    query: &mut Query,
) -> Result<bool, Capacity> {
    let before = atoms.len();
    for index in 0..query.frames.instances.len() {
        let ground: Vec<_> = atoms
            .iter()
            .copied()
            .filter(|atom| query.frames.seen.insert((index, *atom)))
            .collect();
        let (rule, conditions) = query.frames.instances[index].clone();
        for definition in definitions(world, &[rule], &ground) {
            for literal in definition.guards.iter().chain(&definition.conclusions) {
                collect_literal(literal, atoms);
            }
            if atoms.len() > MAX_ATOMS {
                return Err(Capacity::Atoms);
            }
            if conditions.is_empty() {
                query.rules.push(definition);
            } else {
                query
                    .choices
                    .push(conditional_instance(&definition, &conditions));
            }
        }
    }
    Ok(atoms.len() != before)
}

fn read(
    world: &World,
    value: &Linear,
) -> Option<(VersionId, Vec<Linear>, Vec<CheckedRangeProjection>)> {
    let [(atom, 1)] = value.terms.as_slice() else {
        return None;
    };
    if value.constant != 0 {
        return None;
    }
    let AtomDef::Read {
        version,
        indices,
        projection,
    } = &world.atoms[*atom as usize].def
    else {
        return None;
    };
    Some((*version, indices.clone(), projection.clone()))
}

/// An instantiated unchanged conclusion defines the selected exit reads by
/// entry reads, under its existing premises. Only reads already in the
/// problem participate; these definitions create no further instance round.
/// Projections come from those reads, so a generic frame preserves exactly
/// the projections selected at its concrete caller, without expanding T.
pub(super) fn definitions(world: &mut World, conclusions: &[Rule], ground: &[AtomId]) -> Vec<Rule> {
    let mut definitions = Vec::new();
    for conclusion in conclusions {
        let [relation] = conclusion.conclusions.as_slice() else {
            continue;
        };
        if relation.relation != Relation::Equal {
            continue;
        }
        let Some((exit_version, exit_indices, mut exit_path)) = read(world, &relation.left) else {
            continue;
        };
        let Some((entry_version, entry_indices, mut entry_path)) = read(world, &relation.right)
        else {
            continue;
        };
        if exit_path.pop() != Some(CheckedRangeProjection::WholeElement)
            || entry_path.pop() != Some(CheckedRangeProjection::WholeElement)
        {
            continue;
        }
        for atom in ground {
            let value = &world.atoms[*atom as usize];
            let AtomDef::Read {
                version,
                indices,
                projection,
            } = &value.def
            else {
                continue;
            };
            if *version != exit_version
                || indices.len() < exit_indices.len()
                || !projection.starts_with(&exit_path)
            {
                continue;
            }
            let mut guards = conclusion.guards.clone();
            guards.extend(
                indices
                    .iter()
                    .zip(&exit_indices)
                    .filter(|(left, right)| left != right)
                    .map(|(left, right)| {
                        Literal::new(left.clone(), Relation::Equal, right.clone())
                    }),
            );
            let mut selected = entry_indices.clone();
            selected.extend_from_slice(&indices[exit_indices.len()..]);
            let mut path = entry_path.clone();
            path.extend(super::super::world::shift_projection(
                &projection[exit_path.len()..],
                exit_indices.len(),
                entry_indices.len(),
            ));
            let entry_read = world.read(entry_version, selected, path, value.ty);
            definitions.push(Rule {
                guards,
                conclusions: vec![Literal::new(
                    Linear::atom(*atom),
                    Relation::Equal,
                    entry_read,
                )],
            });
        }
    }
    definitions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::model::{BindingId, IntegerType};
    use crate::semantic::range_facts::CheckedRangeBinder;
    use crate::semantic::range_judgment::world::ReadSource;

    fn node() -> crate::NodePath {
        crate::NodePath {
            components: Vec::new(),
        }
    }

    fn frame_fact(entry: VersionId, exit: VersionId, length: i128) -> Fact {
        let place = |root| CheckedRangePlace {
            root,
            path: Vec::new(),
            fixed_length: None,
        };
        let exit_place = place(CheckedRangeRoot::Binding(BindingId(0)));
        let entry_place = place(CheckedRangeRoot::Entry(BindingId(0)));
        let view = |version| PlaceView::Run {
            container: 0,
            version,
            generation: 0,
            prefix: Vec::new(),
            offset: Linear::constant(0),
            length: Linear::constant(length),
        };
        let read = |place| CheckedRangeTerm::Read {
            place,
            shape: CheckedRangeShape::Run,
            indices: vec![CheckedRangeTerm::Bound(0)],
            projection: vec![CheckedRangeProjection::WholeElement],
            element: IntegerType::U64,
            implicit_indices: Vec::new(),
            guarded_from: None,
        };
        Fact {
            frame: Frame {
                places: BTreeMap::from([
                    (exit_place.clone(), view(exit)),
                    (entry_place.clone(), view(entry)),
                ]),
                ..Frame::default()
            },
            clause: CheckedRangeClause {
                declaration: crate::DeclarationId::from_index(0).unwrap(),
                name: "frame".into(),
                node: node(),
                binders: vec![CheckedRangeBinder {
                    start: CheckedRangeTerm::Constant(0),
                    end: CheckedRangeTerm::Constant(length),
                }],
                guards: Vec::new(),
                conclusions: vec![CheckedRangeRelation {
                    node: node(),
                    left: read(exit_place),
                    comparison: RangeComparison::Equal,
                    right: read(entry_place),
                    projected: false,
                }],
            },
        }
    }

    fn differs(left: Linear, right: Linear) -> Query {
        Query {
            units: vec![Literal::new(left, Relation::NotEqual, right)],
            ..Query::default()
        }
    }

    /// [RANGE-3] A write cannot re-establish identity by restoring its value;
    /// a sibling selection and a different element still retain identity.
    #[test]
    fn whole_identity_is_untyped_and_descendant_writes_cannot_restore_it() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let field = vec![CheckedRangeProjection::Field(0)];
        let changed = world.new_version(VersionDef::Write {
            previous: entry,
            indices: vec![Linear::constant(0)],
            projection: field.clone(),
            values: BTreeMap::from([(
                Vec::new(),
                Stored::Read(ReadSource {
                    version: entry,
                    indices: vec![Linear::constant(0)],
                    projection: field,
                }),
            )]),
        });
        for (index, projection, expected) in [
            (0, vec![CheckedRangeProjection::WholeElement], Verdict::Open),
            (
                0,
                vec![
                    CheckedRangeProjection::Field(0),
                    CheckedRangeProjection::WholeElement,
                ],
                Verdict::Open,
            ),
            (
                0,
                vec![
                    CheckedRangeProjection::Field(1),
                    CheckedRangeProjection::WholeElement,
                ],
                Verdict::Refuted,
            ),
            (
                1,
                vec![CheckedRangeProjection::WholeElement],
                Verdict::Refuted,
            ),
        ] {
            let old = world.read(
                entry,
                vec![Linear::constant(index)],
                projection.clone(),
                Some(IntegerType::U64),
            );
            let new = world.read(
                changed,
                vec![Linear::constant(index)],
                projection,
                Some(IntegerType::U64),
            );
            assert!(world.atoms[new.terms[0].0 as usize].ty.is_none());
            assert_eq!(
                judge(&mut world, &[], &[], &[], differs(old, new)),
                Ok(expected)
            );
        }
    }

    /// [RANGE-2] Direct container copies and lazily materialized collections
    /// keep their value projections, but neither inherits element identity.
    #[test]
    fn copied_collections_preserve_values_without_preserving_identity() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        for (source_indices, source_projection, arity) in [
            (Vec::new(), Vec::new(), 0),
            (
                vec![Linear::constant(4)],
                vec![CheckedRangeProjection::Field(0)],
                1,
            ),
        ] {
            let copied = world.new_version(VersionDef::Copied {
                source: ReadSource {
                    version: entry,
                    indices: source_indices.clone(),
                    projection: source_projection.clone(),
                },
                arity,
            });
            for (projection, expected) in [
                (vec![CheckedRangeProjection::Field(1)], Verdict::Refuted),
                (vec![CheckedRangeProjection::WholeElement], Verdict::Open),
            ] {
                let mut source_path = source_projection.clone();
                source_path.extend((0..arity).map(|position| {
                    CheckedRangeProjection::Index((source_indices.len() + position) as u32)
                }));
                source_path.extend(projection.clone());
                let source_tuple = source_indices
                    .iter()
                    .cloned()
                    .chain([Linear::constant(0)])
                    .collect();
                let old = world.read(entry, source_tuple, source_path, Some(IntegerType::U64));
                let new = world.read(
                    copied,
                    vec![Linear::constant(0)],
                    projection,
                    Some(IntegerType::U64),
                );
                assert_eq!(
                    judge(&mut world, &[], &[], &[], differs(old, new)),
                    Ok(expected)
                );
            }
        }
    }

    /// [RANGE-2] A generic frame defines concrete descendant reads only under
    /// its range and guards; unrelated selections receive no equality.
    #[test]
    fn imported_frame_defines_requested_projections_under_its_premises() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let exit = world.new_version(VersionDef::Fresh);
        let mut fact = frame_fact(entry, exit, 2);
        fact.clause.guards.push(CheckedRangeRelation {
            node: node(),
            left: CheckedRangeTerm::Bound(0),
            comparison: RangeComparison::Equal,
            right: CheckedRangeTerm::Constant(0),
            projected: false,
        });
        for (index, expected) in [
            (0, Verdict::Refuted),
            (1, Verdict::Open),
            (2, Verdict::Open),
        ] {
            let projection = vec![
                CheckedRangeProjection::Field(3),
                CheckedRangeProjection::BoxContent,
                CheckedRangeProjection::Index(1),
                CheckedRangeProjection::Field(5),
            ];
            let indices = vec![Linear::constant(index), Linear::constant(7)];
            let old = world.read(
                entry,
                indices.clone(),
                projection.clone(),
                Some(IntegerType::U32),
            );
            let new = world.read(exit, indices, projection, Some(IntegerType::U32));
            assert_eq!(
                judge(&mut world, &[fact.clone()], &[0], &[], differs(old, new)),
                Ok(expected)
            );
        }
    }

    /// [RANGE-1] Undefined entry reads condition a frame, including when its
    /// written range extends to an appended element; they never define it.
    #[test]
    fn entry_definedness_limits_imported_frames_without_a_new_obligation() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let exit = world.new_version(VersionDef::Fresh);
        let mut fact = frame_fact(entry, exit, 2);
        for (place, view) in &mut fact.frame.places {
            if matches!(place.root, CheckedRangeRoot::Entry(_)) {
                let PlaceView::Run { length, .. } = view else {
                    unreachable!()
                };
                *length = Linear::constant(1);
            }
        }
        for (index, expected) in [(0, Verdict::Refuted), (1, Verdict::Open)] {
            let old = world.read(
                entry,
                vec![Linear::constant(index)],
                Vec::new(),
                Some(IntegerType::U64),
            );
            let new = world.read(
                exit,
                vec![Linear::constant(index)],
                Vec::new(),
                Some(IntegerType::U64),
            );
            assert_eq!(
                judge(&mut world, &[fact.clone()], &[0], &[], differs(old, new)),
                Ok(expected)
            );
        }
    }

    /// [RANGE-3] Scalar facts on the entry version become usable in round
    /// two. A chain needing a third round remains unproved.
    #[test]
    fn frame_definitions_keep_exactly_two_instantiation_rounds() {
        let mut world = World::default();
        let initial = world.new_version(VersionDef::Initial);
        let middle = world.new_version(VersionDef::Fresh);
        let exit = world.new_version(VersionDef::Fresh);
        let mut scalar = frame_fact(initial, initial, 1);
        if let CheckedRangeTerm::Read { projection, .. } = &mut scalar.clause.conclusions[0].left {
            *projection = vec![CheckedRangeProjection::Field(0)];
        }
        scalar.clause.conclusions[0].right = CheckedRangeTerm::Constant(7);
        let first = frame_fact(initial, middle, 1);
        let second = frame_fact(middle, exit, 1);
        for (version, facts, active, expected) in [
            (
                middle,
                vec![scalar.clone(), first.clone()],
                vec![0, 1],
                Verdict::Refuted,
            ),
            (
                exit,
                vec![scalar, first, second],
                vec![0, 1, 2],
                Verdict::Open,
            ),
        ] {
            let value = world.read(
                version,
                vec![Linear::constant(0)],
                vec![CheckedRangeProjection::Field(0)],
                Some(IntegerType::U64),
            );
            assert_eq!(
                judge(
                    &mut world,
                    &facts,
                    &active,
                    &[],
                    differs(value, Linear::constant(7))
                ),
                Ok(expected)
            );
        }
    }

    /// [RANGE-3] An already instantiated frame defines a projection first
    /// read by a second-round instance, without needing a third round.
    #[test]
    fn second_round_projection_reads_receive_existing_frame_definitions() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let exit = world.new_version(VersionDef::Fresh);
        let frame = frame_fact(entry, exit, 1);
        let mut relay = frame.clone();
        let relation = &mut relay.clause.conclusions[0];
        std::mem::swap(&mut relation.left, &mut relation.right);
        if let CheckedRangeTerm::Read { projection, .. } = &mut relation.left {
            *projection = vec![CheckedRangeProjection::Field(0)];
        }
        if let CheckedRangeTerm::Read { projection, .. } = &mut relation.right {
            *projection = vec![CheckedRangeProjection::Field(1)];
        }
        let result = world.read(
            exit,
            vec![Linear::constant(0)],
            vec![CheckedRangeProjection::Field(0)],
            Some(IntegerType::U64),
        );
        let known = world.read(
            entry,
            vec![Linear::constant(0)],
            vec![CheckedRangeProjection::Field(1)],
            Some(IntegerType::U64),
        );
        let mut query = differs(result, Linear::constant(7));
        query
            .units
            .push(Literal::new(known, Relation::Equal, Linear::constant(7)));
        assert_eq!(
            judge(&mut world, &[frame, relay], &[0, 1], &[], query),
            Ok(Verdict::Refuted)
        );
    }

    /// [RANGE-3] Projections of one tuple share its instance allowance;
    /// distinct selected tuples retain the 256-instance ceiling.
    #[test]
    fn lazy_projections_share_instances_but_do_not_bypass_the_tuple_ceiling() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let exit = world.new_version(VersionDef::Fresh);
        let fact = frame_fact(entry, exit, 300);
        let mut one_tuple = Query::default();
        for field in 0..257 {
            let value = world.read(
                exit,
                vec![Linear::constant(0)],
                vec![CheckedRangeProjection::Field(field)],
                Some(IntegerType::U64),
            );
            one_tuple
                .support
                .push(Literal::new(value, Relation::Equal, Linear::constant(0)));
        }
        assert!(judge(&mut world, &[fact.clone()], &[0], &[], one_tuple).is_ok());
        let mut many_tuples = Query::default();
        for index in 0..257 {
            let value = world.read(
                exit,
                vec![Linear::constant(index)],
                vec![CheckedRangeProjection::Field(0)],
                Some(IntegerType::U64),
            );
            many_tuples
                .support
                .push(Literal::new(value, Relation::Equal, Linear::constant(0)));
        }
        assert_eq!(
            judge(&mut world, &[fact], &[0], &[], many_tuples),
            Err(Capacity::Instances)
        );
    }

    /// [RANGE-3] Lazy definitions share the existing atom allowance: one
    /// tuple with too many requested projections is still over the ceiling.
    #[test]
    fn lazy_projection_definitions_count_toward_the_atom_ceiling() {
        let mut world = World::default();
        let entry = world.new_version(VersionDef::Initial);
        let exit = world.new_version(VersionDef::Fresh);
        let fact = frame_fact(entry, exit, 1);
        let mut query = Query::default();
        for field in 0..2100 {
            let value = world.read(
                exit,
                vec![Linear::constant(0)],
                vec![CheckedRangeProjection::Field(field)],
                Some(IntegerType::U64),
            );
            query
                .support
                .push(Literal::new(value, Relation::Equal, Linear::constant(0)));
        }
        assert_eq!(
            judge(&mut world, &[fact], &[0], &[], query),
            Err(Capacity::Atoms)
        );
    }
}
