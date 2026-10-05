# Halo F4 safepoint stress validation

Safepoint stress preserves all 80 stored Redis replies at budgets 1, 7 and
1000 (240/240 with stress on and 240/240 off). Each stressed script collects;
each budget completes 20,450 collections. The 64 MiB logical heap limit
returns a Lua memory error and a following script succeeds on the same
engine/store, in both modes at all budgets.

F4 found and fixed dynamic call arguments excluded from the live-stack
bound, consumed callback snapshots retained after restoration/reset, and
missing logical heap-limit enforcement. All four missing-root mutations
are detected; three need explicit local witnesses beyond the ordinary corpus.
Every-allocation stress and a complete reachability verifier remain untested,
as do process RSS, peak temporary allocation and platform parity beyond
Darwin arm64. This establishes F4's safepoint and recovery observations,
not its original every-allocation verifier goal.

This record serves VM.md F4 and is retained with the reproducible collector
experiment; remove or supersede it when maintained collector tests own these
observations or Halo is retired.

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

The stopped-stack mutation removes only the loop marking `vm.stopped_stack`.
Its native build exited 0 in 5.80 s (user 5.53 s, system 0.27 s), using
the existing module cache. Three `counter-closure`, budget-7 stress samples
matched with 8 collections in 0.004917, 0.005512 and 0.005808 s.
The isolated suspended witness at budget 1 returned
`ERR user_script:5: attempt to index a function value` instead of `zzz`,
after 5 collections (runner exit 1, native exit 0, 0.304898 s).
Without the extra suspended checkpoint, the unchanged ordinary corpus
passed 240/240 runs at budgets 1, 7 and 1000 (runner exit 0, wall 2.39 s).
Thus the ordinary corpus also misses this root category. The collector was
restored byte-for-byte and `git diff --exit-code -- lib/halo/vm/collect.wf`
exited 0 before the final build.

| Removed root | Ordinary stress corpus | Discriminating witness | Detected |
| --- | --- | --- | --- |
| Open upvalues | 240/240 pass | `open-upvalues`, budgets 1/7/1000: `wrong`, expected `kept` | Yes |
| Frame closures | 240/240 pass | `frame-closure`, budget 1, `--isolate-frames`: invalid upvalue, expected `qqq` | Yes |
| Constants | 90/240 pass | 50 corpus failures per budget | Yes |
| Stopped stack | 240/240 pass | `suspended-stack`, budget 1, `--collect-suspended`: function-index error, expected `zzz` | Yes |

Stopped mutant parent: `9e7554bd1cec7354770c3f887a0447f44a537cfe`;
selected-witness runner/source digest:
`48583609fb048d599bcf506d0209e1dd8e516d06f2bcc00024c51586c820fb1e`;
executable SHA-256:
`d45e7b68c21b50141a8283bb32f42e34372f542948b4c53bad56788e94b6b681`.

## Final committed-collector validation

The native inputs are those at `d02e723a80e8e2188ec32cd6464f9281767da77c`;
subsequent edits only change documentation and the design record. The final
native build exited 0 in 413.40 s (user 323.75 s, system 44.32 s), with
module caching enabled. The smaller heap-graph build exited 0 in 0.52 s
(user 0.47 s, system 0.07 s). Build time is excluded from all execution rows.

Final executable SHA-256:
`3b0ed569e0577ee03400d4d54839f4a8e63f4a46cf2f6b1925a08d44906196fa`.
Corpus runner/source/fixture digest:
`0097333aa4ca8d225a8517196b806884246ca9e17ee42ca6a178107bce057907`.
Local three-case digest:
`4189654c025fd2557ede227f57773526f0dec9e889e134a0f4cfa9aaf1e26eb9`.
Collector SHA-256 (identical to `74f4f521b`):
`d343996ccc9a712fbe1080cc8e68e3762c299cebcd1466c2593e66ead63c8709`.

Before scaling execution, three budget-7 counter-closure samples took
0.404867, 0.003751 and 0.003956 s with stress, and 0.003638, 0.003713
and 0.003956 s without. The first launch was slower; both subsequent
stress samples and all three off samples justify this seconds-scale batch.
Stress samples collected 8 times; off samples collected 0. Budget-7
memory/recovery samples passed in 0.372260 s on and 0.372850 s off.
Three fixed stress SHA-1 samples matched hashlib in 0.004604, 0.004568
and 0.004291 s before the larger vector check.

