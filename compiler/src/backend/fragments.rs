//! [MOD-8] The link fragments of one emitted module, split without LLVM.
//!
//! The compiler/incremental-compilation decisions retain separate IR
//! fragments, optimization plans and native objects instead of handing LLVM
//! the whole program as one module on each edit. The emitter writes one
//! structured module; this partitions its recorded definitions and dependencies
//! into the fragments a ThinLTO link joins.
//!
//! A fragment defines one group of externally visible functions, one
//! function or the functions of one source module, with the local
//! definitions that belong to the group alone: every private helper, thunk,
//! constant and global whose every use lies inside the group, directly or
//! through other such definitions. Those are exactly the local definitions
//! the group dominates in the graph of references, and they keep their local
//! linkage. A local definition that more than one group reaches is owned by
//! a fragment of its own, together with the local definitions it alone
//! reaches, and becomes `hidden`: it is defined once in the link, and every
//! user names it across the boundary, as the
//! [modular compilation design](../../../research/investigations/modular-compilation/DESIGN.md#llvm-fragments-and-optimization-regions)
//! requires of constants, release helpers, clones, variants and thunks.
//!
//! Each fragment declares what it names from the others, without parameter
//! names, and carries the named types and attribute groups its lines use; all
//! of them are written in name order. A fragment's text is therefore a
//! function of its own definitions and of the signatures they name. With
//! stable symbols and type names, an unchanged function keeps the bytes of
//! its fragment across an edit elsewhere, and with them its cached object.
//! A missing symbol, type or attribute definition fails the split.

use std::collections::{BTreeMap, BTreeSet};

/// How a module is split into link fragments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FragmentGranularity {
    /// One fragment per source module; the instances of prelude and root
    /// generic templates share one, and the build caller has its own.
    Module,
    /// One fragment per externally visible function.
    Function,
}

/// Why an emitted module could not be split.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitFailure(String);

impl core::fmt::Display for SplitFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "cannot split the emitted module: {}", self.0)
    }
}

impl std::error::Error for SplitFailure {}

fn failure(message: impl Into<String>) -> SplitFailure {
    SplitFailure(message.into())
}

use super::emission::{Module, References};
use super::emitter::LlvmModule;

/// Where one entity's definition lives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Owner {
    Group(usize),
    Root(usize),
}

/// Splits one emitted module into link fragments, in a deterministic order:
/// the groups of externally visible functions by key, then the fragments of
/// shared local definitions by symbol.
///
/// # Errors
///
/// Returns a failure when a recorded dependency has no definition or
/// declaration, or a symbol is defined more than once.
pub fn split_module(
    llvm: &LlvmModule,
    granularity: FragmentGranularity,
) -> Result<Vec<String>, SplitFailure> {
    split_emission(&llvm.model, granularity)
}

fn split_emission(
    module: &Module,
    granularity: FragmentGranularity,
) -> Result<Vec<String>, SplitFailure> {
    if let Some(name) = module.repeated_declaration() {
        return Err(failure(format!("@{name} is declared or defined twice")));
    }
    let mut names = module.declarations.keys().cloned().collect::<BTreeSet<_>>();
    for entity in &module.entities {
        if !names.insert(entity.name.clone()) {
            return Err(failure(format!(
                "@{} is declared or defined twice",
                entity.name
            )));
        }
    }
    let by_name = module
        .entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (entity.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    // What each entity names: other entities, and declared functions.
    let mut references = Vec::with_capacity(module.entities.len());
    for entity in &module.entities {
        let named = &entity.references.symbols;
        let mut entities = BTreeSet::new();
        let mut declared = BTreeSet::new();
        for name in named {
            if let Some(&index) = by_name.get(name.as_str()) {
                if module.entities[index].name != entity.name {
                    entities.insert(index);
                }
            } else if module.declarations.contains_key(name) {
                declared.insert(name.clone());
            } else {
                return Err(failure(format!(
                    "@{} names @{name}, which the module neither defines nor declares",
                    entity.name
                )));
            }
        }
        references.push((entities, declared));
    }
    // The groups of externally visible functions, by key.
    let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, entity) in module.entities.iter().enumerate() {
        if !entity.linkage.is_local() {
            let key = match granularity {
                FragmentGranularity::Function => entity.name.clone(),
                FragmentGranularity::Module => fragment_module(&entity.name),
            };
            keys.entry(key).or_default().push(index);
        }
    }
    let groups = keys.into_values().collect::<Vec<_>>();
    let owners = owners(module, &groups, &references);
    // One fragment per group, then one per shared local definition.
    let mut members = vec![Vec::new(); groups.len()];
    let mut roots: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, owner) in owners.iter().enumerate() {
        match *owner {
            Owner::Group(group) => members[group].push(index),
            Owner::Root(root) => roots
                .entry(module.entities[root].name.as_str())
                .or_default()
                .push(index),
        }
    }
    members
        .iter()
        .chain(roots.values())
        .map(|members| fragment(module, &owners, &references, members))
        .collect()
}

