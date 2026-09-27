//! Canonical rendering [ENT-6]: residuals, relations, terms, goals
//! and places spelled in the source's own terms.

use super::*;

impl Input<'_, '_> {
    pub(super) fn render_integer_domain_goal(
        &self,
        operation: CheckedIntegerOperation,
        arguments: &[CheckedExpression],
    ) -> String {
        let rendered = arguments
            .iter()
            .map(|argument| self.render_expression(argument))
            .collect::<Vec<_>>();
        match (operation, rendered.as_slice()) {
            (CheckedIntegerOperation::AddExact, [left, right]) => {
                format!("{left} +defined {right}")
            }
            (CheckedIntegerOperation::SubtractExact, [left, right]) => {
                format!("{left} -defined {right}")
            }
            (CheckedIntegerOperation::MultiplyExact, [left, right]) => {
                format!("{left} *defined {right}")
            }
            (CheckedIntegerOperation::DivideExact, [left, right]) => {
                format!("{left} /defined {right}")
            }
            (CheckedIntegerOperation::RemainderExact, [left, right]) => {
                format!("{left} %defined {right}")
            }
            (CheckedIntegerOperation::AbsoluteExact, [value]) => {
                format!("iabs.defined({value})")
            }
            (CheckedIntegerOperation::NegateExact, [value]) => {
                format!("ineg.defined({value})")
            }
            (CheckedIntegerOperation::ShiftLeftExact, [value, amount]) => {
                format!("ishl.defined({value}, {amount})")
            }
            (CheckedIntegerOperation::ShiftRightExact, [value, amount]) => {
                format!("ishr.defined({value}, {amount})")
            }
            _ => "<invalid integer-domain goal>".to_owned(),
        }
    }

    /// Renders one INV-1 incoming-edge target using only source spellings.
    ///
    /// The checked relation contains immutable binding identities, which are
    /// appropriate for proof but useless in a source diagnostic. For a
    /// counted backedge the only compiler-written value transition is the
    /// hidden unit update, so occurrences of that binder are rendered as the
    /// exact source expression the writer must preserve. Ordinary loops pass
    /// no binder and therefore render the header relation unchanged.
    pub(super) fn render_checked_invariant_relation(
        &self,
        relation: &CheckedAffineRelation,
        counted_next_binder: Option<BindingId>,
    ) -> String {
        let left = self.render_checked_affine_expression(&relation.left, counted_next_binder);
        let right = self.render_checked_affine_expression(&relation.right, counted_next_binder);
        match relation.bound {
            0 => format!("{left} <= {right}"),
            -1 => format!("{left} < {right}"),
            // INV-1 formation currently admits only strict and non-strict
            // ordered roots. Keep a source-level fallback so an internal
            // inconsistency never leaks an affine term identity.
            bound => format!("({left} - {right}) <= {bound}_i128"),
        }
    }

    pub(super) fn render_checked_affine_expression(
        &self,
        expression: &CheckedAffineExpression,
        counted_next_binder: Option<BindingId>,
    ) -> String {
        let mut values = Vec::new();
        for expression in expression.postorder() {
            let rendered = match &expression.kind {
                CheckedAffineExpressionKind::Constant { value, ty } => {
                    format!("{value}_{}", integer_type_name(*ty))
                }
                CheckedAffineExpressionKind::Local { binding, .. } => {
                    let name = self.binding_name(*binding);
                    if counted_next_binder == Some(*binding) {
                        format!("({name} + 1_u64)")
                    } else {
                        name
                    }
                }
                // [INV-1] a measure factor renders as the writer wrote it: the
                // former over the place, never an internal term identity.
                CheckedAffineExpressionKind::Measure(measure) => self
                    .render_affine_measure(measure)
                    .unwrap_or_else(|| "?".to_owned()),
                CheckedAffineExpressionKind::ConstGeneric { name, .. } => name.clone(),
                CheckedAffineExpressionKind::Add(_, _)
                | CheckedAffineExpressionKind::Subtract(_, _) => {
                    let right = values.pop().expect("postorder retains the right child");
                    let left = values.pop().expect("postorder retains the left child");
                    let operator =
                        if matches!(expression.kind, CheckedAffineExpressionKind::Add(_, _)) {
                            '+'
                        } else {
                            '-'
                        };
                    format!("({left} {operator} {right})")
                }
                CheckedAffineExpressionKind::MultiplyByConstant {
                    constant,
                    constant_ty,
                    ..
                } => {
                    let value = values.pop().expect("postorder retains the scaled child");
                    format!("({constant}_{} * {value})", integer_type_name(*constant_ty))
                }
            };
            values.push(rendered);
        }
        values.pop().expect("postorder visits the root")
    }

