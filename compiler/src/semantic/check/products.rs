//! Retained structural module products, imported before composition judgments.
//!
//! This adapter uses the one checked-body representation. An incomplete
//! identity table or a changed input is a miss and invokes ordinary checking.

mod discovery;

use discovery::DiscoveryProduct;
pub(super) use discovery::DiscoveryRequests;
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
    fn has_module(&self, module: crate::ModuleId) -> bool;
    fn load_body(&self, module: crate::ModuleId, key: &[u8]) -> Option<Vec<u8>>;
    fn store_body(&self, module: crate::ModuleId, key: &[u8], bytes: &[u8]);
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
    sources: std::rc::Rc<SourceIdentities<'a>>,
    headers: std::rc::Rc<crate::resolution::CallableHeaders<'a>>,
    names: std::cell::RefCell<BTreeMap<Identity, Vec<u8>>>,
    current: std::cell::RefCell<BTreeMap<Vec<u8>, Identity>>,
    counts: std::cell::RefCell<BTreeMap<IdentityKind, usize>>,
}

impl<'a> ProductIdentities<'a> {
    pub(super) fn new(declarations: &'a DeclarationInventory<'a>) -> Option<Self> {
        Some(Self {
            sources: std::rc::Rc::new(SourceIdentities::new(
                declarations.resolved,
                &declarations.tree,
            )?),
            headers: std::rc::Rc::new(declarations.resolved.callable_headers().ok()?),
            names: Default::default(),
            current: Default::default(),
            counts: Default::default(),
        })
    }

    fn staged(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            headers: self.headers.clone(),
            names: Default::default(),
            current: Default::default(),
            counts: Default::default(),
        }
    }
}

/// The part of body checking whose publication is independent of entailment.
struct BodyProduct {
    checked: CheckedFunctionInventory,
    written_effects: Option<EffectSet>,
    query_ids: Vec<super::super::model::ContractQueryId>,
    discovery: DiscoveryProduct,
}
record_struct!(BodyProduct {
    checked,
    written_effects,
    query_ids,
    discovery
});

struct RetainedIdentity {
    old: Identity,
    name: Vec<u8>,
    inputs: Vec<u8>,
    recipe: Vec<u8>,
}
record_struct!(RetainedIdentity {
    old,
    name,
    inputs,
    recipe
});

struct ImportedBody {
    body: BodyProduct,
    queries: Vec<super::super::model::CheckedContractQuery>,
}

/// Inventory cursors delimit the products published by one body.
struct DiscoveryState {
    nominals: usize,
    elements: usize,
    signatures: usize,
    derived_consts: usize,
    functions: usize,
    visible_nominals: usize,
    queries: usize,
    tail_rejections: usize,
    selectors: usize,
    schema_instances: usize,
    declaration_arguments: usize,
    unavailable: usize,
    references: usize,
}

