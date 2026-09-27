# Rows whose entries on one parameter overlap

## Question and scope

A row such as `reads(p), writes(p.x)` states exactly what a body does that
reads all of `p` and writes one field of it. Before this change [EFF-2]
admitted it at the declaration and [EFF-5] refused it at every call: EFF-5
compared every pair of a call's substituted entries, including two entries
one argument supplies, and `p` overlaps `p.x` [OWN-7]. The only callable row
for such a body was the coarser `writes(p)`, which kills every caller fact
about `p` and invalidates every reference into it. [EFF-1] refused only the
same-path pair `reads(p), writes(p)`.

The owner ruled on 2026-09-25 for direction B of the question raised in
PR #123: "our goal is performance under safety; B gives better performance".
This record states the rule, argues its soundness, and records adversarial
probes against the implementation. The baseline is PR #123 at `90259428`.

## The rule

1. [EFF-5] compares every two substituted entries that different arguments
   supply, and every two entries one reference argument supplies whose
   declared paths do not overlap at every position. Two declared paths of one
   parameter overlap at every position when one is the other followed by
   zero or more steps, or when the first pair of steps at which they differ
   is one that [OWN-7] and [WIN-2] fix as overlapping whatever values the
   positions take: two payload steps naming different variants, an index
   position and `.filled`, `.next` and `.free`, or `.last` and `.filled`.
   Steps compare as written, so two index or range positions are the same
   step exactly when they name the same value parameters. Such a pair is not
   compared. Every other pair of one argument's entries
   (`reads(v[i]), writes(v[j])`, an index position against `.last`, a range
   position against any other step) is compared at the call exactly as
   before, and so is every pair from different arguments.
2. [EFF-1] extends its subsumption: `writes(p)` states every access at or
   below `p`, so an entry at or below the path of another `writes` entry of
   the same row (`reads(p.x)` or `writes(p.x)` beside `writes(p)`, and
   `reads(p)` beside `writes(p)` as before) is an EFF-1 rejection at that
   entry's `effect`, carrying the entry that covers it. "At or below" is
   EFF-2's covering relation: the same root and a step prefix, an index or
   range position matching exactly when it names the same value parameters.
   After the owner's ruling on this record's read-below-read question, a
   `reads` entry below another `reads` entry (`reads(p.x)` beside
   `reads(p)`) is refused the same way, since `reads(p)` states every read
   below `p`; the same path twice in one category stays EFF-1's repeated
   entry.
3. EFF-2's suggested row becomes the exhibited row without the entries
   another of its entries covers. It needs no merge: every pair left on one
   parameter either overlaps at every position, and is not compared, or
   depends on position values, which the call's own proof decides.

## Soundness

EFF-5's per-call comparison protects four consumers. Each is examined for a
pair of entries one argument supplies.

**The callee.** The body is checked once, against its own row, for every
caller [EFF-2]. Both entries are paths rooted at the same parameter binding,
and the callee's own judgments already treat them as overlapping: an [ENT-5]
kill fires on a write to `deref(p).x` for every fact whose support overlaps
`p.x` under OWN-7, which includes every fact over `deref(p)`, and a local
reference to `deref(p)` survives a write below it as a content write while a
reference below a written proper prefix dies [REF-2]. Nothing in the callee
assumes two entries of its own row are disjoint, and no fact enters the
callee from the caller's EFF-5 check: a body starts from its own requirements
and the implicit facts of its parameters and their types [FN-8]. A callee
that writes `p.x` and then reads `p` observes its own write through the same
pointer.

**The caller.** After the call, the caller kills every fact whose support
overlaps any substituted `writes` entry [CALL-5, ENT-5] and applies every
substituted write's invalidation to every live reference [EFF-5 clause 3,
REF-2]. Both walk the complete substituted row; neither reads which pairs
EFF-5 compared. A read entry kills nothing [CALL-1], and a row with
`reads(p), writes(p.x)` kills exactly what `writes(p.x)` kills, which is
fewer facts than `writes(p)`: this is the performance B buys.

**The backend.** The emitter gives each reference parameter `noalias`,
`nonnull`, no-capture and `dereferenceable` (compiler/backend-facts), with no
per-entry alias scopes. LLVM's `noalias` on a parameter requires that memory
modified during the call through pointers based on that parameter is not
accessed through pointers not based on it. Entries one argument supplies are
all reached through that one parameter pointer, so their overlap is outside
the contract. Entries of different arguments are still compared, so a place
written through one parameter is still disjoint from every place another
argument reaches, and `swap` [OP-11] keeps its own exception. [OWN-9] states
this consequence and is reworded to say why it holds.

