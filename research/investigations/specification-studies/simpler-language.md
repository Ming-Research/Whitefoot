# How to make the language itself clearer, simpler and more efficient: results

2026-11-12. I read read-only copies of each repository's main (Whitefoot `55b6fd7a7`). I did not build the compiler or run any
test. Everything that needs a machine to confirm is marked "unverified", with what to run. The process is in `process.md` in the
same folder.

Root cause 1 is worked through end to end. Root causes 2 and 3 are worked to "has examples, has a replacement, has options to
choose from". Root causes 4, 5 and 6 are only worked to "pointed out, direction given", and each says what is still missing.

## Conclusion first

Phrases of the form "except for …" really are rare in the specification. The design-tree decision (every fact is stated once, no
rule carries an exception clause) is kept to the letter. The exceptions have not gone away; they are written another way: **a
rule does not say "every X works, except Y". It lists the forms that work, one by one, and ends with a sentence saying "no other
form counts".** Each new way of writing something, or each new position, means going back to the list to add a line. Wherever no
line was added, the writer sees an exception.

- Checked numbers: in the specification, "except" occurs 13 times and "unless" 10 times; "only" 412 times, "never" 200 times,
  "every other" 87 times, "in this version" 12 times (counted over the whole of `spec/kernel-spec.md`).
- Of the 91 entries in the specification change log `spec/log.md`, 26 have as their main content "carry an existing treatment
  over to one more position or one more way of writing". Another 6 do this for one place on the side. 10 close holes, and most
  of these close "a list missed an item" (the entry-by-entry classification is in `process.md` §1.2 A).
- The owner said the same thing in the approval note of the v0.151 entry (`spec/log.md`, the "Owner-approved" line): "the root
  cause is that the checker does not know which values hold runtime resources and that adding exceptions one by one is a
  stopgap".

The longest lists, and the ones patched most often, are not in the grammar and not in ownership. They are in the proof chapter
(§15) and around it. Counted by rule, the rules most often patched by "add one position" or "close one missing item" are [ENT-2]
10 times, [ENT-3] 7 times, [PAR-2] 6 times, [ENT-5] 6 times, [FN-9] 5 times, [RANGE-2] 5 times.

The root causes are these, ordered by how much they matter:

1. **Facts hang on the "name of a place", not on the "value"** ([ENT-2][ENT-5][MSR-3]). When a value moves, its facts are lost,
   so each way of moving needs a rule. When a place is written another way, it is not recognized, so each way of writing needs a
   rule. Three other layers in the specification (ownership paths, the affine layer, the range judgment) already work by value;
   only the earliest layer does not. Worked through; see section 1.
2. **The same function body is walked twice by two proofs, each keeping its own state** ([ENT-1…6] and [RANGE-2][RANGE-3]).
   What one side can do, the other side has to be told again. Which obligations can be handed from one side to the other is
   listed by shape. Whether a function "takes part" in the range judgment changes the verdict on the same sentence. See section 2.
3. **The same kind of relation is written in five places, and each place admits different shapes** ([FN-8] requires, [FN-9]
   ensures, [TYPE-11] type invariants, [INV-1] loop and local invariants, [RANGE-1] range clauses). `requires` can say
   `a + b <= c`; `ensures` cannot. See section 3. This one is the cheapest to change.
4. **Properties of types rest on lists of names.** Adding one storage shape, `Paged`, changed 36 rules. Three unsoundnesses were
   list omissions. See section 4.
5. **The parallel permission [PAR-2] recognizes loops by how they are written.** It does not reject programs; it only affects
   how fast they run. See section 5.
6. Not a root cause, but a property that magnifies the repair cost of every item above: **"reject what is redundant"**
   ([OP-5][PRF-1][EFF-2]). See section 6.

I looked at all three readings of "efficient". The conclusions:

- **Cost of checking**: the replacement for root cause 1 is expected to remove two things: "make the closure complete before
  every write" and "rebuild the index from L0 to the affine layer whenever a fact changes" (the slowness measured on the status
  board is in these two places). But this has not been measured. There is also one thing the owner should know: the
  [RANGE-3] derivation is an exhaustive case split within the problem-size limits, and this is in tension with the second
  decision in `design/language.md` (the work of checking does not grow exponentially with program size). See section 2.
- **How fast programs run**: where the language itself makes programs slower is the runtime tests, masks and copies that writers
  add to win back lost facts (Halo rechecks bounds after swapping stacks; Firn masks the database number with
  `iand(database, 15)` as a fallback, the latter recorded in the item `keep-type-invariants-through-fields-and-containers`), and
  the loop forms that [PAR-2] does not recognize. Whether proved facts reach the backend is a compiler matter, not a language
  rule matter; I did not look at it.
- **How hard it is for the writer**: in syntax, what trips writers most is the flat form and the layout, but that is a deliberate
  tradeoff by the owner, and those rules themselves are uniform (see section 8). What really makes a writer "need to understand
  the checker's internals to write it right" is root causes 1, 2 and 3.

---

## 1. Root cause one: facts hang on the "name of a place", not on the "value"

### What it is like now

The checker records facts as it walks the function body, for example `i < a.len`. [ENT-2] lays down that the "terms" that can
appear in a fact are mainly **places**: a variable, a field path, a measure (`a.len`). Places are recognized by how they are
written. So three things each need their own set of rules:

1. **When a place is written, the facts about it must be invalidated.** [ENT-5] computes a "support" for each fact (the places
   it mentions, the references and `Box`es passed through to read it, the variables used in subscripts …), and lists four
   classes (a)–(d) of "kill" events: `set`, `swap`, in-place update, a call's write effects, consumption, scope exit.
2. **When a value moves from one place to another, the facts do not follow it; they must be moved on purpose.** The facts speak
   of the "old place", and the old place is then killed. So [MSR-3] has a "placement table" listing five "naming events" that
   move facts; [ENT-5] also has "bounded relation delivery" (`give`) and "conditional payload transport" (the payload of
   `Result`/`Option`); [ENT-3.S5] creates a "commit value" for `set`.
3. **When one place has two spellings, the rules must say when they count as the same.** [ENT-2] has a long passage listing case
   by case: through a reference variable that was not rebound, aliases of range references, two subscripts that "read the same
   variable that was not written", and so on.

