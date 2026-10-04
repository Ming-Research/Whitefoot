# Redis Lua codec compatibility experiment

`python3 -B research/experiments/halo-luacodecs/run.py --compiler /path/to/whitefootc --redis-source /path/to/redis-7.0.15`
builds a scratch standalone reference from Redis’s bundled Lua and all four
libraries, compiles the existing Halo end-to-end driver, and compares typed
RESP2 replies for a generated corpus, including binary strings and errors.
Use `--binary` to reuse the Halo executable and `--filter` for a small sample.
The runner never uses the network or changes either general codec package.

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
methods are heap closures with a native prototype marker, so binding an
instance does not consume host function IDs. Instance reclamation and Lua
local/field error names remain limitations recorded in `docs/todo.md`.
The local reference runs on macOS; Linux/glibc qualification remains required
by the Halo VM design's H4 platform reference.
