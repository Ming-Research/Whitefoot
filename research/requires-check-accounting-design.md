# Checked requirements, proof obligations, and performance check accounting

Status: **REVIEWED DESIGN — narrowed first slice accepted; no normative change selected**

Date: 2026-07-11

Scope: FN-8 checked `requires`, OP-4 proof-elided checks, compiler-generated
guards, per-check artifact accounting, and writer guidance for variable-size
outputs.

Review disposition:
[requires-check-accounting-REVIEW.md](requires-check-accounting-REVIEW.md).
Prototype progress on 2026-07-11: the accepted body-first analyzer, structured
relationship diagnostics, facts/facts-off equality check, 44-case diagnostic
oracle, and the review-B2 bounds-v1 checked-automation policy are implemented.
Base64 is the first dual-pinned whole-unit review root: 27 sites are
automatically accounted and no finding remains. Source acceptance is unchanged,
and report collection is byte-transparent. One independent correctness fix does
change indexed-element borrows from the erroneous base pointer to the checked
element address. Guard versioning, counterexample generation, domain records,
backend proof credit, and retained-site approvals are not implemented; the
manifest therefore requires `approvals: []`.

Repository status (2026-07-22): that implementation and its democ-bound
codegen-parity harness were retired. Reusable source cases remain under
`tests/codegen/`; the old runner is under `archive/tests/codegen/`. The material
below is preserved research, not current compiler capability or roadmap
authority.

This draft exists because PROOF-2 exposed two different failure modes that the
prototype originally retained safely but did not explain or govern:

1. a checked entry fact and the body can drift apart (`3:4` capacity versus an
   `o += 5` body), leaving inner checks retained; and
2. a writer can omit a useful checked fact entirely, accept slower code, and
   ignore a warning.

The opposite mistake is equally important: a writer can state a sound but
grossly over-restrictive requirement, such as demanding a decoder's 1 MiB
worst-case output buffer when 99% of inputs need less than 1 KiB. That can make
the hot loop fast while making the API and whole program worse.

The proposed direction is:

- preserve the language rule that every unproved access is dynamically checked;
- derive proof obligations from bodies rather than waiting for magic source
  spelling;
- generate a proven fast path plus the original checked fallback when doing so
  preserves semantics;
- give every in-scope implicit-bounds site a B2 disposition: proved,
  affirmatively intrinsic-dynamic, hard finding, or unaccounted;
- reject obligation-backed and indeterminate debt at the checked-automation
  subgate while preserving intrinsic-dynamic checks as the safety floor;
- withhold promotion credit from unapproved or overconstrained requirement
  domains, even when their checked facts make codegen sound; and
- teach that `requires` states an actual caller violation, never a mere common
  case or a convenient worst-case allocation bound.

## 1. Decision state

Already selected and not reopened here:

- [FN-8](../spec/kernel-spec-v0.6.md) is a checked callee-entry prologue. It
  is not `assume`, and a false condition traps.
- [OP-4](../spec/kernel-spec-v0.6.md) retains bounds checks unless a
  deterministic proof discharges them.
- [OP-5](../spec/kernel-spec-v0.6.md) never elides an explicit source
  `check`.
- [EFF-2](../spec/kernel-spec-v0.6.md) remains syntactic: proof does not
  silently remove `traps` from a function's declared row.
- [ERR-4](../spec/kernel-spec-v0.6.md) classifies expected input or
  environment failure as a value (`Result`) and contract violation as a trap.
- [DIAG-2/3](../spec/kernel-spec-v0.6.md) require retained/eliminated checks,
  proofs, and check density to be visible in the canonical artifact/report
  family.

Resolved by review and prototyped here:

1. obligation-driven body analysis;
2. review-B2 states for the bounds-v1 checked-automation subgate; and
3. writer guidance for exact versus variable-output APIs.

Deferred after review:

1. compiler-generated guard versioning and its frequency/size instruments;
2. B1 approvals, normalized domain records, and real GATE-1 audit authority;
3. backend, explicit, allocation, overflow, imported/transitive accounting;
4. the provisional `requires` spelling decision; and
5. experiments required before any addition becomes normative.

No normative rule ID, syntax, manifest format, or trap-ordering change is
selected by this draft. The checked-automation schema described below is a
prototype build-policy format only.

## 2. Constitutional and rule grounding

The design must satisfy all of the following rather than optimizing one in
isolation:

- **P0/R0:** missing a fact must not silently strand a hot loop far below the
  earned code-generation ceiling.
- **W1:** a weak writer must receive a mechanical obligation and repair, not be
  expected to rediscover a compiler pattern.
- **W3:** a writer may neither assert a fact unchecked nor silence performance
  accounting with a source-level escape.
- **T1/T2 and OP-4:** uncertainty costs a runtime check, never memory safety or
  UB.
- **R4:** proof is preferred; otherwise use a check or recoverable value, never
  silent corruption.
- **R3:** one spelling does not justify choosing the wrong semantic boundary;
  `requires` remains provisional until authorship evidence distinguishes it
  from alternatives.
- **GATE-1/D0a:** any mechanism intended to prevent a bad writer from accepting
  all performance debt must place approval outside that writer's ordinary
  authority.
- **ERR-4:** expected capacity shortage is not laundered into a contract trap.
- **DIAG-2/3:** final enforcement must consume byte-stable canonical artifact
  facts, not optimizer remarks or advisory prose. The current external
  `proof_report` is an interim build-subgate input and does not yet satisfy the
  canonical DIAG-2 boundary.

The [Constitution](../design/constitution.md) makes safety and cheat-proofness
floors. Performance pressure can create proof, guarded execution, or explicit
project approval; it cannot create an unchecked access.

## 3. The decidability boundary

For an arbitrary function, there is no complete algorithm that can always
answer:

- whether a particular check is reachable;
- whether it can fail on any execution;
- the weakest condition under which every relevant access succeeds;
- whether an entry check is intentionally partial; or
- whether a different trap point is part of the intended behavior.

Loops, recursion, input-dependent control, user calls, and unbounded storage
make the general verification problem undecidable. Even where a finite model
exists in principle, exhaustive analysis is not a practical compiler rule.
Author intent is not derivable from code at all.

