//! Ordered checking views over identities retained for the whole check.
//!
//! A view selects availability and deterministic discovery order. Leaving a
//! view never removes an interned identity or makes an old handle mean a new
//! type. Source fields are activated at completion, after declaration heads.

use std::collections::HashSet;

use super::generics::{GenericArgument, GenericSubstitution};
use super::{
    CheckContext, CheckStop, CheckedFunctionInventory, CheckedNominalKind, CheckedType, Checker,
    FunctionId, FunctionSignature, NominalId, TypeContext,
};
use crate::SemanticCompilerFailure;

#[derive(Clone, Default)]
pub(super) struct InventoryView {
    pub(super) nominals: Vec<NominalId>,
    nominal_members: HashSet<NominalId>,
    pub(super) functions: Vec<FunctionId>,
    function_members: HashSet<FunctionId>,
}

impl InventoryView {
    pub(super) fn contains_nominal(&self, id: NominalId) -> bool {
        self.nominal_members.contains(&id)
    }

    pub(super) fn add_nominal(&mut self, id: NominalId) -> bool {
        if !self.nominal_members.insert(id) {
            return false;
        }
        self.nominals.push(id);
        true
    }

    pub(super) fn contains_function(&self, id: FunctionId) -> bool {
        self.function_members.contains(&id)
    }

    pub(super) fn clear_functions(&mut self) {
        self.functions.clear();
        self.function_members.clear();
    }

    pub(super) fn add_function(&mut self, id: FunctionId) -> bool {
        if !self.function_members.insert(id) {
            return false;
        }
        self.functions.push(id);
        true
    }
}

impl TypeContext<'_> {
    pub(super) fn function_instance(
        &self,
        declaration: crate::DeclarationId,
        substitution: &GenericSubstitution,
        formal: Option<super::generics::GenericParameterKey>,
    ) -> Option<FunctionId> {
        self.functions_by_declaration
            .get(&declaration)?
            .iter()
            .copied()
            .find(|id| {
                self.signatures.get(id.0 as usize).is_some_and(|signature| {
                    signature.substitution == *substitution && signature.formal_parameter == formal
                })
            })
    }

    pub(super) fn activate_function(&mut self, id: FunctionId) -> Result<(), CheckStop> {
        if !self.view.add_function(id) {
            return Ok(());
        }
        let signature = self
            .signatures
            .get(id.0 as usize)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.activate_substitution(&signature.substitution)?;
        for parameter in &signature.parameters {
            self.activate_type(parameter.ty)?;
        }
        for result in &signature.results {
            self.activate_type(result.ty)?;
        }
        self.activate_type(signature.result)?;
        // Allocation closure is a judgment product. A newly selected view
        // starts from the declaration's own base fact, then closes its bodies.
        self.signatures[id.0 as usize].declared_effects.allocates =
            super::generics::HEAP_ALLOCATING_PRELUDE_FUNCTIONS.contains(&signature.name.as_str());
        Ok(())
    }

    pub(super) fn retain_signature(
        &mut self,
        signature: FunctionSignature,
    ) -> Result<FunctionId, CheckStop> {
        let id = signature.id;
        if id.0 as usize != self.signatures.len()
            || self
                .function_instance(
                    signature.declaration,
                    &signature.substitution,
                    signature.formal_parameter,
                )
                .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        self.functions_by_declaration
            .entry(signature.declaration)
            .or_default()
            .push(id);
        self.signatures.push(signature);
        self.activate_function(id)?;
        Ok(id)
    }

    pub(super) fn select_view(&mut self, view: InventoryView) {
        self.view = view;
        self.nominal_layouts_acyclic_at = None;
    }

    /// A declared source head precedes its fields, including a recursive Box
    /// edge. A derived type follows its arguments. These are the construction
    /// orders, even when another checking view already interned the identity.
    pub(super) fn activate_nominal(&mut self, id: NominalId) -> Result<(), CheckStop> {
        if self.view.contains_nominal(id) {
            return Ok(());
        }
        if let Some((_, substitution)) = self.source_nominal_instance_entry(id)? {
            let substitution = substitution.clone();
            self.activate_substitution(&substitution)?;
            self.view.add_nominal(id);
            self.nominal_layouts_acyclic_at = None;
            self.activate_nominal_fields(id)?;
        } else {
            self.activate_nominal_fields(id)?;
            self.view.add_nominal(id);
            self.nominal_layouts_acyclic_at = None;
        }
        Ok(())
    }

    pub(super) fn activate_nominal_fields(&mut self, id: NominalId) -> Result<(), CheckStop> {
        let fields = match &self.nominal(id)?.kind {
            CheckedNominalKind::Struct { fields } => fields.iter().map(|field| field.ty).collect(),
            CheckedNominalKind::Enum { variants } => variants
                .iter()
                .flat_map(|variant| &variant.fields)
                .map(|field| field.ty)
                .collect(),
            CheckedNominalKind::Box { referent, .. } => vec![*referent],
            CheckedNominalKind::Opaque => Vec::new(),
        };
        for ty in fields {
            self.activate_type(ty)?;
        }
        Ok(())
    }

    pub(super) fn activate_type(&mut self, ty: CheckedType) -> Result<(), CheckStop> {
        match ty {
            CheckedType::Nominal(id) => self.activate_nominal(id),
            CheckedType::Array { element, .. }
            | CheckedType::Buffer { element }
            | CheckedType::Segments { element }
            | CheckedType::Window { element, .. } => {
                self.activate_type(self.element_type(element)?)
            }
            _ => Ok(()),
        }
    }

    pub(super) fn activate_substitution(
        &mut self,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        for (_, argument) in substitution.entries() {
            match argument {
                GenericArgument::Type(ty) => self.activate_type(*ty)?,
                GenericArgument::Function(super::behavior::FunctionArgument::Source {
                    reference,
                    ..
                }) => {
                    let reference = self.function_reference(*reference)?;
                    self.activate_substitution(&reference.substitution)?;
                }
                GenericArgument::Const(_)
                | GenericArgument::Function(super::behavior::FunctionArgument::Parameter(_)) => {}
            }
        }
        Ok(())
    }
}

impl Checker<'_, '_> {
    /// Close the selected view in discovery order, retaining judgments of the
    /// other view by stable identity. Selected bodies are checked afresh:
    /// their selector universe and consumed callee facts can have changed.
    pub(super) fn check_function_view(
        &mut self,
        context: &CheckContext<'_>,
        prior: Vec<CheckedFunctionInventory>,
    ) -> Result<Vec<CheckedFunctionInventory>, CheckStop> {
        let mut functions = prior.into_iter().map(Some).collect::<Vec<_>>();
        let mut cursor = 0;
        while cursor < self.types.view.functions.len() {
            let id = self.types.view.functions[cursor];
            let checked = self.check_function(context, id.0 as usize)?;
            functions.resize_with(self.types.signatures.len(), || None);
            functions[id.0 as usize] = Some(checked);
            cursor += 1;
        }
        functions
            .into_iter()
            .map(|function| {
                function.ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
            })
            .collect()
    }
}
