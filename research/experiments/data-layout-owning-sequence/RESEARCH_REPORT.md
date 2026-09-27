# E0.1 Research Report: Data Layout and Owning Sequences

Status: superseded non-normative evidence report, 2026-07-13. Its measurements,
audits, and rejected-prototype findings remain evidence, but it is no longer an
active decision surface and authorizes no work.

Current disposition, 2026-07-13: the evidence and rejected-prototype record below
remain valid, but this report is no longer the immediate decision surface. The
broader capability-floor audit in
`../../optimizer-language-research/implementation/general-purpose-data-structure-capability-RESEARCH.md`
suspends the paired ownership protocol before Lock A. D11 replaces the earlier
monolithic upstream gate with bounded G0-Core plus an exact lock for each family.
G0-Core and dense Family Lock A are necessary but not sufficient for a later
owner decision to lift this pause. Section 10's local questions are historical;
the dense lock must explicitly retain, revise, or supersede their relevant arms
and measurements rather than automatically restarting them.

## 0. Executive conclusion

Specification and implementation audits, not performance experiments, confirm the
first real expressiveness gap: Whitefoot can manually express SoA with multiple
`buffer<primitive>` values, but it cannot safely place an ordinary named record in a
`buffer<Record>` or guarantee that `index<Record>(rows, i).field` lowers correctly to
a row GEP, field GEP, and scalar load/store. Programs with an obviously row-oriented
producer and consumer are therefore forced into parallel arrays. This constrains
performance choices and scatters field additions and removals, length consistency,
capacity checks, and API boundaries across several buffers.

This does not mean that the current wfc should change from SoA to AoS. The existing
compiler has many consumers that scan only a kind, span, or child-link column. SoA's
unit stride, lack of record padding, and per-column alias scope may all be superior
for those consumers. Static accounting even shows that directly replacing only the
fixed Token/Ast storage with natural AoS under the same full-capacity policy would
increase requested memory for those two tapes by 11.1% and for the entire 30-column
frontend by 3.85%. The correct objective is therefore to let the language explicitly
express either layout while protecting the existing SoA from regressions, not to
assume one global winner.

The evidence fixes constraints but does not yet select a language design:

1. Fixed AoS record storage and scalar field access must be expressible without
   taxing unchanged primitive-buffer or SoA programs.
2. Storage eligibility, implicit copying, explicit repetition, and physical layout
   are separate decisions. No one property may silently grant the others.
3. The existing `buffer_new<T>(n, fill)` contract must remain T:Copy unless an owner
   decision deliberately changes that operation and accounts for the new spelling
   and evaluation rules.
4. Whole-row moves that leave holes, observable padding, bytewise comparison or
   hashing, and accidental ABI promises remain outside the demonstrated capability.
5. Do not use a feature flag. Baseline and candidate are always two isolated
   toolchains, each with one unconditional semantics.

The prior project record carded a *declarative* `copy struct` tier: an author would
mark one all-Copy-field record as Copy. That is not automatic structural Copy, under
which every structurally eligible record silently changes ownership class. The
earlier draft incorrectly grouped these together and rejected both. This revision
keeps declarative `copy struct`, a separately derived `Flat(T)` storage predicate,
dedicated initialization forms, and explicit Repeat/Clone as distinct alternatives.
It does not choose among them.

The isolated prototype proved that two properties can hold together: on
x86_64/AArch64 it emits target-stride 24/56-byte rows whose field hot paths contain
only a row GEP, field GEP, and scalar load/store; it also keeps all four existing
SoA/full-compiler raw-IR pins byte-identical. Hostile review nevertheless rejected
that prototype as a production candidate because it copies one affine fill value
into N slots. The same review also found pre-existing mainline conformance defects.
Commit `38d642e` closed the first index-atom liveness/readability hole. Commit
`7438e17` closes the broader current-language expression-context gaps in index,
match, try, and return handling; commit `50a1ddd` repairs recursive
projection and enforces GRAM-9 in both executable frontends. These repairs do not
select the record-storage design, and a green candidate test suite does not cure the
affine-fill violation.

A second gap also exists: the current `buffer<T>`, whose entire length is initialized,
cannot express `{ptr, len, capacity}` with only `[0, len)` initialized at zero overhead.
Using `buffer<Option<T>>` or filling the entire capacity introduces tag, branch,
initialization, and drop costs. This must be decided separately as E0.1b unless the
selected E0.1a initializer inherently requires per-slot construction. Implementing
initialized-prefix storage together with AoS would otherwise make it impossible to
attribute effects to layout, unused-capacity initialization, or growth policy.
Moreover, an opaque kernel `sequence<T>` would reverse STOR-1's recorded direction
that growable collections are libraries over `buffer<T>`; that is a real META-5
redecision, not an implementation detail.

