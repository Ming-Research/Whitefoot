//! Call-entry substitution and storage-version writes [EFF-5, RANGE-2].
use super::super::world::{CallFootprintLoss, WriteIndex};
use super::*;
use crate::semantic::model::CheckedEffectStep;
use crate::semantic::places::WindowPart;

pub(super) enum Write {
    View(Box<View>),
    Descriptor(ContainerId, CheckedMeasure),
    Elements {
        container: ContainerId,
        indices: Vec<WriteIndex>,
        projection: Vec<CheckedRangeProjection>,
        loss: Option<CallFootprintLoss>,
    },
}

impl Write {
    pub(super) fn view(view: View) -> Self {
        Self::View(Box::new(view))
    }
}

impl Walker<'_> {
    pub(super) fn call_write(
        &mut self,
        state: &mut State,
        callee: &CheckedFunction,
        values: &[Value],
        ordinal: usize,
        steps: &[CheckedEffectStep],
    ) -> Write {
        let parameter = &callee.parameters[ordinal];
        let Some(Value::Ref(mut view)) = values.get(ordinal).cloned() else {
            return Write::view(View::Unknown);
        };
        let mut ty = parameter.ty;
        match &view {
            View::Run {
                container,
                barrier: Some(barrier),
                ..
            }
            | View::Element {
                container,
                barrier: Some(barrier),
                ..
            }
            | View::Unindexed {
                container,
                barrier: Some(barrier),
                ..
            } => {
                let mut precise_values = values.to_vec();
                let mut precise_view = view.clone();
                match &mut precise_view {
                    View::Run { barrier, .. }
                    | View::Element { barrier, .. }
                    | View::Unindexed { barrier, .. } => *barrier = None,
                    _ => unreachable!(),
                }
                precise_values[ordinal] = Value::Ref(precise_view);
                let precise = self.call_write(state, callee, &precise_values, ordinal, steps);
                let loss = match precise {
                    Write::View(view) => match *view {
                        View::Element {
                            indices,
                            projection: Some(projection),
                            ..
                        } => Some(CallFootprintLoss {
                            frames: barrier.frames.clone(),
                            indices: indices.into_iter().map(WriteIndex::Point).collect(),
                            projection,
                        }),
                        _ => None,
                    },
                    Write::Elements {
                        indices,
                        projection,
                        ..
                    } => Some(CallFootprintLoss {
                        frames: barrier.frames.clone(),
                        indices,
                        projection,
                    }),
                    _ => None,
                };
                // An unresolved diagnostic suffix supplies no precise
                // same-frame footprint. Its offsets are not part of the
                // actual containing-range write and cannot mark that write.
                let loss = loss.filter(|loss| {
                    !loss
                        .indices
                        .iter()
                        .any(|index| index.has_offset_in(&self.world.unresolved_call_offsets))
                });
                let mut indices: Vec<_> = barrier
                    .prefix
                    .iter()
                    .cloned()
                    .map(WriteIndex::Point)
                    .collect();
                indices.push(WriteIndex::Range {
                    start: barrier.start.clone(),
                    end: barrier.end.clone(),
                });
                return Write::Elements {
                    container: *container,
                    indices,
                    projection: barrier.projection.clone(),
                    loss,
                };
            }
            View::Unindexed { .. } => return self.call_region(state, view, ty, WriteIndex::Any),
            _ => {}
        }
        let mut range = parameter.mode.is_range();
        let actual = |declaration| {
            let ordinal = callee
                .parameters
                .iter()
                .position(|parameter| parameter.declaration == declaration)?;
            match values.get(ordinal)? {
                Value::Int(value) => Some(value.clone()),
                _ => None,
            }
        };
        for step in steps {
            if let CheckedEffectStep::Measure(measure) = step {
                return match view {
                    View::Place(location) => {
                        let arity = if matches!(ty, CheckedType::Segments { .. }) {
                            2
                        } else {
                            1
                        };
                        match state.container(&mut self.world, location, arity) {
                            Some(container) => Write::Descriptor(container, *measure),
                            None => Write::view(View::Unknown),
                        }
                    }
                    View::Element {
                        container,
                        indices,
                        projection: Some(mut projection),
                        barrier,
                    } => {
                        projection.push(CheckedRangeProjection::Measure(*measure));
                        Write::view(View::Element {
                            container,
                            indices,
                            projection: Some(projection),
                            barrier,
                        })
                    }
                    other => Write::view(other),
                };
            }
            if let CheckedEffectStep::Index(declaration) = step {
                // A missing symbolic capture cannot establish separation.
                // Retain a fresh unknown point rather than dropping its write.
                let index = actual(*declaration).unwrap_or_else(|| self.unresolved_call_offset());
                let selected = self.goal_subscript(state, Value::Ref(view.clone()), ty, index);
                let Some(Value::Ref(selected)) = selected else {
                    return Write::view(view);
                };
                view = selected;
                ty = if range {
                    range = false;
                    ty
                } else if let CheckedType::Segments { element } = ty {
                    CheckedType::Buffer { element }
                } else if let Some(element) = ty.storage_element() {
                    self.elements[element.0 as usize]
                } else {
                    return Write::view(view);
                };
                continue;
            }
            let region = match step {
                CheckedEffectStep::Range { start, end } => Some(WriteIndex::Range {
                    start: actual(*start).unwrap_or_else(|| self.unresolved_call_offset()),
                    end: actual(*end).unwrap_or_else(|| self.unresolved_call_offset()),
                }),
                CheckedEffectStep::Part(WindowPart::Next | WindowPart::Free) => {
                    let target = effect_target(&view);
                    self.target_length(state, &target, ty).map(WriteIndex::From)
                }
                CheckedEffectStep::Page(_)
                | CheckedEffectStep::Part(WindowPart::Last | WindowPart::Filled) => {
                    Some(WriteIndex::Any)
                }
                _ => None,
            };
            if let Some(region) = region {
                // Range/index, page/index and part/index are OWN-7 barriers:
                // an unseparated pair overlaps together with every descendant,
                // so a field or subscript after this selector cannot separate.
                return self.call_region(state, view, ty, region);
            }
            let selected = match (step, ty) {
                (CheckedEffectStep::Field(field), CheckedType::Nominal(nominal)) => {
                    match &self.nominals[nominal.0 as usize].kind {
                        CheckedNominalKind::Struct { fields } => {
                            Some((Step::Field(*field), fields[*field as usize].ty))
                        }
                        _ => None,
                    }
                }
                (CheckedEffectStep::Deref, CheckedType::Nominal(nominal)) => {
                    match self.nominals[nominal.0 as usize].kind {
                        CheckedNominalKind::Box { referent, .. } => {
                            Some((Step::BoxContent, referent))
                        }
                        _ => None,
                    }
                }
                (CheckedEffectStep::Payload { variant, field }, CheckedType::Nominal(nominal)) => {
                    match &self.nominals[nominal.0 as usize].kind {
                        CheckedNominalKind::Enum { variants } => variants
                            .iter()
                            .find(|item| item.tag == *variant)
                            .map(|selected| {
                                (
                                    Step::Payload {
                                        variant: *variant,
                                        field: *field,
                                        variants: variants.len() as u32,
                                    },
                                    selected.fields[*field as usize].ty,
                                )
                            }),
                        _ => None,
                    }
                }
                _ => None,
            };
            let Some((step, selected_type)) = selected else {
                return Write::view(view);
            };
            ty = selected_type;
            view = match view {
                View::Place(location) => View::Place(state.resolve(&location.child(step))),
                View::Element {
                    container,
                    indices,
                    projection: Some(mut projection),
                    barrier,
                } => {
                    projection.extend(super::super::world::owned_projection(&[step]).unwrap());
                    View::Element {
                        container,
                        indices,
                        projection: Some(projection),
                        barrier,
                    }
                }
                other => {
                    return Write::view(other);
                }
            };
        }
        if matches!(view, View::Run { .. } | View::Unindexed { .. }) {
            return self.call_region(state, view, ty, WriteIndex::Any);
        }
        Write::view(view)
    }

    /// A function-formal row can retain an offset declaration from its
    /// template. Keep its unknown value in the real write, and mark only
    /// this term so diagnostics can trace queries through affected versions.
    fn unresolved_call_offset(&mut self) -> Linear {
        let value = self.world.opaque(Some(IntegerType::U64));
        self.world
            .unresolved_call_offsets
            .extend(value.terms.iter().map(|(atom, _)| *atom));
        value
    }

    fn call_region(
        &mut self,
        state: &mut State,
        view: View,
        ty: CheckedType,
        selected: WriteIndex,
    ) -> Write {
        let (container, mut indices, projection, selected) = match view {
            View::Element {
                container,
                indices,
                projection: Some(mut projection),
                ..
            } => {
                projection.push(CheckedRangeProjection::Index(indices.len() as u32));
                (container, indices, projection, selected)
            }
            View::Place(location) => {
                let arity = if matches!(ty, CheckedType::Segments { .. }) {
                    2
                } else {
                    1
                };
                let Some(container) = state.container(&mut self.world, location, arity) else {
                    return Write::view(View::Unknown);
                };
                (container, Vec::new(), Vec::new(), selected)
            }
            View::Run {
                container,
                prefix,
                projection,
                offset,
                length,
                ..
            } => {
                let shifted = match selected {
                    WriteIndex::Any => offset
                        .plus(&length)
                        .map(|end| WriteIndex::Range { start: offset, end }),
                    WriteIndex::Range { start, end } => offset
                        .plus(&start)
                        .zip(offset.plus(&end))
                        .map(|(start, end)| WriteIndex::Range { start, end }),
                    WriteIndex::From(start) => offset.plus(&start).map(WriteIndex::From),
                    WriteIndex::Point(index) => offset.plus(&index).map(WriteIndex::Point),
                };
                let Some(shifted) = shifted else {
                    return Write::view(View::Unknown);
                };
                (container, prefix, projection, shifted)
            }
            View::Unindexed {
                container,
                indices,
                projection,
                ..
            } => (container, indices, projection, WriteIndex::Any),
            other => return Write::view(other),
        };
        let mut indices: Vec<_> = indices.drain(..).map(WriteIndex::Point).collect();
        indices.push(selected);
        Write::Elements {
            container,
            indices,
            projection,
            loss: None,
        }
    }

    pub(super) fn forget_call_write(&mut self, state: &mut State, write: Write) {
        match write {
            Write::Descriptor(container, written) => {
                state.havoc_measure(&mut self.world, container, written);
            }
            Write::Elements {
                container,
                indices,
                projection,
                loss,
            } => {
                state.havoc_elements(&mut self.world, container, indices, projection);
                if let Some(loss) = loss {
                    let version = state.version(&mut self.world, container);
                    self.world.call_footprint_losses.insert(version, loss);
                }
            }
            Write::View(view) => match *view {
                View::Unindexed {
                    container,
                    indices,
                    projection,
                    ..
                } => {
                    state.havoc_elements(
                        &mut self.world,
                        container,
                        indices.into_iter().map(WriteIndex::Point).collect(),
                        projection,
                    );
                }
                View::Run { container, .. } => {
                    state.havoc_container(&mut self.world, container, false);
                }
                View::Element {
                    container,
                    indices,
                    projection,
                    ..
                } => {
                    state.write_element(
                        &mut self.world,
                        container,
                        indices,
                        projection.unwrap_or_default(),
                        BTreeMap::new(),
                    );
                }
                View::Place(location) => state.havoc_location(&mut self.world, &location),
                View::Scalar(_) | View::Unknown => state.havoc_everything(&mut self.world),
            },
        }
    }
}

fn effect_target(view: &View) -> Target {
    match view {
        View::Place(location) => Target::Location(location.clone()),
        View::Element {
            container,
            indices,
            projection,
            ..
        } => Target::Element {
            container: *container,
            indices: indices.clone(),
            projection: projection.clone(),
        },
        View::Run {
            container,
            prefix,
            projection,
            offset,
            length,
            ..
        } => Target::Run {
            container: *container,
            prefix: prefix.clone(),
            projection: projection.clone(),
            offset: offset.clone(),
            length: length.clone(),
        },
        _ => Target::Unknown,
    }
}
