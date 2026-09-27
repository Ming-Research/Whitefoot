//! The [ENT-3] fact sources: which checked shape establishes which relation,
//! at which point of the flow.
//!
//! Each source below is one clause of [ENT-3], recognized on the checked tree
//! and expressed in the normalized difference-bound vocabulary [ENT-2]. The
//! walk in the parent module owns where a source fires; this module owns what
//! it establishes. Sources are additive and never reject: a shape outside a
//! source's stated form contributes nothing, which only under-derives, the
//! version-monotone direction [ENT-1].
//!
//! Implemented here: S1 (through the parent's arm entry), S4, S5, S6, S7,
//! and S9. Retired labels are not reused [ENT-3].

use super::super::super::goal::CheckedRequirement;
use super::super::super::model::{
    BindingId, CheckedArrayRoot, CheckedConst, CheckedConversionMode, CheckedEnumType,
    CheckedExpression, CheckedIntegerOperation, CheckedMeasure, CheckedNominalKind,
    CheckedNumericType, CheckedPlaceStep, CheckedSetTarget, CheckedType, CheckedValue, IntegerType,
    MeasuredKind, NominalId, SubscriptedTerm,
};
use super::super::super::places::CapturedTerm;
use super::super::fragment_type;
use super::super::state::{
    DerivationId, DerivationNode, FactState, FlowEventId, FlowEventKind, Relation, close,
};
use super::super::term::{
    CountedCaptureSide, MeasurePlacement, PlaceRoot, PlaceStep, ResolvedPlace, TermId, TermKind,
    ZERO, integer_value,
};
use super::super::{
    CountedAtomicDerivation, CountedBoundDerivation, CountedDerivationSet,
    CountedEqualityDerivation, CountedProofPoint,
};
use super::operation_facts::{self, Interval, Row, Span};
use super::*;
use std::rc::Rc;
/// Which term one evaluated value's [ENT-3] image is established on: the
/// place a `let` binder introduces, the compiler-owned commit value of one
/// `set` occurrence, or a checked integer conversion's private success
/// payload [ENT-2, ENT-5].
/// These destinations use the same admitted source image; a conditional
/// payload interprets it only inside its own success context.
#[derive(Clone, Copy)]
pub(super) enum ValueImage<'a> {
    Binding(BindingId),
    Commit(&'a crate::NodePath),
    ResultPayload(TermId),
}

/// The three S11 terms installed for one counted range.
pub(super) struct CountedTerms {
    pub(super) lower_source: TermId,
    pub(super) lower: TermId,
    pub(super) binder: TermId,
    pub(super) upper: TermId,
    pub(super) upper_source: TermId,
}

/// The affine half of one admitted unsigned exact-division transfer.
/// The S7 fact bounding the quotient by its dividend supplies the retained
/// source proof; the parent flow binds the scaled quotient image to the exact
/// current operand values.
pub(super) struct EstablishedUnsignedDivision {
    pub(super) literal_divisor: Option<i128>,
    pub(super) parent: DerivationId,
}

/// One right-hand side read as a row of the [ENT-3.S7] table: its row, its
/// selected and result types, and its operands in order.
struct OperationShape<'a> {
    row: Row,
    operand: IntegerType,
    result: IntegerType,
    operands: &'a [CheckedExpression],
    carrier: &'a crate::NodePath,
}

/// The three preheader relations after their complete S11 snapshot has been
/// materialized, but before continuing kills are applied.
pub(super) struct CountedPreheader {
    terms: CountedTerms,
    lower_capture_eq_endpoint: CountedEqualityDerivation,
    upper_capture_eq_endpoint: CountedEqualityDerivation,
    binder_eq_lower_capture: CountedEqualityDerivation,
}

impl Reasoning<'_, '_, '_> {
    // ------------------------------------------------------------------
    // S11 counted-range structural facts
    // ------------------------------------------------------------------

    /// Establishes the once-only endpoint snapshots and binder
    /// initialization, before the caller materializes [ENT-4] closure and
    /// applies the counted continuing-kill summary.
    pub(super) fn establish_counted_preheader(
        &mut self,
        range_path: &[u32],
        binder: BindingId,
        lower: &CheckedExpression,
        upper: &CheckedExpression,
        state: &mut FactState,
        event: FlowEventId,
    ) -> CountedTerms {
        // [FN-1] each capture copies its endpoint's value, so an admitted
        // endpoint reads exactly as an ordinary `let` of the same atom: a
        // measure term through its [MSR-1] former, otherwise the tracked
        // place or constant [ENT-2].
        let lower_source = self
            .copy_source(lower)
            .expect("checked counted lower endpoint must be an ENT-2 term or constant");
        let upper_source = self
            .copy_source(upper)
            .expect("checked counted upper endpoint must be an ENT-2 term or constant");
        let lower_capture = self.vocabulary.terms.intern(TermKind::CountedCapture {
            range_path: range_path.to_vec(),
            side: CountedCaptureSide::Lower,
        });
        let upper_capture = self.vocabulary.terms.intern(TermKind::CountedCapture {
            range_path: range_path.to_vec(),
            side: CountedCaptureSide::Upper,
        });
        let binder = self.vocabulary.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(binder), false, Vec::new()),
            IntegerType::U64,
        ));
        state.establish(
            &Relation::Equal {
                left: lower_capture,
                right: lower_source,
                difference: 0,
            },
            &mut self.vocabulary.derivations,
            event,
        );
        state.establish(
            &Relation::Equal {
                left: upper_capture,
                right: upper_source,
                difference: 0,
            },
            &mut self.vocabulary.derivations,
            event,
        );
        state.establish(
            &Relation::Equal {
                left: binder,
                right: lower_capture,
                difference: 0,
            },
            &mut self.vocabulary.derivations,
            event,
        );
        CountedTerms {
            lower_source,
            lower: lower_capture,
            binder,
            upper: upper_capture,
            upper_source,
        }
    }
}

/// Captures the three once-only S11 equality roots from the already
/// materialized post-capture state. This does not close or walk again.
pub(super) fn capture_counted_preheader(
    terms: CountedTerms,
    state: &FactState,
) -> CountedPreheader {
    let equality = |left: TermId, right: TermId| {
        let forward = Relation::Bound {
            left,
            right,
            bound: 0,
        };
        let reverse = Relation::Bound {
            left: right,
            right: left,
            bound: 0,
        };
        CountedEqualityDerivation {
            relation: Relation::Equal {
                left,
                right,
                difference: 0,
            },
            forward: CountedAtomicDerivation {
                relation: forward,
                proof_point: CountedProofPoint::PreheaderSnapshot,
                parent: state
                    .bound_parent(left, right, 0)
                    .expect("materialized S11 equality must retain its forward parent"),
            },
            reverse: CountedAtomicDerivation {
                relation: reverse,
                proof_point: CountedProofPoint::PreheaderSnapshot,
                parent: state
                    .bound_parent(right, left, 0)
                    .expect("materialized S11 equality must retain its reverse parent"),
            },
        }
    };
    CountedPreheader {
        lower_capture_eq_endpoint: equality(terms.lower, terms.lower_source),
        upper_capture_eq_endpoint: equality(terms.upper, terms.upper_source),
        binder_eq_lower_capture: equality(terms.binder, terms.lower),
        terms,
    }
}

impl Vocabulary {
    /// Adds exactly S11's two facts on an executed true header edge.
    pub(super) fn establish_counted_body_entry(
        &mut self,
        node_path: &crate::NodePath,
        counted: CountedPreheader,
        state: &mut FactState,
        event: FlowEventId,
    ) -> CountedDerivationSet {
        let lower_relation = Relation::Bound {
            left: counted.terms.lower,
            right: counted.terms.binder,
            bound: 0,
        };
        let lower_parent = state.establish_bound_with_proof(
            counted.terms.lower,
            counted.terms.binder,
            0,
            &mut self.derivations,
            event,
        );
        let upper_relation = Relation::Bound {
            left: counted.terms.binder,
            right: counted.terms.upper,
            bound: -1,
        };
        let upper_parent = state.establish_bound_with_proof(
            counted.terms.binder,
            counted.terms.upper,
            -1,
            &mut self.derivations,
            event,
        );
        CountedDerivationSet {
            counted_node_path: node_path.clone(),
            lower_capture_eq_endpoint: counted.lower_capture_eq_endpoint,
            upper_capture_eq_endpoint: counted.upper_capture_eq_endpoint,
            binder_eq_lower_capture: counted.binder_eq_lower_capture,
            lower_capture_le_binder: CountedBoundDerivation {
                relation: lower_relation.clone(),
                atomic: CountedAtomicDerivation {
                    relation: lower_relation,
                    proof_point: CountedProofPoint::BodyEntry,
                    parent: lower_parent,
                },
            },
            binder_lt_upper_capture: CountedBoundDerivation {
                relation: upper_relation.clone(),
                atomic: CountedAtomicDerivation {
                    relation: upper_relation,
                    proof_point: CountedProofPoint::BodyEntry,
                    parent: upper_parent,
                },
            },
        }
    }
}

