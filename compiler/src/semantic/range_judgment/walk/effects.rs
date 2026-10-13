//! Call-entry substitution and storage-version writes [EFF-5, RANGE-2].
use super::*;
use crate::semantic::model::CheckedEffectStep;
use crate::semantic::places::WindowPart;

pub(super) enum Write {
    View(View),
    Descriptor(ContainerId, CheckedMeasure),
}

impl Walker<'_> {
    pub(super) fn call_write(
        &mut self,
        state: &mut State,
        callee: &CheckedFunction,
        values: &[Value],
        ordinal: usize,
        steps: &[CheckedEffectStep],
        node: &NodePath,
    ) -> Write {
        let parameter = &callee.parameters[ordinal];
        let Some(Value::Ref(mut view)) = values.get(ordinal).cloned() else {
            return Write::View(View::Unknown);
        };
        let mut ty = parameter.ty;
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
        for (step_index, step) in steps.iter().enumerate() {
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
                            None => Write::View(View::Unknown),
                        }
                    }
                    View::Element {
                        container,
                        indices,
                        projection: Some(mut projection),
                    } => {
                        projection.push(CheckedRangeProjection::Measure(*measure));
                        Write::View(View::Element {
                            container,
                            indices,
                            projection: Some(projection),
                        })
                    }
                    other => {
                        self.imprecise_call.get_or_insert_with(|| node.clone());
                        Write::View(other)
                    }
                };
            }
            let index = match step {
                CheckedEffectStep::Index(declaration) => actual(*declaration),
                CheckedEffectStep::Part(WindowPart::Next) => {
                    // Appends expose only previously uninitialized tuples;
                    // old range facts cannot describe those tuples. Shrinking
                    // before appending also declares `last`/`filled`, whose
                    // all-element write below discards the old live contents.
                    let target = effect_target(&view);
                    self.target_length(state, &target, ty)
                }
                _ => None,
            };
            if matches!(
                step,
                CheckedEffectStep::Index(_) | CheckedEffectStep::Part(WindowPart::Next)
            ) {
                let selected = index.and_then(|index| {
                    self.goal_subscript(state, Value::Ref(view.clone()), ty, index)
                });
                let Some(Value::Ref(selected)) = selected else {
                    self.imprecise_call.get_or_insert_with(|| node.clone());
                    return Write::View(view);
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
                    self.imprecise_call.get_or_insert_with(|| node.clone());
                    return Write::View(view);
                };
                continue;
            }
            if matches!(
                step,
                CheckedEffectStep::Range { .. }
                    | CheckedEffectStep::Page(_)
                    | CheckedEffectStep::Part(
                        WindowPart::Last | WindowPart::Filled | WindowPart::Free
                    )
            ) {
                // This version vocabulary has point writes, not interval or
                // page writes. Retain the selected element's descriptor and
                // siblings, but forget its entire element storage. Any proof
                // left open after this widening is a compiler capability gap.
                // WIN-2 makes every element overlap `last` and `filled`.
                // A suffix or a narrower region needs a representation this
                // walk does not have yet, so only those cases are widened.
                if step_index + 1 != steps.len()
                    || !matches!(
                        step,
                        CheckedEffectStep::Part(WindowPart::Last | WindowPart::Filled)
                    )
                {
                    self.imprecise_call.get_or_insert_with(|| node.clone());
                }
                return Write::View(self.effect_elements(state, view, ty));
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
                self.imprecise_call.get_or_insert_with(|| node.clone());
                return Write::View(view);
            };
            ty = selected_type;
            view = match view {
                View::Place(location) => View::Place(state.resolve(&location.child(step))),
                View::Element {
                    container,
                    indices,
                    projection: Some(mut projection),
                } => {
                    projection.extend(super::super::world::owned_projection(&[step]).unwrap());
                    View::Element {
                        container,
                        indices,
                        projection: Some(projection),
                    }
                }
                other => {
                    self.imprecise_call.get_or_insert_with(|| node.clone());
                    return Write::View(other);
                }
            };
        }
        if let View::Run {
            container,
            ref prefix,
            ref offset,
            ref length,
        } = view
        {
            let complete_length = match prefix.as_slice() {
                [] => Some(self.world.measure(
                    container,
                    state.generation(container),
                    CheckedMeasure::Length,
                )),
                [row] => Some(self.world.segment_length(
                    container,
                    state.generation(container),
                    row.clone(),
                )),
                _ => None,
            };
            if *offset != Linear::constant(0) || complete_length.as_ref() != Some(length) {
                self.imprecise_call.get_or_insert_with(|| node.clone());
            }
            return Write::View(self.effect_elements(state, view, ty));
        }
        Write::View(view)
    }

    fn effect_elements(&mut self, state: &mut State, view: View, ty: CheckedType) -> View {
        match view {
            View::Element {
                container,
                indices,
                projection: Some(mut projection),
            } => {
                projection.push(CheckedRangeProjection::Index(indices.len() as u32));
                View::Element {
                    container,
                    indices,
                    projection: Some(projection),
                }
            }
            View::Place(location) => {
                let arity = if matches!(ty, CheckedType::Segments { .. }) {
                    2
                } else {
                    1
                };
                match state.container(&mut self.world, location, arity) {
                    Some(container) => View::Element {
                        container,
                        indices: Vec::new(),
                        projection: Some(Vec::new()),
                    },
                    None => View::Unknown,
                }
            }
            View::Run {
                container, prefix, ..
            } => View::Element {
                container,
                indices: prefix,
                projection: Some(Vec::new()),
            },
            other => other,
        }
    }

    pub(super) fn forget_call_write(&mut self, state: &mut State, write: Write) {
        match write {
            Write::Descriptor(container, written) => {
                state.havoc_measure(&mut self.world, container, written);
            }
            Write::View(View::Run { container, .. }) => {
                state.havoc_container(&mut self.world, container, false);
            }
            Write::View(View::Element {
                container,
                indices,
                projection,
            }) => {
                state.write_element(
                    &mut self.world,
                    container,
                    indices,
                    projection.unwrap_or_default(),
                    BTreeMap::new(),
                );
            }
            Write::View(View::Place(location)) => state.havoc_location(&mut self.world, &location),
            Write::View(View::Scalar(_) | View::Unknown) => state.havoc_everything(&mut self.world),
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
        } => Target::Element {
            container: *container,
            indices: indices.clone(),
            projection: projection.clone(),
        },
        View::Run {
            container,
            prefix,
            offset,
            length,
        } => Target::Run {
            container: *container,
            prefix: prefix.clone(),
            offset: offset.clone(),
            length: length.clone(),
        },
        _ => Target::Unknown,
    }
}
