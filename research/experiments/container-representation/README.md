# Container representation experiments

This bundle supplies executable evidence for container representation and
performance choices, including historical designs and current library costs.
The design question belongs to
`research/investigations/containers-and-resources/`; this directory owns only the
reproducible source probes, finite checker model, native controls, and measurements.
Its parts answer different questions:

- [Rust and C++ comparison](ECOSYSTEM.md): practical standard-library and
  Abseil baselines for the five reusable sequence, map, and queue families,
  retaining the C controls to investigate observed differences. The comparison
  contract and measurement criteria are recorded before implementation.

- `x1/`: explicit post-PR-70 source probes separating ordinary vacancy
  exchange, ordered drain and copy-element spans from specified interface
  limits; results and reproduction are in `x1/RESULTS.md`.

- `dense/`: executable Whitefoot construction/update probes, helper boundaries,
  and native controls for aggregate versus final-destination storage; source
  capability and machine cost are reported separately.
- `lifecycle/`: actual compiler outcomes for pool contracts, element properties,
  complete and partial linear cleanup, and wrapped access, with nearby invalid
  programs that must still be rejected.
- `authority/`: finite concrete range certificates compared with an independent
  per-slot oracle; this is neither a language extension nor evidence that a
  symbolic library implementation already passes the compiler. A separate
  native two-index control distinguishes weak identity, retained membership and
  logical access retirement without claiming a WF lifetime design.
- `foundation/`: matched native layouts, a safe Rust boxed-array baseline,
  concrete construction/failure/retirement protocols, and a finite comparison of
  whole-result and field-destination construction. A current Whitefoot wide-result
  probe distinguishes these proposed mechanisms from implemented capability.
- `costs/`: same-algorithm native hash lookup layout and extent-validation
  controls, with a matched-query Rust standard-map comparator. It tests whether
  a retained runtime check is material under its stated workload and boundary;
  an owning sparse-map control additionally compares one-backing layouts through
  mutation, migration/refusal and cleanup. These are not checked Whitefoot map
  implementations.
- `families/`: current-language hash-table operations, a binary heap with a
  native cost control, an ordered-node component, and variable-record byte-page
  insertion/deletion; boxed-entry dynamic migration and matched byte-growth
  allocation/refusal controls. Intentional rejected forms distinguish source
  restrictions from the repaired nested-region substitution and Box replacement
  defects. Both former defect witnesses now execute in the ordinary check;
  owning key-generic maps and element-generic queues exercise static behavior
  groups, branded keys and hostile equality. Their retained-helper measurements
  separate behavior binding from owning exchange and value/result ABI costs;
  `families/RESULTS.md` states coverage, attribution and remaining limits.
- `vector-library/`: the reusable source `GrowVector` operation chain against
  reverse/pop and direct ordered-consumption C controls over v0.60. Scalar
  and 256-byte records exercise reserved, growing and reused backing with
  ordinary and retained helpers. An independent sequence oracle and matched
  allocation bytes precede timing; `RESULTS.md` separates lowering from
  source-composition costs and identifies the retained historical samples.
- `slab-library/`: bounded generation-handle lookup and reuse over the actual
  library, compared with a matching one-slot layout and a compact tagged C
  cell. Both scalar and wide inline payloads retain ownership and cleanup.
- `deque-library/`: two-ended churn and wrapped rebase over the actual library,
  compared with the same element loop and a bulk two-extent C conversion.
  Normal and retained helper measurements distinguish source composition from
  lowering; neither experiment supplies a workload-frequency distribution.

## Current-library replay

The Slab, Vector, Deque, PriorityQueue and shared-heap Indexed harnesses use
`std::collections` from the selected compiler. Those library records are
embedded in `whitefootc` (`compiler/src/library.rs`), so rebuild the compiler
after a library edit; the harnesses depend on that executable, not a separate
on-disk library checkout. `WHITEFOOTC` selects both the compiler and its library.

```sh
make -C compiler build
for name in slab vector deque priority indexed; do
  perl .github/run-check.pl container-replay \
    make -C research/experiments/container-representation/$name-library check
done
```

On the compiler source at `4459df880b04a7e87f46398969e1382b52637af0`, all
five checks passed in normal and retained modes on Darwin arm64 with Apple
Clang 21.0.0. The gate-profile compiler SHA-256 was
`419c1aa4108d8bfcd4c5a0c8df806bcd3060811911760fce8c959f8a0abb16cd`.
These are correctness and retained-boundary checks, not new timing results.
The priority [comparison contract](priority-library/RESULTS.md) describes the
raw-prefix cleanup boundary and the limit on using the earlier port's timings.

Map's alternative layouts, Ordered's insertion snapshots and Indexed's
`HEAP_STYLE=standalone` remain historical replay modes. Their source syntax,
overlays and pinned identities need a complete port before current-compiler
use; the default bundle-wide targets below are not a claim that those modes
have been ported. Their dated results remain unchanged. The reopening scope
is recorded in [the maintained TODO](../../../docs/todo.md).

These small programs test specific capabilities and costs. They are not a
representative corpus of real applications and supply no workload-frequency data.
Broader demand selection needs evidence from established applications in languages
such as C++, Rust, and Go; that external study is not part of this bundle.

From the repository root:

```sh
perl .github/run-check.pl container-research-check make -C research/experiments/container-representation check
perl .github/run-check.pl container-research-measure make -C research/experiments/container-representation measure
```

These are explicitly invoked research targets, never dependencies of daily CI
or canonical `make check`. Historical sub-experiments retain their recorded
language conditions; their sources and verdicts are not an x1 capability claim.
The focused x1 entry is `make -C research/experiments/container-representation x1-observe`,
run under the shared guard as `x1/RESULTS.md` describes.
The focused Slab/Deque entries are `slab-deque-check` and
`slab-deque-measure` in the same Makefile, also run under that guard; they do
not require historical experiments to accept a newer language revision.
`measure` also records timing samples and retains
the foundation probe's producer boundary for generated-storage inspection;
timing is descriptive evidence, not a host-speed-dependent acceptance threshold.
The C harnesses in `dense/` and `families/` include `native.mk` to link the
same ordinary prelude implementations and private runtime dependencies as
`whitefootc`. Their LLVM adapters restore the controls' closed helper linkage through
`linkage.rs`, expose measured functions, rename the fixture launcher, and
instrument the stated allocation sites; they do not
substitute special host operations or fake implementations. Keep the shared
source list aligned with the CLI's `runtime_units` when that library changes.

Each subdirectory's `RESULTS.md` states its measured revision, interpretation, and
limits. Results from different evidence levels must not be substituted for one
another: an unsafe native control is not an accepted Whitefoot program, and a
finite certificate model does not establish a production proof or backend.

When production tests and maintained measurements supersede a sub-experiment's
architecture question, merge or retire it with an explanation. Do not preserve an
obsolete expected rejection as the verdict for a newly selected language rule.
