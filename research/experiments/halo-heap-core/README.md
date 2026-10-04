# Halo heap core comparisons

This explicitly invoked experiment checks the heap described in
[VM.md section 2](../../investigations/halo/VM.md#2-heap) against Redis's PUC
Lua 5.1.5. It is not wired into repository gates. The fixture is a module
program binding the real Halo package under MOD-11. `run.py` compares output
from native Whitefoot and the Lua executable, reads their exit codes directly,
and reports every different or missing row. Python only drives native tools
and compares independent observations; it does not implement a table.

From the repository root, provide the existing compiler and reference Lua:

```sh
python3 research/experiments/halo-heap-core/run.py --compiler "$WF_COMPILER" --lua "$LUA"
```

The compiler is the supplied v0.90 binary; it is never rebuilt. The native
sample took less than 0.4 seconds, so the experiment stays at 2,048
pseudo-random steps and 288 dedicated border observations, without a timing
comparison or larger batch. The LCG seed is 12,345, recurrence
`(1664525 * seed + 1013904223) mod 2^32`. Both programs generate their own
operations from that seed.

Each random step sets a numeric key, reads it, traverses the table, and finds
its border. Whitefoot also checks every key against a separate dense value
model, and checks every traversed key/value and duplicate or missing key.
Every 31st step deletes current keys divisible by three during traversal.
The trace aggregates traversal count and weighted value sum, allowing the
node-index order that VM.md deliberately leaves different from Lua. Trace
columns are `tag step key value border count weighted-sum`. Tags 1 and 2
cover six insertion orders and subsequent hole-producing deletions; tag 0
is the random sequence.

Before the trace, `units` checks equal and unequal interned strings, the
public canonical `alloc_string` path, embedded
zeros, a deliberately equal sampled hash with different full bytes, 128
one-byte reinternings, collision-chain deletion, freed string/table/upvalue
handle reuse, byte subtraction, repeated free, closure creation/release,
nil/NaN key errors, invalid `next`, signed zero, fractional and boolean keys,
string keys, table keys, and builtin keys. A failed assertion returns exit
status 2; model/traversal failures return 1. The comparison driver exercises
its different-field and missing-row failure paths with known wrong traces.

## Current interface blocker and diagnostic mode

The supplied `pkg::value` exports its enums but keeps all their payload fields
private, including the views' payloads. MOD-5 forbids heap construction of
`Str`, `Tab`, `Fun`, and numeric/view access. No value-module edit is allowed
in this task. The ordinary command therefore exits 2 with the actual module
rejection; it does not silently substitute another build.

For diagnostic execution only:

```sh
python3 research/experiments/halo-heap-core/run.py --compiler "$WF_COMPILER" --lua "$LUA" --diagnostic-bundle --layout
```

This explicitly requested mode generates a temporary source bundle from the
current value types/bodies, heap types/bodies and fixture. It removes module
qualifiers, local aliases, and interface visibility modifiers, so value
payloads become accessible in the same module. Heap function bodies are
otherwise used verbatim. The temporary source and executable are removed.
This diagnoses heap behavior; **it does not establish module acceptance**.
The command still exits nonzero for the original module rejection or any
trace mismatch. The optional layout instrumentation prints tag 9 rows for
steps 185 through 225; those rows are reported separately, never substituted
for expected results.

To inspect the oracle's sizes at those same steps without changing its
operations, build the optional C observer with the headers and existing
library of the reference Lua:

```sh
cc -I "$LUA_SRC" research/experiments/halo-heap-core/layout_probe.c "$LUA_SRC/liblua.a" -lm -o "$SCRATCH/probe"
"$SCRATCH/probe" research/experiments/halo-heap-core/reference.lua > "$SCRATCH/lua-trace" 2> "$SCRATCH/lua-layout"
```

Its stderr columns are `step array-size nodes-size occupied live`. The C
probe exists solely to attribute observed border differences through Lua's
actual internal layout, not to change the reference or model. Remove these
experiment fixtures when replaced by maintained heap/collector tests; keep
the dated results as historical evidence. Each test source is consumed by
the graph or the comparison command above; neither generated bundles nor
binaries are committed.