This report does not request a production implementation. Its former request to
select one ownership/initialization prototype is superseded by D11. No G0-Core,
family-lock, candidate, timing, or production work starts from this document.
Even a later successful dense-family candidate would not imply that wfc should
change layouts.

## 1. Scope

This phase answers only two questions:

- **E0.1a:** Which, if any, ownership and initialization model should make
  fixed-length AoS record storage expressible without adding a runtime tax?
- **E0.1b:** Can an initialized prefix plus capacity be expressed without exposing
  uninitialized memory, and does that require reversing STOR-1's library-over-buffer
  direction?

E0.1a and E0.1b remain serial if the selected initializer does not require partial
initialization. A builder or initialized-prefix initializer deliberately couples
them and therefore requires one newly scoped protocol before either implementation.
Modules, methods, contracts, borrowed aggregates, loop facts, byte literals/bulk
append, and SIMD belong to later E0.2-E0.5 work and must not be added to this
candidate.

The superseded experimental boundary was the following. It grants no current
permission:

- Do not add E0.1 candidate semantics to the specification, checker, stage 0, wfc,
  or teaching material in the main worktree. Independently authorized repairs that
  enforce existing rules remain ordinary mainline conformance work.
- Keep an executable candidate only in a detached disposable worktree, with its
  semantics applied unconditionally. Archive the exact reviewed source diff in the
  repository before treating an experiment or review step as durable.
- Do not introduce a switch, dual grammar, or a path that co-locates baseline and
  candidate behavior.
- Static accounting, correctness tests, negative tests, IR/assembly shape checks,
  and unscored smoke runs were allowed for the completed isolated work.
- Formal timing would have required a separately frozen protocol after protocol
  review.
- Production implementation would have required explicit confirmation after
  review of this report; D11 now supersedes that route.

## 2. What the current baseline establishes

The frozen baseline is Git `58baa71fb4c36a4728dd42aea6b05ce4be7aa0b1`. A full
`make check` passed before any candidate implementation: checker modelcheck covered
10,000 cases with 0 soundness violations, codegen parity had 0 failures, and
conformance had 231 passes/13 skips with 90/90 rules covered.

The fixed input for the canonical compiler unit is:

| Item | Value |
|---|---:|
| source bytes | 1,029,044 |
| source SHA-256 | `17c28914ec3cd109f0411cc8a83423623c1541be239e753e91144a66bea93f65` |
| tokens | 211,374 |
| AST nodes | 105,550 |
| per-column capacity | 1,029,045 |
| total request for 30 fixed columns | 214,041,360 B (204.13 MiB) |

The current three Token columns have widths `4 + 8 + 8 = 20 B/index`; natural AoS
stride is 24 B. The current seven Ast columns have widths `4 + 6 x 8 = 52 B/index`;
natural AoS stride is 56 B.

| Metric | Token + AST |
|---|---:|
| current fixed SoA request | 74,091,240 B (70.66 MiB) |
| fixed natural AoS request | 82,323,600 B (78.51 MiB) |
| semantic field bytes in the live prefixes | 9,716,080 B (9.27 MiB) |
| Token / AST capacity utilization | 20.54% / 10.26% |

These numbers expose three independent variables:

1. **Layout:** SoA has no row padding and gives a single-column consumer contiguous
   access. AoS makes whole-row producers and consumers contiguous and reduces
   repeated length/capacity operations across parallel columns.
2. **Allocation shape:** Token/Ast currently use ten field buffers, while AoS would
   use two allocations. Allocation count and layout locality must not be described
   as the same benefit.
3. **Initialization policy:** Every column is fully initialized to a capacity of
   `source_len + 1`, while only about 10%-21% is used. This may be the larger memory
   or cold-start problem, but it is orthogonal to AoS versus SoA.

A direct comparison of "current SoA versus growable AoS" would therefore bundle
three changes, and no result could support a language-design conclusion.

## 3. Actual performance and maintenance impact of the missing capabilities

### 3.1 Missing `buffer<Record>`

Performance constraints:

- A row-oriented lexer/parser producer must address, bounds/capacity-check, and
  write several allocations.
- An author cannot directly choose to share a cache line and address calculation
  among fields in a row.
- The language has no zero-cost representation for small records or whole-row
  traversal that is naturally suited to AoS.
- A library cannot encapsulate AoS as a reusable component and must instead repeat
  hand-written parallel buffers.

Maintenance constraints:

- Fields of one logical record are scattered across several globals/structure
  members and function parameters.
- Adding or removing a field requires synchronized changes to initialization,
  writes, capacity, borrows, and consumers.
- "Every column has the same length" remains a cross-object invariant rather than
  being encapsulated by one storage owner in the type system.
- Code review has difficulty distinguishing buffers that jointly form one tape
  from unrelated arrays.

