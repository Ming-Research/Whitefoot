//! The forward walk of one function body [RANGE-2, RANGE-3, RANGE-5].
//!
//! The walk evaluates every statement once over a symbolic state. A branch
//! forks the state and the arms rejoin through a join whose values remember
//! which arm produced them; a loop header forgets what the body writes and
//! assumes the loop's invariants; a call discharges the callee's range
//! requirements and forgets what the callee writes. Every expression the walk
//! does not model evaluates to a fresh opaque value, and every write it
//! cannot place forgets every container, so what the walk does not know it
//! never assumes.

use std::collections::{BTreeMap, BTreeSet};

use crate::NodePath;

use super::super::UnsupportedSemanticFeature;
use super::super::model::{
    BindingId, CheckedAffineExpression, CheckedAffineExpressionKind, CheckedAffineRelation,
    CheckedArrayRoot, CheckedBooleanOperation, CheckedConversionMode, CheckedEnumType,
    CheckedExpression, CheckedFunction, CheckedIntegerOperation, CheckedLoopId, CheckedMatchArm,
    CheckedMeasure, CheckedMode, CheckedNumericType, CheckedPlaceStep, CheckedRangeElementPlace,
    CheckedRangeSource, CheckedSegmentSelect, CheckedSetTarget, CheckedStatement, CheckedType,
    CheckedValue, IntegerType,
};
use super::super::places::PlaceRoot;
use super::super::range_facts::{
    CheckedRangeClause, CheckedRangePlace, CheckedRangeRoot, CheckedRangeShape, CheckedRangeStep,
    CheckedRangeTerm,
};
use super::facts::{self, Fact, Frame, PlaceView, Query};
use super::solver::{Capacity, Linear, Literal, Relation, Verdict};
use super::world::{
    Cond, ContainerId, FactId, Location, Modified, Origin, Slot, State, Step, Value, View, World,
    join, join_states, join_values, negated,
};
use super::{ApartFailure, CertifiedLoop, RangeIssue};

/// One element access recorded while a certificate's iteration runs.
#[derive(Clone, Debug)]
pub(super) struct Access {
    pub(super) container: ContainerId,
    pub(super) indices: Vec<Linear>,
    pub(super) write: bool,
    pub(super) node: NodePath,
    pub(super) conds: Vec<Literal>,
    pub(super) choices: Vec<Vec<Vec<Literal>>>,
}

/// One access a certificate cannot place at one element.
#[derive(Clone, Debug)]
pub(super) struct Unplaced {
    /// The container reached, or every container.
    pub(super) container: Option<ContainerId>,
    pub(super) write: bool,
    /// Whether the access reads or writes a descriptor rather than elements.
    pub(super) descriptor: bool,
    pub(super) node: NodePath,
}

/// What one certificate iteration did.
#[derive(Default)]
pub(super) struct Recording {
    pub(super) accesses: Vec<Access>,
    pub(super) unplaced: Vec<Unplaced>,
}

pub(super) struct Walker<'program> {
    pub(super) functions: &'program [CheckedFunction],
    pub(super) function: &'program CheckedFunction,
    pub(super) world: World,
    pub(super) facts: Vec<Fact>,
    pub(super) issues: Vec<RangeIssue>,
    /// The first loop whose header forgot everything because its nest was
    /// deeper, or its written set took more walks to settle, than this
    /// checker follows.
    pub(super) imprecise: Option<NodePath>,
    pub(super) certified: Vec<CertifiedLoop>,
    /// Nonzero while a loop body is walked only to learn what it writes.
    dry: usize,
    recording: Option<Recording>,
    /// The state at each `break`, by the loop it leaves.
    breaks: BTreeMap<u32, Vec<State>>,
    /// One sink per open value initializer: each `give`'s state and value.
    gives: Vec<Vec<(State, Value)>>,
    /// Each parameter's value at entry.
    entry: BTreeMap<BindingId, Value>,
    /// The node the walk cites for an access it records.
    cite: NodePath,
}

/// The largest number of nested loop dry walks; a deeper nest forgets
/// everything its body could write.
const MAX_DRY_DEPTH: usize = 8;
/// The most dry walks one header takes to settle what its body writes;
/// one that does not settle forgets everything.
const MAX_HEADER_ROUNDS: usize = 8;

pub(super) fn integer_range(ty: IntegerType) -> (i128, i128) {
    match ty {
        IntegerType::I8 => (i8::MIN as i128, i8::MAX as i128),
        IntegerType::I16 => (i16::MIN as i128, i16::MAX as i128),
        IntegerType::I32 => (i32::MIN as i128, i32::MAX as i128),
        IntegerType::I64 => (i64::MIN as i128, i64::MAX as i128),
        IntegerType::U8 => (0, u8::MAX as i128),
        IntegerType::U16 => (0, u16::MAX as i128),
        IntegerType::U32 => (0, u32::MAX as i128),
        IntegerType::U64 => (0, u64::MAX as i128),
    }
}

/// Whether an aggregate `expression` yields is a copy of storage: it reads
/// a binding or a place, and does not consume it. A consuming read hands
/// the storage itself over, and any other expression, such as a call,
/// yields a new value.
const fn copies(expression: &CheckedExpression) -> bool {
    let consumed = matches!(
        expression,
        CheckedExpression::Binding {
            consume_root: true,
            ..
        } | CheckedExpression::Project {
            consume_root: true,
            ..
        }
    );
    let reads_storage = matches!(
        expression,
        CheckedExpression::Binding { .. }
            | CheckedExpression::Project { .. }
            | CheckedExpression::ProjectValue { .. }
            | CheckedExpression::ArrayIndex { .. }
            | CheckedExpression::RangeIndex { .. }
            | CheckedExpression::BufferIndex { .. }
            | CheckedExpression::ReadStorage { .. }
            | CheckedExpression::BoxDeref { .. }
            | CheckedExpression::DerefAddressed { .. }
    );
    reads_storage && !consumed
}

const fn integer_type(ty: CheckedType) -> Option<IntegerType> {
    match ty {
        CheckedType::Integer(integer) => Some(integer),
        _ => None,
    }
}

fn literal(left: Linear, relation: Relation, right: Linear) -> Literal {
    Literal::new(left, relation, right)
}

impl<'program> Walker<'program> {
    pub(super) fn new(
        functions: &'program [CheckedFunction],
        function: &'program CheckedFunction,
    ) -> Self {
        Self {
            functions,
            function,
            world: World::default(),
            facts: Vec::new(),
            issues: Vec::new(),
            imprecise: None,
            certified: Vec::new(),
            dry: 0,
            recording: None,
            breaks: BTreeMap::new(),
            gives: Vec::new(),
            entry: BTreeMap::new(),
            cite: empty_path(),
        }
    }

    // ----- entry -----

    pub(super) fn run(&mut self) {
        let function = self.function;
        let mut state = State::default();
        for parameter in &function.parameters {
            let value = match parameter.mode {
                CheckedMode::Range => {
                    let location = Location::root(Origin::Parameter(parameter.binding));
                    match self.world.container(location, 1) {
                        Some(container) => {
                            let length = self.world.measure(container, 0, CheckedMeasure::Length);
                            Value::Ref(View::Run {
                                container,
                                prefix: Vec::new(),
                                offset: Linear::constant(0),
                                length,
                            })
                        }
                        None => Value::Ref(View::Unknown),
                    }
                }
                CheckedMode::Reference => Value::Ref(View::Place(Location::root(
                    Origin::Parameter(parameter.binding),
                ))),
                CheckedMode::Own => match parameter.ty {
                    CheckedType::Integer(ty) => Value::Int(self.world.opaque(Some(ty))),
                    CheckedType::Bool => Value::Bool(Cond::Unknown),
                    _ => Value::Owned(Location::root(Origin::Parameter(parameter.binding))),
                },
            };
            self.entry.insert(parameter.binding, value.clone());
            state.values.insert(parameter.binding, value);
        }
        // Affine requirements hold at entry.
        for requirement in &function.requirements {
            if let Some(literal) = self.goal_literal(&state, &requirement.template.root) {
                state.conds.push(literal);
            }
        }
        for clause in &function.range_facts.requirements {
            let frame = self.frame(&mut state.clone(), clause, &|root| match root {
                CheckedRangeRoot::Binding(binding) => state.values.get(&binding).cloned(),
                _ => None,
            });
            let id = self.add_fact(clause.clone(), frame);
            state.facts.push(id);
        }
        let body = function.body.as_deref().unwrap_or_default();
        self.block(state, body);
    }

    pub(super) fn add_fact(&mut self, clause: CheckedRangeClause, frame: Frame) -> FactId {
        let id = FactId::try_from(self.facts.len()).unwrap_or(FactId::MAX);
        self.facts.push(Fact { clause, frame });
        id
    }

    // ----- frames -----

    /// What each place and value of `clause` denotes in `state`, with each
    /// root's value given by `roots`.
    pub(super) fn frame(
        &mut self,
        state: &mut State,
        clause: &CheckedRangeClause,
        roots: &dyn Fn(CheckedRangeRoot) -> Option<Value>,
    ) -> Frame {
        let mut frame = Frame::default();
        let mut segments: BTreeSet<CheckedRangePlace> = BTreeSet::new();
        let mut terms: Vec<&CheckedRangeTerm> = Vec::new();
        for binder in &clause.binders {
            terms.push(&binder.start);
            terms.push(&binder.end);
        }
        for relation in clause.relations() {
            terms.push(&relation.left);
            terms.push(&relation.right);
        }
        for term in &terms {
            segment_places(term, &mut segments);
        }
        for place in clause.places() {
            let view = self.place_view(state, &place, segments.contains(&place), roots);
            frame.places.insert(place, view);
        }
        let mut values = Vec::new();
        for term in &terms {
            term.collect_values(&mut values);
        }
        for root in values {
            let value = match roots(root) {
                Some(Value::Int(value)) => value,
                _ => self.world.opaque(None),
            };
            frame.values.insert(root, value);
        }
        frame
    }

