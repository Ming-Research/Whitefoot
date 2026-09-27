//! [REF-1, REF-2, REF-3, REF-4] references: the checker side.
//!
//! A reference is a local name for a path [REF-1]. It is not storage of its
//! own, it carries no permission marker, no region and no loan, and it never
//! escapes the function that formed it [REF-3]. What the checker therefore
//! has to carry for a reference binding is exactly three things: the set of
//! paths it names, whether it is a `&T` or a `&[T]` [REF-4], and whether it
//! is still valid [REF-2].
//!
//! The validity fact is threaded through the statement walk rather than
//! recomputed on demand, because [REF-2]'s closing sentence puts it outside
//! the entailment fragment [ENT-1] and its invalidation set is exactly
//! enumerated: a proper prefix of the path is written, or the path or a
//! prefix is moved out of or released, by a statement, by a call [EFF-5], or
//! by a compiler-derived release at scope exit [STOR-3]; the scope of the
//! local variable the path starts at ends; or a window operation removes its
//! selected slot. An already selected payload survives its match's exit;
//! replacing its enum still invalidates it. Content writes preserve their
//! captured target, while unknown-depth covers conservatively invalidate
//! other potentially destroyed targets. Primitive leaf stores preserve
//! pointers, never the old contents' proof facts.
//!
//! Every path question — prefix, overlap, index and range separation — is
//! [OWN-7]'s, asked through [`super::super::places`] and answered nowhere
//! else.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, FixedTerminal, LexicalUseRole, Production,
    ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
    UnsupportedSemanticFeature,
};

use super::super::model::{
    BindingId, CheckedCallSeparation, CheckedContainerRoot, CheckedEffectStep, CheckedExpression,
    CheckedLoopId, CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedPlaceStep,
    CheckedRangeRoot, CheckedRangeSource, CheckedStatePath, CheckedTargetDomainObligation,
    CheckedType, IntegerType, WindowShape,
};
use super::super::places::{
    CapturedRange, CapturedTerm, CapturedValue, DescendantTarget, PlaceRoot, PlaceStep,
    ResolvedPlace, UnprovedSeparations,
};
use super::{
    CheckStop, Checker, EffectPath, FunctionSignature, LocalBinding, PlaceAccess, TypedExpression,
};

// [DIAG-1] same-node judgment order at a reference use: OWN-1's liveness and
// spelling judgments are asked before REF-2's validity judgment, and REF-1
// fixes the path a reference names before REF-2 judges that path's validity,
// because each earlier-defined rule cites first at one offending use.
//
// This replaces the v0.59 ordering over OWN-5, OWN-6, OWN-10 and OWN-14,
// whose subject was the loan-and-reborrow apparatus; none of those four rules
// exists in the active specification, so the pinned order had nothing left to
// pin. The successor ordering is the one a reference use asks in.
const _: () = {
    assert!(SemanticRule::Own1.definition_rank() < SemanticRule::Ref1.definition_rank());
    assert!(SemanticRule::Ref1.definition_rank() < SemanticRule::Ref2.definition_rank());
    assert!(SemanticRule::Ref2.definition_rank() < SemanticRule::Ref3.definition_rank());
    assert!(SemanticRule::Ref3.definition_rank() < SemanticRule::Ref4.definition_rank());
};

/// [REF-1]'s restructuring for a `&` over a reference variable.
pub(super) const REF1_NAME_THE_PATH: &str = "name the path the reference names";

/// [REF-2]'s restructuring for a use of an invalidated reference.
pub(super) const REF2_FORM_AGAIN: &str = "form the reference again after that event";

/// [REF-3]'s restructuring for an escaping reference.
pub(super) const REF3_RETURN_AN_INDEX: &str =
    "return an index and let the caller form the reference";

/// [REF-4]'s restructuring for a range reference over a `Ring`.
pub(super) const REF4_RING: &str = "a ring hands out single slots; take the elements one at a time";

/// [WIN-3]'s restructuring for a move out of a window slot or array element.
pub(super) const WIN3_NO_TAKE: &str = "use take_back, remove_at, or swap [OP-10, OP-11]";

/// [OWN-1]'s restructuring for a `move` of a place reached through a `deref`.
pub(super) const OWN1_ROOTED_CONSUME: &str =
    "consume a place rooted in a live own-mode binding of this function";

/// Which reference kind a binding names [GRAM-3, REF-4].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReferenceKind {
    /// `&T`: one place.
    Single,
    /// `&[T]`: a range of elements, whose one measure is `len` [REF-4].
    Range,
}

/// One validity variable at a loop header, owned by the reference binding
/// whose preheader and backedge states it joins.
///
/// The ids are function-local, but every dependency is eliminated before the
/// checked function is published. Keeping the owner in the identity makes
/// aliases and mutually assigned references a finite dependency graph rather
/// than accidentally one boolean for the whole loop.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct LoopReferenceToken {
    pub(super) loop_id: CheckedLoopId,
    pub(super) owner: BindingId,
}

/// One reference use whose validity depends on an arbitrary loop header.
/// The checker retains it only until the owning loop closes and resolves all
/// of its header variables.
#[derive(Clone, Debug)]
pub(super) struct DeferredLoopReferenceUse {
    pub(super) node: NodeId,
    pub(super) declaration: DeclarationId,
    pub(super) dependencies: Vec<LoopReferenceToken>,
    pub(super) preservations: Vec<CheckedCallSeparation>,
}

/// The exactly enumerated events [REF-2] admits as invalidating, carried into
/// the diagnostic so the rejection names the event rather than only the use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum InvalidationEvent {
    /// A proper prefix of the path was written by a statement [SET-1].
    PrefixWritten,
    /// The path or a prefix of it was moved out of [OWN-1, WIN-3].
    PrefixMoved,
    /// A call's substituted row writes a proper prefix of the path
    /// [EFF-5 clause 3].
    CallWrite,
    /// The scope of the local variable the path starts at ended.
    RootScopeEnded,
    /// A window operation moved the boundary or the logical origin the
    /// reference's bound was formed under [OP-10, REF-4].
    ///
    /// This is not [EFF-5]'s prefix rule: `take_back` writes `r.len`, which
    /// is no prefix of `r[i]`, and yet [OP-10] states that such a reference
    /// dies there, because the bound `i < r.len` its formation established no
    /// longer holds. `place_back`'s `ensures` carries that bound across the
    /// call and `insert_at`'s occupant change leaves the slot itself, so
    /// neither is one of these events.
    WindowBoundaryMoved,
}

impl InvalidationEvent {
    /// The exact event phrase the [REF-2] diagnostic carries.
    pub(super) const fn phrase(&self) -> &'static str {
        match self {
            Self::PrefixWritten => "a proper prefix of the reference's path was written",
            Self::PrefixMoved => "the reference's path or a prefix of it was moved out of",
            Self::CallWrite => "a call wrote a proper prefix of the reference's path",
            Self::RootScopeEnded => "the scope of the local variable the path starts at ended",
            Self::WindowBoundaryMoved => {
                "a window operation moved the boundary or origin the reference was formed under"
            }
        }
    }
}

/// [REF-2] "p is valid" is a fact like any other.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ReferenceValidity {
    Valid,
    /// Re-established only by forming the reference again.
    Invalid(InvalidationEvent),
}

