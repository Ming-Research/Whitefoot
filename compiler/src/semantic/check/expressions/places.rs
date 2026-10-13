//! Type-directed place suffixes [GRAM-5, TYPE-7].
//!
//! The postfix `^` selects the referent of `&T`, or the run named by `&[T]`
//! or `&Run<T>`; a run admits only an index, range step or `len` measure.
//! A Box exposes its content through the ordinary field `inner` [TYPE-9]. Resolving
//! a reference step substitutes the path that reference names; every
//! [OWN-7] judgment reads that same resolved place [REF-1].

use crate::semantic::check::CheckContext;
use crate::semantic::check::{DeclarationInventory, TypeContext};
use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeferredUseRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    CheckedContainerRoot, CheckedExpression, CheckedMeasure, CheckedMode, CheckedNominalKind,
    CheckedOwnedTakeCleanup, CheckedPlaceStep, CheckedType, IntegerType,
};
use super::super::super::places::{
    CapturedTerm, CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace, WindowPart,
};
use super::super::references::{OWN1_ROOTED_CONSUME, WIN3_NO_TAKE};

use super::super::{CheckStop, Checker, EffectSet, LocalBinding, PlaceAccess, TypedExpression};
use super::{PlaceUseContext, PlaceUseOptions, ResolvedPlaceSet};

/// Type-directed member formation, before a use judges access or ownership.
/// A source field and a Box content carry the same selected type but retain
/// their different storage steps. The measure judgment handles descriptor
/// observations before asking for a storage member.
#[derive(Clone, Copy)]
pub(in crate::semantic::check) enum PlaceMember {
    /// A readonly field the prelude declares [TYPE-2, OP-15].
    Measure(CheckedMeasure),
    Field {
        nominal: super::super::super::model::NominalId,
        index: u32,
        ty: CheckedType,
    },
    BoxContent {
        nominal: super::super::super::model::NominalId,
        ty: CheckedType,
    },
}

impl PlaceMember {
    pub(in crate::semantic::check) fn ty(self) -> CheckedType {
        match self {
            Self::Field { ty, .. } | Self::BoxContent { ty, .. } => ty,
            Self::Measure(_) => CheckedType::Integer(IntegerType::U64),
        }
    }

    pub(in crate::semantic::check) fn storage_step(self) -> CheckedPlaceStep {
        match self {
            Self::Field { index, .. } => CheckedPlaceStep::Field(index),
            Self::Measure(measure) => CheckedPlaceStep::Measure(measure),
            Self::BoxContent { nominal, .. } => CheckedPlaceStep::BoxReferent(nominal),
        }
    }
}

/// A typed place, before its read, measure, set or consume judgment.
///
/// The v0.59 `borrow` and `holder_pending` fields are gone with the loans: a
/// reference is not storage of its own, so a resolved place never carries one
/// — the `^` step has already been replaced by the path the reference
/// names [REF-1].
#[derive(Clone)]
pub(in crate::semantic::check) struct ElaboratedPlace {
    /// The source declaration the *written* base names. For a `^` path
    /// that is the reference binding, whose [REF-2] validity the caller
    /// rechecks; `resolved` below is rooted at what that reference names.
    pub(super) declaration: DeclarationId,
    pub(in crate::semantic::check) ty: CheckedType,
    pub(in crate::semantic::check) mode: CheckedMode,
    pub(super) expression: CheckedExpression,
    pub(super) resolved: ResolvedPlaceSet,
    /// [OP-15, MSR-1] the measure a trailing `.len`, `.cap` or
    /// `.head` reads. A measure selects no storage below itself, so it is
    /// always the last written suffix and the place it is read over is the
    /// one this record otherwise describes.
    pub(super) measure: Option<super::super::super::model::CheckedMeasure>,
    /// Whether the written path starts with `^` on a range reference
    /// [REF-4, TYPE-7]. [MSR-1] gives that run its own row, whose one cell is
    /// the range's element count. This record carries the element type, so
    /// the row cannot be recovered from that type.
    pub(super) range_referent: bool,
}

impl ElaboratedPlace {
    /// The compact field representation used by ordinary binding reads and
    /// writes. Addressed and measured places keep their richer formed path.
    pub(super) fn plain_fields(&self) -> Option<Vec<u32>> {
        if self.measure.is_some() {
            return None;
        }
        self.resolved
            .identity
            .path
            .iter()
            .map(|step| match step {
                PlaceStep::Field(index) => Some(*index),
                _ => None,
            })
            .collect()
    }
}