    fn place_view(
        &mut self,
        state: &mut State,
        place: &CheckedRangePlace,
        segments: bool,
        roots: &dyn Fn(CheckedRangeRoot) -> Option<Value>,
    ) -> PlaceView {
        enum Reached {
            Location(Location),
            Run(View),
        }
        let mut reached = match roots(place.root) {
            Some(Value::Owned(location)) => Reached::Location(location),
            Some(Value::Ref(view)) => Reached::Run(view),
            _ => return PlaceView::Unknown,
        };
        for step in &place.path {
            reached = match (reached, step) {
                (Reached::Run(View::Place(location)), CheckedRangeStep::Referent) => {
                    Reached::Location(location)
                }
                (Reached::Run(view @ View::Run { .. }), CheckedRangeStep::Referent) => {
                    Reached::Run(view)
                }
                (Reached::Location(location), CheckedRangeStep::Field(field)) => {
                    Reached::Location(location.child(Step::Field(*field)))
                }
                (Reached::Location(location), CheckedRangeStep::BoxContent) => {
                    Reached::Location(location.child(Step::BoxContent))
                }
                _ => return PlaceView::Unknown,
            };
        }
        match reached {
            Reached::Run(View::Run {
                container,
                prefix,
                offset,
                length,
            }) => PlaceView::Run {
                container,
                version: state.version(&mut self.world, container),
                generation: state.generation(container),
                prefix,
                offset,
                length,
            },
            Reached::Location(location) => {
                let location = state.resolve(&location);
                let Some(container) = self.world.container(location, if segments { 2 } else { 1 })
                else {
                    return PlaceView::Unknown;
                };
                let version = state.version(&mut self.world, container);
                let generation = state.generation(container);
                let length = self
                    .world
                    .measure(container, generation, CheckedMeasure::Length);
                if segments {
                    PlaceView::Segments {
                        container,
                        version,
                        generation,
                        rows: length,
                    }
                } else {
                    PlaceView::Run {
                        container,
                        version,
                        generation,
                        prefix: Vec::new(),
                        offset: Linear::constant(0),
                        length,
                    }
                }
            }
            Reached::Run(_) => PlaceView::Unknown,
        }
    }

    // ----- obligations -----

    /// Whether `clause`, framed by `frame`, holds in `state`: for fresh bound
    /// variables in range whose guards hold, every conclusion holds.
    pub(super) fn holds(
        &mut self,
        state: &State,
        clause: &CheckedRangeClause,
        frame: &Frame,
    ) -> Result<Option<Option<NodePath>>, Capacity> {
        let binders: Vec<Linear> = clause
            .binders
            .iter()
            .map(|_| self.world.opaque(None))
            .collect();
        let Some(formed) = facts::form(&mut self.world, clause, frame, &binders, &[]) else {
            return Ok(Some(None));
        };
        for (position, conclusion) in formed.conclusions.iter().enumerate() {
            let (units, choices) = state.premises(&self.world);
            let mut query = Query {
                units,
                choices,
                ..Query::default()
            };
            query.units.extend(formed.premises.iter().cloned());
            let negation = conclusion_negation(conclusion);
            match negation.as_slice() {
                [single] => query.units.push(single.clone()),
                _ => query
                    .choices
                    .push(negation.into_iter().map(|literal| vec![literal]).collect()),
            }
            let verdict = facts::judge(&mut self.world, &self.facts, &state.facts, &[], query)?;
            if verdict != Verdict::Refuted {
                let relation = clause.conclusions[position].node.clone();
                return Ok(Some(
                    (!relation.components().is_empty()).then_some(relation),
                ));
            }
        }
        Ok(None)
    }

    fn require(
        &mut self,
        state: &State,
        clause: &CheckedRangeClause,
        frame: &Frame,
        node: &NodePath,
        site: &'static str,
    ) {
        if self.dry > 0 {
            return;
        }
        match self.holds(state, clause, frame) {
            Ok(None) => {}
            Ok(Some(relation)) => self.issues.push(RangeIssue::Undischarged {
                node: node.clone(),
                fact: clause.name.clone(),
                site,
                relation,
                capacity: None,
            }),
            Err(Capacity::Arithmetic) => self.issues.push(RangeIssue::Unsupported {
                node: node.clone(),
                feature: UnsupportedSemanticFeature::RangeArithmetic,
            }),
            Err(capacity) => self.issues.push(RangeIssue::Undischarged {
                node: node.clone(),
                fact: clause.name.clone(),
                site,
                relation: None,
                capacity: Some(capacity.describe()),
            }),
        }
    }

    // ----- statements -----

    pub(super) fn block(&mut self, state: State, statements: &[CheckedStatement]) -> Option<State> {
        let mut state = state;
        for statement in statements {
            state = self.statement(state, statement)?;
        }
        Some(state)
    }

    fn statement(&mut self, mut state: State, statement: &CheckedStatement) -> Option<State> {
        match statement {
            CheckedStatement::Let {
                node_path,
                binding,
                value,
            } => {
                self.cite = node_path.clone();
                let evaluated = self.stored_value(&mut state, value);
                self.bind(&mut state, *binding, value.ty(), evaluated);
                Some(state)
            }
            CheckedStatement::DestructuringLet {
                node_path,
                bindings,
                value,
                ..
            } => {
                self.cite = node_path.clone();
                let evaluated = self.eval(&mut state, value);
                let copied = copies(value);
                for (binding, ty, ordinal) in bindings {
                    let field = match &evaluated {
                        Value::Owned(location) => match self.read_location(
                            &mut state,
                            &location.child(Step::Field(*ordinal)),
                            *ty,
                        ) {
                            // A field of storage that is not handed over is
                            // a copy; a statement form that always consumes
                            // its operand [OWN-1] never reaches this arm.
                            Value::Owned(_) if copied => self.copied(),
                            other => other,
                        },
                        Value::Struct(fields) => fields
                            .get(*ordinal as usize)
                            .cloned()
                            .unwrap_or(Value::Unknown),
                        _ => Value::Unknown,
                    };
                    self.bind(&mut state, *binding, *ty, field);
                }
                Some(state)
            }
            CheckedStatement::PropagateLet {
                node_path,
                binding,
                scrutinee,
                ok_type,
                ..
            } => {
                self.cite = node_path.clone();
                let _ = self.eval(&mut state, scrutinee);
                self.bind(&mut state, *binding, *ok_type, Value::Unknown);
                Some(state)
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                self.cite = node_path.clone();
                let evaluated = self.stored_value(&mut state, value);
                self.set(&mut state, target, evaluated, node_path);
                Some(state)
            }
            CheckedStatement::Evaluate { node_path, value }
            | CheckedStatement::DropExpression {
                node_path, value, ..
            } => {
                self.cite = node_path.clone();
                let _ = self.eval(&mut state, value);
                Some(state)
            }
            CheckedStatement::Proof(proof) => {
                if let Some(literals) = self.affine_relation(&state, &proof.target) {
                    state.conds.extend(literals);
                }
                Some(state)
            }
            CheckedStatement::Return {
                node_path, value, ..
            } => {
                // No range fact is owed at a return [RANGE-3]: the returned
                // value is evaluated only for the obligations it contains.
                self.cite = node_path.clone();
                self.eval(&mut state, value);
                None
            }
            CheckedStatement::Match {
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                let fork = state.conds.len();
                let arms_out = self.arms(&mut state, scrutinee, *enum_type, arms);
                join(&mut self.world, fork, arms_out)
            }
            CheckedStatement::ValueMatchLet {
                node_path,
                binding,
                result_type,
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                self.cite = node_path.clone();
                let fork = state.conds.len();
                self.gives.push(Vec::new());
                let _ = self.arms(&mut state, scrutinee, *enum_type, arms);
                let given = self.gives.pop().unwrap_or_default();
                if given.is_empty() {
                    return None;
                }
                let (states, values): (Vec<State>, Vec<Value>) = given.into_iter().unzip();
                let (mut joined, join_id) = join_states(&mut self.world, fork, states)?;
                let value = match join_id {
                    None => values.into_iter().next().unwrap_or(Value::Unknown),
                    Some(join_id) => {
                        let all: Vec<&Value> = values.iter().collect();
                        join_values(&mut self.world, join_id, &all)
                    }
                };
                self.bind(&mut joined, *binding, *result_type, value);
                Some(joined)
            }
            CheckedStatement::Give {
                node_path, value, ..
            } => {
                self.cite = node_path.clone();
                let given = self.stored_value(&mut state, value);
                if let Some(sink) = self.gives.last_mut() {
                    sink.push((state, given));
                }
                None
            }
            CheckedStatement::Loop {
                id,
                node_path,
                body,
                ..
            } => {
                self.cite = node_path.clone();
                self.unbounded_loop(state, *id, body)
            }
            CheckedStatement::CountedRange {
                id,
                node_path,
                binder,
                lower,
                upper,
                invariants,
                body,
                ..
            } => {
                self.cite = node_path.clone();
                self.counted_loop(
                    state, *id, node_path, *binder, lower, upper, invariants, body,
                )
            }
            CheckedStatement::Break { target, .. } => {
                self.breaks.entry(target.0).or_default().push(state);
                None
            }
            CheckedStatement::Atomic {
                node_path,
                target,
                binding,
                guard,
                body,
                continues,
                ..
            } => {
                self.cite = node_path.clone();
                let _ = self.eval(&mut state, target);
                state.havoc_everything(&mut self.world);
                self.unplaced(None, true, true);
                state.values.insert(*binding, Value::Ref(View::Unknown));
                if let Some(guard) = guard {
                    let _ = self.eval(&mut state, guard);
                }
                let after = self.block(state, body);
                if !continues {
                    return None;
                }
                let mut after = after?;
                after.havoc_everything(&mut self.world);
                Some(after)
            }
        }
    }

    /// Walks the arms of one `match` or `if`, each from its own fork of
    /// `state`, and returns the live ones.
    fn arms(
        &mut self,
        state: &mut State,
        scrutinee: &CheckedExpression,
        enum_type: CheckedEnumType,
        arms: &[CheckedMatchArm],
    ) -> Vec<State> {
        let value = self.eval(state, scrutinee);
        // A by-value binder of storage the match does not consume is a copy
        // of the payload, as a `let` of it would be.
        let copied = copies(scrutinee);
        let mut live = Vec::new();
        match enum_type {
            CheckedEnumType::Bool => {
                let cond = match value {
                    Value::Bool(cond) => cond,
                    _ => Cond::Unknown,
                };
                for arm in arms {
                    let truth = arm.tag == 1;
                    if cond.excludes(truth) {
                        continue;
                    }
                    let mut forked = state.clone();
                    cond.literals(truth, &mut forked.conds);
                    if let Some(after) = self.block(forked, &arm.body) {
                        live.push(after);
                    }
                }
            }
            CheckedEnumType::Nominal(_) => {
                let (location, fields, known) = match &value {
                    Value::Owned(location) => {
                        let location = state.resolve(location);
                        let known = state.variants.get(&location).copied();
                        (Some(location), None, known)
                    }
                    Value::Variant { variant, fields } => {
                        (None, Some(fields.clone()), Some(*variant))
                    }
                    _ => (None, None, None),
                };
                for arm in arms {
                    if known.is_some_and(|variant| variant != arm.tag) {
                        continue;
                    }
                    let mut forked = state.clone();
                    if let Some(location) = &location {
                        forked.variants.insert(location.clone(), arm.tag);
                        let enabled: Vec<FactId> = forked
                            .routed
                            .iter()
                            .filter(|(at, variant, _)| at == location && *variant == arm.tag)
                            .map(|(_, _, fact)| *fact)
                            .collect();
                        forked.facts.extend(enabled);
                    }
                    for binder in &arm.binders {
                        let bound = match (&location, &fields) {
                            (_, Some(fields)) => fields
                                .get(binder.field as usize)
                                .cloned()
                                .unwrap_or(Value::Unknown),
                            (Some(location), None) => {
                                let payload = location.child(Step::Payload {
                                    variant: arm.tag,
                                    field: binder.field,
                                });
                                if binder.mode.is_reference() {
                                    Value::Ref(View::Place(forked.resolve(&payload)))
                                } else {
                                    match self.read_location(&mut forked, &payload, binder.ty) {
                                        Value::Owned(_) if copied => self.copied(),
                                        other => other,
                                    }
                                }
                            }
                            (None, None) => Value::Unknown,
                        };
                        let ty = binder.ty;
                        self.bind(&mut forked, binder.binding, ty, bound);
                    }
                    if let Some(after) = self.block(forked, &arm.body) {
                        live.push(after);
                    }
                }
            }
        }
        live
    }

