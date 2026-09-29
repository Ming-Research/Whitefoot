# io-completion-bench

Program-level measurement of ordinary linked I/O functions against native
controls and the same Whitefoot source under different lowering options.
The [C2 measurements](C2-RESULTS.md) and their raw samples describe the pre-merge C2 revision (then numbered v0.58);
the earlier completion-model tables remain historical evidence.

## What it serves

`research/investigations/io-model/RESULTS.md` held only C-level
microbenchmarks of the completion core — round-trip cost, cached `pread`
delta, park/wake parity. Those numbers cannot answer the question the design
stands or falls on: whether a whole Whitefoot program that does real I/O
reaches the best native performance. This bundle answers that question and
supplies the evidence section that RESULTS.md now carries.

It is removed when the completion model stops being an open performance
question — when the numbers are stable, the bar is settled, and no further
runtime or lowering change is being measured against them.

## The three lines

Every line publishes the same bytes; a line that publishes anything else
cannot report a time.

- **N** — the best hand-written native C shape. `baseline.c`: a single-threaded
  blocking loop, a pthread pool over a striped index range, and on Linux a raw
  `io_uring` read pipeline (`uring_baseline.h`, kernel ABI directly, no
  liburing). Compiled `-O2` with no handicap.
- **S** — the Whitefoot program built with `whitefootc --no-overlap`.
- **C** — the same Whitefoot source built the way it ships. The C and S lines
  retain two configurations of one source. These labels do not promise a
  difference: all four C2 many-files programs emit byte-identical LLVM in
  both configurations. Every linked call holds its loans until return.

The matched ordinary-call controls use one C traversal and the same linked
library, with explicit LLVM bridges for the public WF view ABI and its private
C body ABI. `make ordinary-check` verifies data, EOF, factory-credit refusal
and cleanup on both routes; `programs-check` includes it. `make ordinary-bench`
runs the original runner, seeds, two warmups and seven alternating passes over
native direct/public/body and WF narrow/loop pairs. This separates the public
view-wrapper route from the remaining WF source-lowering cost. It does not
separately price native engine scheduling, factory serialization or lost
permission. The controls are removed with the attribution they support.

## Workloads

### The many-files workload

`programs/many_files_wide.wf` opens and reads four generated files per round;
`programs/many_files_wide8.wf` widens the same source shape to eight. Under the ordinary-host-values amendment, each open and read uses the same exclusive `HandleFactory`, so distinct
files and destination buffers do not make those calls independent. The
programs retain their source width but execute the calls sequentially.
`programs/many_files_narrow.wf` is the same work written as the natural
one-file-at-a-time loop, with its name and destination buffers hoisted above
the loop; it exists to measure what a writer gets who does not hand-widen, and
the answer is no overlap at all.
`programs/many_files_loop.wf` is the one-file-at-a-time counted loop with the
name and destination buffers constructed inside the body. It retains the
workload previously used to exercise PAR-3 staging. C2 deletes PAR-3, and its
current ordinary loop has no parallel grant. Comparing it with the narrow
program includes the per-iteration scratch allocation and source-shape costs;
it does not separately measure a lost staged pipeline.
`programs/pipe_relay.wf` pushes two independent byte streams at two
consumers through the ordinary `Inputs.stdout` and `Inputs.stderr` fields.
Its writes also share the passed factory.

The generated tree and the checksum are defined once, in `workload.h`, and
shared by the generator, the native baselines, and the Whitefoot programs. The
checksum is position-weighted so a four-wide lane split and a one-at-a-time
loop fold to the same value.

### The read-heavy workload

`programs/read_heavy_narrow.wf` and `programs/read_heavy_wide8.wf`, with their
`_4k` variants, answer the question the many-files workload cannot. That
workload opens a file per unit of work, and on the macOS host one `openat`
costs 116 us against a 1.9 us `pread`, so its table is mostly a measurement of
the host's endpoint-security stack. These four programs open eight 64 MiB
files once, before any read, and then perform tens of thousands of positioned
reads into them: 32,768 reads of 64 KiB, or the same number of 4 KiB, so
what the time is made of is reads. Both windows do the same number of reads,
because an uncached read on this host costs about the same at either size --
it is a device round trip, not a bandwidth question -- so equal read counts
keep the two tables comparable and roughly equal in wall time.

