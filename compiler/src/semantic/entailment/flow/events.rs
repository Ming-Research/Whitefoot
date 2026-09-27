//! Kill and scope events: which facts, measures, goals and entry
//! images an event invalidates, forming the kills a call, write or set
//! commits, and applying them and the scope exits to the per-path state.

use super::*;

impl Input<'_, '_> {
    pub(super) fn summary(&self, binding: BindingId) -> Option<&BindingSummary> {
        self.places.summary(binding)
    }

    /// [REF-1] the body-side reading of one clause place's projections.
    ///
    /// A clause names a reference parameter's referent as `deref(p)`, and the
    /// declaration-boundary template keeps that step as its leading
    /// projection because a caller substitutes the actual's own path for the
    /// formal and consumes exactly it [FN-8, CALL-6]. Inside the body the
    /// parameter name *is* the path, so the step names no place of its own
    /// and is dropped: that is the identity every other term over `deref(p)`
    /// already carries, `is_holder` above having synthesized none.
    pub(super) fn body_projections<'projections>(
        &self,
        root: PlaceRoot,
        projections: &'projections [GoalProjection],
    ) -> &'projections [GoalProjection] {
        let PlaceRoot::Binding(binding) = root else {
            return projections;
        };
        if !self.places.is_reference(binding) {
            return projections;
        }
        match projections.split_first() {
            Some((GoalProjection::Deref, rest)) => rest,
            _ => projections,
        }
    }

    /// The [REF-1] resolved place one term's path names, which replaces a
    /// reference-variable root by the path that reference names.
    ///
    /// A join may give a reference variable more than one path [REF-1]; a
    /// term may substitute an origin only when it is unique. Otherwise its
    /// reference-local identity denotes the selected referent, without
    /// asserting equality to any one possible origin. Kill judgments must
    /// still consider every origin through `resolved_places_overlap`.
    pub(super) fn resolve(&self, place: &ResolvedPlace) -> ResolvedPlace {
        let candidates = self.places.resolve(place.root, &place.path);
        match candidates.as_slice() {
            [unique] => unique.clone(),
            _ => place.clone(),
        }
    }

    /// [REF-1, ENT-5] a write can invalidate a fact through any member of
    /// either path set. Taking only the first member loses real writes at
    /// a control-flow join.
    pub(super) fn resolved_places_overlap(
        &self,
        separations: &dyn SeparationOracle,
        left: &ResolvedPlace,
        right: &ResolvedPlace,
    ) -> bool {
        let lefts = self.places.resolve(left.root, &left.path);
        let rights = self.places.resolve(right.root, &right.path);
        if lefts.is_empty() || rights.is_empty() {
            // The place prepass represents an unresolved reference with no
            // candidates. It is unknown storage, not proof of separation.
            return true;
        }
        lefts.iter().any(|left| {
            rights
                .iter()
                .any(|right| self.places.overlaps(separations, left, right))
        })
    }

    /// [MSR-2] whether one event kills a measure of `support`.
    ///
    /// A write at an element position of P carries the written element's own
    /// place, `P[i]`, so it overlaps the descriptor storage of `P[i]` and
    /// none of P's own: it kills every measure of `P[i]` and no measure of P.
    /// A write of a whole value writes everything under it, so it kills the
    /// measures of every place it reaches. Two subscripts of one base are the
    /// same step of that reach unless their offsets are provably distinct
    /// [OWN-7].
    ///
    /// A written place that names a measure word of P -- `writes(window.len)`
    /// in an [OP-10] row, which [WIN-2] states "is itself a write target and
    /// overlaps no slot" -- does not contain P, and the containment above
    /// alone would let the pre-call relations of P survive the operation that
    /// changes them. Such a write reaches exactly the word it names, because
    /// [EFF-2]'s both-ways check is what admits the row: a body that changed
    /// another word of P's descriptor would exhibit a write the row does not
    /// carry. It reaches no element storage either, so the measures of an
    /// element of P survive it.
    pub(super) fn event_kills_measure(
        &self,
        separations: &dyn SeparationOracle,
        measure: CheckedMeasure,
        support: &ResolvedPlace,
        root: PlaceRoot,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write {
                place: written,
                element,
                ..
            }
            | KillEvent::EntryImageHolderWrite {
                place: written,
                element,
                ..
            } => self.write_overlaps_measure(separations, written, support, measure, *element),
            KillEvent::Consume { binding, .. } => root == PlaceRoot::Binding(*binding),
            KillEvent::EntryImageHolderConsume { .. } => false,
        }
    }

    /// [ENT-5, MSR-2] a measure's mutable support is its descriptor word.
    /// Whole-value writes overlap it, element writes below it do not, and
    /// an uncertain index may select the measured element. Definite path
    /// identity is insufficient here: failure to prove two captures equal
    /// must never preserve a fact about storage the write may replace.
    pub(super) fn write_overlaps_measure(
        &self,
        separations: &dyn SeparationOracle,
        written: &ResolvedPlace,
        support: &ResolvedPlace,
        measure: CheckedMeasure,
        element_write: bool,
    ) -> bool {
        // [REF-4, MSR-2] a range reference's `len` is the immutable
        // descriptor value captured at formation. An element write through
        // any range view and a window-part or descriptor-word write on its
        // backing window therefore leave it unchanged. The ordinary path
        // walk cannot discover this after two unequal range steps:
        // conservative range overlap stops it before the final
        // Index/Measure or Range/Part pair. A measured element has an Index
        // after its range step and therefore does not take this case.
        //
        // Both classifiers require complete, nonempty resolution. A holder
        // rebind is a whole-place write with no Part/Measure suffix, while an
        // unresolved write remains conservative and reaches the descriptor.
        if self.is_range_descriptor_support(support)
            && (element_write || self.is_window_extent_write(written))
        {
            return false;
        }
        let mut descriptor = support.clone();
        descriptor.path.push(PlaceStep::Measure(measure));
        // The fact's own place goes first: [WIN-2]'s liveness is a question
        // about the window above its index [`Reasoning::event_live_bounds`].
        self.resolved_places_overlap(separations, &descriptor, written)
    }

    /// Whether `support` is the range value itself, rather than an element
    /// selected from it. A unique resolved origin ends in its captured range
    /// step; a joined reference may have several such origins.
    pub(super) fn is_range_descriptor_support(&self, support: &ResolvedPlace) -> bool {
        let resolved = self.places.resolve(support.root, &support.path);
        !resolved.is_empty()
            && resolved
                .iter()
                .all(|place| matches!(place.path.last(), Some(PlaceStep::Range(_))))
    }

    /// Whether every resolved write reaches one named part or descriptor
    /// word of a window. These suffixes are the exact [WIN-2] effect-row
    /// targets; a field, holder, or whole-owner replacement does not qualify.
    pub(super) fn is_window_extent_write(&self, written: &ResolvedPlace) -> bool {
        let resolved = self.places.resolve(written.root, &written.path);
        !resolved.is_empty()
            && resolved.iter().all(|place| {
                matches!(
                    place.path.last(),
                    Some(PlaceStep::Part(_) | PlaceStep::Measure(_))
                )
            })
    }

    /// [ENT-5] whether one event writes or consumes a binding an offset
    /// occurring in `support` reads.
    ///
    /// The support of a measure term over P contains the support of every
    /// offset occurring in P, so a write to that offset's own binding kills
    /// the measure at every level it occurs in.
    pub(super) fn event_kills_offset_support(
        &self,
        separations: &dyn SeparationOracle,
        support: &ResolvedPlace,
        event: &KillEvent,
    ) -> bool {
        support.path.iter().any(|step| {
            let PlaceStep::Index(offset) = step else {
                return false;
            };
            let Some(binding) = offset.support() else {
                return false;
            };
            let read = ResolvedPlace::binding(binding);
            match event {
                KillEvent::Write { place: written, .. }
                | KillEvent::EntryImageHolderWrite { place: written, .. } => {
                    self.resolved_places_overlap(separations, &read, written)
                }
                KillEvent::Consume {
                    binding: consumed, ..
                } => binding == *consumed,
                KillEvent::EntryImageHolderConsume { .. } => false,
            }
        })
    }

    pub(super) fn resolve_goal_support(
        &self,
        support: &GoalSupport,
    ) -> (ResolvedPlace, Vec<BindingId>) {
        let mut resolved = ResolvedPlace {
            root: PlaceRoot::Binding(support.root),
            path: Vec::new(),
        };
        // Goal support is already in body-place form. A reference root is the
        // holder of that place, while every projection is a real step below
        // its referent and must remain part of the support.
        let holders = self
            .places
            .is_reference(support.root)
            .then_some(support.root)
            .into_iter()
            .collect::<Vec<_>>();
        resolved.path.extend(
            support
                .projections
                .iter()
                .map(|projection| projection.place_step()),
        );
        (resolved, holders)
    }

    /// An ordinary-let origin is available only while the binding whose
    /// initializer it describes has not itself been written or consumed.
    /// This key guard is separate from the goal's value support: invalidating
    /// it stops future alias expansion without erasing a signed snapshot fact
    /// that an earlier branch already established.
    pub(super) fn event_kills_goal_origin_binding(
        &self,
        separations: &dyn SeparationOracle,
        binding: BindingId,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &ResolvedPlace::binding(binding), place)
            }
            KillEvent::Consume {
                binding: consumed, ..
            } => binding == *consumed,
            KillEvent::EntryImageHolderConsume { .. } => false,
        }
    }

    pub(super) fn event_kills_entry_image(
        &self,
        separations: &dyn SeparationOracle,
        image: &EntryImageRecord,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { element: true, .. }
            | KillEvent::EntryImageHolderWrite { element: true, .. }
                if image.datum.measure.is_some() =>
            {
                false
            }
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &image.place, place)
            }
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => {
                image.holders.contains(binding) || image.place.root == PlaceRoot::Binding(*binding)
            }
        }
    }

    /// The [REF-1] resolved place one argument expression names, and whether
    /// it reaches that place through a reference variable rather than through
    /// a `borrow_expr` written here.
    ///
    /// [EFF-5] substitutes the actual's path into the callee's row, so what a
    /// call writes through parameter i is this place extended by that row's
    /// `epsuffix*`. An argument that is not a place names none.
    ///
    /// A range formed at the call, `&x[lo..hi]` or `&deref(part)[a..b]`, is a
    /// `borrow_expr` written here exactly as `&x[i]` is: it names its source's
    /// resolved places extended by the formation's own range step [REF-4,
    /// OWN-7], the same path a bound range reference would name. That step
    /// against an index step of the same base separates only where the index
    /// is proved outside the range, so a write through it reaches every
    /// other element fact of that base unless the paths separate earlier.
    pub(super) fn argument_referents(
        &self,
        argument: &CheckedExpression,
    ) -> Vec<(ResolvedPlace, bool)> {
        let Some(named) = named_place(argument) else {
            return Vec::new();
        };
        let through_reference = match named.form {
            NamingForm::Binding(binding) if self.places.is_reference(binding) => true,
            NamingForm::Borrow | NamingForm::Range => false,
            NamingForm::Binding(_) | NamingForm::Read => return Vec::new(),
        };
        named
            .resolve(&self.places, true)
            .into_iter()
            .map(|place| (place, through_reference))
            .collect()
    }

    /// The [ENT-5] kills one call's write through a viewed range projects
    /// [CALL-3].
    ///
    /// The write reaches the viewed range's element storage and no measure
    /// term over the origin place itself, nor over the view. The event is the
    /// actual's range place with one element step of unknown offset below it,
    /// so `len_of(origin)` and `len_of(view)` both survive it [MSR-2], while
    /// a fact over any element the range may contain dies with that element's
    /// storage: a field of it, or, for an element type with descriptor
    /// storage of its own, its measures [CALL-3]. Whether the actual is a
    /// bound view or a range formed at the call, `argument_referents` names
    /// the same range place.
    pub(super) fn collect_view_write_kills(
        &self,
        argument: &CheckedExpression,
        call: &crate::NodePath,
        events: &mut Vec<KillEvent>,
    ) {
        for (place, entry_image_only) in self.argument_referents(argument) {
            let place = element_write_place(place, CapturedValue::unknown());
            if entry_image_only {
                events.push(KillEvent::EntryImageHolderWrite {
                    place,
                    element: true,
                    source: call.clone(),
                });
            } else {
                events.push(KillEvent::Write {
                    place,
                    element: true,
                    source: call.clone(),
                });
            }
        }
    }

    /// Collects [ENT-5] kill events (b) and (c) from one expression tree.
    pub(super) fn collect_expression_kills(
        &self,
        expression: &CheckedExpression,
        events: &mut Vec<KillEvent>,
    ) {
        match expression {
            CheckedExpression::Binding {
                carrier,
                binding,
                consume_root,
                ty,
                ..
            } => {
                if is_holder(*binding) {
                    if *consume_root {
                        events.push(KillEvent::EntryImageHolderConsume {
                            binding: *binding,
                            source: carrier.clone(),
                        });
                    }
                } else if !self.is_copy(*ty)
                    // [VIEW-1, OWN-5] a borrow of a view is not a consume of
                    // it. A view binding occurs in a `borrow_expr` as itself,
                    // because the descriptor is what a borrow of one carries,
                    // and that occurrence carries `consume_root: false`; the
                    // exclusive view is affine, so without this the ordinary
                    // affine kill would end every fact about a view the
                    // moment it is handed to a call that only borrows it.
                    && *consume_root
                {
                    events.push(KillEvent::Consume {
                        binding: *binding,
                        source: carrier.clone(),
                    });
                }
            }
            CheckedExpression::Project {
                carrier,
                binding,
                consume_root,
                ..
            } => {
                if *consume_root {
                    events.push(KillEvent::Consume {
                        binding: *binding,
                        source: carrier.clone(),
                    });
                }
            }
            CheckedExpression::BoxTake {
                carrier, binding, ..
            } => {
                // [TYPE-9, WIN-3, ENT-5] the selected content leaves through
                // this expression, but the complete owning root ceases to
                // exist. The checked take no longer embeds a synthetic
                // Binding child, so its root consume is explicit here.
                events.push(KillEvent::Consume {
                    binding: *binding,
                    source: carrier.clone(),
                });
            }
            // These wrappers are checked reads of one place. Their nested
            // expression preserves source spelling and lowering structure;
            // it is not a second consuming evaluation of an affine holder.
            CheckedExpression::BoxDeref { .. } | CheckedExpression::ProjectValue { .. } => {}
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                actual_captures,
                formal_effects,
                ..
            } => {
                let callee = self.context.callee(*function);
                for argument in arguments {
                    self.collect_expression_kills(argument, events);
                }
                let offsets = substituted_offsets(callee, actual_captures);
                for (index, argument) in arguments.iter().enumerate() {
                    let boundary_writes = formal_effects.as_ref().and_then(|effects| {
                        let declaration = callee?.parameter_declarations.get(index)?;
                        Some(
                            effects
                                .writes
                                .iter()
                                .filter(|path| path.root == *declaration)
                                .map(|path| path.steps.clone())
                                .collect::<Vec<_>>(),
                        )
                    });
                    let Some(writes) = boundary_writes
                        .as_ref()
                        .or_else(|| callee.and_then(|callee| callee.parameter_writes.get(index)))
                    else {
                        continue;
                    };
                    // [CALL-5] the transport is the declared parameter's and
                    // never the actual's spelling: a shared borrow is a kill
                    // event for nothing [CALL-1], a view confines the write to
                    // the range's element storage [CALL-3], and every other
                    // parameter kills conservatively.
                    let transport = callee
                        .and_then(|callee| callee.parameter_transports.get(index).copied())
                        .unwrap_or_default();
                    if transport == CallTransport::ReadOnlyReference {
                        continue;
                    }
                    let element = transport.writes_element_storage();
                    if !writes.is_empty() && element {
                        self.collect_view_write_kills(argument, call, events);
                        continue;
                    }
                    for (place, entry_image_only) in self.argument_referents(argument) {
                        for steps in writes {
                            let mut written = place.clone();
                            written.path.extend(substituted_steps(steps, &offsets));
                            if entry_image_only {
                                events.push(KillEvent::EntryImageHolderWrite {
                                    place: written,
                                    element,
                                    source: call.clone(),
                                });
                            } else {
                                events.push(KillEvent::Write {
                                    place: written,
                                    element,
                                    source: call.clone(),
                                });
                            }
                        }
                    }
                }
            }
            // [BLK-0, EFF-1] a row's declared effect row is a callee effect
            // like any other: the place its `writes` names is written by the
            // call, so [ENT-5] kills every fact whose support that place
            // reaches. Without this the store's pre-call measure facts and
            // the pre-transfer call-datum equality survive beside the row's
            // own post-state relations, and the two together are a
            // contradiction the row introduces.
            _ => {
                for child in expression_children(expression) {
                    self.collect_expression_kills(child, events);
                }
            }
        }
    }

    pub(super) fn affine_event_kills_binding(
        &self,
        separations: &dyn SeparationOracle,
        binding: BindingId,
        event: &KillEvent,
    ) -> bool {
        match event {
            KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. } => {
                self.resolved_places_overlap(separations, &ResolvedPlace::binding(binding), place)
            }
            KillEvent::Consume {
                binding: consumed, ..
            }
            | KillEvent::EntryImageHolderConsume {
                binding: consumed, ..
            } => binding == *consumed,
        }
    }

    /// The [ENT-5] commit kill of one `set` target. The statement walk and
    /// the loop summary both form it here, so the event a loop body applies
    /// is the event its head subtracts.
    pub(super) fn commit_kill(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
    ) -> KillEvent {
        match target {
            CheckedSetTarget::Place(place) => {
                let spelled = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.binding),
                    is_holder(place.binding),
                    place.fields.clone(),
                );
                KillEvent::Write {
                    place: self.resolve(&spelled),
                    element: false,
                    source: node_path.clone(),
                }
            }
            CheckedSetTarget::RangeIndex(target) => {
                let mut spelled = ResolvedPlace::spelled(
                    PlaceRoot::Binding(target.root.binding),
                    is_holder(target.root.binding),
                    Vec::new(),
                );
                spelled.path.extend(target.place_path());
                KillEvent::Write {
                    place: self.resolve(&spelled),
                    element: true,
                    source: node_path.clone(),
                }
            }
            // [MSR-2] an element store into a run overlaps the descriptor
            // storage of `v[i]` and none of `v`'s own, so it kills the
            // measures of the element and none of the run's.
            CheckedSetTarget::Storage(target) => KillEvent::Write {
                place: self.container_root_place(target),
                element: true,
                source: node_path.clone(),
            },
        }
    }

    /// The commit kill of one `set` target, and the goal-origin state a
    /// whole-place commit invalidates. One target list's commits are
    /// exactly this event per target, on the same edge.
    pub(super) fn collect_target_kill(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        state: &mut ProofFlowState,
        target_kills: &mut Vec<KillEvent>,
    ) {
        target_kills.push(self.commit_kill(node_path, target));
        if let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
        {
            state.facts.origins.remove(&place.binding);
        }
    }
}