The central issue is not record methods or inheritance; the data structure itself
cannot have the physical representation selected by its author. A future `impl`
could group the names of a hand-written SoA API, but it could not make AoS
expressible.

### 3.2 Missing initialized-prefix owning sequence

Performance constraints:

- A buffer preallocated for the maximum input must initialize the unused tail.
- `buffer<Option<T>>` adds per-slot state, branches, and drop scanning.
- A library cannot provide a no-growth push that is only a store plus `len += 1`
  after capacity has been reserved.
- Growth cannot express that elements have moved into a new allocation and the old
  block must be raw-freed without dropping them again.

Maintenance constraints:

- `buffer<T> + len` is merely a convention in a comment; existing buffer semantics
  still state that every slot is initialized.
- Reserve/grow behavior, borrow invalidation, failure atomicity, and exact-once
  dropping cannot be centralized in one opaque owner.
- Every library would reinvent an incomplete vector convention.

The current operation set lacks the substrate needed to uphold these invariants, but
that fact does not select a kernel `sequence<T>`. STOR-1 currently records growable
collections as future library structures over `buffer<T>` plus ordinary aggregates
and generics. One route would preserve that direction and add only the checked
storage operations a library owner cannot implement today. Another would reverse it
by adding an opaque kernel sequence. Either route must preserve the initialization
and destruction contract of existing fixed buffers; the latter also owes a full
META-5 delta and a new selection ground.

## 4. E0.1a option comparison

| Option | Performance expressiveness | Cost visibility / maintenance | Principal unresolved cost |
|---|---|---|---|
| A. Keep only hand-written SoA | No new cost | Existing P2 guidance remains singular; parallel-column invariants stay exposed | Row-centric AoS remains inexpressible |
| B. Infer automatic structural Copy | AoS uses existing Copy-buffer operations | No declaration or call-site marks a potentially large copy | Every eligible record changes ownership class and canonical `move` spelling |
| C. Add declarative `copy struct` | AoS uses existing Copy-buffer operations | The type declaration exposes the choice, but each later bare use still copies implicitly | Copy semantics affect assignment, arguments, returns, and matches, not only storage |
| D. Derive `Flat(T)` and add affine record storage | Keeps copying separate and can preserve both layouts | One record owner; ordinary uses remain affine | A sound, GRAM-9-compatible initialization spelling is unresolved |
| E. Add an `@soa`/`@aos` representation attribute | Explicitly names layout | Central declaration | Couples a source type to layout, field access, and possible ABI expectations |
| F. Select or convert layout automatically | Could theoretically choose per workload | Less source policy | Cross-function access patterns and hidden conversion costs make the choice unstable |

Options B and C are intentionally separate. Option C is the copy-struct tier carded
in the 2026-07-10 project record; Option B is the later structural-inference idea
that the record did not approve. Option D does not need a layout marker because
`buffer<Record>` already names a contiguous row per element, while parallel buffers
name SoA. None of B-F is selected by the present evidence.

No design-tree path moves while this decision remains open. If the owner later
selects a production route that supersedes the recorded declarative `copy struct`
path, the same production decision must move that path into the appropriate
`mcts_mem` `.alt/` branch and record paired reasons for the selected route and the
superseded alternative. Selecting declarative `copy struct` likewise requires
updating its card with the new decision evidence. This report does neither.

## 5. Design constraints and unresolved candidate space

### 5.1 Storage eligibility, OWN-1, and cost visibility

If the owner selects a Flat-storage route, `Flat(T)` would mean only that the target
can statically determine size and layout, that the value contains no region/borrow or
drop obligation, and that it can serve as a fixed-storage element. It would not mean:

- a bare use copies the value;
- every bit pattern, zero, or padding is valid;
- `memcmp` or bytewise hashing/serialization is allowed;
- field offsets, ABI, or FFI representation are stable;
- a whole-record load/store is necessarily cheap.

OWN-1 creates an important precedent that cannot be ignored. Tag-only enums became
Copy because their affinity bought no resource safety and forced Bool/state
workarounds with a measured 1.6-1.8x kernel loss. A narrowly defined Flat record is
also free of direct borrow and drop obligations, so memory safety alone does not
require it to remain affine.

Memory safety is not the only correctness axis, however. A nominal record with Copy
leaves can encode a unique authorization state, protocol token, or private-constructor
invariant even when none of its fields has a destructor. Automatically inferring Copy
would permit semantic duplication of that nominal state and would preclude a future
module from relying on one-value authority. Declarative `copy struct` leaves that
choice with the type author; an affine Flat route preserves nominal uniqueness by
default. This is a correctness and maintenance distinction, not merely a copy-cost
distinction.

