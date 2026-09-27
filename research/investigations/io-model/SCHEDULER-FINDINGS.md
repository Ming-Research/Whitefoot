# Scheduler experiment findings

This is the digest of a 68-experiment investigation into the network and compute
gap between Whitefoot programs and hand-written native references. The
experiments ran on a family of `codex/io-*` work branches on 2026-09-06 and
2026-09-07 and accumulated in one 10,318-line notebook,
`research/investigations/io-model/SCHEDULER-EXPERIMENT.md`, on
`codex/io-runtime-followup`. That notebook was never merged; this file is what
it established, so the branches can go without the reasoning going with them.

Most experiments moved a mechanism inside the unified compute/completion
scheduler the tree retired on 2026-09-10 — see [mcts_mem/whitefoot/parallelism.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/parallelism.md),
"2026-09-10 (6816e9bd) boundary", and [mcts_mem/whitefoot/system-interface.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/system-interface.md),
"2026-09-10 boundary". Compute joins now run on ordinary native thread stacks,
there is no switchable-stack pool, the managed-stack enumerator is gone, and
suspended user-call connection concurrency is temporarily unsupported. A result
about a ready queue, a stack pool, a pinned owner ring or a park-on-miss wake
describes no code that exists, and is kept only where it closes a door. What
survives is substantial: the language-design negatives, the emitted continuation
lowering and its qualification, the measurement apparatus, and a panel of native
references — io_uring, epoll, C++ coroutines, Go and Rayon — built and
hash-pinned here and nowhere else.

One correction belongs at the front. `RESULTS.md` and
[mcts_mem/whitefoot/system-interface.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/system-interface.md) both named reaper-local scheduling, a
per-thread ready list and a steal from idleness as the next performance work on
this line. Experiment 1 built and measured exactly that and found no throughput
gain. The prediction is retracted in place in `RESULTS.md`; this file holds the
measurement that retracts it.

**Reading conventions.** Each paragraph states the question, the setup in one
sentence, the numbers the notebook reports, and a conclusion. Numbers are quoted,
never recomputed. A *paired ratio* is the median of same-pass candidate/reference
ratios within one cohort on one host, and the bracketed interval after it is the
observed minimum and maximum of those pairs, never a confidence interval. A
median of rates and a median of ratios are different statistics. Absolute rates
from different hosted runners are never compared.

**Verdicts.** **holds** means the finding is about something the tree still has —
a language rule, the completion model, the emitted continuation lowering, a
measurement method, a native baseline. **superseded** means it is about the
retired runtime and is kept as a closed door, with the reason to keep it closed.
**unresolved** means the experiment did not settle its question, and the verdict
says what would. Sources give a branch and commit pair plus the notebook
heading; most are `codex/io-runtime-followup@6de4557c`, the file-level and
content-level superset of the family, and where a result exists only on an
earlier head that head is named. Hashes are given because the branch names are
deleted: every one of the family's sixty-one heads, and `codex/compute-runtime`,
is a parent of the archive commit on `archive/codex-io-2026-09-11`, so each
cited pair resolves from that one branch, while the per-experiment measured
revisions quoted inside paragraphs are text here, not reachable objects.

## Scheduler core, ready queues and cooperative checkpoints

**Experiment 1 — does the worker that resumes a parked stack matter?** Three
runtimes were linked from one emitted program: a global FIFO, per-worker FIFOs
preferring the parking worker, and per-worker FIFOs preferring the enqueuing
worker, with the shared mutex, wake epoch, ring and stack representation fixed.
At four connections and 64-byte payloads the preferences give paired throughput
0.851 [0.750, 1.028] and 0.809 [0.671, 1.361]; at 64 and 1024 connections the
medians are 0.991/0.995 and 1.000/1.000. The mechanism did move — cross-worker
resumes at 64 connections fall from 55.3% under the global queue to 38.1% under
parking-worker preference and rise to 64.5% under the other — and server CPU at
four connections rose from 20.375 to 25.750 and 24.375 microseconds per trip.
Locality moved and throughput did not follow; the same cohort's io_uring and
epoll references ran at 146,367 and 167,946 trips/s against 133,282.
*Verdict:* **superseded** — the queue is gone, but the door stays shut: queue
preference alone is not the fix, and placing work on the reaping worker's queue
does not even keep the connection there. `codex/io-runtime-followup@6de4557c`,
*First measurement: 2026-09-06* (revision `b714ced7`).

**Experiment 2 — is the idle window starving completions?** Six idle policies
(256-spin/16-yield base, sleep, short, spin, progress every look, progress every
sixteen looks) ran across four CPU placements. Progress every sixteen looks
gains 1.154, 1.553 and 1.549 paired rate at one peer under shared4, shared2 and
split2 and loses at 0.794 under split1; at 64 peers all five variants sit within
0.987..1.003 of base while base/uring stays near 0.77. The oversubscribed
compute control decides it: at eight workers on four logical CPUs the six forms
take 1,608.14, 2,483.25, 2,404.11, 2,047.37, 2,407.28 and 2,007.09 ms.
*Verdict:* **superseded** — the idle window belonged to the unified core; the
door is that a fixed idle policy cannot be selected from a low-occupancy cell
without paying the oversubscribed compute control.
`codex/io-runtime-followup@6de4557c`, *Completed idle-policy comparison*
(revision `06d93a46`).

**Experiment 3 — does a long compute call block unrelated connections?** A mixed
protocol was added in which every fourth connection requests a dependent 64-bit
recurrence of up to 2,097,152 rounds and the rest request zero, against an epoll
reference performing the same recurrence inline. At 64 peers under shared4 the
paired WF/C rate is 0.801 at zero compute and 1.415 and 1.401 at 262,144 and
2,097,152 rounds, with light p99 30,457 us against 36,953 at the middle cost.
*Verdict:* **superseded** — the overlap came from the retired scheduler; what is
retained is the workload, since a mixed light/heavy protocol is required before
any mixed-load claim and an inline reference is a control, not the best native
design. `codex/io-runtime-followup@6de4557c`, *Third experiment: network service
while connections compute*.

**Experiment 4 — is the tail delayed first service or worker occupancy?**
Admission was added so every connection completes one zero-compute request
before timing, and a native C reference with explicit continuations at quanta
1024/16384/65536 was built to compete. At 64 peers, 2,097,152 rounds and
admitted split2, WF light p99 is 32,138 us against the quanta's 358, 780 and
2,070, while WF/quantum throughput is 1.126 [1.003, 1.496], 1.118 and 1.119. The
burst's light/heavy active-span ratios — 0.942 for WF against 0.008, 0.019 and
0.042 — show why aggregate throughput cannot rank the forms.
*Verdict:* **holds** — the admission control and the cooperative-quantum native
reference are apparatus, and the rule that a scheduler must not look faster by
serving more cheap work is method. `codex/io-runtime-followup@6de4557c`, *Fourth
experiment: admission and cooperative compute reference* (revision `4f951acc`).

**Experiment 5 — can a still-private completion record skip the handshake?**
`WF_COMPLETION_LOCAL_INLINE=1` wrote the result head and released DONE directly
for records not yet offered to an engine. Median local/base paired rates across
four placements and five cases lie between 0.989 and 1.022, the 64-peer shared4
cell consistently 1.006 in a 1.001..1.009 range; local/uring stays at 0.852 in
shared4 and 0.864 in split2, and split2 server CPU moves from 11.563 to 11.406
microseconds per trip.
*Verdict:* **superseded** — a measurement of the retired handshake; the door is
that the private-record shortcut is a small qualified optimization and not where
the completion cost is. `codex/io-runtime-followup@6de4557c`, *Fifth experiment:
completing a record that is still private* (revision `95ba7202`).

**Experiment 6 — do both classes stay active long enough to compare?** A
common-duration admitted mode kept every connection issuing to one deadline and
reported both class rates over the same interval. At 64 peers and 2,097,152
rounds under split2, WF serves 1,530 light and 478 heavy requests/s while
quantum 1024 serves 267,366 light and 67 heavy; paired native heavy-rate ratios
against WF are 1.003 inline, 0.141, 0.619 and 0.858 at the three quanta. The
zero-compute split2 control has WF at about 245,515 requests/s against inline
C's 320,451, at 7.29 against 6.17 server CPU microseconds per request.
*Verdict:* **holds** — methodological and runtime-independent: aggregate
requests/s and CPU/request are not comparable when the served mix changes, and a
request-weighted percentile hides a slow connection, which is why per-peer
minimum counts and worst-peer tails were added.
`codex/io-runtime-followup@6de4557c`, *Sixth experiment: keeping both request
classes active* (revision `4e874daa`).

**Experiment 7 — can the compiler insert checkpoints without an annotation?**
`--par --sched-quantum N` added a private counter and an always-inline helper on
natural-loop backedges, with a new enumerated schedule S24 covering the yield
transition at 1.15 and 2.81 million states with zero bounded executions. At 64
peers and 2,097,152 rounds under shared4, light p99 falls from 258,431 us at
base to 576, 939 and 2,691 at intervals 1024/16384/65536 and the minimum
light-peer count rises from 11 to 3,297/2,376/1,358. Pure computation pays:
median wall at four workers is 1,162.02 ms at base against
1,533.98/1,450.12/1,453.03, about 25% even at the larger intervals and under 1%
at two workers.
*Verdict:* **superseded** — the runtime checkpoint it calls is gone; the
language door it opens is restated under experiment 12.
`codex/io-runtime-followup@6de4557c`, *Seventh experiment: compiler-inserted
cooperative loop checkpoints* (revision `62b626c1`).

**Experiment 8 — how much footprint is unused lane storage?**
`WF_SCHED_INIT_USED_LANES=1` cleared only the configured lanes of a core whose
64-lane array is 20,478,032 bytes at a 319,520-byte stride. At 64 peers and 64
bytes, median peak RSS falls from 35,712 to 18,672 KiB in shared4 and 35,760 to
18,472 in split2; at 1024 peers the pairs are 79,204/62,512 and 79,732/61,900.
Paired throughput across four placements and five cases stays in 0.922..1.028
with ranges crossing one.
*Verdict:* **superseded** — the 64-lane core is part of the retired design; the
retained distinction is that reserving a lane and making its storage resident
are different costs, and the remaining 1024-peer footprint is not startup
storage. `codex/io-runtime-followup@6de4557c`, *Eighth experiment: initialize
only configured worker lanes* (revision `87585b8e`).

**Experiment 9 — what does a fixed offered load show that a closed loop
cannot?** Each light peer received 20/100/500 scheduled requests per second from
a common origin, latency measured from the scheduled arrival and every planned
request drained and verified after timing. At 2,097,152 heavy rounds on split2
the base completes 572, 571 and 572 heavy requests/s at 960/4800/24000 light
arrivals/s while its light p99 runs 26,988, 635,639 and 941,338 us with 4, 3,007
and 22,176 requests pending at the deadline; checkpoint interval 16384 holds
560/544/464 heavy completions at light p99 1,272/1,039/576 us, with paired WF/C
heavy-rate ratios 1.000, 0.989 and 0.951.
*Verdict:* **holds** — the protocol is the method: latency beginning at the
intended send time, explicit dispatch-delay reporting, retained backlog and a
drain phase are what stop a slow server from improving its percentile by
suppressing demand. `codex/io-runtime-followup@6de4557c`, *Ninth experiment: fix
the light request arrival schedule* (revision `609e4437`).

