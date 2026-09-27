//! [MOD-8] The link fragments of one emitted module, split without LLVM.
//!
//! The compiler/incremental-compilation decisions retain separate IR
//! fragments, optimization plans and native objects instead of handing LLVM
//! the whole program as one module on each edit. The emitter writes one
//! textual module in a closed, regular subset of LLVM's form; this splits it
//! into the fragments a ThinLTO link joins, reading each top-level line once.
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
//! A top-level line outside the emitter's form fails the split rather than
//! being guessed at.

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

/// How far a definition is visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Linkage {
    /// Visible to the link and prevailing there.
    External,
    /// Visible to the link, where another definition may prevail.
    Weak,
    /// Visible only inside the module: `private` or `internal`.
    Local,
}

/// One top-level definition of a symbol: a function with its body, or a
/// global variable.
struct Entity<'module> {
    name: String,
    /// The `define` line or the global's line.
    header: &'module str,
    /// A function's body lines through its closing brace; empty for a
    /// global.
    body: Vec<&'module str>,
    linkage: Linkage,
    global: bool,
}

/// The top-level lines of one emitted module.
#[derive(Default)]
struct Module<'module> {
    header: Vec<&'module str>,
    types: BTreeMap<String, &'module str>,
    declarations: BTreeMap<String, &'module str>,
    entities: Vec<Entity<'module>>,
    attributes: BTreeMap<u64, &'module str>,
}

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
/// Returns a failure when the module holds a top-level line outside the
/// emitter's form, or a function names a symbol the module neither defines
/// nor declares.
pub fn split_module(
    llvm: &str,
    granularity: FragmentGranularity,
) -> Result<Vec<String>, SplitFailure> {
    let module = parse(llvm)?;
    let by_name = module
        .entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (entity.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    // What each entity names: other entities, and declared functions.
    let mut references = Vec::with_capacity(module.entities.len());
    for entity in &module.entities {
        let mut named = BTreeSet::new();
        symbol_references(entity.header, &mut named);
        for line in &entity.body {
            symbol_references(line, &mut named);
        }
        let mut entities = BTreeSet::new();
        let mut declared = BTreeSet::new();
        for name in named {
            if let Some(&index) = by_name.get(name.as_str()) {
                if module.entities[index].name != entity.name {
                    entities.insert(index);
                }
            } else if module.declarations.contains_key(&name) {
                declared.insert(name);
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
        if entity.linkage != Linkage::Local {
            let key = match granularity {
                FragmentGranularity::Function => entity.name.clone(),
                FragmentGranularity::Module => fragment_module(&entity.name),
            };
            keys.entry(key).or_default().push(index);
        }
    }
    let groups = keys.into_values().collect::<Vec<_>>();
    let owners = owners(&module, &groups, &references);
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
        .map(|members| fragment(&module, &owners, &references, members))
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
    module: &Module<'_>,
    groups: &[Vec<usize>],
    references: &[(BTreeSet<usize>, BTreeSet<String>)],
) -> Vec<Owner> {
    // Nodes: 0 is the source, then the groups, then the local definitions.
    let locals = module
        .entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.linkage == Linkage::Local)
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
    module: &Module<'_>,
    owners: &[Owner],
    references: &[(BTreeSet<usize>, BTreeSet<String>)],
    members: &[usize],
) -> Result<String, SplitFailure> {
    let inside = members.iter().copied().collect::<BTreeSet<_>>();
    let mut globals = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    for &member in &inside {
        let entity = &module.entities[member];
        // A local definition that owns its fragment is named from others.
        let shared = entity.linkage == Linkage::Local && owners[member] == Owner::Root(member);
        let header = if shared {
            hidden(entity)?
        } else {
            entity.header.to_owned()
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
            if other.linkage == Linkage::Local && owners[named] != Owner::Root(named) {
                return Err(failure(format!(
                    "@{} names the local definition @{}, which another fragment owns",
                    entity.name, other.name
                )));
            }
            declarations.insert(other.name.as_str(), declaration(other)?);
        }
        for name in declared {
            declarations.insert(name.as_str(), module.declarations[name].to_owned());
        }
    }
    let mut lines: Vec<&str> = Vec::new();
    for line in globals.values().chain(declarations.values()) {
        lines.push(line);
    }
    for (header, body) in definitions.values() {
        lines.push(header);
        lines.extend(body.iter().copied());
    }
    // The named types these lines use, closed over the types' own fields.
    let mut types = BTreeSet::new();
    let mut pending = Vec::new();
    for line in &lines {
        type_references(line, module, &mut pending);
    }
    while let Some(name) = pending.pop() {
        if types.insert(name.clone()) {
            type_references(module.types[&name], module, &mut pending);
        }
    }
    let mut attributes = BTreeSet::new();
    for line in &lines {
        attribute_references(line, &mut attributes);
    }
    let mut text = String::new();
    for line in &module.header {
        text.push_str(line);
        text.push('\n');
    }
    text.push('\n');
    for name in &types {
        text.push_str(module.types[name]);
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
        for line in *body {
            text.push_str(line);
            text.push('\n');
        }
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

fn parse(llvm: &str) -> Result<Module<'_>, SplitFailure> {
    let mut module = Module::default();
    let mut names = BTreeSet::new();
    let mut lines = llvm.lines();
    while let Some(line) = lines.next() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let entity = if let Some(rest) = line.strip_prefix("define ") {
            let linkage = match rest.split_once(' ').map(|(first, _)| first) {
                Some("private" | "internal") => Linkage::Local,
                Some("weak") => Linkage::Weak,
                Some(
                    "external"
                    | "linkonce"
                    | "linkonce_odr"
                    | "weak_odr"
                    | "common"
                    | "appending"
                    | "extern_weak"
                    | "available_externally"
                    | "hidden"
                    | "protected"
                    | "dllexport"
                    | "dllimport",
                ) => return Err(failure(format!("a definition of another form: {line}"))),
                _ => Linkage::External,
            };
            if !line.ends_with(" {") {
                return Err(failure(format!(
                    "a definition header without its body: {line}"
                )));
            }
            let mut body = Vec::new();
            loop {
                let next = lines
                    .next()
                    .ok_or_else(|| failure(format!("a definition without its end: {line}")))?;
                body.push(next);
                if next == "}" {
                    break;
                }
            }
            Some(Entity {
                name: symbol_after_at(rest)
                    .ok_or_else(|| failure(format!("a definition without a name: {line}")))?,
                header: line,
                body,
                linkage,
                global: false,
            })
        } else if line.starts_with('@') {
            let (_, rest) = line
                .split_once(" = ")
                .ok_or_else(|| failure(format!("a global of another form: {line}")))?;
            let local = ["private ", "internal "]
                .iter()
                .any(|linkage| rest.starts_with(linkage));
            let kind = rest
                .split_once(' ')
                .map(|(_, kind)| kind.strip_prefix("unnamed_addr ").unwrap_or(kind));
            if !local
                || !kind.is_some_and(|kind| {
                    kind.starts_with("constant ") || kind.starts_with("global ")
                })
            {
                return Err(failure(format!("a global of another form: {line}")));
            }
            Some(Entity {
                name: symbol_after_at(line)
                    .ok_or_else(|| failure(format!("a global without a name: {line}")))?,
                header: line,
                body: Vec::new(),
                linkage: Linkage::Local,
                global: true,
            })
        } else if let Some(rest) = line.strip_prefix("declare ") {
            let name = symbol_after_at(rest)
                .ok_or_else(|| failure(format!("a declaration without a name: {line}")))?;
            if !names.insert(name.clone()) {
                return Err(failure(format!("@{name} is declared or defined twice")));
            }
            module.declarations.insert(name, line);
            None
        } else if let Some(rest) = line.strip_prefix('%') {
            let (name, _) = rest
                .split_once(" = type ")
                .ok_or_else(|| failure(format!("an unrecognized line: {line}")))?;
            module.types.insert(name.to_owned(), line);
            None
        } else if let Some(rest) = line.strip_prefix("attributes #") {
            let group = rest
                .split_once(" = ")
                .and_then(|(group, _)| group.parse().ok())
                .ok_or_else(|| failure(format!("an unrecognized line: {line}")))?;
            module.attributes.insert(group, line);
            None
        } else if line.starts_with("source_filename = ") || line.starts_with("target ") {
            module.header.push(line);
            None
        } else {
            return Err(failure(format!("an unrecognized line: {line}")));
        };
        if let Some(entity) = entity {
            if !names.insert(entity.name.clone()) {
                return Err(failure(format!(
                    "@{} is declared or defined twice",
                    entity.name
                )));
            }
            module.entities.push(entity);
        }
    }
    Ok(module)
}

/// The symbol named after the first `@` of `text`, quoted or bare.
fn symbol_after_at(text: &str) -> Option<String> {
    let at = text.find('@')?;
    symbol_at(text.get(at + 1..)?).map(|(name, _)| name)
}

/// The symbol at the start of `text`, which follows an `@` or `%`, and the
/// bytes it spans.
fn symbol_at(text: &str) -> Option<(String, usize)> {
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some((quoted.get(..end)?.to_owned(), end + 2));
    }
    let length = text
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || b"-$._".contains(byte))
        .count();
    (length > 0).then(|| (text[..length].to_owned(), length))
}