Cost visibility remains independently material: a tag is small, while a record may
be 24, 56, or many more bytes. Automatic structural Copy hides that cost at every
bare use; declarative `copy struct` exposes the choice once at the type declaration
but not at each copy; explicit Repeat/Clone can expose it at the operation; an affine
Flat route forbids duplication unless separately authorized. The tag-only precedent
therefore frames the decision but does not settle it.

The first disposable Flat prototype admitted exactly represented integers, tag-only
enums, and recursively eligible non-empty named records. It rejected floats inside
records only because the stage-0 experiment used an intentionally narrow,
hand-maintained layout model and did not trust its float representation path. That is
a disposable implementation limitation, not evidence that `f32` or `f64` is
semantically non-Flat. Existing primitive float buffers were deliberately unchanged.
ZSTs, payload enums, arrays, owning or borrowing fields, erased/generic/unknown
types, finalizers, and by-value cycles were also excluded from that first experiment;
each needs its own justification before any production surface is selected.

### 5.2 Fixed-buffer initialization and the GRAM-9 conflict

Existing `buffer_new<T>(n, fill)` permits only Copy T because its semantics repeat a
once-evaluated `fill` N times. The prototype incorrectly accepts
`buffer_new<Record>(n, move seed)`: the backend loads `seed` once and then stores it N
times in a loop. Although `Record(inner: move seed)` is a fresh top-level construct,
it still copies the nested affine value N times. Flat proves only that a bit-copy
will not double-free; it cannot prove that nominal affine state or a capability may
be copied semantically. Writing `move` explicitly does not make contraction legal.

The earlier suggestion of a "recursively fresh row recipe" has a direct GRAM-9
price. Ordinary construction is flat: every construct field is an atom, and a nested
construct must first be bound by `let`. Once bound, however, the nested record is one
affine value; repeating the outer initializer would duplicate or repeatedly consume
that one value. A recursive per-slot recipe therefore cannot reuse ordinary
construction syntax unchanged. It needs a second recursive construction spelling or
must give up recursive records in the first slice.

The owner-visible alternatives and their preliminary lower-bound costs are below.
This is not a complete META-5 price for any route:

| Initialization route | What it preserves | Language/specification price |
|---|---|---|
| Dedicated recursive recipe | Affine records and recursively nested Flat fields; each slot is semantically fresh | A second recursive construction grammar, evaluation-count/effect rules, parser/AST/checker/diagnostic paths, and a FORM-1/GRAM-9/META-5 justification |
| Single-level dedicated initializer | GRAM-9 atoms and a relatively small operation | Nested record fields are excluded or flattened; a second initializer spelling and exact arity/field-order rules still enter the language |
| Per-slot builder or initialized-prefix owner | Ordinary ANF construction, one move into each slot | Couples E0.1a to partial initialization, failure atomicity, tail inaccessibility, and much of E0.1b |
| Explicit Repeat/Clone | Existing fill shape can remain and repetition is visible at the operation | Broadens value semantics beyond storage; needs conformance, effects, cost rules, and a decision whether repetition is bitwise, semantic, fallible, or allocating |
| Declarative `copy struct` | Existing `buffer_new<T>` and GRAM-9 remain unchanged for selected records | Adds declaration grammar, all-Copy eligibility checking, TYPE-2 aggregate layout/lowering, ownership-context rules and conformance for assignment/call/return/match; every later use is implicit Copy, visible at the declaration but not the call site |

Automatic structural Copy shares the last route's initialization mechanics but
removes even declaration-level visibility and changes every structurally eligible
record. No isolated candidate should be built until one route, its exact evaluation
semantics, and its META-5 surface delta are frozen. This report does not select a
route.

If an affine-storage route is selected and full initialization is solved, a narrow
candidate could allow scalar field reads/writes, `len`, existing conservative
borrowing, and replacement of an existing complete row with a fresh construction.
That candidate would still forbid bare whole-row reads and
`move index<Record>(...)`, which would leave a hole. Padding would have no observable
language semantics.

### 5.3 Required codegen shape

The target `DataLayout` determines size, alignment, stride, and field offsets. Record
allocation must check `count x stride`, alignment rounding, the target pointer-index
or `isize` limit, and allocation failure. Checking only `u64` multiplication overflow
is insufficient to make LLVM `inbounds GEP` valid.

The hot path of `index<Row>(rows, i).field` must be:

1. A bounds check consistent with existing semantics.
2. A row GEP.
3. A field GEP.
4. A scalar load/store for the field.

Field paths must not produce whole-record materialization, aggregate loads/copies,
`memcpy`, `byval`, or `sret`, and cannot rely on later SROA to "perhaps fix it."
Existing primitive-buffer/SoA fixtures in the candidate toolchain must have
byte-identical raw LLVM, and optimized assembly/call/trap/alias shape must also be
unchanged.