`read_heavy_narrow.wf` is the natural loop: one read per iteration into one
destination buffer, eight such loops so that each stays on one file.
`read_heavy_wide8.wf` states eight reads consecutively per round into eight
buffers. Their shared exclusive factory keeps the ordinary calls sequential. Read *k* of a run takes
its file and its window-aligned offset from *k* alone, so the narrow program,
the eight-wide one, and every native baseline traverse exactly the same list
and fold exactly the same value.

The 4 KiB eight-wide source is also the Windows measurement workload. Its
eight fixed file names arrive as command arguments and are copied through
`host_copy_bytes` before `open_file`: ten bytes on a one-byte target and twenty
UTF-16LE bytes on Windows. The no-overlap and default builds therefore receive
the same target-native component ranges without either a source fallback or a
runtime transcode.

The opens are inside the timed region — the runner times whole processes — but
there are exactly eight of them in every line, N, S, and C alike. At 116 us
each that is 0.93 ms against a table whose fastest line is over a hundred
milliseconds, and a constant every line pays identically cannot move a ratio.
This is what "open-once" buys: the open cost stops scaling with the work.

Each line folds the first sixty-fourth of every window rather than all of it,
and publishes the full transferred byte count beside the checksum. The reason
is in `workload.h`: the digest is a serial multiply-add chain running at about
800 MB/s, so folding a whole 64 KiB window costs about 80 us of CPU against a
134 us uncached read and a 7 us warm one. Folding everything would make the
warm table mostly compute. The retained prefix fold keeps both configurations
focused on I/O costs; widening the source no longer grants parallel reads.

### The four-stage chain

`chain.c` is a historical C comparison of the read/parse/request/write chain
in four forms: nested helping, thread compensation, managed stack switching,
and the former staged pipeline. It shares this bundle's ring plumbing and
file fixtures; its measurements remain in
`research/experiments/park-on-miss-measurements/`.

Its switch control depends on the deleted `wf_sched_core` and managed-record
interfaces. The retained `chain` target and historical runner therefore do
not build against the current-stack runtime. They are not canonical test
targets and must not be used to describe current lowering or current timings.
Re-measuring this comparison requires a separately identified historical
runtime checkout or a new, explicitly designed control; this amendment does
not recreate a managed stack implementation.

Every file is opened once, before the timed region, for the reason the
read-heavy workload opens once: an `openat` of a cold inode costs more here
than the read that follows it. The descriptors are opened `O_DIRECT` where the
filesystem allows it, and the printed line says which it was, because on a
buffered tree a read does not wait and no shape can reach any depth.

### WF_IO_NOCACHE

`WF_IO_NOCACHE=1` is a target-policy knob of the same class as `WF_IO_HELPERS`
and `WF_WORKERS`: it is never a language surface, no Whitefoot source names
it, and it changes no byte any program publishes — the `read-verify` target
checks every line's bytes with it off and on. What it changes is where a
read's bytes come from. On Darwin the runtime applies `fcntl(fd, F_NOCACHE, 1)`
to each descriptor an open hands back, which is a mode of the descriptor: every
read through it bypasses the unified buffer cache for the life of the open. On
Linux it applies one `posix_fadvise(fd, 0, 0, POSIX_FADV_DONTNEED)`, which
evicts what is cached at the moment of the open. `O_DIRECT` is deliberately not
used, because its alignment constraints would change the program's own buffers
and so change what is being measured.

The knob lives in `compiler/src/backend/completion/file_adapter.h` and is
applied by both the bounded POSIX adapter and the Linux io_uring adapter, once
per descriptor an open hands back and never to one a kind check refused.
`workload.h` mirrors it for the native baselines, so N and C wait on the same
device rather than on two different cache states.

### Proving a table is uncached

The knob alone does not make a table uncached. `F_NOCACHE` stops a read
populating the page cache; it does not evict a page that is already resident,
so a table run over a tree that was just written, or just read by a warm
table, is served from memory however loudly it asks for the device. Batch
an early table was published that way once; the correction
records it.

Three things now stand between that mistake and the table.

`make read-uncache` regenerates the tree so nothing is resident: the generator
writes each file through a descriptor that does not populate the cache,
flushes it, and on Linux drops its pages. That such a descriptor really keeps
its traffic out of the cache is measured rather than assumed — three passes
over the same eight blocks of a freshly generated file cost 248, 230 and
315 us on this host, with no drift towards the 7 us a resident page would
cost — which is what lets the probe below observe residency without creating
it.

