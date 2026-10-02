# A concurrent hash index under keyed atomic statements

## The question

Every client of `apps/firn` reaches its keyspace through one shared object, so
every command takes one lock ([firn](../firn/DESIGN.md#the-owners-rulings)).
A production server has more than two cores, and the
[shared-object design](../io-model/SHARED.md#remaining-questions) leaves
reader concurrency and several objects per statement open. Can the trusted
runtime provide a concurrent hash index, reached through atomic statements
on one key, that leads the established concurrent hash maps of other
languages on a measurement of its own, and is much faster than what firn
uses today, an `std::collections::hash_map::HashMap` behind one `Shared`
lock?

The writer keeps the language's meaning: a statement on one key runs with
exclusive access to that key's entry and takes effect at one point, as an
atomic statement on a whole object does [SHARE-1]. The mechanism behind it,
lock-free reads, entry locks, incremental resizing and deferred reclamation,
belongs to the runtime.

## The owner's rulings

On 2026-10-01, after the [firn suite](../firn/DESIGN.md#the-full-suite-results):

- **The core is a concurrent hash map.** Scaling past two cores comes down to
  one keyed concurrent index; the index is a hash table, since the keyspace
  needs lookup by key and no command the benchmark sends needs key order.
  Ordered structures stay inside values: a sorted set's tree belongs to one
  key's entry, which that key's statement holds exclusively. The surface,
  keyed atomic statements and a keyed prelude type, is designed in stage (b).
- **The index lives in the trusted runtime, not in Whitefoot source.** A
  lock-free index needs compare-and-swap and deferred reclamation, which
  Whitefoot source could express only through the atomic fields and
  lock-free cells the shared-object design refuses; writing it in Whitefoot
  would deny the language's premise. The goal is a strong trusted base that
  leaves no such problem to the writer. Its verification is model checking of
  the algorithm and of its C11 memory orderings, and linearizability checking
  of the implementation under stress.
- **A standalone stress measurement, fast and outside CI, with control
  groups.** `redis-benchmark` is slow and measures the index only indirectly,
  through parsing, replies and the network, which obscures attribution.
- **The comparators** are the ones listed under [Comparators](#comparators).
- **Win on the index first, then return to firn.** The new index must lead
  the comparators; being much faster than the map firn uses now follows and
  makes the return to firn easy to attribute, and is reported rather than
  made a criterion. The criteria's numbers are fixed before this
  implementation's first measurement.
- **The host for scaling** is the owner's i9-14900K, reached through a
  session running on that machine; this 4-CPU host measures one to four
  threads.

Later on 2026-10-01:

- **The index lives in the runtime,** `compiler/src/backend/concurrent_map.c`,
  with its tests in the runtime's own test stage; the bundle measures that
  source and keeps no copy.
- **Two maps unless one is fastest everywhere.** The concurrent map does not
  replace the standard `HashMap`, which serves a single thread: if the
  concurrent map were faster for a single thread too, only it would remain;
  otherwise both stay, one concurrent and one single-threaded.
- **A `Shared` object holding a `HashMap` stops being the way to share a
  keyspace,** since it cannot be faster than the concurrent map; `Shared`
  objects and atomic statements remain for every other shared state. The
  bundle's control that measured it, `wf-current`, is removed.

Still open, for stage (b) with stage (a)'s evidence: statements over several
keys, acquired in a canonical order inside the runtime (MSET is their
instance), and statements that only read running at the same time (the four
`LRANGE` tests are their instance).

## Why the Redis benchmark cannot measure this

Run against a listener that logs every command it receives and answers
`+OK`, `redis-benchmark` 7.0.15 with `-r 100000 -n 3 -c 1` sends, per test:

- no key: `PING_INLINE`, `PING_MBULK`;
- a key drawn from 100,000: `SET key:<12 digits>`, `GET`, `INCR counter:<12
  digits>`, and `MSET` with ten independently drawn keys;
- one key for the whole test: `mylist` (`LPUSH`, `RPUSH`, `LPOP`, `RPOP`, the
  four `LRANGE` tests), `myset` (`SADD`, `SPOP`), `myhash` (`HSET`),
  `myzset` (`ZADD`, `ZPOPMIN`); `-r` draws only the members and fields
  inside that one value.

So 4 of the 19 measured tests spread over keys, and 13 serialize on one key in
every server. Of the ten tests below the 1.4 criterion on two CPUs at depth
16, a concurrent index reaches `SET` 1.25, `GET` 1.28 and `MSET` 1.12; reader
concurrency reaches the four `LRANGE` tests; `ZADD` 1.19 depends on the
length of one key's critical section; and the two `PING` tests touch no key.
A hundred thousand small keys also fit in this host's caches, and almost no
test grows the table. The benchmark is a fair test of firn and a poor one of
a concurrent index.

## Stages

1. **(a) The index alone,** in C, against the comparators and the controls on
   integer keys, until it meets the criteria; then the same on byte-string
   keys shaped like firn's, where hashing and comparison cost lives.
2. **(b) The language,** keyed atomic statements and the keyed type, with the
   open questions above; measured through Whitefoot code on the same
   workloads.
3. **(c) firn on the new keyspace,** against the suite's criteria, then on the
   14900K.

## Stage (b): the language surface, stated before building

On 2026-10-01 the owner directed the work to continue through the language
and firn to a new run of the benchmark, ruling on the open questions (Q34
on the statement form, Q35 on statements over several keys, Q37 on
statements that only read) and on the choices below when the work is
handed back. Each choice is a proposal until then.

- **`SharedMap<V>`, a shared map from byte strings to values of type `V`,**
  whose keys the runtime hashes and compares. The runtime calls Whitefoot
  code today only to start a context; a key of any type would make it call
  the writer's hash and equality under a cell's lock in the middle of a
  probe, and the index's linearizability would then rest on a protocol that
  the standard `HashMap`'s own documentation says no source admission
  checks. Byte strings are what Redis keys are, and firn can name a key as a
  range of its input without building one. Rejected: a key of any type with
  a `HashMapKey` binding, for that reason; integer keys only, since firn's
  keys are bytes.
- **A keyed statement, `atomic e = &m[key] { ... }`,** whose binding is a
  `&Option<V>` naming the key's entry: the block reads it, replaces the
  value, inserts one by writing `Some` or removes the key by writing `None`,
  with exclusive access to that entry, and takes effect at one point.
  Statements on one key take effect in one order, and statements on
  different keys do not wait for each other. It counts as a waiting call,
  and its block contains no waiting call, as an atomic statement's does.
  One form serves every single-key command, with no library function and no
  callback.
- **A whole-map statement, `atomic s = &m { ... }`,** whose binding is a
  `&SharedMapState<V>` naming the whole map, with exclusive access to every entry,
  for commands over several keys (`MSET`, `DEL` and `EXISTS` of several)
  and for an exact count (`DBSIZE`). Inside its block, and only there,
  `atomic e = &s^[key] { ... }` reaches one entry and waits for nothing. To exclude it, every keyed statement publishes itself
  with one sequentially consistent store on its own thread's cache line, the
  price paid by every keyed statement and measured with firn. A statement
  over a list of keys, taken in one order the runtime fixes (Q35), would let
  `MSET` run beside statements on other keys; it is deferred, since it
  needs a type for a list of keys and a binding over several entries, and
  the whole-map statement gives the same meaning.
- **Every keyed statement holds its entry exclusively, reading or
  writing.** A Whitefoot block runs once and cannot be run again after a
  torn read of a value larger than a word, so the index's lock-free read
  serves no statement yet; statements that only read running at the same
  time (Q37, the four `LRANGE` tests) stay open.
- **A statement that finds its entry or its map held waits in the runtime
  without parking,** since a holder's block contains no waiting call and so
  runs to its end without yielding its driver; a keyed statement that has
  waited long holds the whole map instead, so that it takes effect after a
  bounded number of others (see bounded waits, below).
- **A keyed or whole-map statement's block may contain an atomic statement
  on a `Shared` object, whose own block contains none,** so that an
  append-only file's record of a change is made in the same step as the
  change. Entries and maps are always taken before objects and an object's
  holder takes nothing, so no cycle of waits can form; the inner statement
  waits without parking for the same reason as above, and borrows a hold an
  unlock handed a parked context rather than wait for that context, which
  may wait in its own driver's queue (see bounded waits, below).
- **Entries live in nodes the runtime allocates from its own pool:** the
  key's bytes and a slot for the `Option<V>`. A cell holds the key's hash in
  its key word and the node's address in its value word; a probe compares
  hashes, and the full key only after it has locked the cell, so a node is
  freed under its cell's lock and no probe reads a freed node. A move copies
  the addresses, so an entry stays where its statement's binding names it.
- **The last handle's release drops every value** and frees the map.

Stage (b)'s own measurement, the same workloads through Whitefoot code
against the C index, is deferred: firn's suite measures the surface end to
end first, at the owner's direction.

## Stage (c): firn's keyspace on the shared map

firn's keyspace was one `Shared<Store>`, so every command of every client
took one lock. It is now a `Keyspace` of three fields, each client holding
its own handles (`apps/firn/store/module.wfm`):

- **`map: SharedMap<Entry>`**, created for 2^18 keys, so that the suite's
  2^17 keys (`key:` and `counter:` names drawn from 100,000) fit without a
  move into memory this host has not yet given the process (see this host's
  fresh memory, above). Every single-key command is one keyed statement on
  its key; `MSET`, and `DEL` and `EXISTS` of several keys, hold the whole
  map and reach each key through it; `DBSIZE` counts the held state.
- **`meta: Shared<Meta>`**: the queued expiries, the append-only file's
  pending bytes, the client count, the stop flag and the seed of each
  client's `SPOP` state. A command that logs a change, or queues an expiry,
  does so in a statement on `meta` inside its keyed statement, so the record
  and the change take effect together; the suite runs firn without a file,
  so no measured command takes `meta`.
- **`logging: Bool`**, fixed in each context's handles before any client is
  served, since it changes only once, after the file's replay.

A key is a range of the client's input whose bounds a contract proves
(`key_bounds` in `apps/firn/commands/keys.wf`), so no key is copied or hashed
in Whitefoot. The expiry context takes up to 256 reached expiries from `meta`
in one statement and then removes each key whose expiry still matches in a
statement on that key; two statements are Redis's own meaning, since a key
set between them keeps its new expiry. Each client keeps its `SPOP` state,
seeded from `meta` when it connects, instead of reading and writing one
shared seed in every `SPOP`.

**A quick comparison, before the suite.** On this host, servers on CPUs 0
and 1, `redis-benchmark` on 2 and 3 with two threads, 50 clients, two
interleaved rounds of about six seconds a cell, firn before the change (its
source at `e92a54ed7`, built with this branch's compiler) against firn after
it and Redis 7.0.15, requests a second:

| Test, depth 16 | Redis | firn before | firn after |
|---|---|---|---|
| `SET` | 730,579 to 733,915 | 1,291,434 to 1,332,938 | 1,634,877 to 1,713,633 |
| `GET` | 716,903 to 781,861 | 1,564,945 to 1,636,066 | 1,799,640 to 1,893,940 |
| `LPUSH` | 1,199,201 | 1,564,945 to 1,635,769 | 1,999,556 to 2,117,149 |
| `MSET` | 176,984 to 180,357 | 391,151 to 391,441 | 342,661 to 386,316 |

**What the suite's pilot found.** The first launch of the suite was stopped
at its pilot: firn answered `SPOP` and `ZPOPMIN` at depth 1 at about 29,500 a
second, against 114,000 to 177,000 for its other tests. Both pop one key until
it is empty and then go on missing it, and every statement on an absent key
claimed a cell and left it removed, so each miss walked one more removed cell
than the last: five rounds of 40,000 `SPOP`s on an emptied key answered
53,191, 19,714, 12,296, 9,401 and 7,990 a second. A claim now reuses the first
removed cell its probe passed; the same five rounds answered 159,363 to
160,000. A round of 40,000 lasts about a quarter of a second, and
redis-benchmark's threaded mode measures a round's time in steps of about
250 ms, so those rates only bound it; rounds of 400,000 answered 159,680 to
177,699 a second with the pending claim below and 132,188 to 177,699 with
the first reuse, two interleaved runs of five rounds each on two server CPUs.

**Reuse and a key's second cell.** The first reuse looked on from its claim to
the next empty cell for a cell of the same key another writer claimed
meanwhile. The completion review's re-check found the order it missed: a
writer passes a live key's cell, that key is removed, and a second writer of
the first writer's key reuses the cell behind it before the first claims a
later removed or empty cell; both then hold the key, in two cells. A claim of
either kind is now marked pending in its key word and kept only after a
second read of the key's whole run, from its starting cell to the next empty
one: a settled cell of the key ends the claim with that cell, an earlier
pending claim of the same hash wins and the claim restarts, and a later one
is waited out. Both writers mark and then read, sequentially consistent, and
the cells before a claim never become empty again, so at least one of them
sees the other. The runtime's test drives each order step by step, both on a
removed and on an empty cell, and checks the pending rules on cells set by
hand; a churn of six keys sharing one starting cell, four threads removing
each key half the time, held one key in two statements at once in each of
seven runs of the first reuse, and its runs of the pending claim pass. Left
unmarked, claims of one key deadlock there, which the test's alarm fails.
The quick comparison above predates both changes; none of its four tests
misses a key.

**The handle each statement counted.** Every keyed statement retained and
released its map's handle, two atomic updates of one count every driver's
statements share, so that the map outlives a block that moves the handle it
was reached through. Criterion, stated before measuring: if firn built
without the pair answers depth-16 `SET` or `GET` on two server CPUs at least
5% faster, medians of two interleaved rounds beyond their spread, a statement
that cannot lose its handle omits the pair. Built so (a measurement build, not
safe in general), firn answered `SET` at 1,888,376 and 2,117,149 a second
against 1,713,633 and 1,799,280, `GET` at 1,893,940 twice against 1,713,633
and 1,799,640, and `INCR` at 1,799,280 and 1,999,556 against 1,634,877 and
1,636,066. A statement now omits the pair when it reaches the handle through
a reference parameter whose declared row writes nothing below it, since the
caller keeps that handle live for the call; every firn command reaches its
map and `meta` that way.

At depth 1 firn before and after both answered 171,298 to 179,928 a second
on `SET`, `GET` and `LPUSH`, the same steps of the benchmark's clock, so
those cells did not separate them. `MSET` holds the whole map, as the old
keyspace held its one lock, and adds a lock per key and the hold's wait for
keyed statements under way.

### The suite

The suite (`redis-bench.sh suite`) ran at `ec0bb2e99` from 14:17 to 22:35
UTC on 2026-10-01: every line passed the default suite's 20 tests with no
error, then three interleaved passes ran the 19 measured tests at depths 1
and 16, on two server CPUs (the client on two, `--threads 2`) and on one (the
client on three). firn is this revision; firn-base is firn at `e92a54ed7`,
before its keyspace moved to the map, built with the same compiler. The raw
lines are in
[keyspace-samples.csv](../../experiments/io-completion-bench/keyspace-samples.csv).
firn's rate over the fastest other server's, medians of the three passes,
and firn over firn-base:

| Test | 2 CPUs, d16 | 2 CPUs, d1 | 1 CPU, d16 | 1 CPU, d1 | over base, 2 CPUs d16 | over base, 1 CPU d16 |
|---|---|---|---|---|---|---|
| `PING_INLINE` | 1.09 | 0.98 | 1.33 | 1.00 | 0.97 | 0.95 |
| `PING_MBULK` | 1.12 | 0.96 | 1.24 | 1.02 | 1.00 | 0.95 |
| `SET` | 1.89 | 0.95 | 1.29 | 1.11 | 1.74 | 0.92 |
| `GET` | 1.67 | 0.93 | 1.29 | 1.10 | 1.51 | 0.92 |
| `INCR` | 1.74 | 0.93 | 1.11 | 1.11 | 1.37 | 0.88 |
| `LPUSH` | 1.81 | 1.00 | 1.16 | 1.11 | 1.41 | 0.94 |
| `RPUSH` | 1.67 | 1.04 | 1.13 | 1.05 | 1.33 | 1.00 |
| `LPOP` | 2.06 | 0.98 | 1.43 | 1.11 | 1.33 | 0.98 |
| `RPOP` | 1.94 | 0.94 | 1.40 | 1.14 | 1.31 | 0.98 |
| `SADD` | 1.63 | 0.98 | 1.20 | 1.12 | 1.00 | 0.96 |
| `HSET` | 1.62 | 1.04 | 1.31 | 1.11 | 1.00 | 0.95 |
| `SPOP` | 1.53 | 0.93 | 1.14 | 1.11 | 0.97 | 0.94 |
| `ZADD` | 0.92 | 1.31 | 1.36 | 1.09 | 0.96 | 0.98 |
| `ZPOPMIN` | 1.44 | 0.98 | 1.19 | 1.11 | 1.00 | 0.94 |
| `LRANGE_100` | 0.98 | 1.23 | 1.63 | 1.35 | 0.95 | 0.97 |
| `LRANGE_300` | 1.02 | 1.10 | 1.10 | 1.94 | 1.00 | 1.00 |
| `LRANGE_500` | 1.00 | 0.98 | 1.00 | 2.16 | 1.02 | 0.90 |
| `LRANGE_600` | 0.98 | 1.02 | 1.07 | 1.89 | 0.98 | 1.00 |
| `MSET` | 1.25 | 1.73 | 1.73 | 1.17 | 1.16 | 1.05 |

The fastest other server at depth 16 on two CPUs is Garnet on 11 tests,
Redis or Valkey on 5 and Dragonfly on the `PING` tests and `ZADD`; at depth 1
on two CPUs it is Dragonfly on every test but `PING_MBULK` (Valkey with I/O
threads) and the list ranges (Garnet). Against the firn investigation's criteria
([firn's criteria](../firn/DESIGN.md#criteria-stated-before-measuring)):

- **Latency is met:** firn's median p99 at depth 16 is at most the fastest
  server's on every test on both CPU counts, `ZADD` on two CPUs at 3.69 ms
  against Dragonfly's 30.67.
- **The pipelined lead is met on 11 of 19 tests on two CPUs** (at least 1.4
  times), where firn-base met it on 6: the keyspace took `SET`, `GET`,
  `INCR`, the pushes and pops and `MSET` 1.16 to 1.74 times firn-base's
  rate. On one CPU it is met on 17 of 19 (at least 1.1), firn-base on 18.
- **Without pipelining, firn is behind on two CPUs** by 2 to 7% on 11 tests,
  9 of them against Dragonfly, as firn-base is on 11; on one CPU it is behind
  on none, `PING_INLINE` equal.

Each shortfall was attributed after the suite, on the idle host, with the
servers and the client pinned as in the suite:

- **The list ranges are the client's limit.** During `LRANGE mylist 0 599`
  at depth 16 on two server CPUs, `redis-benchmark` used 1.99 of its two
  CPUs, 35.5 µs of CPU per reply against firn and 35.7 against Garnet, while
  the servers used 0.89 and 0.79 of theirs; both answered about 55,000 a
  second (`perf stat` on each process). With one server CPU the client has
  three, and firn answered 82,520, the client at 2.94 of its three. The two
  servers tie because the client parses the same replies at the same cost.
- **Without pipelining on two CPUs, the client is again the limit, and firn's
  drivers make it pay for waking them.** During `GET` at depth 1 the client
  used 1.91 of its two CPUs and firn 1.57 of its two (Dragonfly 1.77).
  firn's threads went to sleep 0.077 times per request, Dragonfly's 0.031
  (voluntary context switches over 1,500,000 requests), and a profile of the
  client puts 12.1% of its time in the send path's wakeup of the server
  (`sock_def_readable` to `__wake_up_sync_key`) against firn and 7.1%
  against Dragonfly. firn-base answers the same depth-1 rates as firn, so
  the keyspace is not the cause; the runtime's drivers park more often per
  request than Dragonfly's threads ([todo](../../../docs/todo.md)).
- **On one CPU, a keyed statement takes two dependent cache misses where the
  old keyspace took one.** Under `INCR` at depth 16 with one server CPU,
  firn answered 799,361 and firn-base 887,837 a second; `wf_cmap_lock_entry`
  took 28.1% of firn's samples, 43% of them on the load of the cell's key
  word and 41% on the load of the node's length behind it, while firn-base's
  `hash_map_edit` took 19.0%, its bucket holding the key and a short value
  inline (firn's investigation, short strings inside the keyspace). With two
  drivers the map's parallelism outweighs the extra miss; with one nothing
  does ([todo](../../../docs/todo.md)).
- **`ZADD` on two CPUs ties Dragonfly**, 402,000 to 449,000 a second over
  the passes against 420,000 to 449,000, the same steps of the benchmark's
  clock; it adds to one sorted set, so its rate is that of one key's
  critical section in every server, and firn-base answered 411,000 to
  420,000. It was not profiled here.
- **The `PING` tests touch no key**, and firn and firn-base answered them
  alike on both CPU counts.
- **One key read from two drivers costs CPU while they wait.** firn spent
  15.9 µs of server CPU per `LRANGE mylist 0 599` reply with two drivers and
  8.6 µs with one: every statement on the one list holds its entry, so the
  waiting driver spins (Q37).

### Bounded waits

The owner asked whether `atomic e = &map[key] { … atomic m = &meta { … } }`
lets a program deadlock. It does not: entries and maps are always taken
before objects, an object's block takes nothing, and no holder's block
waits, so no cycle of waits forms. Three waits, though, let a begun
statement be overtaken without bound, against [WAIT-2]'s promise that every
begun statement whose guard stays true takes effect:

- a keyed statement that found its entry locked waited with growing pauses
  and retried, and other users could lock the cell first every time;
- whole-map statements raced to set the gate, so one that held the map again
  and again could keep another out;
- an object statement inside a map's block, which cannot park, took back a
  hold an unlock had handed a parked context, which then waited again from
  its queue's head and could lose the object at every hand-off.

**A keyed statement's patience.** The fix the owner first approved marked a
waiter beside its cell and had the unlocker hand the cell to a marked
waiter, in turn. A waiter's place then names a cell of one table: a move,
which other users' claims can start between any two of the waiter's steps,
gives the cell back and sends the waiter to compete for the key's cell in
the next table, so the place does not survive the moves [WAIT-2] allows, and
the owner approved replacing it. Instead a keyed statement counts the pauses
it waits for cells, and each retry after a lost compare-and-swap, a claim
that gave way or a move as one more, over every probe and table it tries
(`wf_cmap_lock_entry`), so a move does not reset the count. The map's test
runs a statement with no patience out of it by each retry that waits for no
cell, alone: a lost claim of an empty cell, a lost claim of a removed cell,
a claim a move gave back and a lost lock of its key's cell. A claim that
gave way to an earlier one meets that claim's locked cell on its next probe
and waits for it, which counts in any case, so no test isolates that retry.
It counts pauses where an object statement counts vain wakes because it
never parks and so is never woken; the count is provisional, since a pause
lasts several times longer on some cores than others. Past 2^16 pauses, 0.77
ms on this host at 11.7 ns a pause, it gives back any claim, leaves the
statements under way, holds the whole map through the gate a whole-map
statement uses, and then locks its entry, which no statement holds by then.
Whole-map statements take turns by ticket. After the statement's hold closes
the gate it waits for at most one keyed statement of each other user, those
already under way, and its hold waits for at most one hold of each other
user ahead of it. Keyed statements that begin between two holds are bounded
by the holds' own steps, not by a count.

The map's test checks the bound with four threads on one key, the first
out of patience at its first wait and the others never, while a fifth user
holds the key until the first thread's first statement has closed its gate,
so that at least that statement holds the map on any host: in each of three
runs on four CPUs the statements that held the map (786 to 982 of 2,000)
were overtaken after closing the gate by at most three others, the bound,
and on one CPU the one forced statement held it. Mutants: never holding the
map fails the test because no statement held it; a hold that leaves the
gate open was overtaken 18,338 to 209,257 times; a hold that does not wait
for the keyed statements a hold before it kept waiting closes the gate over
one counted by hand; holds that race
for the gate instead of taking tickets let one thread hold the map 110 to
191 times in a row while the other waited, where tickets allow one; a
statement that does not count a lost claim, or a move, as a retry fails
the test of that retry; a
statement that gives up without giving back its claim, or without counting
the cell it gave back, fails the white-box test of a claim given up inside
its settling; and one that holds the map without first leaving the
statements under way waits for itself until the alarm. The churn and
counting tests run again with every waiting statement holding the map, a
quarter as many statements as their ordinary runs, which held it 27,124 to
46,225 times in the churn and 602 to 1,286 in the counting test that moves
its map, over two runs of the default and the narrowed-hash builds; with
the default patience the ordinary runs held it 0 to 15 times, waits past
0.77 ms on this four-CPU container. A hold takes the gate by
compare-and-swap once the hold before it has opened it, so the order of an
unhold's two stores does not matter.

**The patience on the suite's hot keys.** Criterion, stated before
measuring: on two server CPUs at depth 16, firn's keyed statements hold the
map at most ten times per million requests in each run, and firn with the
change answers within 5% of firn before it, medians of three interleaved
runs. A build that writes a line per statement that held the map answered
10 million `SET`s, 10 million `LPUSH`es and 2 million `LRANGE_100`s twice:
no `SET` held it, `LPUSH` held it 13 and 8 times (1.3 and 0.8 per million)
and `LRANGE_100`, every statement holding the one list, 15 and 13 times
(7.5 and 6.5 per million). Those are waits past 0.77 ms that the change now
ends. Medians of the three interleaved runs, before and after: `SET`
1,816,860 and 1,904,037, `LPUSH` 1,904,037 and 1,817,191, `LRANGE_100`
332,502 and 319,387 a second. A run of 10 million at these rates lasts
about 5.3 s, so the benchmark's 250 ms clock steps are about 5% apart, and
each of the three differences is one step; the criterion is met at the
resolution these runs have.

**A borrowed hold.** A statement in a map's block that meets an object an
unlock has handed a parked context now borrows that hold, alone, and gives
it back when it ends (`wf__shared_take`); the context keeps the object, and
when it resumes it claims the hold, which ends the borrowing, and waits
only for the borrower under way. The fix the owner first approved, a
context owed the object after a take-back, which reserves the object when
it next runs, was built, and the owner approved replacing it with the
borrowed hold: ordinary statements could still take the object before
the owed context ran, and every hand-off had to stop while any context was
owed, or a context handed the object later became owed and reserved first.

`compiler/src/backend/completion/shared_object_test.c` runs contexts whose
frames it writes on one driver beside two threads that stand for statements
in map blocks on other drivers, and steps them through each hand-off in a
fixed order, so every cycle reaches the moments it checks whatever the
host's speed, load or lock fairness. A thread holds the object until one
context has parked behind it, then unlocks and takes it again while the
other context keeps the driver, until an unlock hands the first context the
object; the other context, on the same driver, must then borrow the handed
hold, as must the second thread, which keeps its borrow until the first
context has resumed and claimed the hold, and gives it back only once the
first thread asks for the object. The test fails at once when an unlock
hands a statement the object after other than two vain wakes, when a
statement is handed the object twice or is overtaken after it resumed, or
when either borrower takes the object without borrowing the hold, and a
watchdog fails it, naming the step, when no cycle advances for 10 s. Its 50
cycles ran 50 hand-offs and 100 borrows exactly in every run: 200 runs on
four CPUs, 60 on two and 30 on one (the slowest 22, 21 and 595 ms), and 30
beside eight busy loops (the slowest 3.5 s). Mutants, eight runs each, fail
in the first cycle: the take-back with a borrower that took the object
without borrowing it, a borrow that ignores the claim with a resumed
statement overtaken twice, a hand-off after one or after three vain wakes
with that count, and a statement that waits instead of borrowing by the
watchdog in the step where the other context borrows. An earlier version,
whose threads held the object again and again untimed, reached its 50
hand-offs in 0.28 s on the hosted Linux runner but only 30 in 30 s on the
hosted macOS one, where an unlocking thread mostly took the object back
before the context on the driver could. The test links the runtime objects
the default-route probe builds, so it adds no compilation to the runtime
group.

### Many cores

The owner ran `redis-bench.sh scale` on an Intel Core i9-14900K under WSL2,
with 2, 4, 8 and 16 server CPUs, three interleaved passes at depth 16
([raw lines](../../experiments/io-completion-bench/scale-14900k-samples.csv)).
Its firn was built at `c5cdcfc9f`, before the bounded waits above. Every
line passed the default suite first. firn's median against the fastest of
Redis, Valkey with I/O threads, Dragonfly and Garnet:

| Test | n = 2 | n = 4 | n = 8 | n = 16 |
|---|---:|---:|---:|---:|
| `SET` | 1.42 | 1.46 | 1.61 | 1.13 |
| `GET` | 1.54 | 1.65 | 1.61 | 1.09 |
| `INCR` | 1.32 | 1.21 | 1.02 | 1.18 |
| `LPUSH` | 1.61 | 1.90 | 1.97 | 2.02 |
| `RPOP` | 1.77 | 1.98 | 2.26 | 2.25 |
| `SADD` | 1.58 | 1.47 | 1.53 | 1.57 |
| `HSET` | 1.55 | 1.35 | 1.60 | 1.72 |
| `ZADD` | 0.88 | 1.23 | 1.13 | 0.95 |
| `LRANGE_100` | 0.95 | 0.58 | 0.61 | 0.44 |
| `MSET` | 1.54 | 1.20 | 1.13 | 0.76 |

The same session then measured two questions on the same host and reported
them in PR #202's comments:

- **The client limits n = 8 and 16.** At n = 8, two `redis-benchmark`
  processes of 8 threads on disjoint halves of the client CPUs answered
  `SET` at 10.7 to 11.6 million a second against firn, 1.55 to 1.71 times
  the 6.8 to 6.9 million of one process of 16 threads in the same round,
  and 1.65 to 2.24 times one process's rate against Garnet. `SET` and
  `GET` barely grow from n = 4 to n = 8 for every line, so the last two
  columns bound the servers from below.
- **firn and firn before the shared map spend their CPUs alike at n = 4.**
  On `SADD`, `HSET`, `ZADD` and `LRANGE_100` firn answered 0.85 to 0.96 of
  firn at `e92a54ed7`, both keeping all four server CPUs busy, with a few
  hundred voluntary context switches in runs of 13 to 55 million requests;
  the servers' CPU time per request, read from `/proc`, was 6 to 20% higher
  for firn there and 13 to 23% lower on `LPUSH` and `RPOP`. Whether that CPU
  goes to spinning on the held entry or to work needs a profile, which the
  host's WSL2 kernel had no `perf` for.

What the table shows:

- **`LRANGE_100` does not scale.** firn answered 1,370,000, 1,548,000,
  1,408,000 and 1,241,000 a second at n = 2, 4, 8 and 16, while Garnet grew
  from 1,448,000 to 2,690,000 at n = 4, so at n = 4 the same client reads
  far more replies than firn sends. Every request reads the one list, and
  every keyed statement holds its entry exclusively, so the replies are
  written one at a time (Q37).
- **`ZADD` stays near a million a second** at every n (907,000 to
  1,127,000), the rate of one key's critical section; firn answered 0.95 to
  1.05 of firn before the shared map at n = 2 to 8.
- **`MSET` falls past n = 4**, from 1,243,000 to 1,103,000 and 802,000: a
  whole-map statement waits for a keyed statement of each other driver, so
  its cost grows with the drivers. Whether the client also limits it at
  n = 16 was not measured.
- **firn before the shared map collapses past n = 4** (`SET` 4,215,000,
  2,509,000, 605,000) where firn holds (6,321,000, 6,597,000, 6,597,000):
  its keyspace was one `Shared` object.

### Shared reads of one key

The shared-maps decision held every keyed statement's entry exclusively
until a workload whose readers of one key contend (Q37). The many-core run
is that workload: every `LRANGE` request reads the one list, and firn
answered `LRANGE_100` at 0.44 to 0.61 of Garnet with 4 to 16 server CPUs.

**The design.** A keyed statement whose guard and block write nothing
through its binder reads its entry beside the other such statements on its
key; the source marks nothing.

- *Which statements read.* The checker forms effect paths rooted at an
  atomic binder, as it does at a reference parameter: for a write through
  the binder, through a place a match binds inside the entry, and through
  a call that writes such a place. A statement on an entry none of whose
  written paths is rooted at its binder reads; the statement then removes
  the binder's paths before its effects reach the row. Seven writing
  shapes (a replacement, a payload write, a field write, a write on one
  branch and in a loop, and calls that write the payload or the entry) and
  three reading ones are pinned in the checker's tests; forming no paths at
  the binder classes all seven as readers, and dropping the call's paths
  classes the two calls as readers. A statement inside a whole-map
  statement's block holds its entry alone, as that statement holds the map.
- *The runtime.* A cell's value word keeps, above the node's 48-bit
  address, how many reads of its entry are under way. A reader adds one
  and reads the key word again, leaving if the cell was locked; a writer
  locks the key word and then waits for the count to fall to zero; all
  four steps are sequentially consistent, so either the reader sees the
  lock or the writer waits for it. A reader that finds the cell locked
  waits as a writer does, and its waits count against the same patience
  (bounded waits above); a writer's wait for readers counts against none,
  since a reader's block waits for nothing and no reader begins once the
  cell is locked. A move waits for a cell's readers before it copies it,
  and a reader that found its entry in a table a move has begun leaves
  and reads it in the next table. A read of an absent key gets a slot of
  zeros, `None`, that the map keeps and no statement writes, instead of a
  claimed cell and a node. The map test pins a reader beside its writer,
  a torn or falling value, a read across a move and a move that waits for
  a read.
- *firn.* `GET`, `LRANGE`, `LLEN`, `HGET`, `SCARD`, `ZCARD` and `ZSCORE`
  removed an expired key in the statement that read it. Each now removes
  it in a second statement on the key, which checks the expiry again, so a
  key set again in between keeps its value. The emitted program calls the
  read in those seven statements and the exclusive lock at its 21 other
  keyed sites.

**One key read from every thread**
([`make entry-reads`](../../experiments/concurrent-map-bench/Makefile)):
every thread reads one entry of 100 words, holding it exclusively or beside
the other reads, writes a reply of 0, 100 or 400 three-byte RESP bulk
strings from it inside the hold, and runs 0, 100 or 300 rounds of a
multiply-add chain between reads, on this four-CPU container. Shared reads
against exclusive holds, medians of five interleaved 0.4 s runs:

| Reply in the hold | Work between | 1 thread | 2 threads | 4 threads |
|---|---:|---:|---:|---:|
| none | 0 | 0.80 | 0.60 | 0.52 |
| none | 100 | 0.93 | 0.85 | 0.81 |
| none | 300 | 0.98 | 1.16 | 1.16 |
| 100 elements | 0 | 0.96 | 0.85 | 0.87 |
| 100 elements | 100 | 0.97 | 0.94 | 1.62 |
| 100 elements | 300 | 1.00 | 0.97 | 2.01 |
| 400 elements | 0 | 0.96 | 1.64 | 2.89 |
| 400 elements | 100 | 1.00 | 1.83 | 2.70 |
| 400 elements | 300 | 0.99 | 1.62 | 2.72 |

A 100-element reply took about 130 ns here (7.70 million exclusive reads a
second on one thread) and a 400-element one about 400 ns (2.48 million).
The gain grows with the hold: shared reads won 1.6 to 2.9 times with the
400 ns hold, and with the 130 ns hold won only at four threads with work
between reads. A shared read costs a second locked read-modify-write of the
cell's line, which an empty hold shows (0.80 on one thread) and which every
reading thread then contends for. An earlier build of the same loop with
GCC at `-O2`, whose 100-element reply took about 400 ns, gave 2.65 to 3.14
at four threads, as the 400-element row does here. How long firn's
`LRANGE_100` holds its list is not measured; its rate on the 14900K bounds
a hold and hand-off at about 650 ns (1,548,000 a second on one key).

**Criteria, stated before firn is measured with shared reads:**

1. On this container at depth 16, firn's server CPU per `LRANGE_600`
   reply on two drivers is within 1.15 times its CPU per reply on one
   (exclusive holds: 15.9 µs against 8.6 µs, stage (c)), and its rate is
   not below the exclusive build's by more than one 250 ms clock step.
2. On this container, `SET`, `GET`, `INCR` and `LPUSH` at depth 16 on one
   and two drivers stay within one clock step of the exclusive build:
   `GET` now pays the reader's second read-modify-write.
3. On the 14900K at four server CPUs, `LRANGE_100` reaches Garnet's
   2,690,000 a second, 1.74 times firn's exclusive 1,548,000, with no other
   test more than 3% below the same build's exclusive medians.

If the 14900K misses criterion 3 while criterion 1 holds, the readers'
shared count is the suspect, and marks a reader sets on its own driver's
line, which a writer scans, are the next design to measure.

**Criteria 1 and 2 on this container**
([raw lines](../../experiments/io-completion-bench/shared-reads-samples.csv)):
firn at `e2b774708` against the same source lowered with exclusive holds
only, one server process per build and pass, interleaved, three passes on
one and two server CPUs and five more on one for `SET`, `GET` and `LPUSH`.
Medians, shared against exclusive:

| Server CPUs | Test | Rate | Server CPU per request |
|---|---|---:|---:|
| 1 | `SET` (8 passes) | 0.976 | 1.02 |
| 1 | `GET` (8 passes) | 0.964 | 1.04 |
| 1 | `INCR` | 0.976 | 1.03 |
| 1 | `LPUSH` (8 passes) | 1.000 | 1.00 |
| 1 | `LRANGE_600` | 1.08 | 0.94 |
| 2 | `SET` | 1.03 | 0.96 |
| 2 | `GET` | 1.03 | 1.00 |
| 2 | `INCR` | 1.03 | 0.99 |
| 2 | `LPUSH` | 1.00 | 0.98 |
| 2 | `LRANGE_600` | 1.06 | 0.59 |

- **Criterion 1 is met.** With shared reads firn spent 9.72 µs of server
  CPU per `LRANGE_600` reply on two drivers and 8.76 µs on one, 1.11 times,
  where the exclusive build spent 16.4 and 9.28 µs, 1.77 times; its rate on
  two drivers was 55,340 a second against 52,422.
- **Criterion 2 is met on two drivers and missed by `GET` on one.** On one
  server CPU `GET` answered 761,325 a second against 789,540, 3.6% lower
  over eight passes, more than one 250 ms clock step (2.5% at these run
  lengths). In the same passes `SET`, whose statements the two builds lower
  alike, was 2.4% lower, and the first three passes put `LPUSH` 6.5% lower
  before eight passes put it at 1.000, so differences of about 2.5% here
  come from the runs rather than the change. A shared read's own cost, one
  more locked read-modify-write, was 8.6 ns in the entry reads above
  (23.7 against 29.8 million a second on one thread), 0.7% of a `GET`'s
  1.23 µs of server CPU. How much of `GET`'s 3.6% the change causes is not
  settled by these runs.

**Criterion 3 on the 14900K**
([raw lines](../../experiments/io-completion-bench/shared-reads-14900k-samples.csv)):
the same two builds at `e2b774708` as `firn` and `firn-base` of the `scale`
mode, three interleaved passes at depth 16. Medians in thousands of requests
a second:

| Server CPUs | Test | Garnet | Shared | Exclusive | Shared / Garnet | Shared / exclusive |
|---|---|---:|---:|---:|---:|---:|
| 4 | `SET` | 4,215 | 5,835 | 6,069 | 1.38 | 0.96 |
| 4 | `GET` | 4,231 | 6,093 | 6,093 | 1.44 | 1.00 |
| 4 | `INCR` | 4,352 | 5,859 | 5,859 | 1.35 | 1.00 |
| 4 | `LPUSH` | 2,626 | 4,761 | 4,913 | 1.81 | 0.97 |
| 4 | `RPOP` | 3,174 | 5,859 | 5,859 | 1.85 | 1.00 |
| 4 | `SADD` | 2,781 | 4,025 | 4,024 | 1.45 | 1.00 |
| 4 | `HSET` | 2,672 | 3,627 | 3,626 | 1.36 | 1.00 |
| 4 | `ZADD` | 523 | 1,079 | 1,079 | 2.06 | 1.00 |
| 4 | `LRANGE_100` | 2,538 | 3,109 | 1,507 | 1.22 | 2.06 |
| 4 | `MSET` | 1,057 | 576 | 564 | 0.55 | 1.02 |
| 8 | `GET` | 4,352 | 6,622 | 6,623 | 1.52 | 1.00 |
| 8 | `LRANGE_100` | 2,720 | 3,311 | 1,347 | 1.22 | 2.46 |
| 16 | `GET` | 5,640 | 6,347 | 6,348 | 1.13 | 1.00 |
| 16 | `LRANGE_100` | 2,720 | 3,109 | 1,208 | 1.14 | 2.57 |

- **`LRANGE_100` reaches Garnet** at four server CPUs, 3,109,000 a second
  against 2,538,000, and stays above it at 8 and 16; each of its three
  passes at four CPUs was above each of Garnet's.
- **Seven of the nine other tests are within 1% of the exclusive build;
  `SET` and `LPUSH` are 3.9% and 3.1% below, past the 3% bound.** Neither
  statement reads, and the two builds lower both alike. `SET`'s passes were
  5,835,000 twice and 6,069,000 once against 6,069,000 three times, two
  neighbouring steps of `redis-benchmark`'s clock, so three passes resolve
  no difference smaller than that step. The `quick` mode, whose rates do
  not move in steps, is the measurement that settles these two.
- **Neither firn nor Garnet answers more `LRANGE_100` past four CPUs.** One
  threaded `redis-benchmark` was the limit for `SET` at eight CPUs (two
  processes together answered 1.6 to 2.0 times one), so these runs do not
  say whether the readers' shared count limits `LRANGE` on many cores.
- **`MSET` is at half its rate of the many-core run** (576,000 against
  1,243,000 at `c5cdcfc9f`) in both builds, so shared reads do not cause
  it.

**Refused alternatives.**

- *A read form the writer marks* (`atomic e = &m[key] reads { ... }`):
  the outcome is the same either way [SHARE-3], so a mark would ask the
  writer to choose for a difference no program observes, and the checker
  already knows the block's writes.
- *Reads that take no lock and check a version afterwards:* a block runs
  once, and after a torn read of a value larger than a word it may follow a
  freed node or prove a false fact; it cannot be run again.
- *Copying the value out under the hold and writing the reply after it:*
  the copy of a list's elements is itself held exclusively, so readers
  still take turns, and the program, not the runtime, would carry the
  copy.
- *Per-driver reader marks a writer scans* (a reader-biased lock): every
  writer of every key would scan every driver's mark; it stays the next
  design if criterion 3 fails.

## The measurement

The bundle is `research/experiments/concurrent-map-bench/`. These rules are
fixed before any measurement.

**Operations.** `get` copies a value out; `insert` inserts or replaces;
`remove` deletes; `update` adds one to a present key's value under exclusive
access to that key, the shape of an atomic statement on one key. A
comparator whose update may run its function more than once, an optimistic
compare-and-swap loop, or that offers only an atomic addition, is flagged in
every table: a Whitefoot block runs once and may do anything to the entry, so
a lead over a flagged update compares different guarantees.

**Mixes,** over N live keys inserted before timing:

- `read`: 100% `get`;
- `mostly-read`: 95% `get`, 5% `update`;
- `balanced`: 50% `get`, 50% `update`;
- `update`: 100% `update`;
- `churn`: 50% `get`, 25% `insert`, 25% `remove`, over 2N possible keys
  with N present at the start;
- `grow`: N distinct `insert`s into an empty map, each thread a disjoint
  share, timed to the last thread's end.

**Key choice.** Uniform; Zipf with θ = 0.99, the constant of the Yahoo!
Cloud Serving Benchmark; and one key, for `update` only, the Redis shape of a
single counter. `churn` and `grow` draw uniformly. Sizes N are 2^10, 2^20
and 2^24. Uniform choice is generated
inside the timed loop by every driver from SplitMix64, integers only, so
every language draws the same sequence. Zipf ranks come from a buffer of
2^20 per thread that one generator writes to a file before timing and every
driver reads; the buffer repeats, so Zipf cells measure hot-key contention
and not the cold tail, which the uniform 2^24 cells measure.

**Keys and values** are 64-bit. A key is one plus a bijection of its index
onto 62-bit integers, the SplitMix64 finalizer's steps taken modulo 2^62, so
keys are uniformly random, no table gains from their order, and none is zero
or has a top bit set, values some comparators reserve. Native implementations
all hash a key by one multiplication with the 64-bit golden ratio, which
spreads these keys over every bit any of them reads at the cost of one
instruction, so integer cells compare structure; byte-string keys add real
hashing. Managed implementations keep their languages' own hashing.

**Drivers.** One C driver serves every C, C++ and Rust implementation,
including this one: each is a separate binary in which the driver calls the
implementation's functions directly, without inlining across the boundary,
the cost a Whitefoot program pays calling the runtime. Java, Go and .NET
implementations have drivers in their own languages with the same streams,
mixes and timing, and are reported as a separate reference group, since
their drivers differ.

**Controls.** `empty` answers every operation without storage: the floor of
the driver itself. `mutex-flat` is Boost's `unordered_flat_map` behind one
mutex: a fast table and one lock, so table speed and lock cost separate.

**Timing.** A process runs one implementation at one size and one key
choice; it prefills untimed, then runs every mix at every thread count,
`churn` last since it changes the key set, each cell warmed for 0.2 seconds
and timed for a fixed duration that a stop flag ends. Threads start at a
barrier and count their own operations. Three repetitions are interleaved
across implementations, and a cell's rate is their median, in millions of
operations per second.

**Placement.** Thread i runs on the i-th CPU of a placement list: one thread
per physical core, performance cores first, then efficiency cores, then
second hardware threads, and each row names its mix of cores.

**Checks.** Every cell checks its own result: `read` finds every key;
`update` leaves the sum of all values equal to the prefill's sum plus the
updates counted, so a lost update fails the cell; `churn` leaves a count of
live keys equal to the prefill plus successful inserts less successful
removes; `grow` finds every key. A cell that fails is reported and not
timed. This implementation adds a linearizability mode: recorded histories
of a few threads over few keys, checked key by key against the sequential
map, which suffices because linearizability is local to each object.

**Profiles.** `quick`, for iteration, runs N = 2^20 at uniform and Zipf
choice, `mostly-read`, `balanced` and single-key `update`, at one thread and
at every CPU, for this implementation, the controls and the fastest
comparators, in a few minutes. `full` runs the whole matrix on request. Both
write only to the scratch root; neither is part of `make check`.

## Criteria, stated before measuring

The owner ruled the form when the work began and the numbers on 2026-10-01,
before this implementation's first measurement. A cell is one size (2^10,
2^20, 2^24), one key choice and mix (uniform `read`, `mostly-read`,
`balanced`, `update`, `churn` and `grow`; Zipf `read`, `mostly-read`,
`balanced` and `update`; one key `update`) and one thread count (one, two
and four on this host): 99 cells. The 14900K judges them again with its own
thread counts.

- **It leads.** In every cell its median reaches at least the median of the
  fastest comparator, native or managed, flagged or not, with the flags
  reported beside it. A margin smaller than the cell's spread, the larger
  of the two implementations' differences between their fastest and slowest
  repetitions as a fraction of their medians, counts as a tie and is listed
  apart, not as a lead.
- **No criterion against firn's map.** The owner ruled that leading the
  comparators suffices.
- **Its single thread is not paid for concurrency.** At one thread, in every
  mix, it reaches at least `mutex-flat`'s rate.
- **The read path judged is the copy-out read.** Every comparator's `get`
  copies a value out, and so does this index's lock-free read. A variant
  whose reads hold the bucket's lock, the read a Whitefoot block needs
  unless it may run again without effect, is measured and reported beside
  it, not judged: it is evidence for stage (b)'s question on statements
  that only read.
- **It passes every check**, including the linearizability mode.

## Comparators

Native, through the C driver, at pinned versions:

- Rust: `papaya` (lock-free, read-optimized), `DashMap` (sharded
  reader-writer locks), `scc::HashMap`;
- C++: Boost 1.92 `concurrent_flat_map`, Intel TBB `concurrent_hash_map`,
  `libcuckoo`, `growt` and the parallel-hashmap library's
  `parallel_flat_hash_map` with a mutex per submap;
- C: liburcu's `cds_lfht`, a lock-free resizable table under
  read-copy-update;
- floors at one thread: Rust's standard `HashMap` and Boost's
  `unordered_flat_map`, with no lock.

Managed, through their own drivers: Java 21 `ConcurrentHashMap`, .NET 8
`ConcurrentDictionary`, Go 1.24 `sync.Map` and `xsync.MapOf`.

Garnet's Tsavorite index is not separated from Garnet; firn meets it again in
stage (c). folly's `ConcurrentHashMap` is left out: its build needs most of
folly.

On 2026-10-01, after a first quick profile in which Boost's map led every
multi-threaded mix but one, the owner added `growt`, which the first list
named only if obtainable, and `parallel_flat_hash_map`, so that a lead
counts against the strongest designs for integer keys; the baseline was
restarted with them so that every comparator is measured in the same
interleaved run.

## The baseline, and how the index is compared

The full profile ran for both of the comparator sets: the first, without
growt and parallel_flat_hash_map, was stopped to add them, and the second
was stopped on 2026-10-01 after its 2^10 and 2^20 sizes, three repetitions
each, when the owner ruled that the full matrix runs only when unavoidable:
it takes hours on this host. Its rows are kept with the bundle's results.

Among the comparators, growt is the fastest in nearly every cell of both
sizes, often by a wide margin over the next: at 2^20 keys, uniform
`mostly-read` on four threads, 94 million operations a second against
Boost's 46 in the quick profile. The exceptions are `update` on one key at
four threads (DashMap), `churn` (DashMap at one thread, scc at two and
four) and `grow` at one thread at 2^20 (DashMap).

So the index is compared, per the owner's ruling, in the `duel` profile:
N = 2^20, uniform `read`, `mostly-read`, `balanced`, `update`, `churn` and
`grow`, Zipf `mostly-read` and `balanced`, and one-key `update`, at one
thread and at every CPU, three interleaved repetitions, against growt,
DashMap and scc, the fastest comparators of the baseline, with `mutex-flat`
for the single-thread criterion. The criteria above apply to the cells it
runs.

## The index

### The first design, and why it was replaced

The first candidate followed the cache-line hash table of David, Guerraoui
and Trigonakis ("Asynchronized concurrency", ASPLOS 2015): 64-byte buckets
of three keys and values with a ticket lock that doubled as the read
version, overflow buckets, and a cooperative move bucket by bucket. In the
first duel it led one cell, tied eight and lost nine, most of them to growt
at between half and four fifths of its rate. Two costs were found: a branch
on which slot held the key mispredicted on most lookups and discarded the
loads of the lookups after it (a branch-free slot match took single-thread
reads at 2^10 keys from about 47 to about 101 million a second), and the
buckets held 2^20 keys in 41.9 bytes a key against growt's 32.4, so a
random probe missed the cache more often and a lookup that reached an
overflow bucket missed twice.

### The cell design

`compiler/src/backend/concurrent_map.c` now follows growt's layout (Maier,
Sanders and Dementiev, "Concurrent hash tables: fast and general?!", 2016),
with a lock in place of its compare-and-swap updates, since a Whitefoot
statement's block runs once with its entry held:

- **A cell is 16 bytes, a key word and a value,** four to a cache line, in
  one array probed linearly from the key's golden-ratio hash. A lookup at
  half load reads one line nearly always, and the cells cost 32 bytes a key
  at that load, as growt's do.
- **The key word carries the lock.** Its top bit locks the cell, and the
  next is kept for marking parked waiters; empty is zero and removed is all
  ones below them, so keys lie in [1, 2^62 - 2]. A writer locks its key's cell, or claims an empty
  one, by one compare-and-swap, runs once and stores the key back. A reader
  takes no lock: it waits while the cell is locked, since a claimed cell has
  no value yet, and then reads the value. A cell never holds another key and
  its value is written by one store, so the value read is one the key held
  during the read; entries larger than a word would need a version in their
  header, and stage (b)'s entries are read only under the cell's lock, since
  every keyed statement holds its entry (Q37). The runtime's test
  fails when a read does not wait, and cannot fail when a read checks the
  key word again after the value, so that check was removed.
- **A removed key stays a removed cell until the table moves,** so a probe
  never stops early. A probe that has seen every cell stops there: a full
  table, which racing claims can leave in a small one, answers absent to a
  read and sends a claim to help a move.
- **A table moves once half its cells are used,** to the fewest cells, a
  power of two, that hold the live keys in three eighths of them, at least
  half its size: growth doubles, and a churning map moves between tables
  of one size with at least an eighth of the cells left to claim. One
  writer makes the next table, and writers that meet the move copy blocks
  of 4,096 cells into it and retry there. A writer checks for a move after
  it locks its cell and gives the cell back unchanged when one has begun, a
  claimed cell as removed, since another writer may have probed past it; a
  mover publishes the move before it reads a cell and waits while the cell
  is locked. All four steps are sequentially consistent, so either the
  mover sees the lock or the writer sees the move, and the mover only reads
  the old table: marking each cell moved by compare-and-swap, as the first
  cell design did, wrote every line of it, and dropping the mark took
  single-thread `grow` from about 9.5 to about 12.6 million a second. A map
  created for N keys starts half full at N, as dense as a table gets before
  it moves, since reads cost less in a smaller table: four times N cells
  took single-thread reads at 2^20 keys from about 26 to about 20 million a
  second.
- **Each user publishes the table it works in,** as growt's handles do: an
  operation compares the map's current table with its user's, and only on a
  change publishes the new one and checks again, both sequentially
  consistent. A moved table is freed once no user is in it, and the cells of
  the newest one freed are kept for the next move to their size, so a map
  whose size holds steady moves between warm tables: on this host, at four
  threads, a move of 2^22 cells took 12 ms into reused cells and 111 ms
  into fresh ones. A
  user belongs to one thread and one map, so a thread can use several maps;
  a thread-local slot could not tell them apart.
- **A writer that finds its key locked waits 16 pauses, then twice as long
  each time up to 1,024,** about 0.2 to 12 microseconds here, as long as a
  sleep and wake by the system. The holder then makes many changes in a row
  instead of handing the cell's line to a waiter on each: one-key `update` at
  four threads went from about 6.5 to about 24 million a second, where
  waiting that started at one pause or yielded the processor stayed between
  6 and 16. A writer of a word's key can be overtaken without bound; a
  keyed statement on an entry holds the whole map once it has waited long
  (see bounded waits, in stage (c)).
- **Cell arrays of 2 MiB or more are mapped and advised into huge pages,**
  since a random probe in a large table otherwise pays a page walk on most
  accesses: uniform `read` at four threads ran at about 127 million a second
  with huge pages and about 105 without.

### This host's fresh memory

This host returns every free 2 MiB block to its hypervisor
(`/sys/module/page_reporting/parameters/page_reporting_order` is 9), so a
huge page faulted in fresh arrives cold: fresh memory faulted in at about
140 MB/s in huge pages and 2 GB/s in 4 KiB pages, and memory the process
had just freed at about 6.6 GB/s. A move
into fresh cells pays this, so a map that grows while measured pays it once
per size; the comparators allocate 4 KiB pages and do not. The first
measurement of `churn` at one thread completed no operation in its window:
its writer spent 0.8 s clearing a fresh 128 MiB table, and at four threads
every writer that crossed the threshold allocated and cleared its own, 1.4
to 2.3 s each. One writer now makes the table, and reuse
leaves the cold cost to the first moves of each size. The 14900K, a host
without page reporting, measures the same cells without it.

### Measured

The fourth duel, on 2026-10-01, gave 3 leads, 15 ties and no loss
([results](../../experiments/concurrent-map-bench/RESULTS.md#the-index)):
uniform `balanced` and `mostly-read` at four threads and Zipf `balanced` at
one. Every cell's spread on this host is wide enough that a median a tenth
to a fifth either side of growt's counts as a tie. At one thread it reaches
`mutex-flat` in every mix but `churn`, whose first moves pay this host's
cold fresh memory. Uniform `read` at one thread is at the hardware's floor
for one miss a read: a flat table with no concurrency control ran at 25 to
27 million a second, growt at 19 to 26 and the index at 25 to 29, so that
cell can tie but not lead.

What would refute it: a lead lost at one thread to the single-thread floors
would show the lock bit costing more than it saves; a lead lost on `update` with one key at four threads once waits
are bounded would show the batching depended on unbounded overtaking; a lead lost
in `grow` on the 14900K would show the cooperative move slower than the
comparators' rebuilds.
