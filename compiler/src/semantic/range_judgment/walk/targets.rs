//! Where a storage path leads: the place a write lands on or a reference
//! names, one of several when a reference was joined [REF-1, RANGE-2].
use super::*;

/// Where one storage path leads.
pub(super) enum Target {
    Location(Location),
    /// A run reached through a range reference, before its subscript.
    Run {
        container: ContainerId,
        prefix: Vec<Linear>,
        offset: Linear,
        length: Linear,
        projection: Vec<CheckedRangeProjection>,
    },
    /// The outermost element and the owned selection below it.
    Element {
        container: ContainerId,
        indices: Vec<Linear>,
        projection: Option<Vec<CheckedRangeProjection>>,
    },
    /// One segment of a `Segments`, before its element subscript.
    Row {
        container: ContainerId,
        row: Linear,
    },
    /// [REF-1] Any one of these, reached through a reference that names
    /// several places; never nested and never holding an unknown member.
    Either(Vec<Target>),
    Unknown,
}

impl Target {
    /// The target naming any one of `targets`: an unknown member leaves the
    /// place unknown, and a single member is that member.
    pub(super) fn either(targets: Vec<Target>) -> Target {
        let mut members = Vec::with_capacity(targets.len());
        for target in targets {
            match target {
                Target::Unknown => return Target::Unknown,
                Target::Either(nested) => members.extend(nested),
                other => members.push(other),
            }
        }
        match members.len() {
            0 => Target::Unknown,
            1 => members.pop().unwrap_or(Target::Unknown),
            _ => Target::Either(members),
        }
    }
}