| Suite | Stress | Budget | Passed | Collections | Execution seconds |
| --- | --- | ---: | ---: | ---: | ---: |
| 80-script oracle | On | 1 | 80/80 | 20450 | 1.393637 |
| 80-script oracle | On | 7 | 80/80 | 20450 | 0.473711 |
| 80-script oracle | On | 1000 | 80/80 | 20450 | 0.323578 |
| 80-script oracle | Off | 1 | 80/80 | 0 | 1.337636 |
| 80-script oracle | Off | 7 | 80/80 | 0 | 0.434205 |
| 80-script oracle | Off | 1000 | 80/80 | 0 | 0.282046 |
| Three local root cases | On | 1 | 3/3 | 25 | 0.012305 |
| Three local root cases | On | 7 | 3/3 | 25 | 0.010738 |
| Three local root cases | On | 1000 | 3/3 | 25 | 0.010633 |
| Three local root cases | Off | 1 | 3/3 | 0 | 0.012342 |
| Three local root cases | Off | 7 | 3/3 | 0 | 0.010797 |
| Three local root cases | Off | 1000 | 3/3 | 0 | 0.010833 |
| Isolated frame closure | On | 1 | 1/1 | 9 | 0.004529 |
| Parked suspended stack | On | 1 | 1/1 | 14 | 0.005065 |

Collections and times above exclude preparation chunks. The isolated frame
row enables `--isolate-frames`; the parked-stack row enables
`--collect-suspended`. Both independently specified replies are restored.
Ordinary local-case rows leave these extra conditions off.

| Memory limit + same-engine/store recovery | Budget | Passed | Collections | Recovered heap bytes | Execution seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| Stress on | 1 | 2/2 replies | 131 | 9846 | 0.376495 |
| Stress on | 7 | 2/2 replies | 131 | 9846 | 0.370845 |
| Stress on | 1000 | 2/2 replies | 131 | 9846 | 0.369553 |
| Stress off | 1 | 2/2 replies | 8 | 9846 | 0.373569 |
| Stress off | 7 | 2/2 replies | 8 | 9846 | 0.375094 |
| Stress off | 1000 | 2/2 replies | 8 | 9846 | 0.370405 |

Each memory run reports `not enough memory` for the unbounded allocator,
then bulk `alive` from the pre-exhaustion store key. Both replies share one
engine/store. Collection counts include both scripts and exclude preparation.
The standalone heap graph passed (exit 0, native wall 0.30 s), checking
rooted/unrooted counts and all three table-reference edge kinds. The
embedding smoke passed (exit 0; native wall rounded to 0.00 s), including
nested host Stop, reset, following execution, pins/cached constants and
collection-count preservation. These are correctness timings.

In each stress mode the additional 1,003 SHA-1 vectors matched Python
hashlib, and all 32 Redis-source-grounded error probes passed. Vector
execution totals were 3.577 s on and 3.558 s off. Entire runner wall times,
including memory, vectors, error probes and corpus transport/comparison,
were 7.61 s on and 7.42 s off; they are not build times or speed comparisons.

## Commands and exit codes

All commands ran from this worktree with the existing gate compiler; no
Cargo, network, push or PR was used. Heavy commands used
`perl .github/run-check.pl LABEL COMMAND ...`; reported native times come
from its separate `time -p` output. Shared-lock refusals (exit 75) were
retried only after the other command released its lock.

For these invocations, the common runner arguments are:

```sh
python3 -B research/experiments/halo-e2e/run.py \
  --compiler compiler/target/gate/whitefootc \
  --binary /private/tmp/halo-f4-finish/final
```

Each runner invocation also used `--report` and `--actual` scratch outputs.
`--cases research/experiments/halo-gc/cases` is abbreviated below as local
cases. The stopped mutation used the separately built `stopped-mutant`
executable, never the final executable.

