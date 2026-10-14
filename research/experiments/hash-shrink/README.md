# HashMap shrink hosted exploration

This explicitly invoked bundle serves the
[registered investigation](../../investigations/hash-shrink/DESIGN.md).
The temporary [workflow](../../../.github/workflows/hash-shrink-hosted.yml)
is manual-only and must be removed before its work branch becomes ready.
Keep this bundle while the investigation needs reproduction; retire its
executable assets when that need ends. It is independent of the canonical
correctness gate. No build, check or measurement runs on a workstation.

## Dispatch and sizing

Dispatch `hash-shrink-hosted.yml` on the work branch with `phase=pilot`.
New experimental stages do not yet have CPU-model budget rows derived from
20 recorded runs. The workflow defaults to refusing execution. If the owner
rules that this temporary manual investigation may collect data with
report-only budgets, select `allow_unbudgeted_experiment=true`; the artifact
records the choice. This is an explicit proposed exception pending that
ruling, not a claim that the ordinary budget requirement is satisfied.
The default baseline is the exact study commit
`8f6a273ca53d6d1a2df56a35ab6093ff3a7165b1`, before automatic removal shrink.
Inspect the `hash-shrink-pilot` artifact: process wall durations, per-cell
minimum/median/maximum, paired intervals, compiler/library/source identities,
and Firn preparation durations. A failed build or a missing/malformed row
provides no timing evidence.

After that inspection, dispatch `phase=panel` with `pilot_run_id` naming that
successful run and `rounds` selected from its duration and spread. The prior
pilot must use the exact same baseline, candidate revision and bundle sources;
a source change requires a new pilot. The panel rebuilds all native images
and runs a new small pilot on its current host. It restores only prior pilot
evidence, never prior native binaries. Both library and Firn runners record
their sizing choice and refuse predicted campaigns over 30 minutes. That
refusal is a request to inspect scale, not a slowdown verdict. Library sizing
uses total process wall, linearly scaled by population with a factor of two;
this is conservative exploratory sizing, not a precise cost model.

Library `rounds` means independent blocks in **each** of two cohorts. Firn
`rounds` means total blocks split into two cohorts. The workflow accepts 4
through 20 and defaults to 6. Both cohorts rotate baseline, identical-build
twin and candidate; the library's second cohort reverses the rotation.
Each cell starts a fresh process or server. The original fixed inputs and
the scale selections remain in the artifact.

The manual experiment follows the report-only wrapper behavior of the
existing compute-bench placement experiment. Every heavy stage uses
`.github/run-check.pl`, including the ordinary `compiler/build` label.
After that explicit selection, temporary experiment labels print budget
comparisons without a budget verdict record; no permanent measured budget is
invented or raised. Deadlines bound
a stuck command and define no language or performance result.

## Measured panel

`generate.py` expands `benchmark.wf.in`; `run.py` calls the Makefile to emit
and link each identical generated source with the baseline or candidate's
embedded library and production runtime. Default sequential lowering,
`clang -O2 -falign-functions=64`, one driver, allocator and inputs remain fixed.
The twin uses a byte-identical copy of the baseline compiler and independently
emits/links an image from the same source. It is an identical-build noise
control, rather than a second independent compiler build. Compiler revisions,
compiler hashes, embedded HashMap module source hashes, image hashes and raw
and optimized LLVM are retained for insertion/growth path inspection.

The single-table library panel crosses these representations and workloads:

| Representation | Workloads |
| --- | --- |
| u64 key / unit value | Insert from zero, presized insertion, hit lookup, sparse random-scan pop, counted full drain |
| Owning eight-byte key / unit value | The same five workloads; keys own a box containing their bytes |
| u64 key / owning 32-word value | The same five workloads; values own a box containing 256 bytes |

