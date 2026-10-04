//! Module graph formation [MOD-1] and module source discovery [MOD-2].
//!
//! A module program is selected by one `modules.wfg` file. Its rows register
//! the program's modules in written order, each with the exact list of earlier
//! modules it may name, and its entries name the functions an invocation may
//! run with their environment requirements. The file's directory is the
//! package root: every registered module is one directory below it holding
//! one interface record, `module.wfm`, and the direct `.wf` implementation
//! records beside it. A graph may bind other packages by their directories
//! [MOD-11]; their graphs are formed by the same rules and their modules
//! follow the program's own.

use std::path::{Path, PathBuf};

use crate::syntax::NodeId;
use crate::syntax::views::{ModulePathRoot, SyntaxView, SyntaxViewFailure};
use crate::{CanonicalSyntaxUnit, ModuleId, ModuleRecord, Package, SourceRole, SyntaxCoordinate};

/// The logical path of a module program's graph record.
pub const GRAPH_FILE_NAME: &str = "modules.wfg";

/// The file name of every module's interface record [MOD-2].
pub const INTERFACE_FILE_NAME: &str = "module.wfm";

/// One named entry of a module graph [MOD-9].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEntry {
    name: String,
    module: ModuleId,
    function: String,
    no_heap: bool,
    coordinate: SyntaxCoordinate,
    /// The entry's place in the graph record, for a rejection that cites the
    /// entry [MOD-9].
    written: Option<crate::Place>,
}

impl GraphEntry {
    /// Returns the entry's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the module that declares the entry function.
    #[must_use]
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns the entry function's name within its module.
    #[must_use]
    pub fn function(&self) -> &str {
        &self.function
    }

    /// Reports whether the entry states the no-heap requirement [STOR-8].
    #[must_use]
    pub const fn no_heap(&self) -> bool {
        self.no_heap
    }

    /// Returns the entry's coordinate in the graph record.
    #[must_use]
    pub const fn coordinate(&self) -> SyntaxCoordinate {
        self.coordinate
    }

    /// Returns the entry's place in the graph record.
    #[must_use]
    pub(crate) const fn written(&self) -> Option<&crate::Place> {
        self.written.as_ref()
    }
}

/// A formed module graph: its registered modules in row order and its named
/// entries in written order [MOD-1].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleGraph {
    modules: Vec<ModuleRecord>,
    entries: Vec<GraphEntry>,
    /// Each bound package's root directory, in package order [MOD-11].
    package_roots: Vec<PathBuf>,
}

impl ModuleGraph {
    /// The standard library's graph, whose modules the compiler carries
    /// [MOD-10].
    #[must_use]
    pub(crate) fn library() -> Self {
        Self {
            modules: crate::library::modules(0),
            entries: Vec::new(),
            package_roots: Vec::new(),
        }
    }

    /// Returns every registered module in row order.
    #[must_use]
    pub fn modules(&self) -> &[ModuleRecord] {
        &self.modules
    }

    /// Returns every named entry in written order.
    #[must_use]
    pub fn entries(&self) -> &[GraphEntry] {
        &self.entries
    }

    /// Returns the named entry with this name.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&GraphEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Returns every module a module may name through its dependencies'
    /// interfaces: its direct dependencies and, because an interface closes
    /// over the interfaces it names, theirs in turn [MOD-8].
    #[must_use]
    pub fn dependency_closure(&self, module: ModuleId) -> Vec<ModuleId> {
        let mut closure = Vec::new();
        let mut pending = self
            .modules
            .get(module.index())
            .map_or_else(Vec::new, |record| record.dependencies().to_vec());
        while let Some(next) = pending.pop() {
            if closure.contains(&next) {
                continue;
            }
            closure.push(next);
            if let Some(record) = self.modules.get(next.index()) {
                pending.extend(record.dependencies().iter().copied());
            }
        }
        closure.sort();
        closure
    }

    /// Records where each entry is written, resolved by `locate` from its
    /// coordinate in the graph record.
    pub(crate) fn locate_entries(
        &mut self,
        locate: impl Fn(SyntaxCoordinate) -> Option<crate::Place>,
    ) {
        for entry in &mut self.entries {
            entry.written = locate(entry.coordinate);
        }
    }

    /// Returns the module registered at this qualified name: `pkg` or
    /// `pkg::a::b` for the program's own, `std::a` for the standard
    /// library's and `label::a` for a bound package's [MOD-10, MOD-11].
    #[must_use]
    pub fn module_named(&self, qualified: &str) -> Option<ModuleId> {
        self.modules
            .iter()
            .position(|module| module.qualified_name() == qualified)
            .and_then(ModuleId::from_index)
    }

