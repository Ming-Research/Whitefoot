# Container architecture reassessment

The current framework assessment is [Containers over x1](X1-LIBRARY.md), which
uses PR #70's decided language. The selection below retains its original
September 2026 semantics and measurements; its provider and region interfaces
are not requirements of that newer framework.

This is the container architecture selection from the independent workload,
critical-case, and semantic reviews of 2026-09-06, their executable experiments,
and adversarial cross-review, supplemented by the pinned external workload traces.
It selects the architectural direction and first implementation scope below; it
does not amend the active specification. The foundation review of 2026-09-07
selects the information and phase boundary described below. The first slice now
uses retained checked call roles, typed cleanup subjects, a shared internal ABI,
and fresh-destination planning before frame layout and emission. It does not
implement general alias-directed placement or new public raw-storage authority.
Its measurements are distinct from the retained pre-implementation evidence.
Keep this decision and its linked evidence current as implementation replaces the
old container paths. `DESIGN.md` points here for the superseding container choice;
its separate resource research is outside this selection.

[FOUNDATION.md](FOUNDATION.md) continues from the merged owned-place implementation
with the performance-ceiling comparison. Its earlier pool-driven implementation
selection is reopened: common containers and kernel/database/cache-server cases
must establish expressibility and cost before a broader foundation is selected.
That research does not change the merged source behavior or measured scope here.

The implementation examined was `ea97222adc0aff481df320f4c39624eb6813e488`, rebased as
`eff095c701b473a0108822a16cd1e1274c43f621` onto main
`8a5ad14b1d9093117dff1bd437d4de9d7aa564c4`. The rebase adds exactly main's intervening
tree changes: container implementation, active and archived specification bytes,
scheduler, and completion code are unchanged from the examined container revision.

## Decision

**Start with compiler-checked container states over general typed owned storage
places. Preserve value ownership semantics; stop representing every owned update
as reconstruction of a whole aggregate. Distinguish full fixed arrays, initialized
prefixes, and circular windows. Use explicit storage identity, initialization, and
range/loan relationships internally. Do not open general source-library
representation privileges in this implementation.**

The comparison concerns architecture, not the final spelling of every future
operation. Existing source spellings and semantics remain in force until their
corresponding specification amendments. The initial programs need no new public
container syntax. Their implementation now supplies evidence for reviewing the
foundation's concrete form, rather than committing to every mechanism it introduced.

### Selected control-header temporary-loan boundary

The 2026-09-07 closeout review distinguishes target capture from authority to
commit. Capturing an index before the RHS permits that RHS to change the index;
it does not permit the RHS to borrow the selected storage and then commit through
an overlapping path while that temporary loan remains live. SET-1/SET-2 already
require the complete post-RHS loan judgment. Enforcing it exposed a separate
choice in OWN-6: the statement-end rule also retained a direct match call's
temporary arguments throughout every arm. On 2026-09-07 the owner selected the
exact boundary below for implementation and specification amendment. The
implementation evidence listed at the end of this subsection remains the
completion criterion; this selection does not claim that those checks have run.

**Decision:** complete an owned enum scrutinee, then end just the temporary
argument loans created while evaluating it before entering an arm. Cover both
statement and value matches. Apply the corresponding boundary to the exact
owned Bool condition of statement and value conditionals; GRAM-4/GRAM-5 admit
call expressions there too. This is one control-header boundary, without a new
region syntax, a callee-name test, or body-dependent escape analysis.

The relevant workload obligation is sequential acquisition with typed refusal:
acquire one node or block, retain its owned success value, then acquire another
through the same provider. The recursive forms in
[`prefix_expression.wf`](../../../tests/programs/prefix_expression.wf) and
[`par_layout.wf`](../../../tests/programs/par_layout.wf) expose that obligation.
They are capability witnesses, not a distribution of real application demand.
A provider's store brand in a returned Box or Vector identifies storage; it does
not keep an exclusive borrow of the provider descriptor alive (STOR-5/PROV-1).

Three alternatives were considered:

| Boundary | Consequence | Assessment |
| --- | --- | --- |
| Whole enclosing match statement | Retains a non-escaping argument loan across later acquisitions. Binding the result first is not a general replacement: child reborrows require a region confined to one statement, and FN-9/BLK-0 route variant facts only from the direct matched call. | No additional live-reference protection has been identified for an owned, borrow-free scrutinee at arm entry. |
| Every call return | Also changes ordered RHS lists and mutation commits. It requires a separate account of borrowed results, surviving views, and their parent authority throughout expression evaluation. | Broader than the composition problem studied here; not recommended in this amendment. |
| Completed non-escaping control header | Retains ordinary statement loans and all persistent/result loans; permits provider reuse before the selected arm's work. | Recommended using the existing mode and stored-content judgments. |

The selected OWN-4/OWN-6 amendment has the following exact boundaries:

1. A successfully checked `own` enum scrutinee in `match_stmt` or `value_match`,
   or the exact `own Bool` condition in `if_stmt` or `value_if`, ends the temporary
   argument loans created by that evaluation after it completes and before any
   arm or branch begins.
2. A temporary created before that header is unaffected. The header cannot
   end a bound borrow holder's region-governed loan, a surviving view's loan,
   or the permanent suspension associated with a candidate-position borrowed result.
3. Matching through a borrow retains OWN-13, including payload-binder loans and
   parent suspension. No successful tag or branch choice narrows provenance.
4. A header-created statement child ends at this boundary. Its parent resumes
   only after its last suspending child ends. The child still must satisfy the
   existing local-region and enclosing-statement admission requirements. Writing
   the temporary's region explicitly does not turn it into a bound holder: that
   region remains its formation and type-validity ceiling, while OWN-6 fixes its
   shorter temporary-loan endpoint. OWN-4 must explicitly defer call-scoped
   temporary liveness to OWN-6 instead of stating an unconditional block-end
   endpoint; bound holders retain OWN-4's block-end rule.
5. Temporary loans in ordinary `set`, `replace`, and ordered result expressions
   still last through their statement. Target capture grants no commit exemption.
6. The boundary ends static access authority, not ownership, destructors, or
   physical storage lifetime. Overlapped execution must complete the header's
   result and argument accesses under PAR-1/PAR-3 before entering its continuation.

The non-escape argument uses existing language premises. STOR-5 recursively
excludes borrows and views from enum payloads, including substituted generic
payloads, and from its enumerated stored-content positions. A direct borrowed
result does not satisfy the owned-scrutinee judgment; a direct view is not an enum. Surviving named
holders and views keep their independent loans. With all argument accesses
finished, no usable reference derived from one of the ended temporary arguments
remains at branch entry. Removing those children can therefore resume a parent
without introducing two usable aliases. This is a local argument under the
current stored-content restriction, not a proof for future borrowed payloads;
admitting those would require revisiting this criterion in the same amendment.

The current backend joins ordinary completion work before a match terminator,
materializes the scrutinee before reading its tag, and drains a staged lane in
join, result copy, release order before the remainder runs. The relevant paths
are `emit_terminator` in `compiler/src/backend/emitter.rs` and
`emit_staged_lane_retirement` in `compiler/src/backend/emitter/parallel.rs`.
Staged permission separately retains call loans from typed argument footprints;
it does not read the lexical checker's temporary-loan stack. Task-frame backing
must still last through join return and the corresponding retirement, including
suspension or worker migration. This boundary needs no runtime API or I/O
scheduling change. The separate I/O branch's publication entry is not present in
this worktree, so this inspection does not certify its unmerged implementation.

