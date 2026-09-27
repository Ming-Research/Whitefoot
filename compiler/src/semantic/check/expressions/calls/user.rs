use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::super::super::goal::{
    EvaluatedValueOccurrence, GoalDatum, GoalExpression, GoalOperation, GoalProjection,
};
use super::super::super::super::model::{
    BindingId, CheckedCallContract, CheckedCallSeparation, CheckedEffectStep, CheckedEffects,
    CheckedExpression, CheckedMode, CheckedNominalKind, CheckedStatePath, CheckedType,
};
use super::super::super::super::places::{
    CaptureId, CapturedRange, CapturedTerm, CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace,
    SeparationOracle, UnprovedSeparations, WindowPart, overlaps_at_every_position, places_overlap,
};
use super::super::super::generics::HEAP_ALLOCATING_PRELUDE_FUNCTIONS;
use super::super::super::references::InvalidationEvent;
use super::super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, TypedExpression,
};

/// One entry of a call's substituted effect row [EFF-5].
///
/// Each declared `effect_path` rooted at reference parameter i takes actual
/// argument i's path, and each IDENT index or range endpoint takes the value
/// its own argument supplies. A by-value argument contributes a consumption
/// (`move`) or a read (copy) of its own place to the same comparison, which
/// is why an entry carries a resolved place and a category and nothing about
/// how it was spelled.
struct SubstitutedEntry {
    place: ResolvedPlace,
    /// Whether this entry writes, replaces, moves out of, or frees the
    /// storage at its path and everything below it [EFF-1].
    write: bool,
    /// Whether this is the consuming contribution of a by-value argument.
    /// Unlike a declared content write, a move also invalidates a reference
    /// naming the argument's exact path [REF-2].
    consuming: bool,
    /// The argument ordinal this entry came from, so the pairwise comparison
    /// can attribute an entry to the parameter that supplied it.
    argument: usize,
    /// The declared effect (or by-value contribution) that produced this
    /// entry. One declared effect can expand to several mutually exclusive
    /// paths when a joined reference is passed; those alternatives are not a
    /// pairwise conflict with each other. Distinct effects still compare when
    /// they came through the same actual argument [EFF-5].
    origin: usize,
    /// How many leading steps of `place` the caller formed: the actual
    /// argument's own path. Every later step is the declared row's suffix,
    /// whose index positions take the values other arguments supply and are
    /// bounded by no obligation at the call [EFF-5, WIN-2].
    formed: usize,
}

/// [EFF-5, WIN-2] the separation oracle of one pair of substituted entries.
///
/// A window part is named only by a row, so the index beside it is the other
/// entry's. An index of an actual's own path was formed at the call and
/// discharged [OP-4] there, or is named by a reference that stays valid only
/// while that bound holds [OP-10], so it is live in the call's entry state.
/// An index a row supplies is the value of another argument, which no
/// obligation at the call bounds by the window's length, so this oracle holds
/// it not live and the pair overlaps; the pairwise comparison then hands the
/// pair to the entailment fragment, which separates it where the call's entry
/// state proves the bound (`CheckedCallSeparationPositions::Live`). Every
/// other question is answered as [`UnprovedSeparations`] answers it, and the
/// index and range pairs its families discharge are handed over the same way.
struct EntryPairSeparations<'entry> {
    entries: [&'entry SubstitutedEntry; 2],
}

impl SeparationOracle for EntryPairSeparations<'_> {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        UnprovedSeparations.indices_distinct(left, right)
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        UnprovedSeparations.ranges_disjoint(left, right)
    }

    fn index_is_live(&self, window: &ResolvedPlace, index: CapturedValue) -> bool {
        let depth = window.path.len();
        self.entries.iter().all(|entry| {
            depth < entry.formed || entry.place.path.get(depth) != Some(&PlaceStep::Index(index))
        })
    }

    fn index_outside_range(&self, index: CapturedValue, range: CapturedRange) -> bool {
        UnprovedSeparations.index_outside_range(index, range)
    }

    /// A range of an actual's own path was formed at the call, discharging
    /// its [REF-4] bound there, or is named by a reference valid only while
    /// that bound holds [OP-10], so it lies within the length of the call's
    /// entry state, as an index of that path is live; a range a row supplies
    /// takes its endpoints from other arguments and is bounded by nothing
    /// until the entailment fragment proves it [EFF-5, WIN-2].
    fn range_within_length(&self, window: &ResolvedPlace, range: CapturedRange) -> bool {
        let depth = window.path.len();
        self.entries.iter().all(|entry| {
            depth < entry.formed || entry.place.path.get(depth) != Some(&PlaceStep::Range(range))
        })
    }

    fn window_length_is_shared(&self, window: &ResolvedPlace) -> bool {
        UnprovedSeparations.window_length_is_shared(window)
    }
}

/// A bound call's row and contract come from the same instantiated formal.
/// Direct calls carry neither override.
struct FormalCallBoundary {
    effects: CheckedEffects,
    contract: CheckedCallContract,
}

