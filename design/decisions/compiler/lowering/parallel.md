# Parallel lowering

How the compiler lowers proved-independent calls and ranges under `--par`: statement groups, range splits, indexed reductions, and which offers it publishes.

### decision range-split-leaves-stay-loops
A parallel range split stops at leaves that each still loop over a subrange, never at leaves of one iteration.
- rejected: an agent proposed splitting a range down to one-iteration leaves; the owner rejected it, because such a leaf removes the loop shape the body's own optimization needs, 3.6 to 7.6 times slower on light bodies (data: `research/investigations/proof-derived-parallelism/loop/DESIGN.md`).

### decision runtime-queues-stay-target-private
Native queues, helper lanes, wakeups and completion ports are private runtime state of the target, never Whitefoot shared storage.
- rejected: an agent proposed exposing runtime queues as language values; the owner rejected it, because the source model has no scheduler, and a runtime structure seen as source storage would need ownership and effects it cannot honestly carry.

### decision broken-parallel-startup-fails-loudly
A Windows module with compute offers needs the runtime at link time, a bad worker setting fails before the body, and a startup with fewer workers than asked keeps those that started.
- rejected: an agent proposed failing the process on any incomplete worker startup; the owner rejected it, because fewer threads than asked is not a broken configuration, and the refused-offer path already computes the same result.

### decision call-offers-need-recursion-or-the-work-unit
Under `--par`, a group call is offered only when its callee reaches a recursion that offers its own calls or its static work reaches the 150,000-unit work unit.
- replaced: The scalar-leaf limit was on main; the owner changed it, because the costly callees took aggregates or branched, outside its reach, and a real program's cheap offers made page setup up to 37 times slower at four workers (data: `research/investigations/call-offer-grain/DESIGN.md`).
- replaced: Exempting each callee that reaches a recursion was on main; the owner changed it, because the budget spends no level in a recursion offering none of its calls; an edit took 3,514 microseconds at four workers, 394 alone (data: `research/investigations/recursive-offer-grain/DESIGN.md`).
- rejected: an agent proposed pricing every offer by its callee's weight, with no recursion exemption; the owner rejected it, because that removes adaptive quadrature's recursive offer, and its kernel then runs 2.3 to 2.4 times slower (data: `research/investigations/call-offer-grain/DESIGN.md`).
- rejected: an agent proposed offering only a callee that reaches a loop or recursion; the owner rejected it, because a loop is not work: loops of zero or one trip kept 72 offer sites, and setup stayed 2.9 to 5.8 times slower (data: `research/investigations/call-offer-grain/DESIGN.md`).
- rejected: an agent proposed keeping the exemption and spending a budget level at each activation that offers; the owner rejected it, because an entry could still hand out some 256 tiny tasks, and it changes a rule Snowghost uses (data: `research/investigations/recursive-offer-grain/DESIGN.md`).
- rejected: an agent proposed keeping the rule and rewriting the downstream program so it reads no cursor pair in parallel; the owner rejected it, because that routes around a compiler gap in the program that exposed it.

### decision sequential-refusal-is-opt-in
`--par-sequential-refusal` is off unless asked for, and the call grain prices each callee from its static work summary alone, with nothing added at run time.
- rejected: an agent proposed sequential refusal by default; the owner rejected it, because it acts only on the 0.3 percent of offers refused for a full deque, and setup stayed as slow as offering everything (data: `research/investigations/call-offer-grain/DESIGN.md`).
- rejected: an agent proposed stopping, at run time, a call site's offers that do not pay; the owner rejected it, because setup stayed 1.5 to 2.2 times slower, it failed the regression rule on two kernels, and offers depended on the run (data: `research/investigations/call-offer-grain/DESIGN.md`).

### decision range-prices-use-captured-extents
A range split prices its iterations from captured counted-loop extents and helper-argument lengths where it has them, and from the static estimate elsewhere.
- replaced: A fixed inner-loop factor was on main; the owner changed it, because it gave one price to blocks of very different sizes, left the measured prefix and histogram blocks unoffered and held wide stencil to a few chunks (data: `research/investigations/compute-model/DESIGN.md`).