**Experiment 10 — is the pure-compute tax the runtime call or the emitted
code?** An observed build at four workers and interval 16384 reported zero
checkpoint calls and zero switches while still losing time, so the per-iteration
counter was replaced by `--par --sched-chunks N`, an IR transformation of
unsigned unit-stride natural loops comparing the index against
`min(upper, saturating_add(start, N))`. Median pure-compute wall at four workers
is 1,347.32 ms for base, 1,886.52 for the counter and 1,519.90/1,442.73/1,442.81
for chunks at 1024/16384/65536; at two workers chunks16384 matches base within
0.1%. The counter costs about 40% at four and eight workers and chunking reduces
the excess to about 7%, with no network gain: paired chunks16384 against
counter16384 heavy rate on split2 is 1.006 [0.952, 1.059].
*Verdict:* **holds** — a code-generation result about an emitted loop: a
per-iteration decrement and branch the optimizer keeps is a measurable tax even
when the runtime is never entered. `codex/io-runtime-followup@6de4557c`, *Tenth
experiment: keep checkpoint bookkeeping out of the inner loop* (revision
`32220011`).

**Experiment 11 — does preferring completion-ready stacks improve the tail?**
A bounded burst budget split record-completion and voluntary checkpoint
resumptions into two FIFOs under the existing mutex. At the highest arrival rate
the paired B=1 and B=8 light-p99 ratios are 2.218 [1.440, 4.277] and
2.093 [1.431, 3.109], a regression in every pass, with heavy-rate medians
1.000/1.007; at the lowest rate B=8 improves light p99 in every pass at 0.859.
Untimed runs recorded 4,089 preferred against 3,548 forced selections for B=1
and 4,093 against 388 for B=8.
*Verdict:* **superseded** — the class and its FIFO were removed; the door is
that a fixed completion-class preference is load-dependent and reverses between
offered rates. `codex/io-runtime-followup@6de4557c`, *Eleventh experiment:
bounded preference for completion-ready stacks (retired)* (revision `6380a17a`).

**Experiment 12 — does loop topology explain the residual no-call tax?** Local
assembly showed the first chunk representation forming its inner cycle around
the checkpoint path and losing the metric-table loop's two-iteration unrolling,
so the chunk loop was given its own header with the original latch as the only
inner backedge. Median wall at two/four/eight workers is
2,957.18/1,609.99/1,678.21 ms for base, 3,248.81/1,764.58/1,839.89 for the former
16384 topology and 2,955.18/1,616.22/1,681.94 for the canonical one: within 0.4%
of base at every worker count against about 10%. Paired canonical/former
heavy-rate ratios on split2 are 1.040 and 1.033 at 4,800 arrivals/s and 1.048
and 1.022 at 24,000.
*Verdict:* **holds** — the most transferable result on the branch: the large
no-call compute tax was an artifact of where the transformation put the cycle,
not evidence that a source-visible suspension colour is needed, and it went away
without changing a signature or a permission judgment.
`codex/io-runtime-followup@6de4557c`, *Twelfth experiment: give the chunk loop a
separate header* (revision `f6b80173`).

**Experiment 13 — are the shared diagnostic counters contended?** Up to 64
counter pairs 128 bytes apart were provided, each host thread taking a stripe on
first update. Paired striped/base rate medians are 1.0030/0.9984/1.0008/0.9995
at 64 peers and 0.9988/1.0023/1.0004/0.9972 at 1024, every cell with samples on
both sides of one. At 64 peers on split2, server CPU is 11.250 against 11.172
microseconds per trip while native io_uring is 9.844, throughput near 163k
against 192k.
*Verdict:* **superseded** — counters and stripes are gone; the door is that a
specific diagnostic-contention explanation was built, measured and rejected.
`codex/io-runtime-followup@6de4557c`, *Thirteenth experiment: retain counters
without one shared write location* (revision `9479d624`).

**Experiment 14 — does the uniform stack-top page offset cost anything?** Each
header and initial stack pointer was moved down by an index-selected offset in
128-byte steps. Paired spread/base rate medians are 1.0068/0.9987/1.0000/1.0032
at 64 peers and 0.9993/1.0086/0.9885/1.0002 at 1024, while residency moved the
wrong way: peak RSS at 64 peers on split2 goes from 36,248 to 39,892 KiB and at
1024 from 79,220 to 85,720.
*Verdict:* **superseded** — the pool layout is retired; the door is that address
spreading is not free, and a layout change raising residency without a paired
timing gain is not a candidate. `codex/io-runtime-followup@6de4557c`,
*Fourteenth experiment: spread stack-top offsets* (revision `bc748c30`).

**Experiment 15 — was experiment 1's negative caused by the shared lock?** Three
forms were compared — the global FIFO, per-worker FIFOs under one mutex, and the
same FIFOs under independent 128-byte-separated mutexes — so the paired
difference between the last two isolates lock assignment. Paired ratios to the
original FIFO are 0.9810..1.0042 at 64 peers and 0.9866..1.0150 at 1024. At
split2/64 the independent-against-shared comparison is 1.0003 [0.9982, 1.0053];
base, independent and epoll run at 212,646, 213,390 and 237,208 trips/s at
8.672, 8.594 and 7.969 server CPU microseconds per trip. At one peer under
shared4 independent locks beat the shared-lock queues by 1.4948 while reaching
only 0.9387 of the original FIFO.
*Verdict:* **superseded** — both variants were removed; the door is precise,
that splitting a lock can repair overhead extra queue scans introduced without
improving the original design. `codex/io-runtime-followup@6de4557c`, *Fifteenth
experiment: independently locked worker ready queues (retired)* (revision
`2f946878`).

**Experiment 16 — do ordinary sequential call stacks impose the gap?** The
native epoll reference gained a sequential handler per connection, with receive
offsets and loop state as automatic locals across suspensions, the same context
switch the runtime used, and a guarded 64 KiB stack per descriptor slot. With
one server worker on a different physical core from the client, paired
stackful/manual echo ratios are 0.9962, 0.9946, 0.9995 and 0.9972 at 1/4/64/1024
small peers; at 64 peers rates are 128,833/128,932 requests/s at identical 7.734
microseconds of CPU per request. The fixed-arrival comparison is sharper: at
16384 steps the sequential/manual heavy-rate ratios are 1.0000 (0.9812..1.0191)
at 4,800 arrivals/s and 0.9976 at 24,000, and under split1 the two forms had
exactly equal heavy capacity in every pass. The cost is memory: 5,804 against
1,864 KiB of peak RSS at 1024 peers.
*Verdict:* **holds** — evidence against attributing the runtime gap to ordinary
sequential call stacks, which is a statement about the language's chosen source
form and not about any scheduler. `codex/io-runtime-followup@6de4557c`,
*Sixteenth experiment: sequential functions on an owner-local event loop*
(revision `001262a3`).

<a id="tcp-packet-policy-comparison"></a>

**Experiment 17 — is the 41 ms large-payload tail the language or the
transport?** `WF_TCP_NODELAY=1` set the option once on POSIX listeners and
outgoing sockets, read back on listener, connected and accepted descriptors
before timing. At 64 peers and 64 KiB the same program and runtime change p99
from 41,667 to 2,410 us in shared2 and 41,625 to 2,390 in split2 — paired p99
0.0579 and 0.0578, rate 1.0304 and 1.0680 — the shared2 ratio staying in
0.0563..0.0657 in all seven pairs. The reverse native control shows the
interaction is engine-dependent: io_uring without the option runs at about 1,562
requests/s with a 41..42 ms p99 in every large-payload placement and gains
11.78x to 14.98x paired throughput when enabled, while epoll shows no collapse.
Small payloads gained nothing (0.9941..1.0073, ranges straddling one), and
split2 large-payload CPU stayed at 64.375 microseconds per request against
native epoll's 39.375.
*Verdict:* **holds** — packet policy must be controlled before a tail is
attributed to the language or a continuation model. The bench sources already
set the option; the magnitude and the reason are recorded nowhere else.
`codex/io-runtime-followup@6de4557c`, *Seventeenth experiment: align TCP packet
coalescing policy* (revision `74e72d3c`).

**Experiment 18 — do persistent ownership and a worker-local ring compose?** Two
changes were separated: resume a suspended stack only on its parking worker, and
submit through that worker's own Linux ring. Independent rings with migration
still allowed improve two-worker throughput in every paired sample — split2
rings/base 1.1411 [1.1334, 1.1429] at 64 peers and 1.1214 at 1024 — while
worsening tails at p99 1.1491 and 1.4139. Pinning is much worse: combined
owner/base rates 0.5890 [0.5299, 0.9262] and 0.4796, with the untimed shared2
64-peer combined run making 2,013 kernel waits and 2,017 host wake writes against
the baseline's 136 and 139. The paced cohort decides it: every multi-worker
paired heavy-capacity ratio is below one, split2 chunk+owner/chunk is 0.4615 and
0.3765 and shared4 0.2353 and 0.2195, while aggregate exchanges/s barely move
(23,758.6 against 23,683.7 at split2/24000) as heavy deadline completions fall
from 352 to 128.
*Verdict:* **superseded** — pinning, owner rings and the stealable hand-out are
retired. Two doors stay shut: opportunistic initial stealing plus permanent
affinity is not a next runtime choice, and aggregate throughput conceals a
three-quarter collapse of per-class capacity.
`codex/io-runtime-followup@6de4557c`, *Eighteenth experiment: persistent
continuations and worker-owned rings* (revisions `2c5d7f94`, `7421a258`).

## Wake and idle

**Experiment 21 — can a running owner skip its own wake?** The broadcast was
omitted when a ready stack is pushed to a pinned queue whose owner is already
executing, identified by a thread-local core pointer rather than by ordinal
alone. The one underoccupied cell it helps is large: quiet/balanced at one peer
under shared4 gives paired throughput 1.3841 (1.1475..1.5739), p99 0.6796 and
median server CPU 48 against 195 microseconds per exchange. At concurrency it
does nothing: paired rates 1.0091, 0.9931, 1.0067 and 1.0203 across the split2
cases, and against the native quantum reference the quiet form loses every
light-tail pair at both arrival rates (medians 1.9390 and 1.6495) with heavy
capacity 0.9894 and 0.9615. The zero-compute split2 control still shows 1,827 us
light p99 against native 187, every paired tail ratio worse, median 8.4251;
untimed runs confirm 67 wake writes for balanced against two for quiet.
*Verdict:* **superseded** — pinned queues are gone; the door is that reduced
wake counters are not a timed win and this was qualified only for underoccupied
owner pools. `codex/io-runtime-followup@6de4557c`, *Twenty-first experiment: omit
a running owner's redundant wake* (revision `1776d1af`).

