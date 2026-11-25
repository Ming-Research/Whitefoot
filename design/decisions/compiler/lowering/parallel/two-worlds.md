# Two worlds

The overlapped and the sequential lowering a `--par` build emits, the recursion budget that moves a call from one to the other, and how a handed-out task's frame is formed and fitted.

### decision par-builds-emit-two-worlds
A `--par` module has an overlapped and a byte-identical sequential lowering of each function that can reach a hand-out, and the default world is chosen once at startup by whether a pool was asked for.
- rejected: an agent proposed one lowering in which a per-task demand signal switches to a sequential subtree; the owner rejected it, because a joined hand-out takes away tail position and can block tail-recursion elimination, and the signal's contended updates charge every fine-grain call.

### decision clone-set-follows-reachable-hand-outs
The functions that get a sequential clone are those reachable from the entry that can reach a hand-out, derived from the call graph and the permission table.
- rejected: an agent proposed cloning every function; the owner rejected it, because only functions that can reach a hand-out lower differently in the two worlds.

### decision refusal-enters-the-sequential-clone
With `--par-sequential-refusal`, a refused compute offer enters the callee's existing sequential clone, with its ordinary signature, joins and result ABI, and offers nothing beneath it.
- rejected: an agent proposed a runtime demand signal read on each call; the owner rejected it, because refusal reuses the failed-acquisition edge every offer already has, so it adds none of the per-task demand query whose contention cost was refused.

### decision recursion-budget-spends-only-at-groups
Each cyclic call component gets a budget variant behind its ordinary signature; only a call in an actualized statement group spends a level, and an empty budget enters the sequential clone.
- replaced: N private layers of parallel functions before the sequential clones was on main; the owner changed it, because the budget variant reaches the same cut with one compare and branch per node and no N copies of each body (data: `research/investigations/compute-runtime/RESULTS.md`).
- replaced: A decrement at every call into the component was on main; the owner changed it, because calls outside every group spent the budget first, holding Snowghost's style traversal to 1.04 times at four workers (data: `research/investigations/call-offer-grain/DESIGN.md`).
- rejected: an agent proposed keeping the decrement at every call and raising the default budget; the owner rejected it, because the descent depth belongs to the input, so no fixed allowance covers every document, and a larger budget deepens offer nesting in balanced recursions.

### decision waiting-components-get-no-budget-variant
A component with a waiting member, a synthesized loop or no sequential clone gets no budget variant, has a `--par-ledger` reason, and makes the offers it reaches without a budget.
- rejected: an agent proposed a budget-carrying frame variant for a waiting component; the owner rejected it, because it needs a second resumable-frame signature and a spent-budget transfer to bound an offer depth no measurement asked for.
- rejected: an agent proposed withdrawing the offers a waiting recursion reaches; the owner rejected it, because they are permitted ordinary calls whose lane frames fit and whose joins complete inside the ordinary function that makes them.

### decision initial-budget-follows-lane-count
The initial recursion budget is floor(log2(64 times the lane count)), at most 24; `--par-recursive-frontier auto|N|off` picks that default, a fixed N in 1..32, or no budget variants.
- rejected: an agent proposed one compile-time depth for all pools; the owner rejected it, because the depth and width comparison found that the best fixed depth changes with the worker count (data: `research/investigations/compute-runtime/RESULTS.md`).

### decision hand-out-claims-its-lane-first
A handed-out call claims its lane before it builds the lane-owned frame, so a refused offer leaves no frame behind.
- rejected: an agent proposed placing task frames in the caller's activation; the owner rejected it, because allocating at caller entry charges every recursive activation for an offer it may not make, and can exhaust the caller's stack.

### decision range-split-snapshots-box-pointers
A range split copies a promoted `Box` owner's pointer value into its splitter and chunk frames, while inline affine aggregates keep their addressed capture and the source scope stays the only owner.
- replaced: Carrying the parent owner-slot address through every task was on main; the owner changed it, because every worker then reloaded live caller-stack storage, and the records kernel regressed at both measured parallel widths.

### decision box-array-captures-carry-element-base
For a promoted `Box<Array<T>>` capture, a split frame carries the element-base pointer and rebuilds the allocation-base pointer at chunk entry from the target's header offset.
- rejected: an agent proposed changing the Box representation for every use; the owner rejected it, because the experiment supports only this bounded capture family and claims no general storage-layout result (data: `research/investigations/access-effects/parallel-array-captures.md`).
- rejected: an agent proposed carrying an additional pointer in the frame; the owner rejected it, because the element base fits the existing one-word frame entry.

### decision oversized-loops-keep-only-needed-captures
A synthesized loop whose capture frame exceeds the lane-frame bound keeps only the captures its finished chunk uses at run time, found by tracing block-parameter forwarding back from real uses.
- rejected: an agent proposed changing the capture transport and code placement of every fitting task; the owner rejected it, because keeping only needed inputs admits the oversized loop while already-fitting loops keep their interface and helper order.
- rejected: an agent proposed a general capture projection or an interprocedural dead-argument optimizer; the owner rejected it, because following the existing runtime operands removes the shown obstruction without a new storage representation or new call semantics.
- rejected: an agent proposed replacing unused value types with tombstones or renumbering the whole chunk; the owner rejected it, because the existing flow graph already names the real definitions and keeps stable value identities.

### decision oversized-candidates-reuse-their-cfg
An oversized loop candidate that is refused moves its finished ordinary control-flow graph into the parent, keeping nested synthesis and rebinding generated capture roots to the original storage.
- rejected: an agent proposed discarding a refused candidate and lowering its source again; the owner rejected it, because rebuilding nested refused candidates repeats construction exponentially.
- rejected: an agent proposed outlining the fallback behind a new call; the owner rejected it, because an ordinary helper call would add run-time cost to every refusal.
- rejected: an agent proposed accepting every candidate and leaving refusal to the emitter; the owner rejected it, because a truly oversized loop would keep splitting recursively although it cannot hand out work.

### decision split-frames-fit-by-target-layout
After needed-capture selection, a range split whose conservative frame estimate still exceeds the lane bound is kept when the shared selected-target layout shows its transported signature fits.
- rejected: an agent proposed a separate copy of the layout arithmetic for frame fitting; the owner rejected it, because the shared selected-target calculation already gives the exact size.
- rejected: an agent proposed deciding exact fit before needed-capture selection; the owner rejected it, because that changes the existing rescue's transport and helper order without serving the obstruction shown after pruning (data: `research/investigations/compute-model/DESIGN.md`).