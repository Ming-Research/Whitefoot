# Modules

How a program is split into modules: the module graph and its file, what a module's verdict depends on, entries, and cached checking.

### decision a-program-is-a-graph-of-modules
A program is a closed graph of modules, each checked on its own against the interfaces it names, and it composes only when every declared function has a verified body [MOD-1, MOD-8].
- replaced: One closed compilation unit with no modules or separate compilation was on main; the owner changed it, because large projects and agents writing interfaces and bodies in parallel need checked modules, and a boundary composed with checked evidence trusts no unverified restatement.
- rejected: an agent proposed accepting an exported declaration with no currently verified implementation; the owner rejected it, because publishing and linking cannot prove that the implementation meets the declared contract.

### decision a-module-verdict-reads-only-interfaces
A module's source verdict depends only on its own records, the graph, the prelude and the interfaces of the modules it may name, never on another module's bodies [MOD-8].
- rejected: an agent proposed checking each module only together with its dependencies' complete bodies; the owner rejected it, because an implementer could not verify a module while a dependency's body is missing or failing, and one module's edit could change another's verdict.

### decision one-graph-file-declares-module-dependencies
One graph record, `modules.wfg`, registers each module once and lists its exact direct dependencies, each an earlier row, which keeps the graph acyclic [MOD-1].
- rejected: an agent proposed ordering modules by namespace subtrees; the owner rejected it, because explicit edges in written order keep the graph inspectable while a graph edit leaves every namespace path unchanged.
- rejected: an agent proposed ranks spread over the modules; the owner rejected it, because explicit edges in one file, each to an earlier row, make the graph inspectable and checkable row by row.
- rejected: an agent proposed import lists repeated in each source; the owner rejected it, because one graph file as the sole source of declared dependencies makes the graph inspectable and checkable row by row.
- rejected: an agent proposed unordered edges, with the order inferred from the imports after cycle detection; the owner rejected it, because the intended direction must be declared before an edge is judged, not rebuilt after a cycle forces a choice of which dependency to keep.
- rejected: an agent proposed granting every earlier module as a dependency; the owner rejected it, because order shows direction but not the exact intended graph, so each used module still needs a listed edge.

### decision graph-entries-name-their-functions
The graph's directory is the package root, and each `entry` of the graph names a public function, with an optional `no_heap`, all entries sharing one module graph [MOD-1, MOD-9].
- rejected: an agent proposed a root binding that the writer names; the owner rejected it, because the graph file already locates its own root.
- rejected: an agent proposed searching for the root from the working directory; the owner rejected it, because the graph file already locates its own root.
- rejected: an agent proposed one heap mode for the whole graph; the owner rejected it, because a heap-free kernel and heap-using tools must reuse libraries without one program's environment being imposed on another.
- rejected: an agent proposed one graph per executable; the owner rejected it, because a heap-free kernel and heap-using tools must reuse libraries without duplicating their graph.
- rejected: an agent proposed a new keyword used only in the graph; the owner rejected it, because reusing the words `entry` and `no_heap` keeps common binding names such as `target` free.

### decision no-heap-covers-only-what-an-entry-reaches
Every module is checked in full whether an entry reaches it or not, while an entry's `no_heap` covers only the functions it reaches, reported at the first that uses the heap [MOD-8, STOR-8].
- rejected: an agent proposed checking safety only for the functions an entry reaches; the owner rejected it, because unused definitions still need their source proofs.
- rejected: an agent proposed refusing unrelated tool code under a kernel's no-heap requirement; the owner rejected it, because a heap-using helper outside an entry's execution closure need not put a heap requirement on that entry.

### decision cached-and-fresh-checks-agree
Cached and fresh compilation reach the same judgments through one checker, and a build validates the current evidence of every dependency it uses.
- rejected: an agent proposed treating imported declarations as fact sources of their own; the owner rejected it, because an implementation edit can make postcondition proofs fail without changing a signature.
- rejected: an agent proposed treating previously successful cache entries as fact sources of their own; the owner rejected it, because a cache must not put stale proof evidence in place of current verification.

### decision generic-instance-component-edges
For recursive-component formation, a call to another module's generic instance counts as a call of every function actual and bundle member supplied to it.
- rejected: an agent proposed forming cross-module components from the callee body's actual call edges; the owner rejected it, because a callee body edit that starts calling a supplied actual would change the caller's verdict with no interface change.

### decision per-module-summary-publication
In a recursive component spanning modules, the members one module's check verifies publish their summaries to that module once all of them verify; composition still checks every member.
- rejected: an agent proposed publishing a component's summaries only after every member, another module's instance included, verifies; the owner rejected it, because a module's verdict would then depend on another module's body.
