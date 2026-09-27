mod conversions;
mod floating;
mod reinterpret;
mod user;

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::syntax::terminal::TerminalPredicate;
use crate::{
    DeclarationClass, DeclarationId, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    CheckedBooleanOperation, CheckedConversionMode, CheckedExpression, CheckedIntegerArgument,
    CheckedIntegerArgumentSource, CheckedIntegerErrorClass, CheckedIntegerOperation,
    CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedNumericType, CheckedType,
};
use super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, PreludeType, TypedExpression,
};

impl<'unit> Checker<'unit> {
    pub(in crate::semantic::check) fn check_call(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        if self.tree.is_constructor_call(node)? {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a function or operation call in this statement position",
                    "a construction",
                ),
            );
        }
        if let Some(key) = self.behavior_call_key(node)? {
            return self.check_behavior_call(node, key, function, bindings, loop_depth);
        }
        let callee = self
            .tree
            .first_child_with(node, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let usage = self.use_at_roles(
            callee,
            &[
                LexicalUseRole::IdentifierCallee,
                LexicalUseRole::OperationCallee,
            ],
        )?;
        match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Function,
            } => self.check_user_call(node, declaration, function, bindings, loop_depth),
            ResolvedTarget::Operation(operation) => {
                self.check_operation(node, operation, function, bindings, loop_depth)
            }
            _ => Err(SemanticCompilerFailure::InvalidResolution.into()),
        }
    }

    fn check_operation(
        &self,
        node: NodeId,
        operation_id: crate::OperationFamilyId,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let spelling = crate::operation_family_spelling(operation_id)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if matches!(spelling, "band" | "bor" | "bxor" | "bnot") {
            return self.check_boolean_operation(node, spelling, function, bindings, loop_depth);
        }
        if matches!(spelling, "eeq" | "ene") {
            return self.check_enum_equality(
                node,
                spelling == "eeq",
                function,
                bindings,
                loop_depth,
            );
        }
        // [OP-1] v0.60's table carries no reader, view, or acquiring row: a
        // measure is a place form [OP-15], a range reference is a
        // `borrow_expr` [REF-4], and every construction is a [PRE-1] function
        // [OP-13], so those twelve spellings are ordinary identifiers and
        // reach no arm here.
        if floating::is_float_operation(spelling) {
            return self.check_float_operation(node, spelling, function, bindings, loop_depth);
        }
        let conversion_mode = match spelling {
            "cvt" => Some(CheckedConversionMode::Exact),
            "cvt.checked" => Some(CheckedConversionMode::Checked),
            "cvt.defined" => Some(CheckedConversionMode::Defined),
            _ => None,
        };
        if let Some(mode) = conversion_mode {
            return self.check_conversion(node, mode, spelling, function, bindings, loop_depth);
        }
        if spelling == "reinterpret" {
            return self.check_reinterpret(node, function, bindings, loop_depth);
        }
        let operation = match spelling {
            "iabs.wrap" => CheckedIntegerOperation::AbsoluteWrap,
            "iabs" => CheckedIntegerOperation::AbsoluteExact,
            "iabs.defined" => CheckedIntegerOperation::AbsoluteDefined,
            "iabs.checked" => CheckedIntegerOperation::AbsoluteChecked,
            "ineg.wrap" => CheckedIntegerOperation::NegateWrap,
            "ineg" => CheckedIntegerOperation::NegateExact,
            "ineg.defined" => CheckedIntegerOperation::NegateDefined,
            "ineg.checked" => CheckedIntegerOperation::NegateChecked,
            "iand" => CheckedIntegerOperation::BitAnd,
            "ior" => CheckedIntegerOperation::BitOr,
            "ixor" => CheckedIntegerOperation::BitXor,
            "inot" => CheckedIntegerOperation::BitNot,
            "ishl.wrap" => CheckedIntegerOperation::ShiftLeftWrap,
            "ishr.wrap" => CheckedIntegerOperation::ShiftRightWrap,
            "ishl" => CheckedIntegerOperation::ShiftLeftExact,
            "ishr" => CheckedIntegerOperation::ShiftRightExact,
            "ishl.defined" => CheckedIntegerOperation::ShiftLeftDefined,
            "ishr.defined" => CheckedIntegerOperation::ShiftRightDefined,
            "irotl" => CheckedIntegerOperation::RotateLeft,
            "irotr" => CheckedIntegerOperation::RotateRight,
            "ipopcount" => CheckedIntegerOperation::PopulationCount,
            "iclz" => CheckedIntegerOperation::LeadingZeros,
            "ictz" => CheckedIntegerOperation::TrailingZeros,
            "ibswap" => CheckedIntegerOperation::ByteSwap,
            "imulhi" => CheckedIntegerOperation::MultiplyHigh,
            "iadd.sat" => CheckedIntegerOperation::AddSaturating,
            "isub.sat" => CheckedIntegerOperation::SubtractSaturating,
            "imul.sat" => CheckedIntegerOperation::MultiplySaturating,
            "imin" => CheckedIntegerOperation::Minimum,
            "imax" => CheckedIntegerOperation::Maximum,
            _ => {
                return self.unsupported(UnsupportedSemanticFeature::OperationFamily, node);
            }
        };
        if self
            .tree
            .first_child_with(node, Production::FieldinitList)?
            .is_some()
        {
            return self.issue_node(
                SemanticRule::Gram11,
                node,
                SemanticIssueKind::InvalidNamedArguments {
                    callee: spelling.to_owned(),
                    declared_parameters: Vec::new(),
                },
            );
        }
        self.reject_written_operation_type_argument(node)?;
        let atoms = self.operation_atoms(node, operation.operand_count())?;
        self.check_integer_operation_row(node, operation, &atoms, function, bindings, loop_depth)
    }

    /// [OP-1] one integer row over its operand atoms, whichever spelling
    /// selected it.
    ///
    /// The named call and the infix form share this judgment exactly, so the
    /// two spellings of one operation cannot drift apart: [OP-2]'s
    /// operand-derived selection, proof obligation identity, and checked-error
    /// result are decided here once.
    pub(in crate::semantic::check) fn check_integer_operation_row(
        &self,
        node: NodeId,
        operation: CheckedIntegerOperation,
        atoms: &[NodeId],
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let operand_count = operation.operand_count();
        if atoms.len() != operand_count {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        let mut operands = Vec::with_capacity(operand_count);
        for atom in atoms.iter().copied() {
            // An `infix_tail` operand is always an `atom`; a `clause_expr`
            // operand may also be a `call`, which is how a measure term
            // reaches a contract clause [MSR-5].
            let checked = self.check_written_operand(
                function,
                atom,
                bindings,
                loop_depth,
                super::super::expressions::PlaceUseContext::Ordinary,
            )?;
            operands.push((atom, checked));
        }
        self.check_integer_operation_operands(node, operation, operands)
    }

    /// The same [OP-1] row judgment over operands the caller has already
    /// checked. A `clause_expr` side is an `affine_expr` whose operands are
    /// not all `atom` nodes [MSR-5], so the row's typing is stated once here
    /// and reached from both the written-atom and the affine paths.
    pub(in crate::semantic::check) fn check_integer_operation_operands(
        &self,
        node: NodeId,
        operation: CheckedIntegerOperation,
        operands: Vec<(NodeId, TypedExpression)>,
    ) -> Result<TypedExpression, CheckStop> {
        let operand_count = operation.operand_count();
        if operands.len() != operand_count {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        let mut arguments = Vec::with_capacity(operand_count);
        let mut argument_metadata = Vec::with_capacity(operand_count);
        let mut effects = EffectSet::NONE;
        // [OP-2] the selected type is derived from the operands: the first
        // operand's exact type is it, and every later operand must be
        // exactly the row's argument type for that selection — which for the
        // two-operand arithmetic and comparison rows is the selected type
        // itself, so "both operands must have one identical exact type"
        // falls out and cites TYPE-5 at the second operand atom.
        let mut operand_type = None;
        for (index, (atom, argument)) in operands.into_iter().enumerate() {
            if argument.mode != CheckedMode::Own {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        format!("own {}", self.checked_type_name(argument.expression.ty())?),
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            let selected = match operand_type {
                Some(selected) => selected,
                None => {
                    let selected = argument.expression.ty();
                    if !operation.accepts_operand_type(selected) {
                        return self.issue_node(
                            SemanticRule::Op1,
                            node,
                            SemanticIssueKind::InvalidOperation,
                        );
                    }
                    operand_type = Some(selected);
                    selected
                }
            };
            if Some(argument.expression.ty()) != operation.argument_type(selected, index) {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        match operation.argument_type(selected, index) {
                            Some(ty) => format!("own {}", self.checked_type_name(ty)?),
                            None => format!("no operand in position {index} for this row"),
                        },
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            effects = effects.union(argument.effects);
            let source = match &argument.expression {
                CheckedExpression::NamedConstant { declaration, .. } => {
                    CheckedIntegerArgumentSource::NamedConstant {
                        declaration: *declaration,
                    }
                }
                CheckedExpression::Constant(_) => match self
                    .tree
                    .direct_token_with(atom, TerminalPredicate::Literal)?
                {
                    Some(literal) if matches!(self.tree.token_bytes(literal)?, b"0_T" | b"1_T") => {
                        CheckedIntegerArgumentSource::GenericNumericIdentity
                    }
                    Some(_) => CheckedIntegerArgumentSource::TypedLiteral,
                    None => CheckedIntegerArgumentSource::Other,
                },
                _ => CheckedIntegerArgumentSource::Other,
            };
            argument_metadata.push(CheckedIntegerArgument {
                node_path: self.tree.path(atom)?.clone(),
                source,
            });
            arguments.push(argument.expression);
        }
        // `operation_atoms` already rejected a wrong operand count, and no
        // integer row is nullary, so the selection is always made by here.
        let operand_type = operand_type.ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // The row's own `signature` cell decides this, so the mapping lives
        // once on the operation and an extraction lock compares it against the
        // specification's cell.
        let checked_error = operation.checked_error().map(|class| match class {
            CheckedIntegerErrorClass::Overflow => PreludeType::Overflow,
            CheckedIntegerErrorClass::DivError => PreludeType::DivError,
        });
        let result = if let Some(error) = checked_error {
            CheckedType::Nominal(self.prelude_nominal(PreludeType::Result(
                operand_type,
                CheckedType::Nominal(self.prelude_nominal(error)?),
            ))?)
        } else {
            operation
                .scalar_result_type(operand_type)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
        };
        Ok(TypedExpression::owned(
            CheckedExpression::IntegerOperation {
                carrier: self.tree.path(node)?.clone(),
                operation,
                operand_type,
                argument_metadata,
                arguments,
                result,
            },
            effects,
        ))
    }

    /// The written type pair of the conversion and reinterpretation rows.
    ///
    /// [DIAG-1] selects the cited rule by the callee's class rather than by
    /// the kind of argument problem: a table operation cites what [OP-2]
    /// selects and never FN-2, which belongs to a user-generic call. [TYPE-5]
    /// is what mandates these arguments, so their absence is its violation —
    /// the same reading [`Self::retained_operation_type_argument`] already
    /// applies to `finf` and `fnan`.
    fn numeric_type_arguments(
        &self,
        node: NodeId,
        function: &FunctionSignature,
        allow_symbolic: bool,
    ) -> Result<[CheckedNumericType; 2], CheckStop> {
        let targs = self.tree.argument_list(node)?.ok_or_else(|| {
            self.issue_value(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::InvalidOperation,
            )
        })?;
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let [source, destination] = arguments.as_slice() else {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        };
        let mut parsed = Vec::with_capacity(2);
        for argument in [*source, *destination] {
            let type_node = self
                .tree
                .first_child_with(argument, Production::Type)?
                .ok_or_else(|| {
                    self.issue_value(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation)
                })?;
            let ty = self.parse_type_with(type_node, &function.substitution)?;
            if !allow_symbolic
                && matches!(
                    ty,
                    CheckedType::GenericInt(_) | CheckedType::GenericFloat(_)
                )
            {
                return self.unsupported(UnsupportedSemanticFeature::Generics, type_node);
            }
            let Some(ty) = CheckedNumericType::from_type(ty) else {
                return self.issue_node(
                    SemanticRule::Op1,
                    node,
                    SemanticIssueKind::InvalidOperation,
                );
            };
            parsed.push(ty);
        }
        parsed
            .try_into()
            .map_err(|_| SemanticCompilerFailure::InvalidCanonicalTree.into())
    }

    fn check_boolean_operation(
        &self,
        node: NodeId,
        spelling: &str,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let operation = match spelling {
            "band" => CheckedBooleanOperation::And,
            "bor" => CheckedBooleanOperation::Or,
            "bxor" => CheckedBooleanOperation::ExclusiveOr,
            "bnot" => CheckedBooleanOperation::Not,
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        self.reject_named_operation_arguments(node, spelling)?;
        self.reject_written_operation_type_argument(node)?;
        // The Bool row has no type parameter to select: every operand is
        // checked against `Bool` below, which is the whole derivation.
        let expected = usize::from(operation != CheckedBooleanOperation::Not) + 1;
        let atoms = self.operation_atoms(node, expected)?;
        let mut arguments = Vec::with_capacity(atoms.len());
        let mut effects = EffectSet::NONE;
        for atom in atoms {
            let argument = self.check_atom(function, atom, bindings, loop_depth)?;
            if argument.expression.ty() != CheckedType::Bool || argument.mode != CheckedMode::Own {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        "own Bool",
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            effects = effects.union(argument.effects);
            arguments.push(argument.expression);
        }
        Ok(TypedExpression::owned(
            CheckedExpression::BooleanOperation {
                carrier: self.tree.path(node)?.clone(),
                operation,
                arguments,
            },
            effects,
        ))
    }

    fn check_enum_equality(
        &self,
        node: NodeId,
        equal: bool,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let spelling = if equal { "eeq" } else { "ene" };
        self.reject_named_operation_arguments(node, spelling)?;
        self.reject_written_operation_type_argument(node)?;
        let atoms = self.operation_atoms(node, 2)?;
        // [OP-2] the selected tag-only nominal is the first operand's exact
        // type; the second is then checked against it.
        let first = self.check_atom(function, atoms[0], bindings, loop_depth)?;
        if first.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Type5,
                atoms[0],
                SemanticIssueKind::type_mismatch(
                    format!("own {}", self.checked_type_name(first.expression.ty())?),
                    self.checked_value_name(first.mode, first.expression.ty())?,
                ),
            );
        }
        let operand_type = first.expression.ty();
        let tag_only = match operand_type {
            CheckedType::Bool => true,
            CheckedType::Nominal(id) => matches!(
                &self.nominal(id)?.kind,
                CheckedNominalKind::Enum { variants }
                    if variants.iter().all(|variant| variant.fields.is_empty())
            ),
            _ => false,
        };
        if !tag_only {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        // The first operand is already checked, and checking an atom can
        // consume its place, so only the remaining one is checked here.
        let mut effects = first.effects;
        let mut arguments = vec![first.expression];
        for atom in &atoms[1..] {
            let argument = self.check_atom(function, *atom, bindings, loop_depth)?;
            if argument.expression.ty() != operand_type || argument.mode != CheckedMode::Own {
                return self.issue_node(
                    SemanticRule::Type5,
                    *atom,
                    SemanticIssueKind::type_mismatch(
                        format!("own {}", self.checked_type_name(operand_type)?),
                        self.checked_value_name(argument.mode, argument.expression.ty())?,
                    ),
                );
            }
            effects = effects.union(argument.effects);
            arguments.push(argument.expression);
        }
        Ok(TypedExpression::owned(
            CheckedExpression::EnumEquality {
                carrier: self.tree.path(node)?.clone(),
                equal,
                operand_type,
                arguments,
            },
            effects,
        ))
    }

    /// [TYPE-5] every table operation outside the closed retained-argument
    /// class carries no written type argument, because its operands supply
    /// the selected type. [OP-2] a written one is a hard error citing OP-1.
    pub(in crate::semantic::check) fn reject_written_operation_type_argument(
        &self,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        if self.tree.argument_list(node)?.is_some() {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        Ok(())
    }

    /// Reads the single written type argument of a retained-argument table
    /// operation. [TYPE-5] keeps these exactly where no operand can supply
    /// the type — here, `finf` and `fnan`, whose rows are nullary.
    pub(in crate::semantic::check) fn retained_operation_type_argument(
        &self,
        node: NodeId,
        function: &FunctionSignature,
    ) -> Result<CheckedType, CheckStop> {
        let targs = self.tree.argument_list(node)?.ok_or_else(|| {
            self.issue_value(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::InvalidOperation,
            )
        })?;
        let targs = self.tree.children_with(targs, Production::Targ)?;
        if targs.len() != 1 {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        let ty = self
            .tree
            .first_child_with(targs[0], Production::Type)?
            .ok_or_else(|| {
                self.issue_value(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation)
            })?;
        self.parse_type_with(ty, &function.substitution)
    }

    /// The [GRAM-11] half of the old type-argument reader: a table operation
    /// takes positional atom operands, never named arguments.
    pub(in crate::semantic::check) fn reject_named_operation_arguments(
        &self,
        node: NodeId,
        spelling: &str,
    ) -> Result<(), CheckStop> {
        if self
            .tree
            .first_child_with(node, Production::FieldinitList)?
            .is_some()
        {
            return self.issue_node(
                SemanticRule::Gram11,
                node,
                SemanticIssueKind::InvalidNamedArguments {
                    callee: spelling.to_owned(),
                    declared_parameters: Vec::new(),
                },
            );
        }
        Ok(())
    }

    fn invalid_named_arguments(signature: &FunctionSignature) -> SemanticIssueKind {
        SemanticIssueKind::InvalidNamedArguments {
            callee: signature.name.clone(),
            declared_parameters: signature
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        }
    }

    pub(in crate::semantic::check) fn operation_atoms(
        &self,
        node: NodeId,
        expected: usize,
    ) -> Result<Vec<NodeId>, CheckStop> {
        let Some(list) = self.tree.first_child_with(node, Production::AtomList)? else {
            if expected == 0 {
                return Ok(Vec::new());
            }
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        };
        let atoms = self.tree.children_with(list, Production::Atom)?;
        if atoms.len() < expected {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        if atoms.len() > expected {
            return self.issue_node(
                SemanticRule::Op1,
                atoms[expected],
                SemanticIssueKind::InvalidOperation,
            );
        }
        Ok(atoms)
    }
}

/// The [MSR-1] measure one operation spelling names, if it names one.
///
/// The four spellings are one operation family over one place [OP-1]; this is
/// the selection of the row within it and the only place a spelling reaches a
/// measure.
pub(in crate::semantic::check) const fn measure_former(spelling: &str) -> Option<CheckedMeasure> {
    match spelling.as_bytes() {
        b"len_of" => Some(CheckedMeasure::Length),
        b"cap_of" => Some(CheckedMeasure::Capacity),
        b"head_of" => Some(CheckedMeasure::Head),
        _ => None,
    }
}
