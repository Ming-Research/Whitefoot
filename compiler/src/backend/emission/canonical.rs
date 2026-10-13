//! Import an independently lowered sequential world from its recorded graph.
//!
//! LLVM text is rewritten only at identifier tokens, including metadata IDs.
//! It never supplies graph edges; unrecorded metadata references are rejected.

use super::{Declaration, Entity, Linkage, Metadata, Module, ModulePart, References};
use crate::backend::emitter::BackendFailure;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Dependency {
    Symbol(String),
    Type(String),
    Attributes(u64),
    Metadata(u64),
}

#[derive(Default)]
struct Closure {
    entities: BTreeSet<String>,
    declarations: BTreeSet<String>,
    types: BTreeSet<String>,
    attributes: BTreeSet<u64>,
    metadata: BTreeSet<u64>,
}

fn dependencies(pending: &mut BTreeSet<Dependency>, references: &References) {
    pending.extend(
        references
            .metadata
            .iter()
            .copied()
            .map(Dependency::Metadata),
    );
    pending.extend(references.symbols.iter().cloned().map(Dependency::Symbol));
    pending.extend(references.types.iter().cloned().map(Dependency::Type));
    pending.extend(
        references
            .attributes
            .iter()
            .copied()
            .map(Dependency::Attributes),
    );
}

fn entities(module: &Module) -> Result<BTreeMap<String, &Entity>, BackendFailure> {
    if module.has_conflict() {
        return Err(BackendFailure::InvalidIr);
    }
    let mut types = BTreeSet::new();
    for part in &module.parts {
        if let ModulePart::Type(name) = part
            && !types.insert(name)
        {
            return Err(BackendFailure::InvalidIr);
        }
    }
    let mut entities = BTreeMap::new();
    for entity in &module.entities {
        if module.declarations.contains_key(&entity.name)
            || entities.insert(entity.name.clone(), entity).is_some()
        {
            return Err(BackendFailure::InvalidIr);
        }
    }
    Ok(entities)
}

fn compatible_declaration(entity: &Entity, declaration: &Declaration) -> bool {
    entity.declaration == declaration.text
        && entity.declaration_references == declaration.references
}

/// A skipped shared/weak body keeps its identity, but every type and attribute
/// named by its declaration must retain the same meaning in both modules.
fn compatible_contract_dependencies(
    canonical: &Module,
    destination: &Module,
    references: &References,
) -> bool {
    let mut pending = references.types.clone();
    let mut attributes = references.attributes.clone();
    let mut metadata = references.metadata.clone();
    let mut visited = BTreeSet::new();
    while let Some(name) = pending.pop_first() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(declaration) = canonical.types.get(&name) else {
            return false;
        };
        if destination.types.get(&name) != Some(declaration) {
            return false;
        }
        pending.extend(declaration.references.types.iter().cloned());
        attributes.extend(declaration.references.attributes.iter().copied());
        metadata.extend(declaration.references.metadata.iter().copied());
    }
    let Ok(metadata) = canonical.metadata_closure(&metadata) else {
        return false;
    };
    if !metadata
        .iter()
        .all(|id| canonical.metadata.get(id) == destination.metadata.get(id))
    {
        return false;
    }
    attributes.iter().all(|id| {
        canonical.attributes.contains_key(id)
            && canonical.attributes.get(id) == destination.attributes.get(id)
    })
}