/// Calls `found` with each name a line writes after `sigil`, outside its
/// string constants and its comment.
fn sigil_names(line: &str, sigil: u8, mut found: impl FnMut(String)) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'"' && index > 0 && bytes[index - 1] == b'c' {
            // A `c"..."` string constant is data.
            index += 1;
            while bytes.get(index).is_some_and(|byte| *byte != b'"') {
                index += 1;
            }
            index += 1;
        } else if byte == b';' {
            return;
        } else if byte == sigil
            && let Some((name, length)) = line.get(index + 1..).and_then(symbol_at)
        {
            found(name);
            index += 1 + length;
        } else {
            index += 1;
        }
    }
}

/// Every `@symbol` a line names.
fn symbol_references(line: &str, named: &mut BTreeSet<String>) {
    sigil_names(line, b'@', |name| {
        named.insert(name);
    });
}

/// Every named type a line uses.
fn type_references(line: &str, module: &Module<'_>, found: &mut Vec<String>) {
    sigil_names(line, b'%', |name| {
        if module.types.contains_key(&name) {
            found.push(name);
        }
    });
}

/// Every attribute group, such as `#0`, a line names.
fn attribute_references(line: &str, found: &mut BTreeSet<u64>) {
    let bytes = line.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'#' || index == 0 || bytes[index - 1] != b' ' {
            continue;
        }
        let digits = bytes[index + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if let Some(group) = line
            .get(index + 1..=index + digits)
            .and_then(|digits| digits.parse().ok())
        {
            found.insert(group);
        }
    }
}