The reviewed disposable prototype satisfied this shape only on the frozen 64-bit
x86_64/AArch64 lane. It used an i64 allocation/index limit and hard-coded 64-bit
`_size_align` facts. On i386, `Row { u64, u8 }` really has size 12/alignment 4, while
the archived prototype declares `dereferenceable(16) align 8`; requests over 4 GiB
can also be truncated by 32-bit `malloc(size_t)` before writes continue. A production
implementation must derive stride, ABI alignment, pointer-index width, allocator
`size_t`, and `isize::MAX` from the target DataLayout. The reviewed prototype cannot
claim to be target-generic.

### 5.4 Limited evidence from the isolated prototype

The executable prototype was built in the disposable worktree
`/private/tmp/whitefoot-e01a-candidate`, used no feature flag, and never entered the
production toolchain. Its exact reviewed source is durable: commit `68a55e4` archives
the 57,547-byte [`DETACHED_CANDIDATE.patch`](DETACHED_CANDIDATE.patch), based on
`58baa71fb4c36a4728dd42aea6b05ce4be7aa0b1`, with SHA-256
`bed070414f9552ea105857404d6d1296b98542a28cc65fa6899a197830e6774e`.
The disposable executable/worktree need not survive; the reviewed source and its
identity do. That prototype passed:

- Full `make check`, ending in `ALL VERIFICATION LAYERS GREEN`.
- Checker 73/73 and modelcheck with 10,000 cases / 0 soundness violations.
- Four full-compiler/SoA raw-IR identity pins, with facts both on and off.
- All 135 baseline-success cases among 259 existing sources, with no acceptance or
  IR-hash change with facts either on or off.
- Fourteen legacy buffer signatures, including f32/f64 and different tag enums,
  byte-identical to baseline.
- Native run, ASan, and UBSan for the Flat fixture with facts on and off.
- 64-bit TokenRow/AstRow strides of 24/56 and field-only paths with no aggregate
  copies.

These tests show that the layout/codegen direction is implementable; they do not
show that the candidate semantics are sound. The suite missed affine contraction,
32-bit facts, and several independent mainline conformance defects. Commit `38d642e`
fixed only the first index-atom liveness/readability reproduction. Commit `7438e17`
covers the full index operand, match scrutinee, payload-borrow, try, and
contextual-return seams. Commit `50a1ddd` repairs recursive
projection and pins both executable parsers. Those repairs are independent of Flat
storage and are not evidence for or against nested records. A green candidate result
therefore cannot justify adoption.

## 6. Minimal semantic outline for E0.1b (implementation remains closed)

An earlier draft said that, after E0.1a, the next candidate should be an opaque
affine `sequence<T>`. That statement was too strong. STOR-1 explicitly records
growable collections as future library structures over `buffer<T>` plus ordinary
struct/enum and generics, rather than kernel constructs. Adding `sequence<T>` would
reverse that direction.

The following are requirements for any initialized-prefix route, whether it is a
library owner enabled by narrower checked storage operations or a new opaque kernel
type:

- Logical state is `{data, len, capacity}`.
- Only `[0, len)` is initialized, readable, and subject to drop. The tail is
  inaccessible and has no per-slot tag or bitmap.
- Separate `reserve/grow` from `push_within_capacity`.
- A push with proven `len < capacity` must be only an element store and length
  increment.
- Reject any potentially growing operation while an element/slice borrow is live.
- After successful growth, do not drop moved elements in the old allocation; only
  raw-deallocate the old block.
- Allocation failure leaves the old sequence intact.
- Select the initial element class only after the ownership route is chosen. An
  affine-Flat experiment may begin with `Flat`; a copy-tier experiment begins with
  the Copy class admitted by that route, whether inferred or declared. Arbitrary
  drop-bearing affine elements wait for STOR-3
  exact-once destruction.
- Provide `replace` initially; do not expose a hole-producing `take`, public
  `MaybeUninit`, or arbitrary partial initialization.

These properties make the initialized prefix an internal invariant of its owner
without adding a capacity field, growth branch, drop loop, or runtime `needs_drop`
test to an ordinary fixed buffer. They do not decide where that owner lives.

Preserving STOR-1 requires identifying the smallest checked operation set that lets a
library implement the invariant without exposing uninitialized storage. Reversing
STOR-1 requires a full META-5 declaration: type and operation spellings; storage,
ownership, borrow, growth, failure, and destruction rules; diagnostics and teaching;
exceptions added or removed; and an evidence-selected ground. If that redecision is
later approved, the design tree must move the old library-over-buffer direction to
an alternative in the same production change. This report makes no such decision
and does not authorize that change.