**Experiment 22 — does a completed-I/O service budget buy fairness?** A progress
opportunity was added after N completed I/O joins on one worker. Persistent
worker cadence regresses high-load echo relative to balanced: paired rate
0.9545 (0.9273..0.9696) at 64 peers and 0.9361 at 1024, with p99 1.7254 and
1.7336. Per-turn 16 is often inactive — untimed split2/64 observations report
zero I/O checkpoints for it against 256,258 for per-turn 1 and 16,016 for
persistent 16. All three candidates lose every native-quantum light-tail pair at
both arrival rates, and the zero-compute control's median p99 is 2,144 us for
balanced and 2,725/2,256/2,192 for the candidates against native quantum's 87.
Finite compute stays within about 0.4% of balanced.
*Verdict:* **superseded** — the budget applied to the retired join path; the
door is that a per-join opportunity is not a substitute for understanding
admission, event service and continuation order, and raising progress counts
does not imply better tails. `codex/io-runtime-followup@6de4557c`,
*Twenty-second experiment: completed-I/O service and progress budgets* (revision
`c35a6ef2`).

<a id="windows-notification-replay"></a>

**Experiment 24 — is the Windows stall a lost IOCP notification?** The stall
seen when a Windows scheduler job timed out at its 20-minute limit was
reproduced in an isolated probe: the notify path posts one unaddressed packet
per announced sleeper while the park path consumes a received wake
unconditionally, so a newer park capturing the new epoch can take an older
sleeper's packet. The baseline replay reports `wake-replay expected=2 received=0`
and exits 1 at the wake-count assertion. The repair records native wait calls in
an intrusive list under the existing wait lock, publishes once per previously
unnotified node, and routes a new park to the runtime condition until the
notified cohort drains; at `04106a23` the native Windows job reports
`expected=2 received=2` on both `gate=0` and `gate=1` replays and the scheduler
job passes all 32 pinned/four-worker repetitions and all 18 staged IOCP socket
configurations.
*Verdict:* **holds** — the only correctness fix in the family rather than a
rejected policy, and it is now in the tree:
`io/salvage-iocp-uring` carries the repair into
`compiler/src/backend/completion/windows_iocp.c`, so `wf_windows_return_wake`
preserves a packet for a notified cohort rather than for a parked count. Its
replay harness was not carried, so what exercises the repaired path on the
current stack is the `completion-windows` job's native adapter probe rather
than an on-demand reproduction of the old interleaving.
`codex/io-windows-wake-fix@04106a23` (reproduction at
`codex/io-windows-wake-experiments@3821ce73`), *Twenty-fourth experiment: Windows
wake ownership under re-entry*.

## Client and service

**Experiment 29 — is the load generator's pump the cause of the tail?** The
client's per-connection pump was capped at eight completed round trips or one,
with the continuation queued in an owner-local FIFO and the zero-budget build's
optimized LLVM required to match the unchanged source. Budget eight never
exhausted in any of its 210 timed rows while budget one yielded 9,473,361 times,
so only budget one is a control. It lowers every split1 zero-compute balanced
tail pair at median 0.6541 and every native elided tail pair at 0.6520, but
long-compute split2 still loses every balanced tail pair to native at median
1.8277 with heavy capacity essentially unchanged, and the zero-compute split2
dispatch-wait median stays at 1,472 us for WF against 54 for native. The
apparent throughput convergence is partly a native loss: budget-one/original
native rate is 0.9661 (0.9438..0.9852), losing every pass.
*Verdict:* **holds** — budget one is retained as a measured client control, and
the negative is about the generator: capping consecutive exchanges within a pump
does not establish the cause of dispatch waiting.
`codex/io-runtime-followup@6de4557c`, *Twenty-ninth experiment: bounded client
service* (revision `f42ebdd8`).

**Experiment 43 — does the client have spare capacity?** One server worker was
held on one logical CPU while the client changed from one worker on another
physical core to two workers on that core's SMT siblings. Every small-message
paired rate range includes 1.0; both native uring large-message rates improve in
all pairs but only by median 1.5% and 2.9%, and pure-ring p99 becomes 1.77 to
2.49 times worse. The WF large-message medians are not stable: `callee-small`
has five single-client samples around 21..22k trips/s and two around 36..38k,
the faster ones also dropping server CPU from about 46 to 26..28 microseconds
per trip, while two-client samples cluster at 23..24k while occupying nearly two
client logical CPUs; epoll's single-client rates range 20.7k to 39.1k.
*Verdict:* **unresolved** — no demonstrably idle generator exists for all forms
on this VM. Settle it with additional independent physical client cores or a
separate client host and a qualified client-engine comparison; an extra SMT
sibling is neither free nor proof of headroom.
`codex/io-client-headroom@0ebe924b`, *Forty-third experiment: client CPU headroom
before server saturation claims*.

**Experiment 56 — what is the client actually doing?** Worker-owned accounting
was added for every socket call, size bin, poll batch and event flag, with
conservation checks after timing. Every unprofiled row makes exactly one
successful full send per round with zero short sends and zero send EAGAIN, and
the receive side is almost entirely full-message transfers with roughly one
empty probe per round (EAGAIN per round 0.999883..1.004688). The client uses
0.9795..0.9999 CPU per wall second. Large-message system CPU per round is
23.030/23.398/41.075 us against epoll and 43.869/43.866/43.675 against uring-64k
in pass order: nearly identical syscall counts with materially different CPU.
All four attached profiles are *incomplete: unknown_symbols* — 194/196 and
196/198 sampled small-message callchains contain unknown frames, and the large
captures have 257/274 and 280/292 chains with only kernel addresses.
*Verdict:* **holds** — the honest coverage statement is the finding: full
caller-based kernel attribution is not qualified on this host, counts alone do
not identify the expensive kernel path, and the original failure policy was kept
rather than relaxed to obtain a profile. `codex/io-client-diagnostic@bc119e9a`,
*Fifty-sixth experiment: observe the client syscall and readiness work*.

**Experiment 58 — does removing the speculative receive probe help?** One
per-connection read-event mask was kept so a delivered read edge stays
actionable across a partial send and is cleared only by EAGAIN or a complete
response. All sixteen small-message readiness cells reach zero receive EAGAIN
against about one per round in their controls, with receive calls per trip
falling from 2.0000 to 1.0000. Timing is mixed: native uring at 64 small peers
loses all five rate pairs (0.9648 [0.9539, 0.9831]) with p99 ratios
1.0251..3.1556, while the same server gains in all five pairs at 4 and 1024
peers (1.0569 and 1.1319). At 64 KiB all four servers still sit at 0.996..1.000
client CPU per wall second before and after, with client system CPU near
19.0..19.6 microseconds per trip against 2.2..2.8 of user CPU.
*Verdict:* **holds** — the readiness client is retained opt-in, and the negative
is about the generator: removing the empty probe does not solve the observed
large-transfer client limit. `codex/io-client-readiness@063cbef4`, *58. Remove
speculative client receive probes*.

**Experiment 58, fixture half — can a qualification fixture dominate gate
time?** The canonical scheduler job reached its eight-minute limit inside the
first observer client's 8 MiB fixture, each fragmented case taking about 12.25
seconds; the bounded follow-up changed only the fixture's requested server
`SO_SNDBUF` from 4,096 to 65,536 bytes while keeping every case, the payload,
the fragment limits and every short-transfer assertion. The same-host comparison
with identical observed binaries gives 12.52 and 12.54 seconds on the old
fixture against 0.17 and 0.19 on the new one; both new fixtures still encounter
partial sends and receives and send EAGAIN at counts 634/1,970 and 686/1,955,
all 36 retained counter phases conserve totals, and all 96 socket-case results
have the expected shapes.
*Verdict:* **holds** — a fixture's own socket configuration can dominate gate
time, and the repair weakened no assertion; the frozen measurement revision kept
its original fixture. `codex/io-client-fixture@e424b800`, *Qualification fixture
capacity follow-up*.

**Experiment 60 — how much does client width change the answer?** Four distinct
reported ARM cores were admitted (raw package and core identities, disjoint
sibling lists, visible quota, initial and final topology equality) and the
server held on one while the client pool widened to one, two and three of the
others. Every large-message wider/base rate pair improves — epoll
1.4364 [1.3970, 1.5755] at width two — while all four 64-peer small-message
cells regress in all five passes at both wider settings and `callee-small` falls
to 0.6960 at 1024 small peers. The consequence is the finding: at 64 peers and
64 KiB the same-pass server/epoll ratios reverse across widths, from
1.0420/1.0813/1.0351 for uring-64k, `callee-small` and `wf-coro-index` at width
one to 0.8330/0.8237/0.7905 at width two, all five passes reversing; epoll's
medians are 40,246.9/57,954.2/56,053.1 trips/s at widths 1/2/3. The mechanism is
in the client's counters: for epoll's large cell, events per nonempty wait fall
62.825 → 1.168 → 1.033 while waits per trip rise 0.01591 → 0.91031 → 1.00816,
and native 64-peer small cells go from roughly 54..61 events per nonempty wait
to about 1.35..1.41 at width two.
*Verdict:* **holds** — the most consequential methodological result in the
family: every earlier same-host ranking taken under a width-one client is
suspect, the default client stays unchanged, and client width and architecture
become explicit comparison axes. `codex/io-client-capacity@d241cf7d`, *60. Client
capacity on reported independent ARM cores*.

## CPU controls and the Rayon gap

**Experiment 45 — is the short-run WF/Rayon gap fixed cost or sustained loss?**
The same emitted program ran at twelve and 1,100 reserved stacks against the
frozen Rayon reference at one, four and sixteen batches, with 26 artifact and
source hashes verified against retained files or exact revisions. Paired
WF/Rayon wall is 1.1391 [1.1266, 1.1644] at one batch and 1.0156 at sixteen with
twelve stacks, and 1.2081/1.0206 with 1,100; paired 1,100-minus-12 differences
are 22.31/27.20/29.60 ms of wall and 24.43/28.47/32.26 ms of CPU at 1/4/16
batches — a bounded absolute cost across sixteen times the work.
*Verdict:* **superseded** — the pool it varies is gone and the stack-count
setting is inert. Two things keep: batch length is how a fixed cost is separated
from a sustained parallelism loss, and the roughly 20% single-batch gap is not a
20% parallelism loss on this workload. `codex/io-cpu-resource-controls@60073e1d`,
*Forty-fifth experiment: control CPU stack capacity and fixed costs*.

