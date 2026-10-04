# Halo F4 safepoint stress validation

Environment: Darwin arm64. The existing gate compiler was used without
Cargo, network, push or pull requests. The worktree branch is
`claude/halo-slice1`; the task base is
`c0cff8caed353ef0c601d407f14f47a5c888f25e`.

Compiler SHA-256:
`8d391bd75e31dbd2068f30e586c22cea59f10ef16b21b3f587d4364fec2beeb2`.
The stored Redis 7.0.15 corpus is the reply oracle; no expected reply or
oracle script was regenerated. All 80 scripts are included (6 apps,
10 libraries, 48 Lua core and 16 Redis API).

## Criterion and scope

The criterion was recorded in README.md before the runs, in local milestone
`44aa95c19a8a69c96c31684607edd9f57f229c0b`. A correct result requires exact
typed RESP2 parity at budgets 1, 7 and 1000, positive completed-collection
counts on every tested script, discrimination of each missing-root mutant,
and a Lua memory error followed by a successful script on the same engine
and store. Each mutant changes one root category and is restored before the
next. Build time and execution time are reported separately.

Stress uses full collections at the existing call and loop safepoints,
plus the initial embedding prototype-call boundary. Ordinary operation
keeps stress off. This is not collection at every allocation and is not a
complete heap-reachability verifier. The 64 MiB bound is on logical live
heap bytes after a safepoint collection, not process RSS or peak temporary
allocation; slab/intern reserve and native library configuration are outside
that existing accounting model.

## Initial controls and defects

The initial native build exited 0 in 443.65 s (user 346.87 s, system
50.90 s). The one-script, budget-7 sample `lua-core/counter-closure`
matched Redis with 7 collections; repeated direct native executions took
0.004048, 0.003436 and 0.003349 s. That scale justified the full corpus
comparison; correctness decisions do not depend on these timings.

The first stress comparison exited 1: 237/240 replies matched. Only
`lua-core/unpack-select-varargs` failed, at all three budgets, losing the
expected final integer 30 from its first array. `stack_limit` used only
fixed frame tops and cleared dynamic call arguments above that range.
Including `Vm.top` in the live bound fixes the underlying collector path.
The 12 scripts without an in-script safepoint had zero collections; the
initial prototype call now also invokes the ordinary collector path.

The bounded negative memory control was:

```lua
local t={} local payload=string.rep("x",1048576)
for i=1,70 do t[i]=payload..tostring(i) end
return #t
```

It exited 0 in 0.363477 s, returned 70, and reported 74,462,693 logical
heap bytes with a 67,108,864-byte limit. This showed that the limit was
unenforced without risking an unbounded allocator on the defective engine.
`gc_due` now forces collection when a nonzero limit is exceeded; surviving
bytes above it raise `not enough memory`, and every collector caller
propagates the failure through ordinary unwinding.

A consumed callback-stack snapshot also remained rooted after restoration.
The snapshot is now released after budget restoration or engine reset;
a host-stopped snapshot remains available through nested stop unwinding.
The smoke probe covers nested host Stop, reset, a following script and
collection-count preservation across reset.

## Root isolation conditions

- `open-upvalues.lua` expects bulk `kept`: abandoning a higher capture,
  then capturing a lower local after collection can corrupt the open-cell
  ordering when the abandoned cell is recycled. A retained middle local
  must close before its register is reused.
- `frame-closure.lua` expects bulk `qqq`. At budget 1,
  `--isolate-frames` clears only the redundant function-slot alias still
  equal to the executing frame's closure. The frame record becomes its
  sole owner. The ordinary corpus keeps that alias and cannot isolate the
  explicit frame-marking path.
- `suspended-stack.lua` expects bulk `zzz`. Local padding places `held`
  beyond the caller's stack length. At budget 1, `--collect-suspended`
  executes one synthetic collector back-edge before restoring the parked
  callback stack, using the original mirrored constant pool. Normal
  resumption restores that stack before collection and can conceal the
  missing snapshot root.

These last two flags are explicit experimental conditions, not default
execution. The scripts' expectations follow from their Lua source; they
are not replies copied from Halo.

## Missing-root mutations

The open-upvalue and frame mutants each passed all 240 ordinary corpus
runs, exposing coverage gaps. The local open-upvalue witness returned bulk
`wrong` instead of `kept` at every budget (runner exit 1). The isolated
frame witness returned `ERR invalid upvalue index` instead of `qqq` at
budget 1 (runner exit 1); its ordinary corpus run exited 0. The nested
host-stop smoke also exited 0 on that build.

The constants mutant failed 50 scripts at each budget, 150/240 runs
(runner exit 1). The failure sets and categories were identical across
budgets: 28 wrong replies, 15 error replies and 7 setup runtime failures
(native exit 4). No signal/crash was observed. The seven setup failures
emit no main-script telemetry. The mutation removes the constant marking
call while retaining reads of the pool: deleting the entire loop first
failed EFF-2 at compilation (exit 1, 2.77 s), an unused `reads(consts)`
effect; that compilation result is not counted as runtime discrimination.