[MSR-3] itself spells out the reason for item 2 ([checked], the specification's original text):

> That the datum is minted before the statement's kills and read after them is the whole content of every placement: the event
> consumes or overwrites the source, so without a term with empty support standing between the two places a measured value would
> arrive at its new place with no measures at all, and `let built = move spare;` would lose what the caller proved about `spare`.

That is: the value has not really changed; it has only changed name. But the facts are recorded on the name, so every way of
changing the name needs its own "carry it over" rule.

### The special cases it forces (all checked against the specification's original text)

**"Compiler-owned values" are added one kind at a time.** [ENT-2] lists terms as eleven kinds, (a) to (k). Six of them, (d)(e)(f)
(h)(i)(j), really speak of the same thing — "the value one evaluation produced, which will not change again" — yet each has its
own name and its own identity rule ((i) is the "private root" of an enum payload, not listed in the table):

| How [ENT-2] writes it | What it is | When added |
| --- | --- | --- |
| (d) two capture terms | the values the two endpoints of a `for` evaluate to | — |
| (e) symbolic result datum | the result value in a postcondition | — |
| (f) commit value | the value the right side of a `set` evaluates to | — |
| (f) given value | the value the operand of a `give` evaluates to | — |
| (f) returned value | the value the integer expression of a `return` evaluates to | 2026-10-23 |
| (h) call datum / entry datum / placement datum | an argument's value at the moment of the call / a parameter's value at function entry / the source place's value at the moment of the move | — |
| (j) target-instance measure term | the measure of the slot for the loop's next iteration | v0.101 |

("When added" is filled in only where the change log since v0.81 shows it; "—" means it existed earlier or I did not find it.
There is also (k), the "sum term", added in v0.133; it belongs to root cause 3.)

**The list of "fact moves": a move outside the table carries nothing.** [MSR-3]'s placement table has only five rows: REBIND (the
right side of `let`/`set`/`give` is a bare use of a place), ELEMENT, CONSTRUCT, DESTRUCTURING, PAYLOAD. Every move outside the
table is a "lost fact":

- `swap`: [ENT-5] (a) counts it as "one read and one write of each of the two targets", kills the facts on both sides, and moves
  none. See the small program below.
- Putting in with `place_back`, taking out with `take_back`: the [MSR-3] original text says "a window put into a slot by a window
  operation and taken back out by one therefore arrives with no measures of its own, and a caller that needs one reads it and
  branches". The conformance case `msr3-neg-a-boundary-row-carries-no-measure-through-the-slot` pins this rejection.
- A call's result: [FN-9] has only four "landing sites" (a direct `let` binding, a direct `set` target, each binding of a
  destructuring `let`, an enum's conditional context). When the result goes straight into a struct field or an array element,
  the postcondition has nowhere to land.
- The right side of `return`: before 2026-10-23, `return value + 1_u64;` could not prove the postcondition, while
  `let r = value + 1_u64; return r;` could. That revision added the "returned value", but the original text still says "Every
  other returned expression, a call among them, evaluates to no term".
- The conditional evidence of `Result`: the [ENT-5] original text says "An enum value held in an aggregate field or indexed
  storage, a borrowed selection, and a multi-result call add no conditional transport in this version".
- Putting a value into a `Box`, and `give`-ing a copied struct out of a branch: the status board has an observation for each
  (`boxing-a-value-drops-its-field-facts`, `copy-field-equality-lost-after-give`; these two come only from the status board and
  were not recompiled on main).

**"Kill" is a table too, and it has to be applied to eight things.** The [ENT-5] original text: "The table applies to L0, signed
goals and integer exclusions, live origin links [ENT-4.OT], current affine-image associations [ENT-6], entry-image stability
[FN-9], conditional-result evidence below, and current storage-version transitions [RANGE-2]". Several of the historical holes
were exactly one of these missing a kill: v0.104 (a Bool variable recorded "it is the result of `x < 4_u64`"; this record ended
only when the variable was `set` directly. Writing it through a reference, or a callee writing it, did not end the record, so
main accepted `if inside { return table^[x]; }`, an out-of-bounds read when `x = 7`), and v0.100 (tests made through a rebound
parameter; five programs that should not have been accepted were accepted).

**Below a subscript, only readonly fields are recognized.** [ENT-2] (b): for a place after a subscript to become a term, its last
step must select a readonly integer field, and every subscript must be a variable or a constant. The reason (original text): "an
offset of any other form, such as an element read or a computed value, could come to select another element with no event
killing the term". So a writable field below a subscript is not a term: testing `cells^[0_u64].len > 1_u8` leaves no fact
(pinned by the conformance case `ent2-neg-indexed-writable-goal-datum`); `requires entries^[i].head <= n;` is rejected
(observation `writable-element-fields-are-not-proof-quantities`, only from the status board); the guide teaches the writer to
"copy it to a local first". (PR #540 is opening up integer elements and integer fields inside elements on the current structure.
It is finished but not merged.)

**Before every kill, the closure must first be made complete.** The [ENT-5] original text: "Immediately before every
specification-ordered batch of kills (a)–(d), materialize the complete [ENT-4] least closure while every pre-kill term and goal
remains available; then remove exactly the conclusions whose own support the batch kills". Without completing it first,
conclusions that connect only through the killed place would be lost. So the moment at which the closure is computed became
something the specification must pin down — it affects what is accepted.

### A small program: where it is awkward

Halo swaps two stacks when it switches coroutines. This is its witness program (item `keep-length-and-capacity-facts-after-swap`;
the item's "current state" section was checked against main on 2026-11-12: [OP-11] gives `swap` no postcondition, and [ENT-5]
kills the facts on both sides):

```wf
fn exchange(active: &Box<Slots<u64>>, saved: &Box<Slots<u64>>) -> r: unit writes(active), writes(saved) contract {
  requires saved^.inner.len >= 256_u64;
  ensures active^.inner.len >= 256_u64;
} {
  swap(first: active, second: saved);
  return unit;
}
```

Rejected today: the `ensures` cannot be proved. A person sees at a glance that it is right — the stack of length at least 256 that
was in `saved` is now in `active`. The checker does not see it, because what it recorded is "the name `saved^.inner.len` is
≥ 256". `swap` writes that name, so the fact is invalidated; `active^.inner.len` is a different name, and nobody moves the fact
over. Halo now checks the stack bounds again at run time; the standard library's double-ended queue avoids `swap` and exchanges
whole values instead.

The three options the status board laid out for it (give `swap` a general postcondition; exchange whole values; design a new
linear in-place update) each **write one more rule just for `swap`, this one kind of move**. The next kind of move will bring the
same thing again.

### Replacement: facts speak of values; which value a place currently holds is kept in a separate table

The new rule in a few sentences:

1. **Each evaluation produces a value, and values do not change.** A fact is a relation between values. Once it holds, it holds
   forever; there is no "invalidation".
2. **The checker keeps a separate table: which value each place currently holds.** Reading a place means looking it up in the
   table; if it is not in the table, it gets a fresh value about which only its type range is known. Wholes such as structs and
   arrays are values too; their fields, elements and measures are determined by them (the same whole value, reading the same
   field, gives the same value).
3. **Moving a value only changes the table.** `let`, `set`, `give`, `return`, passing an argument, filling a field in a
   construction, destructuring, a `match` binding a payload, `move`, `swap`, `place_back`/`take_back`: the destination place now
   holds the value the source place held. No fact is produced and no fact is lost.