    /// Binds `value` to `binding`, giving an aggregate a location.
    fn bind(&mut self, state: &mut State, binding: BindingId, ty: CheckedType, value: Value) {
        let value = match value {
            Value::Struct(fields) => {
                let location = Location::root(Origin::Constructed(self.world.new_origin()));
                self.place_fields(state, &location, &fields, None);
                Value::Owned(location)
            }
            Value::Variant { variant, fields } => {
                let location = Location::root(Origin::Constructed(self.world.new_origin()));
                self.place_fields(state, &location, &fields, Some(variant));
                state.variants.insert(location.clone(), variant);
                Value::Owned(location)
            }
            Value::Unknown => match ty {
                CheckedType::Integer(integer) => Value::Int(self.world.opaque(Some(integer))),
                CheckedType::Bool => Value::Bool(Cond::Unknown),
                CheckedType::Unit | CheckedType::Float(_) => Value::Unknown,
                _ => Value::Owned(Location::root(Origin::Binding(
                    binding,
                    self.world.new_origin(),
                ))),
            },
            other => other,
        };
        state.set_value(&mut self.world, binding, value);
    }

    fn place_fields(
        &mut self,
        state: &mut State,
        location: &Location,
        fields: &[Value],
        variant: Option<u32>,
    ) {
        for (index, field) in fields.iter().enumerate() {
            let step = match variant {
                Some(variant) => Step::Payload {
                    variant,
                    field: index as u32,
                },
                None => Step::Field(index as u32),
            };
            let at = location.child(step);
            match field {
                Value::Int(value) => state.set_slot(&mut self.world, at, Slot::Int(value.clone())),
                Value::Owned(source) => {
                    let source = state.resolve(source);
                    state.set_slot(&mut self.world, at, Slot::Alias(source));
                }
                Value::Ref(view) => state.set_slot(&mut self.world, at, Slot::Ref(view.clone())),
                Value::Struct(inner) => {
                    let inner = inner.clone();
                    self.place_fields(state, &at, &inner, None);
                }
                Value::Variant {
                    variant: inner_variant,
                    fields: inner,
                } => {
                    let inner = inner.clone();
                    self.place_fields(state, &at, &inner, Some(*inner_variant));
                    state.variants.insert(state.resolve(&at), *inner_variant);
                }
                Value::Bool(_) | Value::Unknown => {}
            }
        }
    }

    /// The value stored at `location`, typed `ty`.
    fn read_location(&mut self, state: &mut State, location: &Location, ty: CheckedType) -> Value {
        let location = state.resolve(location);
        match state.slots.get(&location) {
            Some(Slot::Int(value)) => return Value::Int(value.clone()),
            Some(Slot::Ref(view)) => return Value::Ref(view.clone()),
            _ => {}
        }
        match ty {
            CheckedType::Integer(integer) => {
                // One unknown field reads as one value until it is written.
                let value = self.world.opaque(Some(integer));
                state.slots.insert(location, Slot::Int(value.clone()));
                Value::Int(value)
            }
            CheckedType::Bool => Value::Bool(Cond::Unknown),
            CheckedType::Unit | CheckedType::Float(_) => Value::Unknown,
            _ => Value::Owned(location),
        }
    }

    // ----- writes -----

    fn set(&mut self, state: &mut State, target: &CheckedSetTarget, value: Value, node: &NodePath) {
        let stored = match &value {
            Value::Int(value) => Some(value.clone()),
            _ => None,
        };
        match target {
            CheckedSetTarget::Place(place) => {
                if place.fields.is_empty() {
                    if place.mode.is_reference() {
                        // A reference rebinding: the name now reaches other
                        // storage, which a loop header cannot summarize.
                        if let Some(log) = &mut self.world.log {
                            log.everything = true;
                        }
                        state.set_value(&mut self.world, place.binding, value);
                        return;
                    }
                    if let Some(Value::Owned(location)) = state.values.get(&place.binding).cloned()
                    {
                        state.havoc_location(&mut self.world, &location);
                        self.unplaced_location(state, &location, node);
                    }
                    self.bind(state, place.binding, place.ty, value);
                    return;
                }
                let Some(Value::Owned(base)) = state.values.get(&place.binding).cloned() else {
                    self.forget_all(state, node);
                    return;
                };
                let mut location = base;
                for field in &place.fields {
                    location = location.child(Step::Field(*field));
                }
                self.store(state, &location, place.ty, value, node);
            }
            CheckedSetTarget::RangeIndex(place) => match self.range_element(state, place) {
                Some((container, indices, whole)) => {
                    self.access(container, &indices, true, node, state);
                    state.write_element(
                        &mut self.world,
                        container,
                        indices,
                        if whole { stored } else { None },
                    );
                }
                None => self.forget_all(state, node),
            },
            CheckedSetTarget::Storage(root) => {
                match self.path_target(state, root.root, &root.path) {
                    Target::Element {
                        container,
                        indices,
                        below,
                    } => {
                        self.access(container, &indices, true, node, state);
                        state.write_element(
                            &mut self.world,
                            container,
                            indices,
                            if below { None } else { stored },
                        );
                    }
                    Target::Location(location) => {
                        self.store(state, &location, root.ty, value, node);
                    }
                    Target::Row { container, .. } | Target::Run { container, .. } => {
                        state.havoc_container(&mut self.world, container, false);
                        self.unplaced(Some(container), true, false);
                    }
                    Target::Unknown => self.forget_all(state, node),
                }
            }
        }
    }

    fn store(
        &mut self,
        state: &mut State,
        location: &Location,
        ty: CheckedType,
        value: Value,
        node: &NodePath,
    ) {
        let location = state.resolve(location);
        state.havoc_location(&mut self.world, &location);
        self.unplaced_location(state, &location, node);
        match value {
            Value::Int(value) => state.set_slot(&mut self.world, location, Slot::Int(value)),
            Value::Ref(view) => state.set_slot(&mut self.world, location, Slot::Ref(view)),
            Value::Owned(source) => {
                let source = state.resolve(&source);
                state.set_slot(&mut self.world, location, Slot::Alias(source));
            }
            Value::Struct(fields) => self.place_fields(state, &location, &fields, None),
            Value::Variant { variant, fields } => {
                self.place_fields(state, &location, &fields, Some(variant));
                state.variants.insert(location, variant);
            }
            Value::Bool(_) | Value::Unknown => {
                let _ = ty;
            }
        }
    }

    fn forget_all(&mut self, state: &mut State, node: &NodePath) {
        let _ = node;
        state.havoc_everything(&mut self.world);
        self.unplaced(None, true, true);
    }

    // ----- places -----

    /// Where one storage path leads.
    fn path_target(
        &mut self,
        state: &mut State,
        root: PlaceRoot,
        path: &[CheckedPlaceStep],
    ) -> Target {
        let PlaceRoot::Binding(binding) = root else {
            return Target::Unknown;
        };
        let mut target = match state.values.get(&binding).cloned() {
            Some(Value::Owned(location)) => Target::Location(location),
            Some(Value::Ref(View::Place(location))) => Target::Location(location),
            Some(Value::Ref(View::Run {
                container,
                prefix,
                offset,
                ..
            })) => Target::Run {
                container,
                prefix,
                offset,
            },
            Some(Value::Ref(View::Element { container, indices })) => Target::Element {
                container,
                indices,
                below: false,
            },
            _ => Target::Unknown,
        };
        for step in path {
            target = match (target, step) {
                (Target::Location(location), CheckedPlaceStep::Field(field)) => {
                    Target::Location(location.child(Step::Field(*field)))
                }
                (Target::Location(location), CheckedPlaceStep::BoxReferent(_)) => {
                    Target::Location(location.child(Step::BoxContent))
                }
                (Target::Location(location), CheckedPlaceStep::Subscript(subscript)) => {
                    let index = self.int(state, &subscript.offset);
                    let location = state.resolve(&location);
                    if matches!(subscript.base_type, CheckedType::Segments { .. }) {
                        match self.world.container(location, 2) {
                            Some(container) => Target::Row {
                                container,
                                row: index,
                            },
                            None => Target::Unknown,
                        }
                    } else {
                        match self.world.container(location, 1) {
                            Some(container) => Target::Element {
                                container,
                                indices: vec![index],
                                below: false,
                            },
                            None => Target::Unknown,
                        }
                    }
                }
                (Target::Row { container, row }, CheckedPlaceStep::Subscript(subscript)) => {
                    let index = self.int(state, &subscript.offset);
                    Target::Element {
                        container,
                        indices: vec![row, index],
                        below: false,
                    }
                }
                (
                    Target::Run {
                        container,
                        prefix,
                        offset,
                    },
                    CheckedPlaceStep::Subscript(subscript),
                ) => {
                    let index = self.int(state, &subscript.offset);
                    let mut indices = prefix;
                    match offset.plus(&index) {
                        Some(absolute) => {
                            indices.push(absolute);
                            Target::Element {
                                container,
                                indices,
                                below: false,
                            }
                        }
                        None => Target::Unknown,
                    }
                }
                (
                    Target::Element {
                        container, indices, ..
                    },
                    _,
                ) => Target::Element {
                    container,
                    indices,
                    below: true,
                },
                (_, CheckedPlaceStep::Subscript(subscript)) => {
                    let _ = self.int(state, &subscript.offset);
                    Target::Unknown
                }
                _ => Target::Unknown,
            };
        }
        target
    }