    /// Returns a bound package's root directory [MOD-11].
    #[must_use]
    pub fn package_root(&self, package: Package) -> Option<&Path> {
        match package {
            Package::Bound(place) => self.package_roots.get(usize::from(place)).map(PathBuf::as_path),
            Package::Program | Package::Standard => None,
        }
    }

    /// Returns every module of the program's own package and of the packages
    /// it binds, in package and row order; the standard library's modules
    /// follow them [MOD-10, MOD-11].
    pub fn program_modules(&self) -> impl Iterator<Item = ModuleId> + '_ {
        self.modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.package() != Package::Standard)
            .filter_map(|(index, _)| ModuleId::from_index(index))
    }
}

/// Why a graph row or entry is refused [MOD-1].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphIssueKind {
    /// A second row registers an already registered module.
    DuplicateModule {
        /// The module's qualified name.
        path: String,
    },
    /// A row lists its own module as a dependency.
    SelfDependency {
        /// The module's qualified name.
        path: String,
    },
    /// A row lists one dependency twice.
    DuplicateDependency {
        /// The dependency's qualified name.
        path: String,
    },
    /// A row lists a module that no row registers.
    UnregisteredDependency {
        /// The dependency's qualified name.
        path: String,
    },
    /// A row lists a module registered only by a later row; dependencies
    /// point to earlier rows, which is what keeps the graph acyclic.
    LaterDependency {
        /// The dependency's qualified name.
        path: String,
    },
    /// A second entry takes an already used entry name.
    DuplicateEntry {
        /// The entry name.
        name: String,
    },
    /// An entry's target names no function of a registered module.
    UnregisteredEntryModule {
        /// The written target.
        target: String,
    },
    /// A row registers, or an entry names, a path of the standard library,
    /// which only its own graph registers [MOD-10].
    StandardPath {
        /// The written path.
        path: String,
    },
    /// A `std` dependency names no standard library module [MOD-10].
    UnknownStandardModule {
        /// The written dependency.
        path: String,
    },
    /// The standard library's own graph writes `std`, where its records
    /// name the library `pkg` [MOD-10].
    StandardPrefixInLibrary {
        /// The written path.
        path: String,
    },
    /// A row registers, or an entry names, a path rooted at a name: a graph
    /// registers and selects only its own package's modules [MOD-1].
    BoundPath {
        /// The written path.
        path: String,
    },
    /// A package binding's STRING is not a relative location [MOD-11].
    InvalidLocation {
        /// The written STRING.
        location: String,
    },
    /// A graph binds one name twice [MOD-11].
    DuplicateBindingName {
        /// The name.
        name: String,
    },
    /// A graph binds one package twice [MOD-11].
    DuplicatePackage {
        /// The second binding's name.
        name: String,
    },
    /// A graph binds its own package, or a package whose bindings lead back
    /// to it [MOD-11].
    BindingCycle {
        /// The binding's name.
        name: String,
    },
    /// A dependency is rooted at a name its graph binds to no package
    /// [MOD-11].
    UnboundPackage {
        /// The written dependency.
        path: String,
    },
    /// A dependency names no module of the bound package [MOD-11].
    UnknownBoundModule {
        /// The written dependency.
        path: String,
    },
}

/// One refused graph row or entry, at its written path [MOD-1].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphIssue {
    pub(crate) coordinate: SyntaxCoordinate,
    pub(crate) kind: GraphIssueKind,
}

impl GraphIssue {
    /// Returns the numbered rule owning the rejection.
    #[must_use]
    pub const fn rule_id(&self) -> &'static str {
        match self.kind {
            GraphIssueKind::StandardPath { .. }
            | GraphIssueKind::UnknownStandardModule { .. }
            | GraphIssueKind::StandardPrefixInLibrary { .. } => "MOD-10",
            GraphIssueKind::InvalidLocation { .. }
            | GraphIssueKind::DuplicateBindingName { .. }
            | GraphIssueKind::DuplicatePackage { .. }
            | GraphIssueKind::BindingCycle { .. }
            | GraphIssueKind::UnboundPackage { .. }
            | GraphIssueKind::UnknownBoundModule { .. } => "MOD-11",
            _ => "MOD-1",
        }
    }

    /// Returns the coordinate of the offending written path.
    #[must_use]
    pub const fn coordinate(&self) -> SyntaxCoordinate {
        self.coordinate
    }

    /// Returns the structured payload.
    #[must_use]
    pub const fn kind(&self) -> &GraphIssueKind {
        &self.kind
    }
}

/// Trusted graph-formation invariant failure, never a source rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphCompilerFailure {
    /// The canonical graph tree did not have the `graph_file` shape.
    InvalidGraphTree,
}

