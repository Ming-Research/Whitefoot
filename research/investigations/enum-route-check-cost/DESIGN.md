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
than treating a hosted runner as a precise performance qualification.
The temporary workflow now uses timings and counters alone: the owner
requested removal of perf installation and sampling after the hosted kernel
had no matching installed perf tool. Counter attribution does not measure
costs outside the instrumented checker paths.

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

## Third-candidate CI and selector-identity repair

The owner reported [run 38411980870](https://github.com/Ming-Research/Whitefoot/actions/runs/38411980870)
at `d6d48610907cd59061529898d57b3accad56e7b4`: Firn's five interleaved rounds
gave median paired CPU ratios after/before 1.1321, fix/before 0.9824
(0.9622..0.9989), and twin/before 0.9885. Firn meets the 1.03 target.
Halo did not reach timing: the unchanged unused-call root test was rejected
at its `ensures when Some(value: n)` selector. These numbers and the failure
are supplied CI results; no new artifact analysis was performed for this run.

That test is valid. TYPE-6 resolves a result-route label against the declared
result enum. FN-9 requires a variant of that enum and an admitted field type;
the test has `Outcome::Some(value: u64)`. CALL-4 permits omitting the result
binder for its sole result ordinal. The resolver instead accepted lexical
prelude `Some` before inspecting the source result enum, so FN-9 correctly
refused the wrong `Option::Some` declaration identity. The affected resolver
code is unchanged between the routed-ensures merge and this CI revision;
the demand optimization exposed the defect through its new test.

The repair enumerates the declared result owners before allowing a successful
lexical lookup to preempt a unique contextual source variant. Explicit result
binders restrict that enumeration. Actual prelude owners are read from the
existing PRE-1 declaration preorder, without a `Some` name special case.
An ordinal that can carry the lexical prelude target, a symbolic generic
ordinal, or conflicting source owners retains its previous lookup behavior.
FN-9/CALL-4 admission, payload-data checks and proof publication are unchanged.

The five demand tests remain unchanged. Only the unused-call root test writes
the colliding selector; the others use enum-qualified constructions, match
labels, or actual `Option` values. The existing root assertion continues to
require exactly one conditional S12 root for the unread call. A new resolver
test checks seven identities: an omitted source route, explicit source and
Option routes, preserved mixed-owner omission, unrelated Result and scalar
ordinals, and preserved conflicting source owners. The old resolver fails its
source-identity assertion by inspection. The existing focused test exercises
the end-to-end rejection; the new resolver test belongs to the ordinary gate
and is not added to the temporary workflow's filters.
The older enum-route fixtures were also inspected: `Pair`, `Item` and `Held`
do not collide with prelude variants. The foreign-variant negative fixture
intentionally selects `Some` from an enum without that variant; it must still
receive FN-9, and its assertion is unchanged. The remaining results tests
exercise numeric states directly rather than selector-name resolution.

Inspection also found a separate unresolved boundary: with distinct result
enum types that both declare `Some`, an omitted ordinal does not uniquely
identify the owner. TYPE-6 gives result-owned lookup, while CALL-4's explicit
ambiguity rule names repeated ordinals of the same enum type. This repair
does not choose a new rule for distinct owners; it preserves their existing
behavior. Generic result-type routes likewise retain the existing symbolic
lookup rather than gaining a new late-resolution mechanism. A separate
selector audit should settle these cases before extending their behavior;
no observed conformance result or new language judgment is claimed here.
No board connector was available to file that follow-up, so this record and
the report carry the unresolved scope boundary without inventing a board key.

## Validation and outstanding evidence

No compiler, build, test suite or measurement runs on the workstation.
No conformance source, verdict or specification is changed; `design/log.md` gains one entry.

Round 4 of the temporary workflow
([run 38420072424](https://github.com/Ming-Research/Whitefoot/actions/runs/38420072424))
measured the selector-repaired third candidate `bd70c6b71` against the
pre-#431 main `7d9118eba` on the same hosted-runner panel, five interleaved
rounds, compiler user+sys CPU, no check cache:

| Workload | after/before | fix/before (min–max) | before-twin/before |
|---|---:|---:|---:|
| Halo `--check-module pkg::vm` at 80f0ee4 | 1.1891 | 1.0250 (1.0156–1.0363) | 0.9953 |
| Firn whole program (`--entry firn --check`) | 1.1197 | 0.9780 (0.9729–0.9908) | 0.9964 |

Both are within the 1.03 criterion. The focused results tests and all eight
`enum_routed_ensures` tests, including the unread-routed-call root test, pass
in that run. The temporary workflow is removed and the branch merged with
main (`7e983ba3f`, no conflicts); the project gate on the merged head is the
remaining execution evidence. The fix includes main's intervening change after
#431, so fix/after is not a single-commit comparison; fix/before isolates the
end state against the pre-regression compiler.

Halo's VM test host module checked far slower after #431 (0.83 s with
wf-d837678c6905, 190.6 s with wf-af7665be672d; Halo-wf run 38424433942):
constructing a variant stored a contradiction context for every data field
of every other variant, and every context was joined. A temporary workflow
([run 38428431313](https://github.com/Ming-Research/Whitefoot/actions/runs/38428431313))
checked `--graph tests/halo-vm/modules.wfg --check-module pkg::test` at Halo
9de699da with d837678c6905, main 8ba7da5c6 and this branch `7cb40102e`, three
interleaved rounds, compiler user+sys CPU: median 1.97 s, 629.55 s and 1.99 s;
branch/old 1.005 (0.985–1.015), main/old 312.4. Joins were 840, 549,483,471
and 840. The omitted unread computations cover this case without the
contradiction-route snapshot change proposed separately.

### Re-measurement after merging main (`c40acc366`)

Merging main brought rule OP-5 (a conditional test whose outcome the checker
already knows is rejected) and a grammar change that makes `held` unusable as
a parameter name, so the merged compiler rejects the Halo and Firn sources the
earlier rounds used. A temporary workflow applied the compiler's own
mechanical fix for each rejection, one at a time, with the merged compiler
(Halo `pkg::test` 2 fixes; Halo `pkg::vm` 132: 130 OP-5 tests and 2 EFF-2
effect rows; Firn 240, including the `held` renames in its Halo dependency),
so all four compilers checked the same accepted source and every exit code was
0. The adapted sources and their diffs are workflow artifacts only. The four
compilers: base `7d9118eba` (pre-#431 main; `d837678c6905` for `pkg::test`, the
compiler of its earlier measurement), prebranch `e2f2bfe2f` (this
fix before the merge), main `c40acc366` (the merged main, with the #431
regression), and branch (this fix after the merge; the compiler tree is
identical at `1c35e4d66`, `2adcddb39`, `c7312e7b8` and `44f33c996`, the commits
the three runs built). Compiler user+sys CPU, interleaved rounds,
no check cache:

| Workload (host) | rounds | base s | prebranch s | main s | branch s | branch/base | branch/prebranch |
|---|---:|---:|---:|---:|---:|---:|---:|
| Halo `pkg::test` at 9de699da (EPYC 7763; [run](https://github.com/Ming-Research/Whitefoot/actions/runs/39577631558)) | 3 (main 1) | 1.99 | 1.99 | 588.99 | 1.97 | 0.990 | 0.990 |
| Halo `pkg::vm` at 9de699da (EPYC 7763; [run](https://github.com/Ming-Research/Whitefoot/actions/runs/39585762233)) | 5 | 16.33 | 8.66 | 11.75 | 9.65 | 0.592 (0.585–0.601) | 1.117 (1.110–1.139) |
| Firn whole program at b83b67fa (Xeon 8573C; [run](https://github.com/Ming-Research/Whitefoot/actions/runs/39610827470)) | 5 | 84.01 | 62.65 | 69.48 | 57.00 | 0.678 (0.667–0.691) | 0.918 (0.892–0.921) |

Work counters (one counted run each; joins is the `join:join` count, pairs the
`join:pairs_evaluated` count, cells the `closure:cells_sum` count):

| Workload | base | prebranch | main | branch |
|---|---:|---:|---:|---:|
| Halo `pkg::test` joins / pairs | 827 / 641,730 | 827 / 641,730 | not counted | 827 / 565,698 |
| Halo `pkg::vm` joins / pairs / cells | 25,170 / 1,122,732 / 8.29M | 28,568 / 1,176,523 / 8.39M | 290,038 / 1,554,023 / 14.03M | 28,568 / 1,176,621 / 9.61M |
| Firn pairs / cells | 7,477,212 / 67.2M | 7,582,217 / 64.1M | 7,448,963 / 80.6M | 6,304,847 / 61.4M |

What this shows, and no more: on the merged tree the omitted unread
computations still remove the #431 cost. Against the unfixed merged main the
branch is 0.820 on Firn (57.00 s against 69.48 s, ratio of medians) and 0.821
on Halo `pkg::vm`, and Halo `pkg::test` stays at the pre-regression 2 s
instead of 589 s; the Halo `pkg::vm` join count (28,568) equals the
pre-merge branch's and is a tenth of main's, and Firn's closure cells fall
from main's 80.6M to 61.4M, below base's 67.2M. The three
branch/base ratios meet the 1.03 criterion, but they are flattered by
unrelated speedups that main gained after `7d9118eba` (the pre-merge branch
is already 0.528 and 0.746 of base on `pkg::vm` and Firn), so the comparison
that isolates this fix is branch against main. Halo `pkg::vm` is 11.7% slower
and its closure cells are 14.4% higher after the merge than before it, with
the join work unchanged; that is main's own cost (for example OP-5 asks the
checker one more question per conditional test), and attributing it would
need a main without OP-5, which does not exist. Firn runs 8.2% faster after
the merge with fewer join pairs and cells, so the merge did not add work there.

### Re-measurement after the second merge of main (`b59f94622`)

Main advanced 192 commits (changes under `semantic/entailment`, `semantic/places`
and `resolution`, among others) and was merged again without a textual
conflict. The same method and adapted Halo source ran on the merged head
`da79abd12` ([run](https://github.com/Ming-Research/Whitefoot/actions/runs/39649159705),
EPYC 7763; adaptation: `pkg::test` 2 fixes, `pkg::vm` 132, every exit code 0).
Here prebranch is the head before this merge (`44f33c996`) and main is
`b59f94622` (with the #431 regression).

| Workload | rounds | base s | prebranch s | main s | branch s | branch/base (min–max) | branch/prebranch (min–max) |
|---|---:|---:|---:|---:|---:|---:|---:|
| Halo `pkg::test` (base `d837678c6905`) | 3 (main 1) | 2.05 | 2.08 | 604.56 | 2.07 | 0.995 (0.958–1.035) | 0.990 (0.981–1.015) |
| Halo `pkg::vm` (base `7d9118eba`) | 5 | 16.56 | 9.46 | 11.94 | 9.70 | 0.587 (0.581–0.596) | 1.025 (0.994–1.045) |

| Counter | base | prebranch | main | branch |
|---|---:|---:|---:|---:|
| `pkg::vm` joins / pairs / cells | 25,170 / 1,122,732 / 8.29M | 28,568 / 1,176,621 / 9.61M | 290,038 / 1,554,023 / 14.03M | 28,568 / 1,176,621 / 9.61M |
| `pkg::test` joins / pairs / cells | 827 / 641,730 / 4.14M | 827 / 565,698 / 4,584,143 | not counted | 827 / 565,698 / 4,584,254 |

The second merge left the work unchanged: on `pkg::vm` the branch's join,
pair and cell counts equal the pre-merge head's exactly and its CPU ratio to
that head (1.025) has a min–max range (0.994–1.045) that contains 1, which is a range of five rounds and not a confidence interval; on `pkg::test` the pair
count is equal and the cell count differs by 111 (0.002%). Against the unfixed
merged main the branch is 0.812 on `pkg::vm` (9.70 s against 11.94 s) and
`pkg::test` stays at 2 s against 605 s. Main's own counters on `pkg::vm` equal
those of the first merge, so the 192 intervening commits did not move the
unfixed workload either. Firn was not timed again: its source adaptation takes
more than an hour, and the Firn figures above stand for the first merge.

### Firn re-measurement on the merged head (`10f74e7e8`)

Firn was last timed on the first merge of main; the compiler has changed since
(192 commits under `semantic/entailment`, `semantic/places` and `resolution`),
and a measurement counts only for the compiler that merges. Main advanced again
by ten commits that change no file under `compiler/`, and the compiler sources
of this head differ from the timed `da79abd12` only inside test modules, so the
Halo figures above stand and only Firn is timed again. The question: does the
fix still remove the #431 work from current main on Firn's whole-program check.

The first attempt planned the first merge's five sides (`base` = pre-#431 main
`7d9118eba`, a twin of it, current main, the head before the second merge, the
merged head) and stopped in the source adaptation, before any timing
([run 39677484147](https://github.com/Ming-Research/Whitefoot/actions/runs/39677484147),
52 fixes then GRAM-10 `InvalidMatchFields`): Firn destructures `Inputs` with
every field listed, and main gave `Inputs` a `host_paths` field after the
pre-#431 compiler and after the head before the second merge. No source that
the merged compiler accepts is accepted by those two compilers, so a
branch/base ratio on one source is not measurable. The adapt step now also
appends the missing field with an unused binding, and this run's sides are the
two compilers that accept the adapted source. The ratio to pre-#431 main needs
a base column on a source without the bindings of the `Inputs` fields the
baseline lacks; the baseline run recorded after the third-merge re-measurement
below adds it.

Same method as the first merge's Firn run otherwise: five interleaved rounds,
compiler user+sys CPU, no check cache, then one work-counter run per compiler.
Branch is now current main plus this change alone, so the comparison with
main is a single-change one. Sides and what each result shows:

- `main` (`0c40731d3`, current main with the regression): the denominator.
- `twin`: the same `main` binary run again in each round; the median and
  min–max of its ratio to `main` are this run's noise. A twin median outside
  the min–max of its own five rounds around 1 is not noise, and the run is
  repeated.
- `branch` (the merged head): a median ratio to `main` whose five-round
  min–max lies wholly below 1 and the twin's does not shows the fix still pays
  on Firn; a range that straddles 1 or lies above it shows it does not, and the
  join pairs and closure cells (one counted run each) say where the work
  differs: counts at or above main's reject the claim that the fix still
  removes #431's work from Firn.

Result ([run 39682072749](https://github.com/Ming-Research/Whitefoot/actions/runs/39682072749),
EPYC 7763, workflow head `7d8d7e0e3` whose compiler tree equals the merged head
`10f74e7e8`; the source adaptation took 241 fixes, the first merge's 240 and the
`host_paths` field, and every exit code was 0):

| Compiler | rounds | median CPU s | ratio to `main` (median, min–max) |
|---|---:|---:|---:|
| `main` (`0c40731d3`) | 5 | 71.23 | 1.000 |
| `twin` (same binary as `main`) | 5 | 71.20 | 1.001 (0.993–1.004) |
| `branch` (merged head) | 5 | 58.52 | 0.822 (0.817–0.825) |

| Counter (one counted run each) | `main` | `branch` |
|---|---:|---:|
| `join:join` | 993,232 | 248,266 |
| `join:pairs_evaluated` | 7,448,963 | 6,304,847 |
| `closure:cells_sum` | 80,599,888 | 61,390,283 |
| `snapshot:count` | 100,464 | 46,752 |

The branch's five-round range lies wholly below 1 and the twin's straddles 1,
so on this source the fix still removes 17.8% of main's Firn check CPU (ratio
of medians), with 75% fewer joins, 15.4% fewer join pairs and 23.8% fewer
closure cells. The ranges are the min and max of five paired rounds, not
confidence intervals. The first merge's run recorded the same join pairs in
both columns (main 7,448,963; branch 6,304,847) and closure cells 1,110 lower
in both (80,598,778 and 61,389,173); intern calls differ by 1,290 and closure
rows by 210 in both columns as well. The shift is the same in both columns, so
it does not come from the fix; it is bounded above by what the one more binding
of the adapted source and the difference between the two `main`s add. This
run has no base column, so it does not give the ratio to pre-#431 main; the
figures 0.978 (before the first merge) and 0.678 (on the first merge) belong to
those heads.

### Re-measurement after the third merge of main (`643931102`)

Main advanced again (`7b11c52fb`) with changes under `semantic/check/`
(references, linearity, frozen, nominal instances, atomic) and `prelude`, none
under `semantic/entailment/flow`, `semantic/places` or `resolution/engine`. A
measurement counts only for the head that merges, so all three workloads are
timed once more in one run on this head. Firn's direct ratio to pre-#431
main is measured in the baseline run that follows this one. The pull request
head later took two more merges of main (`b50dc6e41`, `35fcd251a`), which
brought checker changes; the section "Re-timing on the compiler tree of the pull
request head" below times that tree. Method as before: five interleaved
rounds (three for `pkg::test`, main once), compiler user+sys CPU, no check
cache, one work-counter run per compiler, sources adapted by applying the
earlier runs' adaptation diffs and then the compiler's own mechanical fixes.
Sides and what each result shows:

- `base` (Halo only in this run; `7d9118eba` for `pkg::vm`, `d837678c6905`
  for `pkg::test`): the denominator of the acceptance rows. Branch/base at
  most 1.03 for `pkg::vm`, and at most 1.5 for `pkg::test`, meets them; a
  larger median rejects them.
- `twin`: the `base` binary run again (for Firn, which has no base column in
  this run, the `main` binary run again); the median and min–max of its ratio
  are this run's noise. A twin median outside the min–max of its own rounds
  around 1 is not noise, and the run is repeated.
- `main` (`7b11c52fb`, with the #431 regression): branch/main isolates the fix.
  A branch range reaching 1 or above rejects the claim that the fix still
  pays on this head.
- `branch` (the merged head): the work counters `join:join`,
  `join:pairs_evaluated` and `closure:cells_sum` of both compilers say where
  the work differs from main.

Result ([run 39693987787](https://github.com/Ming-Research/Whitefoot/actions/runs/39693987787),
workflow head `0f904ca1a`, whose compiler tree equals that of `c4d0dcbeb` and
`131338b85`, the heads before the last two merges of main; every exit code was 0; the three workloads ran on
different hosted CPUs, so only ratios within one workload are comparable):

| Workload (host CPU) | Side | rounds | median CPU s | ratio to `base` (median, min–max) |
|---|---|---:|---:|---:|
| `pkg::vm` (EPYC 7763) | `base` (`7d9118eba`) | 5 | 16.18 | 1.000 |
| | `twin` | 5 | 16.17 | 1.000 (0.996–1.002) |
| | `main` (`7b11c52fb`) | 5 | 11.56 | 0.715 (0.714–0.716) |
| | `branch` | 5 | 9.41 | 0.582 (0.579–0.584) |
| `pkg::test` (EPYC 9V74) | `base` (`d837678c6905`) | 3 | 1.58 | 1.000 |
| | `twin` | 3 | 1.57 | 1.000 (0.987–1.000) |
| | `main` | 1 | 453.80 | 285.409 |
| | `branch` | 3 | 1.61 | 1.025 (1.006–1.032) |

Both Halo rows meet their acceptance rows: `pkg::vm` branch/base 0.582 against
at most 1.03, `pkg::test` branch/base 1.025 against at most 1.5. `main` is
faster than `base` on `pkg::vm` because of changes between the two that are
not this work's; branch/main is 9.41 / 11.56 = 0.814 there. The Halo counters
(one counted run each; `main` was not counted for `pkg::test`):

| Counter | `pkg::vm` `base` | `pkg::vm` `main` | `pkg::vm` `branch` | `pkg::test` `base` | `pkg::test` `branch` |
|---|---:|---:|---:|---:|---:|
| `join:join` | 25,170 | 290,038 | 28,568 | 827 | 827 |
| `join:pairs_evaluated` | 1,122,732 | 1,554,023 | 1,176,621 | 641,730 | 565,698 |
| `closure:cells_sum` | 8,288,239 | 14,034,924 | 9,605,432 | 4,141,909 | 4,584,258 |

Firn in this run had no `base` column (EPYC 9V74, five rounds): `main` 84.25 s
and `branch` 73.28 s, branch/main 0.869 (0.866–0.876) against a `main` twin at
0.995 (0.983–1.008); joins 993,232 in `main` and 248,266 in `branch`, join pairs
7,448,963 and 6,304,847, closure cells 80,599,956 and 61,390,351. The branch
range lies wholly below 1 and the twin's straddles it, so on every workload the
fix still removes the #431 work from current main.

### Firn baseline column after the third merge of main

The acceptance row for Firn asks for the median ratio of the merged head to
pre-#431 main (`7d9118eba`) of at most 1.03, measured in a run on the head
that merges. The third-merge run above timed Firn without that column, because the
baseline compiler cannot check the adapted source (its `Inputs` lacks fields
the adapted source binds) and the column was judged unmeasurable until the
acceptance returned to the direct criterion; the missing column could not be
foreseen before the run. This run adds it, for Firn only, in one run on the
same hosted machine type: five interleaved rounds, compiler user+sys CPU, no
check cache, and one work-counter run per compiler.

The baseline compiler checks a copy of the adapted source from which the
bindings of the `Inputs` fields it does not declare are removed; the merged
compilers check the adapted source with them. Those bindings are the whole
difference between the two sources. The first attempt
([run 39694799784](https://github.com/Ming-Research/Whitefoot/actions/runs/39694799784))
removed only `host_paths` and stopped at the baseline compiler's check of that
source: GRAM-10 `InvalidMatchFields`, `Inputs` declared without `process_id`,
the field main gained with the process-id change merged in `7b11c52fb`. The
plan had assumed a single binding because the earlier adaptation reported
only the first missing field. The step now removes, from the line the baseline
compiler reports, every `unused_<field>` binding of a field its own diagnostic
does not declare, repeating for at most five rejections, fails unless the
baseline then accepts the source, and lists the bindings it removed. It also
applies the third-merge run's complete adaptation diff
([run 39693987787](https://github.com/Ming-Research/Whitefoot/actions/runs/39693987787),
186 fixes) instead of redoing those fixes, which gives the same adapted
source. Columns and what each result shows:

- `base`: `7d9118eba` on the source without those bindings, the denominator. A
  branch/base median of at most 1.03 meets the row; a larger median rejects
  it.
- `twin`: the `base` binary on the same source again; the median and min–max of
  its ratio to `base` are this run's noise, read as before.
- `main` (`7b11c52fb`, with the #431 regression) and `branch` (the head of the third merge of main),
  both on the adapted source with the bindings: the join, join-pair and
  closure-cell counters of the two columns show the work the fix removes. The
  bindings' own contribution is bounded by the shift of these counters, in both
  columns, from the first merge's run (source without any of the bindings) to
  this run, set against this run's branch column (figures below). The bound is on those work counters, not on CPU time; a branch/base
  median above 1.03 by less than what the counters leave room for would still
  be reported as above 1.03.

Result ([run 39715369711](https://github.com/Ming-Research/Whitefoot/actions/runs/39715369711),
EPYC 9V45, workflow head `131338b85` with the same compiler tree as `0f904ca1a`;
the step removed two bindings, `host_paths` and `process_id`, both on one line of
`firn/server/server.wf`, and the baseline compiler then accepted the source;
every exit code was 0):

| Compiler | rounds | median CPU s | ratio to `base` (median, min–max) |
|---|---:|---:|---:|
| `base` (`7d9118eba`, source without the two bindings) | 5 | 53.47 | 1.000 |
| `twin` (same binary as `base`) | 5 | 53.38 | 0.976 (0.963–1.034) |
| `main` (`7b11c52fb`) | 5 | 63.10 | 1.171 (1.139–1.245) |
| `branch` (third-merge head) | 5 | 54.89 | 1.027 (0.979–1.072) |

The branch/base median is 1.027, which meets the acceptance row's at most 1.03,
by a margin smaller than this run's noise: the twin's own median ratio is 0.976
with a 0.963–1.034 range, and the branch range 0.979–1.072 contains 1, so five
pairs detect neither a regression nor an improvement against pre-#431 main. The
spread is mainly the `base` column (51.64–56.10 s, 8.6% between its lowest and
highest round), while `branch` (54.14–55.37 s) and `main` (62.50–64.29 s) are
tight. Per-round branch/base: 1.072, 0.985, 0.979, 1.027, 1.054. The
regression itself is visible in the same run: `main` is 1.171 of `base`, and
branch/main is 54.89 / 63.10 = 0.870, the same as the 0.869 of the previous run
on a different hosted CPU.

Counters (one counted run each):

| Counter | `base` | `main` | `branch` |
|---|---:|---:|---:|
| `join:join` | 241,482 | 993,232 | 248,266 |
| `join:pairs_evaluated` | 7,477,212 | 7,448,963 | 6,304,847 |
| `closure:cells_sum` | 67,231,069 | 80,599,956 | 61,390,351 |
| `function:intern_calls` | 40,303,170 | 50,656,817 | 32,406,930 |
| `closure:rows_sum` | 2,220,141 | 3,207,658 | 2,372,719 |
| `function:affine_interval_warm_probes` | 0 | 216,036 | 216,036 |

Against `base` the branch counts 15.7% fewer join pairs, 8.7% fewer closure
cells, 19.6% fewer intern calls, 2.8% more joins and 6.9% more closure rows.
The affine-interval probes are work that the compilers after `base` perform and
`base` does not, in the same amount in `main` and `branch`, so they come from
other changes in main, not from this work. The counters give no sign of checker
work that the fix adds over `base`; they do not explain CPU time, and the 2.7%
median excess is within the noise above.

The bindings' contribution to these counters is bounded by the shift from the
first merge's run (source without any of the bindings; its `base` counters are
identical to this run's `base` counters, so the stripped source is that run's
source for the baseline compiler) to this run: closure cells +1,178 in both
columns (`main` 80,598,778 to 80,599,956, `branch` 61,389,173 to 61,390,351),
intern calls +1,384 in both (50,655,433 to 50,656,817; 32,405,546 to
32,406,930), closure rows +278 in both (3,207,380 to 3,207,658; 2,372,441 to
2,372,719), joins and join pairs unchanged. That is at most 0.002% of the
branch's cells, 0.004% of its intern calls and 0.012% of its rows. The shift
also contains whatever the `main` of the first merge and `7b11c52fb` differ by,
so it is an upper bound for the two bindings, and it is a bound on work
counters, not on CPU time.

### Re-timing on the compiler tree of the pull request head (`ef01fc5ca`)

The two runs above timed the compiler tree of `0f904ca1a` (equal to `c4d0dcbeb`
and `131338b85`). The pull request head then took two more merges of main
(`b50dc6e41`, `35fcd251a`). Between the timed tree and `ef01fc5ca`, `compiler/src`
differs in 46 files (+3,238 −133 lines), 14 of them under `semantic/entailment`
(among them `flow.rs`, `flow/frontier.rs`, `flow/walk.rs`, `flow/judge.rs` and
`flow/invariants.rs`, the code this change edits). An acceptance row counts a
result only from the head that merges, and the earlier timings are of another
compiler tree, so the three workloads are timed once more in one run on the compiler tree of the pull request head, from a temporary
workflow whose commit leaves `compiler/` as `ef01fc5ca` has it. The sides are
those of the two sections above, with the `main` column moved to the merge base
of the head and main (`1fbd6623d`), so that the pull request's change is the
only difference between `main` and `branch`:

- `base` (`7d9118eba` for `pkg::vm` and Firn, `d837678c6905` for `pkg::test`):
  the denominator of the acceptance rows. Branch/base at most 1.03 (`pkg::vm`,
  Firn) and at most 1.5 (`pkg::test`) meets them; a larger median rejects them.
  Firn's `base` checks the adapted source without the `Inputs` bindings it lacks,
  as above.
- `twin`: the `base` binary run again; the median and min–max of its ratio to
  `base` are this run's noise. A row whose margin is smaller than the twin's
  range is reported with that range, as the Firn row was, and read by its median.
- `main` (`1fbd6623d`, with the #431 regression): branch/main isolates the fix.
  A branch range reaching 1 or above rejects the claim that the fix still pays
  on this tree.
- `branch` (the compiler tree of `ef01fc5ca`): the counters `join:join`,
  `join:pairs_evaluated` and `closure:cells_sum` of the compilers say where the
  work differs from `main`.

Five interleaved rounds (three for `pkg::test`, `main` once there), compiler
user+sys CPU, no check cache, one work-counter run per compiler, sources adapted
by applying the earlier runs' adaptation diffs and then the compiler's own
mechanical fixes. This run's result is the one reported; it is repeated only if
a twin median lies outside its own min–max range or an exit code is not 0.

Result ([run 39725157249](https://github.com/Ming-Research/Whitefoot/actions/runs/39725157249),
workflow head `328e8ae95`, whose `compiler/` is that of `ef01fc5ca`; every exit
code was 0 and every twin median lay inside its own range, so nothing was
repeated; the three workloads again ran on different hosted CPUs, so only ratios
within one workload compare). The ratio column is the median over the rounds of
the per-round ratio to `base`, with the min–max over the rounds:

| Workload (host CPU) | Side | rounds | median CPU s | ratio to `base` (median, min–max) |
|---|---|---:|---:|---:|
| `pkg::vm` (EPYC 7763) | `base` (`7d9118eba`) | 5 | 16.48 | 1.000 |
| | `twin` | 5 | 16.46 | 0.999 (0.990–1.005) |
| | `main` (`1fbd6623d`) | 5 | 11.80 | 0.722 (0.716–0.731) |
| | `branch` | 5 | 9.56 | 0.583 (0.580–0.594) |
| `pkg::test` (EPYC 9V74) | `base` (`d837678c6905`) | 3 | 2.06 | 1.000 |
| | `twin` | 3 | 2.04 | 0.995 (0.981–0.995) |
| | `main` | 1 | 594.58 | 288.631 |
| | `branch` | 3 | 2.10 | 1.019 (1.014–1.029) |
| Firn (EPYC 9V45) | `base` (`7d9118eba`, source without `host_paths`, `process_id`) | 5 | 53.59 | 1.000 |
| | `twin` | 5 | 53.96 | 0.998 (0.987–1.017) |
| | `main` | 5 | 65.22 | 1.239 (1.166–1.248) |
| | `branch` | 5 | 57.78 | 1.067 (0.998–1.122) |

Against the criteria written above: `pkg::vm` 0.583 and `pkg::test` 1.019 meet
their at-most-1.03 and at-most-1.5 criteria. **Firn does not meet its at-most-1.03
criterion: the branch/base median is 1.067.** Its per-round branch/base ratios
are 1.122, 0.998, 1.039, 1.081 and 1.067, and the twin's median (0.998, range
0.987–1.017) shows a noise of about 2%, so the excess is outside the noise of
this run, unlike the 1.027 of the earlier Firn run, whose margin was smaller
than its noise (branch range 0.979–1.072, twin 0.976). The two Firn
measurements on trees one merge of main apart have overlapping ranges, and the
base column is the noisier one in both. `pkg::vm` branch/main is 0.808
(0.807–0.821, per-round); Firn branch/main is 0.866 (0.852–0.905), the five
per-round ratios being 0.905, 0.852, 0.890, 0.866 and 0.861; `pkg::test` runs
the branch 283 times faster than `main` once (2.10 s against 594.58 s).

Counters of the Firn compilers (one counted run each):

| Counter | `base` | `main` | `branch` | branch vs `base` | branch vs `main` |
|---|---:|---:|---:|---:|---:|
| `join:join` | 241,482 | 993,281 | 248,315 | +2.8% | −75.0% |
| `join:pairs_evaluated` | 7,477,212 | 7,453,179 | 6,309,063 | −15.6% | −15.4% |
| `closure:cells_sum` | 67,231,069 | 80,662,125 | 61,453,106 | −8.6% | −23.8% |
| `closure:rows_sum` | 2,220,141 | 3,211,049 | 2,376,122 | +7.0% | −26.0% |
| `closure:count` | 165,907 | 248,964 | 188,963 | +13.9% | −24.1% |
| `function:intern_calls` | 40,303,170 | 50,664,822 | 32,413,865 | −19.6% | −36.0% |
| `snapshot:cells_sum` | 30,793,300 | 39,311,968 | 22,613,478 | −26.6% | −42.5% |
| `function:affine_attempts` | 41,744 | 69,241 | 69,241 | +65.9% | 0% |
| `function:affine_interval_warm_probes` | 0 | 215,561 | 215,561 | new | 0% |
| `function:affine_interval_construction_probes` | 0 | 93,430 | 93,430 | new | 0% |

What these columns do and do not settle. They settle that the fix still removes
the #431 work on this tree: the branch counts a quarter of `main`'s
joins and 24% fewer closure cells, and runs Firn in 0.866 of `main`'s time. They
do not settle where the remaining 6.7% over pre-#431 main comes from, because
`base` and `branch` differ by every change in main since `7d9118eba` (65
first-parent merges), not only by this change. Four observations bear on the
attribution, none of them a measurement of it:

- The affine-interval counters are 0 in `base` and identical in `main` and
  `branch`, so that work comes from other merges in main; the branch's join
  pairs, closure cells, intern calls and snapshot cells are below `base`'s, while
  its closure count (+13.9%) and closure rows (+7.0%) are above it.
- #431 cost Firn 14.7% on the tree and Firn source of the first measurement
  (median ratio 1.147), so removing exactly that cost from `main` would give
  1 / 1.147 = 0.872 of `main`'s time here if the cost were unchanged; the
  observed 0.866 is at or below that, within the 2% noise.
- On the original tree this change measured 0.978 of pre-#431 Firn (run
  38420072424, third candidate), before the merges of main that followed.
- `main` itself moved from 1.171 of `base` (`7b11c52fb`) to 1.239 (`1fbd6623d`)
  between the two Firn runs, a rise of 5.8%, and the branch from 1.027 to 1.067,
  a rise of 3.9%: both rose with main's later merges.

A direct separation needs one more column, which this run does not have: the
compiler of the #431 merge commit `af7665be6`, whose first parent is `7d9118eba`
(`git log --first-parent 7d9118eba..af7665be6` lists only that merge), on the same
source as `base`. Then `af7665be6`/`base` is #431's own cost on this source,
`main`/`af7665be6` is what the 65 merges since add on top of it, and
`branch`/`main` is this change's effect, so that the 6.7% divides between them.
It is not run in this run; the next section plans it.

### Separating the cost of #431 from the other merges in main, on the head that merges

Written before the measurement. The previous section leaves open whether the
6.7% by which the Firn branch exceeds `base` (`7d9118eba`) is left by this change
or added by the 65 merges of main between `base` and the head. The architect
approved one more measurement on the head that merges, after the fourth merge of
main brought 85 more commits (the head's merge base with main is `48b94ccfc`),
with these columns on the same adapted source, in one run, five interleaved
rounds, compiler user+sys CPU, no check cache, one work-counter run per
compiler:

- `base`: `7d9118eba`, main just before #431, on the source without the two
  `Inputs` bindings it lacks (Firn), as before.
- `twin`: the `base` binary again; its ratio to `base` is this run's noise.
- `c431`: `af7665be6`, the merge of #431. Its first parent is `7d9118eba`
  (`git log --first-parent 7d9118eba..af7665be6` lists only that merge), so it is
  `base` plus #431 and nothing else. On the same source as `base`. Not run for
  `pkg::test`, whose criterion has a wide margin.
- `main`: `48b94ccfc`, the merge base of the head with main, with the #431
  regression and without this change.
- `branch`: the head that merges (merge base `48b94ccfc`), so that the only
  difference between `main` and `branch` is this change.

A column "main with #431 reverted" would be more direct than the product below,
and was tried first: applying the reverse of #431's `compiler/` diff
(`git diff af7665be6^1 af7665be6 -- compiler`, 20 files) to `48b94ccfc` with
`git apply -R --3way` conflicts in `semantic/check/ensures.rs`,
`semantic/range_judgment/walk.rs`, `semantic/range_judgment/walk/calls.rs`,
`semantic/entailment/flow/walk.rs`, `semantic/entailment/flow/postconditions.rs` and
`driver/pinned_repairs.rs`, and does not apply to
`driver/pinned_repairs/enum_routed_ensures.rs`. Merges since #431 changed the
same code, so there is no clean revert on this main, and a whole-merge revert
conflicts in the same hunks; the `c431` column is used instead.

Per round the run records `a` = `c431`/`base` (the cost of #431 itself on this
source), `b` = `main`/`c431` (what the merges since #431 add), `c` =
`branch`/`main` (this change's effect), `D` = `branch`/`base` (the acceptance row's
ratio), and `N` = `c` × `a` (the branch against the main that has #431's cost
removed, if that cost is the same on the later main). Medians over the rounds,
with min–max.

Decision rules, fixed now, for the Firn row (`pkg::vm` and `pkg::test` keep their
rows, `D` at most 1.03 and at most 1.5):

1. `D` ≤ 1.03: the row's own criterion is met; nothing else is needed.
2. `D` > 1.03 and `N` ≤ 1.03: this change removes at least what #431 added, and
   the rest of the excess comes from the merges since. The criterion of the row
   is the owner's; a card goes to the owner with the segment figures, and the
   row stays open until it is ruled.
3. `D` > 1.03 and `N` > 1.03: this change still leaves cost on Firn. The work
   counters are then profiled for the residual (closure count +13.9% and
   closure rows +7.0% over `base` in the previous run are the first suspects)
   before anything else.

A larger median than 1.03 rejects, as in the earlier rows; the run is repeated
only if a twin median lies outside its own min–max range or an exit code is not
0. `N` carries the noise of two ratios, about twice the twin's range, and is
read by its median.

`N` assumes that #431's relative cost on the later main equals its cost at
`c431`. Two checks on the work counters, which are deterministic, can refute it;
they are evaluated on Firn and on `pkg::vm`:

- **P1.** `function:affine_interval_warm_probes` and
  `function:affine_interval_construction_probes` are 0 in `base` and in `c431` and
  positive in `main` and `branch`. A positive value in `c431` means #431
  contains affine work, and the affine counters do not separate the two segments.
- **P2.** The counters #431 raised (`join:join`, `snapshot:rows_sum`,
  `closure:count`) move between `c431` and `main` by at most a tenth of the
  amount #431 added (`c431` minus `base`). A larger movement means merges since
  #431 changed the work #431 caused, and `a` does not carry over to `main`. The
  tenth is picked; the values are reported whatever they are.

If P1 or P2 is refuted, `N` is not used, the segment figures are reported
as they are, and the architect is asked again.

The run reuses the adapted sources of the earlier runs where they apply, and
additionally saves the adaptation of the Halo library checked out inside Firn
(`deps/halo-wf`, a submodule that the outer diff does not contain), which the
earlier runs redid every time.

### Reviews

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
candidate had no execution evidence at that review.

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
evidence for the third candidate; subsequent supplied CI is recorded above.

An independent `gpt-6.1-sol` review of the selector repair against `d6d486109`
covered the resolver, its identity test, perf-step removal, relevant language
and compiler decisions, and applicable checklist groups. It found no
functional defect within the corrected single-owner scope. Its minor finding
was an uncited open-work comment in the new test; that comment now states
only the existing behavior the test preserves, while this record describes
the unresolved broader boundary. The primary agent inspected the final
identity selection, PRE-1 ordering premise and unchanged proof consumer.
Compilation, the new resolver test, the gate and Halo timing remain unverified.

The temporary experiment follows the supplied bisect workflow's unbudgeted
measurement labels. It changes no maintained budgets and does not replace a
project gate. Inspection also found that the existing `compiler/build` label
in `compiler/Makefile` has no row in `.github/time-budgets.txt`; this unrelated
budget inventory gap is left unchanged here and recorded as a status-board
backlog item. The focused test-build label has its existing budget.
