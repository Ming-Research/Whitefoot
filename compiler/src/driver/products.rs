//! Module source dependencies of retained structural bodies.

use super::{BuildCache, CompilerLimits, Fields, reads};
use crate::{ModuleId, ResolvedSyntaxUnit, SourceRole};

pub(super) struct CheckProducts<'a> {
    cache: &'a BuildCache,
    modules: Vec<Option<Vec<u8>>>,
    names: Vec<String>,
    reuse_proofs: bool,
}

impl<'a> CheckProducts<'a> {
    pub(super) fn new(
        cache: &'a BuildCache,
        resolved: &ResolvedSyntaxUnit,
        limits: CompilerLimits,
        reuse_proofs: bool,
    ) -> Self {
        let bundle = resolved.syntax().classified_bundle().source_bundle();
        let mut digests = std::collections::BTreeMap::new();
        let modules = (0..bundle.modules().len())
            .map(|index| {
                let target = ModuleId::from_index(index)?;
                let own = bundle
                    .files()
                    .iter()
                    .filter(|file| file.prelude().is_none() && file.module() == target)
                    .collect::<Vec<_>>();
                if !own
                    .iter()
                    .any(|file| file.role() == SourceRole::Implementation)
                {
                    return None;
                }
                let mut fields = Fields::default();
                fields.push(b"module-products 1");
                fields.push(bundle.module(target)?.qualified_name().as_bytes());
                let mut closure = std::collections::BTreeSet::new();
                let mut pending = vec![target];
                while let Some(module) = pending.pop() {
                    if closure.insert(module) {
                        pending.extend(bundle.module(module)?.dependencies());
                    }
                }
                let mut graph = closure
                    .iter()
                    .map(|module| {
                        let module = bundle.module(*module)?;
                        let mut dependencies = module
                            .dependencies()
                            .iter()
                            .map(|dependency| Some(bundle.module(*dependency)?.qualified_name()))
                            .collect::<Option<Vec<_>>>()?;
                        dependencies.sort();
                        Some((module.qualified_name(), dependencies))
                    })
                    .collect::<Option<Vec<_>>>()?;
                graph.sort();
                for (module, dependencies) in graph {
                    fields.push(module.as_bytes());
                    for dependency in dependencies {
                        fields.push(dependency.as_bytes());
                    }
                    fields.push(b"end dependencies");
                }
                fields.push(b"own records");
                let mut own = own;
                own.sort_by_key(|file| file.logical_path().as_str());
                for file in own {
                    fields.push(file.logical_path().as_str().as_bytes());
                    fields.push(if file.role() == SourceRole::Interface {
                        b"interface"
                    } else {
                        b"implementation"
                    });
                    fields.push(file.bytes());
                }
                fields.push(b"consumed declarations");
                for (module, item) in reads::read_declarations(resolved, target)? {
                    if module == target {
                        continue;
                    }
                    let interface = bundle.files().iter().find(|file| {
                        file.module() == module && file.role() == SourceRole::Interface
                    })?;
                    let digests = digests
                        .entry(module)
                        .or_insert_with(|| reads::declaration_digests(interface.bytes(), limits))
                        .as_ref()?;
                    fields.push(bundle.module(module)?.qualified_name().as_bytes());
                    fields.push(item.0.as_bytes()).push(item.1.as_bytes());
                    fields.push(digests.get(&item)?);
                }
                Some(fields.into_bytes())
            })
            .collect();
        let names = bundle
            .modules()
            .iter()
            .map(crate::ModuleRecord::qualified_name)
            .collect();
        Self {
            cache,
            modules,
            names,
            reuse_proofs,
        }
    }
}

impl crate::semantic::ProofReceipts for CheckProducts<'_> {
    fn load(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.reuse_proofs
            .then(|| crate::semantic::ProofReceipts::load(self.cache, key))
            .flatten()
    }
    fn store(&self, key: &[u8], bytes: &[u8]) {
        if self.reuse_proofs {
            crate::semantic::ProofReceipts::store(self.cache, key, bytes);
        }
    }
    fn products(&self) -> Option<&dyn crate::semantic::ModuleProducts> {
        Some(self)
    }
}

impl crate::semantic::ModuleProducts for CheckProducts<'_> {
    fn source_key(&self, module: ModuleId) -> Option<&[u8]> {
        self.modules.get(module.index())?.as_deref()
    }
    fn load_body(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.cache.load("module-bodies", key)
    }
    fn store_body(&self, key: &[u8], bytes: &[u8]) {
        let _ = self.cache.store("module-bodies", key, bytes);
    }
    fn header_checked(&self) {
        self.cache.header_checked();
    }
    fn body_checked(&self, module: Option<ModuleId>) {
        self.cache.body_work(
            module
                .and_then(|module| self.names.get(module.index()))
                .map_or("prelude", String::as_str),
            false,
        );
    }
    fn body_reused(&self, module: Option<ModuleId>) {
        self.cache.body_work(
            module
                .and_then(|module| self.names.get(module.index()))
                .map_or("prelude", String::as_str),
            true,
        );
    }
}
