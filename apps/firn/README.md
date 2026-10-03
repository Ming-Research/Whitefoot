# firn

firn is a server of Redis's protocol written in Whitefoot. It answers RESP2
and inline requests, quoted arguments included, over TCP, pipelined or not,
and keeps strings, lists, sets, hashes and sorted sets in one keyspace, a
shared state that every connection reaches through atomic statements holding
the entries of the keys a command names, so that commands on different keys
do not wait for each other. It is named for firn, snow that has lasted a
season: stored and compacted.

Its commands are the ones `redis-benchmark`'s default suite sends, with
expiry, an append-only file and the connection commands clients send on their
own:

- keys: `DEL`, `EXISTS`, `TYPE`, `EXPIRE`, `PEXPIRE`, `PEXPIREAT`, `TTL`,
  `PTTL`, `PERSIST`, `DBSIZE`;
- strings: `GET`, `SET` with `EX` or `PX`, `MSET`, `INCR`;
- lists: `LPUSH`, `RPUSH`, `LPOP`, `RPOP`, `LRANGE`, `LLEN`;
- sets: `SADD`, `SREM`, `SPOP`, `SCARD`;
- hashes: `HSET`, `HGET`;
- sorted sets: `ZADD`, `ZPOPMIN`, `ZCARD`, `ZSCORE`, with scores read and
  written as Redis 7.0.15 reads and writes them;
- connection: `PING`, `ECHO`, `QUIT`, `AUTH`, `HELLO` with no version or
  version 2, version 3 being refused as unsupported, `SELECT 0`, firn
  having one database, and `CLIENT ID`, `CLIENT GETNAME` and
  `CLIENT SETNAME`;
- server: `CONFIG GET` with Redis's glob patterns over the parameters
  `appendfilename`, `appendonly`, `bind`, `databases`, `port`,
  `requirepass`, `save` and `timeout`, `TIME`, and `COMMAND` and
  `COMMAND COUNT`, which describe no command. `COMMAND DOCS` is answered as
  an unknown subcommand, so that `redis-cli` uses its own help. `FLUSHALL`
  and `FLUSHDB`, with `ASYNC` or `SYNC`, empty firn's one database and its
  queued expiries in one atomic statement and are appended to the
  append-only file as Redis appends them; the old keys are released before
  the reply under either option. `FUNCTION FLUSH`, with `ASYNC` or `SYNC`,
  succeeds as Redis does with no function loaded, firn having none, and is
  appended to the file as Redis appends it; every other `FUNCTION`
  subcommand is answered as an unknown one.

`HELLO` reports the server as `redis` version 7.0.15, the version whose
replies firn follows.

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
- `scores`: sorted-set scores read as Redis's `strtod` reads them, to the
  nearest double, and written as its `%.17g` writes them;
- `store`: the keyspace, one shared state holding a keyed table of entries
  and, after it, the queued expiries, the append-only file's pending bytes
  and the server's counts
  ([firn under the shared-state design](../../research/investigations/shared-state/DESIGN.md#firn-under-the-design));
- `commands`: one file per kind of value, the connection and server
  commands, and the dispatch;
- `persistence`: the append-only file's writer and its replay;
- `server`: connections, active expiry, the invocation's options and `main`.