Therefore, **a missing `requires` is not a general language error**. Making it
one would cause valid-program acceptance to depend on prover completeness and
would make acceptance unstable as the optimizer evolves.

Closed fragments are different. For the exact base64 shape, the compiler can
prove:

```text
i = 3k
o = 4k
stores use o + d for 0 <= d < 4
tail is exactly 0, 1, or 2
```

and derive the exact capacity condition:

```text
C >= 4 * ceil(N / 3)
```

or the overflow-safe equivalent used by PROOF-2:

```text
N <= 3 * floor(C / 4)
```

Within such a fragment the compiler may claim that a condition is sufficient,
and sometimes necessary and sufficient. It still may not claim that omission
is semantically wrong unless a separate source or project contract requires
the corresponding performance property.

## 4. Separate four concepts

The current discussion becomes confused if these are collapsed:

1. **Explicit requirement:** a source-written callee-entry check. False means
   contract violation and traps under FN-8.
2. **Implicit safety check:** an OP-4 check inserted for an otherwise unproved
   access.
3. **Optimization guard:** a compiler-derived Boolean that selects a proven
   fast path or an original checked path. False is not an error and does not
   trap by itself.
4. **Performance acceptance:** a project/toolchain policy governing whether
   retained checks, versioned slow paths, code size, and unused facts are
   acceptable for a promoted artifact.

An optimization guard must never be printed or reasoned about as though it
were a source requirement. A performance-gate failure must never be presented
as a memory-safety failure.

## 5. Obligation-driven analysis

The current PROOF-2 implementation begins with a recognized `requires` fact and
then asks whether the body has the matching loop. The proposed architecture
reverses that dependency:

```text
checked function body
        |
        v
body-shape analyzer
        |
        +--> candidate access sites
        +--> exact proof premises
        +--> proof obligation P(parameters)
        |
        v
fact/guard planner
        |
        +--> checked Q equivalent to/entailing P: direct proof
        +--> no Q, derivable P: guarded fast path candidate
        +--> Q/body mismatch: precise diagnostic + checked fallback
        +--> unknown: retained checks
```

For base64, body analysis should produce an obligation similar to:

```text
obligation output-capacity-lockstep {
  source: src
  output: out
  input_stride: 3
  output_stride: 4
  output_offsets: [0, 1, 2, 3]
  tail_cases: [1, 2]
  condition: len(src) <= 3 * floor(len(out) / 4)
  sites: [out[o], out[o+1], out[o+2], out[o+3], ...]
}
```

Every rejected premise must have a first-failure reason: wrong source, wrong
target, nonzero base, wrong stride, increment ordering, offset outside the
funded group, mutation, alias, user call, tail mismatch, or unsupported
control. This turns the existing adversarial corpus into a diagnostic oracle,
not only an elision oracle.

The analyzer remains deliberately incomplete. Failure to generate an
obligation retains checks and reports `unknown`; it never licenses codegen.

## 6. Compiler-generated guarded fast path

When the body yields a sufficient obligation `P` but no source requirement
supplies it, the compiler can preserve behavior with versioning:

```text
if P(actual parameters):
    execute proven fast region without the covered implicit checks
else:
    execute the original region with every ordinary check retained
```

For base64:

```text
if len(src) <= 3 * floor(len(out) / 4):
    encode_fast_without_output_bounds_checks()
else:
    encode_original_with_output_bounds_checks()
```

The guard itself is not a trap. If it is false, a legal corner case continues
through the original checked behavior. This avoids the observable trap-timing
change that silently inserting an FN-8 `requires` would cause.

Required soundness conditions:

- the fast region is entered only through an evaluated compiler guard whose
  lowered machine predicate `G_machine` is proved total and to entail the
  obligation `P` under the actual fixed-width arithmetic;
- the deterministic proof covers every removed check on that path;
- every guard subtraction/multiplication is proved non-wrapping or lowered to
  an overflow-safe equivalent, and every guard dependency remains stable;
- the fallback is behaviorally the original checked region;
- ownership, moves, drops, effects, and returned values join identically;
- the canonical artifact exposes the guard, both paths, and proof references;
- no raw `llvm.assume` substitutes for the guard or proof; and
- a guard/proof defect is treated as a T1/T2-class compiler defect.

Engineering cautions:

- naive loop cloning can cause serious code-size and instruction-cache cost;
- the checked path should be outlined or marked cold only when semantics and
  measured branch frequency justify it;
- a guard whose true rate is low may make code slower;
- nested versioning can multiply paths; a deterministic budget is required;
- versioning must not be selected from unstable profile data when acceptance
  or proof status depends on it; and
- the existing scoped-alias work already demonstrates that guarded parity can
  carry a large code-size bill, so size is a first-class metric.

An exact source `requires` remains useful. It deliberately chooses early trap
semantics and may let the compiler omit the fallback entirely, producing a
smaller artifact than automatic versioning.

A generated guard may be stronger than the weakest fast-path condition without
changing program semantics: false simply takes the checked fallback. That can
hurt hit rate and therefore fail the performance profile, but it does not
reject a valid call. The same strengthening in source `requires` changes the
accepted domain and is governed much more strictly.

## 7. Requirement relation to a derived obligation

Let `P` be a body-derived capacity condition and `Q` the checked source
requirement. Only a restricted analyzer may label `P` exact/minimal; otherwise
it is merely sufficient. “Exact/minimal” is always scoped to the named access
site set and proof obligation. It does not describe every function trap, prove
full completion, or define the intended public API domain.

| Relationship | Safety/codegen treatment | Performance/accounting treatment |
|---|---|---|
| `Q` equivalent to exact `P` | Directly prove covered sites | `discharged-exact` |
| `Q` implies sufficient `P`, exactness of `P` unknown | Sound to prove covered sites | `discharged-sufficient`; domain cost remains unknown |
| exact `P`; `Q` implies `P`, but `P` does not imply `Q` (`Q` is stronger) | Sound for covered sites on admitted calls | `overconstrained`; do not call the boundary optimal |
| exact `P`; `P` implies `Q`, but `Q` does not imply `P` (`Q` is weaker) | Cannot prove all covered sites | Retain/version; report missing strength |
| `Q` and `P` are incomparable | Prove only independently entailed sites | Mixed accounting with explicit residue |
| Relationship unknown | No logical use beyond independently recognized facts | Retain checks; report `unknown` |