impl Vocabulary {
    pub(super) fn proof_event(
        &mut self,
        kind: FlowEventKind,
        node_path: Option<&crate::NodePath>,
    ) -> FlowEventId {
        self.derivations.event(kind, node_path.cloned())
    }

    /// Whether leaving the scopes of `exited` kills a fact supported by
    /// `term`: the support contains every tracked place's root binding and
    /// every holder read through, which is the spelling root here.
    pub(super) fn scope_kills_term(&self, term: TermId, exited: &HashSet<BindingId>) -> bool {
        match self.terms.kind(term) {
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => false,
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => false,
            // [ENT-5] the support of every offset occurring in the place is
            // part of the term's own support, so a term dies with the scope
            // of an offset's binding exactly as it dies with its root's.
            TermKind::Place(place, _) | TermKind::Measure(_, place) => {
                let rooted = match place.root {
                    PlaceRoot::Binding(binding) => exited.contains(&binding),
                    PlaceRoot::Constant(_) => false,
                };
                rooted
                    || place.path.iter().any(|projection| {
                        matches!(projection, PlaceStep::Index(offset)
                            if offset.support().is_some_and(|binding| exited.contains(&binding)))
                    })
            }
        }
    }

    /// Materializes the complete [ENT-4] closure before an event-kill batch.
    ///
    /// The existing event predicates remain the sole authority for which
    /// terms, goals, and origins die. Materialization only makes every
    /// survivor-to-survivor consequence independently live before one of its
    /// supporting endpoints disappears.
    pub(super) fn materialize_before_event_kill(
        &mut self,
        state: &mut FactState,
        events: &[KillEvent],
    ) {
        if events.is_empty() {
            return;
        }
        materialize_closure_before_kill(state, &self.terms, &self.goals, &mut self.derivations);
    }

