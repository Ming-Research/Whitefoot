# Shared object model: the comparison

## Status

Carried out 2026-10-14 on branch `claude/single-object-atomic`, after main at
`6878a970a` (specification v0.151, with #437's release classes and #441's
per-partition check merged) was merged into it. It does the comparison that
[DESIGN.md](DESIGN.md) fixed before any work (its case set and its criteria)
and applies the criteria of [ONE-OBJECT.md](ONE-OBJECT.md) section 10 to the
direction the owner set on 2026-10-13, as ONE-OBJECT.md stood at `4c3c4e705`
(atomic values as one-word objects). Verdicts for the proposed candidates
come from reading rule text, as DESIGN.md requires; only the column for the
current language (candidate P) was compiled, by the probes in
`research/experiments/shared-object-model/probes/` (section 8). No
specification, checker or runtime was changed.

## 1. Candidates

- **P, the current patches.** The specification and checker on main:
  multi-object atomic statements, SHARE-2's rule that targets whose state
  types unify are compared from one root, the release-class list of PROV-6
  (#437), the decision cell's thirteen rule changes (#364), #441's
  per-partition check, `Shared<T: drop>`.
- **S, identity and description only** (option B of the scope card):
  #478's descriptor table for every runtime type, plus a proof-only handle
  identity; classes 3 to 5 stay with the per-class patches (#484 for
  invariants, the lease design for explicit-close values, the
  decision-cell-free boundary for effects).
- **M, description plus identity, all five derived** (DESIGN.md's proposal
  under test): S, with invariant transport, explicit-close ownership and
  effect visibility derived from description and identity.
- **O, one object per atomic statement** (ONE-OBJECT.md): an atomic statement
  names one handle; maps and shards become parts of a shared state; a word
  that contexts change by single operations is its own one-word object,
  `Atomic<T>` (ONE-OBJECT.md 2.3 as revised on 2026-10-14); lock units are
  lowering's choice, equivalent to locking the whole object. O is not a model of all five
  classes by itself. For class 2 it takes #478's descriptor without its
  identity column, for class 3 #484's sealed values, for class 4 a
  linear-state shared object, and for class 5 one effect marker
  (ONE-OBJECT.md section 2.6). Where this study says O, it means that
  combination.

S, M and O share the descriptor of class 2. The comparison that matters is
therefore identity (S, M) against one object (O) for class 1, plus how each
treats classes 3 to 5.

## 2. Case set

Fixed by DESIGN.md before any candidate was worked through:

- **A.** The five examples of the board item, with their expected verdicts.
- **B.** The Firn sites in the item's plan: the 7 calls of the `select-db`
  branch that hand two or more databases to one writing function (EXEC,
  scripts, FLUSHALL, MOVE, COPY across databases, SWAPDB; board item
  `firn-req-distinct-shared-maps`), and RENAME, COPY, SMOVE and LMOVE, which
  hand two entries of one table to one call.
- **C.** The 13 rules the decision cell changed (spec log, #364): PRE-1,
  SHARE-1, SHARE-2, SHARE-3, WAIT-2, PAR-1, PAR-2, EFF-3, CAP-1, TYPE-2,
  PROV-6, STOR-3, OP-9.
- **D.** Programs not drawn from the board, taken from every multi-object
  statement outside Firn's command files on main (a code search of Halo-wf and
  Snowghost-wf found none there; GitHub's code index may be incomplete):
  - D1 FLUSHALL exchanges the keyspace's map for a fresh map object and
    releases the old map after the statement (`apps/firn/commands/server.wf`
    `run_flush`; `tests/programs/map_resets.wf`);
  - D2 a guard over a work object or a timeout's cancellation flag
    (`tests/programs/cancellation.wf`, `bor(item^, timer^.fired)`);
  - D3 a guard over two cancellation views and an ordinary object
    (`cancellation.wf`, `band(first^.fired, second^.fired)`);
  - D4 a map entry and a separate `received` object, the guard reading the
    entry (`tests/programs/map_watch.wf`);
  - D5 a snapshot of a map with a separate journal sequence
    (`tests/programs/snapshot_export.wf`);
  - D6 two objects of one type taken in opposite orders
    (`tests/programs/map_lock_order.wf`);
  - D7 generic targets whose state types differ only by type arguments
    (`share-pos-targets-repeated-type-parameter-disjoint`).

## 3. Verdicts

"exp" is the verdict the board item or the program's purpose expects.
"list" marks a verdict that needs a type-name list or a rule-specific
exception. O's verdicts are for the program rewritten into one object where
the original names two; the rewrite is shown where it is not mechanical.

### A. The board examples

| case | exp | P | S / M | O |
|---|---|---|---|---|
| A1 `clear_both` on two fresh maps | accept | reject, SHARE-2 one root (case `share-neg-targets-same-type-one-call`; Firn run 38416798793); the same program with ordinary fields of one state is accepted (probe p1) | accept if the distinctness fact survives the move into `Databases` and the field reads (a value fact on two fields; needs a `distinct` predicate in the type invariant to cross a call) | the program as written is rejected (two handles); rewritten as `Databases { dbs: Array<ConcurrentHashMap<u64>, 2> }` and `clear_both(&s^.dbs[0], &s^.dbs[1])`, accepted by OWN-7 on two constant indices |
| A1' second table replaced by a copy of the first | reject | reject | reject (identities equal) | the copy cannot be written as a second object; `&s^.dbs[0]` twice is rejected by OWN-7 |
| A2 `walk` (`musttail` with a key set alive) | reject | reject since #437, **list** (PROV-6 names `KeySet`) | reject, descriptor | reject, descriptor (same as S/M) |
| A3 `reader` through `Frozen` | accept | reject, FN-8 at the call (probe p3-frozen; a map entry likewise, p3-entry; a held whole state is accepted, p3-held) | accept only with #484 (S); M claims transport from identity, but identity says nothing about which invariant holds | accept with #484: a frozen value is valid from `frozen_new`; held state is sealed at entry |
| A4 `park` (a must-close value in `Shared`) | accept, last handle takes it back | reject, `Shared<T: drop>` (probe p4) | S: reject until the lease work; M: needs a linear-state rule that identity does not supply | accept with the linear-state object of ONE-OBJECT.md 2.6, given the two extra rules of section 4.4 |
| A5 `bump` (`pure` but changes shared state) | treated as a shared effect | handled: an atomic statement makes the function waiting and EFF-3 excludes waiting calls (probe p5); the decision cell needs the separate **list**-like "decision-cell-free boundary" | S: as P; M: "effect visibility derived from identity" needs rows that name identities, a new effect system, which DESIGN.md names as the case that splits class 5 out of M | handled by one "shared effects" marker implied by `waits`; the cell-free boundary retires |

### B. Firn sites

| case | exp | P | S / M | O |
|---|---|---|---|---|
| B1 EXEC, scripts, INFO, eviction, FLUSHALL over all 16 databases (16 targets, helpers with 16 reference parameters) | accept | reject, EFF-5 (Firn run 38416798793) | accept with 120 pairwise distinctness facts in the directory's invariant (`distinct(db0, …, db15)`), carried across calls | accept: `atomic s = &store`, helper takes `&s^.dbs`; no fact needed |
| B2 MOVE and COPY across databases, SWAPDB, each choosing two databases by number after `from != into` | accept | reject | **reject in the first version**: two independent 16-way selections lose the link between the selected handle and the number (the handle-distinctness study's own falsifier; card `shared-distinct-direction`) | accept: `&s^.dbs[from]` and `&s^.dbs[into]` are apart by OWN-7 from `from != into`, the comparison Redis semantics already require |
| B3 RENAME, COPY, SMOVE, LMOVE on two keys of one table | accept, no added key comparison | accepted only in the key-set form with a run-time index comparison (today's Firn) | identity does not apply (one table) | same as P with open choice 2b; with 2a, a byte comparison's `false` result is a key-disequality fact for OWN-7. Every one of the four already compares the keys for Redis's own same-key behaviour (RENAME a a, COPY a a's error, SMOVE a a, LMOVE a a's rotation), so neither form adds a comparison the command lacks |

### C. The decision cell's 13 rules

P keeps all 13. S and M keep them: identity does not touch the cell, and
DESIGN.md left the atomic-cell question out of their scope. Under O the
decision cell becomes `Atomic<DecisionState>` (ONE-OBJECT.md 2.3):

| rule | change under O |
|---|---|
| PRE-1 | `DecisionCell` and its five functions retire; one `Atomic<T>` declaration with one operation table replaces them and #403's operations; the claim-once guarantee, if wanted, is a `std` type with a private `Atomic<DecisionState>` |
| SHARE-3 | the decision-cell paragraph becomes the atomic-value paragraph (each operation its own point); the cell-free boundary paragraph retires |
| WAIT-2 | unchanged reference to the one order |
| SHARE-1 | the `DecisionCell` entry of the frozen list becomes `Atomic<T>`, a handle to a changing object like `Shared` (with the descriptor, the identity column decides it) |
| SHARE-2 | "guard calls are decision-cell-free" becomes the rule for guards that load atomic values (open choice 1) |
| PAR-1, PAR-2, EFF-3, CAP-1 | the cell-free boundary becomes the shared-effect marker |
| TYPE-2, PROV-6, STOR-3, OP-9 | the `DecisionCell` handle's entries become `Atomic<T>`'s (one handle, an 8-byte pointer, a handle release), or its descriptor row |

### D. Programs not drawn from the board

| case | P | S / M | O |
|---|---|---|---|
| D1 FLUSHALL swaps the map with a fresh map object, releases the old map after the statement | accept | accept | **rejected as written, and the obvious rewrite regresses**: a map cannot be a local, so the old contents are released by `map_clear` inside the hold and every other statement on the keyspace waits for the whole release. ONE-OBJECT.md missed this case. Repair (section 4.1): a detach operation |
| D2 guard `bor(item^, timer^.fired)` over two objects | accept | accept (different types) | accept only with open choice 1 (guards may load atomic values), `fired` being an `Atomic<Bool>`; otherwise rewritten with `until` and a cancellation clause |
| D3 guard `band(first^.fired, second^.fired)` plus an ordinary object | accept | accept | as D2; both flags are atomic values, one object held |
| D4 map entry and `received` object, guard reads the entry | accept | accept | accept, rewritten as one object `{ boxes, received }`; the entry is locked before the guard, the watch registered per unit (R3) |
| D5 map snapshot with a journal sequence | accept | accept | accept as one object `{ table, journal }`; the snapshot's common observation point is the statement's point |
| D6 two objects of one type in opposite orders | accept (tests the run-time identity order) | accept | not expressible; the program exists only to test the multi-object lock order, which O removes |
| D7 repeated type parameter, two whole targets | accept by unification | accept | rewritten as one object; the unification rule it tests retires |

## 4. Findings the draft did not have

### 4.1 Detaching a map for release outside the hold (D1)

Firn releases the old keyspace after the statement so that no command waits
for the free of a large map; `tests/programs/map_resets.wf` does the same.
Under O a map is never a local and no statement holds a second object, so
neither the fresh-object exchange nor a local exchange is available. The
repair that keeps O's rules is one prelude operation,
`map_detach(map: &ConcurrentHashMap<V>) -> Shared<ConcurrentHashMap<V>>`,
row `writes(map)`: it moves the map's entries into a new shared object (an
exchange of table pointers with a fresh empty table) and returns its only
handle, so dropping that handle after the statement releases the old entries
outside the hold. It holds no second object and needs no identity. `Shards`
needs the same operation if it is ever cleared in bulk. Without it, criterion
4 of ONE-OBJECT.md fails for FLUSHALL on a large keyspace (unmeasured, but
the release is linear in the number of entries and runs under the hold).

### 4.2 What a guard's atomic load proves inside the block (open choice 1)

ONE-OBJECT.md 2.4 places a guard's held reads at the statement's point and its
loads of atomic values right after it. The guard is then true in that
combined observation, but the loaded value can change at any later instant,
while the held state cannot. The rule must say that the block gets the
guard's held conjuncts as facts at its entry and no fact from a loaded
conjunct (the decision cell already says this of an `Open` observation). For
the repository's guards (D2, D3, Firn's `refresh_idle_limit`) nothing is lost:
the flag only goes from false to true and the block re-tests nothing about
it. A guard over an atomic value that can go back (`count == 0`) is admitted
and means "was 0 when the statement took effect", which the writer must know;
a monotonicity judgment per type would be new machinery and is not needed by
any program.

### 4.3 Type invariants cannot read atomic values

The first draft made cells fields of a shared state; a type invariant reading
one would have been false at a block's entry whenever another context changed
the word, and would have needed an exclusion rule. The revised form (an
atomic value is its own object, reached only by operations through its handle)
leaves no path from a state into an atomic value's content, so TYPE-11's
binder data (projections that end at integers in the state) cannot reach one.
No rule is needed; this was checked against TYPE-11's definition of a binder
datum.

### 4.4 Linear-state objects need two more rules (class 4)

ONE-OBJECT.md 2.6 proposes that a shared object whose state is linear has
linear handles, each closed by `shared_close(h) -> Option<T>`, the last closer
receiving the state. Two holes:

- **Cycles.** A linear handle stored in its own object's state (directly or
  through another linear-state object) keeps the count above zero forever, so
  the state, and the file it holds, is never closed. Today no nodrop value can
  be lost this way, because `Box` is unique. Rule: the state type of a
  linear-state object has no part that is a handle to a linear-state object.
- **Maps of linear values.** `map_clear`, `map_detach` and a map's release
  drop every entry; `ConcurrentHashMap<V>` keeps its `V: drop` bound.

The lease study (card `lease-direction`) rejected "the last handle takes the
state back" as the design for Firn's AOF lease, because it cannot say whether
a lease is still out or who must return it. That objection stands: a context
that takes `Some(file)` out of the object in one statement is obliged by
linearity to consume the file, not to put it back. The linear-state object
therefore meets the board's class-4 example (`park` accepted, the last handle
obliged to take the file back) and is the substrate either lease option
needs (the owner's reclaim right is the last handle), but the lease's return
obligation remains the lease card's design. This is the same under every
candidate.

### 4.5 Inconsistencies in ONE-OBJECT.md

Its section 5 says "frozen parts carry the invariants established at
construction" although section 2.3 drops frozen parts; it should say that a
`Frozen<T>` object's value carries them from `frozen_new` (#484). Its section
7 lists only removals; section 5 below counts the additions too. (Left for the
session that owns that record, which was editing it while this study ran.)

### 4.6 What O reverses

- The owner's approval of multi-object atomic statements (#231, 2026-10-05).
- The design-tree decision (language/waiting/shared-objects, revised by #364)
  that rejected "general atomic fields and general lock-free cells, which
  expose interleavings inside an atomic step". `Atomic<T>` exposes them: a
  program can load, decide and store and lose a race, a race condition and
  not a data race. ONE-OBJECT.md lists this reversal as its open choice 3;
  it needs the owner's approval with the design-tree change.
- The board item's acceptance step, written for M: under O the first example
  cannot be accepted as written (two handles in one statement). Its O form is
  the `dbs` array rewrite of A1, and the copy-handle variant becomes the same
  index used twice.

## 5. Cost, counted from the rule text

Normative paragraphs changed, estimated by reading v0.151; the binding count
comes with the amendment (ONE-OBJECT.md criterion 3).

**O removes (12):** SHARE-2's overlap paragraph for several targets, its
two-handles-one-object and unification paragraph, its entry and key-set
target rows and the `Entries` binding fact (the subscript paths of the
existing `t^[k]` paragraph replace them), and "a statement holds whole every
map a call reaches" (a lowering matter once the block has the whole state);
the cell-free boundary definition in SHARE-3 and its users in SHARE-2's guard
rule, PAR-1, PAR-2, EFF-3 and CAP-1 (each replaced by a citation of the
marker); the cancellation-state update as a kind of shared effect (it becomes
a store to an `Atomic<Bool>`). The decision cell's entries in TYPE-2, PROV-6,
STOR-3, OP-9 and SHARE-1 are renamed to `Atomic<T>`, not removed.

**O adds (7, one of them a changed rule):** the one-target rule (SHARE-2); the new homes of
maps and shards (TYPE-9, changed); the `Atomic<T>` section with its operation
table and own points (generalized from the decision cell); the ordering rule
(every lock before the first operation on an atomic value); guards that load
atomic values (4.2); the shared-effect marker (EFF); `map_detach` (4.1). The
key-disequality fact (open choice 2a) would be an eighth.

**Common to every candidate except P:** the descriptor table (class 2), #484's
sealed values (class 3), linear-state objects with 4.4's two rules (class 4).

**S and M add, beyond the common part:** identity terms (fresh at creation,
kept by share, read conversion, move and field storage), a `distinct`
predicate in invariants and `use` steps, identity facts' transport and
invalidation through fields, calls and joins, SHARE-2's comparison by identity
facts, and, for B2, either a relation between a selection and its number or
quantified distinctness over an array of handles, neither of which the
handle-distinctness study wrote down. They remove only the unification
paragraph, and keep the multi-object lock order, duplicate merging, #441's
partition check and the 13 decision-cell rules.

**Checker and runtime under O:** the multi-target comparison, #441's
partition walk, lazy type groups, the run-time sort and duplicate merge of
objects of one type retire; lowering gains the lock-unit computation over the
whole program, field-order locking, per-unit watches, padding of lock words
and cells to their own lines, and the entry inference of ONE-OBJECT.md 2.1.
About half of the 67 conformance files with a multi-target statement retire
and the rest are rewritten; the 19 `range-atomic-alias` cases of #441 retire.
Firn's 62 statements on `main` become one-handle statements on one `Store`;
the `select-db` branch loses its 16-parameter helpers.

## 6. The atomic-cell card

Card `atomic-cells-into-model` asks whether #403's atomic value and the
merged decision cell become one "atomic cell" inside the unified design
(recommended A) or #403 merges first (B). Under O both are `Atomic<T>`, one
one-word object type with one operation table and one ordering rule
(ONE-OBJECT.md 2.3 and its section 11, which compares it with every open
shared-word request), and the 13 decision-cell rule changes collapse as in
section 3C. The study therefore supports the card's recommendation A, with
one refinement of form: the atomic value is a type of its own (a one-word
shared object), not a column of an object description, and approving A also
approves reversing the design tree's rejection of general atomic fields
(4.6). It does not change the recommendation.

## 7. Criteria

### DESIGN.md's criteria for M (and S)

1. Board examples: A1 depends on a `distinct` fact crossing a struct and a
   call; A3 and A4 are not derived from identity; A5 is not derived. **Fails.**
2. Firn sites: B2 is rejected in the first version. **Fails.**
3. No name list in the model's rules: the descriptor removes them for
   class 2. Passes.
4. Derivation fixed and terminating: the pairwise facts are finite (120 for
   16 databases); quantified distinctness for B2 is not designed, so this is
   **unsettled**.
5. Out-of-board programs: no wrong verdict (D1 to D7 accepted as by P).
   Passes.

Rejection conditions: class 5 cannot be expressed without a new effect system
(M narrows to S plus classes 3 and 4 by their own designs), and the rules
changed are not fewer than the per-class patches would change (S and M add
identity machinery and remove one paragraph). **M is rejected; S, which is M
narrowed, still fails criteria 1 and 2 on class 1.**

### ONE-OBJECT.md's criteria for O

1. Every program of its section 6 and the five board examples written under
   the model with the expected verdicts: **passes by reading**, with A1 in its
   rewritten form (4.6), D1 only with `map_detach` (4.1), D2 and D3 only with
   guards that load atomic values (4.2), B3 with either form of open
   choice 2.
2. No rule mentions a handle comparison, an identity or a list of shared type
   names: passes; the descriptor carries class 2.
3. Removed rules outnumber added rules: **passes by estimate** (12 against 7,
   section 5); binding only with the amendment.
4. Performance on the 14900K within twin noise: **not measured**; needs the
   implementation.

Revisit conditions: no case needs a second held object once `map_detach`
exists (D1 came closest); lowering's ability to meet R1 to R3 without a shared
write on the statement path is argued, not shown.

## 8. Measurements: today's verdicts (candidate P)

Run 38539581494 of the temporary workflow `tmp-shared-object-model`, hosted
Linux, compiler built from this branch at `2e0e96a7f` (main `6878a970a` plus
research files only). Each probe is a whole program in
`research/experiments/shared-object-model/probes/`; `expected.tsv` holds the
expectation written before the run.

| probe | what it is | expected | observed |
|---|---|---|---|
| p1-one-object-fields | the conformance case `share-neg-targets-same-type-one-call` rewritten with the two counters as two fields of one state | accept | accept |
| p1-maps-as-fields | two maps as an array field of one state, the shape O needs for A1 and B1 | reject, TYPE-9 | reject, TYPE-9 (invalid restricted type placement) |
| p3-local-control | `remaining(w)` subtracts `w.len - w.next` under the invariant `w.next <= w.len`, called on a local | accept | accept |
| p3-held-reader | the same call on the whole state held by an atomic statement | accept | accept |
| p3-frozen-reader | the board's `reader`: the same call on `&f^.inner` of a `Frozen<Window>` | reject (board prediction) | reject, FN-8 at the call (the parameter's type invariant is not discharged) |
| p3-entry-reader | the same call on the `Some` payload of a map entry | reject (board prediction) | reject, FN-8 at the call |
| p4-park | a nodrop value parked in `Shared<Option<T>>` | reject | reject, PROV-6 (linearity bound mismatch at `shared_new`) |
| p5-bump | the board's `bump`, `pure waits` with an atomic statement | accept | accept |

What this establishes for P: the one-object form of class 1 is already
accepted when the parts are ordinary fields (p1), and is refused only because
maps may not be fields (p1-maps); class 3 holds for a held whole state and
fails for frozen values and map entries exactly as the board predicted, so
#484 (or O's reading of it) is needed for A3; class 4 is refused by the drop
bound; class 5's example is accepted as a waiting function, which EFF-3
already excludes from merging. The other P verdicts in section 3 rest on
existing conformance cases (`share-neg-targets-same-type-one-call`,
`fn10-neg-key-set-release`) and on Firn's run 38416798793.

## 9. Result

**O survives**, as the combination of section 1: one object per atomic
statement, #478's descriptor without identity, #484's sealed values, the
linear-state object with section 4.4's two rules, the shared-effect marker,
`Atomic<T>` folding the decision cell and #403, and `map_detach`.
It is the only candidate that decides class 1's Firn cases (B1, B2) without
new proof machinery, it removes more rules than it adds, and it removes the
lock-order, merge and partition machinery from the checker and the runtime.
Its price is that unrelated objects can never be combined atomically: they
must be designed as parts of one object (D6 and D7 lose their purpose; D1
needs a new operation).

**M is rejected** (and S with it, being M without classes 3 to 5): identity
does not decide Firn's selection by database number (B2) without a further
relation that no study has designed, it adds rules instead of removing them,
it keeps every multi-object mechanism, and it does not derive classes 3 to 5.

**P is rejected**: class 1 has no static argument at all for objects of one
type, and class 2 remains a type-name list.

**Not settled here:** the performance criterion (needs implementation and the
14900K); the exact rule count (needs the amendment); the meaning of guards
over atomic values that can go back (4.2 proposes the plain rule); whether open choice
2 takes a new fact kind; how the linear-state object and the lease design
compose in detail (card `lease-direction`); and whether the second Firn
witness of `firn-req-distinct-shared-maps` (two references exchanged under a
condition, losing "always different") still matters once B2 uses indices.
