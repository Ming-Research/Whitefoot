# Runtime window initialization

## Question and scope

Can an independently restored Copy stack use the language's initialized
prefix window directly, without ordering all restored elements through one
length update? This is the Q82 dependency discovered in Snowghost X5.
The example is a runtime-length range of Copy values, independent indexed
initialization, and subsequent push/pop operations; it does not depend on
renderer types or an input page.

This investigation owns the selection and validation record for a full
runtime Slots constructor. Keep it with the decision it supports; supersede
it in place if its scope changes. The compiler implementation and normative
cases belong in their existing source and test directories.

## Existing boundary

At base c61498f8a9bd916c30f0edfc3350a0732ee580b8, TYPE-9 confines runtime
Array and Slots storage to a Box and forbids moving that content into an
inline binding. PRE-1's slots_from_array and slots_into_array accept only
fixed-capacity values. An empty Slots window cannot be indexed beyond its
initialized prefix, and source cannot set its readonly length.

A sequential append loop can express restoration but adds a shared-length
chain. An application can also keep Copy values in a full Array with its own
logical length, but then it implements a different storage abstraction.
Neither observation means the complete stack program is inexpressible.

## Proposed direction, pending owner approval

Add box_slots_filled<T: copy>(count, value), returning one owned
Box<Slots<T>> whose length and capacity equal count and whose initialized
values equal value. Calls write the element type explicitly. The prelude row
publishes the ordinary measure and generic range postconditions.

The dependency chain is allocation and initialization, independent indexed
writes, then consumption of the completed window. Initialization does not
publish elements one by one through a shared length. The runtime cost of
filling and overwriting Copy values remains real and unmeasured.

A boxed full-Array/full-Slots conversion would also preserve independent
initialization and could transfer non-Copy elements. It is the stronger fit
when the producer already owns a full array. This witness can construct the
final window directly, avoiding that intermediate representation and its
conversion stage. Array and Slots have different descriptor layouts under
the current compiler representation; zero-cost conversion is not presumed.
The boxed conversion question stays open for a consumer requiring it.

An application-defined length over a full Array admits this Copy workload,
but its unused capacity still owns initialized values and its pop does not
transfer storage ownership. It would replace the existing window boundary
and effects rather than supply the missing constructor.

The existing operations also express parallel restoration by making one
owned window per leaf, recursively constructing disjoint halves, growing the
left result and appending the right result. Each level is independent across
siblings, but parents wait for both results. This adds logarithmically many
merge levels and, for ordinary balanced merges, n log n element transfers
through append plus any copying grow needs. There are linearly many leaf
allocations and merges. These are structural deductions for that algorithm,
not measured costs or a claim that its execution span is logarithmic: a
serial bulk copy on the root path can still total linear work.

The existing append contract gives two lower bounds on the destination's
length, not their sum. A recursive implementation can prove its operation
domains from capacities and read the final length; an exact length result
contract needs an additional check or a separately justified contract
change. This proposal does not change append. Direct filled construction
avoids the intermediate windows, merge dependencies and repeated transfers.
No measurement has selected this proposal.

## Rule delta

PRE-1 adds the explicit Copy constructor and its length, capacity and fill
contracts. OP-13 includes it among construction functions and states full
initialization. STOR-8 includes it in the allocating operations unavailable
to no-heap programs. TYPE-9, ownership and release, initialized-prefix
semantics, subscript bounds, push/pop domains, explicit generic arguments,
measure derivation and range derivation remain unchanged.

The constructor uses the current allocation-size and heap-exhaustion rules.
Those rules already differ from Snowghost's compiler pin; they are not part
of this proposal. No Snowghost pin change follows from this record.

## Validation criteria, written before implementation or execution

- Restore independently stated u64 sequences at zero, one and multiple
  lengths, consume them in the expected order, then exercise grow/push/pop.
  Do not compare a transformation only against its own round trip.
- Require the independent indexed initialization loop to receive parallel
  permission and the same expected result with one and four workers.
  Changing it to shared append or writes to one common element must fail
  the independence expectation; do not change any safety verdict.
- Check precise length/capacity facts and integer fill facts. Writing one
  slot must invalidate the affected content fact. Non-Copy fill, missing
  explicit type arguments, empty pop, full push, index at len and no-heap
  construction must reject for their owning rules, with each failure
  independently witnessed rather than hidden behind an earlier rejection.
- Cover zero count and zero-byte element storage without element access
  outside the initialized extent. Verify allocation/release through the
  existing runtime ownership checks without introducing a second owner.
- Reuse maintained test harnesses and inspect existing coverage before
  adding cases. Run focused checks first, coordinate host-lock use, and do
  not queue autonomous build or benchmark matrices. Cache reuse preserves
  every source judgment; timeouts and lock contention are not passing tests.
- The canonical gate and independent completion review remain necessary.
  No performance speedup is claimed without a separate prior comparison.

## Results

The implementation and focused cases are present on the work branch. The
recommended interface remains unapproved. Focused semantic and native
validation passes; the complete gate and Snowghost integration remain
unverified.

Six focused Rust tests cover lowering operands and allocation obligations,
fill facts and invalidation, independent-loop permission and shared-write
refusals, one/four-worker known-sequence execution, aggregate/zero-byte
ownership, and allocator alignment. Four conformance rows cover the positive
constructor and the non-Copy, implicit-type and no-heap refusals. The shared
program exercises zero/one/multiple lengths and grow/push/pop against stated
values. The huge zero-byte native witness uses the existing harness's
five-second deadline so a mistaken element loop fails promptly.

At 7abe0da41b07e136e43e47a23603daa06fb50bef on an Apple M1 Pro running
macOS 26.6.2, the six-test `filled_runtime_slots` filter passed (22.66 seconds
including 20.19 seconds rebuilding the test binary; tests took 1.71 seconds).
The run used the identical source tree ecc92cb643d8c8865f11336e7bff934f5d5ac00b
before a commit-message-only amendment. The existing window backend group
then passed all 25 tests in 5.59 seconds including startup. Both used the
ordinary gate profile, with its assertions and overflow checks enabled,
and separate bounded host-lock acquisitions.

The initial focused run failed three of six tests in 67.07 seconds. It
exposed a missing SSA operand prefix in the new element-fill store and an
excess effect declaration in the proof-only test consumer. The repairs
preserve the pointer helper's existing convention and the same range-fact
acceptance/rejection expectations. The failed log remains alongside the
successful runs in `compiler/target/filled-window-validation/`; earlier
lock-busy attempts remain recorded and are not passing tests.

`make spec-prose-integrity design-lint` passed at
67202ea37a1db58f8123d1536438bd195f528233. This is a prose/design check, not
compiler validation. The four new conformance rows have not yet run through
the compiler. The canonical gate and independent review remain unrun.

## Found along the way

The compiler prelude design named twenty owned operations, while the base
COMPILER_OWNED_PRELUDE_ROWS inventory declares twenty-six. Remove the stale
count from the decision; the source inventory continues to own enumeration.
This wording repair changes no dispatch or implementation decision.
