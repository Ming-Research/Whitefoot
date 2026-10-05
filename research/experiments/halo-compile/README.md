# Halo compiler oracle

Compiler builds and checks have a persistent cache at
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-compile`.
`--cache DIR` overrides it; `--no-cache` disables caching. Cache paths must be
outside the repository and survive scratch-executable cleanup.
This runner reports program execution times, so native builds default to
`--full-lto`. The compiler rejects combining `--full-lto` with `--cache`;
`--incremental` selects the persistent cache instead. Cached runtime timings
have unvalidated differences from full LTO and are only sizing observations.
Use the default or explicit `--full-lto` for runtime performance measurements.
Reused binaries must have been built with the corresponding mode.

This experiment compares the public `halo::compile::compile` result against
Redis 7.0.15's bundled PUC Lua 5.1 compiler. The `dump` Whitefoot module is
its consumer. `run.py` supplies the existing Halo oracle corpus, generated
small programs, and malformed programs. These files remain useful until this
port is replaced or Halo is retired; they run explicitly and are outside the
repository gate.

The runner builds the candidate with the supplied Whitefoot compiler; first run a
small sample. Heavy commands use the repository's host-wide verification lock:

```sh
perl .github/run-check.pl halo-compile-oracle python3 -B \
  research/experiments/halo-compile/run.py --compiler "$WHITEFOOTC" \
  --incremental --lua-source "$LUA_SOURCE" --sample 3 --output "$SCRATCH/sample.json"
perl .github/run-check.pl halo-compile-oracle python3 -B \
  research/experiments/halo-compile/run.py --compiler "$WHITEFOOTC" \
  --full-lto --lua-source "$LUA_SOURCE" --output "$SCRATCH/results.json"
```

`WHITEFOOTC` names an existing Whitefoot executable; `LUA_SOURCE` names the
Redis Lua `src` directory; `SCRATCH` is an existing directory outside this
repository. No Cargo build or network access is used. The runner copies only
C sources and headers into temporary storage, times a single `lcode.c` build,
and builds `luac` there. `--luac` can reuse an already-built scratch oracle.
`--halo-dump PATH` reuses a prebuilt candidate. `--filter TEXT` selects case names. Missing or empty corpora fail the run.
The candidate dump reads one complete source from stdin and uses `=stdin` as
its chunk name, matching `luac`'s stdin source name. The runner also checks
interning exhaustion on string/name tokens, lookahead names/strings and synthetic
`arg`/numeric-for locals: one extra dump invocation argument sets heap accounting to
u64 MAX, while two arguments set it to MAX minus 25 (one one-byte string).
These six checks must return raw `not enough memory` rather than a Script.
They test the heap interface's documented Nil result without allocating
unbounded storage.

## Independent mechanical mapping

The oracle is `luac -l -l -o <scratch> -`, including instruction line numbers,
constant contents and prototype headers. The runner parses that listing;
it does not parse Lua source or invoke Halo's private compiler functions.
Prototype addresses in the listing identify `CLOSURE` children and become
lexical preorder indexes, with main at zero. Each prototype's code and
constants are concatenated in that order. The main prototype can therefore
be inspected separately even when function bodies were parsed first.

- PUC's printed negative RK operand `-1-k` becomes constant operand `k`,
  selecting the corresponding RR, RK, KR or KK cell family. Registers retain
  their numbers. `LOADK` selects `LoadKx` when `k >= 256`.
- EQ, LT and LE plus their following JMP become EqJmp, LtJmp and LeJmp.
  The expected boolean A is unchanged: **if (cmp(b, c) == a) jump to target
  else fall through**. TEST and TESTSET fuse similarly, preserving their C
  truth expectation; TESTSET copies B to A only on the taken path.
  TFORLOOP also consumes its following JMP.
- Each prototype's original instruction indexes map to absolute cell indexes.
  A removed JMP maps to its preceding fused cell. Relative jumps resolve
  using the original instruction PC before this map is applied. The fused
  cell keeps the first instruction's line number.
- SETLIST's extra data word when C is zero is represented by the cell's u32 C
  field and removed from the instruction sequence. Ordinary batching is 50
  fields. Constructor size fields retain PUC's floating-byte encoding.
- Closure capture MOVE/GETUPVAL pseudo-instructions remain separate cells,
  with A zero and the original local register or enclosing upvalue index.
  The VM consumes exactly the child's `nups` captures after Closure.
- Script constants receive 256 final Nil entries. Prototype headers compare
  start, kbase, nk, parameter count, maximum stack, upvalue count and the
  boolean vararg flag. PUC's legacy HASARG/NEEDSARG bits have no fields in the
  supplied Proto interface; this comparison records their boolean projection.
- Constants compare by Lua type and contents. String listings are decoded
  from PUC's escaped text and compared as bytes, including NUL. Numbers compare
  using PUC's displayed `%.14g`, matching Halo's number formatter. This observes
  listing parity, not every binary64 payload bit.

The runner compares complete records, including lengths, rather than matching
only common prefixes. Its controls perturb an operand and remove a cell.
RESULTS.md records the measured revision, comparisons and remaining limitations.
