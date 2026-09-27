use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::syntax::terminal::FixedTerminal;
use crate::{
    DeclarationClass, DeclarationId, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    CheckedArrayRoot, CheckedBufferRoot, CheckedConst, CheckedContainerRoot, CheckedExpression,
    CheckedLayoutCeiling, CheckedLayoutMagnitude, CheckedMeasure, CheckedMode, CheckedNominalKind,
    CheckedPlaceStep, CheckedPlaceSubscript, CheckedRangeElementPlace, CheckedRangeRoot,
    CheckedSetTarget, CheckedTargetDomainObligation, CheckedType, IntegerType, MeasureCell,
    MeasuredKind, NominalId, SubscriptedTerm,
};
use super::super::super::places::{
    CaptureId, CapturedTerm, CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace,
};
use super::super::references::RequiredReferent;

/// [WIN-3] the restructuring a move out of a window slot or an array element
/// names.
const WIN3_NO_SLOT_MOVE: &str = "use take_back, remove_at, or swap [OP-10, OP-11]";
use super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, PlaceAccess, TypedExpression,
};
use super::{MutationTarget, PlaceUseOptions, ResolvedPlaceSet};

#[derive(Clone)]
pub(in crate::semantic::check) struct CheckedArrayPlace {
    pub(super) root: CheckedArrayRoot,
    declaration: Option<DeclarationId>,
    array_type: CheckedType,
    element_type: CheckedType,
    length: CheckedConst,
}

impl CheckedArrayPlace {
    fn resolved_place(&self) -> Option<ResolvedPlace> {
        let CheckedArrayRoot::Binding { binding, fields } = &self.root else {
            return None;
        };
        Some(ResolvedPlace::fields(*binding, fields.clone()))
    }
}

/// One indexable base reached through a `&[T]` range reference [REF-4].
///
/// [TYPE-7] makes the referent a `deref` of a range reference selects the
/// element type, so the measure-table row [MSR-1] gives `&[T]` cannot be
/// recovered from that type; this place carries the row by construction.
#[derive(Clone)]
pub(in crate::semantic::check) struct CheckedRangePlace {
    root: CheckedRangeRoot,
    declaration: DeclarationId,
    element_type: CheckedType,
    /// The place the reference names [REF-1], for effects and [OWN-7].
    resolved: ResolvedPlaceSet,
}

#[derive(Clone)]
pub(in crate::semantic::check) struct CheckedBufferPlace {
    root: CheckedBufferRoot,
    declaration: DeclarationId,
    element_type: CheckedType,
    resolved: ResolvedPlaceSet,
    offsets: CarriedOperands,
}

#[derive(Clone)]
pub(in crate::semantic::check) enum CheckedIndexedPlace {
    Array(CheckedArrayPlace),
    Buffer(CheckedBufferPlace),
    /// The run of elements a `&[T]` range reference names [REF-4, OP-4].
    Range(CheckedRangePlace),
    /// One storage shape [TYPE-9]: its measure-table row is [MSR-1], and its
    /// indexability is [OP-4].
    Container(CheckedContainerPlace),
}

#[derive(Clone)]
pub(in crate::semantic::check) struct CheckedContainerPlace {
    root: CheckedContainerRoot,
    resolved: ResolvedPlaceSet,
    /// The source declaration this place is rooted in, where it has one; a
    /// place rooted in a named const [CONST-2] has none.
    declaration: Option<DeclarationId>,
    /// The effects and accesses of every offset occurring inside the place
    /// [EFF-2]: an offset that reads a binding is a read of that binding,
    /// wherever in the place it occurs.
    offsets: CarriedOperands,
}

/// The effects and accesses one place's own offset operands exhibit.
#[derive(Clone, Default)]
pub(in crate::semantic::check) struct CarriedOperands {
    pub(in crate::semantic::check) effects: EffectSet,
    pub(in crate::semantic::check) accesses: Vec<PlaceAccess>,
}

fn add_layout_magnitude(
    left: CheckedLayoutMagnitude,
    right: CheckedLayoutMagnitude,
) -> CheckedLayoutMagnitude {
    match (left, right) {
        (CheckedLayoutMagnitude::Finite(left), CheckedLayoutMagnitude::Finite(right)) => {
            left.checked_add(right).map_or(
                CheckedLayoutMagnitude::AboveU64,
                CheckedLayoutMagnitude::Finite,
            )
        }
        _ => CheckedLayoutMagnitude::AboveU64,
    }
}

fn multiply_layout_magnitude(value: CheckedLayoutMagnitude, count: u64) -> CheckedLayoutMagnitude {
    if count == 0 {
        return CheckedLayoutMagnitude::Finite(0);
    }
    match value {
        CheckedLayoutMagnitude::Finite(value) => value.checked_mul(count).map_or(
            CheckedLayoutMagnitude::AboveU64,
            CheckedLayoutMagnitude::Finite,
        ),
        CheckedLayoutMagnitude::AboveU64 => CheckedLayoutMagnitude::AboveU64,
    }
}

fn round_up_layout_magnitude(value: CheckedLayoutMagnitude, align: u64) -> CheckedLayoutMagnitude {
    match value {
        CheckedLayoutMagnitude::Finite(value) => value
            .checked_add(align - 1)
            .map(|sum| sum / align * align)
            .map_or(
                CheckedLayoutMagnitude::AboveU64,
                CheckedLayoutMagnitude::Finite,
            ),
        CheckedLayoutMagnitude::AboveU64 => CheckedLayoutMagnitude::AboveU64,
    }
}

impl CheckedIndexedPlace {
    /// Mutable array owners use the same typed storage path as run slots.
    /// Keep constant arrays on their immutable-global read path.
    fn into_element_storage(self) -> Result<Self, CheckStop> {
        if let Self::Buffer(buffer) = self {
            let CheckedBufferRoot {
                binding,
                path,
                element,
                ..
            } = buffer.root;
            return Ok(Self::Container(CheckedContainerPlace {
                root: CheckedContainerRoot {
                    root: PlaceRoot::Binding(binding),
                    path,
                    ty: CheckedType::Buffer { element },
                },
                resolved: buffer.resolved,
                declaration: Some(buffer.declaration),
                offsets: buffer.offsets,
            }));
        }
        let Self::Array(array) = self else {
            return Ok(self);
        };
        let CheckedArrayRoot::Binding { binding, fields } = &array.root else {
            return Ok(Self::Array(array));
        };
        Ok(Self::Container(CheckedContainerPlace {
            root: CheckedContainerRoot {
                root: PlaceRoot::Binding(*binding),
                path: fields
                    .iter()
                    .copied()
                    .map(CheckedPlaceStep::Field)
                    .collect(),
                ty: array.array_type,
            },
            resolved: ResolvedPlaceSet::one(
                array
                    .resolved_place()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
            ),
            declaration: array.declaration,
            offsets: CarriedOperands::default(),
        }))
    }

    /// The declaration this place is rooted in, where it has one. A place
    /// rooted in a named const has none, and no proof-point admission
    /// restricts a const.
    pub(in crate::semantic::check) const fn root_declaration(&self) -> Option<DeclarationId> {
        match self {
            Self::Array(array) => array.declaration,
            Self::Buffer(buffer) => Some(buffer.declaration),
            Self::Range(range) => Some(range.declaration),
            Self::Container(container) => container.declaration,
        }
    }

    /// The resolved place of the indexed base, for [SET-1]'s element read-out
    /// matching. A slice indexes storage its own descriptor names and is not
    /// a commit target, so it has none here.
    fn indexed_base_place(&self) -> Option<ResolvedPlace> {
        match self {
            Self::Array(array) => array.resolved_place(),
            Self::Buffer(buffer) => Some(buffer.resolved.identity.clone()),
            Self::Range(range) => Some(range.resolved.identity.clone()),
            Self::Container(container) => Some(container.resolved.identity.clone()),
        }
    }

    fn element_type(&self, checker: &Checker<'_>) -> Result<CheckedType, CheckStop> {
        match self {
            Self::Array(array) => Ok(array.element_type),
            Self::Buffer(buffer) => Ok(buffer.element_type),
            Self::Range(range) => Ok(range.element_type),
            Self::Container(container) => match container.root.ty {
                CheckedType::Buffer { element } => checker.element_type(element),
                _ => checker.element_type(
                    container
                        .root
                        .element()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                ),
            },
        }
    }
}