    /// The writer's own spelling of one [INV-1] affine measure factor.
    pub(super) fn render_affine_measure(&self, expression: &CheckedExpression) -> Option<String> {
        let (measure, binding, fields) = match expression {
            CheckedExpression::ArrayMeasure {
                measure,
                root: CheckedArrayRoot::Binding { binding, fields },
                ..
            } => (*measure, *binding, fields.clone()),
            CheckedExpression::BufferMeasure { measure, root } => {
                let place =
                    self.render_place(&ResolvedPlace::from_path(root.binding, root.place_path()));
                return Some(format!("{place}.{}", measure.spelling()));
            }
            CheckedExpression::RangeMeasure { measure, root } => {
                (*measure, root.binding, Vec::new())
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let mut resolved = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    is_holder(place.root.binding),
                    Vec::new(),
                );
                resolved.path.extend(place.place_path());
                return Some(format!(
                    "{}.{}",
                    self.render_place(&resolved),
                    measure.spelling()
                ));
            }
            // [MSR-1] a measured place may carry a subscript, so this one is
            // rendered from the same source-order path every other consumer
            // reads rather than from a field list.
            CheckedExpression::ContainerMeasure { measure, root } => {
                let mut path = container_root_path(root);
                path.path
                    .retain(|projection| !matches!(projection, PlaceStep::Deref));
                let place = self.render_place(&ResolvedPlace {
                    root: root.root,
                    path: path.path,
                });
                return Some(format!("{place}.{}", measure.spelling()));
            }
            _ => return None,
        };
        let place = self.render_place(&ResolvedPlace::spelled(
            PlaceRoot::Binding(binding),
            false,
            fields,
        ));
        Some(format!("{place}.{}", measure.spelling()))
    }

    pub(super) fn binding_name(&self, binding: BindingId) -> String {
        self.context
            .binding_names
            .get(binding.0 as usize)
            .cloned()
            .unwrap_or_else(|| "?".to_owned())
    }

    /// The source spelling of one declaration, such as a generic parameter.
    pub(super) fn declaration_name(&self, declaration: crate::DeclarationId) -> String {
        self.context
            .declarations
            .get(declaration.index())
            .map_or_else(|| "?".to_owned(), |record| record.spelling().to_owned())
    }

    /// One [OP-4] subscript offset, in the spelling the source wrote it in.
    pub(super) fn render_offset(&self, offset: CapturedValue) -> String {
        match offset.term {
            CapturedTerm::Literal(value) => value.to_string(),
            CapturedTerm::Binding(binding) => self.binding_name(binding),
            CapturedTerm::Const(declaration) => self.declaration_name(declaration),
            // [REF-1] the binding's spelling names a later value than the one
            // this index read, and no source spelling names that one.
            CapturedTerm::Superseded(_) | CapturedTerm::Opaque => "?".to_owned(),
        }
    }

    pub(super) fn render_place(&self, place: &ResolvedPlace) -> String {
        let reference_root = matches!(place.root, PlaceRoot::Binding(binding)
            if self.places.is_reference(binding));
        let (mut rendered, mut ty) = match place.root {
            PlaceRoot::Binding(binding) => (
                {
                    // [REF-1, OP-15] a reference variable names a path and is
                    // not storage of its own, so the storage it names is
                    // reached only through `deref`. A term over a reference
                    // anchors at the binding and carries no step of its own,
                    // so the spelling the writer reads puts the step back.
                    let name = self.binding_name(binding);
                    if reference_root {
                        format!("deref({name})")
                    } else {
                        name
                    }
                },
                self.summary(binding).and_then(|summary| summary.ty),
            ),
            PlaceRoot::Constant(id) => (
                self.context
                    .constants
                    .get(id.0 as usize)
                    .map(|constant| constant.name.clone())
                    .unwrap_or_else(|| "?".to_owned()),
                self.context
                    .constants
                    .get(id.0 as usize)
                    .map(|constant| constant.ty),
            ),
        };
        for projection in &place.path {
            match projection {
                PlaceStep::Descendant(target) => {
                    rendered.push_str(".**");
                    ty = Some(target.ty);
                }
                PlaceStep::Payload { variant, field } => {
                    rendered.push_str(&format!(".{variant}.{field}"));
                    ty = None;
                }
                PlaceStep::Range(range) => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.render_offset(range.start),
                        self.render_offset(range.end)
                    ));
                }
                PlaceStep::Part(part) => {
                    rendered.push('.');
                    rendered.push_str(part.spelling());
                    ty = None;
                }
                PlaceStep::Measure(measure) => {
                    rendered.push('.');
                    rendered.push_str(measure.spelling());
                    ty = Some(CheckedType::Integer(IntegerType::U64));
                }
                PlaceStep::Field(field) => {
                    let name = ty
                        .and_then(|current| self.field_name(current, *field))
                        .unwrap_or(None);
                    match name {
                        Some((field_name, field_ty)) => {
                            rendered.push('.');
                            rendered.push_str(&field_name);
                            ty = Some(field_ty);
                        }
                        None => {
                            rendered.push_str(".?");
                            ty = None;
                        }
                    }
                }
                PlaceStep::Index(offset) => {
                    rendered.push_str(&format!("[{}]", self.render_offset(*offset)));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                PlaceStep::Deref => {
                    self.render_content_step(&mut rendered, &mut ty);
                }
            }
        }
        rendered
    }

    /// References cannot be stored in aggregates. After the root reference
    /// step is consumed, a typed Box dereference spells its `inner` member.
    pub(super) fn render_content_step(&self, rendered: &mut String, ty: &mut Option<CheckedType>) {
        let boxed = ty.is_some_and(|ty| {
            matches!(ty, CheckedType::Nominal(id)
                if self.context.nominals.get(id.0 as usize)
                    .is_some_and(|nominal| matches!(nominal.kind, CheckedNominalKind::Box { .. })))
        });
        if boxed {
            rendered.push_str(".inner");
        } else {
            *rendered = format!("deref({rendered})");
        }
        *ty = ty.and_then(|current| self.deref_type(current));
    }

    pub(super) fn deref_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(id) = ty else {
            // Borrow bindings retain the referent type in checked form.
            return Some(ty);
        };
        let nominal = self.context.nominals.get(id.0 as usize)?;
        match nominal.kind {
            CheckedNominalKind::Box { referent, .. } => Some(referent),
            _ => Some(ty),
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn field_name(
        &self,
        ty: CheckedType,
        field: u32,
    ) -> Option<Option<(String, CheckedType)>> {
        let CheckedType::Nominal(id) = ty else {
            return Some(None);
        };
        let nominal = self.context.nominals.get(id.0 as usize)?;
        let CheckedNominalKind::Struct { fields } = &nominal.kind else {
            return Some(None);
        };
        let field = fields.get(field as usize)?;
        Some(Some((field.name.clone(), field.ty)))
    }

    /// One concrete [FN-8] call goal in the terms the source wrote it in.
    ///
    /// The structural dump this replaced published `Integer { operation:
    /// LessEqual, .. }(Place { root: BindingId(6), .. })`: a writer cannot find
    /// either half in their own program, and the blind-writer trial of
    /// 2026-08-28 recorded four rounds of readers failing to. [OP-4]
    /// already publishes its residual as source terms from the
    /// renderers below, so FN-8 publishes its goal from the same ones. The
    /// operation spellings come from the compiler's own exhaustive maps, which
    /// `semantic::tests::operation_table` locks against the specification.
    pub(super) fn render_concrete_goal(&self, expression: &GoalExpression) -> String {
        match expression {
            GoalExpression::Datum(datum) => self.render_goal_datum(datum),
            GoalExpression::Operation { row, arguments, .. } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.render_concrete_goal(argument))
                    .collect::<Vec<_>>();
                render_goal_row(row, &arguments, self.context.declarations)
            }
        }
    }

    pub(super) fn render_goal_datum(&self, datum: &GoalDatum) -> String {
        match datum {
            // A concrete goal has no formal left in it, but a template
            // rendered through this path names the position the formal holds.
            GoalDatum::Parameter {
                ordinal,
                projections,
                ..
            } => self.render_goal_projections(format!("parameter #{ordinal}"), None, projections),
            GoalDatum::NamedConst {
                declaration,
                projections,
                ..
            } => {
                let (name, ty) = self.context.constant(*declaration).map_or_else(
                    || ("?".to_owned(), None),
                    |constant| (constant.name.clone(), Some(constant.ty)),
                );
                self.render_goal_projections(name, ty, projections)
            }
            GoalDatum::Place {
                root, projections, ..
            } => {
                let base = self.binding_name(*root);
                let ty = self.summary(*root).and_then(|summary| summary.ty);
                // [REF-1, OP-15] a reference variable names a path and is not
                // storage of its own, so every place that goes through one is
                // written under a `deref` step: `deref(p)`, `deref(p).field`,
                // `deref(part).len`. A term rooted at a reference anchors at
                // that binding and carries no step of its own — the parameter
                // name *is* the path inside the body — so rendering puts that
                // source wrapper back while retaining every concrete step
                // below the referent.
                let reference = self.places.is_reference(*root);
                let base = if reference {
                    format!("deref({base})")
                } else {
                    base
                };
                self.render_goal_projections(base, ty, projections)
            }
            // Source cannot name this datum: render its structural source
            // role rather than inventing an expression that could reread a
            // different runtime value.
            GoalDatum::EvaluatedValue {
                occurrence,
                captured_type,
                projections,
                ..
            } => {
                let base = match occurrence {
                    // [DIAG-1] fixes this spelling for an FN-8 payload.
                    EvaluatedValueOccurrence::CallArgument { argument, .. } => {
                        format!("argument #{argument} pre-transfer value")
                    }
                    EvaluatedValueOccurrence::ObligationOperand { operand, .. } => {
                        format!("<operand #{operand} evaluated value>")
                    }
                };
                self.render_goal_projections(base, Some(*captured_type), projections)
            }
            // A literal renders as the source spelling that denotes it
            // [FORM-5]: `unit`, a `Bool` variant constructor, a suffixed
            // integer, or a float's canonical literal.
            GoalDatum::Literal(value) => match value {
                CheckedValue::Integer { ty, bits } => {
                    format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty))
                }
                CheckedValue::Float { ty, bits } => {
                    super::super::super::check::floats::float_value_spelling(*ty, *bits)
                }
                CheckedValue::Unit => "unit".to_owned(),
                CheckedValue::Bool(true) => "True()".to_owned(),
                CheckedValue::Bool(false) => "False()".to_owned(),
                // [CONST-1] a const generic is named by its parameter, and a
                // generic-numeric identity is `0_T` or `1_T` [FORM-5].
                CheckedValue::ConstGeneric { declaration, .. } => {
                    self.declaration_name(*declaration)
                }
                CheckedValue::NumericIdentity {
                    ty:
                        CheckedType::GenericInt(declaration) | CheckedType::GenericFloat(declaration),
                    one,
                } => format!("{}_{}", u8::from(*one), self.declaration_name(*declaration)),
                other => format!("{other:?}"),
            },
        }
    }

    pub(super) fn render_goal_projections(
        &self,
        base: String,
        root_type: Option<CheckedType>,
        projections: &[GoalProjection],
    ) -> String {
        let mut rendered = base;
        let mut ty = root_type;
        for projection in projections {
            match projection {
                GoalProjection::Deref => {
                    self.render_content_step(&mut rendered, &mut ty);
                }
                GoalProjection::Field(field) => {
                    match ty
                        .and_then(|current| self.field_name(current, *field))
                        .unwrap_or(None)
                    {
                        Some((name, field_type)) => {
                            rendered.push('.');
                            rendered.push_str(&name);
                            ty = Some(field_type);
                        }
                        None => {
                            rendered.push_str(".?");
                            ty = None;
                        }
                    }
                }
                GoalProjection::Payload { variant, field } => {
                    let selected = ty.and_then(|current| {
                        let CheckedType::Nominal(nominal) = current else {
                            return None;
                        };
                        let CheckedNominalKind::Enum { variants } =
                            &self.context.nominals.get(nominal.0 as usize)?.kind
                        else {
                            return None;
                        };
                        let variant = variants.iter().find(|item| item.tag == *variant)?;
                        let field = variant.fields.get(*field as usize)?;
                        Some((&variant.name, &field.name, field.ty))
                    });
                    if let Some((variant, field, field_type)) = selected {
                        rendered.push_str(&format!(".{variant}.{field}"));
                        ty = Some(field_type);
                    } else {
                        rendered.push_str(&format!(".{variant}.{field}"));
                        ty = None;
                    }
                }
                GoalProjection::Subscript(offset) => {
                    rendered.push_str(&format!("[{}]", self.render_offset(*offset)));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                // [REF-4] the range the actual formed, spelled exactly as it
                // was written: the range names no binding, so its two
                // endpoints are what identifies it to the writer.
                GoalProjection::Range(range) => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.render_offset(range.start),
                        self.render_offset(range.end)
                    ));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
                GoalProjection::FormalSubscript { ordinal } => {
                    rendered.push_str(&format!("[parameter #{ordinal}]"));
                    ty = ty.and_then(|ty| element_type(ty, self.context.elements));
                }
            }
        }
        rendered
    }

    /// Render the checked storage path without reducing subscript offsets
    /// to overlap identities: even a non-term offset retains its source
    /// expression in an obligation's residual.
    pub(super) fn render_storage_place(&self, root: &CheckedContainerRoot) -> String {
        let mut rendered = self.render_place(&ResolvedPlace {
            root: root.root,
            path: Vec::new(),
        });
        let mut ty = match root.root {
            PlaceRoot::Binding(binding) => self.summary(binding).and_then(|summary| summary.ty),
            PlaceRoot::Constant(id) => self
                .context
                .constants
                .get(id.0 as usize)
                .map(|value| value.ty),
        };
        for step in &root.path {
            match step {
                CheckedPlaceStep::Field(field) => {
                    if let Some((name, selected)) =
                        ty.and_then(|ty| self.field_name(ty, *field)).flatten()
                    {
                        rendered.push('.');
                        rendered.push_str(&name);
                        ty = Some(selected);
                    } else {
                        rendered.push_str(".?");
                        ty = None;
                    }
                }
                CheckedPlaceStep::BoxReferent(nominal) => {
                    rendered.push_str(".inner");
                    ty = self.deref_type(CheckedType::Nominal(*nominal));
                }
                CheckedPlaceStep::Subscript(index) => {
                    rendered.push_str(&format!("[{}]", self.render_expression(&index.offset)));
                    ty = Some(index.element_type);
                }
            }
        }
        rendered
    }

    pub(super) fn render_expression(&self, expression: &CheckedExpression) -> String {
        match expression {
            CheckedExpression::Constant(CheckedValue::ConstGeneric { .. }) => {
                "<const parameter>".to_owned()
            }
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits }) => {
                format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty))
            }
            CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty)),
            CheckedExpression::Binding { binding, .. } => self.binding_name(*binding),
            // [OP-15, MSR-1] a measure is read as a member of the measured
            // place, so a residual naming one spells it `p.len` and never as
            // a call of a reader row [FORM-1].
            CheckedExpression::BufferMeasure { measure, root } => format!(
                "{}.{}",
                self.render_place(&ResolvedPlace::from_path(root.binding, root.place_path())),
                measure.spelling(),
            ),
            CheckedExpression::RangeMeasure { measure, root } => format!(
                "{}.{}",
                self.render_place(&ResolvedPlace::spelled(
                    PlaceRoot::Binding(root.binding),
                    is_holder(root.binding),
                    Vec::new()
                )),
                measure.spelling(),
            ),
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let mut resolved = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    is_holder(place.root.binding),
                    Vec::new(),
                );
                resolved.path.extend(place.place_path());
                format!("{}.{}", self.render_place(&resolved), measure.spelling())
            }
            CheckedExpression::ArrayMeasure { measure, root, .. } => format!(
                "{}.{}",
                self.render_place(&array_root_place(root)),
                measure.spelling(),
            ),
            CheckedExpression::ContainerMeasure { measure, root } => {
                format!("{}.{}", self.render_storage_place(root), measure.spelling(),)
            }
            CheckedExpression::Project {
                binding, fields, ..
            } => self.render_place(&ResolvedPlace::spelled(
                PlaceRoot::Binding(*binding),
                false,
                fields.clone(),
            )),
            CheckedExpression::DerefAddressed { binding, .. } => {
                format!("deref({})", self.binding_name(*binding))
            }
            CheckedExpression::BoxDeref { value, .. } => {
                format!("{}.inner", self.render_expression(value))
            }
            CheckedExpression::ProjectValue {
                value,
                nominal,
                field,
                ..
            } => {
                let field_name = self
                    .context
                    .nominals
                    .get(nominal.0 as usize)
                    .and_then(|nominal| match &nominal.kind {
                        CheckedNominalKind::Struct { fields } => {
                            fields.get(*field as usize).map(|field| field.name.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| "?".to_owned());
                format!("{}.{field_name}", self.render_expression(value))
            }
            CheckedExpression::ArrayIndex { root, offset, .. } => {
                let base = array_root_place(root);
                format!(
                    "{}[{}]",
                    self.render_place(&base),
                    self.render_expression(offset)
                )
            }
            CheckedExpression::ReadStorage { root, .. } => self.render_storage_place(root),
            CheckedExpression::BufferIndex { root, offset, .. } => {
                let base = ResolvedPlace::from_path(root.binding, root.place_path());
                format!(
                    "{}[{}]",
                    self.render_place(&base),
                    self.render_expression(offset)
                )
            }
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                let mut base = ResolvedPlace::spelled(
                    PlaceRoot::Binding(place.root.binding),
                    is_holder(place.root.binding),
                    Vec::new(),
                );
                base.path.extend(place.place_path());
                self.render_place(&base)
            }
            _ => "?".to_owned(),
        }
    }
}