impl Checker<'_, '_> {
    fn discovery_state(&self) -> DiscoveryState {
        DiscoveryState {
            nominals: self.types.nominals.len(),
            elements: self.types.elements.len(),
            signatures: self.types.signatures.len(),
            derived_consts: self.types.derived_consts.len(),
            functions: self.types.view.functions.len(),
            visible_nominals: self.types.view.nominals.len(),
            queries: self.analysis.contract_queries.len(),
            tail_rejections: self.analysis.musttail_rejections.len(),
            selectors: self.analysis.postcondition_selectors.len(),
            schema_instances: self.analysis.schema_written_instances.len(),
            declaration_arguments: self.types.behavior.declaration_arguments.len(),
            unavailable: self.analysis.postcondition_unavailable_declarations.len(),
            references: self.types.behavior.reference_count(),
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
        let import_context = CheckContext {
            template_spelling_authority: signature.substitution.len() > 0
                && !signature.substitution.is_symbolic(),
            ..*context
        };
        if let (Some(module), Some(key)) = (module, &key)
            && let Some(bytes) = products.load_body(module, key)
            && let Some(body) = self.read_body_product(&import_context, &bytes, identities)
        {
            if let Some(effects) = body.written_effects {
                self.analysis
                    .written_body_effect_rows
                    .insert(body.checked.function.declaration, effects);
            }
            self.import_discovery(body.discovery);
            products.body_reused(Some(module));
            return Ok(body.checked);
        }
        let before = self.discovery_state();
        *self.types.product_discovery.borrow_mut() = Some(DiscoveryRequests::default());
        products.body_checked(module);
        let checked = self.check_function(context, index);
        let requested = self
            .types
            .product_discovery
            .borrow_mut()
            .take()
            .expect("one body owns its discovery requests");
        let checked = checked?;
        let after = self.discovery_state();
        let query_end = after.queries;
        if before.tail_rejections == after.tail_rejections
            && let Some(key) = key
        {
            let product = BodyProduct {
                discovery: self.retained_discovery(&before, requested),
                query_ids: (before.queries..query_end)
                    .map(|index| super::super::model::ContractQueryId(index as u32))
                    .collect(),
                written_effects: self
                    .analysis
                    .written_body_effect_rows
                    .get(&checked.function.declaration)
                    .cloned(),
                checked,
            };
            if let Some(module) = module
                && let Some(bytes) = self.write_body_product(&product, &before, identities)
            {
                products.store_body(module, &key, &bytes);
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
        if !products.has_module(module) {
            return None;
        }
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
        self.signature_inputs(signature)
            .canonical(|identity| self.identity_name(identity, identities))?
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

    fn signature_inputs(&self, signature: &FunctionSignature) -> Writer {
        let mut signature = signature.clone();
        // A symbolic signature's temporary link spelling can contain its
        // dense ordinal. Import assigns the current spelling after rebinding.
        signature.symbol.clear();
        let mut writer = Writer::default();
        signature.write(&mut writer);
        writer
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
    fn identity_inputs(
        &self,
        (kind, index): Identity,
        identities: &ProductIdentities<'_>,
    ) -> Option<Writer> {
        let mut writer = Writer::default();
        match kind {
            IdentityKind::Function => {
                let signature = self.types.signatures.get(index as usize)?;
                writer = self.signature_inputs(signature);
                let header = identities.headers.header(signature.node).ok()?;
                header.len().write(&mut writer);
                for token in header {
                    write_header_token(&token, &mut writer);
                }
            }
            IdentityKind::Nominal => {
                let mut nominal = self.types.nominals.get(index as usize)?.clone();
                // Diagnostic instance labels contain local ordinals. The
                // declaration/arguments name the type; its fields guard it.
                nominal.name.clear();
                nominal.write(&mut writer);
            }
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
            IdentityKind::FunctionReference => {
                let reference = FunctionReferenceId::from_index(index);
                self.types
                    .function_reference(reference)
                    .ok()?
                    .write(&mut writer);
                // A supplied callable's complete current claims belong to
                // an FN-4 product's inputs, even when its argument types did
                // not change. Without that formed boundary this is a miss.
                self.types
                    .function_reference_instance(reference)
                    .ok()??
                    .write(&mut writer);
            }
            IdentityKind::ContractQuery => return None,
            _ => {}
        }
        Some(writer)
    }

    fn write_body_product(
        &self,
        product: &BodyProduct,
        before: &DiscoveryState,
        identities: &ProductIdentities<'_>,
    ) -> Option<Vec<u8>> {
        let mut payload = Writer::default();
        product.write(&mut payload);
        let mut pending = payload.identities.clone();
        for (kind, start, end) in [
            (
                IdentityKind::Function,
                before.signatures,
                self.types.signatures.len(),
            ),
            (
                IdentityKind::Nominal,
                before.nominals,
                self.types.nominals.len(),
            ),
            (
                IdentityKind::Element,
                before.elements,
                self.types.elements.len(),
            ),
            (
                IdentityKind::DerivedConst,
                before.derived_consts,
                self.types.derived_consts.len(),
            ),
            (
                IdentityKind::FunctionReference,
                before.references,
                self.types.behavior.reference_count(),
            ),
        ] {
            pending.extend((start..end).map(|index| (kind, index as u32)));
        }
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        let mut queries = Vec::new();
        while let Some(identity) = pending.pop_first() {
            if !seen.insert(identity) {
                continue;
            }
            if identity.0 == IdentityKind::ContractQuery {
                let mut query = Writer::default();
                self.analysis
                    .contract_queries
                    .get(identity.1 as usize)?
                    .write(&mut query);
                // Each FN-4 proof owns its local proof arena and names no
                // other global FN-4 query.
                if query
                    .identities
                    .iter()
                    .any(|(kind, _)| *kind == IdentityKind::ContractQuery)
                {
                    return None;
                }
                pending.extend(&query.identities);
                queries.push((identity.1, query.bytes));
                continue;
            }
            let inputs = self.identity_inputs(identity, identities)?;
            pending.extend(&inputs.identities);
            let recipe = self.identity_recipe(identity)?;
            pending.extend(&recipe.identities);
            entries.push(RetainedIdentity {
                old: identity,
                name: self.identity_name(identity, identities)?,
                inputs: inputs.canonical(|identity| self.identity_name(identity, identities))?,
                recipe: recipe.bytes,
            });
        }
        let mut record = Writer::default();
        entries.write(&mut record);
        queries.sort_by_key(|(id, _)| *id);
        queries.write(&mut record);
        payload.bytes.write(&mut record);
        Some(record.bytes)
    }

    fn read_body_product(
        &mut self,
        context: &CheckContext<'_>,
        bytes: &[u8],
        identities: &ProductIdentities<'_>,
    ) -> Option<BodyProduct> {
        let empty = IdentityMap::new();
        let mut reader = Reader::new(bytes, &empty);
        let entries = Vec::<RetainedIdentity>::read(&mut reader)?;
        let queries = Vec::<(u32, Vec<u8>)>::read(&mut reader)?;
        let payload = Vec::<u8>::read(&mut reader)?;
        if !reader.finished() {
            return None;
        }
        let mapping = self.map_retained_identities(&entries, identities)?;
        if mapping.len() == entries.len() {
            let imported =
                self.decode_body_product(&entries, &queries, &payload, mapping, identities)?;
            self.analysis.contract_queries.extend(imported.queries);
            return Some(imported.body);
        }
        // A missing instance may form additional types. Keep that work private
        // until every input and retained record has decoded successfully.
        let mut staged_types = self.types.clone();
        let prior_view = staged_types.view.clone();
        let mut staged_analysis = AnalysisState {
            written_body_effect_rows: self.analysis.written_body_effect_rows.clone(),
            postcondition_selectors: self.analysis.postcondition_selectors.clone(),
            postcondition_unavailable_declarations: self
                .analysis
                .postcondition_unavailable_declarations
                .clone(),
            ..AnalysisState::default()
        };
        let mut body = BodyChecker::default();
        let staged_identities = identities.staged();
        let mapping = {
            let mut staged = Checker {
                types: &mut staged_types,
                body: &mut body,
                analysis: &mut staged_analysis,
                reject_entailment: self.reject_entailment,
                receipts: None,
            };
            staged.form_retained_identities(context, &entries, mapping, &staged_identities)?
        };
        // Formation against an implementation can expose private work that
        // an earlier interface-only check never saw. Such work needs the
        // ordinary walk; do not drop its analysis or unpublished identities.
        if !staged_analysis.contract_queries.is_empty()
            || !staged_analysis.musttail_rejections.is_empty()
            || !staged_analysis.schema_written_instances.is_empty()
            || staged_analysis.postcondition_selectors != self.analysis.postcondition_selectors
            || staged_analysis.postcondition_unavailable_declarations
                != self.analysis.postcondition_unavailable_declarations
        {
            return None;
        }
        for (kind, start, end) in [
            (
                IdentityKind::Function,
                self.types.signatures.len(),
                staged_types.signatures.len(),
            ),
            (
                IdentityKind::Nominal,
                self.types.nominals.len(),
                staged_types.nominals.len(),
            ),
            (
                IdentityKind::Element,
                self.types.elements.len(),
                staged_types.elements.len(),
            ),
            (
                IdentityKind::DerivedConst,
                self.types.derived_consts.len(),
                staged_types.derived_consts.len(),
            ),
            (
                IdentityKind::FunctionReference,
                self.types.behavior.reference_count(),
                staged_types.behavior.reference_count(),
            ),
        ] {
            if (start..end).any(|id| {
                !mapping.iter().any(|((mapped_kind, _), current)| {
                    *mapped_kind == kind && *current as usize == id
                })
            }) {
                return None;
            }
        }
        let formed_view = staged_types.view.clone();
        let original_view = prior_view.clone();
        staged_types.select_view(prior_view);
        let prior = std::mem::replace(self.types, staged_types);
        let imported =
            self.decode_body_product(&entries, &queries, &payload, mapping, &staged_identities);
        if let Some(imported) = imported
            && imported
                .body
                .discovery
                .covers_formation(&original_view, &formed_view)
        {
            self.analysis.contract_queries.extend(imported.queries);
            return Some(imported.body);
        }
        *self.types = prior;
        None
    }

    fn map_retained_identities(
        &self,
        entries: &[RetainedIdentity],
        identities: &ProductIdentities<'_>,
    ) -> Option<IdentityMap> {
        for (kind, count) in [
            (IdentityKind::Function, self.types.signatures.len()),
            (IdentityKind::Nominal, self.types.nominals.len()),
            (IdentityKind::Element, self.types.elements.len()),
            (IdentityKind::Constant, self.types.checked_constants.len()),
            (IdentityKind::DerivedConst, self.types.derived_consts.len()),
            (
                IdentityKind::FunctionReference,
                self.types.behavior.reference_count(),
            ),
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
        let empty = IdentityMap::new();
        let mut mapping = IdentityMap::new();
        let mut seen = BTreeSet::new();
        for entry in entries {
            if !seen.insert(entry.old) {
                return None;
            }
            let current = match entry.old.0 {
                IdentityKind::Declaration
                | IdentityKind::Module
                | IdentityKind::Item
                | IdentityKind::Node => {
                    let mut reader = Reader::new(&entry.name, &empty);
                    let kind = IdentityKind::read(&mut reader)?;
                    let source =
                        crate::semantic::products::identity::SourceIdentity::read(&mut reader)?;
                    if kind != entry.old.0 || !reader.finished() {
                        return None;
                    }
                    Some(identities.sources.resolve(&source)?)
                }
                _ => identities.current.borrow().get(&entry.name).copied(),
            };
            if let Some(current) = current {
                mapping.insert(entry.old, current.1);
            }
        }
        Some(mapping)
    }

    fn form_retained_identities(
        &mut self,
        context: &CheckContext<'_>,
        entries: &[RetainedIdentity],
        mut mapping: IdentityMap,
        identities: &ProductIdentities<'_>,
    ) -> Option<IdentityMap> {
        while mapping.len() < entries.len() {
            let before = mapping.len();
            for entry in entries {
                if mapping.contains_key(&entry.old) {
                    continue;
                }
                if let Some(current) = self.form_retained_identity(context, &entry.recipe, &mapping)
                {
                    if current.0 != entry.old.0
                        || self.identity_name(current, identities)? != entry.name
                    {
                        return None;
                    }
                    mapping.insert(entry.old, current.1);
                }
            }
            if mapping.len() == before {
                return None;
            }
        }
        Some(mapping)
    }

    fn decode_body_product(
        &self,
        entries: &[RetainedIdentity],
        queries: &[(u32, Vec<u8>)],
        payload: &[u8],
        mut mapping: IdentityMap,
        identities: &ProductIdentities<'_>,
    ) -> Option<ImportedBody> {
        for entry in entries {
            let current = (entry.old.0, *mapping.get(&entry.old)?);
            let inputs = self.identity_inputs(current, identities)?;
            if inputs.canonical(|identity| self.identity_name(identity, identities))?
                != entry.inputs
            {
                return None;
            }
        }
        let sources = &identities.sources;
        let origin = |path: &crate::NodePath, role, subtoken| sources.origin(path, role, subtoken);
        let mut retained = Vec::new();
        for (old, bytes) in queries {
            let mut reader = Reader::new(bytes, &mapping).with_origins(&origin);
            let query = super::super::model::CheckedContractQuery::read(&mut reader)?;
            if !reader.finished() {
                return None;
            }
            let index = self
                .analysis
                .contract_queries
                .iter()
                .position(|known| *known == query)
                .or_else(|| {
                    retained
                        .iter()
                        .position(|known| *known == query)
                        .map(|index| self.analysis.contract_queries.len() + index)
                })
                .unwrap_or_else(|| {
                    let index = self.analysis.contract_queries.len() + retained.len();
                    retained.push(query);
                    index
                });
            if mapping
                .insert(
                    (IdentityKind::ContractQuery, *old),
                    u32::try_from(index).ok()?,
                )
                .is_some()
            {
                return None;
            }
        }
        let mut reader = Reader::new(payload, &mapping).with_origins(&origin);
        let mut product = BodyProduct::read(&mut reader)?;
        if !reader.finished() {
            return None;
        }
        let signature = self
            .types
            .signatures
            .get(product.checked.function.id.0 as usize)?;
        if signature.declaration != product.checked.function.declaration {
            return None;
        }
        product.checked.function.symbol = signature.symbol.clone();
        Some(ImportedBody {
            body: product,
            queries: retained,
        })
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

/// The resolver owns header normalization. The product key only replaces
/// the resolved declaration handles in its tokens with persistent identities.
fn write_header_token(token: &crate::resolution::HeaderToken, writer: &mut Writer) {
    use crate::resolution::HeaderToken;
    match token {
        HeaderToken::Open(production) => {
            0_u8.write(writer);
            production.name().to_owned().write(writer);
        }
        HeaderToken::Close => 1_u8.write(writer),
        HeaderToken::Text(text) => {
            2_u8.write(writer);
            text.write(writer);
        }
        HeaderToken::Name(name) => {
            3_u8.write(writer);
            name.write(writer);
        }
        HeaderToken::Use(target) => {
            4_u8.write(writer);
            match target {
                crate::ResolvedTarget::Source {
                    declaration,
                    class: _,
                } => {
                    0_u8.write(writer);
                    declaration.write(writer);
                }
                crate::ResolvedTarget::Prelude(id) => {
                    1_u8.write(writer);
                    id.write(writer);
                }
                crate::ResolvedTarget::Operation(id) => {
                    2_u8.write(writer);
                    id.ordinal().write(writer);
                }
                crate::ResolvedTarget::Container(id) => {
                    3_u8.write(writer);
                    id.ordinal().write(writer);
                }
            }
        }
        HeaderToken::Binder(ordinal) => {
            5_u8.write(writer);
            ordinal.write(writer);
        }
        HeaderToken::BinderUse(ordinal) => {
            6_u8.write(writer);
            ordinal.write(writer);
        }
    }
}