    /// Applies the private capture-scope kill of one counted construct.
    pub(super) fn exit_counted_capture_scope_one(
        &mut self,
        state: &mut FactState,
        range_path: &[u32],
    ) {
        state.kill(|term| {
            matches!(
                self.terms.kind(term),
                TermKind::CountedCapture { range_path: path, .. } if path == range_path
            )
        });
    }

    pub(super) fn exit_counted_capture_scope(
        &mut self,
        states: &mut ProofFlowState,
        range_path: &[u32],
    ) {
        self.promote_flow_contradiction(states);
        for result in states.results.values_mut() {
            self.refresh_result(result, &states.facts);
            materialize_closure_before_kill(
                &mut result.facts,
                &self.terms,
                &self.goals,
                &mut self.derivations,
            );
            self.exit_counted_capture_scope_one(&mut result.facts, range_path);
        }
        self.exit_counted_capture_scope_one(&mut states.facts, range_path);
    }
}

impl Reasoning<'_, '_, '_> {
    /// Whether a kill event kills a fact supported by `term` [ENT-5].
    pub(super) fn event_kills_term(
        &self,
        separations: &dyn SeparationOracle,
        term: TermId,
        event: &KillEvent,
    ) -> bool {
        match self.vocabulary.terms.kind(term) {
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => false,
            // Counted captures and commit values are immutable. A counted
            // capture dies with its construct-scope exit, handled separately
            // from source-place write/consume events; a commit value names one
            // evaluated value that no later event can change.
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => false,
            // [ENT-5] a clause (b) place's written offsets are part of its
            // support, so a write to one kills the term exactly as it kills a
            // measure over the same place. A clause (a) place has none.
            TermKind::Place(place, _) => {
                (match event {
                    KillEvent::Write { place: written, .. }
                    | KillEvent::EntryImageHolderWrite { place: written, .. } => self
                        .input
                        .resolved_places_overlap(separations, place, written),
                    KillEvent::Consume { binding, .. } => {
                        place.root == PlaceRoot::Binding(*binding)
                    }
                    KillEvent::EntryImageHolderConsume { .. } => false,
                }) || self
                    .input
                    .event_kills_offset_support(separations, place, event)
            }
            // [MSR-2] a measure term's support is its place's DESCRIPTOR
            // storage, which is the resolved place of P itself and not of
            // P's root: a write to a sibling field of P overlaps neither.
            TermKind::Measure(measure, place) => {
                let support = self.input.resolve(place);
                self.input
                    .event_kills_measure(separations, *measure, &support, place.root, event)
                    || self
                        .input
                        .event_kills_offset_support(separations, &support, event)
            }
        }
    }

    pub(super) fn event_kills_goal(
        &self,
        separations: &dyn SeparationOracle,
        goal: GoalId,
        event: &KillEvent,
    ) -> bool {
        self.vocabulary.goals.support(goal).iter().any(|support| {
            let (place, holders) = self.input.resolve_goal_support(support);
            // [ENT-5, MSR-2] a current place reads every named offset as
            // well as the selected storage. Goals and L0 terms must lose
            // that place's identity on the same offset event.
            if self
                .input
                .event_kills_offset_support(separations, &place, event)
            {
                return true;
            }
            match event {
                // [MSR-2] a write at an element position carries the written
                // element's own place, `P[i]`, so it reaches the descriptor
                // storage of `P[i]` and none of P's own. A measure goal over
                // a place the written place is a prefix of therefore dies
                // and one over P does not, which is the same sentence
                // `event_kills_measure` reads for an L0 measure term. The
                // blanket "an element write kills no measure goal" this
                // replaces was [ENT-5]'s element-position carve-out, which
                // was only ever true of a table with no measured element
                // type.
                KillEvent::Write {
                    place: written,
                    element,
                    ..
                }
                | KillEvent::EntryImageHolderWrite {
                    place: written,
                    element,
                    ..
                } if support.measure.is_some() => {
                    // [MSR-2, WIN-2] a row naming a measure word of P --
                    // `writes(window.len)` in an [OP-10] row -- writes exactly
                    // that word of P's own descriptor, which contains no
                    // storage the place test above reaches. The L0 measure
                    // term reads the same two sentences in
                    // `event_kills_measure`, and a goal over the same measure
                    // must die on the same event: otherwise the goal-level
                    // reading of `deref(p).len` survives the operation that
                    // changed it and stands beside the row's own post-state
                    // relation as a contradiction.
                    support.measure.is_some_and(|measure| {
                        self.input.write_overlaps_measure(
                            separations,
                            written,
                            &place,
                            measure,
                            *element,
                        )
                    })
                }
                KillEvent::Write { place: written, .. }
                | KillEvent::EntryImageHolderWrite { place: written, .. } => self
                    .input
                    .resolved_places_overlap(separations, &place, written),
                KillEvent::Consume { binding, .. } => {
                    holders.contains(binding) || place.root == PlaceRoot::Binding(*binding)
                }
                KillEvent::EntryImageHolderConsume { .. } => false,
            }
        })
    }

    pub(super) fn scope_kills_goal(&self, goal: GoalId, exited: &HashSet<BindingId>) -> bool {
        self.vocabulary.goals.support(goal).iter().any(|support| {
            let (place, holders) = self.input.resolve_goal_support(support);
            holders.iter().any(|holder| exited.contains(holder))
                || matches!(place.root, PlaceRoot::Binding(binding) if exited.contains(&binding))
                || place.path.iter().any(|projection| {
                    matches!(projection, PlaceStep::Index(offset)
                        if offset.support().is_some_and(|binding| exited.contains(&binding)))
                })
        })
    }

    pub(super) fn apply_kills_one(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        events: &[KillEvent],
    ) {
        if events.is_empty() {
            return;
        }
        self.vocabulary.materialize_before_event_kill(state, events);
        state.kill(|term| {
            events
                .iter()
                .any(|event| self.event_kills_term(separations, term, event))
        });
        for event in events {
            self.kill_s12_candidates_for_event(separations, state, event);
        }
        state.kill_goals(|goal| {
            events
                .iter()
                .any(|event| self.event_kills_goal(separations, goal, event))
        });
        state.goal_origins.retain(|binding, _| {
            !events.iter().any(|event| {
                self.input
                    .event_kills_goal_origin_binding(separations, *binding, event)
            })
        });
        state.ambiguous_goal_origins.retain(|binding| {
            !events.iter().any(|event| {
                self.input
                    .event_kills_goal_origin_binding(separations, *binding, event)
            })
        });
    }

    /// One batch of kill events on the path components after the Result
    /// states: the facts, the affine images and the entry images, in that
    /// order, and the path's record of written bindings [DIAG-1]. Every kill
    /// transfer but a loop head's applies its events through here, so a new
    /// path component joins every one of them at once;
    /// [`Self::apply_loop_kills`] applies the same components, the written
    /// record included, from its summary.
    pub(super) fn kill_path_components(
        &mut self,
        separations: &dyn SeparationOracle,
        states: &mut ProofFlowState,
        events: &[KillEvent],
        shared_event: Option<FlowEventId>,
    ) {
        self.apply_kills_one(separations, &mut states.facts, events);
        self.apply_affine_kills(separations, &mut states.affine, events);
        self.invalidate_entry_images(states, separations, events, shared_event);
        states.record_writes(events);
    }

    /// [WIN-2, ENT-5] the indices the entry state of `events` proves live,
    /// and the ranges it proves to end at or below their window's length.
    ///
    /// Only an index directly below a window whose `next` or `free` one of
    /// the events writes is asked about, through the place that holds
    /// it: a term's, a goal's or an entry image's own path, whose prefix
    /// above the index is the window. The bound `i < r.len` is the one
    /// [OP-4] owed where the subscript was formed, judged again here over the
    /// same terms. A prefix that resolves to more than one place names no one
    /// window whose length a proof bounds, so its index stays unproved.
    ///
    /// A range is asked about in the resolved places of the same holders,
    /// since a range step is reached through the reference its formation
    /// bound: `hi <= r.len` is the [REF-4] bound owed where it was formed,
    /// judged again here.
    pub(super) fn event_live_bounds(
        &mut self,
        states: &ProofFlowState,
        events: &[KillEvent],
    ) -> LiveBounds {
        let mut written_parts = HashSet::new();
        for event in events {
            let (KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. }) =
                event
            else {
                continue;
            };
            for written in self.input.places.resolve(place.root, &place.path) {
                for (depth, step) in written.path.iter().enumerate() {
                    // [WIN-2] every position overlaps `last` whatever its
                    // bound, so a write of it asks for none.
                    if matches!(step, PlaceStep::Part(WindowPart::Next | WindowPart::Free)) {
                        written_parts.insert((written.root, depth));
                    }
                }
            }
        }
        let mut live = LiveBounds::default();
        if written_parts.is_empty() {
            return live;
        }
        let mut places = Vec::new();
        for term in self.vocabulary.terms.ids() {
            if let TermKind::Place(place, _) | TermKind::Measure(_, place) =
                self.vocabulary.terms.kind(term)
            {
                places.push(place.clone());
            }
        }
        for goal in self.vocabulary.goals.ids() {
            for support in self.vocabulary.goals.support(goal) {
                places.push(self.input.resolve_goal_support(support).0);
            }
        }
        places.extend(
            self.input
                .entry_images
                .iter()
                .map(|image| image.place.clone()),
        );
        let mut asked = HashSet::new();
        for place in &places {
            for (depth, step) in place.path.iter().enumerate() {
                let PlaceStep::Index(index) = *step else {
                    continue;
                };
                let window = ResolvedPlace {
                    root: place.root,
                    path: place.path[..depth].to_vec(),
                };
                if !asked.insert((window.clone(), index)) {
                    continue;
                }
                let resolved = self.input.places.resolve(window.root, &window.path);
                let [resolved] = resolved.as_slice() else {
                    continue;
                };
                if written_parts.contains(&(resolved.root, resolved.path.len()))
                    && self.index_live_proof(&window, index, states).is_some()
                {
                    live.indices.insert((resolved.clone(), index));
                }
            }
        }
        let mut asked_ranges = HashSet::new();
        for place in &places {
            for resolved in self.input.places.resolve(place.root, &place.path) {
                for (depth, step) in resolved.path.iter().enumerate() {
                    let PlaceStep::Range(range) = *step else {
                        continue;
                    };
                    let window = ResolvedPlace {
                        root: resolved.root,
                        path: resolved.path[..depth].to_vec(),
                    };
                    if written_parts.contains(&(window.root, depth))
                        && asked_ranges.insert((window.clone(), range))
                        && self
                            .range_within_length_proof(&window, range, states)
                            .is_some()
                    {
                        live.ranges.insert((window, range));
                    }
                }
            }
        }
        live
    }

    /// [WIN-2] the proof `states` gives of `index < len(window)`: the bound
    /// a subscript's [OP-4] obligation states where its place is formed, over
    /// the same length term and the same offset.
    pub(super) fn index_live_proof(
        &mut self,
        window: &ResolvedPlace,
        index: CapturedValue,
        states: &ProofFlowState,
    ) -> Option<ProofResult> {
        let length = self
            .vocabulary
            .terms
            .interned(&TermKind::Measure(CheckedMeasure::Length, window.clone()))?;
        let offset = match (index.capture, index.term) {
            // A place's term identity reads a binding offset by its spelling,
            // whose current value is the binding's own term [ENT-2].
            (CaptureId::SpellingDetermined, CapturedTerm::Binding(binding)) => {
                self.vocabulary.terms.interned(&TermKind::Place(
                    ResolvedPlace::binding(binding),
                    IntegerType::U64,
                ))
            }
            _ => self.captured_index_term(index),
        }?;
        let affine_offset = match index.capture {
            CaptureId::Source(_) => states.affine.indices.get(&index.capture).cloned(),
            _ => None,
        }
        .or_else(|| self.vocabulary.affine_term_value(offset, &states.affine));
        let direct_affine = affine_offset.as_ref().and_then(|offset| {
            let length = self.vocabulary.measure_atom(length, &states.affine);
            AffineInequality::from_bounded_forms(offset, &length, -1, &mut AffineCheckState::new())
                .ok()
        });
        let proof = self.prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: None,
                request: Some(BoundsRequest {
                    left: Some(offset),
                    right: length,
                    bound: -1,
                    distinct: false,
                }),
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge: None,
                affine_left: affine_offset.as_ref(),
            }),
        );
        (proof.disposition == ProofDisposition::Proved).then_some(proof)
    }

    pub(super) fn invalidate_entry_images(
        &mut self,
        states: &mut ProofFlowState,
        separations: &dyn SeparationOracle,
        events: &[KillEvent],
        shared_event: Option<FlowEventId>,
    ) {
        if self.input.entry_images.is_empty() {
            return;
        }
        for event in events {
            let killed = self
                .input
                .entry_images
                .iter()
                .enumerate()
                .filter_map(|(index, image)| {
                    // [MSR-3] a measure operand denotes the entry datum, and
                    // no [ENT-5] event kills a datum. Only a non-measure
                    // operand still reads the live place and can lose it.
                    (image.datum.measure.is_none()
                        && states.entry_images[index].is_none()
                        && self
                            .input
                            .event_kills_entry_image(separations, image, event))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            if killed.is_empty() {
                continue;
            }
            let invalidation = shared_event.unwrap_or_else(|| {
                self.vocabulary.proof_event(
                    FlowEventKind::PostconditionEntryImageInvalidation,
                    Some(event.source()),
                )
            });
            for index in killed {
                states.entry_images[index] = Some(invalidation);
            }
        }
    }

    pub(super) fn apply_affine_kills(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut AffineFlowState,
        events: &[KillEvent],
    ) {
        state.values.retain(|binding, _| {
            !events.iter().any(|event| {
                self.input
                    .affine_event_kills_binding(separations, *binding, event)
            })
        });
        // [MSR-2] a measure atom is retargeted on exactly the events that
        // kill its term, exactly as a local's image is retargeted by a write
        // to that local: the next occurrence of the term mints a fresh atom,
        // so no conclusion published over the old one reaches past the write.
        let stale: Vec<TermId> = state
            .measure_atoms
            .borrow()
            .keys()
            .copied()
            .filter(|term| {
                events
                    .iter()
                    .any(|event| self.event_kills_term(separations, *term, event))
            })
            .collect();
        for term in stale {
            state.measure_atoms.get_mut().remove(&term);
        }
        // An opaque handle names one binding's value at the point the fold
        // took it, so it dies with a write to that binding exactly as the
        // binding's own image does [ENT-5].
        state.opaque_values.retain(|binding, _| {
            !events.iter().any(|event| {
                self.input
                    .affine_event_kills_binding(separations, *binding, event)
            })
        });
        // Published facts name immutable AffineTermId value identities, not
        // mutable bindings. Removing the map above prevents a replacement
        // value from matching an old image; retaining each theorem preserves
        // valid aliases to the old value.
    }
}

impl Analyzer<'_, '_> {
    pub(super) fn apply_kills(&mut self, states: &mut ProofFlowState, events: &[KillEvent]) {
        if events.is_empty() {
            return;
        }
        self.record_continuing(states, events);
        self.vocabulary.promote_flow_contradiction(states);
        let ledger = states.separations.clone();
        let live = self.reasoning().event_live_bounds(states, events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.reasoning().kill_result_evidence(states, events);
        self.reasoning()
            .kill_path_components(&separations, states, events, None);
    }

    /// Applies each event on its own and in order, invalidating entry images
    /// under the transfer event `transfer_event` mints for it before it
    /// applies: the form a postcondition-carrying call or receiver write
    /// needs, where [`Self::apply_kills`] applies one batch. Returns the
    /// indices the events' entry state proves live [WIN-2].
    pub(super) fn apply_kills_each(
        &mut self,
        states: &mut ProofFlowState,
        events: &[KillEvent],
        mut transfer_event: impl FnMut(&mut Self, &KillEvent) -> FlowEventId,
    ) -> LiveBounds {
        if !events.is_empty() {
            self.vocabulary.promote_flow_contradiction(states);
        }
        self.record_continuing(states, events);
        let ledger = states.separations.clone();
        let live = self.reasoning().event_live_bounds(states, events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.reasoning().kill_result_evidence(states, events);
        for event in events {
            let proof_event = transfer_event(self, event);
            self.reasoning().kill_path_components(
                &separations,
                states,
                std::slice::from_ref(event),
                Some(proof_event),
            );
        }
        live
    }

    /// Keeps each event applied on a path inside a loop, where debug
    /// assertions are on, for the back-edge check against that loop's summary.
    pub(super) fn record_continuing(&self, states: &mut ProofFlowState, events: &[KillEvent]) {
        if cfg!(debug_assertions) && !self.frames.loops.is_empty() {
            record_continuing(&mut states.continuing, events);
        }
    }

    /// Applies the scope-exit kills for every scope deeper than `depth`,
    /// as the edge event ordered before any join [ENT-5].
    pub(super) fn exit_scopes_to_one(&mut self, state: &mut FactState, depth: usize) {
        let exited: HashSet<BindingId> = self
            .frames
            .scopes
            .iter()
            .skip(depth)
            .flatten()
            .copied()
            .collect();
        if exited.is_empty() {
            return;
        }
        state.kill(|term| self.vocabulary.scope_kills_term(term, &exited));
        self.reasoning()
            .kill_s12_candidates_for_scope(state, &exited);
        state.kill_goals(|goal| self.reasoning().scope_kills_goal(goal, &exited));
        state.origins.retain(|binding, _| !exited.contains(binding));
        state
            .goal_origins
            .retain(|binding, _| !exited.contains(binding));
        state
            .ambiguous_goal_origins
            .retain(|binding| !exited.contains(binding));
    }

    /// Applies only the lexical support kills. Delivery images call this
    /// directly because they were already constructed from a closed source
    /// state and must retain only their explicit PostconditionGive roots.
    pub(super) fn kill_scopes_to(&mut self, states: &mut ProofFlowState, depth: usize) {
        self.vocabulary.promote_flow_contradiction(states);
        self.exit_scope_components(states, depth);
    }

    /// The scope-exit kills for every scope deeper than `depth` on each path
    /// component: the Result states, the facts and the affine images, in that
    /// order. Both scope transfers apply them through here.
    pub(super) fn exit_scope_components(&mut self, states: &mut ProofFlowState, depth: usize) {
        self.exit_result_scopes(states, depth);
        self.exit_scopes_to_one(&mut states.facts, depth);
        self.exit_affine_scopes_to(&mut states.affine, depth);
    }

    pub(super) fn exit_affine_scopes_to(&mut self, state: &mut AffineFlowState, depth: usize) {
        let exited = self
            .frames
            .scopes
            .iter()
            .skip(depth)
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        state.values.retain(|binding, _| !exited.contains(binding));
        // A measure of a place rooted in an exited binding has no image past
        // that edge, exactly as the binding itself has none, so the next
        // occurrence of that term mints a fresh atom [MSR-2].
        let stale: Vec<TermId> = state
            .measure_atoms
            .borrow()
            .keys()
            .copied()
            .filter(|term| {
                self.vocabulary
                    .measure_term_root(*term)
                    .is_some_and(|binding| exited.contains(&binding))
            })
            .collect();
        for term in stale {
            state.measure_atoms.get_mut().remove(&term);
        }
    }

    pub(super) fn exit_scopes_to(&mut self, states: &mut ProofFlowState, depth: usize) {
        let has_exited_bindings = self
            .frames
            .scopes
            .iter()
            .skip(depth)
            .any(|scope| !scope.is_empty());
        if !has_exited_bindings {
            return;
        }
        // [ENT-4, ENT-5]: a local term may be the middle vertex of a proof
        // whose conclusion names only values that remain live. Fix the least
        // closure while that vertex still exists, then let the ordinary scope
        // kill remove every materialized fact whose own support still names
        // the exiting scope.
        let snapshot = self
            .vocabulary
            .derivations
            .event(FlowEventKind::Snapshot, None);
        states.facts = materialize_closure_at(
            &states.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
            snapshot,
        );
        // Materialization has already promoted any relation or goal
        // contradiction. Apply only the endpoint projection here.
        self.exit_scope_components(states, depth);
    }

    /// Applies capture-scope kills for every loop frame crossed by a
    /// non-local edge. Ordinary loop frames carry no private captures.
    pub(super) fn exit_counted_loops_from(
        &mut self,
        states: &mut ProofFlowState,
        loop_depth: usize,
    ) {
        let loops = self
            .frames
            .loops
            .iter()
            .skip(loop_depth)
            .map(|frame| {
                (
                    frame.id,
                    frame.capture_path.clone(),
                    frame.invariant_declarations.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (loop_id, path, declarations) in loops {
            if let Some(path) = path {
                self.vocabulary.exit_counted_capture_scope(states, &path);
            }
            remove_active_loop_invariants(&mut states.affine, loop_id, &declarations);
        }
    }
}

/// Adds `events` to a path's continuing record, each once.
pub(super) fn record_continuing(continuing: &mut Vec<KillEvent>, events: &[KillEvent]) {
    for event in events {
        if !continuing.contains(event) {
            continuing.push(event.clone());
        }
    }
}

/// [REF-1] a reference variable is not storage of its own.
///
/// v0.59 spelled a holder's referent by inserting a `deref` step into
/// every term over it. v0.60 resolves the root instead: a place rooted at
/// a reference variable is replaced by the path that reference names, so
/// no step is synthesized here and the checked tree's own `deref` nodes
/// are the only ones a path carries [TYPE-7].
pub(super) const fn is_holder(_binding: BindingId) -> bool {
    false
}
