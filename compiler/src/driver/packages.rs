//! Reading a module program's packages [MOD-11]: the program's graph record,
//! the graph record of every package a graph binds, reached depth first in
//! written binding order, and their formation into one module graph.

use std::path::{Path, PathBuf};

use super::{
    Anchor, CompilationFailure, CompilationFailureKind, CompilationStage, CompilerLimits, Place,
    SourceBundle, SourceInput, canonical_syntax,
};
use crate::graph::{
    BoundPackages, FormedPackage, GraphIssue, GraphIssueKind, WrittenBinding, assemble_graph,
    form_package_graph, graph_bindings, registered_paths,
};
use crate::{
    CanonicalSyntaxUnit, DiscoveryFailure, GRAPH_FILE_NAME, ModuleGraph, Package,
    read_graph_record,
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
    let root = graph_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let bytes = read_graph_record(graph_path)?;
    let identity = identity_of(&root)?;
    let mut packages = vec![load(
        root,
        identity,
        String::new(),
        &graph_path.display().to_string(),
        &bytes,
        limits,
    )?];
    let mut walking = vec![0];
    walk(&mut packages, 0, &mut walking, limits)?;
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
    Ok(graph)
}

/// Reads the bindings of `packages[place]` in written order, reaching each
/// bound package's directory and, the first time the walk reaches one,
/// reading its graph and walking its bindings before the next [MOD-11].
fn walk(
    packages: &mut Vec<LoadedPackage>,
    place: usize,
    walking: &mut Vec<usize>,
    limits: CompilerLimits,
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
                &graph_path.display().to_string(),
                &bytes,
                limits,
            )?;
            packages.push(loaded);
            walking.push(target);
            walk(packages, target, walking, limits)?;
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
    display: &str,
    bytes: &[u8],
    limits: CompilerLimits,
) -> Result<LoadedPackage, ModuleProgramFailure> {
    let input = SourceInput::from_host_path(GRAPH_FILE_NAME, display, bytes);
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