impl From<SyntaxViewFailure> for GraphCompilerFailure {
    fn from(_: SyntaxViewFailure) -> Self {
        Self::InvalidGraphTree
    }
}

impl core::fmt::Display for GraphCompilerFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("the canonical graph tree does not have the graph_file shape")
    }
}

/// One written module path and where it is written.
struct WrittenPath {
    root: WrittenRoot,
    components: Vec<String>,
    coordinate: SyntaxCoordinate,
}

/// The root of a written module path [MOD-1, MOD-10, MOD-11].
#[derive(Clone, Eq, PartialEq)]
enum WrittenRoot {
    Pkg,
    Std,
    /// A name the graph may bind to a package.
    Name(String),
}

impl WrittenPath {
    fn qualified(&self) -> String {
        let mut text = match &self.root {
            WrittenRoot::Pkg => "pkg".to_owned(),
            WrittenRoot::Std => "std".to_owned(),
            WrittenRoot::Name(name) => name.clone(),
        };
        for component in &self.components {
            text.push_str("::");
            text.push_str(component);
        }
        text
    }
}

/// A row's dependency: an earlier row of the same graph, a row of a bound
/// package's graph, by the package's place in package order, or a module of
/// the standard library's graph [MOD-1, MOD-10, MOD-11].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dependency {
    Row(usize),
    Bound(usize, usize),
    Library(usize),
}

/// One package binding a graph writes [MOD-11]: the bound name, the
/// location's components and where the binding is written.
#[derive(Clone, Debug)]
pub(crate) struct WrittenBinding {
    pub(crate) name: String,
    pub(crate) location: Vec<String>,
    pub(crate) coordinate: SyntaxCoordinate,
}

/// One package's graph formed on its own: its rows, each with its path and
/// dependencies, and its entries, whose modules are rows of this graph
/// [MOD-1].
pub(crate) struct PackageGraph {
    rows: Vec<(Vec<String>, Vec<Dependency>)>,
    entries: Vec<GraphEntry>,
}

/// The packages a graph's rows may name through its bindings [MOD-11]: each
/// bound name with its package's place in package order, and every
/// package's registered module paths in package order.
pub(crate) struct BoundPackages<'a> {
    pub(crate) names: &'a [(String, usize)],
    pub(crate) registered: &'a [Vec<Vec<String>>],
}

/// Shared readers of one canonical graph unit.
struct GraphReader<'a> {
    view: SyntaxView<'a>,
}

impl GraphReader<'_> {
    fn spelling(&self, terminal: usize) -> Result<String, GraphCompilerFailure> {
        std::str::from_utf8(self.view.token_bytes(terminal)?)
            .map(str::to_owned)
            .map_err(|_| GraphCompilerFailure::InvalidGraphTree)
    }

    fn written_path(&self, node: NodeId) -> Result<WrittenPath, GraphCompilerFailure> {
        let form = self.view.module_path(node)?;
        Ok(WrittenPath {
            root: match form.root {
                ModulePathRoot::Pkg => WrittenRoot::Pkg,
                ModulePathRoot::Std => WrittenRoot::Std,
                ModulePathRoot::Name(terminal) => WrittenRoot::Name(self.spelling(terminal)?),
            },
            components: form
                .components
                .into_iter()
                .map(|terminal| self.spelling(terminal))
                .collect::<Result<_, _>>()?,
            coordinate: self.view.coordinate(node)?,
        })
    }
}

/// A STRING's raw interior read as a relative location [MOD-11]: one or more
/// components separated by one `/`, each `..` or a logical path component
/// [PROG-2]. Every location byte is printable ASCII that FORM-7 spells
/// itself, so a STRING holding any escape is not a location and its raw
/// interior is its value.
fn relative_location(quoted: &[u8]) -> Option<Vec<String>> {
    let interior = quoted.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
    let text = std::str::from_utf8(interior).ok()?;
    let components = text.split('/').map(str::to_owned).collect::<Vec<_>>();
    components
        .iter()
        .all(|component| {
            component == ".."
                || (!component.is_empty()
                    && component != "."
                    && component.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    }))
        })
        .then_some(components)
}