impl<'unit> Checker<'unit> {
    /// Chooses the subscript that establishes the indexable base of a place.
    ///
    /// Ordinary nested storage is addressed inside-out, so its final
    /// subscript selects the value read or written. A `deref` of a range
    /// reference is different: its first subscript selects the range element,
    /// and every later subscript is a typed suffix below that element. Keep
    /// that distinction here so reads, writes and measures all form the same
    /// complete range-element place and retain every [OP-4] obligation in
    /// source order. Borrow formation routes through the same first subscript
    /// in the reference checker.
    pub(super) fn indexing_subscript(
        &self,
        place: NodeId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<usize>, CheckStop> {
        let Some(last) = self.last_subscript(suffixes)? else {
            return Ok(None);
        };
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.has_fixed(pbase, FixedTerminal::Deref)? {
            return Ok(Some(last));
        }
        let inner = self
            .tree
            .first_child_with(pbase, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let inner = self.resolve_explicit_place(place, inner, bindings)?;
        let dereferenced = self.resolve_explicit_dereference(place, pbase, inner, bindings)?;
        if !dereferenced.range_referent {
            return Ok(Some(last));
        }
        for (position, suffix) in suffixes.iter().enumerate() {
            if self.subscript_offset(*suffix)?.is_some() {
                return Ok(Some(position));
            }
        }
        Ok(None)
    }

    fn constant_storage_place(
        &self,
        constant: super::super::super::model::CheckedConstantId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        loop_depth: usize,
        require_named_offsets: bool,
    ) -> Result<CheckedContainerPlace, CheckStop> {
        let value = self.constant(constant)?;
        let (path, ty, offsets) = self.resolve_storage_path(
            suffixes,
            value.ty,
            bindings,
            function,
            loop_depth,
            require_named_offsets,
        )?;
        let mut resolved = ResolvedPlace {
            root: PlaceRoot::Constant(constant),
            path: Vec::new(),
        };
        resolved
            .path
            .extend(path.iter().map(CheckedPlaceStep::place_step));
        Ok(CheckedContainerPlace {
            root: CheckedContainerRoot {
                root: PlaceRoot::Constant(constant),
                path,
                ty,
            },
            resolved: ResolvedPlaceSet::one(resolved),
            // A place rooted in a named const [CONST-2] is immutable static
            // storage and is rooted in no writable declaration.
            declaration: None,
            offsets,
        })
    }

    /// [OP-15] one measure member read over a place rooted in a named
    /// constant [CONST-2].
    ///
    /// [CONST-1] says a const table is read "via a subscript, a measure
    /// member [OP-15], a field suffix, or a `&` reference", and [MSR-2] gives
    /// a measure read only the descriptor storage. Immutable static storage
    /// answers no liveness or ownership question, so the read is the `own
    /// u64` value itself and exhibits only what the place's own offsets do.
    fn check_constant_storage_measure(
        &self,
        node: NodeId,
        constant: super::super::super::model::CheckedConstantId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        let Some(measure) = self.trailing_measure_member(suffixes)? else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let place = self.constant_storage_place(
            constant,
            &suffixes[..suffixes.len() - 1],
            bindings,
            function,
            options.loop_depth,
            false,
        )?;
        let Some(measured) = measured_kind_of(place.root.ty) else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a measured place [MSR-1]",
                    self.checked_type_name(place.root.ty)?,
                ),
            );
        };
        if matches!(
            measure.cell(measured),
            super::super::super::model::MeasureCell::Absent
        ) {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a measured place whose measure table has this row",
                    self.checked_type_name(place.root.ty)?,
                ),
            );
        }
        if options.explicit_move && self.judges_class_spelling() {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: "read the measure without `move`",
                },
            );
        }
        Ok(TypedExpression {
            expression: CheckedExpression::ContainerMeasure {
                measure,
                root: place.root,
            },
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects: place.offsets.effects,
            accesses: place
                .offsets
                .accesses
                .into_iter()
                .map(PlaceAccess::operand)
                .collect(),
        })
    }

    /// [OP-15, MSR-1] one measure member read over a written place whose path
    /// carries a subscript, such as `rows[0_u64].len`.
    ///
    /// [MSR-1] admits a measure place formed with subscripts as well as
    /// field selections, so the measure is a term over the element the
    /// subscript selects rather than a field of it. The subscript inside the
    /// place is an ordinary [OP-4] occurrence and is discharged where the
    /// place is formed [MSR-4]: the container root carries it, and the
    /// measure expression's judgment submits it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_indexed_measure_use(
        &self,
        function: &FunctionSignature,
        use_node: NodeId,
        place: NodeId,
        suffixes: &[NodeId],
        subscript: usize,
        measure: CheckedMeasure,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        // [MSR-1] the measure selects no storage below itself, so the place it
        // is read over is everything written before it.
        let base = &suffixes[..suffixes.len() - 1];
        let anchor = base[subscript];
        let indexed = self
            .check_indexed_place(
                place,
                bindings,
                &base[..subscript],
                anchor,
                function,
                options.loop_depth,
            )?
            .into_element_storage()?;
        let indexed = match indexed {
            CheckedIndexedPlace::Range(range) => {
                let offset_node = self
                    .subscript_offset(anchor)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let mut probe = bindings.clone();
                let offset =
                    self.check_atom(function, offset_node, &mut probe, options.loop_depth)?;
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
                let Some(captured) = Self::captured_of(offset_node, &offset.expression) else {
                    return self
                        .unsupported(UnsupportedSemanticFeature::CompositeValues, offset_node);
                };
                let (path, selected_type, carried) = self.resolve_storage_path(
                    &base[subscript + 1..],
                    range.element_type,
                    bindings,
                    function,
                    options.loop_depth,
                    true,
                )?;
                let Some(measured) = measured_kind_of(selected_type) else {
                    return self.issue_node(
                        SemanticRule::Type5,
                        use_node,
                        SemanticIssueKind::type_mismatch(
                            "a measured place [MSR-1]",
                            self.checked_type_name(selected_type)?,
                        ),
                    );
                };
                if matches!(measure.cell(measured), MeasureCell::Absent) {
                    return self.issue_node(
                        SemanticRule::Type5,
                        use_node,
                        SemanticIssueKind::type_mismatch(
                            "a measured place whose measure table has this row",
                            self.checked_type_name(selected_type)?,
                        ),
                    );
                }
                if options.explicit_move && self.judges_class_spelling() {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::MoveOfCopy {
                            mechanical_fix: "read the measure without `move`",
                        },
                    );
                }
                let mut resolved = range.resolved;
                resolved.append_step(PlaceStep::Index(captured));
                for step in path.iter().map(CheckedPlaceStep::place_step) {
                    resolved.append_step(step);
                }
                for member in &resolved.members {
                    self.check_commit_place_live(member, use_node, false)?;
                }
                let mut effects = offset.effects.union(carried.effects);
                for member in &resolved.members {
                    for path in
                        self.effect_paths_for_descriptor(use_node, member, bindings, measure)?
                    {
                        effects.add_read(path);
                    }
                }
                let mut accesses = offset
                    .accesses
                    .into_iter()
                    .chain(carried.accesses)
                    .map(PlaceAccess::operand)
                    .collect::<Vec<_>>();
                accesses.extend(resolved.members.into_iter().map(|place| PlaceAccess {
                    place,
                    selected: true,
                }));
                return Ok(TypedExpression {
                    expression: CheckedExpression::RangeElementMeasure {
                        carrier: self.tree.path(use_node)?.clone(),
                        measure,
                        place: Box::new(CheckedRangeElementPlace {
                            root: range.root,
                            offset: offset.expression,
                            captured,
                            path,
                            ty: selected_type,
                            obligation: self.tree.path(anchor)?.clone(),
                            target_domain: CheckedTargetDomainObligation::ElementAddress,
                        }),
                    },
                    mode: CheckedMode::Own,
                    reference: None,
                    reference_value: false,
                    effects,
                    accesses,
                });
            }
            indexed => indexed,
        };
        let CheckedIndexedPlace::Container(container) = indexed else {
            // A flat buffer or a range reference has no measured element, so
            // the element this subscript selects carries no measure row.
            return self.issue_node(
                SemanticRule::Type5,
                anchor,
                SemanticIssueKind::type_mismatch(
                    "a measured place [MSR-1]",
                    self.checked_type_name(indexed.element_type(self)?)?,
                ),
            );
        };
        let container = self.extend_storage_place(
            container,
            &base[subscript..],
            bindings,
            function,
            options.loop_depth,
        )?;
        let Some(measured) = measured_kind_of(container.root.ty) else {
            return self.issue_node(
                SemanticRule::Type5,
                use_node,
                SemanticIssueKind::type_mismatch(
                    "a measured place [MSR-1]",
                    self.checked_type_name(container.root.ty)?,
                ),
            );
        };
        if matches!(measure.cell(measured), MeasureCell::Absent) {
            return self.issue_node(
                SemanticRule::Type5,
                use_node,
                SemanticIssueKind::type_mismatch(
                    "a measured place whose measure table has this row",
                    self.checked_type_name(container.root.ty)?,
                ),
            );
        }
        // [OP-15] the measure's exact type is `own u64` and it reads only the
        // descriptor storage [MSR-2], so it is neither a copy read of the
        // measured value nor an affine use of it.
        if options.explicit_move && self.judges_class_spelling() {
            return self.issue_node(
                SemanticRule::Own1,
                use_node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: "read the measure without `move`",
                },
            );
        }
        for member in &container.resolved.members {
            self.check_commit_place_live(member, use_node, false)?;
        }
        let mut effects = container.offsets.effects;
        for member in &container.resolved.members {
            for path in self.effect_paths_for_descriptor(use_node, member, bindings, measure)? {
                effects.add_read(path);
            }
        }
        let mut accesses = container
            .offsets
            .accesses
            .into_iter()
            .map(PlaceAccess::operand)
            .collect::<Vec<_>>();
        accesses.extend(
            container
                .resolved
                .members
                .into_iter()
                .map(|place| PlaceAccess {
                    place,
                    selected: true,
                }),
        );
        let expression = CheckedExpression::ContainerMeasure {
            measure,
            root: container.root,
        };
        self.refuse_unrepresented_term_place(&expression, use_node)?;
        Ok(TypedExpression {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses,
        })
    }

    pub(super) fn check_constant_storage_read(
        &self,
        node: NodeId,
        constant: super::super::super::model::CheckedConstantId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        // [OP-15, CONST-1] a const table is read through a measure member
        // exactly as any other measured place is, and [MSR-1] gives the
        // measure no storage below itself, so it ends the written path.
        if self.trailing_measure_member(suffixes)?.is_some() {
            return self.check_constant_storage_measure(
                node, constant, suffixes, bindings, function, options,
            );
        }
        let place = self.constant_storage_place(
            constant,
            suffixes,
            bindings,
            function,
            options.loop_depth,
            false,
        )?;
        if options.explicit_move
            && place
                .root
                .path
                .iter()
                .all(|step| matches!(step, CheckedPlaceStep::Field(_)))
        {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: "read the constant's copy scalar without `move`",
                },
            );
        }
        self.check_storage_read(node, node, place, bindings, options)
    }

    /// Recomputes the OP-9 ceiling after a generic GoalTemplate's element
    /// type has been instantiated. Keeping this calculation at the type
    /// authority prevents an unresolved schema layout from becoming the
    /// identity of a concrete call requirement. None is unresolved; AboveU64
    /// is a known mathematical result whose allocation limit is zero.
    pub(in crate::semantic::check) fn instantiated_layout_ceiling(
        &self,
        ty: CheckedType,
    ) -> Option<CheckedLayoutCeiling> {
        self.layout_ceiling_inner(ty, &mut HashSet::new())
    }

    fn layout_ceiling_inner(
        &self,
        ty: CheckedType,
        visiting: &mut HashSet<NominalId>,
    ) -> Option<CheckedLayoutCeiling> {
        fn finish(size: CheckedLayoutMagnitude, align: u64) -> Option<CheckedLayoutCeiling> {
            if align == 0 {
                return None;
            }
            let stride = match round_up_layout_magnitude(size, align) {
                CheckedLayoutMagnitude::Finite(0) => CheckedLayoutMagnitude::Finite(1),
                stride => stride,
            };
            Some(CheckedLayoutCeiling {
                size,
                align,
                stride,
            })
        }
        let primitive = |bytes| finish(CheckedLayoutMagnitude::Finite(bytes), bytes.max(1));
        match ty {
            CheckedType::Unit | CheckedType::Bool => primitive(1),
            CheckedType::Integer(integer) => primitive(u64::from(integer.width() / 8)),
            CheckedType::Float(float) => primitive(u64::from(float.width() / 8)),
            CheckedType::Array { element, length } => {
                let length = length.value()?;
                if length == 0 {
                    return finish(CheckedLayoutMagnitude::Finite(0), 1);
                }
                let element =
                    self.layout_ceiling_inner(self.element_type(element).ok()?, visiting)?;
                finish(
                    multiply_layout_magnitude(element.size, length),
                    element.align,
                )
            }
            // [OP-9] a runtime-capacity `Array<T>` is `(16,8)`: a pointer and
            // a length.
            CheckedType::Buffer { .. } => finish(CheckedLayoutMagnitude::Finite(16), 8),
            // [OP-9] a constant-capacity `Slots<T, N>` repeats T's pair N
            // times and then applies the sequence rule to that block followed
            // by one `(8,8)` word, its length; a `Ring<T, N>` follows it with
            // two such words, its length and its window origin.
            CheckedType::Window {
                shape,
                element,
                capacity: Some(length),
            } => {
                let length = length.value()?;
                let words = match shape {
                    super::super::super::model::WindowShape::Slots => 1,
                    super::super::super::model::WindowShape::Ring => 2,
                };
                if length == 0 {
                    return finish(CheckedLayoutMagnitude::Finite(8 * words), 8);
                }
                let element =
                    self.layout_ceiling_inner(self.element_type(element).ok()?, visiting)?;
                let mut size = multiply_layout_magnitude(element.size, length);
                let mut align = element.align.max(1);
                for _ in 0..words {
                    size = round_up_layout_magnitude(size, 8);
                    size = add_layout_magnitude(size, CheckedLayoutMagnitude::Finite(8));
                    align = align.max(8);
                }
                finish(round_up_layout_magnitude(size, align), align)
            }
            // [OP-9] a runtime-capacity `Slots<T>` is `(24,8)`, a pointer, a
            // capacity and a length; a `Ring<T>` is `(32,8)`, those three and
            // a window origin. The block's own elements live in the heap
            // object and enter no sequence [TYPE-9].
            CheckedType::Window {
                shape,
                capacity: None,
                ..
            } => finish(
                CheckedLayoutMagnitude::Finite(match shape {
                    super::super::super::model::WindowShape::Slots => 24,
                    super::super::super::model::WindowShape::Ring => 32,
                }),
                8,
            ),
            CheckedType::Nominal(id) => {
                if !visiting.insert(id) {
                    return None;
                }
                let nominal = self.nominal(id).ok()?;
                let result = match &nominal.kind {
                    // [OP-9] `Box<T>` is `(8,8)`, one pointer; its `inner`
                    // field lives in the heap object and enters no sequence.
                    CheckedNominalKind::Box { .. } => finish(CheckedLayoutMagnitude::Finite(8), 8),
                    CheckedNominalKind::Opaque => finish(CheckedLayoutMagnitude::Finite(32), 16),
                    CheckedNominalKind::Struct { fields } => {
                        self.aggregate_layout_ceiling(fields.iter().map(|field| field.ty), visiting)
                    }
                    CheckedNominalKind::Enum { variants }
                        if variants.iter().all(|variant| variant.fields.is_empty()) =>
                    {
                        primitive(if variants.len() <= 2 { 1 } else { 4 })
                    }
                    CheckedNominalKind::Enum { variants } => self.aggregate_layout_ceiling(
                        std::iter::once(CheckedType::Integer(IntegerType::U32)).chain(
                            variants
                                .iter()
                                .flat_map(|variant| variant.fields.iter().map(|field| field.ty)),
                        ),
                        visiting,
                    ),
                };
                visiting.remove(&id);
                result
            }
            // Symbolic generic bodies are validated but never lowered. A
            // bound-wide ceiling lets that structural pass retain the same
            // expression shape; every concrete instance is checked again and
            // receives its exact ceiling. Int and Float are at most 64 bits.
            CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => primitive(8),
            // An opaque parameter has no known pair. Propagate that absence
            // through by-value aggregates; Box and runtime shape shells
            // already stop expansion above. It is not mathematical overflow.
            CheckedType::Generic(_) => None,
        }
    }

    fn aggregate_layout_ceiling(
        &self,
        fields: impl IntoIterator<Item = CheckedType>,
        visiting: &mut HashSet<NominalId>,
    ) -> Option<CheckedLayoutCeiling> {
        let mut size = CheckedLayoutMagnitude::Finite(0);
        let mut align = 1_u64;
        for ty in fields {
            let field = self.layout_ceiling_inner(ty, visiting)?;
            size = round_up_layout_magnitude(size, field.align);
            size = add_layout_magnitude(size, field.size);
            align = align.max(field.align);
        }
        size = round_up_layout_magnitude(size, align);
        let stride = match size {
            CheckedLayoutMagnitude::Finite(0) => CheckedLayoutMagnitude::Finite(1),
            size => size,
        };
        Some(CheckedLayoutCeiling {
            size,
            align,
            stride,
        })
    }

    /// The [MSR-1] measure read over one already-resolved indexed place.
    ///
    /// [OP-15] makes a measure a place form and no reader row, so its one
    /// caller is [INV-1]'s affine measure factor, which reads no storage and
    /// forms no loan and therefore reaches only this part.
    pub(in crate::semantic::check) fn measure_of_indexed_place(
        &self,
        measure: CheckedMeasure,
        place: CheckedIndexedPlace,
        operand: NodeId,
    ) -> Result<CheckedExpression, CheckStop> {
        Ok(match place {
            CheckedIndexedPlace::Array(array) => CheckedExpression::ArrayMeasure {
                measure,
                root: array.root,
                length: array.length,
            },
            CheckedIndexedPlace::Buffer(buffer) => CheckedExpression::BufferMeasure {
                measure,
                root: buffer.root,
            },
            // [MSR-1] `&[T]` has exactly one row cell, `len`; every other
            // measure is the ordinary [TYPE-5] operand rejection.
            CheckedIndexedPlace::Range(range) => {
                if matches!(measure.cell(MeasuredKind::Range), MeasureCell::Absent) {
                    return self.issue_node(
                        SemanticRule::Type5,
                        operand,
                        SemanticIssueKind::type_mismatch(
                            "a measured place whose measure table has this row",
                            "a range reference, whose one measure is `len` [REF-4]",
                        ),
                    );
                }
                CheckedExpression::RangeMeasure {
                    measure,
                    root: range.root,
                }
            }
            CheckedIndexedPlace::Container(container) => {
                // [MSR-1]: a measure the table gives no row is the
                // ordinary [TYPE-5] operand rejection, carried by the
                // measured types the table does have a row for.
                let measured = container
                    .root
                    .measured()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if matches!(measure.cell(measured), MeasureCell::Absent) {
                    return self.issue_node(
                        SemanticRule::Type5,
                        operand,
                        SemanticIssueKind::type_mismatch(
                            "a measured place whose measure table has this row",
                            self.checked_type_name(container.root.ty)?,
                        ),
                    );
                }
                CheckedExpression::ContainerMeasure {
                    measure,
                    root: container.root,
                }
            }
        })
    }

    /// Extends an already resolved run base to its final selected storage.
    /// Every offset remains in source order, before a mutation's RHS.
    fn extend_storage_place(
        &self,
        mut place: CheckedContainerPlace,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        loop_depth: usize,
    ) -> Result<CheckedContainerPlace, CheckStop> {
        let (path, ty, offsets) = self.resolve_storage_path(
            suffixes,
            place.root.ty,
            bindings,
            function,
            loop_depth,
            false,
        )?;
        for step in path.iter().map(CheckedPlaceStep::place_step) {
            place.resolved.append_step(step);
        }
        place.root.path.extend(path);
        place.root.ty = ty;
        place.offsets.effects = place.offsets.effects.union(offsets.effects);
        place.offsets.accesses.extend(offsets.accesses);
        Ok(place)
    }

    fn check_storage_read(
        &self,
        node: NodeId,
        source_place: NodeId,
        place: CheckedContainerPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        let mut liveness = Ok(());
        for member in &place.resolved.members {
            if liveness.is_ok() {
                liveness = self.check_commit_place_live(member, node, false);
            }
        }
        let copy = self.is_copy_type(place.root.ty)?;
        let read_out = liveness.is_ok()
            && options.explicit_move
            && !copy
            && self.take_commit_element_read_out(&place.resolved.identity);
        // [WIN-3] "A move out of a window slot or an array element is a hard
        // error citing WIN-3 at that `place`, with the restructuring `use
        // take_back, remove_at, or swap [OP-10, OP-11]`." [DIAG-1] gives the
        // event to that rule even when the same use is also dead under
        // [OWN-1]: WIN-3 is the rule the spelling violates, and liveness
        // still prevents spending another read-out and owns later scalar
        // reads, which move no element at all.
        if options.explicit_move && !copy && !read_out {
            return self.issue_node(
                SemanticRule::Win3,
                source_place,
                SemanticIssueKind::MoveOutOfSlot {
                    mechanical_fix: WIN3_NO_SLOT_MOVE,
                },
            );
        }
        liveness?;
        if !copy && !read_out {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::BareAffineUse {
                    mechanical_fix: "move the element out with take_back or remove_at, or \
                                     exchange it with swap [OP-10, OP-11]",
                },
            );
        }
        if copy && options.explicit_move && self.judges_class_spelling() {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: "use the indexed copy place without `move`",
                },
            );
        }
        let mut effects = place.offsets.effects;
        for member in &place.resolved.members {
            for path in self.effect_paths_for_place(node, member, bindings)? {
                effects.add_read(path);
            }
        }
        let mut accesses = place
            .offsets
            .accesses
            .into_iter()
            .map(PlaceAccess::operand)
            .collect::<Vec<_>>();
        accesses.extend(place.resolved.members.into_iter().map(|place| PlaceAccess {
            place,
            selected: true,
        }));
        let expression = CheckedExpression::ReadStorage {
            carrier: self.tree.path(node)?.clone(),
            root: place.root,
        };
        self.refuse_unrepresented_term_place(&expression, node)?;
        Ok(TypedExpression {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses,
        })
    }

    /// [ENT-2, DIAG-1] a measure read or a read whose final step selects a
    /// readonly field below a subscript is a clause (b) term when every
    /// offset is a tracked place or a constant. This compiler captures only
    /// literals, consts and bare bindings, so a tracked-place offset with
    /// projections is a term it cannot represent. Reading the place anyway as
    /// no term could turn the missing fact into a source rejection later, so
    /// the read is reported as the compiler capability it is.
    fn refuse_unrepresented_term_place(
        &self,
        expression: &CheckedExpression,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        let term = match expression {
            CheckedExpression::ContainerMeasure { root, .. } => root.subscripted_term(),
            CheckedExpression::ReadStorage { root, .. } => root.readonly_field_term(&self.nominals),
            CheckedExpression::RangeIndex { place, .. } => {
                place.readonly_field_term(&self.nominals)
            }
            _ => None,
        };
        if term == Some(SubscriptedTerm::Unrepresented) {
            return self.unsupported(UnsupportedSemanticFeature::CompositeValues, node);
        }
        Ok(())
    }

    /// [SET-1] whether this subscript read is the read-out of an element
    /// target of the `set` whose right-hand side is being checked.
    ///
    /// The offset is read here before the ordinary judgment below reaches it,
    /// so that the admission is decided from the same written offset the
    /// target carried. An offset that does not check, or that this rule
    /// cannot decide, matches nothing and leaves every diagnostic below in
    /// its own place: no read-out is recorded and the affine rejection stands.
    fn element_read_out(
        &self,
        function: &FunctionSignature,
        indexed: &CheckedIndexedPlace,
        suffix: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<bool, CheckStop> {
        let Some(mut place) = indexed.indexed_base_place() else {
            return Ok(false);
        };
        let Some(offset_node) = self.subscript_offset(suffix)? else {
            return Ok(false);
        };
        let mut probe = bindings.clone();
        let Ok(offset) = self.check_atom(function, offset_node, &mut probe, loop_depth) else {
            return Ok(false);
        };
        if offset.expression.ty() != CheckedType::Integer(IntegerType::U64)
            || offset.mode != CheckedMode::Own
        {
            return Ok(false);
        }
        place.push_subscript(
            Self::captured_of(offset_node, &offset.expression).unwrap_or(CapturedValue::unknown()),
        );
        Ok(self.take_commit_element_read_out(&place))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_index_use(
        &self,
        function: &FunctionSignature,
        use_node: NodeId,
        place: NodeId,
        suffixes: &[NodeId],
        subscript: usize,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        let suffix = suffixes[subscript];
        let indexed = self
            .check_indexed_place(
                place,
                bindings,
                &suffixes[..subscript],
                suffix,
                function,
                options.loop_depth,
            )?
            .into_element_storage()?;
        if let CheckedIndexedPlace::Container(container) = indexed {
            let container = self.extend_storage_place(
                container,
                &suffixes[subscript..],
                bindings,
                function,
                options.loop_depth,
            )?;
            return self.check_storage_read(use_node, place, container, bindings, options);
        }
        let element_type = indexed.element_type(self)?;
        let (range_path, selected_type, carried) =
            if matches!(indexed, CheckedIndexedPlace::Range(_)) {
                self.resolve_storage_path(
                    &suffixes[subscript + 1..],
                    element_type,
                    bindings,
                    function,
                    options.loop_depth,
                    true,
                )?
            } else if subscript + 1 == suffixes.len() {
                (Vec::new(), element_type, CarriedOperands::default())
            } else {
                // General suffix paths are legal source. Even when the legacy
                // flat-buffer representation cannot carry a valid projection,
                // select the source path first so an invalid field is TYPE-5.
                self.resolve_storage_path(
                    &suffixes[subscript + 1..],
                    element_type,
                    bindings,
                    function,
                    options.loop_depth,
                    true,
                )?;
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, place);
            };
        // [SET-1, WIN-1] the one affine element read a subscript admits: a
        // `move P[i]` in the right-hand side of the `set` whose own target is
        // `P[i]`. The element leaves through the read-out and the same
        // statement's commit reinitialises the slot at one commit, so no
        // program point sees the slot empty and no second owner is minted —
        // which is exactly the ground [SET-2]'s exchange stands on. Every
        // other affine subscript read is the rejection below.
        let element_read_out = range_path.is_empty()
            && options.explicit_move
            && !self.is_copy_type(selected_type)?
            && self.element_read_out(function, &indexed, suffix, bindings, options.loop_depth)?;
        // [TYPE-2] affine elements leave and enter their slots only through
        // [SET-2] replacement and are read in place through borrowed match:
        // a subscript read would mint a second owner of the stored value, so
        // both the bare and the `move` spelling reject here.
        if !element_read_out && !self.is_copy_type(selected_type)? {
            if options.explicit_move {
                // [WIN-3] there is no take operation and no hole: the move
                // out of the slot is refused at the place, and the three
                // operations that move a boundary are what a program writes
                // instead.
                return self.issue_node(
                    SemanticRule::Win3,
                    place,
                    SemanticIssueKind::MoveOutOfSlot {
                        mechanical_fix: WIN3_NO_SLOT_MOVE,
                    },
                );
            }
            return self.issue_node(
                SemanticRule::Own1,
                use_node,
                SemanticIssueKind::BareAffineUse {
                    mechanical_fix: "move the element out with take_back or remove_at, or \
                                     exchange it with swap [OP-10, OP-11]",
                },
            );
        }
        if !element_read_out && options.explicit_move && self.judges_class_spelling() {
            return self.issue_node(
                SemanticRule::Own1,
                use_node,
                SemanticIssueKind::MoveOfCopy {
                    mechanical_fix: "use the indexed copy place without `move`",
                },
            );
        }
        if matches!(indexed, CheckedIndexedPlace::Container(_)) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let offset_node = self
            .subscript_offset(suffix)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let offset = self.check_atom(function, offset_node, bindings, options.loop_depth)?;
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
        // A subscript is not an [EFF-2] trap source: an accepted subscript
        // is discharged [OP-4] and executes no runtime check. Retain only the
        // psuffix identity that the [ENT-6] obligation judgment and [OP-4]
        // rejection cite.
        let obligation = self.tree.path(suffix)?.clone();
        let captured =
            Self::captured_of(offset_node, &offset.expression).unwrap_or(CapturedValue::unknown());
        let mut effects = offset.effects.union(carried.effects);
        let mut accesses = offset
            .accesses
            .into_iter()
            .map(PlaceAccess::operand)
            .collect::<Vec<_>>();
        accesses.extend(carried.accesses.into_iter().map(PlaceAccess::operand));
        match &indexed {
            CheckedIndexedPlace::Array(array) => {
                if let Some(place) = array.resolved_place() {
                    accesses.push(PlaceAccess {
                        place,
                        selected: true,
                    });
                }
            }
            CheckedIndexedPlace::Buffer(buffer) => accesses.extend(
                buffer
                    .resolved
                    .members
                    .iter()
                    .cloned()
                    .map(|place| PlaceAccess {
                        place,
                        selected: true,
                    }),
            ),
            CheckedIndexedPlace::Range(range) => {
                accesses.extend(range.resolved.members.iter().cloned().map(|mut place| {
                    place.path.push(PlaceStep::Index(captured));
                    place
                        .path
                        .extend(range_path.iter().map(CheckedPlaceStep::place_step));
                    PlaceAccess {
                        place,
                        selected: true,
                    }
                }))
            }
            CheckedIndexedPlace::Container(_) => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        }
        let expression = match indexed {
            CheckedIndexedPlace::Array(array) => CheckedExpression::ArrayIndex {
                carrier: self.tree.path(use_node)?.clone(),
                root: array.root,
                element_type: array.element_type,
                length: array.length,
                offset: Box::new(offset.expression),
                obligation,
                target_domain: CheckedTargetDomainObligation::ElementAddress,
            },
            CheckedIndexedPlace::Buffer(buffer) => {
                for member in &buffer.resolved.members {
                    for path in self.effect_paths_for_place(use_node, member, bindings)? {
                        effects.add_read(path);
                    }
                }
                CheckedExpression::BufferIndex {
                    carrier: self.tree.path(use_node)?.clone(),
                    root: buffer.root,
                    offset: Box::new(offset.expression),
                    obligation,
                    target_domain: CheckedTargetDomainObligation::ElementAddress,
                }
            }
            CheckedIndexedPlace::Range(range) => {
                for member in &range.resolved.members {
                    let mut member = member.clone();
                    member.path.push(PlaceStep::Index(captured));
                    member
                        .path
                        .extend(range_path.iter().map(CheckedPlaceStep::place_step));
                    for path in self.effect_paths_for_place(use_node, &member, bindings)? {
                        effects.add_read(path);
                    }
                }
                CheckedExpression::RangeIndex {
                    carrier: self.tree.path(use_node)?.clone(),
                    place: Box::new(CheckedRangeElementPlace {
                        root: range.root,
                        offset: offset.expression,
                        captured,
                        path: range_path,
                        ty: selected_type,
                        obligation,
                        target_domain: CheckedTargetDomainObligation::ElementAddress,
                    }),
                }
            }
            CheckedIndexedPlace::Container(_) => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        };
        self.refuse_unrepresented_term_place(&expression, use_node)?;
        Ok(TypedExpression {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::check) fn check_indexed_set_target(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        suffixes: &[NodeId],
        subscript: usize,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<MutationTarget, CheckStop> {
        let suffix = suffixes[subscript];
        let indexed = self
            .check_indexed_place(
                node,
                bindings,
                &suffixes[..subscript],
                suffix,
                function,
                loop_depth,
            )?
            .into_element_storage()?;
        // [CONST-2] a named const is immutable, program-lifetime, read-only
        // static storage and is never writable. A `&` range reference formed
        // over a const table names exactly that storage [CONST-1, REF-4], so
        // a write through the reference is refused at the target place, and
        // by this rule rather than by any rule about the reference.
        if let CheckedIndexedPlace::Range(range) = &indexed
            && range
                .resolved
                .members
                .iter()
                .any(|place| matches!(place.root, PlaceRoot::Constant(_)))
        {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::ImmutableSetTarget,
            );
        }
        if let CheckedIndexedPlace::Container(container) = indexed {
            if container.root.binding().is_none() {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::ImmutableSetTarget,
                );
            }
            let container = self.extend_storage_place(
                container,
                &suffixes[subscript..],
                bindings,
                function,
                loop_depth,
            )?;
            for member in &container.resolved.members {
                self.reject_unwritable_reference_target(function, node, member, bindings)?;
            }
            self.check_mutation_target_class(node, container.root.ty)?;
            let mut effects = container.offsets.effects;
            for member in &container.resolved.members {
                for path in self.effect_paths_for_place(node, member, bindings)? {
                    effects.add_write(path);
                }
            }
            let Some(declaration) = container.declaration else {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::ImmutableSetTarget,
                );
            };
            return Ok(MutationTarget {
                declaration,
                place: container.resolved,
                through_reference: None,
                element: true,
                target: CheckedSetTarget::Storage(container.root),
                effects,
                unsupported: None,
            });
        }
        let element_type = indexed.element_type(self)?;
        let (range_path, selected_type, carried) =
            if matches!(indexed, CheckedIndexedPlace::Range(_)) {
                self.resolve_storage_path(
                    &suffixes[subscript + 1..],
                    element_type,
                    bindings,
                    function,
                    loop_depth,
                    true,
                )?
            } else if subscript + 1 == suffixes.len() {
                (Vec::new(), element_type, CarriedOperands::default())
            } else {
                self.resolve_storage_path(
                    &suffixes[subscript + 1..],
                    element_type,
                    bindings,
                    function,
                    loop_depth,
                    true,
                )?;
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, node);
            };
        if matches!(indexed, CheckedIndexedPlace::Container(_)) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let offset_node = self
            .subscript_offset(suffix)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let offset = self.check_atom(function, offset_node, bindings, loop_depth)?;
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
        // As in the read path, retain only the psuffix identity for [ENT-6];
        // an accepted target contributes no runtime check or trap carrier.
        let obligation = self.tree.path(suffix)?.clone();
        // [SET-1]/[SET-2] partition the selected element class exactly as
        // they partition every other final selected type.
        self.check_mutation_target_class(node, selected_type)?;
        let offset_place =
            Self::captured_of(offset_node, &offset.expression).unwrap_or(CapturedValue::unknown());
        let mut effects = offset.effects.union(carried.effects);
        let (declaration, place, target) = match indexed {
            CheckedIndexedPlace::Array(_) => {
                // Binding-rooted arrays took the shared storage path above;
                // only immutable constant arrays reach this dispatch.
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::ImmutableSetTarget,
                );
            }
            CheckedIndexedPlace::Buffer(_) => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            CheckedIndexedPlace::Range(range) => {
                for member in &range.resolved.members {
                    let mut member = member.clone();
                    member.path.push(PlaceStep::Index(offset_place));
                    member
                        .path
                        .extend(range_path.iter().map(CheckedPlaceStep::place_step));
                    for path in self.effect_paths_for_place(node, &member, bindings)? {
                        effects.add_write(path);
                    }
                }
                (
                    range.declaration,
                    range.resolved.clone(),
                    CheckedSetTarget::RangeIndex(Box::new(CheckedRangeElementPlace {
                        root: range.root,
                        offset: offset.expression,
                        captured: offset_place,
                        path: range_path.clone(),
                        ty: selected_type,
                        obligation,
                        target_domain: CheckedTargetDomainObligation::ElementAddress,
                    })),
                )
            }
            CheckedIndexedPlace::Container(_) => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        };
        // [MSR-2, SET-1] a subscript target writes one element of `place`,
        // never the run's own storage, so disjointness and the measure kill
        // both read the element flag rather than the place alone.
        let mut place = place;
        place.append_step(PlaceStep::Index(offset_place));
        for step in range_path.iter().map(CheckedPlaceStep::place_step) {
            place.append_step(step);
        }
        for member in &place.members {
            self.reject_unwritable_reference_target(function, node, member, bindings)?;
        }
        Ok(MutationTarget {
            declaration,
            place,
            through_reference: None,
            element: true,
            target,
            effects,
            unsupported: None,
        })
    }

    /// [SET-1] the element-target half of the writability question.
    ///
    /// [SET-1] makes a target writable when it "is `deref(p)` or a path below
    /// it where `p` is a reference parameter whose declared row carries
    /// `writes` of that path [EFF-1, EFF-5]". An index and a field inherit
    /// the writability of their selected base, so an element target reached
    /// through a reference asks the same question of its own complete path,
    /// and a row that declares only `reads` of it supplies no write
    /// authority. [EFF-2] states the same boundary from the other side --
    /// "a write is admitted only where ordinary ownership already admits it
    /// [SET-1]" -- so this refusal precedes the row comparison rather than
    /// following it.
    fn reject_unwritable_reference_target(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        self.reject_readonly_resolved_write(node, place, bindings)?;
        let PlaceRoot::Binding(binding) = place.root else {
            return Ok(());
        };
        if !bindings
            .values()
            .any(|local| local.binding == binding && local.mode.is_reference())
        {
            return Ok(());
        }
        if self.reference_row_writes(function, place, bindings)? {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Set1,
            node,
            SemanticIssueKind::InvalidSetTarget {
                root_class: "a reference whose declared row does not write this path".to_owned(),
                required_classes: super::SET1_WRITABLE_ROOTS,
            },
        )
    }

    /// Resolves a storage place's suffixes into typed field selections and
    /// subscripts, in written order [MSR-1, OWN-7]. Measures and addressed
    /// borrows use the same path and owe the same subscript obligations.
    ///
    /// `len_of(table[i])` is a term, so a measured place is not a field path.
    /// A subscript inside one is an [OP-4] occurrence like every other: it
    /// selects the base's [WIN-1] element and owes `i < len_of(base)`, which
    /// is submitted where the place is formed [MSR-4]. Its offset must be a
    /// term the place relations can name — [OWN-7] decides two subscripts by
    /// their offsets and [ENT-5] takes each offset's own support into every
    /// enclosing measure — so a written literal, a live `own u64` binding and
    /// an in-scope const generic [MSR-6] are admitted and every other offset
    /// is the explicit unsupported capability when a tracked identity is
    /// required. Ordinary evaluated read and write offsets may instead stay
    /// opaque: their expressions still owe bounds, and establish no equal or
    /// distinct place identity.
    pub(in crate::semantic::check) fn resolve_storage_path(
        &self,
        suffixes: &[NodeId],
        mut ty: CheckedType,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        loop_depth: usize,
        require_named_offsets: bool,
    ) -> Result<(Vec<CheckedPlaceStep>, CheckedType, CarriedOperands), CheckStop> {
        let mut path = Vec::new();
        let mut carried = CarriedOperands::default();
        for (position, &suffix) in suffixes.iter().enumerate() {
            let Some(offset_node) = self.subscript_offset(suffix)? else {
                // [TYPE-9] a `Box`'s content is its one member `inner`, and
                // the storage below that member is the box's referent, so
                // this step is the dereference the resolved path already
                // records rather than a field selection.
                if let CheckedType::Nominal(nominal) = ty
                    && let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind
                {
                    let name = self
                        .deferred_use_at(suffix, crate::DeferredUseRole::ProjectedField)?
                        .spelling()
                        .to_owned();
                    if name != "inner" {
                        return self.issue_node(
                            SemanticRule::Type9,
                            suffix,
                            SemanticIssueKind::type_mismatch(
                                "the Box content field `inner`",
                                format!("the field name `{name}`, which a Box does not declare"),
                            ),
                        );
                    }
                    path.push(CheckedPlaceStep::BoxReferent(nominal));
                    ty = referent;
                    continue;
                }
                let (fields, selected) =
                    self.resolve_struct_path(&suffixes[position..=position], ty)?;
                path.extend(fields.into_iter().map(CheckedPlaceStep::Field));
                ty = selected;
                continue;
            };
            // [OP-4] each suffix selects the complete element type of its
            // already-typed base. Array storage can be nested in a run slot.
            let element_type = match ty {
                CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                    self.element_type(element)?
                }
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
            let captured = Self::captured_of(offset_node, &offset.expression);
            if require_named_offsets && captured.is_none() {
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, offset_node);
            }
            let captured = captured.unwrap_or(CapturedValue::unknown());
            carried.effects = carried.effects.union(offset.effects);
            carried.accesses.extend(offset.accesses);
            path.push(CheckedPlaceStep::Subscript(Box::new(
                CheckedPlaceSubscript {
                    base_type: ty,
                    element_type,
                    offset: offset.expression,
                    obligation: self.tree.path(suffix)?.clone(),
                    target_domain: CheckedTargetDomainObligation::ElementAddress,
                    captured,
                },
            )));
            ty = element_type;
        }
        Ok((path, ty, carried))
    }

    /// The immutable value one index expression produced at this occurrence
    /// [REF-1, OWN-7].
    ///
    /// `occurrence` is the source node the offset was evaluated at, which is
    /// what names the captured value: [REF-1] fixes an index inside a path at
    /// formation, and two steps carrying one occurrence hold one value
    /// whatever the program did between the two places' formation. The term
    /// beside it is how the entailment fragment reads that value, classified
    /// over the checked operand and never over its spelling: a literal or
    /// named integer const is its mathematical value, a binding read is that
    /// binding, and a const generic is fixed at instantiation [FN-2].
    pub(in crate::semantic::check) fn captured_of(
        occurrence: NodeId,
        offset: &CheckedExpression,
    ) -> Option<CapturedValue> {
        let capture = CaptureId::source(u32::try_from(occurrence.index()).unwrap_or(u32::MAX - 1));
        match offset {
            CheckedExpression::Constant(super::super::super::model::CheckedValue::Integer {
                bits,
                ..
            }) => Some(CapturedValue::new(capture, CapturedTerm::Literal(*bits))),
            CheckedExpression::NamedConstant {
                value: super::super::super::model::CheckedValue::Integer { bits, .. },
                ..
            } => Some(CapturedValue::new(capture, CapturedTerm::Literal(*bits))),
            CheckedExpression::Constant(
                super::super::super::model::CheckedValue::ConstGeneric { declaration, .. },
            ) => Some(CapturedValue::new(
                capture,
                CapturedTerm::Const(*declaration),
            )),
            CheckedExpression::Binding {
                binding,
                consume_root: false,
                ..
            } => Some(CapturedValue::new(capture, CapturedTerm::Binding(*binding))),
            _ => None,
        }
    }

    /// The immutable value one range endpoint produced at this occurrence
    /// [REF-1, REF-4].
    ///
    /// A formation's two endpoints are that formation's own values, and
    /// [OWN-7] separates two ranges by exactly those values; the entailment
    /// flow files the formation's endpoint images under its start capture.
    /// An endpoint that no place relation can name (a field, a measure, an
    /// element read) is therefore opaque but keeps the identity of the
    /// occurrence that evaluated it, like every other endpoint, rather than a
    /// marker two formations would share.
    pub(in crate::semantic::check) fn captured_endpoint_of(
        occurrence: NodeId,
        endpoint: &CheckedExpression,
    ) -> Result<CapturedValue, CheckStop> {
        let capture = CaptureId::source(
            u32::try_from(occurrence.index())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let term = Self::captured_of(occurrence, endpoint)
            .map_or(CapturedTerm::Opaque, |captured| captured.term);
        Ok(CapturedValue::new(capture, term))
    }

    pub(in crate::semantic::check) fn check_indexed_atom_place(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        loop_depth: usize,
    ) -> Result<CheckedIndexedPlace, CheckStop> {
        if self.has_fixed(node, FixedTerminal::Move)? {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a place, which a subscript indexes",
                    "a written `move`, which consumes rather than indexes",
                ),
            );
        }
        let place = self
            .tree
            .first_child_with(node, Production::Place)?
            .ok_or_else(|| {
                self.issue_value(
                    SemanticRule::Type5,
                    node,
                    SemanticIssueKind::type_mismatch(
                        "a place, which a subscript indexes",
                        "an atom that is not a place",
                    ),
                )
            })?;
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        self.check_indexed_place(place, bindings, &suffixes, place, function, loop_depth)
    }

    /// One indexable place written through an explicit `deref` [TYPE-7].
    ///
    /// The `deref` names a reference's referent, so the place is resolved by
    /// the ordinary [REF-1] walk and the written suffixes continue it. The
    /// v0.59 companion of this function also had to answer for a view
    /// descriptor reached through a holder; views are gone, so one indexable
    /// container place is the whole answer.
    fn check_dereferenced_indexed_place(
        &self,
        node: NodeId,
        pbase: NodeId,
        base_suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        function: &FunctionSignature,
        loop_depth: usize,
    ) -> Result<CheckedIndexedPlace, CheckStop> {
        let inner = self
            .tree
            .first_child_with(pbase, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let inner = self.resolve_explicit_place(node, inner, bindings)?;
        let mut place = self.resolve_explicit_dereference(node, pbase, inner, bindings)?;
        // [REF-4, OP-4] the run a range reference names is an indexable base
        // reached through `deref` [TYPE-7]. [TYPE-8] makes `&[T]` a reference
        // kind rather than a type, so the referent selects the element type
        // and the row [MSR-1] gives the range is carried by this place.
        if place.range_referent {
            if !base_suffixes.is_empty() {
                // A range reference is never stored in an aggregate [REF-3],
                // so no field step reaches one and none descends from it
                // before the subscript.
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, node);
            }
            let (binding, path) = self.explicit_container_path(&place.expression, node)?;
            if !path.is_empty() {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            let element = self.intern_element(place.ty)?;
            return Ok(CheckedIndexedPlace::Range(CheckedRangePlace {
                root: CheckedRangeRoot {
                    binding,
                    element,
                    element_type: place.ty,
                },
                declaration: place.declaration,
                element_type: place.ty,
                resolved: place.resolved,
            }));
        }
        let (binding, mut path) = self.explicit_container_path(&place.expression, node)?;
        let (suffix_path, ty, offsets) = self.resolve_storage_path(
            base_suffixes,
            place.ty,
            bindings,
            function,
            loop_depth,
            true,
        )?;
        for step in suffix_path.iter().map(CheckedPlaceStep::place_step) {
            place.resolved.append_step(step);
        }
        path.extend(suffix_path);
        match ty {
            CheckedType::Buffer { element } => {
                Ok(CheckedIndexedPlace::Buffer(CheckedBufferPlace {
                    root: CheckedBufferRoot {
                        binding,
                        path: path.clone(),
                        element,
                        element_type: self.element_type(element)?,
                    },
                    declaration: place.declaration,
                    element_type: self.element_type(element)?,
                    resolved: place.resolved,
                    offsets,
                }))
            }
            // [OP-4] the indexable bases, reached through `deref` exactly as
            // an inline one is: a run is one measured place wherever it is
            // reached from [MSR-1].
            CheckedType::Array { .. } | CheckedType::Window { .. } => {
                Ok(CheckedIndexedPlace::Container(CheckedContainerPlace {
                    root: CheckedContainerRoot {
                        root: PlaceRoot::Binding(binding),
                        path,
                        ty,
                    },
                    resolved: place.resolved,
                    declaration: Some(place.declaration),
                    offsets,
                }))
            }
            _ => self.issue_node(
                SemanticRule::Op4,
                node,
                SemanticIssueKind::type_mismatch("an indexable base", self.checked_type_name(ty)?),
            ),
        }
    }

    /// The binding and typed storage path one checked place expression
    /// selects, for a target or read the lowering addresses directly.
    pub(in crate::semantic::check) fn explicit_container_path(
        &self,
        expression: &CheckedExpression,
        node: NodeId,
    ) -> Result<(crate::semantic::model::BindingId, Vec<CheckedPlaceStep>), CheckStop> {
        match expression {
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => Ok((*binding, Vec::new())),
            CheckedExpression::BoxDeref { nominal, value, .. } => {
                let (binding, mut path) = self.explicit_container_path(value, node)?;
                path.push(CheckedPlaceStep::BoxReferent(*nominal));
                Ok((binding, path))
            }
            CheckedExpression::ProjectValue { value, field, .. } => {
                let (binding, mut path) = self.explicit_container_path(value, node)?;
                path.push(CheckedPlaceStep::Field(*field));
                Ok((binding, path))
            }
            _ => self.unsupported(UnsupportedSemanticFeature::CompositeValues, node),
        }
    }

    /// Checks "pbase plus the given suffix run" as one place of indexable
    /// storage. A subscript passes the chain before its own `psuffix` and
    /// anchors its wrong-base judgment there [OP-4]; a `len` or `slice_of`
    /// operand passes the complete chain and anchors at the place node.
    pub(in crate::semantic::check) fn check_indexed_place(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        base_suffixes: &[NodeId],
        anchor: NodeId,
        function: &FunctionSignature,
        loop_depth: usize,
    ) -> Result<CheckedIndexedPlace, CheckStop> {
        self.check_indexed_place_rooted(
            node,
            bindings,
            base_suffixes,
            anchor,
            function,
            loop_depth,
            LexicalUseRole::PlaceBase,
        )
    }

    /// The same walk with the root's lexical role named.
    ///
    /// An `affine_factor` names its measure place in a proof position, whose
    /// root carries that position's own use role [INV-1, PRF-1]; every other
    /// caller is an ordinary place base [GRAM-5].
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::check) fn check_indexed_place_rooted(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        base_suffixes: &[NodeId],
        anchor: NodeId,
        function: &FunctionSignature,
        loop_depth: usize,
        root_role: LexicalUseRole,
    ) -> Result<CheckedIndexedPlace, CheckStop> {
        let pbase = self
            .tree
            .first_child_with(node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if self.has_fixed(pbase, FixedTerminal::Deref)? {
            return self.check_dereferenced_indexed_place(
                node,
                pbase,
                base_suffixes,
                bindings,
                function,
                loop_depth,
            );
        }
        if !self.tree.children(pbase)?.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let usage = self.use_at(pbase, root_role)?;
        let ResolvedTarget::Source { declaration, class } = usage.target() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let (root, binding, declaration, path, ty, offsets) = match class {
            DeclarationClass::Value => {
                let local = bindings
                    .get(&declaration)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if !local.live {
                    return self.issue_node(
                        SemanticRule::Own1,
                        node,
                        SemanticIssueKind::UseAfterMove {
                            mechanical_fix: "introduce a new `let` binding before reuse",
                        },
                    );
                }
                let (path, ty, offsets) = self.resolve_storage_path(
                    base_suffixes,
                    local.ty,
                    bindings,
                    function,
                    loop_depth,
                    true,
                )?;
                // A borrow holder written where its indexable referent is
                // required is the [TYPE-7] implicit read; a borrow of
                // something no `index` could reach falls through to the
                // operand's own mismatch below.
                if local.mode != CheckedMode::Own
                    && self.reads_implicitly_through_holder(
                        true,
                        ty,
                        RequiredReferent::IndexableStorage,
                    )?
                {
                    return self.issue_node(
                        SemanticRule::Type7,
                        node,
                        SemanticIssueKind::MissingDereference {
                            mechanical_fix: "write `deref(holder)`",
                        },
                    );
                }
                (
                    CheckedArrayRoot::Binding {
                        binding: local.binding,
                        fields: Vec::new(),
                    },
                    Some(local.binding),
                    Some(declaration),
                    path,
                    ty,
                    offsets,
                )
            }
            DeclarationClass::NamedConst => {
                if !base_suffixes.is_empty() {
                    let constant = *self
                        .constants
                        .get(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let place = self.constant_storage_place(
                        constant,
                        base_suffixes,
                        bindings,
                        function,
                        loop_depth,
                        true,
                    )?;
                    if place.root.measured().is_none() {
                        return self.issue_node(
                            SemanticRule::Type5,
                            anchor,
                            SemanticIssueKind::type_mismatch(
                                "an array, buffer, or slice place",
                                self.checked_type_name(place.root.ty)?,
                            ),
                        );
                    }
                    return Ok(CheckedIndexedPlace::Container(place));
                }
                let id = *self
                    .constants
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                (
                    CheckedArrayRoot::Constant(id),
                    None,
                    None,
                    Vec::new(),
                    self.constant(id)?.ty,
                    CarriedOperands::default(),
                )
            }
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        // Field-only fixed-array roots retain the compact root form;
        // nested subscripts use the complete typed storage path.
        let fields = field_prefix(&path);
        match ty {
            CheckedType::Array { element, length } if fields.is_some() => {
                let fields = fields.ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let root = match root {
                    CheckedArrayRoot::Binding { binding, .. } => {
                        CheckedArrayRoot::Binding { binding, fields }
                    }
                    CheckedArrayRoot::Constant(id) => {
                        if !fields.is_empty() {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        CheckedArrayRoot::Constant(id)
                    }
                };
                Ok(CheckedIndexedPlace::Array(CheckedArrayPlace {
                    root,
                    declaration,
                    array_type: ty,
                    element_type: self.element_type(element)?,
                    length,
                }))
            }
            CheckedType::Buffer { element } => {
                let (Some(binding), Some(declaration)) = (binding, declaration) else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let root = CheckedBufferRoot {
                    binding,
                    path: path.clone(),
                    element,
                    element_type: self.element_type(element)?,
                };
                let resolved = ResolvedPlace::from_path(binding, root.place_path());
                Ok(CheckedIndexedPlace::Buffer(CheckedBufferPlace {
                    root,
                    declaration,
                    element_type: self.element_type(element)?,
                    resolved: ResolvedPlaceSet::one(resolved),
                    offsets,
                }))
            }
            // [TYPE-7] a `Box` is not a reference, so no implicit read and no
            // `deref(.)` fix is at issue here: the cell is simply not one of
            // [OP-4]'s indexable bases, and its content is the ordinary field
            // step `b.inner` [TYPE-9]. The refusal is therefore [OP-4]'s
            // non-indexable base.
            _ if self.reads_implicitly_through_holder(
                false,
                ty,
                RequiredReferent::IndexableStorage,
            )? =>
            {
                self.issue_node(
                    SemanticRule::Op4,
                    anchor,
                    SemanticIssueKind::type_mismatch(
                        "an indexable base",
                        self.checked_type_name(ty)?,
                    ),
                )
            }
            // [MSR-1] gives each storage shape a measure-table row and [OP-4]
            // makes it an indexable base.
            CheckedType::Array { .. } | CheckedType::Window { .. } => {
                let (Some(binding), Some(declaration)) = (binding, declaration) else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let resolved_path: Vec<PlaceStep> =
                    path.iter().map(CheckedPlaceStep::place_step).collect();
                Ok(CheckedIndexedPlace::Container(CheckedContainerPlace {
                    root: CheckedContainerRoot {
                        root: PlaceRoot::Binding(binding),
                        path,
                        ty,
                    },
                    resolved: ResolvedPlaceSet::one(ResolvedPlace::from_path(
                        binding,
                        resolved_path,
                    )),
                    declaration: Some(declaration),
                    offsets,
                }))
            }
            _ => self.issue_node(
                SemanticRule::Type5,
                anchor,
                SemanticIssueKind::type_mismatch(
                    "an array, buffer, or slice place",
                    self.checked_type_name(ty)?,
                ),
            ),
        }
    }
}

/// The field selections of a path that carries no subscript, absent where it
/// does.
fn field_prefix(path: &[CheckedPlaceStep]) -> Option<Vec<u32>> {
    path.iter()
        .map(|step| match step {
            CheckedPlaceStep::Field(field) => Some(*field),
            CheckedPlaceStep::BoxReferent(_) | CheckedPlaceStep::Subscript(_) => None,
        })
        .collect()
}

/// The [MSR-1] measure-table row one type selects, if it has one.
///
/// It is the same table [`CheckedMeasure::cell`] reads; this is only the
/// mapping from a checked type to its row, which the clause path needs before
/// it has a place.
pub(in crate::semantic::check) const fn measured_kind_of(
    ty: CheckedType,
) -> Option<super::super::super::model::MeasuredKind> {
    ty.measured()
}

#[cfg(test)]
mod layout_magnitude_tests {
    use super::{
        CheckedLayoutMagnitude, add_layout_magnitude, multiply_layout_magnitude,
        round_up_layout_magnitude,
    };

    #[test]
    fn finite_or_above_u64_preserves_every_layout_ceiling_observation() {
        let finite = CheckedLayoutMagnitude::Finite;
        assert_eq!(multiply_layout_magnitude(finite(8), 3), finite(24));
        assert_eq!(multiply_layout_magnitude(finite(8), 0), finite(0));
        assert_eq!(
            multiply_layout_magnitude(finite(8), u64::MAX),
            CheckedLayoutMagnitude::AboveU64
        );
        assert_eq!(
            multiply_layout_magnitude(CheckedLayoutMagnitude::AboveU64, 0),
            finite(0)
        );
        assert_eq!(round_up_layout_magnitude(finite(9), 8), finite(16));
        assert_eq!(
            round_up_layout_magnitude(finite(u64::MAX), 8),
            CheckedLayoutMagnitude::AboveU64
        );
        assert_eq!(
            add_layout_magnitude(finite(u64::MAX), finite(1)),
            CheckedLayoutMagnitude::AboveU64
        );
        assert_eq!(CheckedLayoutMagnitude::AboveU64.allocation_limit(), 0);
    }
}