The stronger-than-site-necessary row is essential for cheat-proof performance. A
writer must not silence inner checks by demanding absurd capacity and thereby
move the cost into callers, allocation, cache pressure, or normal-case traps.
It is a performance/domain-review finding, not proof that the declared API is
semantically wrong; a privileged fixed-workspace contract may legitimately be
stronger than access safety.

When exactness is proven, the diagnostic should include a witness. Example:

```text
overconstrained requirement:
  covered access sites need 512 bytes for this input
  requirement demands 1,048,576 bytes
  the fully checked implementation completes for this argument state,
  but the entry requirement traps
```

When exactness is not proven, the compiler must say so. It may report a
possibly over-restrictive API but cannot manufacture a hard semantic claim.

## 8. Artifact states and performance acceptance

Compiler truth, final code presence, and project authorization must be
distinct. Every implicit runtime-check **origin** first has one elaboration
proof state:

| Elaboration proof state | Meaning | Frontend action |
|---|---|---|
| `proved` | Deterministic proof discharges every path reaching this origin | May omit the check with `proof_ref` |
| `versioned` | A proved fast path omits it; the false-guard path preserves the original check | Emit guard + path-specific origins |
| `unproved` | No admitted proof discharges the origin | Emit the original check |

Each check origin/path instance separately records final lowering state:

| Final lowering state | Meaning |
|---|---|
| `present` | Check/trap edge remains in final code |
| `frontend-elided` | Frontend omitted it under an admitted proof |
| `backend-eliminated` | Frontend emitted it, but later optimization removed it |

The last row is not automatically equivalent to `proved`. Existing corpus
cases can be `unproved` in the external report while LLVM independently removes
the branch. DIAG-2-complete promotion requires importing a verified backend
proof/provenance reference; until then report `backend-eliminated-unverified`.
Its dynamic cost is zero, but neither W3 proof credit nor artifact-honesty credit
may be invented.

Review B2 narrows the first policy slice to obligation-backed implicit-bounds
debt. Its implemented disposition is:

| Policy disposition | Meaning | Promotion default |
|---|---|---|
| `automatically-accounted` | A valid admitted frontend proof removed the implicit check, after the complete schema-versioned analyzer set ran | Accept |
| `intrinsic-dynamic` | The site remains checked and every registered obligation analyzer affirmatively excludes it | Accept as the safety floor |
| `hard-finding` | The analyzer derived a dischargeable obligation but the source fact is missing or mismatched | Fail |
| `unaccounted` | Analysis/provenance is incomplete, a premise failed, state is unknown, a matched obligation remained checked, ceiling mode was used, or only unverified backend elimination claims credit | Fail closed |

Malformed state is a compiler/harness error, not a policy disposition. A valid
hard or unaccounted report yields “performance promotion failed,” never “unsafe
program.” Missing a fact still leaves the runtime check and never changes
language acceptance.

`not-applicable` is evidence, not a default. Every site begins with incomplete
analysis; the compiler iterates the schema-versioned analyzer registry, records
the exact analyzers that returned, and only then may an explicitly excluded site
become intrinsic-dynamic. A conservative candidate frontier covers every
nonliteral indexed write plus accesses through current unique-reference roots;
unrecognized candidate sites stay indeterminate. Fact-generated origins do not
inherit source-site analysis. These rules close the enumerated n27–n33
syntax/alias escapes and FN-4 metadata-copy escape found by adversarial review,
not every possible user control, call, import, or fixed-literal rewrite.

Versioning is absent from this slice per B4. It cannot become automatically
accounted until independently measured hit-rate and size gates exist.
Backend-eliminated checks receive no credit without verified provenance.

Profile scope is intended to be owner-selected. The current implementation
duplicates pins for base64's case, facts-on variant, function, source, SHA-256,
and closed-unit scope. The dedicated `--promotion` invocation requires the full
corpus plus default manifest, forbids filters, and verifies every pinned root
ran. It conservatively accounts the whole compilation unit rather than only the selected function, preventing
same-unit helper extraction from hiding debt. Exact reachable-instance/imported
closure remains deferred. These are two coordinated repository edits, not an
implemented authority boundary: protected external owner review must govern
changes to both pins before the workflow satisfies GATE-1.

Explicit source checks are outside the first classifier. Review B3 requires
them to be counted and cost-reported but always passing; no policy may create
an incentive to delete a defensive check. The current harness has only coarse
IR/assembly trap counts, so complete per-origin explicit inventory remains
deferred. They escalate only with independent profile evidence plus an
available proof/guard alternative.

This inventory is not omniscient. A writer can rewrite a check as ordinary
conditional/`Result` control that is not generally identifiable as a safety
check. Static check accounting makes no complete claim over arbitrary control;
codegen parity and end-to-end workload gates own that remaining channel.

The gate error is not “unsafe program.” It is:

```text
performance acceptance failed:
  function encode has 12 hard-finding bounds sites
  candidate obligation: output-capacity-lockstep
  first missing fact: len(src) <= 3*floor(len(out)/4)
```

This distinction keeps language acceptance stable while making the pinned
build's bounds-v1 disposition enforceable inside the prototype subgate.

### 8.1 Approval properties

A legitimate retained check must not force every legal program into warning-
only status. It can be approved, but the approval must not be a writer-owned
escape hatch.

Candidate approval record:

```json
{
  "function": "decode_chunk",
  "stable_site_id": "body.loop@decode.index[3]",
  "dependency_cone_digest": "<64 lowercase hex>",
  "fact_class": "bounds",
  "obligation": "output-capacity-lockstep",
  "reason_class": "prover-debt",
  "reason_detail": "source NeedMoreOutput guard dominates; prover did not consume it",
  "dominating_recoverable_guard_ref": "body.loop@decode.capacity-match",
  "gate_record": "owner-gate-record-id"
}
```

Required properties:

- no source-level `allow`, warning suppression, or blanket function exemption;
- exact function, stable site, reason class, and dependency-cone identity;
- a nonempty reason from a closed reason class plus prose detail;
- a reason that states the retained edge's actual failure semantics; claiming a
  recoverable result requires a dominating source guard reference—the OP-4 edge
  itself traps;
