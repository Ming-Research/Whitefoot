# firn

firn is a server of Redis's protocol written in Whitefoot. It answers RESP2
and inline requests, quoted arguments included, over TCP, pipelined or not,
and keeps strings, lists, sets, hashes and sorted sets in one keyspace, a
shared state that every connection reaches through atomic statements holding
the entries of the keys a command names, so that commands on different keys
do not wait for each other. It is named for firn, snow that has lasted a
season: stored and compacted.

Its commands are these, which began as the ones `redis-benchmark`'s default
suite sends, with expiry, an append-only file and the connection commands
clients send on their own:

- keys: `DEL`, `UNLINK`, `EXISTS`, `TOUCH`, `TYPE`, `RENAME`, `RENAMENX`,
  `COPY` with `REPLACE` and `DB 0`, `EXPIRE`, `PEXPIRE`, `EXPIREAT` and
  `PEXPIREAT` with their options `NX`, `XX`, `GT` and `LT`, `TTL`, `PTTL`,
  `EXPIRETIME`, `PEXPIRETIME`, `PERSIST`, `DBSIZE`;
- strings: `GET`, `SET` with its options `NX`, `XX`, `GET`, `KEEPTTL`,
  `EX`, `PX`, `EXAT` and `PXAT`, `SETNX`, `SETEX`, `PSETEX`, `GETSET`,
  `GETDEL`, `GETEX`, `MGET`, `MSET`, `MSETNX`, `INCR`, `INCRBY`, `DECR`,
  `DECRBY`, `INCRBYFLOAT`, `APPEND`, `STRLEN`, `GETRANGE`, `SUBSTR`,
  `SETRANGE`;
- lists: `LPUSH`, `RPUSH`, `LPUSHX`, `RPUSHX`, `LPOP` and `RPOP` with or
  without a count, `LRANGE`, `LLEN`, `LINDEX`, `LSET`, `LREM`, `LTRIM`,
  `LINSERT`, `LPOS` with `RANK`, `COUNT` and `MAXLEN`, and `LMOVE` and
  `RPOPLPUSH`, which move an element in one statement holding both keys;
- sets: `SADD`, `SREM`, `SPOP` and `SRANDMEMBER` with or without a count,
  `SCARD`, `SMEMBERS`, `SISMEMBER`, `SMISMEMBER`, `SMOVE`, `SINTER`,
  `SUNION`, `SDIFF`, `SINTERCARD` with `LIMIT`, and `SINTERSTORE`,
  `SUNIONSTORE` and `SDIFFSTORE`, each in one statement holding every key
  it names;
- hashes: `HSET`, `HMSET`, `HSETNX`, `HGET`, `HMGET`, `HDEL`, `HEXISTS`,
  `HSTRLEN`, `HLEN`, `HGETALL`, `HKEYS`, `HVALS`, `HINCRBY`, `HINCRBYFLOAT`,
  in the x87 extended precision Redis computes it in on x86-64, and
  `HRANDFIELD` with a count and `WITHVALUES`;
- sorted sets: `ZADD` with `NX`, `XX`, `GT`, `LT`, `CH` and `INCR`,
  `ZINCRBY`, `ZRANGE` with `BYSCORE`, `BYLEX`, `REV`, `LIMIT` and
  `WITHSCORES`, `ZREVRANGE`, `ZRANGEBYSCORE`, `ZREVRANGEBYSCORE`,
  `ZRANGEBYLEX`, `ZREVRANGEBYLEX`, `ZCOUNT`, `ZLEXCOUNT`, `ZRANK`, `ZREVRANK`,
  `ZSCORE`, `ZMSCORE`, `ZCARD`, `ZREM`, `ZPOPMIN` and `ZPOPMAX` with a count,
  `ZREMRANGEBYRANK`, `ZREMRANGEBYSCORE` and `ZREMRANGEBYLEX`, with scores
  read and written as Redis 7.0.15 reads and writes them;