`make read-warm` is the other half: it reads every block of every file back in
through plain descriptors, so a warm table has the state it claims. A full
sequential pass rather than a rerun of the workload, because 32,768
pseudo-random reads would leave about two per cent of the blocks untouched.

`make read-settle` then waits the host out. Writing half a gigabyte does not
only put bytes on the device: something on this machine reads the new files
back. A probe run every fifteen seconds on an otherwise idle machine watches
residency walk from the first file towards the last over a minute or two and
then vanish as the pages age out, with no benchmark line running and every
line reading through the non-populating policy; `XprotectService` holds about
a tenth of a core throughout. So a fresh tree is left alone until the same
probe the table is gated on passes quietly, which is a wait for an outside
reader to finish rather than a softer threshold. The gate probe below is
unchanged and still refuses the table.

`read_baseline probe-uncached` then checks the claim rather than trusting it.
It times sixteen positioned reads in each of the eight files, through
descriptors that do not populate the cache and at offsets that differ on every
invocation, and refuses the label unless all but ten per cent of those reads
cost more than 40 us — the gap between a 6-to-20 us cache hit and a 134 us
device read on this host. It runs immediately before and immediately after
every table, which is what catches both the tree being resident when a table
starts and something making it resident while the table runs. `probe-warm` is
the same check in the other direction, so the warm tables are labelled by
measurement too.

### The TCP echo workload

The control test `research/investigations/io-model/NETWORK.md` section 6 asks
for, and the bar it sets: the reference is the fastest existing solution
regardless of language, because the target is first place, and the gap is the
batch's result rather than something to hide.

Three servers, one contract, one load generator. The contract is what makes
the three comparable and what keeps the generator from telling them apart:
a server takes `PORT` and `CONNECTIONS`, listens on `127.0.0.1:PORT`, echoes
every byte of every connection back to that connection, and exits zero once
`CONNECTIONS` connections have been accepted in total and every one of them
has closed. A server that echoes the wrong bytes, drops a byte, reorders one,
or leaves a connection unserved fails the run instead of reporting a fast
time.

- **`uring_echo`**: the io_uring reference, and the number every other line is
  a ratio to. Written against the kernel ABI directly rather than liburing, as
  the read baselines are.
- **`epoll_echo`**: the second reference, the shape most deployed servers
  still have: one epoll instance and one `SO_REUSEPORT` listener per thread,
  edge triggered, reading until `EAGAIN` and carrying a per-connection buffer
  for what a short write leaves behind.
- **`waiting_echo`**: the runtime shape a `waits` program would compile to,
  written by hand in C: one waiting context per connection running
  straight-line receive-then-send code on its own small stack, pinned to a
  driver thread that owns one ring and one `SO_REUSEPORT` listener.
  `research/investigations/io-model/WAITS.md` states the question it answers,
  its criterion and its result. Experiment 2 there judges the compiled server
  against it at one driver thread; it is removed once no experiment compares
  the compiled runtime with the hand-written shape.
- **`wf_echo`**: `tests/programs/tcp_contexts.wf`, the maintained context
  server, built with the default flags: the entry accepts and each connection
  is served by a context of its own [PAR-4]. Its contexts run on one driver
  thread per CPU, as the C servers default to one thread per CPU; `wf1` is the
  same binary with `WF_DRIVERS=1`, which the C servers' `--threads 1` runs
  match.

`programs/context_starts.wf` measures what a context start and its join cost
apart from any I/O: a thousand batches of a thousand contexts that return at
once, each batch joined as its function returns, then a million bound starts
each joined by the next statement. It exits zero when the bound results add
up. `WAITS.md`, Experiment 5, compares it at one and four drivers
(`WF_DRIVERS`), timed with `/usr/bin/time`; it goes when no experiment
compares start costs.

What the io_uring reference does that a portable server cannot, which is what
the ratio is against:

- **multishot accept.** One `IORING_OP_ACCEPT` with `IORING_ACCEPT_MULTISHOT`
  per listener yields a completion per connection. There is no accept call and
  no submission per connection at all.
- **multishot receive.** One `IORING_OP_RECV` with `IORING_RECV_MULTISHOT` per
  connection yields a completion per arrival, so a server that is echoing does
  not submit a read between one message and the next.