/// The package bindings a canonical `graph_file` unit writes, in written
/// order [MOD-11]: each location must be a relative location and each name
/// bound once in the graph. The first refusal in written order is returned.
pub(crate) fn graph_bindings(
    unit: &CanonicalSyntaxUnit,
) -> Result<Result<Vec<WrittenBinding>, GraphIssue>, GraphCompilerFailure> {
    let reader = GraphReader {
        view: SyntaxView::new(unit)?,
    };
    let mut bindings: Vec<WrittenBinding> = Vec::new();
    for node in reader.view.graph()?.packages {
        let form = reader.view.package_decl(node)?;
        let name = reader.spelling(form.name)?;
        let coordinate = reader.view.coordinate(node)?;
        let Some(location) = relative_location(reader.view.token_bytes(form.location)?) else {
            return Ok(Err(GraphIssue {
                coordinate,
                kind: GraphIssueKind::InvalidLocation {
                    location: String::from_utf8_lossy(reader.view.token_bytes(form.location)?)
                        .into_owned(),
                },
            }));
        };
        if bindings.iter().any(|earlier| earlier.name == name) {
            return Ok(Err(GraphIssue {
                coordinate,
                kind: GraphIssueKind::DuplicateBindingName { name },
            }));
        }
        bindings.push(WrittenBinding {
            name,
            location,
            coordinate,
        });
    }
    Ok(Ok(bindings))
}

/// The paths of the modules a graph's `pkg` rows register, in row order: the
/// modules a binding graph may name in this package [MOD-11].
pub(crate) fn registered_paths(
    unit: &CanonicalSyntaxUnit,
) -> Result<Vec<Vec<String>>, GraphCompilerFailure> {
    let reader = GraphReader {
        view: SyntaxView::new(unit)?,
    };
    let mut paths = Vec::new();
    for row in reader.view.graph()?.rows {
        let module = reader.written_path(reader.view.module_row(row)?.module)?;
        if module.root == WrittenRoot::Pkg {
            paths.push(module.components);
        }
    }
    Ok(paths)
}

