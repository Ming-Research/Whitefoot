# Take/replace for affine places — design decision (batch 0070, W2)

> Superseded 2026-08-18: applied at v0.31 activation (eb8e8634). Historical design evidence; the active spec owns the normative text.

Date: 2026-08-17. Authority: the ACTIVE `docs/current-plan.md` W2 under batch
record `docs/ongoing/0070-gap-closure-and-take-replace.md`. This document is
the decision record behind the v0.31 candidate. It selects semantics; the
candidate bytes in `spec/kernel-spec.md` are the sole normative statement.

## 1. The question

How does an affine value leave and enter a container slot — a struct field, a
buffer element, box/arena content — without an uninitialized hole becoming
writer-observable, without implicit destruction of the old value, and without
weakening the obligation-discharge model?

Recorded constraints honored (none overturned):

- STOR-1 (v0.30): SET-1 overwrites only a copy-typed final place; no
  take/replace operation, no temporary hole, no move-from-target exception,
  no implicit destruction. This design supersedes that absence by owner
  approval of the candidate; it does not reinterpret it.
- OWN-1: partial moves kill the whole binding; reinitialization only via a
  new `let`; `move` on copy is an error (one spelling per meaning).
- OWN-5: content reached through any borrow may never be moved (the new rule
  is written as this rule's sole, named exception, with the argument below).
- The arena-index-pool collection basis stays rejected (mcts_mem data-model:
  well-typed slot recycling resurrects use-after-free). Nothing here recycles
  identity; a replace exchanges values under one unchanged owner.
- `buffer<T>` stays affine, single-owner, length fixed at allocation, no
  in-place growth. Growth remains allocate-new + move + release-old, now
  expressible because the struct field holding the buffer can be replaced.
- Assume-without-check stays rejected: the new statement introduces no fact
  without an executed check or a checker derivation, and its kill semantics
  only remove facts.
- No writer-accessible unsafe; safe Rust in the compiler.

## 2. Alternatives weighed

### (a) Atomic replace — SELECTED

`let old = replace p = e;` — the replacement value is required in the same
operation; the old value moves into the fresh binding in the same commit. No
program point has an empty slot.

- Hole: none, by construction. Early `return`/`propagate`/trap between the
  replace and any later statement leaves every container fully initialized.
- Destruction: none is implicit. The old value's sole owner is the new
  binding; its disposition is the ordinary binding lifecycle — moved onward,
  or abandoned and released by the ordinary compiler-derived scope-exit drop
  (STOR-3). The binder is grammatically mandatory: a bare `replace p = e;`
  statement form was rejected exactly because the old value would need an
  implicit drop, which the recorded constraint forbids. The let-RHS shape is
  derived from the constraint, not taste.
- Checker cost: zero new flow state. Target formation, writability, loan
  judgment, and revalidation are SET-1's, reused verbatim; the binder is an
  ordinary OWN-1 binding.
- Fits the calculus: consumption of the RHS is an ordinary `move`; creation
  of the old-value owner is an ordinary fresh `let`. No binding is revived,
  killed, or retyped.

### (b) Typed hole (slot type flows to an Option-like vacant state) — REJECTED

Concrete failure: the slot's type must change per program point, so the
checker needs flow-sensitive per-place type states. That machinery is exactly
what owner ruling D1a's simplification levers exclude (mcts_mem ownership:
frontend-scale checker, lexical explicit regions, singleton holders — T-A).
It also leaks into signatures: a function receiving `&uniq 'r Vec` could not
state whether `deref(v).buf` is whole or vacant without effect-row-like
vacancy annotations on every boundary. Cost lands on every writer and every
rule; the consumers below need none of it. The Option-shaped *value* (not
type-state) reappears legitimately in §6 as the element vacancy story, where
it is an ordinary enum, checked by ordinary match, with no checker extension.

### (c) Swap-only (`swap p, q` exchanging two places) — REJECTED

Concrete failure: after `swap deref(v).buf, nbuf`, the local binding `nbuf`
holds the old buffer — the binding's value changed without `set` (it is
affine, so SET-1 rejects it) and without dying. That is a third mutation path
that breaks OWN-1's "reinitialization requires a new let" and every fact,
loan, and liveness judgment keyed to binding initialization. Repairing it
means either treating `swap` as consume-plus-rebind of `q` — at which point
it *is* alternative (a) spelled worse, with two targets to judge instead of a
target and an expression — or admitting binding revival. Everything (c) can
express, (a) expresses as `let old = replace p = move q;`.

