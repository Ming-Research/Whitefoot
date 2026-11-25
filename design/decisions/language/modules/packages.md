# Packages

How a module program's graph binds other packages: where they are, what a package is, who names it, what a binding exposes, the order packages are read in and how they are checked.

### decision package-bindings-are-relative-graph-lines
A module graph binds another package with `package name = "location";`, a path relative to the graph's own directory, under a name the binding graph chooses.
- rejected: an agent proposed a separate manifest file for package dependencies; the owner rejected it, because the graph is already the program's single dependency authority.
- rejected: an agent proposed package roots given on the command line or in the environment; the owner rejected it, because a relative location keeps a checked-out tree buildable wherever it is placed, with no search path or environment input.
- rejected: an agent proposed absolute package locations; the owner rejected it, because a relative location keeps a checked-out tree buildable wherever it is placed.
- rejected: an agent proposed a name the library declares for itself; the owner rejected it, because a name the binder chooses leaves a library's own source unchanged whatever its users call it.
- rejected: an agent proposed placing Halo and its codec libraries inside firn's package until bindings exist; the owner rejected it, because they are meant to become separate projects and would be written against firn's package root instead of their own.
- rejected: an agent proposed shipping reusable third-party libraries in the standard library; the owner rejected it, because the standard library ships with the compiler and every program that names it selects it, so it would carry a Lua engine for programs that never run Lua.

### decision a-package-is-its-directory
Bindings from any graphs that reach one directory bind one package, whose modules and types are the same under every name.
- rejected: an agent proposed package identity by a declared name and version; the owner rejected it, because a library that returns a value of a package its user also binds, as Halo returns JSON values to firn, must return the type its user names, and identity by location needs no version rule.

### decision package-bindings-expose-public-modules-only
A binding exposes every module the bound graph registers, each through its public declarations only, and is not transitive.
- rejected: an agent proposed a package-level export list; the owner rejected it, because a module's `module.wfm` is already its public boundary and a graph row already the per-module permission, so the list would be a second visibility mechanism.
- rejected: an agent proposed that a package see the packages its dependencies bind; the owner rejected it, because a graph row is already the per-module permission, so a package names only the packages its own graph binds.

### decision packages-have-no-version
A package has no version: a binding reads whatever its directory holds, and caches key on its records' bytes.
- rejected: an agent proposed version constraints in the binding; the owner rejected it, because no consumer needs two versions of one library, and a pinned checkout fixes a version without a language rule.

### decision a-bound-name-is-private-to-its-graph
A bound name means its package only in the package whose graph binds it, and the compiler labels each package by its first binding's name, adding `.2`, `.3` when that label is taken.
- rejected: an agent proposed that one name mean one package across a whole program; the owner rejected it, because a graph chooses the name as a file chooses an alias, two libraries written apart must not have to agree on their dependencies' names, and its only gain was a label without a suffix.

### decision packages-are-read-depth-first-without-cycles
The package order is the program's own package, then each bound package in the order a depth-first walk of the bindings first reaches it, and a binding that closes a cycle is refused.
- rejected: an agent proposed allowing package cycles whose module edges stay acyclic; the owner rejected it, because a graph's rows name modules its bound graphs register, so those graphs must be read first, and an acyclic package graph lets each package be checked before the packages that bind it.
- rejected: an agent proposed taking the package order from directory names; the owner rejected it, because a fixed walk makes diagnostics, the bound unit's module order and the labels independent of the host.

### decision a-bound-name-is-only-a-module-prefix
A bound name joins no declaration domain, so a local binder or a module declaration may share its spelling, while a lowercase alias of that spelling is refused.
- rejected: an agent proposed putting bound names in the lexical-identifier domain with module aliases; the owner rejected it, because the grammar tells a path's root from a value, as for `pkg` and `std`, and a package-wide reservation makes each record of a library avoid its dependencies' names.

### decision bound-packages-are-checked-with-the-program
Every module a bound package's graph registers is checked with the program's own modules, while the bound graph's entries are judged only for their form and select nothing.
- rejected: an agent proposed checking only the modules an entry's composition selects; the owner rejected it, because a library's defects should surface when its user checks the program, not only when an entry happens to select the broken module.
- rejected: an agent proposed judging a library's entries as the program's; the owner rejected it, because the library's entries name functions of its own builds.