4. **Writing a place only changes the table.** The written place gets a new value; the whole that contains it is therefore a new
   whole value, whose unwritten parts are the same as before ([RANGE-2]'s wording today: "A write makes a new version defined by
   the old one"); other places that may overlap it and cannot be proved to be different ([OWN-7], the same overlap test that
   "kill" uses today) are erased from the table, and the next read of them gives a fresh value.
5. **A call:** by the effect row, erase from the table the places that overlap the written paths; the postconditions are facts
   about "the arguments' values before the call, the result's value, and the written places' values after the call".
6. **Joins and loop headers as before:** if a place holds the same value in every branch, it still holds it after the join; if
   not, it gets a fresh value, and that value carries the facts that the values of all branches satisfy (the current [ENT-5] join
   rule, "take the weakest bound", unchanged). Places that the loop body may write get fresh values at the loop header, and the
   header invariant speaks of these fresh values (current rule, unchanged).
7. **Whether two places are the same is decided by the resolved path; subscripts are compared by value.** `a[i]` and `a[j]` are
   the same place exactly when `i` and `j` currently hold the same value (or are proved equal). The "value" a reference variable
   holds is the path it points to.

This is not a new invention. Three places in the specification already work this way; only the earliest layer does not:

- [REF-1] (the ownership layer recognizes paths): "An index expression inside a path is evaluated when the reference is formed;
  the path records that value, and later assignments to the variables the expression used do not change it".

- [ENT-6] (the affine layer): "The affine part of one ProofContext contains a current-value image for every live own-mode integer
  binding… A write, consume, or scope exit changes or removes the current binding-to-image map but does not make an already
  proved theorem about the old image false" (the second half is in [ENT-5]).
- [RANGE-2] (the state the range judgment carries as it walks the function body): "each binding's value, each storage location's
  contents as one version… a place denotes one version of one storage location, so a fact never changes meaning. A write makes a
  new version defined by the old one". It already treats `place_back`, `take_back`, copying a whole, and `match` bindings as "the
  value moves over".

So the replacement, stated exactly, is: **make the approach that [RANGE-2], [ENT-6] and [REF-1] already use the only approach**;
the earliest difference-constraint layer (the place terms of [ENT-2], the support and kills of [ENT-5], the transport table of
[MSR-3]) is rebuilt on top of it.

### The same examples, before and after the replacement

| Program | Today | After the replacement | Why |
| --- | --- | --- | --- |
| `exchange` above (`swap` of two boxed windows) | rejected (`ensures` cannot be proved) | accepted, unchanged | `swap` exchanges the values the two places hold; `requires` speaks of that value's length |
| `return value + 1_u64;` with `ensures result <= …` | accepted after the 10-23 revision | accepted, without needing that revision | what is returned is the value of that evaluation |
| `return f(x: n);`, `f` has a postcondition, this function's `ensures` relies on it | rejected ([FN-9]: "a call among them, evaluates to no term") | accepted | same as above; a call's result is also a value |
| `msr3-neg-a-boundary-row-carries-no-measure-through-the-slot`: `place_back` an empty window of capacity 8, take it back with `take_back`, then put one element into it | rejected ([FN-8], the window taken back "has no measures") | accepted | what is taken back is the value that was put in |
| `ent2-neg-indexed-writable-goal-datum`: after testing `cells^[0_u64].len > 1_u8` false, compute `ref^.len + 254_u8`; `len` is a writable field, with no write in between | rejected ([OP-2]) | accepted | both reads read the same field of the same element, which was not written |
| `ent5-neg-field-image-swap`: `requires counter^.count + delta <= 800`, `swap` in a struct whose `count` is any value, then compute `count + delta` | rejected | still rejected | `counter^.count` now holds the value swapped in; `requires` does not speak of it |
| `ent3-neg-s7-relation-dies-with-operand`: after `let low = iand(value, other); set value = 0_u64;`, wants `low <= value` | rejected | still rejected | `low` ≤ the original value of `value`; `value` now holds 0 |
| `msr3-neg-a-parameter-value-written-back-loses-its-entry-image`: `ensures total == count;`, function body `set count = 1_u64; return count;` | rejected ("entry image unavailable") | still rejected, the reason becomes "cannot prove 1 equals `count` at entry" | `count` in `ensures` means the value at entry, and it is always there |

(The "After the replacement" column is derived from the new rule and is unverified; the source and current verdicts of these
conformance cases are checked.)

### Which programs change

**From rejected to accepted** ([inferred]): every program rejected today because "the value moved and the facts did not follow" or
"the same place was spelled another way". Among the status board's observations there are about thirty of these (the second and
fourth groups of `process.md` §1.2 B). Of these, the ones I confirmed against the specification's original text as belonging to
this class are: `swap`, window operations in and out, a call result that does not land on a `let`, `return` of a call, a `Result`
held in a field or element, a writable field below a subscript, the entry value of a rebound parameter. Conformance cases that
pin such rejections must change verdict; of the five I looked at, two change verdict (table above). I did not look one by one at
all 96 `ent5-neg-*`, 29 `ent2-neg-*` and 10 `msr3-neg-*` cases.

**From accepted to rejected**: none becomes rejected for safety reasons, but two "reject what is redundant" rules will turn some
programs accepted today into rejections:

- [OP-5]: an `if` whose outcome the checker can decide is an error. Once facts are kept, the tests writers added to win back
  facts become redundant. The runtime bounds check in Halo after swapping stacks, and the two zero tests in the ring-buffer tests,
  will all be required to be deleted. Halo-wf PR #71 is an instance of this effect: one compiler upgrade made 265 tests
  redundant, and with them 87 `let`s, 3 `return`s and 2 effect rows had to change.
- [PRF-1]: writing a `use` certificate for a goal that automatic derivation can already prove is an error. Places that needed a
  certificate before may no longer need one.

The owner should know this: **as long as these two rules stand, every time the checker gets a little stronger, a batch of existing
programs must change.** This is a property of the current design, not something peculiar to this replacement (section 6 deals
with it separately).

### Why it is still safe

The argument has two halves, and each has only one thing to guarantee.

- **The fact half.** A fact speaks of a relation between some values. A value is the result of one evaluation, and once fixed it
  does not change. So a fact is true at the moment it is established and stays true forever after on this execution path; later
  writes change "what a place holds", and cannot change a value already computed. Facts come from the same sources as today (the
  branches taken, the entry `requires`, the operation table [ENT-3.S7], verified postconditions, proved invariants), and that each
  source is true when established does not change.
