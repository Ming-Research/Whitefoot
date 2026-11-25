# Incremental compilation

How the compiler reuses work across builds: its queries and identities, the evidence and objects it keeps, the cache, and the module workflow commands.

### decision verdicts-and-analyses-are-recording-queries
Module verdicts and function proof analyses are dependency-recording queries over stable module and declaration identities; an entry's composition reruns its whole closure when its records change.
- rejected: an agent proposed letting row positions, `pkg` paths, alias spellings or whole-table hashes decide whether a verdict, analysis or object is reused; the owner rejected it, because spelling edits can keep identities, while deleting an edge or retargeting an alias can break a use.
- replaced: A first stage that committed to persistent formation, lookup, instance, summary and lowering queries was on main; the owner changed it, because no edit-build experiment had yet shown which composition reruns limit a consumer.
- replaced: Building the owned representation only once an edit-latency measurement called for it was on main; the owner changed it, because that weighed build cost only, while the borrowed program had structural faults and a library module must be checked once.
- rejected: an agent proposed importing retained bodies and lowered functions into each composition by per-product identity tables; the owner rejected it, because it missed the needed edit-build cost and grew memory and cache storage (data: `research/experiments/modular-build-cost/RESULTS.md`).

### decision interface-renderer-supplements-handwritten
The handwritten public interface is complete, and a read-only renderer and a conservative revision comparison read the same resolved metadata that incremental consumers read.
- rejected: an agent proposed using generated output to supply missing public declarations; the owner rejected it, because the handwritten interface is the complete public boundary and collaboration benefits from that resolved boundary.
- rejected: an agent proposed treating unchanged local `.wfm` bytes as complete semantic and ABI equality; the owner rejected it, because dependency selection and physical descriptions can change without any change to the local interface text.

### decision source-checks-feed-target-composition
Reusable source checks and concrete heap-use summaries feed target-specific composition checks, and module identity does not depend on entry labels or heap requirements.
- rejected: an agent proposed placing every target flag in all body-proof keys; the owner rejected it, because several executables can share checked libraries, while entry, layout, call or release changes alter only one target's heap requirement.
- rejected: an agent proposed deriving heap freedom only from the public signature or the optimized output; the owner rejected it, because entry, private-layout, call or release changes alter a target's heap requirement without changing the signature.

### decision module-is-one-compilation-unit
A Whitefoot module is one logical compilation unit with one internal inventory, while checking queries, LLVM regions and cached backend units are compiler-owned partitions apart from file names.
- rejected: an agent proposed making every source file a link unit; the owner rejected it, because internal references need neither per-file interfaces nor per-file objects.
- rejected: an agent proposed always equating one Whitefoot module with one LLVM module and object; the owner rejected it, because frontend reuse alone cannot make an indivisible LLVM backend incremental.

### decision boundaries-and-evidence-have-separate-identities
Semantic boundaries and implementation evidence have separate identities, and a retained caller derivation is rebound to a new implementation only after its consumed claims are shown equal.
- rejected: an agent proposed repeating every caller proof when the callee's proof bytes change; the owner rejected it, because a new valid implementation proof need not change the proposition the caller used.

### decision instance-and-component-graphs-maintained
FN-6 instantiation and FN-9 recursive components run on maintained template, binding and call graphs that retract deleted dependencies and recompute affected components from their initial state.
- rejected: an agent proposed treating the module graph as the proof graph; the owner rejected it, because an acyclic module graph still permits concrete callback recursion.
- rejected: an agent proposed keeping an add-only cache; the owner rejected it, because changed call structure can invalidate proofs without any signature change.

### decision generic-instances-are-shared-build-wide
Concrete generic requests share one build-wide instance identity and owner, while keeping distinct symbolic and concrete contexts and per-site obligations.
- rejected: an agent proposed repeating monomorphization in every importer; the owner rejected it, because identical explicit arguments need one checked specialized body and one implementation whatever the request order.
- rejected: an agent proposed replacing specialization with runtime dictionaries; the owner rejected it, because identical explicit arguments need one checked specialized body and physical implementation.

### decision optimization-dependencies-kept-apart
Semantic and optimization dependencies stay distinct: checked bodies and facts cross module boundaries, while IR fragments, optimization plans and native objects are kept separately.
- rejected: an agent proposed permanently opaque independent objects; the owner rejected it, because inlining and specialization create implementation dependencies that do not require source reproof.