**Experiment 47 — is unused native ring initialization the short-run cost?** The
same binary was paired with the native-ring control unset and set, after
observations confirmed the default build initializes a ring carrying zero
submissions, zero submission enters and zero completions. Default-minus-disabled
paired medians are -0.123 ms wall and -0.133 ms CPU at one batch and +6.036 and
+7.812 at sixteen, the direction changing across passes at both lengths; paired
default/disabled wall is 0.9997 [0.9833, 1.0034] and 1.0010. Mechanism
qualification succeeded rather than silently comparing two fallback rows: the
default observations emit a ring report with zero traffic and the disabled ones
emit none.
*Verdict:* **holds** — the bridge still initializes a completion ring for a
program whose only I/O is its final write, so the negative applies; the
four-bracket phase plan it motivated (init to worker arrival, submission to
status-post, status-post to exit-group, exit-group to reap) is the reusable
method. `codex/io-cpu-ring-controls@e71ec278`, *Forty-seventh experiment: control
unused native ring initialization*.

**Experiment 50 — where in the process does the short-run time go?** Configured
lane initialization was measured as an ordinary control and a coarse uprobe
trace taken at seven runtime entry symbols. The ordinary rows give no stable
benefit: used-lanes-minus-default medians are +3.634 ms wall at one batch and
-8.924 at sixteen, the sign changing across passes. The first trace was rejected
rather than used: system-wide write events included the recorder's own, 335,616
of 338,471 decoded lines in one capture, and its body-to-submit intervals of
430.523 and 426.745 ms exceed the unprobed whole-process median of 310.030 ms.
The corrected recapture places the time inside the body: init entry to first run
0.646..1.870 ms, submission to post-status 0.199..0.221, post-status to
`exit_group` 0.027..0.072 and `exit_group` to reap 0.435..0.728, against body
intervals of 304.545..313.012 ms. In the first 100 ms Whitefoot accumulates
273.5..295.6 thread-ms of group running time against Rayon's 393.2..397.0, with
individual threads accruing 58.5..67.7 ms runnable off CPU.
*Verdict:* **holds** — the corrected phase attribution is method, and rejecting
the contaminated trace rather than letting it stand is the part worth copying;
the used-lanes policy itself belongs to the retired core.
`codex/io-cpu-startup-controls@8dd5b44a` for the ordinary rows and rejected
trace, `codex/io-cpu-phase-recheck@c81ac477` for the correction; *Fiftieth
experiment: configured-lane initialization and process boundaries*.

**Experiment 61 — is parallel tree initialization the mismatch?** One call in
the emitted module was changed from the parallel builder to the already emitted
sequential clone, the substitution reversed and the whole IR compared byte for
byte, so the control also moves worker creation from the first build pair to the
first layout pair. Paired candidate/default wall is
1.010129 [0.997610, 1.016587] at one batch and 0.997319 at sixteen, against a
same-binary control whose own spread is 0.999434 [0.991685, 1.016073] and
0.997114. The publication accounting is exact: steals plus owner inline
executions sum to 100,863 at one batch and 1,612,863 at sixteen, which is 63
publications per layout times 1,600 layouts plus the default builder's 63.
*Verdict:* **holds** — publication, owner-inline execution and lazy worker
startup all still exist, so the negative applies, and so does its correction of a
tempting misreading: owner inline executions are successful publications
executed by their owner, not failed acquisitions.
`codex/io-cpu-build-control@dc88c433`, *61. Control parallel tree
initialization*.

**Experiment 63 — does a coarser fork depth help?** Four parallel levels of both
layout functions were specialized into sixteen copied bodies and thunks so each
layout makes 15 publications instead of 63, matching the frozen Rayon grain's
join count. Paired candidate/default wall is 1.013568 [1.013112, 1.022401] at
one batch and 1.000606 at sixteen; short-task CPU is higher in all five pairs by
1.064..1.686% and total context switches rise in every pair at both lengths. The
intended change is confirmed — steals plus owner inline runs fall from 100,863
to 24,063 at one batch and 1,612,863 to 384,063 at sixteen — and the honesty note
is transferable: this is not an identical-code control, with ELF `.text` growing
from 49,266 to 56,690 bytes and the 4,160-byte `.rodata` moving from 61,440 to
69,632.
*Verdict:* **holds** — publication count is still a compiler policy the tree
owns, and the warning that an IR-copy grain control also changes specialization
and layout applies to any future grain experiment.
`codex/io-cpu-grain-control@a5cf6389`, *63. Control layout fork depth*.

**Experiment 64 — does the supported stack floor help?** The same executable ran
at twelve and five stacks, five being the floor at four computing threads.
Paired S5/S12 wall is 1.007120 [0.978888, 1.051702] at one batch and 0.998386 at
sixteen, inside the same-binary duplicate's own spread; voluntary switches fall
in every pair (0.711864 and 0.800855) while involuntary switches rise in every
pair (4.600671 and 2.847880), and no-target compute join turns rise from zero to
8,336 as parks fall from 4,154 to 1,910. Native readback gives 1,073,741,824
bytes per stack on a 1,073,745,920-byte stride, so pool virtual reservation
drops from 12,884,951,040 to 5,368,729,600 bytes while ordinary paired peak-RSS
deltas are only +820 KiB and +128 KiB with mixed signs.
*Verdict:* **superseded** — there is no pool to size; the retained lesson is
general, that seven GiB of removed virtual reservation is not a resident-memory
saving. `codex/io-cpu-stack-floor@543bf48c`, *64. Control stack capacity at the
supported floor*.

## Baselines against native, Go and Rayon

**The baseline matrix and evidence levels.** The most reusable prose in the
family is not an experiment but a standing table of what each measurement class
can establish, added when the in-tree epoll control had been described as the
fastest possible implementation. It separates the *competitive* comparison, in
which each backend uses its best qualified storage and scheduling policy within
one resource envelope, from the *diagnostic* comparison, which holds engine,
storage, compiler or scheduler fixed and changes one factor — noting that a
64 KiB io_uring provided buffer and a 64 KiB epoll worker scratch share a
transfer ceiling and not an ownership model. Its evidence ladder is explicit:
**audited** (source and configuration understood), **qualified** (the shared
protocol oracle passes), **screened** (same-host paired measurements exist),
**confirmed** (tuning frozen before independent confirmation), **NIC-qualified**
(a separate machine and physical network repeat it), each attaching to an exact
revision and configuration rather than to a language or library name. Its
resource axes state the limits that recur throughout: whole-process CPU is
centisecond-quantized and includes startup and drain; closed-loop p99 is not an
overload service level; occupied echo cells consume nearly all assigned client
CPU; equal provided bytes does not imply equal resident memory; an observer is
never part of a timed binary; seven alternating passes are screening evidence,
not confirmation after tuning.
*Verdict:* **holds** — nothing in it depends on the retired runtime, and it is
the checklist any future comparison on this line should meet.
`codex/io-runtime-followup@6de4557c`, *Baseline matrix and evidence levels*.

**Experiment 36 — is the in-tree io_uring reference strong enough?** A source
audit found two invariant defects in `uring_echo.c` — a fixed 64-buffer
per-connection queue that used payload size to bound TCP fragment count, and a
`ring_enter` clearing pending submissions even when an interrupted or short enter
had not consumed the published SQEs — and the repair gave the queue one link per
provided buffer and reread submission debt from the shared SQ head. The forced
backpressure qualification then reached queue depths of 152 and 248 against the
old 64-entry assumption. At equal reserved payload storage the larger chunk
wins: 64 KiB against 8 KiB provided buffers gives paired throughput
1.1898 [1.0951, 1.2135] on split1 and 1.1841 on split2, server CPU per trip
0.8372 and 0.9049 and p99 0.5215 and 0.6059, improving in every paired
large-message sample, while all eight small-message ranges cross 1.0.
*Verdict:* **holds** — a native baseline and a correctness repair to it,
independent of any Whitefoot runtime, and the reason the earlier epoll-only
panel cannot be read as a complete native target. `io/salvage-iocp-uring`
carries both defects' repairs and the selectable provided-buffer size into the
tree's `uring_echo.c`, with `make uring-check` deciding the queue, re-arm and
retirement invariants without a kernel or a network.
`codex/io-native-baselines@0357259d`, *Thirty-sixth experiment: strengthen the
native completion reference* (screen at `475008b5`).

**Experiment 38 — is an immediate send a stronger native control?** One
nonblocking `sendmsg` was attempted per send arm before falling back to the
ordered ring send. At 8 KiB provided buffers this is a severe regression: paired
hybrid/pure rate 0.4251 [0.4171, 0.4342] on split1 and 0.4604 on split2, with
server CPU per trip 2.3529 and 2.2917 and large p99 2.6075 and 2.4346. At 64 KiB
it is neutral, rate and p99 ranges crossing 1.0. The same branch corrected a
shutdown loan without a completion proof — a worker can observe global `finished`
and exit before consuming its eventfd READ completion, after which successful
cleanup frees the operation's wake storage — and, separately, a missing
store-to-load barrier on the unmeasured SQPOLL path between publishing the SQ
tail and reading the wake-needed flag, matched against upstream liburing.
*Verdict:* **holds** for the send-policy screen and the shutdown-loan
correction, both native-reference facts; the SQPOLL barrier is a correctness
fact of the same file and `io/salvage-iocp-uring` carries it, together with the
inline-send configuration and the shutdown-loan correction, into
`research/experiments/io-completion-bench/uring_echo.c`.
`codex/io-native-baselines@0357259d`, *Thirty-eighth experiment: native
completion receive with immediate send*.

**Experiment 40 — what is a credible CPU-parallel reference?** A standalone Rust
crate ported `tests/programs/par_layout.wf` preserving floating-point expression
order, fused multiply-adds, traversal order, child-result grouping and node
writes, with `rayon::join` for disjoint siblings, one pool created once, and
grain calibrated on a separate cohort and frozen before confirmation. The Linux
confirmation medians are 1,320.81 ms for sequential Rust, 670.37 and 354.04 ms
for Rayon at two and four workers, and 1,329.70/707.68/425.54 ms for the
matching Whitefoot forms; paired WF/Rayon wall is 1.025992, 1.056016 and
1.202192 at widths one, two and four with total-CPU ratios 1.025329, 1.029694
and 1.045739. CPU per wall second is 3.454 for Whitefoot at four workers against
Rayon's 3.974, and the sequential medians differ by about 0.7% — which is what
separates extra CPU work from missing overlap. The reference is hash-pinned:
binary `2a8289d8…`, `main.rs` `86f4e33f…`, `Cargo.toml` `7c11ba05…`,
`Cargo.lock` `49a17a42…`.
*Verdict:* **holds** — a frozen, independently implemented, hash-pinned CPU
reference with its own qualification protocol, unrelated to which runtime
executes the Whitefoot side. `codex/io-rayon-baselines@d79ffaf1`, *Fortieth
experiment: independent Rust and Rayon CPU reference*.