/// A local definition's line with `hidden` for its local linkage, so the
/// fragments that name it reach its one definition.
fn hidden(entity: &Entity<'_>) -> Result<String, SplitFailure> {
    let (prefix, rest) = if entity.global {
        let (name, rest) = entity
            .header
            .split_once(" = ")
            .ok_or_else(|| failure(format!("a global of another form: {}", entity.header)))?;
        (format!("{name} = "), rest)
    } else {
        (
            "define ".to_owned(),
            entity
                .header
                .strip_prefix("define ")
                .unwrap_or(entity.header),
        )
    };
    let rest = rest
        .strip_prefix("private ")
        .or_else(|| rest.strip_prefix("internal "))
        .ok_or_else(|| {
            failure(format!(
                "a local definition of another form: {}",
                entity.header
            ))
        })?;
    Ok(format!("{prefix}hidden {rest}"))
}

/// The declaration another fragment writes for `entity`: a function's header
/// without linkage, parameter names or body, or a global's type without its
/// initializer, `hidden` when the definition was local.
fn declaration(entity: &Entity<'_>) -> Result<String, SplitFailure> {
    let visibility = if entity.linkage == Linkage::Local {
        "hidden "
    } else {
        ""
    };
    let other = || failure(format!("a definition of another form: {}", entity.header));
    if entity.global {
        // `@name = LINKAGE [unnamed_addr] constant|global TYPE VALUE[, align N]`
        let (name, rest) = entity.header.split_once(" = ").ok_or_else(other)?;
        let (_, rest) = rest.split_once(' ').ok_or_else(other)?;
        let rest = rest.strip_prefix("unnamed_addr ").unwrap_or(rest);
        let (kind, rest) = rest.split_once(' ').ok_or_else(other)?;
        let ty = leading_type(rest).ok_or_else(other)?;
        let align = rest
            .rsplit_once(", align ")
            .filter(|(_, align)| {
                !align.is_empty() && align.bytes().all(|byte| byte.is_ascii_digit())
            })
            .map_or_else(String::new, |(_, align)| format!(", align {align}"));
        return Ok(format!("{name} = external {visibility}{kind} {ty}{align}"));
    }
    let rest = entity
        .header
        .strip_prefix("define ")
        .and_then(|rest| rest.strip_suffix(" {"))
        .ok_or_else(other)?;
    let rest = ["private ", "internal ", "weak "]
        .iter()
        .find_map(|linkage| rest.strip_prefix(linkage))
        .unwrap_or(rest);
    let at = rest.find('@').ok_or_else(other)?;
    let open = at + rest[at..].find('(').ok_or_else(other)?;
    let close = matching_parenthesis(rest, open).ok_or_else(other)?;
    Ok(format!(
        "declare {visibility}{}({}){}",
        &rest[..open],
        unnamed_parameters(&rest[open + 1..close]),
        &rest[close + 1..]
    ))
}