- **The table half.** The table says "place p currently holds value v". What must be guaranteed is: as long as p is still in the
  table, it really still holds v. Every action that can change a place's contents is a write in the sense of [EFF-2]; each write
  erases every place that overlaps the written path, with the overlap test of [OWN-7] (erring on the side of erasing more). So the
  places left in the table were not touched. This is the same argument as today's for "kill", except that today it must be proved
  for each of eight things that nothing is missed, and after the replacement it is proved once, for this one table.
- **Joins and loop headers**: a fresh value carries only facts that "the value of every branch satisfies", the same as today.

Several of the holes closed in the past cannot arise by construction under this approach: v0.104 (a Bool's "origin" missed a
kill) — the Bool's value is just "the result of comparing those two values", there is no place in it, nothing to kill; v0.100
(tests made through a rebound parameter) — the path the parameter holds has changed, so what is read is the value on the new
path; old facts speak of the old value and naturally do not match.

Unverified: the above is an argument, not a proof, and it has not been tried on the checker. To confirm, first, in the Lean model,
replace "place terms plus kills" with "values plus a table" and see whether the soundness theorem for fact-state transitions still
goes through, and with how many cases (the item `prove-fact-state-transfer-sound` is meant to prove that each step of today's
"write and kill facts" keeps the meaning of facts; it has not started, so it could prove the new formulation directly, instead of
proving the old one first and then proving again). Second, run all conformance cases on the new checker and look one by one at
every case whose verdict changes: whether those that change from rejected to accepted all belong to the two classes above; and
whether any `*-neg-*` safety case (an old fact cannot be used after a write) has become accepted — one such case overturns the
argument.

### Cost of checking, generated code

**Cost of checking** ([inferred], not measured):

- Removed: the step of making the closure complete before each batch of "kills" (it is forced by the specification, not an
  implementation choice); the support computation for each of the eight things. Several status-board observations about slow
  checking point right here: `fallback-conclusions-copy-and-reclose-the-whole-state`, `a-new-let-term-discards-remembered-closures`,
  `check-time-cubic-in-integer-facts`. Once facts only grow and never shrink, the closure can be computed incrementally.
- Added: there are more values than places (one per evaluation), and without cleanup the difference-constraint table keeps
  growing. The remedy is to clear values that can no longer be reached — for difference constraints, completing the closure and
  then dropping a value loses no information, so whether and when to clear can be left to the implementation without affecting
  what is accepted. Today's "complete the closure before a kill" really does the same thing, except that the specification pins it
  to every write.
- The cost of joins does not change (take the weakest bound for each pair of places).
- What must be run for this to count: before and after the change, on the same machine, run `firn-check-time.yml` (Firn's check
  time) and the conformance suite, and compare the `WHITEFOOT_CHECK_WORK` counters. Check time that does not rise, or rises by no
  more than a limit the owner sets, confirms it; clearly slower overturns it (then look at whether cleanup comes too late).

