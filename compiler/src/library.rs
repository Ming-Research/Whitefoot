//! The standard library the toolchain supplies [MOD-10]: its graph record and
//! its modules' records, carried from the compiler's own build so that a
//! program is always checked against the library its compiler ships, and the
//! host modules whose definitions the build supplies [PRE-2].

/// The logical path of the standard library's graph record.
#[cfg(test)]
pub(crate) const GRAPH_PATH: &str = "std/modules.wfg";

/// The standard library's graph record, `modules.wfg` below its root: the
/// library's own statement of its modules, which [`MODULES`] carries and a
/// test holds equal to it.
#[cfg(test)]
pub(crate) const GRAPH: &str = include_str!("../../lib/std/modules.wfg");

/// Every record of the standard library's modules, each with its logical
/// path, its path below the library's root after the component `std`
/// [MOD-10], in the order the library's graph registers their modules and
/// each module's interface record first [MOD-2].
pub(crate) const RECORDS: &[(&str, &str)] = &[
    (
        "std/io/module.wfm",
        include_str!("../../lib/std/io/module.wfm"),
    ),
    (
        "std/text/module.wfm",
        include_str!("../../lib/std/text/module.wfm"),
    ),
    (
        "std/fs/module.wfm",
        include_str!("../../lib/std/fs/module.wfm"),
    ),
    (
        "std/net/module.wfm",
        include_str!("../../lib/std/net/module.wfm"),
    ),
    (
        "std/process/module.wfm",
        include_str!("../../lib/std/process/module.wfm"),
    ),
    (
        "std/collections/vector/module.wfm",
        include_str!("../../lib/std/collections/vector/module.wfm"),
    ),
    (
        "std/collections/vector/grow-vector.wf",
        include_str!("../../lib/std/collections/vector/grow-vector.wf"),
    ),
    (
        "std/collections/deque/module.wfm",
        include_str!("../../lib/std/collections/deque/module.wfm"),
    ),
    (
        "std/collections/deque/deque.wf",
        include_str!("../../lib/std/collections/deque/deque.wf"),
    ),
    (
        "std/collections/slab/module.wfm",
        include_str!("../../lib/std/collections/slab/module.wfm"),
    ),
    (
        "std/collections/slab/slab.wf",
        include_str!("../../lib/std/collections/slab/slab.wf"),
    ),
    (
        "std/collections/hash_map/module.wfm",
        include_str!("../../lib/std/collections/hash_map/module.wfm"),
    ),
    (
        "std/collections/hash_map/hash-map.wf",
        include_str!("../../lib/std/collections/hash_map/hash-map.wf"),
    ),
    (
        "std/collections/priority_queue/module.wfm",
        include_str!("../../lib/std/collections/priority_queue/module.wfm"),
    ),
    (
        "std/collections/priority_queue/priority-queue.wf",
        include_str!("../../lib/std/collections/priority_queue/priority-queue.wf"),
    ),
    (
        "std/collections/ordered_map/module.wfm",
        include_str!("../../lib/std/collections/ordered_map/module.wfm"),
    ),
    (
        "std/collections/ordered_map/ordered-map.wf",
        include_str!("../../lib/std/collections/ordered_map/ordered-map.wf"),
    ),
];

/// The standard library's modules in the order its graph registers them,
/// each with its path and its dependencies' paths [MOD-1]. The graph record
/// is the library's own statement of them, and a test holds this table to
/// what forming that record gives.
pub(crate) const MODULES: &[(&str, &[&str])] = &[
    ("io", &[]),
    ("text", &[]),
    ("fs", &["io", "text"]),
    ("net", &["io"]),
    ("process", &["io", "text", "fs"]),
    ("collections::vector", &[]),
    ("collections::deque", &[]),
    ("collections::slab", &[]),
    ("collections::hash_map", &[]),
    ("collections::priority_queue", &[]),
    ("collections::ordered_map", &[]),
];