The initialized-prefix evidence may ultimately be more valuable than the layout
change: the baseline requests 204.13 MiB for a 1 MiB source while Token/Ast use only
about 10%-21% of capacity. That is still static evidence, not a performance result.
If the first initialized-prefix slice depends on `Flat(T)`, E0.1a must define and
trust that predicate first. If the owner instead selects a per-slot builder to solve
E0.1a initialization, the two phases become coupled and require a newly scoped
protocol rather than pretending the original serial boundary still holds.

## 7. Principal attack surfaces found by hostile review

1. **Flat bypasses affinity.** A moved fill, or a nested move inside a fresh wrapper,
   is evaluated once and written N times. This is a production blocker; the
   initialization semantics must change.
2. **Hostile review found independent OWN-1 expression-context defects.** Commit
   `38d642e` fixed the first dead-index-atom reproduction, but it covered only
   liveness/readability. Commit `7438e17` closes the broader offset ownership/type,
   match, payload-borrow, try, and return seams and pins each with conformance cases.
   These are no longer E0.1 design blockers, though a future
   detached candidate must include the repaired current-language baseline.
3. **Match scrutinees previously bypassed ordinary ownership flow.** Commit
   `7438e17` makes Copy matches non-consuming, affine matches consuming, explicit
   `move` of Copy illegal, and borrowed payloads provenance-preserving. This repair
   is current-language conformance, not a premise for selecting a record design.
4. **32-bit DataLayout/facts are unsound.** The i64 limit and manually computed
   alignment suit only the current 64-bit experiment. Production needs
   target-derived allocator/index/facts behavior.
5. **Recursive projection parsing had drifted from the language surface.** Stage 0
   combined `inner.value` into one field in `index<Outer>(...).inner.value`.
   Commit `50a1ddd` expands every suffix segment and pins stage 0 and wfc to the same
   recursive projection. The historical bug is not a reason to exclude nested
   records or invent different syntax.
6. **A recursive recipe conflicts with GRAM-9.** Nested ordinary construction is not
   an atom, while binding the nested record creates one affine value that cannot be
   repeated. Any recipe route must account for its second spelling and evaluation
   semantics; a single-level route must state its expressiveness loss.
7. **An opaque sequence reverses STOR-1.** It is not a neutral implementation of an
   already selected design. It requires an explicit META-5 redecision against the
   existing library-over-buffer direction.
8. **`F-SOA-P` is not a single-variable allocation-count control.** Coallocation
   also changes pages/TLB, alignment, pointer provenance, aliasing, and lifetime. It
   may be used only as a combined diagnostic, not for causal subtraction.
9. **F/R/D still change the entire owner policy.** Fixed-to-sequence changes header,
   API, checks, and drop behavior together. Reserve-to-doubling changes the capacity
   trajectory, reallocation, and bytes moved together. Report only end-to-end total
   effects.
10. **AoS padding can be overlooked.** Token grows from 20 to 24 bytes and Ast from
   52 to 56 bytes. Record requested, initialized, live, and touched bytes; RSS alone
   is insufficient.
11. **Producer time can conceal consumer behavior.** Measure single-column,
   mixed-field, child-link, whole-row, and complete-frontend workloads.
12. **Padding can leak into semantics.** Canonicalize results field by field; forbid
    raw record comparison or hashing.
13. **Field projection can take the wrong backend path.** Pin checker, typed-AST, and
    lowering behavior for `.field` after `index<T>`; do not first select the
    whole-element scalar fallback.
14. **Statistical rules can permit cherry-picking.** Freeze workload, phase, and
    memory metrics in advance. Gains must meet a conservative CI bound, while
    guardrails must use a preregistered non-inferiority margin.
15. **Cold and retained lifetimes can be unfairly compared.** Keep cold latency,
    lifecycle/destruction, and retained measurements as separate lanes, and do not
    carry capacity across corpus inputs.
16. **A feature flag creates two languages.** Rejected. Experiments use two isolated
    single-semantics toolchains.
17. **A capability experiment can be misreported as wfc migration.** These require
    different gates. A local AoS win cannot overturn SoA for column-heavy wfc.
18. **The pattern doctrine is a production gate.** P2 currently teaches an
    append-only index-linked SoA pool. A production AoS capability needs an
    owner-reviewed PATTERNS disposition and evidence-backed selection guidance; an
    isolated expert win cannot silently change the closed taught catalog.
19. **The default AI writer is a risk.** An expert selecting the right layout does
    not show that a low-tier writer will do so. Default teaching needs a separately
    authorized, benchmark-blind Terra writer panel.

An early attempt isolated the candidate with checker/codegen flags. Review correctly
identified that approach as violating a single canonical language, and all related
mainline code was fully withdrawn. The detached-worktree method now removes that
attack surface during experimentation instead of leaving it for production repair.

## 8. Experimental design and decision gates (scoring requires owner approval)

