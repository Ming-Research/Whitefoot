# Rust and C++ container comparison

## Question and scope

How does the current Whitefoot library perform against ordinary production
Rust and C++ containers when they complete the same application task, and
which observed differences deserve the next investigation?

The comparison was framed after the container delivery at `0f22b026b`; its
implementation now incorporates main `ad51e05df`. No timings were published
against the earlier baseline. It does not select a language amendment or
change a library algorithm. Existing C controls remain attribution tools,
not an asserted performance ceiling. The sources, commands, toolchain
identities, raw samples, and qualifications below travel with any ratio.

| Whitefoot family | Rust baseline | C++ baselines |
|---|---|---|
| GrowVector | `Vec` | `std::vector` |
| Deque | `VecDeque` | `std::deque` |
| HashMap | `std::collections::HashMap` | `std::unordered_map`, `absl::flat_hash_map` |
| PriorityQueue | `BinaryHeap` | standard heap algorithms over `std::vector` for returned move-only owners |
| OrderedMap | `BTreeMap` | `std::map`, `absl::btree_map` |

The first comparison reuses the five existing family drivers and their
independent behavior oracles. Slab, independently retained membership, and
the indexed composite require different comparison APIs and are outside this
first matrix. These synthetic traces provide discriminating costs, not a
real-application workload-frequency distribution.

The existing priority-queue traces consume the removed owner. C++
`std::priority_queue::top` exposes a const reference and `pop` returns nothing;
the native owning baseline therefore uses `make_heap`, `push_heap`, and
`pop_heap`, without implementing a private sift algorithm. Its two repairs for
replacement remain an API/algorithm difference. The existing raw-storage
setup/cleanup path does not heapify and is reported as a storage control,
outside the priority-queue ranking.

The deque's existing explicit rebase is not a common application operation:
`std::deque` has no corresponding reserve/rebase guarantee, and that old trace
does not use the extra capacity. A new common growth trace instead fills the
initial population, appends beyond it, and consumes all added values. Whitefoot
explicitly rebases its full deque; native deques use their ordinary growth.
Same-capacity hash rehash has no portable Rust standard-map operation and stays
outside the common ranking; reserve-for-more-entries is compared separately.

## Comparison contract

Practical comparisons preserve the requested application outcomes while
allowing each library its ordinary algorithms, representations, capacities,
growth policies, and optimized APIs. Do not implement the Whitefoot algorithm
inside a Rust or C++ wrapper and call it a standard-library comparison. Record
reference stability, iteration order, ownership of replaced/removed values,
logical capacity limits, and cleanup obligations for each trace. An adapter
needed to deliver the requested outcome belongs in its cost; an outcome that
the application does not require must not be imposed just to mimic Whitefoot.

Small scalars and wide inline values are separate cases. Wide bytes alone do
not establish nested-owner performance. Do not add a per-element allocation
to a competitor merely because its value is large. All consumed results must
contribute to the independent oracle, and every allocation must be reclaimed.

The existing map keys are `u64`. Returning an old value while retaining an
equal stored key produces the same outcome for those keys, but does not
establish equivalent behavior for distinct, comparator-equal owning keys.
Replacement at a logical ceiling must still succeed; use entry-style APIs
below the ceiling where available instead of imposing a redundant lookup.

Normal optimized builds provide the practical ranking. Retained public-helper
builds, optimized IR, and existing C variants are attribution evidence, with
their changed visibility and ABI conditions stated. Do not infer a causal
percentage by subtracting unrelated whole-trace timings.

Hash maps need two separately labelled questions: the ordinary default hasher
and an aligned hash calculation for attribution. Neither a cheap integer hash
nor a randomized default should silently stand in for the other. Different
table layouts and load policies remain visible in both series.

## Measurement criteria recorded before running

- Rebuild Whitefoot, native C controls, Rust, and C++ against one recorded
  source revision. Pin external-library versions; record compiler versions,
  flags, target, and allocator conditions. Historical samples are not the
  denominator for this run.
- Start with small, medium, and large populations already in each family
  driver, with scalar and wide values. Keep its complete trace, fixed input
  generation, correctness oracle, and consumption of results. Record any
  necessary trace change before using its measurements.
- Separate build time, correctness execution, allocation accounting, and
  timing. Practical timed builds use ordinary allocation without live
  accounting counters; separately instrumented executions report allocation
  counts, requested bytes, and peak live requested bytes. A requested-byte
  peak is not process RSS or allocator-resident memory.
