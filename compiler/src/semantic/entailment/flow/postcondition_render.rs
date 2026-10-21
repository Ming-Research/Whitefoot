//! Written postcondition clauses, independently of their normalized proof relation.

use super::*;
use crate::semantic::postcondition::{
    ParameterDenotation, PostconditionClause, SelectedPostconditionReturn,
};

impl Input<'_, '_> {
    pub(super) fn render_postcondition_clause(
        &self,
        postcondition: &CheckedPostcondition,
        selected: &SelectedPostconditionReturn,
        returned: &[&CheckedExpression],
    ) -> String {
        let render_datum = |datum: &RelationDatum| {
            self.render_postcondition_datum(datum, postcondition, selected, returned)
        };
        if let Some(clause) = self
            .context
            .postcondition_clauses
            .get(&(self.function.id, postcondition.selector.block.clone()))
        {
            return self.render_written_postcondition(clause, &render_datum, false);
        }
        // A type invariant can generate a postcondition without a written
        // ensures expression. Its admitted operands still retain their order.
        let operands = postcondition
            .relation
            .operands
            .iter()
            .map(|operand| {
                let mut text = render_datum(&operand.datum);
                if let Some(parameter) = &operand.parameter {
                    text = format!("({text} + {})", render_datum(parameter));
                }
                if operand.displacement != 0 {
                    text = format!(
                        "({text} + {})",
                        self.render_normalized_integer(operand.displacement, operand.ty()),
                    );
                }
                text
            })
            .collect::<Vec<_>>();
        render::render_operation_spelling(postcondition.relation.operation.spelling(), &operands)
    }

    fn render_written_postcondition(
        &self,
        clause: &PostconditionClause,
        datum: &impl Fn(&RelationDatum) -> String,
        nested: bool,
    ) -> String {
        match clause {
            PostconditionClause::Datum(value) => datum(value),
            PostconditionClause::Literal { value, spelling } => spelling
                .clone()
                .unwrap_or_else(|| self.render_goal_datum(&GoalDatum::Literal(value.clone()))),
            PostconditionClause::NamedConst {
                declaration,
                projections,
                spelling,
            } => self.render_goal_projections(
                spelling.clone(),
                self.context
                    .constant(*declaration)
                    .map(|constant| constant.declared_type),
                projections,
            ),
            PostconditionClause::Operation { row, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.render_written_postcondition(argument, datum, true))
                    .collect::<Vec<_>>();
                let rendered = render::render_goal_row(row, &arguments, self.context.declarations);
                if nested && matches!(row, GoalOperation::Integer { .. }) && arguments.len() == 2 {
                    format!("({rendered})")
                } else {
                    rendered
                }
            }
        }
    }

    fn render_postcondition_datum(
        &self,
        datum: &RelationDatum,
        postcondition: &CheckedPostcondition,
        selected: &SelectedPostconditionReturn,
        returned: &[&CheckedExpression],
    ) -> String {
        match datum {
            RelationDatum::Result {
                ordinal,
                projections,
                ..
            } => self.render_postcondition_result(
                *ordinal, projections, postcondition, selected, returned,
            ),
            RelationDatum::Parameter {
                ordinal,
                projections,
                denotation,
                ..
            } => self.render_postcondition_parameter(*ordinal, projections, *denotation),
            RelationDatum::NamedConst {
                declaration,
                projections,
                ty,
            } => self.render_goal_datum(&GoalDatum::NamedConst {
                declaration: *declaration,
                projections: projections.clone(),
                ty: *ty,
                admission: Default::default(),
            }),
            RelationDatum::Literal { value, .. } => {
                self.render_goal_datum(&GoalDatum::Literal(value.clone()))
            }
            RelationDatum::Measure(measure, place) => {
                let base = match place.root {
                    PostconditionPlaceRoot::Result { ordinal } => self.render_postcondition_result(
                        ordinal, &place.projections, postcondition, selected, returned,
                    ),
                    PostconditionPlaceRoot::Parameter { ordinal } => self.render_postcondition_parameter(
                        ordinal, &place.projections, ParameterDenotation::EntryDatum,
                    ),
                    PostconditionPlaceRoot::ExitParameter { ordinal } => self.render_postcondition_parameter(
                        ordinal, &place.projections, ParameterDenotation::ExitState,
                    ),
                };
                format!("{base}.{}", measure.spelling())
            }
        }
    }

    fn render_postcondition_parameter(
        &self,
        ordinal: u32,
        projections: &[GoalProjection],
        denotation: ParameterDenotation,
    ) -> String {
        let Some(parameter) = self.function.parameters.get(ordinal as usize) else {
            return "?".to_owned();
        };
        let mut base = self.binding_name(parameter.binding);
        if denotation == ParameterDenotation::EntryDatum {
            base = format!("entry({base})");
        }
        let projections = if parameter.mode.is_reference()
            && let Some((GoalProjection::Deref, rest)) = projections.split_first()
        {
            // The formal keeps its source reference step, but its checked
            // type already names the referent. Consume that step before a
            // Box content step can be rendered from the referent type.
            base.push('^');
            rest
        } else {
            projections
        };
        self.render_goal_projections_at(
            base,
            Some(parameter.ty),
            parameter.mode.is_range(),
            0,
            projections,
        )
    }

    fn render_postcondition_result(
        &self,
        ordinal: u32,
        projections: &[GoalProjection],
        postcondition: &CheckedPostcondition,
        selected: &SelectedPostconditionReturn,
        returned: &[&CheckedExpression],
    ) -> String {
        let Some(mut expression) = returned.get(ordinal as usize).copied() else {
            return "?".to_owned();
        };
        let mut source = self.context.postcondition_sources.get(&selected.statement);
        if selected.values.len() > 1 {
            source = source.and_then(|source| source.fields.get(ordinal as usize));
        }
        let mut suffix = Vec::new();
        if ordinal == postcondition.selector.ordinal
            && let Some(route) = postcondition.selector.route
        {
            if let CheckedExpression::ConstructEnum { fields, .. } = expression {
                if let Some(field) = fields.get(route.field as usize) {
                    expression = field;
                    source = source.and_then(|source| source.fields.get(route.field as usize));
                }
            } else {
                suffix.push(GoalProjection::Payload {
                    variant: route.variant,
                    field: route.field,
                });
            }
        }
        suffix.extend_from_slice(projections);
        // A projected construction denotes its field operand; selecting that
        // operand also preserves its own literal type and source occurrence.
        while let (
            CheckedExpression::ConstructStruct { fields, .. },
            Some(GoalProjection::Field(field)),
        ) = (expression, suffix.first())
        {
            let Some(value) = fields.get(*field as usize) else {
                break;
            };
            source = source.and_then(|source| source.fields.get(*field as usize));
            expression = value;
            suffix.remove(0);
        }
        let base = match expression {
            CheckedExpression::Constant(value) => source
                .filter(|source| source.literal)
                .map(|source| source.spelling.clone())
                .unwrap_or_else(|| self.render_goal_datum(&GoalDatum::Literal(value.clone()))),
            _ => expression
                .carrier()
                .and_then(|path| self.context.postcondition_sources.get(path))
                .or(source)
                .map(|source| source.spelling.clone())
                .unwrap_or_else(|| self.render_expression(expression)),
        };
        let base = if !suffix.is_empty() && matches!(expression, CheckedExpression::UserCall { .. }) {
            format!("({base})")
        } else {
            base
        };
        self.render_goal_projections(base, Some(expression.ty()), &suffix)
    }
}
