# Sealed type invariants

Status: direction ruled by the owner on 2026-10-13 (board card on where a
type invariant is kept once a value is stored, option A′ "sealed value
valid"). This note is the soundness argument written before the
specification change and the implementation. Nothing in it has been compiled
or run; every verdict below is derived from the rules stated here and the
active specification (v0.150), not observed.

As built in the specification amendment "affine type invariants hold of
sealed values in fields, payloads, boxes and storage elements" of
2026-10-22 (`spec/log.md`): range type invariants stay out of sealing and
keep their function-boundary sites [TYPE-11], so T8 and T13 apply to
affine invariants only, and the range walk takes the affine instances that
body entry establishes as path conditions [RANGE-2], as it took the
generated requirements before; the mentions of range invariants in T8, T13,
result 3 and the specification table below describe the proposal. The
cursor refinement, point 4 of the points for the owner, is in: a debt
written through a reference that resolves to several members is restored
over that reference's own terms while it is not rebound [TYPE-11.S5], and a
whole store of a sealed value through that reference removes the debts it
recorded at or below the stored place, and no other [TYPE-11.S3]. A
debt at an unknown position is satisfied at a hand-off that fixes its
position by the instance there, and stays for the other positions
[TYPE-11.S5], so an element checked before it is handed on needs no
restoration of a sibling whose position was lost. An atomic target
provisionally starts every value it owns in debt [TYPE-11.S2], instead of
sealed as the atomic-statement argument of result 1 assumes, pending the
shared-object model; by the same rule one checked element of its state can
be handed on. By the same provisional rule the state the last `shared_close`
returns keeps only its own invariants: every struct value it owns starts in
debt at the binding the result continues into, and a hand-off of the unbound
result fails while that state may own one [TYPE-11.S2, TYPE-11.S4]; the
supplied results the specification table below calls sealed exclude it,
and sealing, which that table calls type-based, also reads the identities of
four prelude functions, at a concrete instance those of a function-kind
parameter's actual ([below](#prelude-host-and-module-boundaries)). A hand-off of a whole
container still needs every element restored, as result 2 states. A map and an `Entries` own none of their
entries: a hand-off of one requires no debt below an entry, so a body owes
every entry it reaches through a reference parameter or an atomic target
until it proves it (T1, T5).

The design study that preceded the ruling compared five candidates and named
three points a soundness argument must settle before implementation:
restoration obligations across reference joins, a finite representation of
container evidence, and a polynomial bound on the added checking work. This
note settles them, the third as a bound polynomial in the body's size and
in the number of debts an atomic block's entry and a map or `Entries`
parameter's body entry start with ([result
3](#result-3-the-bound-on-the-added-work)), states the rule precisely
enough to implement and review,
and lists the specification rules and conformance cases the change needs.

## Question

[TYPE-11] makes a struct's type invariant a requirement and a postcondition
at a function boundary only where the parameter or result *is* the struct (or
an own enum whose variant field is the struct). Everywhere else the
invariant is lost: a struct field, an element of a storage shape, a `Box`
content, a payload of a nested enum, a map entry. Firn's witness shows the
smallest form:

```text
struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct Holder {
  index: Index;
}

fn load(index: &Index, table: &Array<u64, 16>) -> result: u64 reads(index.value), reads(table) {
  let at = index^.value;
  return table^[at];
}

fn stored(holder: &Holder, table: &Array<u64, 16>) -> result: u64 reads(holder.index.value), reads(table) {
  return load(index: &holder^.index, table: table);
}
```

Today `stored`'s call is refused (FN-8): nothing established
`holder^.index.value < 16_u64` at its entry, because `Holder` declares no
invariant and TYPE-11 does not look inside its fields. The accepted
workaround restates the relation on every enclosing type, and does not
reach containers at all; Firn's select-database code therefore masks the
index with `iand(database, 15_u64)`, which computes a different value instead
of proving the stored one valid.

Any rule that makes the invariant available at `holder^.index` must still
refuse this program shape (the counterexample the ruling names):

```text
set holder.index.value = 31_u64;
let at = holder.index.value;      // must not regain `at < 16` from the type Index
let got = table[at];              // must be refused
```

The question: can a struct's invariant travel with its value into every
owned position and be assumed where the value is read, while a function or
an atomic block may still break it temporarily, without any path on which a
broken value is read as valid, with a finite checker state and polynomial
checking work, and without runtime checks?

## The ruled rule

Option A′ as the owner ruled it:

1. A struct's type invariants travel with its value into fields, `Option`
   and enum payloads, storage shapes, `Box`, shared objects and map entries.
   They belong to the implicit requirement and postcondition of every
   hand-off of a value that owns the struct value, so a body takes each
   instance it uses from the hand-off that delivered the value (body entry,
   a call's return, a construction, an atomic block's entry), never from
   the read alone: the read only retrieves what that hand-off established,
   provided no write since then may have broken it.
2. Inside a function body or an atomic block a value may be temporarily
   broken. A write that can falsify an invariant kills the facts that
   depend on it at once.
3. Before the value is handed off (passed as an argument, returned, given,
   stored elsewhere, or left by an atomic block) its restoration must be
   proved; otherwise the program is rejected at that hand-off, naming the
   breaking write.
4. Existing stepwise updates of several fields stay legal.
5. A generic struct may declare invariants over its own fields.
6. No runtime check is added; every proof is erased before lowering.

"Sealed" names the proof state, not the storage: a sealed place holds a
value of which every invariant of every struct it owns holds. A′ does not
claim that every intermediate store state satisfies the invariant; it claims
that every observation that receives an invariant fact, and every hand-off,
touches only sealed values.

## Alternatives and prior objections

The study's candidates, kept here so the reasons survive:

| candidate | rule | why not selected |
|---|---|---|
| A, strict | every write proves every affected invariant at once | refuses the stepwise multi-field update the design tree keeps (a relation between two fields cannot hold between their two writes) |
| A′, selected | sealed values; breaking writes create obligations owed at the next hand-off | — |
| B, per-slot refinement | every storage slot carries validity evidence | when complete it is A or A′ described as storage; when it checks only whole store and load it misses a subfield write through an alias and is unsound |
| C, opt-in per field or container | an annotation marks which slots keep the invariant | the same machinery as A′ plus annotation, compatibility and migration rules; unannotated slots still lose the fact |
| D, explicit proof form | a written step restores the fact at a read | a proof can only consume evidence that some rule maintained; maintaining it is B again, and without it the step cannot be sound |

Objections already recorded in the design tree, and how A′ meets them:

- *"An invariant owed at every construction and field write" was rejected*
  because a relation between two fields cannot be kept while they are
  written one at a time. A′ owes it only at hand-offs; between two writes
  the value is merely broken.
- *A supplied definition promises no generated postcondition*, because
  `take_back` would otherwise hand out an element no write checked. Under A′
  every write into storage is checked (a store is a hand-off; an in-place
  write leaves an obligation that blocks every later hand-off of the
  container, including the hand-off to `take_back` itself). The prelude's
  rows then return sealed values by parametricity
  ([prelude and host](#prelude-host-and-module-boundaries)), not by a
  trusted postcondition.
- *The enum-payload rule was limited to an own enum's direct struct field*
  because extending it "needs a semantic rule of its own: which writes owe
  the invariant and which reads receive it". This note is that rule.
- *Establishing an enum parameter's routed invariant at entry as an
  ordinary fact* was rejected because an unsatisfiable invariant over a
  variant the value does not hold would make the entry contradictory. A′
  keeps that boundary: an invariant instance below a payload step is
  established only where a current refinement fact [ENT-3.S15] says the
  enum holds that variant, the same condition under which the payload is a
  place at all [REF-1].
- *Type invariants relating a value to another value* stay rejected; a
  generic struct's invariant reads only its own fields
  ([generic structs](#generic-structs)).

## The model

### Validity

Write `Valid(v)` for a value v of type T: every struct value that v owns,
at any depth, satisfies every type invariant of its struct. "Owns" is
[PROV-6]'s ownership closure restricted to parts that exist: struct fields,
`Box` content, the payload of the variant the enum holds (never another
variant's), every element of an `Array` and of a `Segments`, every slot
inside a window [WIN-1] (never a free slot), and a shared or frozen
object's state. A handle owns no state [PROV-6]; the state's validity is
kept by the atomic rules below, not by the handle's. A map and an
`Entries` own none of their entries, whose validity no hand-off delivers
(T1, T5).

Validity is a judgment about runtime values used only in this argument.
The checker never represents `Valid(v)` for an individual value; it
represents which places are *not known* valid.

### Anchors and read sets

An **anchor** is a resolved place [REF-1] whose type, after the instance's
substitution [FN-2], is a source struct with at least one type invariant.
For an invariant I of that struct, its **read set** `Rd(I)` is the finite
set of relative paths I reads below its binder:

- an affine invariant reads its binder data [TYPE-11]: each fragment-integer
  place, and for a measure member the supporting descriptor word [MSR-2];
- a range invariant reads [EFF-5]'s contract-read footprint of its clause,
  with the binder as the parameter: ranges, guards and conclusions, every
  subscript position widened to the whole run.

`a.Rd(I)` is the read set appended to the anchor.

### Debts

The checker state gains a finite set D of **debts**. A debt is a key
`(a, I)` — an anchor shape and one invariant of its type — together with
the set of **breaking events** that created it (statements or calls), kept
only for the diagnostic. An anchor shape is a resolved place in which an
index, range or page position is either a captured offset identity, as REF-1
records one, or the unknown position `[?]`, which overlaps every position of
its run; a debt may also be anchored at a REF-1 cover `R.**`, standing for
every anchor at or below R and every invariant of each.

A place p is **sealed** at a point when no debt in D may name an anchor at
or below p: none whose anchor is p or a descendant of p, judged by [OWN-7]
with an undischarged separation counting as overlap, and none anchored at a
cover that overlaps p. Debts anchored strictly above p do not affect p: a
broken cross-field relation of `Holder` says nothing about the `Index` it
owns.

"At or below" and "the same anchor" compare **levels**, not step counts. A
place's level is the number of values its steps select below its root: a
field, payload or `Box` content step and a subscript of a value's element
each add one; a range step, a page step, a subscript of a `Segments` and a
window part select a run of elements, which is no value, and add none; the
root of a range or run parameter is a run. So `cells[0..4][1]` and
`cells[1]` both have level 1, as [REF-4] makes them the same element; a
step count would put them at two depths and let a debt at one spelling miss
a read or a hand-off at the other. A cover step `**` stands for any number
of levels unless both places pass through the same one.

**Rebasing** a debt that may be at or below a value place p onto a place q
keeps the anchor's steps below the value at p's level and puts q's steps
above them: below p's own steps when the anchor begins with them, and
otherwise below the anchor's shortest prefix that selects a value at p's
level. When the anchor has no such prefix free of cover steps, or its steps
below it pass a cover step (a reference's current target identity, which no
reference to q carries), the rebased debt is the cover of q, or of q
followed by the steps before that cover step.

### The sealing invariant

The argument rests on one invariant of the abstract state, written SI:

> At every point of a checked body and in every runtime state that reaches
> it, for every struct place π that exists in that state and is reached from
> a root of the activation (a local binding, a parameter's referent, or,
> inside an atomic block, a target's state) and every type invariant I of
> π's type: if no debt in D may denote (π, I), then I holds of the value at
> π.

Consequence: a sealed place holds a valid value. Every rule below either
preserves SI or rejects the program.

**SI as a chain of hand-offs.** Proof is continuous: a fact is used only
after a hand-off proved it, as for any requirement and postcondition. A
struct's invariants are the implicit requirement and postcondition of every
hand-off of a value that owns the struct value: the caller proves them
before a call (T5) and the body receives them at entry (T1); the callee
proves them at each exit (T7) and the caller receives them when the call
returns (T6); a construction proves them over its operands and its result
is received valid; a store into any place but a complete own local binding
is itself a hand-off (T5), proved before the store. Between two hand-offs
the body runs the ordinary proof rules, and a write that may falsify an
instance kills the facts over it and records the debt that names the write.
SI therefore says: for a place no debt may denote, the fact the delivering
hand-off established has not been touched since. T13 adds no assumption of
its own: it retrieves that hand-off's fact at the read, and Result 1 is the
induction that every hand-off in the chain was proved.

### Transfer rules

T1 **Body entry.** D is the union of the unfoldings [TYPE-11.S2] below
the referents of the reference parameters of type `&ConcurrentHashMap<V>`
or `&Entries<V>`: a debt for each invariant of each struct value an entry
owns or is, `[?]` at every index position, and the cover of a
value whose type is that of a value above it on its path, so the set is
finite. Every parameter value
and every reference parameter's referent is sealed, because each caller
proved it at the call (T5); the build establishes the same for the entry's
arguments [PROG-3]. That proof covers no entry of a map or an `Entries`
(T5), so a body that reaches one through such a parameter owes it until it
proves it. A map is only ever a shared state [SHARE-1], reached through an
atomic target or a reference passed from inside an atomic block, so these
parameters and the atomic entry are the only ways a body reaches an entry.

T2 **Write.** For a write event with resolved write place w — a [SET-1]
commit, each of a `swap`'s two targets [OP-11], an [OP-12] update target,
and each projected `writes` path of a call [EFF-5] — and for every anchor a
that is a strict prefix of w, or that w's cover may reach, and every
invariant I of a's type with `a.Rd(I)` overlapping w [OWN-7]: add the debt
`(a, I)` with this event. When w passes through a cover `R.**`, the anchors at and
below the reference's current target are named through that target's
identity [REF-1], R itself included when it is the target; an anchor the
cover hides strictly above the target, R included otherwise, reads w only
through a value of the target's referent type, so the debt for the hidden
anchors is the cover itself, added only when an invariant of R or of a
struct value R owns reads through that type. A write through a cursor over a list
or tree whose invariants read only their own node's fields therefore adds
no cover, and point 4 under the owner's points restores it through the
cursor. A `grow` or `grow_paged` relocation write applies [ENT-5]'s
relocation table: it adds a debt only where `a.Rd(I)` overlaps storage that
does not survive the relocation (a capacity word, not the length or filled
elements) [OP-10]. Anchors at or below w are handled by the rule of the event (T3,
T6, T10).

T3 **Whole-value store of a sealed value.** T3 applies to a SET-1 commit or
an OP-12 update at w exactly when the stored value is sealed: its right-hand
side is a hand-off (T5: every store whose target is not a complete own local
binding, and every OP-12 update), a call result (T6) or a construction.
Every debt whose anchor is *proved* at or below w — equal steps, equal
captured offsets — is removed; a debt whose anchor only may overlap w (an
index not proved equal) stays, because the store may have reached a
different element. A store at a place spelled through a reference r whose
place resolves to several members reached only the member r names, so no
debt is proved at or below any member w: there T3 removes exactly each
debt proved at or below w that records r and is anchored at r's referent
in w followed by the projection it records. T8 restores such a debt by
its instance over `r^` followed by that projection [TYPE-11.S5], and after
the store that place lies within the sealed value stored, so the removal
is a restoration the store proves; a debt that records nothing or another
reference, such as the one a write straight to one member leaves, stays.
T2 still adds debts for the anchors above w. A commit
into a complete own local binding whose right-hand side is a place read or
a `move` is not a T3 store; T4 governs it.

T4 **Local move and copy.** `let y = move p;`, `let y = p;` of a copy
struct, a by-value `match` binder, a destructuring binder [PROV-6], the
`Ok` continuation of `propagate`, and a SET-1 commit `set y = p;` or
`set y = move p;` whose target y is a complete own local binding are not
hand-offs. In this order: (1) take the set S of every debt that may be at
or below the source p, each rebased onto the destination y (see
[Debts](#debts)); (2) drop every debt *proved* at or below y
(equal steps, equal captured offsets), because y's old value is released
or overwritten whole [WIN-3]; (3) add S. Taking S before the drop matters
when p lies below y: `set y = move y.Some.value.inner.next;` must keep the
debts of the nodes it moves up. A debt only possibly at or below p is
copied onto y and also stays where it is; a debt proved at or below p is
moved for a `move` (the source root is dead [OWN-1]) and copied for a
copy. A field moved out consumes its whole owner [WIN-3], so debts proved
rooted at the owner die with it. So `set a.value = 31_u64; let b =
Index(value: 4_u64); set b = move a;` leaves b with a's debt, and a later
`table[b.value]` is refused.

T5 **Hand-off.** These are the hand-offs, each over the place or value it
hands on:

| hand-off | the handed place |
|---|---|
| call argument, own or copy | the argument value |
| call argument, `&` or a range reference | the referent (the run, for a range reference) |
| `return` and every propagated error exit | the returned value, and the referent of each reference parameter whose row writes under it |
| `give` | the delivered value |
| construction operand of a struct or an enum variant (`Some`, `Ok`, `Err` included) | the operand |
| a SET-1 right-hand side whose target is not a complete own local binding (a field, an element, `Box` content, a referent, a shared state) | the right-hand side |
| the argument of an OP-12 update | the old value |
| each edge leaving an atomic statement | each target's state (the state, the map entry, or each `Entries` element) |
| a spawn's arguments [WAIT-3] | as call arguments |

Three prelude rows are not hand-offs, because their supplied definitions
neither read a value's data nor hand a value to code that assumes its
invariant: `swap` [OP-11], whose two arguments only exchange their values
(T10 governs it), and the value-discarding rows `release` and `map_clear`
[PRE-1, SHARE-1], whose values are released, as a scope exit releases
them, with no user action on release [STOR-3]. Their arguments owe nothing
here. A hand-off requires no debt whose anchor passes an entry of a map or
an `Entries` below the handed place: the handed value owns none of that
entry. An atomic entry already owes every entry of a map target, so a
hand-off that required them would make every call that passes a map from
inside an atomic block unprovable; the price is T1's entry debts, since a
callee may leave an entry broken and no body may assume one valid
(`type11-neg-map-entry-read-unchecked`,
`type11-pos-entries-entry-left-broken`). **Release and root death.** When a value is released (`release`,
`map_clear`, a scope exit) or its root binding dies, only the debts
*proved* at or below the released place or rooted at the dead binding
(equal steps, equal captured offsets, the same root) are dropped; a debt
only possibly there stays. A local binding is its own root, so its scope
exit drops exactly its debts. Two whole targets of one atomic statement
whose state types unify may be one root [SHARE-2], so `map_clear` on one of
them drops no debt that was created through the other. This list is
closed; every other prelude or host row is an ordinary call.

At a hand-off, every debt that may be at or below the handed place must be
**restored** (T8) in the state before the hand-off's own events; a debt that
is not restored rejects the program there (see
[diagnostics](#rejection-and-diagnostics)). After a successful hand-off the
restored debts are removed.

T6 **Call.** After the hand-offs of its arguments (T5), the callee's
projected writes apply T2 to the anchors strictly above each argument's
place, and to no anchor at or below a reference argument's referent: the
callee left a referent it received in a hand-off sealed (T7 at a source
callee's exits, parametricity at a supplied definition's), and `swap` and
`map_clear`, whose arguments are no hand-offs, move (T10) or clear (T5's
release) the values below theirs without writing into one. The call's
results carry no debt, and a binding that receives a `shared_close` result
takes the unfolding below its state. This rule needs only the callee's
declared row, the declared parameter and result types and whether the callee
is `shared_close` or one of T5's three prelude rows, never its body; at a
concrete instance, a call through a function-kind parameter is a call of its
actual ([below](#prelude-host-and-module-boundaries)).

T7 **Body exit.** Each `return` and propagated error exit is a hand-off of
the returned value and of the referent of every reference parameter whose
row writes under it (T5). Together with T1 this is the function-boundary
guarantee: a reference parameter is sealed at entry and sealed at every
exit, so a callee may break its referent's invariants only between the two.

T8 **Restoration.** A debt `(a, I)` is restored at a point when the instance
of I over a — I with its binder replaced by a's terms [ENT-2] — is proved in
the current ProofContext by the existing judgment: [MSR-4] and [ENT-6] for
an affine invariant, [RANGE-3] for a range invariant, with written
`invariant`/`use` certificates available as everywhere [INV-1, PRF-1]. A
debt whose anchor has an unknown position `[?]` or is a cover cannot be
formed as terms and is never restored; only T3, T4 or the death of its root
removes it.

Restoration is attempted at exactly these points, so the result is
deterministic:

1. at each hand-off, for the debts it bears on (T5), and at each `swap`,
   for the debts that may be at or below either target, before its events
   kill the facts that would restore them (T10);
2. immediately before a kill batch of [ENT-5] that would remove the support
   of a term the anchor needs — an offset identity in its path, or the last
   reference variable through which the anchor is named — in the same
   pre-kill closure ENT-5 already materializes; a debt not restored there is
   widened (T9);
3. on every loop backedge and every `continue` [FN-1], for every debt;
4. on each incoming edge of a merge, for every debt on that edge, after the
   edge's own events and before the merge (T11).

T9 **Widening.** When an event of T8 point 2 removes an offset identity of
an anchor and the debt is not restored, that position becomes `[?]`, and so
does every later offset of the anchor that reads a tracked place, a binding
or a field or dereference place below one (ENT-2); when the anchor
was reached only through a reference whose path set REF-1 summarizes by a
cover, the debt is anchored at that cover. Widening only coarsens a debt:
the widened anchor overlaps everything the old one did. Widening the later
offsets too keeps the shapes of one anchor to one more than its offsets
that read a tracked place ([result 3](#result-3-the-bound-on-the-added-work)):
`grid[r][c]` widens to `grid[r][?]` when the inner index changes and to
`grid[?][?]` when the outer one does, never to `grid[?][c]`.
(The finer anchor for linked-structure cursors proposed under
[points for the owner](#points-for-the-owner), point 4, was adopted: a
debt written through a cursor is anchored at its current target identity,
and only one not restored when the cursor moves widens to the cover.)

T10 **Swap.** For `swap(first: &f, second: &s)`: a debt *proved* at or below
f (equal steps, equal captured offsets) is moved onto s, exchanged, and one
proved at or below s onto f. A debt that only may be at or below f — an
index not proved equal to f's, a reference whose path set holds several
targets — stays where it is and an exchanged copy is added at s; likewise
for s. When the two targets may be the same place, both receive the union.
T2 adds debts above each target. A debt exchanged onto a target t names its
invariant at t itself when its anchor lies at the other target's level, and
is the cover of t otherwise: an exchange appends no steps below t. So after
`let r = &c[j]; set r^.value = 31_u64; let x = Index(value: 1_u64);
swap(first: &c[i], second: &x);`, with i not proved equal to j, c keeps the
debt at `c[j]` and x receives a copy at x, while a debt at `h1.index` that
`swap(first: &h1, second: &h2)` moves becomes the cover of h2, which only a
whole store into h2 or its death removes.

Exchanging whole anchors below the targets was the first form of this rule;
it does not terminate. A loop that swaps a value with one of its own
recursive parts stacks the other target's steps onto its anchors every
round (the review of Whitefoot#484 traced `swap(first: &x, second:
&child^.inner)` and its mirror through two lists, the anchors growing by
three steps a swap), so the loop header's key set never stops growing.
Cutting a moved anchor where the exchanged value's type recurs below the
other target makes it finite but not small: with struct types T1 to TN,
T(k+1) holding two fields f and g of type Tk, and a loop of the N swaps
`swap(first: &A(k), second: &B(k))` with A(k) the path of N−k f steps and
B(k) the path of N−k−1 f steps and one g step, one round maps the f/g word
of a debt's anchor as a binary decrement, so the header collects all 2^N
words, one key each, and the iteration walks the body 2^N times (derived by
hand, not run). Every rule that keeps an exchanged anchor's steps below the
target composes this way; ending the anchor at the target is the one that
appends nothing. What it gives up is precision for a value broken below its
own level, exchanged and then repaired in its new place, rather than
repaired before the exchange or replaced whole: the repair the diagnostic
offers for a debt an exchange moved. Repairing before the exchange works
because T8 offers the targets' debts for restoration at the `swap`, in the
state before its write rows kill the facts the repair established; without
that point a debt repaired before the swap would still be on record there
and become a cover.

T11 **Join.** At a merge of control inputs, in this order:

1. On each incoming edge separately, every debt on that edge is offered for
   restoration (T8) in that edge's own fact state; a restored debt leaves
   that edge's set.
2. D after the merge is the union of the incoming sets that remain, events
   united per key.
3. For each key that is on no incoming set after step 1 — sealed on every
   edge — and on each incoming edge where that key's anchor satisfies T13's
   premises (its root live, every payload step in it carrying a current
   [ENT-3.S15] refinement on that edge, every position in it proved live
   [OP-4], its data formable as terms), the instance of the key is
   established on that edge before the merge intersects the fact states
   [ENT-5, ENT-6]. An edge where the premises fail receives nothing.

A key broken on one edge is never given an instance on another edge to make
the intersection keep a fact the broken edge lacks: it is either restored on
its own edge (step 1) or stays a debt (step 2). The premises of step 3 make
every added instance true of a place that exists on that edge at run time,
so no added fact can make a reachable edge contradictory. Without them, an
edge on which an enum holds another variant could receive an invariant of a
payload that does not exist, and an unsatisfiable invariant would make that
edge contradictory and let the merge discard it
(`type11-neg-join-unrefined-payload` below).

T12 **Loop header.** D at a header is the least set containing the
entering edge's debts and every backedge's and `continue`'s debts. It is
computed by monotone iteration (see [result 3](#result-3-the-bound-on-the-added-work)).

T13 **Instance at a sealed place** (the delivering hand-off's fact,
retrieved; see [SI as a chain of hand-offs](#the-sealing-invariant)). A new
ENT-3 fact source: when a
statement reads a binder datum q of an invariant I at an anchor a
(q = a.d), a fragment-integer place or a measure member of a measured
place, which is read as a field [OP-15], and `(a, I)` has no debt that may
denote it, and a's root is live, and every payload step in a carries a
current [ENT-3.S15] refinement, and every position in a is proved live
[OP-4] (it is, since the read itself owes that), the instance of I over a
is established after the
read, with the ordinary support of its terms [ENT-5]. For a range invariant
the instance becomes an active fact of the range walk [RANGE-2]. When the
anchor's other data are not terms — a place under a subscript that is no
[ENT-2] term — and I has exactly one binder datum, the instance is
established over the term the read binds (`let v = indexes[i].value;`
gives `v < 16`). An instance is a fact like any other: a later write that
overlaps its support kills it [ENT-5], and the same write adds the debt
(T2), so the fact cannot be re-established until the debt is restored.
The support of a measure member is the one descriptor word it names, so a
write of an element of the measured place or of another word kills nothing
of it and adds no debt for it [MSR-2] (cases
`type11-pos-measure-read-sealed`, `type11-neg-measure-read-keeps-debt` and
`type11-pos-element-write-keeps-measure-sealed`).
Four points read sealed values without a statement read. At a selected
return, a written postcondition is judged with the instance at each anchor
of a value or written referent the return hands back that its datums read
through: T7 has just required each of them sealed, so a clause that
restates the invariant of a forwarded call result's payload holds. At body
entry, the instance at each parameter of the struct and at each referent of
a reference parameter to it is established (T1), the fact the caller's
hand-off proved. Once the kills of a statement that makes a call have run,
the instance is established at each referent of the struct the call
received at a reference parameter whose row writes it, the fact the
callee's return proved (T7), or for a supplied definition the fact its
parametricity gives ([below](#prelude-host-and-module-boundaries)),
provided no debt may denote it and no kill of
the same statement consumed the anchor's root, rebound a reference whose
target identity the anchor carries, replaced what one of its offsets reads
or wrote, at or above its level, an enum one of its payload steps selects
from: those kills would leave the anchor naming something other than the
referent the call returned. A reference into the referent's window whose
bound the call re-judges [OP-10, REF-2], a later call's requirement and a
later subscript then use the instance as they use the body-entry one,
which before this design the caller took from the generated postcondition
of the written reference parameter. Each binding of the struct that a
statement gives a value, and the target of a `set` commit, takes its
instance the same way once the statement's kills, for the commit its own,
have run: the binding of a `let` whatever its initializer, a binder of a
destructuring `let`, a by-value `match` binder and the `Ok` binder of
`propagate`. Only the debts a continuation brings along (T4)
can leave such a value unsealed, and the premise checks them: a call result
carries none (T6), a construction proved its invariants, a `give` and a
store into anything but a complete own local binding handed the value off
(T5), and T3 removes the debts proved at the stored place, or, through a
reference that resolves to several members, those that reference recorded,
so a debt at a member the store may not have reached stays and withholds
the instance. Before this
design the caller took the same fact from the generated result
postcondition, and a `match` arm or a `propagate` binder from the routed
requirement or postcondition of the enum parameter or result it unpacks;
without this point a subscript or a call in the statement that first reads
such a value would see no fact.

## Result 1: restoration obligations are never lost

**Statement.** For every checked body: (a) SI holds at every point; (b) at
every accepted hand-off the handed value is valid; (c) every fact T13
establishes is true. Hence no program path reads a broken value as valid.

**Proof sketch.** Induction over the steps of any execution of a checked
program, every activation's abstract state paired with its runtime state.

- *Base.* At body entry D is T1's set and every parameter value and referent
  is valid, by (b) applied to the caller's hand-off (or [PROG-3] for the
  entry), apart from the entries of a map or an `Entries`, which that
  hand-off did not deliver valid (T5). T1's debts denote every key at or
  below such an entry, and SI asserts nothing of a key a debt may denote,
  so SI holds.
  Recursion needs no separate argument: the induction is over execution
  steps, not over call depth, so a recursive call relies on (b) at an earlier
  step.
- *T2.* A write changes only storage at or below w. An invariant I at an
  anchor a can change truth value only if `a.Rd(I)` overlaps w. If a is at
  or below w the event's own rule applies; otherwise a is a strict prefix of
  w and T2 adds `(a, I)`, so SI asserts nothing about it afterwards. The
  overlap judgment is [OWN-7]'s, conservative where separation is unproved,
  so an alias that may name the same storage is treated as naming it.
- *Levels.* Every runtime value has one level, the number of values on the
  ownership path from its root down to it, and every resolved place that
  denotes it has that level: a step that selects a value goes one value
  down, a run selector stays within the value it selects from, and a
  subscript of a run selects an element of the value the run lies in, one
  level below it. So two places that may denote one value have one level,
  and a place denoting a value at or below another's has at least its level;
  "may denote" and "may be at or below" ask exactly these levels and
  [OWN-7]'s conservative overlap, and miss no runtime aliasing. A cover step
  stands for any number of levels, so it never makes them miss one either.
- *Rebasing and exchange.* When the value at p moves to q, a runtime anchor
  v at or below it moves to q followed by v's steps below p's value. An
  anchor that denotes v has, by the level argument, a prefix at p's level
  denoting p's value, and its steps below that prefix lead from p's value to
  v; the rebased anchor puts q above exactly those steps. Where the prefix
  cannot be found, or the steps pass a cover step, the rebased debt is a
  cover above the moved anchors, which only adds what SI must not assume.
  An anchor that does not begin with p's steps is only possibly at or below
  p, so its debt also stays where it is (T4, T10): a copy rebased from an
  anchor that denotes another value adds a debt and removes none. A `swap`
  exchange is a rebasing whose steps below the target are dropped into the
  target's cover, coarser still.
- *Reference joins.* A write through a reference variable whose target REF-1
  describes by a path set or a cover resolves to every member [EFF-2, REF-1];
  T2 is applied to every member's anchors, and to the cover's, so no member
  path escapes. A hand-off through such a reference checks every member's
  referent (T5 uses the same resolution). Debts are keyed by resolved places,
  never by reference variables, so renaming a reference (`let s = r;`),
  rebinding it, or letting it go out of scope cannot remove a debt: only T3,
  T4, T8 or the death of the root removes one, and each removal is justified
  below.
- *Branch joins.* T11 removes a debt from an edge only by a restoration
  proved in that edge's own state, then takes the union, so a key broken and
  unrestored on any incoming edge is broken after the join. A key on no
  remaining set was sealed or restored on every edge, so SI holds for it on
  each, hence after the merge. T11 adds an instance only for such a key and
  only on an edge where the anchor exists there (T13's premises); by SI it
  is true there, so it cannot make a reachable edge contradictory, and the
  fact intersection stays sound.
- *Calls through write references.* Before an ordinary call T5 proves every
  argument referent sealed; `swap`, `release` and `map_clear`, which T5
  exempts, fall under the exchange and release cases. The callee starts with
  D empty over that referent (T1) and must restore every debt at or below it
  at every exit (T7), so the referent is valid again when control returns;
  T6 therefore adds no debt at or below the referent and SI holds there.
  Anchors strictly above the referent may have lost a cross-field relation
  through the callee's writes; T6 applies T2 to them, using the callee's
  declared row, whose exactness [EFF-2] guarantees it covers every write. A
  callee taking `&u64` at the argument `&idx.value` writes below the anchor
  `idx`, which is above the referent, so the caller owes `idx`'s restoration
  (case `type11-neg-u64-helper-breaks` below). The instance T13 establishes
  at the referent once the call's statement has run its kills is true by
  SI, since T6 left no debt that may denote it; an anchor above the
  referent that T6 put in debt gets none (case
  `type11-neg-written-referent-ancestor-window`).
- *Removals.* T8 removes a debt only when the instance is proved in the
  current state, so I holds at that moment, which is exactly what SI asks.
  T3 removes a debt only when the store is proved to have replaced that very
  anchor's value with a value already sealed: a hand-off, a call result or a
  construction, never a place read or move, whose debts T4 carries instead.
  A store through a reference r that resolves to several members is proved
  at none of them, so there T3 removes only a debt that records r and is
  anchored at r's referent followed by the projection p it records. T8
  restores such a debt by the instance over `r^` followed by p, on the
  ground [TYPE-11.S5] states: each of its breaking events wrote through r
  to the member r still names. After the store that place lies within the
  sealed value stored there, so the instance holds and the removal is that
  restoration. A debt that records nothing or another reference may lie at
  a member the store did not reach, and stays (case
  `type11-neg-joined-reference-store-keeps-debt`).
  T4 takes the source's debts first, then drops only the destination's
  debts proved at or below it (its old value is gone), then adds the taken
  set, so a source below the destination keeps its debts. Every removal or
  transfer at a release, a dead root or a `swap` (T5 note, T10) applies only
  to debts *proved* at or below the place, by equal steps, equal captured
  offsets and the same root; a debt only possibly there stays, and a `swap`
  copies it to the other target, so an unproved alias can never carry a
  debt away from the place it may denote. A root that dies takes its proved
  anchors with it; a consumed value continues as the
  destination's (T4) or reaches a hand-off (T5).
- *Widening.* T9 replaces a debt by one that overlaps at least the same
  anchors, so SI's premise only weakens.
- *Loops.* T12's set is an inductive invariant of the header (below), so
  every iteration starts in a state where SI holds for its keys.
- *Atomic statements.* A shared state belongs to no activation. Its
  validity is an object invariant: valid whenever no atomic statement holds
  it. It holds after `shared_new` and `shared_map_new` (their arguments are
  hand-offs; an empty map has no entries), and every atomic statement's
  leaving edges restore every target's state (T5). [SHARE-3] gives the
  statements on one object exclusive access in one total order, so each
  block starts from a valid state: at block entry D holds no debt rooted at
  a target's state, which is SI there. Two targets that may name one object
  are compared from one root [SHARE-2], so a write through one adds debts the
  other's hand-off sees. A frozen object is never written [SHARE-1], so its
  value stays valid from `frozen_new`.
- *Other contexts.* A context writes only its own storage and shared states
  inside atomic statements [WAIT-3, SHARE-2, PAR-1]; nothing another context
  does can falsify an invariant of a place this activation holds.
- *(b)* follows from SI: T5 restores every debt at or below the handed place,
  so the place is sealed, hence valid.
- *(c)* T13 fires only for a key with no debt that may denote it, at a read
  and at each of its four other points alike, so SI gives the invariant at
  that anchor at that moment; the instance's support then carries it
  exactly as long as ENT-5 keeps any fact.

Every way a value comes into existence is covered: a literal or constant
(construction checked [TYPE-11, CONST-2]), a construction (its own
invariants owed as today, its operands handed off), a call result (the
callee's return is a hand-off; a supplied definition's result is sealed by
parametricity, below, apart from the values below the state `shared_close`
returns, which start in debt at the binding the result continues into and
fail any hand-off of the unbound result), a place read (sealed or carrying its debts, T4), a
value initializer (`give` is a hand-off), a propagated payload (T4), an
atomic target (SI of the state), a parameter (T1).

**Laundering attempts and where each is refused.**

| attempt after `set holder.index.value = 31_u64;` | refused by |
|---|---|
| read `holder.index.value` and index the table | T13 withholds the instance; the S5 image `== 31` refutes the bound (OP-4) |
| `let r = &holder.index; let s = r;` then `load(index: s, ...)` | T5 at the call: the resolved referent `holder.index` has the debt (FN-8) |
| `let c = holder.index;` (copy) then use `c.value` | T4 copies the debt to `c`; as the first row |
| `let b = Index(value: 4_u64); set b = move holder.index;` then use `b.value` | T4 drops b's old debts, then rebases the source's onto b; as the first row (the move also consumes `holder` [WIN-3]) |
| `Some<Index>(value: holder.index)` | T5 at the construction (TYPE-11) |
| `place_back(window: &indexes, value: holder.index)` | T5 at the call (FN-8) |
| `give holder.index;` | T5 at the give (TYPE-11) |
| a helper `restore_later(index: &Index) writes(index)` | T5 at its call: a callee may not receive a broken value (FN-8) |
| `return` without restoring, `holder` a written parameter | T7 at the return (FN-9) |

**Confidence: 4/5.** Each step is a local case over a closed list of events
the specification already enumerates (ENT-5's kill events, EFF-2's write
attribution, REF-1's path sets, SHARE-3's order). What could overturn it: an
event that changes stored data and appears in none of those lists — for
example a write a supplied definition makes outside its declared row, which
[EFF-2] and [SCOPE-3] already forbid, or a host operation that writes into a
type-parameter value; the implementation review must check that T2 is
attached to exactly the event list ENT-5 kills on, so the two cannot drift
apart.

## Result 2: container evidence has a finite representation

**Statement.** No per-element evidence is ever stored. For a container
place c — a storage shape, `Box`, an enum payload, a map entry, a shared
state — "every element is valid" is SI applied to the element anchors below
c that no debt names. A container is sealed exactly when no debt is anchored
at or below it, and every operation keeps that true by construction.

**Why it holds.** Elements enter a container only through a hand-off and
change in place only through writes that leave debts:

| operation | effect on the container's sealedness |
|---|---|
| `place_back`, `insert_at`, `place_front` [OP-10]; `array_filled`, `box_array_filled`, `slots_from_array` and the other constructions [OP-13]; `Some`, `Ok`, `Err`, `box_new` | the inserted value is a call argument or construction operand: a hand-off (T5) |
| `set c[i] = v;`, `set b.inner = v;`, `set t^ = Some<V>(value: v);` on a map entry | the right-hand side is a hand-off; T3 removes debts proved at `c[i]` |
| `set c[i].f = x;` or a write through `let r = &c[i];` | T2: debt `(c[i], I)` with i's captured identity, restorable while i's identity lives (T8), widened to `c[?]` when it dies unrestored (T9) |
| a call taking `&c[i]` with `writes` | the callee restores the element (T7); T6 adds debts only above it |
| iteration (`for k in 0..n`) reading `c[k]` | T13 gives each read element's instance, as long as no debt names `c[?]` or `c[k]` |
| a loop writing `c[k].f` | restoration is attempted at the backedge (T8.3); a debt left there loses k's identity at the counted update and widens to `c[?]` (T9), which no later point can restore |
| `swap(first: &c[i], second: &c[j])` | not a hand-off (T5's closed list); T10: the elements' debts exchange; sealed elements stay sealed; ancestors' relations get debts |
| `take_back`, `take_front`, `remove_at`, `split_off`, `slots_into_array` | the call's reference argument is a hand-off, so the container is sealed when the call begins; the returned value or container is sealed by parametricity (below) |
| `append`, `grow`, `grow_paged` | values move without change: no debt at or below the window; `grow` applies the relocation table (T2) |
| `map_clear`, `release`, release at scope exit | values are released, not handed off (T5's closed list); their debts die with them |
| atomic statement on a map entry or `Entries` | every value the entry owns starts in debt at block entry, under the provisional atomic rule above; a map or an `Entries` owns no entry, so no hand-off of one requires an entry restored (T5) |

So "every element satisfies its type's invariant" needs no record: it is
the absence of a debt, and every element that the program could not prove
valid is named by a finite debt (one per write site, with `[?]` covering
every element an unrestored loop may have touched).

**What this costs a writer.** An element broken in place must be restored
before its index identity is lost: within the same iteration, or through a
callee that takes the element by reference (T7). As built, one element
handed on alone after that only needs its own instance proved there
[TYPE-11.S5]. A program that breaks every element in one pass and repairs
them in a second pass is refused at the next hand-off of the container,
naming the write, because no finite fact states "every element was
repaired". This is a completeness limit, not a soundness
gap; a range fact over the run [RANGE-1] could discharge a `c[?]` debt in a
later change, and the refusal reopens that question if a real program needs
it. A direct write `set c[i].f = x;` receives no S5 post-write image today
(an index target has no commit value [ENT-3.S5]), so its restoration is
provable only when the element is written through a reference
(`let r = &c[i]; set r^.f = x;`), whose place `r^.f` is a tracked term
[ENT-2]. Extending S5 to subscripted targets is optional and separable.

**Confidence: 4/5.** The table covers every [PRE-1] row with a type
parameter in an element position and every source write form; what could
overturn it is a prelude or host row that forms or changes a value of a
type parameter other than by moving one it was given — the parametricity
obligation below makes that a stated TCB condition, and the implementation
must audit each row against it.

## Result 3: the bound on the added work

**Statement.** Per concrete function instance (and per generic schema),
with n the number of statements and places written in the body, d the
longest written path, K the largest number of invariants of one struct and
r the largest read set:

- the number of debt keys is at most A = (R + 1)·B·(d + 1)²·K + C, with R
  the number of roots in the body (bindings, parameters and atomic targets'
  states), B = n·d + U + S the base anchors, d here also bounding the paths
  of the anchors atomic entries and the body entries of map and `Entries`
  parameters add (T1), U their number, S the number of `swap`
  targets and C the number of covers. An anchor is a base anchor (a proper
  prefix of a written place, at most n·d, one an atomic entry or a body
  entry adds, or a `swap` target) or a root followed by the steps a base
  anchor has below a value, at most d + 1 per base anchor: T4 rebasing onto
  a root keeps such steps and a T10 exchange appends none below its target. T9 widens each
  anchor to at most d + 1 shapes, because a widened offset widens every
  later one;
- the debt state at a point is a subset of those keys; the breaking-event
  sets are diagnostic only and take no part in the fixed point;
- a write event costs at most d·K·r overlap judgments, each an existing
  [OWN-7] judgment;
- a join is a set union, O(A); T11's added instances are at most A per edge;
- restoration attempts are at most A per hand-off, per backedge, per merge
  edge and per
  kill batch, each one existing ENT query (or one RANGE-3 problem under its
  existing structural ceilings).

**Monotonicity.** Order abstract states by debts (more debts is higher) and
facts (fewer facts is higher). Every rule is monotone in this order:

- the only fact sources that depend on debts are T13 and T11 step 3, and
  each requires a key to be absent from the debts, so more debts give fewer
  of their instances; ENT-5's closure, kills and the merge intersection are
  monotone in their input facts, so fewer facts in give fewer facts out;
- every rule that removes a debt does so on a proof — T8 restoration, T3's
  proved-equal anchor — so fewer facts give fewer removals and more debts;
  T11 step 1 is T8 applied per edge, before any union;
- T2, T4, T9, T10, T11 step 2 and the debt-dropping events (death of a root,
  a release) map each debt set elementwise or by union, so a larger set in
  gives a larger set out.

Hence, with all other inputs fixed, the map from debts entering a program
point to debts leaving it is monotone. The earlier form of T11, which gave
an instance to an edge where a key was absent while another edge still held
it as a debt, broke this: adding a key at a header could make the key
absent from a clean edge only, give that edge an instance, let the merge
keep a fact that restored a second key, and so lower the output. That form
is gone: step 3 now fires only for a key absent from every edge, so adding a
key can only remove instances.

**Loop fixed point.** Fix every other input; the header's debt set D_h maps
to F(D_h) = entering debts ∪ the debts on the backedges and `continue`s after
one walk of the body starting from D_h. By the monotonicity above F is
monotone. The key lattice is finite (A keys, each with or without its
recorded reference), so Kleene iteration from the entering set reaches the
least fixed point in at most 2·A + 1 walks. The least
fixed point is sound by induction over iterations: the entering edge
satisfies SI for every key outside it, and if an iteration starts with SI
for those keys, the walk proves the backedge leaves them restored or keeps
them as debts, so the next iteration starts the same way; this is the same
argument [INV-1] uses for a written header invariant.

**Nested loops and the whole body.** Header sets never shrink during the
computation: iterate the whole body with a monotone update of every header
(chaotic iteration without resets). Each re-walk adds at least one key to at
least one header or drops a recorded reference [TYPE-11.S2] from one, so
there are at most 2·H·A + 1 walks, H the number of loop headers. Each walk
is the existing proof walk plus the work above, so the total is O(H·A·C), C
the existing per-body checking cost — polynomial in the body's size and U. The least fixed point of a monotone function on a finite
lattice is unique, so the iteration order, worklist order and thread
schedule cannot change it [ENT-1]. No budget, depth limit or timeout
appears; the iteration ends because a finite set grows.

The numeric fact state and the debt state depend on each other only through
T13 (and T8's use of facts). Facts at a loop header are already computed
from the structural kill set of the body [ENT-5], and any key a body write
can break has its instance's support written by that same write, so its
instance is killed at the header in every round; the re-walk changes only
which instances appear inside the body, which is what the monotone
iteration accounts for.

Two earlier forms of the rules broke this bound. Exchanging whole anchors
below the `swap` targets composes: N nested swaps of distinct types reach
2^N anchors and walk a loop 2^N times, and a swap into a value's own
recursive part never stops growing them (T10). Widening each offset
independently gives an anchor with m offsets that read bindings 2^m shapes,
when the kill batches of different paths widen different subsets (T9).

U is the one term that is not polynomial in the body's size. The
provisional atomic rule trusts no struct value a target's referent owns
below it, so an atomic entry adds one debt per anchor of the referent's
type unfolding, with `[?]` for each element and a cover where a type
recurs on the path; that is one per struct value a value of the type holds
down to its first recursion, which is exponential in the number of distinct
struct types nested along shared paths (S1 holding two S2, S2 two S3, and
so on). The shared-object model, whose ruling the provisional rule waits
for, decides which of those values a block may trust; a block that trusts
them adds no entry debts. A map or an `Entries` reference parameter adds
the same unfolding of its entry type at body entry (T1), in every body
with such a parameter whether or not it reaches an entry. T5 names it as
the price of a map owning none of its entries, which the provisional
atomic rule requires, so the same ruling reopens it; U is zero for a body
with neither. Both costs stand as a provisional choice
([entry unfoldings](#entry-unfoldings)).

**What remains to measure.** The bound is polynomial in the body's size
and U; whether it is cheap
is an empirical question the criterion below answers. The common case is one
walk per loop (every key restored on the backedge on the first round), and a
function that writes no invariant data, has no map or `Entries` reference
parameter and enters no atomic block has no debts at all.

**Confidence: 4/5** for the bound. What could overturn practicality (not
the bound): the T13 instances turning many more functions into range-judgment
participants [RANGE-2] when structs with range invariants are read widely;
the measurement below separates that cost.

### Entry unfoldings

Two debt sets come from unfolding a type rather than from writes: an atomic
block's entry, under the provisional atomic rule, and a map or `Entries`
reference parameter's body entry (T1). Each holds a debt for every
invariant of every struct value the type holds, `[?]` at each entry and
element position, and the cover of a value whose type is that of a value
above it on its path.

*Cost.* For a tree-shaped type the count is linear in the type's size. For
distinct struct types shared along paths it is exponential: with `N0`
declaring an invariant and each `Ni` holding two `N(i-1)` fields, an entry
of type `Nk` starts with 2^k debts from k + 1 declarations (derived from
`unfolding_below`, not run). The checker keeps the debt set as a list
and finds a key by a linear search at each addition, so forming that set
takes on the order of 4^k key comparisons, and every later join, loop
header and hand-off walks it. A continuation into a local binding (T4) of
a value these debts lie at or below, such as a map entry's value or a field
of an atomic target's state, rebases each of them onto the local, so each
such continuation adds as many keys again: c of them in a body give about
(c + 1)·2^k keys, formed in on the order of (c + 1)²·4^k comparisons.
Result 3's bound is accordingly polynomial in the body's size and in U, not
in the body's size apart from U. A body pays at each map or `Entries`
reference parameter whether or not it reaches an entry, once any struct of
the program declares an affine invariant; a part of the type that holds no
invariant-bearing struct adds nothing. No downstream program pays it
today: in Firn, Halo and Snowghost no struct type invariant lies below a map
entry or a shared state (read from their sources: Firn's invariants are all
loop invariants, Halo uses `Entries` only in research witnesses, and
Snowghost uses no map).

*Alternatives.*

- An on-demand form reaching the same verdicts (proposed, not built). One
  region debt would stand for a parameter's whole set, which holds only
  `[?]` anchors and covers and is never restored, and a read would test its
  path against the region. A hand-off below an entry still owes every
  instance the handed value holds: it would enumerate them depth first,
  stop at the first it cannot prove, and skip each subtree whose
  invariants hold with no facts, decided once per type; a body of
  polynomial size supplies facts for polynomially many places, so the
  enumeration stays polynomial (deduced). A value copied out of an entry,
  and an atomic entry's fixed-position debts, are restored one by one, so
  a region would also record its restored subtrees, intersected at joins,
  and every event row (write, rebasing, exchange, widening, join, header,
  hand-off, release) would handle regions. It needs its own argument that
  its verdicts equal the eager set's, and its diagnostics would name the
  first unrestored debt the enumeration meets.
- The on-demand form for the parameters alone: their regions need no
  restored subtrees, but a copy of an entry value into a local unfolds
  again, so the exponential case remains, only no longer at every
  parameter.
- Cutting every repeated type of the unfolding to a cover: polynomial, but
  a value holding two fields of one invariant-bearing struct could then
  never be handed off whole, even after both are checked, which the
  boundary-only invariants accepted.
- Restoring a value read from an entry only as a whole: its hand-off still
  proves every instance the value holds, so it saves no enumeration, and it
  refuses the partial restorations that per-anchor debts accept.

*Choice.* The eager unfolding stays as a provisional choice, against the
design tree's root decision that no checking work grows exponentially with
program size. Both sets follow from the provisional atomic rule, and the
shared-object model that settles it can remove both: a block that may
trust the state it enters starts with no entry debts, and a map may then
own its entries, so a parameter's entries arrive sealed (deduced; board
item `shared-invariant-through-reads` carries the argument and the removal
of this cost). Building the on-demand form now would rebuild the debt
representation that ruling may make unnecessary. The uncertainty is whether
the shared-object model removes the entry debts. The choice is reopened
when that model's design is settled, or when a program whose entry or
state type is shared deeply makes checking noticeably slower.

## Generic structs

TYPE-11 today admits an invariant only on a struct that declares no
generics. The change admits generics under one condition on the binder
data: **every step of a binder datum's projection selects a field whose
declared type does not depend on a type parameter, or is the final measure
member of a measured type** (whose measure is `u64` whatever its element
type), and the datum's own type is a fixed fragment integer. Const generic
parameters are admitted as constant terms [ENT-2 (c)]. So

```text
struct Arena<T: drop> {
  entries: Slots<T, 16>;
  next: u64;
  invariant bounded(arena): arena.next <= arena.entries.len;
}
```

is admitted, while `struct Pair<T: Int> { a: T; b: T; invariant
ordered(pair): pair.a <= pair.b; }` is refused at the invariant (its datum's
type is a type parameter, which [ENT-1] does not admit as an L0 fragment
type in the generic schema).

**Why it is sound under substitution.** The relation names no datum whose
place or type depends on a type argument, so at every instance it is the
same relation over the same paths; its read set is the same at every
instance. Every TYPE-11 site, and every debt, is checked at each concrete
instance [FN-2] and once in the generic schema [ENT-1]. Inside a generic
body a value of type T has no known fields, so the body can change it only
as a whole (a move, a store of a sealed value, a `swap`), each of which keeps
it sealed (T3, T4, T10); so the schema and every instance compute the same
debts at T-typed places, namely none. `Valid(Arena<S>)` is the arena's own
invariant plus `Valid` of each element; the first is the arena anchor's, the
second the element anchors' (S's invariants), and neither definition unfolds
the type, so recursive generic types (`struct Node<T> { kids:
Box<Slots<Node<T>>>; ... }`) add no checking work: anchors are only places
written in the body. A relation between an arena and a separate table
remains a contract, as the design tree already decided.

## Fields the invariant does not read

The separate request asks that a public field that no invariant reads may be
writable. Under A′ the two compose with nothing extra: a write to a field f
outside the read set of every invariant of every anchor above it adds no debt
there (T2's overlap test fails); anchors at or below f are sealed by the
store's own hand-off (`set x.count = v;` hands off v) or carry debts the
writing function must restore before its next hand-off of them, in whatever
module that function lives.

This observation goes further than the request, and is reported to the owner
rather than adopted here: **under A′ the readonly restriction is no longer
needed for soundness at all.** Today a type invariant is kept by the
declaring module, so every other module must be kept from writing the
fields it reads. Under A′ the obligation is carried by the debt of whichever
function writes, in any module, so even a field the invariant reads could be
public writable without unsoundness. Keeping the read-set restriction is then
an encapsulation choice (the declaring module decides how its invariant is
re-established) rather than a safety requirement. The two options for the
owner: (a) the requested rule, public writable only outside the read set; (b)
no field restriction from TYPE-11 at all. Both are sound under A′.

## Prelude, host and module boundaries

**Parametricity of supplied definitions.** A supplied definition [SCOPE-3,
PRE-1, PRE-2] that is generic in T has no way to form a value of T except by
moving or copying one it was given through its parameters or found in
storage its parameters reach, and it changes such values only by moving
them. Under A′ a value that a hand-off delivers to the call, as an argument
or a referent, is valid, and so is every value it owns, so every such T value
the definition returns or leaves in the caller's storage is valid. `swap`,
`release` and `map_clear` receive no hand-off (T5): they exchange, release or
clear the values they are given and form none, and T10 moves the debts with
the values `swap` exchanges. `shared_close` returns a state it found through
a handle, which owns nothing, so no hand-off delivered it (below). The
specification states this
as a supplied-definition obligation (part of the trusted base, as the row
exactness of [EFF-2] already is) and drops the rule that a supplied
definition promises no generated postcondition. This is what lets
`take_back` hand back a valid element: the window was sealed at the call.
A non-generic prelude or host row has parameter and result types the prelude
or a host module declares; none of them is a source struct with an invariant,
so such a row has nothing to promise.

The audit the implementation owes: every [PRE-1] and [PRE-2] row with a type
parameter in a result or written-element position (`take_back`,
`take_front`, `remove_at`, `split_off`, `append`, `slots_into_array`,
`slots_from_array`, `array_filled`, `box_array_filled`, `swap`, the shared,
frozen and map rows) moves or copies only values it received.

**The state `shared_close` returns.** Specification v0.158 added
`shared_close`, whose call as an object's last handle returns the object's
state; the argument above does not cover it. Its parameter is a handle, which
owns nothing [TYPE-11.S1], so no hand-off delivered the state to the call,
and every atomic block on the object owes only its target's own invariants
on its leaving edges, the provisional reading of shared state that also
starts each atomic entry with the unfolding below the target. The state
therefore keeps its own invariants, since `shared_new` received it in a
hand-off and each block owes them, while every struct value it owns is in
debt: a continuation of the result into a binding takes the unfolding below
the state's place there, and a hand-off of the unbound result fails while a
value of the state's type may own a struct value with an affine invariant,
since no term names a value that no place holds. Trusting the state as a
sealed call result is unsound: a block may write a nested value out of range
and end, and the last close then returns it. Putting the whole state in debt
would make a writer recheck invariants every block already owes. A
shared-object model that makes leaving edges restore the values below a
target (board item shared-invariant-through-reads) would seal the state and
retire this exception. The concrete instance of a generic body is judged
with its substituted types, so a generic function that returns its
`shared_close` result whole is refused at an instance whose state owns such
a value. Cases: `type11-neg-shared-close-state-field-unchecked`,
`type11-pos-shared-close-state-field-checked`,
`type11-neg-shared-close-result-stored-whole`,
`type11-neg-shared-close-result-given-whole`,
`type11-pos-shared-close-state-own-invariant`,
`type11-neg-shared-close-generic-wrapper` and
`type11-neg-shared-close-function-actual`; a hand-off of the unbound result
is a return, a `give` or a store, since a call argument and a construction
operand are atoms [GRAM-5].

**Module boundaries.** The sealing boundary is a property of declared types
and of the identities of four prelude functions: parameters sealed at entry,
written reference parameters sealed at exit, results carrying no debt, the
arguments of `swap`, `release` and `map_clear` no hand-offs, and a binding of
a `shared_close` result in debt below its state. The prelude is fixed and
every module imports it, so a module's verdicts still depend only on the
interfaces it imports [MOD-8]. A struct's invariants, and hence its read
sets, are part of its declaration in the module interface; a client that
cannot name a private field cannot write it except by replacing a whole
value, which is a sealed store. The call-graph component rules of [DIAG-2]
gain nothing, because no generated postcondition is published any more:
whether a result carries debt is read from the declared types and the
callee's prelude identity, not from a summary.

**Function-kind parameters.** A concrete instance of a body resolves each
call through a function-kind parameter to a call of its actual [FN-2, FN-5],
and the prelude identities above apply there: a formal bound to
`fn shared_close::<S>` gives the binding of its result the debts below the
state, and a hand-off of its unbound result fails, while the arguments of
one bound to `fn swap::<T>` are no hand-offs; the call's projected writes
still use the formal's row for T6, as every other effect judgment does
[EFF-2]. At the body's symbolic instance the formal is no prelude function,
so its arguments are handed off and its result carries no debt. That
judgment may trust a result a concrete instance puts in debt, and stays
sound because generics are monomorphization-only and every concrete
instance is checked again as concrete code [FN-2], so the code that runs has
been judged with its actual (case
`type11-neg-shared-close-function-actual`).
[FN-4]'s refinement queries lose the generated
clauses that formal and actual used to share, so a formal `ensures` that
restates a result's invariant would no longer follow from an actual that
writes none. Each query therefore takes, beside its written premise set,
the body-entry instances (T1) at every datum it ranges over: the
parameters for the `requires` query, and the entry states, exit states and
selected result datums for the `ensures` query. Both sides owe and receive
these instances alike, so the check still compares only what the two
signatures write.

## Rejection and diagnostics

A failed restoration at a hand-off rejects the program at that hand-off.
To keep the existing cases' rules stable, the citation follows the site:
FN-8 at a call argument, FN-9 at a return or propagated exit (and at a
written reference parameter's exit), RANGE-3 for a range invariant at those
sites, and TYPE-11 at a construction operand, a `give`, a store, and an
atomic leaving edge, as TYPE-11 cites today. Every such diagnostic names the
anchor, the invariant, the first unproved conclusion and each breaking event
in the debt, so the writer sees the write that broke the value and the
hand-off that needed it, with a repair offering to restore the relation
before the hand-off or to replace the value whole. A debt that is
unrestorable because it was widened says which identity was lost (the loop
iteration or the reference that named the element).

## Falsifying conditions

What would show the rule unsound or force a redesign, before or after
implementation:

1. **Unsoundness.** Any accepted program in which a value read from a place
   the checker treats as sealed violates its invariant at run time. The
   negative cases below are built to hit each hand-off and each aliasing
   route; a failure of any of them is a rule or implementation defect, not
   a tuning question.
2. **A real program that must pass an unrestored value across an ordinary
   call.** A′ refuses a two-step update split across two helpers that each
   take `&S` (the first helper's exit owes restoration). The supported forms
   are helpers over the fields (`&u64`, whose caller owes restoration), a
   helper that does the whole update, or a whole-value replacement. If a
   real program needs a callee that receives a broken `&S`, the design needs
   an open-state callable form, which the study left out of scope; that goes
   back to the owner.
3. **Element-wise repair across iterations.** A real program that breaks
   many elements in one loop and repairs them in another is refused (result
   2). If one appears, the extension is range-fact discharge of a `c[?]`
   debt; it would go back to the owner as a separate change.
4. **Cost.** If the criterion's check-time comparison fails, the iteration
   of result 3 is the first suspect (rounds per loop) and the T13 range
   participation the second.

## Criterion before implementation

The change is accepted when, on the implementing branch:

1. every case listed below gives its stated verdict and cites its stated
   rule, in the native conformance adapter;
2. Firn's select-database code, rewritten to store an `Index`-like bounded
   type and index the table with its value without `iand`, is accepted at
   Firn's pinned compiler built from the branch, and the same program with
   the stored value written to 31 before the read is refused at the read or
   at the hand-off;
3. the existing corpus (compiler tests, programs, Firn, Halo and Snowghost
   checks at their pins) keeps every verdict except the changed cases
   listed below;
4. checking time of Snowghost's largest entries and Firn's full check, paired
   base/head runs with a twin control on the 14900K runner, is at most 5%
   slower than the base; a larger cost is reported with its attribution
   (rounds per loop, T13 instances, range participants) before merging.

## Specification changes

Rules to change, with the intended behavior (not final text):

| rule | intended behavior |
|---|---|
| TYPE-11 | Define validity through owned parts (fields, `Box`, the held variant's payload, live elements and slots, map entries, shared and frozen states); define anchors, read sets, debts and the hand-off list; restoration at hand-offs replaces the per-site requirement list; admit generic structs under the datum condition; replace the field-writability clause by the owner's choice (read set, or none); diagnostics name the breaking events. The direct-enum-payload bullet is subsumed and removed. |
| FN-8 | The generated type-invariant requirements become the hand-off of each argument (value or referent): discharged by sealedness or by restoration proof. Body entry establishes every parameter value and referent as sealed (T1). |
| FN-9 | The generated postconditions become exit hand-offs: each written reference parameter's referent restored at every exit, below its top level too; results sealed. The routed enum-payload postconditions and their vacuity rule are subsumed. |
| FN-4 | Each refinement query also takes the body-entry instances at the data it ranges over, in place of the generated clauses formal and actual shared. |
| ENT-3 | New source T13 (instance at a sealed anchor when its datum is read, including the one-datum copy form, and at the points T13 lists without a read: body entry, the referents a call wrote, each binding a statement gives a value and a selected return's clause); S4 covers parameters through T13 rather than as generated requirements. |
| ENT-5 | Debt state: T2 on exactly the write events this rule kills on, including the relocation table; restoration attempts in the pre-kill closure (T8.2); widening (T9); join union and edge instances before intersection (T11); loop headers as least fixed points (T12). |
| ENT-2 | Optional: an S5 post-write image for a subscripted target with captured offsets, so `set c[i].f = x;` can restore without a reference. |
| SET-1 | The right-hand side of a store into anything but a complete own local binding is a hand-off; the commit applies T2 and T3. |
| OP-11 | `swap`'s arguments are not hand-offs; it exchanges the targets' debts (T10) and applies T2 above both. |
| OP-12 | The updated value's argument is a hand-off; the result is sealed. |
| GIVE-1 | A `give` of a value is a hand-off. |
| PRE-1, SCOPE-3 | Replace "a supplied definition promises no generated postcondition" with the parametricity obligation; supplied results are sealed; `release` and `map_clear` arguments are not hand-offs (T5's closed list). |
| SHARE-2 | Every target kind (whole state, map entry, `Entries`) is sealed at block entry and restored on every leaving edge; today only a whole `Shared<S>` or `SharedRead<S>` target of a struct is. |
| RANGE-2 | As built: the affine instances body entry establishes (T13) are path conditions of the range walk, as the generated requirements were; range type invariants keep their function-boundary sites and take no part in sealing. |
| CALL-4, CALL-6 | Generated invariant relations no longer flow as published result relations; sealedness of results replaces them, and the routed generated clauses are removed. |
| DIAG-1 | The restoration rejection's repair and its naming of breaking events. |
| DIAG-2 | Drop "type-invariant-generated" from the postconditions a component member receives as induction hypotheses: sealedness is type-based and needs none. |

Rules that need no change in behavior: OWN-1, OWN-7, REF-1, REF-2, REF-3,
WIN-3, EFF-2 and FN-6. EFF-5 keeps its contract-read footprint and
only rewords "the type invariants its parameters take". The decisions in the design tree's checks
and proofs node that change are the type-invariant scope decision (owed at
hand-offs, kept by any module), the enum-payload rule and its deferral (both
subsumed), and the supplied-definition decision (replaced by parametricity);
"owed at every field write" stays rejected, and candidates B, C and D join
the rejected list with the reasons in the table above.

## Conformance cases

New cases (Firn's witnesses among them). Rule cited for negatives; all
other verdicts accept.

| case | shape | verdict |
|---|---|---|
| `type11-pos-stored-field-holder` | Firn's `Holder`/`Index`: `stored(holder: &Holder)` passes `&holder^.index` to `load`, which indexes a 16-element table | accept |
| `type11-pos-nested-inner-outer` | Firn's `Outer`/`Inner` without the duplicated `Outer` invariant | accept |
| `type11-neg-stored-field-corrupt-then-read` | `set holder.index.value = 31_u64; let at = holder.index.value; table[at]` | reject OP-4 |
| `type11-neg-stored-field-corrupt-then-hand-off` | the same write, then `load(index: &holder.index, ...)`; diagnostic names the write | reject FN-8 |
| `type11-pos-stored-field-restored` | write 31, write 3, then the hand-off | accept |
| `type11-pos-container-element` | `Slots<Index, 4>` filled by `place_back`; `let r = &indexes[i]; table[r^.value]` and `let v = indexes[i].value; table[v]` | accept |
| `type11-neg-container-store-broken` | `place_back(window: &indexes, value: idx)` after breaking `idx` | reject FN-8 |
| `type11-neg-container-element-mutated` | `let r = &indexes[0_u64]; set r^.value = 31_u64;` then hand off `&indexes` | reject FN-8 |
| `type11-pos-container-element-restored-in-iteration` | a counted loop writes `r^.value` through `let r = &indexes[k];` and restores it in the same iteration; hand-off after the loop | accept |
| `type11-neg-container-element-broken-across-iterations` | the loop leaves `indexes[k]` broken; a second loop repairs; hand-off after both | reject FN-8 (names the write and the lost iteration identity) |
| `type11-pos-generic-storage-result` | today's `type11-neg-generic-storage-result`: `pop_last::<Span>` returns a `take_back` element | accept (verdict flips) |
| `type11-pos-option-payload-field` | `Option<Holder>` matched through `&`; `table[h^.index.value]` | accept |
| `type11-neg-option-payload-broken` | a write through the reference binder to `h^.index.value`, then the `Option` is passed on | reject FN-8 |
| `type11-pos-box-content` | `Box<Index>` content read as an index | accept |
| `type11-pos-map-entry-payload` | an atomic statement stores `Some<Index>` into a map entry; another reads it and indexes | accept |
| `type11-neg-map-entry-left-broken` | an atomic block writes the entry's payload field out of range and ends | reject TYPE-11 at the atomic statement |
| `type11-pos-generic-arena` | `Arena<T: drop>` with `next <= entries.len`; `push` places and increments; a reader indexes `entries[k]` for `k < next` | accept |
| `type11-neg-generic-arena-writer-exit` | `Arena` invariant `next == entries.len`; a writer places without incrementing | reject FN-9 |
| `type11-neg-generic-type-parameter-datum` | `struct Pair<T: Int>` with `a <= b` over T-typed fields (replaces `type11-neg-generic-struct`, whose `Pair<T>` with `u64` fields becomes admitted) | reject TYPE-11 |
| `type11-pos-branch-join-restored` | one branch breaks and restores, the other does not touch; hand-off after the join | accept |
| `type11-pos-branch-join-restored-after` | one branch breaks; after the join `set idx.value = 3_u64;` then the hand-off (needs T11's edge instance) | accept |
| `type11-neg-branch-join-broken` | one branch breaks and does not restore; hand-off after the join | reject FN-8 |
| `type11-pos-call-through-write-reference` | `fix(index: &Index) writes(index)` breaks and restores inside; the caller indexes with `holder.index.value` after the call without restating anything | accept |
| `type11-neg-call-through-write-reference-exit` | `spoil(index: &Index) writes(index)` writes 31 and returns | reject FN-9 |
| `type11-neg-u64-helper-breaks` | `bump(x: &u64) writes(x)` writes 31 at `&idx.value`; the caller hands off `idx` | reject FN-8, naming the call |
| `type11-pos-u64-helper-restored` | the same helper, then the caller writes 3 before the hand-off | accept |
| `type11-neg-ancestor-after-callee` | `Holder` with its own invariant relating `index.value` to another field; a callee restores `Index` but breaks the relation; the caller hands off `holder` | reject FN-8 |
| `type11-neg-swap-breaks-ancestor` | `swap` of two `Index` fields of a struct whose invariant orders them, then a hand-off | reject FN-8 |
| `type11-pos-swap-elements` | `swap` of two sealed elements of `Slots<Index, 4>`, then the hand-off | accept |
| `type11-neg-copy-carries-debt` | a copy struct broken, `let c = idx;`, then `table[c.value]` | reject OP-4 |
| `type11-neg-reference-alias-write` | `let r = &holder.index; let s = r; set s^.value = 31_u64;` then `load(index: r, ...)` | reject FN-8 |
| `type11-neg-give-broken` | a `value_if` gives a broken `Index` | reject TYPE-11 at the give |
| `type11-neg-construction-operand-broken` | `Holder(index: idx)` with `idx` broken | reject TYPE-11 at the construction |
| `type11-pos-move-carries-debt` | `set a.value = 99_u64; let b = move a; set b.value = 3_u64;` then the hand-off of `b` | accept |
| `type11-neg-loop-reference-cover` | a reference rebound along a chain inside a loop writes through each target and is rebound before restoring; hand-off after the loop | reject FN-8 |
| `type11-pos-public-unread-field` | the study's module program writing `state.count` from another module | accept (with the field rule) |
| `type11-neg-swap-possible-alias-keeps-debt` | `let r = &c[j]; set r^.value = 31_u64; let x = Index(value: 1_u64); swap(first: &c[i], second: &x);` with i not proved equal to j, then `pass(xs: &c)` | reject FN-8 |
| `type11-neg-swap-joined-reference-keeps-debt` | the same with the first target a reference `value_if` choosing `&a` or `&b`, a debt on only one of them | reject FN-8 |
| `type11-neg-joined-reference-store-keeps-debt` | a reference chooses `&left` or `&right`; `set right.value = 31_u64;` then a sealed store `set target^ = Index(value: 4_u64);` and the hand-off of `&right` | reject FN-8 |
| `type11-pos-joined-reference-store-restores` | the same reference writes 31 through `target^.value`, then `set target^ = fresh();`; hand-offs of `&left` and `&right` | accept |
| `type11-neg-joined-reference-field-store-keeps-debt` | a reference chooses between two `Holder`s; a write through it breaks the holder's own invariant, then a sealed store replaces only its `index` field; the hand-off of one holder | reject FN-8 |
| `type11-neg-map-clear-other-target` | `atomic m1 = &h1, m2 = &h2 { ... break the payload of an entry through m2 ...; map_clear(map: m1); }` with h1, h2 handles of one map type | reject TYPE-11 at the atomic statement |
| `type11-neg-move-up-list-keeps-debt` | `struct L { v: u64; next: Option<Box<L>>; invariant ok(l): l.v < 16_u64; }`; break the second node's v through y, then `set y = move y.Some.value.inner.next;` and hand off y | reject FN-8 |
| `type11-neg-local-set-move-carries-debt` | `set a.value = 31_u64; let b = Index(value: 4_u64); set b = move a;` then `table[b.value]` (T3/T4 order) | reject OP-4 |
| `type11-neg-join-unrefined-payload` | `struct Never { v: u64; invariant impossible(n): n.v < 0_u64; }`, `enum E { A(n: Never), B() }`, e always `B`; `match &e { A(n: n) => { set n^.v = 5_u64; set x = 1_u64; } B() => { set x = 9_u64; } }`, then `table[x]` with a 2-element table (no edge may receive the payload's instance) | reject OP-4 |
| `type11-pos-swap-broken-then-restore` | break `indexes[0]` through a reference, `swap` it with `indexes[1]`, restore through a reference to `indexes[1]`, then hand off `&indexes` | accept |
| `type11-pos-release-broken` | break a local `Index`, then `release(value: move idx)` | accept |
| `type11-neg-fallible-call-straddles-update` | `set t^.next = 99_u64; let v = propagate step(); set t^.next = 0_u64;` in a function writing `t`; the propagated exit hands off a broken referent | reject FN-9 |

Existing cases whose verdict or rule changes with the specification:

- `type11-neg-generic-struct` becomes admitted (its invariant reads `u64`
  fields); it is replaced by `type11-neg-generic-type-parameter-datum`.
- `type11-neg-enum-payload-struct-field` becomes accepted (renamed to a
  positive case): the payload's `Holder.span` is sealed.
- `type11-neg-generic-storage-result` becomes accepted (above).
- `type11-neg-enum-payload-argument`, `type11-neg-enum-payload-rewrap`,
  `type11-neg-enum-payload-returned-construction` and
  `type11-neg-enum-payload-generic-argument` stay refused but at the earlier
  hand-off, the `Some` construction, citing TYPE-11 rather than FN-8 or
  FN-9, where their source wraps a broken value before the call or return.
- `type11-neg-writable-public-field` follows the owner's field rule.
- Every other TYPE-11 case keeps its verdict and rule; the manifest text of
  cases that describe "generated requirements" is reworded to hand-offs.

## Points for the owner

1. **The readonly restriction is not needed for soundness under A′**
   ([fields](#fields-the-invariant-does-not-read)). The requested read-set
   rule is sound; so is dropping the restriction entirely. Which to adopt
   is an encapsulation choice.
2. **A moved value is not handed off when it moves into a local binding**
   (T4): the ruling lists argument, return, give, store elsewhere and atomic
   exit; this note reads a move into a fresh or complete local binding as
   the same value continuing, carrying its debts. It is sound either way;
   treating it as a hand-off would refuse `let b = move a;` after a breaking
   write even when `b` is repaired before any real hand-off.
3. **Two completeness limits, not soundness gaps:** a broken value cannot
   cross an ordinary call by `&S`, and container elements broken across loop
   iterations cannot be restored later. Each reopens only with a real
   program that needs it.
4. **Linked-structure cursors (proposal, recommended in scope now).** A
   loop that walks a structure with a reference rebound each iteration
   (`set cursor = &cursor^.next.inner;`) has a target REF-1 summarizes by a
   cover. As T2, T8 and T9 stand, a write through that cursor creates a debt
   on the cover, which T8 can never restore, even when the same iteration
   restores the relation before rebinding. Proposed refinement: anchor such
   a debt at the cursor's *current target identity* plus the finite suffix
   below it, both of which REF-1 already retains for a summarized
   description; restore it through the cursor's terms (`cursor^.f`); attempt
   restoration before the cursor is rebound or leaves scope, and before any
   write that may change which place its current target identity names (a
   write to a prefix of that target, which REF-2 already treats as
   invalidating) (T8 point 2);
   widen to the cover only if that fails. It is sound because one target
   identity denotes one place for as long as it is current, which is the
   same fact REF-2 uses to keep the cursor valid. It costs one more key form
   per cursor, so if adopted the bound of result 3 holds with C counting
   cursor target identities as well as covers. Recommendation: in scope, because Snowghost's tree and
   arena walks use this shape and the data already exists; it adds one
   positive case (restored before rebinding) and one negative case (rebound
   before restoring, today's `type11-neg-loop-reference-cover`).
5. **Stepwise updates around calls and releases.** Two consequences of the
   hand-off list, stated as accepted costs:
   - a stepwise multi-field update cannot straddle a call that takes the
     struct, nor any fallible call: a propagated error exit hands off the
     written referent (T7), so the referent must be restored before the
     fallible call, or the update reordered after it
     (`type11-neg-fallible-call-straddles-update`). Both are required for
     soundness, since the callee or the caller on the error path relies on a
     valid value; the remedy is the writer's order, not a rule change.
   - releasing a broken value would be refused if `release(value:)` were an
     ordinary call. This note proposes, and T5 includes, a closed exemption:
     `release`, `map_clear` and `swap` are not hand-offs, because their
     supplied definitions only discard or exchange values and no user code
     runs on release [STOR-3]. This is a choice the owner may refuse; refusing
     it costs only those three uses, and T10 then becomes dead (a `swap`
     operand would have to be sealed).
6. Nothing found makes the ruled rule unsound or requires narrowing it.