The Rust comparison is narrower than copying temporary destruction rules. The
[Rust Reference](https://doc.rust-lang.org/reference/destructors.html#temporary-scopes)
keeps match-scrutinee temporary storage outside a dedicated temporary scope;
that concerns destruction, not whether every argument reference stays live
through every arm. Rust separately uses
[non-lexical borrow lifetimes](https://doc.rust-lang.org/stable/edition-guide/rust-2018/ownership-and-lifetimes/non-lexical-lifetimes.html).
Whitefoot's expected W1 benefit is composable typed acquisition under a fixed
local judgment; its W3 boundary remains mandatory static proof with no writer
escape. This choice claims no measured speed advantage over Rust and no general
substitute for Rust's wider borrowed-result expressiveness.

The selected delta is numbered rules +0, tokens +0, operation spellings +0,
exceptions +1: one non-escaping control-header lifetime boundary in OWN-6,
including child resumption at that boundary, with OWN-4 explicitly deferring
temporary-loan liveness to OWN-6. The selection ground is the
composition obstruction and type-based non-escape argument, not preservation of
existing compiler output. Implementation evidence must cover
owned statement/value headers, parent resumption, named and result-carrying loans,
borrowed matches, sibling argument overlap, repeated acquisition, and native
staged retirement. Existing negative mutation cases remain negative. The full
canonical gate and CI must then validate the actual amended revision.

### Empirical ground

The maintained [experiment bundle](../../experiments/container-representation/README.md)
contains all sources, native controls, a finite checker model, exact current
compiler outcomes, and the retained 168 timing samples.

These are bounded capability and cost witnesses, not a sample of production
workload prevalence. Whitefoot does not yet have a substantial real-application
corpus. The first storage implementation is justified by the witnessed ownership
and machine-work requirements; this evidence does not rank broader container
demand or establish that the selected families cover production use.

| Evidence | Observation | Selection consequence |
| --- | --- | --- |
| [Dense storage baseline](../../experiments/container-representation/dense/RESULTS.md) | With four updates, 16/256/4096 `u64` elements cost about 408 ns/176 us/43.9 ms at the examined compiler baseline, versus 65 ns/1.50 us/29.7 us in the native local-build/value-return control. Whole-payload transfers remain in that baseline's element loops. | Final storage placement and in-place element work are an immediate architecture requirement. The gap is for this workload and host, not a language-wide speed claim or a measurement of the new implementation. |
| Same-layout native controls | Value return and explicit destination are close; forcing whole-value transfer at every append is also expensive in C. A separate retained-call control uses a caller-provided `sret` destination without an aggregate copy. | Keep value semantics, but make storage reuse/result destinations an explicit lowering responsibility. Do not hope an optimizer repairs every aggregate chain. |
| [Lifecycle source](../../experiments/container-representation/lifecycle/RESULTS.md) | Five complete programs execute. Pool count contracts, direct result contracts, and several local field/variant paths work. Ten probes identify precise limitations, invalid programs, or two compiler defects. | Preserve working contracts and fix the exact missing relationships. A broad claim that all container facts are lost is false. |
| Static-block alternative | A normal struct containing `FixedVector<u8,4>` retains capacity through checkout, use, and return. The neighboring initialized-length claim still fails. | Put persistent fixed extent and full initialization in the relevant type/state. This pool does not yet justify general quantified element refinements. |
| Linear cleanup | Individual linear values are discharged on success and failure, but an emptied run remains unconsumable. | Add checked empty-owner discharge without weakening element or outer nominal obligations. |
| [Finite range model](../../experiments/container-representation/authority/RESULTS.md) | All 510 live sets at capacities 1..8 can be accounted for by checked interval tokens; 184 fit one circular window. Two-span loans and four-slot sparse reuse pass independent occupancy/conservation checks. | One window is not a universal live-set representation. Finite ranges are a useful internal basis; this is not evidence that a public symbolic representation-proof system is ready. |

The dense comparison keeps the same data and descriptor layout in the controls.
It does **not** attribute the measured gap to ring metadata. The reason to
distinguish full arrays from prefix/ring states is their different initialization
guarantees and representation needs, also exposed by the static-block negative.

The [external sample](EXTERNAL-WORKLOADS.md) adds six source-level traces from
ripgrep 14.1.1, DuckDB v1.2.0, and Kubernetes v1.32.0 at pinned commits. It recovers
application contracts and candidate needs across Rust, C++, and Go; it includes no
upstream timing, allocation profile, or prevalence measurement. Its consequences
and remaining causal uncertainties are recorded under workload evidence below.

### Storage and lowering boundary

- An owned value has a typed storage place, an initialization/ownership state, and
  a lifetime. Fields, indices, and cell dereferences are projections of that place.
  Semantic resolution and lowering must walk type structure generally, rather
  than recognizing a run of one particular element shape or rebuilding each
  enclosing aggregate after an element update.
- Callee results may be constructed in a caller-provided destination. A consumed
  owner's place may be reused when the checked lifetime and alias relationships
  permit it. This is a storage decision, not a second source mutation API. Do not
  mark two potentially identical input/output pointers as independent `noalias`
  destinations or duplicate their ownership to obtain this representation.
- A known place is not proof of target uniqueness or initialized payload. In the
  external grouping trace, different input rows can map to one aggregate state;
  a slot reserved during construction is not yet a usable row reference. Keep
  those relations separate from address formation and result placement.
- Track which fields/elements own initialized values throughout construction and
  replacement, including failure and variant returns. A location alone is not
  enough information to generate correct cleanup. Scalar/Copy values may remain
  ordinary SSA values; owned aggregate representation must support real places.
- Preserve evaluation, allocation, and release order. In the current flat source
  form, `let value = construct(...);` followed by
  `heap_box(store: &uniq heap, value: move value)` may not become allocate-first
  merely to avoid a copy: refusal could skip observable construction effects.
  A program needing
  allocation-first construction must acquire backing first in its source, then
  construct and seal it. Typed failure returns the actual initialized prefix,
  unconsumed inputs, and remaining resource responsibilities.
- Moving a descriptor does not move or prolong its backing. Moving inline payload
  can change its address. Form loans at a location whose lifetime covers their
  use, and forbid relocating/reusing that payload while the loan remains live.
  Staged storage additionally lasts through join return, result consumption, and
  its retirement. This requirement belongs in storage identity/lifetime handling
  now, even though staged-window optimization is not the first slice.

### Foundation review: authority, representation, and placement

When construction, replacement, borrowing, helper transfer, cleanup, or deferred
execution exposes a problem, first identify the missing state or transition and
its owning layer. Repeated adapters are evidence to examine that boundary; they
are not proof that the language needs another operation. Conversely, an isolated
incorrect use of a sufficient representation is an implementation bug, not a
reason to redesign the whole compiler.

| Layer | Responsibility | Boundary |
| --- | --- | --- |
| Source specification and semantic checker | Admit operations using ownership, initialized domains, loan origins, effects, and verified callable contracts. | A backend destination grants no new source authority. Missing source permissions require specified rules and their checker, not an emitter exception. |
| Typed lowering and normalized storage/call representation | Preserve value identity separately from storage identity, typed projections, construction/commit order, ownership transfer, cleanup, and address-use lifetimes. | Lowering may choose a physical construction destination for an already admitted value. It must preserve every observable source behavior and responsibility. |
| Target storage, ABI, and optimization | Select layouts and frame objects, implement calls, and reuse storage when contents and derived addresses no longer need it. | This is compiler correctness analysis, not another source acceptance path. STOR-6 checks target materializations before optional optimization; optimizer facts cannot supply missing source proof. |

The current [storage planner](../../../compiler/src/backend/storage.rs) analyzes
immutable IR contents and their CFG interference. It does not infer source
ownership: a failed coalescing opportunity keeps separate backing. Loads remain
snapshots, exposed backing is protected, and coalescing is disabled for functions
with overlap or completion pipelines. These are legitimate conservative compiler
choices. That conservatism does not yet model the complete lifetime of an address
carried across asynchronous execution.

The earlier phase-boundary gap was visible at `bb8eb30f`. An
[`IrOperation::Call`](../../../compiler/src/lowering.rs) names value arguments;
the aggregate result-pointer ABI is introduced later by the
[emitter bridge](../../../compiler/src/backend/emitter/places.rs).
[`promote_binding_if_needed`](../../../compiler/src/lowering/builder/storage.rs)
could then introduce separate addressable owner storage. That dense measurement
retained a whole result-to-binding copy, and ordinary, system, and parallel emission
routes have each needed to honor that late bridge. The system-operand correction
at `bb8eb30f` and its parallel failure were concrete evidence that the shared
representation deserves review; they do not establish that every ABI conversion
is avoidable or unsound.

The selected implementation direction is **one typed storage/call
normalization before emission**, extending the existing IR rather than building a
second complete IR or a new source checker. Scalar SSA, CFG structure, typed place
projections, checked cleanup, and permission-derived scheduling remain useful.
Explicit aggregate destinations and a shared call representation must be consumed
by ordinary, system, and parallel paths. Runtime-specific marshalling still exists,
but must not independently reconstruct whether an operand is a value, a borrowed
place, or an aggregate destination. The current implementation shares
`FunctionAbi` across definitions and call routes and computes fresh destinations
in `FunctionStoragePlan` before physical frame layout. Emitter consumers resolve
that plan; they do not independently infer source permissions. The following
boundary also constrains extensions beyond its conservative placement cases.

#### Information retained at the checked boundary

Code inspection at `bb8eb30f` identifies information that an erased-IR pass
cannot recover by value type. The checked binding occurrence carries
`consume_root`, but the binding arm in
[`IrBuilder::expression`](../../../compiler/src/lowering/builder.rs) does not
retain it. Formal modes pass through `lower_borrow_mode_type`: a borrow of direct
content becomes an address, while a descriptor or opaque handle can keep exactly
the same IR type in own and borrow modes. User-call lowering also does not retain
its checked result-provenance relation. This is compatible with the current
conservative backend; it is insufficient authority for a new ownership-directed
reuse pass. A false consume flag alone does not establish a copy operation.

The first implementation step now retains source parameter and result modes in
`IrSourceSignature`, independently of the representation types. It is populated
from the checked function for source declarations and monomorphized instances;
compiler-synthesized functions carry no invented source signature. Its mode record
carries no loan origin/lifetime and grants no input/result aliasing permission by
itself.

User calls now also retain a per-occurrence `IrSourceCall`: direct binding and
field-projection consumption, formed borrows/reborrows, place reads, and other
computed values. The same descriptor value can be borrowed and then consumed;
recording its role on the value alone would lose that distinction. Moving a
unique holder consumes that holder, not its referent. The callee's retained formal
mode supplies this distinction without duplicating its signature at every call.
A place read does not imply enclosing-owner consumption or permission to copy an
affine payload.

For a direct borrow result, the checker retains the sole candidate argument's
ordinal. Lowering relates the result to that existing actual operand and its
address/projection graph. It does not reconstruct a source path or evaluate an
index again. The candidate may cover a wider place than the returned suffix, so
this relation does not grant exact disjointness. An absent candidate is not proof
of fresh backing or of no outstanding loans: owned views have separate origin
sets, not retained by this record. Standalone uses, constructor/kernel transfers,
view-result origins, and general activation lifetimes remain unfinished. The
bounded lane driver now retains issue-local values and addressed owners until its
exact drain; this does not supply a general destination normalizer. These metadata
steps themselves do not normalize destinations or change the emitted ABI.

| Information | Origin and use |
| --- | --- |
| Operand use and formal/result role | Preserve the checked distinction between copy/read, ownership transfer, and shared/exclusive access. Retain or consume it in an explicit lowering operation before erasure. Do not infer it from pointer/descriptor shape. |
| Place identity and projection | Keep a typed root plus field/index/dereference path. A borrowed root refers to existing caller storage; making it addressable must not allocate another owner. |
| Borrow origin and storage-use boundary | Preserve the resolved source origin and result relationship needed for storage placement. Schedule lowering supplies the actual issue, join, consumption, and retirement uses; a source region name alone is not an activation storage plan. |
| Commit and cleanup responsibility | Carry the checker's target/RHS/commit order and ordered release records to their current value or place. Reusing a physical slot neither duplicates nor combines these responsibilities. |
| Physical content and layout | Normalization and target planning can derive representation, sizes, alignment, content interference, and legal slot sharing from these explicit operations. They cannot grant source permissions. |

Source ownership is therefore decided once. The implementation may validate that
its generated IR preserves these records and uses well-typed destinations; such a
failure is a compiler defect, not a source rejection or a runtime fallback.

#### Minimal operation distinction

The following is schematic internal notation, not proposed Whitefoot syntax or
committed Rust API names:

```text
reserve p : T                    // physical backing; no initialized T owner yet
initialize p from value          // a fresh logical destination receives a value
call f(arguments) into p         // complete result becomes available at delivery
snapshot p -> v                  // immutable content needed independently of p
project p.path -> q              // same backing; neither read nor ownership transfer
commit q <- replacement -> old   // existing-place exchange, after RHS completion
release subject, checked_action  // the checked owner/value/place on its exact edge
```

An initialized `T` here means a valid value of its type, not initialized bytes
throughout its full capacity: an empty run is a complete value. Logical ownership
and immutable content snapshots are separate from physical backing. In particular,
an internal snapshot of affine representation does not grant a second owning
responsibility or permit a new source copy. The exact enum/tag and padding rules
remain the target representation's, not inferred from this notation.

At `f5dab70c`, the dense witness's logical chain is `v2 = Call build(seed)`,
`v3 = AddressOf(v2)`, projected updates through `v3`, then a whole-owner `Load`
feeding cleanup. The retained raw LLVM has three aggregate fields: call result,
addressable owner, and cleanup snapshot. The selected shape instead reserves the
binding's place, calls `build` into that fresh place, updates its projections, and
applies the checked cleanup to that place. A content snapshot remains where a
later read actually needs the old content. This describes the intended removal of
the recorded copy and extra fields; it is not a new measurement or a claim that
current target code already has that shape.

The cleanup step now carries a saved value or typed place as the checked release
subject. Scope exits and whole-binding disposal project existing addressed owners;
they do not create another whole-owner value. Straight-line releases form one
ordered group, just as edge releases already do. Emission captures all content
needed by that group before its first release and skips loads for no-op owner
nodes. Descriptor fields are ordinary typed places; this does not change the
source borrow ABI of a descriptor or grant descriptor replacement through a borrow.

Direct cleanup of a place needs its own ordering argument. Existing jump delivery
snapshots incoming values before cleanup because a reused destination may overwrite
an owner that cleanup still reads. Replacing every cleanup load with an arbitrary
late address read would lose that protection. The normalization must preserve the
subject's contents until the checked release executes, or retain a real snapshot.
The grouped capture implements that ordering. Proper-part consumes still evaluate
their selected value and residual releases, and replacement/swap/phi snapshots
remain. Fresh-result planning now joins the last two physical destinations of the
dense chain without removing those required snapshots.

#### Calls and dynamic storage instances

A call's result destination is fresh with respect to live inputs and borrowed
storage by default. Consuming an argument ends the caller's owning use, not the
callee's reads of it. Input/result aliasing therefore needs explicit read/write
ordering evidence; a move annotation alone cannot permit it. The existing
aggregate swap and `rewrite` witnesses retain both old field values before their
writes. Existing-place replacement reads the old owner after the RHS, but only
when the target remains writable under the complete post-RHS loan state. An
RHS temporary borrow of the target prevents that commit; the ordering rule does
not authorize using storage while it is still borrowed.

Each ordinary invocation has its own storage instance. A staged static definition
can have several dynamic instances in flight: the place must be associated with
the existing activation/window slot, not only its static value ID. These are
compiler relationships, not a request for runtime owner IDs or generation tags.
The existing generated slot/drain CFG supplies the reuse boundary.

The bounded lane implementation now carries an addressed binding's pointer, not
a snapshot loaded while its callee may still be writing. Every issue-stage
address definition receives one typed backing element per pipeline slot, planned
and target-checked with the rest of the caller frame. The drain reloads that
address and reads the callee's completed updates; a slot is reused only after its
remainder and checked releases. Ordinary invocation places keep their ordinary
backing. This closes the former conservative fallback for issue-local addressed
owners and remainder reads without changing the source permission judgment or
adding a runtime storage protocol.

| Execution route | Storage obligation |
| --- | --- |
| Direct call | Inputs live through the call's reads; the fresh result is available on return. Its caller-owned backing survives subsequent reads and loans. |
| Ordinary refused handout | No task frame receives the arguments. The deferred direct invocation still needs its argument storage at join, even though the source call occurrence is earlier. |
| Granted handout | The admitted frame holds the existing argument/result layout. Borrowed pointers retain their external backing. Join returns before result delivery, and delivery reads the result before frame release. |
| Staged iteration | Issue-side carry, result, and any borrowed backing belong to the same dynamic slot through that iteration's drain and retirement. A later issue cannot reuse it merely because the worker marked DONE. |

Fresh destination selection requires one initializing value in its storage group,
one use by the new addressable binding, and definition before that binding in
the same block. Implicit staged carries count as uses. Static construction is
limited to acyclic blocks: a single static use does not exclude an address from a
previous dynamic iteration. Repeated blocks are eligible only when the actual
selected pipeline supplies per-slot backing and drains before reuse. The emitter
resolves that backing with the current block's slot and still publishes the
address at its original definition. Failing these conditions retains separate
storage; it neither rejects source nor asserts that a more general placement is
impossible. Construction writes a complete admitted value; it grants no access
to uninitialized source storage and moves no observable allocation or release.

Final materialization accounts for the actualized schedule before STOR-6 target
layout checks. Every route uses the same operand/result roles; adapters implement
their existing transport and timing. No route gains permission by changing frame
capacity, replacing inline payload with pointers, or adding a scheduling edge.

#### Scope selected by the comparison

| Candidate | Decision and current status |
| --- | --- |
| Extend only the emitter's value-to-slot map | Useful for current conservative content coalescing, but not the selected foundation: it lacks checked use distinctions and leaves destination/cleanup conventions to multiple emission routes. |
| Preserve checked uses and extend the existing typed IR, then normalize storage/calls once | Selected direction. Shared ABI roles, typed cleanup subjects, and fresh-destination planning address the first slice while reusing the CFG and explicit releases. General alias-directed reuse still requires additional origin and activation-lifetime evidence. |
| Introduce a second complete storage IR | Not selected for this slice: the needed distinctions can extend the existing typed CFG. No current witness requires replacing scalar operations and control-flow machinery. |
| Expose a public partial-object construction protocol first | Not a prerequisite for these existing value-return programs. It answers a separate source-authority question and needs its own workload and checked transition evidence. |

Implementation first preserves checked information, then applies it to fresh
destinations and cleanup. A normalized form must not reach a consumer that has
not been updated to understand it. Shared consumption by ordinary and parallel
paths remains part of completion; an ordinary-only subset is insufficient.

These are requirements on the representation, not a mandate for a runtime
per-field bitmap, a new allocation, a universal storage wrapper, or a second
general theorem prover. LIV-1 already makes scope-exit release unconditional on
each checked edge; `CheckedDrop` and `IrDrop` retain the ordered responsibilities.
In the failure witness, the first successful acquisition creates one complete
cell; second-acquisition refusal releases that cell, with no pair owner yet.
Success transfers the two cell owners into the complete pair and then the result.
Even if placement puts a cell in a future pair field early, it remains that cell's
responsibility until the checked construction transfers it. A universal source
partial-pair/seal state machine is not needed for this program. Container window
state, genuinely dynamic occupancy, and source-level linear discharge remain their
own semantic responsibilities.

Current source authority is narrower than arbitrary construction into a chosen
raw slot. A constructor builds a complete value; `fixed_vector()` builds a valid
empty run; `place_back` consumes a completed element and proved room. `&uniq T`
authorizes access to an already valid `T`, while `writes(dst)` is a write footprint,
not a guarantee of initialization on every exit. A `MutSlice` covers initialized
length, not raw capacity. Borrowing an empty run's first slot fails OP-4, and BLK-4
also excludes source `&uniq` run parameters through nested fields and generics.
Thus a source-written raw-slot helper cannot currently be obtained by changing a
parameter spelling. Whether a real workload needs that additional authority is
a separate source-design question under D17; fresh value-result placement does
not require it.

Use existing complete witnesses to judge the candidate, with their actual limits:

| Witness | Required distinction and current evidence |
| --- | --- |
| [Dense scalar, wide record, and inline view](../../experiments/container-representation/dense/RESULTS.md) | The retained matrix executes, element loops no longer copy whole payloads, and the fresh-destination checkpoint removes the one-time result-to-binding transfer. These are cost/capability probes, not production prevalence data. |
| [Owned-place execution tests](../../../compiler/src/backend/tests/owned_places.rs): `replace_captures_the_target_before_rhs_changes_its_index` and `replaced_aggregate_snapshots_survive_writes_and_helper_returns` | Normal and retained helper calls preserve the captured target when the RHS changes a separate index, and the old aggregate remains a snapshot after later target writes. The earlier same-target RHS mutation witness was erroneously accepted: its temporary loan conflicts with the subsequent commit under OWN-5/OWN-6. That source now supplies a semantic rejection case, not ordering evidence. |
| Same suite: `partial_construction_refusal_preserves_effects_values_and_release_order` | Passed with retained calls and refusal at each of three allocations, observing exact allocation/release order and returned values. The source constructs two complete cells before making the pair; this does **not** demonstrate source-visible partially initialized struct authority. |
| Same suite: `returned_element_borrows_and_inline_views_reach_the_owners_storage`; [semantic neighbors](../../../compiler/src/semantic/tests/owned_places.rs) | Passed owner-storage execution and bounds/loan checks, including refusal of raw-slot access and moving a borrowed owner. Root value liveness alone cannot replace loan/storage identity. |
| [Linear lifecycle programs](../../experiments/container-representation/lifecycle/RESULTS.md) | Actual linear values are discharged on success and failure; the leak neighbor is rejected. A proved-empty run of linear values still cannot be discharged. That missing language capability is not solved by an aggregate ABI. |
| [Parallel execution tests](../../../compiler/src/backend/tests/parallel.rs) and [generic nominals](../../../tests/programs/generic_nominals.wf) | At `bb8eb30f`, generic nominals exits abnormally with four workers. The corrected aggregate adapters now pass three focused native cases: ordinary aggregate results, staged aggregate results with cleanup, and staged inline storage mutated through a helper. Each compares sequential execution, forced refusal, actual workers, and deferred execution until join; the staged controls require multiple simultaneously held frames. The complete corpus and gate have separate validation below. |

The current placement is exercised by these same witnesses; their source results
and effects remain constraints on future changes. The staged inline-place case
also requires construction directly in the per-slot backing before observing
deferred mutation and retirement. The frozen external contracts below guide the
next semantic comparisons; they do not expand this slice into a public raw-storage
framework or six application ports.

### Selected initialization states and public authority

The selected direction distinguishes these semantic state families. The current
specification still has the two window runs of BLK-1 and transitional legacy
arrays; this table is not a claim that three new source families already exist:

| State | Inherent guarantee | Representation requirement |
| --- | --- | --- |
| Full fixed array | Exactly N initialized `T` values; length/extent are properties of the type | Dense payload; no universal variable-length or ring-head state |
| Prefix run | Exactly `[0,len)` is initialized within its capacity | Dense backing plus the needed runtime length/capacity state |
| Circular run | One logical sequence maps to one or two physical initialized intervals | Head/order information only for a workload that needs it |

Backing placement is a separate axis: inline, a provider allocation, or an owning
cell/store. This is not a demand for a runtime tagged union of every representation
or a new wrapper/indirection on every array. A fully initialized fixed block held
at a stable allocation should retain its full-state type when the owning reference
moves through a free list. At the examined baseline, the boxed source probes
were blocked by incorrect region inference and owned-cell measure resolution;
those compiler defects do not refute that representation.
An empty-backing pool has an additional contract: failed partial construction must
return the same empty storage. The next [foundation selection](FOUNDATION.md#empty-backing-across-library-and-pool-boundaries)
addresses that state separately; a full-array type alone does not express it.

The proposed full-state construction proceeds through a checked partial state.
This is a follow-on source capability, distinct from constructing an already
admitted complete value in its physical result destination. Sealing into a full fixed
array requires full initialization, matching extent, and the correct contiguous
layout; it transfers ownership rather than treating raw slots as values. For an
already acquired provider allocation, a compatible seal may retain that allocation
and its identity. It must account for exact backing extent/alignment and release
authority, not silently substitute a second allocation. Dynamic full extents and
arbitrary sparse source containers are later selection questions, not additional
metadata required by the fixed-array implementation.

These families are not an exhaustive account of application state. DuckDB's
nullable list producer writes payload only for valid cells while its logical
extent includes NULL positions; its dictionary selection is a separate mapping.
An `Option<T>` element representation is a candidate, while a separate bitmap and
payload layout requires its own checked initialization relation. Neither the
need for that layout nor its cost advantage is established by source occurrence.

Internal range/loan relationships must be explicit enough to justify disjoint
views and initialized accesses, using finite specified checks. This does not mean
copying the concrete model's per-slot tokens into the compiler or runtime, or
unrolling every source loop. Native state transitions can establish the required
symbolic range relations from the existing fixed proof families.

General source-library representation authority (the B direction) is **deferred,
not disproved**. The model does not verify symbolic bodies, arbitrary abstract
invariants, multi-store provenance, typed payload identity, or proof erasure.
Opening such privileges needs that complete checked path on a real library and
workload. The current fixed-array/pool experiment can advance without it. D17's
long-term commitment remains intact.

### Callable and resource contracts

- Preserve existing verified pool count and direct measured-result contracts.
  Inherent type/state facts survive storage and helper transfer. Incidental facts
  about a particular former element do not become facts about whichever element
  a later operation returns.
- Extend result relations over finite typed field projections, with the same
  entry snapshots, result publication point, and write invalidation as direct
  results. A wrapper must be able to state a true relationship about its returned
  field. Admit only relations proved for every relevant return; do not replace
  missing premises with a runtime guard or a new trusted assertion.
- Full-state types solve the fixed block's persistent initialization requirement;
  they do not solve arbitrary dynamic capacities, graph membership, or
  cross-collection invariants. Those remain valid future demand sources rather
  than being excluded because this pool has a simpler representation.
- Consume a kernel run proved empty without inventing any element release. The
  existing `dispose` surface can carry this rule in its amendment: a linear-element
  run needs proved zero length, every required provider, and ended loans. It does
  not permit discarding live elements or an independently declared `linear`
  wrapper merely because one field is empty. Scope-relative provider obligations
  continue to apply even when no element is live.
- Two-span views require more than two descriptors. Each result must retain its
  owning allocation, physical interval, access mode, and lifetime through helper
  transfer. One checked split establishes coverage/order and disjointness; ending
  one loan does not end its sibling. Lift the blanket same-region/result-type
  restriction only with these explicit result-origin relationships.
- Do not adopt B6's proposed requirement for a nontrivial relation on every
  representation measure. Check the promised abstraction and operation domains;
  an otherwise correct front removal must not be rejected for hiding its head
  coordinate. Do not weaken a written promise to compensate.

### Rejected defaults and reconsideration conditions

| Alternative | Disposition and ground |
| --- | --- |
| Keep aggregate reconstruction and rely on LLVM | Rejected for dense owner operations by retained copies, stack growth, and the measured scaling gap. |
| One ring representation for every sequence | Rejected as the universal semantic/representation requirement. Full initialization is a different persistent guarantee; sparse states are not all single windows. No claim that removing head alone repairs the measured performance. |
| Make every container a stable handle/store entry | Rejected as a default: the dense workload needs no identity lookup. Preserve a separate stable-allocation route for actual retained-address requirements. |
| Open generalized library representation proofs first | Deferred: no symbolic implementation/contract/erasure evidence yet, and the immediate programs have a smaller path. Revisit when a real library cannot meet its frozen contract with the selected states, or a checked alternative supplies better complete evidence. |
| Add general element refinements to recover fixed capacity | Not selected for this witness: the executable static-block alternative already retains capacity. Revisit for a real dynamic or relational requirement that inherent state and callable contracts cannot express. |
| Replace lost facts with checks, tags, or another backing | Not an equivalent solution when it violates that workload's behavior, resource, or cost contract. Necessary dynamic algorithm state remains legitimate. |

## First implementation scope and completion evidence

Begin with **general owned-place and result-destination support**, including the
semantic projection/borrow support needed by the existing probes. This includes
semantic work: the wide-record and inline-view programs stopped before the
relevant backend path at the examined baseline. Preserve source semantics during
this first slice.

Its concrete acceptance criteria are:

1. The frozen dense construction/update programs execute correctly for the full
   runtime-seed/round matrix. Element loops perform element-sized work, not a
   whole-payload transfer per operation. Construction and updates have linear
   work and bounded backing; repeat the same recorded measurements and attribute
   remaining differences against comparable native code.
2. The existing wide-record program and inline exclusive-view probe **compile and
   execute correctly**. Merely retaining today's known unsupported diagnostic does
   not satisfy this criterion. Field/index/cell paths use the general mechanism.
3. Owned results cross real retained helper calls without mandatory intermediate
   aggregate copies where the lifetime/alias conditions permit destination reuse.
   Partial-construction and failure cleanup preserve every responsibility and the
   source's effect order. Negative ownership, borrow, and raw-read cases remain
   rejected for their rules.
4. Storage lifetime is represented independently of descriptor/binding lifetime,
   so subsequent staged-window integration can preserve the supplied retirement
   boundary without replacing this foundation. No new I/O/runtime protocol is
   introduced, and private runtime slot layouts are not assumed.

The implementation executes the frozen scalar correctness matrix
and the mandatory wide-record and inline exclusive-view programs. Its
[owned-place execution tests](../../../compiler/src/backend/tests/owned_places.rs)
observe value snapshots, simultaneous assignment, returned element borrows,
retained helper calls, allocation refusal, cleanup order, and mutation evaluation
order. In particular, SET-1 captures target components before the RHS, while
SET-2 reads the displaced owner after the RHS only if post-RHS liveness and
loan checks still admit the commit. A phi edge snapshots its inputs,
performs predecessor cleanup, then writes the destination: liveness may allow the
destination to reuse storage that cleanup still needs before that point.

The old borrowed-buffer descriptor path exposed an existing unsafe capability:
replacing a descriptor passed by value can free backing without updating its
caller. Replacing an owning borrowed wrapper can likewise retire backing held by
an earlier prepared target. These forms now stop explicitly as
`BorrowedBufferDescriptorMutation`, after ordinary source judgments; no normative
verdict was changed to call them invalid. Borrowed element writes and direct
owned replacements remain supported. Current BLK-4 excludes mutating a new run
through a source helper, including nested run owners. Any later lifting of that
restriction must prove captured target storage identity and lifetime, not merely
that the root binding is live after the call.

The [parallel aggregate adapters](../../../compiler/src/backend/emitter/parallel.rs)
now use one typed [internal ABI plan](../../../compiler/src/backend/abi.rs), shared
with function definitions, on the ordinary, refused, staged, thunk and split-call
paths. Parameters distinguish values from aggregate content pointers; results
distinguish values from destination passing. Source modes do not grant aliasing
permission, and qualified system wrappers retain their separate value ABI and
proof-only argument erasure. Granted frames retain their complete inline argument
and result layout. A joined aggregate is copied into caller backing before frame
release, and staged carries/results are materialized into their declared caller
destinations. These adapters complete the current representation's delivery paths.
Fresh-destination planning removes the late result-to-owner transfer when its
use and lifetime conditions hold. Granted task-frame results still transfer into
caller backing before frame release. The split call retains a value load/save
bridge, but current production split results are Unit or scalar reductions;
aggregate split results are not admitted by its builder. The per-iteration backing
above addresses a lifetime requirement that copying a descriptor could not satisfy.

The [dense measurements and generated-code analysis](../../experiments/container-representation/dense/RESULTS.md)
compare the pre-implementation baseline, first owned-storage checkpoint
`f5dab70c`, shared-ABI/place-cleanup checkpoint `d5c0bb86`, and fresh-destination
implementation under the same scalar kernels and procedure. The latest 168-sample
run has four-pass medians of 109.07 ns/1.50 us/24.5 us at 16/256/4096 elements.
The scalar raw frame now contains one aggregate, and the constructor writes
directly into its addressable binding. At N=4096 the static entry frame is
32,848 bytes, compared with 93,040 at `d5c0bb86`, 125,872 at `f5dab70c`, and 32,832
in the current native control. The remaining zeroing, window-address computation
and target optimization choices are described with the generated code. These
dated local runs do not establish a statistically significant timing change or a
language performance ranking, and do not retroactively alter earlier results.

Then implement the selected bounded semantic capabilities: full-state construction
and sealing, checked empty-run consume, projected result contracts, and two-span
result provenance. These may amend the specification with their derived evidence;
they do not require a general library-proof framework. Broad source migration and
legacy retirement follow only when the replacement capabilities run the relevant
programs, including fixed blocks on the stable-storage route. Do not retire the
full-array guarantee merely because a variable run accepts a constant literal.

This first slice preserves the existing typed IR and source judgments, centralizes
ABI roles, and selects conservative destinations before physical frame planning.
It does not need complete origin sets or input/result aliasing authority for its
fresh-storage cases. General alias-directed reuse, sparse storage,
dynamic refinements, destruction, and device extensions remain bounded follow-on
questions. The measured implementation does not establish that proposed source
rules or general checked-library representations have passed a compiler.

## Selection ground

[Project instructions](../../../AGENTS.md), the
[constitution](../../../docs/constitution.md), the
[active specification](../../../spec/kernel-spec.md), and current
[decision practice](https://github.com/mbbill/Whitefoot/blob/efe40194a05b339c46b931e87bd05a40036f23cf/docs/practice.md#decision-work) supply
the ground. Historical design selections and compiler convenience do not define a
new architecture's requirements. Current language behavior remains the active
specification's, including its proof rules; historical claim/trap language does not
authorize a writer-accessible escape.

- Performance is the reason to exist. Evaluate ordinary accepted source against an
  equivalent reference implementation, including its layout, algorithm, resource
  contract, and generated machine code. A smaller operation table is not evidence
  of a better design. Name the advantage over Rust in performance, checked safety
  without writer-accessible unsafe, or enforced fast source forms.
- Safety and proof authority cannot be traded away. Memory validity, exclusivity,
  initialization, ownership, and partial-operation domains must be established by
  the specified deterministic checker. No proof-search timeout, work budget, or
  runtime fallback selects acceptance.
- The default source form must reliably expose the appropriate fast representation.
  An expert-only rewrite does not establish that ordinary accepted forms are good.
  Necessary dynamic program state is allowed; redundant safety scaffolding is not
  justified merely because it makes a source program compile.
- An `existence-only` decision separates a derived need from a minimality-selected
  form. Read its rationale and reconsideration condition. New workloads can
  disprove the adequacy of the form without disproving the need.
- D17 commits to checked representation authority as a long-term lane. It does not
  require implementing a universal storage proof framework before the next useful
  program. Such machinery earns priority by resolving an immediate experiment.

Frequency determines optimization and investigation priority, not whether a needed
capability may exist. A rare workload can veto a representation for its own hard
contract; it does not thereby get to impose its metadata on every dense array.
Do not average away a hard-contract failure with wins on common cases. Conversely,
do not turn contiguous layout, stable addresses, no allocation, or no runtime tags
into universal requirements without a workload-specific derivation.

## Independent obligations

| Obligation | What must stay distinct |
| --- | --- |
| Store provenance | A store brand identifies the provider; it does not identify an allocation or a recycled object generation. |
| Object identity | Identity, address stability, and current ownership are different properties. |
| Ownership | Every live affine or linear element has exactly one owning responsibility, including failure and early-return paths. |
| Loans | Authorized ranges remain valid through their promised use; copying a descriptor does not extend backing lifetime. |
| Logical shape | Length, order, continuity, keys, and occupancy need not describe the same physical representation. |
| Initialization | Only constructed values may be read or destroyed; temporary holes need checked authority. |
| Abstraction | Useful relations must survive helpers, fields, variants, and elements through verified contracts, with sound invalidation. |
| Resources | Allocation failure, peak storage, backing reuse, and destruction are part of the complete operation. |

Runtime occupancy, a ring's head, and a genuine failure outcome can carry necessary
information. Rechecking a property already guaranteed by a returned capability or
verified contract is a different issue. Proof erasure must not discard semantic
facts needed to select efficient lowering.

## Workload evidence and coverage gaps

These are candidate workload classes, not measured prevalence estimates. Existing
Whitefoot tests and small programs are capability probes; their distribution
reflects what the compiler supports and what its authors chose to test. They cannot
establish production demand, including the relative importance of byte storage,
dense construction, or other classes below. The compiler's own Rust implementation
is one additional demand source, not a representative corpus or a reason to start
full self-hosting.

When prevalence or application-scale composition is needed to select priorities,
use a bounded, explicitly sampled corpus of established C++, Rust, and Go
applications across relevant domains. Trace actual container use through helper
boundaries and complete lifetimes, retaining the source revision and workload
context. Separate static use-site counts from execution frequency, element-size
and collection-size distributions, allocation behavior, and live-storage costs;
the latter require representative runtime measurements. Language/library defaults
and corpus selection can bias the observed forms, so infer requirements from the
program's behavior and resource contract rather than copying its container API.
Report conclusions within the sampled corpus instead of claiming industry-wide
percentages. The external source sample below is now available; no external
distribution study or upstream runtime measurement has been performed here.

For each consequential representation, ask the counterfactual question: which
constraints remain if the source language's ownership, borrowing, proof, layout,
or library limitations are removed? Recover the required behavior, ordering,
identity/address lifetime, failure behavior, and resource/cost contract before
classifying the observed container. Distinguish evidence for an algorithmic
necessity or measured performance choice from language/library workarounds,
compatibility constraints, and historical accidents. Causes can be mixed or
unknown; occurrence counts alone do not establish them. Comments, design records,
profiles, and relevant alternatives help test the interpretation.

For example, an observed handle table does not by itself establish a requirement
for handle lookup: the required property might be stable identity, stable addresses,
or simply a way to express otherwise awkward references. Conversely, replacing
that table with dense ownership is invalid if the application really needs its
identity and lifetime behavior. A Whitefoot alternative must preserve the recovered
contract and demonstrate its claimed representation or proof advantage. Label it
as a hypothesis until tested; cross-language agreement can still reflect shared
constraints and is not independent proof of necessity.

Use those external traces to select and ground subsequent Whitefoot experiments.
Keep the existing probes for the narrower semantic and cost questions they answer;
their successful execution does not substitute for external demand evidence.

### External sample and the decisions it can change

[EXTERNAL-WORKLOADS.md](EXTERNAL-WORKLOADS.md) carries the six complete operation
traces, precise revisions, source links, and failure/lifetime boundaries. The
sample was selected for discriminating contracts, not weighted by prevalence.
Its three domains are text search, analytical query execution, and scheduler
state. Multiple traces from one application are related evidence, not additional
independent samples.

| External trace | Recovered need | Unknown cause or cost | Next implementation decision |
| --- | --- | --- | --- |
| EW1, ripgrep search window | Retain context and incomplete lines, compact/reuse backing, preserve configured growth refusal and contiguous matcher input | Whether copying tails or initializing new capacity materially costs time; whether a ring wins after boundary handling | Compare prefix compaction with two-span/coalescing only under the same matcher and memory contract; do not select a ring from byte-buffer occurrence alone |
| EW2, DuckDB nullable nested columns and selection | Distinguish logical extent, valid payload, nested intervals, and selected row mappings | Separate bitmap versus per-cell enum layout, copying, and vectorization costs; reused NULL bytes are not an initialization contract | Ground a later nullable-storage/relational-contract probe; first test a checked ordinary element form before requiring public library layout authority |
| EW3, DuckDB string construction | Reserve, populate, then publish; inline short values coexist with stable long-string backing and retained references | Optimal inline threshold/chunk policy; cost of replacing pointers with handles | Keep destinations and stable backing separate; compare optional store forms while preserving construction and refusal order |
| EW4, DuckDB grouped aggregation | Distinguish empty, reserved, and usable slots; preserve row references during pointer-table rehash; admit many-to-one group mappings | Need for pointer/salt packing versus a typed state sum; collision-dependent costs and complete failure recovery | Require reservation and target-alias contracts in a bounded map/scatter experiment; no automatic disjointness from distinct input indices |
| EW5, Kubernetes indexed heap | Preserve map-domain/key-array correspondence and positions while heap order changes | Best map representation and the role of Go pointer/runtime conventions | Exercise compositional contracts through swap/update/remove helpers; neither stable array addresses nor a universal handle layer follows from key identity |
| EW6, Kubernetes event history | Retire interior reader markers out of order and reclaim only history no remaining reader needs | List versus segmented-log costs; maximum retained history under slow readers | Compare stable-node and indexed-log ownership with the same ordering, repeated-finish, and reclamation contract; this is container evidence, not an I/O/runtime protocol change |

None of these traces falsifies owned places as the current lowering foundation.
They do prevent its address knowledge, the initial state-family list, or the
finite range model from being treated as a complete library proof system. They
also make the reconsideration condition concrete: if an ordinary checked form
cannot preserve one of these frozen contracts, fix the owning semantic layer or
reopen representation authority rather than adding a guard, hard cap, extra scan,
or allocation that changes the contract. A more compact upstream layout alone is
insufficient evidence that the alternative is required. These follow-up questions
do not expand the current slice into six application ports.

### Local capability witnesses and remaining gaps

| Class | Complete trace to preserve | Local demand source |
| --- | --- | --- |
| Tiny temporary sequences | Empty, a few inserts, occasional spill, helper transfer, clear and reuse | `compiler/src/lowering/builder.rs`: block successors and builder collections |
| Fully initialized fixed objects | Construct, store in a record, return through a helper, repeatedly update | `tests/programs/raw_deflate_dynamic.wf`, `fir_filter.wf` |
| Growing nested owned values | Grow or fail, preserve existing elements and unconsumed inputs, clean up | `tests/programs/growable_vec.wf`; byte-only examples underexercise affine payloads |
| Bulk transformations | In-place shrinking, overlapping copy, and LZ back-reference expansion | `tests/programs/percent_decode.wf`, `utf8parse.wf`, `raw_deflate_dynamic.wf` |
| Retained tails and rings | Append, consume a prefix, wrap, inspect two spans, reuse backing | `tests/programs/wfgrep.wf`, `run_queue.wf` |
| Variable records and names | Many short or empty values, rare long values, nesting, retained references | `tests/programs/byte_string.wf`, `wfgrep.wf` |
| Graphs and coordinated tables | Adjacency, membership, discovery state, worklists, deterministic readiness | `compiler/src/semantic/entailment.rs`: SCC and postcondition scheduling |
| Sparse keyed collections | Insert, find, delete, reuse, maintain index relationships | `compiler/src/lowering/builder.rs`; `tests/programs/option_slots.wf` |
| Partial construction | Fail after several owned values, return every responsibility exactly once | `tests/programs/prefix_expression.wf` and ownership rules; broader failure trace needed |
| Fixed pools | Exhaust, lease, return in a different order, reuse without extra backing | `tests/programs/block_pool.wf` |
| Borrowed stable storage | Publish, suspend, resume, join, consume result, retire, then reuse | Runtime interface supplied by the I/O/runtime agent; container side only |
| Destruction | Early exit with a deep owned structure, bounded or explicit cleanup storage | `PROV-6` release graph; a measured deep-chain case is still needed |

Bulk operations need their own semantics: disjoint copying, overlap-preserving
movement, and LZ expansion reading newly written output are not interchangeable.
Likewise, an owned tree does not cover shared graphs or sparse stable identities.

DMA mapping/coherence, general cancellation, arbitrary strided splitting, and
concurrent reclamation remain boundary questions. Their possible future value is
not a reason to add pinning, atomic metadata, or a registry to every container now.

## Baseline defects and current implementation boundary

The following distinctions prevent an implementation gap from being mistaken for
a language design result:

1. **Baseline implementation limitation, now a required positive:** forming an
   exclusive view of an inline run reported
   `SemanticUnsupported::ExclusiveViewOverInlineRun`. It was not a source-language
   rejection. The owned-place implementation now compiles and executes the
   mandatory inline-view probe; that closes this particular capability stop, not
   the complete storage/lifetime work. The minimal source form is:

   ```whitefoot
   command fn main() -> status: own ExitStatus pure {
     let empty = fixed_vector::<u8, 4>();
     let built = place_back(vector: move empty, value: 7_u8);
     region {
       let view = mut_slice_of(&uniq built);
       set view[0_u64] = 9_u8;
     }
     let byte = built[0_u64];
     return exit_status(code: byte);
   }
   ```

2. **Confirmed baseline lowering concern:** inline-run emission represented
   runs as aggregates and staged whole values through storage slots around
   element operations. A 4096-byte fill probe retained whole-aggregate work inside
   its loop after local Apple Clang 21 `-O2` lowering. This establishes a problematic
   machine shape for that probe, not a portable timing result. By-value source
   ownership does not itself require aggregate copying; destination construction
   and in-place updates must be evaluated separately from the source API.

3. **Specified composition gap:** `MSR-3` does not carry incidental inner element
   measures across the run boundary. `CALL-4`/`FN-9` also limit projected result
   contracts. The source experiments distinguish these from working count
   contracts, local field/variant transport, and inherent fixed-capacity facts.
   Improving aggregate lowering alone cannot supply a missing callable contract.

4. **Specified view restriction:** `VIEW-2` supplies a contiguous view of a
   nonwrapping window, while `VIEW-6` restricts repeated result view types/regions.
   A wrapped ring's two-span consumption needs an actual checked expression; a
   second full backing allocation is not an equivalent solution for a one-backing
   workload contract.

5. **Unimplemented proposal to reconsider:** `DESIGN.md`'s CALL-7/B6 proposal asks
   for nontrivial relations for every measure. Its own examples exclude useful
   front operations and measured generic results. Contract adequacy should follow
   the abstraction and the operation's required domain, not a demand to expose
   every representation coordinate. This is a design issue, not a claimed defect
   in an implemented B6 checker.

6. **Evidence boundary:** the lifecycle suite now confirms the empty linear-owner
   gap, and the finite model covers concrete sparse reuse. Full-capacity generic
   permutation, general typed sparse storage, and measured deep destruction still
   require their own executable traces. Do not generalize the bounded results.

Do not generalize one bad fill loop into a ban on all initialization. The existing
`research/experiments/buffer-initialization-cost/RESULTS.md` measures a case where
one-time initialization is negligible for long-lived reusable storage. Repetition,
element width, lifetime, and the surrounding operation matter.

## Architecture alternatives after cross-review

The first review identified three families. Cross-review showed that they are not
three mutually exclusive complete solutions:

| Direction | Question answered | Strongest challenge |
| --- | --- | --- |
| Compiler-owned state machines | Does the compiler directly own full-array, prefix, ring, and any justified additional initialization states? | A new required layout must not need another trusted family merely to avoid extra storage or redundant work. |
| Checked library representation | Can libraries obtain precise backing and initialized-range authority through checked invariants and finite evidence? | Helpers and representation abstraction must compose without replaying long proofs, exposing private layouts everywhere, or requiring unbounded automatic reasoning. |
| Stable objects and owning stores | Where do stable identity and storage live when values are deleted, recycled, or borrowed across suspension? | Identity must not become mandatory indirection or allocation for dense scans; recycled IDs must not silently regain authority. |

The first two primarily compete over **where representation authority lives**.
The third is a **storage/identity choice** that can accompany either. Full arrays,
prefixes, rings, sparse slots, and stable objects should be compared on their own
required state and conversion costs; no universal layout is assumed.

All three also need compositional abstract contracts. Storage validity alone does
not establish `free + outstanding = total`, graph/worklist membership implications,
or atomic ownership recovery after a two-index insertion fails. This is a separate
language capability question, not something a storage descriptor solves.

A useful adversarial trace has a four-slot backing: construct values in slots 0,
1, and 2 while 3 remains raw, keep 0 and 2 borrowed, discharge the owner in slot 1,
then reuse only slot 1. One circular live window does not express its `0101` state.
The initial three-slot proposal was wrong: `101` at capacity 3 is a wrapped window,
as the model's near-neighbor control demonstrates.
Ordinary tagged slots, independent slot capabilities, or chunked ownership may
solve it; each must demonstrate its complete trace and cost. This is a prospective
non-I/O sparse-lifecycle case. It does **not** require the current staged runtime
to retire source iterations out of order.

## Evidence needed to select a form

Compare candidates with the same behavior and declared resource contract. Each
candidate must show natural source, helper contracts, initialization transitions,
failure cleanup, final storage placement, and generated machine shape. Pair each
positive trace with a nearby invalid trace: raw read, duplicate ownership, aliased
exclusive ranges, stale identity, missing release, or premature backing reuse.

Record allocation count, bytes copied/initialized, peak live storage, stack demand,
steady-state machine work, proof size, diagnostic usefulness, and checker cost.
Measure only the dimensions relevant to that trace. Runtime state inherent to the
algorithm is not a proof overhead. Reference timings must use comparable algorithms
and layouts; source occurrence counts are not execution hotness.

The smallest useful selection experiment couples a candidate ordinary path with
one critical path that distinguishes the candidates, crosses a real helper
boundary, and ends the relevant storage lifetime. A choice becomes actionable when
these programs run, nearby invalid programs are rejected for their rules, and the architecture
tradeoff is visible in source and cost evidence. Remaining limits should be named;
an ever-growing scenario list or a universal framework is not the completion test.

## I/O boundary and validation

Keep the runtime's acquire/publish/join/read-result/release protocol opaque. A
published callee may change workers and suspend. Its storage must remain valid
through join return, result consumption, and the corresponding retirement; DONE
alone is insufficient. Form an address-bearing view only once its backing is in
the required stable location. Current slot sizes and experimental worker policies
are not language premises. This review adds no I/O, pinning, lease, or scheduling
interface and makes no change to the other agent's worktree.

Before rebase, the compiler build and the six ordinary `programs::runs` tests passed.
After rebase, the three changed unreadable-path program tests passed. The rebased
tree was checked against both input revisions. The dense, lifecycle, and authority
experiment checks pass and are wired into the root research stage. A sandboxed root
`make check` passed 1,518 library tests but failed seven existing loopback program
tests because binding a local socket was prohibited; 66 other program tests passed.
The unchanged canonical `make check` then passed with loopback permission, including
the compiler and research checks, the full native conformance adapter (730 passed,
3 skipped), and the snapshot corpus (484 passed, no flips). No test was weakened or
removed to obtain this result. Those counts establish the pre-implementation
experiment and selection revision. Implementation is now in progress; the
implementation checkpoint above has its own bounded performance result, but the
earlier green gate does not validate the modified compiler. No main merge is
proposed here.

At the `f5dab70c` implementation checkpoint, the canonical compiler `test-unit`
stage passes 1,488 tests (67 sampling tests belong to its separate stage). The
root `make check` passes specification archive/prose checks and conformance
structure/coverage (25 runner tests, 161/161 rules), then stops at compiler
formatting. A formatting-only correction in `cost_shape.rs` resolves the one
unrelated diff; remaining format differences and the independent Clippy
`write_with_newline` failure were in the then-unapplied parallel adapter. They
are corrected with the parallel implementation. No check was disabled.

The subsequent canonical partition check confirms 1,555 library cases split into
1,488 unit and 67 sampling cases; all 67 sampling cases pass. The snapshot corpus
passes all 484 cases with no flips. The full native conformance run exposed one
diagnostic-priority regression: a repeated affine element read-out must report
TYPE-2 before its simultaneous OWN-1 violation under DIAG-1. Correcting the
compiler preserves the original case and verdict, and the rerun passes 732 cases
with one declared pending case. The 19 focused storage semantic tests include
the legal swap control and the repeated-read-out negative.

Integration exposed two further ordinary-path observations. System wrapper calls
must render a stored aggregate's materialized operand rather than its former SSA
name; the corrected path passes all 11 network program tests. The FIR observer
now checks direct enclosing-field addresses instead of requiring aggregate
reconstruction, and retains its unchanged runtime result checks. The canonical
integration rerun passes its binary and adapter harnesses and 72 program cases;
the remaining program case fails because `generic_nominals.wf` exits abnormally
under `--par` with four workers. That existing fixture returns stored aggregates
from eligible sibling calls. That run exposed a parallel integration defect,
not a host loopback-permission failure or a reason to narrow the corpus.

The complete gate must run after parallel integration. Existing green sampling
counts do not establish aggregate result/carry retirement: their ordinary frame
boundary case uses array arguments with a scalar result, and the staged user-call
fixture also returns a scalar. The new aggregate and inline-place execution
controls exercise that missing boundary. LoopSplit currently charges each aggregate capture the
entire 256-byte lane payload during admission; a small record capture can be
permission-eligible yet never select split actualization. A sequential fallback
for that source is not evidence of a parallel aggregate capture.

The signature-metadata step passes all 30 lowering tests, including three new
controls for identical descriptor types with different source roles, shared/unique
borrow results, and compiler-synthesized signatures. The gate-profile compiler
build completes without warnings. LLVM output is byte-identical before and after
this step for the retained N=256 dense program, wide-record program, inline view,
and `generic_nominals.wf` with `--par`; a local staged-call prototype also matches.
That last comparison is exploratory emission evidence, not a committed staged
runtime test. No parallel adapter was changed and no new aggregate/staged runtime
result is claimed. The metadata step does not complete destination normalization
or make the existing full gate green.

The user-call metadata step passes all 34 lowering tests and all 37 focused
borrow-semantic tests. Four new controls exercise borrowing and consuming the
same actual value, unique-holder transfer, projected-root consumption, and a
returned borrow tied to the second actual argument's indexed address. The
gate-profile compiler build is warning-free, modified Rust files pass formatting,
and the same five LLVM comparisons remain byte-identical to the signature step.
This is evidence for preserving checked information, not a new performance or
parallel execution result. The complete storage normalization and repository gate
remain unfinished. The canonical Clippy invocation again reports only the
then-existing `write_with_newline` diagnostic in the parallel emitter; this is
a failed lint run, not a whole-compiler lint pass.

The parallel integration step passes the three new native execution controls in
all four worlds: the sequential reference, forced lane refusal, native workers,
and publications deferred until their actual joins. Both staged controls require
at least two frames held simultaneously, and all source allocations and lane
frames are released exactly once. At `bf8cdc56`, the canonical root `make check`
completes with `WHITEFOOT ALL TESTS GREEN`: formatting, Clippy, all 1,566 library
cases (1,496 unit and 70 sampling), all 73 program cases, runtime checks, research
experiments, full native conformance (732 passed, one declared pending), and the
snapshot corpus (484 passed, zero flips). The previously failing
`generic_nominals.wf --par` corpus case now passes. This establishes the parallel
integration checkpoint, not complete destination normalization or new performance
measurements. Its green result does not validate subsequent compiler changes.

The cleanup-place controls observe both explicit disposal and scope exit of an
addressed owner containing a store-branded cell and a legacy buffer. Their expected
allocation/release trace follows PROV-6's field declaration order. They exposed
an existing reversal: the checker and recursive cleanup emitter pushed fields in
forward order onto last-in-first-out work stacks. Reversing insertion preserves
the specified traversal, including enum payloads and partial-consume residuals;
binding scope order remains STOR-3's reverse declaration order. The regression
test keeps the normative expected trace. This is a compiler correction, not a
specification or conformance-verdict amendment.
Three ordinary semantic tests had encoded the same reversed field order; their
expected paths now follow PROV-6, retaining their exact release counts and the
separate assertion of reverse binding order. A native test that counted releases
without observing their order no longer claims an order in its name or comments.

After the selected control-header boundary was implemented, the focused native
`owned_match_headers_and_staged_results_observe_completed_scratch` test passed once
in 3.48 seconds. Its direct user-call match establishes source admission but takes
the ordinary completion path in the current actualizer. The separate bound-result
then match shape uses the staged handout; its deferred native observer sees the
expected value 48 and the exact release trace after join. This is focused evidence
for header-loan semantics and staged retirement, not evidence that a direct
user-call match currently selects staged actualization or that the full gate passes.