| Command or runner arguments | Exit | Observation |
| --- | ---: | --- |
| `compiler/target/gate/whitefootc --graph research/experiments/halo-e2e/modules.wfg --check-interface halo::heap --cache /private/tmp/halo-gc-cache` | 0 | Small interface sample, 0.02 s |
| `compiler/target/gate/whitefootc --graph research/experiments/halo-e2e/modules.wfg --entry test --cache /private/tmp/halo-gc-cache -o /private/tmp/halo-f4-finish/stopped-mutant` | 0 | Stopped-root mutant build |
| Mutant: stress + local `gc/suspended-stack`, budget 1, `--collect-suspended` | 1 | Expected discrimination; native exit 0, wrong error reply |
| Mutant: `--gc-stress --budgets 1,7,1000`, ordinary oracle | 0 | 240/240, missing root concealed |
| `compiler/target/gate/whitefootc --graph research/experiments/halo-e2e/modules.wfg --entry test --cache /private/tmp/halo-gc-cache -o /private/tmp/halo-f4-finish/final` | 0 | Restored collector build |
| `--gc-stress --budgets 1,7,1000 --verify-memory --verify-errors --verify-sha1` | 0 | Final stressed oracle and auxiliary checks |
| `--budgets 1,7,1000 --verify-memory --verify-errors --verify-sha1` | 0 | Final ordinary oracle and auxiliary checks |
| Local cases, `--gc-stress --budgets 1,7,1000` | 0 | 9/9 |
| Local cases, `--budgets 1,7,1000` | 0 | 9/9 |
| Local cases, `--filter gc/frame-closure --gc-stress --budgets 1 --isolate-frames` | 0 | Sole frame root restored |
| Local cases, `--filter gc/suspended-stack --gc-stress --budgets 1 --collect-suspended` | 0 | Sole snapshot root restored |
| `/private/tmp/halo-f4-finish/final one two three` | 0 | Embedding smoke |
| `compiler/target/gate/whitefootc --graph research/experiments/halo-gc/modules.wfg --function pkg::test::main --cache /private/tmp/halo-gc-cache -o /private/tmp/halo-f4-finish/heap` | 0 | Heap-graph build |
| `/private/tmp/halo-f4-finish/heap` | 0 | Heap-graph execution |
| `git diff --exit-code -- lib/halo/vm/collect.wf` | 0 | Collector restored exactly |
| `make design-lint` | 0 | Small structural sample, 7.40 s before `make static` |
| `make static` | 0 | All static stages, 32.39 s |
| `make static` at `f90aeae389d849ddab78e6877d63f52b8463f959` | 0 | Provisional node included, all static stages, 32.57 s |
| `git diff --check` | 0 | Patch whitespace |

The earlier open/frame/constants evidence above was recovered from the
previous session's saved reports and replies, with their counts and digests
checked here; those mutations were not rerun in this session.

## Instrumentation boundary assessment

The completion review identified a material interface choice in the earlier
stress commit: `set_gc_stress` and `collection_count` are public embedding
operations, while the public Heap carries their storage. The current
recommendation (Q1, awaiting owner ruling) keeps those embedding operations.
They state the engine's default-off behavior and the meaning of completed,
saturating, reset-preserved collection telemetry at the client's boundary.
The viable alternative is to let research clients write/read Heap fields
directly; that ties each client to collector storage and makes it interpret
lifecycle details itself. Keeping the API adds an embedding contract that
must be maintained; direct access avoids that additional contract.

This is a technical boundary judgment, not a measured advantage of one API
placement. F4 validates the current contract's observations but does not
select API placement empirically. Reopen at the production firn binding or
a collector-storage change. The provisional tree node lives in the existing
compiler tree beside the Halo closure decision; it serves F4's repeatable
embedding checks and is retired or superseded with the controls when Halo
is retired or its production boundary replaces them. No approval log entry
is written before the owner's ruling.

## Findings and remaining scope

The live-stack bound, snapshot retention and memory-limit TODO items are
resolved by the corrected code and these observations. The stale coercion
TODO wording was corrected, as were VM.md's fixed-frame-only stack bound,
verifier status and overly broad byte-limit description. The experiment
README now links the final validation rather than promising it below. The outdated embedding boundary narration is
recorded in `docs/todo.md` for its next update; its technical witnesses were
not broadened into compatibility claims. The every-allocation verifier gap
is also recorded there with an explicit reopening condition. CJSON instance
configuration reclamation remains a separate existing TODO.

No specification, conformance expectation or oracle reply was changed by
this finishing session. Its final collector source is byte-for-byte the
committed `74f4f521b` version. A complete repository `make check`
was not run: this task uses the supplied compiler and forbids Cargo. The
experiment establishes these observations on Darwin arm64 only.