fn closure(
    canonical: &Module,
    destination: &Module,
    roots: &BTreeSet<String>,
    shared: &BTreeSet<String>,
    source: &BTreeMap<String, &Entity>,
    existing: &BTreeMap<String, &Entity>,
) -> Result<Closure, BackendFailure> {
    let mut closure = Closure::default();
    let mut pending = roots
        .iter()
        .cloned()
        .map(Dependency::Symbol)
        .collect::<BTreeSet<_>>();
    let mut visited = BTreeSet::new();
    while let Some(dependency) = pending.pop_first() {
        if !visited.insert(dependency.clone()) {
            continue;
        }
        match dependency {
            Dependency::Symbol(name) => {
                let entity = source.get(&name).copied();
                let declaration = canonical.declarations.get(&name);
                if entity.is_none() && declaration.is_none() {
                    return Err(BackendFailure::InvalidIr);
                }
                if shared.contains(&name) {
                    // The caller owns intentional differences in shared safety
                    // support; importing its Off implementation would erase them.
                    let contract = if let Some(entity) = entity {
                        Declaration {
                            text: entity.declaration.clone(),
                            references: entity.declaration_references.clone(),
                        }
                    } else {
                        declaration.cloned().ok_or(BackendFailure::InvalidIr)?
                    };
                    let compatible = existing
                        .get(&name)
                        .is_some_and(|entity| compatible_declaration(entity, &contract))
                        || destination.declarations.get(&name) == Some(&contract);
                    if !compatible
                        || !compatible_contract_dependencies(
                            canonical,
                            destination,
                            &contract.references,
                        )
                    {
                        return Err(BackendFailure::InvalidIr);
                    }
                    continue;
                }
                if let Some(entity) = entity {
                    if entity.linkage == Linkage::Weak {
                        // A strong native definition overrides this name.
                        // Cloning a weak fallback would break that contract.
                        let declaration = Declaration {
                            text: entity.declaration.clone(),
                            references: entity.declaration_references.clone(),
                        };
                        let compatible = existing
                            .get(&name)
                            .is_some_and(|other| compatible_declaration(other, &declaration))
                            || destination.declarations.get(&name) == Some(&declaration);
                        if !compatible
                            || !compatible_contract_dependencies(
                                canonical,
                                destination,
                                &declaration.references,
                            )
                        {
                            return Err(BackendFailure::InvalidIr);
                        }
                        continue;
                    }
                    closure.entities.insert(name);
                    dependencies(&mut pending, &entity.references);
                    dependencies(&mut pending, &entity.declaration_references);
                } else if let Some(declaration) = declaration {
                    closure.declarations.insert(name);
                    dependencies(&mut pending, &declaration.references);
                }
            }
            Dependency::Type(name) => {
                let declaration = canonical
                    .types
                    .get(&name)
                    .ok_or(BackendFailure::InvalidIr)?;
                closure.types.insert(name);
                dependencies(&mut pending, &declaration.references);
            }
            Dependency::Metadata(id) => {
                let node = canonical
                    .metadata
                    .get(&id)
                    .ok_or(BackendFailure::InvalidIr)?;
                closure.metadata.insert(id);
                pending.extend(node.references.iter().copied().map(Dependency::Metadata));
            }
            Dependency::Attributes(id) => {
                if !canonical.attributes.contains_key(&id) {
                    return Err(BackendFailure::InvalidIr);
                }
                closure.attributes.insert(id);
            }
        }
    }
    Ok(closure)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum GlobalKind {
    Constant,
    Mutable,
    Alias,
}

fn global_kind(entity: &Entity) -> Result<GlobalKind, BackendFailure> {
    // Module::global retains its kind in the generated header prefix. This
    // narrow printing classification discovers no symbols or dependencies.
    let (_, prefix) = entity
        .header
        .split_once(" = ")
        .ok_or(BackendFailure::InvalidIr)?;
    for token in prefix.split_whitespace() {
        match token {
            "constant" => return Ok(GlobalKind::Constant),
            "global" => return Ok(GlobalKind::Mutable),
            "alias" => return Ok(GlobalKind::Alias),
            "private" | "internal" | "hidden" | "unnamed_addr" | "local_unnamed_addr" => {}
            token if token.starts_with("thread_local") => {}
            _ => return Err(BackendFailure::InvalidIr),
        }
    }
    Err(BackendFailure::InvalidIr)
}

fn renamed_dependencies(
    entity: &Entity,
    renaming: &BTreeMap<String, String>,
    types: &BTreeMap<String, String>,
) -> bool {
    !entity.references.metadata.is_empty()
        || !entity.declaration_references.metadata.is_empty()
        || entity
            .references
            .symbols
            .iter()
            .chain(&entity.declaration_references.symbols)
            .any(|name| {
                renaming
                    .get(name)
                    .is_some_and(|replacement| replacement != name)
            })
        || entity
            .references
            .types
            .iter()
            .chain(&entity.declaration_references.types)
            .any(|name| {
                types
                    .get(name)
                    .is_some_and(|replacement| replacement != name)
            })
}

fn type_renaming(
    selected: &Closure,
    canonical: &Module,
    destination: &Module,
) -> Result<BTreeMap<String, String>, BackendFailure> {
    let mut renaming = BTreeMap::new();
    loop {
        let mut changed = false;
        for name in &selected.types {
            let declaration = &canonical.types[name];
            let conflict = destination
                .types
                .get(name)
                .is_some_and(|other| other != declaration);
            let dependency_changed = declaration
                .references
                .types
                .iter()
                .any(|name| renaming.contains_key(name));
            if (conflict || dependency_changed) && !renaming.contains_key(name) {
                // The emitter reserves this namespace for named types; SSA
                // values/parameters/labels use vN, tN, wf.arg., wf.slot., etc.
                // Without that producer contract `%name` is ambiguous in text.
                if !name.starts_with("wf.t.") {
                    return Err(BackendFailure::InvalidIr);
                }
                renaming.insert(name.clone(), format!("wf__par_seq_type.{name}"));
                changed = true;
            }
        }
        if !changed {
            return Ok(renaming);
        }
    }
}

fn renaming(
    selected: &Closure,
    source: &BTreeMap<String, &Entity>,
    existing: &BTreeMap<String, &Entity>,
    destination: &Module,
    types: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, BackendFailure> {
    let mut renaming = BTreeMap::new();
    for name in &selected.entities {
        let entity = source[name];
        if !entity.global {
            if let Some(suffix) = name.strip_prefix("wf_") {
                renaming.insert(name.clone(), format!("wf__par_seq_{suffix}"));
            } else if entity.linkage.is_local() {
                if existing.get(name).copied() != Some(entity) {
                    renaming.insert(name.clone(), format!("wf__par_seq.{name}"));
                }
            } else {
                return Err(BackendFailure::InvalidIr);
            }
        }
    }
    // Propagate changed addresses through helpers, globals and alias cycles.
    // Once a dependency needs a new identity it never reverts to the old one.
    loop {
        let mut changed = false;
        for name in &selected.entities {
            let entity = source[name];
            if !entity.global {
                if !renaming.contains_key(name) && renamed_dependencies(entity, &renaming, types) {
                    renaming.insert(name.clone(), format!("wf__par_seq.{name}"));
                    changed = true;
                }
                continue;
            }
            let dependencies_changed = renamed_dependencies(entity, &renaming, types);
            let same = existing.get(name).copied() == Some(entity);
            let collision =
                existing.contains_key(name) || destination.declarations.contains_key(name);
            let kind = global_kind(entity)?;
            if kind == GlobalKind::Mutable {
                if collision && (!same || dependencies_changed) {
                    // Two worlds must not silently acquire separate instances
                    // of a mutable program global with the same source identity.
                    return Err(BackendFailure::InvalidIr);
                }
                if !collision && dependencies_changed && !entity.linkage.is_local() {
                    return Err(BackendFailure::InvalidIr);
                }
            }
            if (dependencies_changed || collision && !same) && !renaming.contains_key(name) {
                renaming.insert(name.clone(), format!(".wf__par_seq_global.{name}"));
                changed = true;
            }
        }
        if !changed {
            return Ok(renaming);
        }
    }
}

fn references(
    references: &References,
    renaming: &BTreeMap<String, String>,
    types: &BTreeMap<String, String>,
    metadata: &BTreeMap<u64, u64>,
) -> References {
    References {
        symbols: references
            .symbols
            .iter()
            .map(|name| renaming.get(name).unwrap_or(name).clone())
            .collect(),
        types: references
            .types
            .iter()
            .map(|name| types.get(name).unwrap_or(name).clone())
            .collect(),
        attributes: references.attributes.clone(),
        metadata: references
            .metadata
            .iter()
            .map(|id| metadata.get(id).copied().unwrap_or(*id))
            .collect(),
    }
}

fn quoted_end(bytes: &[u8], start: usize) -> Result<usize, BackendFailure> {
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => return Ok(at + 1),
            b'\\' => at += 2,
            _ => at += 1,
        }
    }
    Err(BackendFailure::InvalidIr)
}