- **a provided buffer ring.** `IORING_REGISTER_PBUF_RING` hands the kernel a
  ring of buffers and lets it choose the destination when the bytes arrive,
  rather than committing a buffer per connection before there is anything to
  put in it. The echo is then sent straight out of the buffer the kernel
  filled, so there is no extra userspace copy on either side. Exhaustion
  is real and is handled rather than avoided: a receive that finds no buffer
  answers `-ENOBUFS`, and that connection waits for a buffer to come back
  instead of spinning on a re-arm.
- **one ring per core.** Each thread owns its ring, its buffer ring and its own
  `SO_REUSEPORT` listening socket, so a connection is accepted, received and
  echoed on one thread with nothing shared on the path.
- **a ring the kernel need not interrupt.** `IORING_SETUP_SINGLE_ISSUER` with
  `IORING_SETUP_DEFER_TASKRUN` says one thread submits and the same thread
  waits, which lets the kernel defer completion work to the moment that thread
  asks for it. It is worth about a quarter of the small-message rate here, and
  it is what puts the io_uring line ahead of the epoll line at 64 and 1024
  connections instead of level with it.

`--sqpoll` adds `IORING_SETUP_SQPOLL` and is off by default. The development
host admits it; whether another host does is what `uring_echo --sqpoll` says
there, since a kernel that refuses the flag refuses it at `io_uring_setup` and
the server reports that and exits. The protocol does not run it, because a
poll thread per ring costs a core each and on a four-core host it loses to the
default by a third. It cannot be combined with the deferred task work above,
since there the submitting task is the kernel's own. The path publishes the
submission tail and then reads the poll thread's wake-needed flag with a
sequentially consistent fence between them, as liburing does: a release store
followed by an acquire load does not order a store against a later load, and
without the fence a published entry can sit unread behind a sleeping poll
thread. The path stays off by default and no measurement here enables it.

Everything each server needs is sized before the first accept, from
`CONNECTIONS` by default: the connection tables are indexed by descriptor, the
buffer rings are fixed arrays, and no server allocates per operation. The
io_uring reference's echo queue is a list threaded through the worker's own
provided-buffer records rather than a fixed per-connection array, so its depth
follows the buffers actually loaned to a connection instead of an assumption
about how TCP fragments a message; `WF_BENCH_URING_BUFFER_BYTES` and
`WF_BENCH_URING_BUFFER_COUNT` size that pool for an explicit experiment.

`netload` is the one generator all three are measured with:

    netload PORT CONNECTIONS ROUNDTRIPS BYTES [--threads T]

It opens `CONNECTIONS` connections spread over `T` client threads, each with
its own epoll, and times two phases. The connect phase runs from the first
connect call until every connection is established, which is the
connections-per-second measure. The exchange phase then has every connection
perform `ROUNDTRIPS` round trips of a `BYTES`-byte message with all
connections active at once, which is the round-trips-per-second, the
bytes-per-second and the latency-distribution measure. Every echoed byte is
compared with the byte that was sent, and a refused connect, a peer that
closes mid-exchange, or one wrong byte prints one line and exits nonzero. The
whole result is one line of tab-separated `key=value` fields, and the latency
samples live in one array of `CONNECTIONS*ROUNDTRIPS` 32-bit microsecond
values allocated before the first connect.

`linux-net-bench.sh` is the protocol, and one protocol for every host that can
run it, as `read-bench.sh` is for the read tables. It builds the compiler and
the three tools from one worktree, checks that every server echoes what the
generator sent at four connections before any of them reports a time, and then
runs the plan: four cases -- one connection at 20000 round trips, 64 at 2000,
1024 at 200, all of 64-byte messages, and then 64 connections at 200 round
trips of 65536 bytes for the bytes-per-second line -- across every server
line. A run picks a port below the kernel's ephemeral range, starts the server
for exactly that case's connection count, waits for the listening socket to
exist, runs the generator, and requires the server's own exit status to be
zero. Nothing here is decided by a timeout: the only waits are for a port to
appear and for a child to exit, and the wait for the port ends early if the
server process is gone, with the server's diagnostic channel printed.