/// What one reference binding names [REF-1].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReferenceInfo {
    pub(super) kind: ReferenceKind,
    /// The path set. It has more than one member only after a control-flow
    /// join, where [REF-1] takes the union of the incoming edges' sets and
    /// every check must hold for every member.
    pub(super) paths: Vec<ResolvedPlace>,
    pub(super) validity: ReferenceValidity,
    /// Header validity variables this reference still depends on. A freshly
    /// formed reference has none. Copying a reference name retains the
    /// dependencies along with its path; forming a new reference through an
    /// old one first checks the old dependencies and then creates a fresh
    /// valid reference.
    loop_dependencies: Vec<LoopReferenceToken>,
    /// Conjunctive event-site questions needed to preserve this reference.
    /// They become obligations only when the reference is used.
    pub(super) preservations: Vec<CheckedCallSeparation>,
}

impl ReferenceInfo {
    /// One freshly formed reference naming one path.
    pub(super) fn formed(kind: ReferenceKind, path: ResolvedPlace) -> Self {
        Self::formed_paths(kind, vec![path])
    }

    /// One freshly formed reference naming every path a joined base may
    /// select at run time [REF-1]. No member is representative: validity,
    /// overlap and effect judgments must retain the complete union.
    fn formed_paths(kind: ReferenceKind, paths: Vec<ResolvedPlace>) -> Self {
        Self {
            kind,
            paths,
            validity: ReferenceValidity::Valid,
            loop_dependencies: Vec::new(),
            preservations: Vec::new(),
        }
    }

    pub(super) const fn is_valid(&self) -> bool {
        matches!(self.validity, ReferenceValidity::Valid)
    }

    /// [REF-1] one further step below every path this reference names, which
    /// is how a payload binder names the scrutinee path extended by its own
    /// payload step [OWN-13].
    ///
    /// The selection is checked under the current refinement [ENT-3.S15];
    /// its existing place does not depend on that fact's lexical extent.
    pub(super) fn extend(&mut self, step: PlaceStep) {
        for path in &mut self.paths {
            path.path.push(step);
        }
    }

    /// [REF-2] invalidation is a fact about the whole reference: a path set
    /// with one invalidated member is invalid, because every check on the
    /// binding must hold for every member of the set [REF-1].
    pub(super) fn invalidate(&mut self, event: InvalidationEvent) {
        if self.is_valid() {
            self.validity = ReferenceValidity::Invalid(event);
        }
    }

    /// [REF-1] the join of two incoming edges: the union of the path sets,
    /// and the meet of the validity facts, because a check must hold on every
    /// incoming edge.
    ///
    /// An index one edge superseded is superseded after the join too, so the
    /// same capture on both edges stays one member rather than two.
    pub(super) fn join(&mut self, other: &Self) {
        for path in &other.paths {
            if !self.paths.contains(path) {
                self.paths.push(path.clone());
            }
        }
        let superseded = self
            .paths
            .iter()
            .flat_map(ResolvedPlace::superseded_indices)
            .collect::<Vec<_>>();
        if !superseded.is_empty() {
            let mut joined = Vec::with_capacity(self.paths.len());
            for mut path in std::mem::take(&mut self.paths) {
                for (capture, binding) in &superseded {
                    path.supersede_capture(*capture, *binding);
                }
                if !joined.contains(&path) {
                    joined.push(path);
                }
            }
            self.paths = joined;
        }
        if let ReferenceValidity::Invalid(event) = &other.validity {
            self.invalidate(event.clone());
        }
        for preservation in &other.preservations {
            if !self.preservations.contains(preservation) {
                self.preservations.push(preservation.clone());
            }
        }
        for dependency in &other.loop_dependencies {
            if !self.loop_dependencies.contains(dependency) {
                self.loop_dependencies.push(*dependency);
            }
        }
    }

    /// Marks this outer reference as one of a loop header's incoming values.
    /// Only a holder whose source body may rebind it receives generalized
    /// captures; every other holder retains its exact path precision.
    pub(super) fn enter_loop_header(
        &mut self,
        token: LoopReferenceToken,
        widen_captures: bool,
    ) -> Result<Option<Vec<ResolvedPlace>>, SemanticCompilerFailure> {
        if !self.loop_dependencies.contains(&token) {
            self.loop_dependencies.push(token);
        }
        if !widen_captures {
            return Ok(None);
        }
        let mut widened = Vec::with_capacity(self.paths.len());
        for (ordinal, path) in self.paths.iter().enumerate() {
            widened.push(path.loop_carried(
                token.loop_id,
                token.owner,
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            )?);
        }
        self.paths.clone_from(&widened);
        Ok(Some(widened))
    }

    pub(super) fn loop_dependencies(&self) -> &[LoopReferenceToken] {
        &self.loop_dependencies
    }

    /// Eliminates one completed loop's header variable from this reference.
    /// A failed dependency invalidates the reference with the event that made
    /// the arbitrary backedge invalid. A successful dependency is replaced
    /// by the still-open outer-loop variables its equation depends on.
    pub(super) fn resolve_loop_dependency(
        &mut self,
        token: LoopReferenceToken,
        resolution: Result<(&[LoopReferenceToken], &[CheckedCallSeparation]), &InvalidationEvent>,
    ) {
        if !self.loop_dependencies.contains(&token) {
            return;
        }
        self.loop_dependencies
            .retain(|candidate| *candidate != token);
        match resolution {
            Ok((dependencies, preservations)) => {
                for preservation in preservations {
                    if !self.preservations.contains(preservation) {
                        self.preservations.push(preservation.clone());
                    }
                }
                for dependency in dependencies {
                    if !self.loop_dependencies.contains(dependency) {
                        self.loop_dependencies.push(*dependency);
                    }
                }
            }
            Err(event) => self.invalidate(event.clone()),
        }
    }

    /// Whether the alternatives still belong to the entering static shapes,
    /// before unknown-depth widening becomes necessary [REF-1].
    ///
    /// Two shapes agree when their roots agree and their step kinds agree
    /// step for step, ignoring captured index and endpoint values.
    #[cfg(test)]
    pub(super) fn shape_agrees_with(&self, other: &Self) -> bool {
        self.paths.iter().all(|left| {
            other
                .paths
                .iter()
                .any(|right| path_shapes_agree(left, right))
        })
    }
}

/// Whether two resolved paths have one static shape [REF-1].
fn path_shapes_agree(left: &ResolvedPlace, right: &ResolvedPlace) -> bool {
    left.root == right.root
        && left.path.len() == right.path.len()
        && left
            .path
            .iter()
            .zip(&right.path)
            .all(|(left, right)| step_shapes_agree(*left, *right))
}

