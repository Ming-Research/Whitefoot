//! Typed function fragments under current checked, physical and target inputs.

use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;
use crate::semantic::products::identity::{SourceIdentities, SourceIdentity};
use crate::semantic::products::{Identity, IdentityKind, IdentityMap, Reader, Record, Writer};

pub(super) struct Products<'a> {
    store: &'a dyn LoweringProducts,
    sources: SourceIdentities<'a>,
    data: &'a CheckedProgramData,
    context: LoweringContext<'a>,
    symbols: &'a [String],
    names: RefCell<BTreeMap<Identity, Vec<u8>>>,
}

impl<'a> Products<'a> {
    pub(super) fn new(
        store: &'a dyn LoweringProducts,
        resolved: &'a crate::ResolvedSyntaxUnit,
        view: &'a crate::syntax::views::SyntaxView<'a>,
        data: &'a CheckedProgramData,
        context: LoweringContext<'a>,
        symbols: &'a [String],
    ) -> Option<Self> {
        Some(Self {
            store,
            sources: SourceIdentities::new(resolved, view)?,
            data,
            context,
            symbols,
            names: RefCell::default(),
        })
    }

    fn semantic_name(&self, identity: Identity) -> Option<Vec<u8>> {
        let mut writer = Writer::default();
        identity.0.write(&mut writer);
        if let Some(source) = self.sources.name(identity) {
            source.write(&mut writer);
            return Some(writer.bytes);
        }
        let index = identity.1 as usize;
        match identity.0 {
            IdentityKind::Function => self.data.functions.get(index)?.symbol.write(&mut writer),
            IdentityKind::Nominal => {
                let spelling = self.data.nominal_spellings.get(index)?;
                spelling.write(&mut writer);
                if spelling.is_none() {
                    self.data.nominals.get(index)?.name.write(&mut writer);
                }
            }
            IdentityKind::Element => self.data.elements.get(index)?.write(&mut writer),
            IdentityKind::Constant => self.data.constant_spellings.get(index)?.write(&mut writer),
            IdentityKind::DerivedConst => self.data.derived_consts.get(index)?.write(&mut writer),
            // Lowering consumes the accepted call, never an FN-4 proof handle.
            IdentityKind::ContractQuery => {}
            _ => return None,
        }
        writer.canonical(|identity| self.semantic_name(identity))
    }

    /// Canonical physical graphs use local discovery ordinals for recursive
    /// nominal edges. Equality includes complete layouts and reclamation, not
    /// merely a nominal's display name.
    fn physical_name(&self, root: Identity) -> Option<Vec<u8>> {
        if let Some(name) = self.names.borrow().get(&root) {
            return Some(name.clone());
        }
        if let Some(source) = self.sources.name(root) {
            let mut writer = Writer::default();
            root.0.write(&mut writer);
            source.write(&mut writer);
            return Some(writer.bytes);
        }
        let mut out = Writer::default();
        let mut pending = vec![root];
        let mut local = BTreeMap::from([(root, 0_u32)]);
        let mut cursor = 0;
        while cursor < pending.len() {
            let (kind, id) = pending[cursor];
            let mut writer = Writer::default();
            kind.write(&mut writer);
            match kind {
                IdentityKind::Nominal => {
                    let mut nominal = self.context.nominals.get(id as usize)?.clone();
                    nominal.name.clear();
                    nominal.write(&mut writer);
                }
                IdentityKind::Element => self.context.elements.get(id as usize)?.write(&mut writer),
                IdentityKind::Constant => {
                    self.context.constants.get(id as usize)?.write(&mut writer)
                }
                IdentityKind::Function => self.symbols.get(id as usize)?.write(&mut writer),
                _ => return None,
            }
            let record = writer.canonical(|identity| {
                let ordinal = *local.entry(identity).or_insert_with(|| {
                    let ordinal = pending.len() as u32;
                    pending.push(identity);
                    ordinal
                });
                let mut name = Writer::default();
                identity.0.write(&mut name);
                ordinal.write(&mut name);
                Some(name.bytes)
            })?;
            record.write(&mut out);
            cursor += 1;
        }
        self.names.borrow_mut().insert(root, out.bytes.clone());
        Some(out.bytes)
    }

