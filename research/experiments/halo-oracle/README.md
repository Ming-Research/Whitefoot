# Halo correctness oracle

This corpus records the exact RESP2 replies of Redis 7.0.15 for small Lua 5.1
`EVAL` scripts. It supplies independent expected behavior for the Lua engine
and its firn bindings described in [Halo's design](../../investigations/halo/DESIGN.md).
It is explicitly invoked research tooling, outside the compiler gate.

There are 80 scripts: 48 `lua-core`, 16 `redis-api`, 6 `apps`, and 10 `libs`.
The scripts, replies and runner belong here until the oracle is replaced or
Halo/firn scripting is retired. Each case is consumed by `run.sh`; the table
below is the index of the observations it protects.

## Layout and invocation

- `scripts/<group>/<name>.lua`: one observation or small related set per file.
- `expected/<group>/<name>.txt`: the typed reply produced by Redis 7.0.15.
- `run.sh`: POSIX shell entry point with an embedded Python 3 standard-library
  RESP2 client. Python handles binary replies independently of Whitefoot;
  there is no redis-cli display-mode or locale ambiguity.

Every script starts with `-- checks:`, JSON string arrays `-- KEYS:` and
`-- ARGV:`, and an `-- expects:` description. Zero or more `-- setup:` JSON
command arrays give the exact pre-existing-key setup, executed in order.
Keys used by setup commands are declared in `KEYS`. `-- error: true` marks
an intentional top-level RESP error; errors caught by Lua remain normal
successful EVAL replies. A top-level error without that marker, or a missing
error with it, fails the runner. `-- needs: COMMAND` identifies a command
absent from [firn's command list](../../../apps/firn/README.md).

From the repository root, regenerate and then compare without rewriting:

```sh
research/experiments/halo-oracle/run.sh --generate
research/experiments/halo-oracle/run.sh --check
```

The default server is `/private/tmp/wf-redis-7.0.15/src/redis-server`.
Python 3 and that executable are the only dependencies. `--server PATH`
can select another Redis 7.0.15 executable; its reported version is checked.
The runner starts it on a free loopback port with `--save '' --appendonly no`,
keeps all server files in a temporary directory under `/private/tmp` outside
the repository, flushes the database before and after each case, and stops
and waits for the server on completion, failure or interruption. Socket
operations have a five-second timeout. Redis replies are collected before
any expected file is written, so an unexpected script error cannot partly
regenerate the baseline. Missing, empty and surplus expected files fail a
full comparison.

A small sample and two independent generations can be run as follows; the
second generation stays in scratch storage:

```sh
research/experiments/halo-oracle/run.sh --check --filter apps/rate-limiter
second=$(mktemp -d /private/tmp/halo-oracle-repeat.XXXXXX)
research/experiments/halo-oracle/run.sh --generate --output "$second"
diff -r research/experiments/halo-oracle/expected "$second"
rm -r "$second"
```

`--filter GROUP` selects a group; `--filter GROUP/NAME` selects a case.
Changing any script, including its header, requires regenerating its reply:
Redis's uncaught script errors include the SHA1 of the complete script and
its source line number. Neither hashes nor error locations are normalized.

## Comparing a later Halo/firn build

Start the candidate server separately, then use the same scripts, metadata
and reply decoder against its port:

```sh
research/experiments/halo-oracle/run.sh --check --port 6380
```

`--host` selects a host other than the default `127.0.0.1`. External-server
mode requires `--check` and cannot regenerate expected replies. It does not
start or stop the candidate. Use a dedicated test instance: before and after
each case it deletes every declared corpus key with `DEL`. All writes in
this corpus target declared `halo-oracle:*` keys, so this isolates cases
without requiring `FLUSHDB`, which firn currently lacks. Other clients must
not alter these keys or scripting state during the run. A nonzero exit means
a setup/protocol failure, an unexpected top-level error state, or at least
one byte-for-byte reply mismatch; mismatches identify the case and expected
file. The current firn lacks EVAL, so this comparison becomes executable
when its Halo binding exists. The sliding-window case additionally needs
`ZREMRANGEBYSCORE`; it is retained and marked rather than silently skipped.

## Reply format

Each `.txt` is one ASCII JSON document, indented by two spaces, ending in one
newline. Types remain distinct even if their payloads look identical:

```json
{
  "type": "array",
  "items": [
    {"type": "integer", "value": 1},
    {"type": "bulk", "bytes": "OK"},
    {"type": "status", "bytes": "OK"},
    {"type": "error", "bytes": "ERR example"},
    {"type": "nil", "kind": "bulk"}
  ]
}
```

The compact entries above illustrate the schema; the runner indents all
objects. Arrays have ordered `items`; a null array uses `type: nil` and
`kind: array`. Bulk strings, statuses and errors use `bytes`. Each decoded
JSON code point U+0000 through U+00FF represents **one wire byte**, not UTF-8
text: `\u0000` is NUL, `\u00ff` is byte 255, and UTF-8 bytes are represented
individually. JSON escapes preserve quotes, backslashes and control bytes.
RESP length prefixes and CRLF delimiters are parsed and checked, rather than
stored; payload bytes, integer values, array structure and nil kind are exact.
The dump neither flattens nested replies nor converts bulk/status/error into
one string type. It is RESP2 throughout; there is no HELLO 3 negotiation.

## Determinism and platform limits

The pinned reference was generated on Darwin arm64 with Redis 7.0.15,
64-bit, libc allocator (build `88d1d7f6dca87e23`). The replies are exact
observations of that build, not portable-language promises for every case.

- `integer-doubles`, `float-print`, `format-14g`, `negative-division`,
  `numeric-coercion`, `concat-numbers`, `nonfinite`, `math-powers`,
  `math-round-extrema`, `math-random`, `string-format`, `cjson-numbers`, and
  `struct-strings-floats` expose floating-point arithmetic, libm, C/Lua or
  cjson formatting, or binary double representation. Lua's `tostring` uses
  `%.14g`; NaN spelling/sign and rounding details can differ on other
  platforms. Explicit endian choices in struct cases remove native byte
  order variation. These outputs still define the chosen Redis target.
- `array-holes` pins the observed array boundary as well as checking Lua
  5.1's boundary property. The language does not uniquely define `#` for
  every table with holes; the chosen result can differ with implementation
  and allocation layout.
- `math-random` is **deterministic** here. Redis 7.0.15 replaces Lua's libc
  random functions with `redisLrand48`/`redisSrand48`, a 48-bit generator.
  Its initial state corresponds to seed `0x1234abcd` (305441741); Redis 7
  does not reset it on every EVAL. The script explicitly restores that
  seed, samples all three random forms, then seeds 12345 twice to check
  reproducibility. The numeric generator is intended to agree across
  architectures; its returned fractional values also exercise `%.14g`.
  This behavior is evidenced by Redis 7.0.15 `src/rand.c` and
  `src/script_lua.c` in the reference source distribution.
- `next-pairs` sorts its observations, avoiding unspecified hash iteration
  order. JSON encodings use arrays or a single object member, avoiding
  unspecified multi-member object order. No script prints table/function
  addresses, samples wall time, or returns an expiry countdown. Application
  scripts use a fixed window timestamp and long expiry values (60 seconds).

Five API cases intentionally return top-level errors: `call-error`,
`return-error-table`, `error-reply`, `global-read` and `global-write`.
Redis 7.0.15 reports the global-write refusal as "Attempt to modify a readonly
table". No script was dropped; there were no unexpected top-level rejections.
Two complete generations must agree before treating a changed corpus as a
reproducible reference. Cross-platform differences should be investigated,
not automatically used to overwrite this baseline.

## Script index

| Group | Script | Purpose |
| --- | --- | --- |
| apps | [hash-cas](scripts/apps/hash-cas.lua) | Hash compare-and-set leaves mismatches untouched and reports a match. |
| apps | [lock-extend](scripts/apps/lock-extend.lua) | PEXPIRE extends a lock only for its current owner. |
| apps | [queue-move](scripts/apps/queue-move.lua) | LPOP/RPUSH moves jobs in order and stops on an empty source. |
| apps | [rate-limiter](scripts/apps/rate-limiter.lua) | INCR and first-hit PEXPIRE enforce a limit without clock-valued output. |
| apps | [redlock-release](scripts/apps/redlock-release.lua) | Lock release deletes only a matching owner and handles a missing lock. |
| apps | [sliding-window](scripts/apps/sliding-window.lua) | Sorted-set limiter removes old scores and counts a fixed-time window. |
| libs | [bit-logical](scripts/libs/bit-logical.lua) | bit.band, bor and bxor use signed 32-bit results. |
| libs | [bit-shifts-hex](scripts/libs/bit-shifts-hex.lua) | Shifts mask counts and tohex formats signed values as hex. |
| libs | [cjson-arrays-nested](scripts/libs/cjson-arrays-nested.lua) | JSON arrays encode and nested object values decode without relying on object order. |
| libs | [cjson-invalid](scripts/libs/cjson-invalid.lua) | Malformed JSON raises an error caught by pcall. |
| libs | [cjson-numbers](scripts/libs/cjson-numbers.lua) | cjson number formatting is independent of RESP integer conversion. |
| libs | [cjson-objects](scripts/libs/cjson-objects.lua) | JSON objects, empty table encoding and decoded null/boolean values. |
| libs | [cmsgpack-binary](scripts/libs/cmsgpack-binary.lua) | MessagePack exact scalar bytes and binary strings survive packing. |
| libs | [cmsgpack-roundtrip](scripts/libs/cmsgpack-roundtrip.lua) | MessagePack round-trips maps, arrays, booleans, integers and strings. |
| libs | [struct-integers](scripts/libs/struct-integers.lua) | struct packs explicit-endian signed and unsigned integer widths. |
| libs | [struct-strings-floats](scripts/libs/struct-strings-floats.lua) | struct packs fixed strings and doubles with explicit endian and offsets. |
| lua-core | [array-holes](scripts/lua-core/array-holes.lua) | Lua 5.1 length selects a boundary for arrays with holes. |
| lua-core | [assert](scripts/lua-core/assert.lua) | assert returns all successful arguments and raises a chosen message. |
| lua-core | [concat-numbers](scripts/lua-core/concat-numbers.lua) | Concatenation coerces numbers with Lua number formatting. |
| lua-core | [counter-closure](scripts/lua-core/counter-closure.lua) | Separate counter closures retain independent upvalue state. |
| lua-core | [embedded-zero](scripts/lua-core/embedded-zero.lua) | Embedded NUL survives length, slicing, byte lookup and RESP bulk. |
| lua-core | [error-levels](scripts/lua-core/error-levels.lua) | error levels select caller locations; level zero has no location. |
| lua-core | [error-values](scripts/lua-core/error-values.lua) | pcall preserves string and table error objects without stringifying tables. |
| lua-core | [float-print](scripts/lua-core/float-print.lua) | tostring uses Lua number formatting and exponent notation. |
| lua-core | [format-14g](scripts/lua-core/format-14g.lua) | Explicit %.14g rounding at fourteen significant digits. |
| lua-core | [integer-doubles](scripts/lua-core/integer-doubles.lua) | Integer-valued doubles, precision boundary and number type. |
| lua-core | [ipairs](scripts/lua-core/ipairs.lua) | ipairs stops at the first nil even when later array entries exist. |
| lua-core | [loop-closures](scripts/lua-core/loop-closures.lua) | Numeric and generic loops create captured iteration locals. |
| lua-core | [math-powers](scripts/lua-core/math-powers.lua) | sqrt, math.pow, exponentiation and math.huge. |
| lua-core | [math-random](scripts/lua-core/math-random.lua) | Redis deterministic default seed and explicit reseeding. |
| lua-core | [math-round-extrema](scripts/lua-core/math-round-extrema.lua) | floor, ceil, abs, max, min and fmod for signed numbers. |
| lua-core | [meta-call](scripts/lua-core/meta-call.lua) | __call receives the table as its first argument and can return many values. |
| lua-core | [meta-comparisons](scripts/lua-core/meta-comparisons.lua) | Shared __eq, __lt and __le control table comparisons. |
| lua-core | [meta-concat-tostring](scripts/lua-core/meta-concat-tostring.lua) | __concat handles table operands; tostring honors __tostring. |
| lua-core | [meta-index](scripts/lua-core/meta-index.lua) | __index supports both table delegation and a function fallback. |
| lua-core | [meta-le-fallback](scripts/lua-core/meta-le-fallback.lua) | Without __le, less-or-equal falls back to reversed __lt. |
| lua-core | [meta-newindex](scripts/lua-core/meta-newindex.lua) | __newindex intercepts absent entries while existing ones write directly. |
| lua-core | [meta-protection-raw](scripts/lua-core/meta-protection-raw.lua) | Metatable protection and raw operations bypass ordinary metamethods. |
| lua-core | [meta-table-length](scripts/lua-core/meta-table-length.lua) | Lua 5.1 ignores __len on tables. |
| lua-core | [metamethod-error](scripts/lua-core/metamethod-error.lua) | Errors raised inside a metamethod propagate through pcall. |
| lua-core | [multiple-returns](scripts/lua-core/multiple-returns.lua) | Assignment and table constructor expand only final multiple returns. |
| lua-core | [negative-division](scripts/lua-core/negative-division.lua) | Lua 5.1 division is floating; modulo follows floor division. |
| lua-core | [next-pairs](scripts/lua-core/next-pairs.lua) | next and pairs visit hash entries; output is sorted to avoid hash order. |
| lua-core | [nil-returns](scripts/lua-core/nil-returns.lua) | select observes nil return slots that RESP arrays would truncate. |
| lua-core | [nonfinite](scripts/lua-core/nonfinite.lua) | Infinity and NaN arithmetic, comparisons and string conversion. |
| lua-core | [numeric-coercion](scripts/lua-core/numeric-coercion.lua) | Arithmetic coerces numeric strings; tonumber handles bases and failures. |
| lua-core | [pcall-xpcall](scripts/lua-core/pcall-xpcall.lua) | Protected calls preserve success returns and handler-transformed errors. |
| lua-core | [shared-upvalues](scripts/lua-core/shared-upvalues.lua) | Sibling closures share the same mutable captured local. |
| lua-core | [string-byte-char](scripts/lua-core/string-byte-char.lua) | byte ranges and char preserve boundary bytes including NUL. |
| lua-core | [string-escapes](scripts/lua-core/string-escapes.lua) | Quoted escapes, decimal byte escapes and long bracket strings. |
| lua-core | [string-find](scripts/lua-core/string-find.lua) | find plain mode, pattern captures, start offsets and no match. |
| lua-core | [string-format](scripts/lua-core/string-format.lua) | format exercises %d %s %q %x %g and width/precision. |
| lua-core | [string-gmatch](scripts/lua-core/string-gmatch.lua) | gmatch iterates captured words and digits. |
| lua-core | [string-gsub-dynamic](scripts/lua-core/string-gsub-dynamic.lua) | gsub table and function replacements preserve false or nil matches. |
| lua-core | [string-gsub-string](scripts/lua-core/string-gsub-string.lua) | gsub supports capture substitutions, whole match, percent and limits. |
| lua-core | [string-length-order](scripts/lua-core/string-length-order.lua) | String byte length and lexicographic comparisons. |
| lua-core | [string-match](scripts/lua-core/string-match.lua) | Patterns support captures, balanced matching and frontier boundaries. |
| lua-core | [string-transforms](scripts/lua-core/string-transforms.lua) | sub negative indices, upper/lower, rep and reverse. |
| lua-core | [table-concat](scripts/lua-core/table-concat.lua) | table.concat uses separators, slice bounds and numeric elements. |
| lua-core | [table-insert-remove](scripts/lua-core/table-insert-remove.lua) | Array insertion shifts entries; removal returns and shifts values. |
| lua-core | [table-parts](scripts/lua-core/table-parts.lua) | Array and hash entries coexist; hash entries do not add array length. |
| lua-core | [table-sort](scripts/lua-core/table-sort.lua) | Default and custom comparator sort numbers into opposite orders. |
| lua-core | [tail-recursion](scripts/lua-core/tail-recursion.lua) | Proper tail calls allow deep recursion without growing call stack. |
| lua-core | [unpack-select-varargs](scripts/lua-core/unpack-select-varargs.lua) | unpack ranges and select preserve vararg count including nil. |
| redis-api | [call-error](scripts/redis-api/call-error.lua) | redis.call raises command errors to the EVAL reply. |
| redis-api | [call-success](scripts/redis-api/call-success.lua) | redis.call returns successful write and read replies. |
| redis-api | [error-reply](scripts/redis-api/error-reply.lua) | redis.error_reply constructs a RESP error reply. |
| redis-api | [global-read](scripts/redis-api/global-read.lua) | Redis refuses access to an undeclared global. |
| redis-api | [global-write](scripts/redis-api/global-write.lua) | Redis refuses assignment to an undeclared global. |
| redis-api | [log-keys-argv](scripts/redis-api/log-keys-argv.lua) | redis.log is allowed; KEYS and ARGV remain ordered strings. |
| redis-api | [lua-arrays](scripts/redis-api/lua-arrays.lua) | Nested array conversion stops at first nil and ignores hash entries. |
| redis-api | [lua-nil](scripts/redis-api/lua-nil.lua) | A top-level Lua nil becomes a RESP nil bulk. |
| redis-api | [lua-scalars](scripts/redis-api/lua-scalars.lua) | Lua numbers truncate toward zero; true is one and false is nil. |
| redis-api | [pcall-success-error](scripts/redis-api/pcall-success-error.lua) | redis.pcall returns successes and error tables without raising. |
| redis-api | [resp-error-raised](scripts/redis-api/resp-error-raised.lua) | A Redis error caught by Lua pcall is a string rather than a reply table. |
| redis-api | [resp-values](scripts/redis-api/resp-values.lua) | Integer, bulk, missing bulk, array and status replies become Lua values. |
| redis-api | [return-error-table](scripts/redis-api/return-error-table.lua) | A returned err field becomes a RESP error. |
| redis-api | [return-status-table](scripts/redis-api/return-status-table.lua) | A returned ok field becomes a RESP status. |
| redis-api | [sha1hex](scripts/redis-api/sha1hex.lua) | redis.sha1hex hashes empty, ASCII and embedded NUL byte strings. |
| redis-api | [status-reply](scripts/redis-api/status-reply.lua) | redis.status_reply constructs a RESP status reply. |