/// The owner of every entity: an externally visible function belongs to its
/// group; a local definition belongs to the fragment of its immediate
/// dominator, where the graph's source reaches every group and every entry
/// of the part no group reaches, and a definition reaches the local
/// definitions it names. An entry is a local definition no group reaches
/// and no other such definition names, or else the first member of a cycle
/// nothing outside it names. A local definition the source dominates
/// directly, which is such an entry or which more than one group or entry
/// reaches, owns a fragment.
fn owners(
    module: &Module,
    groups: &[Vec<usize>],
    references: &[(BTreeSet<usize>, BTreeSet<String>)],
) -> Vec<Owner> {
    // Nodes: 0 is the source, then the groups, then the local definitions.
    let locals = module
        .entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.linkage.is_local())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let node_of_local = locals
        .iter()
        .enumerate()
        .map(|(position, entity)| (*entity, 1 + groups.len() + position))
        .collect::<BTreeMap<_, _>>();
    let nodes = 1 + groups.len() + locals.len();
    let mut successors = vec![BTreeSet::new(); nodes];
    for (group, members) in groups.iter().enumerate() {
        successors[0].insert(1 + group);
        for member in members {
            for named in &references[*member].0 {
                if let Some(&node) = node_of_local.get(named) {
                    successors[1 + group].insert(node);
                }
            }
        }
    }
    for (entity, node) in &node_of_local {
        for named in &references[*entity].0 {
            if let Some(&target) = node_of_local.get(named) {
                successors[*node].insert(target);
            }
        }
    }
    // A local definition no group reaches is still written, and what it
    // names must still reach it: the source reaches the entries of that
    // part, so a definition only another unreached one names stays beside it.
    let reach = |successors: &[BTreeSet<usize>], reached: &mut [bool]| {
        for node in reverse_postorder(successors) {
            reached[node] = true;
        }
    };
    let mut reached = vec![false; nodes];
    reach(&successors, &mut reached);
    let unreached = node_of_local
        .values()
        .copied()
        .filter(|node| !reached[*node])
        .collect::<Vec<_>>();
    let named = unreached
        .iter()
        .flat_map(|node| successors[*node].iter().copied())
        .collect::<BTreeSet<_>>();
    for node in &unreached {
        if !named.contains(node) {
            successors[0].insert(*node);
        }
    }
    reach(&successors, &mut reached);
    for node in &unreached {
        if !reached[*node] {
            successors[0].insert(*node);
            reach(&successors, &mut reached);
        }
    }
    let immediate = immediate_dominators(&successors);
    let mut owners = vec![Owner::Root(0); module.entities.len()];
    for (group, members) in groups.iter().enumerate() {
        for member in members {
            owners[*member] = Owner::Group(group);
        }
    }
    // Reverse postorder visits a node's dominator before the node.
    let node_entity = node_of_local
        .iter()
        .map(|(entity, node)| (*node, *entity))
        .collect::<BTreeMap<_, _>>();
    let mut node_owner = vec![None; nodes];
    for (group, slot) in node_owner.iter_mut().skip(1).take(groups.len()).enumerate() {
        *slot = Some(Owner::Group(group));
    }
    for node in reverse_postorder(&successors) {
        let Some(&entity) = node_entity.get(&node) else {
            continue;
        };
        let owner = match immediate[node] {
            Some(0) | None => Owner::Root(entity),
            Some(dominator) => node_owner[dominator].unwrap_or(Owner::Root(entity)),
        };
        node_owner[node] = Some(owner);
        owners[entity] = owner;
    }
    owners
}

