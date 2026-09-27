//! Module source dependencies of retained structural bodies.

use super::{BuildCache, CompilerLimits, Fields, reads};
use crate::{ModuleId, ResolvedSyntaxUnit, SourceRole};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::BTreeMap;

struct ModuleUnit {
    key: Vec<u8>,
    raw: RefCell<Vec<u8>>,
    offsets: BTreeMap<Vec<u8>, (usize, usize)>,
    bodies: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
    changed: Cell<bool>,
}

impl ModuleUnit {
    fn open(cache: &BuildCache, key: Vec<u8>) -> Self {
        let (raw, offsets) = cache
            .load("module-bodies", &key)
            .and_then(|bytes| {
                let fields = Fields::ranges(&bytes)?;
                if fields.len() % 2 != 0 {
                    return None;
                }
                let mut offsets = BTreeMap::new();
                for pair in fields.chunks(2) {
                    let body_key = bytes[pair[0].0..pair[0].1].to_vec();
                    if offsets.insert(body_key, pair[1]).is_some() {
                        return None;
                    }
                }
                Some((bytes, offsets))
            })
            .unwrap_or_default();
        Self {
            key,
            raw: RefCell::new(raw),
            offsets,
            bodies: RefCell::default(),
            changed: Cell::new(false),
        }
    }

    fn publish_fields(&self) -> Fields {
        let raw = self.raw.borrow();
        let bodies = self.bodies.borrow();
        let mut fields = Fields::default();
        let mut stored = self.offsets.iter().peekable();
        let mut updates = bodies.iter().peekable();
        loop {
            match (stored.peek(), updates.peek()) {
                (None, None) => break,
                (Some(stored_entry), None) => {
                    let key = stored_entry.0;
                    let (start, end) = *stored_entry.1;
                    fields.push(key).push(&raw[start..end]);
                    stored.next();
                }
                (None, Some(update_entry)) => {
                    let key = update_entry.0;
                    let body = update_entry.1;
                    fields.push(key).push(body);
                    updates.next();
                }
                (Some(stored_entry), Some(update_entry)) => {
                    let stored_key = stored_entry.0;
                    let (start, end) = *stored_entry.1;
                    let update_key = update_entry.0;
                    let body = update_entry.1;
                    match stored_key.cmp(update_key) {
                        Ordering::Less => {
                            fields.push(stored_key).push(&raw[start..end]);
                            stored.next();
                        }
                        Ordering::Equal => {
                            fields.push(stored_key).push(body);
                            stored.next();
                            updates.next();
                        }
                        Ordering::Greater => {
                            fields.push(update_key).push(body);
                            updates.next();
                        }
                    }
                }
            }
        }
        fields
    }
}

pub(super) struct CheckProducts<'a> {
    cache: &'a BuildCache,
    modules: Vec<Option<ModuleUnit>>,
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
                fields.push(b"module-products 2");
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
                Some(ModuleUnit::open(cache, fields.into_bytes()))
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

impl Drop for CheckProducts<'_> {
    fn drop(&mut self) {
        // A later source failure does not invalidate earlier completed
        // structural walks. Whole-container publication is still atomic.
        for unit in self
            .modules
            .iter()
            .flatten()
            .filter(|unit| unit.changed.get())
        {
            let _ = self.cache.store(
                "module-bodies",
                &unit.key,
                &unit.publish_fields().into_bytes(),
            );
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
    fn has_module(&self, module: ModuleId) -> bool {
        self.modules
            .get(module.index())
            .is_some_and(Option::is_some)
    }
    fn load_body(&self, module: ModuleId, key: &[u8]) -> Option<Vec<u8>> {
        let unit = self.modules.get(module.index())?.as_ref()?;
        if let Some(body) = unit.bodies.borrow().get(key) {
            return Some(body.clone());
        }
        let &(start, end) = unit.offsets.get(key)?;
        Some(unit.raw.borrow()[start..end].to_vec())
    }
    fn store_body(&self, module: ModuleId, key: &[u8], bytes: &[u8]) {
        if let Some(Some(unit)) = self.modules.get(module.index()) {
            unit.bodies
                .borrow_mut()
                .insert(key.to_vec(), bytes.to_vec());
            unit.changed.set(true);
        }
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