impl Walker<'_> {
    /// [RANGE-2] A write that may land on any of several places leaves each
    /// of them unknown where the write overlaps it, and keeps the rest.
    pub(super) fn forget_target(&mut self, state: &mut State, target: Target, node: &NodePath) {
        match target {
            Target::Element {
                container,
                indices,
                projection,
            } => {
                self.access(container, &indices, true, node, state);
                state.write_element(
                    &mut self.world,
                    container,
                    indices,
                    projection.unwrap_or_default(),
                    BTreeMap::new(),
                );
            }
            Target::Location(location) => {
                let location = state.resolve(&location);
                state.havoc_location(&mut self.world, &location);
                self.unplaced_location(state, &location, node);
            }
            Target::Row { container, .. } | Target::Run { container, .. } => {
                state.havoc_container(&mut self.world, container, false);
                self.unplaced(Some(container), true, false);
            }
            Target::Either(members) => {
                for member in members {
                    self.forget_target(state, member, node);
                }
            }
            Target::Unknown => self.forget_all(state, node),
        }
    }

    /// Where one storage path leads.
    pub(super) fn path_target(
        &mut self,
        state: &mut State,
        root: PlaceRoot,
        path: &[CheckedPlaceStep],
    ) -> Target {
        let value = match root {
            PlaceRoot::Binding(binding) => state.values.get(&binding).cloned(),
            PlaceRoot::Constant(_) => None,
        };
        let mut target = match value {
            Some(Value::Owned(location)) => Target::Location(location),
            Some(Value::Ref(View::Place(location))) => Target::Location(location),
            Some(Value::Ref(View::Run {
                container,
                prefix,
                offset,
                length,
                projection,
                ..
            })) => Target::Run {
                container,
                prefix,
                offset,
                length,
                projection,
            },
            Some(Value::Ref(View::Element {
                container,
                indices,
                projection,
                ..
            })) => Target::Element {
                container,
                indices,
                projection,
            },
            Some(Value::Ref(View::Either(members))) => {
                Target::either(members.iter().map(super::effects::effect_target).collect())
            }
            _ => Target::Unknown,
        };
        // Even an unmodelled root has evaluated offsets and typed bounds.
        // A constant array's fixed length, for example, needs no storage view.
        for step in path {
            let evaluated = if let CheckedPlaceStep::Subscript(subscript) = step {
                let index = self.int(state, &subscript.offset);
                // [OP-4] The bound holds at whichever place the path names.
                if let Target::Either(members) = &target {
                    for member in members {
                        let length = self.target_length(state, member, subscript.base_type);
                        self.ordinary_bound(state, &subscript.obligation, &index, length);
                    }
                } else {
                    let length = self.target_length(state, &target, subscript.base_type);
                    self.ordinary_bound(state, &subscript.obligation, &index, length);
                }
                Some(index)
            } else {
                None
            };
            target = self.step_target(state, target, step, evaluated.as_ref());
        }
        target
    }

    /// Where one step of a storage path leads from `target`; `evaluated` is
    /// the step's subscript value, evaluated once for every member.
    fn step_target(
        &mut self,
        state: &mut State,
        target: Target,
        step: &CheckedPlaceStep,
        evaluated: Option<&Linear>,
    ) -> Target {
        match (target, step) {
            (Target::Either(members), step) => {
                let stepped = members
                    .into_iter()
                    .map(|member| self.step_target(state, member, step, evaluated))
                    .collect();
                Target::either(stepped)
            }
            (Target::Location(location), CheckedPlaceStep::Field(field)) => {
                Target::Location(location.child(Step::Field(*field)))
            }
            (
                Target::Location(location),
                CheckedPlaceStep::Payload {
                    nominal,
                    variant,
                    field,
                },
            ) => match &self.nominals[nominal.0 as usize].kind {
                CheckedNominalKind::Enum { variants } => {
                    Target::Location(location.child(Step::Payload {
                        variant: *variant,
                        field: *field,
                        variants: variants.len() as u32,
                    }))
                }
                _ => unreachable!(),
            },
            (Target::Location(location), CheckedPlaceStep::BoxReferent(_)) => {
                Target::Location(location.child(Step::BoxContent))
            }
            (Target::Location(location), CheckedPlaceStep::Subscript(subscript)) => {
                let index = evaluated.cloned().unwrap_or_else(|| unreachable!());
                let location = state.resolve(&location);
                if matches!(subscript.base_type, CheckedType::Segments { .. }) {
                    match state.container(&mut self.world, location, 2) {
                        Some(container) => Target::Row {
                            container,
                            row: index,
                        },
                        None => Target::Unknown,
                    }
                } else {
                    match state.container(&mut self.world, location, 1) {
                        Some(container) => Target::Element {
                            container,
                            indices: vec![index],
                            projection: Some(Vec::new()),
                        },
                        None => Target::Unknown,
                    }
                }
            }
            (Target::Row { container, row }, CheckedPlaceStep::Subscript(_)) => {
                let index = evaluated.cloned().unwrap_or_else(|| unreachable!());
                Target::Element {
                    container,
                    indices: vec![row, index],
                    projection: Some(Vec::new()),
                }
            }
            (
                Target::Run {
                    container,
                    prefix,
                    offset,
                    projection,
                    ..
                },
                CheckedPlaceStep::Subscript(_),
            ) => {
                let index = evaluated.cloned().unwrap_or_else(|| unreachable!());
                let mut indices = prefix;
                match offset.plus(&index) {
                    Some(absolute) => {
                        indices.push(absolute);
                        Target::Element {
                            container,
                            indices,
                            projection: Some(projection),
                        }
                    }
                    None => Target::Unknown,
                }
            }
            (
                Target::Element {
                    container,
                    mut indices,
                    mut projection,
                },
                step,
            ) => {
                if matches!(step, CheckedPlaceStep::Subscript(_)) {
                    if let Some(path) = &mut projection {
                        path.push(CheckedRangeProjection::Index(indices.len() as u32));
                    }
                    indices.push(evaluated.cloned().unwrap_or_else(|| unreachable!()));
                } else {
                    self.project_element(state, &mut projection, step);
                }
                Target::Element {
                    container,
                    indices,
                    projection,
                }
            }
            (_, CheckedPlaceStep::Subscript(subscript)) => {
                let _ = subscript;
                Target::Unknown
            }
            _ => Target::Unknown,
        }
    }

    /// Unsupported descendants still access their outer element, but cannot
    /// supply a projected scalar or a narrower write footprint.
    pub(super) fn project_element(
        &mut self,
        state: &mut State,
        projection: &mut Option<Vec<CheckedRangeProjection>>,
        step: &CheckedPlaceStep,
    ) {
        match step {
            CheckedPlaceStep::Field(field) => {
                if let Some(path) = projection {
                    path.push(CheckedRangeProjection::Field(*field));
                }
            }
            CheckedPlaceStep::Payload {
                nominal,
                variant,
                field,
            } => {
                let CheckedNominalKind::Enum { variants } = &self.nominals[nominal.0 as usize].kind
                else {
                    unreachable!();
                };
                if let Some(path) = projection {
                    path.push(CheckedRangeProjection::Payload {
                        variant: *variant,
                        field: *field,
                        variants: variants.len() as u32,
                    });
                }
            }
            CheckedPlaceStep::BoxReferent(_) => {
                if let Some(path) = projection {
                    path.push(CheckedRangeProjection::BoxContent);
                }
            }
            CheckedPlaceStep::Measure(_) => {
                *projection = None;
            }
            CheckedPlaceStep::Subscript(subscript) => {
                let _ = self.int(state, &subscript.offset);
                *projection = None;
            }
        }
    }

    /// The reference view of a storage target.
    pub(super) fn target_view(
        &mut self,
        state: &mut State,
        target: Target,
        barrier: Option<RangeBarrier>,
    ) -> View {
        match target {
            Target::Either(members) => {
                let views: Vec<View> = members
                    .into_iter()
                    .map(|member| self.target_view(state, member, barrier.clone()))
                    .collect();
                View::either(views)
            }
            Target::Location(location) => View::Place(state.resolve(&location)),
            Target::Element {
                container,
                indices,
                projection,
            } => View::Element {
                container,
                indices,
                projection,
                barrier,
            },
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
                    barrier,
                    projection: Vec::new(),
                }
            }
            Target::Run {
                container,
                prefix,
                offset,
                length,
                projection,
            } => View::Run {
                container,
                prefix,
                offset,
                length,
                projection,
                barrier,
            },
            _ => View::Unknown,
        }
    }
}