/// The nodes reachable from node 0, in reverse postorder.
fn reverse_postorder(successors: &[BTreeSet<usize>]) -> Vec<usize> {
    let mut visited = vec![false; successors.len()];
    let mut order = Vec::with_capacity(successors.len());
    let mut stack = vec![(0, successors[0].iter())];
    visited[0] = true;
    while let Some((node, children)) = stack.last_mut() {
        if let Some(&child) = children.next() {
            if !std::mem::replace(&mut visited[child], true) {
                stack.push((child, successors[child].iter()));
            }
        } else {
            order.push(*node);
            stack.pop();
        }
    }
    order.reverse();
    order
}

/// Each node's immediate dominator over the graph from node 0, by the
/// iterative intersection of Cooper, Harvey and Kennedy; `None` for a node
/// node 0 does not reach.
fn immediate_dominators(successors: &[BTreeSet<usize>]) -> Vec<Option<usize>> {
    let order = reverse_postorder(successors);
    let mut rank = vec![usize::MAX; successors.len()];
    for (position, node) in order.iter().enumerate() {
        rank[*node] = position;
    }
    let mut predecessors = vec![Vec::new(); successors.len()];
    for (node, targets) in successors.iter().enumerate() {
        for target in targets {
            predecessors[*target].push(node);
        }
    }
    let mut immediate = vec![None; successors.len()];
    immediate[0] = Some(0);
    let mut changed = true;
    while changed {
        changed = false;
        for &node in order.iter().skip(1) {
            let mut candidate: Option<usize> = None;
            for &predecessor in &predecessors[node] {
                if immediate[predecessor].is_none() {
                    continue;
                }
                candidate = Some(match candidate {
                    None => predecessor,
                    Some(current) => {
                        let (mut left, mut right) = (predecessor, current);
                        while left != right {
                            while rank[left] > rank[right] {
                                left = immediate[left].unwrap_or(0);
                            }
                            while rank[right] > rank[left] {
                                right = immediate[right].unwrap_or(0);
                            }
                        }
                        left
                    }
                });
            }
            if candidate.is_some() && immediate[node] != candidate {
                immediate[node] = candidate;
                changed = true;
            }
        }
    }
    immediate
}

/// One fragment's text: `members` defined, and declarations, types and
/// attribute groups for everything they name.
fn fragment(
    module: &Module,
    owners: &[Owner],
    references: &[(BTreeSet<usize>, BTreeSet<String>)],
    members: &[usize],
) -> Result<String, SplitFailure> {
    let inside = members.iter().copied().collect::<BTreeSet<_>>();
    let mut globals = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    let mut dependencies = References::default();
    for &member in &inside {
        let entity = &module.entities[member];
        dependencies.extend(&entity.references);
        // A local definition that owns its fragment is named from others.
        let shared = entity.linkage.is_local() && owners[member] == Owner::Root(member);
        let header = if shared {
            entity.hidden_header.clone()
        } else {
            entity.header.clone()
        };
        if entity.global {
            globals.insert(entity.name.as_str(), header);
        } else {
            definitions.insert(entity.name.as_str(), (header, &entity.body));
        }
        let (entities, declared) = &references[member];
        for &named in entities {
            if inside.contains(&named) {
                continue;
            }
            let other = &module.entities[named];
            if other.linkage.is_local() && owners[named] != Owner::Root(named) {
                return Err(failure(format!(
                    "@{} names the local definition @{}, which another fragment owns",
                    entity.name, other.name
                )));
            }
            declarations.insert(other.name.as_str(), other.declaration.clone());
            dependencies.extend(&other.declaration_references);
        }
        for name in declared {
            declarations.insert(name.as_str(), module.declarations[name].text.clone());
            dependencies.extend(&module.declarations[name].references);
        }
    }
    // Dependencies are recorded at construction; only their transitive type
    // closure is computed here. No LLVM instruction or header is reparsed.
    let mut types = BTreeSet::new();
    let mut pending = dependencies.types.into_iter().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        if types.insert(name.clone()) {
            let definition = module
                .types
                .get(&name)
                .ok_or_else(|| failure(format!("type %{name} is used but not defined")))?;
            pending.extend(definition.references.types.iter().cloned());
        }
    }
    let attributes = dependencies.attributes;
    let mut text = String::new();
    for line in &module.header {
        text.push_str(line);
        text.push('\n');
    }
    text.push('\n');
    for name in &types {
        text.push_str(&module.types[name].text);
        text.push('\n');
    }
    for line in globals.values().chain(declarations.values()) {
        text.push_str(line);
        text.push('\n');
    }
    for (header, body) in definitions.values() {
        text.push('\n');
        text.push_str(header);
        text.push('\n');
        text.push_str(body);
        text.push_str("}\n");
    }
    text.push('\n');
    for group in attributes {
        let line = module
            .attributes
            .get(&group)
            .ok_or_else(|| failure(format!("attribute group #{group} is used but not defined")))?;
        text.push_str(line);
        text.push('\n');
    }
    Ok(text)
}