    fn key(
        &self,
        function: &crate::semantic::CheckedFunction,
        context: LoweringContext<'_>,
        permission: Option<&FunctionPermissions>,
        overlap: OverlapLowering,
    ) -> Option<Vec<u8>> {
        let crate::semantic::CheckedFunction {
            name,
            symbol,
            parameters,
            result_mode,
            result,
            declared_state_writes,
            body,
            body_disposition,
            formal_hypothesis: _,
            id: _,
            declaration: _,
            module: _,
            function_actuals: _,
            region_parameters: _,
            requirements: _,
            requirement_places: _,
            postconditions: _,
            reference_origins: _,
            allocates: _,
            call_separations: _,
            permission_separation_queries: _,
            obligations: _,
            entailment: _,
        } = function;
        let mut source = Writer::default();
        name.write(&mut source);
        symbol.write(&mut source);
        parameters.write(&mut source);
        result_mode.write(&mut source);
        result.write(&mut source);
        declared_state_writes.write(&mut source);
        body.write(&mut source);
        matches!(
            body_disposition,
            crate::semantic::CheckedBodyDisposition::Uninhabited { .. }
        )
        .write(&mut source);
        permission.is_some().write(&mut source);
        if let Some(permission) = permission {
            permission.pairs.len().write(&mut source);
            for pair in &permission.pairs {
                pair.first.write(&mut source);
                pair.second.write(&mut source);
                pair.verdict.is_eligible().write(&mut source);
            }
            permission.runs.write(&mut source);
            permission.loops.len().write(&mut source);
            for permission in &permission.loops {
                permission.statement.write(&mut source);
                permission.actualization.write(&mut source);
            }
        }
        let mut key = Writer::default();
        b"lowered-function 1".to_vec().write(&mut key);
        source
            .canonical(|identity| self.semantic_name(identity))?
            .write(&mut key);
        // These closed compiler enums and ABI fields contain no source ids;
        // their complete Debug records are input bytes, never parsed output.
        format!("{:?}", context.target).write(&mut key);
        (overlap != OverlapLowering::Off).write(&mut key);
        let mut physical = Writer::default();
        for identity in &source.identities {
            match identity.0 {
                IdentityKind::Nominal => {
                    if let Some(id) = context
                        .erasure
                        .nominals
                        .get(identity.1 as usize)
                        .copied()
                        .flatten()
                    {
                        id.write(&mut physical);
                    }
                }
                IdentityKind::Element => {
                    if let Some(id) = context
                        .erasure
                        .elements
                        .get(identity.1 as usize)
                        .copied()
                        .flatten()
                    {
                        id.write(&mut physical);
                    }
                }
                IdentityKind::Constant => IrConstantId(identity.1).write(&mut physical),
                _ => {}
            }
        }
        context.physical_calls.len().write(&mut physical);
        for (path, callee) in context.physical_calls {
            path.write(&mut physical);
            physical.identity(IdentityKind::Function, *callee);
            context
                .function_results
                .get(*callee as usize)?
                .write(&mut physical);
        }
        physical
            .canonical(|identity| self.physical_name(identity))?
            .write(&mut key);
        Some(key.bytes)
    }

    pub(super) fn lower(
        &self,
        function: &crate::semantic::CheckedFunction,
        index: usize,
        symbol: &str,
        context: LoweringContext<'_>,
        permission: Option<&FunctionPermissions>,
        overlap: OverlapLowering,
    ) -> Result<IrFunction, LoweringFailure> {
        let key = self.key(function, context, permission, overlap);
        let module = self
            .sources
            .name((IdentityKind::Module, function.module.index() as u32));
        let module = match module {
            Some(SourceIdentity::Module(name)) => name,
            _ => "prelude".to_owned(),
        };
        if let Some(key) = &key
            && let Some(bytes) = self.store.load(key)
            && let Some(function) = self.read(&bytes, context)
        {
            self.store.lowered(&module, true);
            return Ok(function);
        }
        self.store.lowered(&module, false);
        let checkpoint = context.synthesis.borrow().checkpoint();
        let first_helper = context.synthesis.borrow().next_ordinal();
        let result = lower_function(function, index, symbol, context, permission, overlap)?;
        if let Some(key) = key
            && let Some(first_helper) = first_helper
            && let Some(synthesized) = context.synthesis.borrow().retain_since(&checkpoint)
            && let Some(bytes) = self.write(&result, &synthesized, first_helper)
        {
            self.store.store(&key, &bytes);
        }
        Ok(result)
    }

