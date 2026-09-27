use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

use super::super::super::super::model::{
    CheckedExpression, CheckedFloatOperation, CheckedMode, CheckedType,
};
use super::super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, TypedExpression,
};

pub(super) fn is_float_operation(spelling: &str) -> bool {
    float_operation(spelling).is_some()
}

fn float_operation(spelling: &str) -> Option<CheckedFloatOperation> {
    Some(match spelling {
        "fadd.strict" => CheckedFloatOperation::AddStrict,
        "fsub.strict" => CheckedFloatOperation::SubtractStrict,
        "fmul.strict" => CheckedFloatOperation::MultiplyStrict,
        "fdiv.strict" => CheckedFloatOperation::DivideStrict,
        "feq" => CheckedFloatOperation::Equal,
        "flt" => CheckedFloatOperation::Less,
        "fle" => CheckedFloatOperation::LessEqual,
        "fgt" => CheckedFloatOperation::Greater,
        "fge" => CheckedFloatOperation::GreaterEqual,
        "fne" => CheckedFloatOperation::NotEqual,
        "fneg" => CheckedFloatOperation::Negate,
        "fabs" => CheckedFloatOperation::Absolute,
        "fcopysign" => CheckedFloatOperation::CopySign,
        "fmin" => CheckedFloatOperation::Minimum,
        "fmax" => CheckedFloatOperation::Maximum,
        "ffloor" => CheckedFloatOperation::Floor,
        "fceil" => CheckedFloatOperation::Ceil,
        "ftrunc" => CheckedFloatOperation::Truncate,
        "froundeven" => CheckedFloatOperation::RoundEven,
        "frem" => CheckedFloatOperation::Remainder,
        "fsqrt.strict" => CheckedFloatOperation::SquareRootStrict,
        "ffma.strict" => CheckedFloatOperation::FusedMultiplyAddStrict,
        "finf" => CheckedFloatOperation::Infinity,
        "fnan" => CheckedFloatOperation::Nan,
        _ => return None,
    })
}

impl<'unit> Checker<'unit> {
    pub(super) fn check_float_operation(
        &self,
        node: NodeId,
        spelling: &str,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let operation =
            float_operation(spelling).ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.reject_named_operation_arguments(node, spelling)?;
        let operand_count = operation.operand_count();
        if operand_count > 0 {
            self.reject_written_operation_type_argument(node)?;
        }
        let atoms = self.operation_atoms(node, operand_count)?;
        let mut arguments = Vec::with_capacity(atoms.len());
        let mut effects = EffectSet::NONE;
        // [OP-2] the selected type is the first operand's exact type. `finf`
        // and `fnan` have no operand at all, so [TYPE-5] retains their
        // written result type and they are the one float row that reads it.
        let mut selected = None;
        for atom in atoms {
            let argument = self.check_atom(function, atom, bindings, loop_depth)?;
            let operand_type = *selected.get_or_insert(argument.expression.ty());
            if argument.mode != CheckedMode::Own || argument.expression.ty() != operand_type {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        format!("own {}", self.checked_type_name(operand_type)?),
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            effects = effects.union(argument.effects);
            arguments.push(argument.expression);
        }
        let operand_type = match selected {
            Some(operand_type) => operand_type,
            None => self.retained_operation_type_argument(node, function)?,
        };
        if !matches!(
            operand_type,
            CheckedType::Float(_) | CheckedType::GenericFloat(_)
        ) {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        Ok(TypedExpression::owned(
            CheckedExpression::FloatOperation {
                carrier: self.tree.path(node)?.clone(),
                operation,
                operand_type,
                arguments,
            },
            effects,
        ))
    }
}