**Generated code**: proofs are all erased before lowering, so the same program generates the same code. What changes is that
writers no longer need to write runtime tests to win back facts (like Halo's stack bounds check), and the checker proves more
facts, so more can be handed to the backend.

### What it takes to change

- **Specification**: [ENT-2] (the passages on terms and identity rewritten, much shorter); S5, S6, S7, S12, S13 of [ENT-3] (the
  "where it is established" parts; the operation table does not change); [ENT-5] (most of the passages on support, kills, the
  closure before kills, conditional payload transport, `give`'s delivery, relocation write, entry-image stability removed; joins
  and loop headers stay); [ENT-6] (forming images stays; the bridge from L0 to the affine layer goes); [MSR-2], [MSR-3] (the
  placement table and the three kinds of datum go; `entry(p)` stays); [CALL-1] to [CALL-5] (mostly become consequences); [FN-9]
  (the lists of return sites and landing sites go); [GIVE-1] (the distinction between the two kinds of carrier goes). Together
  these rules are now about 150 KB (measured rule by rule); the specification without library declarations and the diagnostics
  table is about 570 KB, so they are more than a quarter of it. The closure rule of [ENT-4], the seven steps of [MSR-4], the
  certificates of [PRF-1] and the operation table do not change.
- **Checker**: the state and flow parts of `compiler/src/semantic/entailment/` (about 54,000 lines, of which the one file
  `state.rs` is 11,490 lines); the prover (`flow/prover.rs`) and the affine part are mostly untouched. `range_judgment/` (about
  16,600 lines) is already this model and can serve as a starting point. This is a large change, not a patch.
- **Lean model**: in `proof/WhitefootProof/`, the parts of `Facts.lean`, `FactsSound.lean`, `Sealing*.lean` and `Checker.lean`
  that concern fact state must be redone; the [ENT-2], [ENT-3] and [ENT-5] lines in `proof/coverage.txt` must be re-matched.
- **Conformance cases**: the `*-neg-*` cases that pin "lost when moved" change verdict to accepted; those where an old fact cannot
  be used after a write stay.
- **Existing programs**: only the places that [OP-5] and [PRF-1] judge redundant must be deleted.

### For the owner to decide

Three options:

- **A: replace the whole layer with "facts speak of values".** As written above. Cost: rewrite the core of specification §15,
  redo the checker's fact state, redo the corresponding parts of the Lean model, change the verdicts of a batch of conformance
  cases, and delete a batch of tests in existing programs that become redundant. Gain: the special cases listed above go at once,
  and adding a new way of moving values or a new way of writing a place access later no longer means coming back to patch lists.
- **B: keep the current structure and complete the lists.** Give `swap` a postcondition, and add one transport each for window
  operations, `Box`, a `Result` in a field, and `return` of a call. Cost: each is a specification revision plus a card, and the
  [MSR-3] table and the [ENT-2] terms keep getting longer; the status-board observation says in its own words "each item needs its
  own acceptance rule and cost evidence". Gain: no large change; patch whichever program gets stuck. This is what the change log
  has been doing all along (26 of the 91 entries from 09-29 to 10-23).
- **C: do half first — write only "subscripts are compared by value, any integer place below a subscript can be a term" and "the
  list of moves is replaced by one general rule (any action that puts a value unchanged into another place carries its known
  facts over)" into the specification, and keep support and kills for now.** Cost: two kinds of identity (place and value)
  coexist for longer, and [ENT-5] does not get shorter. Gain: `swap`, windows in and out, `Box`, `return` of a call and the rest
  are solved at once, with a much smaller change than A.

I lean toward A, in two steps (C first, then A). The reason: three layers in the specification already work the A way, the one
remaining layer will have to line up with them sooner or later, and every list patch (B) adds to the work of lining up later. C
can first test whether "one general rule" works in the checker, and the two sentences it writes into the specification stay as
they are in A.

A would overturn these design-tree decisions (all in `design/language/checks-and-proofs.md` and below it):

- "A place reached through one or more subscripts is a term of the fact language exactly when its last step selects a readonly
  integer field…" (below a subscript only readonly fields are recognized) — in A any integer place works, without this limit.
- The three decisions of `proof-identity.md` (how places through reference variables are recognized, how segment and page
  selections are recognized, how range aliases are recognized) — in A they are replaced by the single rule "by the resolved path,
  subscripts compared by value".
- "…MSR-3's placements carry such a field's value wherever they carry measures, minting a value only for a field whose source
  place is already a term of the state" (only fields that are already terms are moved) — in A there is no transport.

---

In the design tree and the research record, I found no record of "facts speak of values" as a whole-layer approach being put up
for comparison. The scope of the search: I read through `design/language/checks-and-proofs.md` and, below it, `proof-identity`,
`obligation-discharge`, `join-relations`, `certificate-fold` and `redundant-tests`; of `automatic-facts.md` I read only the
openings of its 23 decisions, and `range-facts.md` I only searched for keywords; in `research/investigations/` I read the several
routes of `ordinary-range-obligations`, the alternatives table of `result-proof-transport` and the opening of `writer-lost-facts`,
and did not read the hundred or so others. Those 23 decisions in `automatic-facts.md` are themselves a patch list: one handles
`give` handing over a bare integer term, one handles `give` handing over other types, one handles returned integer expressions,
one handles "integer field places whose root is a binding", one handles keeping the length across growth… The two related
choices I found were both local: in `result-proof-transport` the owner chose "the evidence follows the returned value", but
limited to a local `Result`; `ordinary-range-obligations` compared "the range judgment takes over what ordinary entailment leaves"
(chosen) with "build another set of element terms for ordinary entailment" (rejected, one of the reasons being exactly "it would
build again the model the range judgment already has").

---

## 2. Root cause two: the same function body is walked twice by two proofs, each keeping its own state

### What it is like now

Ordinary entailment ([ENT-1] to [ENT-6]) first walks every function once. Then [RANGE-2]: "The range judgment runs after every
function's ordinary entailment, over one forward walk of each function body that takes part". The range judgment has its own state
(values, storage versions, path conditions, join records) and its own verdict ([RANGE-3]). The two sides are joined by a few
sentences:

- An obligation that ordinary entailment cannot discharge is owed to the range judgment **in a function that "takes part"**, and
  only when the goal's shape is "a comparison, or a conjunction of comparisons, whose sides are range terms" (the second sentence
  of [RANGE-2]).
- The condition for a function to "take part": it has a range clause of its own, or calls a function with a range clause, or
  constructs a struct with a range type invariant, or has a corresponding atomic statement.
- What the ordinary side has must be named item by item to enter the range judgment: "An affine requirement of the function and
  every published [INV-1] target are path conditions"; the relations of the [ENT-3.S7] operation table were added in v0.131; what
  is forgotten after a call was rewritten in v0.161; integer `match` is stated again in [RANGE-2].

### The special cases it forces

- **"Takes part or not" changes the verdict.** In the conformance cases' descriptions, ten say "no RANGE-2 trigger"
  (`tests/conformance/manifest.jsonl`, [checked]), for example `ent2-neg-indexed-writable-goal-datum`: "With no placement source
  and no RANGE-2 trigger, the later unsubscripted reference read has only its type bounds". The case writer had to make sure on
  purpose that the function has no range clause for the rejection to stand. That is, adding an unrelated range clause to a
  function may change the verdict on an obligation elsewhere. [META-2] says "no rule's meaning depends on surrounding context";
  here the letter of the rule does not depend on context, but the verdict does.
- **Whether it can be handed over depends on shape.** `range2-neg-ordinary-disjunction` ([checked] source and description):

  ```wf
  fn need(x: u64) -> result: unit pure contract {
    define below = x < 2_u64;
    define above = x > 2_u64;
    requires bor(below, above);
  } {
    return unit;
  }

  fn probe(xs: &[u64]) -> result: unit reads(xs) contract {
    requires forall different(k in 0_u64..xs^.len): xs^[k] != 2_u64;
  } {
    if 0_u64 < xs^.len {
      let x = xs^[0_u64];
      need(x: x);
    }
    return unit;
  }
  ```

  Rejected. Every element is not equal to 2, so `x` is less than 2 or greater than 2; this is right. But "or" is not "a comparison
  or a conjunction of comparisons", so this obligation cannot be handed to the side that knows the element facts, and stays on the
  side that does not. If `need` were written `requires x != 2_u64;`, it would pass.
- **Order.** Observation `order-between-the-two-proofs-is-unspecified` (only from the status board): in v2h's interpreter loop, a
  call's precondition can be proved only by the range judgment; ordinary entailment walks first, cannot prove it, so does not
  adopt this call's postcondition, and a later loop invariant that relies on this postcondition cannot be proved. Adding
  `invariant small: si < 65536_u64;` before the call makes it pass. The specification does not say which counts.
- **Every capability is stated twice.** Seven change-log entries make the range judgment "also able to do" what ordinary
  entailment can already do: 10-22 (`define` expansion), v0.131 (Bool tests, operation relations, `take_back`), v0.112, v0.161
  (forgetting after a call; the revision itself says "the call was the one coarser place"), v0.138, v0.133, v0.145. The matching
  observations: the two judgments use different contracts for function parameters, each has its own model of "place" (these two
  only from the status board), and type-invariant substitution has five implementations (checked during the migration; all five
  names exist).

### Replacement

With the replacement for root cause 1, the two sides' state is already one and the same (values, versions, the place-to-value
table). So walk only once:

- One walk of a function body, one state.
- Every obligation goes through the same column of proof steps: first the current seven steps of [MSR-4] (cheap, no case split);
  if they cannot prove it, then the [RANGE-3] derivation.