Conditional draft matrix after an ownership/initialization route is selected:

| ID | Layout | Storage / initialization | Attribution purpose |
|---|---|---|---|
| F-SOA | Current SoA | fixed, full-capacity initialization | Sole production baseline |
| F-SOA-P | Separate columns, one tape allocation | same fixed/full initialization | Combined coallocation diagnostic; no causal subtraction |
| F-AOS | Selected record-AoS semantics | same fixed/full initialization | Total representation effect versus F-SOA |
| R-SOA / R-AOS | One logical owner each | `reserve_exact`, initialized prefix | Total reserve-policy effect |
| D-SOA / D-AOS | One logical owner each | same doubling trajectory | Total doubling-policy effect |

For any non-builder initializer, E0.1a completes only F-* first; R/D belong to
E0.1b and do not start automatically because E0.1a performs well. A selected builder
route invalidates this serial matrix and requires a replacement protocol that prices
layout and partial initialization together rather than pretending they remain
independent arms.

Hostile review withdrew the early threshold of "wins on at least two non-toy
workloads": two results could be selected after the fact from corpus x phase x scale
x microkernel. Before scoring, freeze a machine-readable endpoint registry that
specifies the complete primary family, candidate/baseline ratio direction, single
primary memory metric, maintenance edit tasks, non-inferiority margins, sample
counts, and multiple-comparison or hierarchical gate. A time or memory gain must
place the conservative upper bound of the 99% CI at `<=0.90` or `<=0.85`,
respectively, rather than relying on a point estimate or "not significant." Every
other primary endpoint and guardrail must pass its preregistered non-inferiority
bound. The exact workload set and margins still require owner review, so scoring
cannot begin now.

Even if the capability passes, wfc migration has a stricter gate. For both complete
compiler cold and retained lanes, the 99% CI upper bound for candidate/S0 must be
`<= 1.000`; lifecycle/destruction, phase, microkernel, scale, memory, and stack
guardrails must all pass preregistered non-inferiority bounds; and there must be a
material gain of at least 5% time, 15% memory, or 15% maintenance burden. Both arms
must use the same 64 MiB virtual stack, while static frame/high-water usage must also
be pinned so a large reservation cannot hide stack growth. Mere parity does not
justify migration risk.

PATTERNS.md is also a production gate, not post-release documentation. P2 currently
teaches an append-only, index-linked SoA pool and gives its cache/vectorization and
scoped-alias rationale. Before any record-storage design enters production, the
closed catalog must state whether AoS is a blessed pattern, the measured access
shapes that select it, and the canonical fallback when those conditions do not hold.
The catalog's COMPLETE and EFFICIENT tests must remain satisfied. Until that review,
P2 remains the default guidance even if an isolated AoS candidate is technically
sound.

Default teaching additionally requires the PATTERNS disposition, an independent
low-tier Terra panel, and new external-disclosure authorization. Previous
percent-decode or utf8parse permission cannot be reused. This phase will not send
material to an external model.

## 9. Conclusions supported and not supported by current evidence

Supported:

- `buffer<Record>` is a real expressiveness gap, not merely wfc implementation debt.
- Storage eligibility and Copy are distinct questions; the evidence does not require
  one predicate to grant both.
- The previously carded declarative `copy struct` tier is distinct from automatic
  structural Copy.
- OWN-1's tag-only Copy precedent makes resource freedom relevant, while nominal
  authority and potentially large record copies make semantic uniqueness and cost
  visibility separate unresolved requirements.
- A derived Flat predicate alone cannot grant Repeat/Clone.
- A recursive fresh-recipe route cannot reuse ordinary construction unchanged under
  GRAM-9; every initialization alternative has a specification price.
- The current SoA has plausible performance justification and must not be replaced
  based only on a maintenance intuition.
- Initialized-prefix ownership is missing from the current operation set, but a
  kernel sequence would reverse STOR-1's library-over-buffer direction.
- A feature flag is incompatible with one canonical language.
- The 64-bit field-GEP/scalar-lowering direction is implementable.
- Safe initialization and target-generic layout must be resolved before any
  performance claim.
- The reviewed checker and parser defects were pre-existing current-language drift:
  `38d642e` fixed the first index liveness case, `7438e17` closes the remaining
  expression-context seams, and `50a1ddd` repairs recursive
  projection and strict GRAM-9. None selects an E0.1 design.

Not supported:

- Fixed AoS is faster for wfc.
- The current detached candidate is semantically sound or production-ready.
- `buffer_new<Record>(n, move seed)` is legal.
- A derived Flat predicate is preferable to declarative `copy struct`, or vice versa.
- Automatic structural Copy and declarative `copy struct` have the same visibility
  or ownership consequences.
- A recursive recipe, single-level initializer, builder, Repeat/Clone, or declarative
  copy tier has already earned selection.