/// The standard library's modules as registered modules: its graph's rows
/// in order, `offset` places after the modules that precede them [MOD-10].
pub(crate) fn modules(offset: usize) -> Vec<crate::ModuleRecord> {
    let path = |text: &str| -> Vec<String> { text.split("::").map(str::to_owned).collect() };
    MODULES
        .iter()
        .map(|(module, dependencies)| {
            crate::ModuleRecord::in_package(
                crate::Package::Standard,
                path(module),
                dependencies
                    .iter()
                    .filter_map(|dependency| {
                        MODULES.iter().position(|(other, _)| other == dependency)
                    })
                    .filter_map(|index| crate::ModuleId::from_index(offset + index))
                    .collect(),
            )
        })
        .collect()
}

/// The standard library paths a source bundle's records write, each as the
/// components after `std`: every `std` word that starts a `::` path, read
/// from the bytes before any stage runs. A `std` inside a string or a
/// longer word also counts, which can only select a module the bundle does
/// not use [PROG-2, MOD-10].
fn written_paths(inputs: &[crate::SourceInput<'_>]) -> Vec<Vec<String>> {
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut paths = Vec::new();
    for input in inputs {
        let bytes = input.bytes();
        let mut at = 0;
        while let Some(found) = bytes[at..].windows(5).position(|window| window == b"std::") {
            let start = at + found;
            at = start + 3;
            if start > 0 && word(bytes[start - 1]) {
                continue;
            }
            let mut components = Vec::new();
            let mut cursor = start + 3;
            while bytes[cursor..].starts_with(b"::") {
                let begin = cursor + 2;
                let end = bytes[begin..]
                    .iter()
                    .position(|byte| !word(*byte))
                    .map_or(bytes.len(), |length| begin + length);
                if end == begin {
                    break;
                }
                components.push(String::from_utf8_lossy(&bytes[begin..end]).into_owned());
                cursor = end;
            }
            if !components.is_empty() {
                paths.push(components);
            }
        }
    }
    paths
}

/// A source bundle whose records name standard library modules: the root
/// module, which may name every library module [PROG-2], followed by the
/// library's modules, and the bundle's records followed by those of the
/// library modules it names and their dependencies [MOD-10]. A bundle that
/// names none has no library part.
pub(crate) fn bundle_part<'input>(
    inputs: &[crate::SourceInput<'input>],
) -> Option<(Vec<crate::SourceInput<'input>>, Vec<crate::ModuleRecord>)> {
    let written = written_paths(inputs);
    if written.is_empty() {
        return None;
    }
    let library = modules(1);
    let mut modules = vec![crate::ModuleRecord::new(
        Vec::new(),
        (1..=library.len())
            .filter_map(crate::ModuleId::from_index)
            .collect(),
    )];
    modules.extend(library);
    let mut selected: Vec<crate::ModuleId> = Vec::new();
    for path in &written {
        let longest = (1..=path.len()).rev().find_map(|length| {
            modules
                .iter()
                .position(|module| module.is_at(crate::Package::Standard, &path[..length]))
                .and_then(crate::ModuleId::from_index)
        });
        let mut pending: Vec<crate::ModuleId> = longest.into_iter().collect();
        while let Some(module) = pending.pop() {
            if selected.contains(&module) {
                continue;
            }
            selected.push(module);
            if let Some(record) = modules.get(module.index()) {
                pending.extend(record.dependencies().iter().copied());
            }
        }
    }
    let mut all = inputs.to_vec();
    all.extend(records(&modules, |module| selected.contains(&module)));
    Some((all, modules))
}

/// The records the compiler carries for the standard library modules of
/// `modules` that `selected` admits, placed in their modules [MOD-10].
pub(crate) fn records(
    modules: &[crate::ModuleRecord],
    selected: impl Fn(crate::ModuleId) -> bool,
) -> Vec<crate::SourceInput<'static>> {
    RECORDS
        .iter()
        .filter_map(|(logical, text)| {
            let (path, role) = record_module(logical)?;
            let module = modules
                .iter()
                .position(|module| module.is_at(crate::Package::Standard, &path))
                .and_then(crate::ModuleId::from_index)?;
            selected(module)
                .then(|| crate::SourceInput::new(logical, text.as_bytes()).in_module(module, role))
        })
        .collect()
}

