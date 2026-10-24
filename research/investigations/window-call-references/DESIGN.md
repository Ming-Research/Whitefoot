# Window references across calls

This record separates the current specification from a proposed amendment.
The proposal is not approved or implemented. The active
[specification](../../../spec/kernel-spec.md) remains authoritative.

## Question and evidence

Can a reference to a surviving window slot remain usable after a direct
`take_back` or `remove_at` call when the caller proves its captured bound in
the call's exit state?

The owner reported two CI failures for revision `28edba826`:

- `eff5-neg-bystander-invalidated-by-call` was accepted instead of rejected
  under REF-2. Its shared Rust test failed for the same reason. The program
  forms a reference to slot zero of a two-element window and then removes
  slot one.
- `ref2-pos-range-reference-after-user-length-ensures` was rejected under
  REF-2 at `prefix^.len`. The helper promises unchanged length, but the new
  preservation query failed to recover the symbolic endpoint's captured
  image.

These are supplied CI observations, not locally reproduced results. No
compiler, build or test was run locally. The repair requires the existing
bystander rejection and the unchanged-length range acceptance to pass in CI,
along with the existing gate. Accepting the former would falsify the claimed
agreement with the current named-removal rule; rejecting the latter would
falsify the claimed range-bound completeness repair.

## Exact current text and its scope

REF-2 states:

> "p is valid" is an ownership fact established where `p` is formed. It is invalidated when a proper prefix of its selected path is written, or when that path or a prefix is moved out of or released, by a statement, call [EFF-5] or compiler-derived release [STOR-3]; when its ultimate root leaves scope; or by the window-selection invalidations of [OP-10].

> Writing the storage at `p`'s path or below it is a content write and does not invalidate `p`.

EFF-5 clause 3 states:

> Every live reference, including an actual argument, receives the invalidations of every substituted write under [REF-2]. A content write at or below its captured target preserves it; argument membership does not protect it from another actual's destructive ancestor write.

WIN-2 states:

> Overlap is fixed data for [OWN-7], read in the state where the two places are compared: an `r[i]` live there, `i < r.len` being proved there, never overlaps `r.next` or `r.free`; any other `r[i]` overlaps both; a range `r[lo..hi]` does not overlap `r.next` or `r.free` when `hi <= r.len` or `hi <= lo` is proved there, and overlaps both otherwise; every `r[i]` and every range overlap `r.last` and `r.filled`, because a row's `r.last` stands for every slot its callee's successive `take_back` calls reach, one `writes(r.last)` covering them all [EFF-2]; `r.filled` and `r.free` do not overlap; `r.next` is contained in `r.free`; `r.last` is contained in `r.filled`; the measure `r.len` is itself a write target and overlaps no slot.

OP-10 states, about `insert_at` and `remove_at`:

> Each of those two is a content write of `r.filled`, so a surviving slot reference names its slot, whose occupant may have changed, exactly as a stale index does [OP-13].

Its final bound paragraphs state:

> A reference into a window is formed under a bound and stays valid while that bound holds: `&r[i]` under `i < r.len`, and `&r[lo..hi]` under `hi <= r.len` [REF-4]; a page reference requires its captured initialized extent [REF-4] to remain filled.

> `place_back`'s `ensures` carries these bounds across the call, while `take_back`'s and `remove_at`'s do not, so such references die there.

A true proper-prefix write invalidates the reference regardless of an
`ensures` clause about length. Overlap with `.last` or `.filled` alone is not
that destructive ancestry: those effects can overwrite the selected slot's
contents. An element write can, however, be a proper-prefix write of a
reference into that element's interior. A whole-window write also remains a
proper-prefix write of its slot references.

The two final OP-10 sentences are in tension: the first describes a general
bound criterion, while the second explicitly invalidates references at two
named operations even when additional caller facts would prove the bound.
The repair retains the specific named-operation restriction. EFF-5 needs no
exception or weakened prefix rule. The bystander case is rejected by OP-10's
named-removal sentence, not by a proper-prefix write. Its source documentation,
manifest description and shared Rust test comment state that reason; its
executable program, rule list and reject verdict remain unchanged.

Append's destination and a user function with a partial window row have no
corresponding unconditional named-operation restriction. Their postconditions
can establish the bound under OP-10's general sentence, provided REF-2's
independent structural invalidations do not apply. A source call is judged
through its declared row and contract; the caller does not unfold its body.

## Current repair

The checker again invalidates window selections at direct prelude
`take_back`, `remove_at` and `clear` calls. It retains unconditional origin-shift and
proper-prefix invalidation. Other supported boundary calls retain the
ordinary post-call proof query, including admitted direct-result publication.

The range query now reads the paired endpoint image recorded by range
formation, rather than looking only for an individual index image. It still
proves the captured upper endpoint is no greater than the current length;
an empty range receives no exemption.

The new prefix-after-`take_back` acceptance case is withdrawn from normative
conformance because it claimed behavior that the explicit current rule does
not authorize. Its unchanged source is retained as
[a proposed witness](prefix-after-take-back.wf), outside every gate. Its
expected future acceptance is not current conformance evidence. No existing
reject verdict is changed.

