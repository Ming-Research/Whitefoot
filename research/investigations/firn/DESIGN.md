# firn: a Redis-compatible server written in Whitefoot

## The question

Can a server written in Whitefoot run Redis's own benchmark and lead Redis and
its competitors on it, and what language, library, compiler or runtime gaps
does reaching that expose? The server is `apps/firn`, which grows out of the
Redis subset of Experiments 7 and 8
([SHARED.md](../io-model/SHARED.md#experiment-7-a-redis-subset),
[TIME-AND-FILES.md](../io-model/TIME-AND-FILES.md)).

## The owner's rulings

All on 2026-09-30:

- **The goal.** Run Redis's benchmark and lead Redis on it, and ideally every
  competitor. A server offered as a Redis clone must be deployable by anyone
  who wants to try it, so its features must suffice; running the benchmark is
  necessary and not sufficient.
- **The benchmark** is `redis-benchmark`'s default suite, the 20 tests of
  version 7.0.15, each at pipeline depths 1 and 16, every server under the same
  load and pinning, interleaved.
- **The lead** is the one measured today, kept.
- **The competitors** are Redis 7.0.15, Valkey 7.2.13 with and without I/O
  threads, Dragonfly 2.0.0 and Garnet. Redis 8 and Valkey 8 are not
  obtainable on this host: neither is packaged for it, and the network policy
  refuses their source archives.
- **The host** is this 4-CPU one; the owner's i9-14900K measures the result
  once the work is done.
- **Scaling past two cores** is a later investigation: one shared keyspace is
  one lock, and a sharded keyspace raises a language question, a command over
  several keys in several shards, that this stage does not answer.
- **The name** is firn, snow that has lasted a season: stored and compacted.
  It lives in `apps/firn/`, a new repository-root directory for deployable
  programs written in the language.
- **The features** of this stage are the commands the benchmark needs; what
  suffices for deployment is set later.

## Criteria, stated before measuring

The baseline below translates "the lead measured today" into numbers. A test's
rate is the median over at least three interleaved passes; the fastest
competitor is the one with the highest median on that test and depth.

- **Correct.** Every one of the 20 tests completes with no error reply, and
  for each command the benchmark sends, a scripted sequence answers exactly as
  `redis-server` answers it.
- **The pipelined lead.** At depth 16 firn reaches, on every test, at least
  1.4 times the fastest competitor's rate on two server CPUs and at least 1.1
  times on one. The baseline's `SET` and `GET` reached 1.49 and 1.50 times by
  mean on two CPUs, and 1.40 and 1.44 when firn's slower pass meets the
  competitors' fastest one; on one CPU, 1.14 and 1.15 by mean.
- **Without pipelining firn is not behind.** At depth 1 firn reaches at least
  the fastest competitor's rate on every test at both CPU counts. On one CPU
  the baseline led by 1.10 and 1.14 by mean; on two it trailed Dragonfly, at
  0.91 for `SET` and 0.95 for `GET`, so this criterion asks for more than the
  baseline held.
- **Latency.** At depth 16 firn's median p99 is no higher than the fastest
  competitor's, as it was in the baseline: 1.85 to 3.24 ms against 2.71 to
  4.06 ms.

Each run lasts at least ten seconds, since `redis-benchmark` with several
threads reports a rate from a clock that advances in steps of about a quarter
second: the baseline's runs of 2.5 to 4 seconds read to within 6 to 10
percent. The suite runs with its defaults, 50 clients and 3-byte values, plus
`-r 100000` so that keys, members and fields are drawn from 100,000, and with
`--threads 2` on two client CPUs or `--threads 3` on three. A criterion that
fails is attributed with a profile before any conclusion is drawn from it.

## The baseline

The subset of `tests/programs/redis_subset.wf` answers 4 of the suite's 20
tests: `PING_MBULK`, `SET`, `GET` and `INCR`. Fourteen fail on a command it
does not know. `PING_INLINE` and `MSET` fail because it closes the connection
on an inline command and on a command of more than five arguments, where Redis
answers them.

On the commands it has, measured against the competitors at depth 16 with
16-byte values and keys drawn from 100,000, requests per second, two passes;
the raw lines, with the exact settings, are
[firn-samples.csv](../../experiments/io-completion-bench/firn-samples.csv):

| Server CPUs | Test | Subset | Redis | Valkey | Valkey, I/O threads | Dragonfly |
|---|---|---|---|---|---|---|
| 2 | `SET` | 799,360 / 799,680 | 499,500 / 443,754 | 443,262 / 470,035 | 469,704 / 443,853 | 499,625 / 570,939 |
| 2 | `GET` | 888,494 / 888,099 | 570,939 / 614,628 | 570,776 / 532,623 | 570,451 / 499,376 | 532,907 / 614,817 |
| 1 | `SET` | 614,251 / 532,340 | 469,814 / 532,340 | 469,704 / 484,614 | not run | 362,911 / 332,557 |
| 1 | `GET` | 614,628 / 614,628 | 498,629 / 570,776 | 532,623 / 469,814 | not run | 420,080 / 362,845 |

Without pipelining every server ran between 66,504 and 119,904 requests per
second. The subset carries a handful of commands with keys of at most 48
bytes, where the others carry a full command table, statistics and encodings,
so the lead may narrow as firn gains the suite's value types; keeping it
anyway is what the criteria ask. One server CPU is the configuration where the
client has the most room, and Dragonfly, built to spread over many cores, is
slowest there.

## Stages

1. **This stage.** Move the subset into `apps/firn`, answer the protocol's
   shapes, add the suite's value types and commands, measure against the
   criteria and close what they find.
2. **Deployment.** The owner sets which features suffice for deployment.
3. **Scaling.** The keyspace past two cores, measured on the i9-14900K.

## Design of the program

(Choices are recorded here as they are made, each with its reason and the
alternative refused.)
