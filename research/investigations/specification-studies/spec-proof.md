# The proof of the specification itself: what blocks it, and the cheapest way forward

2026-11-12. I read read-only copies of each repository's main (Whitefoot 55b6fd7a7), the branches of PR #532 and #523, and the
commit history of `spec/log.md` and `proof/`. I did not build Lean and did not run the compiler. Every conclusion is marked
[checked] (with where I saw it), [inferred] or [not looked up]. The process and the commands that reproduce it are in
`process.md`.

## 1. What matters most: three things block the whole proof, and none of them is that Lean cannot express it

**1. The proof now rests on exactly the part of the rules most likely to change; the one unfinished piece of phase 1 is also in that part.**

Where things stand [checked, `proof/coverage.txt` and each line's hash computed against the specification]: of the specification's
719 KB, 128 KB (17.8%) is marked "modeled", and 84 KB of that is in section 15 (how facts arise, how writes kill them, how merges
keep them: ENT-2 to ENT-6, MSR-4, INV-1). The finished phase 1 proof is 8,370 lines, plus about 4,700 lines in PR #532. Most of it
is properties of a checker written sentence by sentence from these few rules.

A real example [checked, commit history]: on 10-23 the specification changed one small point, so that `return value + 1_u64;` can
prove a postcondition just as `let r = value + 1_u64; return r;` can (#534, specification +8/−7 lines). That day the proof side
changed in four places: the model changed about 58 lines; the merged lemma file `GenGrows.lean` gained one lemma; the obligation
proof then being written (#536) changed +32/−41; the simulation then being written (#532) changed +75/−22, and it gained a new
transform to prove, `returnedValue`, and a new set of premises, still unproved today.

On 11-12 the "make the language simpler" study listed its first version of "rules that will change" (section 2 of its
`process.md`). The first root cause it found is "facts attach to the names of places, not to values", and it leans toward
attaching them to values. The rules it marks as "high confidence" to change are ENT-2, ENT-3's S5/S7/S12/S13, ENT-5, FN-9, MSR-2,
MSR-3; those at "medium confidence" are ENT-6, INV-1, CALL-1 to CALL-6, TYPE-11. Computed against the model's coverage marks
[checked]: of the 128 KB modeled, the "high confidence" rules take 51 KB (40%); adding the "medium confidence" ENT-6, INV-1 and
CALL-6 gives 73 KB (56%). The rules it marks "not planned to change" are OWN-1, OWN-7, REF-1 to REF-3, EFF-1 to EFF-5, ENT-4's
closure, MSR-4's steps, and ENT-6's DIRECT and AUTO.

The one piece of phase 1 not yet started ("state transforms": `bindLet`, `setVar`, `joinStates`, `loopHead`, `returnedValue` and
others, 12 transforms, each needing a proof that it "keeps the facts holding") proves "which facts still hold after a write". That
is exactly ENT-5 and those ENT-3 rows, right in the middle of the rules that are to change.

The four later phases all build on phase 1's two theorems, and the two theorems wait for these 12 lemmas. So today the whole proof
in practice stands still on one piece of work not yet started, and the object of that work may be replaced.

**2. "The whole specification" has no boundary. The five phases do not cover the specification.**

The five phases are: the fact core, references and effect rows, ownership and release, windows, concurrency. The rules below
belong to no phase [checked the size of the rules and the number of cases; "belongs to no phase" is inferred against the "left
for later phases" list in the research record, which lists only names and assigns no phases]:

| Without a home | Size in the specification | Conformance cases citing it | Relation to safety |
|---|---|---|---|
| RANGE-1 to RANGE-5 (facts of the form "every element of a run of storage …", with a separate check of their own) | 29 KB | 347 (more than SHARE's 277 and WIN's 86) | the facts it proves can discharge subscript checks; PAR-2's `apart` clause is RANGE-5 |
| PRF-1 (`use` proof steps) | 9 KB | 58 | the writer gives proof steps and the checker verifies them |
| Generics: FN-2 to FN-6, MSR-6 | about 16 KB | a large part of the FN family's 979 | every prelude type and function is generic (`box_new::<u8>`, `Option<T>`) |
| Enums, `match`, `Result`, `propagate`, `give` | about 12 KB | ERR 61, GIVE 58 | payload steps; ownership moves with the branch |
| Recursion and tail calls: FN-6, FN-10, and v0.145's "a call within one component of the call graph receives the callee's postcondition as its induction hypothesis" | about 5 KB | | the v0.151 defect lay in tail calls |
| Floats, conversions, constants: OP-3, OP-6, CONST-1/2 | about 10 KB | | the "no silent overflow" claim lists OP-6 and CONST-1 |
| The prelude's 47 function declarations (29 with contracts) | 14 KB | PRE 280 | the contracts are trusted; the model must give each function an implementation and prove it meets its own contract |

And the item's "wanted result" says "in the coverage list, every rule the safety claims page lists is marked modeled".
`docs/safety-claims.md` lists 31 rules in all [checked], and RANGE, PRF-1, FN-2, TYPE-11, EFF-2, REF-1, PROV-6 (the rule where the
v0.151 defect lay) and MSR-1 to MSR-3 are not among them. Read literally, that sentence lets the proof be declared complete while
most of the specification is unmodeled; read by the title "The whole specification is proven sound", it means far more than the
five phases.

**3. The specification writes only the checking half; the running half is missing more and more in later phases, and for concurrency even the sentence to prove is missing.**

In phase 1 this gap is still small: OP-2 and OP-4 say how integers, arrays and calls run. Later on, no rule defines "a place
exists at runtime", "a run of storage has been freed", or "two contexts touch one place at the same time". The safety claims page
says so itself [checked]: the data-race guarantee "the specification mentions only in CAP-1 …this page reads it through the rules
below, and this reading is this page's, not any rule's". The model can fill this in, but each thing it fills in decides a sentence
for the specification, and that is the owner's job. Section 4 gives a way to define the concurrency half. Its conclusion is that it
can be defined now, without waiting for the new rules around atomic statements to land, and without Iris.

**The cheapest way forward (leaning; section 5 expands):** first do the three things that are useful however the rules change
(set the boundary of "whole"; write the running half and each phase's statements as a runnable interpreter and theorem statements
in Lean; fill the specification gaps already found). Do phase 1's remaining 12 lemmas and #532 after the "make the language
simpler" study reaches its conclusions.

**The things for the owner to decide are in section 8, six in all.**

## 2. One example worked through: from the specification's sentences, "a close of a handle takes the state" cannot be proved safe

This point belongs to phase 5 (concurrency). It shows what each later "the specification cannot prove it" looks like: what the
rule says now, which step the proof cannot get past, and how it can be filled in.

### What the rule says now

A shared object's state belongs to no binding and lives through its handles (`Shared<T>`). [SHARE-1] says how a handle ends (main
and the branch of PR #523 have the same text):

> `shared_close` consumes its `Shared<T>` argument and ends that handle. Releasing a handle ends that handle. The handles of
> one object end in one total order [WAIT-2]. When the last to end is a close, that close returns `Some` holding the object's
> state, which then belongs to its caller, and every other close returns `None`. When the last to end is a release, the state
> is released once no atomic statement on the object is executing.

[WAIT-2] says this order is "an input of the execution", that is, either may come first. [SHARE-2] says an atomic statement reads
the handle once when it begins, and after that:

> The object stays live until the statement completes, whatever its block does with the handle place [SHARE-1].

"whatever its block does" is meant seriously: the conformance case `share-pos-handle-moved-in-block` moves the handle away inside
the block with `let gone = move cell;`, then still reads the state through the binding, and its verdict is accept [checked, the
case on the #523 branch].

### A program

```wf
fn bump(cell: Shared<Box<u8>>) -> seen: u8 pure waits {
  let seen = 0_u8;
  atomic state = &cell {
    // ① inside the block, end this handle of its own (release is not a waits function, so the block may call it)
    release::<Shared<Box<u8>>>(value: move cell);
    // ③ the block still reads the object's state through state
    set seen = state^.inner;
  }
  return seen;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let boxed = box_new::<u8>(value: 7_u8);
  let cell = shared_new::<Box<u8>>(value: move boxed);
  let other = shared_share::<Box<u8>>(shared: &cell);
  let seen = spawn bump(cell: move cell);
  // ② another context closes the remaining handle
  let last = shared_close::<Box<u8>>(shared: move other);
  // if last is Some(value: b), the Box b belongs to main and is released when it leaves scope
  return std::process::exit_status(code: seen);
}
```

Read by the specification sentences above: the order in which handles end is an input, so the order ①, ②, ③ is one permitted
execution. ② is "the last to end", and it is a close, so it returns `Some`, and the state (that `Box<u8>`) belongs to `main` from
then on; once `main` lets go of it, it is released. ③ is still reading it at that time. The specification gives release "once no
atomic statement is executing", and gives close no such wait.

[inferred] This program will be accepted: every step uses a form pinned down by a case, but the whole program has not been run on
the compiler.

### The step where the proof cannot get past

Among the invariants phase 5 must carry is this one: "while an atomic statement executes, the state its binding points to is
owned by no binding, and other contexts cannot touch it". The proof shows, for each kind of step, "this step keeps the invariant".
At the `shared_close` step, the model is written from [SHARE-1]'s sentence, "when the last to end is a close, hand the state to the
caller", while at that moment `bump`'s statement is still executing. The invariant breaks, and this lemma cannot be proved. It is
not a lack of proof technique: these sentences taken together really permit that execution.

### The implementation is in fact right; the specification is missing a sentence

- The design tree `design/language/waiting/shared-objects.md` has this decision [checked]: "The object stays live until the
  statement completes whatever its block does with the handle place, because the statement holds a handle of its own, or, for a
  linear state, because every handle the block may move away still names the object, a linear handle ending only in a close that
  no block contains". That is: when the state is droppable, the executing statement counts as a handle itself; when the state is
  linear, a handle can end only by `shared_close`, and close is `waits`, which a block may not call, so the block cannot end it.
- The compiler does this [checked the source, not run]: `compiler/src/semantic/check/control/atomic.rs` computes `kept_by_place`;
  when the block may touch the handle and the state is not linear, the statement holds one extra count of its own.
  `atomic_statements_count_a_handle_only_when_the_block_may_lose_it` in `compiler/src/lowering/tests.rs` lists which forms count
  as "may touch".
- So in the implementation, for the program above, ②'s close is not the last, it returns `None`, and the state is released when
  the statement completes. **The specification's text does not have the sentence "the statement counts as a handle itself"**; it
  has only the conclusion "The object stays live". The Lean model is written sentence by sentence from the specification, not from
  the design tree and the compiler.

This is the same kind of thing as the [ENT-6] sentence phase 1 found (the image of immutable data at a merge): the compiler does it
right, the specification is missing a sentence, and read literally it is unsound.

### Three ways to fill it in

| | Fix | The program above | Cost |
|---|---|---|---|
| A | Write the design tree's sentence into [SHARE-1]: when the state has the drop capability, an executing atomic statement counts as a handle of this object from its beginning to its completion, and it ends in the way a release does | ② returns `None`, and the state is released when the statement completes | one or two more sentences in the specification, no verdict changes, the compiler is untouched. In the model a statement is "a third thing that ends a handle", and the proof has one more case |
| B | Make close wait the way release does: when the last to end is a close, return `Some` only once no atomic statement on this object is executing | ② returns `Some` after `bump`'s statement completes, and `main` gets the state | the runtime must change (today it returns `None`); when the statement's guard stays false, close waits forever. The benefit is that "the last close always gets the state" is the same for linear and non-linear states |
| C | Forbid a block from consuming its own statement's handle place | rejected at ① | overturns the design tree decision quoted above (what it rejected is exactly "read the handle place again when the block ends"); `share-pos-handle-moved-in-block` changes from accept to reject. The benefit is that the statement need not hold a handle itself; the compiler's analysis of "may the block lose the handle", with its ten or so cases, is no longer needed, and the proof need not prove it either |

Leaning A: it is what the design tree has already decided and the compiler already does; it only needs writing into the
specification. C is cheapest for the proof, but it overturns a settled decision; whether that is worth it is for the "make the
language simpler" study and the owner to judge. It is recorded in the section of `process.md` addressed to that study.

### Unverified

To confirm, on a machine with the #523 branch's compiler: build this program and run it repeatedly under the thread sanitizer. If
`last` is `None` every time and the sanitizer reports nothing, the implementation follows the design tree (the specification still
lacks this sentence). If `Some` appears, or the sanitizer reports a read after release, the implementation has a hole too, and then
it is a soundness defect and not only a missing sentence in the specification.

## 3. What each of the four later phases must prove, where it is hard, and where it cannot be proved

First, the shape the four phases share [checked, PR #532's description and the research record's additions]. #532's proof is
built like this: one lemma per kind of statement, saying "if the checker accepts this statement, and the state the checker records
on entry to this statement holds on the running frame, then taking this step does not stop in a state with no meaning, and the
state still holds on every path leaving this statement"; loops and calls go by induction on steps. Each later phase does two
things: it adds one such lemma for each new kind of statement, and it thickens the sentence "the state holds" by one layer. After
the thickening, the old statements' lemmas must be proved again to show they also keep the new layer. So the four phases do not
sit side by side.

**The order between the four phases [inferred, from how the rules cite each other; the item says "the four later phases sit side
by side, not confirmed"]:** windows use references (range references REF-4, window operations invalidating references OP-10, the
window parts of effect rows WIN-2) and ownership (WIN-3's moves and releases); concurrency uses references (effect rows, the
overlap judgment), ownership (handles are owned values, moved into spawn), windows (PAR-2's per-index accumulators over
`Array`/`Slots`) and RANGE-5. So the theorems form a chain: references → ownership → windows → concurrency. Only each phase's
syntax, interpreter and checker model can be done side by side. The order the owner set agrees with this chain. There is one
exception: the three sentences of the ownership phase that mention neither facts nor references (live bindings, the unique owner,
release exactly once) are not on the chain, and section 5 says they can come earlier.

### Phase 2: references and effect rows

- **What to prove.** A reference the checker considers valid points at runtime to an existing place of the right type (REF-2).
  Every place a call reads or writes at runtime lies under a path obtained by substituting the arguments into the callee's effect
  row (EFF-2, EFF-5). Two places OWN-7 judges not to overlap are two disjoint runs of storage at runtime. With writes through
  references, phase 1's "every fact holds" still holds (ENT-5's kills (b)(c), CALL-1 to CALL-5). The 7 premises that sealed-value
  validity (#543) carries are proved away here, giving "every value an accepted program hands over satisfies its type invariant".
- **What the model must add.** Structs and fields; a reference variable is a path (REF-1); `&`, `^`; calls with reference
  parameters, and effect rows; entry values and exit values in contracts (MSR-3, a 14 KB table split by "parameter mode × whether
  the effect row writes × place"); how many places a reference points to at a merge, and in loops "below some prefix, depth
  unknown" (the second half of REF-1).
- **Where it is hard.** Three judgments use each other: OWN-7 relies on facts to prove two subscripts differ, ENT-5 relies on OWN-7
  to decide which facts to kill, and a reference into a window relies on a numeric bound to stay valid. The research record already
  says these three can only be proved together in one induction. REF-1's loop-head summary is a process of "propagate until nothing
  changes"; the model must write it as a fixed point and prove "any result it stops at is safe".
- **What cannot be proved as currently written.**
  1. Two points in TYPE-11.S3 [checked: #543's research record, and the rule text at the head of the #484 branch, 07d856f2c]. The
     kill row says "first offers that debt for restoration", without saying that debts this statement itself just added are not
     included; the `swap` row says "moves each debt proved at or below f onto s", without distinguishing whether the target
     resolves to one member or several. Read literally, both let through a program that hands over a bad value (the record has a
     counterexample program for each). The Lean component took the fixes; **the rule text on the #484 branch is still unchanged
     today**.
  2. The sealed-value component is not checked against main's specification [checked]: the 3,971 lines of `Sealing*.lean` have
     not one coverage mark, and `TYPE-11.S` appears 0 times in main's specification. What it proves is the 63 lines of rules on
     the #484 draft. If #484's text changes again, the coverage check will not report it; someone must track it by hand.
  3. The prelude's contracts are trusted (the research record's incidental finding 11). A self-contradictory contract would let
     every obligation of its callers through. The model must give every prelude function it uses an implementation and prove that
     the implementation meets the contract and effect row the specification writes; there is no item for this step now.

### Phase 3: ownership and release

- **What to prove.** At every program point, the bindings the checker considers live are exactly the bindings that have values at
  runtime (LIV-1). Each run of heap storage has only one owning path at any time; on every edge leaving a scope, the release of
  each still-live value runs exactly once (STOR-3); there is no access after release and no double release (REF-2 plus PROV-6's
  "non-empty release" list, plus FN-10's tail calls).
- **What the model must add.** `move` and dead bindings; `Box`; the release traversal derived from the type (PROV-6's executable
  release graph); every kind of edge leaving a scope (`break`, `continue`, `give`, the error edge of `propagate`, return, tail
  call); destructuring; `match` on own enums; the capability bounds of type parameters (`copy`, `drop`, none written).
- **Where it is hard.** The release graph is set by a list of types (`Box`, `Paged`, `Shared`, `SharedRead`, `Frozen`,
  `ConcurrentHashMap`, `KeySet`, `DecisionCell`). In v0.151 the list missed two types, and a tail call carried a reference to freed
  storage across. The research record's remedy is to write one description per prelude type and derive every list from the
  descriptions, so that a missing entry fails the proof. The release depth of a recursive type is a runtime quantity.
- **What cannot be proved as currently written.** I read nothing new. Two points need the owner to state the scope: whether
  `nodrop` (a linear value is consumed exactly once) counts as a safety claim to prove (the safety claims page mentions it only
  indirectly, in the OP-14 row); and the judgments a generic body "makes only once" (OWN-1: "That spelling judgment is made once per
  written body: at a concrete instance of a generic template it is not re-made") need a separate lemma saying "if the symbolic
  instance passes, every concrete instance is safe". That lemma belongs to generics, and generics have no home (section 1, item 2).

### Phase 4: windows

- **What to prove.** `len <= cap`, and the slots holding values are exactly the `len` slots starting at `head` (WIN-1); the mapping
  from logical index to storage slot does not collide (`Ring` modulo, `Paged` paging); every window operation (OP-10) keeps these
  two, and invalidates the references it should invalidate; slots outside the window cannot be read.
- **What the model must add.** `Slots`, `Ring`, `Paged`, `Segments`, and their runtime-capacity forms as the content of a `Box`;
  the prelude's 12 window operations (`place_back`, `take_back`, `insert_at`, `remove_at`, `grow` and others, OP-10) and 11
  constructors (OP-13), `swap` (OP-11), in-place update (OP-12), `free_empty` (OP-14); measures as terms in facts (MSR-1 to MSR-3);
  the four window parts in effect rows and in the overlap judgment (WIN-2's `.next`, `.last`, `.filled`, `.free`); range
  references and page references (REF-4).
- **Where it is hard.** WIN-2's overlap is a fixed answer table ("`r[i]` does not overlap `r.next` when `i < r.len` is proved" and
  ten or so other cells), and each cell is one lemma. Reference validity relies on a numeric bound, which is the tightest point of
  that ring of three judgments using each other. A real example [checked, the research record's incidental finding 6]:
  `set b.inner[0_u64] = refill(cell: &b);`, where the call on the right replaced `b`'s block and the assignment wrote into the old
  block; the compiler accepted it at the time and the result was wrong. Phase 1's probe found it, and v0.163 fixed it (evaluate the
  right side first, then fix the target).
- **What cannot be proved as currently written.** I read nothing new. By the density of phase 1 and the sealed-value component
  (see section 7), this phase and phase 2 are each expected to have around ten readings to choose between and two or three points
  unsound as written [inferred].

### Phase 5: concurrency

See the next section. Two points cannot be proved as currently written: there is no definition of "data race" (section 4); and
the `shared_close` sentence (section 2's example). There is also one point of reading: how "an atomic statement takes effect at one
point" applies to a block that does not end, and to decision-cell operations inside the block (the specification says each has
its own point). The model must take the reading "the statement holds the object exclusively from acquiring it to the end of the
block" [inferred].

### The known points where "the specification cannot prove it", together

| # | Rule | Problem | Kind | Status |
|---|---|---|---|---|
| 1 | ENT-6 | what immutable data keeps at a merge is not written; read literally, it accepts a program that overflows | a missing sentence, unsound as written; the compiler is right | item add-the-join-rule-for-immutable-data-images; the sentence to add is drafted, waiting for the owner's approval; the model already follows the compiler's approach |
| 2 | TYPE-11.S3 kill row | debts the statement itself added are taken for "restoration" | the rule has a hole | the Lean component took the fix; #484's text is unchanged |
| 3 | TYPE-11.S3 `swap` row | when the target may point to several places, debts are moved wrongly | the rule has a hole | same as above |
| 4 | SHARE-1 | an executing atomic statement does not count as a handle, so close can take the state away | a missing sentence, unsound as written; the compiler is right (read from source) | found by this study, section 2; fixes A/B/C |
| 5 | CAP-1 | the data-race guarantee is not stated; there are no definitions of place and access | missing definition | section 4 gives a way to define it |
| 6 | ENT-1 | no rule says "the facts at a point hold in every execution reaching that point" | missing definition | the model's `facts_hold` is this sentence; whether to write it into the specification is for the owner to decide (section 8) |
| 7 | SHARE-3 | how "takes effect at one point" applies to a block that does not end, and to operations in the block that each have their own point of effect | ambiguous | the model must take "exclusive from acquiring the object to the end of the block"; no one has ruled on it |
| 8 | 7 points in the checker, 6 in sealed values, where "the text has several readings" | the model took one reading each | ambiguous | recorded in the research record; the specification was not rewritten to match |

## 4. The memory model the concurrency phase needs

### What it is like now

The specification splits the runtime meaning of concurrency into two halves [checked, §13 on the #523 branch].

**The overlap half has a statement.** [PAR-1]: "Under a permitted overlap, bindings and every Whitefoot state place equal the
source-order result", "No overlapped pair reaches one state place except as the permission conditions above admit". The end of
[PAR-2] even writes out the argument (the accumulator's operation is associative, commutative and has an identity). What is
missing is what "state place" and "reach" are.

**The context half does not even have a statement.** It has only parts: a spawned callee has only value parameters (WAIT-3); a
shared object's state "is storage of no binding and belongs to no context", and only an atomic statement's binding can get into it
(SHARE-1, SHARE-2); an atomic statement holds its object's state exclusively, takes effect at one point, and all shared effects
fall into one total order (SHARE-3). No rule says "so there is no data race", and no rule defines a data race.

### A way to define it

Whitefoot has two rules that make this much simpler than in other languages: values contain no references (STOR-5), and addresses
are not observable (STOR-7). So a value is a tree, with no need for addresses; the sealed-value component `Sealing.lean` is already
built this way. On top of this, only five definitions are needed.

**Roots of storage** come in four kinds, matching the specification's own wording one to one:

- a binding in some call of some context ("private");
- the state of a shared object (SHARE-1: "storage of no binding");
- the value of a frozen object, or a `const` (read-only);
- a single-word cell: a decision cell, a cancellation flag, and later `Atomic<T>`. Each operation on it is itself one step of
  shared effect.

**Place**: a root plus a concrete path (fields, payloads, concrete subscripts, `Box` content), reaching a leaf of a basic type or a
descriptor word (`len`, `head`).

**Access**: who, read or write, which place. A write, replacement, move out or release of a place counts as a write access to it
and to every place below it. This is exactly [EFF-1]'s definition of `writes(path)` ("writes, replaces, moves out of, or frees the
storage at that path and everything below it").

**Strand**: a context (WAIT-2), or a branch split off when the implementation executes with overlap under PAR-1 or PAR-2.

**Race**: in some reachable state, two different strands each have a step they can take now, the two steps access the same place,
at least one is a write, and the place is not "a single-word cell".

The machine runs like this: the steps of the strands interleave arbitrarily. An atomic statement is split into three steps: begin (read
the handle place and record which object it is); acquire (can be taken only when no one holds the object and the guard is true at
this moment; taking it records the object as "held by this context"); the steps of the block; end (let go). An access whose root
lies in some object's state can be made only by the context holding that object. SHARE-3's "total order" is the order of the
statements' "acquire" steps and of the single-word operations.

Written in Lean it is roughly this (draft, not compiled):

```lean
inductive Root
  | local  (ctx : CtxId) (act : ActId) (name : String)
  | object (id : ObjId)
  | frozen (id : ObjId)
  | const  (name : String)
  | cell   (id : CellId)

structure Loc    where root : Root; path : List Step
structure Access where strand : StrandId; write : Bool; loc : Loc

structure Obj where
  state   : Val
  handles : Nat            -- includes the one an executing statement holds itself (section 2, fix A)
  holder  : Option CtxId

-- one strand takes one step: the new configuration and this step's accesses
def step : Config → StrandId → Option (Config × List Access)

def Race (c : Config) : Prop :=
  ∃ s₁ s₂ c₁ c₂ A₁ A₂ a b, s₁ ≠ s₂ ∧
    step c s₁ = some (c₁, A₁) ∧ step c s₂ = some (c₂, A₂) ∧ a ∈ A₁ ∧ b ∈ A₂ ∧
    a.loc = b.loc ∧ (a.write ∨ b.write) ∧ ¬ a.loc.root.isCell

theorem accepted_no_race (p : Program) (hp : p.accepts = true) (hentry : p.EntryHolds) :
    ∀ c, p.ReachesConfig c → ¬ Race c
```

### The three things to prove

1. **Between contexts.** In every state an accepted program can reach: a private root is accessed only by the context of the call
   that owns it; a place in an object's state is accessed only by the context currently holding that object; frozen values and
   `const`s are never written. So there is no race between two contexts.
2. **Overlap.** When PAR-1's permission holds for two adjacent statements, run them as two strands from the same state: there is no
   race; and under the same inputs, every interleaving ends with the same result as running in source order. PAR-2 is the same for
   iterations, and the accumulator values are equal by the algebraic properties the rule itself lists. "The same inputs" cannot be
   dropped: memory readings and clock readings are inputs of the execution (WAIT-2's list), and PRE-2 states plainly that the
   readings of overlapped statements' allocations may differ [checked].
3. **One point.** Every execution is equivalent to an execution in which "the steps of each atomic block run together". This is
   the meaning of SHARE-3's "takes effect at one point", and it follows from item 1 plus "two steps that do not interfere can be
   swapped".

### Iris is not needed [inferred]

The research record and the item both note the risk: "Iris-Lean is not yet complete; this phase may need to reopen the approach".
My judgment is that it is not needed. Iris is for programs where "two threads hold references to the same memory and coordinate by
a protocol". Whitefoot's rules exclude such programs: contexts pass no references to each other, and shared state is touched only
inside an exclusive section with block structure. Item 1 is one invariant about the whole state ("whom each place belongs to right
now"), and the way to prove it is the same as in earlier phases: one "keeps it" lemma per kind of step. It uses what phases 2 and 3
have already proved (references point only into their own root or an object held; ownership is unique), and its own new steps are
only spawn, acquire, end, and the copying and ending of handles. Item 2 uses phase 2's "runtime accesses all lie under the effect
row" and "judged non-overlapping means disjoint", plus one general lemma: "two steps with disjoint accesses can be swapped".

To confirm this judgment: write a toy model from the definitions above, with two contexts, one shared object and integer state only
(three or four hundred lines of Lean), and run `lake build` on CI. If the keeping lemmas for the three steps "acquire", "end" and
"spawn" can be proved directly, the judgment is confirmed; if some step can only be proved by bringing in ghost state for "who
promised what", it is overturned.

This boundary must be stated clearly: what is proved here is "the specification says exclusive, so there is no race". How locks are
implemented and how wait queues hand over lie in the trusted base, or in the "is the checker itself right" study.

Two points I looked at and found no problem with, so no one need check them again [checked the specification text]: the clock,
where the effect row of `now(clock: &Clock)` is `writes(clock)`, so two reads on the same handle are not overlapped, and reads on
another handle given by `clock_share` have no order to begin with; and memory readings, where PRE-2 states plainly that readings are
inputs of the execution and that other strands' readings at allocation may differ. These two points show that the words "the same
inputs" in theorem 2 are what the specification itself requires, not something the model adds.

### The three rules still to be added for the first release do not change the definitions [inferred]

The item makes this phase wait for four things to land: #523, atomic values, sharded fields, `until` deadlines. I read the designs
of these three (decided by the owner on 10-14 in the item). The definitions above and the statements of the three theorems need no
change; what each adds is only keeping lemmas for a few steps:

- **`Atomic<T>`**: it is exactly the "single-word cell" kind of root, and each operation is one step. The owner's decision that
  "the mutexes an atomic statement takes must all be taken before its first atomic-value operation" is in this machine already
  "acquire comes before the steps of the block". The "read then write" it introduces is not a data race; the definition already
  excludes this kind of root.
- **`until`**: give the "acquire" step a parallel exit: when time is up (time is an input), go to else, having acquired nothing.
  The binding is not visible in else, so the invariant is not affected.
- **`Shards<T>` fields**: what the block accesses is "the executing thread's own share". At the level of the specification, "which
  share" can be treated as an input of the execution, and the statement still holds the whole object exclusively. The
  implementation letting a statement that touches only its own share not take the whole object is the latitude that SHARE-3's last
  sentence gives the implementation ("An implementation may hold less than a statement's state ... wherever the program's outcomes
  are those it has when every statement's guard and block execute with exclusive access to the whole state"), and its correctness
  is on the implementation's side. One point to note: the specification says today "the identity of the host thread that executes a
  statement ... not observable"; sharded fields make "which share it lands in" an input, so that sentence must change with them
  when the time comes.

So the memory model and the statements of the three theorems can be written and settled now; only the lemmas for the specific
statements wait for the rules to land. **This turns concurrency from "can only be touched last" into "the definitions can be
settled now".**

### The sentences the specification must add (three ways)

Where things stand: the data-race guarantee is a reading by the safety claims page.

| | Way | Cost |
|---|---|---|
| A | Add a rule in §13 (or extend CAP-1): write out the five definitions of root, place, access, strand and race, and the sentence "an accepted program has no race"; add to SHARE-1 the sentence that the statement counts as a handle itself | about ten more sentences in the specification; it is a new normative fact and needs the owner's approval. After that the safety claims page cites this rule and no longer reads it on its own |
| B | Leave the specification unchanged; the definitions live only in the Lean model and the research record, and the safety claims page cites the model's theorem | no change to the specification. But the design tree has a decision that the model "is evidence about the specification and never a definition of the language"; if the definition of race lives only in the model, the model is defining the language |
| C | Do not claim "no data races" publicly; claim instead "contexts share no writable storage; shared state is accessed only in exclusive atomic statements" | no need to define race; theorem 1 is directly this sentence. But the public claim is weaker, and the opening sentence of README and AGENTS.md, "data races ... unrepresentable", must change |

Leaning A. B conflicts with that design tree decision; C changes the public claim, which is a selling point of the first release.

## 5. The dependency on the "make the language simpler" study

### What is useful however the rules change, and what will be redone

That study's first-version list is in section 1. Sorting the modules on main by its list [line counts checked; the sorting is
inferred by comparing the list with each module's description in the research record, without reading each proof]:

| | Module | Lines | Why |
|---|---|---|---|
| Useful | The gatekeeping tools: `Coverage.lean`, `StandardAxioms.lean`, `Conformance.lean`, `proof.yml` | about 600 | unrelated to the content of the rules |
| Useful | The running half: `Syntax`, `Lexer`, `Parser`, `Interpreter`, `Integers`, `Points` | about 2,200 | the grammar and the runtime meaning are outside the scope that study will change |
| Useful | `Typing.lean` (an accepted program does not stop on a type error) | 1,642 | the type judgment TYPE-5 is not among the most-revised rules |
| Mostly useful | `Static.lean` (judgments that use no facts: names, types, the shape of contracts, reachability) | 414 | the contract shape in it follows FN-8 and FN-9, and FN-9 has been revised 5 times |
| Useful | `FactsSound.lean`, `AffineSound.lean` | 965 | they prove mathematical properties of the difference-constraint closure and of linear forms. The record says the closure's proof "needs neither the matrix's shape nor the closure's fixed point" |
| Useful | #532's `Safety.lean` (the framework for safety by step count) | 942 | unrelated to the content of the rules |
| Mostly useful | #532's `Blocks.lean`, `Loops.lean` (the skeleton of statement lists, `if`, `loop`) | 2,077 | depends on the shape of the checker's traversal code, not on the fact rules; the loop-head part depends on "kill" |
| Will be redone | The checker model: `Facts`, `Affine`, `Checker`, `Walk` | 2,400 | written sentence by sentence from ENT-2 to ENT-6 and MSR-4 |
| Will be redone | `GenGrows`, `Holds`, `ObligationSound`, `ValueSound` | 3,886 | they prove properties of the functions in the four modules above; `Holds` is the definition of "the state holds", and it will get shorter once facts attach to values |
| Interface must change | `DischargeSound` (each form of disposition implies the obligation) | 1,877 | that study does not plan to change MSR-4's steps, but the lemmas are stated against `Holds` |
| Will be redone | #532's `Simulation.lean`, `Align.lean` | 1,590 | each statement's lemma takes apart the code of `Walk` |
| Not yet written | The 12 state-transform lemmas | estimated 3,000 to 5,000 | their object is exactly "which facts still hold after a write" |
| Counted separately | `Sealing*.lean` | 3,971 | against TYPE-11.S1 to S8 on #484, not merged; TYPE-11 meeting "a reference that may point to several places" is where errors are densest |

One thing keeps the loss in check: #532's two theorems are derived from an interface (`Sim.Laws`, one field per lemma) and do not
look directly into the checker's internals. After the rules change, as long as the new checker's transforms still satisfy this
interface, the theorem layer need not be proved again; what must be proved again is "the new transforms satisfy the interface".

### In what order, so the least work is wasted

Three ways:

| | Way | Risk of wasted work | Cost |
|---|---|---|---|
| A | Continue on the current plan: finish the 12 state-transform lemmas and #532 first, then do phase 2 | if the owner adopts "facts attach to values", these 3,000 to 5,000 newly written lines plus the existing roughly 7,900 lines must be redone, and 1,900 lines must change interface; if not adopted, every patch of the form "this place is like `let` too" replays #534 once | phase 1 wraps up fastest, and the two theorems arrive earliest |
| B | Pause the rest of phase 1 and wait for that study's conclusions; meanwhile do what is useful however the rules change: set the boundary of "whole"; write the running half in full (an interpreter for references, the heap, windows, contexts and atomic statements, tested with run cases; at the second part it could read only 46 of 836); write out the invariants and theorem statements of the four later phases; fill the specification gaps in the table of section 3 | small. The interpreter and the statements do not change with the static rules | the two theorems arrive later; phase 1's state stays pending for a while |
| C | Stop the whole proof and wait for the rules to settle | none | during this time no proof watches the specification's changes; the design tree has a decision rejecting "a formal model that goes at its own pace", on the grounds that every precedent first drifted and then froze |

Leaning B. Reasons: what B does, A and C must also do later; and the two other studies can use it now. Every replacement the
"make the language simpler" study proposes must argue that "it still holds for safety"; with a runnable runtime semantics and
written invariants, that argument can first be tried by running cases and random programs on the model, before deciding whether the
change is worth making. B does not stop the gatekeeping: the coverage check and the case comparison run as usual. B does not change
the approach the owner set (a faithful executable model in Lean); what it changes is when phase 1 wraps up. It is not the same as
the "model every rule first, then prove" that the design tree rejected: what is written in full first is only the running half (the
sentences that say "how it runs", such as OP-2, OP-4, OP-10, WIN-1, STOR-3, WAIT-2, WAIT-3, SHARE-3), and the checking rules still
come one slice at a time.

B could do one more thing, but it changes the five-phase order the owner set, so it is listed separately: **the part of phase 3
that uses no facts can be proved now.** "The bindings the checker considers live are exactly the bindings that have values at
runtime", "each run of heap storage has only one owner" and "release exactly once": these three sentences do not mention facts. Not
one of the rules they rely on (OWN-1, LIV-1, STOR-3, PROV-6, OWN-11, OWN-13) is on that study's list, and the skeleton the proof
uses (#532's safety by step count and the framework for statement lists, `if` and `loop`) does not depend on the fact rules either.
The research record once compared making it phase 1 ("B. Ownership and local references"); at the time the fact core was chosen,
on the grounds that every later phase needs the meaning of facts, and what was recorded against it is that the release-list and
`swap` defects need constructs outside it. The reason for bringing it forward now is different: it is the only piece of the proof
at present whose rules are stable and that does not rest on rules that are to change. The remaining sentence, "no reference to
freed storage", must wait for phase 2. [inferred]

In the other direction, the leads for that study (which rules forced many cases while writing the proof) are in section 3 of
`process.md`, eight of them. The weightiest [inferred]: seen from the proof, when all facts attach to unchanging values (a write
replaces the mapping, a merge gives a new atom), a fact about a value never becomes false, and ENT-5's 35 KB of "kill" shrinks in
the proof from "does each fact still hold after a write" to a table of "which value each place corresponds to now"; the loop head
still has to prove "the places the loop body did not write have unchanged values". The ENT-6 layer is already like this today.

## 6. The numbers the due review (2026-11-21) needs

### The numbers

The criterion [checked, the Phase 1 section of the research record]: in the month after the gate takes effect, if more than a
quarter of specification versions need model changes, or the median of such changes exceeds one engineer-day, reopen the
approach. Two more sit alongside it: `proof.yml` taking more than 10 minutes at the end of phase 1; and having to choose among more
than ten points where "the text has several readings" while writing the checker.

`proof.yml` merged with #507 on 10-21 at 15:34 (Pacific time). After that, 11 merges changed the specification, matching one to
one the 11 entries in `spec/log.md` from v0.159 on [checked; each merge commit and its changed line counts are in the table in
`process.md` 1.3]:

- 7 versions did not touch `proof/` (#462, #470, #466, #486, #518, #475, #525).
- 1 version changed only one line of the coverage list (#295, one hash).
- 3 versions changed the model: #516 (a rule gap, `Integers.lean` and `Interpreter.lean` ±27 lines in total, 2 commits), #515
  (`set` evaluates the right side first, `Interpreter.lean` ±33 lines, 1 commit), #534 (the return expression, model changed
  about 58 lines, proof +8 lines, 3 commits; plus one change each on the two branches where proofs were being written, 9 hours from
  the first commit to the last).

**The share is 3/11 = 27%, just over a quarter. Each change took far less than one engineer-day.** The advance estimate was 9% to
16%. The count of readings: 7 in the checker, not over ten; counting the sealed-value component's 6 as well, 13, over.

### These numbers cannot be used at face value

- **The sample covers only three days.** main has merged no PR since 10-29 [checked, the remote main head read with `gh` is
  55b6fd7a7], and all 11 versions are between 10-21 and 10-23. This number will not change by 11-21. One version fewer and it is
  under the line (2/11 is 18%).
- **The share follows coverage; it does not measure whether the approach is good.** In these three days the model covered less
  than a fifth of the specification, and for the first two days only the interpreter. When it covers the whole specification,
  every version's change that touches a safety rule will need a model change, and the share will naturally approach all of them.
  The "more than a quarter" line will certainly be crossed then; it cannot measure whether the approach holds up.
- **The proof still barely exists.** When #516 and #515 merged there was not one proof. What it looks like once there is a proof
  has only one sample, #534: 8 lines of specification, bringing four places of proof work.
- **"Engineer-day" cannot be measured for agents.** What the history lets us measure is commit counts, line counts, rounds, CI
  runs and wall-clock time.

### Three ways for the owner to judge

| | Way to judge | Result |
|---|---|---|
| A | At face value: 27% is over the line, reopen the approach | overturns a settled approach on the numbers of three days and three small changes |
| B | Do not judge this time; change the window to "the 30 versions after main starts merging specification changes again", and count then | waits a while longer; the criterion is unchanged, and as coverage grows it will still cross the line because the share rises naturally |
| C | Change the criterion: instead of the share, look at the proof side's cost each time a change touches covered rules (rounds, CI runs, how many proof lines changed, whether any PR was held up by the proof for more than a day), compared with the size of the specification change itself | measures the approach itself; the line must be drawn first |

Leaning B plus C: this time's numbers are not enough to judge; change the criterion to one measured by cost, with the window
counted in versions. Please also decide whether the sealed-value component's 6 readings count toward the "ten".

### If reopened, what the other approaches look like

| Approach | What it looks like | Cost |
|---|---|---|
| Gatekeeping at two speeds | every PR still checks coverage and runs the case comparison (the model must keep up); the proof may lag, the lags are listed, and the list is cleared before release | specification changes are not held up by the proof. It overturns half of the design tree decision that rejected "catching up only before release": the model does not lag, the proof may. The risk is that the lag keeps piling up |
| Certificates | the checker emits the state at each point for each program, and a small verifier proved in Lean checks it. #532 compared this and did not choose it at the time | the verifier's proof is small and need not change however the rules change. But what it proves is "every program that passes the verifier is safe", not "the rules are sound"; it overturns the A chosen by card spec-proof-method, and the public claim must change. It also covers errors in the checker's implementation, connecting with the third study's topic; it is not the same as the unproved "small verifier" that study talks about, which checks against the same wrong rule and cannot catch errors in the rules |
| Freeze first, then prove | the proof works only against one frozen version of the specification, and rejoins the gatekeeping after the major rule changes are done | cheapest. The design tree has rejected a model that goes at its own pace; it can only be a temporary measure during major rule changes, which is section 5's way B/C |
| An idealized small calculus | a small language that captures the gist of the rules | the research record rejected it, and the reason still stands: every defect found was in the details |

## 7. Amount of work

### What phase 1 actually took [checked, the commit history and the counts for each part in the research record]

- **Days.** The first commit to `proof/` was 10-21 02:02, and #532's last was 10-24 12:23: three and a half days, with several
  engineer agents working side by side; after that, only one commit on 10-29 (sealed values). It has stood still since 10-29.
- **Lines.** `proof/` on main is 18,006 lines: the model about 5,000, the phase 1 proof 8,370, sealed values 3,971, the tools about
  600. #532 has about 4,700 more lines of Lean (the PR adds 4,800 lines in all, including the research record).
- **Against the estimate.** The advance estimate was 1,500 to 2,500 lines of model, 6,000 to 12,000 lines of proof, 25 to 50
  person-days. The model came out 2 to 3 times the estimate. The proof already has about 13,100 lines (8,370 plus #532's about
  4,700), still missing the 12 state transforms; finished, it will be about 16,000 to 18,000 lines [inferred], around 1.4 times the
  top of the estimate.
- **Rounds and CI runs.** The research record logs seven parts: parser and interpreter 10 rounds, checker 10 rounds, model fixes
  2 rounds, affine 5 rounds, typing 7 rounds, obligation disposition 9 rounds, sealed values 6 rounds, 49 rounds in all; three
  parts, L0, the generator and the two theorems, are not logged. `proof.yml` ran about 80 times. Every first draft was written
  without a local Lean install, and failing to build on the first one or two CI runs is normal (the typing part had 30 errors the
  first time).
- **`proof.yml` time.** At #532's head the proof build takes 63.8 seconds, against a budget of 85 seconds.
- **As density.** Per KB of modeled specification text: about 39 lines of model and about 130 lines of proof (counted as finished).
  Per 128 KB of specification text: 7 readings, 1 point unsound as written, 13 incidental findings (v0.162 changed 21 rules for
  them). The 63 lines of sealed-value rules: 6 readings, 2 points unsound as written.

### By this measure, how big the rest is [inferred, large error]

The specification text related to safety and not yet modeled is about 390 KB (sections 4 to 10, section 13, the unmodeled part of
section 15, plus the prelude's declarations). Of the covered rules, 40% of the lines are "outside the model" (mostly how
diagnostics are reported); at the same ratio, what really needs modeling is about 235 KB, 1.8 times what is modeled now.

- Model: about 9,000 more lines.
- Proof: at the same density, about 31,000 more lines; but each time the invariant thickens by a layer, the old statements' lemmas
  must be proved again, and the experience the research record cites is that proof size grows with the square of the statement
  size. Counting 2 to 3 times, that is 60,000 to 100,000 more lines. The whole, when done, is 90,000 to 130,000 lines of Lean, 4 to
  6 times what exists now.
- Split across the phases (by the size of the rule text and the number of invariant layers): references and effect rows is the
  largest, about 40% (it also has to bring along structs, enums and generic instances, which everyone uses); windows about 20%;
  ownership and release about 15%; concurrency about 10% (once the definitions are settled, it mainly uses the results of earlier
  phases); those without a home (RANGE, PRF-1, symbolic instances of generics, recursion) together about 15%.
- As person-days and rounds. The yardstick the research record uses is Cedar's 260 lines of proof per person-day. By that
  yardstick, phase 1 when finished is about 65 person-days (the estimate was 25 to 50); the remaining 60,000 to 100,000 lines of
  proof are 230 to 385 person-days, which by the shares above are: references and effect rows 90 to 150, windows 45 to 75,
  ownership and release 35 to 60, concurrency 25 to 40, those without a home 35 to 60. Person-days are not accurate for agents
  (phase 1 was written side by side in three and a half days); the unit that can be measured is rounds: the seven parts with
  logged rounds total about 15,900 lines and 49 rounds, about 320 lines per round, with one or two `proof.yml` runs per round. The
  remaining 70,000 to 110,000 lines (model plus proof) are then 220 to 340 rounds.
- `proof.yml`: extrapolated linearly by line count to 100,000 lines, about 5 minutes; some modules are far above linear
  (`GenGrows.lean`, 748 lines, takes 20 to 29 seconds). The chance of hitting the 10-minute reopen line before finishing is not
  small; per-module caching should be done early.

**What really sets the pace is not line count.** Phase 1's more than 20,000 lines were written in three and a half days; then it
stood still for 19 days, stopped on one piece of work not yet started and one specification sentence waiting for the owner's
approval. At phase 1's density, there are still dozens of readings and ten or so points unsound as written to be ruled on one by
one. Each one stays stuck longer than it takes to write its proof.

## 8. Six things for the owner to decide

1. **What "whole" means.** Three options: (a) every rule that can change acceptance or runtime meaning; then RANGE, PRF-1,
   generics, enums and recursion each need a home, and the amount of work is the full count of section 7; (b) the 31 rules the
   safety claims page lists; then beyond the five phases only a few numeric rules, OP-6, OP-8 and CONST-1, need adding, plus a
   statement that STOR-6 (the target layout) is not in the source language's model; but PROV-6, RANGE and TYPE-11 are not included,
   and the public claim can only be "these 31 rules are proved on a fragment"; (c) define a named language fragment, and state
   publicly which constructs lie outside it. Leaning (a) as the goal and (c) as what the first release can say: the first release
   says "fragment X is proved, and what lies outside it is listed here", and what lies beyond the five phases goes into the second
   version. The owner's decision that "the first release waits for all five phases to be proved, and Lean is not mentioned publicly
   before then" stays; what is asked here is what the public sentence says once the five phases are proved, and where what lies
   beyond the five phases goes. Please also decide whether "`nodrop` is consumed exactly once" counts as a safety claim.
2. **Finish the rest of phase 1 now, or wait for the conclusions of "make the language simpler".** Section 5, leaning B (wait,
   and meanwhile do the running half and each phase's statements). A related question: whether to bring forward the part of phase
   3 that uses no facts, which changes the settled five-phase order.
3. **Whether to write the data-race guarantee into the specification.** Section 4, leaning A (add the definitions and one
   sentence of guarantee in §13).
4. **How to fill in the `shared_close` sentence.** Section 2, leaning A (write the design tree's sentence into SHARE-1).
5. **How to judge the 11-21 review.** Section 6, leaning not to judge this time and to change the criterion to one measured by
   cost; and whether the 6 sealed-value readings count toward the "ten".
6. **Whether "at every execution reaching this point, the facts at this point all hold" should become a sentence of the
   specification.** Now only SCOPE-3's "no undefined behavior" and ENT-1's "a wrong inference is a compiler defect" imply it (the
   research record's incidental finding 1). It is the sentence the whole proof sets out to prove. Written into ENT-1, the
   specification itself says what it guarantees; not written, the sentence lives only in the model. Leaning to write it.

There is one more thing that needs no decision from the owner but that no one is looking after: the two gaps in TYPE-11.S3, where
the Lean component took the fixes and the rule text on the #484 branch is still unchanged (section 3). Before #484 merges, the
text must be changed to what the component proves, and `Sealing*.lean` must be given coverage marks; otherwise the gatekeeping
does not reach it.

## 9. The boundary with the "is the checker itself right" study (my view)

- What this side proves: programs accepted by the checker written in Lean from the specification are safe under the runtime
  semantics written in Lean from the specification.
- What that side looks after: whether the compiler's checker is the checker the specification describes; whether the generated
  code and the runtime are the runtime semantics the specification describes.
- There are four seams, all joined only by tests: the model's checker and the compiler's checker (the case comparison; today the
  model can read 208 of the 3,156 cases); the model's interpreter and the compiled programs (run cases, 46 of 836 at the second
  part); the model's implementations of prelude functions and the real implementations in the runtime; and, for concurrency, the
  "exclusive" primitive and the implementation of locks.
- PAR-1, PAR-2: this side proves "if the permission holds, the two statements do not interfere"; whether the permission the
  compiler computes is the specification's permission, and whether the generated parallel code is right, is on that side.

## 10. Unverified

| What | How to verify | What confirms, what overturns |
|---|---|---|
| Section 2's program is accepted, and at runtime close returns `None` | build with the #523 branch's compiler and run repeatedly under the thread sanitizer | accepted, always `None`, no reports: the implementation is right, the specification lacks a sentence. `Some` appears, or a read after release: the implementation has a hole too |
| Concurrency does not need Iris | write the toy model with two contexts and one object from section 4's definitions, and `lake build` on CI | the keeping lemmas of the three steps prove directly: confirmed. Ghost state is unavoidable: overturned |
| Section 5's "useful / will be redone" for the modules | for each rule on that study's list, `grep` which definitions its `-- spec` marks fall on, then see which theorems cite those definitions | marks falling in "useful" modules: the sorting is wrong |
| Section 7's sizes | after phase 2 finishes its first part, re-estimate using its line counts and rounds | |
| The order of the four later phases is a chain | once phase 2's invariant is written out, see whether the ownership and window lemmas really cite it | |
| Generics, RANGE and others "have no home" | ask whoever wrote the research record; the record has only a list with no phases assigned | |
