//! Reading a module program's packages [MOD-11]: the program's graph record,
//! the graph record of every package a graph binds, reached depth first in
//! written binding order, and their formation into one module graph.

use std::path::{Path, PathBuf};

use super::{
    Anchor, CompilationFailure, CompilationFailureKind, CompilationStage, CompilerLimits,
    FormatGrammar, Place, SourceBundle, SourceInput, canonical_format, canonical_syntax,
};
use crate::graph::{
    BoundPackages, FormedPackage, GraphIssue, GraphIssueKind, WrittenBinding, assemble_graph,
    form_package_graph, graph_bindings, registered_paths,
};
use crate::{
    CanonicalSyntaxUnit, DiscoveryFailure, GRAPH_FILE_NAME, ModuleGraph, Package,
    discover_module_sources, read_graph_record,
};

/// Why a module program's packages cannot be formed: a rejection of one of
/// its graph records, or an input-envelope failure of its directories.
#[derive(Debug)]
pub enum ModuleProgramFailure {
    /// A graph record is rejected, or the compiler failed while forming it.
    Compilation(CompilationFailure),
    /// A package directory or graph record cannot be read [MOD-2, MOD-11].
    Discovery(DiscoveryFailure),
}

impl From<CompilationFailure> for ModuleProgramFailure {
    fn from(failure: CompilationFailure) -> Self {
        Self::Compilation(failure)
    }
}

impl From<DiscoveryFailure> for ModuleProgramFailure {
    fn from(failure: DiscoveryFailure) -> Self {
        Self::Discovery(failure)
    }
}

/// One module-program record and the canonical bytes computed for it.
/// No write is performed while constructing these records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormattedFile {
    /// The host path from which this record was read.
    pub path: PathBuf,
    /// The exact bytes read before formatting.
    pub original: Vec<u8>,
    /// The exact FORM-2 rendering of the record's derivation.
    pub bytes: Vec<u8>,
}