const fn step_shapes_agree(left: PlaceStep, right: PlaceStep) -> bool {
    match (left, right) {
        (PlaceStep::Descendant(left), PlaceStep::Descendant(right)) => {
            left.loop_id.0 == right.loop_id.0 && left.holder.0 == right.holder.0
        }
        (PlaceStep::Field(left), PlaceStep::Field(right)) => left == right,
        (PlaceStep::Deref, PlaceStep::Deref) => true,
        (
            PlaceStep::Payload {
                variant: left_variant,
                field: left_field,
            },
            PlaceStep::Payload {
                variant: right_variant,
                field: right_field,
            },
        ) => left_variant == right_variant && left_field == right_field,
        // An index or a range step is exactly what a loop-carried rebinding
        // may move, so two of them agree in shape whatever they captured.
        (PlaceStep::Index(_), PlaceStep::Index(_)) | (PlaceStep::Range(_), PlaceStep::Range(_)) => {
            true
        }
        (PlaceStep::Part(left), PlaceStep::Part(right)) => matches!(
            (left, right),
            (
                super::super::places::WindowPart::Next,
                super::super::places::WindowPart::Next
            ) | (
                super::super::places::WindowPart::Last,
                super::super::places::WindowPart::Last
            ) | (
                super::super::places::WindowPart::Filled,
                super::super::places::WindowPart::Filled
            ) | (
                super::super::places::WindowPart::Free,
                super::super::places::WindowPart::Free
            )
        ),
        (PlaceStep::Measure(left), PlaceStep::Measure(right)) => left as u8 == right as u8,
        _ => false,
    }
}

/// The value a position requires of its operand [TYPE-5, TYPE-7].
///
/// A reference is read bare [TYPE-7], so a position that needs the referent
/// states what it needs and takes the path the reference names; there is no
/// read-through operation to insert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RequiredReferent {
    /// A position requires one exact value type, as counted endpoints require
    /// `own u64` [TYPE-5].
    Exact(CheckedType),
    /// A `match` scrutinee requires an enum value [OWN-13, ERR-2].
    Enum,
    /// An index root requires directly indexable storage [OP-4].
    IndexableStorage,
}

impl<'unit, 'classified, 'lexed, 'source> Checker<'unit, 'classified, 'lexed, 'source> {
    /// Retains the structural walk's already-resolved [REF-1] paths without
    /// interpreting their roots again. Recording at formation and rebinding
    /// sites also retains origins of invalidated and out-of-scope holders;
    /// validity and point-current targets remain the ordinary walk's facts.
    pub(super) fn record_reference_origins(&self, binding: BindingId, paths: &[ResolvedPlace]) {
        let mut origins = self.reference_origins.borrow_mut();
        let index = binding.0 as usize;
        if origins.len() <= index {
            origins.resize_with(index + 1, Vec::new);
        }
        for path in paths {
            if !origins[index].contains(path) {
                origins[index].push(path.clone());
            }
        }
    }