- changes inside the dependency cone invalidate stale approvals; unrelated
  artifact edits do not;
- approval never removes the runtime check or adds a trusted optimizer fact;
- approval affects performance promotion only, never safety acceptance; and
- approval is a GATE-1 operation outside ordinary writer authority.

Whole-artifact approval identity was rejected by review B1 because routine
churn would train rubber-stamping. A conservative dependency cone may
over-include control/data/call dependencies, but it must never omit one. No
approval is accepted by the current implementation; `approvals` is fixed to an
empty list until this identity and GATE-1 audit record exist.

The historical codegen-parity gate, now retained under
`archive/tests/codegen/`, pinned
per-site proof counts for selected cases. This proposal generalizes that model
from hand-enumerated regressions to canonical artifact accounting.

### 8.2 Proposed report fields

The DIAG-3 check report would need at least:

```text
artifact_hash
compiler / proof-schema / performance-policy versions
workload/profile corpus digest and review epoch
function
monomorphized or cloned instance identity
node_path / stable site id
fact_class
elaboration_proof_state
final_lowering_state
policy_disposition
proof_ref
proof dependency closure, including every source requirement
obligation_ref
guard_ref / fallback_ref
approval_ref
requirement-domain-ref
requirement true/false-rate and caller-cost evidence refs
target and index dependencies
first_failed_premise
counterexample_ref (when constructively available)
```

This is a proposed DIAG-3 delta, not an implemented claim. The prototype's
current external `proof_report` is not yet the DIAG-2 canonical artifact.

### 8.3 Protecting the accepted domain (general mechanism deferred)

Check accounting creates a second attack unless requirement domains are
protected. A writer can make local codegen look perfect by adding a predicate
equivalent to the following schematic requirement:

```text
requires false
```

or by demanding a maximum buffer so large that difficult inputs never enter
the body. The check is real and codegen is sound, but the benchmark/API domain
has been silently shrunk.

Proposed policy:

- in a performance-gated function, an explicit `requires` predicate may earn
  performance-accounting credit only when its exact normalized domain
  restriction is present in a privileged domain record;
- adding or strengthening that predicate should be reviewed as a GATE-1 domain
  change, even though the first FN-8 slice is concrete-only and absent from
  `fn_sig`;
- an unauthorized source requirement keeps ordinary FN-8 runtime semantics and
  may still be used soundly by codegen, but it cannot make promotion pass;
- the domain record pins the predicate, function/artifact identity, workload
  evidence, caller allocation consequences, and approval; and
- known stronger-than-exact predicates receive `overconstrained`, not
  `discharged-exact`, performance status.

This is intentionally stronger than retained-check authorization. The latter
accepts a runtime cost while preserving behavior; a requirement-domain change
can remove valid behavior from the function and export cost to every caller.

The implemented base64 pilot does not generalize this mechanism. Its review pin
binds the complete reviewed source digest, including the canonical 3:4 checked
condition that the analyzer classifies as equivalent to its derived sufficient
obligation; the digest is admission identity, not an exactness proof. Any
source or requirement edit invalidates that pin and needs protected owner
review. Before arbitrary roots or independently evolving public
APIs are promoted, the normalized domain record and caller-cost evidence above
must replace that pilot-level whole-source admission.

### 8.4 Candidate review invariants

The following are proposed rule sketches, not assigned spec IDs:

1. **Proof independence.** Failure to derive an optimization proof never
   rejects otherwise conforming source. The unproved operation stays checked.
2. **Checked-fallback versioning.** A compiler guard is pure, total, and
   non-trapping; its lowered fixed-width predicate is proved to entail the
   obligation; true enters a proved fast region, and false enters the original
   checked region from identical live-in state.
3. **Behavioral refinement.** The fast region preserves values, effects,
   writes, returns, and all traps not proved unreachable. Nothing before the
   split is duplicated.
4. **Source-check preservation.** The compiler never synthesizes, removes,
   weakens, or strengthens an explicit FN-8 check. A generated guard never
   acquires OP-5 semantics.
5. **Bounds-v1 completeness.** Every implicit bounds origin in the closed
   compilation unit has exact analyzer provenance and a B2 disposition;
   generated sites begin incomplete, and backend elimination receives no credit
   without verified provenance. Broader explicit/transitive completeness is
   deferred.
6. **Performance acceptance.** Every in-scope bounds origin is automatically
   accounted, affirmatively intrinsic-dynamic, or a hard/unaccounted promotion
   failure. Authorization is absent from the implemented slice.
7. **Exact authorization.** Wildcards, count-only allowances, writer-issued
   authorizations, and source suppression are invalid; any future approval is a
   GATE-1 record bound to a per-site dependency-cone digest.
8. **Domain protection.** A performance-gated requirement earns credit only
   under an approved exact domain record; strengthening is a reviewed domain
   change, not a local optimization edit.

## 9. Variable-output decoder case

Assume a decoder has:

```text
1 byte <= actual output need <= 1 MiB
99% of inputs need less than 1 KiB
```

The tempting requirement is:

```text
len(out) >= 1 MiB
```

That condition is sufficient for every valid input but usually wrong as the
API boundary:

- an argument state where the fully checked decoder would succeed with 512
  bytes can still trap at entry;
- callers are pushed toward reserving 1 MiB for nearly every call;
- normal allocation traffic, peak memory, cache/TLB footprint, and concurrency
  capacity can become much worse;
- the fact hides body-check cost by exporting larger cost to every caller; and
- a false condition is normal resource variation, not necessarily a caller
  programming error.

At the stated distribution, a 1 MiB reservation is roughly 1024 times a 1 KiB
normal output and can be vastly larger for smaller results. Whether virtual
reservation, pooling, or lazy commit reduces some physical cost is an empirical
question; it does not make the API restriction free.

### 9.1 The key rule

> Write `requires Q` only when `Q == false` means the caller violated the
> function's intended API contract. Do not use it for a merely common case,
> resource shortage that correct callers encounter, or a global worst-case
> bound chosen only to unlock codegen.

Because FN-8 traps, this follows directly from ERR-4:

- **caller bug / violated established invariant:** `requires` may be right;
- **malformed input:** return `Result<..., DecodeError>`;
- **ordinary output shortage:** return `NeedMoreOutput`, stream, allocate, or
  take a checked fallback;
