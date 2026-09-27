//! Formation recipes and discovery effects of a retained structural body.
//! Missing identities use the ordinary producers in a staged type inventory.

use super::*;

#[derive(Clone, Default)]
pub(in crate::semantic::check) struct DiscoveryRequests {
    functions: Vec<FunctionId>,
    nominals: Vec<NominalId>,
    requests: Vec<(NodeId, GenericSubstitution, NodeId)>,
    bindings: Vec<behavior::BindingSite>,
}

impl TypeContext<'_> {
    pub(in crate::semantic::check) fn record_product_function(&self, id: FunctionId) {
        if let Some(requests) = self.product_discovery.borrow_mut().as_mut()
            && !requests.functions.contains(&id)
        {
            requests.functions.push(id);
        }
    }
    pub(in crate::semantic::check) fn record_product_nominal(&self, id: NominalId) {
        if let Some(requests) = self.product_discovery.borrow_mut().as_mut()
            && !requests.nominals.contains(&id)
        {
            requests.nominals.push(id);
        }
    }
    pub(in crate::semantic::check) fn record_product_request(
        &self,
        template: NodeId,
        substitution: &GenericSubstitution,
        call: NodeId,
    ) {
        if let Some(requests) = self.product_discovery.borrow_mut().as_mut() {
            requests
                .requests
                .push((template, substitution.clone(), call));
        }
    }
    pub(in crate::semantic::check) fn record_product_binding(&self, site: behavior::BindingSite) {
        if let Some(requests) = self.product_discovery.borrow_mut().as_mut() {
            requests.bindings.push(site);
        }
    }
}

enum Recipe {
    Function(
        DeclarationId,
        GenericSubstitution,
        Option<generics::GenericParameterKey>,
    ),
    SourceNominal(DeclarationId, GenericSubstitution),
    PreludeNominal(PreludeType),
    BoxNominal(CheckedType),
    ResultList(Vec<(String, CheckedType)>),
    Element(CheckedType),
    DerivedConst(DerivedConst),
    Reference(behavior::FunctionReference),
}
record_enum!(Recipe {
    0 => Function(declaration, substitution, formal),
    1 => SourceNominal(declaration, substitution),
    2 => PreludeNominal(value),
    3 => BoxNominal(referent),
    4 => ResultList(fields),
    5 => Element(ty),
    6 => DerivedConst(value),
    7 => Reference(value),
});

#[derive(Default)]
pub(super) struct DiscoveryProduct {
    functions: Vec<FunctionId>,
    nominals: Vec<NominalId>,
    requests: Vec<(NodeId, GenericSubstitution, NodeId)>,
    selectors: Vec<CheckedPostconditionSelector>,
    schema_instances: Vec<FunctionId>,
    declaration_arguments: Vec<behavior::FunctionArgument>,
    unavailable: Vec<DeclarationId>,
    binding_sites: Vec<behavior::BindingSite>,
}
record_struct!(DiscoveryProduct {
    functions,
    nominals,
    requests,
    selectors,
    schema_instances,
    declaration_arguments,
    unavailable,
    binding_sites
});

impl DiscoveryProduct {
    pub(super) fn covers_formation(&self, prior: &InventoryView, formed: &InventoryView) -> bool {
        formed
            .functions
            .iter()
            .all(|id| prior.contains_function(*id) || self.functions.contains(id))
            && formed
                .nominals
                .iter()
                .all(|id| prior.contains_nominal(*id) || self.nominals.contains(id))
    }
}

impl Checker<'_, '_> {
    pub(super) fn retained_discovery(
        &self,
        before: &DiscoveryState,
        mut requested: DiscoveryRequests,
    ) -> DiscoveryProduct {
        for id in &self.types.view.functions[before.functions..] {
            if !requested.functions.contains(id) {
                requested.functions.push(*id);
            }
        }
        for id in &self.types.view.nominals[before.visible_nominals..] {
            if !requested.nominals.contains(id) {
                requested.nominals.push(*id);
            }
        }
        let selectors = self
            .analysis
            .postcondition_selectors
            .iter()
            .enumerate()
            .filter(|(index, selector)| {
                *index >= before.selectors || requested.functions.contains(&selector.function)
            })
            .map(|(_, selector)| selector.clone())
            .collect();
        DiscoveryProduct {
            functions: requested.functions,
            nominals: requested.nominals,
            requests: requested.requests,
            selectors,
            schema_instances: self.analysis.schema_written_instances[before.schema_instances..]
                .to_vec(),
            declaration_arguments: self.types.behavior.declaration_arguments
                [before.declaration_arguments..]
                .to_vec(),
            unavailable: self.analysis.postcondition_unavailable_declarations[before.unavailable..]
                .to_vec(),
            binding_sites: requested.bindings,
        }
    }

