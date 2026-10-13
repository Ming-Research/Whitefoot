use crate::semantic::check::CheckContext;
use crate::semantic::check::FunctionContext;
use crate::semantic::check::{DeclarationInventory, TypeContext};
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::syntax::terminal::FixedTerminal;
use crate::{
    DeclarationClass, DeclarationId, LexicalUseRole, PostconditionResolutionRecord, Production,
    ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::goal::{
    CheckedRequirement, GoalDatum, GoalExpression, GoalOperation, GoalProjection, GoalTemplate,
};
use super::super::model::{
    BindingId, CheckedConst, CheckedConversionMode, CheckedExpression, CheckedFloatOperation,
    CheckedIntegerOperation, CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedPlaceStep,
    CheckedStatement, CheckedType, CheckedValue, expression_children,
};
use super::super::places::{CapturedTerm, CapturedValue};
use super::super::postcondition::{ParameterDenotation, PostconditionConstantOrigin};
use super::{CheckStop, Checker, ControlCounters, ControlScope, LocalBinding};

pub(super) struct CheckedRequires {
    pub(super) requirements: Vec<CheckedRequirement>,
    /// [RANGE-1] the range clauses among the requirements, in source order.
    pub(super) range: Vec<super::super::range_facts::CheckedRangeClause>,
    /// Expanded erased definitions used by range-clause contract positions.
    pub(super) definitions: Vec<(BindingId, GoalExpression)>,
    /// The [ENT-2] clause (b) places each requirement forms, index-aligned
    /// with `requirements` [`super::super::model::CheckedFunction::requirement_places`].
    pub(super) places: Vec<Vec<CheckedExpression>>,
}

/// One checked definition's contribution to the requirements that expand it.
pub(super) struct ClauseDefinition {
    pub(super) binding: BindingId,
    /// The clause (b) places its own initializer writes.
    pub(super) places: Vec<CheckedExpression>,
    /// The earlier definitions its initializer reads.
    pub(super) uses: Vec<BindingId>,
}

/// [ENT-2] every clause (b) place one checked clause expression writes, in
/// source order: a measure of a subscripted place and a subscripted read,
/// the two forms whose subscripts owe [OP-4] where the place is formed.
pub(super) fn collect_clause_places(
    expression: &CheckedExpression,
    places: &mut Vec<CheckedExpression>,
) {
    let subscripted = |path: &[CheckedPlaceStep]| {
        path.iter()
            .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
    };
    match expression {
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. }
            if subscripted(&root.path) =>
        {
            places.push(expression.clone());
        }
        CheckedExpression::RangeMeasure { root, .. } if root.formation.is_some() => {
            places.push(expression.clone());
        }
        CheckedExpression::ArrayIndex { .. }
        | CheckedExpression::BufferIndex { .. }
        | CheckedExpression::RangeElementMeasure { .. }
        | CheckedExpression::RangeIndex { .. } => {
            places.push(expression.clone());
        }
        _ => {}
    }
    for child in expression_children(expression) {
        collect_clause_places(child, places);
    }
}

/// Earlier checked definitions after alpha expansion. Their full expressions
/// remain available when a later selector projects an aggregate alias, and
/// preserve an inadmissible computed or element offset for clause rejection.
pub(super) type DefinitionOffsets = HashMap<BindingId, CheckedExpression>;

/// The offset a definition's value supplies after expansion, when it is one.
pub(super) fn definition_offset(
    value: &CheckedExpression,
    offsets: &DefinitionOffsets,
    types: &TypeContext<'_>,
) -> Result<CheckedExpression, CheckStop> {
    let mut expanded = expand_definition_value(value, offsets, types)?;
    expand_definition_offsets(&mut expanded, offsets, types)?;
    Ok(expanded)
}