/// The host modules [PRE-2], by their paths below the standard library: the
/// modules whose functions have no Whitefoot definition, the build supplying
/// each from the runtime units.
pub(crate) const HOST_MODULES: &[&str] = &["io", "text", "fs", "net", "process"];

/// Reports whether a standard library module at `path` is a host module.
pub(crate) fn is_host_module(path: &[String]) -> bool {
    matches!(path, [single] if HOST_MODULES.contains(&single.as_str()))
}

/// The standard library module a record belongs to and its role: the
/// module's path below the library's root and whether the record is its
/// interface [MOD-2].
pub(crate) fn record_module(logical: &str) -> Option<(Vec<String>, crate::SourceRole)> {
    let (directory, file) = logical.strip_prefix("std/")?.rsplit_once('/')?;
    let role = if file == crate::INTERFACE_FILE_NAME {
        crate::SourceRole::Interface
    } else {
        crate::SourceRole::Implementation
    };
    Some((directory.split('/').map(str::to_owned).collect(), role))
}

#[cfg(test)]
mod tests {
    use super::{GRAPH, HOST_MODULES, RECORDS};

    fn library_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/std")
    }

    /// Every record of the library's directory is carried, and nothing else,
    /// so adding a module without listing it here cannot go unnoticed. A
    /// record is a `.wf`, `module.wfm` or `modules.wfg` file [MOD-2]; the
    /// library's documentation beside them is not one.
    #[test]
    fn the_carried_records_are_exactly_the_library_directory() {
        fn walk(root: &std::path::Path, directory: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(directory).expect("the library directory reads") {
                let path = entry.expect("a directory entry reads").path();
                if path.is_dir() {
                    walk(root, &path, found);
                } else if path.extension().is_some_and(|extension| {
                    ["wf", "wfm", "wfg"].iter().any(|kind| extension == *kind)
                }) {
                    found.push(
                        path.strip_prefix(root)
                            .expect("below the root")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let root = library_root();
        let mut found = Vec::new();
        walk(&root, &root, &mut found);
        found.sort();
        let mut carried: Vec<String> = RECORDS
            .iter()
            .map(|(path, _)| {
                path.strip_prefix("std/")
                    .expect("a library path")
                    .to_owned()
            })
            .chain(std::iter::once("modules.wfg".to_owned()))
            .collect();
        carried.sort();
        assert_eq!(found, carried);
        for (path, text) in RECORDS {
            let relative = path.strip_prefix("std/").expect("a library path");
            let on_disk = std::fs::read_to_string(root.join(relative)).expect("the record reads");
            assert_eq!(&on_disk, text, "{path} is carried byte for byte");
        }
        assert_eq!(
            std::fs::read_to_string(root.join("modules.wfg")).expect("the graph reads"),
            GRAPH
        );
    }

    /// [PRE-2] the host modules' graph rows are rows of the library's graph,
    /// and their interface records are the specification's text byte for
    /// byte.
    #[test]
    fn the_host_modules_are_the_specification_text() {
        let spec = crate::spec::ACTIVE_KERNEL_SPEC_TEXT;
        let section = spec
            .split("\n[PRE-2] ")
            .nth(1)
            .expect("the specification states PRE-2");
        let fences: Vec<&str> = section.split("```\n").skip(1).step_by(2).collect();
        let rows = fences.first().expect("PRE-2 states the graph rows");
        for row in rows.lines() {
            assert!(
                GRAPH.lines().any(|line| line == row),
                "{row} is a graph row"
            );
        }
        for module in HOST_MODULES {
            let path = format!("{module}/module.wfm");
            let (_, text) = RECORDS
                .iter()
                .find(|(record, _)| record.strip_prefix("std/") == Some(path.as_str()))
                .expect("every host module is carried");
            let marker = format!("`std::{module}`, the record `{path}`:\n\n```\n");
            let stated = spec
                .split(&marker)
                .nth(1)
                .and_then(|rest| rest.split("```\n").next())
                .expect("PRE-2 states every host module's record");
            assert_eq!(stated, *text, "{path} is the PRE-2 text");
        }
    }
}
