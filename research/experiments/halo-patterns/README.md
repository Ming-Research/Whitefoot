# Halo pattern comparison

The runner builds the Halo end-to-end host and compares authored pattern cases
with local Redis Lua 5.1. It is explicitly invoked research outside the gate.
Keep this invocation guide with the runner; remove both when maintained
pattern tests replace the experiment.

```sh
perl .github/run-check.pl halo-patterns python3 -B research/experiments/halo-patterns/run.py \
  --compiler "$WHITEFOOTC" --lua "$LUA" --incremental --limit 1 --report "$SCRATCH/pattern-sample.md"
perl .github/run-check.pl halo-patterns python3 -B research/experiments/halo-patterns/run.py \
  --compiler "$WHITEFOOTC" --lua "$LUA" --full-lto --report "$SCRATCH/patterns.md"
```

The cache is `${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-patterns`,
persistent outside the repository; `--cache DIR` overrides it and `--no-cache`
disables it. Native builds default to `--full-lto` because the runner reports
execution times. The compiler rejects full LTO with caching; `--incremental`
selects caching and produces runtime sizing observations whose differences
from full LTO remain unvalidated. `--binary PATH` reuses a prebuilt host;
its caller must choose the corresponding build mode. `--budgets 1,7,1000`
selects all supported budgets. `--limit N` sizes a run before the full batch.