- Use warmup and repeated paired samples with rotating implementation order,
  including a reverse-order cohort. Preserve all samples. A short or unstable
  cell is inconclusive until a longer bounded run resolves it. Aim for at
  least 1 ms per ranked sample; replay shorter cells with more work before
  making a close ranking. Report a cohort discrepancy above 10% in the ratio
  as unstable instead of merging the cohorts into one apparently precise
  number.
- Report each workload and payload independently. Use whole-trace elapsed
  time and per-operation normalization only where the denominator is defined;
  do not manufacture isolated lookup or growth latency by subtracting setup.
  Growth-focused traces locate a follow-up question, not a measured p99 pause.
- A repeated gap above 10% in both order cohorts is a triage signal, not a
  correctness gate or universal performance requirement. Smaller differences
  remain descriptive. Large memory differences and missing efficient APIs
  can justify investigation even where elapsed time is close.
- Attribute an observed gap only as far as evidence permits: application
  contract, hashing, algorithm, representation, allocation, or emitted code.
  A plausible explanation without a controlled discriminator stays a
  hypothesis. Record actionable unresolved questions in `docs/todo.md`.

## Reproduction and results

The explicit `ecosystem-build`, `ecosystem-check`, `ecosystem-account`, and
`ecosystem-measure` targets run the five family drivers sequentially. Wrap
them with the repository's `perl .github/run-check.pl <label> <command> ...`
guard and supply the prepared `WHITEFOOTC` and `ABSEIL_PREFIX`. These targets
neither rebuild the compiler nor download dependencies, and none is a
dependency of canonical correctness CI. Executable validation and measurements
are in progress; the final record will link the raw samples and rank follow-up
investigations without silently selecting optimizations.

### Current compiler integration

Main moved the libraries into compiler-bundled `std::collections` modules.
The five measured callers use record-local aliases and positional
`--emit-llvm <fixture>.wf`; they no longer bundle the removed
`lib/containers/*.wf` inputs. This retains all whole-trace scalar exports
without adding a module graph that would select only one entry closure.
Native runtime objects must come from the emitting compiler's revision.

The collection algorithms and public function signatures remain the same,
but module interfaces make representation fields readonly to clients. The
priority storage-only control therefore consumes its raw slots directly,
using the same reverse removal and cleanup operations as before, rather than
constructing a queue through its now-private representation or adding heapify.
Current enum constructors use `Enum<args>::Variant(...)`; match-arm labels
keep their ordinary contextual spelling.

Main also changed small aggregate results to register returns with internal
destination-form bodies. Fresh measurements include that implementation.
Scalar whole-trace C interfaces are unchanged. A retained-helper follow-up
must select qualified public standard-library symbols and exclude generated
`.body` definitions, or it would retain an extra implementation boundary.
This first practical matrix does not add retained native variants.

Frozen historical candidate measurements and their source identities remain
historical. Their replay requires the recorded checkout/compiler; this work
migrates the five current comparison callers rather than claiming that every
old experimental candidate accepts the new specification.

The prepared local toolchain is Apple M1 Pro, eight logical CPUs, 32 GiB,
Darwin 25.6.0 arm64; Apple Clang 21.0.0 and Rust 1.98.1 (LLVM 22.1.8).
Rust and Clang therefore do not share an LLVM version; their practical cost
comparison cannot alone attribute a difference to the source language.
The rebuilt current gate compiler SHA-256 is
`cb918e191bb344733347e0602171d2ec53bd1d201044fdbc5dd7666468eea0a0`;
the specification SHA-256 is
`59951ec5e42c0daa46947d88448be3fae9ca14276ad3600c03af03a54ef74d83`.
Rebuilding the compiler after main integration took 69.28 s. The earlier
prepared compiler is not a timing baseline.

Abseil is pinned to release
[`20260817.0`](https://github.com/abseil/abseil-cpp/releases/tag/20260817.0),
commit `2065f4ded0558c6f89fee67c8e5228feb4eb960e`, built with CMake Release,
C++20, tests disabled and installation enabled. The downloaded official commit
tarball SHA-256 is
`db5de644b448f9c3de4c03fcf5ffc3da0dd0c15363d8af25463c25b2a5db8952`.
The library remains an external experiment dependency, not vendored source or
a new compiler/gate prerequisite. Preparation took 1.74 s to configure and
52.72 s to build/install Abseil; neither duration is a container execution
measurement.
