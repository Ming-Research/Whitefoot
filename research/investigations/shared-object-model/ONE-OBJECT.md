# One object per atomic statement

## Status

A design study written 2026-10-13/14 before any implementation. Nothing in it
was compiled or run. Rules are cited by ID from the active specification
(`spec/kernel-spec.md`, v0.146). Firn sites are cited from Firn-wf `main`
(`55a8495`) and its branch `claude/select-db`. Every performance statement is
a deduction from the runtime design in `design/compiler/waiting-contexts/` and
the measurements cited on the board; none was measured for this study.

It replaces the direction of [DESIGN.md](DESIGN.md) (description plus a
proof-only handle identity). The owner, in the session of 2026-10-13, after a
discussion of why handle identity was needed at all (translated):

> When we define a shared object we implicitly promise that the process inside
> it is atomic: from beginning to end nobody else interferes. Only then is it
> transparent to the checker. Two objects of different types in one `atomic`
> only appear to work: we have merged two atomic regions, and the safety rests
> on the fact that two types cannot name one thing, which is a coincidence.
> With two objects of one type the problem breaks out, not because they are of
> one type but because the checker's coincidental rule no longer hides it. The
> root of the problem is that we tried to invent a form in which several
> unrelated locked objects cross each other and share one atomic space.

and, on the direction below: if it solves the classes on the board item
without a large performance regression (a theoretical argument suffices for
now), it is the clean and simple solution wanted; adding syntax later for an
individual problem is preferable to the multi-object form.

## 1. The argument

**A one-object statement rests on one premise:** its guard and block have
exclusive access to the object's state from its first access to its last
[SHARE-3]. Inside the block the state is reached only through the statement's
binding [SHARE-1], so to the checker it behaves as a value the block owns for
the duration. Place-based facts, separation and invalidation [REF-1, OWN-7,
ENT-5] are sound without any knowledge of handles.

**A multi-object statement rests on a second premise:** its objects are
distinct, or the runtime merges them. That is a run-time fact. SHARE-2
approximates it statically: targets whose state types do not unify start at
two roots, targets whose types unify are compared as paths from one root. For
different types the argument is valid but borrowed: it also needs every state
to be reachable only through its own target, an invariant that any later
feature storing references would break. For one type there is no static
argument at all, which is the first class of the board item
`shared-object-model`.