/// Alpha-expand a checked datum before deciding whether it can be a
/// selector. Projected aliases use the same typed field nodes as ordinary
/// checked expressions; an inadmissible initializer remains visible to the
/// clause admission judgment instead of becoming an erased local's term.
fn expand_definition_value(
    value: &CheckedExpression,
    offsets: &DefinitionOffsets,
    types: &TypeContext<'_>,
) -> Result<CheckedExpression, CheckStop> {
    match value {
        CheckedExpression::Binding {
            binding,
            consume_root: false,
            ..
        } => Ok(offsets
            .get(binding)
            .cloned()
            .unwrap_or_else(|| value.clone())),
        CheckedExpression::Project {
            carrier,
            binding,
            fields,
            consume_root: false,
            ..
        } if offsets.contains_key(binding) => {
            let mut value = offsets
                .get(binding)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .clone();
            for field in fields {
                let CheckedType::Nominal(nominal) = value.ty() else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let CheckedNominalKind::Struct { fields } = &types
                    .nominals
                    .get(nominal.0 as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .kind
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let ty = fields
                    .get(*field as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .ty;
                value = CheckedExpression::ProjectValue {
                    carrier: carrier.clone(),
                    value: Box::new(value),
                    nominal,
                    field: *field,
                    ty,
                };
            }
            Ok(value)
        }
        CheckedExpression::ProjectValue {
            carrier,
            value,
            nominal,
            field,
            ty,
        } => Ok(CheckedExpression::ProjectValue {
            carrier: carrier.clone(),
            value: Box::new(expand_definition_value(value, offsets, types)?),
            nominal: *nominal,
            field: *field,
            ty: *ty,
        }),
        CheckedExpression::BufferMeasure { measure, root } => {
            let mut root = super::super::model::CheckedContainerRoot {
                root: super::super::places::PlaceRoot::Binding(root.binding),
                path: root.path.clone(),
                ty: CheckedType::Buffer {
                    element: root.element,
                },
                proof_base: root.proof_base.clone(),
            };
            expand_definition_root(&mut root, offsets)?;
            Ok(CheckedExpression::ContainerMeasure {
                measure: *measure,
                root,
            })
        }
        CheckedExpression::ArrayMeasure {
            measure,
            root: super::super::model::CheckedArrayRoot::Binding { binding, fields },
            ..
        } if offsets.contains_key(binding) => {
            let value = offsets
                .get(binding)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut root = definition_container_root(value)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for field in fields {
                let CheckedType::Nominal(nominal) = root.ty else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let CheckedNominalKind::Struct { fields } = &types.nominal(nominal)?.kind else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                root.ty = fields
                    .get(*field as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .ty;
                root.path.push(CheckedPlaceStep::Field(*field));
            }
            Ok(CheckedExpression::ContainerMeasure {
                measure: *measure,
                root,
            })
        }
        CheckedExpression::ContainerMeasure { measure, root } => {
            let mut root = root.clone();
            expand_definition_root(&mut root, offsets)?;
            Ok(CheckedExpression::ContainerMeasure {
                measure: *measure,
                root,
            })
        }
        CheckedExpression::BoxDeref { .. } => {
            let mut expanded = value.clone();
            if let CheckedExpression::BoxDeref { value, .. } = &mut expanded {
                **value = expand_definition_value(value, offsets, types)?;
            }
            Ok(expanded)
        }
        _ => Ok(value.clone()),
    }
}

/// A direct alias keeps the same checked storage vocabulary when a later
/// measure or subscript selects below it; no erased definition is a root.
fn definition_container_root(
    value: &CheckedExpression,
) -> Option<super::super::model::CheckedContainerRoot> {
    use super::super::model::CheckedContainerRoot;
    use super::super::places::PlaceRoot;
    let root = |root, path, proof_base| CheckedContainerRoot {
        root,
        path,
        proof_base,
        ty: value.ty(),
    };
    match value {
        CheckedExpression::Binding { binding, .. } => {
            Some(root(PlaceRoot::Binding(*binding), Vec::new(), None))
        }
        CheckedExpression::NamedConstant { constant, .. } => {
            Some(root(PlaceRoot::Constant(*constant), Vec::new(), None))
        }
        CheckedExpression::Project {
            binding, fields, ..
        } => Some(root(
            PlaceRoot::Binding(*binding),
            fields
                .iter()
                .copied()
                .map(CheckedPlaceStep::Field)
                .collect(),
            None,
        )),
        CheckedExpression::DerefAddressed { binding, proof, .. } => Some(root(
            PlaceRoot::Binding(*binding),
            Vec::new(),
            proof.clone(),
        )),
        CheckedExpression::ProjectValue {
            value: base, field, ..
        } => {
            let mut selected = definition_container_root(base)?;
            selected.path.push(CheckedPlaceStep::Field(*field));
            selected.ty = value.ty();
            Some(selected)
        }
        CheckedExpression::BoxDeref {
            value: base,
            nominal,
            ..
        } => {
            let mut selected = definition_container_root(base)?;
            selected.path.push(CheckedPlaceStep::BoxReferent(*nominal));
            selected.ty = value.ty();
            Some(selected)
        }
        CheckedExpression::ReadStorage { root, .. } => Some(root.clone()),
        _ => None,
    }
}

fn expand_definition_root(
    root: &mut super::super::model::CheckedContainerRoot,
    offsets: &DefinitionOffsets,
) -> Result<(), CheckStop> {
    let super::super::places::PlaceRoot::Binding(binding) = root.root else {
        return Ok(());
    };
    let Some(value) = offsets.get(&binding) else {
        return Ok(());
    };
    let mut expanded =
        definition_container_root(value).ok_or(SemanticCompilerFailure::InvalidResolution)?;
    expanded.path.extend(root.path.iter().cloned());
    expanded.ty = root.ty;
    *root = expanded;
    Ok(())
}

/// One subscript offset of a clause place, read through a definition, is the
/// definition's own offset after expansion [FN-8].
fn expand_definition_offset(
    offset: &mut CheckedExpression,
    captured: &mut CapturedValue,
    offsets: &DefinitionOffsets,
    types: &TypeContext<'_>,
) -> Result<(), CheckStop> {
    *offset = expand_definition_value(offset, offsets, types)?;
    captured.term = offset.tracked_offset().unwrap_or(CapturedTerm::Opaque);
    Ok(())
}

fn expand_definition_path_offsets(
    path: &mut [CheckedPlaceStep],
    offsets: &DefinitionOffsets,
    types: &TypeContext<'_>,
) -> Result<(), CheckStop> {
    for step in path {
        if let CheckedPlaceStep::Subscript(subscript) = step {
            let subscript = subscript.as_mut();
            expand_definition_offset(
                &mut subscript.offset,
                &mut subscript.captured,
                offsets,
                types,
            )?;
        }
    }
    Ok(())
}

/// [FN-8] a definition is erased by expansion, so a clause (b) place is formed
/// with every offset it reads through a definition replaced by the offset
/// that definition expands to; its subscripts then owe [OP-4] over the same
/// terms the expanded requirement establishes.
pub(super) fn expand_definition_offsets(
    place: &mut CheckedExpression,
    offsets: &DefinitionOffsets,
    types: &TypeContext<'_>,
) -> Result<(), CheckStop> {
    // Legacy flat indexes carry their base outside CheckedContainerRoot.
    // Canonicalize an erased alias into the shared proof-only storage form
    // before forming its OP-4 obligations.
    let aliased_index = match place {
        CheckedExpression::ArrayIndex {
            carrier,
            root: super::super::model::CheckedArrayRoot::Binding { binding, fields },
            element_type,
            offset,
            obligation,
            target_domain,
            ..
        } if offsets.contains_key(binding) => {
            let value = offsets
                .get(binding)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut root = definition_container_root(value)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for field in fields {
                let CheckedType::Nominal(nominal) = root.ty else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let CheckedNominalKind::Struct { fields } = &types.nominal(nominal)?.kind else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                root.ty = fields
                    .get(*field as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .ty;
                root.path.push(CheckedPlaceStep::Field(*field));
            }
            Some((
                carrier.clone(),
                root,
                *element_type,
                (**offset).clone(),
                obligation.clone(),
                *target_domain,
            ))
        }
        CheckedExpression::BufferIndex {
            carrier,
            root,
            offset,
            obligation,
            target_domain,
        } if offsets.contains_key(&root.binding) => {
            let mut selected = super::super::model::CheckedContainerRoot {
                root: super::super::places::PlaceRoot::Binding(root.binding),
                path: root.path.clone(),
                ty: CheckedType::Buffer {
                    element: root.element,
                },
                proof_base: root.proof_base.clone(),
            };
            expand_definition_root(&mut selected, offsets)?;
            Some((
                carrier.clone(),
                selected,
                root.element_type,
                (**offset).clone(),
                obligation.clone(),
                *target_domain,
            ))
        }
        _ => None,
    };
    if let Some((carrier, mut root, element_type, offset, obligation, target_domain)) =
        aliased_index
    {
        root.path.push(CheckedPlaceStep::Subscript(Box::new(
            super::super::model::CheckedPlaceSubscript {
                base_type: root.ty,
                element_type,
                offset,
                obligation,
                target_domain,
                captured: CapturedValue::unknown(),
            },
        )));
        root.ty = element_type;
        *place = CheckedExpression::ReadStorage { carrier, root };
    }
    match place {
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            expand_definition_root(root, offsets)?;
            expand_definition_path_offsets(&mut root.path, offsets, types)?;
        }
        CheckedExpression::ArrayIndex { offset, .. } => {
            let mut captured = CapturedValue::unknown();
            expand_definition_offset(offset, &mut captured, offsets, types)?;
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            expand_definition_path_offsets(&mut root.path, offsets, types)?;
            let mut captured = CapturedValue::unknown();
            expand_definition_offset(offset, &mut captured, offsets, types)?;
        }
        CheckedExpression::RangeMeasure { root, .. } => {
            if let Some(formation) = root.formation.as_deref_mut() {
                expand_definition_offsets(formation, offsets, types)?;
            }
        }
        CheckedExpression::BorrowSegment { root, segment, .. } => {
            match root {
                super::super::model::CheckedSegmentSource::Storage(root) => {
                    expand_definition_path_offsets(&mut root.path, offsets, types)?
                }
                super::super::model::CheckedSegmentSource::Element(place) => {
                    if let Some(formation) = place.root.formation.as_deref_mut() {
                        expand_definition_offsets(formation, offsets, types)?;
                    }
                    expand_definition_offset(
                        &mut place.offset,
                        &mut place.captured,
                        offsets,
                        types,
                    )?;
                    expand_definition_path_offsets(&mut place.path, offsets, types)?;
                }
            }
            if let super::super::model::CheckedSegmentSelect::One(index)
            | super::super::model::CheckedSegmentSelect::Page(index) = segment
            {
                expand_definition_offset(&mut index.offset, &mut index.captured, offsets, types)?;
            }
        }
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. } => {
            if let Some(formation) = place.root.formation.as_deref_mut() {
                expand_definition_offsets(formation, offsets, types)?;
            }
            expand_definition_offset(&mut place.offset, &mut place.captured, offsets, types)?;
            expand_definition_path_offsets(&mut place.path, offsets, types)?;
        }
        _ => {}
    }
    Ok(())
}

/// The bindings one checked clause expression reads by name.
pub(super) fn collect_clause_reads(expression: &CheckedExpression, reads: &mut Vec<BindingId>) {
    if let CheckedExpression::Binding { binding, .. } | CheckedExpression::Project { binding, .. } =
        expression
    {
        reads.push(*binding);
    }
    for child in expression_children(expression) {
        collect_clause_reads(child, reads);
    }
}

#[derive(Clone, Copy)]
pub(super) enum ClauseKind<'record> {
    Requires,
    Postcondition(&'record PostconditionResolutionRecord),
}

/// One source-stable leaf shared by FN-8 and FN-9 alpha expansion.  The
/// symbolic result is private to this intermediate tree; conversion to a
/// GoalTemplate rejects it, so GoalDatum and GoalTemplate remain unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExpandedClauseDatum {
    Parameter {
        ordinal: u32,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
        /// [MSR-3] which table row gives the datum its meaning: a bare
        /// measure or place of a written reference parameter in ensures
        /// denotes exit state, and an entry former selects the entry datum
        /// before ordinary projections are expanded.
        denotation: ParameterDenotation,
    },
    NamedConst {
        declaration: DeclarationId,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    Literal {
        value: CheckedValue,
        origin: PostconditionConstantOrigin,
    },
    /// One [FN-9] clause result datum: the declared result ordinal it
    /// names [CALL-4], the member path written below it, and the type that
    /// path reaches.
    ///
    /// [OP-15] reads a measure as a member of the measured place, so a
    /// result datum carries the same projection path a parameter datum does:
    /// `result.inner.len` measures the box content the written path reaches,
    /// not the box.
    Result {
        ordinal: u32,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
}

impl ExpandedClauseDatum {
    pub(super) const fn ty(&self) -> CheckedType {
        match self {
            Self::Parameter { ty, .. } | Self::NamedConst { ty, .. } | Self::Result { ty, .. } => {
                *ty
            }
            Self::Literal { value, .. } => value.ty(),
        }
    }

    fn with_projection(mut self, projection: GoalProjection, ty: CheckedType) -> Option<Self> {
        match &mut self {
            Self::Parameter {
                projections,
                ty: datum_ty,
                ..
            }
            | Self::NamedConst {
                projections,
                ty: datum_ty,
                ..
            }
            | Self::Result {
                projections,
                ty: datum_ty,
                ..
            } => {
                projections.push(projection);
                *datum_ty = ty;
                Some(self)
            }
            Self::Literal { .. } => None,
        }
    }
}

/// The one alpha-expanded expression representation used by both clause
/// families.  FN-8 converts the complete tree to GoalExpression; FN-9 admits
/// only a comparison root whose two children downcast to closed datums.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExpandedClauseExpression {
    Datum(ExpandedClauseDatum),
    Operation {
        row: GoalOperation,
        type_arguments: Vec<CheckedType>,
        const_arguments: Vec<CheckedConst>,
        result: CheckedType,
        arguments: Vec<Self>,
    },
    InvalidSelectorUse {
        ty: CheckedType,
    },
}

impl ExpandedClauseExpression {
    pub(super) const fn ty(&self) -> CheckedType {
        match self {
            Self::Datum(datum) => datum.ty(),
            Self::Operation { result, .. } | Self::InvalidSelectorUse { ty: result } => *result,
        }
    }

    pub(super) fn contains_invalid_selector_use(&self) -> bool {
        match self {
            Self::InvalidSelectorUse { .. } => true,
            Self::Operation { arguments, .. } => {
                arguments.iter().any(Self::contains_invalid_selector_use)
            }
            Self::Datum(_) => false,
        }
    }

    fn with_projection(self, projection: GoalProjection, ty: CheckedType) -> Option<Self> {
        let Self::Datum(datum) = self else {
            return None;
        };
        datum.with_projection(projection, ty).map(Self::Datum)
    }

    pub(super) fn into_goal_expression(self) -> Option<GoalExpression> {
        match self {
            Self::Datum(ExpandedClauseDatum::Parameter {
                ordinal,
                projections,
                ty,
                ..
            }) => Some(GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            })),
            Self::Datum(ExpandedClauseDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) => Some(GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
                admission: Default::default(),
            })),
            Self::Datum(ExpandedClauseDatum::Literal { value, .. }) => {
                Some(GoalExpression::Datum(GoalDatum::Literal(value)))
            }
            Self::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => Some(GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments: arguments
                    .into_iter()
                    .map(Self::into_goal_expression)
                    .collect::<Option<Vec<_>>>()?,
            }),
            Self::Datum(ExpandedClauseDatum::Result { .. }) | Self::InvalidSelectorUse { .. } => {
                None
            }
        }
    }
}

