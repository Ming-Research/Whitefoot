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
| fib(20), recursive Call/Return | 6765 | pending |
| numeric for sum 1..100 | 5050 | pending |
| shared counter upvalue, before and after Close | 1, 2 | pending |
| integer/string table keys | 115 | pending |
| table-form __index chain and Lua __add | 17 | pending |
| same __add with budget 1 | 17 | pending |
| pcall of table-valued error and nested error | false, 3, false | pending |
| varargs and select('#', ...) | 4 | pending |
| tail recursion, depth 100000, unlimited | 100000 | pending |
| same tail recursion, budget 1 | 100000 | pending |
| same tail recursion, budget 7 | 100000 | pending |

The Lua reference run completed successfully; its exact output is
`oracle.expected`. Package acceptance and native execution are still pending.

## Reproduction

```sh
WHITEFOOTC=/private/tmp/wf-halo/compiler/target/gate/whitefootc \
LUA=/private/tmp/halo-e1-lua/redis/deps/lua/src/lua \
sh research/experiments/halo-vm/run.sh
```

The runner starts with the smallest smoke build and execution before building the
combined short suite. It invokes no Cargo and uses the host-wide check wrapper.
The suite's exit status identifies its first failed fixture (1..11 in source;
12 is the metatable budget/resume case).

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