/// The type at the start of `text`: one bracketed aggregate, or one word.
fn leading_type(text: &str) -> Option<&str> {
    match text.bytes().next()? {
        b'[' | b'{' | b'<' => {
            let mut depth = 0_usize;
            for (index, byte) in text.bytes().enumerate() {
                match byte {
                    b'[' | b'{' | b'<' => depth += 1,
                    b']' | b'}' | b'>' => {
                        depth = depth.checked_sub(1)?;
                        if depth == 0 {
                            return text.get(..=index);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        _ => text.split(' ').next(),
    }
}

/// The index of the parenthesis closing the one at `open`.
fn matching_parenthesis(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// A parameter list without its parameter names, so a declaration names
/// only the signature.
fn unnamed_parameters(parameters: &str) -> String {
    let mut pieces = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (index, byte) in parameters.bytes().enumerate() {
        match byte {
            b'(' | b'{' | b'[' | b'<' => depth += 1,
            b')' | b'}' | b']' | b'>' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                pieces.push(&parameters[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if !parameters.trim().is_empty() {
        pieces.push(&parameters[start..]);
    }
    pieces
        .into_iter()
        .map(|piece| {
            let piece = piece.trim();
            match piece.rsplit_once(' ') {
                Some((kept, name)) if name.starts_with('%') => kept,
                _ => piece,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    use super::{FragmentGranularity, fragment_module, split_module, symbol_after_at};

    /// A module in the emitter's form: `pkg::a::f` and `pkg::b::g` share a
    /// release helper and a latch; `f` alone reaches a helper that reaches a
    /// constant; `g` alone reads a table; `main` calls both; a weak runtime
    /// fallback stands beside them, and one helper nothing names.
    fn module(g_body: &str) -> String {
        format!(
            "source_filename = \"whitefoot\"
target datalayout = \"e-m:e-i64:64-n8:16:32:64-S128\"
target triple = \"x86_64-unknown-linux-gnu\"

%wf.t.aa = type {{ i64, %wf.t.bb }}
%wf.t.bb = type {{ i8, i8 }}
%wf.t.cc = type {{ i32 }}
@.wf_const.k1 = private unnamed_addr constant [2 x i8] [i8 1, i8 2], align 1
@.wf_const.t1 = private unnamed_addr constant [2 x i32] [i32 7, i32 9], align 4
@.wf_resource_record.latch = private global i32 0, align 4
declare void @abort()
declare i64 @write(i32, ptr, i64)

define i64 @wf_a.f(ptr noalias nonnull %v0, i64 %v1) #0 {{
entry:
  call void @wf.drop.t.aa(%wf.t.aa zeroinitializer)
  %v2 = call i64 @wf_f_helper(i64 %v1)
  store i32 1, ptr @.wf_resource_record.latch, align 4
  ret i64 %v2
}}

define private i64 @wf_f_helper(i64 %v0) #0 {{
entry:
  %v1 = load i8, ptr @.wf_const.k1, align 1
  ret i64 %v0
}}

define private void @wf.drop.t.aa(%wf.t.aa %value) #0 {{
entry:
  ret void
}}

define private void @wf.unused() #0 {{
entry:
  call void @abort()
  ret void
}}

define i32 @wf_b.g(i32 %v0) #0 {{
entry:
{g_body}
  call void @wf.drop.t.aa(%wf.t.aa zeroinitializer)
  %v1 = load i32, ptr @.wf_const.t1, align 4
  %v2 = load i32, ptr @.wf_resource_record.latch, align 4
  ret i32 %v1
}}

define weak i32 @wf__floor_run(ptr %v0) #0 {{
entry:
  ret i32 0
}}

define i32 @main(i32 %argc, ptr %argv) #0 {{
entry:
  %v0 = call i64 @wf_a.f(ptr null, i64 1)
  %v1 = call i32 @wf_b.g(i32 2)
  %v2 = call i32 @wf__floor_run(ptr null)
  ret i32 %v1
}}

attributes #0 = {{ \"probe-stack\"=\"inline-asm\" }}
"
        )
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
        let fragments = split_module(
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
        let module = "target triple = \"x86_64-unknown-linux-gnu\"
define i32 @wf_a.f() #0 {
entry:
  call void @wf.drop.t.x()
  ret i32 0
}
define private void @wf.drop.t.x() #0 {
entry:
  ret void
}
define private void @wf.drop.t.y() #0 {
entry:
  call void @wf.drop.t.x()
  ret void
}
attributes #0 = { nounwind }
";
        let fragments =
            split_module(module, FragmentGranularity::Function).expect("the module splits");
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
        let module = "target triple = \"x86_64-unknown-linux-gnu\"
