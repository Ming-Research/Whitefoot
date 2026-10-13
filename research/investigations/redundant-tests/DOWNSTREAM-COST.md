# Attributing OP-5 cost on Halo and Snowghost

## Question and scope

This investigation extends the [exact-recovery investigation](EXACT-RECOVERY.md)
for [PR #393](https://github.com/Ming-Research/Whitefoot/pull/393). Does the
remaining cost come from deciding each condition, explaining decided conditions,
rechecking rejected modules, or disproportionate work on particular functions?
The owner accepted approximately 25% additional Firn checking time and selected
incremental views along an `if` chain if OP-5 makes any program's check more than
50% slower. This investigation attributes that trigger; it changes neither the
threshold nor the specification, compiler, or downstream sources.

The pre-run implementation inspection below refers to compiler revision
`30169c91105990913b11f7c07dc4e49f63de6997`. The workflow records its actual
compiler revision on execution. Its pinned comparison remains:

| Input | Revision |
| --- | --- |
| Main compiler | `58743a931a5ec6244864d37041fd1a7c1c5a145d` |
| Halo sources | `56878ac4d607f7496c8e23a4c2ffe2d45e9cb552` |
| Snowghost sources | `466e9843f7a0fe4926022be8718d4ab677cafed1` |

## Existing observation, not the new experiment

[Run 38350935269](https://github.com/Ming-Research/Whitefoot/actions/runs/38350935269)
used the gate-profile compilers on one Ubuntu hosted runner, with three
interleaved uninstrumented runs over identical pinned sources. The supplied
artifact gives:

| Workload | Main seconds | Branch seconds | What completed |
| --- | --- | --- | --- |
| Halo library graph | 9.30 / 9.25 / 9.17 | 16.43 / 16.29 / 16.28 | 15 module verdicts, no entries |
| Snowghost renderer graph | 469.37 / 520.54 / 491.78 | Each stopped at the 3000 s deadline | Main: 52 modules and 26 entries; branch: no published verdicts |

Halo's median ratio is 16.29 / 9.25 = 1.761, or 76.1% slower than main.
Main accepts every module; the branch rejects eleven, with 32 failing-body
tasks. Nine rejection inventories are complete; `pkg::compile` and `pkg::vm`
are incomplete. The heap module alone has fourteen tasks. The VM has two,
`builtin` and `prepare_inner`. These are not eleven single diagnostic renders.

Snowghost's three branch compiler time files are empty and its wrapper status
is 124. The wrapper records approximately 3002 s including deadline handling.
The 3000 s deadline is already 6.10 times the main median of 491.78 s; this is
a censored lower bound, not a completed branch duration or a measured OP-5
phase. No diagnostic or module progress was published before the stops.
The artifact's existing summary rejects those empty timing files rather than
reporting a valid completed comparison. Its work-counter files belong to Firn,
not Halo or Snowghost, and cannot attribute either downstream regression.

The same artifact's migrated Firn check succeeds on both compilers: main on
branch sources takes 6.52 s and branch on those sources 8.12 s. Its branch
instrumented run records 4,380 condition judgments and zero redundant
conditions. That is an existing accepted-source control, not an equivalent
migration of either downstream project.

## What the implementation actually does

1. **Decision and view construction.**
   [The condition judgment](../../../compiler/src/semantic/entailment/flow/conditions.rs)
   starts its `condition` timer at line 23 and creates a new view memo at line
   41. [The prover](../../../compiler/src/semantic/entailment/flow/prover.rs),
   lines 517–532, can share a view between the two truths of that condition.
   It does not retain that completed view for the next condition in a chain.
   [Origin transport](../../../compiler/src/semantic/entailment/flow/origin_transport.rs),
   lines 374–413, collects and sorts entering origins and signed facts; lines
   426–468 rebuild the expression classes. Its fixed point at lines 573–647
   scans Boolean members and can invoke ordinary and affine proof searches.
   Existing caches reuse exact collections and expansion work, but a growing
   entering context still leaves repeated preparation and propagation. A bound
   on the number of queries per condition does not bound the work per query.

2. **Explanation and fix-it.** The same condition timer includes
   `condition_facts` at lines 48–54. That routine, lines 113–192, traverses
   deciding derivation ancestors, clones nodes, renders premises and removes
   duplicate strings. Its visited set is fresh for each proof root. This is
   distinct from [final fix-it construction](../../../compiler/src/semantic/check/acceptance.rs),
   lines 160–177, which joins the already rendered facts with fixed wording.
   There is no separate explanation timer. Rendering the condition residual
   also occurs on queries that remain undecided.

3. **Recovery can repeat module analysis.**
   [The module driver](../../../compiler/src/driver.rs), lines 978–980, sends
   a source rejection into `module_rejection`. Its loop at lines 1025–1082
   sets aside the next failing function body and checks the remaining selected
   module records again. A function surviving fourteen such removals may be
   analyzed fifteen times, before any candidate or synthetic analyses. The
   body walk does not stop immediately at an OP-5 decision: it continues through
   the arms in [the flow walk](../../../compiler/src/semantic/entailment/flow/walk.rs),
   lines 1480–1486. Thus repeated recovery can multiply ordinary OP-5 work too.

4. **The old report hides where it stopped.**
   [The CLI](../../../compiler/src/bin/whitefootc.rs), lines 424–457, collects
   all module and entry verdicts before publishing them. Lines 460–473 show
   that selecting one module still uses the same rejection-recovery path.
   Omitting `--report` does not suppress explanation or recovery.

The leading hypotheses are therefore view work and recovery amplification,
possibly together. A superlinear chain cost and a costly explanation traversal
remain hypotheses. Final fix-it string assembly alone has weaker code grounds.
The observed timeouts do not identify a Snowghost module or an asymptotic law.

## Hypotheses and observations that would reject them

These discriminators are recorded before the new run. A timeout or a missing
row cannot reject any hypothesis about the unfinished analysis.

| Hypothesis | Expected distinguishing evidence | Result that rejects it as the dominant explanation |
| --- | --- | --- |
| Preparing and propagating an origin view at each condition dominates | Completed slow analyses spend substantial time in `origin_view_build` or `origin_view_fixed_point`, with large expression/member/pass, closure, or affine work even when `redundant_conditions` is zero. | Slow completed analyses have little origin-view time and work; the excess instead tracks repeated analysis counts or decided-proof work. Absence of a view in an interrupted analysis is not this result. |
| Explaining deciding proofs dominates | Excess concentrates in analyses with decided conditions; `condition` is large while the individually reported view/proof phases do not account for it. This supports suspicion but does not isolate explanation. | The expensive completed invocations have zero `redundant_conditions`, so `condition_facts` and their OP-5 fix-its were never constructed there. A verified accepted equivalent retaining the excess would also reject this explanation for that excess. |
| Rechecking after rejection dominates | Branch functions recur under additional analysis-run IDs, consistent with the module's failed-body tasks; work per corresponding invocation is modest compared with multiplicity. | A slow accepted module has no recovery, or matching main/branch analysis multiplicities retain the excess. Synthetic and candidate analyses must be distinguished before concluding multiplicities match. |
| One or several long chains cause pathological per-analysis work | A small set of modules dominates or times out independently; completed functions have unusually large view inventories, fixed-point/affine work per condition, rather than merely more analysis runs. Inspect those source chains next. | The observed excess is accounted for by recovery multiplicity, without disproportionate work per completed invocation. A hot module alone does not prove chain growth; aggregate counters cannot establish a complexity exponent. |
| Repeated entry composition causes the old Snowghost timeout | All isolated modules finish, while the old combined modules-plus-entries invocation does not. | Individual modules reproduce the severe cost. This rules out entry composition as the sole cause, not an additional contribution. |

## Attribution result: 2026-10-13

[Run 38378768581](https://github.com/Ming-Research/Whitefoot/actions/runs/38378768581)
completed the protocol below at branch compiler
`c9982a5893ef029c5cbb0a8971efb5675a441ad3`, against the main compiler and
downstream pins above. It used gate-profile binaries on Ubuntu hosted runners,
Rust 1.99.0, with one uninstrumented main/branch/main-twin triplet per job and
separate instrumented invocations. The artifact's `op5-summary/measurements.json`,
`per-analysis.jsonl`, `per-function.json`, raw TSVs and report stdout are the
evidence. The local artifact copy is scratch and is not committed. These are
screening measurements, not precise timing confirmations on the 14900K.
The source references in this result section refer to the run's branch head;
downstream source locations refer to the pinned source bundle.

The pinned main compiler is not an OP-5-only ablation: the branch also merged
other main-line changes, including the range rules that reserve `unchanged`.
The wall ratios measure the two compiler versions on the same project sources,
not the isolated causal percentage of OP-5. The OP-5 phase timers and the
rejection-triggered repetition identify particular mechanisms within that
comparison. A precise rule-only percentage would require compiler versions
that share the other changes. This matters especially for the parser aborts
below; they are not OP-5 rejection costs.

| Workload | Uninstrumented main / branch / twin, s | Instrumented main / branch, s | Completed analyses main → branch | Branch result |
| --- | --- | --- | --- | --- |
| Migrated Firn | 5.227 / 6.626 / 5.225 | 5.225 / 6.628 | 3332 → 3339 | Accepted |
| Snowghost text IDNA | 0.568 / 3.420 / 0.517 | 0.517 / 3.420 | 101 → 808 | Rejected; 7 OP-5 tasks |
| Snowghost text normalization | 0.431 / 1.019 / 0.218 | 0.218 / 1.025 | 67 → 402 | Rejected; 5 OP-5 tasks |
| Snowghost text line break | 0.237 / 0.419 / 0.218 | 0.218 / 0.418 | 93 → 186 | Rejected; 1 OP-5 task |
| Snowghost font | 15.543 / 33.660 / 15.340 | 15.540 / 33.564 | 448 → 896 | Rejected; 1 OP-5 task |
| Snowghost layout text | 1.872 / 5.019 / 1.815 | 1.817 / 5.041 | 155 → 465 | Rejected; 2 OP-5 tasks |
| Halo VM | 7.553 / 8.231 / 7.629 | 7.730 / 8.331 | 627 → 398 | Incomplete rejection inventory; 2 OP-5 tasks |
| Snowghost style | 53.019 / 0.719 / 52.491 | 53.493 / 0.719 | 778 → unavailable | Incomplete; 4 FORM-3 parser tasks |
| Snowghost layout | 8.432 / 1.119 / 8.130 | 8.431 / 1.119 | 964 → unavailable | Incomplete; 5 FORM-3 parser tasks |
| Snowghost oracle style | 6.662 / 602.060 / 6.121 | 6.121 / 602.112 | 290 → 288 | Timeout; ultimate verdict unknown |
| Snowghost oracle layout | 65.125 / 602.088 / 62.793 | 63.296 / 602.119 | 322 → 320 | Timeout; ultimate verdict unknown |

### Status of the pre-run hypotheses

| Pre-run hypothesis | Status and discriminating observation |
| --- | --- |
| Preparing and propagating an origin view at each condition dominates | **Supported for the completed hot oracle-style functions.** The cost is predominantly affine search inside view propagation, rather than allocation. The phase of the unfinished frontier analyses remains **unresolved**. |
| Explaining deciding proofs dominates | **Rejected as the dominant explanation for the completed hot functions and complete recovery examples.** The hot style functions have no decided conditions; the explanation envelopes below are small in the rejected examples. Explanation within the unfinished analyses remains **unresolved**. |
| Rechecking after rejection dominates | **Supported** for IDNA, normalization, line break, font and layout text: whole-module analysis inventories recur once per removal. **Rejected for the two oracle timeouts before their deadlines:** they stall in the first inventory, before report recovery starts. |
| One or several long chains cause pathological per-analysis work | A pathological individual analysis is **supported** at each oracle frontier. The specific long-chain or superlinear explanation is **unverified**, neither supported nor rejected by a missing row or a single source size. |
| Repeated entry composition causes the old Snowghost timeout | **Rejected as the sole cause.** Two isolated modules reproduce the severe slowdown with no entry composition. An additional aggregate contribution remains unmeasured. |

Nontermination is **unverified**. The origin-view loop installs signed facts
monotonically over a finite collected set (`origin_transport.rs:571–647`), which
favors expensive finite progress over a no-progress loop in that region. This
is not a global termination proof for every nested prover path, and the artifact
contains no live progress or stack evidence for the interrupted calls.

### The two missing frontier analyses

Oracle style emits every analysis ID from 0 through 287 once. Main emits 290
analyses; the branch adds the synthetic `error_copy_class_description` analysis.
Its last completed body is `structure_differing`; the corresponding main
frontier is `run_structure`, followed by `drive` and `run`. This identifies the
likely active branch invocation as `run_structure`, ID 288, in pinned
`oracle/style/structure.wf:461`. Oracle layout emits 320 analyses with IDs
0 through 320 except 319. Main emits 322; the additional synthetic invocation
shifts the corresponding body frontier to `run_edits`, likely ID 319, in pinned
`oracle/layout/edit.wf:1467`. `rebuild_dump` completes as ID 320 while that peer
is missing. The dependency schedule and concurrent analysis path
(`semantic/check/schedule.rs:171–298` and `semantic/entailment.rs:1558–1586`)
explain a completed peer after a missing ID. These are source/order inferences,
not function-start records. Neither oracle module starts a second recovery
inventory before its deadline.

The phase of both missing calls is **unknown**. The work instrument writes the
entire invocation only in `FunctionScope::drop` (`work.rs:292–305`), and a phase
timer records its time only when dropped (`work.rs:200–205`). Killing an active
scope loses its counters and phases. Summing completed inclusive analysis
times and subtracting them from wall time would not recover the missing phase:
analyses can nest and run concurrently, and other driver work is untimed.

Completed oracle-style bodies nonetheless separate direct decision cost from
explanation:

| Function | Conditions / redundant | Analysis s | Condition s | View build s | View fixed point s | Affine search s | Final-family exhaustions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `run_restyle` | 14 / 0 | 2.465 | 2.421 | 0.006082 | 2.246 | 2.406 | 6322 |
| `run_reach` | 19 / 0 | 1.304 | 1.266 | 0.003567 | 0.620 | 1.253 | 4185 |
| `run_incremental` | 11 / 0 | 1.423 | 1.402 | 0.003541 | 1.062 | 1.394 | 2808 |

These phases are nested, not additive. All three functions test undecided
conditions, so they do not construct OP-5 deciding-fact explanations. Their
combined view-build time is only 13.189 ms, compared with a 5.089 s condition
envelope and an overlapping 5.053 s affine-search envelope. Thus reducing
allocation alone misses the observed hot work. In oracle layout the largest
completed body, `build_page`, spends 34.524 s in analysis and 32.327 s in
closure; its condition time is about 10 microseconds. `full_structure_edit`
spends 4.409 s in analysis, 3.086 s in closure and 0.0155 s in conditions.
Neither completed layout body identifies the timeout phase of `run_edits`.

### Recovery and misleading early finishes

The complete rejected examples contain exactly `(k + 1) × N` analyses, where
`k` is the number of rejection tasks and `N` the main inventory size. Each
removal reruns the remaining bodies and synthetic declarations. Font repeats
`load_face` at 12.683 and 12.724 s, and `compile_layout` at 2.054 and 2.074 s.
Layout text repeats `font_set_add` at 1.072, 1.059 and 1.071 s, and
`release_face` at 0.384, 0.383 and 0.373 s. Their whole-analysis totals are
inclusive, but the repeated expensive bodies make the avoidable work concrete.
IDNA's redundant judgments shrink across its eight inventories as
9/8/6/5/4/3/2/0; normalization's six inventories give 8/7/6/4/2/0. Later failures
have already been encountered during earlier body analyses, before the driver
reports them one at a time.

The fast style and layout results are parser aborts. Four style bodies and
five layout bodies use `let unchanged = ...`; the branch treats `unchanged` as
a reserved token and reports FORM-3 `UnexpectedToken`. No semantic work rows
are emitted. Their apparent speedup hides semantic checking and says nothing
about OP-5's cost. Halo VM completes two inventories of 199 analyses each,
versus main's 627 analyses including later stages. Its tasks name `builtin`
and `prepare_inner`; the inventory is explicitly incomplete. The exact abort
that makes recovery incomplete is not present in the emitted report. Static
inspection suggests a recovery representation failure: the private
`prepare_inner` header at `vm/calls.wf:321` returns `Step`, declared privately
in `vm/state.wf:84` and absent from the interface. `pending_declaration`
copies that header into the interface (`driver.rs:1217–1236`), and a failure
in a synthesized declaration ends recovery as incomplete (`driver.rs:1077–1078`).
This explains a plausible hidden abort, not a recorded diagnostic.
Halo's 8.231 s cannot be compared as a completed accepted check.

### General fixes to investigate, not implementation decisions

**Retain analysis during rejection reporting.** Retain the resolved module and
the per-function results, collect body failures already available, then
invalidate only the dependency closure affected by setting a body aside.
Invalidation must include contracts, instantiated generics, verified
postconditions, ownership checks and other semantic dependencies; unchanged
written boundaries alone are insufficient evidence for reuse. This also
avoids repeatedly reparsing edited text and injecting private headers into an
interface. Preserve the specified verdict, task inventory, written locations
and completeness semantics. The following are opportunities, not measured
post-fix durations:

| Workload | Robust work opportunity | Conditional wall estimate |
| --- | --- | --- |
| Font | Remove one of two whole inventories | About 16–17 s recovered from 33.660 s, toward 17 s |
| Layout text | Remove two of three whole inventories | About 3.3 s recovered from 5.019 s, toward 1.7–1.9 s |
| IDNA | 87.5% fewer inventory blocks | Optimistic equal-pass arithmetic: 3.420 / 8 ≈ 0.43 s |
| Normalization | 83.3% fewer inventory blocks | Optimistic equal-pass arithmetic: 1.019 / 6 ≈ 0.17 s |
| Line break | 50% fewer inventory blocks | Optimistic equal-pass arithmetic: 0.419 / 2 ≈ 0.209 s |

For retained retry fraction `f`, the simple model is
`T_new ≈ T / (k + 1) × (1 + k × f)`. Here `f` includes affected dependents and
unavoidable retry work; it is unknown. No value of `f` is measured or assumed
as a prediction. Especially for short commands, wrapper overhead and the
large main/twin variation make division by block count unreliable as a wall
forecast. The counts establish the avoidable repetition; CI must measure the
actual saving. Recovery changes offer no measured saving before either
oracle timeout, because neither has started recovery.

**Construct explanations once per reported rejection.** Share visited proof
roots and render only the failures actually reported, retaining reachable
proof provenance across ledger pruning rather than retaining bare arena IDs
for later lookup. This is a general reuse opportunity, not the dominant fix
indicated by this run. Even assigning every condition timer in every completed
analysis with a redundant condition to explanation gives these upper envelopes:

| Completed workload analyses | Condition envelope containing possible explanation, ms |
| --- | --- |
| IDNA | 78.232 |
| Normalization | 109.307 |
| Line break | 4.239 |
| Font | 1.915 |
| Layout text | 2.279 |
| Oracle style | 8.423 |
| Oracle layout | 34.999 |

The envelopes include decision work and other conditions, so actual explanation
cost is smaller. They are not independent explanation timers, and say nothing
about interrupted analyses. This fix cannot be credited with recovering the
600 s timeouts from the available evidence.

**Incremental views must reuse propagation and proof work.** The owner's
selected chain-view remedy should retain exact signed-proof and prepared
affine work as well as expression collections, with correct invalidation
when the entering facts or live definitions change. The completed hot style
functions offer up to their 5.089 s condition envelope for investigation,
not a measured saving or a forecast for the whole module. Whether this remedy
addresses `run_structure` or `run_edits` remains unknown until their phase is
observed. No code or design choice is implemented by this note.

The next investigation should use CI-only function-start records and flushed
phase/site/progress observations, or sampled stacks, at the two named
frontiers. It should distinguish view fixed-point propagation, affine-family
exhaustion, explanation and other body work, and record a progress witness
for any suspected loop. A finite-progress requirement remains semantic; no
timeout or work budget may choose source acceptance. No instrumentation or
compiler fix is added in this round.

Migrated Firn is 26.8% slower than the same migrated sources on main, with twin
ratio 1.000. This supports the accepted-source allowance on one workload.
The claim that every increase above 50% comes from rejected sources is not
established: the oracle verdicts remain unknown before recovery starts, and
their completed undecided functions already show direct OP-5 decision cost.
The owner's threshold still covers any program, including diagnostic and
recovery work; this result neither creates an accepted-only exemption nor
establishes a 50% ceiling for accepted programs.

## Available control and instrumentation limits

No verified migrated equivalent of either pinned downstream tree was found in
the repository investigation or locally available project refs. Snowghost's
local main equals its pin; newer local Halo work does not establish a migration
of the pinned tree. Remote refs could not be queried in this session, so this
is a bounded search, not a claim that no migration exists anywhere. Moving to
an unrelated newer source would change the workload and is not a control.

There is also no existing explanation-disable environment/debug switch in the
inspected compiler. `WHITEFOOT_REDUNDANT_CONDITIONS` only writes an inventory;
it leaves the judgment and diagnostics intact. Neither diagnostic format nor
`--report` disables explanation. Consequently the requested migrated-source
or disabled-explanation downstream arm is unavailable within the allowed
changes. Do not manufacture a production switch, mutate the pinned programs,
or present an uninstrumented run as disabling explanations. The retained
migrated Firn control and accepted downstream analyses give partial separation;
decided-condition explanation time remains unresolved if those observations
do not exclude it. Any migration comparator supplied later must be separately
pinned, reviewed for equivalence, and run with both compilers.

[The work instrument](../../../compiler/src/semantic/entailment/work.rs) emits
process ID, analysis-run ID, function, kind, ordinal, metric and value. Keep
those identities as well as function aggregates: candidate reanalysis and
synthetic contract implication also create runs. Repetition is evidence to
reconcile with main and the failed-body inventory, not by itself proof of
recovery. Every phase is inclusive; nested phases and nested analyses overlap.
Do not add them to predict wall time or subtract them to invent explanation
time. Main has no OP-5 phase counters; missing metrics are unavailable, not zero.

Rows are written only when each function analysis finishes (`work.rs`, lines
292–305). A timed-out module can retain earlier functions' rows while omitting
the function responsible for most of its time. A module without analyzed
functions may never open the output file at all. The pinned static-atom module
contains only its constant interface, but this run still emits 40 analyses on
each compiler from the selected checking context; it is not a missing-counter
example. Style and layout instead emit no rows because their parser failures
precede semantic analysis. Missing counters therefore mean unavailable
attribution, not a failed source verdict or a measured zero. Counter output
errors and saturated values invalidate the counter evidence. Counters are
collected in separate invocations because their overhead is not the
uninstrumented cost.

## Executed CI protocol

The temporary [workflow](../../../.github/workflows/op5-cost.yml) built each
compiler once and retained the migrated Firn three-way comparison. It packaged
the same pinned downstream sources and both binaries for independent module
jobs. Halo selected `pkg::vm`; Snowghost selected every module declared in its
pinned renderer graph. No entry composition was included in this screen.

Each module first ran uninstrumented main, branch, and a twin of main, then
ran main and branch with separate `WHITEFOOT_CHECK_WORK` files. Each compiler
invocation had a 600 s deadline. The jobs retained compiler/wrapper statuses,
elapsed times, logs, verdicts and raw counters after a stop, so one pathological
module cannot hide all later modules. The summary ranks timeouts and completed
modules and retains per-function analysis multiplicity, OP-5 work and phase
timers, including the slowest modules. A timeout is censored evidence, never
a source rejection, a zero duration, or a completed ratio.

This completed first isolated sweep is a bounded attribution sample. The main twin
reveals obvious within-job variation; one triplet does not estimate a stable
performance distribution. Counter-on timing is reported separately. Different
module jobs may use different hosted machines: compare compilers within each
job, not their absolute times across hosts as a controlled ranking. The
ranking selects where to look next. Do not compare the sum of isolated-module
times with the old modules-plus-entries total. Precise confirmation, if needed,
uses interleaved uninstrumented repeats and a twin on the CI 14900K runner,
after checking its availability through the coordinator.

The experiment's 600 s censoring limit is not a gate budget or an acceptance
limit. This change adds no ordinary gate stage or budget, and removes no
correctness coverage. The temporary workflow is removed before the branch is
ready, as its existing lifecycle requires.

## Meaning for the owner's reopening condition

The existing Halo ratio and Snowghost bound already exceed 1.50 on the measured
rejected or unfinished invocations. They require investigation now. Recovery
and explanation caused by OP-5 are still costs caused by OP-5; an accepted-input
control clarifies the cause and remedy, and is not a new exemption from the
owner's threshold.

- If substantial excess remains in accepted or zero-redundancy analyses and
  concentrates in origin-view preparation/propagation along chains, that
  directly supports the selected incremental-view implementation. A
  same-workload completed ratio above 1.50, outside twin variation, confirms
  the threshold on that workload.
- If additional analyses dominate and their count follows rejection recovery,
  report that amplification explicitly. Incremental views may reduce the cost
  of each pass but do not remove the repeated passes. The owner's trigger has
  not been disproved; its selected remedy may not address the main cause.
- If deciding-proof explanations remain the plausible dominant cost, the
  permitted instrument cannot quantify them separately. Report that limit;
  choosing a new instrument or remedy is further work, not a claim that the
  chain-view change has been justified.
- If a branch module hits 600 s while both uninstrumented main controls finish
  below 400 s, the deadline is greater than 1.50 times either main observation.
  That establishes a per-module lower bound, not a complete-program ratio or
  a cause. A timeout with a slower or interrupted baseline proves no such bound.
- If isolated modules do not reproduce the old aggregate cost, entry
  composition and report recovery remain open; the old timeout must not be
  relabeled noise or evidence that the condition was not triggered.

## Validation and disposition

This note records source inspection and extraction of supplied artifact data,
not a new compiler run. No local build, test suite, compiler invocation or
measurement was performed. The workflow ran in the completed attribution run
above. GitHub access and the local status-board service were unavailable from
this session; the board update remains pending rather than claimed complete.
The downstream migrated/explanation-off control and an independent explanation
timer remain unavailable under the requested scope. No design approval log,
specification approval log, specification rule, conformance expectation or
downstream source was changed.

An independent read-only Codex review covered this workflow and note against
the compiler revision above, including the wrapper, module-verdict and counter
contracts, relevant design decisions, and repository checklist groups A, D, T
and V. The reviewer found that a counter-write failure could leave a valid TSV
prefix that the initial summary treated as complete evidence. The workflow now
rejects the warning and excludes those rows from numeric summaries; CI-only
negative controls cover the failure. The reviewer inspected that repair and
the no-function/no-counter distinction and reported no remaining concrete
findings within scope. The inherited review model identifier was unavailable.

The supplied artifact verifies execution of the attribution workflow, not a
passing compiler gate. A fresh read-only review using `gpt-6.1-sol` at high
effort checked this prose-only diff against the run's branch head, checklist
groups A, D and V, and relevant design consistency. It independently matched
the raw TSVs to the per-analysis records, checked the numerical envelopes and
source/report interpretation, and found no issues within scope. The interrupted
phases, future reuse soundness and proposed performance gains remain unverified.
The earlier review above concerned the pre-run workflow revision. Checklist T8's
budget-row requirement also remains unverified for the temporary measurement
labels: no budget-table change was authorized by this task's file scope, and
the experiment deadline is not a budget approval. The unavailable downstream
migration/explanation-off control remains an attribution limitation. The review
does not validate a new performance result or approve the pull request.