### decision box-array-extents-reach-helpers
A range's work summary also reads the runtime Array length behind an original read-only `&Box<Array<T>>` formal, carried through exact direct chunk captures.
- rejected: an agent proposed reading any captured address or Box as a descriptor, or following any aggregate path, while pricing; the owner rejected it, because a capture may hold an owner already consumed, and enum or indexed paths may need facts the split site does not have.
- rejected: an agent proposed a separate validity or lifetime analysis in the work estimator; the owner rejected it, because the checker owns that judgment, and lowering can carry its no-write fact for original formals through exact captures.
- rejected: an agent proposed requiring a scalar measure already captured at the outer loop; the owner rejected it, because the affected source reads its length only inside the helper, so the helper gap would stay.

### decision statement-groups-keep-non-call-statements
A permitted run lowers into call-rooted groups in one IR block, and a non-call statement stays on the owning thread at its source place inside the group's window.
- replaced: Ending a group at every non-call member was on main; the owner changed it, because PAR-1 already proves the whole run pairwise independent, so an independent statement between two calls kept them from overlapping although nothing ordered them.
- rejected: an agent proposed keeping only a run's leading call-rooted prefix, dropping the rest after its first unusable call; the owner rejected it, because every part of the run has the same pairwise permission, and it lost the recursive calls after a scalar statement.
- rejected: an agent proposed treating adjacent-pair permission as transitive; the owner rejected it, because a larger group needs every ordered pair proved independent, so adjacent pairs form only two-member groups.
- rejected: an agent proposed redoing the permission analysis in lowering; the owner rejected it, because every group lowering forms already keeps an existing PAR-1 proof of its run.
- rejected: an agent proposed bounded call-group bridges with tail-first publication from the chain root; the owner rejected it, because the identical-image cost control failed before any comparison, so their added cost stayed unqualified (data: `research/investigations/compute-model/DESIGN.md`).

### decision overlap-groups-order-releases-before-borrows
No two members of an overlap group pair a released place with an overlapping borrowed place under [OWN-7], judged with that pair's proved separations; a conflict ends the group.
- rejected: an agent proposed never letting a releasing call share a group with a borrowing call; the owner rejected it, because a written Box reference is both, so recursion over disjoint owned subtrees, as in `tests/programs/parallel/tree.wf`, would never overlap.
- rejected: an agent proposed tracing each argument's root and steps through IR values; the owner rejected it, because control-flow joins lose a shared root, range formation gives one storage unequal path depths, and consuming an aggregate releases sibling fields too.
- replaced: Comparing places with an unproved overlap oracle was on main; the owner changed it, because it serialized sibling calls on proved-disjoint ranges of an owning-element run, and Snowghost's layout lost its four-thread parallelism.
- replaced: Ordering a loaded owner slot against every overlapping release, below it as well, was on main; the owner changed it, because storage below the slot lies in the block it points to, a release frees only its own subtree, and the rule serialized Snowghost's sibling-field swaps.

### decision permission-evidence-is-pair-scoped
The PAR-1 planner records ordered statement-pair range questions that the flow proves at the first statement's state, and permission uses only each pair's own evidence.
- rejected: an agent proposed a function-wide EFF-5 ledger of footprints; the owner rejected it, because a union of footprints loses the statement each came from, and the independence of adjacent pairs is not transitive.

### decision permission-reads-whole-statement-footprints
Permission uses each member's whole footprint, every arm of a call-rooted match included, and refuses exit-bearing or unclassified control flow; non-call members get no hand-out.
- replaced: A footprint of only a call-rooted match's scrutinee was on main; the owner changed it, because it ignored interference from the match's arms.
- rejected: an agent proposed denying every call-rooted match after the reference-only port; the owner rejected it, because the call in an if condition can still be the last member of a group that joins before dispatch.
- rejected: an agent proposed cloning every function that contains any permitted adjacency; the owner rejected it, because non-call members get no hand-out, and the clone rule depends on handed-out calls.