impl<'unit, 'classified, 'lexed, 'source> Checker<'unit, 'classified, 'lexed, 'source> {
    pub(super) fn check_user_call(
        &self,
        node: NodeId,
        declaration: DeclarationId,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        // [OP-10, OP-11, OP-14] a window operation, `swap` and `free_empty`
        // write no type arguments at a call: the operand supplies every type
        // parameter, so the instance is selected from the operand's own type
        // here instead of from a written argument list [FN-2].
        let target = match self.operand_directed_function_for_call(node, declaration, bindings)? {
            Some(target) => target,
            None => self.concrete_function_for_call(node, declaration, &function.substitution)?,
        };
        let signature = self
            .signatures
            .get(target.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.check_selected_user_call(node, signature, None, function, bindings, loop_depth)
    }

    pub(in crate::semantic::check) fn check_behavior_call(
        &self,
        node: NodeId,
        key: crate::semantic::check::generics::GenericParameterKey,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let argument = function
            .substitution
            .function_argument(key)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let target = self.function_argument_instance(argument)?;
        let actual = self
            .signatures
            .get(target.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let formal = self.formal_signature(key, &function.substitution, target)?;
        let binding_site = self.behavior_binding_site(node, key, &function.substitution)?;
        let (effective, formal_effects, formal_contract) =
            self.behavior_call_signature(binding_site, Some(function.id), &formal, actual)?;
        self.check_selected_user_call(
            node,
            &effective,
            Some(FormalCallBoundary {
                effects: formal_effects,
                contract: formal_contract,
            }),
            function,
            bindings,
            loop_depth,
        )
    }

    fn check_selected_user_call(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
        formal: Option<FormalCallBoundary>,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let target = signature.id;
        let fields = if let Some(list) = self
            .tree
            .first_child_with(node, Production::FieldinitList)?
        {
            self.tree.children_with(list, Production::Fieldinit)?
        } else {
            Vec::new()
        };
        if self
            .tree
            .first_child_with(node, Production::AtomList)?
            .is_some()
            || fields.len() != signature.parameters.len()
        {
            return self.issue_node(
                SemanticRule::Gram11,
                node,
                Self::invalid_named_arguments(signature),
            );
        }
        let mut arguments = Vec::with_capacity(fields.len());
        let mut argument_nodes = Vec::with_capacity(fields.len());
        let mut goal_arguments = Vec::with_capacity(fields.len());
        // [EFF-5] each actual's resolved path set, in parameter order. The set
        // has more than one member only where the actual is a reference a
        // control-flow join gave more than one path [REF-1]; every check must
        // then hold for every member.
        let mut actual_paths: Vec<Vec<ResolvedPlace>> = Vec::with_capacity(fields.len());
        let mut actual_captures = Vec::with_capacity(fields.len());
        let mut actual_modes = Vec::with_capacity(fields.len());
        let call = self.tree.path(node)?.clone();
        self.prepare_atomic_update_call(&call, signature)?;
        let mut effects = EffectSet::NONE;
        for (ordinal, (field, parameter)) in
            fields.into_iter().zip(&signature.parameters).enumerate()
        {
            if self.identifier(field)? != parameter.name {
                return self.issue_node(
                    SemanticRule::Gram11,
                    field,
                    Self::invalid_named_arguments(signature),
                );
            }
            let atom = self
                .tree
                .first_child_with(field, Production::Atom)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.enter_atomic_update_argument(&call, ordinal);
            let argument = self.check_call_argument_atom(function, atom, bindings, loop_depth)?;
            self.reject_failed_atomic_update_argument(atom)?;
            // [CONST-2, OWN-11, TYPE-2] every possible origin of a written
            // reference argument must be writable. The checked argument
            // retains those paths through aliases and control-flow joins.
            if parameter.mode.is_reference()
                && signature
                    .declared_effects
                    .writes
                    .iter()
                    .any(|entry| entry.root == parameter.declaration)
            {
                self.check_written_reference_argument(atom, &argument, bindings)?;
            }
            // [TYPE-8] `&[T]` is a kind, so a checked range value carries the
            // element type T beside its `Range` mode. Comparing that element
            // against a parameter's type before the kind would report `u8`
            // where the source wrote `&[u8]`, and it would let a `&[T]` at an
            // `own T` parameter fall into the [TYPE-7] implicit read below,
            // whose `deref(.)` fix is wrong here: `deref` of a range names
            // the whole run [TYPE-7, REF-4], never one element. Disagreement
            // about the range kind itself is therefore judged first, and it
            // is [TYPE-5]'s ordinary argument mismatch.
            {
                use super::super::super::super::model::CheckedMode;
                if (argument.mode == CheckedMode::Range) != (parameter.mode == CheckedMode::Range) {
                    return self.issue_node(
                        SemanticRule::Type5,
                        atom,
                        SemanticIssueKind::type_mismatch(
                            self.checked_value_name(parameter.mode, parameter.ty)?,
                            self.checked_value_name(argument.mode, argument.expression.ty())?,
                        ),
                    );
                }
            }
            if argument.expression.ty() != parameter.ty {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        self.checked_type_name(parameter.ty)?,
                        self.checked_type_name(argument.expression.ty())?,
                    ),
                );
            }
            if argument.mode != parameter.mode {
                // [TYPE-7] "A reference binding used where a value of its
                // referent type is expected is a hard error citing TYPE-7,
                // with the mechanical fix `deref(.)`." The operand mode is
                // what says which of the two refusals this is: a reference
                // standing in an `own` position is the implicit read the rule
                // refuses, and every other disagreement is [TYPE-5]'s.
                if argument.mode.is_reference()
                    && parameter.mode == super::super::super::super::model::CheckedMode::Own
                {
                    return self.issue_node(
                        SemanticRule::Type7,
                        atom,
                        SemanticIssueKind::MissingDereference {
                            mechanical_fix: "write `deref(.)`",
                        },
                    );
                }
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        self.checked_value_name(parameter.mode, parameter.ty)?,
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            // [REF-2] a reference handed to a call must be valid at the call.
            if let Some(reference) = &argument.reference
                && !reference.is_valid()
            {
                return self.issue_node(
                    SemanticRule::Ref2,
                    atom,
                    SemanticIssueKind::InvalidReferenceUse {
                        binder: parameter.name.clone(),
                        event: match &reference.validity {
                            super::super::super::references::ReferenceValidity::Invalid(event) => {
                                event.phrase()
                            }
                            super::super::super::references::ReferenceValidity::Valid => "",
                        },
                        mechanical_fix: super::super::super::references::REF2_FORM_AGAIN,
                    },
                );
            }
            let paths = argument
                .reference
                .as_ref()
                .map(|reference| reference.paths.clone())
                .unwrap_or_else(|| {
                    argument
                        .accesses
                        .iter()
                        .map(|access| access.place.clone())
                        .collect()
                });
            let ordinal_index =
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            goal_arguments.push(self.call_goal_argument(
                function.id,
                &call,
                ordinal_index,
                atom,
                parameter.mode,
                parameter.ty,
                &argument,
                match paths.as_slice() {
                    [unique] => Some(unique),
                    _ => None,
                },
                bindings,
            )?);
            argument_nodes.push(self.tree.path(atom)?.clone());
            actual_paths.push(paths);
            actual_captures.push(
                self.note_capture(
                    Self::captured_of(atom, &argument.expression)
                        .unwrap_or_else(CapturedValue::unknown),
                    bindings,
                ),
            );
            actual_modes.push(parameter.mode);
            effects = effects.union(argument.effects);
            arguments.push(argument.expression);
        }
        // [STOR-8] a unit carrying the no-heap declaration cannot call an
        // allocating prelude row; [OP-11] refuses a `swap` over a copy place.
        self.reject_allocating_call_under_no_heap(node, signature)?;
        self.reject_swap_over_copy(node, signature)?;
        // Both forms share the same activation-replacement conditions. A
        // source marker requires them; an ordinary call merely opts out when
        // they fail. Bound calls are not direct self calls even when their
        // concrete target happens to be this function. The return checker
        // completes this selection after deriving the remaining releases.
        let tail_transfer = formal.is_none()
            && target == function.id
            && self.is_sole_return_call(node)?
            && self.check_self_tail_arguments(
                node,
                function,
                bindings,
                &actual_paths,
                &actual_modes,
            )?;
        // [EFF-5] substitute, compare pairwise, then project the surviving
        // footprint onto the caller's own row [EFF-2].
        // [EFF-3] a call inherits its callee's allocation fact.
        if signature.declared_effects.allocates {
            effects.add_allocation();
        }
        let substituted = self.substitute_call_row(
            node,
            signature,
            &actual_paths,
            &actual_captures,
            &actual_modes,
        )?;
        // [OP-12] precedes [EFF-5] here: an atomic update's own refusal is
        // about the callee's row reaching the updated place, and the target
        // argument's own by-value contribution is exactly the overlap
        // [EFF-5] would otherwise report against that row.
        let atomic_target =
            self.check_atomic_update_row(node, &actual_paths, &substituted, bindings)?;
        self.check_call_pairwise_disjointness(node, signature, &substituted, bindings)?;
        self.invalidate_call_references(node, &substituted, atomic_target.as_ref(), bindings)?;
        self.invalidate_window_operation_references(signature, &substituted, bindings)?;
        self.project_call_effects(node, function, &substituted, bindings, &mut effects)?;
        let result = signature.result;
        let result_mode = signature.result_mode;
        let (formal_effects, formal_contract) = match formal {
            Some(boundary) => (
                Some(Box::new(boundary.effects)),
                Some(Box::new(boundary.contract)),
            ),
            None => (None, None),
        };
        Ok(TypedExpression {
            expression: CheckedExpression::UserCall {
                function: target,
                tail_transfer,
                formal_effects,
                formal_contract,
                call,
                argument_nodes,
                arguments,
                actual_captures,
                goal_arguments,
                goal_regions: Vec::new(),
                requirements: Vec::new(),
                result,
                result_borrow: None,
                allocation: self.allocation_fit_of_call(signature)?,
            },
            mode: result_mode,
            // [REF-3] no call delivers a reference: FN-1 returns owned values
            // and the declaration boundary already refused any other result
            // mode, so the result names no path.
            reference: None,
            reference_value: false,
            effects,
            accesses: Vec::new(),
        })
    }

    /// [OP-9, OP-13, OP-10] the static allocation-size obligation this call
    /// carries, if it is one of the operations that carry one.
    ///
    /// [OP-13] gives it to "each runtime-capacity construction" and [OP-10]
    /// to `grow`, "over that operation's own stored type and count". The
    /// three constructions take the count first and name the stored type in
    /// their own result cell; `grow` remakes the cell it is handed, so its
    /// stored type is that cell's and its count is its second argument. The
    /// constant-capacity rows allocate nothing at runtime and the cell row
    /// `box_new` allocates exactly one value, so neither carries the
    /// obligation.
    fn allocation_fit_of_call(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Option<super::super::super::super::model::CheckedAllocationFit>, CheckStop> {
        let (count, cell) = match signature.name.as_str() {
            "box_array_filled" | "box_slots_new" | "box_ring_new" => (0, signature.result),
            "grow" => {
                let Some(parameter) = signature.parameters.first() else {
                    return Ok(None);
                };
                (1, parameter.ty)
            }
            _ => return Ok(None),
        };
        let Some(element) = self.runtime_capacity_content_element(cell)? else {
            return Ok(None);
        };
        let layout_ceiling = match self.instantiated_layout_ceiling(element) {
            Some(ceiling) => ceiling,
            None if self
                .stabilize_substitution_with_visiting(
                    &signature.substitution,
                    0,
                    &mut HashSet::new(),
                    false,
                )?
                .is_none() =>
            {
                // [ENT-1, FN-2] only a layout depending on an unresolved
                // type or const parameter may defer the schema obligation.
                // This includes an opaque parameter inside an aggregate,
                // but not a fixed-layout Box shell or a known AboveU64
                // ceiling. Inspect the operation's substitution recursively:
                // a nominal argument can still contain a schema parameter.
                // No deferred record grants proof or lowering authority;
                // every concrete replay recomputes its bound.
                return Ok(None);
            }
            None => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        Ok(Some(
            super::super::super::super::model::CheckedAllocationFit {
                cell,
                element,
                layout_ceiling,
                count,
                source_length_upper_bound: None,
            },
        ))
    }

    /// The element type of the runtime-capacity shape a `Box` holds [TYPE-9].
    ///
    /// A runtime-capacity `Array<T>`, `Slots<T>` or `Ring<T>` exists only as
    /// the content of its cell, so this is the one place the stored type of
    /// an allocation is found, and a constant-capacity content, which
    /// allocates no slots of its own, has none.
    fn runtime_capacity_content_element(
        &self,
        cell: CheckedType,
    ) -> Result<Option<CheckedType>, CheckStop> {
        let CheckedType::Nominal(nominal) = cell else {
            return Ok(None);
        };
        let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind else {
            return Ok(None);
        };
        Ok(match referent {
            CheckedType::Buffer { element } => Some(self.element_type(element)?),
            CheckedType::Window {
                element,
                capacity: None,
                ..
            } => Some(self.element_type(element)?),
            _ => None,
        })
    }

    /// [EFF-5] the substituted row of one call.
    ///
    /// Each `effect_path` rooted at reference parameter i takes actual
    /// argument i's path and appends its own `epsuffix*`; a by-value argument
    /// contributes a consumption or a read of its place to the same list. An
    /// index or range position names a value parameter of the same callable,
    /// and the value that parameter's argument supplies is what replaces it.
    fn substitute_call_row(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
        actual_paths: &[Vec<ResolvedPlace>],
        actual_captures: &[CapturedValue],
        actual_modes: &[CheckedMode],
    ) -> Result<Vec<SubstitutedEntry>, CheckStop> {
        let mut entries = Vec::new();
        let mut next_origin = 0usize;
        for (write, declared) in [
            (false, &signature.declared_effects.reads),
            (true, &signature.declared_effects.writes),
        ] {
            for formal in declared {
                let origin = next_origin;
                next_origin = next_origin
                    .checked_add(1)
                    .ok_or(SemanticCompilerFailure::CounterOverflow)?;
                let Some(index) = signature
                    .parameters
                    .iter()
                    .position(|parameter| parameter.declaration == formal.root)
                else {
                    // [EFF-1] a row is rooted at a parameter of the same
                    // callable; the declaration boundary already refused
                    // anything else.
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let steps = self.substitute_effect_steps(signature, formal, actual_captures)?;
                for base in actual_paths.get(index).into_iter().flatten() {
                    let mut place = base.clone();
                    place.path.extend_from_slice(&steps);
                    entries.push(SubstitutedEntry {
                        formed: base.path.len(),
                        place,
                        write,
                        consuming: false,
                        argument: index,
                        origin,
                    });
                }
            }
        }
        // [EFF-5] clause 2: a by-value argument contributes a consumption
        // (`move`) or a read (copy) of its place to this same comparison.
        for (index, mode) in actual_modes.iter().enumerate() {
            if *mode != CheckedMode::Own {
                continue;
            }
            let origin = next_origin;
            next_origin = next_origin
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
            for place in actual_paths.get(index).into_iter().flatten() {
                let consuming = self
                    .is_copy_place_type(signature, index)
                    .is_none_or(|copy| !copy);
                entries.push(SubstitutedEntry {
                    formed: place.path.len(),
                    place: place.clone(),
                    // A `move` empties the place, which [EFF-1] classes with
                    // the writes; a copy argument observes it.
                    write: consuming,
                    consuming,
                    argument: index,
                    origin,
                });
            }
        }
        let _ = node;
        Ok(entries)
    }

    /// [EFF-5] the declared row's entries in the callee's own frame, in the
    /// order [`Self::substitute_call_row`] numbers their origins: every
    /// `reads` entry, then every `writes` entry.
    ///
    /// Each reference parameter is its own root and each index or range
    /// position holds the value parameter it names, so two positions are one
    /// value exactly when they name one parameter [EFF-1]. No actual enters:
    /// whether two entries overlap at every position is a property of the
    /// row, the same at every call.
    fn formal_row_places(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<ResolvedPlace>, CheckStop> {
        let captures = (0..signature.parameters.len())
            .map(|ordinal| {
                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| CheckStop::from(SemanticCompilerFailure::CounterOverflow))?;
                Ok(CapturedValue::new(
                    CaptureId::source(ordinal),
                    CapturedTerm::Binding(BindingId(ordinal)),
                ))
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        let declared = &signature.declared_effects;
        let mut places = Vec::with_capacity(declared.reads.len() + declared.writes.len());
        for formal in declared.reads.iter().chain(&declared.writes) {
            let ordinal = signature
                .parameters
                .iter()
                .position(|parameter| parameter.declaration == formal.root)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let ordinal =
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            places.push(ResolvedPlace {
                root: PlaceRoot::Binding(BindingId(ordinal)),
                path: self.substitute_effect_steps(signature, formal, &captures)?,
            });
        }
        Ok(places)
    }

    /// One declared `epsuffix*`, with its index and range positions replaced
    /// by the values their own arguments supply [EFF-5].
    fn substitute_effect_steps(
        &self,
        signature: &FunctionSignature,
        formal: &CheckedStatePath,
        actual_captures: &[CapturedValue],
    ) -> Result<Vec<PlaceStep>, CheckStop> {
        let captured = |parameter: DeclarationId| -> CapturedValue {
            // The index position names a value parameter and therefore takes
            // the checked value its argument supplied. Its storage access is
            // unrelated: `indices[1]` supplies the value loaded there, not
            // the literal one used to address that load [EFF-5].
            signature
                .parameters
                .iter()
                .position(|candidate| candidate.declaration == parameter)
                .and_then(|index| actual_captures.get(index).copied())
                .unwrap_or_else(CapturedValue::unknown)
        };
        Ok(formal
            .steps
            .iter()
            .map(|step| match step {
                CheckedEffectStep::Field(field) => PlaceStep::Field(*field),
                CheckedEffectStep::Deref => PlaceStep::Deref,
                CheckedEffectStep::Payload { variant, field } => PlaceStep::Payload {
                    variant: *variant,
                    field: *field,
                },
                CheckedEffectStep::Index(parameter) => PlaceStep::Index(captured(*parameter)),
                CheckedEffectStep::Range { start, end } => {
                    PlaceStep::Range(super::super::super::super::places::CapturedRange {
                        start: captured(*start),
                        end: captured(*end),
                    })
                }
                CheckedEffectStep::Part(part) => PlaceStep::Part(*part),
                CheckedEffectStep::Measure(measure) => PlaceStep::Measure(*measure),
            })
            .collect())
    }

    /// Whether the by-value parameter at `index` has a copy type, so its
    /// argument contributes a read rather than a consumption [EFF-5].
    fn is_copy_place_type(&self, signature: &FunctionSignature, index: usize) -> Option<bool> {
        let parameter = signature.parameters.get(index)?;
        self.is_copy_type(parameter.ty).ok()
    }

    /// [OP-12] "`f`'s declared row must not write, move out of, or free any
    /// prefix of `p`, while reading anything and writing disjoint storage is
    /// admitted [EFF-5]; a row that does is a hard error citing OP-12 at the
    /// complete `call`, carrying that substituted path."
    ///
    /// The update is recognized from the two halves the two rules hold: the
    /// enclosing `set` knows its target place and has already checked the
    /// callee's result condition [`atomic_update_target`], and this call
    /// knows whether its first argument is that very place. The row's reach
    /// is the substituted row itself, so no formal path is read twice and
    /// nothing here inspects the callee's body.
    fn check_atomic_update_row(
        &self,
        node: NodeId,
        actual_paths: &[Vec<ResolvedPlace>],
        entries: &[SubstitutedEntry],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<ResolvedPlace>, CheckStop> {
        let Some(target) = self.atomic_update_target(self.tree.path(node)?) else {
            return Ok(None);
        };
        // "where the first argument of the call is the target place itself":
        // the target enters `f` by value, so its actual names exactly one
        // place and that place is the target.
        let Some([first]) = actual_paths.first().map(Vec::as_slice) else {
            return Ok(None);
        };
        if *first != target {
            return Ok(None);
        }
        for entry in entries {
            // The target argument's own by-value contribution is the update's
            // own transfer, not a reach of the row; every other argument's
            // write, consume, or free is the row reaching the caller's
            // storage, and a prefix of the target is what [OP-12] refuses.
            if !entry.write
                || entry.argument == 0
                || !entry
                    .place
                    .may_be_prefix_of(&UnprovedSeparations, &target, true)
            {
                continue;
            }
            return self.issue_node(
                SemanticRule::Op12,
                node,
                SemanticIssueKind::AtomicUpdateReachesTargetPrefix {
                    target: self.render_resolved_place(&target, bindings)?,
                    effect: self.render_resolved_place(&entry.place, bindings)?,
                    mechanical_fix: "declare a row that reaches no prefix of the updated place: reading anything and writing storage disjoint from it is admitted, and an update whose callee must reach the place is written as ordinary statements instead",
                },
            );
        }
        Ok(Some(target))
    }

    /// [EFF-5] clause 1: two compared effects on overlapping paths where at
    /// least one is a write must be proved disjoint.
    ///
    /// The checker holds the actual spellings and the live reference state,
    /// so it owns this comparison; what it cannot do is discharge the index
    /// or range goal the fixed [ENT-6] families own, so a pair the syntax
    /// leaves open is recorded for the entailment fragment and refused there
    /// if it stays undischarged. `swap` is the one operation whose two
    /// arguments may name the same place [OP-11].
    ///
    /// Two effects one argument supplies are compared only when their
    /// declared paths may be separated by the values of their positions: a
    /// pair that overlaps at every position is reached through that one
    /// parameter, which the callee's own body is checked against, so the
    /// call has nothing to prove about it. The caller's kills and reference
    /// invalidations still take every substituted write [REF-2, ENT-5].
    fn check_call_pairwise_disjointness(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
        entries: &[SubstitutedEntry],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        let exchange = self.is_swap_row(signature);
        let formal = self.formal_row_places(signature)?;
        for (index, left) in entries.iter().enumerate() {
            for right in entries.iter().skip(index + 1) {
                if left.origin == right.origin || !(left.write || right.write) {
                    continue;
                }
                if left.argument == right.argument
                    && let (Some(left_formal), Some(right_formal)) =
                        (formal.get(left.origin), formal.get(right.origin))
                    && overlaps_at_every_position(left_formal, right_formal)
                {
                    continue;
                }
                let oracle = EntryPairSeparations {
                    entries: [left, right],
                };
                if !places_overlap(&oracle, &left.place, &right.place) {
                    continue;
                }
                if exchange && left.place.exchange_safe(&oracle, &right.place) {
                    continue;
                }
                // A pair whose only unseparated steps are index or range
                // positions, or an index beside a window part, is the fixed
                // families' question; every other overlap is refused here and
                // now.
                if let Some((positions, window)) =
                    Self::separable_by_position(&left.place, &right.place)
                {
                    self.call_separations
                        .borrow_mut()
                        .push(CheckedCallSeparation {
                            site: self.tree.path(node)?.clone(),
                            exchange,
                            reference_use: None,
                            positions,
                            window,
                            left_spelling: self.render_resolved_place(&left.place, bindings)?,
                            right_spelling: self.render_resolved_place(&right.place, bindings)?,
                            one_argument: left.argument == right.argument,
                        });
                    continue;
                }
                return self.issue_node(
                    if exchange {
                        SemanticRule::Op11
                    } else {
                        SemanticRule::Eff5
                    },
                    node,
                    SemanticIssueKind::OverlappingCallEffects {
                        first: self.render_resolved_place(&left.place, bindings)?,
                        second: self.render_resolved_place(&right.place, bindings)?,
                        // No position separates this pair, so proving one
                        // distinct is no repair here [DIAG-1]. One
                        // argument's pair got here because this call gives
                        // the declared positions that tell its entries apart
                        // the same values over one place the argument names,
                        // or because no family this checker poses at a call
                        // separates the steps at which the two paths differ,
                        // as for two places a joined argument may name. Only
                        // the first is repaired at the call's positions.
                        mechanical_fix: if exchange {
                            "exchange equal or disjoint places without an ancestor relation"
                        } else if left.argument == right.argument
                            && left.place.root == right.place.root
                            && left.place.path.get(..left.formed)
                                == right.place.path.get(..right.formed)
                            && let (Some(left_formal), Some(right_formal)) =
                                (formal.get(left.origin), formal.get(right.origin))
                            && Self::separable_by_position(left_formal, right_formal).is_some()
                        {
                            "this call gives these two entries of the callee's row the same positions: pass positions this call proves do not overlap, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
                        } else if left.argument == right.argument {
                            "these two entries of the callee's row may reach overlapping places through one argument, and no position this call passes separates them: replace the callee's row entries at or below their common path with one `writes` entry of that path"
                        } else {
                            "pass places that do not overlap, or pass the shared place through one argument only"
                        },
                    },
                );
            }
        }
        Ok(())
    }

    /// The ordered position disagreements an admitted [OWN-7] family can
    /// still separate. Index suffixes remain candidates; a divergence with a
    /// range step is the final candidate because its coordinate frames then
    /// differ, whether against another range or against an index, and so is
    /// a position beside a window's `next` or `free`, which [WIN-2] separates
    /// by a bound on that window's length, together with the window it reads.
    /// Beside `last` or `filled` a position overlaps whatever its value.
    pub(in crate::semantic::check) fn separable_by_position(
        left: &ResolvedPlace,
        right: &ResolvedPlace,
    ) -> Option<(
        Vec<super::super::super::super::model::CheckedCallSeparationPositions>,
        Option<ResolvedPlace>,
    )> {
        use super::super::super::super::model::CheckedCallSeparationPositions;
        let mut candidates = Vec::new();
        let mut window = None;
        for (depth, steps) in left.path.iter().zip(&right.path).enumerate() {
            match steps {
                (PlaceStep::Index(first), PlaceStep::Index(second))
                    if first.provably_same(*second) =>
                {
                    continue;
                }
                (PlaceStep::Range(first), PlaceStep::Range(second))
                    if first.start.provably_same(second.start)
                        && first.end.provably_same(second.end) =>
                {
                    continue;
                }
                (PlaceStep::Index(first), PlaceStep::Index(second)) => {
                    candidates.push(CheckedCallSeparationPositions::Indices(*first, *second));
                }
                (PlaceStep::Range(first), PlaceStep::Range(second)) => {
                    candidates.push(CheckedCallSeparationPositions::Ranges(*first, *second));
                    break;
                }
                (PlaceStep::Index(index), PlaceStep::Range(range))
                | (PlaceStep::Range(range), PlaceStep::Index(index)) => {
                    candidates.push(CheckedCallSeparationPositions::IndexOutsideRange(
                        *index, *range,
                    ));
                    break;
                }
                (PlaceStep::Index(_) | PlaceStep::Range(_), PlaceStep::Part(part))
                | (PlaceStep::Part(part), PlaceStep::Index(_) | PlaceStep::Range(_))
                    if matches!(part, WindowPart::Next | WindowPart::Free) =>
                {
                    let position = match steps {
                        (PlaceStep::Index(index), _) | (_, PlaceStep::Index(index)) => {
                            CheckedCallSeparationPositions::Live(*index)
                        }
                        (PlaceStep::Range(range), _) | (_, PlaceStep::Range(range)) => {
                            CheckedCallSeparationPositions::RangeWithinLength(*range)
                        }
                        _ => break,
                    };
                    candidates.push(position);
                    window = Some(ResolvedPlace {
                        root: left.root,
                        path: left.path[..depth].to_vec(),
                    });
                    break;
                }
                (first, second) if first == second => continue,
                _ if candidates.is_empty() => return None,
                _ => break,
            }
        }
        (!candidates.is_empty()).then_some((candidates, window))
    }

    /// [OP-10] the calls that end the bound a reference into a window was
    /// formed under.
    ///
    /// Of the window operations, `place_back`'s `ensures` carries
    /// `i < r.len` across the call and `insert_at` only changes a slot's
    /// occupant, so neither ends it; the rest move a boundary down, move a
    /// run between two windows, shift every logical index, or remake the
    /// block whole. Any other callee whose row writes a window's `last` or
    /// `filled` may take elements back through it as `take_back` and
    /// `remove_at` do, and no `ensures` is read here to show that it does
    /// not, so its call ends the bound as well. Every reference into such a
    /// window dies [REF-2, REF-4].
    fn invalidate_window_operation_references(
        &self,
        signature: &FunctionSignature,
        entries: &[SubstitutedEntry],
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        const BOUND_ENDING_ROWS: [&str; 7] = [
            "take_back",
            "remove_at",
            "append",
            "split_off",
            "place_front",
            "take_front",
            "grow",
        ];
        let prelude = self.tree.is_prelude_node(signature.node)?;
        let window_operation = prelude && BOUND_ENDING_ROWS.contains(&signature.name.as_str());
        for entry in entries {
            // A row entry names a part or a measure word of the window --
            // `writes(window.filled)`, `writes(window.len)` -- and the window
            // whose bound the call ends is the place below that step.
            let cut = entry
                .place
                .path
                .iter()
                .position(|step| matches!(step, PlaceStep::Part(_) | PlaceStep::Measure(_)))
                .unwrap_or(entry.place.path.len());
            let takes_back = !prelude
                && entry.write
                && matches!(
                    entry.place.path.get(cut),
                    Some(PlaceStep::Part(WindowPart::Last | WindowPart::Filled))
                );
            if !window_operation && !takes_back {
                continue;
            }
            let window = ResolvedPlace {
                root: entry.place.root,
                path: entry.place.path[..cut].to_vec(),
            };
            Self::invalidate_window_references(bindings, &window);
        }
        Ok(())
    }

    /// [STOR-8] a compilation unit carrying the no-heap declaration cannot
    /// call an allocating prelude row.
    ///
    /// The five rows the rule names are exactly the allocating [OP-13] and
    /// [OP-10] records that take a cell from the heap; `slots_new`,
    /// `ring_new` and `array_filled` build frame-resident shapes and are not
    /// among them.
    fn reject_allocating_call_under_no_heap(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        if !self.no_heap || !HEAP_ALLOCATING_PRELUDE_FUNCTIONS.contains(&signature.name.as_str()) {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Stor8,
            node,
            SemanticIssueKind::HeapTypeUnderNoHeap {
                spelling: signature.name.clone(),
                mechanical_fix: super::super::super::types::STOR8_NO_HEAP,
            },
        )
    }

    /// [OP-11] a `swap` over a copy place is a hard error at the first
    /// `borrow_expr`, with the restructuring `read the two values and assign
    /// them back`.
    ///
    /// The rule judges the refusal once at the written bound, exactly as
    /// [OWN-1]'s spelling judgment is judged, and does not re-make it at a
    /// concrete instance [FN-2]; `judges_class_spelling` is that same gate.
    /// The rule locates it at the first `borrow_expr`; this locates it at the
    /// complete `call`, whose written arguments are exactly those two
    /// `borrow_expr` nodes, because the refusal is about the operation and
    /// not about one of its two symmetric operands.
    fn reject_swap_over_copy(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        if !self.is_swap_row(signature) || !self.judges_class_spelling() {
            return Ok(());
        }
        let Some(first) = signature.parameters.first() else {
            return Ok(());
        };
        if !self.is_copy_type(first.ty)? {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Op11,
            node,
            SemanticIssueKind::SwapOverCopyPlace {
                place_type: self.checked_type_name(first.ty)?,
                mechanical_fix: "read the two values and assign them back",
            },
        )
    }

    /// [OP-11] the one operation whose two reference arguments may name the
    /// same place.
    fn is_swap_row(&self, signature: &FunctionSignature) -> bool {
        signature.name == "swap"
    }

    /// [EFF-5] clause 3: every live reference, including an actual argument,
    /// receives each substituted effect's ordinary invalidation [REF-2].
    /// Only an access at or below its captured target preserves that target;
    /// being an argument does not exempt it from another actual's write.
    fn invalidate_call_references(
        &self,
        node: NodeId,
        entries: &[SubstitutedEntry],
        atomic_target: Option<&ResolvedPlace>,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        for entry in entries.iter().filter(|entry| entry.write) {
            let event = if entry.consuming {
                // [OP-12] the recognized first argument is not an ordinary
                // move out of `p`: its old value enters the call and its
                // result replaces `p` at one atomic commit, whose effect is
                // `writes(p)`. Equal-path references therefore remain valid
                // while references below `p` still die under [REF-2]'s
                // proper-prefix write rule. Every other consuming argument
                // retains the ordinary move invalidation, including equality.
                if entry.argument == 0 && atomic_target.is_some_and(|target| entry.place == *target)
                {
                    InvalidationEvent::CallWrite
                } else {
                    InvalidationEvent::PrefixMoved
                }
            } else {
                InvalidationEvent::CallWrite
            };
            self.invalidate_references_with_separation(
                bindings,
                &entry.place,
                &event,
                Some(self.tree.path(node)?),
            )?;
        }
        Ok(())
    }

    /// [EFF-2] the caller's own row: each projected entry rooted in a current
    /// formal contributes that formal's corresponding path, and an entry
    /// rooted only in local storage contributes none.
    fn project_call_effects(
        &self,
        node: NodeId,
        caller: &FunctionSignature,
        entries: &[SubstitutedEntry],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        effects: &mut EffectSet,
    ) -> Result<(), CheckStop> {
        for entry in entries {
            for path in self.effect_paths_for_place(node, &entry.place, bindings)? {
                if !caller
                    .parameters
                    .iter()
                    .any(|parameter| parameter.declaration == path.path.root)
                {
                    continue;
                }
                if entry.write {
                    effects.add_write(path);
                } else {
                    effects.add_read(path);
                }
            }
        }
        Ok(())
    }

    /// Captures one already-checked actual's pre-transfer goal image.
    ///
    /// This runs after the actual expression has acquired all of its checked
    /// obligations and after borrow feasibility succeeds. It never rechecks or
    /// reevaluates the source expression. A borrow destination is represented
    /// by the resolved ultimate referent captured in `passed_borrow` before
    /// that transient checker metadata disappears.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::check) fn call_goal_argument(
        &self,
        caller: super::super::super::super::model::FunctionId,
        call: &crate::NodePath,
        ordinal: u32,
        atom: NodeId,
        expected_mode: CheckedMode,
        expected_type: CheckedType,
        argument: &super::super::super::TypedExpression,
        passed_place: Option<&ResolvedPlace>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<GoalExpression, CheckStop> {
        if expected_mode == CheckedMode::Range {
            // [REF-4, MSR-1] a range reference's one measure is `len`, equal
            // to `hi - lo`, and that is no measure of the storage the range
            // was formed over: `&a[2..4]` names two elements whatever `a.len`
            // is. [MSR-1] admits `deref(view)` as a measure place — a root
            // with `deref` wrappings, field selections and subscripts — and
            // admits no place formed with a range step, so
            // the term this instantiation names is the reference the actual
            // names and never that reference's base. Resolving through the
            // reference here would drop the range step and read
            // `deref(part).len` as the owner's `len`, which admits an index
            // outside the range and outside the storage.
            //
            // An actual that forms its range at the call names no binding, so
            // it has no such measure place; its image is the resolved path
            // the formation names, ending in that formation's own range step,
            // whose captured endpoints are what [REF-4] makes the range's
            // `len`.
            return Ok(match &argument.expression {
                CheckedExpression::Binding { binding, .. } => {
                    GoalExpression::Datum(GoalDatum::Place {
                        root: *binding,
                        projections: Vec::new(),
                        ty: expected_type,
                    })
                }
                _ => match passed_place.filter(|place| !place.has_descendant()) {
                    Some(place) => self.goal_referent_image(place, expected_type, atom)?,
                    None => GoalExpression::Datum(GoalDatum::EvaluatedValue {
                        function: caller,
                        occurrence: EvaluatedValueOccurrence::CallArgument {
                            call: call.clone(),
                            argument: ordinal,
                        },
                        captured_type: expected_type,
                        projections: Vec::new(),
                        ty: expected_type,
                    }),
                },
            });
        }
        if expected_mode != CheckedMode::Own {
            if let Some(place) = passed_place.filter(|place| !place.has_descendant()) {
                return self.goal_referent_image(place, expected_type, atom);
            }
            // FN-1's candidate protects every mutable origin a returned
            // borrow may reach, including when the delivered value is a
            // different immutable constant. ENT-2's value identity is the
            // actual holder, never that conservative loan ceiling.
            let place_parent = self
                .tree
                .first_child_with(atom, Production::BorrowExpr)?
                .unwrap_or(atom);
            let place = self
                .tree
                .first_child_with(place_parent, Production::Place)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if self.call_goal_place_contains_subscript(place)? {
                return Ok(GoalExpression::Datum(GoalDatum::EvaluatedValue {
                    function: caller,
                    occurrence: EvaluatedValueOccurrence::CallArgument {
                        call: call.clone(),
                        argument: ordinal,
                    },
                    captured_type: expected_type,
                    projections: Vec::new(),
                    ty: expected_type,
                }));
            }
            let (image, _) = self.call_goal_place_inner(place, bindings)?;
            if image.ty() != expected_type {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            return Ok(image);
        }

        // [MSR-6, ENT-2] a const generic read is already the canonical
        // symbolic constant, or its supplied concrete integer. Preserve that
        // checked value just as for a written literal; re-reading the source
        // place would lose both its value class and its substitution.
        // Named constants retain their separate declaration image below.
        if let CheckedExpression::Constant(value) = &argument.expression {
            return Ok(GoalExpression::Datum(GoalDatum::Literal(value.clone())));
        }

        let place = self
            .tree
            .first_child_with(atom, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if self.call_goal_place_contains_subscript(place)? {
            return Ok(GoalExpression::Datum(GoalDatum::EvaluatedValue {
                function: caller,
                occurrence: EvaluatedValueOccurrence::CallArgument {
                    call: call.clone(),
                    argument: ordinal,
                },
                captured_type: expected_type,
                projections: Vec::new(),
                ty: expected_type,
            }));
        }
        // [MSR-1, FN-8] a measure actual has already been checked as a
        // scalar read. Its suffix is a measure operation, not a struct field;
        // retain that checked operation and its exact storage projection.
        let measure = match &argument.expression {
            CheckedExpression::ContainerMeasure { measure, root } => Some((
                GoalOperation::ContainerMeasure {
                    measure: *measure,
                    measured: root
                        .measured()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                    element: root.element(),
                    constant: root.type_constant(),
                },
                self.goal_referent_image(
                    &ResolvedPlace {
                        root: root.root,
                        path: root.place_path(),
                    },
                    root.ty,
                    atom,
                )?,
            )),
            CheckedExpression::RangeMeasure { measure, root } => Some((
                GoalOperation::ContainerMeasure {
                    measure: *measure,
                    measured: super::super::super::super::model::MeasuredKind::Range,
                    element: Some(root.element),
                    constant: None,
                },
                GoalExpression::Datum(GoalDatum::Place {
                    root: root.binding,
                    projections: Vec::new(),
                    ty: root.element_type,
                }),
            )),
            CheckedExpression::RangeElementMeasure { measure, place, .. } => Some((
                GoalOperation::ContainerMeasure {
                    measure: *measure,
                    measured: place
                        .measured()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                    element: place.element(),
                    constant: place.type_constant(),
                },
                GoalExpression::Datum(GoalDatum::Place {
                    root: place.root.binding,
                    projections: place.goal_projections(),
                    ty: place.ty,
                }),
            )),
            _ => None,
        };
        if let Some((row, measured)) = measure {
            return Ok(GoalExpression::Operation {
                row,
                type_arguments: Vec::new(),
                const_arguments: Vec::new(),
                result: expected_type,
                arguments: vec![measured],
            });
        }
        let (image, holder_pending) = self.call_goal_place_inner(place, bindings)?;
        if holder_pending || image.ty() != expected_type {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(image)
    }

    /// A place may nest another place under a `deref` pbase. Search the whole
    /// source place, not only its outer suffix list, so a future admitted
    /// `deref(boxes[i])` actual receives the same ephemeral treatment and is
    /// never misidentified as a rereadable place.
    fn call_goal_place_contains_subscript(&self, place: NodeId) -> Result<bool, CheckStop> {
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        if self.last_subscript(&suffixes)?.is_some() {
            return Ok(true);
        }
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let Some(nested) = self.tree.first_child_with(pbase, Production::Place)? else {
            return Ok(false);
        };
        self.call_goal_place_contains_subscript(nested)
    }

    /// Forms a caller-visible referent datum. A root that is itself one of the
    /// caller's borrow parameters remains opaque and therefore retains one
    /// `Deref`; a local borrow/reborrow has already resolved through its holder
    /// to an own root and adds no such projection. Owning indirection remains
    /// in the typed storage path: overlap may identify a Box with its content,
    /// but a measure must still select the content's value. This applies to
    /// direct borrows and forwarded holders alike.
    fn goal_referent_image(
        &self,
        place: &ResolvedPlace,
        ty: CheckedType,
        node: NodeId,
    ) -> Result<GoalExpression, CheckStop> {
        let mut projections = Vec::new();
        for step in &place.path {
            match step {
                PlaceStep::Field(field) => projections.push(GoalProjection::Field(*field)),
                PlaceStep::Payload { variant, field } => {
                    projections.push(GoalProjection::Payload {
                        variant: *variant,
                        field: *field,
                    });
                }
                PlaceStep::Deref => projections.push(GoalProjection::Deref),
                PlaceStep::Index(index) => {
                    projections.push(GoalProjection::Subscript(index.goal_identity()));
                }
                // [REF-4] the range a `&[T]` actual names. Dropping this step
                // would read the actual's `len` as the `len` of the storage
                // the range was formed over, which are two different
                // quantities [MSR-1], so it is kept and never collapsed onto
                // its base.
                PlaceStep::Range(range) => {
                    projections.push(GoalProjection::Range(*range));
                }
                // A window-part effect is not a value projection. Failing
                // to represent a value must not substitute its parent.
                PlaceStep::Part(_) | PlaceStep::Measure(_) | PlaceStep::Descendant(_) => {
                    return self.unsupported(
                        super::super::super::super::UnsupportedSemanticFeature::CompositeValues,
                        node,
                    );
                }
            }
        }
        let datum = match place.root {
            PlaceRoot::Constant(constant) => GoalDatum::NamedConst {
                declaration: self
                    .constants
                    .iter()
                    .find_map(|(declaration, id)| (*id == constant).then_some(*declaration))
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                projections,
                ty,
            },
            PlaceRoot::Binding(binding) => GoalDatum::Place {
                root: binding,
                projections,
                ty,
            },
        };
        Ok(GoalExpression::Datum(datum))
    }

    /// Resolves one non-indexed own actual to its concrete caller datum while
    /// preserving own-box dereference and field order. Dereferencing a borrow
    /// holder consumes the holder boundary exactly once and leaves the
    /// ultimate referent image produced above.
    fn call_goal_place_inner(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(GoalExpression, bool), CheckStop> {
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let (mut expression, holder_pending) = if self
            .has_fixed(pbase, crate::FixedTerminal::Deref)?
        {
            let nested = self
                .tree
                .first_child_with(pbase, Production::Place)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let (nested, nested_holder_pending) = self.call_goal_place_inner(nested, bindings)?;
            if nested_holder_pending {
                (nested, false)
            } else {
                let CheckedType::Nominal(nominal) = nested.ty() else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                (
                    nested
                        .with_projection(GoalProjection::Deref, referent)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                    false,
                )
            }
        } else {
            let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
            let ResolvedTarget::Source { declaration, class } = usage.target() else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            match class {
                DeclarationClass::Value => {
                    let local = bindings
                        .get(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    if let Some(reference) = &local.reference {
                        (
                            match reference.paths.as_slice() {
                                [path] if !path.has_descendant() => {
                                    self.goal_referent_image(path, local.ty, place)?
                                }
                                // [REF-1] a joined reference still denotes
                                // one selected referent, but no member of its
                                // possible-target set is its unconditional
                                // value. Keep the reference's identity so
                                // proof kills can resolve every candidate.
                                _ => GoalExpression::Datum(GoalDatum::Place {
                                    root: local.binding,
                                    projections: Vec::new(),
                                    ty: local.ty,
                                }),
                            },
                            true,
                        )
                    } else {
                        (
                            GoalExpression::Datum(GoalDatum::Place {
                                root: local.binding,
                                projections: Vec::new(),
                                ty: local.ty,
                            }),
                            false,
                        )
                    }
                }
                DeclarationClass::NamedConst => {
                    let constant = self
                        .constants
                        .get(&declaration)
                        .copied()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    (
                        GoalExpression::Datum(GoalDatum::NamedConst {
                            declaration,
                            projections: Vec::new(),
                            ty: self.constant(constant)?.ty,
                        }),
                        false,
                    )
                }
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            }
        };

        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        if holder_pending && !suffixes.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        for &suffix in &suffixes {
            let ty = expression.ty();
            // [TYPE-9] a `Box`'s content is its field `inner`, and the storage
            // below that field is the cell's referent, so the image takes the
            // dereference step the content already is rather than a field
            // selection. The field walk below has no step for it.
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
                expression = expression
                    .with_projection(GoalProjection::Deref, referent)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                continue;
            }
            let (fields, selected) = self.resolve_struct_path(std::slice::from_ref(&suffix), ty)?;
            for field in fields {
                expression = expression
                    .with_projection(GoalProjection::Field(field), selected)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            }
        }
        Ok((expression, holder_pending))
    }
}