fn quoted_name(bytes: &[u8]) -> Result<String, BackendFailure> {
    let mut name = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            let digits = bytes.get(at + 1..at + 3).ok_or(BackendFailure::InvalidIr)?;
            let digits = std::str::from_utf8(digits).map_err(|_| BackendFailure::InvalidIr)?;
            name.push(u8::from_str_radix(digits, 16).map_err(|_| BackendFailure::InvalidIr)?);
            at += 3;
        } else {
            name.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(name).map_err(|_| BackendFailure::InvalidIr)
}

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-.$_".contains(&byte)
}

fn symbol_text(
    text: &str,
    renaming: &BTreeMap<String, String>,
    types: &BTreeMap<String, String>,
    metadata: &BTreeMap<u64, u64>,
) -> Result<String, BackendFailure> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b';' {
            let end = bytes[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |end| at + end);
            out.extend_from_slice(&bytes[at..end]);
            at = end;
        } else if bytes[at] == b'"' {
            let end = quoted_end(bytes, at)?;
            out.extend_from_slice(&bytes[at..end]);
            at = end;
        } else if bytes[at] == b'!' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit) {
            at += 1;
            let begin = at;
            while at < bytes.len() && bytes[at].is_ascii_digit() {
                at += 1;
            }
            let id = text[begin..at]
                .parse::<u64>()
                .map_err(|_| BackendFailure::InvalidIr)?;
            // The graph selected the closure before text rewriting. A token
            // outside that map is a missing producer record, not a new edge
            // to infer and never a reference to the destination's same ID.
            let replacement = metadata.get(&id).ok_or(BackendFailure::InvalidIr)?;
            out.extend_from_slice(format!("!{replacement}").as_bytes());
        } else if bytes[at] == b'@' || bytes[at] == b'%' {
            let start = at;
            let sigil = bytes[at];
            at += 1;
            let quoted = bytes.get(at) == Some(&b'"');
            let name = if quoted {
                let end = quoted_end(bytes, at)?;
                let name = quoted_name(&bytes[at + 1..end - 1])?;
                at = end;
                name
            } else {
                let begin = at;
                while at < bytes.len() && identifier_byte(bytes[at]) {
                    at += 1;
                }
                if begin == at {
                    return Err(BackendFailure::InvalidIr);
                }
                text[begin..at].to_owned()
            };
            let mapping = if sigil == b'@' { renaming } else { types };
            if let Some(replacement) = mapping.get(&name) {
                out.push(sigil);
                if quoted || !replacement.bytes().all(identifier_byte) {
                    out.push(b'"');
                    for byte in replacement.bytes() {
                        if byte == b'"' || byte == b'\\' || !byte.is_ascii_graphic() {
                            out.extend_from_slice(format!("\\{byte:02X}").as_bytes());
                        } else {
                            out.push(byte);
                        }
                    }
                    out.push(b'"');
                } else {
                    out.extend_from_slice(replacement.as_bytes());
                }
            } else {
                out.extend_from_slice(&bytes[start..at]);
            }
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).map_err(|_| BackendFailure::InvalidIr)
}

fn declaration(
    value: &Declaration,
    renaming: &BTreeMap<String, String>,
    types: &BTreeMap<String, String>,
    metadata: &BTreeMap<u64, u64>,
) -> Result<Declaration, BackendFailure> {
    Ok(Declaration {
        text: symbol_text(&value.text, renaming, types, metadata)?,
        references: references(&value.references, renaming, types, metadata),
    })
}

fn entity(
    value: &Entity,
    renaming: &BTreeMap<String, String>,
    types: &BTreeMap<String, String>,
    metadata: &BTreeMap<u64, u64>,
) -> Result<Entity, BackendFailure> {
    Ok(Entity {
        name: renaming.get(&value.name).unwrap_or(&value.name).clone(),
        header: symbol_text(&value.header, renaming, types, metadata)?,
        hidden_header: symbol_text(&value.hidden_header, renaming, types, metadata)?,
        declaration: symbol_text(&value.declaration, renaming, types, metadata)?,
        body: symbol_text(&value.body, renaming, types, metadata)?,
        declaration_references: references(
            &value.declaration_references,
            renaming,
            types,
            metadata,
        ),
        linkage: value.linkage,
        global: value.global,
        fragment_local: value.fragment_local,
        references: references(&value.references, renaming, types, metadata),
    })
}