- **likely fast case:** use a compiler optimization guard, not `requires`.

### 9.2 Choose the API from how `need(input)` becomes known

#### A. Exact size is a simple total function of input length

Base64 encode is the ideal case:

```text
need(N) = 4 * ceil(N / 3)
```

A weakest overflow-safe capacity requirement is cheap, exact, and reasonable.
A correct caller can compute it without inspecting content. FN-8 is a good fit.

#### B. Exact size is stored in a validated header or cheap preflight

Prefer a preparation step that binds the size result to the validated input,
then allocate or require exactly that size. Schematic API, not current syntax:

```text
prepare(src) -> Result<PreparedDecode, DecodeError>
required_output(prepared) -> u64
decode_prepared(out, prepared) -> Result<Produced, DecodeError>
```

`PreparedDecode` should carry or own the input/validated plan so the caller
cannot pair a size computed for one input with another input. A loose
`decode(out, src, claimed_need)` parameter would recreate a fact-authorship
hole unless the compiler can revalidate the relationship.

If the output buffer is too small after an exact, validated preflight whose API
requires the caller to honor the returned size, that shortage can reasonably be
classified as a caller contract violation and checked at the boundary.

#### C. Size is known only while decoding

Use one of:

- a streaming API returning `NeedMoreOutput` with consumed/produced progress;
- a growable/allocating output owned by the decoder;
- a two-pass decoder if the first pass is safe and competitive; or
- a checked original path plus a compiler-generated fast-path guard for a
  proven capacity region.

Schematic streaming behavior:

```text
decode_chunk(out, state, src)
  -> Result<DecodeStep, DecodeError>
```

`NeedMoreOutput` is ordinary control, not a trap. Each call may use the actual
buffer supplied, so the 1 MiB rare case does not price every common call.

A candidate canonical streaming result is:

```text
DecodeStep =
    Done { consumed, written }
  | NeedMoreOutput { consumed, written, min_additional }
```

The contract must pin whether the output-producing token was consumed. Prefer
returning before consuming it; otherwise the decoder state must retain the
pending token completely. On every return, `consumed <= len(src)` and
`written <= len(out)`. `min_additional` need not predict final output, but it
must be positive and sufficient for the pending atomic token/step: retrying the
same state/input with exactly that additional capacity must consume input,
produce output, or return a terminal result. This forbids zero-progress
`NeedMoreOutput` livelock.

This shape is schematic and not currently end-to-end lowerable: the prototype
does not yet support general multi-field enum payloads or
`Result<aggregate, E>` payload lowering. The review must either schedule that
compiler work or use a temporary, explicitly non-canonical status/struct ABI
for the experiment.

#### D. The API intentionally mandates a fixed maximum-size workspace

A 1 MiB requirement can be correct when the surrounding protocol already
defines a fixed 1 MiB frame/workspace, callers reserve it deliberately, and the
measured system benefits. That is an API decision, not a compiler workaround.
It should be recorded and measured as such.

The leading decoder architecture to test is therefore:

```text
public variable-output primitive:
    streaming Result with NeedMoreOutput

optional convenience wrapper:
    start near the measured common size, then resume/grow

internal exact-capacity kernel:
    FN-8 requirement only after exact capacity has been established

hot inner region:
    compiler-proved maximum-burst guard
```

This is a candidate for experiment, not a selected library or language rule.

### 9.3 Current FN-8 expressiveness limit

The first FN-8 slice can use parameters and pure-total table operations such as
`len`; it cannot scan compressed input, index a header, call a size function,
or carry an arbitrary proof object. Therefore it often cannot express
`len(out) >= need(src)` for a content-dependent decoder.

The correct response is **not** to substitute `MAX_OUTPUT`. Use a different API
shape or retain checked dynamic behavior until validated plans/refinements gain
their own reviewed design.

There is also a consumer gap: a predicate can be expressible and checked by
FN-8 without being consumed by any proof family. Today PROOF-2 recognizes only
the exact five-statement base64 `3:4` relation and exact associated loop. A
different exact expansion relation, `len(out) >= expected`, or a 1 MiB check
can execute correctly at entry while eliminating zero variable-decoder checks.

Externally reachable publication is also incomplete. FN-8 is absent from
contract `fn_sig`, gated FFI boundary frames are not implemented, and the
machine-readable trap report is still debt. Until those layers exist, an exact-
capacity `requires` on a foreign-facing function is enforced by the callee but
is not yet a complete machine-published boundary contract; keep such uses
audited in the closed unit and record the boundary debt explicitly.

### 9.4 Local guard-to-burst proof

Variable-output formats often have a small maximum output burst `B` for one
token even when total output is unknown. That supports a local checked fast
region without a whole-message capacity requirement:

```text
if o <= len(out):
    remaining = len(out) - o   // non-wrapping on this path
    if remaining >= B:
        decode one token and write at most B bytes without per-write checks
    else:
        execute the original checked boundary path
else:
    execute the original checked boundary path
```

The boundary path determines the next token's actual output. It may complete a
small token even when `remaining < B`; it returns `NeedMoreOutput` only when the
actual token does not fit. Treating `B` as the minimum would recreate the
overconstraint problem at token scale.

That recoverable boundary path must already exist in source semantics. Compiler
versioning may optimize its proven true path, but it cannot turn an implicit
OP-4 trap into `NeedMoreOutput` or invent resumable state. If the original
fallback consists only of bounds-checked writes, the false guard must preserve
those possible traps and partial effects exactly.

The proof obligation is local:

```text
o <= len(out), so remaining subtraction cannot wrap
remaining >= B
token_output <= B
o is stable through the covered address calculations/stores, then advances
  exactly once by token_output; output root and length remain stable throughout
therefore every out[o+d], 0 <= d < token_output, is in bounds
```

This is a stronger decoder candidate than a 1 MiB entry requirement:

- it reserves only one burst of capacity;
- covered output stores can lose their individual bounds branches;
- the near-end path remains recoverable;
- the same primitive handles one-byte and one-megabyte outputs; and
- no statement about the final decoded size is required.

This should be tested as a distinct proof family, not forced into the base64
whole-function 3:4 recognizer.

### 9.5 Resource limit is not a caller contract