impl<'unit> Checker<'_, 'unit> {
    /// A read of a place written through an explicit `^` [TYPE-7].
    pub(super) fn check_dereferenced_place_use(
        &mut self,
        check_context: &CheckContext<'_>,
        use_node: NodeId,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        let place = self.elaborate_value_place(check_context, use_node, node, bindings)?;
        self.check_elaborated_place_use(check_context, use_node, node, bindings, options, place)
    }

    /// Whether a written place selects an element: a subscript or range step.
    fn place_has_element_step(&self, node: NodeId) -> Result<bool, CheckStop> {
        use crate::syntax::views::PlaceSuffix;
        for suffix in self
            .types
            .declarations
            .tree
            .children_with(node, Production::Psuffix)?
        {
            if matches!(
                self.types.declarations.tree.place_suffix(suffix)?,
                PlaceSuffix::Index { .. } | PlaceSuffix::Range { .. }
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_elaborated_place_use(
        &mut self,
        check_context: &CheckContext<'_>,
        use_node: NodeId,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
        place: ElaboratedPlace,
    ) -> Result<TypedExpression, CheckStop> {
        // [MSR-1] measures select copy u64 values, rather than the measured
        // container. [DIAG-1] gives their moves and all other copy moves to
        // [OWN-1] before [SHARE-1] considers a frozen-content consume.
        // [TYPE-7] range referents have already been restricted to their
        // admitted projections during place formation.
        let copy = place.measure.is_some() || self.types.is_copy_type(check_context, place.ty)?;
        if copy && options.explicit_move && Checker::judges_class_spelling(check_context) {
            return self.types.declarations.issue_node(
                SemanticRule::Own1,
                use_node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: self.copy_move_repair(use_node)?,
                },
            );
        }
        for member in &place.resolved.members {
            // [DIAG-1] a move through `^` is OWN-1's and an element take
            // WIN-3's; SHARE-1 owns the remaining explicit non-copy consumes.
            if options.explicit_move
                && !copy
                && !self.types.declarations.tree.place_has_dereference(node)?
                && !self.place_has_element_step(node)?
                && self
                    .types
                    .frozen_member_on_resolved_path(check_context, member, bindings)?
                    .is_some()
            {
                return self.frozen_consume_issue(node);
            }
            if options.explicit_move {
                self.types
                    .reject_shared_read_write(node, member, bindings)?;
            }
            self.check_commit_place_live(member, use_node, false)?;
        }
        // [OP-15, MSR-1] a measure is a read-only `own u64` member of the
        // measured place: it reads that place's descriptor storage and
        // nothing below it, so it is neither a copy nor an affine use of the
        // value itself.
        if let Some(measure) = place.measure {
            let mut effects = EffectSet::NONE;
            for member in &place.resolved.members {
                for path in self.effect_paths_for_descriptor(use_node, member, bindings, measure)? {
                    effects.add_read(path);
                }
            }
            let (binding, path, proof_base) = self
                .types
                .declarations
                .explicit_container_path(&place.expression, node)?;
            // [REF-4, MSR-1] a range reference's one measure is its element
            // count, and the row it belongs to is fixed by the written base
            // rather than by the type the dereference selects [TYPE-7].
            if place.range_referent {
                if !path.is_empty() {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let element = self.types.intern_element(place.ty)?;
                return Ok(TypedExpression {
                    expression: CheckedExpression::RangeMeasure {
                        measure,
                        root: super::super::super::model::CheckedRangeRoot {
                            formation: None,
                            binding,
                            element,
                            element_type: place.ty,
                            proof_base,
                        },
                    },
                    mode: CheckedMode::Own,
                    reference: None,
                    reference_value: false,
                    effects,
                    accesses: place
                        .resolved
                        .members
                        .into_iter()
                        .map(|place| PlaceAccess {
                            place,
                            selected: true,
                        })
                        .collect(),
                });
            }
            return Ok(TypedExpression {
                expression: CheckedExpression::ContainerMeasure {
                    measure,
                    root: CheckedContainerRoot {
                        root: PlaceRoot::Binding(binding),
                        path,
                        ty: place.ty,
                        proof_base,
                    },
                },
                mode: CheckedMode::Own,
                reference: None,
                reference_value: false,
                effects,
                accesses: place
                    .resolved
                    .members
                    .into_iter()
                    .map(|place| PlaceAccess {
                        place,
                        selected: true,
                    })
                    .collect(),
            });
        }
        let read_out = !copy
            && options.explicit_move
            && self.body.take_commit_read_out(&place.resolved.identity);
        if !copy && !read_out {
            if options.explicit_move {
                // [OWN-1] a consume is admitted only for a place rooted in a
                // live own-mode binding. A place written under a `^` is
                // rooted at the storage the reference names, which this
                // function does not own, so the `move` is refused there and
                // not by [WIN-3], whose subject is a window slot or an array
                // element.
                if self.types.declarations.tree.place_has_dereference(node)? {
                    return self.types.declarations.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::MoveThroughReference {
                            mechanical_fix: OWN1_ROOTED_CONSUME,
                        },
                    );
                }
                // [TYPE-9, WIN-3] an owned path through Box content consumes
                // its complete root. Keep the selected value separate from
                // the residual fields and enclosing cells to be released.
                if place
                    .resolved
                    .identity
                    .path
                    .iter()
                    .any(|step| matches!(step, PlaceStep::Deref))
                    && place
                        .resolved
                        .identity
                        .path
                        .iter()
                        .all(|step| matches!(step, PlaceStep::Field(_) | PlaceStep::Deref))
                {
                    return self.types.check_box_unbox(
                        check_context,
                        use_node,
                        node,
                        place.declaration,
                        place.ty,
                        &place.resolved.identity.path,
                        bindings,
                    );
                }
                // [WIN-3] there is no take operation and no hole: a move out
                // of a place a reference names would leave storage no owner
                // can account for.
                return self.types.declarations.issue_node(
                    SemanticRule::Win3,
                    use_node,
                    SemanticIssueKind::InvalidElementMove {
                        mechanical_fix: WIN3_NO_TAKE,
                    },
                );
            }
            if matches!(options.context, PlaceUseContext::Ordinary)
                || !self.types.declarations.tree.place_has_dereference(node)?
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Own1,
                    use_node,
                    SemanticIssueKind::BareAffineUse {
                        mechanical_fix: if self
                            .types
                            .declarations
                            .tree
                            .place_has_dereference(node)?
                        {
                            OWN1_ROOTED_CONSUME
                        } else {
                            "write `move p` for the affine place"
                        },
                    },
                );
            }
        }
        let mut effects = EffectSet::NONE;
        for member in &place.resolved.members {
            for path in self.effect_paths_for_place(use_node, member, bindings)? {
                effects.add_read(path);
            }
        }
        let expression = if read_out {
            let (binding, path, proof_base) = self
                .types
                .declarations
                .explicit_container_path(&place.expression, node)?;
            CheckedExpression::ReadStorage {
                carrier: self.types.declarations.tree.path(use_node)?.clone(),
                root: CheckedContainerRoot {
                    root: PlaceRoot::Binding(binding),
                    path,
                    ty: place.ty,
                    proof_base,
                },
            }
        } else {
            place.expression
        };
        Ok(TypedExpression {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses: place
                .resolved
                .members
                .into_iter()
                .map(|place| PlaceAccess {
                    place,
                    selected: true,
                })
                .collect(),
        })
    }

    /// [GRAM-5, REF-1] the complete written `place`, with every `^` step
    /// already replaced by the path the reference it names names.
    pub(in crate::semantic::check) fn elaborate_value_place(
        &mut self,
        check_context: &CheckContext<'_>,
        carrier: NodeId,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ElaboratedPlace, CheckStop> {
        let suffixes = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Psuffix)?;
        self.elaborate_place_prefix(
            check_context,
            carrier,
            node,
            &suffixes,
            bindings,
            LexicalUseRole::PlaceBase,
        )
    }

    /// [INV-1, ENT-6] the read one written affine factor denotes when it is a
    /// tracked field place: its base names a live binding, followed by `^`
    /// exactly when that binding is a `&T` reference and then by one or more
    /// field selections, and its proof path is that root with field
    /// selections alone, ending at an integer field. `None` for every other
    /// place, which the caller judges by its own rows.
    pub(in crate::semantic::check) fn tracked_field_factor(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        root_role: LexicalUseRole,
    ) -> Result<Option<(DeclarationId, CheckedExpression)>, CheckStop> {
        use crate::syntax::views::PlaceSuffix;
        let suffixes = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Psuffix)?;
        let Some(&first) = suffixes.first() else {
            return Ok(None);
        };
        let dereferenced = matches!(
            self.types.declarations.tree.place_suffix(first)?,
            PlaceSuffix::Dereference
        );
        let fields = if dereferenced {
            &suffixes[1..]
        } else {
            suffixes.as_slice()
        };
        if fields.is_empty() {
            return Ok(None);
        }
        for suffix in fields {
            if !matches!(
                self.types.declarations.tree.place_suffix(*suffix)?,
                PlaceSuffix::Member(_)
            ) {
                return Ok(None);
            }
        }
        let pbase = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.types.declarations.tree.children(pbase)?.is_empty() {
            return Ok(None);
        }
        let usage = self
            .types
            .declarations
            .use_at(check_context, pbase, root_role)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Ok(None);
        };
        // The written `^` follows the root exactly when the root is a `&T`
        // reference; any other spelling is no tracked field place, and the
        // caller's own rows judge it.
        let Some(root) = bindings.get(&declaration) else {
            return Ok(None);
        };
        if !root.live || dereferenced != (root.mode == CheckedMode::Reference) {
            return Ok(None);
        }
        let place = self.elaborate_place_prefix(
            check_context,
            node,
            node,
            &suffixes,
            bindings,
            root_role,
        )?;
        let identity = &place.resolved.identity;
        if !matches!(place.ty, CheckedType::Integer(_))
            || place.measure.is_some()
            || place.range_referent
            || !matches!(identity.root, PlaceRoot::Binding(_))
            || identity.path.is_empty()
            || !identity
                .path
                .iter()
                .all(|step| matches!(step, PlaceStep::Field(_)))
        {
            return Ok(None);
        }
        Ok(Some((declaration, place.expression)))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn elaborate_place_prefix(
        &mut self,
        check_context: &CheckContext<'_>,
        carrier: NodeId,
        node: NodeId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        root_role: LexicalUseRole,
    ) -> Result<ElaboratedPlace, CheckStop> {
        let pbase = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [MSR-3] projects an entry former through the same typed suffix
        // walk as a bare parameter. Its contract admission is checked by
        // check_entry_formers; it introduces no executable place form.
        let projected_entry = check_context.active_postcondition.is_some()
            && self
                .types
                .declarations
                .tree
                .has_fixed(pbase, crate::FixedTerminal::Entry)?;
        if !self.types.declarations.tree.children(pbase)?.is_empty() && !projected_entry {
            return self
                .types
                .declarations
                .unsupported(UnsupportedSemanticFeature::CompositeValues, pbase);
        }
        let usage = self
            .types
            .declarations
            .use_at(check_context, pbase, root_role)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let local = bindings
            .get(&declaration)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if !local.live {
            return self.types.declarations.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::UseAfterMove {
                    mechanical_fix: "introduce a new `let` binding before reuse",
                },
            );
        }
        let place = self.local_place(carrier, &local)?;
        self.elaborate_place_members(check_context, carrier, suffixes, place, bindings)
    }

    fn local_place(
        &self,
        carrier: NodeId,
        local: &LocalBinding,
    ) -> Result<ElaboratedPlace, CheckStop> {
        Ok(ElaboratedPlace {
            declaration: local.declaration,
            ty: local.ty,
            mode: local.mode,
            expression: CheckedExpression::Binding {
                carrier: self.types.declarations.tree.path(carrier)?.clone(),
                binding: local.binding,
                ty: local.ty,
                consume_root: false,
            },
            resolved: ResolvedPlaceSet::one(ResolvedPlace::binding(local.binding)),
            measure: None,
            range_referent: false,
        })
    }

    pub(super) fn elaborate_local_place(
        &mut self,
        check_context: &CheckContext<'_>,
        carrier: NodeId,
        suffixes: &[NodeId],
        local: &LocalBinding,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ElaboratedPlace, CheckStop> {
        let place = self.local_place(carrier, local)?;
        self.elaborate_place_members(check_context, carrier, suffixes, place, bindings)
    }

    fn elaborate_place_members(
        &mut self,
        check_context: &CheckContext<'_>,
        carrier: NodeId,
        suffixes: &[NodeId],
        mut place: ElaboratedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ElaboratedPlace, CheckStop> {
        if suffixes.len() >= 2 {
            let split = suffixes.len() - 2;
            let pages = self
                .types
                .declarations
                .tree
                .source_spelling(suffixes[split])?;
            let len = self
                .types
                .declarations
                .tree
                .source_spelling(suffixes[split + 1])?;
            if pages == ".pages" && len == ".len" {
                let mut base = self.elaborate_place_members(
                    check_context,
                    carrier,
                    &suffixes[..split],
                    place.clone(),
                    bindings,
                )?;
                if base.mode == CheckedMode::Own
                    && !base.range_referent
                    && matches!(
                        base.ty,
                        CheckedType::Window {
                            shape: super::super::super::model::WindowShape::Paged,
                            ..
                        }
                    )
                {
                    base.measure = Some(CheckedMeasure::Pages);
                    return Ok(base);
                }
            }
        }
        for &suffix in suffixes {
            if matches!(
                self.types.declarations.tree.place_suffix(suffix)?,
                crate::syntax::views::PlaceSuffix::Dereference
            ) {
                place = self.resolve_explicit_dereference(carrier, suffix, place, bindings)?;
                continue;
            }
            // [OP-15] a measure is read as a member of the measured place,
            // and [MSR-1] gives it no storage below itself, so it ends the
            // written path. [TYPE-10] refuses a write of one and a read of a
            // window part; a read of a measure is exactly this form.
            if place.measure.is_some() {
                return self.types.declarations.issue_node(
                    SemanticRule::Type10,
                    suffix,
                    SemanticIssueKind::ReservedPseudoField {
                        spelling: self
                            .types
                            .declarations
                            .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                            .spelling()
                            .to_owned(),
                        mechanical_fix: "a measure selects no member of its own [MSR-1]",
                    },
                );
            }
            // A subscript selects a composite element value, which this
            // version does not implement for explicit deref chains; the
            // indexed path in `flat_storage` owns [OP-4].
            if self
                .types
                .declarations
                .tree
                .subscript_offset(suffix)?
                .is_some()
            {
                return self
                    .types
                    .declarations
                    .unsupported(UnsupportedSemanticFeature::CompositeValues, suffix);
            }
            // [TYPE-7] a reference binding used where a value of its referent
            // type is expected needs `p^`; a suffix on one is exactly
            // that use.
            if place.mode.is_reference() {
                return self.types.declarations.issue_node(
                    SemanticRule::Type7,
                    suffix,
                    SemanticIssueKind::MissingDereference {
                        mechanical_fix: "write `p^`",
                    },
                );
            }
            let name = self
                .types
                .declarations
                .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                .spelling()
                .to_owned();
            // [TYPE-10] the selected type determines whether a spelling is
            // a measure, a window part, or an ordinary source field.
            if let Some(measure) = super::super::types::measure_named(&name) {
                // [OP-14, PRE-1] at a boxed argument a compiler-owned row's
                // measure place instantiates as `window.inner`: the shape
                // parameter W admits `Box<Slots<T>>` and `Box<Ring<T>>`, and
                // the clause then reads the content's measure, exactly as any
                // `Box` content is reached [TYPE-9]. Only a row's own clause
                // takes this step; source code writes the `inner` field
                // itself.
                if !place.range_referent
                    && self.types.declarations.tree.is_prelude_node(suffix)?
                    && let CheckedType::Nominal(nominal) = place.ty
                    && let CheckedNominalKind::Box { referent, .. } =
                        self.types.nominal(nominal)?.kind
                    && super::super::expressions::flat_storage::measured_kind_of(place.ty).is_none()
                    && super::super::expressions::flat_storage::measured_kind_of(referent).is_some()
                {
                    place.expression = CheckedExpression::BoxDeref {
                        carrier: self.types.declarations.tree.path(carrier)?.clone(),
                        nominal,
                        referent,
                        value: Box::new(place.expression),
                    };
                    place.ty = referent;
                    place.resolved.append_step(PlaceStep::Deref);
                }
                let measured = if place.range_referent {
                    Some(super::super::super::model::MeasuredKind::Range)
                } else {
                    super::super::expressions::flat_storage::measured_kind_of(place.ty)
                };
                // x1 [TYPE-10]: the measure spellings reserve nothing, so
                // `len` selects a measure only where the type of the place it
                // follows has a row for it. Everywhere else the suffix falls
                // through to the ordinary field walk below, which is how a
                // source struct declares its own field named `len`.
                if let Some(measured) = measured {
                    if matches!(
                        measure.cell(measured),
                        super::super::super::model::MeasureCell::Absent
                    ) {
                        return self.types.declarations.issue_node(
                            SemanticRule::Type5,
                            suffix,
                            SemanticIssueKind::type_mismatch(
                                "a measured place whose measure table has this row",
                                self.types.checked_type_name(place.ty)?,
                            ),
                        );
                    }
                    place.measure = Some(measure);
                    continue;
                }
            }
            self.types.declarations.reject_window_part(
                suffix,
                &name,
                place.ty,
                place.range_referent,
            )?;
            let member = self
                .types
                .elaborate_place_member(check_context, suffix, place.ty)?;
            place.expression = match member {
                PlaceMember::Measure(measure) => {
                    place.measure = Some(measure);
                    continue;
                }
                PlaceMember::BoxContent { nominal, ty } => {
                    place.resolved.append_step(PlaceStep::Deref);
                    CheckedExpression::BoxDeref {
                        carrier: self.types.declarations.tree.path(carrier)?.clone(),
                        nominal,
                        referent: ty,
                        value: Box::new(place.expression),
                    }
                }
                PlaceMember::Field { nominal, index, ty } => {
                    place.resolved.extend_fields(&[index]);
                    CheckedExpression::ProjectValue {
                        carrier: self.types.declarations.tree.path(carrier)?.clone(),
                        value: Box::new(place.expression),
                        nominal,
                        field: index,
                        ty,
                    }
                }
            };
            place.ty = member.ty();
        }
        Ok(place)
    }

    /// [TYPE-7] `place^` denotes the referent of a reference, and
    /// nothing else.
    ///
    /// [REF-1] resolving a place whose `^` step names a reference
    /// variable replaces that step with the path that reference names, so the
    /// resolved place below this step is rooted wherever that path is rooted.
    pub(super) fn resolve_explicit_dereference(
        &mut self,
        carrier: NodeId,
        pbase: NodeId,
        mut inner: ElaboratedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ElaboratedPlace, CheckStop> {
        if !inner.mode.is_reference() {
            return self.types.declarations.issue_node(
                SemanticRule::Type7,
                self.types.declarations.tree.parent(pbase)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix:
                        "a Box's content is its field inner [TYPE-9]; an owned place is named \
                         as itself",
                },
            );
        }
        if inner.mode.is_range() {
            self.types.declarations.check_range_referent(pbase)?;
        }
        let local = bindings
            .get(&inner.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.check_reference_valid(local, pbase)?;
        let binding = local.binding;
        let named = self
            .types
            .resolve_reference_root(inner.declaration, bindings)?;
        if named.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // [REF-1, OP-12] one resolved member is an exact static place even
        // when two different reference holders name it. A true union has no
        // selected member until run time, so its exact identity remains the
        // holder and no overlapping candidate is chosen on its behalf.
        let identity = match named.as_slice() {
            [unique] => unique.clone(),
            _ => ResolvedPlace::binding(binding),
        };
        // The referent is read through the reference; the reference itself
        // stays a distinct expression so lowering never has to guess whether
        // a reference binding is passed on or read through [TYPE-7].
        inner.expression = CheckedExpression::DerefAddressed {
            carrier: self.types.declarations.tree.path(carrier)?.clone(),
            binding,
            ty: inner.ty,
            proof: self.body.exact_description(local),
        };
        inner.range_referent = inner.mode.is_range();
        inner.mode = CheckedMode::Own;
        inner.resolved = ResolvedPlaceSet {
            identity,
            members: named,
        };
        Ok(inner)
    }

    /// The restructuring [TYPE-2] states for a readonly write target.
    pub(in crate::semantic::check) const READONLY_WRITE_TARGET_FIX: &'static str =
        "use the operation that changes it, or replace the whole value";
}

impl<'unit> TypeContext<'unit> {
    /// [TYPE-7] type range-referent steps before the narrower expression
    /// shapes of contracts and proofs can attribute their use to another rule.
    pub(in crate::semantic::check) fn check_range_referent_places(
        &self,
        check_context: &CheckContext<'_>,
        expression: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        for place in self
            .declarations
            .tree
            .descendants_with(expression, Production::Place)?
        {
            let suffixes = self
                .declarations
                .tree
                .children_with(place, Production::Psuffix)?;
            let Some(0) = self.declarations.tree.reference_step(&suffixes)? else {
                continue;
            };
            // [REF-3] result selectors cannot denote references.
            if self
                .declarations
                .postcondition_selector_spelling(check_context, place)?
                .is_some()
            {
                continue;
            }
            let base = self
                .declarations
                .tree
                .first_child_with(place, Production::Pbase)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let usage = self.declarations.use_at_roles(
                check_context,
                base,
                &[
                    LexicalUseRole::PlaceBase,
                    LexicalUseRole::InvariantValue,
                    LexicalUseRole::ProofValue,
                ],
            )?;
            if let ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Value,
            } = usage.target()
                && bindings
                    .get(&declaration)
                    .is_some_and(|local| local.mode.is_range())
            {
                self.declarations.check_range_referent(suffixes[0])?;
            }
        }
        Ok(())
    }

    /// Resolve the type transition without choosing a diagnostic. A write
    /// judges readonly members before ordinary member validity; other uses
    /// judge validity first. Both consume this same type-directed selection.
    pub(in crate::semantic::check) fn place_member(
        &self,
        ty: CheckedType,
        name: &str,
    ) -> Result<Option<PlaceMember>, CheckStop> {
        // [PRE-1, TYPE-2, OP-15] measures on these value types are declared
        // fields. Range and run-selection lengths carry no declaration and
        // are handled only by the observation judgment [REF-4, MSR-1].
        if let Some(measure) = super::super::types::measure_named(name)
            && let Some(measured) = super::super::expressions::flat_storage::measured_kind_of(ty)
            && !matches!(
                measure.cell(measured),
                super::super::super::model::MeasureCell::Absent
            )
        {
            return Ok(Some(PlaceMember::Measure(measure)));
        }
        let CheckedType::Nominal(nominal) = ty else {
            return Ok(None);
        };
        Ok(match &self.nominal(nominal)?.kind {
            CheckedNominalKind::Box { referent, .. } => Some(PlaceMember::BoxContent {
                nominal,
                ty: *referent,
            }),
            CheckedNominalKind::Struct { fields } => fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == name)
                .map(|(index, field)| {
                    Ok::<_, CheckStop>(PlaceMember::Field {
                        nominal,
                        index: u32::try_from(index)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                        ty: field.ty,
                    })
                })
                .transpose()?,
            _ => None,
        })
    }

    /// Form one storage member. Access, move and write judgments consume this
    /// result; none of them resolves a field or a Box member independently.
    pub(in crate::semantic::check) fn elaborate_place_member(
        &self,
        check_context: &CheckContext<'_>,
        suffix: NodeId,
        ty: CheckedType,
    ) -> Result<PlaceMember, CheckStop> {
        // [TYPE-8] stored fields and elements are values, never references.
        // A caret encountered after selecting one therefore violates TYPE-7.
        if matches!(
            self.declarations.tree.place_suffix(suffix)?,
            crate::syntax::views::PlaceSuffix::Dereference
        ) {
            return self.declarations.issue_node(
                SemanticRule::Type7,
                self.declarations
                    .tree
                    .parent(suffix)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "apply `^` only to a reference; name an owned place as itself",
                },
            );
        }
        if self.declarations.tree.subscript_offset(suffix)?.is_some() {
            return self
                .declarations
                .unsupported(UnsupportedSemanticFeature::CompositeValues, suffix);
        }
        let name = self
            .declarations
            .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
            .spelling();
        if name == "pages"
            && matches!(
                ty,
                CheckedType::Window {
                    shape: super::super::super::model::WindowShape::Paged,
                    ..
                }
            )
        {
            let parent = self
                .declarations
                .tree
                .parent(suffix)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let suffixes = self
                .declarations
                .tree
                .children_with(parent, Production::Psuffix)?;
            let position = suffixes
                .iter()
                .position(|node| *node == suffix)
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let indexed = suffixes
                .get(position + 1)
                .map(|next| self.declarations.tree.subscript_offset(*next))
                .transpose()?
                .flatten()
                .is_some();
            return self.declarations.issue_node(
                if indexed { SemanticRule::Op4 } else { SemanticRule::Type10 },
                if indexed { suffixes[position + 1] } else { suffix },
                SemanticIssueKind::ReservedPseudoField { spelling: name.to_owned(), mechanical_fix: "read `p.pages.len` or borrow one initialized page as `&p.pages[k]`" },
            );
        }
        self.declarations
            .reject_window_part(suffix, name, ty, false)?;
        let member = self.place_member(ty, name)?;
        match member {
            Some(PlaceMember::Measure(_)) => {}
            Some(PlaceMember::BoxContent { .. }) => {
                if name != "inner" {
                    return self.declarations.issue_node(
                        SemanticRule::Type9,
                        suffix,
                        SemanticIssueKind::type_mismatch(
                            "the Box content field `inner`",
                            format!("the field name `{name}`, which a Box does not declare"),
                        ),
                    );
                }
            }
            Some(PlaceMember::Field { nominal, index, .. }) => {
                self.reject_inaccessible_field(
                    check_context,
                    nominal,
                    None,
                    index as usize,
                    name,
                    suffix,
                )?;
            }
            None => {
                let is_struct = matches!(ty, CheckedType::Nominal(nominal)
                    if matches!(self.nominal(nominal)?.kind, CheckedNominalKind::Struct { .. }));
                let (expected, actual) = if is_struct {
                    (
                        format!("a declared field of {}", self.checked_type_name(ty)?),
                        format!("the field name `{name}`, which that struct does not declare"),
                    )
                } else {
                    (
                        "a source struct, whose declared field this suffix selects".to_owned(),
                        self.checked_type_name(ty)?,
                    )
                };
                return self.declarations.issue_node(
                    SemanticRule::Type5,
                    suffix,
                    SemanticIssueKind::type_mismatch(expected, actual),
                );
            }
        }
        member.ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    fn checked_owned_take_path(
        &self,
        mut ty: CheckedType,
        path: &[PlaceStep],
    ) -> Result<Vec<CheckedPlaceStep>, CheckStop> {
        let mut checked = Vec::with_capacity(path.len());
        for step in path {
            match (*step, ty) {
                (PlaceStep::Field(field), CheckedType::Nominal(id)) => {
                    let CheckedNominalKind::Struct { fields } = &self.nominal(id)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    ty = fields
                        .get(field as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .ty;
                    checked.push(CheckedPlaceStep::Field(field));
                }
                (PlaceStep::Deref, CheckedType::Nominal(id)) => {
                    let CheckedNominalKind::Box { referent, .. } = self.nominal(id)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    ty = referent;
                    checked.push(CheckedPlaceStep::BoxReferent(id));
                }
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            }
        }
        Ok(checked)
    }
    /// The element type one index or range step selects below `ty`: a range
    /// reference already carries its element type [REF-4], and every other
    /// indexable base names it in its shape [OP-4].
    fn selected_element(
        &self,
        ty: Option<CheckedType>,
        range: bool,
    ) -> Result<Option<CheckedType>, CheckStop> {
        if range {
            return Ok(ty);
        }
        ty.map(|ty| self.container_element_type(ty))
            .transpose()
            .map(Option::flatten)
    }
    /// The selected type of a resolved target. A descendant summary keeps
    /// its referent type even though its containing anchor has another type.
    pub(in crate::semantic::check) fn resolved_place_type(
        &self,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedType>, CheckStop> {
        let mut range = false;
        let mut ty = match path.root {
            PlaceRoot::Binding(binding) => {
                let Some(local) = bindings.values().find(|local| local.binding == binding) else {
                    return Ok(None);
                };
                range = local.mode.is_range();
                local.ty
            }
            PlaceRoot::Constant(id) => self.constant(id)?.declared_type,
        };
        for step in &path.path {
            ty = match *step {
                PlaceStep::Descendant(target) => {
                    range = target.range;
                    target.ty
                }
                PlaceStep::Field(index) => {
                    let CheckedType::Nominal(id) = ty else {
                        return Ok(None);
                    };
                    let CheckedNominalKind::Struct { fields } = &self.nominal(id)?.kind else {
                        return Ok(None);
                    };
                    fields
                        .get(index as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .ty
                }
                PlaceStep::Deref => {
                    let Some(referent) = self.box_content(ty)? else {
                        return Ok(None);
                    };
                    referent
                }
                PlaceStep::Payload { variant, field } => {
                    let CheckedType::Nominal(id) = ty else {
                        return Ok(None);
                    };
                    let CheckedNominalKind::Enum { variants } = &self.nominal(id)?.kind else {
                        return Ok(None);
                    };
                    variants
                        .get(variant as usize)
                        .and_then(|variant| variant.fields.get(field as usize))
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .ty
                }
                PlaceStep::Index(_) | PlaceStep::Range(_) | PlaceStep::Page(_) => {
                    // [TYPE-9] a segment and the run of every element are
                    // runs of T, as a range step's referent is.
                    let (element, run) = if range {
                        (ty, false)
                    } else {
                        match ty {
                            CheckedType::Array { element, .. }
                            | CheckedType::Window { element, .. }
                            | CheckedType::Shards { element } => {
                                (self.element_type(element)?, false)
                            }
                            CheckedType::Buffer { element } => (self.element_type(element)?, false),
                            CheckedType::Segments { element } => {
                                (self.element_type(element)?, true)
                            }
                            _ => return Ok(None),
                        }
                    };
                    range = run || matches!(step, PlaceStep::Range(_) | PlaceStep::Page(_));
                    element
                }
                PlaceStep::Measure(_) => CheckedType::Integer(IntegerType::U64),
                PlaceStep::Part(_) => return Ok(None),
            };
        }
        Ok((!range).then_some(ty))
    }
    /// [TYPE-9, WIN-3] `let n = move b.inner;`.
    ///
    /// [TYPE-9]: "`let n = move b.inner;` consumes the `Box`, yields its
    /// content, and frees the cell [WIN-3]." [WIN-3] makes the move out of a
    /// field a consume of the whole owner, so the cell binding dies here and
    /// the content is this expression's value. A cell has exactly one field,
    /// so no other part of the owner survives to take a derived release.
    ///
    /// [TYPE-9] refuses the same spelling at a runtime-capacity content: its
    /// block is the cell's heap object, and taking the window out of the cell
    /// would leave a `Slots<T>` value in a position the rule admits nowhere.
    #[allow(clippy::too_many_arguments)]
    fn check_box_unbox(
        &self,
        check_context: &CheckContext<'_>,
        use_node: NodeId,
        place_node: NodeId,
        declaration: DeclarationId,
        referent: CheckedType,
        resolved_path: &[PlaceStep],
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
    ) -> Result<TypedExpression, CheckStop> {
        let local = bindings
            .get(&declaration)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if matches!(
            referent,
            CheckedType::Buffer { .. }
                | CheckedType::Segments { .. }
                | CheckedType::Window { capacity: None, .. }
        ) {
            // The last content step selects the runtime-capacity value;
            // its prefix is the Box that can move or be released [TYPE-9].
            let (PlaceStep::Deref, cell_path) = resolved_path
                .split_last()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let mut cell = ResolvedPlace::binding(local.binding);
            cell.path = cell_path.to_vec();
            let cell = self.render_resolved_place(&cell, bindings)?;
            let window = matches!(referent, CheckedType::Window { .. });
            let droppable = self
                .linear_release_obligation(check_context, referent)?
                .is_none();
            return self.declarations.issue_node(
                SemanticRule::Type9,
                use_node,
                SemanticIssueKind::InvalidRestrictedTypePlacement {
                    spelling: self.checked_type_name(referent)?,
                    mechanical_fix: super::super::repairs::runtime_content_move(
                        &cell, window, droppable,
                    ),
                },
            );
        }
        let path = self.checked_owned_take_path(local.ty, resolved_path)?;
        let mut cleanup = Vec::new();
        for action in self.owned_take_cleanup(check_context, local.ty, &path)? {
            match action {
                CheckedOwnedTakeCleanup::Drop { path, ty } => {
                    // [PROV-6, DIAG-1] judge the residual's capability before
                    // omitting an empty runtime release: a nodrop tag-only
                    // enum still cannot be abandoned by this consume.
                    if let Some(obligation) = self.linear_release_obligation(check_context, ty)? {
                        return self.declarations.issue_node(
                            SemanticRule::Prov6,
                            place_node,
                            SemanticIssueKind::LinearValuePartiallyConsumed {
                                obligation,
                                residual: self.checked_type_name(ty)?,
                                mechanical_fix: "destructure the whole value with let N(f: a, ...) = move v;",
                            },
                        );
                    }
                    // [STOR-3] use the same component releases as a named
                    // owner, prefixed by this residual's complete Box path.
                    for (fields, ty) in self.drop_paths(check_context, ty, Vec::new())? {
                        let mut selected = path.clone();
                        selected.extend(fields.into_iter().map(CheckedPlaceStep::Field));
                        cleanup.push(CheckedOwnedTakeCleanup::Drop { path: selected, ty });
                    }
                }
                shell @ CheckedOwnedTakeCleanup::BoxShell { .. } => cleanup.push(shell),
            }
        }
        // [REF-2] a consume is one of the three invalidating actions: every
        // reference into the cell names storage this move has carried away.
        self.invalidate_references(
            bindings,
            &ResolvedPlace::binding(local.binding),
            &super::super::references::InvalidationEvent::PrefixMoved,
        )?;
        bindings
            .get_mut(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .live = false;
        Ok(TypedExpression {
            expression: CheckedExpression::BoxTake {
                carrier: self.declarations.tree.path(use_node)?.clone(),
                referent,
                binding: local.binding,
                path,
                cleanup,
            },
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects: EffectSet::NONE,
            accesses: vec![PlaceAccess {
                place: ResolvedPlace::binding(local.binding),
                selected: true,
            }],
        })
    }
    fn owned_take_cleanup(
        &self,
        check_context: &CheckContext<'_>,
        root: CheckedType,
        selected: &[CheckedPlaceStep],
    ) -> Result<Vec<CheckedOwnedTakeCleanup>, CheckStop> {
        fn walk<'a>(
            checker: &TypeContext<'a>,
            check_context: &CheckContext<'_>,
            ty: CheckedType,
            selected: &[CheckedPlaceStep],
            path: &mut Vec<CheckedPlaceStep>,
            out: &mut Vec<CheckedOwnedTakeCleanup>,
        ) -> Result<(), CheckStop> {
            let Some(step) = selected.first() else {
                return Ok(());
            };
            match (ty, step) {
                (CheckedType::Nominal(id), CheckedPlaceStep::Field(selected_field)) => {
                    let CheckedNominalKind::Struct { fields } = &checker.nominal(id)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    for (index, field) in fields.iter().enumerate() {
                        let index = u32::try_from(index)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                        path.push(CheckedPlaceStep::Field(index));
                        if index == *selected_field {
                            walk(checker, check_context, field.ty, &selected[1..], path, out)?;
                        } else if !checker.is_copy_type(check_context, field.ty)? {
                            out.push(CheckedOwnedTakeCleanup::Drop {
                                path: path.clone(),
                                ty: field.ty,
                            });
                        }
                        path.pop();
                    }
                }
                (CheckedType::Nominal(id), CheckedPlaceStep::BoxReferent(step_id))
                    if id == *step_id =>
                {
                    let CheckedNominalKind::Box { referent, .. } = checker.nominal(id)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    path.push(CheckedPlaceStep::BoxReferent(id));
                    walk(checker, check_context, referent, &selected[1..], path, out)?;
                    path.pop();
                    out.push(CheckedOwnedTakeCleanup::BoxShell {
                        path: path.clone(),
                        nominal: id,
                        referent,
                    });
                }
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            }
            Ok(())
        }

        let mut out = Vec::new();
        walk(
            self,
            check_context,
            root,
            selected,
            &mut Vec::new(),
            &mut out,
        )?;
        Ok(out)
    }
    /// Whether a written `psuffix` run reaches a [TYPE-9] `Box`'s content.
    ///
    /// A `Box`'s one member `inner` *is* that content, so the place below it
    /// is the dereference the content already is and not a field selection.
    /// The ordinary field walk has no step for that and the explicit-place
    /// walker does, so this decides which of the two resolves the place. A
    /// path this walk cannot follow reaches no box content: the ordinary walk
    /// then reports its own rejection at the suffix that failed, which is the
    /// diagnostic the writer needs.
    pub(in crate::semantic::check) fn place_path_reaches_box_content(
        &self,
        check_context: &CheckContext<'_>,
        suffixes: &[NodeId],
        mut ty: CheckedType,
    ) -> Result<bool, CheckStop> {
        for (position, &suffix) in suffixes.iter().enumerate() {
            if self.declarations.tree.subscript_offset(suffix)?.is_some() {
                return Ok(false);
            }
            if let CheckedType::Nominal(nominal) = ty
                && matches!(self.nominal(nominal)?.kind, CheckedNominalKind::Box { .. })
            {
                return Ok(true);
            }
            let Ok((_, selected)) =
                self.resolve_struct_path(check_context, &suffixes[position..=position], ty)
            else {
                return Ok(false);
            };
            ty = selected;
        }
        Ok(false)
    }
    /// [CONST-2, OWN-11, TYPE-2] a callee's declared write requires writable
    /// actual storage. A reference cannot grant write access to a named const,
    /// a compiler-updated counted binder, or a readonly field.
    ///
    /// The checked reference carries the resolved origin path through local
    /// aliases, reborrows and reference-root projections [REF-1]. Walking
    /// that path, rather than the immediate argument syntax, therefore keeps
    /// the readonly provenance that the reference spelling itself no longer
    /// exposes.
    pub(in crate::semantic::check) fn check_written_reference_argument(
        &self,
        check_context: &CheckContext<'_>,
        atom: NodeId,
        argument: &TypedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        let Some(reference) = &argument.reference else {
            return Ok(());
        };
        for path in &reference.paths {
            let immutable = match path.root {
                PlaceRoot::Constant(id) => {
                    Some((SemanticRule::Const2, self.constant(id)?.name.clone()))
                }
                PlaceRoot::Binding(binding) => bindings
                    .values()
                    .find(|local| local.binding == binding && local.compiler_updated)
                    .map(|local| {
                        self.declarations
                            .declaration_spelling(local.declaration)
                            .map(|name| (SemanticRule::Own11, name))
                    })
                    .transpose()?,
            };
            if let Some((rule, binding)) = immutable {
                return self.declarations.issue_node(
                    rule,
                    atom,
                    SemanticIssueKind::ImmutableWrittenArgument {
                        binding,
                        mechanical_fix: "copy the value into a local binding and pass a reference \
                                         to that writable copy",
                    },
                );
            }
            self.reject_readonly_resolved_write(check_context, atom, path, bindings)?;
        }
        Ok(())
    }
    /// Read authority belongs to the resolved atomic root, not an alias's type.
    pub(in crate::semantic::check) fn reject_shared_read_write(
        &self,
        target: NodeId,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        if bindings
            .values()
            .any(|local| local.read_only_state && path.root == PlaceRoot::Binding(local.binding))
        {
            return self.declarations.issue_node(
                SemanticRule::Share2,
                target,
                SemanticIssueKind::ReadonlyWriteTarget {
                    spelling: "SharedRead state".to_owned(),
                    mechanical_fix: "read this state without writing or consuming it; to update an ordinary shared object, use a target of a mutable Shared handle if one is available; fire cancellation through cancel_fire outside the atomic statement",
                },
            );
        }
        Ok(())
    }
    /// [TYPE-2] applies the readonly provenance carried by one resolved path
    /// to either a direct write through a reference or a written-reference
    /// call argument.
    pub(in crate::semantic::check) fn reject_readonly_resolved_write(
        &self,
        check_context: &CheckContext<'_>,
        target: NodeId,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        self.reject_shared_read_write(target, path, bindings)?;
        let Some(spelling) =
            self.readonly_member_on_resolved_path(check_context, path, bindings)?
        else {
            return Ok(());
        };
        self.declarations.issue_node(
            SemanticRule::Type2,
            target,
            SemanticIssueKind::ReadonlyWriteTarget {
                spelling,
                mechanical_fix: Checker::READONLY_WRITE_TARGET_FIX,
            },
        )
    }
    /// [TYPE-2, REF-1] the first readonly declaration member crossed by one resolved origin
    /// path. Every reference alias retains this path, while its referent type
    /// alone cannot say which declaration field supplied it.
    pub(in crate::semantic::check) fn readonly_member_on_resolved_path(
        &self,
        check_context: &CheckContext<'_>,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<String>, CheckStop> {
        self.readonly_path_member(check_context, path, bindings, false)
    }

    pub(in crate::semantic::check) fn frozen_member_on_resolved_path(
        &self,
        check_context: &CheckContext<'_>,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<String>, CheckStop> {
        self.readonly_path_member(check_context, path, bindings, true)
    }

    /// [SHARE-2] whether a table subscript selects `Entries<V>`, a run of
    /// entries, given the step that follows it: a `KeySet` offset selects the
    /// run and the next step subscripts it, while a key offset selects the
    /// one `Option` entry and the next step is its payload. The step after
    /// the subscript is the only place a resolved path records which.
    fn table_selects_entries(next: Option<&PlaceStep>) -> bool {
        matches!(next, Some(PlaceStep::Index(_)))
    }
    /// [SHARE-2] the `Option` entry type of a table's subscript.
    fn table_entry_of(&self, ty: Option<CheckedType>) -> Result<Option<CheckedType>, CheckStop> {
        let Some(CheckedType::Nominal(nominal)) = ty else {
            return Ok(None);
        };
        Ok(match &self.nominal(nominal)?.kind {
            CheckedNominalKind::Shared {
                shape: crate::semantic::CheckedShared::Map { entry },
                ..
            } => Some(*entry),
            _ => None,
        })
    }
    /// [SHARE-2, REF-1] indexed atomic roots already carry the selected
    /// Option/Entries type. Their original self-rooted index is an identity
    /// prefix, not a second element selection: the number of leading steps
    /// of `path` that select nothing below the root's own type.
    fn atomic_index_prefix(
        &self,
        local: &LocalBinding,
        path: &ResolvedPlace,
    ) -> Result<usize, CheckStop> {
        let record = self
            .declarations
            .resolved
            .declaration(local.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if record.role() != crate::DeclarationRole::AtomicBinder
            || !matches!(path.path.first(), Some(PlaceStep::Index(_)))
        {
            return Ok(0);
        }
        let node = self
            .declarations
            .tree
            .node_with_path(record.origin().node())
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let ordinal = self
            .declarations
            .declarations_at(node, crate::DeclarationRole::AtomicBinder)?
            .iter()
            .position(|candidate| candidate.id() == local.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let targets = self
            .declarations
            .tree
            .children_with(node, Production::Place)?;
        let target = *targets
            .get(ordinal)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let suffixes = self
            .declarations
            .tree
            .children_with(target, Production::Psuffix)?;
        let subscripted = suffixes
            .last()
            .map(|suffix| self.declarations.tree.subscript_offset(*suffix))
            .transpose()?
            .flatten()
            .is_some();
        Ok(usize::from(subscripted))
    }

    fn readonly_path_member(
        &self,
        check_context: &CheckContext<'_>,
        path: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        frozen_only: bool,
    ) -> Result<Option<String>, CheckStop> {
        #[derive(Clone, Copy)]
        enum PathType {
            Value(CheckedType),
            Range(CheckedType),
            // [SHARE-2, REF-1] a map selector names either its Option entry
            // or Entries; the next resolved step determines the projection.
            MapSelection(CheckedType),
        }

        let mut prefix = 0;
        let mut ty = match path.root {
            PlaceRoot::Binding(binding) => {
                let Some(local) = bindings.values().find(|local| local.binding == binding) else {
                    // A carried alternative rooted in an iteration-local
                    // binding is invalidated by scope cleanup and the
                    // header validity equation. It has no live type here.
                    return Ok(None);
                };
                prefix = self.atomic_index_prefix(local, path)?;
                if local.mode.is_range() {
                    PathType::Range(local.ty)
                } else {
                    PathType::Value(local.ty)
                }
            }
            PlaceRoot::Constant(constant) => {
                PathType::Value(self.constant(constant)?.declared_type)
            }
        };
        for step in &path.path[prefix..] {
            if let PathType::MapSelection(entry) = ty {
                ty = match step {
                    PlaceStep::Payload { .. } | PlaceStep::Index(_) => {
                        if Self::table_selects_entries(Some(step)) {
                            PathType::Range(entry)
                        } else {
                            PathType::Value(entry)
                        }
                    }
                    // Entries.len is readonly; a checked Option entry cannot
                    // have a measure selection [SHARE-2, OP-15].
                    PlaceStep::Measure(_) | PlaceStep::Descendant(_) => ty,
                    PlaceStep::Field(_)
                    | PlaceStep::Deref
                    | PlaceStep::Range(_)
                    | PlaceStep::Page(_)
                    | PlaceStep::Part(_) => {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                };
            }
            match *step {
                PlaceStep::Descendant(target) => {
                    if target.frozen_content {
                        return Ok(Some("a descendant of Frozen.inner".to_owned()));
                    }
                    if target.readonly && !frozen_only {
                        return Ok(Some("a readonly descendant path".to_owned()));
                    }
                    ty = if target.range {
                        PathType::Range(target.ty)
                    } else {
                        PathType::Value(target.ty)
                    };
                }
                PlaceStep::Field(index) => {
                    let PathType::Value(current) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedType::Nominal(nominal) = current else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let field = fields
                        .get(index as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    if !frozen_only && self.field_withholds_writes(check_context, nominal, field) {
                        return Ok(Some(field.name.clone()));
                    }
                    ty = PathType::Value(field.ty);
                }
                PlaceStep::Deref => {
                    let PathType::Value(current) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedType::Nominal(nominal) = current else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind
                    else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    if self.is_frozen_type(current)? {
                        return Ok(Some("Frozen.inner".to_owned()));
                    }
                    ty = PathType::Value(referent);
                }
                PlaceStep::Payload { variant, field } => {
                    let PathType::Value(current) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedType::Nominal(nominal) = current else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Enum { variants } = &self.nominal(nominal)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let field = variants
                        .get(variant as usize)
                        .and_then(|variant| variant.fields.get(field as usize))
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    if !frozen_only && self.field_withholds_writes(check_context, nominal, field) {
                        return Ok(Some(field.name.clone()));
                    }
                    ty = PathType::Value(field.ty);
                }
                PlaceStep::Index(_)
                | PlaceStep::Range(_)
                | PlaceStep::Page(_)
                | PlaceStep::Part(_) => {
                    // [TYPE-9, SHARE-2, REF-4] one exhaustive shape walk
                    // preserves readonly provenance below every element and
                    // map-entry projection; a new checked shape needs a case.
                    let (element, run) = match ty {
                        PathType::Range(element) => (element, false),
                        PathType::MapSelection(_) => {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        PathType::Value(current) => match current {
                            CheckedType::Array { element, .. }
                            | CheckedType::Window { element, .. }
                            | CheckedType::Buffer { element }
                            | CheckedType::Entries { element }
                            | CheckedType::Shards { element } => {
                                (self.element_type(element)?, false)
                            }
                            CheckedType::Segments { element } => {
                                (self.element_type(element)?, true)
                            }
                            CheckedType::Nominal(nominal) => match &self.nominal(nominal)?.kind {
                                CheckedNominalKind::Shared {
                                    shape: crate::semantic::CheckedShared::Map { entry },
                                    ..
                                } if matches!(step, PlaceStep::Index(_)) => {
                                    ty = PathType::MapSelection(*entry);
                                    continue;
                                }
                                CheckedNominalKind::Struct { .. }
                                | CheckedNominalKind::Enum { .. }
                                | CheckedNominalKind::Box { .. }
                                | CheckedNominalKind::Opaque
                                | CheckedNominalKind::Shared { .. } => {
                                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                                }
                            },
                            CheckedType::Unit
                            | CheckedType::Bool
                            | CheckedType::Integer(_)
                            | CheckedType::Float(_)
                            | CheckedType::Generic(_)
                            | CheckedType::GenericInt(_)
                            | CheckedType::GenericFloat(_)
                            | CheckedType::KeySet => {
                                return Err(SemanticCompilerFailure::InvalidResolution.into());
                            }
                        },
                    };
                    ty = if run
                        || matches!(
                            step,
                            PlaceStep::Range(_)
                                | PlaceStep::Page(_)
                                | PlaceStep::Part(WindowPart::Filled | WindowPart::Free)
                        ) {
                        PathType::Range(element)
                    } else {
                        PathType::Value(element)
                    };
                }
                PlaceStep::Measure(measure) => {
                    return Ok((!frozen_only).then(|| measure.spelling().to_owned()));
                }
            }
        }
        Ok(None)
    }
    /// One resolved place in its source spelling [FORM-2, REF-1]: the place
    /// expression that names the same storage at this point of the body.
    ///
    /// A reference parameter roots every path that goes through it, and the
    /// storage it names is written under `^` [REF-1, TYPE-7]; a local
    /// reference never roots a resolved place, because resolution replaced it
    /// by the path it names. A `Box` content step is the field `inner`
    /// [TYPE-9] and a payload step names its variant and field [FORM-2]. An
    /// index or range position spells the value it captured when the place
    /// was formed [REF-1]: a literal, a const, or the binding it read. A
    /// computed offset has no source name and renders as `?`, and a loop
    /// summary's unknown descendant cover keeps [REF-1]'s `.**` notation.
    ///
    /// This is the one renderer every checker payload naming a resolved place
    /// uses — the [EFF-5] substituted paths, the [OP-12] target and the
    /// [REF-2] preservation pairs — so none of them shows a checker-internal
    /// binding number or field ordinal. The selected type is kept through
    /// every step, a window part included [WIN-2], so a field or payload step
    /// whose type is lost is an internal resolution failure and never an
    /// ordinal in a repair.
    pub(in crate::semantic::check) fn render_resolved_place(
        &self,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<String, CheckStop> {
        let mut range = false;
        let mut prefix = 0;
        let (mut rendered, mut ty) = match place.root {
            PlaceRoot::Binding(binding) => {
                match bindings.values().find(|local| local.binding == binding) {
                    Some(local) => {
                        let name = self.declarations.declaration_spelling(local.declaration)?;
                        prefix = self.atomic_index_prefix(local, place)?;
                        range = local.mode.is_range();
                        if local.mode.is_reference() {
                            (format!("{name}^"), Some(local.ty))
                        } else {
                            (name, Some(local.ty))
                        }
                    }
                    None => ("?".to_owned(), None),
                }
            }
            PlaceRoot::Constant(constant) => {
                let constant = self.constant(constant)?;
                (constant.name.clone(), Some(constant.declared_type))
            }
        };
        for (position, step) in place.path.iter().enumerate() {
            match *step {
                PlaceStep::Descendant(target) => {
                    rendered.push_str(".**");
                    ty = Some(target.ty);
                    range = target.range;
                }
                PlaceStep::Field(field) => {
                    let Some(CheckedType::Nominal(nominal)) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let declared = fields
                        .get(field as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    rendered.push('.');
                    rendered.push_str(&declared.name);
                    ty = Some(declared.ty);
                }
                PlaceStep::Deref => match ty.map(|ty| self.box_content(ty)).transpose()? {
                    Some(Some(referent)) => {
                        rendered.push_str(".inner");
                        ty = Some(referent);
                    }
                    _ => {
                        rendered = format!("{rendered}^");
                        // A borrow keeps its referent's type; a shared
                        // object's `^` reaches its state [SHARE-1].
                        if let Some(CheckedType::Nominal(nominal)) = ty
                            && let CheckedNominalKind::Shared {
                                state,
                                shape: crate::semantic::CheckedShared::Object,
                            } = self.nominal(nominal)?.kind
                        {
                            ty = Some(state);
                        }
                    }
                },
                PlaceStep::Payload { variant, field } => {
                    let Some(CheckedType::Nominal(nominal)) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Enum { variants } = &self.nominal(nominal)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let declared = variants
                        .get(variant as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let payload = declared
                        .fields
                        .get(field as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    rendered.push_str(&format!(".{}.{}", declared.name, payload.name));
                    ty = Some(payload.ty);
                }
                PlaceStep::Page(offset) => {
                    rendered.push_str(&format!(
                        ".pages[{}]",
                        self.declarations.render_captured_offset(offset, bindings)?
                    ));
                    ty = self.selected_element(ty, range)?;
                    range = true;
                }
                PlaceStep::Index(offset) => {
                    // [SHARE-2] an indexed atomic binder's own subscript is
                    // not in the writer's spelling of the binder.
                    if position < prefix {
                        continue;
                    }
                    rendered.push_str(&format!(
                        "[{}]",
                        self.declarations.render_captured_offset(offset, bindings)?
                    ));
                    // [TYPE-9] a segment is a run of T.
                    let segment = !range && matches!(ty, Some(CheckedType::Segments { .. }));
                    match self.table_entry_of(ty)? {
                        Some(entry) if !range => {
                            ty = Some(entry);
                            range = Self::table_selects_entries(place.path.get(position + 1));
                        }
                        _ => {
                            ty = self.selected_element(ty, range)?;
                            range = segment;
                        }
                    }
                }
                PlaceStep::Range(_)
                    if !range && matches!(ty, Some(CheckedType::Segments { .. })) =>
                {
                    rendered.push_str(".all");
                    ty = self.selected_element(ty, range)?;
                    range = true;
                }
                PlaceStep::Range(span) => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.declarations
                            .render_captured_offset(span.start, bindings)?,
                        self.declarations
                            .render_captured_offset(span.end, bindings)?
                    ));
                    ty = self.selected_element(ty, range)?;
                    range = true;
                }
                PlaceStep::Part(part) => {
                    rendered.push('.');
                    rendered.push_str(part.spelling());
                    ty = self.selected_element(ty, range)?;
                    range = matches!(part, WindowPart::Filled | WindowPart::Free);
                }
                PlaceStep::Measure(measure) => {
                    rendered.push('.');
                    rendered.push_str(measure.spelling());
                    ty = Some(CheckedType::Integer(IntegerType::U64));
                }
            }
        }
        Ok(rendered)
    }
    /// [TYPE-2, TYPE-10] refuses a write target whose path names a readonly
    /// field or a window part of the place it follows.
    ///
    /// The walk is type-directed because x1 reserves neither vocabulary from a
    /// declaration: a suffix names a measure or a part only where the type of
    /// the place it follows has one, and it is an ordinary field everywhere
    /// else. A path whose step this walk cannot follow is left to the
    /// ordinary resolver, which reports the step that failed.
    pub(in crate::semantic::check) fn reject_reserved_write_members(
        &self,
        check_context: &CheckContext<'_>,
        target: NodeId,
        suffixes: &[NodeId],
        mut ty: CheckedType,
    ) -> Result<(), CheckStop> {
        for &suffix in suffixes {
            if self.declarations.tree.subscript_offset(suffix)?.is_some() {
                // [MSR-2] a write at an element position reaches that
                // element's own storage; the offset carries no member name.
                ty = match ty {
                    CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                        self.element_type(element)?
                    }
                    CheckedType::Buffer { element } => self.element_type(element)?,
                    _ => return Ok(()),
                };
                continue;
            }
            let name = self
                .declarations
                .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                .spelling()
                .to_owned();
            self.declarations
                .reject_window_part(suffix, &name, ty, false)?;
            // A measure of a measured place is a readonly field of that
            // shape's [PRE-1] declaration, so a `set` on it is [TYPE-2]'s
            // refusal and never a struct-field rejection.
            if super::super::types::measure_named(&name).is_some()
                && super::super::expressions::flat_storage::measured_kind_of(ty).is_some()
            {
                return self.declarations.issue_node(
                    SemanticRule::Type2,
                    target,
                    SemanticIssueKind::ReadonlyWriteTarget {
                        spelling: name,
                        mechanical_fix: Checker::READONLY_WRITE_TARGET_FIX,
                    },
                );
            }
            let Some(member) = self.place_member(ty, &name)? else {
                return Ok(());
            };
            if let PlaceMember::BoxContent { nominal, .. } = member
                && self.is_frozen_type(CheckedType::Nominal(nominal))?
            {
                return self.declarations.issue_node(
                    SemanticRule::Type2,
                    target,
                    SemanticIssueKind::ReadonlyWriteTarget {
                        spelling: "Frozen.inner".to_owned(),
                        mechanical_fix: Checker::READONLY_WRITE_TARGET_FIX,
                    },
                );
            }
            if let PlaceMember::Field { nominal, index, .. } = member {
                let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                if self.field_withholds_writes(check_context, nominal, &fields[index as usize]) {
                    return self.declarations.issue_node(
                        SemanticRule::Type2,
                        target,
                        SemanticIssueKind::ReadonlyWriteTarget {
                            spelling: name,
                            mechanical_fix: Checker::READONLY_WRITE_TARGET_FIX,
                        },
                    );
                }
            }
            ty = member.ty();
        }
        Ok(())
    }
}

impl<'unit> DeclarationInventory<'unit> {
    /// [TYPE-7] a range referent is a run, admitted only as the base of an
    /// element subscript, a range step or its `len` measure. Inspect the
    /// complete written place even when a caller elaborates only a prefix.
    pub(in crate::semantic::check) fn check_range_referent(
        &self,
        caret: NodeId,
    ) -> Result<(), CheckStop> {
        use crate::syntax::views::PlaceSuffix;
        let place = self
            .tree
            .parent(caret)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        let position = suffixes
            .iter()
            .position(|suffix| *suffix == caret)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let rest = &suffixes[position + 1..];
        if let Some(&next) = rest.first() {
            match self.tree.place_suffix(next)? {
                PlaceSuffix::Index { .. } | PlaceSuffix::Range { .. } => return Ok(()),
                // [MSR-1] the run's one measure ends the place.
                PlaceSuffix::Member(_)
                    if rest.len() == 1
                        && self.trailing_measure_member(rest)?
                            == Some(CheckedMeasure::Length) =>
                {
                    return Ok(());
                }
                _ => {}
            }
        }
        self.issue_node(
            SemanticRule::Type7,
            place,
            SemanticIssueKind::RangeReferentUse {
                mechanical_fix: "write `let q = r;` to alias the reference, `&r^[i]` to borrow one element, or `&r^[a..b]` to borrow a subrange",
            },
        )
    }

    /// The measure a written `psuffix` run ends with [OP-15, MSR-1], if any.
    ///
    /// [MSR-1] gives a measure no storage below itself, so only the last
    /// suffix can name one and a subscript never does. The spelling decides
    /// this only as a routing hint. Elaboration decides from the selected
    /// type whether it is a measure or an ordinary source field [TYPE-10].
    pub(in crate::semantic::check) fn trailing_measure_member(
        &self,
        suffixes: &[NodeId],
    ) -> Result<Option<CheckedMeasure>, CheckStop> {
        let Some(&last) = suffixes.last() else {
            return Ok(None);
        };
        let crate::syntax::views::PlaceSuffix::Member(member) = self.tree.place_suffix(last)?
        else {
            return Ok(None);
        };
        let name = std::str::from_utf8(self.tree.token_bytes(member.field)?)
            .map_err(|_| SemanticCompilerFailure::InvalidSourceEncoding)?;
        Ok(super::super::types::measure_named(name))
    }
    /// The value one index step or range endpoint captured, as the source
    /// names it [REF-1]: an [OP-4] offset is a `u64`, so a literal carries
    /// that suffix [FORM-5].
    fn render_captured_offset(
        &self,
        offset: CapturedValue,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<String, CheckStop> {
        Ok(match offset.term {
            CapturedTerm::Literal(value) => format!("{value}_u64"),
            CapturedTerm::Const(declaration) => self.declaration_spelling(declaration)?,
            CapturedTerm::Binding(binding) => {
                match bindings.values().find(|local| local.binding == binding) {
                    Some(local) => self.declaration_spelling(local.declaration)?,
                    None => "?".to_owned(),
                }
            }
            // The binding's spelling names a later value than the one this
            // index read, and no source spelling names that one.
            CapturedTerm::Superseded(_) | CapturedTerm::Opaque => "?".to_owned(),
        })
    }
    /// [TYPE-10] a window part is effect-row vocabulary and never a place.
    ///
    /// x1 makes the four spellings reserve nothing: they are "selected by the
    /// window type of the place they follow", so `node.next` on a source
    /// struct is that struct's declared field and only `r.next` on a measured
    /// place is the refusal. The measure spellings left this rule with the
    /// reservation: they are the readonly fields [PRE-1] declares, refused as
    /// write targets by [TYPE-2] and read as ordinary fields everywhere else.
    pub(in crate::semantic::check) fn reject_window_part(
        &self,
        suffix: NodeId,
        name: &str,
        followed: CheckedType,
        range_referent: bool,
    ) -> Result<(), CheckStop> {
        if super::super::types::window_part_named(name).is_none() {
            return Ok(());
        }
        if !range_referent
            && super::super::expressions::flat_storage::measured_kind_of(followed).is_none()
        {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Type10,
            suffix,
            SemanticIssueKind::ReservedPseudoField {
                spelling: name.to_owned(),
                mechanical_fix: "use the operation that moves the window boundary [OP-10]",
            },
        )
    }
}
