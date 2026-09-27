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
use super::super::postcondition::PostconditionConstantOrigin;
use super::{CheckStop, Checker, ControlCounters, ControlScope, FunctionSignature, LocalBinding};

pub(super) struct CheckedRequires {
    pub(super) requirements: Vec<CheckedRequirement>,
    /// The [ENT-2] clause (b) places each requirement forms, index-aligned
    /// with `requirements` [`super::super::model::CheckedFunction::requirement_places`].
    pub(super) places: Vec<Vec<CheckedExpression>>,
}

/// One checked definition's contribution to the requirements that expand it.
struct ClauseDefinition {
    binding: BindingId,
    /// The clause (b) places its own initializer writes.
    places: Vec<CheckedExpression>,
    /// The earlier definitions its initializer reads.
    uses: Vec<BindingId>,
}

/// [ENT-2] every clause (b) place one checked clause expression writes, in
/// source order: a measure of a subscripted place and a subscripted read,
/// the two forms whose subscripts owe [OP-4] where the place is formed.
fn collect_clause_places(expression: &CheckedExpression, places: &mut Vec<CheckedExpression>) {
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
        CheckedExpression::RangeElementMeasure { .. } | CheckedExpression::RangeIndex { .. } => {
            places.push(expression.clone());
        }
        _ => {}
    }
    for child in expression_children(expression) {
        collect_clause_places(child, places);
    }
}

/// Each definition whose value can stand as an offset of a clause (b) place,
/// with that value and the captured term it reads: a binding read, an integer
/// literal or named const, or a const generic, followed through earlier
/// definitions [ENT-2].
type DefinitionOffsets = HashMap<BindingId, (CheckedExpression, CapturedTerm)>;

/// The offset a definition's value supplies after expansion, when it is one.
fn definition_offset(
    value: &CheckedExpression,
    definitions: &[ClauseDefinition],
    offsets: &DefinitionOffsets,
) -> Option<(CheckedExpression, CapturedTerm)> {
    match value {
        CheckedExpression::Binding {
            binding,
            consume_root: false,
            ..
        } => match offsets.get(binding) {
            Some(expanded) => Some(expanded.clone()),
            None if definitions
                .iter()
                .any(|definition| definition.binding == *binding) =>
            {
                None
            }
            None => Some((value.clone(), CapturedTerm::Binding(*binding))),
        },
        CheckedExpression::Constant(CheckedValue::Integer { bits, .. })
        | CheckedExpression::NamedConstant {
            value: CheckedValue::Integer { bits, .. },
            ..
        } => Some((value.clone(), CapturedTerm::Literal(*bits))),
        CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
            Some((value.clone(), CapturedTerm::Const(*declaration)))
        }
        _ => None,
    }
}

/// One subscript offset of a clause place, read through a definition, is the
/// definition's own offset after expansion [FN-8].
fn expand_definition_offset(
    offset: &mut CheckedExpression,
    captured: &mut CapturedValue,
    offsets: &DefinitionOffsets,
) {
    if let CheckedExpression::Binding { binding, .. } = offset
        && let Some((value, term)) = offsets.get(binding)
    {
        *offset = value.clone();
        captured.term = *term;
    }
}

fn expand_definition_path_offsets(path: &mut [CheckedPlaceStep], offsets: &DefinitionOffsets) {
    for step in path {
        if let CheckedPlaceStep::Subscript(subscript) = step {
            let subscript = subscript.as_mut();
            expand_definition_offset(&mut subscript.offset, &mut subscript.captured, offsets);
        }
    }
}

/// [FN-8] a definition is erased by expansion, so a clause (b) place is formed
/// with every offset it reads through a definition replaced by the offset
/// that definition expands to; its subscripts then owe [OP-4] over the same
/// terms the expanded requirement establishes.
fn expand_definition_offsets(place: &mut CheckedExpression, offsets: &DefinitionOffsets) {
    match place {
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            expand_definition_path_offsets(&mut root.path, offsets);
        }
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. } => {
            expand_definition_offset(&mut place.offset, &mut place.captured, offsets);
            expand_definition_path_offsets(&mut place.path, offsets);
        }
        _ => {}
    }
}