Each representation also runs the three grow-only workloads with two disjoint
owning tables inside the **same process**, using one identical `--par
--par-call-grain off` image with `WF_WORKERS=1` and `WF_WORKERS=2`. A two-iteration
loop gives each iteration its own clock view, input owners and map, and writes
its observation into a distinct fixed-array slot. The raw row retains both
per-map internal intervals, their maximum, maximum per-table capacity and
total process wall; no sum is presented as parallel wall time. One worker's
setup can overlap another's timed interval, including shared-allocator work;
all arms use the same conditions. Selecting two workers alone does not prove
the loop ran concurrently. The emitted LLVM is retained; actual offering and
scheduling remain unverified until inspected in CI.

The pilot uses 1,024 entries, 128 sparse survivors and 32 sparse pops. The
panel uses 1,000,000 distinct integer IDs, 1,000 sparse survivors and 100 sparse
pops. The presized table starts at two buckets per entry. Owner inputs, query
copies and initial table construction precede the timed interval; destruction
follows it. Insertion and lookup have separate intervals. The insertion trace
transfers prebuilt pairs in descending ID order. Every inserted key and wide
value is checked outside timing; lookups also check their independent expected
sum. The random-scan picker starts at a deterministic pseudorandom bucket and
wraps to the first filled bucket, copying the selected key before removing it.
Sparse preparation is outside timing. Pops include returned owner cleanup and
the independent removal-ledger instrumentation; survivor lookups are outside
timing. These are library traces, rather than network or lock-hold timings.
Eight-byte owning keys model a byte-key representation and do not claim to
reproduce Firn's variable-length strings or exact hash cost.

`firn.py` builds unchanged Firn commit
`c6f6058fb17277d4d44d364a45efad1eae01a22b` and its pinned submodules with all
three compilers, using Firn's full-LTO graph build. It measures fresh-set SADD,
depth-one sparse SPOP, a predicted shrink-threshold crossing, a counted full
drain, unchanged LRANGE_100, and a concurrent request overlapping sparse SPOP,
on one and two server cores. Its command inputs, placement, source/submodule
identities and reply checks are recorded. Client-observed latency includes
network and client work. SCARD overlap is evidence of a concurrent request,
not an internal hold-time measurement. A single threshold/full-drain request
per block supplies a tail observation; per-block p99 remains unqualified when
there are fewer than 100 requests.

`qualify.py` selects six existing maintained fixture functions by replacing
only their final `main`, and compiles the exact same selected sources with
baseline and candidate. It requires baseline assertion exit 1 and candidate
exit 0 for the million-to-thousand bound, hysteresis, empty reuse, regrowth,
ceiling/growth behavior and owning shrink. Compiler rejection, signal exit or
unexpected output fails the qualification rather than counting as the
required before-failure. Generated copies, exact statuses and wall durations
are retained. The smallest selected case runs before the million-entry case.
This research driver changes no formal expected result or gate selection;
the complete maintained fixture remains in ordinary CI.

## Evidence limits and artifacts

Hosted runs are exploratory. They do not meet the registered idle-14900K
condition, prove strict zero grow-only slowdown, or establish application
compatibility. The library reducer reports paired log ratios with deterministic
nonparametric bootstrap 95% intervals for each cohort and a flag when the
registered twin median falls outside [0.99,1.01] or its interval width exceeds
0.02. A flag calls the comparison unresolved; no ratio selects a job pass.
An interval overlapping 1.00 is only no detected regression at that resolution.
The Firn reducer states its own sample resolution in its JSON.

The bundle does not instrument maximum internal atomic-hold time, verify AOF replay or all Firn
consumers, or inspect emitted code automatically. Those registered obligations
remain unverified until their owning sessions provide evidence. Membership and
command-reply checks here protect the measured traces only.

Artifacts contain `host/` tools, CPUs, dispatch and source revisions;
`library/` generated source, LLVM, runtime construction, binary identities,
per-process logs, raw TSV, cohort summaries and scale choice;
`qualification/` selected maintained sources, build logs and raw assertion
statuses; and `firn/` server builds, immutable source identities, RESP inputs,
per-request raw JSONL, placement, logs, summaries and scale choice. A timeout
or incomplete command retains whatever was written and fails the workflow.
Preserve any result relied on in the investigation, since hosted artifacts
expire.