A configured 1 MiB decompression limit protects resources, but an untrusted
input exceeding it is still an expected input failure. Candidate classification:

| Event | Result |
|---|---|
| Public streaming buffer fills | `NeedMoreOutput` value |
| Valid output exceeds configured limit | `Err(OutputLimitExceeded)` |
| Malformed or truncated source | recoverable decode `Err` |
| Header/size arithmetic overflows or exceeds representable allocation | validated size/resource `Err` |
| Internal exact-capacity kernel receives less than its documented exact capacity | FN-8 trap |
| Validated decoder reaches an impossible internal state | trap |
| Unproved index is actually out of bounds | implicit OP-4 trap, never corruption |

Turning a hostile or unusually large input into a caller-contract trap would
misclassify ERR-4 merely because the implementation has a limit.

Recovery requires validating the decoded/header size against the configured
limit **before** calling `buffer_new`. If execution reaches `buffer_new` and
its `n * sizeof(T)` byte-size computation overflows, OP-9 still traps; that
table-fixed behavior cannot be reclassified at the call site.

### 9.6 Current storage constraints

The clean allocating convenience API is not free in the current kernel:

- `buffer<T>` has a fixed length;
- `buffer_new` initializes the whole allocation, so pessimistically asking for
  1 MiB touches/fills it rather than merely reserving an address range;
- whole-buffer replacement is rejected until take/replace and old-storage
  release semantics exist;
- there is no growable buffer or chunk collection in v0; and
- logical length and capacity are not separate buffer properties.

The proposed `DecodeStep` additionally needs multi-field aggregate enum payload
and `Result<aggregate, E>` lowering. LZ-style decoders also need explicit
history/window state: emitting into independent chunks does not by itself
preserve prior bytes required by back-references.

Streaming into caller-provided chunks is implementable sooner. A contiguous
allocating wrapper should be evaluated together with the storage work, not
assumed available in the `requires` decision.

## 10. Writer guideline for `requires`

Apply these questions in order:

1. **Does false mean caller bug?** If a correct caller can normally encounter
   false, do not trap through `requires`.
2. **Is the condition input-specific?** Prefer the actual input's need over a
   type/global maximum.
3. **Is it the weakest practical boundary?** Do not demand more capacity or a
   narrower domain solely to make proof easy.
4. **Is shortage recoverable?** Use `Result`, `NeedMoreOutput`, streaming, or
   allocation when recovery is normal.
5. **Is it only a common-case predicate?** Let the compiler guard/version the
   fast path; common cases are not contracts.
6. **Is the arithmetic total and overflow-safe?** Prefer relations such as
   `N <= 3*floor(C/4)` over overflow-prone `4*ceil(N/3) <= C` in fixed-width
   source arithmetic.
7. **Does it name the actual data roots?** A length computed for another source
   or output cannot discharge this body's accesses.
8. **Does the body still match?** Strides, offsets, tails, mutations, aliases,
   and calls must agree with the fact.
9. **What consumes the fact?** The report should show which obligations/sites
   it discharges. An unused optimization-motivated requirement is a finding.
10. **What cost moved to callers?** Count allocation bytes, reserved memory,
    copying, preflight passes, latency, and code size—not only inner branches.
11. **Would a stronger requirement game the gate?** When an exact body
    condition is available, equivalence rather than mere sufficiency is the
    clean result.
12. **Is the boundary externally reachable?** Direct C/FFI entry still executes
    the check; document the contract in the boundary frame when that layer
    exists.

Short form for the teaching pack:

> `requires` is for “this call is invalid,” not “this call is uncommon.” State
> the weakest practical, input-specific condition. Expected shortage is a
> value; a likely fast case is a compiler guard.

| Situation | Preferred expression |
|---|---|
| Fixed exact expansion computable from parameter lengths | Exact overflow-safe `requires`, if too-small capacity is a caller bug |
| Exact size established by a validated plan bound to this input | Internal exact-capacity `requires` |
| Buffer fills during an ordinary streaming decode | `NeedMoreOutput` value with progress |
| Malformed/truncated input or configured output limit | Recoverable `Result` error |
| Predicate is merely likely and makes a profitable region safe | Compiler guard + checked fallback |
| Only a global worst-case maximum is known | Checked/streaming/allocating API; do not substitute the maximum merely for proof |
| Dynamic check is intrinsically necessary | Retain it and account/authorize the exact site |

## 11. Diagnostics

Diagnostics should explain proof state but should not be the only enforcement
mechanism.

### 11.1 Body/fact mismatch

```text
requires-proof-mismatch:
  checked fact describes input stride 3 / output stride 4
  body advances output by 5 at <node path>

  expected invariant: o = 4*k
  observed mutation:  o = o + 5

  consequence:
    frontend retains 1 output check; final backend presence is reported separately

  witness (restricted affine model):
    len(src)=15, len(out)=20 passes the requirement
    but reaches out[20]

  fixes:
    restore o += 4; revise the API/capacity condition; or request an exact
    retained-check approval if dynamic behavior is intentional
```

### 11.2 Missing boundary fact

```text
missing-proof-input:
  frontend retains 12 output bounds checks in a recognized 3:4 loop
  derived sufficient condition: len(src) <= 3*floor(len(out)/4)

  candidates:
    add the exact checked requirement if false is a caller violation;
    allow compiler guard versioning; or retain/approve the dynamic checks
```

### 11.3 Overconstrained requirement

```text
overconstrained-requires:
  source requirement is stronger than the exact covered-site condition
  fully checked implementation succeeds outside Q: witness <values>

  this fact may remove checks but needs an approved domain rationale before
  satisfying the performance gate
```

### 11.4 Unknown

```text
proof-obligation-unknown:
  compiler cannot derive a stable output-capacity relation for these sites
  frontend retains all sites; final backend presence is reported separately
  no semantic defect is claimed
```

Diagnostic names and severity are provisional. The deterministic structured
report state—not whether the terminal paints a line yellow or red—is the
interim build-subgate input. Canonical byte-stable DIAG-2 state remains deferred.

## 12. Threat model: how a bad writer might game this