impl<'unit> Checker<'_, 'unit> {
    pub(super) fn check_requires(
        &mut self,
        context: FunctionContext<'_, '_>,
        block: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<CheckedRequires, CheckStop> {
        let FunctionContext {
            check_context,
            function,
        } = context;
        let mut expanded_bindings = HashMap::new();
        let mut definitions: Vec<ClauseDefinition> = Vec::new();
        let mut definition_offsets = DefinitionOffsets::new();
        for (ordinal, parameter) in function.parameters.iter().enumerate() {
            let local = bindings
                .get(&parameter.declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let ordinal =
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            expanded_bindings.insert(
                local.binding,
                ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                    ordinal,
                    projections: Vec::new(),
                    ty: parameter.ty,
                    denotation: ParameterDenotation::EntryImage,
                }),
            );
        }
        for definition in self
            .types
            .declarations
            .tree
            .children_with(block, Production::ContractDefine)?
        {
            let expression = self
                .types
                .declarations
                .tree
                .first_child_with(definition, Production::Expr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.types
                .check_range_referent_places(check_context, expression, bindings)?;
            if !self.types.declarations.validate_clause_computation(
                ClauseKind::Requires,
                definition,
                expression,
            )? {
                self.types.declarations.validate_clause_definition_datum(
                    ClauseKind::Requires,
                    definition,
                    expression,
                )?;
            }
            let checked = self
                .check_statement(
                    context,
                    definition,
                    bindings,
                    counters,
                    ControlScope {
                        loops: &[],
                        give_context: None,
                    },
                )
                .map_err(Checker::clause_conditional_repair)?;
            if !checked.can_continue {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            let CheckedStatement::Let { binding, value, .. } = &checked.statement else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            self.types
                .validate_clause_checked_forms(ClauseKind::Requires, definition, value)?;
            self.types.validate_clause_alias_offsets(
                ClauseKind::Requires,
                definition,
                value,
                &definition_offsets,
            )?;
            self.types.validate_clause_copy_local(
                check_context,
                ClauseKind::Requires,
                definition,
                *binding,
                bindings,
            )?;
            let expanded = self.types.build_clause_expression(
                context,
                expression,
                value,
                bindings,
                &expanded_bindings,
            )?;
            expanded_bindings.insert(*binding, expanded);
            let mut places = Vec::new();
            collect_clause_places(value, &mut places);
            for place in &mut places {
                expand_definition_offsets(place, &definition_offsets, self.types)?;
            }
            let offset = definition_offset(value, &definition_offsets, self.types)?;
            definition_offsets.insert(*binding, offset);
            let mut uses = Vec::new();
            collect_clause_reads(value, &mut uses);
            uses.retain(|read| definitions.iter().any(|earlier| earlier.binding == *read));
            definitions.push(ClauseDefinition {
                binding: *binding,
                places,
                uses,
            });
        }

        // [ENT-2, FN-8] a requirement forms its places at body entry, in the
        // state holding the requirements written before it. A definition is
        // erased by expansion, so its places are formed in the first
        // requirement whose expansion reaches it; the state only grows along
        // the requirements, so a later expansion owes nothing the first did
        // not.
        let mut expanded_definitions = HashSet::new();
        let mut requirement_places = Vec::new();
        let mut requirements = Vec::new();
        let mut range = Vec::new();
        for clause in self
            .types
            .declarations
            .tree
            .children_with(block, Production::RequiresClause)?
        {
            if let Some(range_clause) = self
                .types
                .declarations
                .tree
                .first_child_with(clause, Production::RangeClause)?
            {
                range.extend(self.check_range_clause(context, range_clause, bindings)?);
                continue;
            }
            let expression = self
                .types
                .declarations
                .tree
                .first_child_with(clause, Production::ClauseExpr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.types
                .check_range_referent_places(check_context, expression, bindings)?;
            self.types.declarations.validate_clause_condition(
                ClauseKind::Requires,
                clause,
                expression,
            )?;
            let condition = self
                .check_expression(context, expression, bindings, 0)
                .map_err(Checker::clause_conditional_repair)?;
            self.types.validate_clause_checked_forms(
                ClauseKind::Requires,
                clause,
                &condition.expression,
            )?;
            self.types.validate_clause_alias_offsets(
                ClauseKind::Requires,
                clause,
                &condition.expression,
                &definition_offsets,
            )?;
            if condition.mode != CheckedMode::Own || condition.expression.ty() != CheckedType::Bool
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Op5,
                    expression,
                    SemanticIssueKind::InvalidPredicateCondition,
                );
            }
            let root = self
                .types
                .build_clause_expression(
                    context,
                    expression,
                    &condition.expression,
                    bindings,
                    &expanded_bindings,
                )?
                .into_goal_expression()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            requirements.push(CheckedRequirement {
                template: GoalTemplate::new(root),
                clause: self.types.declarations.tree.path(clause)?.clone(),
                subject: None,
            });
            let mut reached = HashSet::new();
            let mut pending = Vec::new();
            collect_clause_reads(&condition.expression, &mut pending);
            while let Some(read) = pending.pop() {
                if let Some(definition) = definitions
                    .iter()
                    .find(|definition| definition.binding == read)
                    && reached.insert(read)
                {
                    pending.extend(definition.uses.iter().copied());
                }
            }
            let mut places = Vec::new();
            for definition in &definitions {
                if reached.contains(&definition.binding)
                    && expanded_definitions.insert(definition.binding)
                {
                    places.extend(definition.places.iter().cloned());
                }
            }
            let own = places.len();
            collect_clause_places(&condition.expression, &mut places);
            for place in &mut places[own..] {
                expand_definition_offsets(place, &definition_offsets, self.types)?;
            }
            requirement_places.push(places);
        }
        let definitions = definitions
            .into_iter()
            .map(|definition| {
                let expression = expanded_bindings
                    .remove(&definition.binding)
                    .and_then(ExpandedClauseExpression::into_goal_expression)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                Ok((definition.binding, expression))
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        Ok(CheckedRequires {
            definitions,
            requirements,
            range,
            places: requirement_places,
        })
    }

    /// The contract-conditional OWN-1 bare-affine repair [#35]. OWN-1's
    /// ordinary mechanical fix is `write move p`, but [FN-8] rejects `move`
    /// inside a contract block, so that instruction would send the writer
    /// from one hard error to another. A definition or clause instead carries
    /// the contract-specific repair.
    pub(super) fn clause_conditional_repair(stop: CheckStop) -> CheckStop {
        let CheckStop::Issue(mut issue) = stop else {
            return stop;
        };
        if matches!(issue.kind, SemanticIssueKind::BareAffineUse { .. })
            && matches!(issue.rule, SemanticRule::Own1)
        {
            issue.kind = SemanticIssueKind::BareAffineUse {
                mechanical_fix: "restate the definition or clause over copy operands or non-consuming admitted reads",
            };
        }
        CheckStop::Issue(issue)
    }

    /// The exact rows a contract clause reads mathematically rather than as a
    /// runtime evaluation, matching the carve-out [INV-1] already gives an
    /// `affine_expr`. Addition, subtraction, and multiplication have a total
    /// meaning over the mathematical integers, so a clause written over them
    /// states a relation rather than requesting an operation, and [FN-9] erases
    /// every clause before lowering. Division, remainder, the shifts, negation,
    /// and absolute value stay inadmissible: each has an input a relation cannot
    /// state its way out of, so admitting them would put a partial operation in a
    /// position with no domain obligation to discharge it.
    const fn clause_affine_operation(operation: CheckedIntegerOperation) -> bool {
        matches!(
            operation,
            CheckedIntegerOperation::AddExact
                | CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::MultiplyExact
        )
    }
}

impl<'unit> DeclarationInventory<'unit> {
    pub(super) fn clause_operand_atoms(
        &self,
        expression: NodeId,
    ) -> Result<Vec<NodeId>, CheckStop> {
        // A `clause_expr` and its affine sides are walked by their own
        // functions [MSR-5]; what reaches here is one written operand or one
        // `contract_define` `expr`.
        match self.tree.production(expression)? {
            Production::Atom => return Ok(vec![expression]),
            Production::Call => {
                let Some(list) = self
                    .tree
                    .first_child_with(expression, Production::AtomList)?
                else {
                    return Ok(Vec::new());
                };
                return self
                    .tree
                    .children_with(list, Production::Atom)
                    .map_err(Into::into);
            }
            _ => {}
        }
        if let Some(tail) = self
            .tree
            .first_child_with(expression, Production::InfixTail)?
        {
            let left = self
                .tree
                .first_child_with(expression, Production::Atom)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let right = self
                .tree
                .first_child_with(tail, Production::Atom)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            return Ok(vec![left, right]);
        }
        if let Some(call) = self.tree.first_child_with(expression, Production::Call)? {
            let Some(list) = self.tree.first_child_with(call, Production::AtomList)? else {
                return Ok(Vec::new());
            };
            return self
                .tree
                .children_with(list, Production::Atom)
                .map_err(Into::into);
        }
        self.tree
            .first_child_with(expression, Production::Atom)?
            .map_or_else(|| Ok(Vec::new()), |atom| Ok(vec![atom]))
    }
    /// The const generic one clause `atom` names directly [MSR-6], if any.
    ///
    /// A const generic is one `pbase` with no `^` suffix and no
    /// suffix; every other place shape resolves through the ordinary walk.
    fn clause_const_generic_base(
        &self,
        check_context: &CheckContext<'_>,
        atom: NodeId,
    ) -> Result<Option<DeclarationId>, CheckStop> {
        let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
            return Ok(None);
        };
        if !self
            .tree
            .children_with(place, Production::Psuffix)?
            .is_empty()
        {
            return Ok(None);
        }
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Ok(None);
        };
        let usage = self.use_at(check_context, pbase, LexicalUseRole::PlaceBase)?;
        Ok(match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::ConstGeneric,
            } => Some(declaration),
            _ => None,
        })
    }
    /// [FN-8, MSR-5] one `clause_expr`: one `affine_expr`, or two around one
    /// `clause_op`.
    ///
    /// The operator is one of the Bool-valued rows and is judged like every
    /// other selected row; the `+`, `-`, and `*` inside a side are the
    /// mathematical integer expression [MSR-5] fixes and select no row, so
    /// the exactness test does not reach them. Every factor of every side is
    /// validated, which is what makes the admission a property of the clause
    /// rather than of its leftmost operand.
    pub(super) fn validate_clause_condition(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: NodeId,
    ) -> Result<(), CheckStop> {
        // [RANGE-1] the grammar retains this shape to diagnose its misplaced use.
        if self.tree.has_fixed(expression, FixedTerminal::Unchanged)? {
            return self.issue_node(
                SemanticRule::Range1,
                expression,
                SemanticIssueKind::InvalidRangeClause {
                    reason: "unchanged occurs in an ordinary relation",
                    mechanical_fix: "state unchanged in an ensures forall conclusion",
                },
            );
        }
        match self.tree.children(expression)? {
            [side] => {
                let side = *side;
                self.validate_clause_affine(clause, entry, side)
            }
            [left, operator, right] => {
                let (left, operator, right) = (*left, *operator, *right);
                if self
                    .infix_operation(self.clause_operator_node(operator)?)?
                    .is_exact()
                {
                    return self.invalid_clause(clause, entry);
                }
                self.validate_clause_affine(clause, entry, left)?;
                self.validate_clause_affine(clause, entry, right)
            }
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }
    /// One `affine_expr`, `affine_term`, or `affine_factor` of a clause, down
    /// to the written operands [FN-8] judges.
    fn validate_clause_affine(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        match self.tree.production(node)? {
            Production::AffineExpr => {
                for term in self.tree.children_with(node, Production::AffineTerm)? {
                    self.validate_clause_affine(clause, entry, term)?;
                }
                Ok(())
            }
            Production::AffineTerm => {
                for factor in self.tree.children_with(node, Production::AffineFactor)? {
                    self.validate_clause_affine(clause, entry, factor)?;
                }
                Ok(())
            }
            Production::AffineFactor => {
                let child = self.tree.only_child(node)?;
                self.validate_clause_affine(clause, entry, child)
            }
            Production::Atom => self.validate_clause_atom(clause, entry, node),
            Production::Call => self.validate_clause_operation(clause, entry, node),
            // A `construct` derives under the production and is no datum and
            // no operation-table form, so [FN-8] refuses it here.
            _ => self.invalid_clause(clause, entry),
        }
    }
    /// Validates a clause computation, reporting whether the expression was
    /// one of the two spellings [FN-8] admits for it.
    ///
    /// [FN-8] requires "an ANF [GRAM-9] call to, or infix spelling of, a
    /// non-trapping, total operation-table row with effect `pure`", and
    /// [GRAM-5] gives those two spellings distinct `expr` shapes. `Ok(false)`
    /// means the expression is neither, leaving each caller to say whether
    /// its position admits a bare atom.
    pub(super) fn validate_clause_computation(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: NodeId,
    ) -> Result<bool, CheckStop> {
        if let Some(tail) = self
            .tree
            .first_child_with(expression, Production::InfixTail)?
        {
            self.validate_clause_infix(clause, entry, expression, tail)?;
            return Ok(true);
        }
        if let Some(call) = self.tree.first_child_with(expression, Production::Call)? {
            if self.tree.is_constructor_call(call)? {
                return Ok(false);
            }
            self.validate_clause_operation(clause, entry, call)?;
            return Ok(true);
        }
        Ok(false)
    }
    /// Validates the infix spelling of a row against the same [FN-8] subset
    /// the named spelling faces.
    ///
    /// The operator token selects the row under [OP-1] (ii), so admission
    /// asks whether the selected row is proof-required exact rather than
    /// re-reading its spelling. Both operands are
    /// clause atoms and both are validated; [GRAM-9] admits exactly one
    /// operation per expression, so there is no deeper operand to reach.
    fn validate_clause_infix(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: NodeId,
        tail: NodeId,
    ) -> Result<(), CheckStop> {
        let operator = self.infix_operator_node(tail)?;
        let operation = self.infix_operation(operator)?;
        if operation.is_exact() && !Checker::clause_affine_operation(operation) {
            return self.invalid_clause(clause, entry);
        }
        let left = self
            .tree
            .first_child_with(expression, Production::Atom)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.validate_clause_atom(clause, entry, left)?;
        let right = self
            .tree
            .first_child_with(tail, Production::Atom)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.validate_clause_atom(clause, entry, right)
    }
    fn validate_clause_operation(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        call: NodeId,
    ) -> Result<(), CheckStop> {
        let callee = self
            .tree
            .first_child_with(call, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let is_callee = |usage: &&crate::LexicalUseRecord| {
            matches!(
                usage.role(),
                LexicalUseRole::IdentifierCallee | LexicalUseRole::OperationCallee
            )
        };
        let usage = match clause {
            ClauseKind::Requires => self.resolved.lexical_uses_at(callee).find(is_callee),
            ClauseKind::Postcondition(record) => {
                let callee_path = self.tree.path(callee)?;
                record
                    .provisional_uses
                    .iter()
                    .filter(|usage| usage.origin().node() == callee_path)
                    .chain(self.resolved.lexical_uses_at(callee))
                    .find(is_callee)
            }
        };
        let Some(usage) = usage else {
            return self.invalid_clause(clause, entry);
        };
        let ResolvedTarget::Operation(operation) = usage.target() else {
            // FN-8 admits only table-operation calls. Ordinary function
            // callees have already resolved successfully, so they are an
            // InvalidRequires source form rather than a compiler-resolution
            // failure.
            return self.invalid_clause(clause, entry);
        };
        let spelling = crate::operation_family_spelling(operation)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if matches!(
            spelling,
            "ineg" | "iabs" | "ishl" | "ishr" | "buffer_new" | "box_new"
        ) {
            return self.invalid_clause(clause, entry);
        }
        if let Some(arguments) = self.tree.first_child_with(call, Production::AtomList)? {
            for atom in self.tree.children_with(arguments, Production::Atom)? {
                self.validate_clause_atom(clause, entry, atom)?;
            }
        }
        Ok(())
    }
    /// Validates a definition initializer that selected no operation row.
    ///
    /// [FN-8] admits a definition and clause expression built from
    /// "non-consuming datums, measure place forms [OP-15], and
    /// operation-table forms", so an initializer with no operation at all is
    /// admitted exactly when it is one bare datum: `define spare = table.len`
    /// reads a measure member and selects nothing. A construction reaching
    /// here stays refused, because [FN-8] names construction inadmissible and
    /// its operands are not the expression.
    pub(super) fn validate_clause_definition_datum(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: NodeId,
    ) -> Result<(), CheckStop> {
        if self
            .tree
            .first_child_with(expression, Production::Call)?
            .is_some()
        {
            return self.invalid_clause(clause, entry);
        }
        let Some(atom) = self.tree.first_child_with(expression, Production::Atom)? else {
            return self.invalid_clause(clause, entry);
        };
        self.validate_clause_atom(clause, entry, atom)
    }
    fn validate_clause_atom(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        atom: NodeId,
    ) -> Result<(), CheckStop> {
        if self.tree.has_fixed(atom, FixedTerminal::Move)?
            || self
                .tree
                .first_child_with(atom, Production::BorrowExpr)?
                .is_some()
        {
            return self.invalid_clause(clause, entry);
        }
        // Typed admission decides whether a selected place is an ENT-2
        // term. A terminal subscript is an ordinary place suffix.
        if let Some(place) = self.tree.first_child_with(atom, Production::Place)? {
            // [ENT-2(b), FN-8] an element is an ordinary clause datum, but
            // an offset selecting it must still be a clause (a) place or a
            // clause (c) constant. Diagnose a written nested element before
            // the template builder attempts to represent its selector.
            for suffix in self.tree.children_with(place, Production::Psuffix)? {
                let Some(offset) = self.tree.subscript_offset(suffix)? else {
                    continue;
                };
                let Some(offset_place) = self.tree.first_child_with(offset, Production::Place)?
                else {
                    continue;
                };
                for step in self.tree.children_with(offset_place, Production::Psuffix)? {
                    if self.tree.subscript_offset(step)?.is_some() {
                        return self.invalid_clause(clause, entry);
                    }
                }
            }
            return Ok(());
        }
        if self
            .tree
            .direct_token_with(atom, crate::TerminalPredicate::Literal)?
            .is_some()
        {
            return Ok(());
        }
        Err(SemanticCompilerFailure::InvalidCanonicalTree.into())
    }
    pub(super) fn invalid_clause<T>(
        &self,
        clause: ClauseKind<'_>,
        node: NodeId,
    ) -> Result<T, CheckStop> {
        match clause {
            ClauseKind::Requires => {
                self.issue_node(SemanticRule::Fn8, node, SemanticIssueKind::InvalidRequires)
            }
            ClauseKind::Postcondition(_) => self.issue_node(
                SemanticRule::Fn9,
                node,
                SemanticIssueKind::InvalidPostconditionClause,
            ),
        }
    }
}

impl<'unit> TypeContext<'unit> {
    pub(super) fn validate_clause_alias_offsets(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: &CheckedExpression,
        offsets: &DefinitionOffsets,
    ) -> Result<(), CheckStop> {
        let mut places = Vec::new();
        collect_clause_places(expression, &mut places);
        for mut place in places {
            expand_definition_offsets(&mut place, offsets, self)?;
            self.validate_clause_checked_forms(clause, entry, &place)?;
        }
        Ok(())
    }

    /// The [MSR-1] row one written measure selects over a place of this type
    /// [OP-15].
    ///
    /// A range referent carries its own row: [MSR-1] gives `&[T]` a row of
    /// its own, and the referent type a `^` on one selects is the
    /// element type, so the row cannot be recovered from that type.
    fn clause_measure_row(
        &mut self,
        measure: CheckedMeasure,
        ty: CheckedType,
        range_referent: bool,
    ) -> Result<GoalOperation, CheckStop> {
        let measured = if range_referent {
            super::super::model::MeasuredKind::Range
        } else {
            super::expressions::flat_storage::measured_kind_of(ty)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
        };
        let (element, constant) = match ty {
            _ if range_referent => (Some(self.intern_element(ty)?), None),
            CheckedType::Window {
                element, capacity, ..
            } => (Some(element), capacity),
            CheckedType::Array { element, length } => (Some(element), Some(length)),
            CheckedType::Buffer { element }
            | CheckedType::Segments { element }
            | CheckedType::Entries { element } => (Some(element), None),
            // [SHARE-1] a key set's keys are no element type of the program.
            CheckedType::KeySet => (None, None),
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        Ok(GoalOperation::ContainerMeasure {
            measure,
            measured,
            element,
            constant,
        })
    }
    /// Alpha-expands one already-checked admitted FN-8/FN-9 expression. Source
    /// atoms supply declaration/projection identity; the checked expression
    /// supplies the uniquely selected row and types.
    pub(super) fn build_clause_expression(
        &mut self,
        context: FunctionContext<'_, '_>,
        source: NodeId,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        // [GRAM-5, MSR-5] a clause and its affine sides have their own
        // shapes, and each is walked against the checked expression the
        // typer built for exactly that node.
        match self.declarations.tree.production(source)? {
            Production::ClauseExpr => {
                return self.build_clause_root(
                    context,
                    source,
                    checked,
                    bindings,
                    expanded_bindings,
                );
            }
            Production::AffineExpr | Production::AffineTerm | Production::AffineFactor => {
                return self.build_clause_affine(
                    context,
                    source,
                    None,
                    checked,
                    bindings,
                    expanded_bindings,
                );
            }
            _ => {}
        }
        // [FN-8] a definition's initializer is a clause expression, and such
        // an expression may be one bare non-consuming datum with no
        // operation at all, including a measure place form [OP-15], which the
        // atom walk already expands whole. Reaching the operation walk below
        // with such an expression would wrap the measure the atom produced in
        // a second measure row.
        if self
            .declarations
            .tree
            .first_child_with(source, Production::Call)?
            .is_none()
            && self
                .declarations
                .tree
                .first_child_with(source, Production::InfixTail)?
                .is_none()
            && let Some(atom) = self
                .declarations
                .tree
                .first_child_with(source, Production::Atom)?
        {
            return self.build_clause_atom(
                context,
                atom,
                Some(checked),
                bindings,
                expanded_bindings,
            );
        }
        let atoms = self.declarations.clause_operand_atoms(source)?;
        let operation = match checked {
            CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                arguments,
                result,
                ..
            } => Some((
                GoalOperation::Integer {
                    operation: *operation,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                *result,
                arguments.as_slice(),
            )),
            CheckedExpression::FloatOperation {
                operation,
                operand_type,
                arguments,
                ..
            } => Some((
                GoalOperation::Float {
                    operation: *operation,
                    operand_type: *operand_type,
                },
                if matches!(
                    operation,
                    CheckedFloatOperation::Infinity | CheckedFloatOperation::Nan
                ) {
                    vec![*operand_type]
                } else {
                    Vec::new()
                },
                Vec::new(),
                operation.result_type(*operand_type),
                arguments.as_slice(),
            )),
            CheckedExpression::NumericConversion {
                mode,
                source,
                destination,
                value,
                result,
                ..
            } => Some((
                GoalOperation::NumericConversion {
                    mode: *mode,
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                *result,
                std::slice::from_ref(value.as_ref()),
            )),
            CheckedExpression::Reinterpret {
                source,
                destination,
                value,
                ..
            } => Some((
                GoalOperation::Reinterpret {
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                destination.ty(),
                std::slice::from_ref(value.as_ref()),
            )),
            CheckedExpression::BooleanOperation {
                operation,
                arguments,
                ..
            } => Some((
                GoalOperation::Boolean(*operation),
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments.as_slice(),
            )),
            CheckedExpression::ValueEquality {
                equal,
                operand_type,
                arguments,
                ..
            } => Some((
                GoalOperation::ValueEquality {
                    equal: *equal,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments.as_slice(),
            )),
            _ => None,
        };
        if let Some((row, type_arguments, const_arguments, result, checked_arguments)) = operation {
            if atoms.len() != checked_arguments.len() {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            let arguments = atoms
                .into_iter()
                .zip(checked_arguments)
                .map(|(atom, argument)| {
                    self.build_clause_operand(
                        context,
                        atom,
                        Some(argument),
                        bindings,
                        expanded_bindings,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(ExpandedClauseExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            });
        }

        if matches!(
            checked,
            CheckedExpression::ArrayMeasure { .. }
                | CheckedExpression::BufferMeasure { .. }
                | CheckedExpression::ContainerMeasure { .. }
        ) {
            if atoms.len() != 1 {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            let argument =
                self.build_clause_atom(context, atoms[0], None, bindings, expanded_bindings)?;
            let row = match (checked, argument.ty()) {
                (
                    CheckedExpression::ArrayMeasure {
                        measure, length, ..
                    },
                    CheckedType::Array {
                        element,
                        length: argument_length,
                    },
                ) if argument_length == *length => GoalOperation::ArrayMeasure {
                    measure: *measure,
                    element,
                    length: *length,
                },
                (
                    CheckedExpression::BufferMeasure { measure, root },
                    CheckedType::Buffer { element },
                ) if element == root.element => GoalOperation::BufferMeasure {
                    measure: *measure,
                    element,
                },
                // [MSR-1] a storage shape's measure. The measured
                // kind and the written constant are the row's identity, and
                // the operand's own type is what fixes both.
                (CheckedExpression::ContainerMeasure { measure, root }, argument_type)
                    if argument_type == root.ty =>
                {
                    let measured = root
                        .measured()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured,
                        element: root.element(),
                        constant: root.type_constant(),
                    }
                }
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            };
            return Ok(ExpandedClauseExpression::Operation {
                row,
                type_arguments: Vec::new(),
                const_arguments: Vec::new(),
                result: CheckedType::Integer(super::super::model::IntegerType::U64),
                arguments: vec![argument],
            });
        }

        if atoms.len() != 1 {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        self.build_clause_operand(
            context,
            atoms[0],
            Some(checked),
            bindings,
            expanded_bindings,
        )
    }
    /// [GRAM-5] one `clause_expr` root: one `affine_expr`, or two around one
    /// `clause_op` whose row the typer has already selected.
    fn build_clause_root(
        &mut self,
        context: FunctionContext<'_, '_>,
        source: NodeId,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        match self.declarations.tree.children(source)? {
            [side] => {
                let side = *side;
                self.build_clause_affine(context, side, None, checked, bindings, expanded_bindings)
            }
            [left, _operator, right] => {
                let (left, right) = (*left, *right);
                let (row, arguments, result) = match checked {
                    CheckedExpression::IntegerOperation {
                        operation,
                        operand_type,
                        arguments,
                        result,
                        ..
                    } => (
                        GoalOperation::Integer {
                            operation: *operation,
                            operand_type: *operand_type,
                        },
                        arguments,
                        *result,
                    ),
                    CheckedExpression::ValueEquality {
                        equal,
                        operand_type,
                        arguments,
                        ..
                    } => (
                        GoalOperation::ValueEquality {
                            equal: *equal,
                            operand_type: *operand_type,
                        },
                        arguments,
                        CheckedType::Bool,
                    ),
                    _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                };
                let [checked_left, checked_right] = arguments.as_slice() else {
                    return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
                };
                Ok(ExpandedClauseExpression::Operation {
                    row,
                    type_arguments: Vec::new(),
                    const_arguments: Vec::new(),
                    result,
                    arguments: vec![
                        self.build_clause_affine(
                            context,
                            left,
                            None,
                            checked_left,
                            bindings,
                            expanded_bindings,
                        )?,
                        self.build_clause_affine(
                            context,
                            right,
                            None,
                            checked_right,
                            bindings,
                            expanded_bindings,
                        )?,
                    ],
                })
            }
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }
    /// One `affine_expr`, `affine_term`, or `affine_factor` of a clause side,
    /// walked against the checked expression the typer built for that exact
    /// node [MSR-5].
    ///
    /// `terms` bounds an `affine_expr`'s left-associative fold to its first
    /// `terms` `affine_term` children, which is the same bound the typing
    /// walk uses, so the two walks stay in step over `a + b - c`.
    fn build_clause_affine(
        &mut self,
        context: FunctionContext<'_, '_>,
        source: NodeId,
        terms: Option<usize>,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        match self.declarations.tree.production(source)? {
            Production::AffineExpr => {
                let children = self.declarations.tree.children(source)?.to_vec();
                let count = terms.unwrap_or_else(|| children.len().div_ceil(2));
                let last = count
                    .checked_mul(2)
                    .and_then(|doubled| doubled.checked_sub(2))
                    .and_then(|index| children.get(index).copied())
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if count == 1 {
                    return self.build_clause_affine(
                        context,
                        last,
                        None,
                        checked,
                        bindings,
                        expanded_bindings,
                    );
                }
                self.build_clause_affine_operation(
                    context,
                    (source, Some(count - 1)),
                    last,
                    checked,
                    bindings,
                    expanded_bindings,
                )
            }
            Production::AffineTerm => {
                let factors = self
                    .declarations
                    .tree
                    .children_with(source, Production::AffineFactor)?;
                match factors.as_slice() {
                    [factor] => self.build_clause_affine(
                        context,
                        *factor,
                        None,
                        checked,
                        bindings,
                        expanded_bindings,
                    ),
                    [left, right] => self.build_clause_affine_operation(
                        context,
                        (*left, None),
                        *right,
                        checked,
                        bindings,
                        expanded_bindings,
                    ),
                    _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                }
            }
            Production::AffineFactor => {
                let child = self.declarations.tree.only_child(source)?;
                self.build_clause_affine(context, child, None, checked, bindings, expanded_bindings)
            }
            _ => self.build_clause_operand(
                context,
                source,
                Some(checked),
                bindings,
                expanded_bindings,
            ),
        }
    }
    /// One binary node of a clause side's affine fold [MSR-5].
    fn build_clause_affine_operation(
        &mut self,
        context: FunctionContext<'_, '_>,
        (left, left_terms): (NodeId, Option<usize>),
        right: NodeId,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let CheckedExpression::IntegerOperation {
            operation,
            operand_type,
            arguments,
            result,
            ..
        } = checked
        else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let [checked_left, checked_right] = arguments.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        Ok(ExpandedClauseExpression::Operation {
            row: GoalOperation::Integer {
                operation: *operation,
                operand_type: *operand_type,
            },
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: *result,
            arguments: vec![
                self.build_clause_affine(
                    context,
                    left,
                    left_terms,
                    checked_left,
                    bindings,
                    expanded_bindings,
                )?,
                self.build_clause_affine(
                    context,
                    right,
                    None,
                    checked_right,
                    bindings,
                    expanded_bindings,
                )?,
            ],
        })
    }
    /// One written clause operand. An `atom` is a leaf datum; every other
    /// written form — today exactly a `call`, which is how [MSR-5] admits a
    /// measure term as an operand — is expanded by the ordinary clause walk
    /// against the row the typer already selected for it.
    fn build_clause_operand(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        checked: Option<&CheckedExpression>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        if self.declarations.tree.production(node)? == Production::Atom {
            return self.build_clause_atom(context, node, checked, bindings, expanded_bindings);
        }
        let checked = checked.ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.build_clause_expression(context, node, checked, bindings, expanded_bindings)
    }
    fn build_clause_atom(
        &mut self,
        context: FunctionContext<'_, '_>,
        atom: NodeId,
        checked: Option<&CheckedExpression>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let check_context = context.check_context;
        if self
            .declarations
            .postcondition_selector_use_inside(check_context, atom)?
        {
            let ty = check_context
                .active_postcondition
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .result_type;
            if let Some(expanded) =
                self.build_clause_result_place(context, atom, bindings, expanded_bindings)?
            {
                return Ok(expanded);
            }
            return Ok(ExpandedClauseExpression::InvalidSelectorUse {
                ty: checked.map_or(ty, CheckedExpression::ty),
            });
        }
        if let Some(literal) = self
            .declarations
            .tree
            .direct_token_with(atom, crate::TerminalPredicate::Literal)?
        {
            let Some(CheckedExpression::Constant(value)) = checked else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            let bytes = self.declarations.tree.token_bytes(literal)?;
            let origin = if matches!(bytes, b"0_T" | b"1_T") {
                let usage = self.declarations.use_at(
                    check_context,
                    atom,
                    LexicalUseRole::GenericNumericSuffix,
                )?;
                let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::GenericType,
                } = usage.target()
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                PostconditionConstantOrigin::GenericNumericIdentity {
                    type_parameter: declaration,
                    one: bytes == b"1_T",
                }
            } else {
                PostconditionConstantOrigin::Literal
            };
            return Ok(ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Literal {
                    value: value.clone(),
                    origin,
                },
            ));
        }
        // [MSR-6] a clause operand is an in-scope const generic wherever it
        // is a named const. It is a constant and not a measure former, so it
        // contributes no place support; the ordinary place-use judgment has
        // already folded its value for this concrete instance, and the clause
        // reads that value rather than re-resolving the parameter.
        if let Some(declaration) = self
            .declarations
            .clause_const_generic_base(check_context, atom)?
        {
            let Some(CheckedExpression::Constant(value)) = checked else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            return Ok(ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Literal {
                    value: value.clone(),
                    origin: PostconditionConstantOrigin::ConstGeneric { declaration },
                },
            ));
        }
        let place = self
            .declarations
            .tree
            .first_child_with(atom, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let result = self.build_clause_place(context, place, bindings, expanded_bindings)?;
        if checked.is_some_and(|expression| expression.ty() != result.ty()) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(result)
    }
    /// One clause operand written as a place over the clause's own result
    /// datum [FN-9, CALL-4], if the atom is one.
    ///
    /// Three shapes are admitted and nothing else. A bare selector spelling
    /// is the result datum itself. A place whose trailing member is one of
    /// [MSR-1]'s measures is that measure over the result place the written
    /// member path reaches, because [OP-15] reads a measure as a member of
    /// the measured place and gives it no storage below itself; that is how
    /// `ensures result.len == n` and `ensures result.inner.len == count` are
    /// one relation term and not a second fact class. A place whose member
    /// path of struct-field and `Box` content steps ends at a fragment
    /// integer is that place's value [CALL-4]: `ensures atom.index < n`.
    /// Every other member path over a result datum stays outside the
    /// admitted operand set, and the caller reports it as the ordinary
    /// invalid selector use.
    fn build_clause_result_place(
        &mut self,
        context: FunctionContext<'_, '_>,
        atom: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<Option<ExpandedClauseExpression>, CheckStop> {
        let check_context = context.check_context;
        let Some(place) = self
            .declarations
            .tree
            .first_child_with(atom, Production::Place)?
        else {
            return Ok(None);
        };
        let Some((ordinal, datum_type)) = self
            .declarations
            .postcondition_selector_place_base(check_context, place)?
        else {
            return Ok(None);
        };
        let suffixes = self
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?;
        if suffixes.is_empty() {
            return Ok(Some(ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Result {
                    ordinal,
                    projections: Vec::new(),
                    ty: datum_type,
                },
            )));
        }
        let Some(measure) = self.clause_trailing_measure(
            context,
            &suffixes,
            datum_type,
            false,
            bindings,
            expanded_bindings,
        )?
        else {
            // [CALL-4] a fragment-integer place reached through struct-field
            // and `Box` content steps is a datum as its own value.
            let (projections, reached, _) = self.clause_member_projections(
                context,
                &suffixes,
                datum_type,
                false,
                bindings,
                expanded_bindings,
            )?;
            if !matches!(
                reached,
                CheckedType::Integer(_) | CheckedType::GenericInt(_)
            ) || projections.iter().any(|projection| {
                !matches!(projection, GoalProjection::Field(_) | GoalProjection::Deref)
            }) {
                return Ok(None);
            }
            return Ok(Some(ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Result {
                    ordinal,
                    projections,
                    ty: reached,
                },
            )));
        };
        let (projections, measured_type, measured_range) = self.clause_member_projections(
            context,
            &suffixes[..suffixes.len()
                - if measure == CheckedMeasure::Pages {
                    2
                } else {
                    1
                }],
            datum_type,
            false,
            bindings,
            expanded_bindings,
        )?;
        let row = self.clause_measure_row(measure, measured_type, measured_range)?;
        Ok(Some(ExpandedClauseExpression::Operation {
            row,
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Integer(super::super::model::IntegerType::U64),
            arguments: vec![ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Result {
                    ordinal,
                    projections,
                    ty: measured_type,
                },
            )],
        }))
    }
    /// [TYPE-10] the measure a clause place's last `psuffix` selects. Its
    /// spelling only proposes one: the suffix is a measure exactly when the
    /// place before it has a row for it, as a measured type, a range
    /// reference's run, or, in a prelude row's own clause, a `Box` whose
    /// content is measured [OP-14]. Everywhere else it is the ordinary field
    /// of that name, as it is in an expression.
    fn clause_trailing_measure(
        &mut self,
        context: FunctionContext<'_, '_>,
        suffixes: &[NodeId],
        base: CheckedType,
        range_referent: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<Option<CheckedMeasure>, CheckStop> {
        let Some(measure) = self.declarations.trailing_measure_member(suffixes)? else {
            return Ok(None);
        };
        let (&last, prefix) = suffixes
            .split_last()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if measure == CheckedMeasure::Length
            && !prefix.is_empty()
            && self
                .declarations
                .tree
                .source_spelling(*prefix.last().unwrap())?
                == ".pages"
        {
            let (_, reached, reached_range) = self.clause_member_projections(
                context,
                &prefix[..prefix.len() - 1],
                base,
                range_referent,
                bindings,
                expanded_bindings,
            )?;
            if !reached_range
                && matches!(
                    reached,
                    CheckedType::Window {
                        shape: super::super::model::WindowShape::Paged,
                        ..
                    }
                )
            {
                return Ok(Some(CheckedMeasure::Pages));
            }
        }
        let (ty, range_referent) = if prefix.is_empty() {
            (base, range_referent)
        } else {
            let (_, reached, reached_range) = self.clause_member_projections(
                context,
                prefix,
                base,
                range_referent,
                bindings,
                expanded_bindings,
            )?;
            (reached, reached_range)
        };
        let measured = range_referent
            || super::expressions::flat_storage::measured_kind_of(ty).is_some()
            || (self.declarations.tree.is_prelude_node(last)?
                && self.box_content(ty)?.is_some_and(|referent| {
                    super::expressions::flat_storage::measured_kind_of(referent).is_some()
                }));
        Ok(measured.then_some(measure))
    }
    /// The projection path one written `psuffix` run selects below a clause
    /// datum of this type.
    ///
    /// [TYPE-9] gives a `Box` exactly one member, `inner`, and that member
    /// is the box content itself, represented by a `GoalProjection::Deref`
    /// step in the goal place. Every other member is the
    /// ordinary struct field step and is judged by the ordinary walk.
    ///
    /// `range_referent` says the datum is the run a range reference names,
    /// whose type is its element type [TYPE-8]: its first subscript selects
    /// an element of that type rather than indexing a value of it.
    fn clause_member_projections(
        &mut self,
        context: FunctionContext<'_, '_>,
        suffixes: &[NodeId],
        mut ty: CheckedType,
        mut range_referent: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(Vec<GoalProjection>, CheckedType, bool), CheckStop> {
        let check_context = context.check_context;
        let mut projections = Vec::with_capacity(suffixes.len());
        let mut pages = false;
        for suffix in suffixes {
            let range_step = std::mem::replace(&mut range_referent, false);
            // [ENT-2] clause (b) forms a place with field selections,
            // `^` suffixes and subscripts, which is what makes
            // `table[i].len` and `nodes[i].count` terms. A clause reads that place exactly as the
            // body does, so a subscript written here is one projection and not
            // a composite value this version cannot represent.
            if self.declarations.tree.subscript_offset(*suffix)?.is_some() {
                let (projection, element) = self.clause_subscript_projection(
                    context,
                    *suffix,
                    ty,
                    range_step,
                    bindings,
                    expanded_bindings,
                )?;
                range_referent =
                    !range_step && (pages || matches!(ty, CheckedType::Segments { .. }));
                projections.push(if pages {
                    match projection {
                        GoalProjection::Subscript(offset) => GoalProjection::Page(offset),
                        GoalProjection::FormalSubscript {
                            ordinal,
                            projections,
                        } => GoalProjection::FormalPage {
                            ordinal,
                            projections,
                        },
                        _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                    }
                } else {
                    projection
                });
                pages = false;
                ty = element;
                continue;
            }
            if !range_step
                && matches!(
                    ty,
                    CheckedType::Window {
                        shape: super::super::model::WindowShape::Paged,
                        ..
                    }
                )
                && self.declarations.tree.source_spelling(*suffix)? == ".pages"
            {
                pages = true;
                continue;
            }
            if let CheckedType::Nominal(nominal) = ty
                && let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind
            {
                let name = self
                    .declarations
                    .deferred_use_at(*suffix, crate::DeferredUseRole::ProjectedField)?
                    .spelling()
                    .to_owned();
                if name != "inner" {
                    return self.declarations.issue_node(
                        SemanticRule::Type9,
                        *suffix,
                        SemanticIssueKind::type_mismatch(
                            "the Box content field `inner`",
                            format!("the field name `{name}`, which a Box does not declare"),
                        ),
                    );
                }
                projections.push(GoalProjection::Deref);
                ty = referent;
                continue;
            }
            let (fields, reached) =
                self.resolve_struct_path(check_context, std::slice::from_ref(suffix), ty)?;
            projections.extend(fields.into_iter().map(GoalProjection::Field));
            ty = reached;
        }
        Ok((projections, ty, range_referent))
    }
    /// One written subscript inside a clause (b) place [ENT-2].
    ///
    /// A source offset expands to an ENT-2(a) place or ENT-2(c) constant.
    /// A formal selector retains every field, dereference and admitted measure
    /// projection so body and caller substitution preserve its proof path and
    /// ordinary support. Earlier definitions are erased before this judgment.
    /// A written element read or computation cannot be a selector.
    fn clause_subscript_projection(
        &mut self,
        context: FunctionContext<'_, '_>,
        suffix: NodeId,
        base: CheckedType,
        range_referent: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(GoalProjection, CheckedType), CheckStop> {
        let check_context = context.check_context;
        let element = match base {
            // [REF-4] a range reference's referent is the run of its element
            // type, which is the type its checked datum carries.
            _ if range_referent => base,
            CheckedType::Buffer { element } => self.element_type(element)?,
            CheckedType::Array { element, .. }
            | CheckedType::Window { element, .. }
            | CheckedType::Segments { element } => self.element_type(element)?,
            _ => {
                return self
                    .declarations
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            }
        };
        let offset = self
            .declarations
            .tree
            .subscript_offset(suffix)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let capture = crate::semantic::places::CapturedValue::unknown().capture;
        let value =
            |term| crate::semantic::places::CapturedValue::new(capture, term).goal_identity();
        if let Some(literal) = self
            .declarations
            .tree
            .direct_token_with(offset, crate::TerminalPredicate::Literal)?
        {
            let CheckedValue::Integer { bits, .. } =
                self.declarations.parse_literal(offset, literal)?
            else {
                return self
                    .declarations
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            };
            return Ok((
                GoalProjection::Subscript(value(crate::semantic::places::CapturedTerm::Literal(
                    bits,
                ))),
                element,
            ));
        }
        if let Some(declaration) = self
            .declarations
            .clause_const_generic_base(check_context, offset)?
        {
            // [FN-2, MSR-6] the selector is this concrete instance's
            // constant, exactly as an ordinary checked offset is. A
            // forwarded symbolic argument keeps the supplying declaration.
            let term = match context.function.substitution.const_argument(declaration) {
                Some(CheckedConst::Value(value)) => CapturedTerm::Literal(value),
                Some(CheckedConst::Parameter(supplied)) => CapturedTerm::Const(supplied),
                _ => CapturedTerm::Const(declaration),
            };
            return Ok((GoalProjection::Subscript(value(term)), element));
        }
        let Some(place) = self
            .declarations
            .tree
            .first_child_with(offset, Production::Place)?
        else {
            return self
                .declarations
                .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
        };
        // Offset syntax is deliberately nonrecursive: a selected element
        // cannot select another ordinary element term.
        for step in self
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?
        {
            if self.declarations.tree.subscript_offset(step)?.is_some() {
                return self
                    .declarations
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            }
        }
        let selected = self.build_clause_place(context, place, bindings, expanded_bindings)?;
        let selected = match selected {
            ExpandedClauseExpression::Operation {
                row, mut arguments, ..
            } => {
                let measure = match row {
                    GoalOperation::ArrayMeasure { measure, .. }
                    | GoalOperation::BufferMeasure { measure, .. }
                    | GoalOperation::ContainerMeasure { measure, .. }
                        if measure != CheckedMeasure::Pages =>
                    {
                        measure
                    }
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                };
                if arguments.len() != 1 {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                arguments
                    .pop()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .with_projection(
                        GoalProjection::Measure(measure),
                        CheckedType::Integer(super::super::model::IntegerType::U64),
                    )
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
            }
            other => other,
        };
        match selected {
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Literal {
                value: CheckedValue::Integer { bits, .. },
                ..
            }) => Ok((
                GoalProjection::Subscript(value(CapturedTerm::Literal(bits))),
                element,
            )),
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Literal {
                value: CheckedValue::ConstGeneric { declaration, .. },
                ..
            }) => Ok((
                GoalProjection::Subscript(value(CapturedTerm::Const(declaration))),
                element,
            )),
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                ordinal,
                projections,
                ty: CheckedType::Integer(_),
                ..
            }) if projections.iter().all(|step| {
                matches!(
                    step,
                    GoalProjection::Field(_)
                        | GoalProjection::Deref
                        | GoalProjection::Payload { .. }
                        | GoalProjection::Measure(_)
                )
            }) =>
            {
                Ok((
                    GoalProjection::FormalSubscript {
                        ordinal,
                        projections,
                    },
                    element,
                ))
            }
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::NamedConst {
                declaration,
                projections,
                ..
            }) => {
                let constant = self
                    .constants
                    .get(&declaration)
                    .copied()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if matches!(projections.last(), Some(GoalProjection::Measure(_))) {
                    let place = crate::semantic::places::ResolvedPlace {
                        root: crate::semantic::places::PlaceRoot::Constant(constant),
                        path: projections.iter().map(GoalProjection::place_step).collect(),
                        atomic_aliases: Vec::new(),
                    };
                    return Ok((
                        GoalProjection::Subscript(value(CapturedTerm::Place(
                            crate::semantic::places::ProjectedSelector::new(place),
                        ))),
                        element,
                    ));
                }
                let mut selected = &self.constant(constant)?.value;
                for projection in &projections {
                    let (GoalProjection::Field(field), CheckedValue::Struct { fields, .. }) =
                        (projection, selected)
                    else {
                        return self.declarations.unsupported(
                            crate::UnsupportedSemanticFeature::CompositeValues,
                            suffix,
                        );
                    };
                    selected = fields
                        .get(*field as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                }
                let CheckedValue::Integer { bits, .. } = selected else {
                    return self
                        .declarations
                        .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
                };
                let term = if projections.is_empty() {
                    CapturedTerm::Literal(*bits)
                } else {
                    let place = crate::semantic::places::ResolvedPlace {
                        root: crate::semantic::places::PlaceRoot::Constant(constant),
                        path: projections.iter().map(GoalProjection::place_step).collect(),
                        atomic_aliases: Vec::new(),
                    };
                    CapturedTerm::Place(crate::semantic::places::ProjectedSelector::new(place))
                };
                Ok((GoalProjection::Subscript(value(term)), element))
            }
            _ => self
                .declarations
                .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix),
        }
    }

    fn build_clause_place(
        &mut self,
        context: FunctionContext<'_, '_>,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let (expression, holder_pending, _) =
            self.build_clause_place_inner(context, place, bindings, expanded_bindings)?;
        if holder_pending {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(expression)
    }
    /// Mirrors the already-completed TYPE-7 dereference type walk while
    /// retaining only predicate identity.
    ///
    /// [TYPE-7] `^` denotes the referent of a reference and nothing else:
    /// a `Box`'s content is its field `inner` and is reached by the ordinary
    /// field step [TYPE-9], so only a reference admits the caret step. The written step is retained as one projection of the
    /// declaration-boundary template because a caller substitutes the
    /// actual's own path for the formal and consumes exactly that leading
    /// projection [FN-8, CALL-6]; the callee body, where [REF-1] makes the
    /// parameter name the path itself, drops it instead.
    fn build_clause_place_inner(
        &mut self,
        context: FunctionContext<'_, '_>,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(ExpandedClauseExpression, bool, bool), CheckStop> {
        let check_context = context.check_context;
        let pbase = self
            .declarations
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let (mut expression, mut holder_pending, mut range_referent) = {
            let usage =
                self.declarations
                    .use_at(check_context, pbase, LexicalUseRole::PlaceBase)?;
            let ResolvedTarget::Source { declaration, class } = usage.target() else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            match class {
                DeclarationClass::Value => {
                    let local = bindings
                        .get(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    (
                        expanded_bindings
                            .get(&local.binding)
                            .cloned()
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                        local.mode != CheckedMode::Own,
                        local.mode.is_range(),
                    )
                }
                DeclarationClass::NamedConst => {
                    let constant = self
                        .constants
                        .get(&declaration)
                        .copied()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    (
                        ExpandedClauseExpression::Datum(ExpandedClauseDatum::NamedConst {
                            declaration,
                            projections: Vec::new(),
                            ty: self.constant(constant)?.ty,
                        }),
                        false,
                        false,
                    )
                }
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            }
        };
        if self
            .declarations
            .tree
            .has_fixed(pbase, FixedTerminal::Entry)?
        {
            let ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                denotation, ..
            }) = &mut expression
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            *denotation = ParameterDenotation::EntryDatum;
        }
        let suffixes = self
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?;
        let suffixes = if let Some(&first) = suffixes.first()
            && matches!(
                self.declarations.tree.place_suffix(first)?,
                crate::syntax::views::PlaceSuffix::Dereference
            ) {
            if !holder_pending {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            if range_referent {
                self.declarations.check_range_referent(first)?;
            }
            let ty = expression.ty();
            expression = expression
                .with_projection(GoalProjection::Deref, ty)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            holder_pending = false;
            &suffixes[1..]
        } else {
            suffixes.as_slice()
        };
        // [OP-15] a measure is read as a member of the measured place and
        // [MSR-1] gives it no storage below itself, so it is the last written
        // suffix and everything before it is the ordinary field path.
        let measure = self.clause_trailing_measure(
            context,
            suffixes,
            expression.ty(),
            range_referent,
            bindings,
            expanded_bindings,
        )?;
        let fields_only = if let Some(measure) = measure {
            &suffixes[..suffixes.len()
                - if measure == CheckedMeasure::Pages {
                    2
                } else {
                    1
                }]
        } else {
            suffixes
        };
        if holder_pending && !fields_only.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        if !fields_only.is_empty() {
            let (projections, final_ty, selected_range) = self.clause_member_projections(
                context,
                fields_only,
                expression.ty(),
                range_referent,
                bindings,
                expanded_bindings,
            )?;
            for projection in projections {
                expression = expression
                    .with_projection(projection, final_ty)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            }
            range_referent = selected_range;
        }
        // [OP-14, TYPE-9] `free_empty` writes one source contract for both
        // direct windows and Boxes holding runtime-capacity windows. At the
        // boxed instance its prelude-owned `window.len` measure place denotes
        // `window.inner.len`, matching the ordinary expression judgment's
        // prelude-only implicit content step. Retain that step in the
        // GoalTemplate so CALL-6 substitutes the caller's content measure.
        if measure.is_some()
            && let Some(suffix) = suffixes.last()
            && self.declarations.tree.is_prelude_node(*suffix)?
            && let Some(referent) = self.box_content(expression.ty())?
            && super::expressions::flat_storage::measured_kind_of(referent).is_some()
        {
            expression = expression
                .with_projection(GoalProjection::Deref, referent)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            range_referent = false;
        }
        if let Some(measure) = measure {
            let row = self.clause_measure_row(measure, expression.ty(), range_referent)?;
            expression = ExpandedClauseExpression::Operation {
                row,
                type_arguments: Vec::new(),
                const_arguments: Vec::new(),
                result: CheckedType::Integer(super::super::model::IntegerType::U64),
                arguments: vec![expression],
            };
            return Ok((expression, false, false));
        }
        Ok((expression, holder_pending, range_referent))
    }
    /// The typed half of [FN-8]'s admission, judged on the checked clause.
    ///
    /// An erased exact conversion is admitted by the whole endpoint types,
    /// not by another clause or a particular operand's value. Numeric bounds
    /// remain symbolic here and use the same finite-domain totality judgment
    /// as executable conversion obligations.
    ///
    /// A subscript is admitted only inside an [ENT-2] clause (b) place: a
    /// measure read or a selected integer whose every offset is itself a
    /// tracked place or constant. Formation still proves every OP-4 before
    /// the containing clause can establish its conclusion.
    pub(super) fn validate_clause_checked_forms(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        expression: &CheckedExpression,
    ) -> Result<(), CheckStop> {
        if let CheckedExpression::NumericConversion {
            mode: CheckedConversionMode::Exact,
            source,
            destination,
            ..
        } = expression
            && !source.converts_totally_to(*destination)
        {
            return self.declarations.invalid_clause(clause, entry);
        }
        // A measure's selected value is not an integer element, but its
        // place has the same selector admission. This judgment also runs
        // after definition expansion, before template construction, so an
        // element hidden behind an alias cannot become an offset here.
        // Flat ArrayMeasure roots contain fields only; indexed aliases are
        // normalized to ContainerMeasure by expand_definition_offsets.
        let invalid_measure_selector = match expression {
            CheckedExpression::ContainerMeasure { root, .. } => root.subscripted_term().is_none(),
            CheckedExpression::BufferMeasure { root, .. } => root.subscripted_term().is_none(),
            CheckedExpression::RangeElementMeasure { place, .. } => {
                place.subscripted_term().is_none()
            }
            _ => false,
        };
        if invalid_measure_selector {
            return self.declarations.invalid_clause(clause, entry);
        }
        let subscripted = match expression {
            CheckedExpression::ReadStorage { root, .. } => root
                .path
                .iter()
                .any(|step| matches!(step, CheckedPlaceStep::Subscript(_))),
            CheckedExpression::ArrayIndex { .. }
            | CheckedExpression::BufferIndex { .. }
            | CheckedExpression::RangeIndex { .. }
            | CheckedExpression::BorrowRangeIndex { .. } => true,
            _ => false,
        };
        if subscripted && expression.integer_element_term().is_none() {
            return self.declarations.invalid_clause(clause, entry);
        }
        for child in expression_children(expression) {
            self.validate_clause_checked_forms(clause, entry, child)?;
        }
        Ok(())
    }
    /// Holds a clause local to [FN-8]'s "own copy value", judged on the type
    /// the checker derived for it.
    ///
    /// The mode half of that phrase needs no check — the grammar admits no
    /// written mode and [FN-8] fixes it — but the copy half is a real
    /// restriction that the deleted annotation used to carry. The admitted-row
    /// filter does not imply it: `array_new` and the `checked` arithmetic rows
    /// are pure, total and non-trapping, and yield an `array<T, N>` and a
    /// `Result<T, Overflow>` respectively.
    pub(super) fn validate_clause_copy_local(
        &self,
        check_context: &CheckContext<'_>,
        clause: ClauseKind<'_>,
        entry: NodeId,
        binding: BindingId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        let local = bindings
            .values()
            .find(|local| local.binding == binding)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.is_copy_type(check_context, local.ty)? {
            return self.declarations.invalid_clause(clause, entry);
        }
        Ok(())
    }
}
