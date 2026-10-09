//! TYPE-11 range templates, formed once and substituted at each callable site.
use super::super::range_facts::{CheckedRangeClause, CheckedRangePostcondition, CheckedRangeRoot};
use super::*;

impl Checker<'_, '_> {
    pub(super) fn form_range_type_invariant(
        &mut self,
        check_context: &CheckContext<'_>,
        declaration: DeclarationId,
        invariant: NodeId,
        range: NodeId,
        nominal: NominalId,
    ) -> Result<CheckedRangeClause, CheckStop> {
        let binder = self
            .types
            .declarations
            .declaration_at(invariant, DeclarationRole::InvariantBinder)?;
        let parameter = ParameterSignature {
            declaration: binder.id(),
            node_path: self.types.declarations.tree.path(invariant)?.clone(),
            name: binder.spelling().to_owned(),
            mode: CheckedMode::Reference,
            ty: CheckedType::Nominal(nominal),
        };
        let signature = FunctionSignature {
            id: FunctionId(u32::MAX),
            declaration: binder.id(),
            node: invariant,
            name: String::new(),
            symbol: String::new(),
            region_parameters: Vec::new(),
            parameters: vec![parameter.clone()],
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            results: Vec::new(),
            result_list: None,
            effects_node: invariant,
            declared_effects: EffectSet::default(),
            waits: false,
            formal_parameter: None,
            substitution: GenericSubstitution::default(),
        };
        let bindings = HashMap::from([(
            binder.id(),
            Checker::parameter_local(&parameter, BindingId(0))?,
        )]);
        let check_context = CheckContext {
            writing_module: self
                .types
                .declarations
                .resolved
                .declaration(declaration)
                .and_then(crate::DeclarationRecord::module),
            ..*check_context
        };
        self.check_range_clause(
            FunctionContext {
                check_context: &check_context,
                function: &signature,
            },
            range,
            &bindings,
        )?
        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree.into())
    }

    pub(super) fn append_range_type_invariant_contracts(
        &mut self,
        check_context: &CheckContext<'_>,
        signature: &FunctionSignature,
        parameters: &[CheckedParameter],
    ) -> Result<(), CheckStop> {
        let results: Vec<CheckedType> = if signature.results.len() > 1 {
            signature.results.iter().map(|result| result.ty).collect()
        } else {
            Vec::new()
        };
        let shared_new = signature.name == "shared_new"
            && self
                .types
                .declarations
                .tree
                .is_prelude_node(signature.node)?;
        for (parameter, checked) in signature.parameters.iter().zip(parameters) {
            let nominal = self.declared_invariant_struct(check_context, parameter)?;
            // shared_new's T is supplied by its operand rather than written in
            // its signature. TYPE-11 still owes the argument's invariant.
            let nominal = nominal.or_else(|| {
                (shared_new && parameters.first() == Some(checked))
                    .then_some(parameter.ty)
                    .and_then(|ty| match ty {
                        CheckedType::Nominal(id) => Some(id),
                        _ => None,
                    })
            });
            let Some(nominal) = nominal else {
                continue;
            };
            let Some(clauses) = self.types.range_type_invariants.get(&nominal) else {
                continue;
            };
            for clause in clauses {
                let clause = clause.with_subject(
                    CheckedRangeRoot::Binding(checked.binding),
                    parameter.mode == CheckedMode::Reference,
                );
                self.body.range_facts.requirements.push(clause.clone());
                if parameter.mode == CheckedMode::Reference
                    && super::ensures::parameter_has_exit_state(signature, parameter)
                {
                    self.body
                        .range_facts
                        .postconditions
                        .push(CheckedRangePostcondition {
                            clause,
                            route: None,
                            results: results.clone(),
                            owed: true,
                        });
                }
            }
        }
        for (ordinal, result) in signature.results.iter().enumerate() {
            let CheckedType::Nominal(nominal) = result.ty else {
                continue;
            };
            if result.mode != CheckedMode::Own {
                continue;
            }
            let Some(ty) = self
                .types
                .declarations
                .tree
                .first_child_with(result.rtype, Production::Type)?
            else {
                continue;
            };
            if self
                .types
                .written_invariant_struct(check_context, ty, nominal)?
                .is_none()
            {
                continue;
            }
            for clause in self
                .types
                .range_type_invariants
                .get(&nominal)
                .into_iter()
                .flatten()
            {
                self.body
                    .range_facts
                    .postconditions
                    .push(CheckedRangePostcondition {
                        clause: clause
                            .with_subject(CheckedRangeRoot::Result(ordinal as u32), false),
                        route: None,
                        results: results.clone(),
                        owed: true,
                    });
            }
        }
        Ok(())
    }
}