| Attempt | Required response |
|---|---|
| Ignore warnings | The build subgate consumes structured policy states; known missing/mismatch obligations and every indeterminate state fail regardless of warning handling |
| Add `check true` | It entails no obligation; sites remain unaccounted |
| Add `requires false` or demand maximum/absurd capacity | Current pinned-source digest forces review for the base64 pilot; general domain records and representative workload enforcement are required but not implemented |
| Write a weaker relation | Proof does not discharge; the check remains and the obligation finding fails promotion |
| Rewrite into an unrecognized shape | OP-4 checks remain; conservative candidate-frontier sites become unaccounted rather than `not-applicable` |
| Add source suppression | No source-level suppression is admitted |
| Delete/repoint the promoted root | `--promotion` requires the dual-pinned source/function/digest set and forbids filters; protected external owner review is still required for changes to both repository pins |
| Edit the approval manifest | Approvals must be empty until GATE-1 dependency-cone records exist |
| Hide the check through same-unit helpers | Whole-unit closure still sees it; imported/transitive closure remains deferred and receives no claimed credit |
| Move checks into explicit source checks | B3 requires explicit checks to remain passing; current coarse trap metrics are not a complete per-origin inventory, so independent profile enforcement is deferred |
| Encode equivalent work as ordinary conditional/`Result` control | Static check inventory does not claim completeness; codegen and end-to-end workload gates must catch the cost |
| Claim a guard is hot | Guard frequency comes from independent measurement, not writer metadata |
| Force worst-case caller allocation | Not covered by bounds-v1; future whole-workload allocation, memory, and latency gates must expose exported cost |
| Use unchecked access | No writer-emittable unchecked index exists |

A performance approval is not a trusted safety assertion: the dynamic check
continues to run. The authority boundary exists solely so a bad writer cannot
normalize unmeasured performance debt.

This mechanism cannot prove globally optimal algorithms. A writer can still
choose a poor algorithm or add redundant non-check work; codegen parity and
end-to-end workload gates remain necessary. Check accounting makes known
safety-check and domain-hardening debt non-ignorable, not all performance
pathology impossible.

## 13. Experiment plan

### 13.1 Base64 obligation/diagnostic matrix

Reuse and extend the 44-case output-capacity corpus:

- exact requirement + exact body -> direct proof;
- missing requirement + exact body -> versioning candidate;
- weaker requirement -> retained/versioned residue;
- stronger requirement -> overconstraint witness;
- wrong stride/offset/tail/buffer -> first failed premise;
- algebraically equivalent spelling -> normalization result recorded;
- mixed valid/invalid sites -> exact per-site states; and
- facts-off -> identical safety behavior with all implicit checks retained.

Acceptance properties:

- zero falsely eliminated sites;
- deterministic states/reasons/counterexamples;
- byte-identical safety semantics on fallback paths; and
- source-independent report collection.

Add a non-shipping audit mode that bypasses the entry requirement while
retaining every implicit body check. Differential search then looks for:

```text
Q(actual parameters) is false
but the fully checked body succeeds
```

Any witness proves that `Q` is stronger than necessary for that observed
implementation execution; it does not by itself invalidate an approved API
domain. Absence of a witness is evidence only, never proof. Record the
requirement true/false rate, caller allocation delta, peak memory, and whether
failure occurs before the first body effect.

### 13.2 Variable-output decoder experiment

Phase one builds a bounded decoder-shaped kernel with a deterministic output
need distribution:

```text
99%: 1..1023 bytes
1%:  1024..1,048,576 bytes
```

Compare at least:

1. 1 MiB caller allocation + worst-case `requires`;
2. exact preflight + exact allocation/requirement;
3. streaming fixed-size chunks + `NeedMoreOutput`;
4. allocating/growable decoder output;
5. source-level recoverable boundary path with compiler-versioned fast region;
6. baseline per-access checked decoding;
7. facts-off and perfect-elision/codegen-ceiling controls; and
8. equivalent mature C and safe-Rust implementations.

Variants 3–5 require aggregate-result lowering or the explicitly non-canonical
temporary status/struct ABI described above. Variant 4 additionally remains
host-language-only until grow/replace exists, unless those prerequisites are
implemented first. Variant 5 must start from a source API that already returns
`NeedMoreOutput`; compiler versioning cannot convert an OP-4 trap into a
recoverable result.

These strategies do not automatically have the same accepted domain or failure
timing. Either place them behind one behaviorally equivalent public wrapper or
report them explicitly as different APIs rather than claiming an implementation-
only comparison.

Also sweep initial streaming capacities (256 B, 1 KiB, 4 KiB, 16 KiB), and
compare resume against optimistic 1 KiB decode plus rare restart. The 1% large
requests may dominate total output bytes, so request-weighted and byte-weighted
results must both be reported.

Phase two instantiates the leading candidates in a real format and reweights a
real corpus. The synthetic kernel may not select the design: it can make
preflight unrealistically cheap and omit token parsing, malformed-input
recovery, overlapping copies, history windows, and burst distributions. Record
input size, compression ratio, token/burst shape, and decoder-history demand.
For LZ/DEFLATE families, include window state and copying rather than treating
output chunks as independent.

Measure:

- decoded throughput and latency distribution;
- p50, p99, and p99.9 request latency;
- cycles per input byte and per output byte, split by small and large classes;
- total allocated/reserved/committed bytes;
- peak RSS and maximum concurrent jobs before memory pressure;
- allocation count and allocator time;
- bytes initialized/touched and bytes copied during growth/restart;
- extra passes and bytes reread by preflight;
- code size, instruction-cache behavior, and guard true rate;
- time to first output and number of resume calls;
- retained/eliminated/versioned check counts;
- normal-case trap rate (must be zero for APIs classifying shortage as normal);
- correctness against a reference over valid, malformed, truncated, and
  adversarial inputs; and
- repair success when writers are shown missing/mismatch/overconstraint
  diagnostics.

Do not select the decoder API from the inner-loop benchmark alone. The winning
candidate must include caller allocation and memory-system cost.

### 13.3 Versioning code-size gate

For every versioned candidate, compare:

- direct checked source requirement;
- inline fast/slow clones;
- cold outlined fallback; and
- retained checks without versioning.

Record hot-path performance, total text bytes, cold text bytes, branch behavior,
and compile time. Pre-register a deterministic versioning budget before making
this a general transform.