impl Judging<'_, '_, '_> {
    // ------------------------------------------------------------------
    // S4 requires facts
    // ------------------------------------------------------------------

    /// [ENT-3] S4: the complete concrete body goal enters as a positive opaque
    /// fact, with its one exact comparison-root projection when present.
    pub(super) fn establish_requires_facts(
        &mut self,
        requirement: &CheckedRequirement,
        state: &mut FactState,
        event: FlowEventId,
    ) {
        let Some(goal) = self.input.body_requirement_goal(requirement) else {
            return;
        };
        let goal = self.reasoning().intern_goal_expression(goal);
        state.establish_goal(
            goal,
            super::super::state::GoalSign::Positive,
            &mut self.vocabulary.derivations,
            event,
        );
        if let Some(relation) = self.vocabulary.goals.projection(goal).cloned() {
            state.establish(&relation, &mut self.vocabulary.derivations, event);
        }
        // [ENT-3] Signed Boolean decomposition of the established body goal.
        self.reasoning().establish_boolean_decomposition(
            goal,
            super::super::state::GoalSign::Positive,
            state,
            event,
        );
        self.record_boolean_decomposition(goal, super::super::state::GoalSign::Positive, state);
    }
}

impl Analyzer<'_, '_> {
    // ------------------------------------------------------------------
    // Establishment of one evaluated value's image: S5, S6, S7, S9
    // ------------------------------------------------------------------

    /// Every source that establishes at an `ordinary_let_rhs` binding, in one
    /// place because they are mutually exclusive on the initializer's shape.
    /// A [SET-1] commit evaluates its right-hand side under exactly these
    /// rules before its target kill, naming the value by the occurrence's own
    /// commit-value term instead of a binder [ENT-2, ENT-3.S5].
    pub(super) fn establish_value_image(
        &mut self,
        node_path: &crate::NodePath,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) -> Option<EstablishedUnsignedDivision> {
        if self
            .reasoning()
            .establish_length_facts(node_path, destination, value, state, event)
        {
            return None;
        }
        if self
            .reasoning()
            .establish_element_range(node_path, destination, value, state, event)
        {
            return None;
        }
        if let Some(shape) = operation_shape(value) {
            return self.establish_operation_facts(
                node_path,
                destination,
                value,
                &shape,
                state,
                event,
            );
        }
        self.reasoning()
            .establish_copy_fact(node_path, destination, value, state, event);
        None
    }
}

impl Vocabulary {
    fn binding_event(
        &mut self,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
        kind: FlowEventKind,
        node_path: &crate::NodePath,
    ) -> FlowEventId {
        if let Some((existing_kind, id)) = event {
            debug_assert_eq!(*existing_kind, kind);
            return *id;
        }
        let id = self.proof_event(kind, Some(node_path));
        *event = Some((kind, id));
        id
    }

    /// The exact term named by one writable fragment place [ENT-2].
    fn writable_place_term(
        &mut self,
        binding: BindingId,
        fields: &[u32],
        ty: CheckedType,
    ) -> Option<TermId> {
        let fragment = fragment_type(ty)?;
        // [REF-1] a reference root is resolved to the path it names rather
        // than spelled with a synthesized `deref`, so the term carries the
        // written path and nothing else.
        let kind = TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, fields.to_vec()),
            fragment,
        );
        Some(self.terms.intern(kind))
    }

    /// [ENT-3.S5] admits direct scalar places, including fields and Box
    /// content reached through a reference. A subscript anywhere in the
    /// path excludes a commit image, even if its offset is a literal.
    pub(super) fn commit_target_term(&mut self, target: &CheckedSetTarget) -> Option<TermId> {
        match target {
            CheckedSetTarget::Place(target) => {
                self.writable_place_term(target.binding, &target.fields, target.ty)
            }
            CheckedSetTarget::Storage(target)
                if !target
                    .path
                    .iter()
                    .any(|step| matches!(step, CheckedPlaceStep::Subscript(_))) =>
            {
                let fragment = fragment_type(target.ty)?;
                let place = container_root_path(target);
                Some(self.terms.intern(TermKind::Place(place, fragment)))
            }
            _ => None,
        }
    }

    /// The term one evaluated value's image is established on: a freshly
    /// bound integer place, a commit value, or a private success payload.
    fn bound_term(
        &mut self,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
    ) -> Option<TermId> {
        match destination {
            ValueImage::Binding(binding) => self.writable_place_term(binding, &[], value.ty()),
            ValueImage::Commit(node_path) => self.commit_value_term(node_path, value),
            ValueImage::ResultPayload(payload) => Some(payload),
        }
    }

    /// The commit-value term of one `set` occurrence, interned on first use.
    /// Its identity is the statement's NodePath and the value's fragment
    /// type, so every source establishing at that one occurrence names one
    /// term [ENT-2].
    fn commit_value_term(
        &mut self,
        node_path: &crate::NodePath,
        value: &CheckedExpression,
    ) -> Option<TermId> {
        let kind = commit_value_kind(node_path, value)?;
        Some(self.terms.intern(kind))
    }

    /// The same term when a source above already formed it, and nothing when
    /// the right-hand side matched no source: an image-free value needs no
    /// term and no post-write equality [ENT-3.S5].
    pub(super) fn interned_commit_value_term(
        &self,
        node_path: &crate::NodePath,
        value: &CheckedExpression,
    ) -> Option<TermId> {
        self.terms.interned(&commit_value_kind(node_path, value)?)
    }
}

fn commit_value_kind(node_path: &crate::NodePath, value: &CheckedExpression) -> Option<TermKind> {
    let ty = fragment_type(value.ty())?;
    Some(TermKind::CommitValue {
        commit_path: node_path.components().to_vec(),
        ty,
    })
}

impl Reasoning<'_, '_, '_> {
    /// The value image shared by an ordinary let, a direct-place SET-1
    /// commit and a counted endpoint capture [FN-1]. Every admitted exact
    /// integer conversion preserves its input's mathematical value; checked
    /// and defined rows have another result type.
    fn copy_source(&mut self, value: &CheckedExpression) -> Option<TermId> {
        match value {
            CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Exact,
                source: CheckedNumericType::Integer(_),
                destination: CheckedNumericType::Integer(_),
                value: operand,
                ..
            } => self.copy_source(operand),
            _ => self.copy_operand(value),
        }
    }

    /// One atom read as an admitted [ENT-2] term or constant: a measure term
    /// through its [MSR-1] former, otherwise a tracked place or constant. This
    /// is the one complete reader every value image and S7 operand uses.
    fn copy_operand(&mut self, value: &CheckedExpression) -> Option<TermId> {
        self.measure_operand(value)
            .or_else(|| self.read_operand(value))
    }
}

impl Vocabulary {
    fn establish_copy_equality(
        &mut self,
        node_path: &crate::NodePath,
        destination: TermId,
        source: TermId,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) {
        let event = self.binding_event(event, FlowEventKind::S5, node_path);
        state.establish(
            &Relation::Equal {
                left: destination,
                right: source,
                difference: 0,
            },
            &mut self.derivations,
            event,
        );
    }
}

/// The place term a binding names directly, for length facts over an
/// allocated or borrowed collection.
pub(super) fn bound_place(binding: BindingId) -> ResolvedPlace {
    ResolvedPlace::binding(binding)
}

impl Reasoning<'_, '_, '_> {
    /// [ENT-3] S5: `let x: T = lit;` establishes x = value(lit);
    /// `let x: T = p;` with p a term establishes x = p; and
    /// `let y: Dst = cvt::<Src, Dst>(p);` after its [OP-6] domain proof
    /// establishes y = p, the conversion being exactly value-preserving.
    /// [MSR-3] the rebind placement, first half: at the pre-transfer point of
    /// one `let` or one [LIV-2] `set` whose right-hand side is a measured
    /// place, one immutable datum per [MSR-1] measure, established equal to
    /// that measure.
    ///
    /// The datum contains no place, so the consume the same statement
    /// performs cannot kill it. `let built = move spare;` is what needs it:
    /// without the datum a fresh name for a measured value starts with no
    /// measures at all, because the equality to the source dies with the
    /// source.
    pub(super) fn mint_rebind_datums(
        &mut self,
        node_path: &crate::NodePath,
        ordinal: u32,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Option<MeasureCarry> {
        let source = self.input.placement_source_place(value)?;
        self.mint_measure_datums(
            node_path,
            ordinal,
            MeasurePlacement::Rebind,
            source,
            value.ty(),
            state,
        )
    }
}

impl Input<'_, '_> {
    /// A placement reads the exact checked source place, including a moved
    /// field or Box content. Computed values and calls have their own fact
    /// sources and are not placements [MSR-3].
    pub(super) fn placement_source_place(
        &self,
        value: &CheckedExpression,
    ) -> Option<ResolvedPlace> {
        let source = match value {
            CheckedExpression::Project {
                binding, fields, ..
            } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: fields.iter().copied().map(PlaceStep::Field).collect(),
            }),
            CheckedExpression::BoxTake { binding, path, .. } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: path.iter().map(CheckedPlaceStep::place_step).collect(),
            }),
            _ => self.read_place_path(value),
        }?;
        // A source subscript must name an MSR-1 offset. A computed offset
        // and a possible-descendant cover have no exact placement identity.
        (!source.path.iter().any(|step| {
            matches!(step, PlaceStep::Descendant(_))
                || matches!(step, PlaceStep::Index(offset) if offset.term == CapturedTerm::Opaque)
        }))
        .then_some(source)
    }
}

