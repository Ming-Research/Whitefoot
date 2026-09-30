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

The subset, `tests/programs/redis_subset.wf` as it stood at `32a0cbf87`
before firn replaced it, answers 4 of the suite's 20 tests: `PING_MBULK`,
`SET`, `GET` and `INCR`. Fourteen fail on a command it does not know. `PING_INLINE` and `MSET` fail because it closes the connection
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

`apps/firn` is a module program of six modules and about 4,900 lines;
[its README](../../../apps/firn/README.md) lists them. Each choice below keeps
Redis's observable behavior on the suite's commands and says what it refused.

- **Keys and values are byte strings of their own length**, each a
  `Box<Slots<u8>>`, where the subset kept keys in 48-byte arrays and values up
  to 1,024 bytes. Redis limits neither below 512 MB, and hashing only a key's
  own bytes is cheaper than hashing 48 positions.
- **A key's value is one enum over the five kinds**, `Text`, `Items`,
  `Members`, `Fields` and `Ranked`, beside its expiry, so a command on another
  kind finds the variant and answers `WRONGTYPE`, as Redis does.
- **The kinds are the standard library's containers**: a list is a ring
  (`std::collections::deque`), a set and a hash are hash maps, and a sorted set
  is an ordered map of score and member beside a hash map from member to score.
  Containers written for firn were refused: they would duplicate the library,
  and a gap the library shows is one to fix there. Redis's compact encodings
  for small values (listpack, intset) have no counterpart, which costs memory
  and possibly time; the measurement shows how much.
- **Each connection has one `Client` record** that the command passes into its
  atomic statement as the visitor's environment: the reply window, which grows
  to any size, the strings and scores the command copied out of the request
  before the statement, and the numbers a visitor reads and reports. The
  library's containers reach a stored value only through a callback with one
  environment, so the record is that environment; copying the arguments
  before the statement keeps it to the keyspace's own work. Taking an entry
  out of the keyspace and putting it back, which needs no callback, was
  refused: it costs two hash operations per command where the callback costs
  one.
- **A command is found by a number packed from its name's first eight
  letters**, compared once per known command, rather than by comparing names
  letter by letter.
- **The append-only file records each change as Redis 7 records it**: a
  command as it was sent, an expiry as `PEXPIREAT`, a `SET` with an expiry as
  `SET` then `PEXPIREAT`, and an `SPOP` as the `SREM` of the members it chose,
  so that a replay removes the same members. `SPOP` draws from a generator
  seeded by the clock when the server starts, as Redis seeds its own.
- **Sorted-set scores are integers below 2^52 in magnitude.** Every score the
  suite sends is one, and Redis prints such a score as the integer. A score
  written as another floating-point number is refused with an error that says
  so: reading one to the nearest double and printing it with 17 significant
  digits, as Redis does, needs exact decimal conversion firn does not have
  yet.
- **A malformed request is answered with Redis's protocol error and the
  connection is closed**, as Redis closes it, where the subset closed it
  without an answer.

Building firn found three things outside the program, recorded under
[docs/todo.md](../../../docs/todo.md) unless fixed:

- **A lowering defect, fixed.** A borrowed match binder read or written through
  and then passed to a call was passed as a load of its field instead of the
  field's address, and the backend refused the result as invalid IR.
  `tests/programs/borrowed_binders.wf` fails to build without the fix.
- **A generic operation called inside its own callback with other arguments
  is refused as polymorphic recursion** [FN-6], although the instantiations
  end: a hash map's lookup whose callback looks up a field in another hash
  map. firn alternates `hash_map_edit` and `hash_map_lookup` instead.
- **No command renders a program in canonical form** [FORM-2], which every
  program must be in; writing firn needed the renderer the corpus test calls.