/// Forms one package's graph from its canonical `graph_file` unit [MOD-1,
/// MOD-10, MOD-11].
///
/// Rows are read in written order. Each row registers one module of
/// `package`; each of its `pkg` dependencies must be an earlier row, other
/// than itself, each `std` dependency a module of `library`, the standard
/// library's graph, and each dependency rooted at a name `bound` binds a
/// module the bound package registers, every dependency listed once. Each
/// entry takes a fresh name and targets a function of a registered module:
/// the last component of its written path is the function's name and the
/// components before it name the module. The first refusal in written order
/// is returned.
pub(crate) fn form_package_graph(
    unit: &CanonicalSyntaxUnit,
    package: Package,
    library: Option<&ModuleGraph>,
    bound: &BoundPackages<'_>,
) -> Result<Result<PackageGraph, GraphIssue>, GraphCompilerFailure> {
    let reader = GraphReader {
        view: SyntaxView::new(unit)?,
    };
    let view = &reader.view;
    let graph = view.graph()?;
    let mut rows: Vec<(Vec<String>, Vec<Dependency>)> = Vec::new();
    for &row in &graph.rows {
        let form = view.module_row(row)?;
        let module = reader.written_path(form.module)?;
        let refused = |kind| {
            Ok(Err(GraphIssue {
                coordinate: module.coordinate,
                kind,
            }))
        };
        match (&module.root, package) {
            (WrittenRoot::Pkg, _) => {}
            (WrittenRoot::Std, Package::Standard) => {
                return refused(GraphIssueKind::StandardPrefixInLibrary {
                    path: module.qualified(),
                });
            }
            (WrittenRoot::Std, _) => {
                return refused(GraphIssueKind::StandardPath {
                    path: module.qualified(),
                });
            }
            (WrittenRoot::Name(_), _) => {
                return refused(GraphIssueKind::BoundPath {
                    path: module.qualified(),
                });
            }
        }
        if rows
            .iter()
            .any(|(registered, _)| *registered == module.components)
        {
            return refused(GraphIssueKind::DuplicateModule {
                path: module.qualified(),
            });
        }
        let mut edges = Vec::new();
        for dependency in &form.dependencies {
            let dependency = reader.written_path(*dependency)?;
            let issue = |kind| {
                Ok(Err(GraphIssue {
                    coordinate: dependency.coordinate,
                    kind,
                }))
            };
            let edge = match &dependency.root {
                WrittenRoot::Std => {
                    let found = library.and_then(|library| {
                        library.modules().iter().position(|registered| {
                            registered.path() == dependency.components.as_slice()
                        })
                    });
                    match (package, found) {
                        (Package::Standard, _) => {
                            return issue(GraphIssueKind::StandardPrefixInLibrary {
                                path: dependency.qualified(),
                            });
                        }
                        (_, Some(index)) => Dependency::Library(index),
                        (_, None) => {
                            return issue(GraphIssueKind::UnknownStandardModule {
                                path: dependency.qualified(),
                            });
                        }
                    }
                }
                WrittenRoot::Name(name) => {
                    let Some(&(_, target)) =
                        bound.names.iter().find(|(bound_name, _)| bound_name == name)
                    else {
                        return issue(GraphIssueKind::UnboundPackage {
                            path: dependency.qualified(),
                        });
                    };
                    let Some(row) = bound.registered.get(target).and_then(|registered| {
                        registered
                            .iter()
                            .position(|path| *path == dependency.components)
                    }) else {
                        return issue(GraphIssueKind::UnknownBoundModule {
                            path: dependency.qualified(),
                        });
                    };
                    Dependency::Bound(target, row)
                }
                WrittenRoot::Pkg => {
                    if dependency.components == module.components {
                        return issue(GraphIssueKind::SelfDependency {
                            path: dependency.qualified(),
                        });
                    }
                    let Some(target) = rows
                        .iter()
                        .position(|(registered, _)| *registered == dependency.components)
                    else {
                        let later = graph
                            .rows
                            .iter()
                            .map(|&later| reader.written_path(view.module_row(later)?.module))
                            .collect::<Result<Vec<_>, GraphCompilerFailure>>()?
                            .iter()
                            .any(|registered| {
                                registered.root == WrittenRoot::Pkg
                                    && registered.components == dependency.components
                            });
                        return issue(if later {
                            GraphIssueKind::LaterDependency {
                                path: dependency.qualified(),
                            }
                        } else {
                            GraphIssueKind::UnregisteredDependency {
                                path: dependency.qualified(),
                            }
                        });
                    };
                    Dependency::Row(target)
                }
            };
            if edges.contains(&edge) {
                return issue(GraphIssueKind::DuplicateDependency {
                    path: dependency.qualified(),
                });
            }
            edges.push(edge);
        }
        rows.push((module.components, edges));
    }

    let mut entries: Vec<GraphEntry> = Vec::new();
    for entry in graph.entries {
        let form = view.graph_entry(entry)?;
        let name = reader.spelling(form.name)?;
        let no_heap = form.no_heap;
        let target = reader.written_path(form.target)?;
        match target.root {
            WrittenRoot::Pkg => {}
            WrittenRoot::Std => {
                return Ok(Err(GraphIssue {
                    coordinate: target.coordinate,
                    kind: GraphIssueKind::StandardPath {
                        path: target.qualified(),
                    },
                }));
            }
            WrittenRoot::Name(_) => {
                return Ok(Err(GraphIssue {
                    coordinate: target.coordinate,
                    kind: GraphIssueKind::BoundPath {
                        path: target.qualified(),
                    },
                }));
            }
        }
        if entries.iter().any(|earlier| earlier.name == name) {
            return Ok(Err(GraphIssue {
                coordinate: view.coordinate(entry)?,
                kind: GraphIssueKind::DuplicateEntry { name },
            }));
        }
        let unregistered = || {
            Ok(Err(GraphIssue {
                coordinate: target.coordinate,
                kind: GraphIssueKind::UnregisteredEntryModule {
                    target: target.qualified(),
                },
            }))
        };
        let Some((function, module_path)) = target.components.split_last() else {
            return unregistered();
        };
        let Some(module) = rows
            .iter()
            .position(|(registered, _)| registered.as_slice() == module_path)
            .and_then(ModuleId::from_index)
        else {
            return unregistered();
        };
        entries.push(GraphEntry {
            name,
            module,
            function: function.clone(),
            no_heap,
            coordinate: view.coordinate(entry)?,
            written: None,
        });
    }
    Ok(Ok(PackageGraph { rows, entries }))
}

/// One formed package of a module program, in package order [MOD-11]: the
/// program's own first, then each bound package with its label, root and
/// bindings.
pub(crate) struct FormedPackage {
    pub(crate) package: Package,
    pub(crate) label: String,
    pub(crate) root: Option<PathBuf>,
    pub(crate) bindings: Vec<(String, usize)>,
    pub(crate) graph: PackageGraph,
}

