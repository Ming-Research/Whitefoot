# Halo dispatch-core experiment

The hand-written `Cell` scripts are checked against the equivalent programs in
`oracle.lua`, executed by the requested PUC Lua 5.1.5 binary. The fixtures compare
result counts, values and booleans; they return a distinct nonzero status on a
mismatch. The tail fixtures also require the value stack to remain at 1024 slots
and the frame stack to be empty at completion. A finite-budget tail run must
actually suspend. The budget-1 metatable fixture must suspend inside `__add` and
resume to the same result, rather than treating exhaustion as an error.

This experiment directory owns these fixtures and their runner. Remove them when
an equivalent maintained Halo program suite takes ownership of these observations.

| Script | PUC result | Native status |
| --- | --- | --- |
| fib(20), recursive Call/Return | 6765 | PASS |
| numeric for sum 1..100 | 5050 | PASS |
| shared counter upvalue, before and after Close | 1, 2 | PASS |
| integer/string table keys | 115 | PASS |
| table-form __index chain and Lua __add | 17 | PASS |
| same __add with budget 1 | 17 | PASS |
| pcall of table-valued error and nested error | false, 3, false | PASS |
| varargs and select('#', ...) | 4 | PASS |
| tail recursion, depth 100000, unlimited | 100000 | PASS |
| same tail recursion, budget 1 | 100000 | PASS |
| same tail recursion, budget 7 | 100000 | PASS |

The Lua reference run completed successfully; its exact output is
`oracle.expected`. All package modules accept; the native smoke entry passes; the combined suite passes.

## Reproduction

```sh
WHITEFOOTC=/private/tmp/wf-halo/compiler/target/gate/whitefootc \
LUA=/private/tmp/halo-e1-lua/redis/deps/lua/src/lua \
sh research/experiments/halo-vm/run.sh
```

The runner starts with the smallest smoke build and execution before building the
combined short suite. It invokes no Cargo and uses the host-wide check wrapper.
The suite's exit status identifies its first failed fixture: 1 smoke, 2 fib,
3 numeric for, 4 counter, 5 tables, 6 metatable, 7 protected error, 8 varargs,
9 tail unlimited, 10 tail budget 1, 11 tail budget 7, 12 metatable budget 1,
13 host return, 14 host raise, 15 host stop, 16 metamethod host stop.

## Checker cost

The first full `--check-modules` run on the resumed worktree was stopped after 494.80
seconds without returning a verdict (wrapper exit 143). At that point `pkg::vm`
was 4,883 lines / 171,919 bytes across its interface and seven implementation
files; `run` occupied 2,098 lines in `dispatch.wf` (the file was 2,128 lines /
88,033 bytes). This is a checker-cost finding, not a source-language rejection.
The dispatch match keeps numeric fast paths in its arms; calls, continuations,
state, builtins, closure captures and the shared slow executor already have
separate functions. A natural next split, if required, is ending each arm before
the common fact join, following the accepted self-tail prototype.

The next acceptance attempt factors arithmetic and comparisons by family, leaving
register/constant operand decoding in `run` and the one shared slow executor.
Before that attempt the criterion is: every module must accept; a shorter check
would support the hypothesis that the large generic proof inventory caused the
observed cost. This refactoring introduces helper calls on numeric paths; native
inlining and dispatch performance have not been measured.

The family-helper attempt was stopped without a verdict after 621.93 seconds
(wrapper exit 143), at 4,216 lines / 146,585 bytes for the VM. It therefore did
not demonstrate a shorter check. The next attempt removes the common dispatch
fact join: each instruction arm ends in its own guaranteed self-tail call, with
a shared helper for outcome handling. The acceptance and elapsed-time criterion
is unchanged.

The per-arm attempt was stopped without a verdict after 368.58 seconds (wrapper
exit 143); `dispatch.wf` was 2,093 lines / 91,693 bytes. A second one-second
local stack sample still showed generic-body proof derivation.

The current attempt moves each opcode body into a private handler function,
keeping its numeric/table fast path and shared `slow` fallback together. `run`
remains the eight-parameter guaranteed self-tail dispatcher. Its `Cell` match
selects a handler, and one checked continuation feeds the tail call. This bounds
the proof inventory of each opcode; native inlining and its performance effect
are unmeasured. The acceptance and checker-time criterion remains unchanged.

The first opcode-split run returned a source rejection after 324.13 seconds:
`slow.wf` read its comparison-result destination after a callback without an
available stack-length fact. Both comparison paths now restore the entry length
before reading that slot. At that attempt the VM was 6,647 lines / 236,864 bytes,
with `run` in a 237-line / 16,456-byte file. The next check must establish full
acceptance; the elapsed time is a checker-cost finding even if it accepts.

## Accepted module milestone

On revision `96a17771077da8e097ed2a069bfc0ea75e2ae46b`, the requested compiler with
`--cache /private/tmp/halo-vm-build-cache --graph lib/halo/modules.wfg
--check-modules` accepted `pkg::value`, `pkg::number`, `pkg::lex`, `pkg::heap`
and `pkg::vm`. The wrapper reported 57.43 seconds, exit 0 (compiler: 57.36 real,
53.18 user, 4.11 system). The VM was 6,649 lines / 236,968 bytes.
The same opcode split that first exposed the callback-bound failure now accepts
after the length restoration was moved before both comparison result reads.
This establishes acceptance, not native performance or complete Lua parity.

## Native smoke milestone

The smoke entry built successfully in 120.06 seconds (exit 0) and ran in
0.42 seconds (exit 0). Fixture repairs changed only canonical graph spacing,
`Env` construction field order, and float literal spellings; the Lua-derived
expected values are unchanged. The combined suite build follows this smallest
useful successful sample.

## Native suite milestone

On core revision `d0e623b0d07f88e25083ef3841c74e135fdfffda`, the combined suite
built in 59.45 seconds (exit 0). Its first execution returned fixture index 7
after 0.43 seconds: the hand-built protected-error script retained its first
error in register 4, which the second overlapping call frame reused. The fixture
now keeps that value in register 1, below the second call's function slot.
The expected `false, 3, false` result and all assertions remain unchanged.
After this fixture repair, the suite rebuilt in 59.54 seconds (exit 0) and
ran in 0.54 seconds (exit 0). All sixteen fixture observations passed,
including host return/raise/stop and preservation of the stopped stack inside
a metamethod. The equivalent Lua reference was also executed and compared with
`oracle.expected` successfully. These short runs are correctness observations,
not performance comparisons.