impl Reasoning<'_, '_, '_> {
    /// Renders one normalized relation for diagnostics.
    pub(super) fn render_relation(&self, relation: &Relation) -> String {
        match relation {
            Relation::Bound { left, right, bound } => format!(
                "{} - {} <= {bound}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            // An undisplaced relation reads as the writer wrote it; a
            // displaced one names its difference, exactly as a bound does.
            Relation::Equal {
                left,
                right,
                difference: 0,
            } => format!("{} = {}", self.render_term(*left), self.render_term(*right)),
            Relation::Equal {
                left,
                right,
                difference,
            } => format!(
                "{} - {} = {difference}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            Relation::Distinct {
                left,
                right,
                difference: 0,
            } => format!(
                "{} != {}",
                self.render_term(*left),
                self.render_term(*right)
            ),
            Relation::Distinct {
                left,
                right,
                difference,
            } => format!(
                "{} - {} != {difference}",
                self.render_term(*left),
                self.render_term(*right)
            ),
        }
    }

    pub(super) fn render_term(&self, term: TermId) -> String {
        match self.vocabulary.terms.kind(term) {
            TermKind::Zero => "0".to_owned(),
            TermKind::Constant(value) => value.to_string(),
            TermKind::ConstParameter(..) => "<const parameter>".to_owned(),
            TermKind::Place(place, _) => self.input.render_place(place),
            TermKind::Measure(measure, place) => {
                format!("{}.{}", self.input.render_place(place), measure.spelling())
            }
            TermKind::CountedCapture { side, .. } => match side {
                CountedCaptureSide::Lower => "<counted lower capture>".to_owned(),
                CountedCaptureSide::Upper => "<counted upper capture>".to_owned(),
            },
            TermKind::IndexCapture { .. } => "<captured index>".to_owned(),
            TermKind::ResultPayload(_) => "<success payload>".to_owned(),
            TermKind::CommitValue { .. } => "<assigned value>".to_owned(),
            TermKind::CallDatum { measure, .. } => measure.map_or_else(
                || "<argument value at the call>".to_owned(),
                |measure| format!("<argument {} at the call>", measure.spelling()),
            ),
            // [MSR-3] an entry datum is what the writer wrote: a measure of
            // the parameter, at the one state an `ensures` gives it.
            TermKind::EntryDatum {
                formal,
                projections,
                measure,
            } => {
                let mut place = self
                    .input
                    .function
                    .parameters
                    .get(*formal as usize)
                    .map_or_else(
                        || "?".to_owned(),
                        |parameter| {
                            if matches!(parameter.mode, CheckedMode::Reference) {
                                format!("entry({})", parameter.name)
                            } else {
                                parameter.name.clone()
                            }
                        },
                    );
                for projection in projections {
                    match projection {
                        PlaceStep::Descendant(_) => place.push_str(".**"),
                        PlaceStep::Deref => place = format!("deref({place})"),
                        PlaceStep::Field(field) => {
                            place = format!("{place}.{field}");
                        }
                        PlaceStep::Payload { variant, field } => {
                            place = format!("{place}.{variant}.{field}");
                        }
                        PlaceStep::Index(offset) => {
                            place = format!("{place}[{}]", self.input.render_offset(*offset));
                        }
                        PlaceStep::Range(range) => {
                            place = format!(
                                "{place}[{}..{}]",
                                self.input.render_offset(range.start),
                                self.input.render_offset(range.end)
                            );
                        }
                        PlaceStep::Part(part) => {
                            place = format!("{place}.{}", part.spelling());
                        }
                        PlaceStep::Measure(measure) => {
                            place = format!("{place}.{}", measure.spelling());
                        }
                    }
                }
                format!("{place}.{}", measure.spelling())
            }
            // A measure datum has no source spelling of its own: it is the
            // measure the carried value had at the event that renamed it.
            TermKind::MeasureDatum {
                measure, placement, ..
            } => {
                let event = match placement {
                    MeasurePlacement::Rebind => "the rebind",
                    MeasurePlacement::Construct => "the construct",
                    MeasurePlacement::Destructuring => "the destructuring",
                    MeasurePlacement::Element => "the element position",
                    MeasurePlacement::Payload => "the payload",
                };
                format!("<{} at {event}>", measure.spelling())
            }
        }
    }
}