**Overlapping execution.** [PAR-1] compares the complete write and read
footprints of two statements, and [PAR-2] reads a call's complete projected
row as accesses on its actual places; neither compares two entries of one
call. Both keep the same footprints.

The pairs B still compares are those whose overlap is decided by position
values. Comparing them keeps the call-site obligations EFF-5 already has
(`reads(v[i]), writes(v[j])` needs `i != j` at the call). The same argument
shows that exempting them would also be sound, since the callee, the caller's
kills and the backend never rely on that distinctness; they stay compared
because the change is needed only where the comparison refuses every call,
and such a pair is callable wherever its positions are proved distinct. The
EFF-1 extension removes the rows B would otherwise make callable while they
state one access twice, keeping one spelling per row [FORM-1].

## Dependents

- [OWN-7] states that a call's substituted paths "are compared against each
  other"; it now defers to EFF-5 for which pairs are compared.
- [OWN-9] is non-normative; its reason is rewritten.
- `reference_parameter_facts` in `compiler/src/backend/emitter.rs` explains
  `noalias` by the pairwise check; its comment is rewritten. The emitted
  attributes do not change.
- design/language/effects/call-site-check states that a call rejects "any
  pair with a write"; an amendment replaces that decision. design/language/
  effects gains the EFF-1 subsumption as an amendment, and
  compiler/rejection-payloads' suggestion decision is replaced by an
  amendment, because its merge existed only to route around the old EFF-5
  refusal and can suggest a write wider than the body's.
- compiler/backend-facts relies on "ordinary EFF-5 call-site disjointness
  where the target contract needs it"; per-parameter `noalias` needs only the
  cross-argument pairs, which EFF-5 still compares, so the decision holds.
- design/language/surface-form/borrow-lexicon and
  design/language/system-interface/handle-factory derive exclusivity between
  a call's arguments, and between a factory and competing access, from the
  pairwise call check; both concern pairs from different arguments, which
  are still compared, so both stand.