impl Reasoning<'_, '_, '_> {
    /// [MSR-3] the first half of every placement below the entry and the
    /// call: at the point before the statement's own kills, one immutable
    /// datum per [MSR-1] measure of the source place, established equal to
    /// that measure.
    ///
    /// The datum contains no place, so neither the consume the statement
    /// performs nor the write it commits can kill it. That is the whole
    /// content of a placement: without a term with empty support standing
    /// between the two names, a measured value would arrive at its new name
    /// with no measures at all, because the equality to the source dies with
    /// the source.
    ///
    /// `ordinal` separates the placements one statement carries — a
    /// construct's field, a destructuring's binder, a target list's target,
    /// the displaced and the stored halves of one `replace`.
    pub(super) fn mint_measure_datums(
        &mut self,
        node_path: &crate::NodePath,
        ordinal: u32,
        placement: MeasurePlacement,
        source: ResolvedPlace,
        ty: CheckedType,
        state: &mut ProofFlowState,
    ) -> Option<MeasureCarry> {
        let mut carried = Vec::new();
        for (path, measured_type) in self.measured_paths(&source, ty) {
            let Some(measured) = super::measured_kind(measured_type) else {
                continue;
            };
            let constant = super::type_constant(measured_type);
            let mut place = source.clone();
            place.path.extend(path.iter().copied());
            let event = self
                .vocabulary
                .proof_event(FlowEventKind::S5, Some(node_path));
            let mut datums = Vec::with_capacity(4);
            for measure in MEASURES {
                let live = self.place_measure_term(measure, place.clone(), measured, constant);
                let datum = self.vocabulary.terms.intern(TermKind::MeasureDatum {
                    statement: node_path.components().to_vec(),
                    ordinal,
                    path: path.clone(),
                    placement,
                    measure,
                });
                self.vocabulary
                    .adopt_measure_atom(datum, live, &state.affine);
                state.facts.establish(
                    &Relation::Equal {
                        left: datum,
                        right: live,
                        difference: 0,
                    },
                    &mut self.vocabulary.derivations,
                    event,
                );
                datums.push(datum);
            }
            carried.push(CarriedMeasures {
                path,
                measured,
                constant,
                datums,
            });
        }
        (!carried.is_empty()).then_some(MeasureCarry { carried })
    }

    /// [MSR-1, MSR-3] measured descendants reached through owned fields,
    /// payloads and Box content. A structural walk covers acyclic type paths;
    /// already registered exact source terms supply deeper recursive paths.
    /// Both inventories are finite, without a depth limit on a written path.
    ///
    /// A term's presence does not establish its old facts: the datum is
    /// equated to the source in the current state, after all earlier kills.
    /// Unregistered descendants have no numeric facts beyond their standing
    /// type facts, which are available at the destination without transport.
    /// Payload paths stay variant-specific and add no refinement fact.
    fn measured_paths(
        &self,
        source: &ResolvedPlace,
        ty: CheckedType,
    ) -> Vec<(Vec<PlaceStep>, CheckedType)> {
        let mut found = Vec::new();
        if !self
            .input
            .collect_measured_paths(ty, &mut Vec::new(), &mut Vec::new(), &mut found)
        {
            return found;
        }
        let source = source.clone().term_identity();
        for term in self.vocabulary.terms.ids() {
            let TermKind::Measure(CheckedMeasure::Length, place) = self.vocabulary.terms.kind(term)
            else {
                continue;
            };
            if place.root != source.root {
                continue;
            }
            let Some(path) = place.path.strip_prefix(source.path.as_slice()) else {
                continue;
            };
            // No alias resolution or overlap test turns a possible target
            // into this source. The suffix must select concrete owned
            // fields; an unknown descendant cover is never such a step.
            let Some(selected) = self.input.measured_path_type(ty, path) else {
                continue;
            };
            if !found.iter().any(|(existing, _)| existing == path) {
                found.push((path.to_vec(), selected));
            }
        }
        found
    }
}

impl Input<'_, '_> {
    /// Returns whether a nominal cycle left paths for the finite source-term
    /// inventory to supply. Acyclic operands need no registry scan.
    fn collect_measured_paths(
        &self,
        ty: CheckedType,
        path: &mut Vec<PlaceStep>,
        ancestors: &mut Vec<NominalId>,
        found: &mut Vec<(Vec<PlaceStep>, CheckedType)>,
    ) -> bool {
        if super::measured_kind(ty).is_some() {
            found.push((path.clone(), ty));
            return false;
        }
        let CheckedType::Nominal(nominal) = ty else {
            return false;
        };
        let Some(kind) = self
            .context
            .nominals
            .get(nominal.0 as usize)
            .map(|record| &record.kind)
        else {
            return false;
        };
        if ancestors.contains(&nominal) {
            return true;
        }
        ancestors.push(nominal);
        let mut recursive = false;
        match kind {
            CheckedNominalKind::Struct { fields } => {
                for (ordinal, field) in fields.iter().enumerate() {
                    let Ok(ordinal) = u32::try_from(ordinal) else {
                        continue;
                    };
                    path.push(PlaceStep::Field(ordinal));
                    recursive |= self.collect_measured_paths(field.ty, path, ancestors, found);
                    path.pop();
                }
            }
            CheckedNominalKind::Enum { variants } => {
                for variant in variants {
                    for (ordinal, field) in variant.fields.iter().enumerate() {
                        let Ok(field_ordinal) = u32::try_from(ordinal) else {
                            continue;
                        };
                        path.push(PlaceStep::Payload {
                            variant: variant.tag,
                            field: field_ordinal,
                        });
                        recursive |= self.collect_measured_paths(field.ty, path, ancestors, found);
                        path.pop();
                    }
                }
            }
            CheckedNominalKind::Box { referent, .. } => {
                path.push(PlaceStep::Deref);
                recursive |= self.collect_measured_paths(*referent, path, ancestors, found);
                path.pop();
            }
            CheckedNominalKind::Opaque => {}
        }
        ancestors.pop();
        recursive
    }

    /// Replays a finite owned projection against its actual operand type.
    /// Element and range storage are not aggregate ownership projections.
    fn measured_path_type(&self, mut ty: CheckedType, path: &[PlaceStep]) -> Option<CheckedType> {
        for step in path {
            let CheckedType::Nominal(nominal) = ty else {
                return None;
            };
            ty = match (&self.context.nominals.get(nominal.0 as usize)?.kind, step) {
                (CheckedNominalKind::Struct { fields }, PlaceStep::Field(field)) => {
                    fields.get(*field as usize)?.ty
                }
                (CheckedNominalKind::Box { referent, .. }, PlaceStep::Deref) => *referent,
                (CheckedNominalKind::Enum { variants }, PlaceStep::Payload { variant, field }) => {
                    variants
                        .iter()
                        .find(|candidate| candidate.tag == *variant)?
                        .fields
                        .get(*field as usize)?
                        .ty
                }
                _ => return None,
            };
        }
        super::measured_kind(ty).map(|_| ty)
    }
}

impl Reasoning<'_, '_, '_> {
    /// [MSR-3] the rebind placement, second half: after the transfer, the
    /// destination binding's own measures equal the datums minted before it.
    pub(super) fn establish_rebind_datums(
        &mut self,
        node_path: &crate::NodePath,
        binding: BindingId,
        rebind: &MeasureCarry,
        state: &mut FactState,
    ) {
        let target = bound_place(binding);
        self.establish_measure_datums(node_path, target, rebind, state);
    }

    /// [MSR-3] the second half of every placement: after the statement's own
    /// kills, the destination place's measures equal the datums minted before
    /// them.
    pub(super) fn establish_measure_datums(
        &mut self,
        node_path: &crate::NodePath,
        destination: ResolvedPlace,
        carry: &MeasureCarry,
        state: &mut FactState,
    ) {
        let event = self
            .vocabulary
            .proof_event(FlowEventKind::S5, Some(node_path));
        for carried in &carry.carried {
            let mut place = destination.clone();
            place.path.extend(carried.path.iter().copied());
            for (measure, datum) in MEASURES.into_iter().zip(&carried.datums) {
                let left = self.place_measure_term(
                    measure,
                    place.clone(),
                    carried.measured,
                    carried.constant,
                );
                state.establish(
                    &Relation::Equal {
                        left,
                        right: *datum,
                        difference: 0,
                    },
                    &mut self.vocabulary.derivations,
                    event,
                );
            }
        }
    }

    fn establish_copy_fact(
        &mut self,
        node_path: &crate::NodePath,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) {
        let Some(source) = self.copy_source(value) else {
            return;
        };
        let Some(bound) = self.vocabulary.bound_term(destination, value) else {
            return;
        };
        self.vocabulary
            .establish_copy_equality(node_path, bound, source, state, event);
    }
}

impl Vocabulary {
    /// [ENT-3] S5 at a SET-1 value commit. The caller has already evaluated
    /// the right-hand side to the `commit` term above and killed every fact
    /// about the old target value; this equality names that evaluated value,
    /// so an arithmetic or other computed right-hand side carries its own
    /// image across the write exactly as an intervening `let` would. Only a
    /// direct fragment place receives it; indexed storage establishes
    /// nothing, since one element write is no image of the whole collection.
    pub(super) fn establish_commit_copy_fact(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        commit: TermId,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) {
        let Some(destination) = self.commit_target_term(target) else {
            return;
        };
        self.establish_copy_equality(node_path, destination, commit, state, event);
    }
}

