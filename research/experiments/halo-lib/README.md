# Halo slice 1 library comparison

Compiler builds and checks have a persistent cache at
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-lib`.
`--cache DIR` overrides it; `--no-cache` disables caching. Cache paths must be
outside the repository and survive scratch-executable cleanup.
This runner reports program execution times, so native builds default to
`--full-lto`. The compiler rejects combining `--full-lto` with `--cache`;
`--incremental` selects the persistent cache instead. Cached runtime timings
have unvalidated differences from full LTO and are only sizing observations.
Use the default or explicit `--full-lto` for runtime performance measurements.
Reused binaries must have been built with the corresponding mode.

This experiment compiles Lua source with `pkg::compile`, runs it with
`pkg::vm::start`, and compares printed text with Redis's bundled Lua 5.1.5.
It serves the slice 1 library and number wiring described in
`research/investigations/halo/VM.md`, sections 3 and 8. The adapter, corpus,
and comparison runner live here, outside the compiler gates; remove the
adapter and runner when an embedding-level oracle replaces this experiment.

Work is local to this worktree, with no Cargo invocation or network access.
Use the supplied native compiler through the `--compiler` option.

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
  --compiler /path/to/whitefootc --incremental --lua /path/to/lua \
  --redis-source /path/to/redis-7.0.15 --sample
python3 research/experiments/halo-lib/run.py \
  --compiler /path/to/whitefootc --full-lto --lua /path/to/lua \
  --redis-source /path/to/redis-7.0.15 \
  --results research/experiments/halo-lib/RESULTS.md
```

`--adapter` reuses an existing native adapter; `--cache` overrides the persistent
compiler cache directory used by `--incremental`. The runner checks process exit codes directly, compares bytes
and prints unified diffs. A correctness run uses budgets unlimited, 7 and 1.
Comparator controls change an output byte, remove a line and reorder lines;
each must turn an equal comparison into a mismatch. Builds and generated
outputs are temporary and are not committed.

The reference interpreter is the supplied standalone Lua executable, which
uses libc random by default and has dynamic library loading disabled. For
`random.lua` only, the runner relinks its adjacent `lua.o` and `liblua.a` with
`reference.c`, Redis's original `src/rand.c`, and the original `linit.c` under
a renamed initialization symbol. The shim replaces only the random callbacks
at library initialization. The callback bodies come from `script_lua.c`;
the bootstrap represents a script invocation, so registry invocation checks
are omitted. A C compiler and those adjacent Lua build objects are required.
All non-random scripts use the original executable as the oracle, and also
check that the relinked executable has identical output. `reference.lua`
loads each unchanged corpus source as `@user_script`. The C bootstrap earns
its place as an independent oracle for Redis's C conversion and error-stack
behavior, which a Lua wrapper cannot preserve across tail calls. Remove it
with this comparison runner.

The host `print` adapter prints scalar arguments as Lua text, with tab
separators and a newline; the corpus explicitly calls `tostring` for
metamethod results. It does not implement an embedding's full print builtin.
Pointer-shaped default object text uses Halo handles, so literal PUC address
strings cannot be compared. Host-specific last-bit libm and NaN spelling
differences remain observable. These tests cover the requested functions and
selected boundaries, rather than exhaustively certifying every floating
input or platform-dependent C conversion.

`string.format` follows the supplied macOS Lua's NaN flag behavior: NaN
suppresses sign flags, while infinity retains them. Script metadata currently
provides line numbers but no original source name or local debug names. Error
locations therefore use the existing `user_script` convention and registered
builtin names; aliases and method calls cannot reproduce every PUC debug name
without metadata changes outside this task's file boundary.
