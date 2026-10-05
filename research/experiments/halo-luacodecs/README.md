# Redis Lua codec compatibility experiment

Compiler builds and checks have a persistent cache at
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-luacodecs`.
`--cache DIR` overrides it; `--no-cache` disables caching. Cache paths must be
outside the repository and survive scratch-executable cleanup.
Native builds use the cache by default. `--full-lto` selects a whole-build
LTO link without caching; this runner reports build time, not program speed.

`python3 -B research/experiments/halo-luacodecs/run.py --compiler /path/to/whitefootc --incremental --redis-source /path/to/redis-7.0.15`
builds a scratch standalone reference from Redis’s bundled Lua and all four
libraries, compiles the existing Halo end-to-end driver, and compares typed
RESP2 replies for a generated corpus, including binary strings and errors.
Use `--binary` to reuse the Halo executable and `--filter` for a small sample.
The runner never uses the network or changes either general codec package.

The local reference uses the error-handler stack metadata from Redis 7.0.15
`eval.c` and the final error suffix from `script_lua.c`. Python supplies SHA-1
of the exact snippet bytes independently with `hashlib`; error source, line
and digest remain part of the strict byte comparison.

The new library files implement the requested Redis compatibility in the
existing VM home; remove them only if Halo no longer provides those libraries.
The runner and corpus serve this compatibility experiment and are retained
with its results; retire them when a maintained library compatibility suite
owns these observations. Python is the compiler-independent reference driver.

The acceptance criterion is equality of typed RESP2 replies, including every
binary byte and error byte; a nonzero runner exit keeps the criterion open.
The scratch reference probes library globals and versions after registration,
because a Lua interpreter built without these libraries cannot be the oracle.
`RESULTS.md` records the compiler and executable digests and every mismatch;
`ORACLE.md` records the unchanged stored `libs/` group separately.

The JSON package has a stricter grammar and Unicode policy than Redis CJSON.
Halo therefore checks Redis's grammar and retains its scalar bytes, runs a
normalized document through the general decoder, and reconstructs Lua values.
Encoding uses the general writer's structural state while the adapter writes
Redis's raw byte escaping and invalid-number spelling. Finite number text
uses Halo's default `%.14g` formatter or its configured precision. These
policies belong to the Redis adapter, so neither general package is changed.
MessagePack uses the general token decoder and writer, with Redis's legacy
tag exclusions, signed Lua-integer conversion and stream protocol in Halo.

CJSON instances keep independent configuration and reusable buffers. Their
methods are heap closures with the reserved no-prototype sentinel and three
closed captures: the builtin ID and the two 32-bit halves of the full instance
index. Binding an instance does not consume host function IDs. Instance reclamation and Lua
local/field error names remain limitations recorded in `docs/todo.md`.
The local reference runs on macOS; Linux/glibc qualification remains required
by the Halo VM design's H4 platform reference.

## Closure binding assessment

A configured CJSON method must keep function identity after extraction from
its table, select the entire VM configuration index, and preserve all host
IDs at and above 4096. The draft implementation represents that state as
checked closed captures in an ordinary heap closure. The no-prototype
sentinel selects native binding interpretation; the capture decoder checks
live cells, closed numeric captures and the CJSON builtin range. Ordinary
native calls reset the active instance to zero before dispatch. CJSON code
makes no Lua callbacks, so its handler snapshots settings before work without
requiring a nested native activation record.

Packing an instance into `Closure.proto` was replaced because it truncated
the instance namespace to 23 bits and combined function and configuration
selection in an opaque integer. A VM registry keyed only by closure handles
would need coordinated deletion on collection and reuse; storing captures
with the heap closure keeps their tracing and reuse lifetime together.
An explicit native variant of the heap closure remains a viable alternative:
it would make native function binding a heap-level type, while these captures
use the same traced captured-value representation as other functions. The
capture representation is a provisional choice, to reopen when another
stateful native library requires a different binding payload or a native
function needs Lua callbacks. Instance configuration reclamation remains a
separate missing lifetime connection, as the TODO describes.

The new draft tree node `design/halo/closures.md` records this
recommendation (Q1); no owner approval or log entry is inferred.