`WARMUP` unrecorded passes are followed by `ROUNDS` recorded ones, a pass
being every line of every case once, with alternate passes in reverse order,
for the reason `runner.c` states: a host drifts over the minutes a table
takes, and a grouped schedule turns that drift into a difference between
lines. The table reports the median of each measure over the recorded passes
and the ratio of each line's round-trip rate to the io_uring reference and to
the epoll one. `NET_LINES` names a subset of `uring epoll wf` when one server
cannot complete a run yet and the others still owe a table; the table names
the lines it holds.

`redis-bench.sh` is Experiment 7 of
`research/investigations/io-model/SHARED.md`: the Redis subset
`tests/programs/redis_subset.wf` against `redis-server` with persistence off,
both driven by `redis-benchmark`. It builds the subset with the worktree's
compiler, requires every line to pass a correctness pass (100,000 increments
from 50 clients reach one counter, and a short command sequence answers as
Redis does) before any line reports a rate, and then runs `ROUNDS` interleaved
passes of `SET` and `GET` without pipelining and with 16 requests per
pipeline. The servers and the client are pinned to disjoint CPUs; the output
is `redis-benchmark`'s CSV line per run, prefixed with the server line, the
pass and the pipeline depth. `redis-samples.csv` holds the raw output of the
experiment's runs, including its attribution runs. Both are removed with the
experiment's record.

## Reproducing

    make -C research/experiments/io-completion-bench programs-check  # compile every program; the gate's `bench-programs` stage

    make -C research/experiments/io-completion-bench verify       # bytes only
    make -C research/experiments/io-completion-bench bench        # macOS table
    make -C research/experiments/io-completion-bench bench-pipe
    make -C research/experiments/io-completion-bench linux        # Linux table

    make -C research/experiments/io-completion-bench read-verify  # bytes only
    make -C research/experiments/io-completion-bench bench-read   # macOS tables
    make -C research/experiments/io-completion-bench linux-read   # Linux tables

    make -C research/experiments/io-completion-bench net-tools    # the C tools
    make -C research/experiments/io-completion-bench uring-check  # the reference's own traces
    make -C research/experiments/io-completion-bench net-verify   # bytes only
    make -C research/experiments/io-completion-bench linux-net    # the TCP table
    sh research/experiments/io-completion-bench/redis-bench.sh    # the Redis subset

The TCP targets are Linux-only, as `linux` and `linux-read` are: `epoll_echo`
and `uring_echo` are written against Linux interfaces, and the workload's
point is the fastest shape that kernel offers.

`uring-check` is the one target here that measures nothing. It builds
`uring_echo_check.c`, which includes `uring_echo.c` and replaces only kernel
setup, `io_uring_enter` and the synchronous send result with a deterministic
fixture: the buffer-loan queue, the receive re-arm after exhaustion and the
completion handling those traces drive are the ones the measured binary runs.
It therefore decides a verdict about the reference without a kernel that
supports io_uring and without a network. It still requires Linux headers and
runs through `programs-check` in the Linux gate. It is built for both send
policies, because an inline send and a ring send retire a queue prefix along
different paths.

On native Windows, `windows-bench.ps1` records matched runtime measurements:

    pwsh research/experiments/io-completion-bench/windows-bench.ps1 \
      -Root $PWD -Out $env:TEMP/whitefoot-windows-bench \
      -Rounds 15 -Warmup 2

It builds every contender from one compiler revision, generates the same eight
64 MiB deterministic files as the other read-heavy protocols, warms them with
a complete sequential pass, and pins every sampled child to one recorded
processor mask. `windows_runner.c` takes each sample with
`QueryPerformanceCounter` and `GetProcessTimes`; a nonzero exit, a byte of
stderr, or stdout different from the committed exact oracle invalidates the
sample before its time is reported. A sampled child or the untimed observer
that exceeds two minutes is terminated and invalidates the run.

Before any timed cohort, the no-overlap and default 4 KiB read-heavy builds
must both publish the exact oracle. The five alternating paired cohorts are
compute (`par_layout.wf`, default against `--par`), warm 4 KiB reads
(`--no-overlap` against default), mixed no-overlap/default, mixed default/`--par`,
and mixed no-overlap/`--par`. These names describe compiler modes. Both read
builds use the same ordinary linked read implementation; `--no-overlap` does
not select a separate direct-read body.

