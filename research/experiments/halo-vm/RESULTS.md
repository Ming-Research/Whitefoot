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
| error levels 0, 1, 2, 3 through pcall | bare, child line 2, bare, parent line 3 | PASS |
| __add calling error at level 2 | caller line 2 | PASS |
| __concat calling error at level 2 | caller line 2 | PASS |

The Lua reference run completed successfully; its exact output is
`oracle.expected`. All package modules accept; the native smoke entry and final twenty-two-case
native suite pass.

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
13 host return, 14 host raise, 15 host stop, 16 metamethod host stop,
17–20 error levels 0–3, 21 __add error location, 22 __concat error location.

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

The second acceptance attempt factored arithmetic and comparisons by family, leaving
register/constant operand decoding in `run` and the one shared slow executor.
Before that attempt the criterion is: every module must accept; a shorter check
would support the hypothesis that the large generic proof inventory caused the
observed cost. This refactoring introduces helper calls on numeric paths; native
inlining and dispatch performance have not been measured.

The family-helper attempt was stopped without a verdict after 621.93 seconds
(wrapper exit 143), at 4,216 lines / 146,585 bytes for the VM. It therefore did
not demonstrate a shorter check. The third attempt removed the common dispatch
fact join: each instruction arm ends in its own guaranteed self-tail call, with
a shared helper for outcome handling. The acceptance and elapsed-time criterion
is unchanged.

The per-arm attempt was stopped without a verdict after 368.58 seconds (wrapper
exit 143); `dispatch.wf` was 2,093 lines / 91,693 bytes. A second one-second
local stack sample still showed generic-body proof derivation.

The fourth attempt moved each opcode body into a private handler function,
keeping its numeric/table fast path and shared `slow` fallback together. `run`
remains the eight-parameter guaranteed self-tail dispatcher. Its `Cell` match
selects a handler, and one checked continuation feeds the tail call. This bounds
the proof inventory of each opcode; native inlining and its performance effect
are unmeasured. The acceptance and checker-time criterion remains unchanged.

The first opcode-split run returned a source rejection after 324.13 seconds:
`slow.wf` read its comparison-result destination after a callback without an
available stack-length fact. Both comparison paths now restore the entry length
before reading that slot. At that attempt the VM was 6,647 lines / 236,864 bytes,
with `run` in a 237-line / 16,456-byte file. That rejection did not establish full
acceptance; its elapsed time is a checker-cost finding.

## Accepted module milestone

On revision `96a17771077da8e097ed2a069bfc0ea75e2ae46b`, the requested compiler with
`--cache /private/tmp/halo-vm-build-cache --graph lib/halo/modules.wfg
--check-modules` accepted `pkg::value`, `pkg::number`, `pkg::lex`, `pkg::heap`
and `pkg::vm`. The wrapper reported 57.43 seconds, exit 0 (compiler: 57.36 real,
53.18 user, 4.11 system). The VM was 6,649 lines / 236,968 bytes.
The same opcode split that first exposed the callback-bound failure now accepts
after the length restoration was moved before both comparison result reads.
This establishes acceptance, not native performance or complete Lua parity.
These runs used an incremental cache and changed proof bodies; they are not a
controlled comparison attributing checker time solely to the structural split.

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

## Review and implementation limits

A separate read-only review checked groups A, D, C, M and V, with R limited to
the opcode split, against base `17a96f98694a9787b8c178717a7f099bd7944c5d`.
It read the local PUC sources and did not rerun green suites. Its findings were:
missing error-level locations, incomplete fixture coverage documentation,
callback frame PCs that lost the calling instruction's location, and the
concat continuation's distinct PC convention. The code and fixtures now address
these paths; final verification is recorded below. The opcode split remains an
implementation deviation from the single inline match described in VM.md.
There is one eight-parameter guaranteed self-tail dispatcher, private opcode
handlers with inline numeric/table operations, and one shared slow executor.
No native inlining or performance claim follows from the short correctness runs.

All 71 Cell variants have handlers: moves and loads; globals and upvalues;
tables, Self and SetList; arithmetic and unary operations; length and concat;
fused comparisons/tests and jumps; Call, TailCall, Return and VarArg; numeric
and generic for loops; Closure and Close. Builtins 0–7 and host IDs >=4096 are
wired. The compiler conventions are matched: exact Script field shape, closure
capture descriptors following Closure, and fused jumps taken when comparison
or truthiness equals the encoded flag.

`TODO(number)` remains explicit in `number_coercion_pending`,
`concat_coercion_pending` and `power_pending`. Arithmetic on numeric strings,
number-to-string concat, numeric-string for-loop/select/error-level inputs,
and numeric error-message formatting are incomplete. Numeric Pow also remains
unavailable: pkg::number is empty here and the current language has no pow
intrinsic; the stub raises an explicit unavailable error, which differs from
Lua's result. The __pow metamethod path is present. The other implemented
arithmetic paths operate on f64 values. No pkg::number function is called.

Source identity is fixed to Halo's documented `@user_script` chunk name because
Script has no source-name field. Missing or zero line information leaves a string
error unchanged. Runtime-generated errors currently retain generic text without
source-location decoration. Tail-call traceback levels have not been validated
and Frame does not retain a count of eliminated frames. String ordering is byte
lexicographic; locale-sensitive PUC ordering is unverified. Heap GC/limits remain
the heap module's responsibility. These limits mean this result is not full Lua
5.1 conformance, and fixtures do not exercise every opcode or metamethod branch.

The protected-error register repair, comparison callback bound repair, and the
error-location findings were fixed in the allowed files. No compiler, heap,
specification, design tree, or repository gate was changed. No network, Cargo,
push, or PR operation was performed. `make check` was not run because the task
prohibits Cargo; `make static` passed in 32.22 seconds (exit 0). The smallest
static sample, `make design-lint`, passed in 8.04 seconds before that full run.

## Error-location verification

The twenty-case suite (including error levels 0–3) built in 120.18 seconds and
ran in 0.54 seconds, both exit 0. Adding the __add callback location observation
gave twenty-one cases: build 117.64 seconds and execution 0.53 seconds, exit 0.
The equivalent Lua probes distinguish level 1's child location, level 2's
line-less native pcall frame, and level 3's parent location.

The __concat fixture places its caller's concat at line 2 and the immediately
preceding load at line 1. With only concat PC normalization deliberately removed,
the suite built in 117.92 seconds (exit 0), then returned fixture index 22 in
0.54 seconds. Thus this observation detects the incorrect preceding-instruction
location while all prior cases pass. The correction was restored before final
validation. No deliberately failing version was committed.

The corrected VM module (6,761 lines / 240,433 bytes) accepted with the requested
all-module command in 57.18 seconds (exit 0; compiler 57.17 real, 53.24 user,
3.80 system). The final corrected twenty-two-case result is recorded below.

## Final corrected native result

At revision `fc780072e4b497d053a26ca515ae2ea01a6500db`, the corrected twenty-two-case
suite built in 117.71 seconds (exit 0; compiler 117.68 real, 108.66 user,
8.61 system) and executed in 0.54 seconds (exit 0; executable 0.45 real,
0.04 user, 0.00 system). All required Lua-derived fixtures and the additional
host, budget/metamethod, and error-location observations pass. The reference
Lua output matches `oracle.expected`; `sh -n run.sh` and `git diff --check` pass.
The changed-hunk review reported no remaining concrete logic issue after the
concat PC normalization. Its stated diagnostic and conversion limits are
recorded above rather than presented as implemented behavior.