/// The bindings one checked clause expression reads by name.
fn collect_clause_reads(expression: &CheckedExpression, reads: &mut Vec<BindingId>) {
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
        /// Bare exclusive measures in ensures denote exit state. An entry
        /// former clears this bit before ordinary projections are expanded.
        exit_state: bool,
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

    fn into_goal_expression(self) -> Option<GoalExpression> {
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

impl<'unit> Checker<'unit> {
    pub(super) fn check_requires(
        &self,
        function: &FunctionSignature,
        block: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<CheckedRequires, CheckStop> {
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
                    exit_state: false,
                }),
            );
        }
        for definition in self.tree.children_with(block, Production::ContractDefine)? {
            let expression = self
                .tree
                .first_child_with(definition, Production::Expr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if !self.validate_clause_computation(ClauseKind::Requires, definition, expression)? {
                self.validate_clause_definition_datum(
                    ClauseKind::Requires,
                    definition,
                    expression,
                )?;
            }
            let checked = self
                .check_statement(
                    function,
                    definition,
                    bindings,
                    counters,
                    ControlScope {
                        loops: &[],
                        give_context: None,
                    },
                )
                .map_err(Self::clause_conditional_repair)?;
            if !checked.can_continue {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            let CheckedStatement::Let { binding, value, .. } = &checked.statement else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            self.validate_clause_checked_forms(ClauseKind::Requires, definition, value)?;
            self.validate_clause_copy_local(ClauseKind::Requires, definition, *binding, bindings)?;
            let expanded =
                self.build_clause_expression(expression, value, bindings, &expanded_bindings)?;
            expanded_bindings.insert(*binding, expanded);
            let mut places = Vec::new();
            collect_clause_places(value, &mut places);
            for place in &mut places {
                expand_definition_offsets(place, &definition_offsets);
            }
            if let Some(offset) = definition_offset(value, &definitions, &definition_offsets) {
                definition_offsets.insert(*binding, offset);
            }
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
        for clause in self.tree.children_with(block, Production::RequiresClause)? {
            let expression = self
                .tree
                .first_child_with(clause, Production::ClauseExpr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.validate_clause_condition(ClauseKind::Requires, clause, expression)?;
            let condition = self
                .check_expression(function, expression, bindings, 0)
                .map_err(Self::clause_conditional_repair)?;
            self.validate_clause_checked_forms(
                ClauseKind::Requires,
                clause,
                &condition.expression,
            )?;
            if condition.mode != CheckedMode::Own || condition.expression.ty() != CheckedType::Bool
            {
                return self.issue_node(
                    SemanticRule::Op5,
                    expression,
                    SemanticIssueKind::InvalidPredicateCondition,
                );
            }
            let root = self
                .build_clause_expression(
                    expression,
                    &condition.expression,
                    bindings,
                    &expanded_bindings,
                )?
                .into_goal_expression()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            requirements.push(CheckedRequirement {
                template: GoalTemplate::new(root),
                clause: self.tree.path(clause)?.clone(),
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
                expand_definition_offsets(place, &definition_offsets);
            }
            requirement_places.push(places);
        }
        Ok(CheckedRequires {
            requirements,
            places: requirement_places,
        })
    }

    /// The contract-conditional OWN-1 bare-affine repair [#35]. OWN-1's
    /// ordinary mechanical fix is `write move p`, but [FN-8] rejects `move`
    /// inside a contract block, so that instruction would send the writer
    /// from one hard error to another. A definition or clause instead carries
    /// the contract-specific repair.
    fn clause_conditional_repair(stop: CheckStop) -> CheckStop {
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

    /// Alpha-expands one already-checked admitted FN-8/FN-9 expression. Source
    /// atoms supply declaration/projection identity; the checked expression
    /// supplies the uniquely selected row and types.
    pub(super) fn build_clause_expression(
        &self,
        source: NodeId,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        // [GRAM-5, MSR-5] a clause and its affine sides have their own
        // shapes, and each is walked against the checked expression the
        // typer built for exactly that node.
        match self.tree.production(source)? {
            Production::ClauseExpr => {
                return self.build_clause_root(source, checked, bindings, expanded_bindings);
            }
            Production::AffineExpr | Production::AffineTerm | Production::AffineFactor => {
                return self.build_clause_affine(
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
            .tree
            .first_child_with(source, Production::Call)?
            .is_none()
            && self
                .tree
                .first_child_with(source, Production::InfixTail)?
                .is_none()
            && let Some(atom) = self.tree.first_child_with(source, Production::Atom)?
        {
            return self.build_clause_atom(atom, Some(checked), bindings, expanded_bindings);
        }
        let atoms = self.clause_operand_atoms(source)?;
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
            CheckedExpression::EnumEquality {
                equal,
                operand_type,
                arguments,
                ..
            } => Some((
                GoalOperation::EnumEquality {
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
                    self.build_clause_operand(atom, Some(argument), bindings, expanded_bindings)
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
            let argument = self.build_clause_atom(atoms[0], None, bindings, expanded_bindings)?;
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
        self.build_clause_operand(atoms[0], Some(checked), bindings, expanded_bindings)
    }

    /// [GRAM-5] one `clause_expr` root: one `affine_expr`, or two around one
    /// `clause_op` whose row the typer has already selected.
    fn build_clause_root(
        &self,
        source: NodeId,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        match self.tree.children(source)? {
            [side] => {
                let side = *side;
                self.build_clause_affine(side, None, checked, bindings, expanded_bindings)
            }
            [left, _operator, right] => {
                let (left, right) = (*left, *right);
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
                            left,
                            None,
                            checked_left,
                            bindings,
                            expanded_bindings,
                        )?,
                        self.build_clause_affine(
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
        &self,
        source: NodeId,
        terms: Option<usize>,
        checked: &CheckedExpression,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        match self.tree.production(source)? {
            Production::AffineExpr => {
                let children = self.tree.children(source)?.to_vec();
                let count = terms.unwrap_or_else(|| children.len().div_ceil(2));
                let last = count
                    .checked_mul(2)
                    .and_then(|doubled| doubled.checked_sub(2))
                    .and_then(|index| children.get(index).copied())
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if count == 1 {
                    return self.build_clause_affine(
                        last,
                        None,
                        checked,
                        bindings,
                        expanded_bindings,
                    );
                }
                self.build_clause_affine_operation(
                    source,
                    Some(count - 1),
                    last,
                    checked,
                    bindings,
                    expanded_bindings,
                )
            }
            Production::AffineTerm => {
                let factors = self.tree.children_with(source, Production::AffineFactor)?;
                match factors.as_slice() {
                    [factor] => self.build_clause_affine(
                        *factor,
                        None,
                        checked,
                        bindings,
                        expanded_bindings,
                    ),
                    [left, right] => self.build_clause_affine_operation(
                        *left,
                        None,
                        *right,
                        checked,
                        bindings,
                        expanded_bindings,
                    ),
                    _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                }
            }
            Production::AffineFactor => {
                let child = self.tree.only_child(source)?;
                self.build_clause_affine(child, None, checked, bindings, expanded_bindings)
            }
            _ => self.build_clause_operand(source, Some(checked), bindings, expanded_bindings),
        }
    }

    /// One binary node of a clause side's affine fold [MSR-5].
    fn build_clause_affine_operation(
        &self,
        left: NodeId,
        left_terms: Option<usize>,
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
                    left,
                    left_terms,
                    checked_left,
                    bindings,
                    expanded_bindings,
                )?,
                self.build_clause_affine(right, None, checked_right, bindings, expanded_bindings)?,
            ],
        })
    }

    /// One written clause operand. An `atom` is a leaf datum; every other
    /// written form — today exactly a `call`, which is how [MSR-5] admits a
    /// measure term as an operand — is expanded by the ordinary clause walk
    /// against the row the typer already selected for it.
    fn build_clause_operand(
        &self,
        node: NodeId,
        checked: Option<&CheckedExpression>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        if self.tree.production(node)? == Production::Atom {
            return self.build_clause_atom(node, checked, bindings, expanded_bindings);
        }
        let checked = checked.ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.build_clause_expression(node, checked, bindings, expanded_bindings)
    }

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

    fn build_clause_atom(
        &self,
        atom: NodeId,
        checked: Option<&CheckedExpression>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        if self.postcondition_selector_use_inside(atom)? {
            let ty = self
                .active_postcondition
                .get()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .result_type;
            if let Some(expanded) =
                self.build_clause_result_place(atom, bindings, expanded_bindings)?
            {
                return Ok(expanded);
            }
            return Ok(ExpandedClauseExpression::InvalidSelectorUse {
                ty: checked.map_or(ty, CheckedExpression::ty),
            });
        }
        if let Some(literal) = self
            .tree
            .direct_token_with(atom, crate::TerminalPredicate::Literal)?
        {
            let Some(CheckedExpression::Constant(value)) = checked else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            let bytes = self.tree.token_bytes(literal)?;
            let origin = if matches!(bytes, b"0_T" | b"1_T") {
                let usage = self.use_at(atom, LexicalUseRole::GenericNumericSuffix)?;
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
        if let Some(declaration) = self.clause_const_generic_base(atom)? {
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
            .tree
            .first_child_with(atom, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let result = self.build_clause_place(place, bindings, expanded_bindings)?;
        if checked.is_some_and(|expression| expression.ty() != result.ty()) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(result)
    }

    /// One clause operand written as a place over the clause's own result
    /// datum [FN-9, CALL-4], if the atom is one.
    ///
    /// Two shapes are admitted and nothing else. A bare selector spelling is
    /// the result datum itself. A place whose trailing member is one of
    /// [MSR-1]'s measures is that measure over the result place the written
    /// member path reaches, because [OP-15] reads a measure as a member of
    /// the measured place and gives it no storage below itself; that is how
    /// `ensures result.len == n` and `ensures result.inner.len == count` are
    /// one relation term and not a second fact class. Every other member
    /// path over a result datum stays outside the admitted operand set, and
    /// the caller reports it as the ordinary invalid selector use.
    fn build_clause_result_place(
        &self,
        atom: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<Option<ExpandedClauseExpression>, CheckStop> {
        let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
            return Ok(None);
        };
        let Some((ordinal, datum_type)) = self.postcondition_selector_place_base(place)? else {
            return Ok(None);
        };
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        if suffixes.is_empty() {
            return Ok(Some(ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Result {
                    ordinal,
                    projections: Vec::new(),
                    ty: datum_type,
                },
            )));
        }
        let Some(measure) = self.trailing_measure_member(&suffixes)? else {
            return Ok(None);
        };
        let (projections, measured_type) = self.clause_member_projections(
            &suffixes[..suffixes.len() - 1],
            datum_type,
            false,
            bindings,
            expanded_bindings,
        )?;
        let row = self.clause_measure_row(measure, measured_type, false)?;
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

    /// The projection path one written `psuffix` run selects below a clause
    /// datum of this type.
    ///
    /// [TYPE-9] gives a `Box` exactly one member, `inner`, and that member
    /// is the box content itself, so the goal place below it is the same
    /// dereference a `deref` former used to write. Every other member is the
    /// ordinary struct field step and is judged by the ordinary walk.
    ///
    /// `range_referent` says the datum is the run a range reference names,
    /// whose type is its element type [TYPE-8]: its first subscript selects
    /// an element of that type rather than indexing a value of it.
    fn clause_member_projections(
        &self,
        suffixes: &[NodeId],
        mut ty: CheckedType,
        mut range_referent: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(Vec<GoalProjection>, CheckedType), CheckStop> {
        let mut projections = Vec::with_capacity(suffixes.len());
        for suffix in suffixes {
            let range_step = std::mem::replace(&mut range_referent, false);
            // [ENT-2] clause (b) forms a place with field selections,
            // `deref` wrappings and subscripts, which is what makes
            // `table[i].len` and `nodes[i].count` terms. A clause reads that place exactly as the
            // body does, so a subscript written here is one projection and not
            // a composite value this version cannot represent.
            if self.subscript_offset(*suffix)?.is_some() {
                let (projection, element) = self.clause_subscript_projection(
                    *suffix,
                    ty,
                    range_step,
                    bindings,
                    expanded_bindings,
                )?;
                projections.push(projection);
                ty = element;
                continue;
            }
            if let CheckedType::Nominal(nominal) = ty
                && let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind
            {
                let name = self
                    .deferred_use_at(*suffix, crate::DeferredUseRole::ProjectedField)?
                    .spelling()
                    .to_owned();
                if name != "inner" {
                    return self.issue_node(
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
            let (fields, reached) = self.resolve_struct_path(std::slice::from_ref(suffix), ty)?;
            projections.extend(fields.into_iter().map(GoalProjection::Field));
            ty = reached;
        }
        Ok((projections, ty))
    }

    /// One written subscript inside a clause (b) place [ENT-2].
    ///
    /// [ENT-2] fixes what may stand there: "Each offset occurring inside a
    /// clause (b) place is itself a clause (a) or clause (c) term, because the
    /// place's identity is decided over its offsets." A clause is no
    /// evaluation, so the offset carries no occurrence of its own: [ENT-2]
    /// makes two places one term when "their canonical source spellings are
    /// byte-identical", which is exactly what keys this projection, and
    /// [ENT-5] puts the offset's own support into every enclosing term so a
    /// write to it kills them all.
    ///
    /// A parameter offset is kept as the formal it names, because a caller
    /// substitutes its own actual there [FN-8, CALL-6]; a literal and a const
    /// are values and need no substitution. Any other tracked-place offset,
    /// one with projections or a definition, is admitted but not represented
    /// here and is reported as the compiler capability it is [DIAG-1]; an
    /// element read never reaches here, because a subscript that ends a
    /// clause place is refused first.
    fn clause_subscript_projection(
        &self,
        suffix: NodeId,
        base: CheckedType,
        range_referent: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(GoalProjection, CheckedType), CheckStop> {
        let element = match base {
            // [REF-4] a range reference's referent is the run of its element
            // type, which is the type its checked datum carries.
            _ if range_referent => base,
            CheckedType::Buffer { element } => self.element_type(element)?,
            CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                self.element_type(element)?
            }
            _ => {
                return self
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            }
        };
        let offset = self
            .subscript_offset(suffix)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let capture = crate::semantic::places::CapturedValue::unknown().capture;
        let value =
            |term| crate::semantic::places::CapturedValue::new(capture, term).goal_identity();
        if let Some(literal) = self
            .tree
            .direct_token_with(offset, crate::TerminalPredicate::Literal)?
        {
            let bytes = self.tree.token_bytes(literal)?;
            let CheckedValue::Integer { bits, .. } = self.parse_literal(offset, bytes)? else {
                return self
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            };
            return Ok((
                GoalProjection::Subscript(value(crate::semantic::places::CapturedTerm::Literal(
                    bits,
                ))),
                element,
            ));
        }
        if let Some(declaration) = self.clause_const_generic_base(offset)? {
            return Ok((
                GoalProjection::Subscript(value(crate::semantic::places::CapturedTerm::Const(
                    declaration,
                ))),
                element,
            ));
        }
        let Some(place) = self.tree.first_child_with(offset, Production::Place)? else {
            return self.unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
        };
        if !self
            .tree
            .children_with(place, Production::Psuffix)?
            .is_empty()
        {
            return self.unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
        }
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source { declaration, class } = usage.target() else {
            return self.unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
        };
        if class == DeclarationClass::NamedConst {
            let constant = self
                .constants
                .get(&declaration)
                .copied()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let CheckedValue::Integer { bits, .. } = self.constant(constant)?.value else {
                return self
                    .unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
            };
            return Ok((
                GoalProjection::Subscript(value(crate::semantic::places::CapturedTerm::Literal(
                    bits,
                ))),
                element,
            ));
        }
        if class != DeclarationClass::Value {
            return self.unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix);
        }
        let local = bindings
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        match expanded_bindings.get(&local.binding) {
            Some(ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                ordinal,
                projections,
                ..
            })) if projections.is_empty() && local.mode == CheckedMode::Own => Ok((
                GoalProjection::FormalSubscript { ordinal: *ordinal },
                element,
            )),
            _ => self.unsupported(crate::UnsupportedSemanticFeature::CompositeValues, suffix),
        }
    }

    /// The [MSR-1] row one written measure selects over a place of this type
    /// [OP-15].
    ///
    /// A range referent carries its own row: [MSR-1] gives `&[T]` a row of
    /// its own, and the referent type a `deref` of one selects is the
    /// element type, so the row cannot be recovered from that type.
    fn clause_measure_row(
        &self,
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
            CheckedType::Window {
                element, capacity, ..
            } => (Some(element), capacity),
            CheckedType::Array { element, length } => (Some(element), Some(length)),
            CheckedType::Buffer { element } => (Some(element), None),
            _ if range_referent => (Some(self.intern_element(ty)?), None),
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        Ok(GoalOperation::ContainerMeasure {
            measure,
            measured,
            element,
            constant,
        })
    }

    /// The const generic one clause `atom` names directly [MSR-6], if any.
    ///
    /// A const generic is one `pbase` with no `deref` wrapping and no
    /// suffix; every other place shape resolves through the ordinary walk.
    fn clause_const_generic_base(&self, atom: NodeId) -> Result<Option<DeclarationId>, CheckStop> {
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
        if self.has_fixed(pbase, FixedTerminal::Deref)? {
            return Ok(None);
        }
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        Ok(match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::ConstGeneric,
            } => Some(declaration),
            _ => None,
        })
    }

    fn build_clause_place(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let (expression, holder_pending, _) =
            self.build_clause_place_inner(place, bindings, expanded_bindings)?;
        if holder_pending {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(expression)
    }

    /// Mirrors the already-completed TYPE-7 dereference type walk while
    /// retaining only predicate identity.
    ///
    /// [TYPE-7] `deref` denotes the referent of a reference and nothing else:
    /// a `Box`'s content is its field `inner` and is reached by the ordinary
    /// field step [TYPE-9], so the only nested place this step admits is a
    /// reference. The written step is retained as one projection of the
    /// declaration-boundary template because a caller substitutes the
    /// actual's own path for the formal and consumes exactly that leading
    /// projection [FN-8, CALL-6]; the callee body, where [REF-1] makes the
    /// parameter name the path itself, drops it instead.
    fn build_clause_place_inner(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        expanded_bindings: &HashMap<BindingId, ExpandedClauseExpression>,
    ) -> Result<(ExpandedClauseExpression, bool, bool), CheckStop> {
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let (mut expression, holder_pending, mut range_referent) =
            if self.has_fixed(pbase, FixedTerminal::Deref)? {
                let nested = self
                    .tree
                    .first_child_with(pbase, Production::Place)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let (nested, nested_holder_pending, nested_range) =
                    self.build_clause_place_inner(nested, bindings, expanded_bindings)?;
                if !nested_holder_pending {
                    // [TYPE-7] an owned place is named as itself; only a
                    // reference has a referent this step can name.
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let ty = nested.ty();
                (
                    nested
                        .with_projection(GoalProjection::Deref, ty)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                    false,
                    nested_range,
                )
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
                        (
                            expanded_bindings
                                .get(&local.binding)
                                .cloned()
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            local.mode != CheckedMode::Own,
                            local.mode == CheckedMode::Range,
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
        if self.has_fixed(pbase, FixedTerminal::Entry)? {
            let ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                exit_state, ..
            }) = &mut expression
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            *exit_state = false;
        }
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        // [OP-15] a measure is read as a member of the measured place and
        // [MSR-1] gives it no storage below itself, so it is the last written
        // suffix and everything before it is the ordinary field path.
        let measure = self.trailing_measure_member(&suffixes)?;
        let fields_only = if measure.is_some() {
            &suffixes[..suffixes.len() - 1]
        } else {
            suffixes.as_slice()
        };
        if holder_pending && !fields_only.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        if !fields_only.is_empty() {
            let (projections, final_ty) = self.clause_member_projections(
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
            range_referent = false;
        }
        // [OP-14, TYPE-9] `free_empty` writes one source contract for both
        // direct windows and Boxes holding runtime-capacity windows. At the
        // boxed instance its prelude-owned `window.len` measure place denotes
        // `window.inner.len`, matching the ordinary expression judgment's
        // prelude-only implicit content step. Retain that step in the
        // GoalTemplate so CALL-6 substitutes the caller's content measure.
        if measure.is_some()
            && let Some(suffix) = suffixes.last()
            && self.tree.is_prelude_node(*suffix)?
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
    /// measure read, or a read whose final step selects a readonly field of
    /// one fragment type. Any other subscripted read names an element value,
    /// which a clause cannot.
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
            return self.invalid_clause(clause, entry);
        }
        let subscripted_field = match expression {
            CheckedExpression::ReadStorage { root, .. } => root
                .path
                .iter()
                .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
                .then(|| root.readonly_field_term(&self.nominals)),
            CheckedExpression::RangeIndex { place, .. } => {
                Some(place.readonly_field_term(&self.nominals))
            }
            CheckedExpression::ArrayIndex { .. }
            | CheckedExpression::BufferIndex { .. }
            | CheckedExpression::BorrowRangeIndex { .. } => Some(None),
            _ => None,
        };
        if subscripted_field == Some(None) {
            return self.invalid_clause(clause, entry);
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
        clause: ClauseKind<'_>,
        entry: NodeId,
        binding: BindingId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        let local = bindings
            .values()
            .find(|local| local.binding == binding)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.is_copy_type(local.ty)? {
            return self.invalid_clause(clause, entry);
        }
        Ok(())
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
        if operation.is_exact() && !Self::clause_affine_operation(operation) {
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
        if self.has_fixed(atom, FixedTerminal::Move)?
            || self
                .tree
                .first_child_with(atom, Production::BorrowExpr)?
                .is_some()
        {
            return self.invalid_clause(clause, entry);
        }
        if let Some(place) = self.tree.first_child_with(atom, Production::Place)? {
            return self.validate_clause_place(clause, entry, place);
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

    fn validate_clause_place(
        &self,
        clause: ClauseKind<'_>,
        entry: NodeId,
        place: NodeId,
    ) -> Result<(), CheckStop> {
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [ENT-2] clause (b): a place formed with field selections,
        // `deref` wrappings and at least one subscript whose final step
        // selects a readonly field is a term of the clause language,
        // `deref(rows)[i].len` and `deref(nodes)[i].count` alike. A clause names no element value, so
        // a subscript that ends the place is this rule's refusal here; one
        // followed by a further step is judged against the selected field's
        // declaration once the place is typed [`validate_clause_checked_forms`].
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        for (position, &suffix) in suffixes.iter().enumerate() {
            if self.subscript_offset(suffix)?.is_some() && position + 1 == suffixes.len() {
                return self.invalid_clause(clause, entry);
            }
        }
        if self.has_fixed(pbase, FixedTerminal::Deref)? {
            let nested = self
                .tree
                .first_child_with(pbase, Production::Place)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.validate_clause_place(clause, entry, nested)?;
        }
        Ok(())
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