### (d) Closed-scope hole (take legal if provably refilled in-scope) — REJECTED

Concrete failure mode, written out:

```
let x = take deref(v).buf;        v.buf is now a hole
let n = len(x);
let nbuf = buffer_new(n2, 0_u8);  traps on size overflow -> EFF-4: no cleanup,
                                  abort with a hole in v: sound only because
                                  trap is abort — but
let r = propagate helper(...);    an Err edge here LEAVES the scope with the
                                  hole open; the checker must forbid return,
                                  break, propagate, and every trapping call
                                  between take and refill, or synthesize
                                  repair cleanup on those edges
```

So the checker must track per-place hole state across statements (a vacancy
flow analysis — the same machinery as (b) minus the type surface), forbid or
repair every scope-leaving edge in the window, and define what a `&uniq`
holder means while its referent is vacant. All of it buys only the ability to
hold the old and new values at distinct program points — which neither
consumer needs, since both construct the replacement first. Deferred, not
banned: if a real program needs a use-then-refill window, this returns as a
separate reviewed extension; its `.alt` record is this section.

## 3. Selected semantics (normative-ready; the candidate states these as rules)

New statement-level let form, one spelling:

```
let old = replace p = e;
```

- Grammar: `replace_let_rhs := "replace" place "=" expr ";"` as a fourth
  `let_stmt` right-hand side; line-bearing in FORM-2 like the ordinary and
  propagate forms. `replace` becomes a fixed grammar atom (FORM-3 excludes it
  from IDENT automatically).
- Rule [SET-2], beside [SET-1]:
  - Target formation, evaluation order, subscript discharge in target
    position, writability, loan judgment, and post-RHS revalidation are
    SET-1's, by reference. Same closed writability relation (own-rooted
    frame/box/arena/buffer storage, or explicit `deref` of a live usable
    `&uniq` holder; never through a slice; never a const, counted binder,
    bare holder, or conflicted place).
  - The target's final selected type T must be affine (OWN-1) and region-free
    (STOR-5's relation). A copy T is a hard error (spell `set`; read the old
    value bare). A region-bearing T (slice/arena at any depth) is a hard
    error: a slice binding's static origin set and an arena's confinement are
    fixed at initialization, and admitting replacement there would make both
    flow-sensitive. (Slice/arena-typed *content* cannot occur in storage —
    STOR-5 — so this bites only direct local bindings.)
  - The RHS must produce exactly `own T` (TYPE-5 exactness, as for `set`).
  - Commit: one read of the old value into the fresh binding and one write of
    the replacement into resolved(p), with no intervening writer-observable
    program point. If the RHS traps, no commit occurs and the binding is
    never created; the place still holds the old value (EFF-4 abort follows).
  - The commit is NOT an OWN-1 consuming use of the target root: the root
    binding stays live, no partial-move death occurs. The moved-out value's
    sole owner is the new binding, an ordinary `own T` binding thereafter.
  - Through a `&uniq` holder this is the sole exception to OWN-5's
    no-move-through-borrow sentence. Soundness argument: that sentence exists
    because a move through a borrow would leave the owner on the far side of
    the region holding a hole it cannot see. A replace commit never exposes a
    hole at any program point; the far-side owner still owns exactly one
    valid T in that slot afterward. Exclusivity (OWN-5) already guarantees no
    other path can observe the slot during the statement.
  - Effects: the commit is one read and one write of the target's ultimate
    storage origin under EFF-2's attribution (SET-1's commit is one write;
    the read half is the old value leaving).
  - Drops: a successful commit derives no drop, release, finalizer, or
    cleanup edge (STOR-3): nothing is destroyed. The old value's later
    release, if abandoned, is its binding's ordinary scope-exit drop.
  - DIAG-2: the checked program retains the target path, each discharged
    target check, the RHS value, the revalidation, the read-out, the
    write-in, and the binding initialization.

### Element access for affine-element buffers

- TYPE-2 widens buffer element *formation* to: T copy, or T region-free
  affine. Construction stays gated per operation: `buffer_new` keeps its
  copy-only fill row (duplicating an affine fill value is unsound — that is
  the recorded reason the copy restriction existed), and a new operation

  ```
  buffer_vacant<T>(n) : (u64) -> own buffer<Option<T>>   allocates(heap), traps
  ```

  allocates a flat buffer of n elements, every element `None()` of
  `Option<T>` (T region-free, written type argument; OP-9's u64 size-product
  trap applies over sizeof(Option<T>)). The compiler mints the n `None`
  values; no source value is duplicated. T1 (total initialization) is
  preserved — there is no partial occupancy, no occupancy metadata, and no
  new proof obligation: vacancy is an ordinary enum value, inspected by
  ordinary `match`, taken and installed by ordinary [SET-2] element replace.