### decision thinlto-summaries-are-the-optimization-boundary
ThinLTO summaries are the optimization boundary: stock planning reruns on every build over current summaries, with LLVM's backend object cache.
- rejected: an agent proposed building a version-specific planner adapter before a measurement calls for it; the owner rejected it, because source checking and runtime construction dominate the observed build cost.

### decision final-link-runs-in-full
The final native link runs in full over cached or rebuilt objects and cached runtime units, with composition and target and ABI matching checked before the executable is published.
- rejected: an agent proposed incremental image patching; the owner rejected it, because ordinary optimized linking meets the selected endpoint without incremental-link padding, indirection or a new linker.

### decision fresh-command-validates-sources
A fresh command validates source contents and direct directory membership before reusing the cache, and an unchanged semantic value stops downstream invalidation.
- rejected: an agent proposed treating unchanged timestamps and metadata as content equality for constant-time no-op builds; the owner rejected it, because timestamps and advisory watchers do not establish the source snapshot whose proofs are reused.

### decision module-verdicts-run-concurrently
After frontend admission an entry composition computes its module verdicts concurrently, one thread per processor, and reports the first later rejection in module order.
- replaced: Computing the module verdicts one after another was on main; the owner changed it, because a cold composition ran them on one processor while the others idled, though each verdict is an independent query (data: `research/investigations/compile-speed/DESIGN.md`).

### decision claims-fields-and-receipts-are-distinct-consumers
Written callable claims, published field identities and current proof receipts are distinct query consumers.
- rejected: an agent proposed inferring an unwritten getter equation; the owner rejected it, because a body edit can keep the caller's claims while a change to a published field a contract names changes their meaning.
- rejected: an agent proposed invalidating every importer from one whole-interface digest; the owner rejected it, because a body edit can keep the caller's claims, so such a digest invalidates importers it does not affect.

### decision analyses-run-level-by-level
An inventory's analyses run level by level over its postcondition schedule, each level's functions concurrently, and every concurrent task draws worker threads from one budget of processors.
- replaced: Analyzing a module's functions one after another was on main; the owner changed it, because the slowest module analyzed up to 148 functions of a level in turn; Halo's `pkg::vm` checks in 17.2 s on four processors, 23.5 s on one (data: `research/investigations/compile-speed/DESIGN.md`).

### decision evidence-retained-in-component-fragments
Checked evidence is kept in per-component fragments, finalized once each, and composition assembles the program's receipt, which alone grants lowering, from validated references to current fragments.
- rejected: an agent proposed one checked-program value that owns and discards all the evidence of the exact compilation unit; the owner rejected it, because re-traversing and remapping every derivation of the whole program after each edit would defeat proof-level reuse.
- rejected: an agent proposed discarding retained work after any failed build; the owner rejected it, because a failure elsewhere does not invalidate a proof whose inputs are unchanged.

### decision legacy-bundle-is-one-synthetic-module
The driver keeps a legacy source-bundle entry that forms one synthetic module, exporting only the entry signature and fed to the same checker.
- rejected: an agent proposed converting every existing input when modules land; the owner rejected it, because the existing conformance cases, tests and programs remain the regression evidence for every unchanged rule while they move to module form.

### decision cache-keys-carry-no-host-path
Cache keys identify each record by its stable module identity and logical path with the content its query reads, and a recorded rejection is reused only while the paths it prints are unchanged.
- replaced: Keying every verdict on the display path was on main; the owner changed it, because relative, absolute and `..` spellings of one graph are the same input, yet an unchanged graph was rebuilt whenever its spelling changed.
- rejected: an agent proposed rewriting the paths inside a recorded rejection's text on reuse; the owner rejected it, because a rendering quotes several records' paths in free text beside quoted source, so substitution can alter text that is not a path.
- rejected: an agent proposed that the compiler prune its cache directory of records other compiler identities wrote; the owner rejected it, because a developer's local cache is the developer's to clean, and the owner cleans it himself.

### decision emitted-source-positions-are-templates
Emitted modules keep source positions as templates with fixed module name, logical file, line and column, and bind the display path of the current invocation when cached code is reused.
- rejected: an agent proposed putting the display path into the cache key; the owner rejected it, because every new spelling of the same graph would then rebuild the whole program.

### decision frontend-admission-keeps-parses
Program-wide frontend admission keeps each module's successful parse for later rendering and checking, and only an acceptance covering the exact source records skips those passes.
- rejected: an agent proposed finishing one module before reading the next; the owner rejected it, because every selected record must give its independent syntax faults before later judgments.
- rejected: an agent proposed reparsing every successful module after a diagnostic preflight; the owner rejected it, because successful records need no repeated derivation.