- A fact proved by either step is a fact in this state, usable by both kinds of step from the point where it was proved.
- No "takes part or not"; no "which shapes of obligation can be handed over" — as I read step 4 of [RANGE-3] ("The clause holds
  when every branch is contradictory"), its derivation is "negate what is to be proved, put it together with what is known, and
  derive a contradiction in every branch"; the negation of "or" is "and", which it can handle.

The same examples: `probe` above is accepted (`x != 2` contradicts `x >= 2`, `x <= 2`); v2h's loop does not need that added
`invariant`; of those ten "no RANGE-2 trigger" cases, every one the [RANGE-3] derivation can prove changes verdict to accepted
(which ones, I did not derive one by one).

### Cost, and one thing the owner should know

The [RANGE-3] derivation is an exhaustive case split: "Each branch is split on an open item into one branch per case … until a
branch is contradictory or has no open item". The design-tree decision in `checks-and-proofs/range-facts.md` sets limits only on
problem size (4096 atoms, 256 instances per fact), not on work. Within these limits, the number of branches can in the worst case
multiply with the number of "unsettled items". And the second decision in `design/language.md` says the work of checking does not
grow exponentially with program size. I found no explanation of how these two hold at the same time today (in the compiler design
tree, `design/compiler/range-judgment.md` has one decision that sets a depth limit on loop nesting and reports "unsupported" beyond
it, with the stated reason of avoiding exponential cost; there is nothing for the case split). How large the branch counts are on
real programs is unverified: add a temporary counter to the derivation in the checks of Snowghost and v2h (how many branches each
problem opens), and look at the maximum and the distribution.

How this relates to the replacement: today only obligations of a suitable shape, in functions that "take part", enter this
derivation; after the replacement it is every obligation in every function that the cheap steps cannot prove. A program that will
be rejected runs the derivation one more time before being rejected (rejected programs are already slow today: one status-board
observation says a Halo module rejected by the redundancy rules was about 12 times slower than when accepted; the cause was not
attributed).

### For the owner to decide

- **A: one walk, one state, one column of steps, for every function.** As written above. The shortest rules; more programs
  accepted; the derivation reaches further, and its cost must be measured first.
- **B: one walk, one state, but the derivation is used only when the goal reads the storage that some live range fact speaks of.**
  "Takes part or not" shrinks from the whole function to the obligation itself: the verdict looks only at this obligation and the
  facts at that point, not at what is written elsewhere in the function. The cost profile is about the same as today. One sentence
  more of rules than A.
- **C: no change.** Write the conditions for "taking part" and the order of the two walks into the specification (they are blank
  now); otherwise as before.

I lean toward B: it removes the one thing that should least exist, "adding an unrelated clause changes a verdict", without
widening where the derivation reaches; once the branch counts are measured, see whether to go on to A. Both A and B presuppose the
replacement for root cause 1. They overturn the qualifier "in a function that has range facts" in the `range-facts.md` decision
"In a function that has range facts, an ordinary integer-comparison obligation… is proved by the range judgment at the same
point".

---

## 3. Root cause three: the same kind of relation is written in five places, and the shapes each admits differ

### What it is like now

"A relation between two quantities" has five places to be written in the language, each with its own rules:

| Where it is written | Rule | What can be written ([checked] original text) |
| --- | --- | --- |
| `requires` | [FN-8] | any pure, totally defined Bool expression; one side can be a whole linear expression. Original text: "a requirement side carries the whole affine expression and is not narrowed to the difference-bound fragment a published relation is" |
| `ensures` | [FN-9] | only one comparison; each side "one datum plus a constant", with at most one side also adding an integer parameter with coefficient one and no projection. Original text: "These are the complete admitted side shapes" |
| type invariant | [TYPE-11] | one comparison; each side "one datum plus a constant, or only a constant"; the struct cannot have generic parameters and cannot be opaque |
| loop-header and local `invariant` | [INV-1] | linear expressions with literal coefficients; the places that can be atoms are a list, which ends with "…are not admitted as affine atoms in this version" |
| range clause | [RANGE-1] | its own set of terms; cannot be written in the contract of a `fn_sig` (parameters of function type, interface members) |

What "data" can be mentioned is also listed separately: [CALL-4] says what a result may mention is "integers or measures reached
through struct fields and the `inner` of a `Box`", "An enum-payload step, a subscript, and a step below a value whose declared type
is a type parameter supply no datum"; a clause with `when` can bind only one field of one variant.

### The special cases it forces

- (k) of [ENT-2], the "sum term": a kind of term built just for the "at most one extra parameter" of `ensures`.
- [ENT-3.S16]: an invariant's conclusion enters the difference-constraint layer only when it has exactly the shape `a - b <= c`.
- [ENT-3.S4]: a comparison in `requires` that does not have this shape enters by a separate "affine image" route.
- The "narrow receiver route" of [FN-9]: a separate route opened just for the form `set x = f(x, …)`.
- The design-tree rejection "Multi-field routed clauses… reopen when a program needs a cross-field relation".

### A small program

The standard library's bulk append (observation `contracts-cannot-state-sums-variants-or-element-fields`; checked against main's
specification during the migration, word for word):

```wf
fn append<W, X>(destination: &W, source: &X) -> result: unit writes(destination.free), writes(destination.len), writes(source.filled), writes(source.len) contract {
  requires source^.len <= destination^.cap - destination^.len;
  ensures destination^.len >= entry(destination)^.len;
  ensures destination^.len >= entry(source)^.len;
  ensures source^.len == 0_u64;
};
```

