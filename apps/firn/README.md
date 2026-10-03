# firn

firn is a server of Redis's protocol written in Whitefoot. It answers RESP2
and inline requests over TCP, pipelined or not, and keeps strings, lists, sets,
hashes and sorted sets in one keyspace, a shared map that every connection
reaches through atomic statements on one key, or on the whole keyspace for a
command over several keys. It is named for firn, snow that has lasted a season: stored
and compacted.

Its commands are the ones `redis-benchmark`'s default suite sends, with
expiry and an append-only file:

- keys: `DEL`, `EXISTS`, `TYPE`, `EXPIRE`, `PEXPIRE`, `PEXPIREAT`, `TTL`,
  `PTTL`, `PERSIST`, `DBSIZE`;
- strings: `GET`, `SET` with `EX` or `PX`, `MSET`, `INCR`;
- lists: `LPUSH`, `RPUSH`, `LPOP`, `RPOP`, `LRANGE`, `LLEN`;
- sets: `SADD`, `SREM`, `SPOP`, `SCARD`;
- hashes: `HSET`, `HGET`;
- sorted sets: `ZADD`, `ZPOPMIN`, `ZCARD`, `ZSCORE`, with scores read and
  written as Redis 7.0.15 reads and writes them;
- connection and server: `PING`, `ECHO`, `CONFIG GET`.

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

The arguments are, in order:

1. the port to listen on, on the loopback address 127.0.0.1;
2. how many clients to accept before stopping, 0 for no limit;
3. the name of an append-only file in the working directory, or `-` for
   none: firn replays the file before it listens, appends every change to it
   and syncs it once a second, as Redis's `appendfsync everysec` does;
4. how many seconds a silent client is kept before it is closed, 0 for no
   limit.

`WF_DRIVERS` sets how many threads serve the connections, one per CPU by
default.

## Layout

`modules.wfg` registers the modules, each a directory:

- `bytes`: byte strings, their hash and order, and integers read as Redis
  reads them;
- `protocol`: reading requests and writing replies;
- `scores`: sorted-set scores read as Redis's `strtod` reads them, to the
  nearest double, and written as its `%.17g` writes them;
- `store`: the keyspace, a shared map of entries beside a shared object
  holding the queued expiries, the append-only file's pending bytes and the
  server's counts;
- `commands`: one file per kind of value, and the dispatch;
- `persistence`: the append-only file's writer and its replay;
- `server`: connections, active expiry and `main`.