- The [range-reference facts investigation](../range-reference-facts/DESIGN.md#the-proved-fact)
  summarizes EFF-5 as rejecting any overlapping pair; it is dated evidence,
  and its `noalias` conclusion depends only on cross-argument disjointness.

## Probes and criterion

The criterion, fixed before the probes ran: B holds when every probe below
receives the stated verdict from the implementation, every running probe
returns its stated exit status under the ordinary optimized build, and the
emitted parameter attributes of a one-argument row are the per-parameter
attributes the unchanged emitter gives every reference parameter. Any probe
that is accepted when it must be refused, or that runs to a wrong result,
refutes the implementation or the rule.

The probes are in [`probes/`](probes/). Each is compiled with
`whitefootc <probe>.wf -o <out>` from this branch, and running probes are
executed.

| Probe | Attempt | Required verdict |
|---|---|---|
| `covering-read-runs` | `reads(stats), writes(stats.count)` read whole, write a field, read back | accepted; exit 0 |
| `write-then-read` | the callee writes `p.x` and then reads `p` and returns the field | accepted; exit 0 |
| `two-parameters-one-place` | the same place passed to the row's parameter and to a second reading parameter that reaches the written field | EFF-5 |
| `two-parameters-sibling` | the second parameter reaches a sibling field only | accepted; exit 0 |
| `swap-ancestor` | `swap` of a place and its field | OP-11 |
| `window-append` | `reads(window), writes(window.next), writes(window.len)` over `place_back` | accepted; exit 0 |
| `window-index-last` | `reads(window[i]), writes(window.last)` from one argument | EFF-5 (position-dependent) |
| `window-index-filled` | `reads(window[i]), writes(window.filled)` from one argument | accepted (fixed overlap) |
| `payload-variants` | `writes(slot.Full.value), writes(slot.Spare.amount)` beside `reads(slot)` | accepted (fixed overlap); exit 0 |
| `index-positions` | `reads(values[i]), writes(values[j])` with `i == j` at the call | EFF-5 |
| `covering-read-indexed` | `reads(values), writes(values[j])` | accepted; exit 0 |
| `inline-range` | `reads(part), writes(part[k])` with `part: &values[0..4]` formed at the call | accepted; exit 0 |
| `range-positions` | `reads(part[a..b]), writes(part[k])` from one argument | EFF-5 (position-dependent) |
| `joined-reference` | one argument that is a joined reference naming `values[i]` or `values[j]`, row `reads(cell), writes(cell.count)` | accepted |
| `formal-row` | a call through a function-kind formal whose row is `reads(p), writes(p.x)` | accepted; exit 0 |
| `generic-row` | a generic `reads(pair), writes(pair.first)` at two instances | accepted; exit 0 |
| `recursion` | a self call passing its own parameter under `reads(p), writes(p.x)` | accepted; exit 0 |
| `caller-kill` | a fact over the written field is used after the call | OP-4 (the fact died) |
| `caller-keep` | a fact over a sibling field is used after the call | accepted; exit 0 |
| `reference-keep` | a live reference to a sibling field used after the call | accepted; exit 0 |
| `reference-kill` | a live reference below a written Box field used after the call | REF-2 |
| `redundant-read` | `reads(p.x), writes(p)` | EFF-1 at `reads(p.x)` |
| `redundant-write` | `writes(p), writes(p.x)` | EFF-1 at `writes(p.x)` |
| `par1-disjoint` | two adjacent one-argument-row calls on different roots | overlap permitted |
| `par1-overlap` | the call beside a read of the written field | overlap denied |
| `par2-elements` | a counted loop calling `reads(cell), writes(cell.count)` on `&cells[i]` | accepted; exit 0 |
| `par2-ranges` | a counted loop calling `reads(part), writes(part[k])` on disjoint ranges | accepted; exit 0, also under `--par` |

## Results

The criterion held. Every probe received its required verdict from this
branch's compiler and every running probe exited 0; the PR #123 compiler
refused every probe that uses a one-argument row at its call.

| Probe | PR #123 | This branch |
|---|---|---|
| `covering-read-runs`, `write-then-read`, `two-parameters-sibling`, `window-append`, `window-index-filled`, `payload-variants`, `covering-read-indexed`, `inline-range`, `joined-reference`, `formal-row`, `generic-row`, `recursion`, `caller-keep`, `reference-keep`, `par1-disjoint`, `par1-overlap`, `par2-elements`, `par2-ranges` | EFF-5 | accepted; exit 0 |
| `two-parameters-one-place`, `window-index-last`, `index-positions`, `range-positions` | EFF-5 | EFF-5 |
| `swap-ancestor` | OP-11 | OP-11 |
| `caller-kill` | EFF-5 | OP-4 `after < table.len`: the call killed the fact over `stats.count` |
| `reference-kill` | EFF-5 | REF-2 at `held`: "a call wrote a proper prefix of the reference's path" |
| `redundant-read` | accepted (no call) | EFF-1 `SubsumedEffectEntry { entry: "reads(stats.count)", covering: "writes(stats)" }` |
| `redundant-write` | accepted (no call) | EFF-1 `SubsumedEffectEntry { entry: "writes(stats.count)", covering: "writes(stats)" }` |

Overlapping execution, from `whitefootc --par-ledger`:

- `par1-disjoint`: `pair(record, record)` is permitted; the following read of
  both counts is denied under condition 1.
- `par1-overlap`: `pair(record, observe)` is denied under condition 1, the
  write of `&first` overlapping the read of `&first.count`.
- `par2-elements`: the loop is denied under condition 2, because `&cells[i]`
  is not a proved range; the program runs sequentially and exits 0.
- `par2-ranges`: the loop is permitted, and the `--par` build run with
  `WF_WORKERS=4` exits 0.

Backend: `covering-read-runs` emits
`define i8 @wf_record(ptr noalias nonnull nocapture dereferenceable(16) %v0)`,
and a one-field `Stats` callee declared `reads(stats), writes(stats.count)`
on this branch emits the same attributes as the same body declared
`writes(stats)` on PR #123:
`ptr noalias nonnull nocapture dereferenceable(8) %v0`.

Existing rows: no declaration in the maintained sources, the prelude records
or the container libraries under `research/experiments/container-representation`
writes an entry at or below another `writes` entry of its own row, so the
EFF-1 extension refuses none of them. The rows it refuses appear only in
the new conformance negatives. The conformance corpus reaches every declared
verdict (Pass=1212), including the six uncalled rows of
`ref2-pos-bystander-preservation`, which the case now calls.

Two items outside the ruling surfaced:

- EFF-2's printed fix, "add every missing category and path and remove every
  extra one", never removes a declared entry a missing entry covers: a body
  that reads `stats.count` and calls a helper declared `writes(stats)`,
  declared `reads(stats.count)`, gets `missing: ["writes(stats)"]` and
  `extra: []`, and adding the entry gives a row EFF-1 now refuses; after the
  read-below-read ruling, a missing `reads(stats)` does the same.
  `expected_row` itself is admitted in both. This joins the
  printed-restructuring audit in `docs/todo.md`.
- A read below another read of the same row, such as
  `reads(stats), reads(stats.count)`, was still admitted beside the shorter
  `reads(stats)` for the same body, so one body had two admitted rows. Read
  pairs are never compared at a call and kill nothing, so the redundancy
  cost no caller anything. The owner ruled to refuse it as well, and EFF-1
  now does (rule 2 above; conformance case `eff1-neg-read-below-read-path`).
  One maintained row had this shape, a unit test declaring
  `reads(packet), reads(packet.Data.value)`, and now declares
  `reads(packet)`.

## Follow-up: an index or a window part beside a range

Rule 1 left three kinds of one-argument pair compared at every call with
nothing able to separate them: an index position beside a range position,
and a range position beside `.next`, `.free`, `.last` or `.filled`, for which
OWN-7 and WIN-2 had no family, and an index position beside `.last`, which
WIN-2 separates but the checker posed to no call. Every call of such a row
was refused, `reads(values[start..end]), writes(values[slot])` with
`start: 0_u64, end: 2_u64, slot: 3_u64` included, and EFF-2 could suggest
such a row, so rule 3's ground failed for these pairs. Two directions kept
it: (a) class the pairs with those that overlap at every position, so no
call compares them; (b) give OWN-7 a family that separates an index from a
range and WIN-2 rows for a range beside a part, so each call proves the pair
apart as it proves two indices apart. The owner chose (b) on 2026-09-26. The
completion review then refuted every separation from `.last`, and the owner
agreed the same day to fix those pairs as overlapping; see
[The last filled slot](#the-last-filled-slot).

The rule, in kernel-spec v0.74:

1. [OWN-7] An index step and a range step under containing paths that are
   identical step for step, or differ only in index steps, are
   disjoint when the ProofContext proves `index < range.start`,
   `range.end <= index` or `range.end <= range.start`; a proved separation
   separates everything below both.
2. [WIN-2] A range `r[lo..hi]` does not overlap `r.next` or `r.free` when
   `hi <= r.len` or `hi <= lo` is proved where the places are compared, and
   overlaps both otherwise. A range formed in that state ends at or below
   `r.len` there by its REF-4 obligation.
3. [WIN-2] Every index and every range overlaps `r.last` and `r.filled`,
   whatever its value. v0.73 separated `r[i]` from `r.last` where
   `i != r.len - 1` was proved; v0.74 drops that separation.
4. [EFF-5] An index or range position beside `.last` or `.filled` joins the
   pairs that overlap at every position, so one argument's such pair is not
   compared at a call, while two arguments' such pair is compared and
   refused. Every other pair with a range position stays compared and now
   has a family.

Why (b): some values separate an index or a range from a range, `.next` or
`.free`, so those pairs do not overlap at every position, and (a) would call
a position-dependent pair fixed. Both are sound by the argument under
[Soundness](#soundness), which holds for any exempted pair. They differ in
what a caller keeps. Under (a) OWN-7 still has no family, so a write beside
a range kills every caller fact below it and a call whose read range holds
the written slot is admitted. Under (b) the call proves the pair apart, and
the same family keeps a caller fact on an element outside a written range,
or below a range an append does not reach [CALL-5, ENT-5].

### The last filled slot

The first draft of this follow-up also separated a range from `r.last`
where `hi < r.len` or `hi <= lo` was proved, and posed v0.73's index row,
`i != r.len - 1`, at every call. The completion review refuted both. EFF-2
covers any number of `take_back` calls with one `writes(r.last)` entry, and
each call lowers `r.len` by one, so after `k` of them the callee has emptied
every slot from the entry state's `r.len - k` up; `i != r.len - 1` at entry
bounds only the first. The draft accepted three programs that show it,
stored under [`probes/`](probes/) with a `review-` prefix:

- `p07`: a callee declared `reads(source[i]), writes(target.last)` takes
  back twice and places back once. Called with one window for both
  parameters and `i` one below the last slot, it changed `source[i]` during
  the call, and the program exited 42.
- `p20`: a reference to `window[1_u64].value` survived a one-argument call
  that took back two elements, and the program read the released box.
- `p21`: the same with a range reference.

A row states no count of take-backs, so no entry-state bound confines the
slots its `r.last` reaches; a count in the row would be a larger language
change that no program needs. The owner agreed on 2026-09-26 to overlap
every index and every range with `r.last`, as with `r.filled` (rule 3). One
argument's `reads(r[i]), writes(r.last)` then overlaps at every position
and is not compared, so the probe `window-index-last`, which v0.73 refused,
is accepted; a pair two arguments supply is refused, and the call ends every
reference into the window (below). v0.73's separation of an index from
`r.last` was unsound in the same way for a user row, but no check applied
it: calls compared the pair and refused it, kill events never answered it,
and every reference into the window died at `take_back`. Only the prelude's
`take_back`, which takes back exactly one element, reaches no slot but its
entry state's `r.len - 1`, and nothing used that either.

### Defects the reviews found beside the rule

- **References after a user call that takes elements back.** The checker
  ended a window reference's bound [OP-10] only at the prelude's window
  operations, so a reference into a window outlived a user function whose
  row writes `r.last` and that took its slot back: `p01` read a released
  slot through a range reference, `p02` released a box twice through it,
  and `p03` wrote through the freed box (stored as `review-p01` to
  `review-p03`). The v0.73 checker accepts all three. A call now also ends
  that bound for every window whose `last` or `filled` its substituted row
  writes, as `take_back` and `remove_at` do. It reads no `ensures` for
  this, so a callee that restores the length still ends the bound;
  `docs/todo.md` records that loss.
- **Two ranges a row supplies.** A pair of range positions was proved apart
  only from the endpoint images of a range formed at the call, so two
  ranges a row takes from other arguments, `reads(values[lo..hi]),
  writes(values[a..b])`, were refused even where the entry state proves
  `hi <= a`. The flow now falls back to each endpoint's own image, so such
  a call is separated where the ordering is proved and refused otherwise.
- **A write through a range frame.** The second review found that REF-2's
  proper-prefix question counted steps, so `outer[0_u64..2_u64][1_u64]`,
  which selects `outer[1_u64]` through a range frame, was never a prefix of
  a reference to `outer[1_u64][0_u64]`: a write replacing that element, or
  a call taking elements back through it, left the reference live. The
  v0.73 checker accepts these too, and the review's probes read freed
  memory, corrupted the heap and segfaulted. The question now compares
  storage depth: a range step followed by another step descends no level of
  its own, and a run a path ends at lies half a level below its base. One
  program v0.73 accepted is now refused: a callee declared `writes(part)`,
  called with `part: &v[0_u64..2_u64]`, writes a place above `v[1_u64]`,
  so a reference `&v[1_u64]` dies at the call, as `&deref(part)[1_u64]`
  already did in v0.73 (`ref2-neg-element-reference-after-run-write`).

### Soundness of the families

- A slot `k` of `r[lo..hi]` satisfies `lo <= k < hi`, and each of
  `index < lo`, `hi <= index` and `hi <= lo` excludes `k = index`. Steps
  below the index and below the range are relative to different frames, so
  only a separation proved at this pair separates their descendants.
- Containing paths. Two containing paths that differ only in index steps
  select one storage or two: their other steps are identical, so each pair
  of index steps selects elements of one frame, and where their offsets are
  equal the paths name one storage, where the orderings apply, and where
  they differ, different storages. Containing
  paths that differ in a range step are frames every later step is relative
  to, so they are not compared. The checker already separated the first
  kind, and v0.73 accepted two ranges formed at a call below unproved row
  indices; the owner ruled on 2026-09-26 that OWN-7 state that relation.
  The cases `own7-pos-*-below-unproved-row-indices-separate` pin the three
  forms, and `eff5-neg-ranges-below-different-range-frames-overlap` the
  refused frames.
- Every slot of a range is below `hi`, so `hi <= r.len` puts it below the
  append slot and every free slot, and an empty range holds no slot. Every
  range overlapping `r.last` and `r.filled` is a fixed answer, as it is for
  every slot.
- Where the bound is read. A call proves its positions in its entry state.
  A kill event proves a range bound, as it proves liveness, in its own entry
  state and never carries it along an edge, since `r.len` changes. The flow's
  ledger records only the separations of two indices, two ranges, and an
  index and a range, whose captured values no write changes, and drops them
  at a join a predecessor lacks.
- Loop headers. A header's kills stand for every iteration's events and read
  bounds from the preheader state. A range proved there to end at or below
  `r.len` stays so at each such event, because `r.len` falls only at a write
  of `r.last`, `r.filled` or the whole window, and each of those kills every
  fact below the range, since every range overlaps `r.last` and `r.filled`.
- PAR-1 poses no index-and-range query, so such a pair overlaps unless its
  literals decide it. It reads a range either statement forms as within the
  length, as it reads a subscript either forms as live, under the existing
  shared-length guard.

### Criterion and results

Criterion: (b) holds when every new conformance case below reaches its
declared verdict through the ordinary compiler path, every running case
exits 0, the v0.73 compiler refuses every positive case, and no existing
case changes its verdict. It was fixed before the cases ran. The review's
fixes added the `.last`, two-row-range, `ref2` and `op10` rows under the
same test, except `ref2-pos-reference-survives-user-push` and
`op10-pos-slot-reference-survives-insert-at`, controls fixed in advance to
stay accepted on both compilers.

| Case | v0.73 compiler | v0.74 |
|---|---|---|
| `own7-pos-index-outside-range-separate` | EFF-5 | exit 0 |
| `eff5-neg-index-inside-written-range` | EFF-5 | EFF-5 |
| `eff5-neg-index-beside-range-unproved` | EFF-5 | EFF-5 |
| `win2-pos-range-within-length-beside-append` | EFF-5 | exit 0 |
| `eff5-neg-range-reaching-append-slot` | EFF-5 | EFF-5 |
| `eff5-pos-index-beside-last-not-compared` | EFF-5 | exit 0 |
| `eff5-pos-range-beside-last-not-compared` | EFF-5 | exit 0 |
| `eff5-neg-two-arguments-index-beside-last` | EFF-5 | EFF-5 |
| `eff5-neg-two-arguments-range-beside-last` | EFF-5 | EFF-5 |
| `eff5-pos-range-beside-filled-not-compared` | EFF-5 | exit 0 |
| `eff5-pos-two-arguments-range-beside-append` | EFF-5 | exit 0 |
| `call3-pos-range-write-beside-element-keeps-its-measure` | OP-4 | exit 0 |
| `ent5-pos-range-element-measure-survives-append` | OP-4 | exit 0 |
| `own7-pos-two-row-ranges-separate` | EFF-5 | exit 0 |
| `eff5-neg-two-row-ranges-overlap` | EFF-5 | EFF-5 |
| `ref2-neg-index-reference-after-user-pop` | exit 0 | REF-2 |
| `ref2-neg-range-reference-after-user-pop` | exit 0 | REF-2 |
| `ref2-pos-reference-survives-user-push` | exit 0 | exit 0 |
| `ref2-neg-reference-after-user-remove` | exit 0 | REF-2 |
| `op10-pos-slot-reference-survives-insert-at` | exit 0 | exit 0 |
| `ref2-neg-element-reference-after-range-frame-pop` | exit 0 | REF-2 |
| `ref2-neg-element-reference-after-range-row-pop` | exit 0 | REF-2 |
| `ref2-neg-element-reference-after-range-frame-write` | exit 0 | REF-2 |
| `ref2-neg-element-reference-after-run-write` | exit 0 | REF-2 |

The criterion held. The `ref2-neg` rows are programs the v0.73 checker
wrongly accepted; `ref2-pos-reference-survives-user-push` shows a user call
that only appends keeps its bound, and
`op10-pos-slot-reference-survives-insert-at` that a slot reference survives
the prelude's `insert_at`, whose `ensures` carries its bound. The pinned repair pair
`declared-row-omits-a-range-read-beside-a-written-slot.wf` shows EFF-2's
suggested `reads(values[start..end].len), writes(values[slot])` accepted at
a call that passes the slot outside the range. Of the probes above,
`range-positions` and `window-index-last` are accepted by v0.74: the first
passes positions its entry state proves apart, and the second's pair beside
`.last` is no longer compared; their table rows record the v0.73 rule, and
every other probe keeps its verdict. The stored probes predate the move of
`ExitStatus` and `exit_status` into `std::process`, so a current compiler
refuses each with TYPE-5; they were rerun here with the two `alias` lines
the conformance cases declare, followed by a blank line.