/// Joins a program's formed packages and the standard library into one
/// module graph [MOD-10, MOD-11]: every package's modules in package order
/// and then every module of `library`, each module carrying the bindings of
/// its package, and the program's own entries. Entries of a bound package
/// belong to its own graph and select nothing here.
pub(crate) fn assemble_graph(
    packages: Vec<FormedPackage>,
    library: Option<&ModuleGraph>,
) -> Result<ModuleGraph, GraphCompilerFailure> {
    let mut offsets = Vec::with_capacity(packages.len());
    let mut total = 0;
    for formed in &packages {
        offsets.push(total);
        total += formed.graph.rows.len();
    }
    let library_offset = total;
    let library_modules = library.map_or(&[][..], ModuleGraph::modules);
    let id = |index: usize| ModuleId::from_index(index).ok_or(GraphCompilerFailure::InvalidGraphTree);
    let mut modules: Vec<ModuleRecord> = Vec::with_capacity(total + library_modules.len());
    let mut package_roots = Vec::new();
    let mut entries = Vec::new();
    for (place, formed) in packages.into_iter().enumerate() {
        let bindings = formed
            .bindings
            .iter()
            .map(|(name, target)| {
                let package = if *target == 0 {
                    Package::Program
                } else {
                    Package::Bound(
                        u16::try_from(*target - 1).map_err(|_| GraphCompilerFailure::InvalidGraphTree)?,
                    )
                };
                Ok((name.clone(), package))
            })
            .collect::<Result<Vec<_>, GraphCompilerFailure>>()?;
        for (path, edges) in &formed.graph.rows {
            let dependencies = edges
                .iter()
                .map(|edge| match *edge {
                    Dependency::Row(index) => id(offsets[place] + index),
                    Dependency::Bound(target, index) => id(offsets[target] + index),
                    Dependency::Library(index) => id(library_offset + index),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let record = match formed.package {
                Package::Bound(_) => ModuleRecord::in_bound_package(
                    formed.package,
                    formed.label.clone(),
                    path.clone(),
                    dependencies,
                ),
                _ => ModuleRecord::in_package(formed.package, path.clone(), dependencies),
            };
            modules.push(record.with_bindings(bindings.clone()));
        }
        if place == 0 {
            entries = formed.graph.entries;
        } else {
            package_roots.push(formed.root.ok_or(GraphCompilerFailure::InvalidGraphTree)?);
        }
    }
    for module in library_modules {
        let dependencies = module
            .dependencies()
            .iter()
            .map(|dependency| id(library_offset + dependency.index()))
            .collect::<Result<Vec<_>, _>>()?;
        modules.push(ModuleRecord::in_package(
            Package::Standard,
            module.path().to_vec(),
            dependencies,
        ));
    }
    Ok(ModuleGraph {
        modules,
        entries,
        package_roots,
    })
}

/// Forms the graph a canonical `graph_file` unit writes for `package`, whose
/// caller has found that it binds no package [MOD-1, MOD-10]: its rows'
/// modules in row order and then every module of `library`.
pub(crate) fn form_graph(
    unit: &CanonicalSyntaxUnit,
    package: Package,
    library: Option<&ModuleGraph>,
) -> Result<Result<ModuleGraph, GraphIssue>, GraphCompilerFailure> {
    let graph = match form_package_graph(
        unit,
        package,
        library,
        &BoundPackages {
            names: &[],
            registered: &[],
        },
    )? {
        Ok(graph) => graph,
        Err(issue) => return Ok(Err(issue)),
    };
    assemble_graph(
        vec![FormedPackage {
            package,
            label: String::new(),
            root: None,
            bindings: Vec::new(),
            graph,
        }],
        library,
    )
    .map(Ok)
}

/// One module source read from the package directory [MOD-2].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSourceFile {
    /// The module the record belongs to.
    pub module: ModuleId,
    /// Whether the record is the module's interface or an implementation.
    pub role: SourceRole,
    /// The record's portable path below the package root.
    pub logical_path: String,
    /// The host path it was read from, for diagnostics.
    pub display_path: String,
    /// The record's exact bytes.
    pub bytes: Vec<u8>,
}

/// Why a package directory cannot supply a registered module's records.
///
/// These are input-envelope failures of the invocation, not source-language
/// rejections: the package directory is not in the shape [MOD-2] requires.
#[derive(Debug)]
pub enum DiscoveryFailure {
    /// A registered module's directory is absent or is not a directory.
    MissingModuleDirectory {
        /// The directory.
        path: PathBuf,
    },
    /// A registered module has no `module.wfm`.
    MissingInterface {
        /// The expected interface path.
        path: PathBuf,
    },
    /// A root, module directory or source record is a symbolic link, whose
    /// ownership would depend on host path resolution.
    SymbolicLink {
        /// The link.
        path: PathBuf,
    },
    /// Two entries of one module directory differ only in letter case.
    CaseCollision {
        /// The first entry.
        first: PathBuf,
        /// The second entry.
        second: PathBuf,
    },
    /// A source record's file name is not a portable logical path component.
    InvalidFileName {
        /// The record.
        path: PathBuf,
    },
    /// A package binding's location reaches no directory, or a package
    /// root has no graph record [MOD-11].
    MissingPackage {
        /// The directory or graph record.
        path: PathBuf,
    },
    /// The selected graph record is not a file named `modules.wfg`.
    GraphRecordName {
        /// The selected path.
        path: PathBuf,
    },
    /// A directory or record cannot be read.
    Unreadable {
        /// The path.
        path: PathBuf,
        /// The host error.
        error: std::io::Error,
    },
}

impl core::fmt::Display for DiscoveryFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingModuleDirectory { path } => write!(
                formatter,
                "[MOD-2] the registered module's directory {} does not exist",
                path.display()
            ),
            Self::MissingInterface { path } => write!(
                formatter,
                "[MOD-2] the registered module has no interface record {}",
                path.display()
            ),
            Self::SymbolicLink { path } => write!(
                formatter,
                "[MOD-2] {} is a symbolic link; module sources are read only from real directories and files",
                path.display()
            ),
            Self::CaseCollision { first, second } => write!(
                formatter,
                "[MOD-2] {} and {} differ only in letter case",
                first.display(),
                second.display()
            ),
            Self::InvalidFileName { path } => write!(
                formatter,
                "[MOD-2] {} is not a portable source record name",
                path.display()
            ),
            Self::MissingPackage { path } => write!(
                formatter,
                "[MOD-11] the bound package's {} does not exist",
                path.display()
            ),
            Self::GraphRecordName { path } => write!(
                formatter,
                "[MOD-1] {} is not a graph record; a module program is selected by the file {GRAPH_FILE_NAME} in its package root",
                path.display()
            ),
            Self::Unreadable { path, error } => {
                write!(formatter, "cannot read {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for DiscoveryFailure {}

/// Reads the graph record a build selects [MOD-1]: a regular file named
/// `modules.wfg`, not a symbolic link, whose directory is the package root.
pub fn read_graph_record(path: &Path) -> Result<Vec<u8>, DiscoveryFailure> {
    if path.file_name() != Some(std::ffi::OsStr::new(GRAPH_FILE_NAME)) {
        return Err(DiscoveryFailure::GraphRecordName {
            path: path.to_path_buf(),
        });
    }
    let unreadable = |error| DiscoveryFailure::Unreadable {
        path: path.to_path_buf(),
        error,
    };
    let metadata = std::fs::symlink_metadata(path).map_err(unreadable)?;
    if metadata.file_type().is_symlink() {
        return Err(DiscoveryFailure::SymbolicLink {
            path: path.to_path_buf(),
        });
    }
    if !metadata.is_file() {
        return Err(DiscoveryFailure::GraphRecordName {
            path: path.to_path_buf(),
        });
    }
    std::fs::read(path).map_err(unreadable)
}

/// Reads every registered module of the program's own package from `root`
/// and of each bound package from its root [MOD-2, MOD-11]: each module's
/// `module.wfm` and then its direct `.wf` files in byte order of their
/// names, modules in package and row order. The standard library's records
/// come with the compiler [MOD-10]. Child directories are not read into
/// their parent, and no symbolic link or case-folding collision is followed
/// or tolerated.
pub fn discover_module_sources(
    root: &Path,
    graph: &ModuleGraph,
) -> Result<Vec<ModuleSourceFile>, DiscoveryFailure> {
    let unreadable = |path: &Path, error| DiscoveryFailure::Unreadable {
        path: path.to_path_buf(),
        error,
    };
    let reject_link = |path: &Path| -> Result<std::fs::Metadata, DiscoveryFailure> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| unreadable(path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(DiscoveryFailure::SymbolicLink {
                path: path.to_path_buf(),
            });
        }
        Ok(metadata)
    };
    reject_link(root)?;
    for bound in &graph.package_roots {
        reject_link(bound)?;
    }
    let mut sources = Vec::new();
    for (index, module) in graph.modules().iter().enumerate() {
        // [MOD-10] the standard library's records come with the compiler.
        let package_root = match module.package() {
            Package::Program => root,
            Package::Bound(_) => graph
                .package_root(module.package())
                .ok_or_else(|| DiscoveryFailure::Unreadable {
                    path: root.to_path_buf(),
                    error: std::io::Error::other("a bound package has no root"),
                })?,
            Package::Standard => continue,
        };
        let module_id =
            ModuleId::from_index(index).ok_or_else(|| DiscoveryFailure::Unreadable {
                path: root.to_path_buf(),
                error: std::io::Error::other("too many modules"),
            })?;
        let mut directory = package_root.to_path_buf();
        for component in module.path() {
            directory.push(component);
            match std::fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(DiscoveryFailure::SymbolicLink { path: directory });
                }
                Ok(metadata) if metadata.is_dir() => {}
                _ => return Err(DiscoveryFailure::MissingModuleDirectory { path: directory }),
            }
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&directory).map_err(|error| unreadable(&directory, error))? {
            let entry = entry.map_err(|error| unreadable(&directory, error))?;
            names.push(entry.file_name());
        }
        let mut lowered: Vec<(String, PathBuf)> = Vec::new();
        for name in &names {
            let path = directory.join(name);
            let lower = name.to_string_lossy().to_lowercase();
            if let Some((_, first)) = lowered.iter().find(|(seen, _)| *seen == lower) {
                return Err(DiscoveryFailure::CaseCollision {
                    first: first.clone(),
                    second: path,
                });
            }
            lowered.push((lower, path));
        }
        let path = module.path().join("/");
        let prefix = module.record_prefix();
        let logical = |file: &str| {
            if path.is_empty() {
                format!("{prefix}{file}")
            } else {
                format!("{prefix}{path}/{file}")
            }
        };
        let interface = directory.join(INTERFACE_FILE_NAME);
        match std::fs::symlink_metadata(&interface) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(DiscoveryFailure::SymbolicLink { path: interface });
            }
            Ok(metadata) if metadata.is_file() => {}
            _ => return Err(DiscoveryFailure::MissingInterface { path: interface }),
        }
        sources.push(ModuleSourceFile {
            module: module_id,
            role: SourceRole::Interface,
            logical_path: logical(INTERFACE_FILE_NAME),
            display_path: interface.display().to_string(),
            bytes: std::fs::read(&interface).map_err(|error| unreadable(&interface, error))?,
        });
        let mut records = Vec::new();
        for name in names {
            // A record is named by its bytes: a name ending in `.wf` that is
            // not a portable component is refused, never skipped.
            if !name.as_encoded_bytes().ends_with(b".wf") {
                continue;
            }
            let path = directory.join(&name);
            let metadata = reject_link(&path)?;
            if !metadata.is_file() {
                continue;
            }
            let Some(text) = name.to_str().filter(|text| {
                text.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            }) else {
                return Err(DiscoveryFailure::InvalidFileName { path });
            };
            records.push((text.to_owned(), path));
        }
        records.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        for (name, path) in records {
            sources.push(ModuleSourceFile {
                module: module_id,
                role: SourceRole::Implementation,
                logical_path: logical(&name),
                display_path: path.display().to_string(),
                bytes: std::fs::read(&path).map_err(|error| unreadable(&path, error))?,
            });
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::{DiscoveryFailure, read_graph_record};

    /// A fresh directory for one test, removed by the returned guard.
    struct Directory(std::path::PathBuf);

    impl Directory {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("whitefoot-graph-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the test directory");
            Self(path)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// [MOD-1] a module program is selected by its `modules.wfg` alone: a
    /// graph under another name, and one reached through a symbolic link,
    /// are refused before any byte of them is read.
    #[test]
    fn only_a_regular_modules_wfg_is_a_graph_record() {
        let directory = Directory::new("record");
        let graph = directory.0.join("modules.wfg");
        std::fs::write(&graph, b"pkg: [];\n").expect("write the graph");
        assert_eq!(
            read_graph_record(&graph).expect("the graph record"),
            b"pkg: [];\n"
        );
        let other = directory.0.join("other.wfg");
        std::fs::write(&other, b"pkg: [];\n").expect("write another graph");
        assert!(matches!(
            read_graph_record(&other),
            Err(DiscoveryFailure::GraphRecordName { .. })
        ));
        #[cfg(unix)]
        {
            let linked = directory.0.join("linked");
            std::fs::create_dir_all(&linked).expect("create a directory");
            std::os::unix::fs::symlink(&graph, linked.join("modules.wfg")).expect("link the graph");
            assert!(matches!(
                read_graph_record(&linked.join("modules.wfg")),
                Err(DiscoveryFailure::SymbolicLink { .. })
            ));
        }
    }

    /// [MOD-2] a directory entry named by bytes ending in `.wf` is a record
    /// of its module, so one whose name is not a portable component is
    /// refused rather than skipped. Linux file systems store such a name;
    /// APFS and NTFS refuse to create it, so no such record exists there.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_record_name_that_is_not_utf8_is_refused() {
        use std::os::unix::ffi::OsStrExt;

        let directory = Directory::new("names");
        std::fs::write(directory.0.join("modules.wfg"), b"pkg: [];\n").expect("write the graph");
        std::fs::write(directory.0.join("module.wfm"), b"\n").expect("write the interface");
        let name = std::ffi::OsStr::from_bytes(b"bad\xff.wf");
        std::fs::write(directory.0.join(name), b"\n").expect("write the record");
        let graph = crate::form_module_graph(
            crate::SourceInput::new("modules.wfg", b"pkg: [];\n"),
            crate::CompilerLimits::default(),
        )
        .expect("the graph forms");
        assert!(matches!(
            super::discover_module_sources(&directory.0, &graph),
            Err(DiscoveryFailure::InvalidFileName { .. })
        ));
    }
}
