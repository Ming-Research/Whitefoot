# MessagePack comparison

The runner builds the standalone MessagePack adapter and a local Redis cmsgpack
reference, then checks independently expected wire bytes and decoded values.
This invocation guide serves the explicit codec experiment; remove it with the
runner when maintained library tests replace the experiment.

```sh
perl .github/run-check.pl msgpack python3 -B research/experiments/msgpack/run.py \
  --compiler "$WHITEFOOTC" --redis-src "$REDIS_SOURCE/deps/lua/src" \
  --lua-lib "$REDIS_SOURCE/deps/lua/src/liblua.a" --scratch "$SCRATCH" \
  --incremental --generated 0
perl .github/run-check.pl msgpack python3 -B research/experiments/msgpack/run.py \
  --compiler "$WHITEFOOTC" --redis-src "$REDIS_SOURCE/deps/lua/src" \
  --lua-lib "$REDIS_SOURCE/deps/lua/src/liblua.a" --scratch "$SCRATCH" \
  --full-lto --generated 3000
```

Use an existing compiler and local Redis 7.0.15 sources/archive; no download or
Cargo build is needed. `--scratch` must be outside the repository. The cache is
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/msgpack`, persistent
across runs; `--cache DIR` overrides it and `--no-cache` disables it. The runner
reports native execution times, so native builds default to `--full-lto`.
Full LTO and the compiler cache are mutually exclusive; `--incremental` selects
caching and labels runtime timings as having unvalidated LTO differences.
The sample still runs the fixed boundaries and comparator controls.
[API.md](API.md) describes the interface; [RESULTS.md](RESULTS.md) records dated evidence.
