# Facts a writer establishes and the checker does not carry

## Question

Four agent-written Snowghost modules (the CSS rules parser, the CSS color
parser, Unicode normalization and the WHATWG URL parser) reported that the
checker loses a fact the writer had just established, and that they worked
around it with re-clamps before a return (`if x > n { set x = n; }`),
duplicated branch code or a changed result type. They reported six shapes:

1. a loop header `invariant` is gone after the loop is left by `break`;
2. `let ok = band(guard, other); if ok { ... }` does not make `guard` usable;
3. `let x = if c { give a; } else { give b; };` loses a relation the
   statement form `if c { return a; } return b;` proves;
4. `ensures when Ok(value: r):` has no `Option` counterpart;
5. a loop invariant chained with a just-returned callee postcondition through
   `use` steps kept failing;
6. facts about arrays grown in lockstep inside a struct do not cross fields or
   calls.

For shapes 1 to 5 this investigation reproduces the loss with the smallest
program, reads the active specification (v0.77) to decide whether the fact is
already entitled (a compiler defect) or not (a language gap), proposes the
smallest rule change for each gap with its soundness argument, and estimates
cost and impact. Shape 6 belongs to the
[aggregate-postconditions investigation](https://github.com/mbbill/Whitefoot/blob/research/aggregate-postconditions/research/investigations/aggregate-postconditions/DESIGN.md)
(PR #169) and is only cross-referenced here. This record edits no
specification or compiler; its proposals are the pending amendments listed at
the end.

## Criteria

A proposal is recommended only when all four hold:

1. **Sound under the existing kill and join rules.** Every delivered or
   retained relation must hold on every execution reaching the point, with the
   ordinary [ENT-5] support kills and all-predecessor joins deciding its
   lifetime; no rule may identify values of different loop iterations.
2. **Inside the fixed families.** No new search, budget, fact family or
   Boolean formula synthesis; [ENT-1]'s derivable set may grow only by rules a
   reader can apply from the text.
3. **Reducible to an admitted spelling where possible.** A change is smallest
   when it makes the checker treat a source form exactly as it already treats
   a slightly longer, accepted spelling of the same program; then soundness is
   inherited from the accepted spelling rather than argued afresh.
4. **Measured impact.** The change must remove at least one clamp that a real
   module needs today. The census below first establishes which clamps are
   needed by deleting each one; that census fixed this criterion before the
   emulation runs that test each proposal.

Recommendations are ordered by measured impact per unit of cost (spec
sentences, conformance cases, compiler lines), with soundness risk breaking
ties.

## Method

All probes ran on the gate `whitefootc` built from this branch's base,
`85e2c89bf4acec9c7bf48bdba4e307ae42be7180` (specification v0.77), with
`whitefootc --check <probe>.wf`. Each probe below also carries the two
`ExitStatus` alias lines and a `main` returning `exit_status(code: 0_u8)`,
omitted here. Diagnostic lines are quoted exactly; lines that repeat the
source or the repair text are omitted.

The clamp census used the Snowghost `trial/run-url` branch at `2377817`.
`pkg::url` was copied to scratch with only the `module.wfm` interfaces of its
two dependencies (`pkg::text::idna`, `pkg::text::normalization`) and checked
with `whitefootc --graph modules.wfg --check-module pkg::url`; the unmodified
copy is `accepted`. Each clamp block was then deleted alone and the module
rechecked; each proposal was emulated by inserting the admitted source that
the proposal makes implicit, deleting the clamp, and rechecking.

## Shape 1: a header invariant after `break`

**Probe.**

```wf
fn skip_spaces(data: &[u8], start: u64) -> result: u64 reads(data) contract {
  requires start <= data^.len;
  ensures result <= data^.len;
} {
  let length = data^.len;
  let pos = start;
  loop (
    invariant bounded: pos <= length
  ) {
    if pos >= length {
      break;
    }
    let byte = data^[pos];
    if byte != 32_u8 {
      break;
    }
    set pos = pos + 1_u64;
  }
  return pos;
}
```

**Diagnostic.**

```text
p1-loop-exit.wf:22:3: error[FN-9]: UndischargedPostcondition
  source:   return pos;
  relation: pos - data^.len <= 0
  disposition: Unproved
```

Inserting `invariant at_end: pos <= length;` before the first `break` and
`invariant at_stop: pos <= length;` before the second makes the same function
accepted (exit 0) with no other change.

**Verdict: language gap.** [ENT-5] states: "Header assumptions are removed on
every edge leaving their loop, while local invariant conclusions follow
[ENT-5]'s canonical control-flow intersection independently of their
proof-only names." [INV-1] makes the header batch "the current-iteration
assumption throughout the body" and nothing more. The compiler implements the
removal exactly: `remove_active_loop_invariants`
(`compiler/src/semantic/entailment/flow/invariants.rs`) drops every affine
fact tagged with the loop's identity from each break state and from the
counted false-header state in `walk_statement`
(`compiler/src/semantic/entailment/flow/walk.rs`, ordinary and counted loop
arms) and on edges leaving several loops in `exit_counted_loops_from`
(`compiler/src/semantic/entailment/flow/events.rs`). The unit test
`ordinary_loop_break_does_not_export_its_header_invariant`
(`compiler/src/semantic/tests/loop_invariants.rs`) pins that behavior. The
first-break edge carries only `length <= pos` in L0 after the removal; the
join with the second break keeps nothing on the `(pos, length)` pair.

**Proposed change.** Replace the quoted [ENT-5] sentence with: header
conclusions and local invariant conclusions alike follow [ENT-5]'s canonical
control-flow intersection on every edge, including every edge leaving their
loop, independently of their proof-only names; a header invariant's name still
leaves lexical scope with the loop body [INV-1]. [ENT-6]'s tie-break "the
retained representative is that predecessor's occurrence with the fewest
active-loop dependencies" exists only so that a loop-independent duplicate
survives the removal, and is retired with it. The counted exhaustion rule of
[INV-1] is unchanged and remains the only binder-free conclusion of the false
header edge; a raw header theorem over the binder's iteration atom rides out
inertly because no live binding has that image after the binder leaves scope.

**Soundness.** [ENT-5] already defines an affine invariant conclusion as "a
theorem over the immutable mathematical value-image atoms captured when that
invariant occurrence was proved". A header conclusion is such a theorem over
the atoms of one arbitrary iteration's head. Every execution that takes a
`break` passed that head in the same iteration, where the batch was proved by
the base or backedge obligation, so the theorem is true of the values those
atoms denote; leaving the loop changes no atom's value. Whether a later goal
can use it is decided by the binding-to-image map, not by the theorem: if the
scan variable is written between the head and the `break`, its image is a new
form and the theorem about the old atom does not bound it (for example
`set pos = pos + 2_u64; break;` would still reject the probe's `ensures`). No
iterations are identified, because the abstract walk visits the body once and
the next head still reintroduces only the proved header batch. By criterion 3
the change adds nothing a writer cannot already obtain: when the variables are
unchanged since the head, AUTO proves the same relation as a local
`invariant` at the break from the one header premise with a zero residual, and
that local conclusion already survives the exit edge.

**Existing evidence it changes.** No conformance verdict: the seven reject
cases that combine an invariant with a `break`
(`inv1-neg-repeated-name`, `inv1-neg-ordinary-cursor-overshoot`,
`inv1-neg-replaced-loop-limit`, `inv1-neg-sequential-guarded-steps`,
`op4-neg-nonstrict-loop-head-index`, `op4-neg-ordinary-loop-exit-index`,
`op2-neg-break-removes-exhaustion-bound`) fail on a repeated name, at a
backedge, inside the body, on a strict bound the retained `<=` does not give,
or on a binding overwritten before its `break`. One compiler unit test flips: the one named above accepts
under the proposal, correctly, because `value` is zero at that `break`.

**Cost.** Specification: one sentence replaced in [ENT-5], one sentence
retired in [ENT-6]. Conformance: two to three cases (the probe as a positive
run case with two breaks; a negative case writing the variable before its
`break`; a labeled `break` leaving an inner loop). Compiler: about 20 changed
lines at the four removal sites, which keep removing only the invariant
names; the `active_loops` field and its join preference (about 30 lines in
`flow.rs`, `flow/domain.rs` and `flow/invariants.rs`) would then serve no
rule and can be removed. Checking cost: each retained theorem enlarges AUTO's premise list for
the rest of the function, as local invariants already do; the maintained
programs' check time must stay within noise.

**Impact.** Measured in the URL module (table below): 7 post-loop upper clamps
become removable, and 4 lower clamps with them when the writer adds a
`start <= pos` header relation. The CSS rules module writes three
`clamp_between` calls after break-exited loops that carry no header invariant
(`renderer/css/rules/scan.wf`); the change is what would let a header
invariant replace them, but that was not measured. The color module's fifteen
`clamp_u64` calls sit in a loop with no header invariant and no `break`, so
this shape does not explain them.

## Shape 2: a conjunction guard

**Probe.**

```wf
fn peek_is(data: &[u8], pos: u64, other: Bool) -> result: Bool reads(data) {
  let length = data^.len;
  let inside = pos < length;
  let ok = band(inside, other);
  if ok {
    let byte = data^[pos];
    return byte == 32_u8;
  }
  return False();
}
```

**Diagnostic.** None: accepted (exit 0). Twelve further variants were also
accepted: `other` bound from a user call; `bor(at_end, other)` with the read on
the false continuation; `bnot(ok)` with the read on the false continuation;
`band` as a loop guard; a measure reached through a reference and two struct
fields; `band` nested twice with two strict bounds used at two subscripts, in
two layouts; the condition renamed through `let go = ok;` and used by a
`value_if`; `bnot` of a `>=` comparison under `band`; a range reference's
`len`; `band` guarding a loop with two header invariants; and the URL module's
own layout (`let c = if more { give input^[pos]; } else { give 0_u8; }`
followed by `band(more, c == 47_u8)` and a call requiring `pos + 1 <= len`).

**Verdict: entitled and implemented; no defect found.** [ENT-3]'s goal-origin
expansion replaces `ok` and `inside` by their defining right-hand sides, S1
establishes the expanded `+band(pos < length, other)` at the then-entry, and
signed Boolean decomposition establishes `+(pos < length)` with its exact L0
projection. The design node
`language/checks-and-proofs/obligation-discharge/goal-decomposition` records
that choice.

Two adjacent shapes do lose the guard, both as the specification says:

```wf
fn has_byte(data: &[u8], pos: u64) -> result: Bool reads(data) {
  let length = data^.len;
  let inside = pos < length;
  return inside;
}
```

used as `let inside = has_byte(data: data, pos: pos); let ok = band(inside, other);`
rejects the read with `error[OP-4]: UndischargedBoundsObligation`,
`residual: pos < data^.len`, because [ENT-3] gives user-function results no
comparison origin and [FN-9] has no Bool result route. And the short-circuit
form

```wf
  let ok = if inside {
    give other;
  } else {
    give False();
  }
  if ok {
    let byte = data^[pos];
```

rejects with the same OP-4 residual, because a `value_if` is no admitted value
expression and so has no goal origin, and [GIVE-1]'s delivery carries only
fragment integers. Either would be the natural spelling of "guard, then read
under the guard".

**Proposed change.** None for `band`. The short-circuit gap has a small
candidate: a `let` whose initializer is a `value_if` with condition C whose
else branch is exactly `give False();` would establish, at the then-entry of a
later `if` on that binding, the members of C's goal-origin set, under the same
no-kill and no-`set` conditions as comparison origin (b); the mirror form with
`give True();` in the then branch would establish C's negation at the else
entry. It is sound because the binding is true only if the then branch ran,
and the support conditions already keep C's operands unchanged since. The Bool
helper gap needs a routed Bool postcondition and is larger. Neither is
recommended now: the four modules contain eleven `give False()`/`give True()`
initializers (URL 9, rules 1, normalization 1); the three URL ones inspected
are decisions rather than guards for a proof, and the URL module's
integer-read layout is accepted today.

**Cost** (short-circuit candidate, if reopened): two to three specification
sentences in [ENT-3], three conformance cases, roughly 60 to 100 compiler lines
in the S1 origin construction.

## Shape 3: an expression-form `if`

**Probe.**

```wf
fn statement_form(pos: u64, length: u64) -> result: u64 pure contract {
  ensures result <= length;
} {
  if pos < length {
    return pos;
  }
  return length;
}

fn expression_form(pos: u64, length: u64) -> result: u64 pure contract {
  ensures result <= length;
} {
  let x = if pos < length {
    give pos;
  } else {
    give length;
  }
  return x;
}
```

**Diagnostic.** `statement_form` is accepted;

```text
p3-value-if.wf:21:3: error[FN-9]: UndischargedPostcondition
  source:   return x;
  relation: x - length <= 0
  disposition: Unproved
```

The literal form `else { give 0_u64; }` against the same `ensures` fails the
same way. Both become accepted when the given value is first copied into a
branch-local binding: `let bound = length; give bound;` and
`let zero = 0_u64; give zero;`. So does giving `pos` in both branches under
`requires pos <= length`.

**Verdict: language gap.** [ENT-5]'s bounded relation delivery takes "exactly
each L0 bound or disequality whose normalized terms contain d" and replaces d
by the receiver x. For `give length;` the only facts containing `length` are
relations to other terms and its reflexive and type bounds, which become
`x - x <= 0` and type bounds on x; the carrier equality x = d is never among
them ("This transport reads no pre-existing fact on x, forms no inverse
`x ↦ d`"). A literal carrier "forms no image" at all. The compiler implements
exactly this in `eligible_delivery_terms`, `delivery_edge_state` and
`value_delivery_image` (`compiler/src/semantic/entailment/flow/sources.rs`).
With `let bound = length; give bound;` the branch-local copy carries
[ENT-3.S5]'s `bound = length`, whose substitution delivers `x = length`; the
two spellings differ only in whether the copy is written.

**Proposed change.** On an eligible `give d;` edge, besides the substituted
relations, deliver the carrier equality: `x = d` (both bounds) when d is a
bare own fragment-integer term, and `x = value(d)` when d is a typed integer
literal or an integer-typed named const, which [GIVE-1] then admits as
carriers. The equality is delivered only when every support of it other than x
survives the edge's own kills, like every other delivered relation. The
substitution itself is kept, because [ENT-4]'s closure has no rule carrying a
disequality through an equality, so a closure-only formulation would lose
delivered disequalities.

**Soundness.** By criterion 3 this is exactly the delivery that
`let fresh = d; give fresh;` receives today, with S5's copy or literal row
establishing `fresh = d` and the existing substitution turning it into
`x = d`. On that edge the delivered value is d's value by [GIVE-1]; the
equality's support includes d, so any later write, consume or scope exit of d
kills it by [ENT-5]; the receiver is fresh, and the join keeps a relation only
when every non-contradictory edge holds it, so `x = pos` on one edge and
`x = length` on another join to their common weakest bounds (here
`x <= length`) and nothing else. The existing
`ent5-neg-value-if-unbounded-delivery` case stays rejected: its unguarded
`give value;` edge delivers `picked = value`, which proves no `< 128`.

**Cost.** Specification: two sentences in [ENT-5]'s bounded relation delivery
and the carrier list in [GIVE-1]. Conformance: three cases (an outer-term
carrier, a literal carrier, and a negative case writing the outer carrier
after the initializer). Compiler: about 40 to 80 lines in the three
`sources.rs` functions; the unit test
`nonbare_carriers_and_branch_local_support_create_no_delivery_roots`
(`compiler/src/semantic/tests/entailment.rs`) asserts that its `scoped`
function delivers nothing and would then see `picked = value` delivered, so it
is updated with the change.

**Impact.** Low in the census: none of the URL module's clamps is this shape.
Its value is the reported rewrite from expression form to duplicated statement
form; the URL module has 17 value initializers and 7 literal integer gives,
normalization 7 and 3.

## Shape 4: `Option` results

**Probe.**

```wf
fn find_space(data: &[u8]) -> result: Option<u64> reads(data) contract {
  ensures when Some(value: found): found < data^.len;
} {
  let length = data^.len;
  let pos = 0_u64;
  loop (
    invariant bounded: pos <= length
  ) {
    if pos >= length {
      break;
    }
    let byte = data^[pos];
    if byte == 32_u8 {
      return Some<u64>(value: pos);
    }
    set pos = pos + 1_u64;
  }
  return None<u64>();
}
```

**Diagnostic.**

```text
p4-option.wf:5:16: error[FN-9]: InvalidPostconditionSelector
  source:   ensures when Some(value: found): found < data^.len;
  marker:                ^^^^^^^^^^^^^^^^^^
```

The same function with `Result<u64, unit>`, `when Ok(value: found):`,
`Ok<u64, unit>(value: pos)` and `Err<u64, unit>(error: unit)`, and a caller
indexing `data^[at]` in its `Ok` arm, is accepted. The diagnostic names no
admitted form, which leaves a writer to discover the `Result` detour alone.

**Verdict: language gap.** [FN-9]: "A routed clause is admitted only as exact
`when Ok(value: r):` or `when b is Ok(value: r):` for a result ordinal whose
mode and type are `own Result<T,E>`"; "non-Ok ... cannot supply a relation
datum in this version". [ENT-5]'s conditional transport is stated for "a local
own `Result<T,E>` with T one fragment integer". The compiler implements that:
`validate_postcondition_selector` and `postcondition_route_carrier`
(`compiler/src/semantic/check/ensures.rs`) admit only the `Ok` variant of a
`Result`, and `result_payload_type` (`compiler/src/semantic/entailment/flow/results.rs`)
creates conditional evidence only for a type with an `Ok` variant.

**Proposed change.** Admit `when Some(value: r):` and `when b is Some(value: r):`
for an ordinal of type `own Option<T>`, T a fragment integer, and extend
[ENT-5]'s conditional transport from `Result<T,E>` to `Result<T,E>` or
`Option<T>`: each type's success variant (`Ok`, `Some`) carries the payload
context, its failure variant (`Err`, `None`) gives a contradictory success
context and definitely-failure tag, and an own match's success arm selects the
context. [CALL-4]'s route grammar is already variant-generic and its ambiguity
rule already counts "that route's enum type". `propagate` stays
`Result`-only; an `Option` is forwarded by return or delivery.

**Soundness.** `Option<T>` and `Result<T, unit>` are the same two-variant enum
with one integer payload on the success variant, and every sentence of the
conditional transport reads only the success payload and the failure tag,
never E. The `Option` instance is therefore the accepted `Result<T, unit>`
instance with its variants renamed (criterion 3).

**Cost.** Specification: about eight sentences touched in [FN-9], [ENT-5],
[ENT-2] clause (i) and [CALL-4], each replacing "Result"/"Ok"/"Err" by the
success/failure variant pair. Conformance: four to five cases (a routed
`Some` with a caller match, a forwarded `Option`, a rejected `when None(...)`,
an unproved `Some` payload at a selected return, and a `None` arm that selects
nothing). Compiler: about 100 to 150 lines across the five route sites in
`ensures.rs`, the success-variant lookup and routed-publication filter in
`results.rs`, and the failure-tag construction. PR #169 rewrites the same
[FN-9] admission sentence for struct payloads, so the two amendments should
land in one specification version or in sequence.

**Impact.** The color, normalization and URL modules declare 14 functions
returning `Option<u32>` (11) or `Option<u64>` (3): URL 8, normalization 4,
color 2. Each is a candidate to publish its payload bound instead of making
its caller re-check. The census did not measure how many callers carry such a
re-check.

## Shape 5: a header invariant with a callee postcondition

**Probe.**

```wf
fn take(limit: u64) -> result: u64 pure contract {
  ensures result <= limit;
} {
  return limit;
}

fn fill(cap: u64) -> result: u64 pure contract {
  ensures result <= cap;
} {
  let used = 0_u64;
  loop (
    invariant fits: used <= cap
  ) {
    if used >= cap {
      break;
    }
    let room = cap - used;
    let k = take(limit: room);
    invariant next: used + k <= cap {
      use fits;
      use (k <= room);
    }
    set used = used + k;
  }
  invariant done: used <= cap;
  return used;
}
```

**Diagnostics.** Three in sequence, each after the previous one is repaired
the way its message or intuition suggests:

```text
p5-chain.wf:22:5: error[PRF-1]: UndischargedSourceProof
  name: next
  obligation: RedundantUseBlock
  mechanical_fix: remove the use block; AUTO already proves this invariant target from the same entering context in this specification version
```

```text
p5b-chain-auto.wf:25:3: error[INV-1]: UndischargedLocalInvariant
  source:   invariant done: used <= cap;
  name: done
  disposition: Unproved
```

```text
p5c-chain-after-loop-use.wf:26:9: error[INV-1]: InvisibleUse
  source:     use fits;
  spelling: fits
  role: InvariantFact
```

A certificate that genuinely needs its steps is accepted: two header
invariants and two callee postconditions combined as
`use a; use b; use (k <= lim1); use (m <= lim2);` (four premises, beyond
AUTO's pair family) check. A destructured two-result call's `ensures next <= length`
also survives a following `propagate` call that writes an unrelated reference
and re-proves the header at the backedge.

**Verdict: no defect; the failures are shape 1 plus two intended rules.**
The callee's `k <= room` is an L0 fact whose canonical image over the affine
images `k` and `cap - used` is exactly the target `used + k - cap <= 0`, so
[ENT-6]'s DIRECT succeeds and [PRF-1] rejects the block as redundant, as the
tree decision "A nonempty explicit certificate is rejected when automatic
derivation already proves its target" (`language/checks-and-proofs`) intends.
After the loop the header theorem is removed (shape 1), and [INV-1]'s "The
header invariant name itself does not escape the loop body" makes `use fits`
invisible there.

**Proposed change.** None separately; shape 1 removes the second and third
failures. Two diagnostics would have shortened the loop:
`InvalidPostconditionSelector` (shape 4) and `InvisibleUse` for an expired
header name carry no repair; both are recorded in `docs/todo.md`.

## Shape 6: lockstep arrays and struct fields

Not re-investigated. PR #169's investigation proposes integer fields of struct
results and routed `Ok` payloads as relation data, and records separately that
two scalars grown together under a branch lose `a == b` at the join because
[ENT-6] joins images per binding and [INV-1] conclusions are affine only. The
URL census meets both limits. `base.wf` returns a Bool flag beside two
clamped integers because no clause can be conditional on the flag. And
`find_byte_or_end` shows the join limit in isolation: with a local
`invariant ... : pos <= length;` before each of its two `break`s and only its
lower clamp kept,

```wf
  let result = pos;
  let too_near = result < start;
  if too_near {
    set result = start;
  }
  return result;
```

rejects `ensures result <= length;` with `error[FN-9]: UndischargedPostcondition`,
`relation: result - length <= 0`, although each branch satisfies it: the then
edge holds it in L0 through `start <= length`, the false edge only as the
affine theorem over `pos`'s image, so the L0 join drops it and [ENT-6] gives
`result` a fresh atom because its two images differ. No local invariant can
carry it either, since the two edges' conclusions are different canonical
inequalities. This is the same limit PR #169 records for lockstep counters;
`docs/todo.md` records this witness as its own item. Shape 4's `Option` route
and PR #169's struct payload route touch the same [FN-9] sentence.

## Measured impact: the URL module's clamps

The module has 18 upper clamp blocks (`let too_far = x > n; if too_far { set x = n; }`)
and 5 lower ones (`too_near`). Deleting each upper block alone:

| Site | Today | Cause | After emulating shape 1 |
|---|---|---|---|
| `parser.wf` 1034, 1099, 1109, 1150, 1178 | accepted without it | defensive; the callee's `ensures next <= length` already holds | n/a |
| `parser.wf` 573, 740, 882 | FN-8/FN-8/OP-4 | header invariant lost at `break` (shape 1) | accepted |
| `builder.wf` 110, `ipv4.wf` 90, `parser.wf` 614, 846 | FN-9/REF-4 | shape 1, plus a lower clamp whose join discards the affine bound | accepted with a writer-added `start <= pos` header relation and a `start <= next` local invariant; the four lower clamps go too |
| `parser.wf` 746 | FN-8 | shape 1, needs `host_start <= scan_pos` as a second header relation | not tried |
| `base.wf` 25, 30 | FN-9 | a Bool flag returned beside the integers (shape 6 route) | not addressed |
| `bytes.wf` 127 | FN-9 | the inner decoder publishes no `ensures`; writer-side | not addressed |
| `host.wf` 56, `parser.wf` 673 | INV-1 | an advance of 1 or 3 chosen per branch; the bound is path-correlated | not addressed |

Emulation inserts, before every `break` of the loop, a local `invariant` with
the header relation, which is exactly what the proposal makes implicit when
the variable is unchanged since the head (every measured `break` here). Of
the 23 blocks, 5 are removable today, the shape 1 change removes 11 more
(7 upper, 4 lower), and 7 remain for other reasons.

## Recommendation and order

1. **Shape 1, header conclusions leave the loop.** Largest measured impact
   (11 of the URL module's 23 clamp blocks, 5 others being removable
   already), smallest cost (two specification sentences, about 50 compiler
   lines of which 30 are deletions), and soundness reduced to the existing
   local-invariant rule.
2. **Shape 3, the carrier equality and literal carriers.** Small and reduced
   to an accepted spelling; low measured clamp impact but removes the
   expression-to-statement rewrite.
3. **Shape 4, the `Some` route.** Medium cost, reduced to the accepted
   `Result<T, unit>` spelling; coordinate with PR #169's [FN-9] rewrite.
4. **Shape 2's short-circuit guard origin and the Bool helper route.** Defer:
   no measured consumer, and the integer-read layout is accepted today.
5. **Shape 5.** No separate change.

No shape is a compiler defect: every observed rejection follows the rule the
checker cites, so nothing is implemented on this branch.

## Rejected alternatives

- Re-prove the header batch at each `break` over the current images, as an
  obligation: rejected because a `break` taken after the variables changed
  (`op2-neg-break-removes-exhaustion-bound` overwrites its accumulator first)
  would turn today's accepted programs into rejections.
- Re-prove it at each `break` and publish only the successes: rejected for now
  because a failure-silent judgment is new to [INV-1], and every measured
  `break` leaves its variables unchanged, where retaining the head theorem
  already suffices; reopen when a consumer writes before its `break`.
- Copy header conclusions into L0 as the fix for shape 1: rejected because
  retaining the affine theorem already removes every measured shape-1 clamp
  (four of them with a writer-added floor relation), while an L0 copy adds a
  second source for the same theorem and changes what L0 closure sees. Projecting affine conclusions into L0 at a join
  remains only an unvalidated candidate for the separate join limit above.
- Deliver `x = d` by closure only, without the substitution: rejected because
  [ENT-4] has no rule carrying a disequality through an equality, so delivered
  disequalities would be lost.
- General computed `give` expressions: already an open question in
  `docs/todo.md` ("Remaining value-evidence boundaries"); the carrier equality
  needs none of its evaluated-expression images.
- A route for every enum with one integer-payload variant: rejected as larger
  than the evidence; `Option` is the one reported consumer and has a fixed
  success/failure pair like `Result`.

## Pending amendments and follow-up

- `design/amendments/loop-fact-retention-header-exit.md`: shape 1, on
  `language/checks-and-proofs/obligation-discharge/loop-fact-retention`.
- `design/amendments/automatic-facts-carrier-and-option.md`: shape 3's
  carrier equality and shape 4's replacement of the conditional-Result
  decision, on `language/checks-and-proofs/automatic-facts`.
- `design/amendments/requires-entry-contract-some-route.md`: shape 4's
  replacement of the `Ok(value: name)` decision, on
  `language/checks-and-proofs/requires-entry-contract`.
- `docs/todo.md` records the three language changes with their validation
  criteria and the two diagnostics without a repair.