define i32 @wf_a.f() #0 {
entry:
  ret i32 0
}
define private void @wf.drop.t.b() #0 {
entry:
  ret void
}
define private void @wf.drop.t.a() #0 {
entry:
  call void @wf.drop.t.b()
  ret void
}
define private void @wf.cycle.one() #0 {
entry:
  call void @wf.cycle.two()
  ret void
}
define private void @wf.cycle.two() #0 {
entry:
  call void @wf.cycle.one()
  ret void
}
attributes #0 = { nounwind }
";
        let fragments =
            split_module(module, FragmentGranularity::Function).expect("the module splits");
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
        let fragments = split_module(
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
            let before = split_module(&module("  %v9 = add i32 %v0, 1"), granularity)
                .expect("the module splits");
            let after = split_module(&module("  %v9 = mul i32 %v0, 3"), granularity)
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

    /// A line outside the emitter's form, or a name nothing defines, fails
    /// the split instead of being guessed at.
    #[test]
    fn an_unrecognized_module_is_refused() {
        let base = module("  %v9 = add i32 %v0, 1");
        for (from, to) in [
            (
                "declare void @abort()",
                "declare void @abort()\n@g = global i32 0",
            ),
            (
                "declare void @abort()",
                "declare void @abort()\nmodule asm \"nop\"",
            ),
            ("define weak i32", "define linkonce_odr i32"),
            ("call void @abort()", "call void @missing()"),
        ] {
            let edited = base.replacen(from, to, 1);
            assert_ne!(edited, base);
            assert!(
                split_module(&edited, FragmentGranularity::Function).is_err(),
                "{to} must be refused"
            );
        }
    }

    /// A symbol is read bare or quoted, after any linkage.
    #[test]
    fn a_symbol_is_read_quoted_or_bare() {
        for (definition, symbol) in [
            (
                "void @wf_runtime.queue.new(ptr %wf.result) #0 {",
                "wf_runtime.queue.new",
            ),
            (
                "ptr @\"wf_box_new$instance$39\"(ptr %wf.arg.v0) #0 {",
                "wf_box_new$instance$39",
            ),
            (
                "weak i32 @wf__floor_run(i32 %argc, ptr %argv) #0 {",
                "wf__floor_run",
            ),
        ] {
            assert_eq!(symbol_after_at(definition).as_deref(), Some(symbol));
        }
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
