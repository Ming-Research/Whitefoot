# Halo heap core results

2026-10-04. Base revision: `fccf91aabf6bfa2ff9da90b25cab65783814a569`.
The source hashes below identify the tested worktree. Only the heap and this
experiment changed; no compiler, other Halo module, specification, design tree,
network, push or PR was used.

The lead selected PUC's scatter layout to make hole borders and scalar-key
iteration order equal Redis Lua, replacing linear probing and its order difference.

## Comparison result

Every module accepts, the bound module program builds, and the native assertion
suite and both independent value models pass. All 9,507 trace rows match Redis's
Lua, including every border field and every ordered key/value pair. This replaces
the earlier 68-border-mismatch result; no oracle expectation was weakened.

```
Module check exit: 0; seconds: 0.213
pkg::value: accepted
pkg::number: accepted
pkg::lex: accepted
pkg::heap: accepted

Mode: bound module program.
Build exit: 0; seconds: 2.163

Native exit: 0; seconds: 0.251

Lua exit: 0; seconds: 0.030

Behavior rows: 2336
Ordered pairs: 7107; snapshots: 64
Trace rows: WF 9507; Lua 9507; mismatches 0
```

The existing 2,336 behavior rows include 2,048 random operations with deletion
of the current traversal cursor and 288 dedicated insertion-order/hole `#`
observations. The added eight mixed-key sequences make 8,192 assignments and
1,327,104 independent dense-model lookup checks (162 after each assignment).
They compare 7,107 ordered pairs in 64 traversals, with terminal pair counts
and borders. Their 162 input keys include fractions, negative and large numbers,
infinities, subnormals, finite extrema, both booleans, long strings differing in
their first byte, and short strings containing zero and byte 255. The assertion
suite retains its interning, allocation/release, error and signed-zero checks.

The one-sequence mixed sample built in 2.234 seconds and ran natively in
0.357 seconds. That bounded sample justified eight sequences; no longer run
or performance comparison was needed. Build time is separate from execution.

## Source correspondence and controls

`ltable.c` supplies chained scatter nodes and Brent relocation, lookup that
reuses a nil-valued key, replacement of a nil-valued main position, a decreasing
free-node scan, and rehash only when that scan is exhausted. Histogram sizing
counts non-nil values plus the extra key even if assigned nil. Resize preserves
array entries, reinserts a shrinking array's vanished slice in increasing order,
and reinserts old hash nodes in decreasing order. Empty node storage is the
immutable dummy's equivalent: lookups return nil, traversal is empty, and the
first absent-key assignment rehashes. `luaH_getn` and array-then-node traversal
remain unchanged; the shared insertion helper serves live assignment and resize.

Two descriptions in the request differ from the actual bundled sources. This
port follows the sources and independent Redis oracle:

- `ltable.c:hashnum` hashes the unchanged double, sums its two unsigned 32-bit
  words with wrapping, and normalizes either signed zero to main position zero.
  There is no `luai_hashnum` macro or addition of 1 in this tree. Numbers and
  handles use modulus `(node-size - 1) | 1`; strings and booleans use power-of-two
  positions.
- Redis's `lstring.c:luaS_newlstr` uses `step = 1`, hashing every byte. Halo's
  previous stock-Lua sampled hash could not preserve Redis string order. The
  interning collision witness now uses two unequal 96-byte strings whose full
  hash is 3,554,209,104, retaining the full-byte equality check under collisions.

Discriminating controls were run with temporary changes restored afterward:

- The previous linear-probe table, with constructors adapted only to the new
  fields, built and passed the native models but produced 4,366 mismatched rows,
  including the original 68 border differences; the comparison exited 1.
- Adding 1 to the number before hashing built and passed native models but
  produced 3,999 mismatched rows; the comparison exited 1.
- The first mixed-key sample with the previous sampled string hash passed its
  native models but produced 330 order mismatches. Full-byte hashing eliminated
  them. This observed failure exposed the source difference above.
- The driver asserts detection of a changed field, a missing row and reordered
  pairs, and accepts equivalent space/tab tokenization.

## Interface and remaining scope