/// The module fragment a symbol belongs to: its module path for a module's
/// function or an instance of one of its templates, the build caller's own
/// fragment for the launcher and runtime fallbacks, and one shared fragment
/// for instances of prelude and root generic templates.
fn fragment_module(symbol: &str) -> String {
    let Some(name) = symbol.strip_prefix("wf_") else {
        return "caller".to_owned();
    };
    if name.starts_with('_') {
        return "caller".to_owned();
    }
    let (base, instance) = match name.split_once("$instance$") {
        Some((base, _)) => (base, true),
        None => (name, false),
    };
    match base.rsplit_once('.') {
        Some((module, _)) => format!("module {module}"),
        None if instance => "instances".to_owned(),
        None => "module pkg".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FragmentGranularity, fragment_module, split_emission};
    use crate::backend::emission::{
        FunctionBody, Linkage, Module, Parameter, References, Signature,
    };

    /// A module in the emitter's form: `pkg::a::f` and `pkg::b::g` share a
    /// release helper and a latch; `f` alone reaches a helper that reaches a
    /// constant; `g` alone reads a table; `main` calls both; a weak runtime
    /// fallback stands beside them, and one helper nothing names.
    fn module(g_body: &str) -> Module {
        let mut module = Module::default();
        module.header("source_filename = \"whitefoot\"".to_owned());
        module.header("target datalayout = \"e-m:e-i64:64-n8:16:32:64-S128\"".to_owned());
        module.header("target triple = \"x86_64-unknown-linux-gnu\"".to_owned());
        module.named_type(
            "wf.t.aa".to_owned(),
            "{ i64, %wf.t.bb }".to_owned(),
            References {
                types: ["wf.t.bb".to_owned()].into(),
                symbols: [].into(),
                ..References::default()
            },
        );
        module.named_type(
            "wf.t.bb".to_owned(),
            "{ i8, i8 }".to_owned(),
            References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            },
        );
        module.named_type(
            "wf.t.cc".to_owned(),
            "{ i32 }".to_owned(),
            References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            },
        );
        module.global(
            ".wf_const.k1".to_owned(),
            "unnamed_addr constant",
            "[2 x i8]".to_owned(),
            "[i8 1, i8 2]".to_owned(),
            Some(1),
            References::default(),
        );
        module.global(
            ".wf_const.t1".to_owned(),
            "unnamed_addr constant",
            "[2 x i32]".to_owned(),
            "[i32 7, i32 9]".to_owned(),
            Some(4),
            References::default(),
        );
        module.global(
            ".wf_resource_record.latch".to_owned(),
            "global",
            "i32".to_owned(),
            "0".to_owned(),
            Some(4),
            References::default(),
        );
        let mut signature = Signature::new("abort", "void", vec![]);
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        module.declare(signature);
        let mut signature = Signature::new(
            "write",
            "i64",
            vec![
                Parameter::unnamed("i32"),
                Parameter::unnamed("ptr"),
                Parameter::unnamed("i64"),
            ],
        );
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        module.declare(signature);
        let mut signature = Signature::new(
            "wf_a.f",
            "i64",
            vec![
                Parameter::named("ptr noalias nonnull", "%v0"),
                Parameter::named("i64", "%v1"),
            ],
        );
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.references.types.extend(["wf.t.aa".to_owned()]);
        body.instructions(
            "  call void @wf.drop.t.aa(%wf.t.aa zeroinitializer)\n",
            &["wf.drop.t.aa"],
        );
        body.instructions("  %v2 = call i64 @wf_f_helper(i64 %v1)\n", &["wf_f_helper"]);
        body.instructions(
            "  store i32 1, ptr @.wf_resource_record.latch, align 4\n",
            &[".wf_resource_record.latch"],
        );
        body.instructions("  ret i64 %v2\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature =
            Signature::new("wf_f_helper", "i64", vec![Parameter::named("i64", "%v0")]);
        signature.linkage = Linkage::Private;
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions(
            "  %v1 = load i8, ptr @.wf_const.k1, align 1\n",
            &[".wf_const.k1"],
        );
        body.instructions("  ret i64 %v0\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature = Signature::new(
            "wf.drop.t.aa",
            "void",
            vec![Parameter::named("%wf.t.aa", "%value")],
        );
        signature.linkage = Linkage::Private;
        signature.references = References {
            types: ["wf.t.aa".to_owned()].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  ret void\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature = Signature::new("wf.unused", "void", vec![]);
        signature.linkage = Linkage::Private;
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  call void @abort()\n", &["abort"]);
        body.instructions("  ret void\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature = Signature::new("wf_b.g", "i32", vec![Parameter::named("i32", "%v0")]);
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions(g_body, &[]);
        body.push('\n');
        body.references.types.extend(["wf.t.aa".to_owned()]);
        body.instructions(
            "  call void @wf.drop.t.aa(%wf.t.aa zeroinitializer)\n",
            &["wf.drop.t.aa"],
        );
        body.instructions(
            "  %v1 = load i32, ptr @.wf_const.t1, align 4\n",
            &[".wf_const.t1"],
        );
        body.instructions(
            "  %v2 = load i32, ptr @.wf_resource_record.latch, align 4\n",
            &[".wf_resource_record.latch"],
        );
        body.instructions("  ret i32 %v1\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature =
            Signature::new("wf__floor_run", "i32", vec![Parameter::named("ptr", "%v0")]);
        signature.linkage = Linkage::Weak;
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  ret i32 0\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        let mut signature = Signature::new(
            "main",
            "i32",
            vec![
                Parameter::named("i32", "%argc"),
                Parameter::named("ptr", "%argv"),
            ],
        );
        signature.references = References {
            types: [].into(),
            symbols: [].into(),
            ..References::default()
        };
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  %v0 = call i64 @wf_a.f(ptr null, i64 1)\n", &["wf_a.f"]);
        body.instructions("  %v1 = call i32 @wf_b.g(i32 2)\n", &["wf_b.g"]);
        body.instructions(
            "  %v2 = call i32 @wf__floor_run(ptr null)\n",
            &["wf__floor_run"],
        );
        body.instructions("  ret i32 %v1\n", &[]);
        module.define(signature.define(body, "").expect("fixture definition"));
        module.attribute_group(0, "\"probe-stack\"=\"inline-asm\"".to_owned());
        module
    }

    fn defining<'fragments>(fragments: &'fragments [String], line: &str) -> Vec<&'fragments str> {
        fragments
            .iter()
            .filter(|fragment| {
                fragment
                    .lines()
                    .any(|candidate| candidate.starts_with(line))
            })
            .map(String::as_str)
            .collect()
    }

    /// Every definition lands in exactly one fragment; what one group alone
    /// reaches stays local beside it; what several reach is defined once,
    /// hidden, in a fragment of its own, and declared by its users.
    #[test]
    fn each_definition_has_one_owner_and_shared_definitions_become_hidden() {
        let fragments = split_emission(
            &module("  %v9 = add i32 %v0, 1"),
            FragmentGranularity::Function,
        )
        .expect("the module splits");
        // `wf_a.f`, `wf_b.g`, `wf__floor_run`, `main`; the release helper,
        // the latch and the unreached helper.
        assert_eq!(fragments.len(), 7, "{fragments:#?}");
        for definition in [
            "define i64 @wf_a.f(",
            "define private i64 @wf_f_helper(",
            "@.wf_const.k1 = private unnamed_addr constant",
            "define i32 @wf_b.g(",
            "@.wf_const.t1 = private unnamed_addr constant",
            "define hidden void @wf.drop.t.aa(",
            "@.wf_resource_record.latch = hidden global i32 0, align 4",
            "define hidden void @wf.unused(",
            "define weak i32 @wf__floor_run(",
            "define i32 @main(",
        ] {
            assert_eq!(
                defining(&fragments, definition).len(),
                1,
                "{definition} must be defined exactly once: {fragments:#?}"
            );
        }
        let f = defining(&fragments, "define i64 @wf_a.f(")[0];
        assert!(f.contains("define private i64 @wf_f_helper("), "{f}");
        assert!(
            f.contains("@.wf_const.k1 = private unnamed_addr constant"),
            "{f}"
        );
        assert!(
            f.contains("declare hidden void @wf.drop.t.aa(%wf.t.aa) #0\n"),
            "{f}"
        );
        assert!(
            f.contains("@.wf_resource_record.latch = external hidden global i32, align 4\n"),
            "{f}"
        );
        // The named types the fragment's lines use, and theirs.
        assert!(
            f.contains("%wf.t.aa = type") && f.contains("%wf.t.bb = type"),
            "{f}"
        );
        assert!(!f.contains("%wf.t.cc"), "{f}");
        assert!(f.contains("attributes #0 = "), "{f}");
        let main = defining(&fragments, "define i32 @main(")[0];
        assert!(
            main.contains("declare i64 @wf_a.f(ptr noalias nonnull, i64) #0\n"),
            "{main}"
        );
        assert!(
            main.contains("declare i32 @wf__floor_run(ptr) #0\n"),
            "{main}"
        );
        let unused = defining(&fragments, "define hidden void @wf.unused(")[0];
        assert!(unused.contains("declare void @abort()\n"), "{unused}");
    }

    /// A helper nothing calls is still written once, and what it calls is
    /// reached from two fragments, so it owns one of its own.
    #[test]
    fn an_unreached_helper_keeps_what_it_names_reachable() {
        let module = {
            let mut module = Module::default();
            module.header("target triple = \"x86_64-unknown-linux-gnu\"".to_owned());
            let mut signature = Signature::new("wf_a.f", "i32", vec![]);
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  call void @wf.drop.t.x()\n", &["wf.drop.t.x"]);
            body.instructions("  ret i32 0\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.drop.t.x", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.drop.t.y", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  call void @wf.drop.t.x()\n", &["wf.drop.t.x"]);
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            module.attribute_group(0, "nounwind".to_owned());
            module
        };
        let fragments =
            split_emission(&module, FragmentGranularity::Function).expect("the module splits");
        assert_eq!(fragments.len(), 3, "{fragments:#?}");
        for definition in [
            "define i32 @wf_a.f(",
            "define hidden void @wf.drop.t.x(",
            "define hidden void @wf.drop.t.y(",
        ] {
            assert_eq!(defining(&fragments, definition).len(), 1, "{fragments:#?}");
        }
        for user in ["define i32 @wf_a.f(", "define hidden void @wf.drop.t.y("] {
            let fragment = defining(&fragments, user)[0];
            assert!(
                fragment.contains("declare hidden void @wf.drop.t.x() #0\n"),
                "{fragment}"
            );
        }
    }

    /// A dead helper chain keeps its shape: the helper only another dead
    /// helper calls stays local beside it, and a dead cycle is owned by its
    /// first member.
    #[test]
    fn an_unreached_chain_keeps_its_callee_beside_it() {
        let module = {
            let mut module = Module::default();
            module.header("target triple = \"x86_64-unknown-linux-gnu\"".to_owned());
            let mut signature = Signature::new("wf_a.f", "i32", vec![]);
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  ret i32 0\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.drop.t.b", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.drop.t.a", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  call void @wf.drop.t.b()\n", &["wf.drop.t.b"]);
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.cycle.one", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  call void @wf.cycle.two()\n", &["wf.cycle.two"]);
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            let mut signature = Signature::new("wf.cycle.two", "void", vec![]);
            signature.linkage = Linkage::Private;
            signature.references = References {
                types: [].into(),
                symbols: [].into(),
                ..References::default()
            };
            let mut body = FunctionBody::default();
            body.open_block("entry".to_owned());
            body.instructions("  call void @wf.cycle.one()\n", &["wf.cycle.one"]);
            body.instructions("  ret void\n", &[]);
            module.define(signature.define(body, "").expect("fixture definition"));
            module.attribute_group(0, "nounwind".to_owned());
            module
        };
        let fragments =
            split_emission(&module, FragmentGranularity::Function).expect("the module splits");
        assert_eq!(fragments.len(), 3, "{fragments:#?}");
        let chain = defining(&fragments, "define hidden void @wf.drop.t.a(");
        assert_eq!(chain.len(), 1, "{fragments:#?}");
        assert!(
            chain[0].contains("define private void @wf.drop.t.b("),
            "{}",
            chain[0]
        );
        let cycle = defining(&fragments, "define hidden void @wf.cycle.one(");
        assert_eq!(cycle.len(), 1, "{fragments:#?}");
        assert!(
            cycle[0].contains("define private void @wf.cycle.two("),
            "{}",
            cycle[0]
        );
    }

    /// A source module's functions share a fragment, and the local
    /// definitions only that module reaches stay beside them.
    #[test]
    fn module_fragments_group_a_modules_functions() {
        let fragments = split_emission(
            &module("  %v9 = add i32 %v0, 1"),
            FragmentGranularity::Module,
        )
        .expect("the module splits");
        let caller = defining(&fragments, "define i32 @main(")[0];
        assert!(
            caller.contains("define weak i32 @wf__floor_run("),
            "{caller}"
        );
        let f = defining(&fragments, "define i64 @wf_a.f(")[0];
        assert!(!f.contains("define i32 @wf_b.g("), "{f}");
    }

    /// Editing one function's body leaves every other fragment's bytes as
    /// they were.
    #[test]
    fn an_edit_changes_only_the_fragment_of_the_edited_function() {
        for granularity in [FragmentGranularity::Function, FragmentGranularity::Module] {
            let before = split_emission(&module("  %v9 = add i32 %v0, 1"), granularity)
                .expect("the module splits");
            let after = split_emission(&module("  %v9 = mul i32 %v0, 3"), granularity)
                .expect("the module splits");
            assert_eq!(before.len(), after.len());
            let changed = before
                .iter()
                .zip(&after)
                .filter(|(before, after)| before != after)
                .count();
            assert_eq!(changed, 1, "{granularity:?}: {before:#?} {after:#?}");
        }
    }

    /// Missing dependency records and duplicate definitions cannot produce a
    /// fragment. The retired text-parser tests for unsupported top-level LLVM
    /// and linkage spellings are replaced by this model integrity boundary:
    /// those spellings have no constructor in the emission model.
    #[test]
    fn incomplete_or_duplicate_module_records_are_refused() {
        let base = module("  %v9 = add i32 %v0, 1");
        assert!(split_emission(&base, FragmentGranularity::Function).is_ok());
        let mut missing_symbol = base.clone();
        missing_symbol.declarations.remove("abort");
        let mut missing_type = base.clone();
        missing_type.types.remove("wf.t.bb");
        let mut missing_attribute = base.clone();
        missing_attribute.attributes.remove(&0);
        let mut duplicate = base.clone();
        duplicate.entities.push(duplicate.entities[0].clone());
        let mut duplicate_declaration = base.clone();
        duplicate_declaration.declare(Signature::new("abort", "void", Vec::new()));
        for (name, edited) in [
            ("symbol", missing_symbol),
            ("transitive type", missing_type),
            ("attribute", missing_attribute),
            ("duplicate definition", duplicate),
            ("duplicate declaration", duplicate_declaration),
        ] {
            assert!(
                split_emission(&edited, FragmentGranularity::Function).is_err(),
                "{name} must be refused"
            );
        }
    }

    /// Quoting at an instruction's printing site does not change the symbol
    /// identity carried by its dependency record.
    #[test]
    fn a_quoted_reference_uses_its_recorded_symbol_identity() {
        let mut input = module("  %v9 = add i32 %v0, 1");
        let main = input
            .entities
            .iter_mut()
            .find(|entity| entity.name == "main")
            .expect("main");
        main.body = main.body.replace("@wf_b.g", "@\"wf_b.g\"");
        let fragments =
            split_emission(&input, FragmentGranularity::Function).expect("recorded name");
        let main = defining(&fragments, "define i32 @main(")[0];
        assert!(main.contains("declare i32 @wf_b.g(i32) #0"));
        assert!(main.contains("@\"wf_b.g\""));
    }

    /// A fragment groups a module's functions and the instances of its
    /// templates, keeps the build caller apart, and puts instances of the
    /// prelude's and the root module's templates together.
    #[test]
    fn a_fragment_follows_the_module_that_owns_its_functions() {
        assert_eq!(
            fragment_module("wf_runtime.queue.push"),
            "module runtime.queue"
        );
        assert_eq!(
            fragment_module("wf_runtime.run_two$instance$37"),
            "module runtime"
        );
        assert_eq!(fragment_module("wf_start"), "module pkg");
        assert_eq!(fragment_module("wf_box_new$instance$39"), "instances");
        assert_eq!(fragment_module("wf__main_body"), "caller");
        assert_eq!(fragment_module("main"), "caller");
    }
}
