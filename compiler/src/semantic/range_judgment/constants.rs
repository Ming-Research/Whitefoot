//! Constant constructions use the same range derivation as body constructions.
use super::super::model::{CheckedMeasure, CheckedValue};
use super::super::range_facts::{CheckedRangeClause, CheckedRangeStep};
use super::RangeIssue;
use super::facts::{self, Frame, PlaceView, Query};
use super::solver::{Linear, Literal, Relation, Verdict};
use super::world::{Location, Origin, State, Step, Value, World, stored_projections};
use std::collections::BTreeMap;

pub(super) fn value(world: &mut World, state: &mut State, value: &CheckedValue) -> Value {
    match value {
        CheckedValue::Integer { ty, bits } => Value::Int(Linear::constant(
            crate::semantic::entailment::integer_value(*ty, *bits),
        )),
        CheckedValue::Bool(truth) => world.boolean(super::world::Cond::Constant(*truth)),
        CheckedValue::ConstGeneric { declaration, ty } => {
            Value::Int(world.const_generic(*declaration, *ty))
        }
        CheckedValue::Struct { fields, .. } => Value::Struct(
            fields
                .iter()
                .map(|field| self::value(world, state, field))
                .collect(),
        ),
        CheckedValue::Array { elements, .. } => {
            let location = Location::root(Origin::Constructed(world.new_origin()));
            let container = world
                .container(location.clone(), 1)
                .expect("fresh constant storage");
            for measure in [CheckedMeasure::Length, CheckedMeasure::Capacity] {
                let measured = world.measure(container, 0, measure);
                state.conds.push(Literal::new(
                    measured,
                    Relation::Equal,
                    Linear::constant(elements.len() as i128),
                ));
            }
            for (index, element) in elements.iter().enumerate() {
                let element = self::value(world, state, element);
                let mut stored = BTreeMap::new();
                stored_projections(world, state, &element, &mut Vec::new(), &mut stored);
                state.write_element(
                    world,
                    container,
                    vec![Linear::constant(index as i128)],
                    Vec::new(),
                    stored,
                );
            }
            Value::Owned(location)
        }
        _ => Value::Unknown,
    }
}

pub(crate) fn judge(
    clause: &CheckedRangeClause,
    constant: &CheckedValue,
    node: &crate::NodePath,
) -> Option<RangeIssue> {
    let mut world = World::default();
    let mut state = State::default();
    let root = value(&mut world, &mut state, constant);
    let mut frame = Frame::default();
    for place in clause.places() {
        let mut selected = root.clone();
        for step in &place.path {
            selected = match (selected, step) {
                (value, CheckedRangeStep::Referent) => value,
                (Value::Struct(fields), CheckedRangeStep::Field(field)) => fields
                    .get(*field as usize)
                    .cloned()
                    .unwrap_or(Value::Unknown),
                (Value::Owned(location), CheckedRangeStep::Field(field)) => {
                    Value::Owned(location.child(Step::Field(*field)))
                }
                _ => Value::Unknown,
            };
        }
        let view = match selected {
            Value::Owned(location) => {
                let container = world
                    .container(state.resolve(&location), 1)
                    .expect("constant array storage");
                PlaceView::Run {
                    container,
                    version: state.version(&mut world, container),
                    generation: state.generation(container),
                    prefix: Vec::new(),
                    offset: Linear::constant(0),
                    length: world.measure(
                        container,
                        state.generation(container),
                        CheckedMeasure::Length,
                    ),
                }
            }
            _ => PlaceView::Unknown,
        };
        frame.places.insert(place, view);
    }
    if facts::vacuous(&mut world, clause, &frame) {
        return None;
    }
    let binders: Vec<_> = clause.binders.iter().map(|_| world.opaque(None)).collect();
    let failure = |relation, capacity| RangeIssue::Undischarged {
        node: node.clone(),
        fact: clause.name.clone(),
        site: "a construction",
        relation,
        capacity,
    };
    let Some(formed) = facts::form(&mut world, clause, &frame, &binders, &[]) else {
        return Some(failure(None, None));
    };
    for (position, conclusion) in formed.conclusions.iter().enumerate() {
        let (mut units, choices) = state.premises(&world);
        units.extend(formed.premises.iter().cloned());
        units.extend(conclusion.guards.iter().cloned());
        units.push(super::world::negated(&conclusion.conclusions[0]));
        match facts::judge(
            &mut world,
            &[],
            &[],
            &[],
            Query {
                type_facts: formed.type_facts.clone(),
                units,
                choices,
                rules: formed.conditions.clone(),
                support: formed
                    .conclusions
                    .iter()
                    .zip(&clause.conclusions)
                    .filter(|(_, written)| written.projected)
                    .flat_map(|(rule, _)| rule.guards.iter().chain(&rule.conclusions))
                    .cloned()
                    .collect(),
            },
        ) {
            Ok(Verdict::Refuted) => {}
            Ok(_) => {
                return Some(failure(
                    Some(clause.conclusions[position].node.clone()),
                    None,
                ));
            }
            Err(super::solver::Capacity::Arithmetic) => {
                return Some(RangeIssue::Unsupported {
                    node: node.clone(),
                    feature: super::super::UnsupportedSemanticFeature::RangeArithmetic,
                });
            }
            Err(capacity) => return Some(failure(None, Some(capacity.describe()))),
        }
    }
    None
}