Almost everything the multi-object form added exists to carry the second
premise or to manage several locks: the static order of types and the
run-time order of identities, merging duplicate objects into one hold,
"unifying types start at one root", the per-partition invariant check of
#441, lazy type groups, and the proposals to add a proof-only identity
(#478, branch `claude/shared-handle-distinctness`). A statement that holds one
object needs none of them, and statements that each hold one object can never
wait for each other in a cycle.

**Consequence: the atomic domain is the object.** Things that must change in
one atomic step are, by definition, one atomic domain, and therefore one
object. What made "put it in one object" look restrictive was conflating the
atomic domain (semantics, program structure) with the lock unit
(implementation, performance). The concurrent hash map already shows they are
separable: one object, an unbounded number of dynamically created parts, each
locked alone, with the checker comparing two entries as two subscripts of one
root.

## 2. The model

### 2.1 One handle per statement

An atomic statement names exactly one handle place. Its header binds the
object's state and may name selections from that state whose identity is a
run-time value (map entries, key-set entries, a shard), because those must be
known before any of them is locked.

    atomic s = &store, slot = &s^.keys[key] {
      ... slot^ ...         // the entry
      ... s^.meta ...       // a field: found by the compiler, no header item
    }

- `s` is a reference to the whole state. Every other header item is a place
  formed *from `s`*, so it is an ordinary path below one root. Two entries
  `&s^.dbs[i][k1]` and `&s^.dbs[j][k2]` are compared exactly as two
  subscripts are [OWN-7]: disjoint when `i != j` or the keys are proved
  different, otherwise possibly overlapping.
- The semantics is unchanged from SHARE-3: the block has the whole state at
  one point. What is actually locked is the implementation's choice, derived
  from the block's footprint (section 3), as SHARE-3's last sentence already
  permits.
- Header syntax is open (section 9.1); the form above is the one that needs
  the least new grammar, because `atomic x = &place` stays the only target
  form and the rest are ordinary reference bindings.

### 2.2 Where the special state kinds live

TYPE-9 gives each restricted type one home. The homes change from "the whole
state of an object" to "a part of a shared state":

| type | home today | home proposed |
|---|---|---|
| `ConcurrentHashMap<V>` | the whole state of a `Shared` | any field (at any depth, including a fixed array element) of a shared state; never a local, never moved in or out whole; exchanged only by `swap` |
| `Shards<T>` (#385) | the whole state | the same as the map |
| a cell (section 2.3) | `DecisionCell` handle; #403's `AtomicValue` state | a field of a shared state |

This keeps the owner's principle of 2026-10-05, "a concurrent hash table is
shared by nature; a type's behavior should not change with where it is put"
(design log, `ConcurrentHashMap` entry): a map exists only inside shared state
and behaves the same wherever it sits there. It also avoids what made the
earlier table-field design (#208) complex: header grants, row checks and
provenance sets existed because the owner had then made a table an ordinary
movable value (Q18), so the checker had to track which state a table belonged
to. A map that can only ever be a part of a shared state is reached only by a
path from that state's binding, so there is nothing to track.

### 2.3 Three kinds of part

A shared state's parts differ only in how they are accessed:

- **Held parts** (the default): reached only inside an atomic statement on
  the object, at the statement's one point.
- **Cell parts**: a word-sized value (an integer, `Bool` or a tag-only enum)
  operated on by one instant operation: load, store, exchange, add (wrapping
  or saturating), max, min, bitwise or/and, compare-and-exchange. An
  operation never waits, may be written anywhere a handle or a held binding
  reaches the cell, including inside another statement's block and in a
  function that does not wait, and is its own point in the one total order of
  shared effects [SHARE-3, WAIT-2]. Cells are explicitly *outside* every
  atomic domain: no statement's atomicity covers them, and their type says so.
- **Frozen parts** (optional, section 9.4): written only at construction,
  readable through any handle without a statement, as `Frozen<T>`'s `inner`
  is today [SHARE-1].

The decision cell (#364) is a cell of `DecisionState` with
compare-and-exchange from `Open`. #403's atomic value is a cell of `u64` with
add and max. A cancellation's `fired` is a cell of `Bool`. Each needs no rule
of its own: the 13 rules the decision cell changed and #403's "one operation
per path, outside loops, no alias" become the one cell rule.

One ordering rule ties cells to statements, generalized from #403's finding
that a sequence number taken at the statement's lock point orders statements
consistently with their effects: **inside an atomic statement, every lock the
statement takes is taken before its first cell operation.** The statement's
point can then be placed immediately before that operation (two-phase locking
makes any point between the last acquisition and the first release valid),
so a sequence number taken there orders statements by effect, and a decision
claimed there is visible to others before the long statement ends.

### 2.4 Crossing objects

The only interaction between two objects inside one statement is through
cells: a block may operate on another object's cell, because a cell holds
nothing and waits for nothing. Nothing else crosses: no statement holds a
second object.

The recurring cross-object need in the repository is a guard of the form
"work is available or this was cancelled" (`cancellation.wf`,
`cancel-guard-run-readiness`, Firn's `refresh_idle_limit`). Proposed: a guard
may load cells of any object. A cell write checks the cell's watcher word
(one load on a line it already owns) and, when watches exist, wakes them as a
write to a held part does today. A guard's held reads take effect at the
statement's point and its cell loads at their own points before it; since a
cancellation cell only goes from false to true, the guard that saw it true
remains true. Waiting on two independent objects' *held* state (a Go-style
select over two channels) is not expressible; section 8 gives the syntax that
would add it without merging atomic regions.

### 2.5 What the checker sees

One root per statement, with ordinary paths below it. Specifically:

- Separation between header selections and fields is OWN-7 on paths below one
  root. Map entries under byte keys need a key-disequality fact to be proved
  apart; key-set entries already compare by integer index (section 9.5).
- Facts about the state hold from the statement's entry to its exit and are
  killed at exit like the facts of any reference whose referent leaves scope
  [REF-2].
- Type invariants: the state's type invariants, including those relating two
  fields, are assumed at entry and owed at exit [TYPE-11], because
  semantically the block has the whole state. For the implementation to match,
  **a part counts as accessed when the block's proofs mention it, not only
  when its code reads it**, so a statement that relies on `meta.count ==
  keys.len` holds both. This also settles the board item about atomic
  bindings mentioned only in proofs (`firn-wf-atomic-proof-mentions`) as a
  consequence rather than an exception.
- The checker never compares handles, has no identity facts, no per-type
  overlap rule and no partition enumeration.

### 2.6 What stays outside this model

These classes of the board item are orthogonal to the number of objects per
statement and keep their own designs:

- **Release (class 2):** each runtime type declares its release action in one
  place (#478's description part, without its identity part).
- **Explicit-close state (class 4):** proposed shape: a shared object whose
  state is linear has linear handles, each closed by `shared_close(h) ->
  Option<T>`, the last closer receiving the state and its obligation. Firn's
  AOF file then lives in its own small object (only its handles are linear),
  taken out by exchanging `Some(file)` for `None` in a statement, written
  outside any statement and put back; waiters use a guard on `Some`. This
  needs checking against the reasons the lease study (card `lease-direction`)
  rejected "the last handle takes the state back" before it is proposed.
- **Effects (class 5):** the effect row gains one marker for "may perform
  shared effects" (statements, cell operations, cancellation firing);
  `waits` implies it. Merging and reordering rules and PAR-1/PAR-2 consult
  only it, and the declaration-derived "decision-cell-free boundary"
  [SHARE-3, PAR-1, PAR-2] retires. This is the one place where shared state
  is not transparent: memory safety does not need to know, but "may this call
  be reordered" does.

## 3. Lock units and lock order (lowering)

### 3.1 Units follow the declared structure

A lock unit is a part the declaration fixes as independent:

- each map is a family of units (its entries, and its whole gate), as today;
- each `Shards` is a family of units (its slots), as today;
- every other top-level field of the state is one unit;
- a fixed-length array field whose element type contains a map or shards is
  expanded element by element, recursively through the element's fields.

The writer controls granularity with the structure itself: fields that change
together go into one struct-typed field and are one unit; fields that change
apart are separate fields. This is the grain today's programs get from
separate objects, with no new syntax. Holding a subtree whole (a whole
`s^.dbs` for FLUSHALL) takes each unit inside it in order. No unit has an
"intention" counter on its ancestors, so no hold writes a line shared by
statements that touch different parts (section 4, R2).

### 3.2 One static order

Units are ordered by their position in the state's declaration (field order,
then array index, then, within a map, the sorted key order of today). A
statement takes each unit at its first use on the path, together with every
earlier unit it will need and has not taken, and takes header entries and
guard-read units before the guard, which are today's rules with field order in
place of the order of type names and with the run-time identity order gone.
The writer now chooses the order: a part needed only conditionally (Firn's
`meta` when logging) can be declared after the parts every command uses, the
case the atomic-targets study found today's type-name order cannot express
(its section 0, "a conditional target whose type precedes a used one").

### 3.3 Watches per unit

A guard registers its watch on the units it reads, not on the object, so a
write to a key's entry does not wake a guard waiting on `meta`. Today the
same separation comes from separate objects; keeping it is an obligation of
this design (R3).

## 4. Performance, argued

The comparison point is today's equivalent program: the same parts as
separate objects, the form Firn already uses.

**Per statement, Firn's SET with AOF on.** Today: read two handles from
`Keyspace` (no handle count since #409), take the `keys` entry (gate, cell),
take the `meta` object's lock, both ordered by type at compile time. Proposed:
read one handle, take the `keys` part's entry, take the `meta` unit's lock,
ordered by field position at compile time. The atomic operations and the
cache lines written are the same; one handle read fewer.

**Regression risks and what removes each:**

- **R1 false sharing.** Parts of one object sit in one allocation; two units'
  lock words on one cache line would bounce between cores that touch
  different parts. Lowering pads each unit's lock word, and each cell, to its
  own line (128 bytes on cores that fetch line pairs). Cost: one or two lines
  per unit, negligible beside a map.
- **R2 coarse units or ancestor counters.** If all scalar fields shared one
  lock, or if every statement registered on an object-wide gate, unrelated
  statements would meet. Section 3.1's unit rule and the absence of
  intention counters prevent both. The object header (count, descriptor) is
  read-only on the statement path.
- **R3 spurious wakes.** Watches per object would wake every guard on any
  write. Section 3.3 keeps them per unit, matching today's separate objects.
- **R4 a callee given the whole state.** A call whose row reaches `s` whole
  reaches every map in it, and a map reached by a call is held whole; today
  the same is true of a map reference passed to a call [SHARE-2]. Writers
  pass the part (`&s^.meta`), as Firn passes `meta` today; `--lock-ledger`
  reports a whole hold.
- **R5 whole-subtree holds.** FLUSHALL, INFO and transactions over all
  databases take 16 map gates and `meta`, the same as the 17 targets of the
  `select-db` branch.

**Expected improvements:**

- no run-time sort or duplicate merge of objects of one type;
- one handle per context instead of four in Firn (`keys`, `meta`, `server`,
  `scripts`), so spawning a connection retains one object;
- the lock order is chosen by the writer (section 3.2), so conditional parts
  cost nothing when unused regardless of their type;
- cell operations replace exclusive statements for counters, flags and
  sequences: on 8 P-cores a hardware add on one word ran 118 M/s against
  2.81 M/s for an exclusive statement on one object (board item
  `firn-req-shared-max`, run 38304337014), with shards still the answer for
  counters that must scale further.

**Conclusion:** by deduction, no statement that today touches a set of
separate objects costs more when those objects are parts of one, provided
R1 to R3 are met in lowering; several paths become cheaper. The deduction
fails, and the design must be revisited, if a measured statement on parts is
slower than the same statement on separate objects beyond twin noise
(section 10).

## 5. The board's classes and requests under the model

- **Identity (class 1).** Firn's multi-database calls become two subscripts
  of `s^.dbs`; `clear_both` receives `&s^.dbs[i]` and `&s^.dbs[j]` after
  `i != j`. Two keys of one table are two subscripts of one map (section
  9.5). Capacities `n+1` and `n+2` no longer arise: there are no two objects
  to compare. Items `firn-req-distinct-shared-maps`, `atomic-binder-distinct-keys`
  and `coord-wfbl-04-24` close by this design or reduce to the key fact.
- **Release (class 2):** unchanged, section 2.6.
- **Invariants (class 3):** for held parts, section 2.5; frozen parts carry
  the invariants established at construction; map entries and shard slots
  follow the sealed-value rule of #484.
- **Explicit-close values (class 4):** section 2.6.
- **Effects (class 5):** section 2.6.
- **Global sequence (`firn-req-sequence-take`):** a `u64` cell in the store,
  added inside the key's statement after its locks (section 2.3). #403's
  `AtomicValue` type and its special rules are not needed.
- **Decision verdict (`firn-req-attempt-publish`):** a `DecisionState` cell.
- **Read holds (`gran-read-hold-read-only`):** per unit, derived from the
  footprint; no language change.
- **Shards (`firn-req-shared-max`, `firn-req-shards-owned`):** a `Shards`
  field; Firn's "lock a key and my shard" is `slot = &s^.keys[k]`, `part =
  &s^.parts[mine]` in one header.
- **Deadline (`firn-req-wait-key-or-flag`):** `until d { } else { }`
  unchanged, and simpler to withdraw: a pending statement's partial holds are
  units of one object, released in reverse order.
- **Spawn in a block (`firn-req-timer-from-atomic`):** unchanged.
- **Cancellation guards:** section 2.4.

## 6. Firn under the model

All 62 multi-target statements on Firn `main` name parts of one `Keyspace`
value whose fields are four handles (`keys`, `meta`, `server`, `scripts`) and
a flag; one (`refresh_idle_limit`, `firn/server/server.wf:253`) adds a
cancellation view, read for its `fired` flag. The keyspace becomes one object:

    struct Store {
      keys: ConcurrentHashMap<Entry>;
      meta: Meta;
      server: ServerState;
      scripts: EnginePool;
    }

    // before (firn/commands/strings.wf and 40 others)
    atomic slot = &store^.keys[key], m = &store^.meta { ... }
    // after
    atomic s = &store, slot = &s^.keys[key] { ... s^.meta ... }

The `select-db` branch is where the multi-object form hurts most and where one
object helps most. Today it names sixteen databases as sixteen targets
(`firn/commands/transaction.wf:249`, `scripting/entry.wf:489`,
`commands/info.wf:485`, `commands/eviction.wf:495`, `commands/databases.wf:522`)
and passes them to helpers with sixteen reference parameters
(`held_databases_run`). With `dbs: Array<ConcurrentHashMap<Entry>, 16>` the
header is `atomic s = &store` and the helper takes `&s^.dbs`. MOVE is

    atomic s = &store, source = &s^.dbs[from][key], target = &s^.dbs[into][key] { ... }

after Redis's own refusal of `from == into`, and SWAPDB is
`swap(first: &s^.dbs[left], second: &s^.dbs[right])` under `left != right`,
the branch Firn already writes.

## 7. What the model removes

From the specification: SHARE-2's paragraph on two handles naming one object
and on unifying state types; "a whole target alone on its handle place"; the
decision-cell rules and the decision-cell-free boundary; #403's atomic-value
rules (not merged). From the design tree and runtime: the type order and
identity order of locks, duplicate-object merging, per-type lock groups
(replaced by per-field order). From the research lines: the identity part of
#478, the handle-distinctness study, the #441 partition check. From the
conformance suite: the cases that test those rules (about half of the 67
files with a multi-target statement; the rest are rewritten into one object).

## 8. What the model gives up, and syntax that could restore it

- **Waiting on the held state of several independent objects** (select over
  two channels). If a program needs it: a `select` of several one-object
  statements, each with its guard; the runtime registers each guard and runs
  the first that becomes ready. Only one alternative executes, holding one
  object, so no atomic regions merge. No current program needs it (the
  deadline card `wait-key-or-cancel-form` reached the same conclusion for its
  option C, a general select).
- **Holding an object found inside held state** (the atomic-targets study's
  option B). Under this model the handle is copied out and used in a second
  statement. No current program needs it.
- **Atomically combining two objects created separately by unrelated code.**
  They must be designed as parts of one object. This is the price, and the
  point: the atomic domain is declared in the types.

## 9. Open choices

1. **Header syntax.** (a) `atomic s = &store, slot = &s^.keys[key]` (above);
   (b) selections written relative to the handle, `atomic s = &store
   [keys[key]]`; (c) entries named inside the block with a `hold` form. (a)
   adds least grammar.
2. **Lock-unit rule.** Section 3.1's default (top-level fields, maps, shards,
   arrays of independent parts) or a finer one (every field). Finer units
   cost more acquisitions per statement.
3. **Guards over cells.** Allow (section 2.4) or express cancellation only
   through `until` and a cancellation clause on the statement.
4. **Frozen parts.** Whether `Frozen<T>` stays a separate kind or becomes a
   part kind of any shared state.
5. **Two keys of one table.** Either a prelude byte comparison whose `false`
   result is a key-disequality fact for OWN-7, or the existing key-set form,
   whose entries compare by integer index. The first is one new fact kind,
   general rather than shared-specific; the second needs nothing new and is
   what Firn's RENAME already does.

## 10. Criteria and check plan, fixed before work

**Adopt** if all hold:

1. Every program in section 6 and the five board examples are written under
   the model with the expected verdicts, by reading the proposed rules.
2. No rule of the model mentions a handle comparison, an identity, or a list
   of shared type names.
3. The specification rules removed outnumber the rules added (counted rule by
   rule in the amendment).
4. After implementation, on the 14900K's 8 P-cores, interleaved with twins:
   Firn's SET, GET and MSET with AOF off and on are within twin noise of
   today's multi-object build; a guard on `meta` is woken no more often than
   today under a key-writing load (R3); a shard update stays at or above 0.5
   of the C per-thread line (the `firn-req-shared-max` criterion).

**Revisit** if any holds: a case in section 5 needs a second held object;
lowering cannot meet R1 to R3 without a shared write on the statement path;
or criterion 4 fails beyond twin noise.

**Check plan.** Conformance cases: one handle per statement (positive) and two
handles (negative, citing SHARE-2); two map entries proved apart by index and
by key fact; a field and an entry in one statement; a cross-field type
invariant held by a statement that mentions only one field in code (it must
hold both); a cell operated inside another object's statement; a guard over a
cell woken by a store; a statement whose cell operation precedes a later
lazily taken unit (the lowering must take the unit first). Programs: Firn's
SET shape, MOVE and SWAPDB over `dbs`, the cancellation guard. Measurements as
in criterion 4.