    /// The element one range element place selects: its container, its
    /// index tuple, and whether it is the whole element.
    fn range_element(
        &mut self,
        state: &mut State,
        place: &CheckedRangeElementPlace,
    ) -> Option<(ContainerId, Vec<Linear>, bool)> {
        let index = self.int(state, &place.offset);
        let Some(Value::Ref(View::Run {
            container,
            prefix,
            offset,
            ..
        })) = state.values.get(&place.root.binding).cloned()
        else {
            return None;
        };
        let mut indices = prefix;
        indices.push(offset.plus(&index)?);
        Some((container, indices, place.path.is_empty()))
    }

    fn view_of(&mut self, state: &mut State, root: PlaceRoot, path: &[CheckedPlaceStep]) -> View {
        match self.path_target(state, root, path) {
            Target::Location(location) => View::Place(state.resolve(&location)),
            Target::Element {
                container,
                indices,
                below: false,
            } => View::Element { container, indices },
            Target::Row { container, row } => {
                let generation = state.generation(container);
                let length = self
                    .world
                    .segment_length(container, generation, row.clone());
                View::Run {
                    container,
                    prefix: vec![row],
                    offset: Linear::constant(0),
                    length,
                }
            }
            Target::Run {
                container,
                prefix,
                offset,
            } => {
                let _ = (container, prefix, offset);
                View::Unknown
            }
            _ => View::Unknown,
        }
    }

    /// The run a range source names.
    fn run_of(&mut self, state: &mut State, location: Location) -> View {
        let location = state.resolve(&location);
        match self.world.container(location, 1) {
            Some(container) => {
                let generation = state.generation(container);
                let length = self
                    .world
                    .measure(container, generation, CheckedMeasure::Length);
                View::Run {
                    container,
                    prefix: Vec::new(),
                    offset: Linear::constant(0),
                    length,
                }
            }
            None => View::Unknown,
        }
    }

    // ----- accesses -----

    fn access(
        &mut self,
        container: ContainerId,
        indices: &[Linear],
        write: bool,
        node: &NodePath,
        state: &State,
    ) {
        if self.recording.is_some() {
            let (conds, choices) = state.premises(&self.world);
            if let Some(recording) = &mut self.recording {
                recording.accesses.push(Access {
                    container,
                    indices: indices.to_vec(),
                    write,
                    node: node.clone(),
                    conds,
                    choices,
                });
            }
        }
    }

    fn unplaced(&mut self, container: Option<ContainerId>, write: bool, descriptor: bool) {
        let node = self.cite.clone();
        if let Some(recording) = &mut self.recording {
            recording.unplaced.push(Unplaced {
                container,
                write,
                descriptor,
                node,
            });
        }
    }

    fn unplaced_location(&mut self, state: &State, location: &Location, node: &NodePath) {
        if self.recording.is_none() {
            return;
        }
        let _ = node;
        let location = state.resolve(location);
        for container in self.world.containers_under(&location) {
            self.unplaced(Some(container), true, true);
        }
    }

    fn unplaced_view(&mut self, view: &View, write: bool, node: &NodePath, state: &State) {
        match view {
            View::Run { container, .. } => self.unplaced(Some(*container), write, false),
            View::Element { container, indices } => {
                self.access(*container, indices, write, node, state);
            }
            View::Place(location) => {
                let location = state.resolve(location);
                for container in self.world.containers_under(&location) {
                    self.unplaced(Some(container), write, write);
                }
            }
            View::Unknown => self.unplaced(None, write, write),
        }
    }

    // ----- expressions -----

    fn int(&mut self, state: &mut State, expression: &CheckedExpression) -> Linear {
        match self.eval(state, expression) {
            Value::Int(value) => value,
            _ => self.world.opaque(integer_type(expression.ty())),
        }
    }

    fn opaque_of(&mut self, ty: CheckedType) -> Value {
        match ty {
            CheckedType::Integer(integer) => Value::Int(self.world.opaque(Some(integer))),
            CheckedType::Bool => Value::Bool(Cond::Unknown),
            _ => Value::Unknown,
        }
    }

    /// A value about to be stored in a binding, a place or a field. Reading
    /// an aggregate that already lives in storage yields that storage's
    /// location; only a consuming read hands the storage itself over, and
    /// any other read is a copy, whose contents this walk then forgets, so
    /// that a write to the copy can never be taken for a write to its
    /// source.
    fn stored_value(&mut self, state: &mut State, expression: &CheckedExpression) -> Value {
        let value = self.eval(state, expression);
        match value {
            Value::Owned(_) if copies(expression) => self.copied(),
            other => other,
        }
    }

    /// A copy of an aggregate: new storage whose contents are unknown.
    fn copied(&mut self) -> Value {
        Value::Owned(Location::root(Origin::Constructed(self.world.new_origin())))
    }