C2's [ordinary-host-values amendment](../../investigations/ordinary-host-values/DESIGN.md)
deletes PAR-3 and its suspension/completion classification. The mixed source
window remains `read_at, compute_pair, read_at`, but each ordinary read returns
before the next statement executes. Its exclusive factory also participates
in the ordinary effect and loan rules. The script checks the direct-call order
inside `wf_exercise`, rather than a build launcher or operation-specific
submit/join lowering. `compute_pair` still contains the independent
`churn, churn` pair; its ordinary scheduler lane must acquire, publish, join
and release in that order. The exact tree oracle remains
`17574306422404092952\n`.

A second check uses an observed link: the same ordinary prelude C bodies and
LLVM callable wrappers as `whitefootc`, their private native dependencies,
and `grant_observer.c`. It requires the exact output oracle, exactly one
`grants=` diagnostic with a positive count, and exit zero under
`WF_REQUIRE_WINDOWS_IOCP=1`. The last condition is the linked runtime's own
assertion that the completion port carried at least one submission and reaped
every submission it made. It observes the selected native implementation;
it grants no source-level early result, loan release or overlap permission.
The emitted launcher defines C `main`, so its link omits `-municode`.
The native sample runner retains `-municode` for its own `wmain`. Both Winsock
and shell32 dependencies of the ordinary linked library are present.

The compute pair gives both builds three inert command arguments.
`par_layout.wf` counts the complete invocation vector, including the invoked
name, and therefore runs four identical batches in one process. Every batch
resets both fold seeds, so the exact 34-byte oracle is unchanged. Ordinary
argument-free corpus runs still execute one batch.

Each cohort records fifteen candidate/reference and native/reference ratios after two unrecorded warm-up rounds. The order balances positions and precedence; optional worker-count comparisons share each serial reference and native control. All raw samples remain in `raw.tsv`. The summary reports medians, paired ratios, MAD and percentile spread, including spread relative to the native control. The 5 percentage-point MAD and 10 percentage-point width margins describe stability only: a wide spread is reported without retrying to select a better cohort or failing CI. Speed ratios do not select success. Correct output, ordinary scheduler activity and the observed IOCP path remain required.

Earlier Windows results were collected under a different protocol: PAR-3 kept
a read outstanding across compute and the following read, and `-Enforce`
required compute <= 0.90, warm I/O <= 1.10, and mixed <= 0.95 relative to its
two controls, with a stability retry. Those bounds and the cross-call overlap
assertion are retired because C2 deletes that source behavior and explicitly
requires measurements rather than performance gates. Historical tables remain
historical; the new mode labels and ordinary call order must accompany new
samples. Losing the old overlap is an outcome to measure, not a failed attempt
to implement the old rule.

The host, Windows build, CPU, processor mask, memory, power scheme, toolchain,
revision and every raw sample ship with the table. A hosted VM is not treated
as a persistent cross-revision hardware baseline.

The hosted measurement uses one fewer compute worker than the affinity
mask's logical-processor count, with a minimum of two: W=3 on the four-logical
runner. The full mask remains available to every child. This leaves scheduler
capacity for other VM work; it does not reserve an exclusive core or establish
the physical host's SMT topology. The summary records both the visible count
and the guest's reported cores/logical processors. Process priority is normal.
All five cohorts, fifteen Whitefoot pairs and two warmups are retained.
There is no stability retry and no numerical performance gate.

The native control reuses the existing safe Rust/Rayon layout twin in
`research/investigations/proof-derived-parallelism/bench/rust/`, built with
loop and SLP vectorization disabled. It runs the same reduced worker count,
normal priority, full mask and QPC runner, and must publish the known exact
layout fold oracle before its time counts. Compute uses three reset-seed
full-width batches in one pool; the four shorter cohorts use one half-width
batch. It witnesses concurrent CPU availability during each cohort; it is
not an I/O throughput baseline or an excuse for variation specific to I/O.
The summary keeps raw wall and paired distributions visible beside the
relative margins. The additional control samples add no Whitefoot pair or
retry, and the job duration remains observable on the hosted runner.

