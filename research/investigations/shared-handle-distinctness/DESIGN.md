# Proving distinct shared state through handles and selections

## Decision to make

**Recommendation: A, allocation distinctness carried by verified directory
invariants, together with D, a separate finite relation for disjoint selected
references. Confidence: 4/5 in the separation of these two mechanisms; 3/5 in
their proposed proof transport until the checks below have been demonstrated.**
This is a design proposal, not an amendment or an implemented capability.
It covers the two normalized semantic witnesses, not yet every possible Firn
selector. Two separate database-number dispatches can require a relation
between the selected handle and its index; inspect those selectors before
treating A+D as a complete release solution. B directly supplies that relation.

The question is independent of Redis: after constructing two distinct shared
objects, storing their handles in an aggregate and passing that aggregate to a
function, can one atomic statement pass both states to an ordinary helper that
writes both? Separately, can two disjoint references remain provably disjoint
after a conditional permutation? The desired answer to both is yes, without a
runtime identity test, an inlined helper, separate atomic statements, or a
combined shared object.

The supplied downstream report identifies seven blocked Firn call sites and
puts its 16-database support in the first release at P0. That motivates the
study; those counts and the downstream CI diagnostics were not independently
verified here. The semantic witnesses, rather than Firn's command names, define
the required capability.

| Direction | What the writer gains | Principal cost and limitation | Recommendation |
| --- | --- | --- | --- |
| A. Allocation distinctness | An invariant such as `distinct(d.db0, d.db1)` proves that two handles retain different objects across a directory boundary. | A proof-only identity domain, placement transport and invariant obligations. Freshness alone loses conditional correlations. | Adopt as the general foundation. |
| B. Indexed family | A family constructor guarantees distinct objects at distinct indices; ordinary integer proofs discharge `i != j`. | A new container, construction and selection rules; arbitrary independently selected references still need D. | Reconsider if indexed selection is the decisive remaining obstacle; do not require a representation change to solve the field witness. |
| C. Overlap-permitting parameters | A declared helper works even when its two arguments alias. | Changes the callee's alias, proof, scheduling and optimizer contracts. Does not prove maps distinct when the algorithm requires it. | Useful independent language direction, but too broad for this need and unable to replace distinctness. |
| D. Must-disjoint selected references | A pair that is disjoint on every incoming edge stays disjoint after a join. | A finite relational ownership fact and its invalidation/loop rules. Supplies no missing shared-object identity by itself. | Develop separately, then compose with A. |

The comparison that could reject the recommendation is concrete: express the
two witnesses, a directory factory and consumer in different modules, and a
two-database selection from 16 fields with the proposed rules; compare with B
using indices and C using an alias-safe helper. Reject A+D as sufficient for
the release if the selection requires a fact neither mechanism expresses, or
if the proof requires enumerating combinations of branch histories. No timing
or successful compilation is claimed by this study.

## Current contract and evidence

