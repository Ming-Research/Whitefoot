# Declaring what runtime-managed types do: release action and object identity

Status: design investigation, written before any implementation. Nothing here
is an amendment, a design-tree decision or an implemented capability. Line
numbers cite the branch `claude/release-class` (PR #437, specification v0.151)
at commit 8689a62a6, and `claude/atomic-range-aliases` (PR #441) at c5755445e.

## 1. Question

How should "what release does for a value of this type" be stated once, so
that the specification's release graph [PROV-6, STOR-3, SHARE-1], the checker's
tail-transfer and scope judgments [FN-10, PROV-6] and the backend's cleanup all
derive from it, and so that forgetting to state it for a new type is an error
when the compiler is built or the declaration is read, never a silent
acceptance?

The owner widened the question twice on 2026-10-13 (both remarks written in
Chinese, translated): "the root problem is that the checker does not know which
values hold runtime resources; adding exceptions one by one is a stopgap", and
"this is still caused by the checker being incompatible with our interior
mutability mechanism". Section 5 therefore treats a second axis of the same
question: which handles may or must name the same runtime object, and how a
writer proves two handles distinct.

## 2. Current contract and evidence

PR #437 fixed this defect: `return musttail f(...)` [FN-10] with a live
reference into a `KeySet` owned by the current activation was accepted, while
the backend released the key set's runtime store before the jump, leaving the
reference dangling. The specification's non-empty-release clause omitted
`KeySet`, and the checker, IR and backend each kept their own list. The
checker's old list ended in a wildcard arm (`_ => {}` in
`has_nonempty_release`, removed by #437), so a type absent from it was read as
"releases nothing".

After #437 one cycle-safe walk, `has_release` in `compiler/src/release.rs`,
owns the action and edge policy, and two adapters translate types into its
`ReleaseShape`: `release_shape` in `compiler/src/semantic/check/linearity.rs:206`
and `type_derives_release` in `compiler/src/ir.rs:290`. Both adapters are
exhaustive over `CheckedType`/`IrType` and over the nominal kinds. That closes
one of the two routes by which a new runtime type enters the compiler, but not
the other:

1. **A new type constructor** (`CheckedType::KeySet`, `IrType::KeySet`, the
   storage shapes): resolution maps the prelude name through
   `CONTAINER_NOMINALS` (`compiler/src/resolution/kernel.rs:95-118`) to its own
   enum variant. Every exhaustive match then fails to compile until the new
   variant is decided. This route is now guarded, but only where the match has
   no wildcard; the inventory below finds properties that still use one.
2. **A prelude or host nominal classified by its name**:
   `compiler/src/semantic/check/nominal_instances.rs:786-850` turns
   `DecisionCell` into `Shared { state: u32 }`, `Frozen` into a `Box` with the
   frozen release class, and `Shared`/`SharedRead`/`ConcurrentHashMap` into
   `Shared`, by comparing `template.name` strings. Any other opaque
   declaration falls through: fieldless becomes `CheckedNominalKind::Opaque`
   (release `Empty`, `linearity.rs:221`; layout `{ i128, i128 }`,
   `backend/emitter.rs:3687`), and one with fields becomes an ordinary
   `Struct` whose release is its fields'. A new runtime type declared this way
   with readonly measure fields, exactly as `KeySet` is declared
   (`compiler/src/prelude.rs`, `opaque nocopy struct KeySet { readonly len:
   u64; readonly bytes: u64; }`), silently gets the empty release of two
   integers. Rust exhaustiveness cannot see a missing string.

The specification has the same shape. PROV-6 (`spec/kernel-spec.md:824`)
names the non-empty types one by one; STOR-3 (`:903-917`) lists one action
per type name and gives "every other opaque struct" its fields' release;
SHARE-1 (`:2352`) lists the parts a frozen value may not hold by name; PRE-2
(`:2691`) gives every fieldless host handle an empty release by class.

## 3. Inventory: properties decided by a list of names or a match on constructors

"Fails safe" means omitting a new type rejects a program or stops the compiler
with an internal error. "Fails unsafe" means omission accepts a program it
should reject or emits wrong code. The example in each row is a new prelude
type `R` that holds runtime storage, added without updating that site.

| # | Property | Where | Omission | Failure example |
|---|---|---|---|---|
| 1 | Release action is non-empty (tail transfer, scope release) | Spec PROV-6 `:824`, STOR-3 `:903-917`; checker `linearity.rs:206-260`; IR `ir.rs:290-347`; walk `release.rs` | **Unsafe** via route 2 (name fall-through to `Opaque`/`Struct`); route 1 guarded since #437 | The #437 defect again: `musttail` with a reference into a local `R` accepted, backend frees `R` before the jump. |
| 2 | Which release action runs (the emitted cleanup) | `backend/emitter/cleanup.rs:59-80` (helper per kind), `:905-990` (per-type emission); `emitter/shared.rs:1042` name check for `DecisionCell` | **Unsafe** (leak or wrong free) via route 2; route 1 errors with `InvalidIr` | `R` classified `Opaque` emits no cleanup: its store leaks; if the runtime frees it on another path, nothing in the checker accounts for it. |
| 3 | Copy capability | Spec OWN-1 `:687-689`; `semantic/model.rs:1171-1225` (`type_has_copy_capability`) reads the declared `nocopy`; no rule ties copy to release | **Unsafe**: a fieldless or measure-only opaque declared without `nocopy` is copy | Copying an `R` value duplicates its store pointer; both copies release it: double free. Nothing checks "non-empty release implies nocopy". |
| 4 | Drop capability / linearity (`nodrop`) | Spec PROV-6 `:796-812`; `linearity.rs` closure | Safe: missing `nodrop` makes the type affine, which still releases; the result is a leak of an external resource that was meant to be closed explicitly, not memory unsafety | A host file handle declared `nocopy` instead of `nodrop` is dropped without `close`; the descriptor stays open. |
| 5 | Frozen content admissibility | Spec SHARE-1 `:2352` (list: `Shared`, `SharedRead`, `DecisionCell`, host handle); `semantic/check/frozen.rs:40-56` stops at `Shared{Object}` or a fieldless host `Opaque` | **Unsafe** | A new interior-mutable runtime type classified `Opaque` (not from a host module) or `Struct` is admitted in `Frozen<T>`; frozen values cross contexts without an atomic statement: a data race. |
| 6 | Parallel-mode storage reclaim (overlap narrowing) | `linearity.rs:179` (`may_release_storage`) over the same shapes; spec PAR-1/PAR-2 through STOR-3 | **Unsafe** via route 2 (the `Opaque => Empty` arm answers for both queries) | A write through `&R` that reclaims its store is scheduled to overlap a statement reading it: use after free under `--par`. |
| 7 | Whether a function needs the heap | `semantic/entry.rs:279-340` (`holds_heap_storage`/`type_holds_heap`); `Opaque` and `KeySet` answer false | **Unsafe** if `R` allocates from the program heap | A function whose only heap-backed value is an `R` is planned as heap-free; its release calls the heap that was never set up for it. `KeySet` answers false because its store is the runtime's; whether that holds for every future type is exactly what no declaration says. |
| 8 | Snapshot-resource-free judgment | Spec SNAP-1 `:3355`; `semantic/check/held.rs:90-115` walks `Struct` fields | **Unsafe** via route 2 with fields; safe for fieldless (`_ => snapshot_resource`) | A measure-only `R` passes as a plain struct; the encoder writes its two counters and loses its content. |
| 9 | Physical representation | `backend/emitter.rs:3680-3695` (`Opaque` is `{ i128, i128 }`, handles `ptr`, `KeySet` `{ i64, ptr }` at `:3611`); `backend/storage.rs:30-50` | Unsafe in principle (the runtime and the emitted code disagree on size), but a mismatch shows on the first runtime test of the type | A fieldless `R` gets the uniform host representation while its runtime functions expect a pointer. |
| 10 | Equality admissibility | `semantic/check/expressions/equality.rs:215-262` rejects `nocopy`, `Box`, `Shared`; `Opaque` passes | Unsafe only through row 3 | A copy `R` admits `==`, comparing store pointers or measures instead of contents. |
| 11 | Lock order of atomic targets by type | Spec state-lock order ("prelude nominals in PRE-1 order", `design/compiler/waiting-contexts/state-locks.md`); `semantic/check/types.rs:1925-1945` ranks `Box`, `Shared`, `SharedRead`, `ConcurrentHashMap` by name, `_ => 100` | Safe: every context uses the same static order, so no deadlock; it can diverge from the stated PRE-1 order | `DecisionCell` and a new `R` are ordered by module path and name rather than PRE-1 position. Unverified whether a `DecisionCell` can be an atomic target. |
| 12 | Self-changing host handles (guards) | Spec PRE-2 table; `library.rs:260-270`, test against the specification table | Unsafe (lost wakeup, not memory): a guard over an omitted handle is admitted and never re-evaluated. The list is already checked against the specification table, so omission requires omitting both | A new self-changing clock handle missing from both tables. |
| 13 | Diagnostic repair kind of an opaque struct | `nominal_instances.rs:1966-2000` (`opaque_struct_kind`), `_ => InvalidResolution` | Safe (internal error) | — |
| 14 | Concrete type identity, region substitution, place steps, nominal activation | `generics.rs:2088`, `nominal_instances.rs:1230-1246`, `expressions/places.rs:2040-2056`, `inventory.rs:150-170` | Safe (exhaustive, `Opaque` is an internal error or has no parts) | — |
| 15 | May cross contexts (spawn arguments, `Frozen`, handles) | Spec WAIT-3 `:2334-2343`: every value may move into a spawn; no per-type property exists | Unsafe only for a future thread-affine runtime type, which today cannot be declared differently | A runtime type with a non-atomic count, moved into a spawn. Today every runtime type is thread-safe by construction of the runtime. |
| 16 | Which handles may name the same object | Spec SHARE-2 `:2390` ("no rule proves that two handles name two objects"; targets alias exactly when their state types unify); PR #441's partition walk | Neither: it is a missing representation, not a list (section 5) | A new handle type `H` that may alias must also be judged by SHARE-2's unification criterion; nothing says whether a new type's values may alias. |

Rows 1, 2, 3, 5, 6, 7 and 8 are the defect class the owner named: each is
decided by the type's runtime behaviour, each has a name-keyed or
fall-through step, and each fails unsafe.

## 4. Alternatives for the release action

### A. A mandatory runtime-behaviour field on each opaque prelude declaration

Every opaque type the prelude declares carries a release action from a closed
set, stated once in the specification as a table in PRE-1 and once in the
compiler's prelude table, with the classification of every other type derived
structurally from its fields:

| Action | Types today |
|---|---|
| none | `Entries` (a statement's record, never released) |
| element-wise, no storage of its own | `Array`, constant-capacity `Slots`, `Ring` |
| element-wise, then pages | `Paged` |
| content, then the cell | `Box` |
| return runtime-held storage | `KeySet` |
| release held values, then return runtime-held storage | `ConcurrentHashMap` |
| release a handle; the last handle releases the state | `Shared`, `SharedRead`, `Frozen`, `DecisionCell` |

Fieldless host handles keep PRE-2's class rule (release empty), stated as a row
of the same table for the class rather than one per handle; host opaque
structs with fields (`Instant`, `CancelSource`, `CancelWatch`, `CancelState`)
release their fields.

In the compiler the prelude table's entries gain a typed field (a Rust enum),
so an entry without it does not build; the name comparisons in
`nominal_instances.rs:786-850`, `emitter/shared.rs:1042` and `types.rs:1925`
read that field instead of strings; and an opaque prelude declaration whose
entry has no field is a compiler failure when the prelude is read, replacing
today's fall-through to `Opaque`/`Struct`. A unit test compares the compiler
table with the specification table, as `library.rs` already does for the
self-changing host handles.

- Guarantees: route 2 closes. A new prelude type cannot get a classification
  nobody chose, and the checker, IR and backend read one value.
- Cost: one table in PRE-1, one typed column in the prelude table, a test, and
  rewriting PROV-6's non-empty clause and STOR-3's per-type lines to cite the
  table. No grammar change; the prelude records stay ordinary source
  declarations [PRE-1].
- Generics and user structs: unchanged. A user struct derives its action from
  its fields; a symbolic parameter is read at its capability bound [FN-10],
  which is sound only if "non-empty release implies `nocopy`" holds (see D).
- Relation to `nodrop`: orthogonal. `nodrop` is an obligation to consume
  explicitly; the action is what implicit release does. A host handle that
  owns an operating-system resource chose "release empty, `nodrop`, consumed
  by a closing host function"; the table makes that a stated row.
- Surface: specification and compiler structure only.

### B. Exhaustive matches with no wildcard arms

Keep the derived classification and forbid wildcard arms in every function
that decides a type property, enforced by Clippy's `wildcard_enum_match_arm`
restriction lint denied in those modules.

- Guarantees: route 1. A new `CheckedType`/`IrType` variant or nominal kind
  does not build until every property is decided. The pre-#437 `_ => {}` arm
  that caused the `KeySet` defect would have been refused by this lint.
- Cost: small; some long matches.
- Limit: route 2 is invisible to it. `DecisionCell`, `Frozen`, `Shared`,
  `SharedRead` and `ConcurrentHashMap` all entered through name comparison, so
  route 2 is the route new runtime types actually take.
- Surface: none.

### C. A source-visible capability, such as `releases`

A third capability beside copy and drop that user types and generic bounds
carry, e.g. `fn f<T: drop + noreleases>`.

- Guarantees: a generic body could know a parameter's release is empty
  without being copy.
- Cost: a language surface change and a new bound vocabulary [FN-2, OWN-1].
- Why not: a source type can hold runtime resources only through prelude and
  host types, so its release is already derived; STOR-3 forbids user-defined
  finalizers, so there is nothing for a user to declare. The one consumer that
  reads a symbolic parameter, FN-10, is conservative and correct at the copy
  bound once D's coherence holds. No current program needs it.

### D. One runtime-behaviour descriptor per opaque declaration, with coherence checks (A + B, extended)

A's field becomes a descriptor from which every runtime-dependent property in
section 3 is read, B's lint guards the constructor dimension, and the compiler
checks coherence between the descriptor and the declaration:

- release action (rows 1, 2, 6): as A;
- object identity (row 5, 16): unique owned / aliasable mutable /
  aliasable immutable / external (section 5);
- program-heap use (row 7): whether the action frees program-heap storage;
- snapshot encodability (row 8): only "none", element-wise and owned cell
  content are walked; every other descriptor is a snapshot resource;
- representation class (row 9): pointer handle, runtime pair, uniform host,
  or field-wise.

Coherence checks, run when the prelude and host records are read:

1. a non-empty release action or an aliasable identity requires `nocopy` on
   the declaration (closes row 3 and, through it, row 10);
2. frozen-content admissibility is "no part whose identity is aliasable
   mutable or external", which reproduces SHARE-1's present list (`Shared`,
   `SharedRead`, `DecisionCell`, host handle) from the identity column instead
   of names (row 5);
3. "release a handle" exactly when the identity is aliasable.

- Guarantees: both routes closed for rows 1, 2, 3, 5, 6, 7, 8 and 10, and a
  new type cannot be declared incoherently (a copy type with a store, a
  mutable alias admitted in frozen content).
- Cost: A's cost plus three derived columns and the checks. The checker's
  frozen walk, heap-need walk and snapshot walk change from kind and name
  matches to descriptor reads.
- Generics, user structs, `nodrop`, surface: as A.

## 5. The identity axis: interior mutability and the place model

### The present model

The checker's facts are keyed by places: a root and steps [REF-1], one
representation for references, effects, proof support and invalidation
(`design/compiler/checker-facts.md`). Ordinary reference rules make two places
that can be written at the same time either equal or provably disjoint
[OWN-7, EFF-5]. `Shared` handles are interior mutability: several handles,
held in distinct variables the checker sees as distinct roots, name one object
whose state is written under an atomic statement. The object has no
representation in the place model. SHARE-2 bridges the gap with one rule
(`spec/kernel-spec.md:2390`): paths from two targets whose state types unify
are compared as paths from one root, otherwise from two roots, and "no rule
proves that two handles name two objects".

Two consequences follow, both observed:

- **PR #441** (atomic targets that may alias, branch
  `claude/atomic-range-aliases`): because a pair of unifying targets may be
  one object or two, the range walk checks every partition of possibly
  aliased targets into objects, placing one object's targets at one storage
  location (`design/compiler/range-judgment.md`, the decision #441 adds). The
  count of partitions grows as a Bell number; the decision records that cost
  as provisional.
- **Firn's distinct maps** (study
  `research/investigations/shared-handle-distinctness/DESIGN.md` at
  e86f961dc): a directory of database maps cannot be passed, two maps at a
  time, to one helper that writes both, because no fact says two handles name
  two objects. That study recommends a proof-only `distinct(h1, h2, ...)`
  relation carried by type invariants and contracts (its A), plus a separate
  relation for disjoint selected references (its D).

Release behaviour depends on the same missing fact. "Release a handle; the
last handle releases the state" is the release action of an aliasable
identity, and "content, then the cell" is the action of a unique one; the
#437 defect is the release-side symptom of the checker not knowing which
values hold runtime objects, and #441 and Firn's study are the identity-side
symptoms.

### An object-identity model

Each runtime-managed type's descriptor states its identity semantics:

| Identity | Meaning | Types |
|---|---|---|
| unique owned | the value is the only name of its storage; a place names it | `Box`, `KeySet`, storage shapes |
| aliasable mutable | handles carry an object identity; several handles may carry one; state is reached only under an atomic statement | `Shared`, `SharedRead`, `DecisionCell`, `ConcurrentHashMap` as shared state |
| aliasable immutable | handles may share an object that nothing writes | `Frozen` |
| external | the host may relate it to other handles outside the program | host handles |

The proof model then gives every value of an aliasable type an identity term,
erased before lowering: `shared_new` and `shared_map_new` introduce a fresh
term distinct from every live one; `shared_share`, `shared_read`,
`shared_read_share` and a move preserve it; a target's state root is
`state(identity)` rather than the target binding. Two roots are the same when
their terms are equal, different when a fact proves them distinct, and
possibly the same otherwise. Writers state distinctness through the Firn
study's `distinct(...)` relation in type invariants, `requires` and `ensures`.

What changes in the specification:

- SHARE-1: fresh and preserved identity of each handle-forming function, as
  semantic facts of the existing operations (no runtime field).
- SHARE-2 `:2390`: possible overlap of targets is "identity terms not proved
  distinct", with state-type unification kept as a necessary condition; the
  "no rule proves" sentence is replaced by the relation's sources.
- OWN-7 and REF-1: a path may be rooted at `state(term)`.
- ENT-2, ENT-3, ENT-5: an identity fact domain with its sources, support,
  invalidation and meet (the Firn study's appendix A table).
- TYPE-11, FN-8, FN-9 and the call rules: `distinct` in invariants and
  contracts, with substitution at boundaries.
- PRE-1 / STOR-3 / SHARE-1 frozen rule: the identity column of the
  descriptor table, shared with the release action.

What changes in the checker: the atomic-target overlap test reads identity
facts beside type unification; the range walk's partitions are restricted to
those consistent with proved equalities and distinctness.

### #441's partition walk under this model

Kept, not replaced. Identity facts prune it: a pair proved distinct is never
grouped, a pair proved equal is always grouped, and only unknown pairs are
enumerated. With full distinctness the walk has one partition; with no facts
it is #441 unchanged. Replacing it outright would need one of two things the
design record has refused or not chosen: requiring distinctness before
holding two handles together (rejected in
`design/language/waiting/shared-objects.md`: runtime handles may name one
object and merging their targets makes that case correct), or a weak-update
approximation of unknown aliases, which is linear in cost but rejects
programs #441 accepts, such as the "reset" cases where both orders of aliasing
end with the invariant restored. The model makes the Bell-number case
avoidable by writing `distinct`, and leaves it exact where the writer cannot.

### What one model would not cover

- External aliasing: two host handles reaching one file or socket. HOST-1
  orders host effects; the place model is not the tool for it.
- Correlated selection after a join, e.g. a conditional swap of two selected
  references: the Firn study's D, a relation between selected targets, is a
  separate fact domain.
- Injectivity of an indexed collection of handles (`Array<Shared<T>, n>`): the
  study's B (an indexed family) or quantified `distinct` invariants.
- The partition cost for genuinely unknown handles.

### Same item or separate

The identity column of the descriptor belongs in the release-action item:
it is one more field of the same declaration, it already decides frozen
admissibility (row 5), and "release a handle" is defined by it. The identity
reasoning (terms, `distinct`, the fact domain, partition pruning, SHARE-2's
rewrite) is a separate item: it changes the proof rules and contracts, needs
its own owner decisions (the Firn study's A and D), and is not needed to close
the release defect. It should depend on the descriptor item, so that the
identity semantics it reasons about are read from the descriptor and not from
a fourth list.

## 6. Recommendation and the comparison that could reject it

Recommend D: one runtime-behaviour descriptor per opaque prelude declaration,
stated as a PRE-1 table and as a typed field of the compiler's prelude table,
with B's wildcard lint on the property functions, the three coherence checks,
and the identity column; the identity reasoning of section 5 follows as its own
item. Confidence 4/5 that this closes the observed defect class; 3/5 that the
closed set needs no new member for the next runtime type (a file-backed or
memory-mapped type might need "return host storage", which would be a new row
by a specification change, the intended path).

Falsifiers, stated before implementation:

1. **Omission must fail to build.** On the implementing branch, a scratch
   commit that removes `KeySet`'s descriptor, and another that adds a prelude
   declaration `opaque nocopy struct Probe { readonly len: u64; }` without
   one, must each fail the compiler build or the prelude read in CI. If either
   builds and runs the conformance suite, reject D as implemented.
2. **B alone versus D.** Apply both to two additions: (i) a new type
   constructor in the style of `KeySet`; (ii) a new nominal in the style of
   `DecisionCell`, classified by name. If B's lint alone catches (ii), D's
   descriptor is unnecessary and B should be chosen. The prediction is that B
   catches (i) and misses (ii).
3. **No name dispatch remains for an inventoried property.** After the
   change, a search for `template.name ==`, `.name !=` and string `matches!`
   over prelude names in `compiler/src/semantic`, `lowering`, `ir.rs` and
   `backend` finds only diagnostic-repair selection (row 13). Any remaining
   comparison deciding rows 1-3, 5-10 rejects the change as incomplete.
4. **The closed set must express today's types without an escape.** If any
   existing type (`Frozen` as a `Box` kind, `DecisionCell` as a `u32` shared
   object, `Entries`) needs a per-type exception outside the table, reject the
   set and redesign it.
5. **No verdict changes except named defects.** The conformance suite and the
   program tests must keep every verdict. A changed verdict is either a defect
   the descriptor exposed, named with its own case, or a reason to reject.
   Coherence check 1 is expected to refuse nothing today: every opaque prelude
   declaration with a non-empty action is already `nocopy`, and `Array` has
   no action of its own.

## 7. Coverage

The descriptor item covers rows 1, 2, 3, 5, 6, 7, 8, 9 and 10, and supplies the
identity column that section 5's item will read. Rows 4 (a resource-owning
host handle chooses `nodrop`; its row in the table states it), 11 (lock order
read from PRE-1 position through the descriptor table instead of a name list)
and 13 fall out of the same table at small cost and can be in the same change.

Separate items:

- Identity reasoning (row 16): section 5; depends on the descriptor item and
  on owner decisions about the Firn study.
- Self-changing host handles (row 12): already checked against its
  specification table; folding it into the descriptor is a cleanup, not a
  correctness fix.
- Context transfer (row 15): no property exists and none is needed while every
  runtime type is thread-safe; a thread-affine type would need a new
  specification rule and a descriptor column, decided when one is proposed.

## 8. Uncertain

- Row 7: whether the heap-need judgment is meant to cover runtime-held
  storage (`KeySet` answers false). A descriptor must state which storage is
  the program heap's; this needs reading the runtime allocator before writing
  the column.
- Row 11: whether a `DecisionCell` can be an atomic target, and so whether its
  missing rank is reachable.
- Row 9: the claim that a representation mismatch shows at the first runtime
  test is from reading the emitter, not from a run.
- Whether the PRE-1 table should list each fieldless host handle or keep the
  class row; a per-handle row would make a future auto-closing host handle a
  table change rather than a silent class member, at the cost of about 25
  rows.
