//! Retained structural module products, imported before composition judgments.
//!
//! This adapter uses the one checked-body representation. An incomplete
//! identity table or a changed input is a miss and invokes ordinary checking.

use std::collections::{BTreeMap, BTreeSet};

use super::behavior::FunctionReferenceId;
use super::*;
use crate::semantic::products::identity::SourceIdentities;
use crate::semantic::products::{
    Identity, IdentityKind, IdentityMap, Reader, Record, Writer, record_enum, record_struct,
};

/// Source inputs and storage supplied by the module driver, separate from
/// proof receipts and target-dependent lowering products.
pub(crate) trait ModuleProducts {
    fn source_key(&self, module: crate::ModuleId) -> Option<&[u8]>;
    fn load_body(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn store_body(&self, key: &[u8], bytes: &[u8]);
    fn body_checked(&self, module: Option<crate::ModuleId>);
    fn header_checked(&self);
    fn body_reused(&self, module: Option<crate::ModuleId>);
}

record_struct!(ParameterSignature {
    declaration,
    node_path,
    name,
    mode,
    ty
});
record_struct!(ResultSignature { mode, ty, rtype });
record_struct!(FunctionSignature {
    id,
    declaration,
    node,
    name,
    symbol,
    region_parameters,
    parameters,
    result_mode,
    result,
    results,
    result_list,
    effects_node,
    declared_effects,
    formal_parameter,
    substitution,
});
record_struct!(EffectSet {
    reads,
    writes,
    allocates
});
record_struct!(CheckedFunctionInventory {
    function,
    binding_names
});
record_enum!(PreludeType {
    0 => Option(ty),
    1 => Result(ok, error),
    2 => Overflow,
    3 => DivError,
    4 => NarrowError,
});

/// One identity catalogue per checking view; identities are retained as that
/// view discovers new instances. Source lookups and structural names are
/// shared by every imported body.
pub(super) struct ProductIdentities<'a> {
    sources: SourceIdentities<'a>,
    names: std::cell::RefCell<BTreeMap<Identity, Vec<u8>>>,
    current: std::cell::RefCell<BTreeMap<Vec<u8>, Identity>>,
    counts: std::cell::RefCell<BTreeMap<IdentityKind, usize>>,
}

impl<'a> ProductIdentities<'a> {
    pub(super) fn new(declarations: &'a DeclarationInventory<'a>) -> Option<Self> {
        Some(Self {
            sources: SourceIdentities::new(declarations.resolved, &declarations.tree)?,
            names: Default::default(),
            current: Default::default(),
            counts: Default::default(),
        })
    }
}

/// The part of body checking whose publication is independent of entailment.
struct BodyProduct {
    checked: CheckedFunctionInventory,
    written_effects: Option<EffectSet>,
}
record_struct!(BodyProduct {
    checked,
    written_effects
});

/// Inventory lengths whose change means this body discovered more products.
/// Such a body is checked normally until its discovery products can also be
/// imported. No metadata is silently dropped on a structural hit.
#[derive(Eq, PartialEq)]
struct DiscoveryState {
    nominals: usize,
    nominal_generation: u64,
    elements: usize,
    signatures: usize,
    derived_consts: usize,
    requests: usize,
    functions: usize,
    visible_nominals: usize,
    queries: usize,
    tail_rejections: usize,
    selectors: usize,
    schema_instances: usize,
    declaration_arguments: usize,
    unavailable: usize,
    behavior: (usize, usize),
}

impl Checker<'_, '_> {
    fn discovery_state(&self) -> DiscoveryState {
        DiscoveryState {
            nominals: self.types.nominals.len(),
            nominal_generation: self.types.nominal_generation,
            elements: self.types.elements.len(),
            signatures: self.types.signatures.len(),
            derived_consts: self.types.derived_consts.len(),
            requests: self.types.instance_requests.len(),
            functions: self.types.view.functions.len(),
            visible_nominals: self.types.view.nominals.len(),
            queries: self.analysis.contract_queries.len(),
            tail_rejections: self.analysis.musttail_rejections.len(),
            selectors: self.analysis.postcondition_selectors.len(),
            schema_instances: self.analysis.schema_written_instances.len(),
            declaration_arguments: self.types.behavior.declaration_arguments.len(),
            unavailable: self.analysis.postcondition_unavailable_declarations.len(),
            behavior: self.types.behavior.product_counts(),
        }
    }