### 13.4 Authorship test

Give model tiers tasks with:

- exact fixed expansion;
- bimodal/data-dependent expansion;
- intentionally streaming output; and
- a stale requirement/body pair.

Measure whether writers:

- choose exact versus maximum requirements;
- classify `NeedMoreOutput` as a value rather than a trap;
- repair from the first diagnostic;
- attempt to silence the gate or strengthen the contract; and
- understand the provisional `requires` spelling consistently.

### 13.5 Governance tripwire and telemetry (deferred)

The pilot has no automatic inventory of known-hot roots outside the selected
scope. Until that tripwire exists, review must manually reconcile the project's
hot-function inventory with the dual-pinned root list; this is a process check,
not a writer-proof mechanism. Retained-site approvals are forbidden, so there
are no approval-action metrics yet. When additional roots or approvals are
introduced, record policy revisions, false detentions/precision, reviewer
actions, and time to promotion. The n27–n33 frontier hardening is the first
policy-revision evidence, not proof that the frontier is complete.

## 14. Proposed implementation sequence after review

Progress note (2026-07-11): items 1–4 are implemented in the narrowed,
authorization-free bounds-v1 slice. Constructive counterexamples, domain
records, backend credit, and per-site approvals remain deferred. The next
experiment is caller-owned streaming QOI decode; versioning remains blocked by
review B4's hit-rate and code-size prerequisites.

1. Refactor PROOF-2 into a body analyzer producing obligations and failed
   premises, without changing codegen.
2. Extend the structured report with obligation and relationship states.
3. Gate diagnostics/counterexamples against the existing adversarial corpus.
4. Generalize codegen parity into a reviewable performance-accounting manifest;
   keep all retention and requirement-domain approvals external to ordinary
   source authorship.
5. Either implement multi-field/aggregate result lowering and the required
   grow/replace/history storage features, or explicitly scope the first decoder
   experiment to caller-owned streaming chunks and mark other variants host-only.
6. Run the synthetic and then real-format QOI decoder comparisons.
7. Run the authorship/repair experiment, including the worst-case-requirement
   trap.
8. Build a dependency-cone identity only when a legitimate retained-site
   authorization is needed; until then keep `approvals: []`.
9. Prototype guard versioning only after the independent hit-rate and code-size
   instruments required by B4 exist.
10. Only then decide spec deltas, syntax/spelling, and broader promotion
    defaults.

## 15. Review questions and dispositions

The review resolved the first-slice choices: owner-selected roots; B2
obligation-backed debt; GATE-1 per-site dependency-cone approval identity; no
automatic versioning credit; QOI for the decoder experiment; bounds only;
constant-time policy out of scope; explicit checks passing under B3; and no
backend credit without verified provenance. The remaining numbered questions
below are historical design prompts or deferred expansion questions, not
license to revert those decisions.

1. Should performance accounting apply to every function or only an owner-
   selected root set of promoted/hot functions? In either case the root set is
   privileged and its reachable-instance closure is mandatory; how is that
   closure represented compactly?
2. Is elaboration state `versioned` automatically accounted by default, or must
   code-size-sensitive kernels explicitly permit the fallback clone?
3. Should checked fallbacks be outlined automatically, and under what
   deterministic budget?
4. Which relationship engine is admitted: exact structural normalization only,
   Presburger-style affine proof with a small checker, or something else?
5. When may the compiler label a body-derived condition necessary and
   sufficient rather than merely sufficient?
6. Does a known overconstrained `requires` fail performance promotion, require
   approval, or merely report? This draft recommends failure/approval rather
   than silent credit.
7. Is retained-check approval an extension of GATE-1 or a separate non-safety
   owner policy gate?
8. Does GATE-1 cover adding or strengthening `requires` in every performance-
   gated function? This draft recommends yes; what is the privileged statement
   of the accepted domain while FN-8 remains absent from `fn_sig`?
9. Historical prompt, rejected by review B1: after a whole-artifact first
   slice, could a function/dependency-cone digest survive unrelated edits
   without allowing stale approval drift? The accepted direction starts with a
   conservative dependency cone; it does not ship whole-artifact approvals.
10. How should public/FFI APIs publish checked requirements before gated boundary
   frames and machine-readable trap reports exist?
11. What validated-plan representation can bind content-derived output size to
    the exact input without adding a writer-asserted fact?
12. Does `requires` communicate “checked invalid-call boundary” well enough, or
    does its conventional precondition meaning cause writers to overstate
    domains?
13. Which decoder should instantiate the variable-output experiment: base64
    decode, QOI, LZ4 block decode, or DEFLATE/inflate?
14. What minimum independently measured guard hit rate and dynamic saving let a
    `versioned` origin count as automatically accounted?
15. Must every performance profile include allocation/peak-memory budgets so
    worst-case caller allocation cannot game local codegen?
16. Which checks enter the first accounting slice: bounds only, or overflow,
    allocation, explicit checks, and transitive/FFI checks as well?
17. Is secret-dependent guard timing out of scope, or must the design reserve a
    future constant-time profile now?
18. How are LLVM/backend-eliminated emitted checks given verified proof
    provenance so DIAG-2 and final-machine accounting agree?
19. Which explicit check roles are automatically mandatory versus individually
    authorized in the first performance profile?

## 16. Implemented review disposition

Implemented as non-normative compiler/harness policy:

- obligation-driven proof discovery;
- obligation-backed hard/unaccounted promotion failure for implicit bounds;
- affirmative intrinsic-dynamic safety-floor acceptance;
- dual-pinned, unfilterable whole-unit bounds-v1 evaluation of base64; and
- the writer rule that expected shortage is a value and common-case predicates
  are guards, not `requires`.

Deferred rather than silently credited:

- guarded fast paths/versioning;
- retained-check approvals and their B1 dependency-cone identity;
- privileged general domain records;
- backend elimination provenance;
- explicit/overflow/allocation/transitive/FFI accounting; and
- exact reachable-instance closure beyond the closed compilation unit.

Do not promote:

- missing `requires` as a general semantic error;
- silent compiler insertion of an early-trapping requirement;
- worst-case capacity as a generic decoder pattern;
- source-level warning suppression; or
- any claim that a condition is minimally restrictive without a checked proof
  of that relationship.