/// One goal operation applied to already-rendered operands, in the spelling
/// the source uses for that row.
///
/// An operation whose [OP-1] spelling is a call name renders as a call; the
/// arithmetic rows, whose only spelling is the infix operator [GRAM-6], render
/// as the infix expression a writer would have to write.
pub(super) fn render_goal_row(
    row: &GoalOperation,
    arguments: &[String],
    declarations: &[crate::DeclarationRecord],
) -> String {
    match row {
        GoalOperation::Integer { operation, .. } => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::Float { operation, .. } => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::Boolean(operation) => {
            render_operation_spelling(operation.spelling(), arguments)
        }
        GoalOperation::EnumEquality { equal, .. } => {
            render_operation_spelling(if *equal { "eeq" } else { "ene" }, arguments)
        }
        GoalOperation::NumericConversion {
            mode,
            source,
            destination,
        } => format!(
            "{}::<{}, {}>({})",
            match mode {
                CheckedConversionMode::Exact => "cvt",
                CheckedConversionMode::Checked => "cvt.checked",
                CheckedConversionMode::Defined => "cvt.defined",
            },
            numeric_type_name(*source, declarations),
            numeric_type_name(*destination, declarations),
            arguments.join(", ")
        ),
        GoalOperation::Reinterpret {
            source,
            destination,
        } => format!(
            "reinterpret::<{}, {}>({})",
            numeric_type_name(*source, declarations),
            numeric_type_name(*destination, declarations),
            arguments.join(", ")
        ),
        // [OP-15, MSR-1]: one quantity, one name, term and reader alike, and
        // the name is the member spelling `p.len` of the measured place. The
        // v0.59 `len_of(p)` former is not a v0.60 spelling.
        GoalOperation::ArrayMeasure { measure, .. }
        | GoalOperation::BufferMeasure { measure, .. }
        | GoalOperation::ContainerMeasure { measure, .. } => match arguments {
            [place] => format!("{place}.{}", measure.spelling()),
            _ => "<invalid measure goal>".to_owned(),
        },
        GoalOperation::ArrayIndex { .. }
        | GoalOperation::BufferIndex { .. }
        | GoalOperation::RunIndex { .. } => match arguments {
            [collection, offset] => format!("{collection}[{offset}]"),
            _ => "<invalid index goal>".to_owned(),
        },
        GoalOperation::BufferFits { .. } => render_operation_spelling("buffer_fits", arguments),
    }
}