    pub(super) fn check_retained_function(
        &mut self,
        context: &CheckContext<'_>,
        index: usize,
        identities: Option<&ProductIdentities<'_>>,
    ) -> Result<CheckedFunctionInventory, CheckStop> {
        let Some(products) = self.receipts.and_then(|receipts| receipts.products()) else {
            return self.check_function(context, index);
        };
        let Some(identities) = identities else {
            return self.check_function(context, index);
        };
        let module = self
            .types
            .signatures
            .get(index)
            .and_then(|signature| {
                self.types
                    .declarations
                    .resolved
                    .declaration(signature.declaration)
            })
            .and_then(crate::DeclarationRecord::module);
        let signature = self
            .types
            .signatures
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if self.types.declarations.tree.is_body_less(signature.node)? {
            products.header_checked();
            return self.check_function(context, index);
        }
        let key = self.body_product_key(index, products, identities);
        if let Some(key) = &key
            && let Some(bytes) = products.load_body(key)
            && let Some(body) = self.read_body_product(&bytes, identities)
        {
            if let Some(effects) = body.written_effects {
                self.analysis
                    .written_body_effect_rows
                    .insert(body.checked.function.declaration, effects);
            }
            products.body_reused(module);
            return Ok(body.checked);
        }
        let before = self.discovery_state();
        products.body_checked(module);
        let checked = self.check_function(context, index)?;
        if before == self.discovery_state()
            && let Some(key) = key
        {
            let product = BodyProduct {
                written_effects: self
                    .analysis
                    .written_body_effect_rows
                    .get(&checked.function.declaration)
                    .cloned(),
                checked,
            };
            if let Some(bytes) = self.write_body_product(&product, identities) {
                products.store_body(&key, &bytes);
            }
            return Ok(product.checked);
        }
        Ok(checked)
    }

    fn body_product_key(
        &self,
        index: usize,
        products: &dyn ModuleProducts,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        let signature = self.types.signatures.get(index)?;
        let module = self
            .types
            .declarations
            .resolved
            .declaration(signature.declaration)?
            .module()?;
        let mut writer = Writer::default();
        b"structural-body 1".to_vec().write(&mut writer);
        products.source_key(module)?.to_vec().write(&mut writer);
        self.identity_name((IdentityKind::Function, signature.id.0), identities)?
            .write(&mut writer);
        // The symbolic and ordinary selector universes can differ even for
        // a nongeneric function, so they are part of its structural inputs.
        let selectors = self
            .analysis
            .postcondition_selectors
            .iter()
            .filter(|selector| selector.function == signature.id)
            .cloned()
            .collect::<Vec<_>>();
        self.canonical_record(&selectors, identities)?
            .write(&mut writer);
        self.canonical_record(
            &self
                .analysis
                .written_body_effect_rows
                .get(&signature.declaration)
                .cloned(),
            identities,
        )?
        .write(&mut writer);
        self.canonical_record(signature, identities)?
            .write(&mut writer);
        Some(writer.bytes)
    }

    fn canonical_record(
        &self,
        value: &impl Record,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        let mut writer = Writer::default();
        value.write(&mut writer);
        writer.canonical(|identity| self.identity_name(identity, identities))
    }

    fn identity_name(
        &self,
        identity: Identity,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        if let Some(name) = identities.names.borrow().get(&identity) {
            return Some(name.clone());
        }
        let name = self.form_identity_name(identity, identities)?;
        identities.names.borrow_mut().insert(identity, name.clone());
        Some(name)
    }

    fn form_identity_name(
        &self,
        identity: Identity,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        let sources = &identities.sources;
        let mut writer = Writer::default();
        identity.0.write(&mut writer);
        if let Some(source) = sources.name(identity) {
            source.write(&mut writer);
            return Some(writer.bytes);
        }
        let index = identity.1 as usize;
        match identity.0 {
            IdentityKind::Function => {
                let signature = self.types.signatures.get(index)?;
                signature.declaration.write(&mut writer);
                signature.substitution.write(&mut writer);
                signature.formal_parameter.write(&mut writer);
            }
            IdentityKind::FunctionReference => self
                .types
                .function_reference(FunctionReferenceId::from_index(identity.1))
                .ok()?
                .write(&mut writer),
            IdentityKind::Nominal => {
                let id = NominalId(identity.1);
                if let Some((template, substitution)) =
                    self.types.source_nominal_instance_entry(id).ok()?
                {
                    0_u8.write(&mut writer);
                    self.types
                        .nominal_templates
                        .get(template)?
                        .declaration
                        .write(&mut writer);
                    substitution.write(&mut writer);
                } else if let Some(prelude) = self.types.prelude_types.get(index)? {
                    1_u8.write(&mut writer);
                    prelude.write(&mut writer);
                } else if let Some((referent, _)) = self
                    .types
                    .box_nominals
                    .iter()
                    .find(|(_, candidate)| **candidate == id)
                {
                    2_u8.write(&mut writer);
                    referent.write(&mut writer);
                } else {
                    let (fields, _) = self
                        .types
                        .result_list_nominals
                        .iter()
                        .find(|(_, candidate)| **candidate == id)?;
                    3_u8.write(&mut writer);
                    fields.write(&mut writer);
                }
            }
            IdentityKind::Element => self.types.elements.get(index)?.write(&mut writer),
            IdentityKind::Constant => self
                .types
                .checked_constants
                .get(index)?
                .declaration
                .write(&mut writer),
            IdentityKind::DerivedConst => self.types.derived_consts.get(index)?.write(&mut writer),
            IdentityKind::ContractQuery
            | IdentityKind::Declaration
            | IdentityKind::Module
            | IdentityKind::Item
            | IdentityKind::Node => return None,
        }
        writer.canonical(|identity| self.identity_name(identity, identities))
    }