## Proposed rule change, pending owner approval

Replace OP-10's categorical sentence about `take_back` and `remove_at` with
the following clarification of the preceding bound rule:

> At a call's normal continuation, the window-selection bound is judged from the captured index or endpoints in the caller's post-call ProofContext, after the call's ordinary effects, kills and admitted postcondition publication [ENT-3.S12, ENT-3.S13, ENT-5]. A previously formed reference retains its window-selection validity when that bound is proved there. Failure to prove it invalidates the reference [REF-2]. The independent structural invalidations of [REF-2] and the logical-origin invalidations of this rule still apply.

This is a proposal, not an edit to the active specification. It would remove
the named-removal restriction for cases with a proved surviving bound. It
would not exempt proper-prefix writes, owner moves/releases, relocation or
Ring front operations. The existing bystander verdict could change only as
part of the owner's approved specification amendment and corresponding
conformance update.

## Minimal before and after programs

These complete source sketches differ only in when the reference is formed.
They have not been compiled. The first forms it after removal and is intended
to be accepted under the current specification. The second is rejected by the
current named-removal rule and would be accepted under the proposal. Both
would observe `22`, showing that the selected slot, not its old occupant, is
the reference's identity.

Before, form the reference after the call:

```wf
fn main() -> status: std::process::ExitStatus pure {
  let w = slots_new::<u64, 4>();
  place_back(window: &w, value: 11_u64);
  place_back(window: &w, value: 22_u64);
  let removed = remove_at(window: &w, index: 0_u64);
  let first = &w[0_u64];
  let seen = first^;
  if seen == 22_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
```

After, retain the reference across the call:

```wf
fn main() -> status: std::process::ExitStatus pure {
  let w = slots_new::<u64, 4>();
  place_back(window: &w, value: 11_u64);
  place_back(window: &w, value: 22_u64);
  let first = &w[0_u64];
  let removed = remove_at(window: &w, index: 0_u64);
  let seen = first^;
  if seen == 22_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
```

## Soundness argument and limits

For a slot reference, the captured index is immutable. For a range, the
captured endpoints and extent are immutable. With origin and storage retained,
the post-call bound places each selected slot inside the initialized window.
Removing or shifting an occupant does not change that slot's location.
OP-10 already assigns this meaning to surviving slot references; it makes no
promise that their values remain unchanged. Ordinary effect kills must still
discard facts about overwritten values.

The call's effect row is unchanged. EFF-5 still excludes an overlapping
access supplied through another argument, and PAR-1 still excludes an
overlapping concurrent statement. REF-3 prevents a reference from escaping
through stored state. A caller's retained bystander is therefore not a
permission to observe a temporarily empty slot inside the callee; the
proposed preservation judgment concerns its normal continuation.

This reasoning is insufficient for a reference into an element's interior:
replacing the element may replace an owner, enum variant or nested allocation
that the reference selected. REF-2 must invalidate that reference through its
proper-prefix rule even when outer length is unchanged. Likewise, a retained
length cannot preserve a pointer across relocation or a changed Ring origin.
Those independent invalidations stay in force.

The proof is evaluated at the original call boundary. A later guard, refill
or unrelated call cannot repair an earlier failure; REF-2 requires forming a
new reference. Call-entry values must be the recorded S13 datums, not current
values reread through the same spelling. A user helper's body cannot supply
facts absent from its callable contract.

If approved, CI must distinguish a retained prefix from a removed last slot,
accept a removal that changes the surviving slot's occupant, reject an
interior reference after element replacement, and reject restoration of an
already invalid reference. Range checks must include a captured endpoint
whose source binding is later changed and an empty range beyond the new
length. No page-reference or unknown-descendant completeness claim is made
by this repair; their existing conservative behavior remains.

## Follow-up: page references

The repair above left a `Paged` page reference invalidated at every user call
that writes `window.last` or `window.filled`, whatever the callee promises. A
page captures its initialized extent `[k * B, min((k + 1) * B, p.len))` at
formation [REF-4], where B is the page length `paged_page_len<T>()`, a pure
record whose only published fact is its postcondition, so the checker has no
value for B and cannot state the extent's end as a linear fact. The extent
does end at or below the length word its formation read, so the page's bound
is that length: formation files an image of the length it read, and a call
that writes `last` or `filled` keeps the reference exactly when the image is
proved at or below the window's length in the call's exit state, the same
post-call query a slot or range reference owes. The condition is sufficient
and not necessary: a call that shortens the window below the length read at
formation still invalidates a page whose extent the shorter window would
cover, because proving that needs B. A direct `place_back` never needed this:
a page step is separated from `r.next` and `r.free` whatever the extent
[WIN-2], and `take_back`, `remove_at` and `clear` invalidate by name [OP-10].
`grow_paged` and a page index that is not one formation's own evaluation keep
the unconditional invalidation, as does an element reference below a page: a
helper's `last` write is a proper-prefix write of it, as of an interior
reference below a `Slots` element. The cases are the conformance cases whose
id starts `ref2-pos-` or `ref2-neg-` and contains `page-reference`.