impl Reasoning<'_, '_, '_> {
    /// [ENT-3] S6: `len::<T>(P)` for a tracked P establishes m = len_of(P); and
    /// `slice_of…(&'r P)` for a tracked P establishes len_of(s) = len_of(P).
    ///
    /// An `array<T, N>` allocation needs no clause here: its length equality
    /// is the [ENT-2] implicit fact carried by every length term over an
    /// array-typed place, registered wherever that term is interned.
    fn establish_length_facts(
        &mut self,
        node_path: &crate::NodePath,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) -> bool {
        match self.measure_operand(value) {
            Some(source_length) => {
                if let Some(bound) = self.vocabulary.bound_term(destination, value) {
                    let event = self
                        .vocabulary
                        .binding_event(event, FlowEventKind::S6, node_path);
                    state.establish(
                        &Relation::Equal {
                            left: bound,
                            right: source_length,
                            difference: 0,
                        },
                        &mut self.vocabulary.derivations,
                        event,
                    );
                }
                true
            }
            None => false,
        }
    }

    /// The measure term one [MSR-1] measure former reads, over the same
    /// place the obligation judgment forms for P, so both name one term
    /// [ENT-2]. A place whose subscript offsets are not all captured terms
    /// or constants is no term: its identity could not tell two elements
    /// apart.
    pub(super) fn measure_operand(&mut self, value: &CheckedExpression) -> Option<TermId> {
        let represented = |term| term == Some(SubscriptedTerm::Represented);
        let (measure, place, measured, array_length) = match value {
            CheckedExpression::ArrayMeasure {
                measure,
                root,
                length,
            } => (
                *measure,
                array_root_place(root),
                MeasuredKind::ConstantArray,
                Some(*length),
            ),
            CheckedExpression::BufferMeasure { measure, root }
                if represented(root.subscripted_term()) =>
            {
                (
                    *measure,
                    ResolvedPlace::from_path(root.binding, root.place_path()),
                    MeasuredKind::RuntimeArray,
                    None,
                )
            }
            // [MSR-1, REF-4] a range reference's one measure, over the place
            // the reference names [REF-1].
            CheckedExpression::RangeMeasure { measure, root } => (
                *measure,
                ResolvedPlace::spelled(
                    PlaceRoot::Binding(root.binding),
                    is_holder(root.binding),
                    Vec::new(),
                ),
                MeasuredKind::Range,
                None,
            ),
            // [MSR-1] a storage shape's measure reader names the
            // same [ENT-2] term the clause and the invariant name, so a `let`
            // over one is the ordinary [ENT-3.S6] equality a buffer's is.
            CheckedExpression::ContainerMeasure { measure, root }
                if represented(root.subscripted_term()) =>
            {
                return Some(self.place_measure_term(
                    *measure,
                    container_root_path(root),
                    root.measured()?,
                    root.type_constant(),
                ));
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. }
                if represented(place.subscripted_term()) =>
            {
                return Some(self.place_measure_term(
                    *measure,
                    ResolvedPlace::from_path(place.root.binding, place.place_path()),
                    place.measured()?,
                    place.type_constant(),
                ));
            }
            _ => return None,
        };
        Some(self.place_measure_term(measure, place, measured, array_length))
    }
}

// ------------------------------------------------------------------
// S7 operation facts
// ------------------------------------------------------------------

/// The [ENT-3.S7] row one right-hand side reads as: an integer-valued
/// operation of the table or an integer `reinterpret`. Defined, checked
/// and comparison rows produce no integer, and a type-parameter-typed
/// operation has no fragment type, so neither is a row here.
fn operation_shape(value: &CheckedExpression) -> Option<OperationShape<'_>> {
    match value {
        CheckedExpression::IntegerOperation {
            carrier,
            operation,
            operand_type: CheckedType::Integer(operand),
            arguments,
            result: CheckedType::Integer(result),
            ..
        } => Some(OperationShape {
            row: Row::of(*operation)?,
            operand: *operand,
            result: *result,
            operands: arguments,
            carrier,
        }),
        CheckedExpression::Reinterpret {
            carrier,
            source: CheckedNumericType::Integer(operand),
            destination: CheckedNumericType::Integer(result),
            value,
        } => Some(OperationShape {
            row: Row::Reinterpret,
            operand: *operand,
            result: *result,
            operands: std::slice::from_ref(value.as_ref()),
            carrier,
        }),
        _ => None,
    }
}

/// The exact row a checked row's success payload is read as [ENT-5].
fn checked_operation_shape(value: &CheckedExpression) -> Option<OperationShape<'_>> {
    let CheckedExpression::IntegerOperation {
        carrier,
        operation,
        operand_type: CheckedType::Integer(operand),
        arguments,
        ..
    } = value
    else {
        return None;
    };
    Some(OperationShape {
        row: Row::of_checked(*operation)?,
        operand: *operand,
        result: *operand,
        operands: arguments,
        carrier,
    })
}

impl Analyzer<'_, '_> {
    /// [ENT-5] a checked integer row's private success payload denotes the
    /// exact row's mathematical result and receives that row's [ENT-3.S7]
    /// facts, read in the ordinary closed state `facts` already holds.
    pub(super) fn establish_checked_payload(
        &mut self,
        statement: &crate::NodePath,
        payload: TermId,
        value: &CheckedExpression,
        facts: &mut FactState,
    ) {
        if let Some(shape) = checked_operation_shape(value) {
            let _ = self.establish_operation_facts(
                statement,
                ValueImage::ResultPayload(payload),
                value,
                &shape,
                facts,
                &mut None,
            );
        }
    }
}

impl Vocabulary {
    /// The mathematical value of a constant term. Z is the interned form of
    /// the written constant zero, so it reads as one here.
    fn constant_term_value(&self, term: TermId) -> Option<i128> {
        match *self.terms.kind(term) {
            TermKind::Zero => Some(0),
            TermKind::Constant(value) => Some(value),
            _ => None,
        }
    }
}

impl Analyzer<'_, '_> {
    /// [ENT-3.S7] the result bounds and operand relations one table row
    /// establishes on the value it binds.
    ///
    /// Each operand reads one interval in the closed state where the
    /// right-hand side is evaluated: a literal or named-const value is its
    /// value, any other admitted term its strongest closed bounds through Z,
    /// and every other operand its type's interval. The closed bounds read are
    /// the parents of every fact the row establishes, so the retained
    /// derivation shows what each fact stood on [DIAG-2]. Operands are read
    /// with the one complete term reader, so a measure operand is a term here
    /// exactly as it is in a copy [ENT-2].
    ///
    /// An unsigned exact division bound at a binding or commit also returns
    /// the fact bounding the quotient by its dividend, which the caller's
    /// affine division images cite.
    fn establish_operation_facts(
        &mut self,
        node_path: &crate::NodePath,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
        shape: &OperationShape<'_>,
        state: &mut FactState,
        shared_event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) -> Option<EstablishedUnsignedDivision> {
        let terms = shape
            .operands
            .iter()
            .map(|operand| self.reasoning().copy_operand(operand))
            .collect::<Vec<_>>();
        let mut closed = None;
        let mut intervals = Vec::with_capacity(terms.len());
        let mut related = Vec::with_capacity(terms.len());
        let mut parents = Vec::new();
        for (operand, term) in shape.operands.iter().zip(&terms) {
            let ty = fragment_type(operand.ty())?;
            let span = Span::of(ty).interval();
            let Some(term) = *term else {
                intervals.push(span);
                related.push(None);
                continue;
            };
            if let Some(value) = self.vocabulary.constant_term_value(term) {
                intervals.push(Interval::value(value));
                related.push(None);
                continue;
            }
            let view = match &closed {
                Some(view) => Rc::clone(view),
                None => {
                    let view = close(
                        state,
                        &self.vocabulary.terms,
                        &self.vocabulary.goals,
                        &mut self.vocabulary.derivations,
                    );
                    closed = Some(Rc::clone(&view));
                    view
                }
            };
            // At a contradictory point every relation is already derivable.
            if view.contradictory() {
                return None;
            }
            let high = view.tight_bound(term, ZERO).unwrap_or(span.high);
            let low = view
                .tight_bound(ZERO, term)
                .and_then(i128::checked_neg)
                .unwrap_or(span.low);
            parents.extend(view.bound_proof(term, ZERO, high, &mut self.vocabulary.derivations));
            parents.extend(view.bound_proof(ZERO, term, -low, &mut self.vocabulary.derivations));
            intervals.push(Interval::new(low, high));
            related.push(Some(term));
        }
        let single = intervals
            .iter()
            .all(|interval| interval.low == interval.high);
        let product = matches!(shape.row, Row::Multiply { wrap: false })
            .then(|| self.frames.product_intervals.get(shape.carrier).cloned())
            .flatten();
        if let (Some((_, Some(domain))), false) = (&product, single) {
            // The interval-product rule's four products are the measurement
            // the domain decision consumed; that decision is the parent.
            parents = vec![*domain];
        }
        let facts = operation_facts::facts(
            shape.row,
            Span::of(shape.operand),
            Span::of(shape.result),
            &intervals,
            product.map(|(interval, _)| Interval::new(interval.minimum, interval.maximum)),
        );
        if facts.bounds.is_none() && facts.relations.is_empty() {
            return None;
        }
        let result = self.vocabulary.bound_term(destination, value)?;
        let event = self
            .vocabulary
            .binding_event(shared_event, FlowEventKind::S7, node_path);
        let parents = parents.into_boxed_slice();
        let mut upper = None;
        if let Some(bounds) = facts.bounds {
            if let Some(low) = bounds.low.checked_neg() {
                self.vocabulary
                    .establish_operation_fact(state, ZERO, result, low, event, &parents);
            }
            upper = Some(self.vocabulary.establish_operation_fact(
                state,
                result,
                ZERO,
                bounds.high,
                event,
                &parents,
            ));
        }
        let mut dividend_order = None;
        for relation in &facts.relations {
            let Some(term) = related.get(relation.operand).copied().flatten() else {
                continue;
            };
            if let Some(high) = relation.high {
                let proof = self
                    .vocabulary
                    .establish_operation_fact(state, result, term, high, event, &parents);
                if relation.operand == 0 && high == 0 {
                    dividend_order = Some(proof);
                }
            }
            if let Some(low) = relation.low.and_then(i128::checked_neg) {
                self.vocabulary
                    .establish_operation_fact(state, term, result, low, event, &parents);
            }
        }
        // [ENT-3.S7] an unsigned exact division at a binding or commit also
        // captures its value images; a conditional payload captures none.
        if shape.row != Row::Divide
            || shape.operand.signed()
            || matches!(destination, ValueImage::ResultPayload(_))
            || terms.iter().any(Option::is_none)
        {
            return None;
        }
        let literal_divisor = match &shape.operands[1] {
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
                if *ty == shape.operand =>
            {
                let value = integer_value(*ty, *bits);
                (value > 0).then_some(value)
            }
            _ => None,
        };
        let parent = if related[0].is_some() {
            dividend_order
        } else {
            upper
        }?;
        Some(EstablishedUnsignedDivision {
            literal_divisor,
            parent,
        })
    }
}

