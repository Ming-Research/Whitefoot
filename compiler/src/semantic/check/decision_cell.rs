//! Signature-owned reachability of one-shot decision cells [SHARE-3].

use std::collections::HashSet;

use super::super::model::{CheckedNominalKind, CheckedType, FunctionId, NominalId};
use super::{CheckStop, TypeContext};

impl TypeContext<'_> {
    pub(super) fn is_decision_cell(&self, id: NominalId) -> Result<bool, CheckStop> {
        let Some((template, _)) = self
            .source_nominal_instances
            .get(id.0 as usize)
            .and_then(Option::as_ref)
        else {
            return Ok(false);
        };
        let template = &self.nominal_templates[*template];
        Ok(template.name == "DecisionCell"
            && self
                .declarations
                .is_prelude_opaque_declaration(template.node)?)
    }

    /// Reads only the callable boundary, including concrete function-kind
    /// arguments. A nominal/function cycle is visited once; no callee body
    /// supplies authority to wait, overlap, or stabilize a cell observation.
    pub(super) fn decision_cell_boundary(&self, root: FunctionId) -> Result<bool, CheckStop> {
        let mut functions = vec![root];
        let mut seen_functions = HashSet::new();
        let mut seen_types = HashSet::new();
        while let Some(function) = functions.pop() {
            if !seen_functions.insert(function) {
                continue;
            }
            let signature = &self.signatures[function.0 as usize];
            let mut pending = signature
                .parameters
                .iter()
                .map(|parameter| parameter.ty)
                .collect::<Vec<_>>();
            pending.extend(signature.results.iter().map(|result| result.ty));
            for argument in signature.substitution.function_arguments() {
                functions.push(self.function_argument_instance(argument)?);
            }
            while let Some(ty) = pending.pop() {
                if !seen_types.insert(ty) {
                    continue;
                }
                match ty {
                    CheckedType::Nominal(id) => {
                        if self.is_decision_cell(id)? {
                            return Ok(true);
                        }
                        match &self.nominal(id)?.kind {
                            CheckedNominalKind::Struct { fields } => {
                                pending.extend(fields.iter().map(|f| f.ty));
                            }
                            CheckedNominalKind::Enum { variants } => {
                                pending.extend(
                                    variants.iter().flat_map(|v| &v.fields).map(|f| f.ty),
                                );
                            }
                            CheckedNominalKind::Box { referent, .. } => pending.push(*referent),
                            CheckedNominalKind::Shared { state, .. } => pending.push(*state),
                            CheckedNominalKind::Opaque => {}
                        }
                    }
                    CheckedType::Array { element, .. }
                    | CheckedType::Window { element, .. }
                    | CheckedType::Buffer { element }
                    | CheckedType::Segments { element }
                    | CheckedType::Entries { element } => pending.push(self.element_type(element)?),
                    // Symbolic source validation is repeated with concrete types
                    // before it publishes an executable call or permission.
                    _ => {}
                }
            }
        }
        Ok(false)
    }
}