For same-head before/after evidence, dispatch `io-bench` with
`compare_windows_workers=true`, or pass `-CompareWorkers` to
`windows-bench.ps1`. That diagnostic runs only the Windows job. Each of the
fifteen rounds shares one sequential reference and one reduced-count native
control between full-count and reduced-count candidates. A four-treatment
Williams order balances positions and immediate precedence every four
rounds; fifteen rounds differ by one row. It
uses the existing allowance of two candidate cohorts, with fewer reference
children and no retries. The IO-only candidates repeat their unchanged
configuration because they do not use `WF_WORKERS`. The table prints both
policies' raw paired MAD/median and (p90-p10)/median; the full-count rows are
diagnostic; the reduced-count rows report the same native-relative stability
margins. Neither row is filtered by speed or stability. `raw.tsv` records the policy's `worker_limit`, including
on the shared reference; it is not a count of threads actually running.
The [selection criterion](../../investigations/io-model/RESULTS.md#windows-hosted-worker-comparison-criterion-2026-09-11)
records the measured worker-count tradeoff, its later failure, and the
criterion and measured grounds for the native-relative protocol. On the
recorded four-logical-processor guest, the earlier protocol passed every
cohort in 6m18s including build; mixed-total still exceeds the old absolute
spread bound. Neither a passing relative margin nor these finite samples
guarantee the absence of host noise or identify every slow sample's cause.

`linux` builds `linux.Dockerfile` and runs the whole pipeline inside one
container, because the generated tree must sit on a container-local
filesystem: measuring a bind mount would measure the host's file sharing
rather than the kernel's I/O path. It passes
`--security-opt seccomp=unconfined`, without which `io_uring_setup` is refused
and both the native baseline and Whitefoot's own Linux adapter silently fall
back.

`linux-read` runs the read-heavy tables in the same container for the same
reasons, and adds the `io_uring` baselines at depths 4, 8, and 32.

It does that by running `read-bench.sh`, which is one protocol for every host
that can run it: the container, the project's Linux runner, and its macOS
runner. Only paths and the host's own capabilities differ -- `ROOT`, `OUT`,
`CLANG` and `CARGO_TARGET_DIR` name the paths, and `uname -s` decides whether
the io_uring lines are in the plan. The `io-bench` workflow's
`bench-linux-read` and `bench-macos-read` jobs run exactly those bytes; that
workflow runs on demand. Benchmark-program compilation and native host
correctness remain automatic; full storage timing matrices are experiments. Those
Linux, macOS and Windows tables report performance; none selects acceptance
by a speed ratio.

The script differs from `bench-read` in one deliberate way. `bench-read`
refuses to print a table whose cache-state label the probe did not confirm,
which is the right rule on a machine whose two populations are known and far
apart. A hosted runner's storage is not known in advance, and its "device" may
be a host-cached network disk that answers faster than the threshold; there
the honest result is a table labelled by what was measured rather than no
table at all. So the script always runs the probe, always prints its per-file
medians and its verdict, and prints that verdict on the table's own label
line.

The two probes around a table are not worth the same on both hosts, and the
script says so where it runs them. Before the table each host is making the
same claim and the probe checks it. After it, only Darwin is: `F_NOCACHE` is a
mode of the descriptor, so a Darwin table that asked for uncached reads cannot
have populated the cache itself, and a refusal there means something outside
the benchmark made the tree resident. Linux has no per-descriptor mode; its
one lever evicts at open and nothing later, so every Linux line starts from a
cold tree and warms it as it reads. The Linux after-probe measures how far
that went and is reported as that, not as a verdict.

`FILES`, `MAX_KIB`, `ROUNDS`, `WARMUP`, `PIPE_ROUNDS`, and `PIPE_DELAY_US`
override the many-files shape. `FILES` or `MAX_KIB` change the checksum;
`make expected` prints the new one for `EXPECTED`. `READ_FILES`, `READ_KIB`,
`READS_64K`, `READS_4K`, and `READ_ROUNDS` do the same for the read-heavy
shape, and `make expected-read` prints the two checksums they imply.

`bench-read` prints four tables: 64 KiB and 4 KiB, each with the page cache
warm and with `WF_IO_NOCACHE=1`. Both cache states publish the same bytes,
which `read-verify` checks before any line reports a time, and each table's
cache state is checked by the probe above before and after it runs.
`READ_PROBES`, `READ_THRESHOLD_US`, and `READ_TOLERANCE_PERCENT` set that
check.

This bundle is deliberately not reachable from the repository's canonical
`make check`. It generates a large tree and runs for minutes, so correctness
builds do not depend on a performance host. The dedicated `io-bench` workflow
owns the Windows observations and the exploratory Linux/macOS tables.
Generated trees, binaries, and raw output stay in the selected scratch
directory; durable results retain the host identity and raw artifact beside
their summarized table.
