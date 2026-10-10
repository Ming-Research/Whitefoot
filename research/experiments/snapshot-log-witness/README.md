# Reconciled batched export: pre-registered protocol witness

## Question and status

Can current Whitefoot express a batched export of a live
`ConcurrentHashMap<u64>` whose reconciliation equals the map at its recorded
**end sequence S1**, while another context performs sets, increments, deletes
and reinsertions? Does plain scan plus command replay fail on the same trace?

Registered before execution, on `claude/snap-log-witness`, based on
`a69d5afc7d37f65dd34e58366ba9ad5c01b15a2c`. No compilation or execution has
been performed for this registration. The programs are proposed witnesses,
not established compiler capabilities. CI must settle acceptance and results.
There is no specification, compiler, conformance-verdict or gate change.

This implements the expressibility experiment selected by decisions 1–3 of
the [consistent-snapshots investigation](../../investigations/consistent-snapshots/README.md#owner-decisions),
especially [the fuzzy-scan objection](../../investigations/consistent-snapshots/README.md#routes-and-their-costs).
It does not implement a deployable persistence system or the later comparative
resource/latency experiment.

## Expected outcomes and rejection criteria

The CI harness compiles each variant once, then runs **N = 20** instances of
each, alternating correct and wrong. The first three instances of each are
small cost probes included in N; their wall-time spread is printed before
the remaining instances. An unexpected probe outcome stops the extension.

- Correct reconciliation must have **zero mismatches**. Exit 70 is a mismatch
  in either membership or value and rejects correctness.
- The wrong control must mismatch at least once in N runs. In fact its
  initial rendezvous predicts a mismatch on every completed valid run:
  S0 precedes an increment of key zero, and scanning follows that increment.
  Key zero is never subsequently assigned or deleted. Replaying all its
  suffix increments therefore adds at least one twice. Failure to observe
  a control mismatch rejects the experiment's ability to separate the routes.
- At least one correct run must observe a larger journal sequence at its
  last scan batch than at its first. Otherwise consistency was checked but
  interleaved traversal remains untested: the harness reports an inconclusive
  failure, not support for concurrent export. `map_scan`'s count is a hint,
  so multiple batches and this coverage condition require observation.
- Exit 0 means equality with a commit between scan batches; exit 10 means
  equality without that observation. Exit 71 means an invalid experiment
  (log capacity, integer overflow, unexpected key width, or writer failure),
  never a successful comparison. Other exits, signals and timeouts are
  infrastructure/runtime failures. The harness prints all exit codes and
  separate correct/control mismatch counts, and fails on unexpected outcomes.
- A source rejection of a required operation is an expressibility finding
  to minimize before continuing. Preserve the compiler's complete diagnostic,
  exact rule and source extent; write the smallest complete rejected program
  and explain the needed rule or implementation change. In particular, inspect
  entry-plus-journal atomicity, scan-plus-metadata observation, shared-state
  proofs and spawned ownership. Do not replace any of them with another
  spelling, a runtime check, external locking, or changed expected outcome.
  A syntax mistake is an authorship defect; a specified valid operation that
  the compiler refuses is a compiler gap; an operation the specification
  actually excludes is a language gap. Timeout, crash or link failure alone
  establishes none of those source-language conclusions.

## Representation and trace

Integer keys are encoded as exactly one unsigned byte. The live map has
`u64` values; the journal's 256-element `last_write` array stores per-key
sequence metadata beside it, including the last deletion's sequence. This
uses the stated finite key domain, not an assumed scan-position interface.
All mutations participate in the same `mutate` operation.

The journal owns `Slots<Record, 2048>`. Its `len` **is the shared sequence
counter**, initially zero: each mutation takes `len + 1`, updates the map
entry and its stamp, and appends `(key, sequence, Option<u64>, command)` in
one atomic statement. `None` is a tombstone. No counter duplication, log
truncation, or wraparound is needed. The command field is used only by the
negative control. Append order and sequence order are identical.

The root initializes keys 0–31 to zero through `mutate`, starts the writer,
records S0 and releases the writer's start guard. The writer increments key
zero and signals readiness. The root then scans; after the first batch the
writer is free to run eight rounds. Each round assigns, increments, deletes
and reinserts each key 1–31, then increments key zero. The writer finally
deletes key 31. There are 32 initial writes and 1,002 writer mutations, for
1,034 records at completion, below the fixed reserve. S1 may precede writer
completion; the join happens after reconciliation.

The start/readiness/first-batch guards arrange the minimal double-count
witness and prevent all churn from finishing before the first batch. They
do not order subsequent batches against individual mutations. Runs use two
requested context drivers and one compute worker, with no `--par` or CPU
affinity restriction. No sleep or OS lock supplies protocol atomicity.

Capacity refusal is explicit and fails the experiment before changing an
entry. `+checked` defines a total increment operation with an overflow error;
that error also fails the experiment without changing an entry. Neither
changes the successful protocol. The prescribed small values and write
count do not reach either outcome. These are initial operation contracts,
not replacements introduced after a rejected proof.

## Protocol steps and current rules

Line numbers below refer to `spec/kernel-spec.md` at the registered base.
The specification is the authority; the listed cases show analogous source
forms and are not substitutes for this witness's CI result.

| Step in `protocol.wf` | Expression and rule |
| --- | --- |
| Retain shared state | `shared_map_new`, `shared_new` and `shared_share`; SHARE-1, lines 2267–2273. `Shared` and `ConcurrentHashMap` are PRE-1 types, lines 2354–2361; `map_scan` and sharing functions are PRE-1 declarations, lines 2509–2530, rather than source functions in `lib/std`. `lib/std/process/module.wfm` supplies `ExitStatus`. |
| Publish one mutation | `mutate` targets one map entry and the journal together. SHARE-2, lines 2285–2309, permits the different state types in one statement; SHARE-3, lines 2311–2313, gives all their changes one point and one order. OP-10, line 1189, and PRE-1 append contracts govern log growth. |
| Start and concurrent writer | `spawn writer` moves retained handles into a waiting function. WAIT-1, lines 1533–1538, and WAIT-3, lines 2256–2265, admit this and define the join. WAIT-2, lines 2246–2253, supplies context order and conditional progress, with no latency bound. |
| Record S0 | One journal statement reads `records.len` and opens the start guard. SHARE-2/3 prevents an initial writer mutation from straddling this observation. |
| Scan a batch | One whole-map-plus-journal statement calls `map_scan`, copies each returned key's `Option<u64>` and its `last_write` stamp, then releases both. SHARE-1, lines 2275–2277, defines advancing cursor extents and enumeration; SHARE-2, line 2301, permits `table^[key]` below a whole-map target. SHARE-3 makes enumeration and both reads one observation. No reference escapes the block (SHARE-2, line 2304). |
| Record S1 and oracle | One whole-map-plus-journal statement reads the counter, copies the bounded log prefix, and reads every possible key directly into `reference`. SHARE-3 puts these at the same cut, even if the writer continues afterward. |
| Reconcile | For each owned record with `S0 < sequence <= S1`, replace the exported `Option<u64>` only if its sequence exceeds the retained sequence for that key. Tombstones retain their sequence too. Older/equal records cannot roll back a scanned value or a newer replayed record. This is ordinary owned state and comparisons (OP-1, line 924; OP-4, line 1061). |
| Compare and report | Compare presence and value for all 256 possible keys, including absent keys. PRE-2's exit status, line 2975, reports 70 on any difference. Wrong mode replaces the sequence comparison and after-image assignment with command execution over the scanned value. |

Relevant existing examples read for this source are
`tests/conformance/cases/share-pos-map-entry-beside-an-object.wf`,
`share-pos-map-scan-beside-other-contexts.wf`,
`share-pos-map-scan-across-statements.wf`, and
`share-pos-map-whole-target-entries-over-set.wf` in that same directory.
The runner/workflow follow the source-bundle invocation and CI setup of
`research/experiments/context-starvation/run.sh` and
`.github/workflows/ctx-starvation.yml` on `origin/claude/ctx-handoff`.

## Why the end cut is reconstructible

A key with no write in `(S0, S1]` has unchanged membership and value throughout
the scan. Advancing scan extents cover its fixed position once, so a present
entry is copied with that value. A key changed in the interval has a latest
after-image or tombstone in the captured suffix. Every scan observation is
before S1; taking the highest sequence among that observation and the suffix
therefore selects exactly its S1 state. Deleted and reinserted keys obey the
same reasoning, including a key absent when its position was scanned. This
deduction relies on every mutation being logged at the same atomic point.

The reference is independent of that deduction's implementation: it reads
the actual map for all possible one-byte keys under the S1 whole-map hold.
It does not replay commands, choose log maxima, call `map_scan`, or inspect
the exported result or metadata to construct expected values. All writers
only create one-byte keys, so these 256 presence/value comparisons establish
exact equality over this program's entire key domain. This oracle checks the
export protocol, not independently the map runtime's own implementation.

## CI and limits

The temporary workflow runs on pushes only to `claude/snap-log-witness` on
`ubuntu-24.04`: fetch locked dependencies, `make -C compiler build`, then
`sh research/experiments/snapshot-log-witness/run.sh`. The compiler path is
`compiler/target/gate/whitefootc`. The workflow records the actual revision
and host, compiler diagnostics, every outcome and small-sample time spread
in an uploaded artifact. A compile failure stops the harness immediately;
no stale executable is run. Remove this workflow **before any pull request**.
It adds no dependency to the canonical gate or daily research checks.

Still unverified: source acceptance, native execution, interleaved-batch
coverage, equality in all correct runs, and control sensitivity. No language
gap or exact compiler rejection has yet been observed; do not label a
prediction as one. No local build, compilation, test or commit is authorized
for this task.

Pre-execution review: an independent Codex/GPT-6 agent inspected all six
added artifacts against the registered base, the relevant design ancestors,
specification and checklist groups A/D/T/V plus G/DC. No build, compiler,
test or validation command ran. The review found and rechecked two repaired
authoring defects: an early return would have forced WAIT-3's join before
scanning, and the constructor explicitly moved a copy array. Errors are now
recorded until the explicit join; the copy array is passed bare. The review
also reread canonical-syntax repairs and reported no unresolved findings
within its inspection scope. This is source review, not execution evidence.

The witness bounds its key domain and trace, retains the complete small log,
and takes a deliberately expensive full read only for the oracle. It makes
no latency, memory-efficiency, general-key, multi-writer, multi-map,
multi-key-transaction, expiry, nested-owner, durability, cancellation or
bounded-cleanup claim. Production reserve/abort, streaming and reclamation
remain the contracts of the parent investigation; a pass here establishes
only this finite end-cut protocol witness, not the route's comparative merit.