    pub(super) fn eval(&mut self, state: &mut State, expression: &CheckedExpression) -> Value {
        match expression {
            CheckedExpression::Constant(value) | CheckedExpression::NamedConstant { value, .. } => {
                constant(value)
            }
            CheckedExpression::Binding { binding, ty, .. } => match state.values.get(binding) {
                Some(value) => value.clone(),
                None => self.opaque_of(*ty),
            },
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                result,
                formal_effects,
                ..
            } => self.call(
                state,
                *function,
                call,
                arguments,
                *result,
                formal_effects.as_deref(),
            ),
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                result,
                ..
            } => self.integer_operation(state, *operation, arguments, *result),
            CheckedExpression::NumericConversion {
                mode,
                source,
                destination,
                value,
                result,
                ..
            } => {
                let converted = self.eval(state, value);
                match (mode, source, destination, converted) {
                    (
                        CheckedConversionMode::Exact,
                        CheckedNumericType::Integer(_),
                        CheckedNumericType::Integer(_),
                        Value::Int(value),
                    ) => Value::Int(value),
                    _ => self.opaque_of(*result),
                }
            }
            CheckedExpression::BooleanOperation {
                operation,
                arguments,
                ..
            } => {
                let mut parts = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    parts.push(match self.eval(state, argument) {
                        Value::Bool(cond) => cond,
                        _ => Cond::Unknown,
                    });
                }
                Value::Bool(match operation {
                    CheckedBooleanOperation::And => Cond::And(parts),
                    CheckedBooleanOperation::Or => Cond::Or(parts),
                    CheckedBooleanOperation::Not => match parts.pop() {
                        Some(part) => Cond::Not(Box::new(part)),
                        None => Cond::Unknown,
                    },
                    CheckedBooleanOperation::ExclusiveOr => Cond::Unknown,
                })
            }
            CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. } => {
                for argument in arguments {
                    let _ = self.eval(state, argument);
                }
                self.opaque_of(expression.ty())
            }
            CheckedExpression::Reinterpret { value, .. } => {
                let _ = self.eval(state, value);
                self.opaque_of(expression.ty())
            }
            CheckedExpression::ArrayMeasure {
                root,
                length,
                measure,
            } => match (root, length.value()) {
                (_, Some(value)) if *measure != CheckedMeasure::Head => {
                    Value::Int(Linear::constant(i128::from(value)))
                }
                (CheckedArrayRoot::Binding { binding, fields }, _) => {
                    let path: Vec<CheckedPlaceStep> = fields
                        .iter()
                        .map(|field| CheckedPlaceStep::Field(*field))
                        .collect();
                    match self.path_target(state, PlaceRoot::Binding(*binding), &path) {
                        Target::Location(location) => {
                            let location = state.resolve(&location);
                            match self.world.container(location, 1) {
                                Some(container) => {
                                    let generation = state.generation(container);
                                    Value::Int(self.world.measure(container, generation, *measure))
                                }
                                None => self.opaque_of(expression.ty()),
                            }
                        }
                        _ => self.opaque_of(expression.ty()),
                    }
                }
                _ => self.opaque_of(expression.ty()),
            },
            CheckedExpression::ArrayIndex {
                carrier,
                root,
                offset,
                element_type,
                ..
            } => {
                let index = self.int(state, offset);
                match root {
                    CheckedArrayRoot::Binding { binding, fields } => {
                        let path: Vec<CheckedPlaceStep> = fields
                            .iter()
                            .map(|field| CheckedPlaceStep::Field(*field))
                            .collect();
                        match self.path_target(state, PlaceRoot::Binding(*binding), &path) {
                            Target::Location(location) => {
                                let location = state.resolve(&location);
                                match self.world.container(location, 1) {
                                    Some(container) => self.read_element(
                                        state,
                                        container,
                                        vec![index],
                                        *element_type,
                                        carrier,
                                    ),
                                    None => self.opaque_of(*element_type),
                                }
                            }
                            _ => self.opaque_of(*element_type),
                        }
                    }
                    CheckedArrayRoot::Constant(_) => self.opaque_of(*element_type),
                }
            }
            CheckedExpression::BufferIndex {
                carrier,
                root,
                offset,
                ..
            } => {
                let index = self.int(state, offset);
                let path: Vec<CheckedPlaceStep> = root.path.clone();
                match self.path_target(state, PlaceRoot::Binding(root.binding), &path) {
                    Target::Location(location) => {
                        let location = state.resolve(&location);
                        match self.world.container(location, 1) {
                            Some(container) => self.read_element(
                                state,
                                container,
                                vec![index],
                                root.element_type,
                                carrier,
                            ),
                            None => self.opaque_of(root.element_type),
                        }
                    }
                    _ => self.opaque_of(root.element_type),
                }
            }
            CheckedExpression::ReadStorage { carrier, root } => {
                match self.path_target(state, root.root, &root.path) {
                    Target::Element {
                        container,
                        indices,
                        below,
                    } => {
                        if below {
                            self.access(container, &indices, false, carrier, state);
                            self.opaque_of(root.ty)
                        } else {
                            self.read_element(state, container, indices, root.ty, carrier)
                        }
                    }
                    Target::Location(location) => self.read_location(state, &location, root.ty),
                    _ => self.opaque_of(root.ty),
                }
            }
            CheckedExpression::ContainerMeasure { measure, root } => {
                match self.path_target(state, root.root, &root.path) {
                    Target::Location(location) => {
                        let location = state.resolve(&location);
                        let arity = if matches!(root.ty, CheckedType::Segments { .. }) {
                            2
                        } else {
                            1
                        };
                        match self.world.container(location, arity) {
                            Some(container) => {
                                let generation = state.generation(container);
                                Value::Int(self.world.measure(container, generation, *measure))
                            }
                            None => self.opaque_of(expression.ty()),
                        }
                    }
                    Target::Row { container, row } if *measure == CheckedMeasure::Length => {
                        let generation = state.generation(container);
                        Value::Int(self.world.segment_length(container, generation, row))
                    }
                    _ => self.opaque_of(expression.ty()),
                }
            }
            CheckedExpression::RangeMeasure { measure, root } => {
                match (state.values.get(&root.binding), measure) {
                    (Some(Value::Ref(View::Run { length, .. })), CheckedMeasure::Length) => {
                        Value::Int(length.clone())
                    }
                    _ => self.opaque_of(expression.ty()),
                }
            }
            CheckedExpression::RangeIndex { carrier, place } => {
                match self.range_element(state, place) {
                    Some((container, indices, true)) => {
                        self.read_element(state, container, indices, place.ty, carrier)
                    }
                    Some((container, indices, false)) => {
                        self.access(container, &indices, false, carrier, state);
                        self.opaque_of(place.ty)
                    }
                    None => self.opaque_of(place.ty),
                }
            }
            CheckedExpression::RangeElementMeasure { carrier, place, .. } => {
                if let Some((container, indices, _)) = self.range_element(state, place) {
                    self.access(container, &indices, false, carrier, state);
                }
                self.opaque_of(expression.ty())
            }
            CheckedExpression::BorrowRangeIndex { place, .. } => {
                match self.range_element(state, place) {
                    Some((container, indices, true)) => {
                        Value::Ref(View::Element { container, indices })
                    }
                    _ => Value::Ref(View::Unknown),
                }
            }
            CheckedExpression::BorrowAddressed { root, .. } => {
                Value::Ref(self.view_of(state, root.root, &root.path))
            }
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                let CheckedSegmentSelect::One(index) = segment else {
                    return Value::Ref(View::Unknown);
                };
                let row = self.int(state, &index.offset);
                match self.path_target(state, root.root, &root.path) {
                    Target::Location(location) => {
                        let location = state.resolve(&location);
                        match self.world.container(location, 2) {
                            Some(container) => {
                                let generation = state.generation(container);
                                let length =
                                    self.world
                                        .segment_length(container, generation, row.clone());
                                Value::Ref(View::Run {
                                    container,
                                    prefix: vec![row],
                                    offset: Linear::constant(0),
                                    length,
                                })
                            }
                            None => Value::Ref(View::Unknown),
                        }
                    }
                    _ => Value::Ref(View::Unknown),
                }
            }
            CheckedExpression::RangeOf {
                source, start, end, ..
            } => {
                let base = match source {
                    CheckedRangeSource::Storage(root) => {
                        match self.path_target(state, root.root, &root.path) {
                            Target::Location(location) => self.run_of(state, location),
                            _ => View::Unknown,
                        }
                    }
                    CheckedRangeSource::Range(root) => match state.values.get(&root.binding) {
                        Some(Value::Ref(view @ View::Run { .. })) => view.clone(),
                        _ => View::Unknown,
                    },
                    CheckedRangeSource::Element(_) => View::Unknown,
                };
                let start = self.int(state, start);
                let end = self.int(state, end);
                match base {
                    View::Run {
                        container,
                        prefix,
                        offset,
                        ..
                    } => match (offset.plus(&start), end.minus(&start)) {
                        (Some(offset), Some(length)) => Value::Ref(View::Run {
                            container,
                            prefix,
                            offset,
                            length,
                        }),
                        _ => Value::Ref(View::Unknown),
                    },
                    _ => Value::Ref(View::Unknown),
                }
            }
            CheckedExpression::DerefAddressed {
                carrier,
                binding,
                ty,
            } => match state.values.get(binding).cloned() {
                Some(Value::Ref(View::Place(location))) => {
                    self.read_location(state, &location, *ty)
                }
                Some(Value::Ref(View::Element { container, indices })) => {
                    self.read_element(state, container, indices, *ty, carrier)
                }
                _ => self.opaque_of(*ty),
            },
            CheckedExpression::Project {
                binding,
                fields,
                ty,
                ..
            } => {
                let path: Vec<CheckedPlaceStep> = fields
                    .iter()
                    .map(|field| CheckedPlaceStep::Field(*field))
                    .collect();
                match self.path_target(state, PlaceRoot::Binding(*binding), &path) {
                    Target::Location(location) => self.read_location(state, &location, *ty),
                    _ => self.opaque_of(*ty),
                }
            }
            CheckedExpression::ProjectValue {
                value, field, ty, ..
            } => match self.eval(state, value) {
                Value::Struct(fields) => fields
                    .get(*field as usize)
                    .cloned()
                    .unwrap_or(Value::Unknown),
                Value::Owned(location) => {
                    self.read_location(state, &location.child(Step::Field(*field)), *ty)
                }
                _ => self.opaque_of(*ty),
            },
            CheckedExpression::ConstructStruct { fields, .. } => {
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    values.push(self.stored_value(state, field));
                }
                Value::Struct(values)
            }
            CheckedExpression::ConstructEnum {
                variant, fields, ..
            } => {
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    values.push(self.stored_value(state, field));
                }
                Value::Variant {
                    variant: *variant,
                    fields: values,
                }
            }
            CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::BoxDeref { .. }
            | CheckedExpression::BoxTake { .. } => self.opaque_of(expression.ty()),
        }
    }

    fn read_element(
        &mut self,
        state: &mut State,
        container: ContainerId,
        indices: Vec<Linear>,
        ty: CheckedType,
        carrier: &NodePath,
    ) -> Value {
        self.access(container, &indices, false, carrier, state);
        match ty {
            CheckedType::Integer(integer) => {
                let version = state.version(&mut self.world, container);
                Value::Int(self.world.read(version, indices, Some(integer)))
            }
            other => self.opaque_of(other),
        }
    }

    fn integer_operation(
        &mut self,
        state: &mut State,
        operation: CheckedIntegerOperation,
        arguments: &[CheckedExpression],
        result: CheckedType,
    ) -> Value {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.int(state, argument));
        }
        let comparison = |relation| match values.as_slice() {
            [left, right] => Value::Bool(Cond::Literal(literal(
                left.clone(),
                relation,
                right.clone(),
            ))),
            _ => Value::Bool(Cond::Unknown),
        };
        match operation {
            CheckedIntegerOperation::Equal => comparison(Relation::Equal),
            CheckedIntegerOperation::NotEqual => comparison(Relation::NotEqual),
            CheckedIntegerOperation::Less => comparison(Relation::Less),
            CheckedIntegerOperation::LessEqual => comparison(Relation::LessEqual),
            CheckedIntegerOperation::Greater => comparison(Relation::Greater),
            CheckedIntegerOperation::GreaterEqual => comparison(Relation::GreaterEqual),
            // An exact operation's result is its mathematical value: the
            // operation's own obligation proved it in range. A `.defined`
            // spelling is a Bool domain query [OP-7], an unknown here.
            CheckedIntegerOperation::AddExact => match values.as_slice() {
                [left, right] => left
                    .plus(right)
                    .map_or_else(|| self.opaque_of(result), Value::Int),
                _ => self.opaque_of(result),
            },
            CheckedIntegerOperation::SubtractExact => match values.as_slice() {
                [left, right] => left
                    .minus(right)
                    .map_or_else(|| self.opaque_of(result), Value::Int),
                _ => self.opaque_of(result),
            },
            CheckedIntegerOperation::MultiplyExact => match values.as_slice() {
                [left, right] if left.is_constant() => right
                    .scaled(left.constant)
                    .map_or_else(|| self.opaque_of(result), Value::Int),
                [left, right] if right.is_constant() => left
                    .scaled(right.constant)
                    .map_or_else(|| self.opaque_of(result), Value::Int),
                _ => self.opaque_of(result),
            },
            _ => self.opaque_of(result),
        }
    }

    // ----- calls -----

    fn call(
        &mut self,
        state: &mut State,
        function: super::super::model::FunctionId,
        call: &NodePath,
        arguments: &[CheckedExpression],
        result: CheckedType,
        formal: Option<&super::super::model::CheckedEffects>,
    ) -> Value {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.eval(state, argument));
        }
        let Some(callee) = self.functions.get(function.0 as usize) else {
            self.forget_all(state, call);
            return self.opaque_of(result);
        };
        let parameters: Vec<BindingId> = callee
            .parameters
            .iter()
            .map(|parameter| parameter.binding)
            .collect();
        let argument_of = |root: CheckedRangeRoot| -> Option<Value> {
            match root {
                CheckedRangeRoot::Binding(binding) => parameters
                    .iter()
                    .position(|parameter| *parameter == binding)
                    .and_then(|position| values.get(position).cloned()),
                _ => None,
            }
        };
        // [RANGE-3] the callee's range requirements, at the call.
        if self.dry == 0 {
            for clause in &callee.range_facts.requirements {
                let mut scratch = state.clone();
                let frame = self.frame(&mut scratch, clause, &argument_of);
                self.require(state, clause, &frame, call, "a call");
            }
        }
        // Reads and writes through reference arguments [EFF-5].
        let (writes, reads): (Vec<crate::DeclarationId>, Vec<crate::DeclarationId>) = match formal {
            Some(effects) => (
                effects.writes.iter().map(|path| path.root).collect(),
                effects.reads.iter().map(|path| path.root).collect(),
            ),
            None => (
                callee
                    .declared_state_writes
                    .iter()
                    .map(|path| path.root)
                    .collect(),
                callee
                    .declared_state_reads
                    .iter()
                    .map(|path| path.root)
                    .collect(),
            ),
        };
        for (position, parameter) in callee.parameters.iter().enumerate() {
            if !parameter.mode.is_reference() {
                continue;
            }
            let Some(Value::Ref(view)) = values.get(position).cloned() else {
                if writes.contains(&parameter.declaration) {
                    self.forget_all(state, call);
                }
                continue;
            };
            let written = writes.contains(&parameter.declaration);
            // [RANGE-5] an argument the callee's row neither reads nor writes
            // is no access.
            if written || reads.contains(&parameter.declaration) {
                self.unplaced_view(&view, written, call, state);
            }
            if !written {
                continue;
            }
            match view {
                View::Run { container, .. } => {
                    state.havoc_container(&mut self.world, container, false)
                }
                View::Element { container, indices } => {
                    state.write_element(&mut self.world, container, indices, None);
                }
                View::Place(location) => state.havoc_location(&mut self.world, &location),
                View::Unknown => state.havoc_everything(&mut self.world),
            }
        }
        let value = match result {
            CheckedType::Integer(integer) => Value::Int(self.world.opaque(Some(integer))),
            CheckedType::Bool => Value::Bool(Cond::Unknown),
            CheckedType::Unit | CheckedType::Float(_) => Value::Unknown,
            _ => Value::Owned(Location::root(Origin::CallResult(self.world.new_origin()))),
        };
        if callee.body.is_none() {
            // The fill value's type is the element type of both rows.
            let element = arguments.get(1).and_then(|fill| integer_type(fill.ty()));
            self.content_law(state, callee, &values, &value, element);
        }
        value
    }

    /// [RANGE-2] the content a fill constructor gives its result.
    fn content_law(
        &mut self,
        state: &mut State,
        callee: &CheckedFunction,
        arguments: &[Value],
        result: &Value,
        element: Option<IntegerType>,
    ) {
        let Value::Owned(location) = result else {
            return;
        };
        match callee.name.as_str() {
            "box_array_filled" => {
                let (Some(Value::Int(count)), Some(Value::Int(fill))) =
                    (arguments.first(), arguments.get(1))
                else {
                    return;
                };
                let content = location.child(Step::BoxContent);
                let Some(container) = self.world.container(content, 1) else {
                    return;
                };
                let length = self.world.measure(container, 0, CheckedMeasure::Length);
                state
                    .conds
                    .push(literal(length.clone(), Relation::Equal, count.clone()));
                let clause = law_clause(element, CheckedRangeShape::Run);
                let mut frame = Frame::default();
                let version = state.version(&mut self.world, container);
                frame.places.insert(
                    law_place(),
                    PlaceView::Run {
                        container,
                        version,
                        generation: 0,
                        prefix: Vec::new(),
                        offset: Linear::constant(0),
                        length,
                    },
                );
                frame
                    .values
                    .insert(CheckedRangeRoot::Argument(1), fill.clone());
                let id = self.add_fact(clause, frame);
                state.facts.push(id);
            }
            "box_segments_filled" => {
                let (
                    Some(Value::Ref(View::Run {
                        container: lengths,
                        prefix,
                        offset,
                        length: count,
                    })),
                    Some(Value::Int(fill)),
                ) = (arguments.first().cloned(), arguments.get(1).cloned())
                else {
                    return;
                };
                if !prefix.is_empty() {
                    return;
                }
                let payload = location
                    .child(Step::Payload {
                        variant: 1,
                        field: 0,
                    })
                    .child(Step::BoxContent);
                let Some(container) = self.world.container(payload, 2) else {
                    return;
                };
                let rows = self.world.measure(container, 0, CheckedMeasure::Length);
                let lengths_version = state.version(&mut self.world, lengths);
                let version = state.version(&mut self.world, container);
                // Under `Some`: `s.len == lengths.len`, each segment's length
                // is its entry of `lengths`, and every element is the fill.
                let mut frame = Frame::default();
                frame.places.insert(
                    law_place(),
                    PlaceView::Segments {
                        container,
                        version,
                        generation: 0,
                        rows: rows.clone(),
                    },
                );
                frame.places.insert(
                    law_lengths(),
                    PlaceView::Run {
                        container: lengths,
                        version: lengths_version,
                        generation: state.generation(lengths),
                        prefix: Vec::new(),
                        offset,
                        length: count.clone(),
                    },
                );
                frame.values.insert(CheckedRangeRoot::Argument(1), fill);
                let frame_rows = frame.clone();
                let filled = self.add_fact(
                    law_clause(element, CheckedRangeShape::Segments),
                    frame.clone(),
                );
                let sized = self.add_fact(segment_lengths_clause(), frame);
                let at = state.resolve(location);
                state.routed.push((at.clone(), 1, filled));
                state.routed.push((at, 1, sized));
                // The row count is a routed fact too: a clause without
                // bound variables.
                let rows_fact = self.add_fact(rows_clause(), frame_rows);
                let at = state.resolve(location);
                state.routed.push((at, 1, rows_fact));
            }
            _ => {}
        }
    }

    // ----- loops -----

    /// What a body writes, from dry walks that record instead of judging.
    ///
    /// The first walk starts from `state`; a branch it skips because of
    /// something `state` knows, or a reference it resolves through a
    /// binding, may differ in a later iteration once the body has changed
    /// what decided it. So while a walk records a binding or a slot of
    /// storage that `state` already holds, which is what can change control
    /// or a write's target, the body is walked again from the header that
    /// forgets everything recorded so far, until nothing new is recorded.
    fn modified(
        &mut self,
        state: &State,
        body: &[CheckedStatement],
        binder: Option<BindingId>,
    ) -> Modified {
        if self.dry >= MAX_DRY_DEPTH {
            self.imprecise.get_or_insert_with(|| self.cite.clone());
            return Modified {
                everything: true,
                ..Modified::default()
            };
        }
        let saved_log = self.world.log.take();
        let saved_recording = self.recording.take();
        let saved_breaks = std::mem::take(&mut self.breaks);
        let mark = self.world.origin_mark();
        let looped = self.cite.clone();
        let mut total = Modified::default();
        let mut start = state.clone();
        let mut settled = false;
        for _ in 0..MAX_HEADER_ROUNDS {
            self.world.log = Some(Modified::default());
            self.dry += 1;
            let mut dry = start;
            if let Some(binder) = binder {
                let value = self.world.opaque(None);
                dry.values.insert(binder, Value::Int(value));
            }
            self.gives.push(Vec::new());
            let _ = self.block(dry, body);
            self.gives.pop();
            self.breaks.clear();
            self.dry -= 1;
            let log = self.world.log.take().unwrap_or_default();
            let deciding = self.absorb(&mut total, log, state, mark);
            if !deciding {
                settled = true;
                break;
            }
            start = self.header(state, &total);
        }
        if !settled {
            self.imprecise.get_or_insert(looped);
            total.everything = true;
        }
        self.world.log = saved_log;
        if let Some(outer) = &mut self.world.log {
            outer.containers.extend(total.containers.iter().copied());
            outer.descriptors.extend(total.descriptors.iter().copied());
            outer.bindings.extend(total.bindings.iter().copied());
            outer.slots.extend(total.slots.iter().cloned());
            outer.everything |= total.everything;
        }
        self.recording = saved_recording;
        self.breaks = saved_breaks;
        total
    }

    /// Adds one dry walk's record to `total`; whether it newly records
    /// something of `entry`'s that can decide control or a write's target: a
    /// binding, a slot of storage that existed before the loop, or a write
    /// the walk could not place.
    fn absorb(&self, total: &mut Modified, log: Modified, entry: &State, mark: u32) -> bool {
        let existed = |location: &Location| match location.origin {
            Origin::Parameter(_) => true,
            Origin::Binding(_, origin)
            | Origin::CallResult(origin)
            | Origin::Constructed(origin) => origin <= mark,
        };
        let mut deciding = log.everything && !total.everything;
        total.everything |= log.everything;
        total.containers.extend(log.containers);
        total.descriptors.extend(log.descriptors);
        for binding in log.bindings {
            if total.bindings.insert(binding) && entry.values.contains_key(&binding) {
                deciding = true;
            }
        }
        for slot in log.slots {
            let old = existed(&slot);
            if total.slots.insert(slot) && old {
                deciding = true;
            }
        }
        deciding
    }

    /// The header state: `entry` with everything the body writes forgotten,
    /// and the variants of every written enum.
    fn header(&mut self, entry: &State, modified: &Modified) -> State {
        let mut header = entry.clone();
        if modified.everything {
            header.havoc_everything(&mut self.world);
        }
        for container in &modified.containers {
            let descriptor = modified.descriptors.contains(container);
            header.havoc_container(&mut self.world, *container, descriptor);
        }
        for binding in &modified.bindings {
            if let Some(value) = header.values.get(binding).cloned() {
                let fresh = match value {
                    Value::Int(_) => Value::Int(self.world.opaque(None)),
                    Value::Bool(_) => Value::Bool(Cond::Unknown),
                    Value::Owned(_) => Value::Owned(Location::root(Origin::Binding(
                        *binding,
                        self.world.new_origin(),
                    ))),
                    _ => Value::Unknown,
                };
                header.values.insert(*binding, fresh);
            }
        }
        for binding in &modified.bindings {
            if let Some(Value::Owned(location)) = entry.values.get(binding) {
                let location = entry.resolve(location);
                header.forget_variants(&location);
            }
        }
        for slot in &modified.slots {
            header.slots.retain(|location, held| {
                !(location.starts_with(slot) && !matches!(held, Slot::Alias(_)))
            });
            header.forget_variants(slot);
        }
        header
    }

    fn unbounded_loop(
        &mut self,
        entry: State,
        id: CheckedLoopId,
        body: &[CheckedStatement],
    ) -> Option<State> {
        let modified = self.modified(&entry, body, None);
        let invariants = self.loop_invariants(id);
        let node = self.cite.clone();
        for clause in &invariants {
            let mut scratch = entry.clone();
            let frame = self.frame(&mut scratch, clause, &|root| binding_value(&entry, root));
            self.require(&entry, clause, &frame, &node, "a loop entry");
        }
        let mut header = self.header(&entry, &modified);
        for clause in &invariants {
            let mut scratch = header.clone();
            let frame = self.frame(&mut scratch, clause, &|root| binding_value(&header, root));
            let fact = self.add_fact(clause.clone(), frame);
            header.facts.push(fact);
        }
        let fork = header.conds.len();
        let saved = self.breaks.remove(&id.0);
        let end = self.block(header, body);
        if let Some(end) = end {
            for clause in &invariants {
                let mut scratch = end.clone();
                let frame = self.frame(&mut scratch, clause, &|root| binding_value(&end, root));
                self.require(&end, clause, &frame, &node, "a loop back edge");
            }
        }
        // The loop leaves only by its breaks: the state after it is their
        // join.
        let broke = self.breaks.remove(&id.0).unwrap_or_default();
        if let Some(saved) = saved {
            self.breaks.insert(id.0, saved);
        }
        join(&mut self.world, fork, broke)
    }

    #[allow(clippy::too_many_arguments)]
    fn counted_loop(
        &mut self,
        mut entry: State,
        id: CheckedLoopId,
        node: &NodePath,
        binder: BindingId,
        lower: &CheckedExpression,
        upper: &CheckedExpression,
        affine: &[super::super::model::CheckedLoopInvariant],
        body: &[CheckedStatement],
    ) -> Option<State> {
        let low = self.int(&mut entry, lower);
        let high = self.int(&mut entry, upper);
        let invariants = self.loop_invariants(id);
        // [RANGE-3] each range invariant holds at the first header.
        for clause in &invariants {
            let mut at_entry = entry.clone();
            at_entry.values.insert(binder, Value::Int(low.clone()));
            let mut scratch = at_entry.clone();
            let frame = self.frame(&mut scratch, clause, &|root| binding_value(&at_entry, root));
            self.require(&at_entry, clause, &frame, node, "a loop entry");
        }
        // [RANGE-5] the certificate, over the entry state.
        if self.dry == 0
            && let Some(apart) = self
                .function
                .range_facts
                .loops
                .get(&id)
                .and_then(|entry| entry.apart.clone())
        {
            self.apart(&entry, id, node, binder, &low, &high, &apart, body);
        }
        // The certificate's walks moved the cite into the body.
        self.cite = node.clone();
        let modified = self.modified(&entry, body, Some(binder));
        let mut header = self.header(&entry, &modified);
        let index = self.world.opaque(None);
        header.values.insert(binder, Value::Int(index.clone()));
        header
            .conds
            .push(literal(index.clone(), Relation::GreaterEqual, low.clone()));
        for invariant in affine {
            if let Some(literals) = self.affine_relation(&header, &invariant.relation) {
                header.conds.extend(literals);
            }
        }
        let fork = header.conds.len();
        let mut exit = header.clone();
        let mut facts_added = Vec::new();
        for clause in &invariants {
            let mut scratch = header.clone();
            let frame = self.frame(&mut scratch, clause, &|root| binding_value(&header, root));
            let fact = self.add_fact(clause.clone(), frame);
            facts_added.push(fact);
        }
        header.facts.extend(facts_added.iter().copied());
        let mut body_state = header.clone();
        body_state
            .conds
            .push(literal(index.clone(), Relation::Less, high.clone()));
        let saved = self.breaks.remove(&id.0);
        let end = self.block(body_state, body);
        if let Some(mut end) = end {
            let next = index
                .plus_constant(1)
                .unwrap_or_else(|| self.world.opaque(None));
            end.values.insert(binder, Value::Int(next));
            for clause in &invariants {
                let mut scratch = end.clone();
                let frame = self.frame(&mut scratch, clause, &|root| binding_value(&end, root));
                self.require(&end, clause, &frame, node, "a loop back edge");
            }
        }
        let mut arms = self.breaks.remove(&id.0).unwrap_or_default();
        if let Some(saved) = saved {
            self.breaks.insert(id.0, saved);
        }
        // The loop leaves at the header where the binder reaches the upper
        // bound, there when the range is not empty, and at each break; the
        // state after it is their join.
        exit.facts.extend(facts_added);
        if self.entailed(
            &entry,
            &literal(low.clone(), Relation::LessEqual, high.clone()),
        ) {
            exit.conds.push(literal(index, Relation::Equal, high));
        } else {
            exit.conds
                .push(literal(index, Relation::GreaterEqual, high));
        }
        exit.values.remove(&binder);
        arms.push(exit);
        // Every arm but the exit holds the binder, so the join drops it.
        join(&mut self.world, fork, arms)
    }

    fn loop_invariants(&self, id: CheckedLoopId) -> Vec<CheckedRangeClause> {
        self.function
            .range_facts
            .loops
            .get(&id)
            .map(|entry| entry.invariants.clone())
            .unwrap_or_default()
    }

    fn entailed(&mut self, state: &State, goal: &Literal) -> bool {
        let (units, choices) = state.premises(&self.world);
        let mut query = Query {
            units,
            choices,
            ..Query::default()
        };
        query.units.push(negated(goal));
        matches!(
            facts::judge(&mut self.world, &self.facts, &[], &[], query),
            Ok(Verdict::Refuted)
        )
    }

    // ----- the certificate -----

    #[allow(clippy::too_many_arguments)]
    fn apart(
        &mut self,
        entry: &State,
        id: CheckedLoopId,
        node: &NodePath,
        binder: BindingId,
        low: &Linear,
        high: &Linear,
        apart: &super::super::range_facts::CheckedApart,
        body: &[CheckedStatement],
    ) {
        let first = self.world.opaque(None);
        let second = self.world.opaque(None);
        let mark = self.world.origin_mark();
        let mut runs = Vec::new();
        for iteration in [&first, &second] {
            let mut state = entry.clone();
            state.values.insert(binder, Value::Int(iteration.clone()));
            state.conds.push(literal(
                iteration.clone(),
                Relation::GreaterEqual,
                low.clone(),
            ));
            state
                .conds
                .push(literal(iteration.clone(), Relation::Less, high.clone()));
            let saved_recording = self.recording.replace(Recording::default());
            let saved_breaks = std::mem::take(&mut self.breaks);
            self.dry += 1;
            self.gives.push(Vec::new());
            let _ = self.block(state, body);
            self.gives.pop();
            self.dry -= 1;
            self.breaks = saved_breaks;
            let recording =
                std::mem::replace(&mut self.recording, saved_recording).unwrap_or_default();
            runs.push(recording);
        }
        let shared = |world: &World, container: ContainerId| -> bool {
            match &world.containers[container as usize].location.origin {
                Origin::Parameter(_) => true,
                Origin::Binding(_, generation)
                | Origin::CallResult(generation)
                | Origin::Constructed(generation) => *generation <= mark,
            }
        };
        let (left, right) = (&runs[0], &runs[1]);
        let written: BTreeSet<ContainerId> = left
            .accesses
            .iter()
            .filter(|access| access.write)
            .map(|access| access.container)
            .chain(
                left.unplaced
                    .iter()
                    .filter(|access| access.write)
                    .filter_map(|access| access.container),
            )
            .filter(|container| shared(&self.world, *container))
            .collect();
        let everything = left
            .unplaced
            .iter()
            .any(|access| access.container.is_none() && access.write);
        let mut failure: Option<ApartFailure> = None;
        let mut beyond_arithmetic = false;
        // An access the certificate cannot place, against a written container.
        for access in left.unplaced.iter().chain(right.unplaced.iter()) {
            let reaches = match access.container {
                None => true,
                Some(container) => {
                    shared(&self.world, container) && (written.contains(&container) || everything)
                }
            };
            if reaches && (access.write || !access.descriptor) && failure.is_none() {
                failure = Some(ApartFailure::Unplaced {
                    access: access.node.clone(),
                });
            }
        }
        let uses: Vec<(FactId, Vec<Linear>, Vec<Linear>)> = Vec::new();
        let mut written_instances = uses;
        for step in &apart.uses {
            let Some(fact) = entry
                .facts
                .iter()
                .copied()
                .find(|fact| self.facts[*fact as usize].clause.declaration == step.fact)
            else {
                failure.get_or_insert(ApartFailure::Use {
                    step: step.node.clone(),
                    reason: "the fact it names does not hold where the loop begins",
                });
                continue;
            };
            let iterations = [first.clone(), second.clone()];
            let mut arguments = Vec::with_capacity(step.arguments.len());
            let frame = self.facts[fact as usize].frame.clone();
            let clause = self.facts[fact as usize].clause.clone();
            let _ = clause;
            for argument in &step.arguments {
                match self.certificate_term(entry, &frame, argument, &iterations) {
                    Some(value) => arguments.push(value),
                    None => {
                        failure.get_or_insert(ApartFailure::Use {
                            step: step.node.clone(),
                            reason: "an argument names a place the judgment cannot view",
                        });
                    }
                }
            }
            written_instances.push((fact, arguments, iterations.to_vec()));
        }
        let mut certified_writes = Vec::new();
        let mut certified_reads = Vec::new();
        if failure.is_none() {
            'pairs: for write in left.accesses.iter().filter(|access| access.write) {
                if !shared(&self.world, write.container) {
                    continue;
                }
                for other in right
                    .accesses
                    .iter()
                    .filter(|access| access.container == write.container)
                {
                    let (units, choices) = entry.premises(&self.world);
                    let mut query = Query {
                        units,
                        choices,
                        ..Query::default()
                    };
                    query.units.extend(write.conds.iter().cloned());
                    query.units.extend(other.conds.iter().cloned());
                    query.choices.extend(write.choices.iter().cloned());
                    query.choices.extend(other.choices.iter().cloned());
                    query
                        .units
                        .push(literal(first.clone(), Relation::NotEqual, second.clone()));
                    for (at, index) in write.indices.iter().zip(&other.indices) {
                        query
                            .units
                            .push(literal(at.clone(), Relation::Equal, index.clone()));
                    }
                    match facts::judge(
                        &mut self.world,
                        &self.facts,
                        &entry.facts,
                        &written_instances,
                        query,
                    ) {
                        Ok(Verdict::Refuted) => {}
                        Ok(Verdict::Open) => {
                            failure = Some(ApartFailure::Overlap {
                                write: write.node.clone(),
                                other: other.node.clone(),
                                other_write: other.write,
                            });
                            break 'pairs;
                        }
                        Err(Capacity::Arithmetic) => {
                            beyond_arithmetic = true;
                            break 'pairs;
                        }
                        Err(capacity) => {
                            failure = Some(ApartFailure::Capacity {
                                write: write.node.clone(),
                                other: other.node.clone(),
                                ceiling: capacity.describe(),
                            });
                            break 'pairs;
                        }
                    }
                }
            }
            for access in left
                .accesses
                .iter()
                .filter(|access| written.contains(&access.container))
            {
                if access.write {
                    certified_writes.push(access.node.clone());
                } else {
                    certified_reads.push(access.node.clone());
                }
            }
        }
        if beyond_arithmetic {
            self.issues.push(RangeIssue::Unsupported {
                node: apart.node.clone(),
                feature: UnsupportedSemanticFeature::RangeArithmetic,
            });
            return;
        }
        match failure {
            Some(failure) => self.issues.push(RangeIssue::Apart {
                node: apart.node.clone(),
                failure,
            }),
            None => self.certified.push(CertifiedLoop {
                id,
                node: node.clone(),
                writes: certified_writes,
                reads: certified_reads,
            }),
        }
    }

    fn certificate_term(
        &mut self,
        entry: &State,
        frame: &Frame,
        term: &CheckedRangeTerm,
        iterations: &[Linear],
    ) -> Option<Linear> {
        // A certificate argument names the loop's own state: form it in a
        // frame over the entry state, its iterations bound.
        let clause = CheckedRangeClause {
            declaration: synthetic_declaration(),
            name: String::new(),
            node: empty_path(),
            binders: Vec::new(),
            guards: Vec::new(),
            conclusions: vec![super::super::range_facts::CheckedRangeRelation {
                node: empty_path(),
                left: term.clone(),
                comparison: super::super::range_facts::RangeComparison::Equal,
                right: term.clone(),
            }],
        };
        let _ = frame;
        let mut scratch = entry.clone();
        let own = self.frame(&mut scratch, &clause, &|root| binding_value(entry, root));
        let formed = facts::form(&mut self.world, &clause, &own, &[], iterations)?;
        formed
            .conclusions
            .first()
            .map(|conclusion| conclusion.left.clone())
    }

    // ----- affine relations -----

    fn affine(&self, state: &State, expression: &CheckedAffineExpression) -> Option<Linear> {
        match &expression.kind {
            CheckedAffineExpressionKind::Constant { value, .. } => Some(Linear::constant(*value)),
            CheckedAffineExpressionKind::Local { binding, .. } => match state.values.get(binding) {
                Some(Value::Int(value)) => Some(value.clone()),
                _ => None,
            },
            CheckedAffineExpressionKind::Add(left, right) => {
                self.affine(state, left)?.plus(&self.affine(state, right)?)
            }
            CheckedAffineExpressionKind::Subtract(left, right) => {
                self.affine(state, left)?.minus(&self.affine(state, right)?)
            }
            CheckedAffineExpressionKind::MultiplyByConstant {
                constant, value, ..
            } => self.affine(state, value)?.scaled(*constant),
            CheckedAffineExpressionKind::ConstGeneric { .. }
            | CheckedAffineExpressionKind::Measure(_) => None,
        }
    }

    /// A checked affine relation `left - right <= bound` (and its reverse
    /// for an equality), where every leaf has a value here.
    fn affine_relation(
        &self,
        state: &State,
        relation: &CheckedAffineRelation,
    ) -> Option<Vec<Literal>> {
        let left = self.affine(state, &relation.left)?;
        let right = self.affine(state, &relation.right)?;
        let bounded = right.plus_constant(relation.bound)?;
        let mut out = vec![literal(left.clone(), Relation::LessEqual, bounded)];
        if relation.equality {
            out.push(literal(
                right.clone(),
                Relation::LessEqual,
                left.plus_constant(relation.bound)?,
            ));
        }
        Some(out)
    }

    /// One affine requirement as a literal, where the walk can read it.
    fn goal_literal(
        &mut self,
        state: &State,
        root: &super::super::goal::GoalExpression,
    ) -> Option<Literal> {
        use super::super::goal::{GoalDatum, GoalExpression, GoalOperation, GoalProjection};
        fn term(
            walker: &mut Walker<'_>,
            state: &State,
            expression: &GoalExpression,
        ) -> Option<Linear> {
            match expression {
                GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) => {
                    Some(Linear::constant(super::super::entailment::integer_value(
                        *ty, *bits,
                    )))
                }
                GoalExpression::Datum(GoalDatum::Parameter {
                    ordinal,
                    projections,
                    ..
                }) if projections.is_empty() => {
                    let parameter = walker.function.parameters.get(*ordinal as usize)?;
                    match state.values.get(&parameter.binding) {
                        Some(Value::Int(value)) => Some(value.clone()),
                        _ => None,
                    }
                }
                GoalExpression::Operation { row, arguments, .. } => {
                    match (row, arguments.as_slice()) {
                        (
                            GoalOperation::Integer {
                                operation: CheckedIntegerOperation::AddExact,
                                ..
                            },
                            [left, right],
                        ) => term(walker, state, left)?.plus(&term(walker, state, right)?),
                        (
                            GoalOperation::Integer {
                                operation: CheckedIntegerOperation::SubtractExact,
                                ..
                            },
                            [left, right],
                        ) => term(walker, state, left)?.minus(&term(walker, state, right)?),
                        (
                            GoalOperation::ContainerMeasure {
                                measure: CheckedMeasure::Length,
                                ..
                            },
                            [
                                GoalExpression::Datum(GoalDatum::Parameter {
                                    ordinal,
                                    projections,
                                    ..
                                }),
                            ],
                        ) => {
                            let parameter = walker.function.parameters.get(*ordinal as usize)?;
                            let value = state.values.get(&parameter.binding)?.clone();
                            match (value, projections.as_slice()) {
                                (
                                    Value::Ref(View::Run { length, .. }),
                                    [] | [GoalProjection::Deref],
                                ) => Some(length),
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        let GoalExpression::Operation { row, arguments, .. } = root else {
            return None;
        };
        let GoalOperation::Integer { operation, .. } = row else {
            return None;
        };
        let relation = match operation {
            CheckedIntegerOperation::Equal => Relation::Equal,
            CheckedIntegerOperation::NotEqual => Relation::NotEqual,
            CheckedIntegerOperation::Less => Relation::Less,
            CheckedIntegerOperation::LessEqual => Relation::LessEqual,
            CheckedIntegerOperation::Greater => Relation::Greater,
            CheckedIntegerOperation::GreaterEqual => Relation::GreaterEqual,
            _ => return None,
        };
        let [left, right] = arguments.as_slice() else {
            return None;
        };
        let left = term(self, state, left)?;
        let right = term(self, state, right)?;
        Some(literal(left, relation, right))
    }
}

/// Where one storage path leads.
enum Target {
    Location(Location),
    /// A run reached through a range reference, before its subscript.
    Run {
        container: ContainerId,
        prefix: Vec<Linear>,
        offset: Linear,
    },
    /// One element; `below` when the path continues inside it.
    Element {
        container: ContainerId,
        indices: Vec<Linear>,
        below: bool,
    },
    /// One segment of a `Segments`, before its element subscript.
    Row {
        container: ContainerId,
        row: Linear,
    },
    Unknown,
}

fn binding_value(state: &State, root: CheckedRangeRoot) -> Option<Value> {
    match root {
        CheckedRangeRoot::Binding(binding) => state.values.get(&binding).cloned(),
        _ => None,
    }
}

fn constant(value: &CheckedValue) -> Value {
    match value {
        CheckedValue::Integer { ty, bits } => Value::Int(Linear::constant(
            super::super::entailment::integer_value(*ty, *bits),
        )),
        CheckedValue::Bool(truth) => Value::Bool(Cond::Constant(*truth)),
        _ => Value::Unknown,
    }
}

/// The negation of a conclusion as alternatives.
fn conclusion_negation(conclusion: &Literal) -> Vec<Literal> {
    conclusion.negation()
}

fn segment_places(term: &CheckedRangeTerm, out: &mut BTreeSet<CheckedRangePlace>) {
    match term {
        CheckedRangeTerm::Read {
            place,
            shape,
            indices,
            ..
        } => {
            if *shape == CheckedRangeShape::Segments {
                out.insert(place.clone());
            }
            for index in indices {
                segment_places(index, out);
            }
        }
        CheckedRangeTerm::SegmentLength { place, segment } => {
            out.insert(place.clone());
            segment_places(segment, out);
        }
        CheckedRangeTerm::Sum { terms, .. } => {
            for (_, part) in terms {
                segment_places(part, out);
            }
        }
        _ => {}
    }
}

/// The declaration a compiler-formed clause carries: no source declaration
/// has it, so no `use` step names it.
fn synthetic_declaration() -> crate::DeclarationId {
    crate::DeclarationId::from_index(u32::MAX as usize).unwrap_or_else(|| unreachable!())
}

fn empty_path() -> NodePath {
    NodePath {
        components: Vec::new(),
    }
}

/// The one place a content law's clause reads: the filled content.
fn law_place() -> CheckedRangePlace {
    CheckedRangePlace {
        root: CheckedRangeRoot::Result,
        path: Vec::new(),
    }
}

/// The lengths a `box_segments_filled` call was given.
fn law_lengths() -> CheckedRangePlace {
    CheckedRangePlace {
        root: CheckedRangeRoot::Argument(0),
        path: vec![CheckedRangeStep::Referent],
    }
}

/// `forall filled(k...): content[k...] == fill`.
fn law_clause(element: Option<IntegerType>, shape: CheckedRangeShape) -> CheckedRangeClause {
    use super::super::range_facts::{CheckedRangeBinder, CheckedRangeRelation, RangeComparison};
    let place = law_place();
    let (ranges, indices) = match shape {
        CheckedRangeShape::Run => (
            vec![CheckedRangeBinder {
                start: CheckedRangeTerm::Constant(0),
                end: CheckedRangeTerm::Measure {
                    place: place.clone(),
                    measure: CheckedMeasure::Length,
                },
            }],
            vec![CheckedRangeTerm::Bound(0)],
        ),
        CheckedRangeShape::Segments => (
            vec![
                CheckedRangeBinder {
                    start: CheckedRangeTerm::Constant(0),
                    end: CheckedRangeTerm::Measure {
                        place: place.clone(),
                        measure: CheckedMeasure::Length,
                    },
                },
                CheckedRangeBinder {
                    start: CheckedRangeTerm::Constant(0),
                    end: CheckedRangeTerm::SegmentLength {
                        place: place.clone(),
                        segment: Box::new(CheckedRangeTerm::Bound(0)),
                    },
                },
            ],
            vec![CheckedRangeTerm::Bound(0), CheckedRangeTerm::Bound(1)],
        ),
    };
    CheckedRangeClause {
        declaration: synthetic_declaration(),
        name: "filled".to_owned(),
        node: empty_path(),
        binders: ranges,
        guards: Vec::new(),
        conclusions: vec![CheckedRangeRelation {
            node: empty_path(),
            left: CheckedRangeTerm::Read {
                place,
                shape,
                indices,
                element: element.unwrap_or(IntegerType::U64),
            },
            comparison: RangeComparison::Equal,
            right: CheckedRangeTerm::Value(CheckedRangeRoot::Argument(1)),
        }],
    }
}

/// `forall sized(d in 0..s.len): s[d].len == lengths[d]`.
fn segment_lengths_clause() -> CheckedRangeClause {
    use super::super::range_facts::{CheckedRangeBinder, CheckedRangeRelation, RangeComparison};
    let place = law_place();
    CheckedRangeClause {
        declaration: synthetic_declaration(),
        name: "sized".to_owned(),
        node: empty_path(),
        binders: vec![CheckedRangeBinder {
            start: CheckedRangeTerm::Constant(0),
            end: CheckedRangeTerm::Measure {
                place: place.clone(),
                measure: CheckedMeasure::Length,
            },
        }],
        guards: Vec::new(),
        conclusions: vec![CheckedRangeRelation {
            node: empty_path(),
            left: CheckedRangeTerm::SegmentLength {
                place,
                segment: Box::new(CheckedRangeTerm::Bound(0)),
            },
            comparison: RangeComparison::Equal,
            right: CheckedRangeTerm::Read {
                place: law_lengths(),
                shape: CheckedRangeShape::Run,
                indices: vec![CheckedRangeTerm::Bound(0)],
                element: IntegerType::U64,
            },
        }],
    }
}

/// `s.len == lengths.len`, as a clause without bound variables.
fn rows_clause() -> CheckedRangeClause {
    use super::super::range_facts::{CheckedRangeRelation, RangeComparison};
    CheckedRangeClause {
        declaration: synthetic_declaration(),
        name: "rows".to_owned(),
        node: empty_path(),
        binders: Vec::new(),
        guards: Vec::new(),
        conclusions: vec![CheckedRangeRelation {
            node: empty_path(),
            left: CheckedRangeTerm::Measure {
                place: law_place(),
                measure: CheckedMeasure::Length,
            },
            comparison: RangeComparison::Equal,
            right: CheckedRangeTerm::Measure {
                place: law_lengths(),
                measure: CheckedMeasure::Length,
            },
        }],
    }
}