/// Computes canonical bytes for a module program's graph records, including
/// bound packages, and every registered interface and implementation record.
///
/// Package traversal, graph judgments and source discovery use the ordinary
/// module-checking path. Graphs are rendered before those judgments so layout
/// faults do not prevent discovery; graph-rule diagnostics refer to that
/// canonical rendering, while lexical and grammar diagnostics retain the
/// original record. Source records are one grammar compilation unit without
/// semantic checking. The result contains graphs in package order followed by
/// source records in discovery order, and no partial result escapes a failure.
/// This function reads files but never rewrites them.
pub fn format_module_program(
    graph_path: &Path,
    limits: CompilerLimits,
) -> Result<Vec<FormattedFile>, ModuleProgramFailure> {
    let (graph, mut files) = load_module_program_graph(graph_path, limits, true)?;
    let root = program_root(graph_path);
    let sources = discover_module_sources(&root, &graph)?;
    let inputs = sources
        .iter()
        .map(|source| {
            SourceInput::from_host_path(&source.logical_path, &source.display_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect::<Vec<_>>();
    let rendered = canonical_format(&inputs, FormatGrammar::Program, limits)?;
    files.extend(
        sources
            .into_iter()
            .zip(rendered)
            .map(|(source, rendered)| FormattedFile {
                path: PathBuf::from(source.display_path),
                original: source.bytes,
                bytes: rendered.bytes,
            }),
    );
    Ok(files)
}

fn program_root(graph_path: &Path) -> PathBuf {
    graph_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// One package read from its directory: its graph record's bundle and
/// canonical unit, its bindings as written and as reached.
struct LoadedPackage {
    /// The directory the package was reached at, which its records are read
    /// below.
    root: PathBuf,
    /// The directory itself, which identifies the package [MOD-11].
    identity: PathBuf,
    /// Its label [MOD-11]; empty for the program's own package.
    label: String,
    bundle: SourceBundle,
    canonical: CanonicalSyntaxUnit,
    written: Vec<WrittenBinding>,
    /// Each binding's name with the bound package's place in package order.
    bound: Vec<(String, usize)>,
    /// Present only for a formatting request, retaining the original bytes.
    formatted: Option<FormattedFile>,
}

/// Forms the module graph of the program whose graph record is
/// `graph_path` [MOD-1, MOD-11]. Every graph record passes the syntax stages
/// and has its bindings judged in package order, the walk reading a bound
/// package's graph when a binding first reaches it; then every graph's rows
/// and entries are judged in package order. The first rejection is
/// returned. Module records are read afterwards, by
/// [`crate::discover_module_sources`].
pub fn form_module_program_graph(
    graph_path: &Path,
    limits: CompilerLimits,
) -> Result<ModuleGraph, ModuleProgramFailure> {
    load_module_program_graph(graph_path, limits, false).map(|(graph, _)| graph)
}

/// The package walk and graph judgments shared by checking and formatting.
fn load_module_program_graph(
    graph_path: &Path,
    limits: CompilerLimits,
    formatting: bool,
) -> Result<(ModuleGraph, Vec<FormattedFile>), ModuleProgramFailure> {
    let root = program_root(graph_path);
    let bytes = read_graph_record(graph_path)?;
    let identity = identity_of(&root)?;
    let mut packages = vec![load(
        root,
        identity,
        String::new(),
        graph_path,
        &bytes,
        limits,
        formatting,
    )?];
    let mut walking = vec![0];
    walk(&mut packages, 0, &mut walking, limits, formatting)?;
    assign_labels(&mut packages);

    let registered = packages
        .iter()
        .map(|package| registered_paths(&package.canonical))
        .collect::<Result<Vec<_>, _>>()
        .map_err(graph_compiler_failure)?;
    let library = ModuleGraph::library();
    let mut formed = Vec::with_capacity(packages.len());
    for (place, package) in packages.iter().enumerate() {
        let kind = if place == 0 {
            Package::Program
        } else {
            Package::Bound(u16::try_from(place - 1).map_err(|_| {
                CompilationFailure::new(
                    CompilationStage::ModuleGraph,
                    CompilationFailureKind::Resource,
                    "a program binds more packages than the compiler counts",
                )
            })?)
        };
        let graph = form_package_graph(
            &package.canonical,
            kind,
            Some(&library),
            &BoundPackages {
                names: &package.bound,
                registered: &registered,
            },
        )
        .map_err(graph_compiler_failure)?
        .map_err(|issue| rejection(&package.bundle, &issue))?;
        formed.push(FormedPackage {
            package: kind,
            label: package.label.clone(),
            root: Some(package.root.clone()),
            bindings: package.bound.clone(),
            graph,
        });
    }
    let mut graph = assemble_graph(formed, Some(&library)).map_err(graph_compiler_failure)?;
    let program = &packages[0].bundle;
    graph.locate_entries(|coordinate| Place::resolve(program, coordinate, Anchor::Start));
    let files = packages
        .into_iter()
        .filter_map(|package| package.formatted)
        .collect();
    Ok((graph, files))
}

/// Reads the bindings of `packages[place]` in written order, reaching each
/// bound package's directory and, the first time the walk reaches one,
/// reading its graph and walking its bindings before the next [MOD-11].
fn walk(
    packages: &mut Vec<LoadedPackage>,
    place: usize,
    walking: &mut Vec<usize>,
    limits: CompilerLimits,
    formatting: bool,
) -> Result<(), ModuleProgramFailure> {
    let written = packages[place].written.clone();
    for binding in written {
        let refused = |kind| {
            ModuleProgramFailure::from(rejection(
                &packages[place].bundle,
                &GraphIssue {
                    coordinate: binding.coordinate,
                    kind,
                },
            ))
        };
        let mut reached = packages[place].root.clone();
        for component in &binding.location {
            reached.push(component);
        }
        match std::fs::symlink_metadata(&reached) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(DiscoveryFailure::SymbolicLink { path: reached }.into());
            }
            Ok(metadata) if metadata.is_dir() => {}
            _ => return Err(DiscoveryFailure::MissingPackage { path: reached }.into()),
        }
        let identity = identity_of(&reached)?;
        if walking
            .iter()
            .any(|ancestor| packages[*ancestor].identity == identity)
        {
            return Err(refused(GraphIssueKind::BindingCycle {
                name: binding.name.clone(),
            }));
        }
        let existing = packages
            .iter()
            .position(|package| package.identity == identity);
        if existing.is_some_and(|target| {
            packages[place]
                .bound
                .iter()
                .any(|(_, bound)| *bound == target)
        }) {
            return Err(refused(GraphIssueKind::DuplicatePackage {
                name: binding.name.clone(),
            }));
        }
        let target = existing.unwrap_or(packages.len());
        packages[place].bound.push((binding.name.clone(), target));
        if existing.is_none() {
            let graph_path = reached.join(GRAPH_FILE_NAME);
            let bytes = match std::fs::symlink_metadata(&graph_path) {
                Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
                    read_graph_record(&graph_path)?
                }
                _ => return Err(DiscoveryFailure::MissingPackage { path: graph_path }.into()),
            };
            let loaded = load(
                reached,
                identity,
                String::new(),
                &graph_path,
                &bytes,
                limits,
                formatting,
            )?;
            packages.push(loaded);
            walking.push(target);
            walk(packages, target, walking, limits, formatting)?;
            walking.pop();
        }
    }
    Ok(())
}

/// Gives every bound package its label [MOD-11], bindings taken in binding
/// order, each graph's in package order and then in written order: a
/// package takes its first binding's name, or, when an earlier package has
/// that label, the name followed by `.` and the least integer from 2 that no
/// earlier label uses. A label holding `.` is no IDENT, so it never equals a
/// module directory's name.
fn assign_labels(packages: &mut [LoadedPackage]) {
    let bindings = packages
        .iter()
        .flat_map(|package| package.bound.iter().cloned())
        .collect::<Vec<_>>();
    for (name, target) in bindings {
        if !packages[target].label.is_empty() {
            continue;
        }
        let taken = |label: &str| packages.iter().any(|package| package.label == label);
        let label = if taken(&name) {
            (2_u64..)
                .map(|suffix| format!("{name}.{suffix}"))
                .find(|label| !taken(label))
                .unwrap_or_default()
        } else {
            name
        };
        packages[target].label = label;
    }
}

/// Reads one graph record through the syntax stages and its bindings'
/// forms [MOD-1, MOD-11].
fn load(
    root: PathBuf,
    identity: PathBuf,
    label: String,
    graph_path: &Path,
    bytes: &[u8],
    limits: CompilerLimits,
    formatting: bool,
) -> Result<LoadedPackage, ModuleProgramFailure> {
    let display = graph_path.display().to_string();
    let input = SourceInput::from_host_path(GRAPH_FILE_NAME, &display, bytes);
    let formatted = if formatting {
        let rendered = canonical_format(&[input], FormatGrammar::Graph, limits)?;
        let rendered = rendered.into_iter().next().ok_or_else(|| {
            CompilationFailure::new(
                CompilationStage::CanonicalSource,
                CompilationFailureKind::Compiler,
                "one graph record rendered no source",
            )
        })?;
        Some(FormattedFile {
            path: graph_path.to_path_buf(),
            original: bytes.to_vec(),
            bytes: rendered.bytes,
        })
    } else {
        None
    };
    let input = formatted
        .as_ref()
        .map_or(input, |file| input.with_bytes(&file.bytes));
    let bundle = SourceBundle::with_limits(&[input], limits.source)
        .map_err(CompilationFailure::source_envelope)?;
    let canonical = canonical_syntax(&bundle, limits, true)?;
    let written = graph_bindings(&canonical)
        .map_err(graph_compiler_failure)?
        .map_err(|issue| rejection(&bundle, &issue))?;
    Ok(LoadedPackage {
        root,
        identity,
        label,
        bundle,
        canonical,
        written,
        bound: Vec::new(),
        formatted,
    })
}

/// The directory a path reaches, which identifies a package [MOD-11].
fn identity_of(path: &Path) -> Result<PathBuf, DiscoveryFailure> {
    std::fs::canonicalize(path).map_err(|error| DiscoveryFailure::Unreadable {
        path: path.to_path_buf(),
        error,
    })
}

/// A graph rejection at its record [MOD-1, MOD-11].
fn rejection(bundle: &SourceBundle, issue: &GraphIssue) -> CompilationFailure {
    CompilationFailure::at_source(
        CompilationStage::ModuleGraph,
        issue.rule_id(),
        issue,
        bundle,
        issue.coordinate(),
        Anchor::Start,
    )
}

fn graph_compiler_failure(failure: crate::GraphCompilerFailure) -> CompilationFailure {
    CompilationFailure::new(
        CompilationStage::ModuleGraph,
        CompilationFailureKind::Compiler,
        failure,
    )
}

#[cfg(test)]
mod formatting_tests {
    use super::{CompilerLimits, format_module_program};
    use std::path::PathBuf;

    struct Directory(PathBuf);

    impl Directory {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "whitefoot-format-packages-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create the test package");
            Self(root)
        }

        fn write(&self, name: &str, bytes: &[u8]) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("a record has a parent"))
                .expect("create the record directory");
            std::fs::write(path, bytes).expect("write the record");
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn module_formatting_reads_bound_graphs_interfaces_and_implementations() {
        let directory = Directory::new("bound");
        let root_graph = b"package lib=\"bound\";\npkg:[];\n";
        let bound_graph = b"pkg:[];\n";
        let interface = b"public const amount:u64=1_u64;\n";
        let implementation = b"fn wrong()->result:u8 pure {\n    return 7_u64;  \n}\n";
        directory.write("modules.wfg", root_graph);
        directory.write("module.wfm", b"\n");
        directory.write("bound/modules.wfg", bound_graph);
        directory.write("bound/module.wfm", interface);
        directory.write("bound/wrong.wf", implementation);
        let files =
            format_module_program(&directory.0.join("modules.wfg"), CompilerLimits::default())
                .expect("graphs and source records format despite semantic errors");
        let expected: &[(&str, &[u8], &[u8])] = &[
            (
                "modules.wfg",
                root_graph,
                b"package lib = \"bound\";\n\npkg: [];\n",
            ),
            ("bound/modules.wfg", bound_graph, b"pkg: [];\n"),
            ("module.wfm", b"\n", b"\n"),
            (
                "bound/module.wfm",
                interface,
                b"public const amount: u64 = 1_u64;\n",
            ),
            (
                "bound/wrong.wf",
                implementation,
                b"fn wrong() -> result: u8 pure {\n  return 7_u64;\n}\n",
            ),
        ];
        assert_eq!(files.len(), expected.len());
        for (file, (path, original, bytes)) in files.iter().zip(expected) {
            assert_eq!(file.path, directory.0.join(path));
            assert_eq!(file.original, *original);
            assert_eq!(file.bytes, *bytes);
            assert_eq!(
                std::fs::read(&file.path).expect("read the original"),
                *original
            );
        }
    }
}