    /// Current semantic inputs of a named identity. Naming equality alone
    /// does not permit a changed physical type or callable claim to be used.
    fn identity_inputs(&self, (kind, index): Identity) -> Option<Writer> {
        let mut writer = Writer::default();
        match kind {
            IdentityKind::Function => self
                .types
                .signatures
                .get(index as usize)?
                .write(&mut writer),
            IdentityKind::Nominal => self.types.nominals.get(index as usize)?.write(&mut writer),
            IdentityKind::Constant => self
                .types
                .checked_constants
                .get(index as usize)?
                .write(&mut writer),
            IdentityKind::Element => self.types.elements.get(index as usize)?.write(&mut writer),
            IdentityKind::DerivedConst => self
                .types
                .derived_consts
                .get(index as usize)?
                .write(&mut writer),
            IdentityKind::FunctionReference => self
                .types
                .function_reference(FunctionReferenceId::from_index(index))
                .ok()?
                .write(&mut writer),
            IdentityKind::ContractQuery => return None,
            _ => {}
        }
        Some(writer)
    }

    fn write_body_product(
        &self,
        product: &BodyProduct,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        let mut payload = Writer::default();
        product.write(&mut payload);
        let mut pending = payload.identities.clone();
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        while let Some(identity) = pending.pop_first() {
            if !seen.insert(identity) {
                continue;
            }
            let inputs = self.identity_inputs(identity)?;
            pending.extend(&inputs.identities);
            entries.push((
                identity,
                self.identity_name(identity, identities)?,
                inputs.canonical(|identity| self.identity_name(identity, identities))?,
            ));
        }
        let mut record = Writer::default();
        entries.write(&mut record);
        payload.bytes.write(&mut record);
        Some(record.bytes)
    }

    fn read_body_product(
        &self,
        bytes: &[u8],
        identities: &ProductIdentities<'_>,
    ) -> Option<BodyProduct> {
        let empty = IdentityMap::new();
        let mut reader = Reader::new(bytes, &empty);
        let entries = Vec::<(Identity, Vec<u8>, Vec<u8>)>::read(&mut reader)?;
        let payload = Vec::<u8>::read(&mut reader)?;
        if !reader.finished() {
            return None;
        }
        let sources = &identities.sources;
        for (kind, count) in [
            (IdentityKind::Function, self.types.signatures.len()),
            (IdentityKind::Nominal, self.types.nominals.len()),
            (IdentityKind::Element, self.types.elements.len()),
            (IdentityKind::Constant, self.types.checked_constants.len()),
            (IdentityKind::DerivedConst, self.types.derived_consts.len()),
        ] {
            let previous = identities.counts.borrow().get(&kind).copied().unwrap_or(0);
            for index in previous..count {
                let id = (kind, u32::try_from(index).ok()?);
                if let Some(name) = self.identity_name(id, identities) {
                    identities.current.borrow_mut().insert(name, id);
                }
            }
            identities.counts.borrow_mut().insert(kind, count);
        }
        let mut mapping = IdentityMap::new();
        for (old, name, inputs) in entries {
            let current = match old.0 {
                IdentityKind::Declaration
                | IdentityKind::Module
                | IdentityKind::Item
                | IdentityKind::Node => {
                    let mut reader = Reader::new(&name, &empty);
                    let kind = IdentityKind::read(&mut reader)?;
                    let source =
                        crate::semantic::products::identity::SourceIdentity::read(&mut reader)?;
                    if kind != old.0 || !reader.finished() {
                        return None;
                    }
                    sources.resolve(&source)?
                }
                _ => *identities.current.borrow().get(&name)?,
            };
            let current_inputs = self.identity_inputs(current)?;
            if current_inputs.canonical(|identity| self.identity_name(identity, identities))?
                != inputs
            {
                return None;
            }
            if mapping.insert(old, current.1).is_some() {
                return None;
            }
        }
        let origin = |path: &crate::NodePath, role, subtoken| sources.origin(path, role, subtoken);
        let mut reader = Reader::new(&payload, &mapping).with_origins(&origin);
        let product = BodyProduct::read(&mut reader)?;
        reader.finished().then_some(product)
    }
}
/// Restores a retained REF-2 diagnostic word through its original producer.
/// A new event without a mapping costs a cache miss, never a new judgment.
pub(in crate::semantic) fn retained_reference_event(word: &str) -> Option<&'static str> {
    use super::references::InvalidationEvent;
    [
        InvalidationEvent::PrefixWritten,
        InvalidationEvent::PrefixMoved,
        InvalidationEvent::CallWrite,
        InvalidationEvent::RootScopeEnded,
        InvalidationEvent::WindowBoundaryMoved,
    ]
    .into_iter()
    .map(|event| event.phrase())
    .find(|candidate| *candidate == word)
}