- `array<T, N>` element formation is left copy-only: no consumer forces it,
  and widening it costs a second constructor story for zero demonstrated
  need.
- STOR-3: the drop of a `buffer<T>` with affine T is each element's
  compiler-derived drop in ascending index order, then the one heap free.
  For every element type constructible in v0.31 (`Option<T>` with T
  region-free and framewise-trivial release) the element drops are empty and
  the composite action remains one free. The compiler stops explicitly
  (unsupported capability, not invalid source) on an element type whose drop
  is nontrivial until the drop-loop lowering exists.

## 4. The hole-vs-facts analysis (ENT-5 interaction — the novel part)

The entailment fragment's exposure to replacement is exactly one soundness
hazard: a stale length fact. `buffer<T>` length is "fixed at allocation", and
ENT-5 leans on that twice — the support of `len(P)` is P's root path *minus
P's element storage*, so an element write never kills a length fact. A
whole-buffer replace changes the length behind the same place spelling:

```
let cap = len(deref(v).buf);          cap = len(deref(v).buf)      [say 8]
let nbuf = buffer_new(2_u64, 0_u8);   len(nbuf) = 2
let old = replace deref(v).buf = move nbuf;
set deref(v).buf[4_u64] = 9_u8;       obligation 4 < len(deref(v).buf):
                                      the stale fact would discharge it and
                                      the accepted program writes out of
                                      bounds — memory unsafety, not a missed
                                      optimization.
```

Resolution, stated as the kill rule and verified against the checker's
implementation:

- ENT-5 kill event (a) now reads: a SET-1 *or SET-2* commit whose resolved
  target overlaps (OWN-7) the resolved place of any support member. Because
  `len(P)`'s support member is P's root path excluding element storage, this
  gives exactly the right two behaviors with no new machinery:
  - whole-place replace of P, of a prefix of P (the containing struct, box
    content), or of anything overlapping P's non-element path kills `len(P)`
    facts — the stale-length hazard above rejects (the obligation is
    re-derivable only from post-replace facts);
  - an element-position replace (`replace deref(v).buf[i] = ...`) is element
    storage, excluded from length support, and kills no containing `len`
    fact — exchanging an element never changes the length, and the loop
    walkthroughs below depend on `cap = len(deref(v).buf)` surviving the
    per-element takes.
- Ordinary place-term and goal supports need no new text: a replace target
  can never be a fragment-typed term's place (fragment types are integers,
  integers are copy, copy targets are rejected), so only the overlap path
  through prefixes fires, identically to a `set` commit on a sibling place.
  The conservative root-overlap behavior of the existing kill machinery
  (a write under a root kills length facts of everything under that root)
  is retained unchanged; it is conservative in the safe direction.
- The RHS `move b` is an ordinary ENT-5 (c) consuming kill of b's facts, in
  the existing order (target judged, RHS effects and kills, commit kills).