/// A call spelling renders `name(a, b)`; an operator spelling renders the
/// binary infix form, which is the only form [GRAM-6] admits for those rows.
pub(super) fn render_operation_spelling(spelling: &str, arguments: &[String]) -> String {
    let infix = !spelling.starts_with(|first: char| first.is_ascii_alphabetic());
    match (infix, arguments) {
        (true, [left, right]) => format!("{left} {spelling} {right}"),
        _ => format!("{spelling}({})", arguments.join(", ")),
    }
}

pub(super) fn numeric_type_name(
    ty: CheckedNumericType,
    declarations: &[crate::DeclarationRecord],
) -> String {
    match ty {
        CheckedNumericType::Integer(integer) => integer_type_name(integer).to_owned(),
        CheckedNumericType::Float(FloatType::F32) => "f32".to_owned(),
        CheckedNumericType::Float(FloatType::F64) => "f64".to_owned(),
        CheckedNumericType::GenericInteger(declaration)
        | CheckedNumericType::GenericFloat(declaration) => {
            declarations.get(declaration.index()).map_or_else(
                || format!("<type-parameter:{}>", declaration.index()),
                |record| record.spelling().to_owned(),
            )
        }
    }
}

pub(super) const fn integer_type_name(ty: IntegerType) -> &'static str {
    match ty {
        IntegerType::I8 => "i8",
        IntegerType::I16 => "i16",
        IntegerType::I32 => "i32",
        IntegerType::I64 => "i64",
        IntegerType::U8 => "u8",
        IntegerType::U16 => "u16",
        IntegerType::U32 => "u32",
        IntegerType::U64 => "u64",
    }
}