**Experiment 42 — what does a mixed external reference look like?** A separate
binary served the same 64-byte recurrence protocol with one Tokio
current-thread I/O driver and B-1 Rayon workers under one total execution
budget, admitting nonzero requests through Q semaphore permits that bound queued
plus executing CPU jobs and are released after result publication rather than
after the socket write. Its Linux qualification passed all four ordinary and
observed runs with every planned light request drained: at two threads one CPU
worker completed 256 submissions with 599 light requests handled while all CPU
workers were active, and at four threads three workers completed 756 with 582,
both ending with zero active, inflight or acquiring work.
*Verdict:* **unresolved** by its own statement — small shared-host smoke samples
are not a fair comparison, because the client shares unrestricted host CPU and
there is no tuning or confirmation cohort. Settle it with frozen queue and
driver settings, the I/O thread counted inside total budgets two and four,
separately established client capacity, and paced light tails, heavy
completions, drain and CPU/RSS against the qualified native and Whitefoot forms.
`codex/io-mixed-rayon-baselines@040bfc4b`, *Forty-second experiment: async TCP
with bounded Rayon CPU offload*.

**Experiment 49 — is there an ordinary sequential-API baseline?** A Go echo
server with one goroutine per connection, a private initialized 64 KiB buffer
and sequential reads and writes was qualified on native Linux against the same
stream oracle, with Go 1.27.1 pinned by release tag and archive digest, no
modules, release and race builds separated, and the two audited runtime sources
hashed (`netpoll_epoll.go` `cd94172e…`, `internal/poll/fd_unix.go` `675f74e8…`).
All four live snapshots show one CPU mask on every thread and one epoll
descriptor with 65 registrations; release builds at GOMAXPROCS 1 and 4 use three
OS threads and 13,288 and 15,336 KiB RSS with zero GC cycles, while race builds
use five and seven threads and about 42 MiB. Disassembly of the CI release
artifact places the 65,536-byte buffer in the handler's stack frame with
explicit zeroing, which is why exit `TotalAlloc` is only 238,640/288,272 bytes.
*Verdict:* **holds** — a protocol-qualified external reference with its resource
behaviour understood, plus the correction that GOMAXPROCS names execution
concurrency, not the thread count or the CPU allocation.
`codex/io-go-baselines@ccf667fd`, *Forty-ninth experiment: qualify a sequential
Go net reference*.

**Experiment 51 — which Go buffer ownership is the fair form?** The same loop
was compiled with the buffer created in the handler's stack-owning wrapper or
allocated by the acceptor and moved into the goroutine, with escape diagnostics
and the CI artifact's disassembly required to confirm each site. At 64 live
peers the heap form reduces the live RSS snapshot from 13,292 to 11,824 KiB at
GOMAXPROCS 1 and from 15,344 to 11,836 at 4 — 11.0% and 22.9% — while moving the
allocation into the GC heap: exit `TotalAlloc` is 4,437,664/4,512,304 bytes
against 240,688/290,640 for the stack form, with one GC cycle. Both own the same
4 MiB aggregate capacity, and race instrumentation moves both buffers to the
heap, so race memory cannot stand in for the release comparison.
*Verdict:* **holds** — a qualified lower-residency reference form, with both
release forms retained for a later fixed-resource calibration.
`codex/io-go-storage@56102eac`, *Fifty-first experiment: compare Go buffer
ownership*.

**Experiment 53 — where does the sequential Go form sit in the fixed screen?**
Both Go storage forms at GOMAXPROCS 1 and 4 joined the sixteen-form echo panel
on one server CPU with the client on another physical core, the measured release
binary byte-identical to the qualification copy. At 64 small peers native uring,
`balanced-small`, `batch32` and Go acceptor/P1 run at 218,940, 215,084, 197,841
and 183,046 trips/s with 4.453/4.688/5.000/5.391 CPU microseconds per trip and
p99 325/334/363/682 us; paired `batch32`/Go rate is 1.089 [1.060, 1.090] with
CPU 0.928 and p99 0.530, and at 1024 small peers 1.116, 0.915 and 0.868.
Oversubscription on one CPU is a poor Go tradeoff: paired P4/P1 p99 is 4.869 at
64 small peers and 5.062 at 1024. Residency separates the Go forms at 1024 small
peers, 74,588 KiB for handler/P1 against 11,956 for acceptor/P1, despite the
same 64 MiB aggregate initialized capacity.
*Verdict:* **holds** — it strengthens reference quality and narrows the
tradeoffs among sequential-API candidates, and explicitly does not identify an
intrinsic cost of sequential user code. `codex/io-go-screen@7aa6191c`,
*Fifty-third experiment: sequential Go in the fixed echo screen*.

**Experiment 62 — what do the native candidates do under the wider client?**
Pure-ring and immediate-send io_uring at 32, 64 and 128 provided 64 KiB buffers
were compared against epoll and the indexed continuation at client widths one
and two, after a correctness repair: the receive path parked a connection on
buffer exhaustion and depended on a later buffer return to rearm, but an earlier
completion can return buffers before the exhaustion completion is consumed,
leaving the connection parked with buffers available. The corrected path records
a per-worker buffer-return generation, snapshots it at every arm and grants one
retry when it changed. At width two, epoll beats every uring form and the
indexed continuation in every pass: the strongest uring median is pure ring at
128 buffers, paired rate 0.89494 [0.86550, 0.92093] of epoll with CPU per trip
1.13208, and the indexed continuation 0.82002 with CPU 1.24074. Pool capacity
matters — 128 against 32 buffers gives paired rate 1.05539 with CPU 0.95313 in
all five passes, at peak RSS rising from 3,844 to 9,988 KiB — and the
observations explain it: pure ring at 32 buffers reports 89,026/92,112/93,064
exhaustion completions and at 128 reports zero. All three inline capacities show
zero exhaustion, peak visible loans of one, and mean bytes per successful send
of about 64 KiB (64,947..65,534), so experiment 38's 8 KiB aggregation failure
does not repeat at this message size.
*Verdict:* **holds** — a native baseline result and a real correctness repair to
the reference, both independent of the Whitefoot runtime.
`codex/io-native-frontier@b82647d5`, *62. Native large-message candidates with a
wider ARM client*.

**Experiment 68 — does the mixed Rayon reference retain admission with its
result?** An optional feature made the worker send a value owning its semaphore
permit, so admission is released when the I/O owner takes the result rather than
when the worker publishes it, and added an independent count of unfinished
closures with a drain waiting for it to reach zero. Local qualification passed
seven default and eight retained Rust tests, four original and six retained
external runs at budgets two and four, and six observed runs reconciling twelve
produced and consumed results with zero held at exit; two single-change negative
controls confirm the assertions bite. The ordinary release module matches its
`ae94bfcf` parent after normalizing ten source-location records, which is
qualified IR equality and explicitly not byte identity.
*Verdict:* **unresolved** — no qualified Linux execution and no mixed
performance comparison; experiment 42's smoke is evidence for the original form
only. Settle it with a Linux cohort comparing both admission boundaries under
one total budget and an independently established client.
`codex/io-runtime-followup@6de4557c`, *68. Result-retained admission for the
mixed Rayon reference* (revision `0a10ceca`).

## Storage and memory

**Experiment 19 — how much of the footprint is idle stack metadata?** State
headers were moved into contiguous aligned cells and a stack context prepared on
its first free-list pop, crossed with the used-lane policy over 840 paired rows.
Median peak RSS at split1 falls from 32,768 KiB at base to 13,044 with used
lanes, 24,536 with compact metadata and 6,928 with both at one peer; at 1024
peers the row is 79,228/61,580/79,236/59,588 against native epoll's 1,980. Used
lanes save roughly 17..20 MiB including at 1024 peers; compact metadata saves
about 8 MiB with few live stacks and essentially nothing at 1024.
*Verdict:* **superseded** — both policies belong to the retired pool; the door
is that a large cold and reserved-memory improvement is not a live-connection
footprint improvement, and roughly 58 MiB at 1024 peers still needed a
live-context and buffer explanation. `codex/io-runtime-followup@6de4557c`,
*Nineteenth experiment: compact stack metadata and first-use contexts* (revision
`a88eedd0`).

**Experiment 23 — do allocation semantics and stack representation explain the
gap?** The native epoll reference was given four receive policies — shared
worker scratch with private spill, private arena slice, private `malloc`,
private `calloc` — under both the manual state machine and the nested stackful
handler, so representation and storage vary independently. At 1024 connections
the manual reference's peak RSS is 1,980 KiB with shared scratch, 67,396 with
the arena, 5,804 with `malloc` and 10,028 with `calloc`, and stackful execution
adds about 4 MiB with the latter two. Against the matched private-`calloc`
stackful reference the small Whitefoot runtime loses every split2 pair at 64
peers, 1024 peers and 64 KiB — paired medians 0.8787 (0.8134..0.9613), 0.8553
and 0.7613 — with tails 1.1907 and 1.4338 and 1024-peer RSS about 4.2..4.4 times
higher.
*Verdict:* **holds** — the key negative of the family, independent of which
scheduler ran the Whitefoot side: allocation semantics and stack representation
alone do not account for the runtime gap. `codex/io-storage-experiments@361cb952`,
*Twenty-third experiment: receive storage and sequential-handler residency*.

**Experiment 25 — why does a sparse private arena cost its whole virtual size?**
The same binaries were launched through a checked wrapper setting the
per-process transparent-huge-page policy, with live mapping snapshots taken
while every connection was open rather than lifetime peaks. The arena answer is
direct: at split2/1024 the permitted snapshot has a 70,660 KiB anonymous mapping
with 65,572 KiB resident including 65,536 KiB of huge pages, while the disabled
mapping is 4,100 KiB resident with none, and the roughly 60 MiB amplification
repeats in every repetition and both placements. Per-connection `malloc` and
`calloc` show no huge pages and unchanged live RSS. Lower amplification did not
close the gap: with the policy disabled, small/calloc throughput still loses
every pair at 0.8625 (0.8470..0.8792) on 64 KiB with p99 worsening at 1.3204,
and the paired RSS ratio stays at 3.8308.
*Verdict:* **holds** — page policy and live mappings are host facts that outlive
any runtime, and the rejection is universal: disabling the policy is not a
performance policy, no representation is a universal winner, and peak lifetime
RSS and live resident memory answer different questions.
`codex/io-pages-experiments@80ffb2c2`, *Twenty-fifth experiment: process page
policy and live storage*.

**Experiment 65 — does receive storage still matter under the wider client?**
Shared-scratch and private-`calloc` epoll were compared as the only changed
variable, with io_uring at 128 buffers and the indexed continuation as anchors,
under the admitted ARM topology and a two-worker large-message client. Private
storage loses to shared scratch in every pass: paired rate
0.84748 [0.83216, 0.88859], CPU per trip 1.16071 and p99 1.27919, peak RSS
rising from 3,648 to 5,228 KiB. The consequence for the Whitefoot row is the
finding: its paired rate against shared epoll is 0.78092 but against private
epoll 0.92454, so the storage policy inside the reference materially changes the
apparent gap. The small-message control shows none of it, paired private/shared
rate 1.00683. Observations record 40 user-visible loans and receive/send
operation counts 32,026/32,026, 32,002/32,002 and 32,001/32,001, each moving
2,097,152,000 bytes in both directions.
*Verdict:* **holds** — receive-storage policy becomes a measured optimization
target; shared epoll stays the competitive reference and private epoll a
diagnostic row. `codex/io-storage-control@396b1123`, *65. Private epoll storage
under the qualified large-message client*.