impl Vocabulary {
    /// Establishes `left - right <= bound` as one [ENT-3.S7] fact whose
    /// parents are the closed operand bounds its row read [DIAG-2].
    fn establish_operation_fact(
        &mut self,
        state: &mut FactState,
        left: TermId,
        right: TermId,
        bound: i128,
        event: FlowEventId,
        parents: &[DerivationId],
    ) -> DerivationId {
        let relation = Relation::Bound { left, right, bound };
        let proof = self.derivations.intern(DerivationNode::OperationFact {
            relation: relation.clone(),
            event,
            parents: parents.into(),
        });
        state.establish_from_proof(&relation, proof, &self.derivations);
        proof
    }
}

impl Reasoning<'_, '_, '_> {
    /// [ENT-3] S9: `let x: T = c[i];` where c is the bare IDENT of a
    /// named const of type `array<T, N>` and T a fragment type establishes
    /// vlo <= x and x <= vhi over its N declared element values. The index's
    /// own bounds obligation is judged separately and is unaffected. Deeper
    /// const shapes establish nothing.
    fn establish_element_range(
        &mut self,
        node_path: &crate::NodePath,
        destination: ValueImage<'_>,
        value: &CheckedExpression,
        state: &mut FactState,
        event: &mut Option<(FlowEventKind, FlowEventId)>,
    ) -> bool {
        let constant = match value {
            CheckedExpression::ArrayIndex {
                root: CheckedArrayRoot::Constant(constant),
                ..
            } => *constant,
            CheckedExpression::ReadStorage { root, .. } => {
                let PlaceRoot::Constant(constant) = root.root else {
                    return false;
                };
                // Typed storage preserves the same bare-constant, single
                // subscript source shape. Fields or deeper subscripts do
                // not gain an element-range fact through this adapter.
                if !matches!(root.path.as_slice(), [CheckedPlaceStep::Subscript(_)]) {
                    return false;
                }
                constant
            }
            _ => return false,
        };
        let Some(constant) = self.input.context.constants.get(constant.0 as usize) else {
            return true;
        };
        let CheckedValue::Array { elements, .. } = &constant.value else {
            return true;
        };
        let mut range: Option<(i128, i128)> = None;
        for element in elements {
            let CheckedValue::Integer { ty, bits } = element else {
                return true;
            };
            let element = integer_value(*ty, *bits);
            range = Some(match range {
                Some((low, high)) => (low.min(element), high.max(element)),
                None => (element, element),
            });
        }
        let (Some((low, high)), Some(bound)) =
            (range, self.vocabulary.bound_term(destination, value))
        else {
            return true;
        };
        let event = self
            .vocabulary
            .binding_event(event, FlowEventKind::S9, node_path);
        state.establish(
            &Relation::Bound {
                left: ZERO,
                right: bound,
                bound: -low,
            },
            &mut self.vocabulary.derivations,
            event,
        );
        state.establish(
            &Relation::Bound {
                left: bound,
                right: ZERO,
                bound: high,
            },
            &mut self.vocabulary.derivations,
            event,
        );
        true
    }

    // ------------------------------------------------------------------
    // Arm entry facts
    // ------------------------------------------------------------------

    /// The [ENT-3] arm facts one match scrutinee admits: the S1 comparison
    /// relation and goal origins of a `Bool` match. A checked row's success
    /// facts reach its `Ok` arm through the scrutinee's conditional Result
    /// context instead [ENT-5].
    pub(super) fn arm_facts(
        &mut self,
        scrutinee: &CheckedExpression,
        enum_type: CheckedEnumType,
        state: &FactState,
    ) -> ArmFacts {
        let node_path = expression_node_path(scrutinee).cloned();
        if enum_type != CheckedEnumType::Bool {
            return ArmFacts {
                node_path,
                comparison: None,
                goals: Vec::new(),
            };
        }
        ArmFacts {
            node_path,
            comparison: self.scrutinee_relation(scrutinee, state),
            goals: self.goal_origin_set(scrutinee, state),
        }
    }
}

/// The normalized relation of one comparison operation over two read
/// operands, shared by the comparison-origin shape and S4's substitution.
///
/// `gap` is the displacement the two sides carry, `right`'s constant minus
/// `left`'s: a clause side is an affine expression [MSR-5], and
/// `at + 2_u64 <= len_of(run)` is the ordinary difference bound
/// `at - len_of(run) <= -2`.
pub(super) fn comparison_relation(
    operation: CheckedIntegerOperation,
    left: TermId,
    right: TermId,
    gap: i128,
) -> Option<Relation> {
    Some(match operation {
        CheckedIntegerOperation::Equal => Relation::Equal {
            left,
            right,
            difference: gap,
        },
        CheckedIntegerOperation::NotEqual => Relation::Distinct {
            left,
            right,
            difference: gap,
        },
        CheckedIntegerOperation::Less => Relation::Bound {
            left,
            right,
            bound: gap.checked_sub(1)?,
        },
        CheckedIntegerOperation::LessEqual => Relation::Bound {
            left,
            right,
            bound: gap,
        },
        CheckedIntegerOperation::Greater => Relation::Bound {
            left: right,
            right: left,
            bound: gap.checked_neg()?.checked_sub(1)?,
        },
        CheckedIntegerOperation::GreaterEqual => Relation::Bound {
            left: right,
            right: left,
            bound: gap.checked_neg()?,
        },
        _ => return None,
    })
}

/// The three [MSR-1] measures, in the order every former reads them.
const MEASURES: [CheckedMeasure; 3] = [
    CheckedMeasure::Length,
    CheckedMeasure::Capacity,
    CheckedMeasure::Head,
];

/// [MSR-3] one placement's datums, held between the mint before the
/// statement's kills and the establishment after them.
///
/// One placement carries one entry per measured place its operand reaches by
/// field or payload selection, so an aggregate operand carries the measures
/// of every run beneath it and not only its own.
pub(super) struct MeasureCarry {
    carried: Vec<CarriedMeasures>,
}

/// The datums of one measured place under one placement, with the aggregate
/// projection path that reaches it from the placement's source and destination.
struct CarriedMeasures {
    path: Vec<PlaceStep>,
    measured: MeasuredKind,
    constant: Option<CheckedConst>,
    datums: Vec<TermId>,
}

