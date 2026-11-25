# Echo-server scaling with core count on Linux

## Question

Does a server written in Whitefoot, whose connections are served by contexts
that run on drivers [WAIT-3], get faster as it is given more cores on Linux?
Concretely: with one echo server and the same client load, does the
throughput rise at least 1.6 times each time the number of server cores
doubles, from 1 to 8 cores?

The question belongs to the runtime alone. Firn's own many-core runs mix
Firn's source with the runtime, so a slow curve there cannot say which one is
slow; this record measures a server with nothing but receive, send and a
context per connection in it. The scope is Linux concurrent I/O. macOS and
Windows are not measured: they start no second driver today. Automatic
parallelism of computation, and the gap between a context-written server and
a hand-written one, are separate questions and use no result from here.

Earlier evidence is the four-CPU sweep of
`research/investigations/io-model/WAITS.md` Experiment 6, which ran the load
generator on the same four CPUs as the server. It says the rate rises with
drivers (at 1024 connections 84,119, 197,629, 177,563 and 216,052 round trips
a second at 1, 2, 3 and 4 drivers) and that three drivers fell below two; it
could not say whether the generator or the runtime was the limit, and asked
for a host with CPUs to spare for the generator.

## Criteria, fixed before the first measured run

The reference programs, the threshold and the validity rules below were
written, and committed, before any sweep of this record ran.

**Subject.** `tests/programs/tcp_contexts.wf`, the maintained echo server: the
entry accepts and each connection is served by a context of its own. It is
built with the repository's compiler and started with `WF_DRIVERS=n` under
`taskset` on n CPUs.

**References.** The two C servers of `research/experiments/io-completion-bench`
(the TCP echo workload): `uring_echo`, the io_uring server with one ring and
one `SO_REUSEPORT` listener per thread, which is the fastest shape this kernel
offers for the workload; and `epoll_echo`, the shape most deployed servers have.
Both run at the same core counts with the same client, `--threads n` under the
same `taskset`. The reference set is these two C servers, with no Rust or C++
language-runtime server (the owner's question on the set was left to its
recommendation). A hand-written C server uses
the same kernel interfaces a C++ or Rust one does, so the curve is a ratio to
the best such server, not a measurement of any one language runtime.
`uring_echo` is also the harness control below.

**Threshold.** Whitefoot's median round trips a second at 2n cores is at least
1.6 times its median at n cores, for 1 to 2, 2 to 4 and 4 to 8. The figure is
the one the owner gave with the item, the doubling threshold Firn's many-core
goal uses; it is not stated in `research/investigations/firn/DESIGN.md`. The
ratios to each reference at every core count are reported and carry no
threshold.

**Load.** Loopback `127.0.0.1`, 1,024 connections at every core count, each
making 2,000 round trips of 64 bytes, all connections active at once. The
server is started for exactly that many connections and must exit zero once
they have all closed. The load generator is `netload`, which compares every
echoed byte with the byte it sent.

**Placement.** The server on n CPUs, one logical CPU to each of n distinct
physical cores, all of one performance level: the performance cores of the
i9-14900K, using the placement rule of `placement.sh` that the Firn
measurements use. Its reservation of CPU 0's core is kept for every n the
host can serve without it, and given up only for the n that needs every
performance core (n = 8, the machine having eight). The client is one set of
CPUs for every n, the efficiency cores, sharing no physical core with any
server CPU, with one client thread to each CPU.

**Passes.** One warm-up pass, then at least five recorded passes; a pass runs
every line at every core count once, the order reversed on alternate passes, so
the drift of the host over the minutes a sweep takes does not become a
difference between core counts. A cell is the median of its recorded passes,
with its minimum and maximum reported beside it; the threshold is judged on the
medians and the range says how far to trust it.

**Validity.** A pass is thrown away, and the run is invalid, if

- the server or the load generator exits nonzero, or a byte, a connection or a
  reply is wrong or missing (the load generator then exits nonzero);
- the Whitefoot server did not run n drivers, as its exit report
  (`WF_SCHED_REPORT=2`) shows; one core may print no report at all, because the
  runtime prints none when one driver ran and nothing was handed over;
- the busiest client CPU in the exchange phase was busier than 80 percent,
  which `netload` reads from `/proc/stat` between the end of the connect phase
  and the end of the exchange; or
- fewer than five valid passes of a cell remain.

Nothing is judged from an invalid pass, and the summary
(`echo-scale-summary.sh`) fails on it. Its own check
(`echo-scale-summary-check.sh`) feeds it fabricated samples first: a curve of
1.9 per doubling reaches, one of 1.3 beside a control of 1.9 is a miss, one of
1.3 beside a control of 1.3 is limited by the harness, and a busy client, a
nonzero exit, a wrong driver count, too few passes, a missing control line and
a short row are each rejected for their own reason; its rows are the ones the
sweep's own row writer produces, so the writer and the reader agree on the
twelve columns. `placement-check.sh` runs the placement rule on a fabricated
host shaped like the machine and checks the placements it must give and the
ones it must refuse.

**What the result can be.** For each doubling of the subject:

- *reached*: at least 1.6 times.
- *harness-limited*: below 1.6, and the `uring_echo` control is also below 1.6
  at the same doubling. The host or the client limits the curve, so it says
  nothing about the runtime; the client CPUs, the client threads and the
  connection count are changed and the sweep run again.
- *miss*: below 1.6 while the control reached it. The runtime is slow. The
  cause is found with the per-driver exit report and a profile, fixed in the
  runtime, and the sweep run again.

The claim is rejected by any *miss* that stays after the fix attempts, by any
lost byte, connection or reply, and by a result whose cells are invalid. The
threshold is not changed to fit a measured curve.

## Method

`linux-net-bench.sh scale` (`make net-scale` in the experiment's directory)
builds the tools and the server, runs the correctness pass of the TCP echo
protocol, runs the sweep and prints the summary. The `scale_echo` input of the
`io-bench` workflow runs it on the runner `compare_runner` names; the
owner's i9-14900K is the machine of record (a Hyper-V virtual machine with 32
virtual CPUs). The host's kernel, CPU model, performance- and efficiency-core
lists, per-CPU frequency, and the CPU lists each core count used are written
to `scale-host.txt` beside the samples, and go into the result below.

## Result

Not measured yet: the criteria above are committed before the sweep. The
measured table, its date, the machine, the run id, the revision and the ranges
go here, and the raw samples go to
`research/experiments/io-completion-bench/echo-scale-14900k-samples.tsv`.
