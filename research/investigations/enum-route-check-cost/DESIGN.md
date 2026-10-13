# Enum route checking cost

## Question and prior evidence

Does unnecessary ordinary closure preparation for an already contradictory
conditional payload context explain the checker regression introduced by
[PR #431](https://github.com/Ming-Research/Whitefoot/pull/431)?

The supplied hosted-runner bisect checked Halo-wf revision `80f0ee4`, module
`pkg::vm`, in five alternating rounds against a fixed older compiler. Its
median user-plus-system ratios were 1.77 for parent
`7d9118ebaf88beac3ecdcd87d408dcd61db6cf3d` and 2.07 for merge
`af7665be672d29912d878dd5da8c3932bb6f296c`. These are supplied observations,
not measurements performed by this investigation. Their quotient suggests
about a 17% regression. The subsequent CI results below include Firn.

## Source hypotheses

1. `compiler/src/semantic/entailment/flow/results.rs`, `payload_routes`,
   now admits every enum data field. `capture_route` creates a contradictory
   context for each nonselected variant, even when no written routed clause
   exists. `refresh_result` nevertheless materializes the ordinary closure;
   `refresh_result_from_snapshot` traverses ordinary candidates that cannot
   strengthen a contradiction. Copies, moves and expression completion repeat
   this work. This is the first implementation change to test.
2. In the same file, `kill_result_evidence` and `exit_result_scopes` prepare
   ordinary snapshots whenever any associations survive, including when all
   surviving contexts are contradictory. `join_result_evidence` prepares one
   snapshot per predecessor before knowing whether any route can use it.
   Expanded enum routes multiply this downstream work.
3. `payload_routes` repeatedly scans variant fields and calls
   `payload_supplies_data`, which can traverse nominal struct/Box descendants.
   `payload_pairs` scans the term inventory for noninteger payloads. These
   costs can grow even without a routed call and may remain after the first
   fix. Ordinary constructions with actual payload terms must still transport
   their facts whether or not a function writes an `ensures` clause.
4. `compiler/src/semantic/range_judgment/walk/calls.rs`,
   `owe_postconditions`, now clones the range state for every owed clause,
   including unrouted clauses. `owe_propagated_postconditions` clones state
   and reads an error payload even before discovering whether any range
   postcondition is owed. This is an independent possible source of cost;
   ordinary entailment counters do not measure range-state cloning.
5. The merge changes selector metadata and verified-summary consumers, but
   changes neither `semantic/check/receipts.rs` nor `driver/cache.rs`.
   The requested command supplies no `--cache`, and the CLI opens a persistent
   cache only when that option is present. Persistent receipt reuse therefore
   cannot explain a warm-versus-cold difference in these runs.

## First candidate implementation boundary

Keep every conditional context and its contradiction witness. ENT-5 requires
a construction's nonselected variants to be contradictory and requires those
alternatives to remain neutral when joined with a live variant. Dropping
them, or admitting routes only for written postconditions, would lose facts.

An already contradictory state derives every relation. Adding ordinary facts
cannot strengthen it. Skip ordinary snapshot preparation and import for that
state; prepare shared snapshots only when a noncontradictory context needs
them. Preserve holder invalidation, missing-as-empty joins, route alignment,
live-state closure, and the existing join operation and its proof parents.
This is an implementation optimization under the existing checker-facts
decisions, not a language or design-decision amendment.

## Comparison and falsifier, before measurement

The temporary `.github/workflows/enum-route-check-cost.yml` compares before,
after and the submitted fix on identical workload bytes and the same hosted
runner within each workload job. It records exact compiler and workload
revisions, toolchain and CPU, separates compiler building from checking,
and times five interleaved rounds after initial successful probes. A second
copy of the before binary supplies a twin noise control. Counters are enabled
only in separate attribution runs. The source census records Halo's routed
clause spellings instead of assuming their absence.

Halo uses the pinned VM module. Firn resolves `main` once per job and first
checks entry `firn`; if that fails, diagnostics remain artifacts and the
comparison names the largest source module accepted by all three compilers.
A fallback module result is not a whole-Firn result.

The initial fix checkout is based on `50773fdcd3007685e4cdd1903b2dcb0cc1052600`,
which also includes PR #432 after the implicated merge. Before/after isolates
PR #431; fix/before includes that intervening revision as well as this fix.
The intervening diff contains only documentation and guidance changes, so it
does not explain the remaining checker cost.

Reject the claim that this fix explains the regression if excess snapshot
work does not decrease, or the fix/before excess remains outside the
observed twin noise range. Counter reduction without timing recovery supports
only a partial attribution. Print each round, medians and ratio spread rather
than treating a hosted runner as a precise performance qualification. A perf
profile of the after compiler supplies symbol attribution for costs the
entailment counters cannot distinguish.

## First CI result and remaining join work

[The first hosted run](https://github.com/Ming-Research/Whitefoot/actions/runs/38392086326)
measured candidate `37d7ed1913dd74b217bdec054b5a1eba69a301dd` on the pinned
Halo VM module. The runner reported Ubuntu 24.04, AMD EPYC 9V74, four logical
CPUs, Rust 1.99.0 and the gate profile. The downloaded `identity.txt`,
`summary.md`, `timings.tsv` and counter TSVs are the evidence for this section.

| Paired CPU ratio to before | Median | Minimum | Maximum |
| --- | ---: | ---: | ---: |
| Routed-ensures merge | 1.1630 | 1.1576 | 1.1659 |
| Contradictory-snapshot fix | 1.0748 | 1.0697 | 1.0766 |
| Before twin | 0.9990 | 0.9981 | 1.0019 |

This rejects complete attribution to contradictory snapshots: the remaining
7.48% median excess over before exceeds the observed twin variation. The
first fix remains useful, but is not sufficient. Its two flow tests and five
enum-route tests passed; these results do not validate subsequent edits.
Firn failed before measurement because its graph needed the Halo submodule.
The owner subsequently added submodule fetching; the second result below
includes Firn. The follow-up fixes do not edit that workflow.

Raw work TSVs show the following totals by function name. `varargs` has two
analysis invocations, both included. After and the first fix have identical
per-pass join metrics; the fix saved work outside those joins.

| Function | Join calls, before / after / first fix | Evaluated pairs, before / after / first fix | Proof intern attempts, before / after / first fix |
| --- | ---: | ---: | ---: |
| slow | 48 / 453 / 453 | 37,073 / 235,008 / 235,008 | 147,521 / 1,004,606 / 981,362 |
| prepare_inner | 22 / 61 / 61 | 26,181 / 77,788 / 77,788 | 136,210 / 334,627 / 318,249 |
| install_libraries | 6 / 71 / 71 | 6,132 / 35,862 / 35,862 | 11,798 / 71,028 / 68,364 |
| varargs | 12 / 22 / 22 | 10,168 / 26,988 / 26,988 | 35,348 / 94,816 / 81,640 |
| start | 8 / 8 / 8 | 37,910 / 37,910 / 37,910 | 125,115 / 125,868 / 125,116 |
| run | 32 / 88 / 88 | 16,584 / 16,584 / 16,584 | 155,447 / 165,038 / 155,507 |

`intern_calls` counts derivation-ledger proof interning attempts, not the
number of interned terms. These artifacts do not measure the TermTable's
cardinality. The extra joins include zero-row joins; thus unchanged pair
counts in `run` do not imply unchanged join bookkeeping. A pass shape does
not identify its source statement or route.

Reading the exact Halo revision identifies the source mechanism:

- `lib/halo/value/module.wfm` declares five integer routes in `Value`:
  `Str.h`, `Tab.h`, `Fun.h`, `Builtin.id`, and `Thread.h`, all `u32`.
  `Nil`, `False`, `True`, and the floating-point `Num` have none.
- `varargs` initializes a local to `Value::Nil()` and conditionally replaces
  it with an indexed load. The five excluded contexts meet five missing
  contexts. Each join reads the same ordinary facts on the loaded-value
  predecessor. Its extra ten passes per invocation all evaluate a 29-row
  matrix: 841 pairs each. Five equal route computations would need one
  computation, saving 6,728 pairs per invocation; this is a prediction
  contingent on full candidate/proof equality, not a measured saving.
- `slow` initializes `tm` and `other` to `Nil` and repeatedly assigns call
  results under successive conditions. It retains and rejoins these routes.
  `install_libraries` similarly copies a constructed table value, then
  conditionally replaces its target with returned values.
- `prepare_inner` constructs `FuncView::Native` while setting a correlated
  configuration value, then rematches it. It also constructs
  `HostOutcome::Raised`, assigns a call result, then matches the outcome;
  `Returned.count` is that enum's integer route. These contexts can retain
  real branch facts. Its extra cost is not all fivefold `Value` duplication.

The source census contains routed prelude `Ok` clauses but no routed
postconditions on these program-declared enums. That does not make their
contexts unnecessary: ENT-5 also transfers construction and ordinary branch
facts. For example, joining an excluded route on one predecessor with a
missing route on another can retain `x <= 7` only under that route. Deleting
the context would lose that fact when the variant is selected. The new fix
therefore removes repeated computations, not any of these facts.

These ordinary snapshots can contain existing call postconditions. At the
fixed Halo revision, `ensure_stack` (`lib/halo/vm/state.wf:304`) ensures both
`stack^.inner.len >= size` and
`stack^.inner.len >= entry(stack)^.inner.len`; `resize_stack` in that file
ensures `stack^.inner.len == size`. The `callback` contract
(`lib/halo/vm/slow.wf:198`) and the public `Host::call` contract
(`lib/halo/vm/module.wfm:79`) preserve the entry stack length. Conversely,
`metamethod` has no written postcondition. The new route contexts import
existing ordinary facts, including branch bounds, rather than new source
contracts. The TSVs do not contain relation or proof identities, so these
clauses identify possible sources, not particular hot matrix cells.

## Follow-up: share exactly equal numeric joins

Within one `join_result_evidence` invocation, memoize the result for an ordered
sequence of incoming numeric states. Each input key includes either its exact
contradiction witness or every live numeric candidate and its proof identity,
including weaker ordinary candidates behind a postcondition-dependent bound.
Missing associations have the empty candidate list. The ordinary facts at
each predecessor and the term vocabulary remain fixed throughout the call.

Equal keys have the same numeric premises and contradiction proofs available
at the same ordered predecessor positions. Reusing their refreshed join
therefore preserves the full join judgment, including postcondition fallback;
the existing `join_at` still computes each distinct case. Keys are compared
for equality, not only by hash. Different exclusions or proofs miss the cache.
No key is reused at another flow point. Per-route payload, selector, success
and definitely-excluded metadata stay separate, and cloned fact states keep
their existing copy-on-write mutation isolation. No source program, type name,
route-count threshold or work budget selects this optimization.

The two added internal tests check duplicate-input reuse versus different
exclusion masks and mutation isolation; and equal winning bounds with weaker
postcondition fallbacks of 5, 10 and 5. Removing the call-dependent candidates
must expose those respective bounds. Both tests require two numeric Join
events for three routes, so the prior implementation's three events fail the
cost assertion. The original missing/excluded-context and complete-candidate
transport tests remain unchanged. All are selected by the existing workflow
filter; no workflow change or local execution is needed.

Before measuring, the prediction is fewer repeated join passes and proof
intern attempts in the rebinding hotspots, with unchanged acceptance. Reject
this candidate as an explanation of the remaining regression if those counts
do not decrease or CPU time does not improve outside twin variation. Complete
recovery still requires fix/before to return within the observed twin range.
Key construction scans each live input's stored bound matrix, including
absent cells, then copies and hashes its complete candidate list; semantically
equal contexts with different proof identities deliberately do not share. Single-route work in
`prepare_inner`, real payload facts, and unrelated range-state cloning may
remain. The existing TSVs do not expose exact route identities or cache hits,
so the next CI run must decide the net benefit. No perf artifact was present
in the supplied first-run bundle.

## Second CI result: unused contexts still dominate the residual

[Run 38400487566](https://github.com/Ming-Research/Whitefoot/actions/runs/38400487566)
measured join sharing at `541f91903c4064e183112cfc82f65ea8e217543d`, using the
same gate profile and hosted Ubuntu 24.04 protocol. Halo checked `pkg::vm`;
Firn checked the whole program with `--graph firn/modules.wfg --entry firn
--check`. These are supplied CI artifacts, not local measurements.
Halo's runner reported AMD EPYC 7763; Firn's reported Intel Xeon 6973P-C.
Each comparison uses one runner, but the two workloads are not a same-machine
comparison with one another. Firn's source was `c7e14fa01b972e187b28e4c24028f4bb37aa993c`;
its Halo gitlink was `56878ac4d607f7496c8e23a4c2ffe2d45e9cb552`, whose
`lib/halo` source has no diff from the standalone Halo pin.

| Workload | After/before median | Fix/before median (min..max) | Twin/before median (min..max) |
| --- | ---: | ---: | ---: |
| Halo VM | 1.1646 | 1.0552 (1.0339..1.0652) | 1.0015 (0.9881..1.0089) |
| Firn whole program | 1.1380 | 1.0413 (1.0150..1.0616) | 1.0021 (0.9749..1.0084) |

Both miss the owner's new acceptance target of a median paired CPU ratio at
most 1.03 against before. Four flow/results tests and five enum-route tests
passed for this candidate; they do not validate the next change.

The component breakdown below uses before / after / second fix in every
cell. Cells count occupied closure or snapshot entries; evaluated join pairs
include absent entries. Proof intern attempts count before deduplication.
These heterogeneous counters explain work but are not elapsed-time units.

| Halo function | Join pairs | Closure cells | Snapshot cells | Proof intern attempts |
| --- | ---: | ---: | ---: | ---: |
| slow | 37,073 / 235,008 / 159,540 | 258,415 / 801,936 / 665,198 | 97,260 / 406,055 / 310,553 | 147,521 / 1,004,606 / 767,622 |
| prepare_inner | 26,181 / 77,788 / 77,788 | 208,143 / 544,883 / 468,357 | 101,730 / 393,086 / 310,875 | 136,210 / 334,627 / 318,249 |
| cjson_decode | 23,367 / 56,972 / 42,736 | 155,178 / 271,181 / 235,246 | 64,184 / 120,225 / 99,728 | 96,774 / 195,345 / 151,507 |
| install_libraries | 6,132 / 35,862 / 22,794 | 58,270 / 188,459 / 150,494 | 21,989 / 122,259 / 107,175 | 11,798 / 71,028 / 51,310 |
| library_cjson | 22,960 / 28,080 / 23,984 | 212,698 / 240,141 / 214,564 | 83,782 / 119,257 / 86,431 | 112,661 / 133,689 / 115,745 |

| Firn function | Join pairs | Closure cells | Snapshot cells | Proof intern attempts |
| --- | ---: | ---: | ---: | ---: |
| to_cell | 17,666 / 17,666 / 17,666 | 35,480 / 2,557,846 / 100,094 | 0 / 3,811,984 / 102,608 | 30,936 / 7,677,294 / 277,868 |
| take_engine | 572,008 / 572,008 / 572,008 | 3,160,894 / 3,160,894 / 3,093,132 | 1,839,378 / 1,839,378 / 1,770,540 | 3,481,176 / 3,481,176 / 3,360,408 |
| aof_rw_dataset | 133,490 / 133,490 / 133,490 | 4,420,450 / 4,420,450 / 3,017,854 | 2,160,926 / 2,160,926 / 1,079,460 | 816,210 / 816,210 / 608,158 |
| slow | 111,219 / 705,024 / 478,620 | 775,245 / 2,405,808 / 1,995,594 | 291,780 / 1,218,165 / 931,659 | 442,563 / 3,013,818 / 2,302,866 |
| eval | 232,317 / 389,967 / 263,847 | 2,780,881 / 3,224,356 / 2,869,363 | 1,112,664 / 1,268,319 / 1,164,549 | 633,193 / 1,056,049 / 715,318 |
| prepare_inner | 78,543 / 233,364 / 233,364 | 624,429 / 1,634,649 / 1,405,071 | 305,190 / 1,179,258 / 932,625 | 408,630 / 1,003,881 / 954,747 |
| install_host | 0 / 0 / 0 | 319,638 / 1,690,718 / 1,664,606 | 96,790 / 1,564,610 / 1,538,498 | 40,828 / 255,400 / 251,496 |

Firn contains three analyses apiece of Halo's `slow` and `prepare_inner`;
each has the same component counts as the standalone Halo analysis. The
function-name summaries include repeated concrete/synthetic analyses, not
source function identities. Unchanged before/after counts in `take_engine`
and `aof_rw_dataset` mean their large totals are not this regression, even
though the general first fixes also reduce their work.

Join sharing only eliminates identical inputs at one merge. Distinct
contradiction witnesses, conditional facts, or fallback proofs still require
separate computations. Every retained live context also incurs independent
refresh, support-kill and scope-exit closure work between merges. Thus fewer
join pairs do not eliminate the excess closure and snapshot cells.

The source has simpler unused-state cases. Halo's `slow` repeatedly updates
`tm` and `other`, passing them to calls without locally selecting their
conditional states. Halo's `to_cell` directly returns constructed `Cell`
variants with integer fields and has no routed postcondition. Firn's
`install_host` constructs and stores table keys and values without reading
their conditional evidence. In contrast, `prepare_inner` rematches its
`FuncView`, so its correlated evidence cannot all be dropped. The next
candidate must distinguish actual consumers rather than merely testing for
written postconditions or an enum type's route count.

## Third candidate: omit conditional computations with no reader

An enum value with no reachable conditional-state consumer need not retain
its conditional contexts. This is observational omission, not creating an
empty context at the first read: a later match can need facts from a branch
that has already joined. For example, starting with `Choice::None()`, assigning
`Choice::Some(value: 0_u32)` only when `n < 10_u32`, then matching `Some`
can recover `n < 10_u32` in that arm without any routed postcondition.
Reconstructing from the ordinary state only at the match would lose that
branch fact. A demanded value therefore keeps its original construction,
refresh, kill and join history.

The existing binding-inventory traversal also records conditional readers
and bare-binding transfer edges. Own payload matches, propagation, routed
returns (including generated clauses), and routed requirements of nested
calls seed demand. Demand follows copies, assignments and `give` deliveries
backward to their source bindings, to a finite fixed point. Every version of
a demanded binding is retained, including versions created before the first
read and loop-carried assignments. This deliberately over-approximates use;
there is no last-use guess, path-feasibility test or proof replay. An ordinary
call does not transport the caller's private conditional context into the
callee; only a routed argument requirement reads it at that boundary.

Unread non-eager local values skip construction/copy capture and subsequent
conditional storage. Without those associations, ordinary flow performs no
route-specific refresh, kill, scope-exit closure or join for them. Existing
integer-Result eagerness stays intact. Every demanded route still uses the
same numerical transport and proof machinery. This is confined to the
existing result-evidence module and binding inventory, not a second body
analysis or another source-fact implementation.

DIAG-2 imposes a separate observation: each established S12 fact is a required
root even when no later goal reads it. Thus routed calls still establish and
root their conditional postconditions before an unread result context is
discarded; ordinary call roots and effects are unchanged. Body-entry routed
requirements also retain their existing establishment. Construction and copy
transport nodes have no independent required root; when neither a goal nor
a required root can reach their contexts, omitting that computation loses no
accepted proof. No clause is delayed or re-instantiated against later mutable
arguments.

The prediction, written before the next run, is lower join pairs in functions
with unread rebinding chains, and lower closure/snapshot cells and proof
intern attempts where unread contexts formerly survived across statements.
Direct constructed returns without routed clauses should also lose their
unused snapshot work. Genuine readers, such as the rematched `FuncView` in
`prepare_inner`, remain and can leave residual work. The candidate succeeds
only if CI preserves the required proof tests and both workload median paired
CPU ratios are at most 1.03. Lower counters alone do not satisfy that target;
the demand inventory and conservative false positives have their own cost.

Five new internal tests are selected by the unchanged workflow filters:

- `unread_constructions_copies_and_returns_create_no_conditional_work`
  inspects the actual walk before ledger pruning: unread construction, copy,
  assignment and direct return create no stored contexts, transport/exclusion
  nodes or private payload terms. The preceding implementation creates those
  terms and nodes, so this detects work that final receipt pruning would hide.
- `route_demand_follows_copy_cycles_and_nested_value_deliveries` checks that
  unused copies stay outside demand while copy/assignment cycles and nested
  `give` receivers retain every predecessor needed by an own match.
- `unread_routed_call_keeps_its_required_conditional_root` checks that an
  unread call result still retains exactly one conditional S12 root after
  rebinding, protecting DIAG-2 independently of future uses.
- `future_match_retains_construction_exclusion_through_copy_and_join` checks
  that an excluded construction joined with an unknown value on a bounded
  branch preserves that branch's numeric bound when later matched. It has no
  routed postcondition and guards against delaying capture until first read.
- `a_call_requirement_retains_its_copied_argument_route` checks a copied
  `Option<Span>` carrying a struct invariant at a call requirement, without
  a local match. It guards the other route-reading boundary.

The cost test's expected failure on the previous implementation and the
other tests' protected failure modes are source-inspection arguments; no
new test, including a negative-control run, has executed locally.

## Validation and outstanding evidence

No compiler, build, test suite or measurement runs on the workstation.
Focused internal regression checks and existing enum-route tests are scheduled
in the temporary workflow; the project gate remains required before delivery.
No conformance source, verdict, specification or approval log is changed.
The first two candidates' timings and focused tests are recorded above. The
third candidate awaits the owner's commit, push and CI results. No new
acceptance claim uses an earlier candidate's green tests. The temporary
workflow is removed before the branch is ready, after the measurements finish.

For the first candidate, an independent read-only review with `gpt-6.1-sol` covered the complete
uncommitted change against the initial fix checkout, the new workflow and this
record, ENT-5, the checker-facts decisions, and the directly affected fact-state
and kill/join consumers. It found no functional defect within that scope.
Repository layout, citations, case ownership, gate isolation and design
correspondence passed source inspection. Execution and performance were
unverified at that review; subsequent CI is recorded above. This review was
not delivery approval.

A fresh independent read-only review with `gpt-6.1-sol` covered the follow-up
diff against `41f19afb3`, including both changed files, the affected consumers,
ENT-5, relevant compiler decisions, and applicable checklist groups A, D, C,
T, V, G and DC. It independently reproduced the counter table from the raw
TSVs and found no functional defect within scope. Its one finding was the
incomplete description of key construction cost; the paragraph above now
includes the matrix scan. The primary agent verified that correction and the
numeric-only state invariant, complete-candidate key, local reuse boundary,
and independent route metadata. Execution and performance were unverified at
that review; the second CI result above now covers join sharing. The third
candidate has no execution evidence yet.

An independent `gpt-6.1-sol` review covered the complete third-candidate diff
against `541f91903` in all six changed files, affected consumers, ENT-5,
DIAG-2, the relevant design decisions and checklist groups A, D, C, T, V, G
and DC. It independently recomputed the new component tables from raw TSVs.
The review identified mandatory unused S12 roots as a constraint; the
implementation retains them. It also identified an ambiguous historical
implementation boundary, now explicitly labeled as the first candidate.
A final narrow review covered the nominal-type transfer filter and fifth
test. No functional finding remains within scope. The primary agent verified
the consumer inventory, backward dependencies, call-root preservation,
return filtering and pre-pruning cost assertion. All five changed Rust files
remain below 4,000 lines. This source review supplies no execution or timing
evidence for the third candidate.

The temporary experiment follows the supplied bisect workflow's unbudgeted
measurement labels. It changes no maintained budgets and does not replace a
project gate. Inspection also found that the existing `compiler/build` label
in `compiler/Makefile` has no row in `.github/time-budgets.txt`; this unrelated
budget inventory gap is left unchanged. The focused test-build label has its
existing budget. No status-board connector was available in this session, so
the inventory finding could not be filed there.