- connection: `PING`, `ECHO`, `QUIT`, `AUTH`, `HELLO` with no version or
  version 2, version 3 being refused as unsupported, `SELECT 0`, firn
  having one database, and `CLIENT ID`, `CLIENT GETNAME` and
  `CLIENT SETNAME`;
- server: `CONFIG GET`, `CONFIG SET`, `CONFIG RESETSTAT` and `INFO`,
  described below, `TIME`, and `COMMAND` and `COMMAND COUNT`, which
  describe no command. `COMMAND DOCS` is answered as an unknown subcommand,
  so that `redis-cli` uses its own help. `FLUSHALL` and `FLUSHDB`, with
  `ASYNC` or `SYNC`, empty firn's one database and its queued expiries in
  one atomic statement and are appended to the append-only file as Redis
  appends them; the old keys are released before the reply is sent, under
  either option. `FUNCTION FLUSH`, with `ASYNC` or `SYNC`,
  succeeds as Redis does with no function loaded, firn having none, and is
  appended to the file as Redis appends it; every other `FUNCTION`
  subcommand is answered as an unknown one. `DEBUG LOG` with a message
  answers OK, as Redis does with its debug command enabled, firn keeping no
  log to write it to; every other `DEBUG` subcommand is answered as an
  unknown one.

`CONFIG GET` takes Redis's glob patterns over firn's parameters:
`appendfilename`, `appendonly`, `bind`, `databases`, `port`, `requirepass`,
`save` and `timeout`, and the parameters whose only effect in Redis is on its
internal encodings, leaving every value as commands read it,
`hash-max-listpack-entries`, `hash-max-listpack-value`,
`list-compress-depth`, `list-max-listpack-size`, `set-max-intset-entries`,
`stream-node-max-bytes`, `stream-node-max-entries`,
`zset-max-listpack-entries` and `zset-max-listpack-value`, with their aliases
`hash-max-ziplist-entries`, `hash-max-ziplist-value`, `list-max-ziplist-size`,
`zset-max-ziplist-entries` and `zset-max-ziplist-value`. firn has none of
those encodings: it reports their parameters with Redis's defaults and keeps
what `CONFIG SET` gives them, and they change nothing else.
`hll-sparse-max-bytes` is not among them, since a HyperLogLog's encoding is
the string `GET` reads, and firn has no HyperLogLog.

`CONFIG SET` takes these parameters with Redis's checks and errors, refusing
any other as Redis refuses one it does not know, and applies all of its pairs
or none. `requirepass` changes the password for every connection that has
not authenticated, as in Redis: a connection authenticates by giving the
password, or by being accepted while none is set, and stays authenticated;
removing the password lets the others in until one is set again. `timeout`
changes the idle limit for new connections and, within a second, for
connections waiting under a limit, while a connection that waits with no
limit reads a new one only once it sends again. A client's silence is
counted from its last request or the replies to it, as Redis counts it from
its last read or write. `appendfilename` and `databases` are refused as Redis
refuses them. An `appendonly`, `port` or `bind` other than the one firn
started with, and a `save` schedule other than the empty one, are refused in
Redis's form for a refused value with firn's own reason, since firn cannot
change them while it runs and saves no snapshot; Redis would apply them.
`CONFIG RESETSTAT` answers OK and zeroes the count of connections the server
has accepted.

`INFO`, with no section, `default`, `all`, `everything` or named sections,
answers Redis's sections in Redis's order and form. Its fields carry real
values for the port, the calendar time, the uptime, the clients connected,
whether the append-only file is kept, the connections accepted and the keys
held, which it counts holding the table whole, as `DBSIZE` does; the other
fields it reports have values that are fixed and true of firn: Redis's
version 7.0.15, no git revision, `redis_git_sha1` being 00000000 as in
Redis's builds from a release, standalone mode, 64 bits, its active
expiry's 10 runs a second, no configuration file, memory limit, eviction,
script, function, replica, background save, rewrite, fork, module, publish
and subscribe, tracking or cluster. What firn does not measure, memory and
processor time, per-command and per-error counts among them, is left out,
so its CPU, Commandstats, Errorstats and Latencystats sections are empty,
and the keyspace line's `expires` and `avg_ttl`, which firn does not count,
are 0.