impl Vocabulary {
    pub(super) fn eligible_delivery_terms(
        &mut self,
        value: &CheckedExpression,
        receiver_type: CheckedType,
    ) -> Option<(BindingId, TermId, IntegerType)> {
        let CheckedExpression::Binding {
            binding,
            ty,
            consume_root: false,
            ..
        } = value
        else {
            return None;
        };
        if *ty != receiver_type {
            return None;
        }
        let fragment = fragment_type(*ty)?;
        let carrier = self.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(*binding), false, Vec::new()),
            fragment,
        ));
        Some((*binding, carrier, fragment))
    }

    pub(super) fn delivery_edge_state(
        &mut self,
        closed: ClosedState,
        context: &DeliveryEdgeContext<'_>,
    ) -> FactState {
        if closed.contradictory() {
            return FactState::contradictory(
                closed
                    .contradiction_proof()
                    .expect("contradictory delivery edge has one exact proof"),
            );
        }
        let mut image = FactState::new();
        let mut explicit = HashMap::new();
        for (source_relation, parent) in closed.delivery_relations() {
            if !source_relation.terms().contains(&context.carrier)
                || !self
                    .derivations
                    .depends_on_explicit_relation(parent, &mut explicit)
            {
                continue;
            }
            let relation =
                substitute_delivery_relation(&source_relation, context.carrier, context.receiver);
            let proof = self.derivations.intern(DerivationNode::PostconditionGive {
                statement: context.statement.clone(),
                carrier: context.carrier_binding,
                receiver: context.receiver_binding,
                relation: Box::new(relation.clone()),
                event: context.event,
                parent,
            });
            image.establish_from_proof(&relation, proof, &self.derivations);
        }
        image
    }

    pub(super) fn retain_delivery_give_parents(&mut self, parents: &[JoinParent]) {
        for parent in parents {
            if !matches!(
                self.derivations.nodes[parent.parent.0 as usize],
                DerivationNode::PostconditionGive { .. }
            ) {
                continue;
            }
            let occurrence = u32::try_from(self.delivery_give_roots.len())
                .expect("value-if give roots exceed the u32 identity space");
            // A full and an ordinary delivery join can share an edge parent.
            // The ledger retains one required root per Give node.
            if !self.delivery_give_roots.insert(parent.parent) {
                continue;
            }
            self.derivations.add_root(
                DerivationRootKind::PostconditionGive { occurrence },
                parent.parent,
            );
        }
    }

    pub(super) fn establish_delivery_join(
        &mut self,
        images: &[FactState],
        context: &DeliveryJoinContext<'_>,
        target: &mut FactState,
    ) {
        self.establish_delivery_join_once(images, context, target, false);
        if !images
            .iter()
            .any(FactState::may_hold_postcondition_candidates)
        {
            return;
        }
        // Substitution preserves both proof layers. The join must do so too:
        // selecting only the strongest edge proof here would lose an ordinary
        // fallback when a later holder event removes call-dependent proofs.
        let mut ordinary = images.to_vec();
        for image in &mut ordinary {
            image.retain_non_postcondition_candidates(&self.derivations);
        }
        self.establish_delivery_join_once(&ordinary, context, target, true);
    }

    pub(super) fn establish_delivery_join_once(
        &mut self,
        images: &[FactState],
        context: &DeliveryJoinContext<'_>,
        target: &mut FactState,
        ordinary_only: bool,
    ) {
        assert!(images.iter().all(|image| {
            image.all_derivable
                || image.live_l0_relations().iter().all(|(_, proof)| {
                    matches!(
                        self.derivations.nodes[proof.0 as usize],
                        DerivationNode::PostconditionGive { .. }
                    )
                })
        }));
        let contributing = images
            .iter()
            .enumerate()
            .filter_map(|(index, image)| (!image.all_derivable).then_some(index))
            .collect::<Vec<_>>();
        let Some((&first_index, rest)) = contributing.split_first() else {
            return;
        };
        let first = &images[first_index];
        let bound_pairs = first
            .bounds
            .cells()
            .map(|(left, right, bound, _)| ((left, right), bound))
            .collect::<Vec<_>>();
        for (pair, first_bound) in bound_pairs {
            if pair.0 != context.receiver && pair.1 != context.receiver {
                continue;
            }
            let mut weakest = first_bound;
            if !rest.iter().all(|index| {
                images[*index]
                    .bounds
                    .get(pair.0, pair.1)
                    .is_some_and(|(bound, _)| {
                        weakest = weakest.max(bound);
                        true
                    })
            }) {
                continue;
            }
            if ordinary_only
                && target
                    .bounds
                    .get(pair.0, pair.1)
                    .is_some_and(|(bound, proof)| {
                        bound <= weakest && !self.derivations.depends_on_postcondition_call(proof)
                    })
            {
                continue;
            }
            let parents = images
                .iter()
                .enumerate()
                .map(|(ordinal, image)| JoinParent {
                    ordinal: u32::try_from(ordinal)
                        .expect("delivery predecessor ordinal exceeds the u32 identity space"),
                    parent: if image.all_derivable {
                        image
                            .contradiction
                            .expect("contradictory delivery image has one proof")
                    } else {
                        image
                            .bounds
                            .get(pair.0, pair.1)
                            .map(|(_, proof)| proof)
                            .expect("every contributing delivery image holds the pair")
                    },
                })
                .collect::<Vec<_>>();
            let relation = Relation::Bound {
                left: pair.0,
                right: pair.1,
                bound: weakest,
            };
            let proof = self
                .derivations
                .intern(DerivationNode::PostconditionDeliveryJoin {
                    detail: Box::new(super::super::state::PostconditionDeliveryJoinDetail {
                        statement: context.statement.clone(),
                        receiver: context.receiver_binding,
                        relation: relation.clone(),
                        event: context.event,
                        parents,
                    }),
                });
            let DerivationNode::PostconditionDeliveryJoin { detail } =
                &self.derivations.nodes[proof.0 as usize]
            else {
                unreachable!("just interned one delivery join")
            };
            let parents = detail.parents.clone();
            self.retain_delivery_give_parents(&parents);
            let occurrence = self.delivery_join_roots;
            self.delivery_join_roots = self
                .delivery_join_roots
                .checked_add(1)
                .expect("value-if delivery join roots exceed the u32 identity space");
            self.derivations.add_root(
                DerivationRootKind::PostconditionDeliveryJoin { occurrence },
                proof,
            );
            target.establish_from_proof(&relation, proof, &self.derivations);
        }

        let mut distinct = first.distinct.iter().copied().collect::<Vec<_>>();
        distinct.sort_unstable();
        for pair in distinct {
            if (pair.0 != context.receiver && pair.1 != context.receiver)
                || !rest
                    .iter()
                    .all(|index| images[*index].distinct.contains(&pair))
            {
                continue;
            }
            if ordinary_only
                && target
                    .distinct_proofs
                    .get(&pair)
                    .is_some_and(|proof| !self.derivations.depends_on_postcondition_call(*proof))
            {
                continue;
            }
            let parents = images
                .iter()
                .enumerate()
                .map(|(ordinal, image)| JoinParent {
                    ordinal: u32::try_from(ordinal)
                        .expect("delivery predecessor ordinal exceeds the u32 identity space"),
                    parent: if image.all_derivable {
                        image
                            .contradiction
                            .expect("contradictory delivery image has one proof")
                    } else {
                        image.distinct_proofs[&pair]
                    },
                })
                .collect::<Vec<_>>();
            let relation = Relation::Distinct {
                left: pair.0,
                right: pair.1,
                difference: 0,
            };
            let proof = self
                .derivations
                .intern(DerivationNode::PostconditionDeliveryJoin {
                    detail: Box::new(super::super::state::PostconditionDeliveryJoinDetail {
                        statement: context.statement.clone(),
                        receiver: context.receiver_binding,
                        relation: relation.clone(),
                        event: context.event,
                        parents,
                    }),
                });
            let DerivationNode::PostconditionDeliveryJoin { detail } =
                &self.derivations.nodes[proof.0 as usize]
            else {
                unreachable!("just interned one delivery join")
            };
            let parents = detail.parents.clone();
            self.retain_delivery_give_parents(&parents);
            let occurrence = self.delivery_join_roots;
            self.delivery_join_roots = self
                .delivery_join_roots
                .checked_add(1)
                .expect("value-if delivery join roots exceed the u32 identity space");
            self.derivations.add_root(
                DerivationRootKind::PostconditionDeliveryJoin { occurrence },
                proof,
            );
            target.establish_from_proof(&relation, proof, &self.derivations);
        }
    }

    pub(super) fn establish_value_delivery_join(
        &mut self,
        frame: &GiveFrame,
        target: &mut ProofFlowState,
    ) {
        assert_eq!(frame.delivery_images.len(), frame.gives.len());
        assert_eq!(frame.delivery_edges.len(), frame.delivery_images.len());
        assert!(
            frame
                .delivery_edges
                .windows(2)
                .all(|pair| { pair[0].components().cmp(pair[1].components()).is_lt() })
        );
        let Some(fragment) = fragment_type(frame.result_type) else {
            return;
        };
        let receiver = self.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(PlaceRoot::Binding(frame.binding), false, Vec::new()),
            fragment,
        ));
        let event = self.proof_event(
            FlowEventKind::PostconditionDeliveryJoin,
            Some(&frame.node_path),
        );
        let context = DeliveryJoinContext {
            statement: &frame.node_path,
            receiver_binding: frame.binding,
            receiver,
            event,
        };
        let facts = frame
            .delivery_images
            .iter()
            .map(|image| image.facts.clone())
            .collect::<Vec<_>>();
        self.establish_delivery_join(&facts, &context, &mut target.facts);
    }

    /// Captures one unsigned division for the fixed product consequence and
    /// publishes the existing literal-divisor scaled image. Later writes get
    /// new atoms and cannot retarget either consequence.
    pub(super) fn establish_unsigned_division_image(
        &mut self,
        quotient: &AffineForm,
        dividend: &AffineForm,
        divisor: &AffineForm,
        established: sources::EstablishedUnsignedDivision,
        state: &mut AffineFlowState,
    ) {
        self.unsigned_divisions.push(CapturedUnsignedDivision {
            quotient: quotient.clone(),
            dividend: dividend.clone(),
            divisor: divisor.clone(),
            parent: established.parent,
        });
        let Some(scale) = established.literal_divisor else {
            return;
        };
        let Ok(scaled_quotient) = quotient.scale(scale, &mut AffineCheckState::new()) else {
            return;
        };
        let Some(inequality) = affine_less_equal(&scaled_quotient, dividend) else {
            return;
        };
        state.facts.push(ActiveAffineFact {
            inequality,
            evidence: AffineFactEvidence::Derivation(established.parent),
            active_loops: Vec::new(),
        });
    }
}