    pub(super) fn import_discovery(&mut self, discovery: DiscoveryProduct) {
        for id in discovery.functions {
            if self.types.view.add_function(id) {
                let signature = &mut self.types.signatures[id.0 as usize];
                signature.declared_effects.allocates =
                    generics::HEAP_ALLOCATING_PRELUDE_FUNCTIONS.contains(&signature.name.as_str());
            }
        }
        for id in discovery.nominals {
            if self.types.view.add_nominal(id) {
                self.types.nominal_layouts_acyclic_at = None;
            }
        }
        for (template, substitution, call) in discovery.requests {
            self.types
                .record_instance_request(template, &substitution, call);
        }
        for selector in discovery.selectors {
            if !self.analysis.postcondition_selectors.contains(&selector) {
                self.analysis.postcondition_selectors.push(selector);
            }
        }
        self.analysis
            .schema_written_instances
            .extend(discovery.schema_instances);
        for argument in discovery.declaration_arguments {
            if !self
                .types
                .behavior
                .declaration_arguments
                .contains(&argument)
            {
                self.types.behavior.declaration_arguments.push(argument);
            }
        }
        for declaration in discovery.unavailable {
            self.analysis.mark_postcondition_unavailable(declaration);
        }
        self.types
            .behavior
            .import_binding_sites(discovery.binding_sites);
    }

    pub(super) fn identity_recipe(&self, (kind, index): Identity) -> Option<Writer> {
        let index = index as usize;
        let recipe = match kind {
            IdentityKind::Function => {
                let signature = self.types.signatures.get(index)?;
                Recipe::Function(
                    signature.declaration,
                    signature.substitution.clone(),
                    signature.formal_parameter,
                )
            }
            IdentityKind::Nominal => {
                let id = NominalId(index as u32);
                if let Some((template, substitution)) =
                    self.types.source_nominal_instance_entry(id).ok()?
                {
                    Recipe::SourceNominal(
                        self.types.nominal_templates.get(template)?.declaration,
                        substitution.clone(),
                    )
                } else if let Some(prelude) = self.types.prelude_types.get(index)? {
                    Recipe::PreludeNominal(*prelude)
                } else if let Some((referent, _)) = self
                    .types
                    .box_nominals
                    .iter()
                    .find(|(_, candidate)| **candidate == id)
                {
                    Recipe::BoxNominal(*referent)
                } else {
                    let (fields, _) = self
                        .types
                        .result_list_nominals
                        .iter()
                        .find(|(_, candidate)| **candidate == id)?;
                    Recipe::ResultList(fields.clone())
                }
            }
            IdentityKind::Element => Recipe::Element(*self.types.elements.get(index)?),
            IdentityKind::DerivedConst => {
                Recipe::DerivedConst(*self.types.derived_consts.get(index)?)
            }
            IdentityKind::FunctionReference => Recipe::Reference(
                self.types
                    .function_reference(FunctionReferenceId::from_index(index as u32))
                    .ok()?,
            ),
            _ => return Some(Writer::default()),
        };
        let mut writer = Writer::default();
        recipe.write(&mut writer);
        Some(writer)
    }

    pub(super) fn form_retained_identity(
        &mut self,
        context: &CheckContext<'_>,
        bytes: &[u8],
        mapping: &IdentityMap,
    ) -> Option<Identity> {
        let mut reader = Reader::new(bytes, mapping);
        let recipe = Recipe::read(&mut reader)?;
        if !reader.finished() {
            return None;
        }
        Some(match recipe {
            Recipe::Function(declaration, substitution, formal) => {
                let id = if let Some(id) =
                    self.types
                        .function_instance(declaration, &substitution, formal)
                {
                    id
                } else {
                    if formal.is_some() {
                        return None;
                    }
                    let template = *self.types.templates_by_declaration.get(&declaration)?;
                    self.instantiate_function_signature(context, template, substitution)
                        .ok()?
                };
                (IdentityKind::Function, id.0)
            }
            Recipe::SourceNominal(declaration, substitution) => {
                let template = *self
                    .types
                    .nominal_templates_by_declaration
                    .get(&declaration)?;
                let id = self
                    .ensure_source_nominal_instance(context, template, substitution)
                    .ok()?;
                (IdentityKind::Nominal, id.0)
            }
            Recipe::PreludeNominal(ty) => (
                IdentityKind::Nominal,
                self.types.intern_prelude_nominal(ty).ok()?.0,
            ),
            Recipe::BoxNominal(ty) => (
                IdentityKind::Nominal,
                self.types.intern_box_nominal(ty).ok()?.0,
            ),
            Recipe::ResultList(fields) => (
                IdentityKind::Nominal,
                self.types.intern_result_list_nominal(&fields).ok()?.0,
            ),
            Recipe::Element(ty) => (IdentityKind::Element, self.types.intern_element(ty).ok()?.0),
            Recipe::DerivedConst(value) => {
                let CheckedConst::Derived(id) =
                    self.types
                        .combine_const(value.operation, value.left, value.right)?
                else {
                    return None;
                };
                (IdentityKind::DerivedConst, id.0)
            }
            Recipe::Reference(value) => {
                let behavior::FunctionArgument::Source { reference, .. } = self
                    .types
                    .intern_function_reference(value.declaration, &value.substitution)
                    .ok()?
                else {
                    return None;
                };
                let mut writer = Writer::default();
                reference.write(&mut writer);
                *writer.identities.first()?
            }
        })
    }
}