**Experiment 67 — can a sequential handler own its received chunk across nested
send waits?** A move-only lease over an owner-local pool node was carried into
the nested send of the elided C++ coroutine engine, so a blocked send suspends
with the lease live, no unsent suffix is copied, and an unused node is returned
before a receive wait. Against private C++ the lease is faster and cheaper in
all five large-message pairs: rate 1.1431 (1.1001..1.1904), CPU per trip 0.8730,
peak-RSS ratio 0.5150, the CPU difference almost entirely system time (0.63
against 0.55 seconds). Against shared C++ and manual epoll it ties in rate
(0.9991 and 0.9959) and is worse in tails — every large lease/manual p99 pair
loses, one pass reaching 3,753 against 1,171 us. The representation is not free:
the root frame is 184 bytes against 168, the pool node 65,552 bytes for 65,536
of capacity, and linked text 9,923 bytes against 9,532 shared and 8,413 manual.
*Verdict:* **holds** — a language-design input: a sequential nested send can
preserve stable ownership of a received chunk and recover the private-storage
rate, CPU and residency penalty without beating shared reuse, which supports
investigating an ownership contract for received chunks and selects no container
or source API. `codex/io-chunk-lease@878b6ae7`, *Sixty-seventh experiment: owned
receive chunks in a sequential handler*.

## Placement and dispatch

**Experiment 20 — is the pinned policy's loss an initial-assignment defect?**
Compiler-admitted staged calls were given an explicit initial owner across the
started worker prefix, with an intrusive incoming FIFO beside each owner's ready
continuations and new schedules covering ownership, duplicates, cycles and
prefix routing. It recovers the loss: split2 paired balanced/base throughput is
1.2248 (1.1646..1.4670) at 64 peers, 1.2242 at 1024 and 1.2839 on 64 KiB, and
balanced/owner is 1.8148, 2.4332 and 1.7002, with an untimed run starting
exactly 32 handlers on each owner and no resume migrations. It does not fix
service: shared4 improves median throughput while worsening every paired p99 at
3.7026 and 2.8969, and the zero-compute fixed-arrival control is the
counterexample — split2 balanced reaches 165,449 exchanges/s against chunk's
127,751 and native quantum's 192,472 while its light p99 is 6,971 us against
2,137 and 171, losing every paired native tail at median 41.2485.
*Verdict:* **superseded** — the policy it repairs no longer exists; the door is
that initial placement was a real implementation defect and that a candidate
cannot be selected from heavy throughput or closed-loop echo alone.
`codex/io-runtime-followup@6de4557c`, *Twentieth experiment: deliberate initial
I/O placement* (revision `b8c94ecd`).

**Experiment 31 — is the two-worker gap caused by sharing physical cores?** A
`separate2` cohort assigned the first core's two SMT siblings to the server and
the second core's to the client, keeping two workers and two logical CPUs each,
with the sibling topology verified before measuring. With the original client
neither rate nor tail improves consistently (separate2/split2 rate
0.9943 [0.9366, 1.0277], tail 1.3571 over a 0.2251..8.1459 range), and the
native reference itself loses every placement rate pair at 0.9739 while its CPU
per exchange rises in every pair. With the one-round client separate2 improves
every rate pair at 1.0242 and its light-tail median falls to 0.1499, yet the
Whitefoot side still loses every native light-tail and post-dispatch pair at
2.7251 and 2.0680. At 2,097,152 heavy steps and 500 light requests/s/peer, heavy
capacity loses every pair at 0.9811 and CPU per exchange at 1.0152, median heavy
counts 416 against 424; client CPU per exchange falls about 10..20%.
*Verdict:* **holds** — the cohort is a measurement control that outlives the
runtime, and the rejection is methodological: physical-role sharing is neither a
sufficient explanation nor a standalone repair, and it is not an isolated
kernel-lock experiment. `codex/io-placement-experiments@dce5ef95`, *Thirty-first
experiment: separate client/server physical cores*.

**Experiment 32 — do the surviving optimizations compose?** One unchanged
accepted-handler module ran as callee-small, balanced, balanced-small and
quiet-small against nine native controls under fixed allocator and page policy.
They compose: split2 balanced-small against callee-small gives paired rate
1.1572 [1.1490, 1.1734] at 64 small peers and 1.1315 at 1024, CPU per exchange
improving in every pair at 0.9574 and 0.9671, and live RSS falling from
27,064/34,756 KiB for balanced to 3,528/15,064. Low occupancy is untouched:
balanced-small loses every one-peer pair at 0.8655 and every four-peer pair at
0.7338 against callee-small, and its one-peer rate against native elided private
is 0.6635 at 5.3529 times its CPU.
*Verdict:* **superseded** — every policy in the combination is retired; the door
is that dispatch and storage optimizations compose and the combination still
does not solve low-occupancy cost, per-connection memory or mixed-load service.
`codex/io-combined-findings@94586997`, *Thirty-second experiment: combine
measured dispatch and storage policies*.

## Allocator and buffer ownership

**Experiment 27 — should the accepted handler own its buffer?**
`tcp_echo_server.wf` was superseded in place so `serve_one` constructs its own
initialized scratch inside the Accepted arm and declares `allocates(heap)`,
removing the caller's region and the scratch parameter with its length
precondition. With two workers the handler form lowers live RSS materially —
split2/1024 falls from 53,808 to 32,904 KiB with ordinary heap residency
19,508..25,012 against the caller's 46,572 — while at one worker both forms
retain exactly 46,516 KiB, because the entry thread executes worker zero.
Throughput did not follow: the paired callee/caller rate at split2/1024 is
0.9904 (0.9735..0.9998) with the page policy disabled, losing every pass despite
lower memory, and against native stackful `calloc` in that cell throughput is
0.8621, CPU per exchange 1.0856 and peak RSS 2.1217. At split1 with large
transfers the same form beats native throughput by 4.17% in the paired median
while spending 36.11% more CPU.
*Verdict:* **holds** — about the writer's source form, which the language still
has: handler-owned initialized buffers are retained as an experimental ownership
form, with no initialization removed and no writer-visible pinning, suspension
annotation or lifetime exception introduced.
`codex/io-allocation-experiments@8b44b5f9`, *Twenty-seventh experiment:
allocation inside the sequential handler*.

**Experiment 30 — is the residual heap the allocator or the program?** The
native reference was made to run worker zero on the process entry thread, and a
second axis set the glibc top-padding tunable explicitly to 131,072 or zero with
the loader's readback checked per environment. The effect is reproduced with no
Whitefoot type, proof or task lowering: at split1/1024 native manual main-worker
`calloc` changes ordinary heap residency from 46,584 to 8,240 KiB while its
total heap mapping stays 65,644 KiB, and both Whitefoot buffer placements move
from exactly 46,516 to 8,212 KiB — a roughly 37.4 MiB one-worker reduction. No
repeatable rate gain accompanies it: the base split2/1024 form loses every
zero-padding rate pair at 0.9856 [0.9574, 0.9978], and against the native
main-worker forms every rate and CPU pair still loses (0.8490/1.1318 and
0.8561/1.1239 at split2/1024).
*Verdict:* **holds** — a glibc main-arena property reproduced in native C, and
the closing sentence is the durable one: the remaining Whitefoot work is
continuation state and service cost, not removal of source initialization.
`codex/io-allocator-findings@eaf681f1`, *Thirtieth experiment: main-thread
allocation and heap padding*.

## Continuation lowering

This is the theme whose conclusions outlive the retired scheduler, because it is
about representation rather than about a ready queue: a feasibility probe in a
native engine, a compiler mode emitting LLVM switched-resume continuations from
ordinary sequential source, a measured comparison against the native panel, and
three bounded improvements to the experimental executor that drives it. None of
the emitter is in the tree.

**Experiment 26 — can nested coroutine frames live inside their parent?** The
native epoll engine gained a C++20 coroutine handler with an elision attribute
promising that a directly awaited child cannot outlive its caller. At `-O2` the
child frames disappear: on Linux the four-peer echo preflight changes 84 heap
allocations and frees (8,576 bytes) into four roots (672 bytes), a 64-byte root
plus a 104-byte child per send becoming one 168-byte root, and the quantum
preflight changes 168 allocations (14,016 bytes) into four 248-byte roots. Peak
RSS at 1024 small peers is 4,096/7,904/4,364 KiB for C++ manual, stackful and
elided with shared scratch, a paired elided/stackful ratio of 0.5521 consistent
with one resident stack page per connection. Throughput does not follow: every
elided/manual and elided/stackful paired rate range crosses one.
*Verdict:* **holds** — feasibility resolved and one arrangement explicitly
rejected, since keeping a parked root stack per connection merely to drive heap
continuations would retain the live stack-page cost; the toolchain caveat holds
too, as the installed LLVM's coroutine-end signature differs from the upstream
documentation. `codex/io-coroutine-experiments@2de6c000`, *Twenty-sixth
experiment: nested coroutine frames in the same native engine*.

**Experiment 33 — do nested stackless frames hold a real completion loan?** A
parent owned an initialized 128-byte local buffer while a child received into
that borrowed storage in a loop through the real runtime's submit and join
entries, with only the bridge's completion callback redirected to a coordinator
that still calls the actual publisher exactly once, over four orderings:
immediate completion, completion before registration, publication during
registration, completion after suspension. On Linux both heap and elided forms
pass all 640 cases and 192 logical exits on each backend — each native run 513
ring and zero helper publications, each forced-fallback run the inverse, every
run 384 registrations and dequeues with 128 completions before registration and
128 during, and frame allocations and frees 1,280/1,280 heap and 640/640 elided
with 312,320 requested bytes either way.
*Verdict:* **holds** — it closes the loan gap the readiness-only probe left:
nested stackless frames retain parent-owned addresses through actual io_uring
completion and scoped draining, supplying no coroutine ABI, registration
protocol, migration or cancellation. `codex/io-continuation-experiments@207b271e`,
*Thirty-third experiment: nested continuations retain real completion loans*.

