//! Canonical rendering [ENT-6]: residuals, relations, terms, goals
//! and places spelled in the source's own terms.

use super::*;

/// How a source rendering spells one counted loop binder.
#[derive(Clone, Copy, Debug)]
pub(super) enum BinderSpelling<'a> {
    /// The hidden next value a counted backedge substitutes, `(i + 1_u64)`.
    Next(BindingId),
    /// A range clause's bound variable standing for the binder [RANGE-1].
    Bound(BindingId, &'a str),
}

struct ProofPlaceNames<'a, 'check, 'unit> {
    input: &'a Input<'check, 'unit>,
    counted: Option<BinderSpelling<'a>>,
}

impl crate::semantic::place_render::PlaceNames for ProofPlaceNames<'_, '_, '_> {
    type Error = std::convert::Infallible;

    fn field(
        &self,
        ty: Option<CheckedType>,
        field: u32,
    ) -> Result<Option<(String, CheckedType)>, Self::Error> {
        Ok(ty.and_then(|ty| self.input.field_name(ty, field)).flatten())
    }

    fn payload(
        &self,
        ty: Option<CheckedType>,
        variant: u32,
        field: u32,
    ) -> Result<Option<(String, String, CheckedType)>, Self::Error> {
        Ok(ty.and_then(|ty| {
            let CheckedType::Nominal(id) = ty else {
                return None;
            };
            let CheckedNominalKind::Enum { variants } =
                &self.input.context.nominals.get(id.0 as usize)?.kind
            else {
                return None;
            };
            let variant = variants.get(variant as usize)?;
            let field = variant.fields.get(field as usize)?;
            Some((variant.name.clone(), field.name.clone(), field.ty))
        }))
    }

    fn deref(
        &self,
        ty: Option<CheckedType>,
    ) -> Result<(bool, Option<CheckedType>), Self::Error> {
        let boxed = ty.is_some_and(|ty| {
            matches!(ty, CheckedType::Nominal(id)
                if self.input.context.nominals.get(id.0 as usize)
                    .is_some_and(|nominal| matches!(nominal.kind, CheckedNominalKind::Box { .. })))
        });
        Ok((boxed, ty.and_then(|ty| self.input.deref_type(ty))))
    }

    fn element(
        &self,
        ty: Option<CheckedType>,
        range: bool,
    ) -> Result<Option<CheckedType>, Self::Error> {
        Ok(if range {
            ty
        } else {
            ty.map(|ty| self.input.storage_element_type(ty).unwrap_or(ty))
        })
    }

    fn table_entry(
        &self,
        ty: Option<CheckedType>,
    ) -> Result<Option<CheckedType>, Self::Error> {
        Ok(ty.and_then(|ty| self.input.table_entry_type(ty)))
    }

    fn offset(&self, value: CapturedValue) -> Result<String, Self::Error> {
        Ok(self.input.render_header_offset(value, self.counted))
    }
}

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
    /// no binder and therefore render the header relation unchanged; a
    /// bound variable restates the relation at one element [RANGE-1].
    pub(super) fn render_checked_invariant_relation(
        &self,
        relation: &CheckedAffineRelation,
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        let left = self.render_checked_affine_expression(&relation.left, counted);
        let right = self.render_checked_affine_expression(&relation.right, counted);
        if relation.equality {
            return format!("{left} == {right}");
        }
        match relation.bound {
            0 => format!("{left} <= {right}"),
            -1 => format!("{left} < {right}"),
            // INV-1 formation currently admits only strict and non-strict
            // ordered roots. Keep a source-level fallback so an internal
            // inconsistency never leaks an affine term identity.
            bound => format!(
                "({left} - {right}) <= {}",
                normalized_integer(bound, None)
            ),
        }
    }

    pub(super) fn render_checked_affine_expression(
        &self,
        expression: &CheckedAffineExpression,
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        let mut values = Vec::new();
        for expression in expression.postorder() {
            let rendered = match &expression.kind {
                CheckedAffineExpressionKind::Constant { value, ty } => {
                    format!("{value}_{}", integer_type_name(*ty))
                }
                CheckedAffineExpressionKind::Local { binding, .. } => {
                    self.render_header_binding(*binding, counted)
                }
                // [INV-1] a measure factor renders as the writer wrote it: the
                // former over the place, never an internal term identity.
                CheckedAffineExpressionKind::Measure(measure) => self
                    .render_affine_measure(measure, counted)
                    .unwrap_or_else(|| "?".to_owned()),
                // [INV-1, ENT-6] a tracked field place factor renders as the
                // writer wrote it, through a reference root's `^`.
                CheckedAffineExpressionKind::Field(field) => match self.read_place_path(field) {
                    Some(place) => self.render_header_place(&place, counted),
                    None => "?".to_owned(),
                },
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
    fn render_affine_measure(
        &self,
        expression: &CheckedExpression,
        counted: Option<BinderSpelling<'_>>,
    ) -> Option<String> {
        let (measure, binding, fields) = match expression {
            CheckedExpression::ArrayMeasure {
                measure,
                root: CheckedArrayRoot::Binding { binding, fields },
                ..
            } => (*measure, *binding, fields.clone()),
            CheckedExpression::BufferMeasure { measure, root } => {
                let place = self.render_header_place(
                    &ResolvedPlace::from_path(root.binding, root.place_path()),
                    counted,
                );
                return Some(format!("{place}.{}", measure.spelling()));
            }
            CheckedExpression::RangeMeasure { measure, root } => {
                return Some(format!(
                    "{}.{}",
                    self.render_header_place(&root.proof_place(), counted),
                    measure.spelling()
                ));
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let resolved = place.proof_place();
                return Some(format!(
                    "{}.{}",
                    self.render_header_place(&resolved, counted),
                    measure.spelling()
                ));
            }
            // [MSR-1] a measured place may carry a subscript, so this one is
            // rendered from the same source-order path every other consumer
            // reads rather than from a field list.
            CheckedExpression::ContainerMeasure { measure, root } => {
                // A repair is written in the body's scope. Keep its local
                // reborrow spelling, not the referent used for proof identity.
                let mut written = ResolvedPlace::spelled(root.root, false, Vec::new());
                written.path.extend(root.place_path());
                let place = self.render_header_place(&written, counted);
                return Some(format!("{place}.{}", measure.spelling()));
            }
            _ => return None,
        };
        let place = self.render_header_place(
            &ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, fields),
            counted,
        );
        Some(format!("{place}.{}", measure.spelling()))
    }

    pub(super) fn binding_name(&self, binding: BindingId) -> String {
        self.context
            .binding_names
            .get(binding.0 as usize)
            .cloned()
            .unwrap_or_else(|| "?".to_owned())
    }

    /// [RANGE-1] a name for a suggested range clause or bound variable: the
    /// first of `stem`, `stem1`, `stem2`, ... that no binding of the function
    /// and no package-level, invariant or range-fact declaration spells, so
    /// the suggestion cannot collide [TYPE-6].
    pub(super) fn fresh_name(&self, stem: &str) -> String {
        use crate::DeclarationRole as Role;
        let taken = |name: &str| {
            self.context
                .binding_names
                .iter()
                .any(|binding| binding == name)
                || self.context.declarations.iter().any(|record| {
                    record.spelling() == name
                        && matches!(
                            record.role(),
                            Role::Function
                                | Role::Struct
                                | Role::Enum
                                | Role::Variant
                                | Role::Interface
                                | Role::NamedConst
                                | Role::GenericType
                                | Role::ConstGeneric
                                | Role::Invariant
                                | Role::RangeFact
                                | Role::Alias
                        )
                })
        };
        crate::semantic::check::repairs::fresh_repair_name(stem, taken)
    }

    /// The source spelling of one declaration, such as a generic parameter.
    pub(super) fn declaration_name(&self, declaration: crate::DeclarationId) -> String {
        self.context
            .declarations
            .get(declaration.index())
            .map_or_else(|| "?".to_owned(), |record| record.spelling().to_owned())
    }

    fn render_header_offset(
        &self,
        offset: CapturedValue,
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        match offset.term {
            CapturedTerm::Literal(value) => format!("{value}_u64"),
            CapturedTerm::Binding(binding) => self.render_header_binding(binding, counted),
            CapturedTerm::Const(declaration) => self.declaration_name(declaration),
            CapturedTerm::Place(id) => {
                self.render_header_place(&self.context.captured_places.get(id), counted)
            }
            // [REF-1] the binding's spelling names a later value than the one
            // this index read, and no source spelling names that one.
            CapturedTerm::Superseded(_) | CapturedTerm::SupersededPlace(_) => "?".to_owned(),
            CapturedTerm::Opaque => offset
                .source
                .and_then(|node| self.context.source_spellings.get(&node))
                .cloned()
                .unwrap_or_else(|| "?".to_owned()),
        }
    }

    pub(super) fn render_place(&self, place: &ResolvedPlace) -> String {
        self.render_header_place(place, None)
    }

    /// Substitute the counted binder's spelling at binding leaves, never
    /// field names.
    pub(super) fn render_header_binding(
        &self,
        binding: BindingId,
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        match counted {
            Some(BinderSpelling::Next(binder)) if binder == binding => {
                format!("({} + 1_u64)", self.binding_name(binding))
            }
            Some(BinderSpelling::Bound(binder, variable)) if binder == binding => {
                variable.to_owned()
            }
            _ => self.binding_name(binding),
        }
    }

    pub(super) fn render_header_place(
        &self,
        place: &ResolvedPlace,
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        let reference_root = matches!(place.root, PlaceRoot::Binding(binding)
            if self.places.is_reference(binding));
        let (rendered, ty) = match place.root {
            PlaceRoot::Binding(binding) => (
                {
                    // [REF-1, OP-15] a reference variable names a path and is
                    // not storage of its own, so the storage it names is
                    // reached only through `^`. A term over a reference
                    // anchors at the binding and carries no step of its own,
                    // so the spelling the writer reads puts the step back.
                    let name = self.binding_name(binding);
                    if reference_root {
                        format!("{name}^")
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
                    .map(|constant| constant.declared_type),
            ),
        };
        let range = match place.root {
            PlaceRoot::Binding(binding) => self.binding_is_range(binding),
            PlaceRoot::Constant(_) => false,
        };
        let index_prefix = usize::from(
            matches!(place.root, PlaceRoot::Binding(binding)
                if self.summary(binding).is_some_and(|summary| summary.indexed_atomic))
                && matches!(place.path.first(), Some(PlaceStep::Index(_))),
        );
        self.render_place_projections_at(
            rendered,
            ty,
            range,
            index_prefix,
            &place.path,
            counted,
        )
    }

    fn binding_is_range(&self, binding: BindingId) -> bool {
        self.summary(binding).is_some_and(|summary| summary.range)
    }

    /// Render body and entry paths through the same typed storage selectors.
    ///
    /// The selected type is kept through every step, so a field or payload
    /// below a window part, a page, a table entry or a shared object is named
    /// by its declaration. A step whose type was lost belongs to an
    /// inconsistent checked place; it is spelled with the `?` marker, never
    /// an ordinal, so the residual degrades rather than stopping the compile,
    /// since this renderer has no error channel.
    fn render_place_projections(
        &self,
        rendered: String,
        ty: Option<CheckedType>,
        projections: &[PlaceStep],
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        self.render_place_projections_at(rendered, ty, false, 0, projections, counted)
    }

    fn render_place_projections_at(
        &self,
        rendered: String,
        ty: Option<CheckedType>,
        range: bool,
        index_prefix: usize,
        projections: &[PlaceStep],
        counted: Option<BinderSpelling<'_>>,
    ) -> String {
        crate::semantic::place_render::render_place_projections(
            &ProofPlaceNames { input: self, counted },
            rendered,
            ty,
            range,
            index_prefix,
            projections
                .iter()
                .copied()
                .map(crate::semantic::place_render::PlaceProjection::Place),
        )
        .unwrap_or_else(|never| match never {})
    }
    /// The type one stored element of a window, array, buffer, segment run or
    /// entries run holds.
    fn storage_element_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let element = ty.storage_element()?;
        self.context.elements.get(element.index()).copied()
    }

    /// [SHARE-2] the `Option` entry a table's subscript selects.
    fn table_entry_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(nominal) = ty else {
            return None;
        };
        match self.context.nominals.get(nominal.0 as usize)?.kind {
            CheckedNominalKind::Shared {
                shape: crate::CheckedShared::Map { entry },
                ..
            } => Some(entry),
            _ => None,
        }
    }

    pub(super) fn deref_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(id) = ty else {
            // Borrow bindings retain the referent type in checked form.
            return Some(ty);
        };
        let nominal = self.context.nominals.get(id.0 as usize)?;
        match nominal.kind {
            CheckedNominalKind::Box { referent, .. } => Some(referent),
            // [SHARE-1] a shared object's `^` reaches its state.
            CheckedNominalKind::Shared {
                state,
                shape: crate::CheckedShared::Object,
            } => Some(state),
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

    /// [REF-4, DIAG-1] every range an unproved call goal measures that the
    /// call formed at an argument, in goal order.
    ///
    /// Such a range names no binding, so its `len` reads to a writer as a
    /// value nothing names. Its length is the difference of its two captured
    /// endpoints, and the repair names that difference, which is what the
    /// requirement actually bounds.
    pub(super) fn range_length_readings(&self, goal: &GoalExpression) -> Vec<RangeLengthReading> {
        let mut ranges = Vec::new();
        collect_measured_ranges(goal, &mut ranges);
        let difference_goal = self.render_difference_goal(goal, true);
        ranges
            .into_iter()
            .map(|(datum, range)| RangeLengthReading {
                base: self.render_goal_datum(&range_base(datum)),
                start: self.range_endpoint_reading(range.start),
                end: self.range_endpoint_reading(range.end),
                difference_goal: difference_goal.clone(),
            })
            .collect()
    }

    /// One endpoint as the repair spells it: a term the relation can name in
    /// its source spelling, and otherwise the occurrence that evaluated it.
    fn range_endpoint_reading(&self, endpoint: CapturedValue) -> RangeEndpointReading {
        match endpoint.term {
            CapturedTerm::Literal(value) => RangeEndpointReading::Spelled(format!("{value}_u64")),
            CapturedTerm::Binding(binding) => {
                RangeEndpointReading::Spelled(self.binding_name(binding))
            }
            CapturedTerm::Const(declaration) => {
                RangeEndpointReading::Spelled(self.declaration_name(declaration))
            }
            // [REF-1] a later write made the binding's spelling name another
            // value, so neither it nor the occurrence spells this one.
            CapturedTerm::Place(id) => RangeEndpointReading::Spelled(
                self.render_place(&self.context.captured_places.get(id)),
            ),
            CapturedTerm::Superseded(_) | CapturedTerm::SupersededPlace(_) => {
                RangeEndpointReading::Unspelled(None)
            }
            CapturedTerm::Opaque => RangeEndpointReading::Unspelled(match endpoint.capture {
                CaptureId::Source(occurrence) => Some(occurrence),
                _ => None,
            }),
        }
    }

    /// The goal with every measured range's `len` written as its endpoint
    /// difference [REF-4], parenthesized unless it is a comparison operand;
    /// absent when some endpoint has no spelling.
    fn render_difference_goal(&self, expression: &GoalExpression, bare: bool) -> Option<String> {
        if let Some((_, range)) = measured_range(expression) {
            let start = self.range_endpoint_reading(range.start);
            let end = self.range_endpoint_reading(range.end);
            let (RangeEndpointReading::Spelled(start), RangeEndpointReading::Spelled(end)) =
                (start, end)
            else {
                return None;
            };
            return Some(if start == "0_u64" {
                end
            } else if bare {
                format!("{end} - {start}")
            } else {
                format!("({end} - {start})")
            });
        }
        match expression {
            GoalExpression::Datum(datum) => Some(self.render_goal_datum(datum)),
            GoalExpression::Operation { row, arguments, .. } => {
                let comparison = matches!(
                    row,
                    GoalOperation::Integer {
                        operation: CheckedIntegerOperation::Equal
                            | CheckedIntegerOperation::NotEqual
                            | CheckedIntegerOperation::Less
                            | CheckedIntegerOperation::LessEqual
                            | CheckedIntegerOperation::Greater
                            | CheckedIntegerOperation::GreaterEqual,
                        ..
                    }
                );
                let arguments = arguments
                    .iter()
                    .map(|argument| self.render_difference_goal(argument, comparison))
                    .collect::<Option<Vec<_>>>()?;
                Some(render_goal_row(row, &arguments, self.context.declarations))
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
                    |constant| (constant.name.clone(), Some(constant.declared_type)),
                );
                self.render_goal_projections(name, ty, projections)
            }
            GoalDatum::Place {
                root, projections, admission, ..
            } => {
                let base = self.binding_name(*root);
                let ty = self.summary(*root).and_then(|summary| summary.ty);
                // [REF-1, OP-15] a reference variable names a path and is not
                // storage of its own, so every place that goes through one is
                // written under a `^` step: `p^`, `p^.field`,
                // `part^.len`. A term rooted at a reference anchors at
                // that binding and carries no step of its own — the parameter
                // name *is* the path inside the body — so rendering puts that
                // source wrapper back while retaining every concrete step
                // below the referent.
                let reference = self.places.is_reference(*root);
                let base = if reference { format!("{base}^") } else { base };
                let index_prefix = usize::from(
                    self.summary(*root)
                        .is_some_and(|summary| summary.indexed_atomic)
                        && matches!(projections.first(), Some(GoalProjection::Subscript(_))),
                );
                self.render_goal_projections_at(
                    base,
                    ty,
                    admission.root_is_range,
                    index_prefix,
                    projections,
                )
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
            GoalDatum::Literal(value) => self.render_value(value),
        }
    }

    /// CONST-2 aggregate values have cvalue spellings, rather than FORM-5
    /// literals: lists and complete constructors in declared field order.
    fn render_value(&self, value: &CheckedValue) -> String {
        match value {
            CheckedValue::Integer { ty, bits } => {
                format!("{}_{}", integer_value(*ty, *bits), integer_type_name(*ty))
            }
            CheckedValue::Float { ty, bits } => {
                super::super::super::check::floats::float_value_spelling(*ty, *bits)
            }
            CheckedValue::Unit => "unit".to_owned(),
            CheckedValue::Bool(true) => "True()".to_owned(),
            CheckedValue::Bool(false) => "False()".to_owned(),
            CheckedValue::ConstGeneric { declaration, .. } => self.declaration_name(*declaration),
            CheckedValue::NumericIdentity { ty, one } => {
                format!("{}_{}", u8::from(*one), self.render_numeric_type(*ty))
            }
            CheckedValue::Array { ty, elements } => {
                if elements.is_empty() {
                    return match ty.storage_element().and_then(|element| {
                        self.context.elements.get(element.index())
                    }) {
                        Some(CheckedType::Integer(IntegerType::U8)) => "\"\"".to_owned(),
                        _ => "?".to_owned(),
                    };
                }
                format!(
                    "[{}]",
                    elements
                        .iter()
                        .map(|value| self.render_value(value))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            CheckedValue::Struct { ty, fields } => {
                let Some(CheckedNominalKind::Struct { fields: declared }) = (match ty {
                    CheckedType::Nominal(id) => self
                        .context
                        .nominals
                        .get(id.0 as usize)
                        .map(|nominal| &nominal.kind),
                    _ => None,
                }) else {
                    return "?".to_owned();
                };
                let Some(name) = self.context.type_spellings.get(ty) else {
                    return "?".to_owned();
                };
                let arguments = declared
                    .iter()
                    .zip(fields)
                    .map(|(field, value)| {
                        format!("{}: {}", field.name, self.render_value(value))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({arguments})")
            }
        }
    }

    fn render_numeric_type(&self, ty: CheckedType) -> String {
        match ty {
            CheckedType::Integer(ty) => integer_type_name(ty).to_owned(),
            CheckedType::Float(FloatType::F32) => "f32".to_owned(),
            CheckedType::Float(FloatType::F64) => "f64".to_owned(),
            CheckedType::GenericInt(declaration) | CheckedType::GenericFloat(declaration) => {
                self.declaration_name(declaration)
            }
            _ => "?".to_owned(),
        }
    }

    pub(super) fn render_normalized_integer(&self, value: i128, ty: CheckedType) -> String {
        if matches!(ty, CheckedType::GenericInt(_)) && matches!(value, 0 | 1) {
            return format!("{value}_{}", self.render_numeric_type(ty));
        }
        normalized_integer(
            value,
            match ty {
                CheckedType::Integer(ty) => Some(ty),
                _ => None,
            },
        )
    }

    pub(super) fn render_goal_projections(
        &self,
        base: String,
        root_type: Option<CheckedType>,
        projections: &[GoalProjection],
    ) -> String {
        self.render_goal_projections_at(base, root_type, false, 0, projections)
    }

    pub(super) fn render_goal_projections_at(
        &self,
        base: String,
        root_type: Option<CheckedType>,
        range: bool,
        index_prefix: usize,
        projections: &[GoalProjection],
    ) -> String {
        use crate::semantic::place_render::{PlaceProjection, render_place_projections};
        let projections = projections.iter().map(|projection| match *projection {
            GoalProjection::FormalSubscript { ordinal }
            | GoalProjection::FormalPlaceSubscript { ordinal, .. } => {
                PlaceProjection::FormalSubscript(ordinal)
            }
            GoalProjection::FormalPage { ordinal }
            | GoalProjection::FormalPlacePage { ordinal, .. } => {
                PlaceProjection::FormalPage(ordinal)
            }
            projection => PlaceProjection::Place(projection.place_step()),
        });
        render_place_projections(
            &ProofPlaceNames { input: self, counted: None },
            base,
            root_type,
            range,
            index_prefix,
            projections,
        )
        .unwrap_or_else(|never| match never {})
    }
    /// Render the checked storage path without reducing subscript offsets
    /// to overlap identities: even a non-term offset retains its source
    /// expression in an obligation's residual.
    pub(super) fn render_storage_place(&self, root: &CheckedContainerRoot) -> String {
        use crate::semantic::place_render::{PlaceProjection, render_place_projections};
        let rendered = self.render_place(&ResolvedPlace {
            atomic_aliases: Vec::new(),
            root: root.root,
            path: Vec::new(),
        });
        let ty = match root.root {
            PlaceRoot::Binding(binding) => self.summary(binding).and_then(|summary| summary.ty),
            PlaceRoot::Constant(id) => self
                .context
                .constants
                .get(id.0 as usize)
                .map(|value| value.ty),
        };
        let projections = root.path.iter().map(|step| match step {
            CheckedPlaceStep::Field(field) => PlaceProjection::Place(PlaceStep::Field(*field)),
            CheckedPlaceStep::Payload { variant, field, .. } => {
                PlaceProjection::Place(PlaceStep::Payload {
                    variant: *variant,
                    field: *field,
                })
            }
            CheckedPlaceStep::BoxReferent(_) => PlaceProjection::Place(PlaceStep::Deref),
            CheckedPlaceStep::Measure(measure) => {
                PlaceProjection::Place(PlaceStep::Measure(*measure))
            }
            CheckedPlaceStep::Subscript(index) => PlaceProjection::Subscript {
                offset: self.render_expression(&index.offset),
                element: index.element_type,
            },
        });
        render_place_projections(
            &ProofPlaceNames { input: self, counted: None },
            rendered,
            ty,
            false,
            0,
            projections,
        )
        .unwrap_or_else(|never| match never {})
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
                self.render_place(&root.proof_place()),
                measure.spelling(),
            ),
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let resolved = place.proof_place();
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
                format!("{}^", self.binding_name(*binding))
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
    /// Renders a normalized relation whose source type nobody supplies, such
    /// as a condition fact: its constants take the integer type of the
    /// relation's own typed terms.
    pub(super) fn render_relation_of_terms(&self, relation: &Relation) -> String {
        let (Relation::Bound { left, right, .. }
        | Relation::Equal { left, right, .. }
        | Relation::Distinct { left, right, .. }) = relation;
        // Offsets, lengths and counted captures are `u64` [OP-4], so a
        // relation with no other typed term reads as one over `u64`.
        let ty = self
            .term_integer_type(*left)
            .or_else(|| self.term_integer_type(*right))
            .unwrap_or(IntegerType::U64);
        self.render_relation(relation, CheckedType::Integer(ty))
    }

    /// The integer type a term is typed by, where the term itself records
    /// one.
    fn term_integer_type(&self, term: TermId) -> Option<IntegerType> {
        match self.vocabulary.terms.kind(term) {
            TermKind::Sum(left, right) => self
                .term_integer_type(*left)
                .or_else(|| self.term_integer_type(*right)),
            TermKind::TargetMeasure { source, .. } => self.term_integer_type(*source),
            TermKind::Measure(..) => Some(IntegerType::U64),
            TermKind::ConstParameter(_, ty)
            | TermKind::Place(_, ty)
            | TermKind::ResultPayload { ty, .. }
            | TermKind::CommitValue { ty, .. }
            | TermKind::CallDatum { ty, .. }
            | TermKind::EntryDatum { ty, .. }
            | TermKind::MeasureDatum { ty, .. } => Some(*ty),
            TermKind::Zero
            | TermKind::Constant(_)
            | TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. } => None,
        }
    }

    /// Renders one normalized relation for diagnostics.
    pub(super) fn render_relation(&self, relation: &Relation, ty: CheckedType) -> String {
        match relation {
            Relation::Bound { left, right, bound } => format!(
                "{} - {} <= {}",
                self.render_term(*left, ty),
                self.render_term(*right, ty),
                self.input.render_normalized_integer(*bound, ty)
            ),
            // An undisplaced relation reads as the writer wrote it; a
            // displaced one names its difference, exactly as a bound does.
            Relation::Equal {
                left,
                right,
                difference: 0,
            } => format!(
                "{} = {}",
                self.render_term(*left, ty),
                self.render_term(*right, ty)
            ),
            Relation::Equal {
                left,
                right,
                difference,
            } => format!(
                "{} - {} = {}",
                self.render_term(*left, ty),
                self.render_term(*right, ty),
                self.input.render_normalized_integer(*difference, ty)
            ),
            Relation::Distinct {
                left,
                right,
                difference: 0,
            } => format!(
                "{} != {}",
                self.render_term(*left, ty),
                self.render_term(*right, ty)
            ),
            Relation::Distinct {
                left,
                right,
                difference,
            } => format!(
                "{} - {} != {}",
                self.render_term(*left, ty),
                self.render_term(*right, ty),
                self.input.render_normalized_integer(*difference, ty)
            ),
        }
    }

    pub(super) fn render_term(&self, term: TermId, ty: CheckedType) -> String {
        match self.vocabulary.terms.kind(term) {
            TermKind::TargetMeasure { source, .. } => {
                format!("next-header {}", self.render_term(*source, ty))
            }
            TermKind::Sum(left, right) => {
                format!(
                    "({} + {})",
                    self.render_term(*left, ty),
                    self.render_term(*right, ty)
                )
            }
            TermKind::Zero => self.input.render_normalized_integer(0, ty),
            TermKind::Constant(value) => self.input.render_normalized_integer(*value, ty),
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
            TermKind::ResultPayload {
                payload,
                path,
                measure,
                ..
            } => {
                let place = self.input.render_place_projections(
                    "<success payload>".to_owned(),
                    Some(*payload),
                    path,
                    None,
                );
                measure.map_or(place.clone(), |measure| {
                    format!("{place}.{}", measure.spelling())
                })
            }
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
                ..
            } => {
                let parameter = self.input.function.parameters.get(*formal as usize);
                let (base, ty, range, projections) = parameter.map_or_else(
                    || ("?".to_owned(), None, false, projections.as_slice()),
                    |parameter| {
                        if parameter.mode.is_reference() {
                            // The clause retains its leading reference step;
                            // the remaining steps select storage below it.
                            let projections = match projections.split_first() {
                                Some((PlaceStep::Deref, rest)) => rest,
                                _ => projections,
                            };
                            (
                                format!("entry({})^", parameter.name),
                                Some(parameter.ty),
                                parameter.mode.is_range(),
                                projections,
                            )
                        } else {
                            (
                                parameter.name.clone(),
                                Some(parameter.ty),
                                false,
                                projections.as_slice(),
                            )
                        }
                    },
                );
                let place = self.input.render_place_projections_at(
                    base, ty, range, 0, projections, None,
                );
                measure.map_or(place.clone(), |measure| {
                    format!("{place}.{}", measure.spelling())
                })
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
                measure.map_or_else(
                    || format!("<value at {event}>"),
                    |measure| format!("<{} at {event}>", measure.spelling()),
                )
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
/// The range a goal node measures, when the node is the `len` of a range an
/// argument formed at its call: a place whose last step is that range [REF-4].
fn measured_range(expression: &GoalExpression) -> Option<(&GoalDatum, CapturedRange)> {
    let GoalExpression::Operation {
        row:
            GoalOperation::ArrayMeasure { measure, .. }
            | GoalOperation::BufferMeasure { measure, .. }
            | GoalOperation::ContainerMeasure { measure, .. },
        arguments,
        ..
    } = expression
    else {
        return None;
    };
    let [GoalExpression::Datum(datum)] = arguments.as_slice() else {
        return None;
    };
    let (GoalDatum::Place { projections, .. } | GoalDatum::NamedConst { projections, .. }) = datum
    else {
        return None;
    };
    match projections.last() {
        Some(GoalProjection::Range(range)) if *measure == CheckedMeasure::Length => {
            Some((datum, *range))
        }
        _ => None,
    }
}

/// The place a measured range was formed over: its datum without the final
/// range step. Rendering reads the root's own type, never the datum's.
fn range_base(datum: &GoalDatum) -> GoalDatum {
    let mut base = datum.clone();
    if let GoalDatum::Place { projections, .. } | GoalDatum::NamedConst { projections, .. } =
        &mut base
    {
        projections.pop();
    }
    base
}

fn collect_measured_ranges<'goal>(
    expression: &'goal GoalExpression,
    ranges: &mut Vec<(&'goal GoalDatum, CapturedRange)>,
) {
    if let Some(range) = measured_range(expression) {
        ranges.push(range);
        return;
    }
    if let GoalExpression::Operation { arguments, .. } = expression {
        for argument in arguments {
            collect_measured_ranges(argument, ranges);
        }
    }
}

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
        GoalOperation::ValueEquality { equal, .. } => {
            render_operation_spelling(if *equal { "==" } else { "!=" }, arguments)
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
                CheckedConversionMode::Wrap => "cvt.wrap",
                CheckedConversionMode::Nearest => "cvt.nearest",
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

/// A normalized relation uses mathematical integers. Prefer its source
/// type, but never give a negative unsigned or out-of-range literal a suffix
/// that cannot spell it. Larger constants remain exact mathematical source
/// expressions composed entirely of legal literals.
fn normalized_integer(value: i128, preferred: Option<IntegerType>) -> String {
    if let Some(ty) = preferred {
        let (minimum, maximum) = type_range(ty);
        if (minimum..=maximum).contains(&value) {
            return format!("{value}_{}", integer_type_name(ty));
        }
    }
    if i64::try_from(value).is_ok() {
        return format!("{value}_i64");
    }
    if u64::try_from(value).is_ok() {
        return format!("{value}_u64");
    }
    fn magnitude(value: u128) -> String {
        if value <= u128::from(u64::MAX) {
            return format!("{value}_u64");
        }
        let base = 1_u128 << 32;
        format!(
            "({} * 4294967296_u64 + {}_u64)",
            magnitude(value / base),
            value % base
        )
    }
    let magnitude = magnitude(value.unsigned_abs());
    if value < 0 {
        format!("(0_i64 - {magnitude})")
    } else {
        magnitude
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_constants_use_only_legal_typed_literals() {
        assert_eq!(normalized_integer(0, Some(IntegerType::U64)), "0_u64");
        assert_eq!(normalized_integer(-1, Some(IntegerType::U64)), "-1_i64");
        assert_eq!(normalized_integer(256, Some(IntegerType::U8)), "256_i64");
        assert_eq!(normalized_integer(i128::from(u64::MAX), None), "18446744073709551615_u64");
        assert_eq!(normalized_integer(i128::from(u64::MAX) + 1, None),
            "(4294967296_u64 * 4294967296_u64 + 0_u64)");
        assert_eq!(normalized_integer(i128::MIN, None),
            "(0_i64 - ((9223372036854775808_u64 * 4294967296_u64 + 0_u64) * 4294967296_u64 + 0_u64))");
    }
}