- Establishment: the commit establishes NOTHING. No `len(p) = len(b)`
  transport is added. This is deliberate minimality with a recorded
  consequence: a shape that replaces first and subscripts the new content
  through the container afterward cannot discharge and must re-derive from a
  fresh `len` read (which yields an unrelated term) or restructure to write
  into the local buffer before installing it. Both consumers restructure
  naturally (below). If a corpus program later needs the transport, it is a
  monotone ENT-1 extension mirroring the existing `value_if` delivery
  substitution (take pre-commit relations containing `len(b)`, substitute
  `len(b) -> len(p)`, apply the commit's kills to remaining support, join) —
  recorded here as the designed successor, not shipped.
- FN-9 entry-image stability and provenance: the SET-2 commit joins the same
  kill classification list those judgments already reference; no separate
  rule. Provenance treats the commit as a write component to the target and
  an ordinary value flow from target to binder (the binder's value derives
  from the target's pre-state, the target's post-state from the RHS) —
  conservative composition of the existing Set and Let treatments.
- Claims: unaffected. A replace introduces no fact, so it can neither make a
  claim redundant nor refute one; a claim whose support the commit kills
  simply stops covering later obligations, which is the ordinary ENT-3/CLM
  behavior for any write.

## 5. Consumer walkthrough 1 — growable vector over `buffer<T>`

Copy elements (the generic `T: Int` shape; `u8` gives the byte-string). The
exact code the design admits, with the obligation ledger. Zero claims.

```
struct Vec<T> {
  buf: buffer<T>;
  len: u64;
}

fn vec_push<T: Int>['a](v: &uniq 'a Vec<T>, x: own T) -> own unit
  reads('a), writes('a), allocates(heap), traps {
  let len = deref(v).len;                     S5: len = deref(v).len
  let cap = len(deref(v).buf);                S6: cap = len(deref(v).buf)
  let full = ige(len, cap);                   comparison origin: len >= cap
  if full {
    let ncap = cap + 8_u64;                   S7 (constant k, trap +):
                                              ncap = cap + 8, so cap < ncap
    let nbuf = buffer_new(ncap, 0_T);         S6: len(nbuf) = ncap
    for i in 0..cap {                         S11: i < cap at body entry
      let e = deref(v).buf[i];                obligation i < len(deref(v).buf):
                                              i < cap = len(deref(v).buf)  OK
      set nbuf[i] = e;                        obligation i < len(nbuf):
                                              i < cap < ncap = len(nbuf)   OK
                                              element set kills no len fact
    }
    set nbuf[cap] = x;                        (x is copy T: bare use, no move)
                                              obligation cap < len(nbuf):
                                              cap < ncap = len(nbuf)       OK
    let old = replace deref(v).buf = move nbuf;
                                              SET-2 commit: kills cap's fact
                                              (target overlaps len support),
                                              kills len(nbuf) facts (b moved,
                                              kill (c)); root v stays live
    let nlen = cap + 1_u64;
    set deref(v).len = nlen;                  copy set; no obligation
    return unit;
  }                                           old dropped at scope exit:
                                              compiler-derived heap free
  set deref(v).buf[len] = x;                  obligation len < len(deref(v).buf):
                                              else-branch: not(len >= cap) so
                                              len < cap = len(...)          OK
  let nlen = len + 1_u64;
  set deref(v).len = nlen;
  return unit;
}
```

Points forced by the design and verified in the walkthrough:

- Every subscript writes into the still-local `nbuf` *before* the replace, so
  no post-replace fact about the new content is ever needed — this is why
  the no-transport minimality holds up.
- The replace is a field replace through `&uniq` — the OWN-5 exception in
  action; own-threading (`own Vec in, own Vec out`) also checks, with the
  target rooted at the own parameter.
- Growth is additive (`cap + 8`) because S7's unconditional sum fact needs a
  constant operand. Doubling (`cap + cap` or `cap * 2`) is not derivable in
  the current fragment — see open question Q2.
- On the runtime path not visible to the checker, `len == cap` in the full
  branch and `len <= cap` is the maintained invariant; the checker never
  needs the invariant because every obligation is discharged from `cap`.
- Cleanup on every exit: `old` (the superseded buffer) has exactly one owner
  and one scope-exit heap free; a trap anywhere aborts with no cleanup
  (EFF-4), which frees nothing and corrupts nothing.

## 6. Consumer walkthrough 2 — byte-string append

`Str` is `Vec<u8>` with `buffer<u8>`: the code above at `T = u8` is the
byte-string push; append is the loop over the source. The only new judgment
is the borrow discipline of calling push in a loop:

```
fn str_append['d, 's](dst: &uniq 'd Vec<u8>, src: &'s Vec<u8>) -> own unit
  reads('d 's), writes('d), allocates(heap), traps {
  let n = len(deref(src).buf);
  let slen = deref(src).len;
  for i in 0..slen {
    region 'c {
      let b = deref(src).buf[i];           obligation i < len(deref(src).buf):
                                           needs slen <= n — NOT derivable
                                           (struct invariant); see below
      let u = vec_push['c](v: &uniq 'c deref(dst), x: b);
    }
  }
  return unit;
}
```

The honest finding: iterating `0..slen` needs the `len <= capacity` struct
invariant, which the fragment deliberately cannot state (no struct
invariants, ENT-3). The design-admitted shape iterates the *capacity* with a
value guard, or (as shipped in the demo programs) iterates `0..n` where `n`
is the buffer length and reads `deref(src).len` inside a branch to stop
copying — or simplest and what the shipped program does: append from a plain
`buffer<u8>` source (`for i in 0..len(src)`), which is the byte-string
append's real boundary shape (appending raw bytes), and discharges directly.
The `region 'c` + statement-scoped child reborrow inside the loop body is the
recorded OWN-11 shape (a borrow_expr in a loop names only regions introduced
in that body).

## 7. Consumer walkthrough 3 — affine elements end to end

`buffer_vacant` plus element-position SET-2. `OptVec` stores affine
`Option<u32>` values (any region-free T payload; u32 shown):

```
struct OptVec {
  buf: buffer<Option<u32>>;
  len: u64;
}

fn optvec_push['a](v: &uniq 'a OptVec, x: own u32) -> own unit
  reads('a), writes('a), allocates(heap), traps {
  let len = deref(v).len;
  let cap = len(deref(v).buf);
  let full = ige(len, cap);
  if full {
    let ncap = cap + 8_u64;
    let nbuf = buffer_vacant<u32>(ncap);     S6-parallel: len(nbuf) = ncap;
                                             every element None
    for i in 0..cap {
      let none = None<u32>();
      let e = replace deref(v).buf[i] = move none;
                                             element take: obligation
                                             i < len(deref(v).buf) from
                                             i < cap = len(...); the element
                                             replace kills NO len fact (the
                                             element-storage exclusion), so
                                             cap's fact survives the loop
      let g = replace nbuf[i] = move e;      element install: i < cap < ncap;
                                             g (a None) is abandoned — its
                                             scope-exit drop is empty
    }
    let s = Some<u32>(value: x);
    let g2 = replace nbuf[cap] = move s;     cap < ncap                    OK
    let old = replace deref(v).buf = move nbuf;
    let nlen = cap + 1_u64;
    set deref(v).len = nlen;
    return unit;                             old: all-None buffer, one free
  }
  let s2 = Some<u32>(value: x);
  let g3 = replace deref(v).buf[len] = move s2;
                                             len < cap = len(deref(v).buf) OK
  let nlen2 = len + 1_u64;
  set deref(v).len = nlen2;
  return unit;
}
```

- The take and the install are both SET-2 element commits; at every program
  point every slot of both buffers holds a valid `Option<u32>` owner. The
  "hole" of classic vector-grow is the ordinary value `None()`.
- The old buffer after the loop is all-None; its drop is the buffer free
  (elements trivially droppable). Nothing is leaked and nothing double-freed
  on any exit, including a trap in `buffer_vacant` (abort, no cleanup).
- Read access is ordinary borrowed match, no replace:
  `match &'r deref(v).buf[i] { None() => {...} Some(value: w) => {...} }`
  with `w : &'r u32` per OWN-13.

## 8. The generic boundary (task #39's trigger) — what the consumers force

Forced and shipped: nothing beyond what v0.30 generics already admit. The
copy-element `Vec<T: Int>` is expressible today (generic struct with a
`buffer<T>` field, `0_T` fill, element `set`, field replace). The affine
element demonstration is concrete (`Option<u32>`).

Not forced, analyzed, deferred — a `Vec<T>` generic over *affine* T needs
two things the kernel currently forbids, and take/replace is neither:

1. The move-spelling split. OWN-1 makes `move x` on copy a hard error and
   bare `x` on affine a hard error, and FN-2 re-checks every instantiation
   concretely — so one generic body cannot contain `let s = Some<T>(value: ? x);`
   for both classes; the `?` spelling differs. The smallest candidate
   extension (recorded, not designed here): admit `move` on a place whose
   type is a live unbounded type parameter, re-judged per instantiation as a
   bare copy use when T instantiates copy. That is a one-clause OWN-1
   amendment but it touches the one-spelling-per-meaning law (FORM-1), so it
   needs its own owner decision with a real generic consumer in hand.
2. Affine fill. `buffer_vacant<T>` already covers generic vacancy (its T is
   any region-free type — `buffer_vacant<MyAffine>(n)` is well-formed), so
   creation does NOT block; only the body spelling split does.

Do-not-build note: no trait system, no `Clone`, no per-type capability
lattice. The recorded Rust-anchor research (mcts_mem data-model, ownership)
stays research; nothing of it is imported by this delta.

Implementation finding (2026-08-17): the compiler additionally stops a
generic function carrying region parameters as unsupported (`Generics`),
so even the copy-element `Vec<T: Int>` with a `&uniq` push cannot
instantiate today; the shipped consumer is the concrete
`tests/programs/growable_vec.wf`. That combination is a compiler
capability gap, not a language rule, and is the first blocker in front of
the generic vector once an ACTIVE plan wants it.

## 9. Prepared mcts_mem delta (appendix — to be applied only at activation)

Per the mcts-mem-use skill, at v0.31 activation, in the same change:

1. [mcts_mem/whitefoot/ownership.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership.md) Items: rewrite the SET-1 bullet:
   "`set` overwrites only a writable copy-typed final place; `replace`
   (SET-2) atomically exchanges a writable region-free affine final place
   with a same-typed replacement, binding the old value under the new `let`
   — no temporary hole, no implicit destruction, and the sole admitted move
   of content reached through a `&uniq` holder."
2. [mcts_mem/whitefoot/ownership.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership.md) Facts append:
   `- 2026-08-17 (<commit>) rationale: v0.31 selects atomic replace for the
   §5 take/replace question because the mandatory old-value binder is forced
   by the no-implicit-destruction constraint and the no-hole constraint is
   met by construction; typed holes and closed-scope holes were rejected as
   per-place flow state (D1a levers), swap-only as binding revival. (sourced)`
3. New node [mcts_mem/whitefoot/ownership/affine-replacement.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/affine-replacement.md) with `.alt/`
   members `typed-hole.md`, `swap-only.md`, `closed-scope-hole.md`, each a
   frozen node carrying the §2 failure mode verbatim and paired Moves lines
   (`replaced [[...]]` / `replaced by [[affine-replacement]]`, why verbatim
   both sides). NOTE: these alternatives were weighed in this investigation,
   never live in the code — record them as weighed rivals per the skill's
   "genuinely weighed" admission, citing this DOSSIER as provenance.
4. [mcts_mem/whitefoot/data-model.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/data-model.md) Items: amend the STOR-1 bullet's
   "current fully initialized Copy buffer cannot alone express…" sentence to
   record that whole-value replacement and Option-shaped vacancy are now
   selected (buffer_vacant + SET-2), while spare capacity without vacancy
   values, sparse occupancy metadata, and failure-atomic multi-slot growth
   remain unselected.
5. [mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md) Facts:
   `- 2026-08-17 (<commit>) statement: the SET-2 commit joins ENT-5 kill (a);
   length-fact support already confined to the non-element root path gives
   whole-place replace the kill and element replace the exemption; the
   commit establishes nothing — the len(b)->len(p) delivery-style transport
   is the recorded monotone successor if a corpus program needs post-install
   subscripts through the container. (code)`
6. Run `npx mcts-mem lint` clean before the activation commit lands.

## 10. Open questions (honesty over completeness)

- Q1 (transport): shipped minimality establishes no fact at the commit; the
  substitution transport is designed (§4) but unproven against a real
  program. Trigger: first corpus program that must subscript through the
  container after installing.
- Q2 (doubling): amortized-O(1) growth needs `cap < cap + cap` (or `cap*2`),
  underivable in the fragment (no two-term sum facts). Candidates: an S7
  unsigned-sum clause (`let d = a + b;` trap-mode establishes `a <= d`,
  `b <= d`, strict when the other operand is provably >= 1 — sound because
  trap `+` returns the mathematical sum), or one aborting claim on the
  cold grow path. Shipped programs use additive growth; the demo's
  asymptotics are not the language's ceiling. Needs its own small candidate.
- Q3 (drop loop): affine-element buffers whose elements carry nontrivial
  drops (e.g. `Option<box<T>>` payloads) are formable but stop at lowering
  as explicit unsupported capability until the per-element drop loop is
  implemented. The spec states the general rule; the compiler is behind it.
- Q4 (`len <= cap` invariants): container iteration bounded by a stored
  logical length still cannot discharge against capacity (no struct
  invariants). Recorded, unresolved, orthogonal to take/replace.
- Q5 (whole-binding replace of slice/arena types): rejected here to protect
  static origin sets and confinement; if a consumer ever needs slice
  rebinding, that is a separate origin-set-join design, not a relaxation of
  this rule.

## Recorded optimization note (owner-flagged, 2026-08-18)

The `buffer_vacant` design pays one discriminant per `Option<T>` slot under
the current tag+payload enum layout. Owner-flagged follow-up: the general
niche layout optimization — `None` occupies an invalid representation of
the payload (null pointer for pointer-backed affine handles), making
`Option<T>` the same size as `T` with a zero-byte tag. Pure representation
change below the checked program; sequence it after affine-element buffer
lowering lands, measurement first. Tracked as task #46.