`HELLO` and `INFO` report the server as `redis` version 7.0.15, the version
whose replies firn follows.

`HINCRBYFLOAT` computes in the long double of Redis on x86-64 Linux, x87's
80-bit extended format, and answers as that Redis does. Redis built where
long double has another format answers it, and `INCRBYFLOAT`, differently:
in binary128 on aarch64 Linux, and in a double where long double is one.

What is not there yet is listed in [docs/todo.md](../../docs/todo.md) under
"firn"; the measurements and the design are in
[research/investigations/firn](../../research/investigations/firn/DESIGN.md).

## Build and run

From the repository root, with the compiler built as the
[README](../../README.md#try-it) describes:

```sh
compiler/target/release/whitefootc --graph apps/firn/modules.wfg --entry firn -o firn
./firn 6379 0 - 0
redis-cli -p 6379 PING
```

Up to four arguments may come first by position, in this order:

1. the port to listen on, 6379 when absent;
2. how many clients to accept before stopping, 0, the default, for no
   limit;
3. the name of an append-only file in the working directory, or `-` for
   none, the default: firn replays the file before it listens, appends every
   change to it and syncs it once a second, as Redis's
   `appendfsync everysec` does;
4. how many seconds a silent client is kept before it is closed, 0, the
   default, for no limit.

Options by name follow them, each `--` and a name, in either case, then its
value; a later value replaces an earlier one:

- `--port` and `--timeout`, as the first and fourth arguments;
- `--bind`, the address to listen on: a dotted IPv4 address, or `*` for every
  one, the loopback address 127.0.0.1 by default;
- `--appendonly yes` or `no`, whether to keep the append-only file, and
  `--appendfilename`, its name, `appendonly.aof` by default, which alone does
  not turn the file on, as in Redis;
- `--requirepass`, a password every client must give with `AUTH` or `HELLO`
  before other commands, none when empty or absent;
- `--clients`, as the second argument.

```sh
./firn --port 6380 --bind 0.0.0.0 --requirepass secret --appendonly yes
```

An unknown option, a value that does not read or an argument by position after
an option stops firn with status 1.

`WF_DRIVERS` sets how many threads serve the connections, one per CPU by
default.

## Layout

`modules.wfg` registers the modules, each a directory:

- `bytes`: byte strings, their hash and order, and integers and glob
  patterns read as Redis reads them;
- `protocol`: reading requests, writing replies, and a connection's state and
  settings;
- `scores`: sorted-set scores and the ends of score ranges read as Redis's
  `strtod` reads them, to the nearest double, and written as its `%.17g`
  writes them, and the long double `HINCRBYFLOAT` computes in, read as
  `strtold` reads it, added as x87 adds it and written as `%.17Lf` writes it;
- `extended`: numbers of x86-64's 80-bit long double, read as glibc's
  `strtold` reads them, added, and written as `%.17Lf` writes them, the
  arithmetic of `INCRBYFLOAT`;
- `store`: the keyspace, one shared state holding a keyed table of entries
  and, after it, the queued expiries, the append-only file's pending bytes
  and the server's counts
  ([firn under the shared-state design](../../research/investigations/shared-state/DESIGN.md#firn-under-the-design)),
  and beside it a second shared state, the server's, with what `CONFIG SET`
  changes, the count of accepted connections and the time the server
  started;
- `commands`: one file per kind of value, sorted sets' ranges in a second,
  the connection and server commands, and the dispatch;
- `persistence`: the append-only file's writer and its replay;
- `server`: connections, active expiry, the invocation's options and `main`.