**Experiment 39 — is the generated continuation competitive?** The exact
sequential `tcp_echo_server.wf` was compiled through the continuation emitter,
linked with the experimental coordinator and measured against the native panel
on one server CPU with the client on another physical core. It is not: paired
continuation/callee rates are 0.760, 0.348, 0.375, 0.276 and 0.967 at 1/4/64/1024
small peers and 64 KiB, with CPU per trip 1.143, 2.833, 2.634, 3.494 and 1.034;
against the highest native median per cell the rates are 0.922, 0.307, 0.371,
0.259 and 0.616. Live RSS is the one favourable column: 3,040/12,448/5,504 KiB
at 64-small/1024-small/64-large against callee 3,504/15,024/5,492, balanced
3,524/15,060/6,212, `uring-64k` 2,244/4,188/3,284, `epoll-calloc-main`
2,008/9,720/3,804 and `cpp-elide-calloc` 4,132/12,104/5,912. Process scheduling
locates the cost: 7.111, 3.745, 2.614, 3.750 and 3.152 switches per trip against
callee's 0.0251..0.0315.
*Verdict:* **holds** — the decisive negative of the theme, about the
representation and its executor rather than a ready queue: the small-message
storage saving does not pay for the execution loss, this two-thread coordinator
is not a competitive executor, and the sequential source and continuation
representation remain viable research inputs.
`codex/io-continuation-screen@2147857e`, *Thirty-ninth experiment: measure
generated staged WF against the native panel*.

**Experiment 34 — can the compiler emit continuations from ordinary source?**
`--continuations --emit-llvm` runs the ordinary parsing, canonical, semantic,
proof and lowering path and selects coroutine representation from derived
`may_suspend` effects across the finite call graph, with no source annotation
and no selection by function name or shape; pure functions keep their ordinary
representation. The emitter uses switched-resume intrinsics, the existing frame
planner owns source storage, child result slots and one opaque wait node per
serial activation, and the parent destroys the completed child before reading
the result. On Linux both generated stream programs pass their zero-byte and
65,673-byte cases on the native ring and the forced helper backend, the
recursive program recording 515 ring and 514 helper publications against the
plain program's 18 and 17.
*Verdict:* **holds** — the lowering exists and is qualified, and the cleanest
engineering decision in the family is here: the CLI refuses native executable
linking, parallel and checkpoint modes and ledgers in this mode rather than
silently claiming to provide them. `codex/io-continuation-lowering@caa316ac`,
*Thirty-fourth experiment: compile sequential WF into continuations*.

**Experiment 35 — can a network operation suspend a generated caller?** Listen,
accept and connect joined the experimental sequential await inventory, reusing
the qualified runtime submits and typed outcome mappers, with accept's three
peer-address outputs reserved by the ordinary frame planner. The oracles
withhold a connection until the host has returned from resume with an accept
node still pending, then withhold input until a pending receive is witnessed, so
an old blocking wrapper cannot pass. The native TCP byte oracles pass at 0 and
65,673 bytes with 3 and 4 ring publications, the forced-helper runs with zero,
and the client and refused-connect cases with 2 each; both occupied-listen
routes report one helper publication, as the existing listen implementation
requires.
*Verdict:* **holds** — the network prerequisite for a concurrent executor, with
no concurrency or performance claim and no change to signatures, container
layout or the staged runtime interface. `codex/io-continuation-network@d5037bfc`,
*Thirty-fifth experiment: network operations suspend the generated caller*.

**Experiment 37 — do checked staged calls own independent continuations?** The
checked issue and drain graph was made to publish may-suspend user calls as
independent continuation roots, source order still deciding the drain, a
completed later task retaining its result and storage until its own join, and
the issuer reserving a 32-byte descriptor, a child-frame pointer and the typed
result per pipeline slot through the normal planner. Two unchanged sources
qualify it: a fanout program lends one iteration-owned scratch buffer to four
staged callees and requires all four peers to connect before any speaks with the
last connected peer finishing first, and the echo server repeats the
reverse-completion protocol over twelve connections with a four-task window.
Both pass under sanitizers and on the native ring, recording 4 and 12 completed
and retired tasks with a peak of 4 retained children.
*Verdict:* **holds** — a correctness qualification of the representation:
storage borrowed by a child stays alive and address-stable through its join, and
a finished flag alone does not permit reuse; arbitrary suspended-frame
destruction, cancellation, asynchronous cleanup, compute checkpoints, multiple
resumers and Windows remain unqualified. `codex/io-continuation-staged@f0d633c1`,
*Thirty-seventh experiment: checked staged calls own independent continuations*.

**Experiment 44 — how much of experiment 39's loss is the thread handoff?** The
sole resumer was made to capture the wake epoch, drive target progress and check
the ready queue itself instead of handing off to a background progress thread.
The occupied cells move sharply: paired owner/threaded rates are 0.963, 2.500,
1.975, 2.774 and 1.007 at 1/4/64/1024 small peers and 64 KiB, with CPU per trip
0.634, 0.398, 0.506, 0.368 and 0.796. At 64 peers median server CPU per trip
falls from 21.172 to 10.703 us, switches per trip from 2.743 to 0.000305 and p99
from 2,569 to 750 us; at 1024 peers switches fall to 0.002354 and p99 from
58,957 to 12,660. The remaining loss is material — paired owner/callee rates
0.759, 0.832, 0.719, 0.752 and 0.978 — and the single-peer case loses throughput
in every pair while using less CPU.
*Verdict:* **holds** — removing a cross-thread handoff in an occupied
continuation executor is a large reproducible improvement, and eliminating a
thread is explicitly not a universal latency or throughput gain.
`codex/io-continuation-owner@f72aacb8`, *Forty-fourth experiment: let the
continuation resumer drive target progress*.

**Experiment 48 — does batching ready resumptions restore submission
aggregation?** The owner was allowed to resume up to a bounded number of ready
waiters between explicit progress calls, progressing immediately whenever the
ready queue empties and never waiting for a batch to fill. Thirty-two against
one gives paired rates 0.999, 1.075, 1.283, 1.271 and 1.108 across the five
cells, CPU per trip 1.067, 0.926, 0.778, 0.792 and 0.911 and p99 1.064, 0.959,
0.797, 0.816 and 0.889; at 64 peers median server CPU falls from 6.953 to 5.391
us per trip. Counters confirm the mechanism: at 64 peers the threaded, owner-one
and owner-thirty-two forms make 125,993/127,932/128,047 ring submissions through
55,606/127,869/4,031 kicks — 2.266, 1.0005 and 31.766 submissions per kick —
while the four-peer preflight gathers only 2.75 and the fragmented stream one.
*Verdict:* **holds** — batching progress in an occupied continuation executor
without changing sequential source, ownership rules or signatures, with the
stated limit that seven same-host samples cannot establish that thirty-two is
optimal. `codex/io-continuation-batch@fc69af15`, *Forty-eighth experiment: batch
ready continuations before target progress*.

**Experiment 52 — does the generated continuation handle compute between socket
operations?** The unchanged `tcp_compute_server.wf` bytes were compiled with
`--continuations --par` and run at all three progress policies against the
common oracle: four concurrent clients sending three independently fixed
seed/round/result vectors in one-byte fragments, a truncated request that must
exit with the source-defined status 9, and an oversized request that must exit
10, each with no response bytes. An independent log audit verifies all eighteen
mixed cases in the exact three-policy, two-route order with positive balanced
waiter counts and exact completed and retired task counts; the previous 66
generated stream invocations, three file-outcome suites and four 640-case nested
loan fixtures also pass.
*Verdict:* **holds** — protocol and lifetime behaviour of the representation are
qualified on a mixed workload, with the stated limit that the pure computation
still runs on the sole resumer, so nothing here establishes multicore
computation or light-request progress during long computation.
`codex/io-continuation-mixed@6a8c19c1`, *Fifty-second experiment: qualify
generated continuations on the mixed protocol*.

**Experiment 54 — is the pending-waiter scan the remaining cost?** The
completion-record address was hashed into 1024 buckets, pointer equality still
identifying a waiter inside its collision list and the same static 8 KiB array
present in both settings. The lookup work almost disappears: at 64 peers visited
nodes fall from 4,677,075 to 134,136 over the same 256,258 lookups — 18.2514
against 0.5234 nodes per call, maxima 64 and 2 — and at 1024 peers from
11,658,924 to 219,886 over 413,698 lookups, maxima 1024 and 7. Timing does not
follow proportionally: paired rates are 1.000618, 0.997269, 1.009701, 1.018090
and 1.005607, only the 64-peer cell improving in all seven pairs as median CPU
falls from 6.796875 to 6.718750 us per trip.
*Verdict:* **holds** — a small lookup improvement under occupancy and a clean
negative: eliminating 97..98% of lookup work does not produce proportional CPU
or throughput savings, so pending lookup does not explain the remaining native
gap. `codex/io-continuation-index@1192ef35`, *Fifty-fourth experiment: index
pending continuation waiters*.

**Experiment 66 — can a generated continuation await pure computation
elsewhere?** An opt-in compiler path kept the source call's arguments, result
and ownership interval while an existing CPU worker runs the ordinary function
and the I/O owner services other suspended activations, selecting candidates by
a checked empty effect row, no suspension and an estimated cost of at least 128
under the existing cost model, with a cheap-entry prefix bounded at 32 copied
scalar operations. Qualification is thorough and entirely non-numeric: an
untimed pipe gate holds a real CPU worker so two submissions fill a two-permit
limit and a third awaits admission while a fourth zero-round request still
returns; a recursive fixture awaits `fib(24)`, returns 46,368, retires one outer
task and balances 75,024 inner publications with 75,024 joins at one and three
workers, and returns the same result with no publication at zero workers. The
Linux job passes all 72 compute invocations.
*Verdict:* **unresolved** — no performance result and no policy selection, and
the light-progress property is qualified only on this fixture. Settle it by
running the mixed paced panel against this path with the native
cooperative-quantum and Tokio/Rayon references under one budget.
`codex/io-runtime-followup@6de4557c`, *66. Await pure computation on the existing
pool* (revision `03d3e05d`).

## Profiling: where the CPU goes

**Experiment 28 — what does a CPU profile say about the mixed-load gap?**
Sampling at 999 Hz covered whole server lifetimes for four forms in the
qualified paced cells, with all 110,784 raw sample events independently
accounted, zero lost samples and unknown symbols retained. Kernel execution
dominates the zero-compute workload: median sample shares are 93.71%/3.73%/2.46%
kernel, program and libc for the Whitefoot base on split1 and 95.41%/1.60%/2.79%
for the native manual quantum, the Whitefoot userspace contribution spread
across submission, ring progress, joins and completion. The attribution limit is
the finding: the end-to-end light p99 ratio of 19.1619 at zero-compute split2
decomposes into a dispatch-wait p99 of 1,560 against 28 us and a post-dispatch
p99 of 186 against 86, and the client pump repeatedly exchanges on one heavy
connection until EAGAIN, so a continuously ready heavy connection can delay the
client's next arrival check even on disjoint CPUs.
*Verdict:* **holds** — the decomposition and its caveat are method: separately
computed quantiles are not additive components, post-dispatch latency includes
client and kernel work, and a bounded client control is needed before selecting
another server policy. `codex/io-profile-experiments@d72d0d25`, *Twenty-eighth
experiment: CPU attribution of the mixed-load gap*.

