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
about a 17% regression. Firn's effect is not yet measured.

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

## Implementation boundary

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
Interpret the ratios with that limitation.

Reject the claim that this fix explains the regression if excess snapshot
work does not decrease, or the fix/before excess remains outside the
observed twin noise range. Counter reduction without timing recovery supports
only a partial attribution. Print each round, medians and ratio spread rather
than treating a hosted runner as a precise performance qualification. A perf
profile of the after compiler supplies symbol attribution for costs the
entailment counters cannot distinguish.

## Validation and outstanding evidence

No compiler, build, test suite or measurement runs on the workstation.
Focused internal regression checks and existing enum-route tests are scheduled
in the temporary workflow; the project gate remains required before delivery.
No conformance source, verdict, specification or approval log is changed.
CI timings, source census, profiling and test outcomes are pending the owner's
commit and push. This record is updated with those results; the temporary
workflow is removed before the branch is ready.

An independent read-only review with `gpt-6.1-sol` covered the complete
uncommitted change against the initial fix checkout, the new workflow and this
record, ENT-5, the checker-facts decisions, and the directly affected fact-state
and kill/join consumers. It found no functional defect within that scope.
Repository layout, citations, case ownership, gate isolation and design
correspondence passed source inspection. Acceptance, execution and performance
remain unverified until CI; this review is not delivery approval.

The temporary experiment follows the supplied bisect workflow's unbudgeted
measurement labels. It changes no maintained budgets and does not replace a
project gate. Inspection also found that the existing `compiler/build` label
in `compiler/Makefile` has no row in `.github/time-budgets.txt`; this unrelated
budget inventory gap is left unchanged. The focused test-build label has its
existing budget. No status-board connector was available in this session, so
the inventory finding could not be filed there.