| Constants mutant case | Failure at budgets 1, 7 and 1000 |
| --- | --- |
| `apps/hash-cas` | Setup runtime error; native exit 4 |
| `apps/lock-extend` | Setup runtime error; native exit 4 |
| `apps/queue-move` | Setup runtime error; native exit 4 |
| `apps/rate-limiter` | Error reply |
| `apps/redlock-release` | Setup runtime error; native exit 4 |
| `apps/sliding-window` | Setup runtime error; native exit 4 |
| `libs/cjson-arrays-nested` | Error reply |
| `libs/cjson-invalid` | Wrong reply |
| `libs/cjson-objects` | Wrong reply |
| `libs/cmsgpack-binary` | Wrong reply |
| `libs/cmsgpack-roundtrip` | Wrong reply |
| `libs/struct-integers` | Wrong reply |
| `libs/struct-strings-floats` | Error reply |
| `lua-core/assert` | Wrong reply |
| `lua-core/concat-numbers` | Wrong reply |
| `lua-core/embedded-zero` | Wrong reply |
| `lua-core/error-levels` | Wrong reply |
| `lua-core/error-values` | Wrong reply |
| `lua-core/meta-call` | Error reply |
| `lua-core/meta-comparisons` | Error reply |
| `lua-core/meta-concat-tostring` | Error reply |
| `lua-core/meta-index` | Wrong reply |
| `lua-core/meta-newindex` | Wrong reply |
| `lua-core/meta-protection-raw` | Wrong reply |
| `lua-core/next-pairs` | Wrong reply |
| `lua-core/nil-returns` | Error reply |
| `lua-core/numeric-coercion` | Error reply |
| `lua-core/pcall-xpcall` | Wrong reply |
| `lua-core/string-byte-char` | Error reply |
| `lua-core/string-escapes` | Wrong reply |
| `lua-core/string-find` | Wrong reply |
| `lua-core/string-format` | Wrong reply |
| `lua-core/string-gmatch` | Wrong reply |
| `lua-core/string-gsub-dynamic` | Wrong reply |
| `lua-core/string-gsub-string` | Wrong reply |
| `lua-core/string-length-order` | Error reply |
| `lua-core/string-match` | Wrong reply |
| `lua-core/string-transforms` | Wrong reply |
| `lua-core/table-concat` | Wrong reply |
| `lua-core/table-insert-remove` | Wrong reply |
| `lua-core/table-parts` | Wrong reply |
| `lua-core/unpack-select-varargs` | Error reply |
| `redis-api/call-error` | Error reply |
| `redis-api/call-success` | Error reply |
| `redis-api/log-keys-argv` | Error reply |
| `redis-api/lua-arrays` | Wrong reply |
| `redis-api/pcall-success-error` | Error reply |
| `redis-api/resp-error-raised` | Setup runtime error; native exit 4 |
| `redis-api/resp-values` | Setup runtime error; native exit 4 |
| `redis-api/sha1hex` | Wrong reply |

All three budget reports list the same failing cases; examples include
missing `KEYS` for `apps/rate-limiter`, a table-call error for
`lua-core/meta-call`, and corrupted bulk/array values for
`lua-core/concat-numbers` and the codec cases.

`open` mutant at parent `74f4f521bbc29a39919f3760b099292b799256b6`: runner source/selected-fixture digest `e22fcfc11163fdf302f36636e9232019733979adb3ba954b74bda2d30eeae689`; executable SHA-256 `4b39d3a2c05df76d2dde9cb53d2b9ebfc6d1d410131107f61f4546fc6ad440ff`.

`frame` mutant at parent `74f4f521bbc29a39919f3760b099292b799256b6`: runner source/selected-fixture digest `648ff079010ad553b3dd4ce3fd1813a734f76d5fc70e9a119f7c471b8cf24777`; executable SHA-256 `8f1315bc11638ae4e2815be67ab6d3c9c43b1b0a7f2af22392fd88383376c273`.

`constants` mutant at parent `74f4f521bbc29a39919f3760b099292b799256b6`: runner source/selected-fixture digest `120de9f42a0f1d1ab24f3eca3629db013596e18f15aa8978cc238e0e85049cf1`; executable SHA-256 `79904dee5865bfb09a84fa182d6edc907c478cca972375f3f2ca1e224b726e0e`.

These digests include all Halo modules, the graph, host, runner and selected
fixtures. They identify uncommitted overlays too; the later smoke additions
are included in frame/constants builds. Every root was restored by copying
the saved original collector, then checking that its diff from the local
GC-fix milestone was empty, before creating the next mutant.