### decision splitter-keeps-runtime-query-entry
A range split enters through the runtime query and the overlapping splitter, while the sequential world calls its chunk directly.
- rejected: an agent proposed a zero-budget caller branch at every split site; the owner rejected it, because it showed no general benefit and regressed records in a qualified comparison, the sequential path that never runs it included (data: `research/investigations/compute-model/DESIGN.md`).
- rejected: an agent proposed replacing the entry with a wrapper, a noinline attribute or an alignment change; the owner rejected it, because none was qualified, and a replacement must show a benefit in both worlds.

### decision paged-growth-releases-its-owner
A written `&Box<Paged<T>>`, `grow_paged` included, releases its owner place, and cell, element, page and run references borrow places below it, under the general overlap boundary.
- replaced: A pair of Paged-specific IR cuts was on main; the owner changed it, because the cuts also kept apart borrows with no overlapping release, while the general released-and-borrowed boundary orders growth against every borrow it affects.

### decision indexed-reductions-use-private-cells
A split indexed reduction gives each leaf private identity-filled cells or false-filled mark masks, combined in leaf order after the join, with no synchronization.
- rejected: an agent proposed atomic updates of the shared cells; the owner rejected it, because each update pays synchronization and hot-cell contention, and minimum, maximum and wrapping multiplication need compare-and-swap loops (data: `research/investigations/indexed-reductions/DESIGN.md`).
- rejected: an agent proposed dividing the cells among the workers; the owner rejected it, because every worker then scans every iteration and evaluates its subscript, multiplying that work by the worker count (data: `research/investigations/indexed-reductions/DESIGN.md`).

### decision indexed-updates-reuse-the-outlined-body
Indexed measure reads and one-step temporary updates use the ordinary outlined body with private-root remapping, and unsigned saturating sums use zero-filled private cells.
- rejected: an agent proposed rebuilding temporary updates in lowering, or checks at run time; the owner rejected it, because the outlined body already keeps each leaf's contributions independent without another executable form (data: `research/investigations/indexed-reductions/DESIGN.md`).

### decision indexed-families-carry-their-layout
Each indexed family carries its operation and identity or mark constant, its root element type and field projection, and its storage mode; dense slabs use the family's value stride.
- rejected: an agent proposed copying whole records into private storage; the owner rejected it, because packed private scalars and masks must address layouts other than the original elements', and nested joins write into their enclosing private storage.

### decision call-updated-roots-get-root-shaped-blocks
When any family of a root has a call update, all its families share one root-shaped private block per leaf, keeping its header and measures; other roots keep dense slabs.
- rejected: an agent proposed passing dense family slabs to the callee; the owner rejected it, because a callee indexes the whole root by logical index and may read its measures, so a dense slab cannot stand for the root (data: `research/investigations/indexed-reductions/DESIGN.md`).

### decision guarded-calls-lower-to-guard-functions
A one-call `if` with a call-free condition and total-read arguments is a PAR-1 member, lowered beside an adjacent call member as a call of a synthesized guard function.
- replaced: Refusing every conditional statement as a group member was on main; the owner changed it, because PAR-1 already admits such a statement, and the refusal blocked every guarded call, not only Snowghost's.
- rejected: an agent proposed a guarded hand-out operation in the emitter; the owner rejected it, because a guard function keeps every member a call in one block, so hand-out, join, call grain and recursion budget apply unchanged.
- rejected: an agent proposed admitting every `if` and `match` statement and handing it out as an outlined task; the owner rejected it, because that needs captures, ownership transfer and frame sizing for forms no measured program needs, while a guarded call covers the measured shape.

### decision held-callers-lower-sequentially
A function that declares a held observation or makes a held-requiring call lowers its permitted calls and loops sequentially; other functions keep their parallel selection.
- rejected: an agent proposed handing those calls to compute helpers; the owner rejected it, because the restricted export's preparation and post-hold activation belong to the calling executor, which the helpers have not prepared.
- rejected: an agent proposed rejecting such source instead; the owner rejected it, because every forwarding path carries the finite held contract, so lowering can keep those calls sequential without refusing the program.
