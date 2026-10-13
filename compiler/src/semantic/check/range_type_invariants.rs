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
            contract_reads: None,
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

    /// [TYPE-11] each parameter's and result ordinal's range type invariants,
    /// read on its type after the function instance's substitution [FN-2]
    /// exactly as the affine ones are: a range requirement of each parameter,
    /// and an unrouted range postcondition over each written reference
    /// parameter's exit state and each result ordinal [RANGE-2]. A callable
    /// whose definition the build supplies publishes no postcondition, as no
    /// body proves one.
    pub(super) fn append_range_type_invariant_contracts(
        &mut self,
        signature: &FunctionSignature,
        parameters: &[CheckedParameter],
    ) -> Result<(), CheckStop> {
        let results: Vec<CheckedType> = if signature.results.len() > 1 {
            signature.results.iter().map(|result| result.ty).collect()
        } else {
            Vec::new()
        };
        let publishes = !self.supplied_definition(signature)?;
        for (parameter, checked) in signature.parameters.iter().zip(parameters) {
            let Some(nominal) = self.declared_invariant_struct(parameter) else {
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
                if publishes
                    && parameter.mode == CheckedMode::Reference
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
            if !publishes || result.mode != CheckedMode::Own {
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
