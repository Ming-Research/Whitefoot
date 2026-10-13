//! Call effects, entry-version frames and selected exit obligations [RANGE-2, RANGE-3].
use super::*;

impl Walker<'_> {
    /// [RANGE-2] Snapshot only entry-rooted places, before any effect. The
    /// stored views include descriptor generations and resolved alias paths.
    pub(super) fn snapshot_entry_places(
        &mut self,
        state: &mut State,
        clause: &CheckedRangeClause,
        roots: &dyn Fn(CheckedRangeRoot) -> Option<Value>,
    ) -> BTreeMap<CheckedRangePlace, PlaceView> {
        let mut segments = BTreeSet::new();
        for binder in &clause.binders {
            segment_places(&binder.start, &mut segments);
            segment_places(&binder.end, &mut segments);
        }
        for relation in clause.relations() {
            segment_places(&relation.left, &mut segments);
            segment_places(&relation.right, &mut segments);
        }
        clause
            .places()
            .into_iter()
            .filter(|place| matches!(place.root, CheckedRangeRoot::Entry(_)))
            .map(|place| {
                let view = self.place_view(state, &place, segments.contains(&place), roots);
                (place, view)
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn call(
        &mut self,
        state: &mut State,
        function: crate::semantic::model::FunctionId,
        call: &NodePath,
        arguments: &[CheckedExpression],
        result: CheckedType,
        formal: Option<&crate::semantic::model::CheckedEffects>,
        contract: Option<&crate::semantic::model::CheckedCallContract>,
    ) -> Value {
        let mut values = Vec::with_capacity(arguments.len());
        let mut argument_reads = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.stored_value(state, argument));
            argument_reads.push(self.last_reads.clone());
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
        let requirements = contract.map_or(callee.requirements.as_slice(), |contract| {
            contract.requirements.as_slice()
        });
        for requirement in requirements {
            let subject = ObligationSubject::CallRequirement {
                callee: function,
                requires_clause: requirement.clause.clone(),
                subject: requirement.subject,
            };
            if self.has_ordinary(call, &subject) {
                let goal = self.goal_comparisons_with(
                    state,
                    &requirement.template.root,
                    callee,
                    &values,
                    &argument_reads,
                );
                self.ordinary_goal_with_reads(
                    state,
                    call,
                    &subject,
                    goal.as_ref()
                        .map(|goal| goal.literals.as_slice())
                        .map_err(|failure| *failure),
                    Some(&[]),
                    ordinary::GoalReads::PerComparison(
                        goal.as_ref().map_or(&[], |goal| goal.reads.as_slice()),
                    ),
                );
            }
        }
        // [RANGE-3] the callee's range requirements, at the call.
        if self.dry == 0 {
            for clause in &callee.range_facts.requirements {
                let mut scratch = state.clone();
                let frame = self.frame(&mut scratch, clause, &argument_of);
                self.require(state, clause, &frame, call, "a call");
            }
        }
        // [RANGE-2] Entry roots refer to the argument storage before effects,
        // including prelude writes and descriptor changes. Framing now also
        // registers every named container with the ordinary forgetting walk.
        let entry_frames: Vec<_> = callee
            .range_facts
            .postconditions
            .iter()
            .map(|post| {
                self.snapshot_entry_places(state, &post.clause, &|root| match root {
                    CheckedRangeRoot::Entry(binding) => {
                        argument_of(CheckedRangeRoot::Binding(binding))
                    }
                    _ => argument_of(root),
                })
            })
            .collect();
        let window_result = (formal.is_none()
            && self.prelude.contains(&callee.declaration)
            && matches!(callee.name.as_str(), "place_back" | "take_back"))
        .then(|| self.place_window_call(state, call, &callee.name, &values, result))
        .flatten();
        let relocated_window = formal.is_none()
            && self.prelude.contains(&callee.declaration)
            && matches!(callee.name.as_str(), "grow" | "grow_paged")
            && self.relocate_window_call(state, call, &values);
        // Resolve every row entry in the call-entry state: a descriptor
        // write must not change the index selected by another `.next` entry.
        let writes = formal.map_or(callee.declared_state_writes.as_slice(), |row| {
            row.writes.as_slice()
        });
        let reads = formal.map_or(callee.declared_state_reads.as_slice(), |row| {
            row.reads.as_slice()
        });
        let mut targets = Vec::new();
        for (position, parameter) in callee.parameters.iter().enumerate() {
            if !parameter.mode.is_reference()
                || ((window_result.is_some() || relocated_window) && position == 0)
            {
                continue;
            }
            let written = writes.iter().any(|path| path.root == parameter.declaration);
            let Some(Value::Ref(view)) = values.get(position).cloned() else {
                if written {
                    targets.push(effects::Write::view(View::Unknown));
                }
                continue;
            };
            // RANGE-5 retains the complete actual's conservative access.
            if written || reads.iter().any(|path| path.root == parameter.declaration) {
                self.unplaced_view(&view, written, call, state);
            }
            for path in writes
                .iter()
                .filter(|path| path.root == parameter.declaration)
            {
                targets.push(self.call_write(state, callee, &values, position, &path.steps, call));
            }
        }
        for target in targets {
            self.forget_call_write(state, target);
        }
        let value = window_result.unwrap_or_else(|| match result {
            CheckedType::Integer(integer) => Value::Int(self.world.opaque(Some(integer))),
            CheckedType::Bool => self.world.boolean(Cond::Unknown),
            CheckedType::Unit | CheckedType::Float(_) => Value::Unknown,
            _ => Value::Owned(Location::root(Origin::CallResult(self.world.new_origin()))),
        });
        self.assume_postconditions(state, callee, &values, &value, &entry_frames);
        value
    }

    /// [RANGE-2] a callee's postconditions after a call: each parameter
    /// denotes its argument, a reference's storage as the call left it, and
    /// the result the call's result; a routed one holds in the arm of a
    /// `match` on the result that takes its variant.
    pub(super) fn assume_postconditions(
        &mut self,
        state: &mut State,
        callee: &CheckedFunction,
        arguments: &[Value],
        result: &Value,
        entry_frames: &[BTreeMap<CheckedRangePlace, PlaceView>],
    ) {
        let parameters: Vec<BindingId> = callee
            .parameters
            .iter()
            .map(|parameter| parameter.binding)
            .collect();
        for (post, entry_frame) in callee.range_facts.postconditions.iter().zip(entry_frames) {
            let mut results = self.result_values(state, Some(result), &post.results);
            let mut routed = None;
            if let Some(route) = post.route {
                let index = route.ordinal as usize;
                let Some(Some(Value::Owned(location))) = results.get(index).cloned() else {
                    continue;
                };
                let at = state.resolve(&location);
                let payload = at.child(Step::Payload {
                    variant: route.tag,
                    variants: self
                        .variant_count(post.results.get(index).copied().unwrap_or(callee.result)),
                    field: route.field,
                });
                results[index] = Some(self.read_location(state, &payload, route.payload));
                routed = Some((at, route.tag));
            }
            let roots = |root: CheckedRangeRoot| match root {
                CheckedRangeRoot::Binding(binding) => parameters
                    .iter()
                    .position(|parameter| *parameter == binding)
                    .and_then(|position| arguments.get(position).cloned()),
                CheckedRangeRoot::Result(ordinal) => results.get(ordinal as usize).cloned()?,
                CheckedRangeRoot::Entry(_) => None,
            };
            let mut frame = self.frame(state, &post.clause, &roots);
            frame.places.extend(entry_frame.clone());
            let id = self.add_fact(post.clause.clone(), frame);
            match routed {
                None => state.facts.push(id),
                Some((at, tag)) => state.routed.push((at, tag, id, Vec::new())),
            }
        }
    }

    /// [CALL-4] each result ordinal a postcondition may name in `value`: the
    /// fields of a result-list value, in ordinal order, or `value` itself
    /// for a single result; `None` where there is no value.
    pub(super) fn result_values(
        &mut self,
        state: &mut State,
        value: Option<&Value>,
        results: &[CheckedType],
    ) -> Vec<Option<Value>> {
        if results.is_empty() {
            return vec![value.cloned()];
        }
        let mut out = Vec::with_capacity(results.len());
        for (ordinal, ty) in (0_u32..).zip(results) {
            out.push(value.map(|value| {
                match value {
                    Value::Owned(location) => {
                        self.read_location(state, &location.child(Step::Field(ordinal)), *ty)
                    }
                    Value::Struct(fields) => fields
                        .get(ordinal as usize)
                        .cloned()
                        .unwrap_or(Value::Unknown),
                    _ => Value::Unknown,
                }
            }));
        }
        out
    }

    /// The selected variant makes precisely its held range relations
    /// available, both at a match arm and a forwarded return [FN-9].
    pub(super) fn activate_routed_facts(&self, state: &mut State, at: &Location, tag: u32) {
        let enabled: Vec<_> = state
            .routed
            .iter()
            .filter(|(location, variant, ..)| location == at && *variant == tag)
            .map(|(_, _, fact, guards)| (*fact, guards.clone()))
            .collect();
        for (fact, guards) in enabled {
            if guards.is_empty() {
                state.facts.push(fact);
            } else {
                state.guarded.push(Guarded { fact, guards });
            }
        }
    }

    /// [ERR-3, FN-9] the propagated error returns its evaluated payload.
    /// Its fork selects only the operand's Err relations; none reaches the
    /// successful continuation.
    pub(super) fn owe_propagated_postconditions(
        &mut self,
        state: &State,
        value: Value,
        error_type: CheckedType,
        node: &NodePath,
    ) {
        let CheckedType::Nominal(nominal) = self.function.result else {
            return;
        };
        let CheckedNominalKind::Enum { variants } = &self.nominals[nominal.0 as usize].kind else {
            return;
        };
        let Some(error) = variants.iter().find(|variant| {
            variant.constructor
                == super::super::super::model::CheckedConstructor::Prelude(
                    crate::BuiltinPreludeId::ERR,
                )
        }) else {
            return;
        };
        let tag = error.tag;
        let variants = variants.len() as u32;
        let mut forked = state.clone();
        let payload = match value {
            Value::Variant {
                variant, fields, ..
            } => {
                if variant != tag {
                    return;
                }
                fields.first().cloned().unwrap_or(Value::Unknown)
            }
            Value::Owned(location) => {
                let at = forked.resolve(&location);
                if forked.variants.get(&at).is_some_and(|known| *known != tag) {
                    return;
                }
                self.activate_routed_facts(&mut forked, &at, tag);
                let payload = at.child(Step::Payload {
                    variant: tag,
                    variants,
                    field: 0,
                });
                self.read_location(&mut forked, &payload, error_type)
            }
            _ => Value::Unknown,
        };
        let returned = Value::Variant {
            variant: tag,
            variants,
            fields: vec![payload],
        };
        self.owe_postconditions(
            &mut forked,
            Some(&returned),
            node,
            "a propagated error exit",
        );
    }

    /// [RANGE-3] the function's range postconditions an exit selects
    /// [FN-9]: `returned` is its value, including the propagated Err value.
    pub(super) fn owe_postconditions(
        &mut self,
        state: &mut State,
        returned: Option<&Value>,
        node: &NodePath,
        site: &'static str,
    ) {
        if self.dry > 0 {
            return;
        }
        let function = self.function;
        for (index, post) in function
            .range_facts
            .postconditions
            .iter()
            .enumerate()
            .filter(|(_, post)| post.owed)
        {
            // Each route assumes only its own variant. Its activated facts
            // must not contribute to another route's exit judgment.
            let mut selected_state = state.clone();
            let state = &mut selected_state;
            if post.route.is_some() && returned.is_none() {
                continue;
            }
            let mut results = self.result_values(state, returned, &post.results);
            if let Some(route) = post.route {
                let index = route.ordinal as usize;
                let payload = match results.get(index).cloned().flatten() {
                    Some(Value::Variant {
                        variant, fields, ..
                    }) => {
                        if variant != route.tag {
                            continue;
                        }
                        fields.get(route.field as usize).cloned()
                    }
                    Some(Value::Owned(location)) => {
                        let at = state.resolve(&location);
                        if state
                            .variants
                            .get(&at)
                            .is_some_and(|known| *known != route.tag)
                        {
                            continue;
                        }
                        self.activate_routed_facts(state, &at, route.tag);
                        let payload = at.child(Step::Payload {
                            variant: route.tag,
                            variants: self.variant_count(
                                post.results.get(index).copied().unwrap_or(function.result),
                            ),
                            field: route.field,
                        });
                        Some(self.read_location(state, &payload, route.payload))
                    }
                    _ => None,
                };
                if let Some(slot) = results.get_mut(index) {
                    *slot = payload;
                }
            }
            if let Some(selected) = self.selected.get_mut(index) {
                *selected = true;
            }
            let entry = self.entry.clone();
            let roots = |root: CheckedRangeRoot| match root {
                CheckedRangeRoot::Binding(binding) => entry.get(&binding).cloned(),
                CheckedRangeRoot::Result(ordinal) => results.get(ordinal as usize).cloned()?,
                CheckedRangeRoot::Entry(_) => None,
            };
            let mut frame = self.frame(state, &post.clause, &roots);
            frame.places.extend(self.entry_places.clone());
            for (root, version) in &self.entry_aggregates {
                if let Some(entry) = frame.aggregates.get_mut(root) {
                    *entry = *version;
                }
            }
            self.require(state, &post.clause, &frame, node, site);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::tests::with_semantics;

    /// [RANGE-2] A call frame uses the argument version before ordinary
    /// effect forgetting, and retains its old descriptor generation too.
    #[test]
    fn imported_entry_places_precede_call_effects() {
        let source = br#"fn preserve(xs: &Box<Slots<u64>>) -> result: unit writes(xs) contract {
  ensures forall kept(k in 0_u64..entry(xs)^.inner.len): xs^.inner[k] unchanged;
} {
  let capacity = xs^.inner.cap;
  grow(cell: xs, capacity: capacity);
  return unit;
}

fn probe(items: &Box<Slots<u64>>) -> result: unit writes(items) {
  preserve(xs: items);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
        with_semantics(source, |outcome| {
            let crate::SemanticOutcome::Complete(program) = outcome else {
                panic!("the element-preserving relocation body must check: {outcome:?}");
            };
            let data = &program.data;
            let functions: Vec<_> = data.functions.iter().collect();
            let probe = data
                .executable_functions()
                .find(|function| function.name == "probe")
                .unwrap();
            let prelude = BTreeSet::new();
            let mut walker = Walker::new(
                &functions,
                &data.nominals,
                &data.elements,
                probe,
                Vec::new(),
                &data.constants,
                JudgmentScope::Concrete,
                &prelude,
            );
            walker.run();
            assert!(walker.issues.is_empty());
            let frame = &walker
                .facts
                .iter()
                .find(|fact| fact.clause.name == "kept")
                .unwrap()
                .frame;
            let (entry, exit) = (
                frame
                    .places
                    .iter()
                    .find(|(place, _)| matches!(place.root, CheckedRangeRoot::Entry(_)))
                    .unwrap()
                    .1,
                frame
                    .places
                    .iter()
                    .find(|(place, _)| matches!(place.root, CheckedRangeRoot::Binding(_)))
                    .unwrap()
                    .1,
            );
            let (
                PlaceView::Run {
                    version: entry_version,
                    generation: entry_generation,
                    ..
                },
                PlaceView::Run {
                    version: exit_version,
                    generation: exit_generation,
                    ..
                },
            ) = (entry, exit)
            else {
                panic!("both places denote the argument's window")
            };
            assert_ne!(
                entry_version, exit_version,
                "the row forgets the exit contents only"
            );
            assert_ne!(
                entry_generation, exit_generation,
                "the old length is a pre-call image"
            );
            assert!(matches!(
                walker.world.versions[*entry_version as usize].def,
                VersionDef::Initial
            ));
            assert!(matches!(
                walker.world.versions[*exit_version as usize].def,
                VersionDef::Fresh
            ));
        });
    }
}
