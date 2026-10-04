# Halo slice 1 library comparison

This experiment compiles Lua source with `pkg::compile`, runs it with
`pkg::vm::start`, and compares printed text with Redis's bundled Lua 5.1.5.
It serves the slice 1 library and number wiring described in
`research/investigations/halo/VM.md`, sections 3 and 8. The adapter, corpus,
and comparison runner live here, outside the compiler gates; remove the
adapter and runner when an embedding-level oracle replaces this experiment.

Work is local to this worktree, with no Cargo invocation or network access.
The supplied compiler is `/private/tmp/wf-halo/compiler/target/gate/whitefootc`.

The library implementation and port lineage are described in
[the VM library license notes](../../../lib/halo/vm/LICENSE.md). Each new VM
source serves the slice 1 builtin dispatcher or its callback continuations;
its existing home is `lib/halo/vm/`, and it is removed if that dispatcher or
its corresponding port is replaced. The experiment files serve this comparison
and are removed when an embedding-level oracle supersedes it.

The registration API is `pkg::vm::library_builtin(index)`, with count
`library_builtin_count`. Each row has a library number (0 globals, 1 string,
2 table, 3 math), fixed name bytes plus a length, and a stable builtin ID.
`install_libraries` consumes these rows and installs the numeric constants
`math.pi` and `math.huge`; embeddings may instead consume the rows directly.
`ipairsaux` is an iterator implementation rather than an installed global.

Run a small sample first, then the corpus after its build and execution times
are known:

```sh
python3 research/experiments/halo-lib/run.py \
  --compiler /path/to/whitefootc --lua /path/to/lua --sample
python3 research/experiments/halo-lib/run.py \
  --compiler /path/to/whitefootc --lua /path/to/lua \
  --results research/experiments/halo-lib/RESULTS.md
```

`--adapter` reuses an existing native adapter; `--cache` selects a scratch
compiler cache. The runner checks process exit codes directly, compares bytes
and prints unified diffs. A correctness run uses budgets unlimited, 7 and 1.
Comparator controls change an output byte, remove a line and reorder lines;
each must turn an equal comparison into a mismatch. Builds and generated
outputs are temporary and are not committed.

The reference interpreter is the supplied standalone Lua executable, which
uses libc random by default. `reference.lua` installs the Redis recurrence
using exact 16-bit arithmetic before compiling the unchanged corpus source
with chunk name `@user_script`. Both sides therefore execute identical corpus
bytes; only the reference's embedding bootstrap differs. The bootstrap is
independent of Halo and follows Redis's C sources. Host-specific final-bit
libm and NaN spelling differences remain visible in the comparison.