    /// [REF-1] one root is added once; a differing shape becomes a cone
    /// whose finite anchor can only shorten. Repeated visits cannot unroll
    /// its unknown tail or mint additional captured identities.
    pub(super) fn join_loop_reference_summary(
        &self,
        token: LoopReferenceToken,
        ty: CheckedType,
        kind: ReferenceKind,
        paths: &[ResolvedPlace],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<bool, CheckStop> {
        let mut contributions = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            let readonly = self
                .readonly_member_on_resolved_path(path, bindings)?
                .is_some();
            let path = path.loop_carried(
                token.loop_id,
                token.owner,
                u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            )?;
            contributions.push((path, readonly));
        }
        let mut summaries = self.loop_reference_summaries.borrow_mut();
        let paths = match summaries.entry(token) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                // Preserve all static entry alternatives until a contribution
                // leaves those shapes. Merely entering a loop must not lose
                // a previously established sibling-field separation.
                entry.insert(contributions.into_iter().map(|(path, _)| path).collect());
                return Ok(true);
            }
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
        };
        let mut changed = false;
        for (incoming, incoming_readonly) in contributions {
            if !incoming.has_descendant()
                && paths.iter().any(|current| {
                    !current.has_descendant() && path_shapes_agree(current, &incoming)
                })
            {
                continue;
            }
            let same_root = paths
                .iter()
                .filter(|path| path.root == incoming.root)
                .collect::<Vec<_>>();
            if same_root.is_empty() {
                paths.push(incoming);
                changed = true;
                continue;
            }
            let mut readonly = incoming_readonly;
            let mut prefix = incoming.cover_prefix().to_vec();
            for current in &same_root {
                readonly |= self
                    .readonly_member_on_resolved_path(current, bindings)?
                    .is_some();
                let shared = prefix
                    .iter()
                    .zip(current.cover_prefix())
                    .take_while(|(left, right)| left == right)
                    .count();
                prefix.truncate(shared);
            }
            let mut joined = ResolvedPlace {
                root: incoming.root,
                path: prefix,
            };
            joined.path.push(PlaceStep::Descendant(DescendantTarget {
                loop_id: token.loop_id,
                holder: token.owner,
                ty,
                range: kind == ReferenceKind::Range,
                readonly,
            }));
            if same_root.as_slice() != [&joined] {
                let index = paths
                    .iter()
                    .position(|path| path.root == incoming.root)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                paths.retain(|path| path.root != incoming.root);
                paths.insert(index, joined);
                changed = true;
            }
        }
        Ok(changed)
    }
    /// [GRAM-3] the parameter kind follows its written prefix, independently
    /// of type substitution: `T`, `&T`, or `&[T]`.
    pub(super) fn parse_parameter_mode(&self, node: NodeId) -> Result<CheckedMode, CheckStop> {
        if !self.has_fixed(node, crate::FixedTerminal::Ampersand)? {
            return Ok(CheckedMode::Own);
        }
        if self.has_fixed(node, crate::FixedTerminal::LeftBracket)? {
            return Ok(CheckedMode::Range);
        }
        Ok(CheckedMode::Reference)
    }

    /// [REF-1] the path set a spelled place root names, read through every
    /// reference variable in the chain.
    ///
    /// Resolving a place rooted at a reference variable replaces that root
    /// with the path that reference names, recursively, and every [OWN-7]
    /// judgment reads the result.
    pub(super) fn resolve_reference_root(
        &self,
        root: DeclarationId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Vec<ResolvedPlace>, CheckStop> {
        let Some(local) = bindings.get(&root) else {
            // A named const [CONST-2] is storage of its own.
            let constant = *self
                .constants
                .get(&root)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            return Ok(vec![ResolvedPlace {
                root: PlaceRoot::Constant(constant),
                path: Vec::new(),
            }]);
        };
        let Some(reference) = &local.reference else {
            return Ok(vec![ResolvedPlace::binding(local.binding)]);
        };
        // A parameter's reference arrives carrying the caller's path by
        // substitution [EFF-5]; inside this body the parameter name *is* the
        // path, so its set anchors at itself and the recursion terminates
        // there. Every other reference's set was resolved when it was formed.
        Ok(reference.paths.clone())
    }

    /// [REF-2] the validity judgment at one use of a reference binding.
    pub(super) fn check_reference_valid(
        &self,
        local: &LocalBinding,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        let Some(reference) = &local.reference else {
            return Ok(());
        };
        match &reference.validity {
            ReferenceValidity::Invalid(event) => self.issue_node(
                SemanticRule::Ref2,
                node,
                SemanticIssueKind::InvalidReferenceUse {
                    binder: self.declaration_spelling(local.declaration)?,
                    event: event.phrase(),
                    mechanical_fix: REF2_FORM_AGAIN,
                },
            ),
            ReferenceValidity::Valid if !reference.loop_dependencies().is_empty() => {
                // The one-pass checker does not yet know which arbitrary
                // backedge reaches this use. Retain the complete owner-tagged
                // dependency set; the owning loop solves it over its entry
                // and every executable backedge before publishing the checked
                // function. The function driver rejects any dependency that
                // survives its function-local loop-id namespace.
                self.deferred_loop_reference_uses
                    .borrow_mut()
                    .push(DeferredLoopReferenceUse {
                        node,
                        declaration: local.declaration,
                        dependencies: reference.loop_dependencies().to_vec(),
                        preservations: reference.preservations.clone(),
                    });
                Ok(())
            }
            ReferenceValidity::Valid => self.demand_reference_preservations(
                node,
                local.declaration,
                &reference.preservations,
            ),
        }
    }

    pub(super) fn demand_reference_preservations(
        &self,
        node: NodeId,
        declaration: DeclarationId,
        preservations: &[CheckedCallSeparation],
    ) -> Result<(), CheckStop> {
        for preservation in preservations {
            let mut query = preservation.clone();
            let use_site = query
                .reference_use
                .as_mut()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            use_site.site = self.tree.path(node)?.clone();
            use_site.binder = self.declaration_spelling(declaration)?;
            let mut queries = self.call_separations.borrow_mut();
            if !queries.contains(&query) {
                queries.push(query);
            }
        }
        Ok(())
    }

    /// [REF-2] applies one access's invalidation to every live reference in
    /// scope.
    ///
    /// The prefix question is [OWN-7]'s, asked conservatively: two indexed
    /// positions on one storage are taken to overlap unless the shared
    /// relation separates them. Writing the storage at a reference's own
    /// path, or below it, is a content write and invalidates nothing. Moving
    /// or releasing the path itself removes the named value and therefore
    /// invalidates it as well as its descendants.
    pub(super) fn invalidate_references(
        &self,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        written: &ResolvedPlace,
        event: &InvalidationEvent,
    ) -> Result<(), CheckStop> {
        self.invalidate_references_with_separation(bindings, written, event, None)
    }

    /// The event site fixes the proof context. Preserving a bystander is an
    /// optional question until a later use demands this validity fact.
    pub(super) fn invalidate_references_with_separation(
        &self,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        written: &ResolvedPlace,
        event: &InvalidationEvent,
        site: Option<&crate::NodePath>,
    ) -> Result<(), CheckStop> {
        // [REF-1] a reference keeps the index value its formation read, so a
        // write of that binding leaves the reference valid but ends the
        // binding's spelling as a name for its index. [EFF-1] A parameter no
        // longer holds its call value.
        if written.path.is_empty()
            && let PlaceRoot::Binding(binding) = written.root
        {
            for reference in bindings
                .values_mut()
                .filter_map(|local| local.reference.as_mut())
            {
                for path in &mut reference.paths {
                    path.supersede_binding(binding);
                }
            }
            for local in bindings
                .values_mut()
                .filter(|local| local.binding == binding)
            {
                local.call_value = false;
            }
        }
        let include_equal = matches!(event, InvalidationEvent::PrefixMoved);
        let primitive_write = !include_equal
            && !matches!(
                written.path.last(),
                Some(PlaceStep::Measure(_) | PlaceStep::Part(_))
            )
            && matches!(
                self.resolved_place_type(written, bindings)?,
                Some(
                    CheckedType::Unit
                        | CheckedType::Bool
                        | CheckedType::Integer(_)
                        | CheckedType::Float(_)
                        | CheckedType::GenericInt(_)
                        | CheckedType::GenericFloat(_)
                )
            );
        let oracle = UnprovedSeparations;
        // A preserved bystander records its separation query once the walk
        // is over: the query carries both places in their source spelling,
        // and naming them reads the same bindings this walk updates.
        let mut preserved = Vec::new();
        for (declaration, local) in bindings.iter_mut() {
            // A write to the enum itself ends every refinement occurrence for
            // that enum even when an equal-path reference remains valid under
            // the proper-prefix write rule. A payload-field write is below
            // the witnessed enum and therefore does not end the fact.
            for witness in &mut local.refinement_witnesses {
                if written.may_be_prefix_of(&oracle, &witness.place, true) {
                    witness.valid = false;
                }
            }
            let Some(reference) = &mut local.reference else {
                continue;
            };
            if primitive_write || !reference.is_valid() {
                continue;
            }
            let mut invalidated = false;
            for path in &reference.paths {
                if !written.may_be_prefix_of(&oracle, path, include_equal) {
                    continue;
                }
                if let Some((site, (positions, window))) =
                    site.zip(Self::separable_by_position(written, path))
                {
                    preserved.push((*declaration, site, positions, window, path.clone()));
                } else {
                    invalidated = true;
                    break;
                }
            }
            if invalidated {
                reference.invalidate(event.clone());
            }
        }
        for (declaration, site, positions, window, path) in preserved {
            let query = CheckedCallSeparation {
                site: site.clone(),
                exchange: false,
                reference_use: Some(super::super::model::CheckedReferencePreservationUse {
                    site: site.clone(),
                    binder: String::new(),
                    event: event.phrase(),
                }),
                positions,
                window,
                left_spelling: self.render_resolved_place(written, bindings)?,
                right_spelling: self.render_resolved_place(&path, bindings)?,
                one_argument: false,
            };
            let Some(reference) = bindings
                .get_mut(&declaration)
                .and_then(|local| local.reference.as_mut())
            else {
                continue;
            };
            if !reference.preservations.contains(&query) {
                reference.preservations.push(query);
            }
        }
        Ok(())
    }

    /// [OP-10, REF-2] every reference into one window becomes invalid.
    ///
    /// A reference into a window is formed under a bound and stays valid
    /// while that bound holds: `&r[i]` under `i < r.len` and `&r[lo..hi]`
    /// under `hi <= r.len` [REF-4]. The operations that move the boundary
    /// down, move a run between two windows, shift every logical index, or
    /// remake the block whole end that bound, and the references formed under
    /// it die with it.
    pub(super) fn invalidate_window_references(
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        window: &ResolvedPlace,
    ) {
        for local in bindings.values_mut() {
            let Some(reference) = &mut local.reference else {
                continue;
            };
            // The subject is a reference *into* the window, formed under a
            // bound the operation ends: `&r[i]` under `i < r.len` and
            // `&r[lo..hi]` under `hi <= r.len` [REF-4]. A reference to the
            // window itself names the window whatever its boundary is, and a
            // callee that received one and moved that boundary still holds
            // it.
            if reference
                .paths
                .iter()
                .any(|path| window.may_be_prefix_of(&UnprovedSeparations, path, false))
            {
                reference.invalidate(InvalidationEvent::WindowBoundaryMoved);
            }
        }
    }

    /// [REF-2] the scope-exit half: every reference whose path starts at a
    /// local variable whose scope ends here becomes invalid, and so does
    /// every reference through a place that scope's compiler-derived release
    /// reaches [STOR-3].
    pub(super) fn invalidate_references_leaving_scope(
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        leaving: &[BindingId],
    ) {
        for local in bindings.values_mut() {
            let Some(reference) = &mut local.reference else {
                continue;
            };
            if reference.paths.iter().any(|path| match path.root {
                PlaceRoot::Binding(binding) => leaving.contains(&binding),
                PlaceRoot::Constant(_) => false,
            }) {
                reference.invalidate(InvalidationEvent::RootScopeEnded);
            }
        }
    }

    /// [REF-3] a reference never escapes: it may not be assigned into any
    /// aggregate, returned, or captured by a stored function value.
    pub(super) fn reject_escaping_reference(
        &self,
        value: &TypedExpression,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        if value.mode == CheckedMode::Own {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Ref3,
            node,
            SemanticIssueKind::EscapingReference {
                mechanical_fix: REF3_RETURN_AN_INDEX,
            },
        )
    }

    /// [REF-1] `&p` where `p` is a reference variable, and [REF-4] the whole
    /// `borrow_expr` judgment.
    ///
    /// `borrow_expr := "&" place` [GRAM-5]: there is no permission marker, no
    /// region, and no other qualifier, so the judgment is exactly the path
    /// resolution plus [REF-4]'s range-formation obligations.
    pub(super) fn check_borrow(
        &self,
        node: NodeId,
        function: &FunctionSignature,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let carrier = self
            .tree
            .parent(node)?
            .filter(|parent| self.tree.production(*parent) == Ok(Production::Atom))
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let place_node = self
            .tree
            .first_child_with(node, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let pbase = self
            .tree
            .first_child_with(place_node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // `deref` is an ordinary path step [TYPE-7], so a `borrow_expr` over
        // one is an ordinary formation; the root of the complete path is what
        // decides the judgment.
        //
        // [REF-1] a place that goes through a reference variable is written
        // under that step — `&deref(p)`, `&deref(p)[i]` — and resolving the
        // step replaces it with the path the reference names. The root of the
        // complete path is therefore that reference variable itself, which is
        // what the reference-root replacement below already reads, so the two
        // spellings reach one judgment and one representation.
        let written_deref = self.has_fixed(pbase, FixedTerminal::Deref)?;
        let pbase = if written_deref {
            let inner = self
                .tree
                .first_child_with(pbase, Production::Place)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if !self
                .tree
                .children_with(inner, Production::Psuffix)?
                .is_empty()
            {
                // A reference is never stored in an aggregate [REF-3], so the
                // only place a `deref` step names is a bare reference
                // variable; anything else is a form this walk cannot root.
                return self
                    .unsupported(UnsupportedSemanticFeature::ReferenceFormation, place_node);
            }
            self.tree
                .first_child_with(inner, Production::Pbase)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
        } else {
            pbase
        };
        let root_use = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let (root, root_type, root_binding) = match root_use.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::NamedConst,
            } => {
                let constant = *self
                    .constants
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                (
                    PlaceRoot::Constant(constant),
                    self.constant(constant)?.ty,
                    None,
                )
            }
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Value,
            } => {
                let local = bindings
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                // [REF-1] a reference variable is not storage of its own, so
                // `&r` where `r` is one has no path to name that `r` does not
                // already name.
                if local.reference.is_some()
                    && !written_deref
                    && self.tree.children(pbase)?.is_empty()
                {
                    let suffixes = self.tree.children_with(place_node, Production::Psuffix)?;
                    if suffixes.is_empty() {
                        return self.issue_node(
                            SemanticRule::Ref1,
                            node,
                            SemanticIssueKind::ReferenceToReferenceVariable {
                                mechanical_fix: REF1_NAME_THE_PATH,
                            },
                        );
                    }
                }
                if !local.live {
                    return self.issue_node(
                        SemanticRule::Own1,
                        place_node,
                        SemanticIssueKind::UseAfterMove {
                            mechanical_fix: "introduce a new `let` binding before reuse",
                        },
                    );
                }
                self.check_reference_valid(local, place_node)?;
                (
                    PlaceRoot::Binding(local.binding),
                    local.ty,
                    Some(local.clone()),
                )
            }
            _ => {
                return self
                    .unsupported(UnsupportedSemanticFeature::ReferenceFormation, place_node);
            }
        };
        let suffixes = self.tree.children_with(place_node, Production::Psuffix)?;
        // [REF-4] `&x[lo..hi]` forms a range reference. The range step is the
        // whole of the formation and selects no element place, so it is
        // resolved here rather than by the ordinary storage walk, and it is
        // the last written `psuffix`: a range reference is not storage
        // [TYPE-8], so nothing below it is written.
        if let Some(position) = self.range_suffix_position(&suffixes)? {
            if position + 1 != suffixes.len() {
                return self
                    .unsupported(UnsupportedSemanticFeature::ReferenceFormation, place_node);
            }
            return self.check_range_formation(
                carrier,
                place_node,
                suffixes[position],
                &suffixes[..position],
                root,
                root_type,
                root_binding.as_ref(),
                written_deref,
                function,
                bindings,
                loop_depth,
            );
        }
        // [REF-1, REF-4, OP-4] borrowing one element through a range names
        // that element place. The range's checked type is its element type,
        // so sending this suffix through the ordinary storage-path walk
        // would incorrectly ask whether T itself is an indexable base.
        if written_deref
            && root_binding
                .as_ref()
                .is_some_and(|local| local.mode == CheckedMode::Range)
            && let Some(first) = suffixes.first()
            && let Some(offset_node) = self.subscript_offset(*first)?
        {
            let local = root_binding
                .as_ref()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut probe = bindings.clone();
            let offset = self.check_atom(function, offset_node, &mut probe, loop_depth)?;
            if offset.expression.ty() != CheckedType::Integer(IntegerType::U64)
                || offset.mode != CheckedMode::Own
            {
                return self.issue_node(
                    SemanticRule::Type5,
                    offset_node,
                    SemanticIssueKind::type_mismatch(
                        "own u64",
                        self.checked_value_name(offset.mode, offset.expression.ty())?,
                    ),
                );
            }
            let captured = self.note_capture(
                Self::captured_of(offset_node, &offset.expression)
                    .unwrap_or(CapturedValue::unknown()),
                bindings,
            );
            let mut places = local
                .reference
                .as_ref()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .paths
                .clone();
            for place in &mut places {
                place.path.push(PlaceStep::Index(captured));
            }
            let (path, ty, carried) = self.resolve_storage_path(
                &suffixes[1..],
                local.ty,
                bindings,
                function,
                loop_depth,
                true,
            )?;
            for place in &mut places {
                place
                    .path
                    .extend(path.iter().map(CheckedPlaceStep::place_step));
            }
            let element = self.intern_element(local.ty)?;
            let expression = CheckedExpression::BorrowRangeIndex {
                carrier: self.tree.path(carrier)?.clone(),
                place: Box::new(crate::semantic::CheckedRangeElementPlace {
                    root: CheckedRangeRoot {
                        binding: local.binding,
                        element,
                        element_type: local.ty,
                    },
                    offset: offset.expression,
                    path,
                    ty,
                    obligation: self.tree.path(*first)?.clone(),
                    target_domain: CheckedTargetDomainObligation::ElementAddress,
                    captured,
                }),
            };
            let effects = offset.effects.union(carried.effects);
            let mut accesses = offset
                .accesses
                .into_iter()
                .map(PlaceAccess::operand)
                .collect::<Vec<_>>();
            accesses.extend(carried.accesses.into_iter().map(PlaceAccess::operand));
            accesses.extend(places.iter().cloned().map(|place| PlaceAccess {
                place,
                selected: true,
            }));
            return Ok(TypedExpression {
                expression,
                mode: CheckedMode::Reference,
                reference: Some(ReferenceInfo::formed_paths(ReferenceKind::Single, places)),
                reference_value: true,
                effects,
                accesses,
            });
        }
        let (path, ty, carried) =
            self.resolve_storage_path(&suffixes, root_type, bindings, function, loop_depth, true)?;
        let place = ResolvedPlace {
            root,
            path: path.iter().map(CheckedPlaceStep::place_step).collect(),
        };
        // [REF-1] the written steps continue the path the root names, so a
        // reference root is replaced by every path it may name before any
        // validity, overlap or effect judgment reads the formation.
        let places = if let Some(reference) = root_binding
            .as_ref()
            .and_then(|local| local.reference.as_ref())
        {
            reference
                .paths
                .iter()
                .cloned()
                .map(|mut resolved| {
                    resolved.path.extend(place.path.iter().copied());
                    resolved
                })
                .collect::<Vec<_>>()
        } else {
            vec![place]
        };
        let kind = ReferenceKind::Single;
        let expression = CheckedExpression::BorrowAddressed {
            carrier: self.tree.path(carrier)?.clone(),
            root: CheckedContainerRoot { root, path, ty },
        };
        let mut accesses = carried
            .accesses
            .into_iter()
            .map(PlaceAccess::operand)
            .collect::<Vec<_>>();
        accesses.extend(places.iter().cloned().map(|place| PlaceAccess {
            place,
            selected: true,
        }));
        Ok(TypedExpression {
            expression,
            mode: match kind {
                ReferenceKind::Single => CheckedMode::Reference,
                ReferenceKind::Range => CheckedMode::Range,
            },
            reference: Some(ReferenceInfo::formed_paths(kind, places)),
            reference_value: true,
            effects: carried.effects,
            accesses,
        })
    }

    /// The position of the one `psuffix` written as a range step, if any
    /// [GRAM-5, REF-4].
    fn range_suffix_position(&self, suffixes: &[NodeId]) -> Result<Option<usize>, CheckStop> {
        for (position, &suffix) in suffixes.iter().enumerate() {
            if self
                .tree
                .first_child_with(suffix, Production::RangeTail)?
                .is_some()
            {
                return Ok(Some(position));
            }
        }
        Ok(None)
    }

    /// [REF-4] `&x[lo..hi]`, over an indexable place or over another range
    /// reference, under `lo <= hi` and `hi <= x.len`.
    #[allow(clippy::too_many_arguments)]
    fn check_range_formation(
        &self,
        carrier: NodeId,
        place_node: NodeId,
        suffix: NodeId,
        base_suffixes: &[NodeId],
        root: PlaceRoot,
        root_type: CheckedType,
        root_binding: Option<&LocalBinding>,
        written_deref: bool,
        function: &FunctionSignature,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let start_node = self
            .subscript_offset(suffix)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let tail = self
            .tree
            .first_child_with(suffix, Production::RangeTail)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let end_node = self
            .tree
            .first_child_with(tail, Production::Atom)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [REF-4] re-slicing: the base is the run another range names, whose
        // element type the `deref` already selected [TYPE-7].
        let range_base =
            written_deref && root_binding.is_some_and(|local| local.mode == CheckedMode::Range);
        let mut carried = super::expressions::flat_storage::CarriedOperands::default();
        let (source, base_places, element_type) = if range_base {
            if !base_suffixes.is_empty() {
                return self
                    .unsupported(UnsupportedSemanticFeature::ReferenceFormation, place_node);
            }
            let local = root_binding.ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let element = self.intern_element(local.ty)?;
            let named = local
                .reference
                .as_ref()
                .map(|reference| reference.paths.clone())
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            (
                CheckedRangeSource::Range(CheckedRangeRoot {
                    binding: local.binding,
                    element,
                    element_type: local.ty,
                }),
                named,
                local.ty,
            )
        } else {
            let (path, ty, offsets) = self.resolve_storage_path(
                base_suffixes,
                root_type,
                bindings,
                function,
                loop_depth,
                true,
            )?;
            carried = offsets;
            // [REF-4] a range over a `Ring` is refused: a wrapped window is
            // two extents and `&[T]` has one `len`.
            if matches!(
                ty,
                CheckedType::Window {
                    shape: WindowShape::Ring,
                    ..
                }
            ) {
                return self.issue_node(
                    SemanticRule::Ref4,
                    suffix,
                    SemanticIssueKind::RangeOverRing {
                        mechanical_fix: REF4_RING,
                    },
                );
            }
            let element = match ty {
                CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                    self.element_type(element)?
                }
                // [OP-4] a runtime-capacity `Array<T>` is an indexable base
                // exactly as the constant-capacity one is [TYPE-9].
                CheckedType::Buffer { element } => self.element_type(element)?,
                _ => {
                    return self.issue_node(
                        SemanticRule::Op4,
                        suffix,
                        SemanticIssueKind::type_mismatch(
                            "an indexable base",
                            self.checked_type_name(ty)?,
                        ),
                    );
                }
            };
            let base = ResolvedPlace {
                root,
                path: path.iter().map(CheckedPlaceStep::place_step).collect(),
            };
            let bases =
                if let Some(reference) = root_binding.and_then(|local| local.reference.as_ref()) {
                    reference
                        .paths
                        .iter()
                        .cloned()
                        .map(|mut resolved| {
                            resolved.path.extend(base.path.iter().copied());
                            resolved
                        })
                        .collect()
                } else {
                    vec![base]
                };
            (
                CheckedRangeSource::Storage(CheckedContainerRoot { root, path, ty }),
                bases,
                element,
            )
        };
        let element = self.intern_element(element_type)?;
        let mut endpoints = Vec::with_capacity(2);
        for node in [start_node, end_node] {
            let mut probe = bindings.clone();
            let value = self.check_atom(function, node, &mut probe, loop_depth)?;
            if value.expression.ty()
                != CheckedType::Integer(crate::semantic::model::IntegerType::U64)
                || value.mode != CheckedMode::Own
            {
                return self.issue_node(
                    SemanticRule::Type5,
                    node,
                    SemanticIssueKind::type_mismatch(
                        "own u64",
                        self.checked_value_name(value.mode, value.expression.ty())?,
                    ),
                );
            }
            carried.effects = carried.effects.union(value.effects.clone());
            carried.accesses.extend(value.accesses.clone());
            endpoints.push(value);
        }
        let end = endpoints
            .pop()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let start = endpoints
            .pop()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // [REF-1, OWN-7] each endpoint keeps the occurrence that evaluated it,
        // whatever its form, so this formation's endpoint images are its own.
        let captured_start = self.note_capture(
            Checker::captured_endpoint_of(start_node, &start.expression)?,
            bindings,
        );
        let captured_end = self.note_capture(
            Checker::captured_endpoint_of(end_node, &end.expression)?,
            bindings,
        );
        // [OWN-7] the formed reference names the base path extended by its
        // own range step; every later separation question reads that step.
        let captured = CapturedRange {
            start: captured_start,
            end: captured_end,
        };
        let places = base_places
            .into_iter()
            .map(|mut place| {
                place.path.push(PlaceStep::Range(captured));
                place
            })
            .collect::<Vec<_>>();
        let expression = CheckedExpression::RangeOf {
            carrier: self.tree.path(carrier)?.clone(),
            source,
            element,
            element_type,
            start: Box::new(start.expression),
            end: Box::new(end.expression),
            obligation: self.tree.path(suffix)?.clone(),
            captured,
        };
        let mut accesses = carried
            .accesses
            .into_iter()
            .map(PlaceAccess::operand)
            .collect::<Vec<_>>();
        accesses.extend(places.iter().cloned().map(|place| PlaceAccess {
            place,
            selected: true,
        }));
        Ok(TypedExpression {
            expression,
            mode: CheckedMode::Range,
            reference: Some(ReferenceInfo::formed_paths(ReferenceKind::Range, places)),
            reference_value: true,
            effects: carried.effects,
            accesses,
        })
    }

    /// Converts a resolved runtime place into the static state path a
    /// callable boundary declares [EFF-1, EFF-2].
    ///
    /// The path is complete wherever [EFF-1] can express it: field
    /// selections, `deref`, payload steps, window parts and measures map one
    /// for one, and an index or range position maps only when the value it
    /// captured is a value parameter of the same callable, which is the only
    /// index spelling a signature admits. Anything else is a dynamic element
    /// and maps to its nearest statically nameable enclosing path, which is
    /// the prefix above the step [EFF-2].
    pub(super) fn state_path(
        &self,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedStatePath>, CheckStop> {
        let PlaceRoot::Binding(binding) = place.root else {
            // A const root contributes no effect [EFF-2, CONST-2].
            return Ok(None);
        };
        let Some(local) = bindings.values().find(|local| local.binding == binding) else {
            return Ok(None);
        };
        let mut steps = Vec::new();
        for step in &place.path {
            let mapped = match step {
                PlaceStep::Descendant(_) => break,
                PlaceStep::Field(field) => CheckedEffectStep::Field(*field),
                PlaceStep::Deref => CheckedEffectStep::Deref,
                PlaceStep::Payload { variant, field } => CheckedEffectStep::Payload {
                    variant: *variant,
                    field: *field,
                },
                PlaceStep::Part(part) => CheckedEffectStep::Part(*part),
                PlaceStep::Measure(measure) => CheckedEffectStep::Measure(*measure),
                PlaceStep::Index(captured) => {
                    let Some(parameter) = self.captured_parameter(*captured, bindings) else {
                        break;
                    };
                    CheckedEffectStep::Index(parameter)
                }
                PlaceStep::Range(range) => {
                    let (Some(start), Some(end)) = (
                        self.captured_parameter(range.start, bindings),
                        self.captured_parameter(range.end, bindings),
                    ) else {
                        break;
                    };
                    CheckedEffectStep::Range { start, end }
                }
            };
            steps.push(mapped);
        }
        Ok(Some(CheckedStatePath {
            root: local.declaration,
            steps,
        }))
    }

    /// The value parameter one captured index or endpoint read, when it read
    /// that parameter's call value [EFF-1]: a signature never contains an
    /// index expression, and a row evaluates its index parameter once at the
    /// call, so this is the only index a declared row admits.
    ///
    /// A capture read the call value when its formation recorded it so
    /// [`Self::note_capture`]. A spelled capture of a parameter that still
    /// holds its call value read it too, since no path to here wrote it.
    fn captured_parameter(
        &self,
        captured: CapturedValue,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Option<DeclarationId> {
        let local = match captured.term {
            CapturedTerm::Binding(binding) | CapturedTerm::Superseded(binding) => {
                bindings.values().find(|local| local.binding == binding)?
            }
            CapturedTerm::Literal(_) | CapturedTerm::Const(_) | CapturedTerm::Opaque => {
                return None;
            }
        };
        let holds_call_value =
            matches!(captured.term, CapturedTerm::Binding(_)) && local.call_value;
        if !holds_call_value
            && !self
                .call_value_captures
                .borrow()
                .contains(&captured.capture)
        {
            return None;
        }
        self.resolved
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.id() == local.declaration
                    && declaration.role() == DeclarationRole::Parameter
            })
            .map(|declaration| declaration.id())
    }

    /// [EFF-1] records a capture that reads a parameter while the parameter
    /// holds its call value on every path to the formation, which is exactly
    /// when the capture names the row's index parameter
    /// [`Self::captured_parameter`]. Every formation of an index or a range
    /// endpoint passes its capture through here; a later write of the
    /// parameter, or a join with an edge that wrote it, does not change what
    /// the capture read.
    pub(super) fn note_capture(
        &self,
        captured: CapturedValue,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> CapturedValue {
        if let CapturedTerm::Binding(binding) = captured.term
            && bindings
                .values()
                .any(|local| local.binding == binding && local.call_value)
        {
            self.call_value_captures
                .borrow_mut()
                .insert(captured.capture);
        }
        captured
    }

    /// [EFF-2] the enclosing formal-rooted effect of one resolved access.
    ///
    /// An access rooted only in local storage contributes no enclosing path,
    /// including through a local reference; the checked access and its
    /// ordinary footprint remain. A const root contributes no read.
    pub(super) fn effect_paths_for_place(
        &self,
        _node: NodeId,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Vec<EffectPath>, CheckStop> {
        let PlaceRoot::Binding(binding) = place.root else {
            return Ok(Vec::new());
        };
        let Some(local) = bindings.values().find(|local| local.binding == binding) else {
            return Ok(Vec::new());
        };
        // [EFF-1] every `effect_path` is rooted at one reference parameter of
        // the same callable; a by-value parameter has no effect entry at all.
        if local.mode == CheckedMode::Own {
            return Ok(Vec::new());
        }
        let is_parameter = self.resolved.declarations().iter().any(|declaration| {
            declaration.id() == local.declaration
                && declaration.role() == DeclarationRole::Parameter
        });
        if !is_parameter {
            return Ok(Vec::new());
        }
        Ok(self
            .state_path(place, bindings)?
            .into_iter()
            .map(EffectPath::from)
            .collect())
    }

    /// A measure reads the selected run's descriptor storage and no slot
    /// [MSR-2, WIN-2].
    pub(super) fn effect_paths_for_descriptor(
        &self,
        node: NodeId,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        measure: CheckedMeasure,
    ) -> Result<Vec<EffectPath>, CheckStop> {
        let mut descriptor = place.clone();
        descriptor.path.push(PlaceStep::Measure(measure));
        self.effect_paths_for_place(node, &descriptor, bindings)
    }

    /// [TYPE-7] whether this operand would be read through a reference or a
    /// cell to satisfy the position it stands in.
    ///
    /// There is no implicit read through a reference or through a cell, so a
    /// reference or `Box` binding written where a value of its referent type
    /// is expected is a hard error whose mechanical fix is `deref(.)`.
    pub(super) fn reads_implicitly_through_holder(
        &self,
        holds_reference: bool,
        ty: CheckedType,
        required: RequiredReferent,
    ) -> Result<bool, CheckStop> {
        if holds_reference {
            return self.satisfies_referent_requirement(ty, required);
        }
        let CheckedType::Nominal(nominal) = ty else {
            return Ok(false);
        };
        let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind else {
            return Ok(false);
        };
        self.satisfies_referent_requirement(referent, required)
    }

    pub(super) fn satisfies_referent_requirement(
        &self,
        ty: CheckedType,
        required: RequiredReferent,
    ) -> Result<bool, CheckStop> {
        Ok(match required {
            RequiredReferent::Exact(required) => ty == required,
            RequiredReferent::Enum => match ty {
                CheckedType::Bool => true,
                CheckedType::Nominal(nominal) => {
                    matches!(self.nominal(nominal)?.kind, CheckedNominalKind::Enum { .. })
                }
                _ => false,
            },
            // [OP-4, TYPE-9] a storage shape is an indexable base, so a shape
            // holder written where its referent is required is the same
            // [TYPE-7] missing `deref`.
            RequiredReferent::IndexableStorage => matches!(
                ty,
                CheckedType::Array { .. } | CheckedType::Buffer { .. } | CheckedType::Window { .. }
            ),
        })
    }

    /// [REF-2] the bindings one scope exit ends, which are the bindings of
    /// the inner state the enclosing state does not declare.
    pub(super) fn bindings_leaving_scope(
        inner: &HashMap<DeclarationId, LocalBinding>,
        enclosing: &[DeclarationId],
    ) -> Vec<BindingId> {
        inner
            .iter()
            .filter(|(declaration, _)| !enclosing.contains(declaration))
            .map(|(_, local)| local.binding)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{InvalidationEvent, ReferenceInfo, ReferenceKind, ReferenceValidity};
    use crate::semantic::model::BindingId;
    use crate::semantic::places::{PlaceStep, ResolvedPlace};

    /// This module is the only place [REF-2]'s validity lattice can be read
    /// directly: `ReferenceInfo` is `pub(super)` inside
    /// `crate::semantic::check`, so `crate::semantic::tests` cannot name it.
    /// The source-level half of the same judgment — which programs the
    /// invalidation events actually reject — is in
    /// `crate::semantic::tests::references`.
    fn reference(binding: u32) -> ReferenceInfo {
        ReferenceInfo::formed(
            ReferenceKind::Single,
            ResolvedPlace::binding(BindingId(binding)),
        )
    }

    /// [REF-2] a freshly formed reference is valid, and validity is
    /// re-established only by forming the reference again: a second
    /// invalidating event does not overwrite the first, because the
    /// diagnostic names the event that made the reference invalid.
    #[test]
    fn invalidation_is_sticky_and_keeps_the_first_event() {
        let mut info = reference(0);
        assert!(info.is_valid());
        info.invalidate(InvalidationEvent::PrefixWritten);
        assert_eq!(
            info.validity,
            ReferenceValidity::Invalid(InvalidationEvent::PrefixWritten)
        );
        info.invalidate(InvalidationEvent::RootScopeEnded);
        assert_eq!(
            info.validity,
            ReferenceValidity::Invalid(InvalidationEvent::PrefixWritten),
            "a second event must not relabel an already invalid reference"
        );
    }

    /// [REF-2] every remaining observable invalidating event, and no other, carries
    /// its own phrase into the diagnostic. The phrases are distinct, so a
    /// rejection names which event happened.
    #[test]
    fn every_invalidation_event_has_its_own_phrase() {
        let events = [
            InvalidationEvent::PrefixWritten,
            InvalidationEvent::PrefixMoved,
            InvalidationEvent::CallWrite,
            InvalidationEvent::RootScopeEnded,
            InvalidationEvent::WindowBoundaryMoved,
        ];
        let mut phrases: Vec<&'static str> = events.iter().map(InvalidationEvent::phrase).collect();
        let written = phrases.len();
        phrases.sort_unstable();
        phrases.dedup();
        assert_eq!(phrases.len(), written, "two events share one phrase");
    }

    /// [REF-1] the join of two incoming edges is the union of the path sets
    /// and the meet of the validity facts, because every check on the binding
    /// must hold on every incoming edge.
    #[test]
    fn a_join_unions_the_paths_and_meets_the_validity() {
        let mut left = reference(0);
        let right = reference(1);
        left.join(&right);
        assert_eq!(
            left.paths.len(),
            2,
            "the join is the union of the path sets"
        );
        assert!(left.is_valid(), "two valid edges join to a valid reference");

        let mut valid = reference(0);
        let mut invalid = reference(1);
        invalid.invalidate(InvalidationEvent::CallWrite);
        valid.join(&invalid);
        assert_eq!(
            valid.validity,
            ReferenceValidity::Invalid(InvalidationEvent::CallWrite),
            "one invalid edge makes the joined reference invalid"
        );
    }

    /// [REF-1] a join does not duplicate a path both edges already name.
    #[test]
    fn a_join_keeps_one_copy_of_a_shared_path() {
        let mut left = reference(0);
        let right = reference(0);
        left.join(&right);
        assert_eq!(left.paths.len(), 1);
    }

    /// [REF-1] an index one edge wrote the binding of after the formation is
    /// superseded after the join: both edges still hold the one capture, so
    /// the joined reference keeps one member, and the binding's spelling no
    /// longer names its index. Another capture from the same binding keeps
    /// its spelling.
    #[test]
    fn a_join_supersedes_an_index_one_edge_wrote() {
        use crate::semantic::places::{CaptureId, CapturedTerm, CapturedValue};
        let indexed = |capture: u32| {
            let mut place = ResolvedPlace::binding(BindingId(0));
            place.push_subscript(CapturedValue::new(
                CaptureId::source(capture),
                CapturedTerm::Binding(BindingId(1)),
            ));
            ReferenceInfo::formed(ReferenceKind::Single, place)
        };
        let mut unwritten = indexed(0);
        let mut written = indexed(0);
        written.paths[0].supersede_binding(BindingId(1));
        unwritten.join(&written);
        assert_eq!(unwritten.paths, written.paths);

        let mut other_formation = indexed(1);
        other_formation.join(&written);
        assert_eq!(other_formation.paths.len(), 2);
        assert_eq!(
            other_formation.paths[0]
                .spelled_indices()
                .collect::<Vec<_>>(),
            [(CaptureId::source(1), BindingId(1))]
        );
    }

    /// [REF-1] classify when a contribution retains its entering shape and
    /// when it needs a descendant summary instead.
    #[test]
    fn shape_classification_ignores_indices_but_distinguishes_descent() {
        let indexed = |capture: u32, value: u64| {
            let mut place = ResolvedPlace::binding(BindingId(0));
            place.push_subscript(crate::semantic::places::CapturedValue::new(
                crate::semantic::places::CaptureId::source(capture),
                crate::semantic::places::CapturedTerm::Literal(value),
            ));
            ReferenceInfo::formed(ReferenceKind::Single, place)
        };
        let first = indexed(0, 1);
        let moved = indexed(1, 2);
        assert!(
            first.shape_agrees_with(&moved),
            "a rebinding may move the captured index value"
        );

        let mut longer = ResolvedPlace::binding(BindingId(0));
        longer.push_subscript(crate::semantic::places::CapturedValue::new(
            crate::semantic::places::CaptureId::source(2),
            crate::semantic::places::CapturedTerm::Literal(1),
        ));
        longer.path.push(PlaceStep::Field(0));
        let extended = ReferenceInfo::formed(ReferenceKind::Single, longer);
        assert!(
            !first.shape_agrees_with(&extended),
            "descent leaves the entering static shape and needs a cover"
        );
    }
}