impl Module {
    /// Imports only the recorded closure of independently emitted Off roots.
    /// Validation finishes before append, so an error leaves this module intact.
    /// The emitter must validate every actual cross-world argument/result
    /// representation before calling this: a private type namespace preserves
    /// two layouts, and cannot make a crossing between those layouts valid.
    pub(crate) fn import_canonical_closure(
        &mut self,
        canonical: &Module,
        roots: &BTreeSet<String>,
        shared: &BTreeSet<String>,
    ) -> Result<(), BackendFailure> {
        let source = entities(canonical)?;
        let existing = entities(self)?;
        let selected = closure(canonical, self, roots, shared, &source, &existing)?;
        let types = type_renaming(&selected, canonical, self)?;
        let renaming = renaming(&selected, &source, &existing, self, &types)?;
        // Numeric IDs belong to one emitted module. Freshen every selected
        // node, including self-referential distinct domains/scopes, rather
        // than equating independent functions' promises by their old IDs.
        let mut next = self.metadata.keys().next_back().copied();
        let mut metadata = BTreeMap::new();
        for id in &selected.metadata {
            let fresh = next
                .map_or(Some(0), |id| id.checked_add(1))
                .ok_or(BackendFailure::CounterOverflow)?;
            metadata.insert(*id, fresh);
            next = Some(fresh);
        }
        let mut imported = Module::default();
        for part in &canonical.parts {
            match part {
                ModulePart::Entity(index)
                    if selected.entities.contains(&canonical.entities[*index].name) =>
                {
                    let value = entity(&canonical.entities[*index], &renaming, &types, &metadata)?;
                    if let Some(previous) = existing.get(&value.name) {
                        if **previous != value {
                            return Err(BackendFailure::InvalidIr);
                        }
                    } else {
                        if self.declarations.contains_key(&value.name)
                            || imported
                                .entities
                                .iter()
                                .any(|previous| previous.name == value.name)
                        {
                            return Err(BackendFailure::InvalidIr);
                        }
                        imported.define(value);
                    }
                }
                ModulePart::Declaration(name) if selected.declarations.contains(name) => {
                    let value =
                        declaration(&canonical.declarations[name], &renaming, &types, &metadata)?;
                    if let Some(previous) = self.declarations.get(name) {
                        if previous != &value {
                            return Err(BackendFailure::InvalidIr);
                        }
                    } else if let Some(previous) = existing.get(name) {
                        if !compatible_declaration(previous, &value) {
                            return Err(BackendFailure::InvalidIr);
                        }
                    } else {
                        imported.parts.push(ModulePart::Declaration(name.clone()));
                        imported.declarations.insert(name.clone(), value);
                    }
                }
                ModulePart::Type(name) if selected.types.contains(name) => {
                    let value = declaration(&canonical.types[name], &renaming, &types, &metadata)?;
                    let imported_name = types.get(name).unwrap_or(name);
                    if let Some(previous) = self.types.get(imported_name) {
                        if previous != &value {
                            return Err(BackendFailure::InvalidIr);
                        }
                    } else {
                        imported.parts.push(ModulePart::Type(imported_name.clone()));
                        imported.types.insert(imported_name.clone(), value);
                    }
                }
                ModulePart::Metadata(id) if selected.metadata.contains(id) => {
                    let node = &canonical.metadata[id];
                    imported.parts.push(ModulePart::Metadata(metadata[id]));
                    imported.metadata.insert(
                        metadata[id],
                        Metadata {
                            body: symbol_text(&node.body, &renaming, &types, &metadata)?,
                            references: node.references.iter().map(|id| metadata[id]).collect(),
                        },
                    );
                }
                ModulePart::Attributes(id) if selected.attributes.contains(id) => {
                    let value = &canonical.attributes[id];
                    if let Some(previous) = self.attributes.get(id) {
                        if previous != value {
                            return Err(BackendFailure::InvalidIr);
                        }
                    } else {
                        imported.parts.push(ModulePart::Attributes(*id));
                        imported.attributes.insert(*id, value.clone());
                    }
                }
                _ => {}
            }
        }
        self.append(imported);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::symbol_text;
    use crate::backend::emission::{
        FunctionBody, Linkage, Module, Parameter, References, Signature,
    };
    use crate::backend::emitter::BackendFailure;
    use std::collections::{BTreeMap, BTreeSet};

    fn module() -> Module {
        let mut module = Module::default();
        module.attribute_group(0, "nounwind".to_owned());
        module
    }

    fn function(module: &mut Module, name: &str, calls: &[&str], linkage: Linkage) {
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        for call in calls {
            body.instructions(&format!("  call void @{call}()\n"), &[*call]);
        }
        body.instructions("  ret void\n", &[]);
        let mut signature = Signature::new(name, "void", Vec::new());
        signature.linkage = linkage;
        module.define(signature.define(body, "").unwrap());
    }

    fn roots(name: &str) -> BTreeSet<String> {
        [name.to_owned()].into()
    }

    fn import(destination: &mut Module, canonical: &Module) {
        destination
            .import_canonical_closure(canonical, &roots("wf_root"), &BTreeSet::new())
            .unwrap();
    }

    fn scoped_module() -> Module {
        let mut module = module();
        module.metadata_node(0, "distinct !{!0}".to_owned(), [0].into());
        module.metadata_node(1, "distinct !{!1, !0}".to_owned(), [0, 1].into());
        module.metadata_node(2, "!{!1}".to_owned(), [1].into());
        module.metadata_node(3, "!{!1}".to_owned(), [1].into());
        module.declare(Signature::new(
            "llvm.experimental.noalias.scope.decl",
            "void",
            vec![Parameter::unnamed("metadata")],
        ));
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions(
            "  call void @llvm.experimental.noalias.scope.decl(metadata !2)\n",
            &["llvm.experimental.noalias.scope.decl"],
        );
        body.instructions("  %value = load i64, ptr %source, !alias.scope !2\n  store i64 %value, ptr %destination, !noalias !3\n  ret void\n", &[]);
        body.references.metadata.extend([2, 3]);
        let signature = Signature::new(
            "wf_root",
            "void",
            vec![
                Parameter::named("ptr", "%source"),
                Parameter::named("ptr", "%destination"),
            ],
        );
        module.define(signature.define(body, "").unwrap());
        module
    }

    #[test]
    fn imported_metadata_remaps_colliding_scopes_and_all_transitive_edges() {
        let canonical = scoped_module();
        let mut destination = scoped_module();
        let original = destination.clone();
        import(&mut destination, &canonical);
        let imported = destination.entities.last().unwrap();
        assert_eq!(imported.name, "wf__par_seq_root");
        assert!(imported.body.contains("scope.decl(metadata !6)"));
        assert!(imported.body.contains("!alias.scope !6"));
        assert!(imported.body.contains("!noalias !7"));
        assert_eq!(imported.references.metadata, [6, 7].into());
        assert_eq!(destination.metadata[&4].body, "distinct !{!4}");
        assert_eq!(destination.metadata[&4].references, [4].into());
        assert_eq!(destination.metadata[&5].body, "distinct !{!5, !4}");
        assert_eq!(destination.metadata[&5].references, [4, 5].into());
        for id in [6, 7] {
            assert_eq!(destination.metadata[&id].body, "!{!5}");
            assert_eq!(destination.metadata[&id].references, [5].into());
        }
        assert_eq!(destination.entities[0], original.entities[0]);
        for (id, node) in &original.metadata {
            assert_eq!(&destination.metadata[id], node);
        }
        assert_eq!(Module::decode(&destination.encode()), Some(destination));
    }

    #[test]
    fn missing_metadata_nodes_edges_and_conflicts_fail_atomically() {
        for defect in [
            "root",
            "transitive",
            "unrecorded root",
            "unrecorded edge",
            "source conflict",
            "destination conflict",
        ] {
            let mut canonical = scoped_module();
            let mut destination = scoped_module();
            match defect {
                "root" => {
                    canonical.metadata.remove(&2);
                }
                "transitive" => {
                    canonical.metadata.remove(&0);
                }
                "unrecorded root" => {
                    canonical.entities[0].references.metadata.clear();
                }
                "unrecorded edge" => {
                    canonical
                        .metadata
                        .get_mut(&1)
                        .unwrap()
                        .references
                        .remove(&0);
                }
                "source conflict" => {
                    canonical.metadata_node(0, "distinct !{!0, i64 1}".to_owned(), [0].into())
                }
                "destination conflict" => {
                    destination.metadata_node(0, "distinct !{!0, i64 1}".to_owned(), [0].into())
                }
                _ => unreachable!(),
            }
            let before = destination.clone();
            assert_eq!(
                destination.import_canonical_closure(
                    &canonical,
                    &roots("wf_root"),
                    &BTreeSet::new()
                ),
                Err(BackendFailure::InvalidIr),
                "{defect}"
            );
            assert_eq!(destination, before, "{defect}");
        }
    }

    #[test]
    fn metadata_token_renaming_preserves_strings_comments_and_exact_numbers() {
        assert_eq!(
            symbol_text(
                "metadata !2, !20, !\"!2\", c\"!2\" ; !2\n",
                &BTreeMap::new(),
                &BTreeMap::new(),
                &BTreeMap::from([(2, 8), (20, 9)])
            )
            .unwrap(),
            "metadata !8, !9, !\"!2\", c\"!2\" ; !2\n"
        );
    }

    #[test]
    fn imports_only_recorded_closure_with_mutual_recursion_and_metadata() {
        let mut canonical = module();
        canonical.header("source_filename = \"canonical bootstrap\"".to_owned());
        canonical.named_type("T".to_owned(), "{ i64 }".to_owned(), References::default());
        canonical.named_type(
            "unused".to_owned(),
            "{ i8 }".to_owned(),
            References::default(),
        );
        canonical.attribute_group(9, "cold".to_owned());
        canonical.declare(Signature::new("native", "void", Vec::new()));
        function(
            &mut canonical,
            "wf_root",
            &["wf_helper", "native"],
            Linkage::External,
        );
        function(&mut canonical, "wf_helper", &["wf_root"], Linkage::Private);
        function(&mut canonical, "wf_unused", &[], Linkage::External);
        canonical.entities[0]
            .references
            .types
            .insert("T".to_owned());
        canonical.entities[0]
            .declaration_references
            .types
            .insert("T".to_owned());
        let mut destination = module();
        import(&mut destination, &canonical);
        let text = destination.render();
        assert!(text.contains("@wf__par_seq_root"));
        assert!(text.contains("call void @wf__par_seq_helper()"));
        assert!(text.contains("call void @wf__par_seq_root()"));
        assert!(text.contains("declare void @native()"));
        assert!(text.contains("%T = type { i64 }"));
        assert!(!text.contains("bootstrap"));
        assert!(!text.contains("unused"));
        assert!(!text.contains("attributes #9"));
        let root = destination
            .entities
            .iter()
            .find(|entity| entity.name == "wf__par_seq_root")
            .unwrap();
        assert_eq!(
            root.references.symbols,
            ["native".to_owned(), "wf__par_seq_helper".to_owned()].into()
        );
        assert!(root.declaration.contains("@wf__par_seq_root("));
        assert!(root.hidden_header.contains("@wf__par_seq_root("));
        assert_eq!(root.declaration_references.types, roots("T"));
    }

    #[test]
    fn private_named_types_keep_both_layouts_and_remap_dependent_types() {
        let mut canonical = module();
        canonical.named_type(
            "wf.t.Cell".to_owned(),
            "{ i64 }".to_owned(),
            References::default(),
        );
        canonical.named_type(
            "wf.t.Cell_long".to_owned(),
            "{ i8 }".to_owned(),
            References::default(),
        );
        canonical.named_type(
            "wf.t.Pair".to_owned(),
            "{ %wf.t.Cell }".to_owned(),
            References {
                types: roots("wf.t.Cell"),
                ..References::default()
            },
        );
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions(
            "  %v0 = alloca %wf.t.Cell\n  %v1 = alloca %wf.t.Cell_long\n  ret void\n",
            &[],
        );
        body.references
            .types
            .extend(["wf.t.Pair".to_owned(), "wf.t.Cell_long".to_owned()]);
        canonical.define(
            Signature::new("wf_root", "void", Vec::new())
                .define(body, "")
                .unwrap(),
        );
        let mut destination = module();
        destination.named_type(
            "wf.t.Cell".to_owned(),
            "{ i32 }".to_owned(),
            References::default(),
        );
        destination.named_type(
            "wf.t.Cell_long".to_owned(),
            "{ i8 }".to_owned(),
            References::default(),
        );
        import(&mut destination, &canonical);
        assert_eq!(
            destination.types["wf.t.Cell"].text,
            "%wf.t.Cell = type { i32 }"
        );
        assert_eq!(
            destination.types["wf__par_seq_type.wf.t.Cell"].text,
            "%wf__par_seq_type.wf.t.Cell = type { i64 }"
        );
        assert_eq!(
            destination.types["wf__par_seq_type.wf.t.Pair"]
                .references
                .types,
            roots("wf__par_seq_type.wf.t.Cell")
        );
        let body = &destination.entities[0].body;
        assert!(body.contains("%v0 = alloca %wf__par_seq_type.wf.t.Cell"));
        assert!(body.contains("%v1 = alloca %wf.t.Cell_long"));
        assert_eq!(
            destination.entities[0].references.types,
            [
                "wf.t.Cell_long".to_owned(),
                "wf__par_seq_type.wf.t.Pair".to_owned()
            ]
            .into()
        );
    }

    #[test]
    fn remaps_exact_symbol_tokens_and_quoted_identifiers_not_constant_text() {
        let renaming = BTreeMap::from([("wf_leaf".to_owned(), "wf__par_seq_leaf".to_owned())]);
        let text = "call @wf_leaf(), @wf_leaf_long(), @\"wf_leaf\"(), @\"wf\\5Fleaf\"()\nc\"@wf_leaf \\22 @wf_leaf_long\" ; @wf_leaf\n";
        assert_eq!(
            symbol_text(text, &renaming, &BTreeMap::new(), &BTreeMap::new()).unwrap(),
            "call @wf__par_seq_leaf(), @wf_leaf_long(), @\"wf__par_seq_leaf\"(), @\"wf__par_seq_leaf\"()\nc\"@wf_leaf \\22 @wf_leaf_long\" ; @wf_leaf\n"
        );
    }

    #[test]
    fn header_and_declaration_dependency_metadata_follow_callee_remapping() {
        let mut canonical = module();
        function(&mut canonical, "wf_personality", &[], Linkage::Private);
        let mut signature = Signature::new("wf_root", "void", Vec::new());
        signature.suffix = " personality ptr @wf_personality".to_owned();
        signature.references.symbols = roots("wf_personality");
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  ret void\n", &[]);
        canonical.define(signature.define(body, "").unwrap());
        let mut destination = module();
        import(&mut destination, &canonical);
        let root = destination
            .entities
            .iter()
            .find(|entity| entity.name == "wf__par_seq_root")
            .unwrap();
        assert!(
            root.header
                .contains("personality ptr @wf__par_seq_personality")
        );
        assert!(
            root.declaration
                .contains("personality ptr @wf__par_seq_personality")
        );
        assert_eq!(
            root.declaration_references.symbols,
            roots("wf__par_seq_personality")
        );
    }

    #[test]
    fn globals_keep_identity_or_remap_aliases_to_the_canonical_function() {
        let mut canonical = module();
        canonical.global(
            "constant".to_owned(),
            "constant",
            "i64".to_owned(),
            "7".to_owned(),
            Some(8),
            References::default(),
        );
        canonical.global(
            "callback".to_owned(),
            "constant",
            "ptr".to_owned(),
            "@wf_leaf".to_owned(),
            None,
            References {
                symbols: roots("wf_leaf"),
                ..References::default()
            },
        );
        canonical.global(
            "alias".to_owned(),
            "alias",
            "ptr,".to_owned(),
            "ptr @callback".to_owned(),
            None,
            References {
                symbols: roots("callback"),
                ..References::default()
            },
        );
        function(&mut canonical, "wf_leaf", &[], Linkage::Private);
        function(&mut canonical, "wf_root", &[], Linkage::External);
        canonical
            .entities
            .last_mut()
            .unwrap()
            .references
            .symbols
            .extend(["constant".to_owned(), "alias".to_owned()]);
        let mut destination = module();
        destination.define(canonical.entities[0].clone());
        destination.define(canonical.entities[1].clone());
        destination.define(canonical.entities[2].clone());
        import(&mut destination, &canonical);
        assert_eq!(
            destination
                .entities
                .iter()
                .filter(|entity| entity.name == "constant")
                .count(),
            1
        );
        let callback = destination
            .entities
            .iter()
            .find(|entity| entity.name == ".wf__par_seq_global.callback")
            .unwrap();
        assert!(callback.header.contains("@wf__par_seq_leaf"));
        let alias = destination
            .entities
            .iter()
            .find(|entity| entity.name == ".wf__par_seq_global.alias")
            .unwrap();
        assert!(alias.header.contains("@.wf__par_seq_global.callback"));
        assert_eq!(
            alias.references.symbols,
            roots(".wf__par_seq_global.callback")
        );
    }

    #[test]
    fn shared_safety_support_stops_before_different_body_dependencies() {
        let mut canonical = module();
        function(
            &mut canonical,
            "wf_root",
            &["wf_resource_abort"],
            Linkage::External,
        );
        function(
            &mut canonical,
            "wf_resource_abort",
            &["wf_off_diagnostic"],
            Linkage::Private,
        );
        function(&mut canonical, "wf_off_diagnostic", &[], Linkage::Private);
        let mut destination = module();
        function(&mut destination, "wf_resource_abort", &[], Linkage::Private);
        let shared_body = destination.entities[0].clone();
        destination
            .import_canonical_closure(&canonical, &roots("wf_root"), &roots("wf_resource_abort"))
            .unwrap();
        assert_eq!(destination.entities[0], shared_body);
        assert!(!destination.render().contains("off_diagnostic"));
        assert_eq!(
            destination.entities[1].references.symbols,
            roots("wf_resource_abort")
        );
    }

    #[test]
    fn shared_safety_support_requires_the_same_callable_contract() {
        let mut canonical = module();
        function(
            &mut canonical,
            "wf_root",
            &["wf_resource_abort"],
            Linkage::External,
        );
        function(&mut canonical, "wf_resource_abort", &[], Linkage::Private);
        for mismatch in ["signature", "attributes"] {
            let mut destination = module();
            if mismatch == "signature" {
                destination.declare(Signature::new(
                    "wf_resource_abort",
                    "i32",
                    vec![Parameter::unnamed("i64")],
                ));
            } else {
                function(&mut destination, "wf_resource_abort", &[], Linkage::Private);
                destination
                    .attributes
                    .insert(0, "attributes #0 = { cold }".to_owned());
            }
            let before = destination.clone();
            assert_eq!(
                destination.import_canonical_closure(
                    &canonical,
                    &roots("wf_root"),
                    &roots("wf_resource_abort")
                ),
                Err(BackendFailure::InvalidIr)
            );
            assert_eq!(destination, before);
        }
    }

    #[test]
    fn weak_runtime_hooks_keep_the_native_override_name() {
        let mut canonical = module();
        function(
            &mut canonical,
            "wf_root",
            &["wf__floor_hook"],
            Linkage::External,
        );
        function(
            &mut canonical,
            "wf__floor_hook",
            &["wf_off_unreachable"],
            Linkage::Weak,
        );
        let mut destination = module();
        function(&mut destination, "wf__floor_hook", &[], Linkage::Weak);
        import(&mut destination, &canonical);
        assert_eq!(
            destination.entities[1].references.symbols,
            roots("wf__floor_hook")
        );
        assert!(!destination.render().contains("off_unreachable"));
        let mut absent = module();
        assert_eq!(
            absent.import_canonical_closure(&canonical, &roots("wf_root"), &BTreeSet::new()),
            Err(BackendFailure::InvalidIr)
        );
    }

    #[test]
    fn weak_runtime_hooks_reject_conflicting_or_missing_attribute_meanings() {
        let mut canonical = module();
        function(
            &mut canonical,
            "wf_root",
            &["wf__floor_hook"],
            Linkage::External,
        );
        function(&mut canonical, "wf__floor_hook", &[], Linkage::Weak);
        for missing in [false, true] {
            let mut destination = module();
            function(&mut destination, "wf__floor_hook", &[], Linkage::Weak);
            if missing {
                destination.attributes.remove(&0);
            } else {
                destination
                    .attributes
                    .insert(0, "attributes #0 = { noreturn }".to_owned());
            }
            let before = destination.clone();
            assert_eq!(
                destination.import_canonical_closure(
                    &canonical,
                    &roots("wf_root"),
                    &BTreeSet::new()
                ),
                Err(BackendFailure::InvalidIr)
            );
            assert_eq!(destination, before);
        }
    }

    fn recursive_hook_module(linkage: Linkage) -> Module {
        let mut module = module();
        module.attribute_group(9, "nounwind".to_owned());
        module.named_type(
            "wf.t.Root".to_owned(),
            "{ %wf.t.Leaf* }".to_owned(),
            References {
                types: roots("wf.t.Leaf"),
                ..References::default()
            },
        );
        module.named_type(
            "wf.t.Leaf".to_owned(),
            "{ %wf.t.Root*, i64 }".to_owned(),
            References {
                types: roots("wf.t.Root"),
                attributes: [9].into(),
                ..References::default()
            },
        );
        let mut signature =
            Signature::new("wf_hook", "void", vec![Parameter::unnamed("%wf.t.Root")]);
        signature.linkage = linkage;
        signature.references.types = roots("wf.t.Root");
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  ret void\n", &[]);
        module.define(signature.define(body, "").unwrap());
        module
    }

    #[test]
    fn skipped_hooks_require_complete_matching_recursive_type_contracts() {
        for linkage in [Linkage::Private, Linkage::Weak] {
            let shared = if linkage == Linkage::Private {
                roots("wf_hook")
            } else {
                BTreeSet::new()
            };
            for mismatch in [
                "none",
                "type",
                "missing destination",
                "missing source",
                "attribute",
                "missing attribute",
            ] {
                let mut canonical = recursive_hook_module(linkage);
                function(&mut canonical, "wf_root", &["wf_hook"], Linkage::External);
                let mut destination = recursive_hook_module(linkage);
                match mismatch {
                    "type" => {
                        destination.types.get_mut("wf.t.Leaf").unwrap().text =
                            "%wf.t.Leaf = type { %wf.t.Root*, i32 }".to_owned()
                    }
                    "missing destination" => {
                        destination.types.remove("wf.t.Leaf");
                    }
                    "missing source" => {
                        canonical.types.remove("wf.t.Leaf");
                    }
                    "attribute" => {
                        destination
                            .attributes
                            .insert(9, "attributes #9 = { noreturn }".to_owned());
                    }
                    "missing attribute" => {
                        canonical.attributes.remove(&9);
                    }
                    "none" => {}
                    _ => unreachable!(),
                }
                let before = destination.clone();
                let result =
                    destination.import_canonical_closure(&canonical, &roots("wf_root"), &shared);
                if mismatch == "none" {
                    assert_eq!(result, Ok(()));
                    assert_eq!(destination.types, before.types);
                    assert_eq!(destination.entities[0], before.entities[0]);
                    assert_eq!(destination.entities[1].references.symbols, roots("wf_hook"));
                } else {
                    assert_eq!(
                        result,
                        Err(BackendFailure::InvalidIr),
                        "{linkage:?}: {mismatch}"
                    );
                    assert_eq!(destination, before, "{linkage:?}: {mismatch}");
                }
            }
        }
    }

    #[test]
    fn cleanup_helpers_reuse_unchanged_or_get_complete_symbol_namespace() {
        let mut canonical = module();
        function(&mut canonical, "wf.drop.t.cell", &[], Linkage::Private);
        function(
            &mut canonical,
            "wf.drop.run.cell",
            &["wf_leaf"],
            Linkage::Private,
        );
        function(&mut canonical, "wf_leaf", &[], Linkage::External);
        function(
            &mut canonical,
            "wf_root",
            &["wf.drop.t.cell", "wf.drop.run.cell"],
            Linkage::External,
        );
        let mut destination = module();
        destination.define(canonical.entities[0].clone());
        destination.define(canonical.entities[1].clone());
        import(&mut destination, &canonical);
        assert_eq!(
            destination
                .entities
                .iter()
                .filter(|entity| entity.name == "wf.drop.t.cell")
                .count(),
            1
        );
        let helper = destination
            .entities
            .iter()
            .find(|entity| entity.name == "wf__par_seq.wf.drop.run.cell")
            .unwrap();
        assert_eq!(helper.references.symbols, roots("wf__par_seq_leaf"));
        assert!(
            destination
                .render()
                .contains("call void @wf__par_seq.wf.drop.run.cell()")
        );
    }

    #[test]
    fn mutable_program_globals_are_shared_and_conflicts_do_not_duplicate_state() {
        let mut canonical = module();
        canonical.global(
            "state".to_owned(),
            "global",
            "i64".to_owned(),
            "0".to_owned(),
            Some(8),
            References::default(),
        );
        function(&mut canonical, "wf_root", &[], Linkage::External);
        canonical.entities[1]
            .references
            .symbols
            .insert("state".to_owned());
        let mut destination = module();
        destination.define(canonical.entities[0].clone());
        import(&mut destination, &canonical);
        assert_eq!(
            destination
                .entities
                .iter()
                .filter(|entity| entity.name == "state")
                .count(),
            1
        );
        let mut conflicting = module();
        conflicting.global(
            "state".to_owned(),
            "global",
            "i64".to_owned(),
            "9".to_owned(),
            Some(8),
            References::default(),
        );
        let before = conflicting.clone();
        assert_eq!(
            conflicting.import_canonical_closure(&canonical, &roots("wf_root"), &BTreeSet::new()),
            Err(BackendFailure::InvalidIr)
        );
        assert_eq!(conflicting, before);
    }

    #[test]
    fn missing_dependencies_and_collision_conflicts_fail_without_partial_import() {
        let mut canonical = module();
        function(&mut canonical, "wf_root", &["missing"], Linkage::External);
        let mut destination = module();
        let before = destination.clone();
        assert_eq!(
            destination.import_canonical_closure(&canonical, &roots("wf_root"), &BTreeSet::new()),
            Err(BackendFailure::InvalidIr)
        );
        assert_eq!(destination, before);
        for dependency in ["type", "attributes"] {
            let mut canonical = module();
            function(&mut canonical, "wf_root", &[], Linkage::External);
            match dependency {
                "type" => {
                    canonical.entities[0]
                        .references
                        .types
                        .insert("missing".to_owned());
                }
                "attributes" => {
                    canonical.entities[0].references.attributes.insert(99);
                }
                _ => unreachable!(),
            }
            assert_eq!(
                destination.import_canonical_closure(
                    &canonical,
                    &roots("wf_root"),
                    &BTreeSet::new()
                ),
                Err(BackendFailure::InvalidIr),
                "{dependency}"
            );
            assert_eq!(destination, before);
        }
        for conflict in ["type", "attributes", "declaration", "definition"] {
            let mut canonical = module();
            canonical.named_type("T".to_owned(), "{ i64 }".to_owned(), References::default());
            canonical.declare(Signature::new("native", "void", Vec::new()));
            function(&mut canonical, "wf_root", &["native"], Linkage::External);
            canonical.entities[0]
                .references
                .types
                .insert("T".to_owned());
            let mut destination = module();
            match conflict {
                "type" => destination.named_type(
                    "T".to_owned(),
                    "{ i32 }".to_owned(),
                    References::default(),
                ),
                "attributes" => destination.attribute_group(0, "cold".to_owned()),
                "declaration" => destination.declare(Signature::new(
                    "native",
                    "i32",
                    vec![Parameter::unnamed("i64")],
                )),
                "definition" => {
                    function(&mut destination, "wf__par_seq_root", &[], Linkage::External);
                }
                _ => unreachable!(),
            }
            let before = destination.clone();
            assert_eq!(
                destination.import_canonical_closure(
                    &canonical,
                    &roots("wf_root"),
                    &BTreeSet::new()
                ),
                Err(BackendFailure::InvalidIr),
                "{conflict}"
            );
            assert_eq!(destination, before, "{conflict}");
        }
    }

    #[test]
    fn repeated_source_type_records_do_not_hide_a_conflict() {
        let mut canonical = module();
        canonical.named_type("T".to_owned(), "{ i64 }".to_owned(), References::default());
        canonical.named_type("T".to_owned(), "{ i32 }".to_owned(), References::default());
        function(&mut canonical, "wf_root", &[], Linkage::External);
        canonical.entities[0]
            .references
            .types
            .insert("T".to_owned());
        let mut destination = module();
        let before = destination.clone();
        assert_eq!(
            destination.import_canonical_closure(&canonical, &roots("wf_root"), &BTreeSet::new()),
            Err(BackendFailure::InvalidIr)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn conflicting_reserved_type_namespace_is_not_overwritten() {
        let mut canonical = module();
        canonical.named_type(
            "wf.t.Cell".to_owned(),
            "{ i64 }".to_owned(),
            References::default(),
        );
        function(&mut canonical, "wf_root", &[], Linkage::External);
        canonical.entities[0]
            .references
            .types
            .insert("wf.t.Cell".to_owned());
        let mut destination = module();
        for name in ["wf.t.Cell", "wf__par_seq_type.wf.t.Cell"] {
            destination.named_type(name.to_owned(), "{ i8 }".to_owned(), References::default());
        }
        let before = destination.clone();
        assert_eq!(
            destination.import_canonical_closure(&canonical, &roots("wf_root"), &BTreeSet::new()),
            Err(BackendFailure::InvalidIr)
        );
        assert_eq!(destination, before);
    }
}
