# Standard library

What the prelude holds, which declarations belong to standard library modules, where the library's source lives and how a program reaches it.

### decision prelude-holds-only-language-declarations
The prelude holds only the declarations a language rule names; every host declaration, ExitStatus and Inputs included, belongs to a standard library module.
- replaced: A prelude holding the host interface was on main; the owner changed it, because no language rule names a host declaration, and injecting all of them into every check cost 36 to 62 percent of a check's instructions (data: `research/investigations/library-modules/DESIGN.md`).
- rejected: an agent proposed keeping ExitStatus, exit_status and Inputs in the prelude to spare the conformance sources that name them; the owner rejected it, because no language rule names them, the runner can recognize the library's ExitStatus as easily as a name, and rewriting tests is no cost.
- rejected: an agent proposed moving the storage shapes, Box or their operations into a library module; the owner rejected it, because their layout, measures and allocation facts are language meaning the checker needs in every module, so a module would remove no compiler knowledge.

### decision prelude-generics-need-no-written-bounds
The prelude's generic declarations, such as `Box<T>`, `Option<T>` and `Result<T, E>`, follow the ordinary generic syntax and leave a bound out where none is needed.
- replaced: A written bound on every type parameter of a prelude declaration was on main; the owner changed it, because a bound is now an optional capability filter in every declaration, where an absent bound already requires no capability.

### decision std-module-read-only-when-named
A check reads a standard library module's interface, and a composition selects the module, only when a graph row lists it or a source bundle names it.
- rejected: an agent proposed that the standard library modules join every check and closure; the owner rejected it, because the graph already decides which interfaces a verdict depends on, and a check that names no host declaration then pays nothing for the host interface.

### decision specification-states-host-module-interfaces
The specification states the exact interface text of the standard library modules whose definitions the build supplies; the library's Whitefoot modules are checked code outside it.
- rejected: an agent proposed moving the host interface out of the specification with the rest of the library; the owner rejected it, because the safety guarantees depend on supplied definitions keeping to their boundaries, and the specification must state the outside conditions they depend on.
- rejected: an agent proposed specifying every library module; the owner rejected it, because a Whitefoot body in the library is checked like any program's.

### decision std-source-ships-inside-the-compiler
The standard library's source is one package in lib/std with its own modules.wfg, and the compiler carries the bytes of its records from its own build.
- rejected: an agent proposed locating the library on disk beside the executable; the owner rejected it, because that adds a way for the library and the compiler to disagree, while a library the compiler carries needs no installed location, search path or version check.
- rejected: an agent proposed a command-line or environment root for the standard library; the owner rejected it, because a program could then be checked against a library other than the one its compiler ships, and no current experiment needs to substitute the library.

### decision containers-are-std-collections-modules
The container libraries are the standard library's Whitefoot modules under std::collections.
- replaced: Container source kept as text a caller concatenates before its own was on main; the owner changed it, because a library shipped with the compiler is shared by every program without a binding, location or version, and the containers are measured code whose boundaries the owner adopted.
- rejected: an agent proposed deferring Whitefoot library modules until more helpers exist; the owner rejected it, because the containers are already measured reusable code whose boundaries the owner adopted, and a library shipped with the compiler is shared by every program.