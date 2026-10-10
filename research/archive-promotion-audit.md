# Archive promotion audit

Status: **non-authoritative inventory**. Audit observations date from 2026-08-01;
current reading pointers follow the repository's live owners.

This document preserves the useful conclusions of the archive audit outside
`archive/`. It is not a work queue and it grants no implementation or language
authority. Open questions and evidence links live in [ideas](../docs/ideas.md),
and known compiler defects and implementation costs on the
[status board](https://claude.ai/artifact/7tocXS3iUdthCLCQCMd3ip).
The [active specification](../spec/kernel-spec.md) defines the language,
the [constitution](../docs/constitution.md) defines project objectives and
principles, and [AGENTS.md](../AGENTS.md) owns the workflow.

The archived originals remain in place as provenance. Promotion means moving a
still-valid conclusion, question, or measured result into a live owner, not
making active code, builds, tests, or tools read the archive. This file should
be updated in place and can be removed when every promotion candidate below is
either placed in its proper live authority or explicitly retired.

## Scope and result

The audit inventoried the complete archive tree in its 2026-08-01 worktree
(4,294 files, 336 MB), reviewed its directory manifests and result indexes,
traced the D0-D27 directive history, sampled the high-value design and
measurement records, and checked their live successors. Of that filesystem
tree, 1,017 files and about 200 MB are tracked; most of the remaining size is
ignored build/cache output, including a 132 MB retired Rust `target/` tree.
Generated evidence, raw debate transcripts, historical binaries, and retired
implementation sources were assessed by cohort rather than reread line by
line. A static scan found no Make, Cargo, compiler, or conformance-runner path
that reads the archive; current references are documentary or historical.

The result is deliberately small:

1. No archive subtree should be moved wholesale into the live repository.
2. The audit recorded D17's representation-invariant proof lane separately
   from D16's sealed-catalog mechanism. The retained current question is in
   [storage transitions and representation privileges](../docs/ideas.md#storage-transitions-and-representation-privileges);
   that entry selects no particular mechanism.
3. The M1 loan/freeze candidate remains parked. The
   [parallel](../docs/ideas.md#wider-parallel-proof-domains) and storage
   questions retain dynamic fan-out and the operation-level storage checklist.
4. Five historical measurements remain useful enough to register here, with
   their limitations. Their harnesses and raw evidence stay archived.
5. Retired compilers, gates, reference models, schemas, old work queues, and
   superseded candidate frameworks stay archived. None should be restored as a
   live dependency.

## Disposition vocabulary

| Disposition | Meaning |
|---|---|
| **Absorbed** | A current live authority or successor already carries the conclusion. Keep only provenance in the archive. |
| **Live evidence** | The durable finding is summarized here; the exact protocol and raw evidence remain archived. It is not a current compiler-capability claim. |
| **Owner decision** | The idea may still be durable, but promotion would change project law, writer doctrine, or future language direction. This audit does not make that decision. |
| **Archive only** | The material is superseded, rejected, raw, or implementation-specific. Consult it historically; do not revive it. |

## Authority crosswalk

The dispositions below record the audit's comparison, including its then-current
language baseline. Consult the linked live owners for current rules; this
crosswalk does not make the old language or approval vocabulary current.

| Archived material | Audited disposition | Successor or reason |
|---|---|---|
| D0-D3 and D2a founding constraints | **Absorbed** | The priority structure, AI-writer premise, safety floor, compact/regular specification, and no writer trust are in the [Constitution](../docs/constitution.md) and active specification. |
| D4 rewrite-first, FFI-narrow policy | **Owner decision** | The active specification has a gated boundary skeleton, but no live authority currently commits to the old rule that source-available foreign code must normally be rewritten or that rich/inbound interop stays out. |
| D6 closed, taught pattern doctrine | **Seeded; not ratified** | [`docs/patterns.md`](../docs/patterns.md) is the live writer-form owner and states the current status. This audit corrects explanatory overclaims; promotion to normative doctrine would require owner ratification. |
| D7-family artifact and `secret` directions | **Absorbed as deferred** | [Reproducibility](../docs/ideas.md#reproducible-compiler-outputs) and [constant-time](../docs/ideas.md#constant-time-preservation) questions remain open without reviving the old artifact ladder or `secret` protocol. Their old targets and protocols remain historical. |
| D9a fixed-model/model-score gate | **Archive only** | The Constitution now says model runs generate realistic mistakes but model scores never gate W1. Measured results may survive independently of that protocol. |
| D10 English-only repository content | **Absorbed** | The live repository instructions carry the rule. |
| D11-D15 capability-floor and B-Strata programs | **Archive only**, with selected questions retained below | D15 explicitly restarted the derivation; the successor systems-performance pass superseded the earlier candidate machinery without establishing a production winner. |
| D16 sealed-catalog mechanism and acceptance ledger | **Deferred** | The [storage questions](../docs/ideas.md#storage-transitions-and-representation-privileges) retain the underlying problem. The old ten-member catalog and its exact admission machinery are not current design authority. |
| D17 proof-gated privileged admission | **Absorbed as deferred** | The audit recorded a long-term representation-invariant proof direction. The current [storage questions](../docs/ideas.md#storage-transitions-and-representation-privileges) leave the proof mechanism, concrete privileges, and schedule unselected. See below. |
| D18-D19 pool and concurrency choices | **Mixed** | Process-abort trap semantics are absorbed. Generational pools, CONC-0, fixed spawn plus `par.for_chunks`, endpoint clone modes, and the old runtime shape remain research choices that any selected parallel design must reconsider rather than inherit. |
| D20-D27 execution plans, compiler architecture, and v0.9-v0.11 transitions | **Absorbed or archive only** | [AGENTS.md](../AGENTS.md), the [active specification](../spec/kernel-spec.md), and the safe-Rust compiler replace their operational roles. Same-kernel replay, product resource profiles, the retired toolchains, and old proposal machinery must not return. |

The original rulings remain available in
[`archive/governance/directives.md`](../archive/governance/directives.md), and
their chronology in the
[`archived decision history`](../archive/governance/decision-log.md). Those
files explain provenance; they do not settle any **Owner decision** row or
override current language and design owners.

## Useful results already promoted

Several of the archive's most important performance conclusions already have
live experiment owners. Copying their archived decision-log entries would only
create competing status. The remaining distinction is between the historical
measurement and the current compiler milestone.

| Channel | Live evidence | Current boundary |
|---|---|---|
| Effect rows to LLVM attributes | [`effect-attrs-channel/RESULTS.md`](experiments/effect-attrs-channel/RESULTS.md) | The measurement used democ and showed the value of exact interprocedural effects. A current [optimizer consumer](../docs/ideas.md#proof-derived-optimizer-facts) needs a sound mapping under the active ABI; `willreturn` also needs separate totality evidence. |
| Borrow-derived alias metadata | [`scoped-alias-channel/RESULTS.md`](experiments/scoped-alias-channel/RESULTS.md) | Short trips improved and long trips approached Rust parity, with a large code-size difference. Current backend use needs its own mapping and evidence; it is not an inherited capability. |
| Checked-law reassociation | [`checked-law-channel/RESULTS.md`](experiments/checked-law-channel/RESULTS.md) | The historical transform measured 3.3x over the serial fold and rejected a false signed-saturating law. Checking a source law does not establish an arbitrary optimizer rewrite; its operation domain, effects, and intermediate arithmetic still matter. |
| Entry contracts and bounds proof | [`port-study/base64/RESULTS.md`](experiments/port-study/base64/RESULTS.md) | The historical proof build discharged its recorded sites with facts-off safety retained. Its retired acceptance rules and measured code shape are not current compiler results. |

## Promotion results and retained candidates

### 1. D17 placement completed

The historical D17 ruling selected a specific, long-term
representation-invariant proof lane:

- a project kernel whose implementation is machine-proved leaves the trusted
  list and retains the privileged representation rights its proof justifies;
- the same lane is open to users, including rights such as partial
  initialization and elimination of proved internal checks;
- proof checking is deterministic and does not depend on search, the invariant
  language is versioned, and every extension receives hostile soundness review;
  and
- proof is not a performance prerequisite for ordinary programs, which may use
  the project-provided default path.

This is narrower and more concrete than the possible future generalization
“proof may grant any narrowly scoped capability.” The audit does not make that
generalization. D16/D17 also described a ten-kernel sealed catalog and a
particular admission endgame. Those mechanisms were not part of the audit's
v0.17 language baseline. In particular,
[STOR-1](../spec/kernel-spec-v0.17.md) defined no temporary uninitialized hole.
That historical gap is not a claim about the active language.

The audit's v0.17 comparison was:

- The Constitution already says checks are removed only by proof and ordinary
  writers cannot emit trust.
- OP-4 recognizes a deterministic derivation or separately verified proof as
  check-elision authority. FN-8 is a runtime callee-entry prologue whose passed
  check may contribute a dominated fact; it is not a caller proof obligation.
- FN-4's law discharge is source-acceptance evidence only, not optimizer
  authority. DIAG-2 carries the checked program that may authorize lowering.
  GATE-1 and LEDGER-1 govern human-approved trusted boundaries; they are not
  proof-admission channels.
- None of those rules defines a general representation-invariant proof system.
- [`docs/why-whitefoot.md`](../docs/why-whitefoot.md) explains the full D17
  direction while also admitting that its feasibility is unproved.
- The then-current direction map retained unresolved storage dimensions.

**Placement recorded 2026-08-01:** the audit placed the representation-invariant
proof direction in the then-current Constitution and kept concrete mechanisms
deferred. Current [storage research questions](../docs/ideas.md#storage-transitions-and-representation-privileges)
require a concrete representation problem and independent grounds for any
additional privilege; they select neither the old catalog nor a general
privileged proof system. Concurrent lock-free proofs were explicitly the
harder historical case. The full provenance is D17 in the
[`original directive record`](../archive/governance/directives.md) and the
[`systems-performance design dossier`](../archive/research/systems-performance-coverage/DESIGN-DOSSIER.md).

### 2. Keep the M1 loan/freeze work as a parked candidate, not a rule set

The archived systems-performance pass produced a reviewed loan/freeze state
machine and a small reference checker. Its evidence reports a 97-program corpus,
a mutation harness that caught 9/9 planted checker mutants, and a single-pass
checker without a fixed point. That is stronger than an abandoned sketch, but
it predates the audit's v0.17 ownership baseline and includes old concurrency
assumptions.

Its durable value is a reopening boundary: if persistent captured loans,
interior views, or a selected parallel construct cannot be expressed by the
current borrow rules, compare the concrete blocker against the parked state
machine.
Do not import its rules or checker. Re-derive the smallest current rule against
the active specification and the real program first. Sources:
[`RULES-RATIFIED.md`](../archive/research/systems-performance-coverage/m1-loan-judgment/RULES-RATIFIED.md)
and
[`M1-PAPER-RESULT.md`](../archive/research/systems-performance-coverage/m1-loan-judgment/M1-PAPER-RESULT.md).

### 3. Dynamic fan-out retained as a parallel-design witness

The historical OWN-11 rule forbade a borrow inside a loop from naming an outer
region. The old concurrency design therefore could not express a runtime-count spawn loop
whose workers all read shared outer state; its answer was fixed straight-line
spawn plus a sealed `par.for_chunks`, with a later carve-out explicitly marked
“must not be lost.”

The [wider parallel proof questions](../docs/ideas.md#wider-parallel-proof-domains)
retain dynamic fan-out as a witness to evaluate against current rules, including
ownership and resource accounting for tasks, results, and cancellation. The
old spawn form and OWN-11 carve-out are not inherited solutions.
Source:
[`systems-performance follow-ups`](../archive/research/systems-performance-coverage-FOLLOW-UPS-2026-07-17.md).

### 4. Storage checklist retained for representation research

The capability-floor work correctly exposed that “add a vector” or “add a hash
map” is too coarse a storage decision. A real proposal must separately account
for at least:

- growable dense storage and replacement of an affine backing allocation;
- move-out, failure, cleanup, and destruction ordering;
- partial initialization and sparse occupancy;
- stable append-only identity versus recyclable identity;
- invalidation, stale-handle detection, and check-elision authority; and
- multi-place access, iteration, and relocation under active loans.

This preserves problem dimensions, not the old report's v0.6 gap verdicts. The
[storage questions](../docs/ideas.md#storage-transitions-and-representation-privileges)
retain these dimensions for a concrete program. Every item must be re-audited
against the active specification and current compiler before proposing a
missing transition or additional privilege. The old G0-Core, Family Lock,
B-Strata, and candidate validators must not be restored; they selected no
current winner and are much larger than the next concrete compiler experiment.
Source:
[`general-purpose data-structure capability research`](../archive/research/capability-floor/general-purpose-data-structure-capability-RESEARCH.md).

## Historical evidence retained live

These are useful results, not current Whitefoot compiler claims.

| Evidence | Durable finding | Limitation and disposition |
|---|---|---|
| [M3 sequence/table dry run](../archive/research/systems-performance-coverage/m3a-kernel-dryrun/RESULTS.md) | The proposed dense-sequence and table shapes reached the preregistered band on four of five measured workloads; table layout and fused group-load details were load-bearing. | C mockups on one Apple M4, not Whitefoot output. Keep the bundle archived; use the findings only when a current port selects the same shape. |
| [M4 arena dry run](../archive/research/systems-performance-coverage/m4-arena-dryrun/RESULTS.md) | In a dependent pointer chase, a retained bounds compare was effectively free; index addressing itself was about 1.6x behind raw pointers in L1 and converged near parity at memory latency. | The literal preregistered checked-versus-raw-pointer L1 band failed at about 1.59x; the check-overhead interpretation passed because checked and check-free index forms were equal. This mixed result does not establish a branded-ID design. |
| [M6 SPSC dry run](../archive/research/systems-performance-coverage/m6a-spsc-dryrun/RESULTS.md) | A bounded operational model caught violations when any of four acquire/release halves was weakened; the C steady path had zero RMW operations. | The model used capacity 2, exactly four values, and a custom acquire/release-plus-reordering semantics; it is not a C11, LLVM, or ARM memory-model proof. The C shape beat `rtrb` on round-trip latency but lost on batched throughput, and no current Whitefoot runtime implements it. |
| [M8 memchr dry run](../archive/research/systems-performance-coverage/m8-memchr-dryrun/RESULTS.md) | A discharged loop-window bound produced an inner loop identical to the unchecked version and bulk parity with `memchr`; an opaque retained check cost roughly 1.7-1.9x. | C modeled the post-proof shape. This supports proof-based check removal, not a claim that the current compiler proves or emits that SIMD loop. |
| [Guarded scatter experiment](../archive/experiments/ai-native-parallelism/RESULTS.md) | The measured O(n) per-call guards mostly consumed a memory-bound parallelism win: guarded speedup was 1.02x, 1.17x, and 1.00x at the three work levels. | Viability needs static/cached/amortized proof, a materially cheaper trusted guard, or a larger residual workload. The guard and seed bundle were themselves unproved and the old schema is retired. |

The archived regions-and-effects study already has a concise live successor,
[`research/notes/regions-effects-vs-safe-rust-2026-07-08.md`](notes/regions-effects-vs-safe-rust-2026-07-08.md),
so its raw archive needs no second promotion. The older phase-2 findings,
debates, matrices, and source snapshots remain citation provenance for
`docs/why-whitefoot.md`; they are not a parallel live
research program.

## Material that stays archived

| Cohort | Why it stays archived |
|---|---|
| Root `ROADMAP.md`, `DECISION_SPRINT.md`, handovers, and compiler plans | Superseded work queues are especially dangerous to promote because they appear to authorize old sequencing. |
| `compiler/` and `toolchains/self-hosting-2026-07-20/` | Retired implementations. General defects may be re-derived as current regressions, but code must not be transplanted. |
| `retired-gate/`, `superseded-lexical-v08/`, and `premature-capability-audit/` | Independent grammar engines, catalogs, policy machinery, and overlay schemas were deliberately removed from the normal compiler path. |
| `tests/reference/`, archived conformance material, and the old codegen runner | Historical evidence or runner-specific expectations. Selected source cases can return only through a small Rust-owned regression reconciled with the active spec. |
| `m3/` model-tier harness | Historical prompts may be mined as mistake generators, but the runner is not replayable from HEAD and its model-score decision protocol conflicts with the current W1 floor definition. |
| `research/minimal-systems-capability/` | A 178 MB superseded candidate framework with no production winner. The four operation-level questions above are the useful residue. |
| Raw debates, web/source captures, matrices, agent outputs, and product-grade artifact/replay designs | Citation and reproducibility evidence, not maintainable live doctrine or current research-compiler work. |

## Navigation and hygiene findings

The existing [`archive/README.md`](../archive/README.md) is a historical map,
not a reliable current index. It predates the large 2026-07-22 migrations,
omits several major cohorts, and names removed successors such as
`THE-PLAN.md`, `/experiments/`, and `tools/verify_project_state.py`. The
archived decision index also links to the removed live path
`governance/directives.md`. These are not repaired here because the promotion
result belongs outside the archive and the historical files should not regain a
maintenance role.

The audit also repaired three live navigation defects rather than sending their
fixes back into the archive: the repository README no longer duplicates the
obsolete v0.12 status, and Pattern P2 now points to the current executable
v0.17 pool example instead of a nonexistent archived `.wf` file. The live
experiment index now distinguishes current self-contained bundles from
historical result bundles whose runners still name the retired democ toolchain.

The live entry points are [`research/README.md`](README.md),
[ideas](../docs/ideas.md), the [status board](https://claude.ai/artifact/7tocXS3iUdthCLCQCMd3ip), the
[active specification](../spec/kernel-spec.md), the
[constitution](../docs/constitution.md), and [AGENTS.md](../AGENTS.md).
If a future audit finds another valuable archived conclusion, add the smallest
status-bearing summary to its actual live owner and update this file; do not
grow a second archive index or restore an old subtree.