- Benefits of an initialized-prefix sequence come from AoS.
- An opaque kernel `sequence<T>` is the only way to implement initialized-prefix
  ownership, or is already compatible with STOR-1.
- wfc should migrate to AoS or growable storage.
- `Flat` should enter the specification or production toolchain.
- Floats are semantically ineligible for Flat storage; their prototype exclusion was
  only a disposable stage-0 limitation.
- Parser drift justifies excluding recursive records from the language design.
- A default AI writer will reliably choose the right layout for different access
  patterns.
- A sequence of arbitrary affine elements is already sound.

External research likewise shows only that layout depends on workload. SoAx reports
SoA advantages in particle workloads; another Lennard-Jones study finds AoS better
under a particular vectorization/padding scheme; and SoCal suggests that factored
multi-buffer layouts can be strong for compiler-like recursive data. These results
justify bidirectional microkernels and complete wfc measurements; they cannot replace
project data.

## 10. Superseded decision questions retained as dense-family inputs

D11 supersedes these questions as an immediate decision surface. They remain a
checklist that a future dense-family Lock A must explicitly retain, revise, or
supersede; none authorizes work by itself.

1. Do you agree that safely expressing fixed `buffer<Record>` storage is a real
   E0.1a language-capability gap?
2. Which ownership route, if any, should receive the next isolated experiment:
   automatic structural Copy, the previously carded declarative `copy struct`, or an
   affine record with a separately derived storage predicate?
3. If records remain affine, which initialization price should be explored:
   dedicated recursive recipe, single-level initializer, builder/initialized-prefix
   coupling, or explicit Repeat/Clone? The operation spelling, GRAM-9 interaction,
   evaluation count, effects, and META-5 delta must be frozen first.
4. Which exclusions belong only to a disposable experiment, and which are intended
   semantics? In particular, float exclusion has no current semantic evidence. The
   parser/checker conformance defects have been repaired separately and cannot be
   reused as design premises.
5. For E0.1b, should the project preserve STOR-1 by enabling a library owner over
   `buffer<T>`, or reopen STOR-1 and consider an opaque kernel sequence with a full
   META-5 and design-tree redecision?
6. Should E0.1a and E0.1b remain serial, or may a builder route deliberately couple
   them under a newly scoped protocol?
7. Do you agree that the revised protocol must not be scored until the endpoint
   registry, lifetime lanes, and causal-control blockers are closed?
8. Do you agree that production adoption requires an owner-reviewed PATTERNS
   disposition, and that wfc migration and default teaching still require separate
   approvals rather than following automatically from capability adoption?

Production implementation remains frozen. The next step is a separate owner
discussion about whether to authorize bounded G0-Core work, not selection from
this historical list.

## References and local evidence

- Frozen local baseline: [`BASELINE.md`](BASELINE.md)
- Candidate semantics: [`FLAT_DESIGN_CANDIDATE.md`](FLAT_DESIGN_CANDIDATE.md)
- Full experimental draft: [`PROTOCOL.md`](PROTOCOL.md)
- Hostile review 0: [`HOSTILE_REVIEW_0.md`](HOSTILE_REVIEW_0.md)
- Hostile review 1: [`HOSTILE_REVIEW_1.md`](HOSTILE_REVIEW_1.md)
- Owner-advisor disposition: [`REVIEW_RESPONSE.md`](REVIEW_RESPONSE.md)
- Separation of external sources and inferences: [`RESEARCH.md`](RESEARCH.md)
- Current pattern doctrine: [`../../../docs/patterns.md`](../../../docs/patterns.md)
- Copy-classification design record:
  [`mcts_mem/whitefoot/ownership/copy-classification.md`](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/copy-classification.md)
- Current data-model direction:
  [`mcts_mem/whitefoot/data-model.md`](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/data-model.md)
- LLVM `getelementptr`: <https://llvm.org/docs/LangRef.html#getelementptr-instruction>
- LLVM aggregate code-shape guidance:
  <https://llvm.org/docs/Frontend/PerformanceTips.html#avoid-creating-values-of-aggregate-type>
- Rust `Vec` initialized-prefix guarantee:
  <https://doc.rust-lang.org/std/vec/struct.Vec.html#guarantees>
- Rust `Copy`: <https://doc.rust-lang.org/reference/special-types-and-traits.html#copy>
- Rust allocation `Layout`: <https://doc.rust-lang.org/std/alloc/struct.Layout.html>
- LLVM vectorizer: <https://llvm.org/docs/Vectorizers.html>
- SoAx: <https://arxiv.org/abs/1710.03462>
- Lennard-Jones AoS/SoA study: <https://arxiv.org/abs/1806.05713>
- SoCal: <https://arxiv.org/abs/2605.01140>
