# Attributing OP-5 cost on Halo and Snowghost

## Question and scope

This pre-run record extends the [exact-recovery investigation](EXACT-RECOVERY.md)
for [PR #393](https://github.com/Ming-Research/Whitefoot/pull/393). Does the
remaining cost come from deciding each condition, explaining decided conditions,
rechecking rejected modules, or disproportionate work on particular functions?
The owner accepted approximately 25% additional Firn checking time and selected
incremental views along an `if` chain if OP-5 makes any program's check more than
50% slower. This investigation attributes that trigger; it changes neither the
threshold nor the specification, compiler, or downstream sources.

Code inspection below refers to compiler revision
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
the function responsible for most of its time. A module without functions may
never open the output file at all: Snowghost's pinned static-atom module contains
only its constant interface. Missing counters therefore mean unavailable
attribution, not a failed source verdict or a measured zero. Counter output
errors and saturated values invalidate the counter evidence. Counters are
collected in separate invocations because their overhead is not the
uninstrumented cost.

## Next CI run

The temporary [workflow](../../../.github/workflows/op5-cost.yml) builds each
compiler once and retains the migrated Firn three-way comparison. It packages
the same pinned downstream sources and both binaries for independent module
jobs. Halo selects `pkg::vm`; Snowghost selects every module declared in its
pinned renderer graph. No entry composition is included in this screen.

Each module first runs uninstrumented main, branch, and a twin of main, then
runs main and branch with separate `WHITEFOOT_CHECK_WORK` files. Each compiler
invocation has a 600 s deadline. The jobs retain compiler/wrapper statuses,
elapsed times, logs, verdicts and raw counters after a stop, so one pathological
module cannot hide all later modules. The summary ranks timeouts and completed
modules and retains per-function analysis multiplicity, OP-5 work and phase
timers, including the slowest modules. A timeout is censored evidence, never
a source rejection, a zero duration, or a completed ratio.

This first isolated sweep is a bounded attribution sample. The main twin
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
measurement was performed. The changed workflow awaits CI execution; GitHub
access and the local status-board service were unavailable from this session.
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

Execution of the workflow and its controls remains unverified. Checklist T8's
budget-row requirement also remains unverified for the temporary measurement
labels: no budget-table change was authorized by this task's file scope, and
the experiment deadline is not a budget approval. The unavailable downstream
migration/explanation-off control remains an attribution limitation. The review
does not validate a new performance result or approve the pull request.