`requires` can state the difference of two quantities. But after the append, "the destination's length equals the sum of the two
original lengths" — the sentence this function most needs to say — cannot be written in `ensures`, because it is a relation of
three quantities. So it can only give two lower bounds. A caller that wants the exact length has to read it again; the observation
records that when the double-ended queue changed its storage, it "could only count loop iterations itself to prove the exact
length" (only from the status board). Task 07 in the writer-trial records is the same kind: `ensures when Success(value: v,
next_pos: p)` needs to relate two fields of one variant, and there is no legal way to write it, so the agent added a runtime test
instead (kit's `tests/doc-trial/trial1-2026-10-13.md`, which records it as "a limit of the kernel's expressiveness").

### Replacement

The first sentence of [MSR-5] already means this: "A contract clause is the relation an invariant already is, over a wider operand
set". Make it real:

1. **One way of writing.** All five places write the same kind of expression: one comparison between two linear expressions
   (coefficients literal or named consts), which can be joined by Bool operations. A range clause adds `forall … when …` around
   it.
2. **One "what can be mentioned".** Starting from the values visible at this position (parameters, results, `entry(…)`, an
   invariant's binders, a range's bound variables), the integers reached through fields, a `Box`'s contents, measures, the payload
   of the variant `when` selects, and subscripts (a subscript itself being this kind of expression).
3. **One "prove".** Proving a clause means handing it to [MSR-4] (as `requires` is handled at a call site today).
4. **One "obtain".** A proved clause becomes a fact, always as [ENT-3.S4] does for `requires` today: take the Bool structure
   apart; what has the difference shape enters the difference constraints; other linear relations stay as a premise for automatic
   derivation.

The same examples: `append` can write `ensures destination^.len == entry(destination)^.len + entry(source)^.len;`, and the caller
has the exact length without reading again; the two fields of task 07 can be written in one `when` clause.

**Which programs change**: only from rejected to accepted (more clauses can be written; callers of a library that writes stronger
postconditions can prove more), plus the knock-on effect described in section 6 (the caller's former re-reads and tests may become
redundant).

**Why it is still safe**: an `ensures` is published only after it is proved at every return site; publishing a proved relation,
whatever its shape, introduces no false fact. The consistency check of [CALL-6] (a set of postconditions cannot contradict itself)
stays as it is; it already says today that "shapes the closure cannot represent contribute no premise".

**Cost** (not measured): each call gives some more "premises" rather than difference constraints. The AUTO of automatic derivation
[ENT-6] tries every pair of premises, so more premises means quadratic growth; that status-board observation itself says "the
inference may grow quadratically with the number of premises, not measured", and there is also
`invariant-proof-time-grows-with-entry-inequalities`. To measure: the check time of Firn and the conformance suite after adding the
exact sum to library functions like `append`.

**What it takes to change**: in the specification, the passage of [FN-9] on "side shapes" and the passage of [CALL-4] on "supplies
no datum", the two sentences of [TYPE-11] on the relation term, the atom list of [INV-1], (k) of [ENT-2], [ENT-3.S16]; in the
checker, forming postcondition templates and establishing them at call sites (`entailment/flow/postconditions.rs`, 3,668 lines).
An order of magnitude smaller than root cause 1. "Integers reached through subscripts and payloads" in step 2 depends on root cause
1 (or on PR #540, which first does the element part on the current structure); steps 1, 3 and 4 do not depend on it and can be
done first.

### For the owner to decide

- **A: do all four steps.** The five places become one.
- **B: do only "`ensures` as wide as `requires`".** Change one passage of [FN-9], remove (k) of [ENT-2]. Things like `append` can
  be written right away. Type invariants and the `invariant` list stay as they are for now.
- **C: no change; wait until some library operation gets stuck and then add that one shape** (the status board's current plan:
  "whichever place a library operation needs first is done first").

I lean toward B first, then A. B is the item in this study that costs least and gains most: the rules get shorter (one kind of
term fewer, one passage of shape lists fewer), the standard library's contracts can say everything, and it does not touch how
facts are stored.

---

## 4. Root cause four: properties of types rest on lists of names

**Now**: storage shapes (`Array`, `Slots`, `Ring`, `Segments`, `Paged`), `Box`, shared objects and host handles each have their
own properties: whether they can be subscripted, whether the subscript is linear, whether a contiguous range can be taken, whether
release must do something, whether moving them invalidates references, whether a guard can read them… The specification does not
state the properties once at the type and have other rules refer to the property; instead each rule lists its own "which types I
accept" list.

**Evidence** ([checked]):

- v0.108 added `Paged`, and its `Rules:` line lists 36 rules. Today 23 rules name `Paged`, 23 name `Slots`, 19 name `Ring`, 17
  name `Segments`.
- Three unsoundnesses were list omissions: v0.151 (the "types whose release must do something" list missed `KeySet` and
  `ConcurrentHashMap`, and references dangled after a tail call); v0.124 (the placement table missed `swap`'s exchange, and after
  a block header was swapped out the access was out of bounds); v0.136 (the table of "host handles that change by themselves"; a
  guard reading one would hang; v0.160 added to the table again). v0.151 is the entry where the owner said "adding exceptions one
  by one is a stopgap".
- Where the lists disagree with each other, the writer sees an exception: the subscripted accumulator of [PAR-2] accepts only
  `Array` and `Slots`, not `Paged` (observation `histogram-loops-over-paged-storage-stay-serial`, current state checked); `Ring`
  cannot take a range reference ([REF-4]); sequences of runtime length cannot be compared with `==` (observation
  `sequences-of-runtime-length-cannot-be-compared-with-equals`, current state checked; Firn hand-wrote a byte loop for this).

**Direction**: in [TYPE-9] (and for host types at their declaration in [PRE-2]), list once for each type which properties it has,
in one table; other rules say only "a type with such-and-such property". Adding a type changes one row of that table and nothing
can be missed; rules cannot differ in how they treat the same type unless there is a real reason (that cell of the table states
the reason).

**What is still missing**: I did not actually build the "rule × shape" table. The next step is to copy every list named in these 23
rules into one table, and see which cells differ for a reason (for example, `Ring`'s subscript wraps around, so it is not a "linear
subscript") and which differ only because it was not done at the time. Only with this table can one say which rules must change
and how much. This item does not depend on the earlier ones; it is mainly a reordering of the specification, and accepted programs
widen only in the cells that were "not done at the time".

---

## 5. Root cause five: the parallel permission recognizes loops by how they are written

[PAR-1][PAR-2] are permissions, not obligations: when a permission cannot be proved, the loop runs in order and the program is not
rejected ([ERR-4] and [PAR-2] both say so). So it does not affect what is accepted, only how fast things run under `--par`.

[PAR-2] is about 21 KB and works by how things are written: a scalar accumulator must be exactly `set acc = acc op x`; for a
subscripted accumulator `set C[e] = C[e] op x`, later revisions (v0.107, v0.114) added, one by one, "first copied to a
`let c = C[e]`, with c used exactly once", "the result first put in a temporary t, with t used exactly once", "the update written
in a callee", "constant markers"; an element's subscript must be `a*i + b`; a range reference must be `&r[s*i+b..s*i+b+s]`. Six
revisions each added a few forms (v0.81, v0.97, v0.102, v0.107, v0.114, v0.121), and two of them gave as their reason "how many of
Snowghost's 32 loops it recognizes". The status board also has about fifteen observations of the kind "written this way, it is not
parallelized" (most only from the status board).

**Direction**: with root cause 1 done, "accumulation" can be stated by value: an update of cell C[e] writes in a value equal to
"the value of C[e] before the update op a value that does not depend on this storage". How many copies were made in between, and
whether a temporary was used, need not be mentioned. "Elements are pairwise distinct" already has a proof-based route today (the
`apart` certificate).

**What is still missing**: I did not go further. This item comes last: it does not block programs, and it depends on root cause 1.

---

## 6. A property that magnifies the repair cost: "reject what is redundant"

Three rules treat "writing too much" as an error: [OP-5] (an `if` whose outcome the checker can decide, added in v0.152), [PRF-1]
(a certificate written where automatic derivation can already prove the goal), [EFF-2] (declared in the effect row but not used by
the function body). The design tree gives the reason (`redundant-tests.md`: "executable source should contain only the tests its
proofs still need").

How it relates to exceptions: these rules make "what is accepted" depend in both directions on exactly how much the checker can
prove — what cannot be proved is rejected, and testing what can be proved is also rejected. So:

- Every time the checker gets a little stronger, a batch of existing programs must change. Halo-wf PR #71: one upgrade deleted 265
  tests, 75 of them the same sentence; with them, 3 `return`s became unreachable and 2 effect rows had to change. PR #540 carries
  a card that asks exactly this: once element facts are kept, do the about 20 places in the tests that "test right after writing"
  count as redundant?
- It has grown openings of its own: a generic function body is tested only once, at the symbolic instance (otherwise one instance
  could decide the outcome and another could not); entering a contradictory state lets it pass; option C of that card is "open an
  exception for conditions containing elements".

**For the owner to decide** (what this would overturn is the first decision of `redundant-tests.md`, so it is only laid out here):

- **A: as now.** Cost as above. One thing that can soften it: bundle the widenings of the earlier items into one go, so existing
  programs migrate only once instead of once for each revision every two days.
- **B: still make the determination, but it is not a rejection.** The compiler deletes decidable tests when generating code (so
  they cost no run time) and reports them; a separate tool command deletes the redundant tests from the source — v0.156 split
  layout this way ("an explicit formatting request outside acceptance"). Then a stronger checker no longer turns accepted programs
  into rejected ones. Cost: redundant tests can stay in the source, and the reader (often an agent) will think the case can still
  happen — which is exactly the reason rejection was chosen in the first place.
- **C: as now, but the redundancy rules catch up with what the checker newly proves only at major version releases.** I have not
  thought through how to define this; I only mention it.

I do not lean toward changing it. I lean toward the "bundle into one go" point within A: if root causes 1, 2 and 3 are all done,
put them in the same specification version, so programs migrate once.

---

## 7. Order

| Order | What to do | How much it costs | What it gains | Depends on |
| --- | --- | --- | --- | --- |
| 1 | Option B of root cause 3: `ensures` as wide as `requires` | small: one or two passages of the specification; two places in the checker, forming and establishing postconditions (I did not read the code for the size; it is an estimate) | library contracts can say everything; one kind of term fewer | nothing |
| 2 | Option C of root cause 1: the list of moves replaced by one general rule; subscripts compared by value | medium: part of [MSR-3] and [ENT-2] in the specification; the transport part of the checker | `swap`, windows in and out, `Box`, returning a call, a `Result` in a field, solved at once | nothing (PR #540 already did the element corner) |
| 3 | Option A of root cause 1: replace the whole layer with facts speaking of values | large: about 150 KB of rules rewritten in the specification; the state and flow in the checker's roughly 50,000 lines; the corresponding parts of the Lean model | the four lists of support, kills, transport and identity go; later adding moves or spellings needs no patches | 2 |
| 4 | Option B of root cause 2: one walk, one state | medium: on top of 3, mainly deletion | "takes part or not", the shape threshold and the order problem go | 3 |
| 5 | Root cause 4: one table of type properties | medium: only known after building the table | adding a type no longer misses items | nothing; can be done any time |
| 6 | Root cause 5: the parallel permission stated by value | not estimated | more loops parallel | 3 |

The one that costs least and gains most is "1". The most worthwhile is "3", but it is a large change, and it will collide on the
same rules with two PRs in progress: #484 (type invariants hold for every value, 110 commits, stopped at round 17) and #540
(elements as terms, finished, not yet merged). Both PRs move one corner toward "by value" on the current structure. One thing for
the owner to decide: should #484 be finished on the current structure, or wait for "3" and be done on the new foundation? I cannot
see which saves more — according to the item's description, #484 and its Lean proof (#543) record the "debt" on places (I did not
read `Sealing*.lean`); how much would need to be proved again on a new foundation is a question for the proof study.

---

## 8. Places I looked at and found no problem

- **"One way of writing" in §2 layout and §3 syntax.** The rules are uniform: one table of atom positions ([GRAM-9]), one layout.
  Writers really do trip here (of 44 diagnostics in the first three rounds of the writer trials, 17 were the flat form and 6 were
  layout), but this is the end the constitution explicitly says may be sacrificed, not a rule carrying exceptions. One unevenness
  the flat form brings — the right sides of `let`, `set`, `give` and `return` each need a proof rule — is a symptom of root cause
  1; it goes away when root cause 1 is replaced, with no change to the syntax.
- **The core of ownership and references** ([OWN-1], [REF-1] to [REF-3], [OWN-7]). A few principles: one owner; a reference is a
  local name for a path; overlap means one is a prefix of the other; references do not escape the function. A subscript in a path
  takes its value when the reference is formed, and does not follow the variable afterwards ([REF-1] original text); this layer is
  already by value. The "R.**" summary of cursors in loops is rather convoluted, but there is only one set of it. [REF-2] has one
  place that calls itself an "exception" (writing into an existing leaf of a basic type does not invalidate references); it has a
  reason and its scope is stated clearly.
- **Effect rows** ([EFF-1] to [EFF-5]). One substitution rule plus pairwise comparison. Its "both ends match" belongs to section 6.
- **The closure of [ENT-4], the seven steps of [MSR-4], the AUTO of [ENT-6], the certificates of [PRF-1].** Fixed finite families;
  each user hands in a goal and gets a result. The opening sentence of [MSR-4], "One numeric goal disposition, shared by every
  consumer… an operation adds a goal, never a route", is exactly how other places should be: the using end is already unified;
  what is not unified is the end that produces facts (root causes 1 and 3). AUTO uses at most two premises; one status-board
  observation says the reason is not recorded; it is a tunable number, not an exception.
- **The operation tables** ([OP-1], [OP-2], [OP-6], [OP-8]). They are tables, uniform; 100 pairs of conversions in one table.
- **The [RANGE-3] decision steps themselves.** The four steps are a complete method; the problems are its seam with ordinary
  entailment (root cause 2) and the upper bound of its cost (the tension described in section 2).

**Not looked at closely**: §13 concurrency ([SHARE-1…3], [WAIT-1…3], [HOLD-1]; [SHARE-1] and [WAIT-2] appear often in the change
log, but classified entry by entry most are new capabilities, not list patches), §11 modules, §12 diagnostics, §14 libraries and
host modules, snapshot export ([SNAP]). I did not myself search the source of Firn, Halo and Snowghost for code written around the
rules; I used the examples already recorded in status-board observations, items and Halo PR #71.

---

## 9. What was checked and what was not

- **Checked (read the original text in the read-only copy of main)**: the specification sentences and rule numbers quoted here;
  the `spec/log.md` entries quoted; the source and descriptions of five conformance cases, the ten "no RANGE-2 trigger" in the
  manifest and `range2-neg-ordinary-disjunction`; rule sizes, code line counts, case counts; the design-tree decisions quoted.
- **Checked by others, quoted by me**: facts in items' "current state" sections and in observations marked "checked during the
  migration" (why `exchange` is rejected today, the contract of `append`, the state of PRs #484 and #540).
- **Only from the status board, not recompiled on main**: whether the small programs in most observations are still rejected
  today; the several sets of check-time measurements; the v2h example.
- **Inferred, unverified**: each example's verdict after the replacement; "accepted programs only increase, never decrease (apart
  from the knock-on effect of section 6)"; the direction of the checking cost; the size of the changes.
- **Not checked**: see the end of section 8.

To turn the inferences into confirmations, the cheapest step is to do "2" of section 7: it is small, it can run all conformance
cases in CI, and the list of changed verdicts it shows is the first test of the argument for root cause 1 — the changed verdicts
should all be of the "lost when moved" class; if any case of the "an old fact cannot be used after a write" kind becomes accepted,
the argument is overturned.