    fn write(
        &self,
        function: &IrFunction,
        synthesized: &split::SynthesisProduct,
        first_helper: u32,
    ) -> Option<Vec<u8>> {
        let mut payload = Writer::default();
        function.write(&mut payload);
        synthesized.write(&mut payload);
        let helpers = synthesized
            .functions
            .iter()
            .map(|function| function.name.clone())
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        for identity in &payload.identities {
            let name = if identity.0 == IdentityKind::Function && identity.1 >= first_helper {
                function_name(helpers.get((identity.1 - first_helper) as usize)?)
            } else {
                self.physical_name(*identity)?
            };
            entries.push((*identity, name));
        }
        let mut record = Writer::default();
        helpers.write(&mut record);
        entries.write(&mut record);
        payload.bytes.write(&mut record);
        Some(record.bytes)
    }

    fn read(&self, bytes: &[u8], context: LoweringContext<'_>) -> Option<IrFunction> {
        let empty = IdentityMap::new();
        let mut reader = Reader::new(bytes, &empty);
        let helpers = Vec::<String>::read(&mut reader)?;
        let entries = Vec::<(Identity, Vec<u8>)>::read(&mut reader)?;
        let payload = Vec::<u8>::read(&mut reader)?;
        if !reader.finished() {
            return None;
        }
        let mut current = BTreeMap::new();
        for (kind, count) in [
            (IdentityKind::Function, self.symbols.len()),
            (IdentityKind::Nominal, context.nominals.len()),
            (IdentityKind::Element, context.elements.len()),
            (IdentityKind::Constant, context.constants.len()),
        ] {
            for index in 0..count {
                let identity = (kind, u32::try_from(index).ok()?);
                let name = self.physical_name(identity)?;
                if current.insert(name, identity).is_some() {
                    return None;
                }
            }
        }
        let first = context.synthesis.borrow().next_ordinal()?;
        for (index, name) in helpers.iter().enumerate() {
            let id = first.checked_add(u32::try_from(index).ok()?)?;
            if current
                .insert(function_name(name), (IdentityKind::Function, id))
                .is_some()
            {
                return None;
            }
        }
        let mut mapping = IdentityMap::new();
        for (old, name) in entries {
            let current = match old.0 {
                IdentityKind::Item
                | IdentityKind::Node
                | IdentityKind::Declaration
                | IdentityKind::Module => {
                    let mut reader = Reader::new(&name, &empty);
                    if IdentityKind::read(&mut reader)? != old.0 {
                        return None;
                    }
                    let source = SourceIdentity::read(&mut reader)?;
                    if !reader.finished() {
                        return None;
                    }
                    self.sources.resolve(&source)?
                }
                _ => *current.get(&name)?,
            };
            if current.0 != old.0 || mapping.insert(old, current.1).is_some() {
                return None;
            }
        }
        let mut reader = Reader::new(&payload, &mapping);
        let function = IrFunction::read(&mut reader)?;
        let synthesized = split::SynthesisProduct::read(&mut reader)?;
        if !reader.finished()
            || synthesized
                .functions
                .iter()
                .map(|function| &function.name)
                .ne(helpers.iter())
        {
            return None;
        }
        context.synthesis.borrow_mut().import(synthesized);
        Some(function)
    }
}

fn function_name(symbol: &str) -> Vec<u8> {
    let mut writer = Writer::default();
    IdentityKind::Function.write(&mut writer);
    symbol.to_owned().write(&mut writer);
    // The graph encoding has one record, containing no inventory references.
    let record = writer
        .canonical(|_| None)
        .expect("a function name has no inventory references");
    let mut out = Writer::default();
    record.write(&mut out);
    out.bytes
}