`new_table`, `table_get`, `table_set`, `table_border`, `table_next` and their error
results keep their signatures. The public representation adds `Node.next` (an
index or `no_handle`) and `Table.lastfree` (an exclusive scan bound). Their docs
explain the dummy representation and retained-key `node_count`. Function docs
now name scatter assignment and exact scalar-key traversal order. Tables,
closures and builtins hash handles instead of PUC addresses; this is the only
remaining key-order difference. Parity requires the same assignments and initial
array/hash capacities, and live interned string handles.

Collector marking/sweeping, memory-limit enforcement and readonly enforcement
remain with the requested future collector/callers. Existing logical accounting
estimates remain string 24 + byte capacity, table 48 + 16 per array capacity +
32 per node capacity, closure 24 + 4 per upvalue and upvalue 32, excluding slab
and intern reserve. These are unconfirmed logical estimates, not measured
Whitefoot or PUC layout sizes. No large-capacity exhaustion or additional target
qualification is claimed.

Found along the way: fixed the experiment graph's missing canonical blank line
and qualified constant uses (via an alias), which prevented the now-accessible
module fixture from building. Removed its superseded diagnostic-bundle path and
stale private-interface blocker guidance. Fixed Redis string hashing and replaced
the obsolete sampled-hash collision witness. VM.md's old layout and sampled-hash
statements are superseded by the lead's instruction; that file and design/TODO
records are outside this task's allowed edit boundary. No specification rule
changed and no approval log was written.

## Validation and review

The command in README ran through the real module graph with the supplied
compiler and Redis's Lua, without a compiler rebuild. `git diff --check` passed.
`make source-size` passed (direct exit 0, under one second). `make static` was attempted but its first stage could not acquire the host-wide
lock: PID 73540 owned an unrelated `firn/build` mutation run. Process inspection
was denied by the sandbox; the lock was left untouched. The full static group,
compiler Cargo suites and full `make check` were not run.

A separate read-only reviewer checked groups A, D, C, R, M and V against the
base revision, including source correspondence and design checks; T was not
applicable. It read the complete diff, affected sources, VM section 2, checklist
and design procedure, and inspected source identities, counts and control logs.
It did not rerun green suites. Its one V2 finding was a stale interface source
hash after a documentation clarification. The full bound comparison was rerun
on that final source and the identities refreshed; the reviewer verified the
local repair and reported no remaining finding within scope. No logic repair
or specification change was needed.

## Tested source identities

```
lib/halo/heap/module.wfm 32831d261994c14b04ac40528224a3ebd7ac351a745b1886c29e6729412d9d7d
lib/halo/heap/slabs.wf 85b21d638df714efd97400acd0b6b109d3896f706935b187bfc09bdb0f48f732
lib/halo/heap/strings.wf 625fc1cf41e830d5f95dba259f9f6c012a55d4e1b156bdd3047686eb7ee0e490
lib/halo/heap/tables.wf af0420e70b46ce75e1dedb1af97e21687388b2b1b6a1f9570c1668a70bcff186
research/experiments/halo-heap-core/test/module.wfm 32cfb0ec38068b052b0dac9786564b97eeb3dbdb4118555b3e043ec62383fbe8
research/experiments/halo-heap-core/test/trace.wf f746d78acad65d14c7a3e666c17c4faac07365bb242dfcee537d60a49f517161
research/experiments/halo-heap-core/modules.wfg 63eb0969a75c13d1ecc639770785ddd4ccde430ebff03309a27d20a07a21d953
research/experiments/halo-heap-core/reference.lua 16379d9bd8c16ca1ce397cee2066727ab786cdf92296154839a9650adac92138
research/experiments/halo-heap-core/run.py 952961a990adc592418a557de4c37cc44a95d09a2768471937943990ec8a25e9
```

Tool and oracle SHA-256 identities:

```
whitefootc 58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11
Redis Lua executable b6b64032ec45c39084dfabd605b6d06626cc38a9c43f0ece61132ad7aff2fb4a
ltable.c b4246557450a810759fadac652c716b444a3d619b7283f581915b7e0eb33f7d4
lstring.c 10a06d2d5194d94f5793fce4d367ef8a78894cd5ee71c72f0fa2e260a91e14dc
luaconf.h 0410ff22f66c275ba8fcee1fa87a0749d26d7952ed30d3bc9161688b39775464
```