**Experiment 41 — is excess parking the cause of the Rayon utilization gap?**
Sixteen inherited CPU-clock and scheduler-event captures were taken of
four-batch executions of the same workload. They do not reproduce the question:
clipped to each pool's common lifetime, raw switch intervals give running time
over lifetime of 3.894 for Whitefoot against 3.862 for Rayon, with summed
runnable delay 125.262 against 141.007 ms and blocked time 13.657 against
42.497 — the instrumented capture even reverses the ordinary wall-time ordering.
Sampled CPU in the workload thread group is 5,228.23 against 5,160.16 ms, of
which layout functions account for 4,809.81 and 4,818.82, runtime and Crossbeam
symbols 106.11 against 56.06 and pthread mutex functions 81.08 against zero.
*Verdict:* **holds** — a negative that closes a door: instrumented four-batch
captures do not reproduce the ordinary single-batch utilization gap and
therefore do not establish excess parking as its cause, and a four-batch trace
cannot be spliced into a one-batch confirmation. `codex/io-compute-profile@0d15c7cc`,
*Forty-first experiment: attribute the WF/Rayon parallel utilization gap*.

**Experiment 55 — are the early runnable waits explicit yields?** Syscall entry
and exit events for the host yield were added to the corrected coarse trace and
each thread's yield intervals intersected with its own runnable off-CPU
intervals, every entered call required to have a matching successful exit. In
the first 100 ms after exec the ordinary form accumulates 286.277 and 291.437
thread-ms of running time with 94.678 and 93.964 ms runnable off CPU, of which
85.665 and 84.897 lie inside a yield — 90.48% and 90.35% — while the matching
Rayon captures use 398.260 and 396.819 thread-ms of running time with only 0.143
and 1.040 ms runnable off CPU. Rayon makes about 32,000 yield calls to the
ordinary form's 1,131 and 1,165.
*Verdict:* **holds** — the compute waiting policy this concerns is still the
tree's, and so is the warning: Rayon's far larger yield count mostly returns
without descheduling, so a raw percentage over a tiny denominator is misleading
and removing yields on their count is not a justified optimization.
`codex/io-cpu-yield-attribution@ab3879e9`, *55. Attribute early runnable waits
before changing CPU placement*.

**Experiment 57 — which yield call site is it?** One entry probe at the host
yield wrapper recorded the return address in the same frozen executables, each
normalized return required to match the instruction after a real call site
rather than the nearest symbol; eight sites were disassembled in advance, among
them the join's COMPLETING wait, the exhausted compute join, the park handshake
and the idle scan's yield rounds. Across eight traces all 75,939 entered yields
return successfully and all 3,465 Whitefoot calls pair with a matching caller
probe. In the first 100 ms the runnable off-CPU time inside any yield is 69.586,
93.099, 73.851 and 79.679 thread-ms, of which the idle scan accounts for 69.184,
92.915, 73.561 and 79.535 — 99.42% to 99.82% — while the startup rendezvous and
COMPLETING handshakes together account for under 0.6%.
*Verdict:* **holds** — it locates a path in the compute waiting policy the tree
still has and states the instrumentation cost honestly: the caller-probe to
syscall-entry interval alone reaches 2.477..3.064 ms in three captures, and
locating a wait is not evidence that useful work was available during it.
`codex/io-cpu-yield-callers@01aabbb6`, *57. Locate the yield wait path in the
same CPU executables*.

**Experiment 59 — does removing the idle scan's yield rounds help?** The
zero-yield-round build override was measured with ordinary binaries against the
same-IR default and the frozen Rayon reference, ELF instruction bytes confirming
that exactly one of eight yield call sites disappears. It does not: paired
candidate-minus-default wall is +1.196% (-0.643..+2.739%) at one batch and
+0.114% at sixteen, with total CPU +1.244% and +0.022% and voluntary switches
+51.939% and +5.383%; the byte-identical same-binary control's own wall spread
is -1.221% to +2.271%. Observed idle waits rise from 21 to 356 at one batch and
162 to 371 at sixteen. The Rayon gap is unchanged: ordinary and zero-yield
paired wall gaps are 12.422% and 13.552% at one batch and 2.117% and 2.114% at
sixteen.
*Verdict:* **holds** — this closes the loop 55 and 57 opened, with a rejection:
removing those yield rounds is not the fix. The result explicitly does not
reject the ownership-based parallel model, and ready-work availability must be
distinguished from OS placement before another waiting-policy change.
`codex/io-cpu-idle-controls@e0a05efc`, *59. Ordinary CPU control without
idle-scan yields*.

## Diagnostics and corrected measurements

**One CPU worker is not sequential I/O.** The first idle cohort stalled before
any timed sample, and per-sample deadlines plus sample logging located it: the
runtime normalized both zero and one requested workers to zero, and the
bootstrap then selected the entire sequential clone including connection
handling, while the load generator keeps every connection open until all
exchanges finish — so the sequential server waited for the first peer's EOF
while another peer waited for its response. The correction preserves a requested
one worker and asks the bootstrap for the minimum appropriate to its reachable
lowering, one for staged I/O hand-outs and two for compute-only hand-outs,
leaving zero as the explicit sequential opt-out. A separate probe with four open
peers and one worker verified 1, 2, 3, 4, 4, 4 responses before any EOF at stack
counts 2, 3, 4, 5, 6 and 9.
*Verdict:* **holds** — a configuration and lowering defect, not evidence that
one worker can support arbitrary I/O concurrency, and its distinction survives
the runtime: optional compute scheduling and externally required I/O progress
are separate design obligations. `codex/io-idle-diagnostic@e4a1d47f` with the
observer repair at `codex/io-idle-retry@06d93a46`, *Failed first attempt: one
worker disabled I/O concurrency*.

**Experiment 46 — why does an immediate send lose half its large-message
throughput?** The exact 64-peer 64 KiB cell was re-run with per-operation
counters distinguishing requested from completed bytes and gathered vectors,
each record moving exactly 2,097,152,000 bytes. The mechanism is
application-level aggregation: both 8 KiB forms receive exactly eight buffers per
round trip, but the pure-ring form keeps one send in flight while later receive
completions append buffers to its next vector, averaging
3.7847 [3.7838, 3.7869] requested vectors per send and 2.1138 send operations
per round trip, while immediate success empties the queue on each receive and
performs exactly eight single-vector calls per round trip. Every inline call
succeeds completely, with no short sends, no EAGAIN and no ring fallback. At
64 KiB each receive already carries about 47..54 kB and the penalty is absent.
Client exchange CPU per wall second is 0.884..0.886 for the 8 KiB ring form and
0.650..0.652 for the inline one.
*Verdict:* **holds** — a native-reference mechanism result with its measurement
limit stated in the source: the counters are send operations, not TCP packets,
and whole-client CPU saturation is unsupported as the cause. The discriminating
follow-up it proposes — defer an immediate send until a bounded completion batch
is collected — was not built. `codex/io-uring-aggregation-diagnostic@f6ff40f7`,
*Forty-sixth experiment: observe aggregation in the native send regression*.

## Not carried

Nothing below is summarized above, and each line says why.

- `codex/io-first-principles` and `codex/io-model-completion`: their
  `FIRST-PRINCIPLES.md`, `DESIGN.md` and `reviews/` documents are already in the
  tree at later revisions, on a history unrelated to the current main line.
- The temporary CI cohort that ran every experiment here: declared temporary in
  experiment 1's own removal note, to go when the investigation closes.
- The experimental runtime settings themselves — ready-shard, stack-spread,
  counter-stripe, completion-burst, owner-ring, local-wake, I/O-quantum,
  compact-stack, used-lane and round-robin: each selects a mechanism that no
  longer exists, and each one's result is recorded above.
- The setup halves of experiments 30 and 32 and the first half of experiment 24:
  intermediate branches whose results are recorded under the completed
  experiment.
- Experiment 2's two failed attempts and experiment 41's two failed profiler
  setups: recorded on the branches as producing no evidence, and they produced
  none.
- Experiment 60's first ARM admission failure at `f2e0f460`: an implementation
  error comparing two identifier namespaces, corrected at `d241cf7d`.
- Experiment 62's final summary-reference failure at `b82647d5`: a
  postprocessing defect repaired by replay over the frozen rows; the cohort
  itself is complete and is summarized above.
- The unstable Windows benchmark cohorts refused across many revisions: no
  qualified Windows performance table was produced by this family, and the
  refusals were retained rather than rerun for green.
- The macOS compatibility shims that let socket fixtures run locally:
  correctness scaffolding, never a timing reference, and never shipped.
- The rebuilt former-compiler comparison build used by experiment 12: temporary
  measurement machinery whose purpose ended with that comparison.

## Open after this record

**Is the Windows IOCP wake defect still live?** The repair at `04106a23` is no
longer absent: `io/salvage-iocp-uring` carries
`compiler/src/backend/completion/windows_iocp.c` and its header onto the current
tree, so the port's notify path publishes one packet per previously unnotified
node of an intrusive waiter list, `wf_windows_return_wake` preserves a polled
packet only while a notified cohort is outstanding, and a new park waits on the
runtime condition until that cohort has left the kernel. What the entry asked
first — whether the repair belongs in the tree — is answered by taking it: the
repair is correct on its own terms whether or not the current wait structure can
still reach the interleaving, and it removes a wake that depends on a parked
count the runtime no longer owns exclusively.

What is still open is the reachability question, and one step less of it can now
be taken. The replay hooks in `native_adapter_probe.c`, the
`WF_WINDOWS_IOCP_WAKE_REPLAY` build and their workflow wiring were deliberately
not carried, because they are a reproduction harness rather than the repair, so
nothing in the tree reproduces the old interleaving on demand; the repaired path
is exercised by the `completion-windows` job's ordinary native adapter probe on
a real Windows host. Settle the remainder by carrying those hooks and their
wiring from the same branch, or by recording that the current wait structure
cannot reach the interleaving.

**Is the SQPOLL store-to-load barrier still missing?** No. The branch found that
a release store of the submission tail and an acquire load of the ring flags do
not by themselves order the publication against the read of the wake-needed bit,
and matched the requirement against upstream liburing.
`io/salvage-iocp-uring` carries the tree's
`research/experiments/io-completion-bench/uring_echo.c` from that branch, and its
poll-thread path now executes a sequentially consistent fence between the two.
The compiler's own io_uring path still has no SQPOLL code at all, so the
requirement has no second site in the tree, and the benchmark's path remains
optional, off by default and enabled by no screen.

**Which of the branch's dated memory facts remain true?** The branch's
[mcts_mem/whitefoot/system-interface.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/system-interface.md) carries 36 dated 2026-09-0x facts
against three in the tree, every one written against the unified scheduler. The
ones this record judges runtime-independent — packet policy, client width,
storage accounting, coroutine frame elision, the send-operations-versus-packets
limit, the allocator and page-policy results, the native and external baselines,
and the continuation-lowering qualifications — are carried forward as new dated
facts alongside this file. The remainder are statements about a ready queue that
no longer exists, are deliberately not carried, and are recorded here as closed
doors instead.