impl Reasoning<'_, '_, '_> {
    /// S4 captures only non-L0 affine ordering leaves already established by
    /// the requirement's fixed signed decomposition. The immutable images
    /// cannot be retargeted by a later scalar assignment or measure kill.
    pub(super) fn establish_requirement_affine_images(
        &mut self,
        requirement: &crate::semantic::goal::CheckedRequirement,
        ordinal: usize,
        state: &mut ProofFlowState,
    ) {
        let Some(expression) = self.input.body_requirement_goal(requirement) else {
            return;
        };
        let goal = self.intern_goal_expression(expression);
        let mut members = vec![(goal, GoalSign::Positive)];
        members.extend(self.signed_boolean_decomposition(goal, GoalSign::Positive, &state.facts));
        for (member, (goal, sign)) in members.into_iter().enumerate() {
            // Existing L0 projections remain on their ordinary route; putting
            // them in this list would widen AUTO's bounded premise sums.
            if self.vocabulary.goals.projection(goal).is_some() {
                continue;
            }
            let expression = self.vocabulary.goals.expression(goal).clone();
            let Some(inequality) =
                self.affine_signed_goal_ordering_target(&expression, &state.affine, sign)
            else {
                continue;
            };
            let parent = state
                .facts
                .opaque_proofs
                .get(&(goal, sign))
                .copied()
                .expect("every S4 decomposition member is established");
            let parent = self.vocabulary.derivations.intern(
                super::super::state::DerivationNode::RequirementAffineImage { goal, sign, parent },
            );
            self.vocabulary.derivations.add_root(
                DerivationRootKind::RequirementAffineImage {
                    requirement: u32::try_from(ordinal).expect("requirement ordinal exceeds u32"),
                    member: u32::try_from(member).expect("decomposition ordinal exceeds u32"),
                },
                parent,
            );
            state.affine.facts.push(ActiveAffineFact {
                inequality,
                evidence: AffineFactEvidence::Derivation(parent),
                active_loops: Vec::new(),
            });
        }
    }

    /// [MSR-3] the datums one [SET-1] commit carries, minted before the
    /// statement's own kills.
    ///
    /// The right-hand side is a bare use of a measured place, which is the
    /// same shape the `let` rebind placement admits: the value keeps every
    /// measure it had and only the name it is reached by changes. Every other
    /// right-hand side mints none, and the ordinary sources establish
    /// whatever that expression publishes.
    pub(super) fn mint_commit_placement(
        &mut self,
        node_path: &crate::NodePath,
        ordinal: u32,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Option<MeasureCarry> {
        let source = self.input.placement_source_place(value)?;
        let destination = set_target_place(target)?;
        let placement = if matches!(destination.path.last(), Some(PlaceStep::Index(_))) {
            MeasurePlacement::Element
        } else {
            MeasurePlacement::Rebind
        };
        self.mint_measure_datums(node_path, ordinal, placement, source, value.ty(), state)
    }

    /// [MSR-3] the construct placement: the datums one `construct`'s field
    /// operands carry into the fields of the value they fill.
    ///
    /// A field whose operand is a bare use of a measured place carries that
    /// place's measures into the field, which is the one event at which a
    /// measured value enters a nominal it did not previously belong to. The
    /// operand shape admitted is the shape every other placement admits: the
    /// value keeps every measure it had and only the place it is reached by
    /// changes.
    pub(super) fn mint_construct_placements(
        &mut self,
        node_path: &crate::NodePath,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Vec<(PlaceStep, MeasureCarry)> {
        let (fields, payload) = match value {
            CheckedExpression::ConstructStruct { fields, .. } => (fields, None),
            // [MSR-3] every enum variant has its own payload step. The
            // constructor selects the exact variant now, so its field
            // destination is `.Variant.field`, even when another variant
            // also carries fields.
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } => {
                let Some(nominal) = self.input.context.nominals.get(nominal.0 as usize) else {
                    return Vec::new();
                };
                let CheckedNominalKind::Enum { variants } = &nominal.kind else {
                    return Vec::new();
                };
                let Some(selected) = variants.get(*variant as usize) else {
                    return Vec::new();
                };
                let tag = selected.tag;
                (fields, Some(tag))
            }
            _ => return Vec::new(),
        };
        let mut carried = Vec::new();
        for (ordinal, field) in fields.iter().enumerate() {
            let Some(source) = self.input.placement_source_place(field) else {
                continue;
            };
            let ordinal = u32::try_from(ordinal).unwrap_or(u32::MAX);
            if let Some(carry) = self.mint_measure_datums(
                node_path,
                ordinal,
                MeasurePlacement::Construct,
                source,
                field.ty(),
                state,
            ) {
                let destination =
                    payload.map_or(PlaceStep::Field(ordinal), |variant| PlaceStep::Payload {
                        variant,
                        field: ordinal,
                    });
                carried.push((destination, carry));
            }
        }
        carried
    }

    /// [MSR-3] the construct placement's second half: after the statement's
    /// own kills, field i of the constructed value has the measures its
    /// operand had.
    pub(super) fn establish_construct_placements(
        &mut self,
        node_path: &crate::NodePath,
        base: &ResolvedPlace,
        carried: &[(PlaceStep, MeasureCarry)],
        state: &mut FactState,
    ) {
        for (step, carry) in carried {
            let mut destination = base.clone();
            destination.path.push(*step);
            let destination = destination;
            self.establish_measure_datums(node_path, destination, carry, state);
        }
    }

    /// [MSR-3] the payload placement: the datums a `match` over an own enum
    /// place carries out of that place's payload.
    ///
    /// The `match` consumes the scrutinee, so a measure of the payload dies
    /// with it; the datum minted here is the value that measure had
    /// immediately before the consume, and the arm binder that names the
    /// payload receives it on its own arm.
    pub(super) fn mint_payload_placements(
        &mut self,
        scrutinee: &CheckedExpression,
        enum_type: CheckedEnumType,
        state: &mut ProofFlowState,
    ) -> Vec<PayloadPlacement> {
        let Some(base) = self.input.placement_source_place(scrutinee) else {
            return Vec::new();
        };
        let Some(node_path) = scrutinee.carrier() else {
            return Vec::new();
        };
        let CheckedEnumType::Nominal(nominal) = enum_type else {
            return Vec::new();
        };
        let Some(nominal) = self.input.context.nominals.get(nominal.0 as usize) else {
            return Vec::new();
        };
        let CheckedNominalKind::Enum { variants } = &nominal.kind else {
            return Vec::new();
        };
        // Clone declaration data before minting terms through `self`.
        let variants = variants.clone();
        let mut placements = Vec::new();
        // [MSR-3] `ordinal within that statement` counts payload binders
        // across the whole match, not separately inside each variant. Two
        // variants' field zero are different naming events and must mint
        // different immutable datums.
        let mut placement_ordinal = 0u32;
        for variant in variants {
            let mut carried = Vec::new();
            for (ordinal, field) in variant.fields.iter().enumerate() {
                let field_ordinal = u32::try_from(ordinal).unwrap_or(u32::MAX);
                let mut source = base.clone();
                source.path.push(PlaceStep::Payload {
                    variant: variant.tag,
                    field: field_ordinal,
                });
                if let Some(carry) = self.mint_measure_datums(
                    node_path,
                    placement_ordinal,
                    MeasurePlacement::Payload,
                    source,
                    field.ty,
                    state,
                ) {
                    carried.push((field_ordinal, carry));
                }
                placement_ordinal = placement_ordinal
                    .checked_add(1)
                    .expect("payload placement ordinal exceeds u32");
            }
            if !carried.is_empty() {
                placements.push(PayloadPlacement {
                    tag: variant.tag,
                    carried,
                });
            }
        }
        placements
    }

    /// [MSR-3] the destructuring placement: the datums a destructuring
    /// consume carries out of the fields it takes apart.
    ///
    /// The operand is a bare use of a measured nominal place — `let N(f: a)
    /// = move v;` — and binder i takes the measures of `v`'s field i. A
    /// `let (a, b) = f(...)` binder list has no such operand and mints
    /// nothing; its ordinals are [CALL-4] destinations instead.
    pub(super) fn mint_destructuring_placements(
        &mut self,
        node_path: &crate::NodePath,
        bindings: &[(BindingId, CheckedType, u32)],
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> Vec<(u32, MeasureCarry)> {
        let Some(base) = self.input.placement_source_place(value) else {
            return Vec::new();
        };
        let mut carried = Vec::new();
        // [MSR-3] the destructuring placement's source is the field the
        // binder names, which the rest marker makes a written ordinal rather
        // than the binder's own position.
        for (position, (_, ty, field)) in bindings.iter().enumerate() {
            let ordinal = u32::try_from(position).unwrap_or(u32::MAX);
            let mut source = base.clone();
            source.path.push(PlaceStep::Field(*field));
            if let Some(carry) = self.mint_measure_datums(
                node_path,
                ordinal,
                MeasurePlacement::Destructuring,
                source,
                *ty,
                state,
            ) {
                carried.push((ordinal, carry));
            }
        }
        carried
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn retain_counted_derivations(
        &mut self,
        occurrence: u32,
        counted: CountedDerivationSet,
    ) {
        assert_eq!(
            occurrence, self.vocabulary.completed_counted_roots,
            "counted S11 groups must complete in statement-walk order"
        );
        let atoms = [
            (
                CountedRootAtom::LowerCaptureToEndpoint,
                counted.lower_capture_eq_endpoint.forward.parent,
            ),
            (
                CountedRootAtom::LowerEndpointToCapture,
                counted.lower_capture_eq_endpoint.reverse.parent,
            ),
            (
                CountedRootAtom::UpperCaptureToEndpoint,
                counted.upper_capture_eq_endpoint.forward.parent,
            ),
            (
                CountedRootAtom::UpperEndpointToCapture,
                counted.upper_capture_eq_endpoint.reverse.parent,
            ),
            (
                CountedRootAtom::BinderToLowerCapture,
                counted.binder_eq_lower_capture.forward.parent,
            ),
            (
                CountedRootAtom::LowerCaptureToBinder,
                counted.binder_eq_lower_capture.reverse.parent,
            ),
            (
                CountedRootAtom::LowerCaptureLeBinder,
                counted.lower_capture_le_binder.atomic.parent,
            ),
            (
                CountedRootAtom::BinderLtUpperCapture,
                counted.binder_lt_upper_capture.atomic.parent,
            ),
        ];
        for (atom, parent) in atoms {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::CountedS11 { occurrence, atom }, parent);
        }
        self.output.counted_derivations.push(counted);
        self.vocabulary.completed_counted_roots = self
            .vocabulary
            .completed_counted_roots
            .checked_add(1)
            .expect("counted S11 root groups exceed the u32 identity space");
    }
}

impl Analyzer<'_, '_> {
    pub(super) fn value_delivery_image(
        &mut self,
        value: &CheckedExpression,
        source: &ProofFlowState,
        context: DeliveryImageContext<'_>,
    ) -> ProofFlowState {
        let Some((carrier_binding, carrier, fragment)) = self
            .vocabulary
            .eligible_delivery_terms(value, context.receiver_type)
        else {
            return ProofFlowState::default();
        };
        let receiver = self.vocabulary.terms.intern(TermKind::Place(
            ResolvedPlace::spelled(
                PlaceRoot::Binding(context.receiver_binding),
                false,
                Vec::new(),
            ),
            fragment,
        ));
        // Every edge explicitly withholds the fresh receiver, including
        // edges visited after an earlier give interned the same stable term.
        // No implicit fact on x may participate in selecting d -> x.
        let facts = close_excluding_term(
            &source.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
            receiver,
        );
        let event = self
            .vocabulary
            .proof_event(FlowEventKind::PostconditionGive, Some(context.statement));
        let edge = DeliveryEdgeContext {
            statement: context.statement,
            carrier_binding,
            receiver_binding: context.receiver_binding,
            carrier,
            receiver,
            event,
        };
        let mut delivered = self.vocabulary.delivery_edge_state(facts, &edge);
        if delivered.may_hold_postcondition_candidates() {
            let mut ordinary = source.facts.clone();
            ordinary.retain_non_postcondition_candidates(&self.vocabulary.derivations);
            let ordinary = close_excluding_term(
                &ordinary,
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
                receiver,
            );
            let fallback = self.vocabulary.delivery_edge_state(ordinary, &edge);
            delivered.merge_relation_candidates_from(&fallback, &self.vocabulary.derivations);
        }
        let mut image = ProofFlowState {
            facts: delivered,
            results: BTreeMap::new(),
            entry_images: Vec::new(),
            separations: source.separations.clone(),
            // Delivery-image construction currently exists only to retain
            // postcondition relations.  Withholding an affine image is
            // conservative; the normal value-initializer join installs the
            // receiver's value separately.
            affine: AffineFlowState::default(),
            written: source.written.clone(),
            continuing: Vec::new(),
        };
        // The forward substitution happens above before the ordinary edge
        // kills, so the carrier's own branch scope cannot delete the image.
        self.kill_scopes_to(&mut image, context.scope_depth);
        self.exit_counted_loops_from(&mut image, context.loop_depth);
        image
    }

    /// Records what one admitted exact multiplication's bound value equals.
    ///
    /// The domain judgment already measured the operands where the product was
    /// formed; this pairs that measurement with the atom the binding took, so
    /// [PRF-1] can recognize `n*p` in a certificate sum as the value `base`
    /// already holds. A product whose result image is not one atom — a
    /// conversion, a further operation — records nothing, because there is
    /// then no single value the monomial equals.
    pub(super) fn record_product_atom(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) {
        let CheckedExpression::IntegerOperation {
            carrier, arguments, ..
        } = value
        else {
            return;
        };
        if !self.frames.product_operands.contains_key(carrier) {
            return;
        }
        let [left, right] = arguments.as_slice() else {
            return;
        };
        let Some(product) = state.values.get(&binding).and_then(AffineForm::unit_term) else {
            return;
        };
        // Constant-scaled products already have a transparent affine image
        // and need no opaque operand handles for a nonlinear certificate fold.
        let nonconstant =
            |image: Option<AffineForm>| image.is_some_and(|image| !image.terms().is_empty());
        if !nonconstant(self.reasoning().affine_pre_domain_form(left, state))
            || !nonconstant(self.reasoning().affine_pre_domain_form(right, state))
        {
            return;
        }
        // What proved the domain and what the fold names are two questions.
        // The domain judgment reads the transparent images, whose intervals are
        // what admit the multiply at all; the record names the bindings, so a
        // certificate scaling by one of them meets the same value here. Reading
        // handles at the domain site instead was tried and costs the interval:
        // an opaque operand is only bounded by its type, and the four endpoint
        // products then leave the range.
        let (Some(left), Some(right)) = (
            self.reasoning().affine_operand_handle(left, state),
            self.reasoning().affine_operand_handle(right, state),
        ) else {
            return;
        };
        self.vocabulary
            .product_atoms
            .insert(product, (left.min(right), left.max(right)));
    }

    /// [ENT-3.S7] A checked product of a captured unsigned quotient and
    /// divisor is no greater than the captured dividend. Matching reads
    /// immutable value images, so a source binding's later assignment cannot
    /// retarget the relation. Its own domain was proved before this transfer.
    pub(super) fn establish_unsigned_division_product(
        &mut self,
        product: &AffineForm,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) {
        let CheckedExpression::IntegerOperation {
            carrier, arguments, ..
        } = value
        else {
            return;
        };
        let Some(&(domain, ordinal)) = self.frames.product_operands.get(carrier) else {
            return;
        };
        let [left, right] = arguments.as_slice() else {
            return;
        };
        let (Some(left), Some(right)) = (
            self.reasoning().affine_pre_domain_form(left, state),
            self.reasoning().affine_pre_domain_form(right, state),
        ) else {
            return;
        };
        let Some(division) = self.vocabulary.unsigned_divisions.iter().find(|division| {
            (division.quotient == left && division.divisor == right)
                || (division.quotient == right && division.divisor == left)
        }) else {
            return;
        };
        let Some(inequality) = affine_less_equal(product, &division.dividend) else {
            return;
        };
        let parent = self
            .vocabulary
            .derivations
            .intern(DerivationNode::UnsignedDivisionProduct {
                product: carrier.clone(),
                division: division.parent,
                domain,
            });
        self.vocabulary
            .derivations
            .add_root(DerivationRootKind::UnsignedDivisionProduct(ordinal), parent);
        state.facts.push(ActiveAffineFact {
            inequality,
            evidence: AffineFactEvidence::Derivation(parent),
            active_loops: Vec::new(),
        });
    }
}

pub(super) fn substitute_delivery_relation(
    relation: &Relation,
    carrier: TermId,
    receiver: TermId,
) -> Relation {
    let replace = |term| if term == carrier { receiver } else { term };
    match relation {
        Relation::Bound { left, right, bound } => Relation::Bound {
            left: replace(*left),
            right: replace(*right),
            bound: *bound,
        },
        Relation::Equal {
            left,
            right,
            difference,
        } => Relation::Equal {
            left: replace(*left),
            right: replace(*right),
            difference: *difference,
        },
        Relation::Distinct {
            left,
            right,
            difference,
        } => {
            let (left, right) = (replace(*left), replace(*right));
            // Ordering the pair reverses the difference with it.
            if left <= right {
                Relation::Distinct {
                    left,
                    right,
                    difference: *difference,
                }
            } else {
                Relation::Distinct {
                    left: right,
                    right: left,
                    difference: -difference,
                }
            }
        }
    }
}
