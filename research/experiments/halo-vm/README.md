# Halo VM comparison

The explicitly invoked runner checks all Halo modules, builds the smoke and
suite entries, and compares local Lua output with the stored oracle. Keep this
guide with the runner; retire both when maintained VM tests replace them.

```sh
WHITEFOOTC=compiler/target/gate/whitefootc LUA="$LUA" \
  research/experiments/halo-vm/run.sh
WHITEFOOTC=compiler/target/gate/whitefootc LUA="$LUA" \
  research/experiments/halo-vm/run.sh --no-cache
```

The default cache is
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-vm`, outside
the repository and persistent across runs. Scratch executables are deleted;
the cache is retained. `--no-cache` disables it. `--full-lto` builds the native
entries with whole-build LTO without caching. The runner checks correctness
and reports build times, not program execution times.