The source authority is the active [kernel specification](../../../spec/kernel-spec.md),
read here as v0.149. The [constitution](../../../docs/constitution.md#development-objectives)
requires useful, feasible construction and verification; its
[safety clause](../../../docs/constitution.md#safety) requires static proof.
SCOPE-2, ENT-2 and the proof rules supply the precise erasure and deterministic
checking obligations: no added object ID at runtime, dynamic alias guard,
fallback, solver budget or branch-history search is proposed.

Three distinctions matter before choosing a mechanism:

1. **Handles are aliasable, but not implicitly copyable.** PRE-1 declares
   `Shared<T>` and `SharedRead<T>` `nocopy`. SHARE-1's `shared_share`,
   `shared_read` and `shared_read_share` retain the same object. A reference
   alias also reaches the same handle. The phrase “copied handle” below means
   an explicitly retained alias; an ordinary value copy still rejects under
   OWN-1. Moving a handle never allocates a new object.
2. **Readonly is not permanent immutability.** TYPE-2 permits writes inside
   the declaring module and permits replacement of the whole owner. The
   invariant must be established and preserved at checked boundaries; the
   field modifier alone is no proof. Type invariants are owned by **TYPE-11**
   in this revision. TYPE-7 owns explicit reference dereferencing.
3. **An atomic state is not its handle slot.** SHARE-2 evaluates the handles
   at entry and holds the objects alive through exit even if handle slots
   change. Distinct struct fields establish distinct handle storage, not
   distinct shared state. For potentially unifying state types, SHARE-2
   supplies possible overlap, and EFF-5 correctly rejects the helper call.

The [shared-map witness](shared-map-alias-witness.wf) has the right semantic
shape. Its comment predicts a rejection at an older pin; it is not a recorded
run of this reduction. The [selected-reference witness](selected-reference-alias-witness.wf)
also records a prediction. Its `set source = second` spelling is not the
rebinding form specified by REF-1 and SET-1; Appendix I uses
`set source = &second^` and its counterpart to isolate the intended gap.
The original files are preserved. No exact diagnostic is claimed for their
unexecuted bytes.

At the second witness's join, REF-1 gives each holder the set `{first,
second}`. Independent sets include the impossible pair `(first, first)`.
The missing fact is a relation between the *simultaneously selected targets*,
not another unary target or integer bound.

The current [shared-object decisions](../../../design/language/waiting/shared-objects.md)
reject requiring distinctness before entering `atomic`: aliases are valid and
the statement holds their object once. A and B preserve that decision. They
provide optional evidence for operations inside the statement. They do not
make distinctness an atomic admission requirement or remove deduplication.
The [checker facts](../../../design/compiler/checker-facts.md) already retain
pairwise possible overlap rather than transitive alias classes; that remains
necessary. From `distinct(a,b)` and an unknown `c`, nothing follows about
either `(a,c)` or `(b,c)`.

## Appendix A: allocation distinctness with an aggregate invariant

Use a new **proof-only** predicate `distinct(h1, h2, ...)`, meaning that every
pair of the evaluated handles retains different shared objects. It is an
erased clause form, not a function call or a runtime `Bool` expression.
Appendix F gives complete programs. A 16-field directory can state one
16-operand clause, expanded into its 120 unordered pairs. Different directory
values need not contain different maps: the guarantee is within each value.

The chosen extension is to TYPE-11 rather than a field marker promising
immutability. Construction owes the relation; a function receiving the
directory gains it through the declared type invariant; a directory factory
proves it on return. A writer can also declare a binary `requires distinct`
or `ensures distinct` over handle parameters and result projections. A body
cannot gain a caller's allocation history implicitly.

### Proposed rule delta

| Owner | Change to the present judgment |
| --- | --- |
| GRAM-2, GRAM-5, TYPE-11 | Admit the new structural invariant/contract clause over at least two accessible `Shared` or `SharedRead` handle places. Type-check all operands without copying or consuming them. Keep the existing non-generic source-struct and visibility restrictions for type invariants. |
| SHARE-1, PRE-1 | Fresh construction establishes an object identity different from all objects already live at that event. Retaining and read-only conversions preserve identity. These are semantic facts of their existing operations, not a runtime field or an unrestricted source freshness annotation. |
| FN-8, FN-9, CALL-4 through CALL-6, FN-4 | Add finite identity-relation templates, result/parameter substitution, requirement discharge and verified postcondition publication. Refinement proves the actual's requirements from the formal's guarantees and preserves promised distinctness; recursive calls use declarations, not unfolded bodies. |
| TYPE-11 | Apply identity invariants at construction, direct parameter and direct result boundaries, written-reference exit states, and atomic entry/exit, with the existing exit coverage. Do not silently extend the current affine-only enum-payload rule. |
| ENT-2, ENT-3, ENT-5 | Define a separate identity fact domain and its sources, placement, support, invalidation and meet. Identity is not a fragment integer and cannot be arithmetically compared or recovered from pointer bits. |
| SHARE-2, OWN-7, EFF-5 | Capture the identity evidence with each atomic target. Proved different identities separate their state roots and all descendants; otherwise retain today's possible-overlap judgment and all ordinary effect comparisons. |
| DIAG-1, DIAG-2 | A failed distinctness obligation names both handle paths and the absent or killed relation; successful obligations retain their derivation. |

`distinct` is symmetric and irreflexive, **not transitive**. Known equality
from retaining a handle substitutes in a distinctness fact. Unknown identity
never proves inequality. Two calls to a wrapper do not imply distinct results
merely because they are different calls: its verified result contract must
provide the required relation. For this proposal, factories return an
invariant-bearing directory; a general “fresh relative to every hidden live
object” function postcondition is outside the initial contract vocabulary.

### Preservation and soundness

| Event | Required behavior |
| --- | --- |
| Fresh allocation | Introduce a fresh current object image, distinct from every represented live image at this event, including old loop-carried handles. A static allocation site is not an object identity across executions. |
| Explicitly retained handle or `SharedRead` conversion | Carry the same image. A retained alias of `a` is not distinct from `a`, but remains distinct from any `b` already proved distinct from `a`. Reference aliases to handle slots do not create objects either. |
| Struct construction, placement or move | Transfer field-to-object associations with the moved value, then invalidate consumed source places as usual. Moving a directory preserves its internal relation but not references into the old owner. |
| Field or whole-owner replacement | Kill every relation supported by the overwritten handle slots or owner before publishing incoming value facts. A newly constructed replacement can carry its own proved invariant. A field replaced by an alias of its sibling immediately loses separation. |
| Temporary invariant break inside its module | Allowed until a boundary that owes the invariant. Do not re-assume the invariant merely because the damaged value still has the same nominal type. A helper demanding distinctness rejects while it is broken. |
| Function boundary | Only declared requirements, verified postconditions and the admitted type-invariant sites transport facts. A potentially writing call kills affected slot facts before its verified exit relation is published. A read of map contents changes no handle identity. |
| Conditional selection | Keep a relation only if its substituted statement-current meaning holds on every reaching input. Independent sets of possible objects are insufficient for swapped selections; D supplies the missing relation. |
| Loop | Stable, unmodified directory facts survive. A replaced loop-carried handle loses its old slot relation; each new allocation may establish a new one within that iteration. Do not identify objects by a repeated source site. Relational induction over changing selected holders is D's separate obligation. |
| Atomic entry and later handle replacement | Capture objects and their separation at entry. Replacing the handle slot kills future slot-based claims, but cannot retarget the already-held state. Those captures end on every atomic exit. |
| Map mutation or map `swap` | Contents change; the containing shared objects do not. Distinctness survives. A content operation never certifies that two unknown handles were distinct. |

The soundness argument is induction over these events: construction creates
different live objects; sharing preserves an object; transport changes only
the names of proved images; mutation removes unsupported facts; and boundaries
introduce only obligations checked against the producer's actual exit state.
An identity relation does not imply equality of the object's numeric fields,
reference validity, or authority to write a `SharedRead` target.

### Cost and limit

For `h` live relevant handle images, an explicit pair representation uses at
most `h(h-1)/2` distinctness entries; placement and kills visit the affected
entries, and a join meets finite current pairs. Freshness can use a generation
representation internally if it answers exactly this specified relation.
The checker must not retain unbounded generations across loops. Facts are
owned by the current function and its finite capture/header vocabulary.

An ordinary one-map path needs no pair relation, no numeric closure enlargement
and no new runtime work. There can still be a small checker dispatch or image
bookkeeping cost; zero elapsed-time cost is a measurement claim not made here.
Optional backend facts must satisfy the complete existing target contract.

A solves the stored-directory and direct-helper witness. It does not make a
general `Array<Shared<T>, n>` injective, automatically recover an arbitrary
factory's freshness, admit identity in runtime expressions, or solve all
relations lost by numeric or reference joins. Nested containers and enum
payload transport require their own boundary rule; they cannot inherit an
invariant merely from a contained type's name.

## Appendix B: a fixed indexed family

The narrow concrete option is a proposed opaque `nocopy`
`SharedFamily<T: drop, const n: u64>`. Its constructor for maps performs `n`
separate `shared_map_new` allocations. `family[i]` is a readonly handle place;
the slots cannot be replaced, even inside a source module. Sharing one of its
handles is allowed and retains that element's identity. Appendix G shows a
complete dynamic-index program, with both maps passed to one helper under one
atomic statement.

An element has proof identity `(family origin, captured index)`. The origin
travels with the family on a move and through its declared type boundary;
it is not the stack address of the container. Different indices of the same
family imply different objects. Two family parameters are not implicitly
different families, nor are equal indices from two unknown families distinct.

**Exact delta:** GRAM-3/PRE-1/TYPE-9 admit the family type and its opaque
constructor, with `T` occupying shared-object state and each stored element
having type `Shared<T>` (a map is never a directly stored array element);
OP-4 admits bounded selection of its handle slots; SHARE-1
specifies injective construction and identity-preserving handle retention;
SHARE-2/OWN-7 consume a captured family/index relation, with ENT-6's existing
index-disequality query; ENT-2/ENT-5 and CALL-4 through CALL-6 transport the
captured origin and index association without integerizing the object; REF-1
and REF-2 preserve normal validity, and TYPE-2/SET-1 refuse slot replacement.
A family constructor is not an ordinary array initialized by repeating one
handle. A generic source container with a quantified `distinct` invariant is
another B, but requires formation, construction and mutation proofs for
quantified identity relations; current integer range clauses do not provide
that capability.

**Soundness cases:** a retained element handle keeps its origin/index;
moving the family keeps all associations; a function over `&SharedFamily`
uses only the family guarantee and its integer requirements; replacing the
whole family kills associations to its slots but not independently retained
old handle snapshots. Replacing an individual slot is rejected. Unmodified
families survive loops, while mutable indices need their ordinary bounds and
disequality at each selection. Capturing `family[i]` records the evaluated
index; later changing `i` never retargets a handle or an atomic state. Separate
conditional selections still need D unless their captured index relation is
already proved; two independent possible-index sets cannot prove correlation.

SWAPDB keeps the handles in their slots and exchanges the held maps' contents
with the existing `swap` meaning. The [state-locking decision](../../../design/compiler/waiting-contexts/state-locks.md)
explicitly keeps object identity stable when maps are swapped. MOVE/COPY use
the same `i < n`, `j < n`, `i != j` boundary to give source and destination
maps to the command helper; within it the normal key, value and command
preconditions still apply. Equal database indices need the command's ordinary
same-database semantics, not a failed proof turned into a runtime fallback.
These comparisons are over database numbers, not object identity.

If the intended SWAPDB implementation actually permutes handles, immutable
slots do not express it. An additional `family_swap` operation could preserve
injectivity while advancing the family's selection version. Old retained
handles keep their old object image; facts relating current slots to old
indices must be killed. That versioned permutation contract is additional
design, not supplied by the minimal family proposal.

**Cost:** one origin and captured index per relevant selection, plus existing
integer separation queries, avoids enumerating `n(n-1)/2` pairs for a regular
family. Construction still allocates `n` maps; it adds no runtime distinctness
test or proof-only state. A single-map path outside this container pays no
family analysis. A one-element family has ordinary container/constructor
cost, not a magical zero-cost representation. This is a medium-to-large
language surface change despite its compact proof. It leaves arbitrary
directories, separately obtained handles, and the standalone reference
witness unsolved unless combined with A/D.

## Appendix C: parameters that may overlap

Propose a signature clause `may_overlap(first, second)` after the effect row.
It means “check this body for the possibility that these arguments alias,”
not “assert that they are equal” and not “turn off EFF-5.” Appendix H gives a
whole-map clearing helper for which aliasing is harmless: clearing one map
twice remains defined. Passing the same map twice would intentionally be
accepted by this declared helper, while ordinary helpers still reject.

**Exact delta and scope:**

- GRAM-3/FN-1/FN-4 add normalized parameter-pair permissions to callable
  signatures. An actual implementation bound to a formal must admit at least
  every alias pair the formal permits. An overlap-capable actual may satisfy
  a stricter disjoint formal, subject to the other refinement checks.
- EFF-5 permits those specified caller pairs, including their contract-read
  footprint comparisons, only because the callee has been checked under the
  same alias possibilities. By-value consumption and unlisted pairs retain
  their ordinary checks. Built-in `swap` retains its defined self-swap meaning.
- OWN-7, REF-1/REF-2, EFF-2 and ENT-5 must account for writes through either
  formal alias. A write through one parameter kills a fact through the other
  if they might overlap; a destructive ancestor write can invalidate a
  descendant reference even when it is another argument.
- FN-9/MSR-3 and TYPE-11 need explicit alias-aware entry/exit and restoration
  rules. A syntactically read-only formal may change through its writable
  alias; its fields cannot be treated as entry-stable on that ground alone.
- PAR-1/PAR-2 and OWN-9 must use the same possible-overlap relation. Adjacent
  calls through the two formals cannot run concurrently merely because their
  parameter names differ. The [backend fact contract](../../../design/compiler/backend-facts.md)
  must withhold `noalias` and scoped-access promises for permitted aliases.

No transitive equivalence class may replace a set of possible alias pairs.
For general references, possible overlap includes ancestors, descendants and
partially overlapping ranges; it is not just “the two roots are equal.” A
map-only initial clause has a smaller relative-position problem, but making
an otherwise general declaration special to maps needs its own justification.

**Soundness cases:** copies, retained handles and moves confer no separation;
the body permits their aliases. Replaced fields invalidate facts/references
through every possibly overlapping formal. Boundaries propagate the alias
permission, not a caller's particular equality pattern. Conditional selection
unions and loop summaries remain conservative and can be accepted when the
helper needs no stronger fact. Recursive and function-parameter calls use
the declared permission. An ordinary disjoint helper cannot be called with
the same pair from this body without a separate proof.

**Cost:** no new runtime comparison, but more alias pairs for support kills,
validity and scheduling, and potentially less optimization within annotated
functions. A whole-function conservative analysis avoids exponential
enumeration of alias partitions. Nonannotated single-map functions keep their
existing assumptions; signature plumbing remains a checker cost. This is a
large semantic and backend audit, not a small EFF-5 edit.

C can admit both witnesses after annotating an alias-safe final helper, but
does not recover either missing fact. In particular MOVE-like code whose
correctness assumes different source and destination states still needs that
precondition. Permitting two names to alias cannot establish it. This is why
C is not the recommended remedy for the stated requirement.

## Appendix D: preserve a finite must-disjoint relation at joins

This is the better companion to A: keep REF-1's conservative unary path sets
for possible location and validity, and add a binary relation over current
selected targets. `disjoint(p,q)` in this section is a mathematical checking
judgment, not a new runtime operation. A successful judgment says every
execution reaching this point selects nonoverlapping storage for the two
holders. Ordinary OWN-7 separation, or A's distinct captured shared objects,
can establish it. Appendix I needs no new source syntax.

**Proposed rule delta:** REF-1 adds current target images and relational
transfer/meet; OWN-7 consults a proved pair for these particular images and
their contained descendants; EFF-5 and PAR-1/PAR-2 consume that same evidence;
REF-2 still independently requires validity; ENT-5 defines relation support
and invalidation; DIAG-2 retains the pair's edge proofs. ENT-2's numeric term
identity is unchanged. Proving two targets disjoint says nothing about the
numeric values selected through them.

At a join with incoming edges `e`, prove an output pair by checking
`disjoint(target_e(p), target_e(q))` separately for **every** edge, then
retain the resulting pair between the output holders. In the permutation,
one edge proves `(first,second)` and the other `(second,first)`. There is no
cross product of targets from different executions. The transfer rules are:

- Binding an alias captures the source's current target image and inherits
  its proved relations. Rebinding a holder changes its image and kills its
  old holder-based relations; other aliases to the old image remain there.
- A projection preserves disjointness below already-disjoint complete
  referents. It cannot infer that unrelated fields below possibly overlapping
  targets are disjoint. New selections still owe bounds and validity.
- A content write does not move its target, but ordinary support kills still
  remove facts about the old contents. A destructive ancestor write, move,
  release or owner exit invalidates the reference and prevents use of any
  pair containing it. No relational fact revives an invalid reference.
- Function entry gets only the disjointness justified by the ordinary EFF-5
  call contract for its compared effects. Distinct formal spellings alone
  do not establish more than that contract promises, particularly for
  read/read arguments. Function calls publish no hidden selection history.
- A conditional that aliases the pair on even one incoming edge loses the
  relation. A late, separate re-test of the same Boolean does not itself
  reconnect forgotten edge histories. Recovering such correlations requires
  a further relational rule, not an implicit path search.

For loops, use the finite set of current header holders and relevant captured
targets, not one fresh symbolic target per dynamic iteration. An unmodified
holder pair retains its relation. A pair of changing header holders must hold
on the preheader and be preserved on every continuing edge under the
simultaneous header relation. A finite must analysis starts from the candidate
pairs that hold at entry and monotonically removes pairs whose backedge
transfer cannot prove preservation, until stable; nested headers are solved
together with REF-1's settled path/validity summaries. Removed pairs are not
restored merely by another iteration. The specification must fix the candidate
vocabulary and transfer rules, not an iteration limit. Unknown-depth covers
provide no separation unless the independent pair proof preserves it.

With `r` relevant holders/captures, there are at most `r(r-1)/2` pair questions
at a point; each join checks the incoming substitutions of those pairs.
This is polynomial relation storage, not enumeration of all permutations.
Loop convergence follows finite monotone removal; proof-query and nested-loop
cost still require a prototype. Single-holder paths have an empty relation;
functions whose ordinary paths already prove all needed separation can avoid
materializing redundant pairs without changing acceptance.

D can and should be split from A: its positive and negative oracle uses two
ordinary `u64` owners, without shared objects. A can separately accept two
direct directory fields while D is absent. Firn's final acceptance must then
exercise their composition. The user reports an existing board item for
selected-target facts at joins; this study supplies a distinctness scenario
to reconcile with that item, not a duplicate or an invented item key. The
[loop relation study](../join-relations/DESIGN.md) addresses numeric relation
transport; its existence does not supply this target relation or make numeric
facts follow a selected reference.

## Appendix E: conformance, remaining gaps and acceptance

The independent oracle is the amended semantic rules above, not the current
compiler's output. These are proposed cases for `tests/conformance/`, not
changes to the supplied research witnesses or current verdicts. Each negative
case must reach its intended obligation, rather than fail on an earlier
ownership, syntax or bounds error.

| Case | A+D expectation and the observation it protects |
| --- | --- |
| Two fresh maps stored in a directory, factory return and separate consumer, one atomic block and one helper call | Accept; construction, placement, boundary and state-root proof all compose. |
| Same directory passed through a generic formal instantiated at that directory type | Accept through TYPE-11's existing instantiated direct-position rule. |
| `shared_share` aliases used for both directory fields | Reject construction's distinctness obligation. |
| Same map supplied twice through different retained handle slots | Reject an ordinary two-writer helper under EFF-5; atomic admission itself remains legal. |
| Implicit value copy of a `Shared` handle | Reject OWN-1 independently; not counted as the identity-negative case. |
| Retained copy of map A paired with fresh B; `SharedRead` alias of A paired with A's mutable handle | First accepts; second cannot prove distinctness, and a write through the read-only target still rejects. |
| Internal field replaced with retained sibling, then helper called before restoration | Reject; nominal type and readonly visibility cannot resurrect the invariant. |
| Damaged invariant restored using a fresh object before every required exit | Accept; temporary internal breakage is permitted, stale evidence is not. |
| Whole directory replacement, and directory move with later use of old references | Incoming directory invariant transports; old references reject under REF-2 where invalidated. |
| Handle slot changed after atomic entry | Already captured distinct targets remain distinct; a subsequent atomic statement uses the new identities. |
| Unannotated factory results or two unknown same-type handles | Reject absent proof; different call sites or variable names supply no freshness. |
| Unknown third handle between a proved-distinct pair | It may alias either; reject unjustified calls against it, preserve the original pair. |
| Conditional permutation of two ordinary references | Accept after D, using the normalized Appendix I program. |
| Conditional pair equal on one branch | Reject; one good branch never establishes a must relation. |
| Two separately correlated selections with no retained relational source | Reject in the initial D; make this limit explicit rather than claiming general path sensitivity. |
| Loop swaps disjoint holders; zero iterations; a backedge that instead makes both holders equal | Accept the preserved relation including zero-trip behavior; reject the aliasing backedge case. |
| Repeated allocation site across iterations, retained old alias, replacement on one backedge | No site-based equality or distinctness; only current captures and checked induction discharge the call. |
| Destructive ancestor write after a disjoint selection | Reject invalid reference use even if an old pair entry exists. |
| Disjoint selected references to integers with unrelated values | Do not infer value equality, bounds or selected-target numeric facts. |
| 16-field directory plus indexed command selection through real helper boundaries | Accept exactly the selections whose pair relation is proved; this is required evidence for the downstream claim. |

If B is chosen, additionally accept distinct proved indices and bounds, reject
equal/unproved/out-of-bounds indices and direct slot replacement, test movement
and retained element aliases across whole-family replacement, and do not assume
two family parameters are different origins. If a permutation operation is
added, test old captured handles against both new slots.

If C is chosen, the same-map case intentionally accepts only at a permitted
helper. Add a negative stale numeric-precondition case after an alias write,
a descendant-reference invalidation case, a read-only formal changed through
an alias, callable-refinement cases in both directions, and a body that calls
an ordinary disjoint helper with unproved arguments. Acceptance cases alone
cannot validate its backend: compiler/runtime checks must show the permitted
aliases carry no contradictory `noalias` promise and execute in source order
when their writes conflict.

All alternatives require proof-erasure inspection: no new runtime identity
comparison, proof storage, fallback, lock, or proof-induced scheduling edge.
Existing atomic locking and deduplication remain. A change to optional
parallel permission must preserve the sequential result; no runtime speedup
is promised by this study.

Run those checks only in CI on an implementation branch, alongside the project
gate. Start any checker-cost study with a small timed CI sample, then compare
one-map functions, fixed multi-map pairs, 16-field directories, joins and
loops; use the prescribed interleaved base/twin/change protocol before making
a performance claim. The falsifiers are unbounded dynamic-identity growth,
branch-history products, changed existing single-map verdicts, or runtime
proof work. No arbitrary proof-work cutoff is an acceptable repair.

### Gaps, disposition and what could change the recommendation

The study found these contracts needing explicit treatment:

1. **Shared object identity has no proof vocabulary.** SHARE-1 specifies fresh
   construction but SHARE-2 intentionally cannot use it. A proposes the missing
   fact and its consumers; no present compiler defect is alleged.
2. **Aggregate boundary transport is currently numeric/range based.** TYPE-11
   and FN/CALL need identity clauses, with alias-safe result publication and
   function refinement. No caller-history inference is proposed.
3. **Readonly does not preserve a relation by itself.** Module-local writes,
   owner replacement and temporary invariant breakage require current support
   tracking. This is a correction to the problem's simplifying premise.
4. **Unary reference joins erase pair correlation.** D addresses disjointness;
   selected integer facts and correlations across separately re-tested guards
   remain separate. Reconcile this scope with the existing board work.
5. **Loop freshness cannot use source-site identity.** Repeated allocations,
   moved captures and aliasing backedges need finite relational induction;
   validate the proposed finite transfer before specifying it as complete.
6. **Ordinary arrays contain distinct slots, not necessarily distinct objects.**
   B supplies injectivity by construction; quantified identity invariants and
   mutable-family permutations remain additional proposals.
7. **Overlap permission changes more than calls.** C affects invariant
   restoration, entry/exit facts, reference lifetime, scheduling, callable
   refinement and target alias promises; an EFF-5-only relaxation is unsound.
8. **Nested invariant transport is limited today.** Direct directory boundaries
   suffice for A's example. Enum payloads, nested aggregates and extracted
   container elements must not inherit unproved identity invariants. Extend
   them only with explicit obligations, if a required consumer needs them.
9. **The supplied selected-reference file has a rebinding-form mismatch.**
   Appendix I normalizes the semantic witness without modifying its supplied
   bytes; CI evidence of the normalized rejection remains outstanding.

Choose B ahead of A if inspection of the actual dynamic selectors shows a
family origin/index contract expresses the required relation while named-field
selection would need general guarded histories. Choose C independently if the
helpers are intended to support real aliasing and their alias-aware contracts
are valuable beyond clearing. Reduce confidence in A+D if a counterexample
breaks their support/loop argument, if modular publication cannot preserve
the required relation without ambient facts, or if a CI prototype shows
impractical iteration cost. The number of files touched or existing programs
to migrate does not decide the language design.

## Appendix F: allocation source forms

Each block is a complete small source-bundle program. Before blocks use the
current language; after blocks use explicitly proposed syntax and are not
offered as runnable on the current compiler. No program here was compiled.

Before A (the helper call lacks shared-root separation):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

struct Databases {
  db0: Shared<ConcurrentHashMap<u64>>;
  db1: Shared<ConcurrentHashMap<u64>>;
}

fn clear_both(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second) {
  map_clear::<u64>(map: first);
  map_clear::<u64>(map: second);
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let a = shared_map_new::<u64>(capacity: 0_u64);
  let b = shared_map_new::<u64>(capacity: 0_u64);
  let d = Databases(db0: move a, db1: move b);
  atomic a_state = &d.db0, b_state = &d.db1 {
    clear_both(first: a_state, second: b_state);
  }
  return std::process::exit_status(code: 0_u8);
}
```

After A (`distinct` is the new invariant clause; the factory and consumer
demonstrate both function boundaries):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

struct Databases {
  db0: Shared<ConcurrentHashMap<u64>>;
  db1: Shared<ConcurrentHashMap<u64>>;
  invariant separate(d): distinct(d.db0, d.db1);
}

fn make_databases() -> result: Databases pure {
  let a = shared_map_new::<u64>(capacity: 0_u64);
  let b = shared_map_new::<u64>(capacity: 0_u64);
  return Databases(db0: move a, db1: move b);
}

fn clear_both(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second) {
  map_clear::<u64>(map: first);
  map_clear::<u64>(map: second);
  return unit;
}

fn clear_directory(d: &Databases) -> result: unit reads(d) waits {
  atomic a = &d^.db0, b = &d^.db1 {
    clear_both(first: a, second: b);
  }
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let d = make_databases();
  clear_directory(d: &d);
  return std::process::exit_status(code: 0_u8);
}
```

The private fields make this a standalone bundle. A modular version publishes
`public readonly` fields and the same invariant in the interface; it owes the
same internal-write and boundary checks. Merely publishing those modifiers
without the invariant changes none of the proof.

## Appendix G: indexed-family source forms

Before B (a complete two-field selection with proved unequal database numbers
still has no shared-object separation):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

struct Databases {
  db0: Shared<ConcurrentHashMap<u64>>;
  db1: Shared<ConcurrentHashMap<u64>>;
}

fn exchange(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second) {
  swap(first: first, second: second);
  return unit;
}

fn exchange_selected(d: &Databases, i: u64, j: u64)
    -> result: unit reads(d) waits
    contract { requires i < 2_u64; requires j < 2_u64; requires i != j; } {
  let a = &d^.db0;
  let b = &d^.db0;
  if i == 1_u64 { set a = &d^.db1; }
  if j == 1_u64 { set b = &d^.db1; }
  let first = shared_share::<ConcurrentHashMap<u64>>(shared: a);
  let second = shared_share::<ConcurrentHashMap<u64>>(shared: b);
  atomic x = &first, y = &second {
    exchange(first: x, second: y);
  }
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let a = shared_map_new::<u64>(capacity: 0_u64);
  let b = shared_map_new::<u64>(capacity: 0_u64);
  let d = Databases(db0: move a, db1: move b);
  exchange_selected(d: &d, i: 0_u64, j: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
```

This before program also exposes a limit: two separately evaluated selectors
need a contract relating each selection to its index. A+D's simultaneous-pair
meet alone does not claim to solve it. The retained handles give the header
separate handle slots so the example reaches the state-overlap question;
without them, SHARE-2 can already reject the two possibly overlapping whole
handle places. These extra retains are part of this negative example, not a
proposed implementation or workaround.

After B (`SharedFamily`, its constructor and indexed handle-place selection
are proposed; numeric requirements and the helper are ordinary forms):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

fn exchange(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second) {
  swap(first: first, second: second);
  return unit;
}

fn exchange_selected(d: &SharedFamily<ConcurrentHashMap<u64>, 16>,
                     i: u64, j: u64)
    -> result: unit reads(d) waits
    contract { requires i < 16_u64; requires j < 16_u64; requires i != j; } {
  atomic x = &d^[i], y = &d^[j] {
    exchange(first: x, second: y);
  }
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let d = shared_map_family_new::<u64, 16>(capacity: 0_u64);
  exchange_selected(d: &d, i: 0_u64, j: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
```

## Appendix H: overlap-permitting source forms

Before C (two explicit handles to one object cannot enter this ordinary
two-writer helper):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

fn clear_both(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second) {
  map_clear::<u64>(map: first);
  map_clear::<u64>(map: second);
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let a = shared_map_new::<u64>(capacity: 0_u64);
  let b = shared_share::<ConcurrentHashMap<u64>>(shared: &a);
  atomic x = &a, y = &b {
    clear_both(first: x, second: y);
  }
  return std::process::exit_status(code: 0_u8);
}
```

After C (the new clause requires checking this helper under possible aliasing):

```whitefoot
alias ExitStatus = std::process::ExitStatus;

fn clear_both(first: &ConcurrentHashMap<u64>, second: &ConcurrentHashMap<u64>)
    -> result: unit writes(first), writes(second)
    may_overlap(first, second) {
  map_clear::<u64>(map: first);
  map_clear::<u64>(map: second);
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let a = shared_map_new::<u64>(capacity: 0_u64);
  let b = shared_share::<ConcurrentHashMap<u64>>(shared: &a);
  atomic x = &a, y = &b {
    clear_both(first: x, second: y);
  }
  return std::process::exit_status(code: 0_u8);
}
```

## Appendix I: correlated selection source forms

Before D, the normalized complete witness below loses the pair at the join.
**After D, the complete source is identical**: only the REF-1/OWN-7 relational
judgment changes. The helper call is then accepted because each incoming edge
supplies a disjoint ordered pair. This is not an invitation to inline or move
the helper into the branches.

```whitefoot
alias ExitStatus = std::process::ExitStatus;

fn write_pair(source: &u64, destination: &u64)
    -> result: unit writes(source), writes(destination) {
  set source^ = source^ +wrap 1_u64;
  set destination^ = destination^ +wrap 1_u64;
  return unit;
}

fn route(first: &u64, second: &u64, reversed: Bool)
    -> result: unit writes(first), writes(second) {
  let source = first;
  let destination = second;
  if reversed {
    set source = &second^;
    set destination = &first^;
  }
  write_pair(source: source, destination: destination);
  return unit;
}

fn main() -> status: ExitStatus pure {
  let first = 0_u64;
  let second = 0_u64;
  let reversed = True();
  route(first: &first, second: &second, reversed: reversed);
  return std::process::exit_status(code: 0_u8);
}
```

## Appendix J: study scope and validation record

This work changes only this research document. It changes no compiler,
specification, design node, approval log, conformance verdict or supplied
witness. Rule deltas above are proposals awaiting selection, so there is no
specification approval or before/after implementation result to record.

The study used the local active specification and design decisions, including
the compiler's checker facts, backend facts, parallel lowering and waiting
contexts, and the shared-object/state-lock/map decisions. They provide
contracts and historical rationale, not empirical proof of these proposals.
No build, test suite, compiler probe, performance measurement or CI run was
performed. No commit or push was made. The status-board MCP and session-title
tools are unavailable in this session, so the related item and project brief
could not be read or updated; the findings above are retained for handoff.

A fresh read-only reviewer, requested as `gpt-6.1-sol` at high effort, reviewed
the complete new document against its absence at the starting checkout
(`8ba7da5c6d4a6a85c3634d1f67cfdbaa2a111698`), the supplied witnesses and the
relevant contracts. Unrelated committed history between local main and that
checkout was outside scope. Checklist A4 and D2 passed; D1 and V3 were not
applicable, as were code, gate and design-tree change groups. The review used
source and git-state inspection only.

The reviewer found that the two indexed-family examples wrote explicit type
arguments on `swap`, contrary to OP-10. Both now use operand-derived types;
the primary verified the rule and the reviewer inspected the correction.
The reviewer also inspected the corrected bare const argument and the retained
handle slots that isolate the negative selection example. No concrete finding
remained within scope. The final family description additionally states the
existing handle's `drop` bound and distinguishes stored handles from their
map states. Identity/loop soundness, real selector coverage, example
compilation, cost and proof erasure remain unverified implementation work;
the review does not certify those proposed mechanisms.
