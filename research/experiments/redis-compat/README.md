# Redis's test suite against firn

Dated 2026-10-03. A harness that runs the test suite of Redis 7.0.15 against
one server and counts, per unit of the suite, the tests passed, failed,
errored, skipped and timed out, with the cause of every failure; its first
results, on Redis 7.0.15 itself and on firn at `cea9188d4`.

## Question

How much of Redis's own test suite does [firn](../../../apps/firn/README.md)
pass? firn is meant to be deployable in place of Redis
([firn](../../investigations/firn/DESIGN.md#the-owners-rulings)), and the suite
Redis runs on itself is the widest description of Redis's behavior there is.
Its pass count, unit by unit, is a measure later work on firn can report as
numbers and move.

The bundle serves that measurement of firn and lives here, beside the firn
measurements of [io-completion-bench](../io-completion-bench/). It goes when
a maintained test of firn's compatibility replaces it, or when firn no longer
aims to be deployable in place of Redis.

- [`run.sh`](run.sh) is the entry point: it fetches and checks the suite,
  starts the server under test anew for every unit, runs the unit, runs it
  again past a test that hangs, and calls `summarize.py`.
- [`summarize.py`](summarize.py) reads a run directory and writes `tests.tsv`,
  `units.tsv` and a report. The suite prints only a log meant for people,
  whose closing list of failed tests has neither counts per unit nor causes,
  so the counting needs a parser; it reads only the suite's output and does
  not depend on the compiler.
- The result files are the `tests.tsv` and `units.tsv` of the runs below.

## Method

### The suite

`redis-7.0.15.tar.gz` from download.redis.io, SHA-256
`98066f5363504b26c34dd20fbcc3c957990d764cdf42576c836fc021073f4341`, the value
Redis publishes in its hash list (github.com/redis/redis-hashes). 7.0.15 is
the version of the comparator the firn investigation measures, Ubuntu 24.04's
`redis-server` package. `run.sh` keeps the archive in `REDIS_COMPAT_CACHE`,
`~/.cache/whitefoot-redis-compat` by default, fetches it when it is missing,
checks its SHA-256 on every run and unpacks a fresh copy of the suite into the
run directory. Tests that run the suite's own `redis-cli` or `redis-benchmark`
find the installed 7.0.15 tools linked into the copy's `src/`, where a built
source tree would have them.

### External mode and the flags

`runtest --host H --port P` runs the suite against a server it did not start.
It skips every test and block tagged `external:skip`: replication,
persistence, configuration files, server logs, process signals and the like,
which need a local server process. It then uses one test client. `run.sh`
passes these flags: three of the options the suite's `tests/README.md` lists
for a server configured differently from Redis, then two that keep a run
going:

- `--singledb`: use database 0 only and never `SELECT`; it also skips the
  tests tagged `singledb:skip`. firn has one database.
- `--ignore-encoding`: skip the assertions on Redis's internal encodings of a
  value, read with `OBJECT ENCODING`.
- `--ignore-digest`: skip the comparisons of `DEBUG DIGEST` and
  `DEBUG DIGEST-VALUE` checksums.
- `--durable`: an error inside a test fails that test instead of ending the
  run.
- `--timeout 120`, described under [Hangs](#hangs).

### One unit at a time

Each of the 84 units the suite lists runs in its own `runtest --single`,
against a server started for that unit alone in an empty directory, so that
one unit's crash, hang or saved state cannot reach the next. The empty
directory matters: in the first full run the servers shared one, and the
server of `unit/lazyfree` loaded at startup the RDB file a test of
`unit/functions` had saved, with a function library in it. With that file in
place, `UNLINK can reclaim memory in background` failed in five runs of the
unit out of five, and a replay of the test's steps left the memory after its
`UNLINK` at 2.13 MB against 1.03 MB at the start, over the test's limit of
twice the start; without the file the replay returned to 1.00 MB from
0.94 MB, and the test has passed in every run since. These checks were made
while building the harness, and their output is not kept.

The suite listens for its own test client on the port 32 below `--baseport`,
21079 by default. Its check that a port is free binds the wildcard address,
so it misses a port another process holds on 127.0.0.1: two `runtest`
processes on one host collided on 21079. `run.sh` therefore picks that port
itself, as it picks the server's, from below the kernel's range of ephemeral
ports: once a port in that range was held by an outgoing connection, which
does not listen, and the suite could not start. An attempt whose suite did
not start is made again, up to three times. A server that does not answer
`PING` within 10 seconds of three starts ends the whole run with exit status
1 and no summary; `summarize.py RUN_DIR` then summarizes the units run so
far.

### Hangs

The suite's `--timeout` is not a limit per test: when no test starts or ends
for that many seconds, the suite prints the test in progress and ends the
whole run. Hence one unit per `runtest`: a hang ends only its unit. `run.sh`
then runs the unit again with the stuck test skipped (`--skipfile`), up to
`--retries` times, 5 by default, and counts the stuck test as timed out; a
test of the same name is skipped with it. 120 seconds is three times the
slowest test that passes on Redis here, 39 seconds for
`Fuzzing dense/sparse encoding: Redis should always detect errors`. A unit
stuck outside any test, in a block's own code, is not run again and is
reported as stalled. A whole unit also stops after an hour, a backstop that
no run below reached.

### What the framework needs from the server

The framework itself, apart from any test, sends these commands to an
external server (`run_external_server_test` in `tests/support/server.tcl`,
`test` in `tests/support/test.tcl`):

- at the start of every `start_server` block, nested blocks included:
  `SELECT 9` unless `--singledb` is given, then `FLUSHALL`, then
  `FUNCTION FLUSH`;
- for every configuration override the block declares: `CONFIG GET` of the
  parameter and `CONFIG SET` of the new value before the block, then, when
  the override turns `appendonly` on, `INFO` until no rewrite of the
  append-only file is in progress, and `CONFIG SET` of the saved value after
  the block;
- before every test, on a new connection: `DEBUG LOG` with the test's name.

An error from any call in the first two groups is outside the framework's
`--durable` handling: it ends the unit, and no test of the unit after it runs.
An error from `DEBUG LOG` is ignored. The suite's helpers add `PING` when they
open a client under `--singledb`, `INFO` for every server property a test
reads, `OBJECT ENCODING` unless `--ignore-encoding` and `DEBUG DIGEST` unless
`--ignore-digest`; those are calls of the tests that use them.

A probe confirmed the framework's part on Redis: a unit of one block with one
override and two empty tests,

```tcl
start_server {tags {"probe"} overrides {save ""}} {
    test {probe one} { }
    test {probe two} { }
}
```

saved as `tests/unit/compat-probe.tcl` in a copy of the suite and run with
`./runtest --host 127.0.0.1 --port PORT --single unit/compat-probe --singledb
--durable` against a fresh `redis-server`, left `INFO commandstats` with
exactly `flushall` 1, `function|flush` 1, `config|get` 1, `config|set` 2 and
`debug` 2, and the server's log with one `DEBUG LOG` line per test. Without
`--singledb` it added `select` 1. The `INFO` after an `appendonly` override
showed in an earlier `--tolerant` run, not kept, that let only the other
five calls fail: there `unit/type/stream` stopped at it with an exception.

### The comparator

`run.sh redis` starts `redis-server` with `--port PORT --dir DIR --bind
127.0.0.1 --save '' --appendonly no --enable-debug-command yes`, `DIR` being
the empty directory above. The last flag is a setting of the suite's own
`tests/assets/default.conf` that Redis 7 does not have by default: Redis 7
refuses `DEBUG` unless it is enabled, and the suite's tests use it.

## Commands

From the repository root of a Linux host, with bash 4.4 or later, GNU
coreutils and sed, curl, `tclsh` 8.5 or later (Ubuntu's `tcl` package),
`redis-server` and `redis-cli` 7.0.15 (`redis-server` and `redis-tools`) and
`python3`. firn is built as
[its README](../../../apps/firn/README.md#build-and-run) says; the runs below
used the compiler that `make -C compiler build` builds, which the gate profile
places in `compiler/target/gate/`:

```sh
make -C compiler build
compiler/target/gate/whitefootc --graph apps/firn/modules.wfg --entry firn \
  -o <scratch-root>/firn-cea9188d4
```

Each run holds the host lock and took here from half a minute (firn's
baseline) to five minutes (Redis), and 16 minutes when tests hung (the
`DEBUG` control); a limit above the lock's default 30 minutes leaves room for
hangs:

```sh
export WHITEFOOT_CHECK_TIMEOUT=10800
exp=research/experiments/redis-compat
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/redis redis
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/firn \
  --reference $exp/redis-7.0.15-tests.tsv firn <scratch-root>/firn-cea9188d4
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/firn-tolerant \
  --tolerant --reference $exp/redis-7.0.15-tests.tsv firn <scratch-root>/firn-cea9188d4
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/redis-noflush \
  --tolerant --reference $exp/redis-7.0.15-tests.tsv redis --rename-command FLUSHALL ''
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/redis-nodebug \
  --reference $exp/redis-7.0.15-tests.tsv redis --enable-debug-command no
# The control with no command but PING: every other top-level command
# renamed away; the names come from any running redis-server 7.0.15, here
# on port 6379.
args=()
for c in $(redis-cli -p 6379 command list | grep -v '|'); do
  [ "$c" = ping ] || args+=(--rename-command "$c" '')
done
[ "${#args[@]}" -gt 0 ] && perl .github/run-check.pl redis-compat $exp/run.sh \
  --out <scratch-root>/stub --tolerant --reference $exp/redis-7.0.15-tests.tsv \
  redis "${args[@]}"
# A server that is already running, not restarted between units:
perl .github/run-check.pl redis-compat $exp/run.sh --out <scratch-root>/external \
  external 127.0.0.1 6379
```

A run directory keeps every unit's output under `logs/`, its server's under
`servers/`, `attempts.tsv`, `meta.txt`, and what `summarize.py` writes:
`tests.tsv`, `units.tsv` and `summary.txt`. `summarize.py RUN_DIR
[--reference TESTS_TSV]` summarizes a directory again. The result files here
are copies of `tests.tsv` and `units.tsv`.

## Columns

`units.tsv` has one row per unit:

| Column | Meaning |
|---|---|
| `unit` | the unit, as the suite names it |
| `passed` | tests that ran and passed |
| `failed` | tests that ran to an assertion that did not hold, or whose result did not match the expected one: the server answered, not as Redis does |
| `errored` | tests stopped by an error other than an assertion: an error reply where the test expected none, a closed connection, a reply the client could not read, or an error of the test's own code |
| `skipped` | tests skipped by a tag (`external:skip`, `singledb:skip`, `large-memory`) or by name |
| `timed_out` | tests during which nothing progressed for the timeout |
| `not_reached` | tests the reference run reports and this run does not: their unit or block stopped before them; `-` without a reference |
| `reference_passed` | tests of the unit the reference run passed; `-` without a reference |
| `skipped_blocks` | blocks skipped whole by a tag; the suite never names their tests, so they are in no other column |
| `block_errors` | errors inside a block but outside its tests; the rest of the block did not run |
| `ended` | `done`; `exception`, an error the suite does not catch, in its framework's calls or outside any block, after which the rest of the unit did not run; `timeout`, the last attempt stopped at a hang; `limit`, the hour's backstop; `incomplete`, none of these |
| `server` | `running` when the server under test was still running after every attempt, else how it had stopped, per attempt |

`tests.tsv` has one row per test, in the order the suite ran them, with the
columns `unit`, `outcome`, `cause`, `detail` and `test`. The outcomes are
`passed`, `failed`, `errored`, `skipped`, `timed-out` and `not-reached`, and
for events that are not tests, whose `test` is `-`: `skipped-block`,
`block-error`, `exception` and `stalled` (no progress for the timeout outside
any test). A row that did not pass has one of these causes:

| Cause | Meaning; `detail` |
|---|---|
| `unknown-command` | the server answered that it does not know the command; the command |
| `unknown-subcommand` | the server answered that it does not know the subcommand; the command and subcommand |
| `error-reply` | another error reply where the test expected none; the reply |
| `wrong-reply` | an assertion that did not hold; its message |
| `connection` | the server closed or refused the connection; the client's message |
| `protocol-error` | a reply the client could not read; the client's message |
| `hang` | nothing progressed for the timeout |
| `other` | any other error, mostly the test's own code failing on a reply it did not expect; the message |

An assertion whose message carries an unknown-command reply, such as an
expected error that came back as "unknown command", counts as
`unknown-command`.

## Results

Measured 2026-10-03 on an Intel Core i9-14900K with 32 processors, under
WSL2 with Ubuntu 24.04.5 and Linux 6.18; tclsh 8.6.14; Redis 7.0.15 from
Ubuntu's package `5:7.0.15-1ubuntu0.24.04.4`; firn compiled from
`cea9188d4`, its executable's SHA-256 starting `16672d30`. Each result below
is one run. The reference, the baseline and the diagnostic also ran in full
under a script that differed only in how it chose ports, and the no-flush
control under one whose `--tolerant` covered five of the six calls, the
`INFO` that Redis answers anyway left out; each gave the same counts, except
that one unit of the diagnostic did not start there (see
[One unit at a time](#one-unit-at-a-time)). After the five runs below, two
changes to `run.sh` bounded the time its `PING` probes may wait and reset the
count of start attempts after a hang; neither applies to these runs, where
every server answered and no start was repeated. The control with no command
but `PING` ran with the final script.

### Redis 7.0.15: the reference

[`redis-7.0.15-units.tsv`](redis-7.0.15-units.tsv),
[`redis-7.0.15-tests.tsv`](redis-7.0.15-tests.tsv). With these flags,
external mode runs 1,995 tests, in 39 of the 84 units; the other 45 units
hold nothing it runs. It skips 96 tests and 108 blocks: every block and 57
tests for `external:skip`, 24 tests for `singledb:skip` and 15 for
`large-memory`.

Redis passes 1,994 of the 1,995. The other one, `SRANDMEMBER with a
dict containing long chain` in `unit/type/set`, errs with `key "pid" not
known in dictionary`: it starts a `BGSAVE` whose child is slowed down on
every key, then kills that child through `get_child_pid 0`, which asks the
framework for the server's process id, known only for a server the suite
started itself. The test lacks the `external:skip` tag in 7.0.15 and errs
against every external server. The first full run, whose servers shared one
data directory, also failed `UNLINK can reclaim memory in background`, as
[One unit at a time](#one-unit-at-a-time) describes.

With `DEBUG` refused, as Redis 7 does by default (`run.sh redis
--enable-debug-command no`), Redis passes 1,807 of the 1,994. 68 tests fail
on the refusal itself, `ERR DEBUG command not allowed`, 67 of them among the
1,994 and the 68th the test that needs the server's process id, which now
stops at an earlier `DEBUG`; two blocks of `unit/scripting` stop at it too,
leaving 78 tests unreached. 22 more fail after
an earlier refusal in their unit: 6 tests of `unit/protocol` around
`DEBUG PROTOCOL` fail as wrong replies, and in `unit/slowlog` (2) and
`unit/tracking` (14) a test had sent `DEBUG` inside `MULTI`, whose refusal
raised an error before the test's `EXEC`, so the transaction stayed open:
the two slowlog tests got `QUEUED` back, and in `unit/tracking` the next
test's `HELLO 3` did too, so the client never switched to RESP3. Six tests
of `unit/tracking` then waited for invalidation messages that never came; the
sixth ended the unit after its five reruns, with 14 tests unreached. Hence
the comparator enables `DEBUG`, as the suite's own servers do.

### firn at `cea9188d4`: the baseline

[`firn-cea9188d4-units.tsv`](firn-cea9188d4-units.tsv). firn passes 2 of
the 1,994 tests Redis passes, the two that check nothing (see
[Limitations](#limitations)), and reaches no other test. Every one of the 40
units with a block that external mode runs stops at the framework's first
`FLUSHALL`, which firn answers with `ERR unknown command 'flushall'`; the
other 44 units hold nothing external mode runs. 2,073 tests the reference
reports are not reached.

The commands whose absence stops whole units are thus the framework's:
`FLUSHALL` in all 40, then `FUNCTION FLUSH`, which every block sends next,
then, for the blocks with overrides, `CONFIG SET` of the parameter, which
firn answers as an unknown option, and `INFO` where the override turns
`appendonly` on. The run's `tests.tsv`, every other test not reached, is not
kept.

### Past the framework's calls: `--tolerant`, a diagnostic

[`firn-cea9188d4-tolerant-units.tsv`](firn-cea9188d4-tolerant-units.tsv),
[`firn-cea9188d4-tolerant-tests.tsv`](firn-cea9188d4-tolerant-tests.tsv).
With the framework's six calls allowed to fail, no unit stops at them, and
the blocks run on firn on a keyspace that nothing clears between the blocks
of a unit. The suite as released stops at the first call, so this is not a
measure of compatibility; it shows what stands behind that call.

| Outcome | Tests |
|---|---|
| passed | 107, of which 25 also pass with no command but `PING` (below) |
| failed | 204 |
| errored | 911 |
| timed out | 1 |
| skipped | 86 |
| not reached | 782, behind 14 block errors |

firn passes 107 of the 1,994 tests Redis passes (5.4%). The 1,116 tests that
failed, errored or timed out did so for these causes:

| Cause | Tests |
|---|---|
| `unknown-command` | 978 |
| `error-reply` | 108 |
| `wrong-reply` | 15 |
| `unknown-subcommand` | 12, all `CONFIG RESETSTAT` |
| `other` | 2, a test's own code failing after an unexpected reply |
| `hang` | 1 |

The commands firn lacks that cost the most tests, of 98 unknown commands in
all:

| Command | Tests failed or errored | Blocks stopped |
|---|---|---|
| `FUNCTION` | 153 | 0 |
| `EVAL` | 81 | 0 |
| `XADD` | 68 | 0 |
| `SORT` | 40 | 0 |
| `COMMAND` | 39 | 0 |
| `FLUSHDB` | 39 | 0 |
| `GEOADD` | 35 | 0 |
| `CLIENT` | 24 | 2 |
| `MULTI` | 23 | 0 |
| `PFADD` | 18 | 0 |
| `SMEMBERS` | 17 | 0 |
| `XGROUP` | 17 | 0 |
| `DEBUG` | 16 | 2 |
| `BITFIELD` | 16 | 0 |
| `INFO` | 15 | 2 |
| `BITCOUNT` | 15 | 0 |
| `WATCH` | 14 | 0 |
| `BITPOS` | 12 | 0 |
| `GETEX` | 12 | 0 |
| `INCRBYFLOAT` | 11 | 0 |
| `GEORADIUS` | 10 | 0 |
| `SRANDMEMBER` | 10 | 0 |
| `HELLO` | 10 | 1 |

These are the commands that cost 10 tests or more, or stop a block;
`summary.txt` of a run lists all 98.

The blocks stopped hold most of the tests not reached. A block that starts
by setting an encoding threshold, to run its tests once per encoding, stops
at that `CONFIG SET`: `unit/type/zset` loses all its 312 tests
(`zset-max-ziplist-entries`), `unit/type/list` 223 (`list-compress-depth`,
and `HELLO` in another block) and `unit/type/hash` 71
(`hash-max-ziplist-value`). `CLIENT` stops all 54 of `unit/tracking`,
`DEBUG` and `CONFIG SET repl-ping-replica-period` cost `unit/scripting` 78,
`INFO` costs `unit/type/string` 23, and `CONFIG SET
latency-monitor-threshold` all 14 of `unit/latency-monitor`. Of the 108
error replies to tests, 86 answer a `CONFIG SET` of a parameter firn does not
know, which also stops 7 blocks, and 15 an `EXPIRE` with an option, `NX`,
`XX`, `GT` or `LT`, which firn answers as a wrong number of arguments.

Not every pass shows a working command. A control, Redis with every
top-level command but `PING` renamed away, so that it answers all else with
the same `unknown command` error as firn, passes 26 of the 1,994 under
`--tolerant`, all of them tests Redis passes too. 25 of them are among
firn's 107:

- 12 tests of malformed requests in `unit/protocol`, which need a RESP parser
  and its error replies but no command;
- `PING`, and three tests that expect an error and accept the unknown-command
  error: `Non existing command`, `RENAME against non existing source key` and
  `CLIENT SETNAME does not accept spaces`;
- four tests of `integration/redis-cli`;
- the two tests that check nothing, and two whose checks the flags turn off:
  `Is the small hash encoded with a listpack?` and
  `Same dataset digest if saving/reloading as AOF?`;
- `INCRBYFLOAT: We can call scripts expanding client->argv from Lua`, whose
  inner block fails, as a block error, without failing the test.

The control's other pass, `Unbalanced number of quotes`, needs quoted inline
arguments, which firn lacks. firn's other 82 passes need its commands, if
only to be reached: `integration/redis-cli` (20), `unit/type/set` (15),
`unit/expire` (15), `unit/type/incr` (9), `unit/keyspace` (7) and eight
other units. At least six of those pass without a correct reply. The two
`Generated sets must be encoded as ...` tests check only an encoding, which
`--ignore-encoding` turns off; the control never reached them, its block
stopping at a `DEL`. `XADD with ID 0-0` and `SETRANGE with huge offset`
accept any error, and `XGROUP CREATE: automatic stream creation fails without
MKSTREAM` one that starts with `ERR`, as firn's unknown-command error does.
`CONFIG sanity` finds nothing to check in firn's empty answer to
`CONFIG GET *`.

The one hang is `Script return recursive object` in `unit/scripting`, in its
variant through `FUNCTION`: reading raw replies, the test takes
`FUNCTION LOAD`'s bulk reply as two lines and waits for the second, which
firn's one-line error never sends. The second attempt skipped it, and with it
the `EVAL` variant of the same name. No firn stopped by itself in any run.

Another control bounds what the uncleared keyspace costs: Redis with `FLUSHALL`
renamed away (`run.sh --tolerant redis --rename-command FLUSHALL ''`) passes
1,948 of the 1,994. Of the other 46, 26 tests call `FLUSHALL` themselves,
and 20 fail on keys or streams that an earlier block or test of their unit
left behind (17 wrong replies, and 3 `XADD` IDs no greater than the stream's
last); the test that needs the server's process id errs as in the reference.
The uncleared keyspace thus explains about 20 of firn's 1,116 failing tests.

### A client: redis-py

Beside the suite, the current Python client, redis-py 8.1.0, in a virtual
environment outside the repository, made five calls against each server:
`PING`, `SET`, `GET`, a pipeline without a transaction (`SET`, `INCR`, `GET`)
and redis-py's default pipeline, which wraps the same three in `MULTI` and
`EXEC`. Redis ran as `redis-server --port PORT --bind 127.0.0.1 --save ''
--appendonly no` and firn as `firn PORT 0 - 0`:

```sh
python3 -m venv <scratch-root>/venv
<scratch-root>/venv/bin/pip install redis==8.1.0
<scratch-root>/venv/bin/python - PORT PROTOCOL <<'EOF'
import sys
import redis

r = redis.Redis(host="127.0.0.1", port=int(sys.argv[1]), protocol=int(sys.argv[2]))

def check(name, run):
    try:
        outcome, value = "ok", repr(run())
    except Exception as error:
        outcome, value = "error", f"{type(error).__name__}: {error}"
    print(f"{name}\t{outcome}\t{value}")

check("PING", lambda: r.ping())
check("SET", lambda: r.set("smoke:key", "value"))
check("GET", lambda: r.get("smoke:key"))
check("pipeline, transaction=False", lambda: r.pipeline(transaction=False).set("smoke:n", 1).incr("smoke:n").get("smoke:n").execute())
check("pipeline, default (MULTI/EXEC)", lambda: r.pipeline().set("smoke:m", 1).incr("smoke:m").get("smoke:m").execute())
EOF
```

| Call | Redis, RESP3 and RESP2 | firn, RESP3 | firn, RESP2 |
|---|---|---|---|
| `PING` | `True` | unknown command `HELLO` | `True` |
| `SET` | `True` | unknown command `HELLO` | `True` |
| `GET` | `b'value'` | unknown command `HELLO` | `b'value'` |
| pipeline, `transaction=False` | `[True, 2, b'2']` | unknown command `HELLO` | unknown command `INCRBY`, the pipeline's second command |
| pipeline, default | `[True, 2, b'2']` | unknown command `HELLO` | unknown command `EXEC` |

redis-py 8.1.0 speaks RESP3 unless told otherwise (its `DEFAULT_RESP_VERSION`
is 3), so it opens every connection with `HELLO 3`, and firn fails every call
made with its defaults. redis-py's `incr` sends `INCRBY key 1`: Redis counted
`incrby` calls and no `incr` in `INFO commandstats`. In the default pipeline
firn answered `MULTI` as unknown and then ran the `SET` that followed at once:
a `GET` afterwards found `smoke:m` set to `1`, while the client saw only the
error. redis-py also sends `CLIENT SETINFO` on connecting, which neither
Redis 7.0 nor firn knows; it ignores the error.

## Limitations

- External mode measures only what an external server can be tested for: the
  suite skips 96 tests and 108 blocks whose tests it never names, among them
  replication, persistence, configuration files, access control lists and
  cluster mode.
- A test is identified by its unit and name. `not_reached` and
  `reference_passed` match names, counting duplicates, and a test skipped
  after a hang takes every test of its name with it. A test the suite defines
  only when the server answers a query in a certain way counts as not
  reached where it is not defined: `SADD, SCARD, SISMEMBER - large data`
  exists only when `CONFIG GET proto-max-bulk-len` shows a 10 GB limit, which
  Redis accepts and then skips the test for `large-memory`, and firn does
  not.
- `--ignore-encoding` and `--ignore-digest` remove the suite's checks of
  Redis's internal representations; the counts do not include them.
- A pass does not always show a working command. Two tests, in
  `integration/replication-buffer`, pass on any server in external mode:
  each consists of a block tagged `external:skip`, which is skipped, so the
  test checks nothing. Others accept any error, check only what the flags
  turn off, or find nothing to check in an empty reply. The control with no
  command but `PING` measures the passes that need no command, 26 of the
  reference's 1,994, but not those that need a command only to be reached.
- One run of each configuration on one host. A timing-sensitive test can fail
  on a loaded host; the runs held the host lock against the repository's
  other heavy commands.
- The comparator enables `DEBUG`, which a default Redis 7 refuses.
- `--tolerant` does not clear the keyspace between the blocks of a unit,
  which the control measures, and it skips nothing firn lacks inside a test.
- The causes come from the text of each failure. An assertion whose message
  quotes no recognizable error, no unknown command or subcommand, closed
  connection or unreadable reply, is a `wrong-reply`, whatever led to it.
- firn was compiled by the gate-profile `whitefootc`, the release profile
  with debug assertions and overflow checks in the compiler itself
  ([`compiler/Cargo.toml`](../../../compiler/Cargo.toml)), where firn's README
  names the release build. The release compiler of the same revision emits
  byte-identical LLVM IR for firn, but its executable differs in 21 bytes of
  `.text` besides the build ID, all in one function,
  `wf__ctx_start_server.main.0.resume`, where a few vector loads and stores
  of equal length come in another order or register, with every function's
  size and place unchanged. Two gate builds are byte-identical.
  [docs/todo.md](../../../docs/todo.md) records the difference.
