# Halo heap core comparisons

This explicitly invoked experiment compares the Halo heap with Redis 7.0.15's
bundled PUC Lua 5.1.5. It is not a gate dependency. The fixture binds the real
Halo package under MOD-11. `run.py` requires acceptance of every Halo module,
builds the bound native program, reads exit codes directly, and compares every
output field and row with an independently executed Lua program. Python drives
native tools and compares observations; it does not implement a table.

From the repository root, provide the existing compiler and reference Lua:

```sh
python3 research/experiments/halo-heap-core/run.py --compiler "$WF_COMPILER" --lua "$LUA" --report "$SCRATCH/report.md"
```

Use the Lua executable built from Redis's bundled sources, rather than stock
Lua 5.1: Redis's `lstring.c` hashes every byte. The supplied compiler is used
without rebuilding it. A one-sequence mixed-key sample executed in less than
0.4 seconds; the final eight-sequence fixture also stays below one second.
These are correctness runs, not performance comparisons.

The original 2,048-step sequence uses seed 12,345 and recurrence
`(1664525 * seed + 1013904223) mod 2^32`. Each step sets a numeric key, reads it,
traverses the table, and finds its border. Whitefoot checks every key against a
separate dense value model and checks traversed values, duplicates and missing
keys. Every 31st step deletes current keys divisible by three during traversal.
Six insertion orders followed by hole-producing deletions supply 288 dedicated
`#` observations. Tags 0, 1 and 2 retain the columns
`tag step key value border count weighted-sum`; every column must match Lua.

Eight further 1,024-step sequences use seeds `12345 + 997 * case`, the same
recurrence, and 162 distinct keys: integers 0 through 95; sixteen additional
numbers (fractions, negative numbers, infinities, subnormals, finite extrema,
word-sum overflow witnesses and integers beyond the array domain); 32 long
strings differing in their first byte; sixteen short strings with embedded
zeros and byte 255; and both booleans. The Lua fixture spells the numeric
values arithmetically, independently of Whitefoot's binary64 bit literals.
Whitefoot checks all 162 key/value lookups after every assignment, including
nil assignments. After each 128-step segment it traverses with `table_next`;
Lua traverses with `next`. Tag 3 prints
`tag case step ordinal key-id value border`, and tag 4 prints
`tag case step pair-count border 0 0`. Key IDs are input identities, not hash
positions. Identical rows require identical order, keys, values and borders;
the terminal rows also check empty or missing traversals. The driver detects
changed fields, missing rows and reordered pairs using deliberately wrong
traces.

Before the traces, `units` checks canonical interning and `alloc_string`,
embedded zeros, unequal full byte strings with equal full-byte hashes, 128
one-byte reinternings, intern-chain deletion, freed string/table/upvalue handle
reuse, byte subtraction, repeated free, closure creation/release, nil/NaN key
errors, invalid `next`, signed zero, fractional and boolean keys, string keys,
table keys and builtin keys. Assertion failures exit 2; trace/model failures
exit 1. The bound module path replaces the retired private-payload diagnostic
bundle.

The table uses PUC's chained scatter nodes, Brent relocation, decreasing
`lastfree`, nil-valued retained-key lookup, rehash sizes and resize insertion
order. A zero-length node buffer represents its immutable dummy node.
Number, string and boolean key order matches the bundled implementation;
table, closure and builtin keys use handles where PUC uses addresses, the
remaining order difference. Function signatures and error results are stable;
`Node.next` and `Table.lastfree` are new public representation fields, documented
in the heap interface. `node_count` still counts retained hash keys.

For optional oracle layout inspection, build the existing C observer against
the same headers and library as the reference executable:

```sh
cc -I "$LUA_SRC" research/experiments/halo-heap-core/layout_probe.c "$LUA_SRC/liblua.a" -lm -o "$SCRATCH/probe"
"$SCRATCH/probe" research/experiments/halo-heap-core/reference.lua > "$SCRATCH/lua-trace" 2> "$SCRATCH/lua-layout"
```

Its stderr columns are `step array-size nodes-size occupied live`, for steps
185 through 225 of the original sequence. It reads the real Lua layout without
changing operations. Remove these fixtures when maintained heap/collector tests
replace them; retain the dated results as evidence. Every fixture is consumed
by the graph, comparison command or documented observer command. Scratch source,
reports and binaries are temporary and are not committed.
