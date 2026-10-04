# Halo Lua 5.1 patterns

Implemented `string.find`, `string.match`, `string.gmatch` and `string.gsub`.
The authored differential corpus has **1,156 cases** with **zero mismatches**
against the supplied Redis 7.0.15 Lua 5.1.5 executable at budgets 7 and 1000.
The existing string oracle corpus passes **10/10** at budget 7.

## Implementation and reference boundaries

The port follows `lstrlib.c`'s matcher, greedy/minimal expansion, capture rollback,
balanced patterns, frontiers, C-locale classes and sets. Pointer positions become
source offsets. The `goto init` tail paths use a loop; genuine non-tail `match`
calls use **bounded machine recursion, at most 200 active match frames**, raising
`pattern too complex` when another call would exceed the bound. This preserves
PUC's control flow without an unbounded native stack or a separate backtracking
interpreter.

The bundled source itself has **no matcher-depth counter**. Its
`LUAI_MAXCCALLS=200` limits nested VM C calls. The requested matcher-depth bound
is therefore an intentional addition, tested separately rather than counted as
reference parity. The seven reference depth probes all succeed; Halo deliberately
raises on the three 200-item non-tail probes. A 1,000-item literal tail path
still succeeds. `LUA_MAXCAPTURES` is **32**: nested captures and position captures
succeed at 32 and raise `too many captures` at 33, as the reference does.

The API preserves Lua 5.1's clamped `init`, automatic plain-search shortcut in
`find` (including its C-string scan of the pattern), binary source strings,
pattern termination at NUL, position captures, lazy malformed-pattern errors,
literal leading `^` in `gmatch`, and empty-match advancement in both iterators
and substitution. `gsub` supports strings/numbers, tables (including `__index`),
functions, capture arguments up to 32, replacement percent escapes, `max-n`,
false/nil preservation, error propagation and the substitution count.

Public string IDs are 41 (`find`), 42 (`match`), 43 (`gmatch`), and 44 (`gsub`).
ID 45 is private iterator dispatch; the existing builtin table installs only the
four public names. The iterator uses a traced closure with three closed upvalues
(source, pattern, cursor) and a reserved prototype sentinel, so it has Lua's
function type and independent mutable state, with its strings retained by the
existing collector. A callable table would expose the wrong Lua type.

Matcher/class helpers do not allocate or invoke Lua. Dynamic substitution uses
the existing nested activation and callback continuation path, because replacement
functions and table metamethods must retain progress across a budget suspension.
VM-owned substitution frames retain source, pattern, replacement, cursor, count
and accumulated bytes; collector marking, error cleanup and reset include them.
The shared callback helper now accepts an argument slice; existing three-argument
callers retain their wrapper. No new Value variant or heap slab is introduced.

The new implementation files belong to `lib/halo/vm/`; the cases and independent
Python comparator belong here and consume the existing end-to-end test host.
Python transports bytes and compares outputs; it does not implement a Lua matcher.
These artifacts are maintained with the pattern capability and retired with it.
The explicit task scope permits VM files and this experiment directory only:
no specification, design-tree, main, remote branch or PR is changed.

## Reproduction

Build the existing native host once with the supplied compiler, then reuse that
binary (placeholder arguments below name local executables and scratch outputs):

```sh
<compiler> --graph research/experiments/halo-e2e/modules.wfg --entry test -o <binary>
python3 -B research/experiments/halo-patterns/run.py --compiler <compiler> --lua <lua> --binary <binary> --report <scratch-report> --budgets 7,1000
python3 -B research/experiments/halo-e2e/run.py --compiler <compiler> --binary <binary> --budgets 7 --filter lua-core/string- --report <scratch-report>
```

`run.py --limit 20 --budgets 7` is the calibration sample. The comparator's
sensitivity checks reject changed values, missing or extra rows and reordered
rows; the native reply must be an array of byte strings. Both engines load the
same authored source under the `user_script` chunk name. Only the leading source
location is stripped from an error; the complete error payload is compared.

## Measured validation

The implementation revision is `fbd2ea7a91e7e81b1a007d256d2e8e4d8c7eda0e`. The final authored cases include
later test-only additions, identified by their byte digest below. All commands
returned exit 0; compiler and program execution were timed separately.

| Observation | Count | Mismatches/failures | Seconds |
| --- | ---: | ---: | ---: |
| VM module check, direct compiler | 1 | 0 | 279.71 |
| Native host build, direct compiler with scratch cache | 1 | 0 | 402.02 |
| Calibration, budget 7 | 22 | 0 | 0.749 |
| Full authored corpus, budget 7 | 1,156 | 0 | 9.844 |
| Full authored corpus, budget 1000 | 1,156 | 0 | 1.383 |
| Depth contract, budget 7 | 7 | 0 | 0.006 |
| Depth contract, budget 1000 | 7 | 0 | 0.005 |
| Stored string oracle scripts, budget 7 | 10 | 0 | see below |
| Stored sort and tostring callback regression scripts, budget 7 | 2 | 0 | see below |

The reference ran the full authored corpus in 0.100 seconds. These elapsed times
size the validation runs; they are not a paired performance experiment or a
claim about matcher throughput. The implementation uses the existing compiler
without Cargo or `run-check.pl`. Repository `make static
DESIGN_REVIEW_BASE=7d6e73ea5` also passed all seven stages; its existing stage
wrapper was used only for those static checks. `git diff --check` passed.

The authored operation rows are 236 find, 464 match,
165 gmatch and 289 gsub, plus two iterator-state probes.
Coverage includes every positive and negative class across all 256 byte values,
sets and ranges, every successful back-reference `%1` through `%9`, anchors,
frontiers at source edges and NULs, balanced delimiters, greedy/minimal/optional
backtracking with captures, plain/init boundaries, error texts, 31/32/33 capture
boundaries, replacement percent escapes, max-n coercion, tables/metamethods,
representative callback argument counts through the 32-capture boundary, nested
substitution, callback error values,
budget suspension, and collection of coerced sources and captured iterator strings.

| Existing oracle script | Budget | Result | Seconds | Failure reason |
| --- | ---: | --- | ---: | --- |
| lua-core/string-byte-char | 7 | PASS | 0.0052 |  |
| lua-core/string-escapes | 7 | PASS | 0.0048 |  |
| lua-core/string-find | 7 | PASS | 0.0044 |  |
| lua-core/string-format | 7 | PASS | 0.0041 |  |
| lua-core/string-gmatch | 7 | PASS | 0.0035 |  |
| lua-core/string-gsub-dynamic | 7 | PASS | 0.0035 |  |
| lua-core/string-gsub-string | 7 | PASS | 0.0034 |  |
| lua-core/string-length-order | 7 | PASS | 0.0037 |  |
| lua-core/string-match | 7 | PASS | 0.0035 |  |
| lua-core/string-transforms | 7 | PASS | 0.0035 |  |
| lua-core/table-sort | 7 | PASS | 0.0043 |  |
| lua-core/meta-concat-tostring | 7 | PASS | 0.0045 |  |

Identity digests (SHA-256):

- Authored cases: `ffca5ac4240368afc83ec642ffe133de86f6d24754303f5f49725ebf257cd9cb`.
- Depth cases: `067025406ea77e6d2167dceeb50007644959e229f69acea45dfe50f293a1508b`.
- End-to-end source/graph/host/runner digest: `4c25c81a63ecd143049c9e429f428f0985aeb2dcb23707443865c1c60f619269`.
- Native executable: `424b85d4b8bfe2d24cccee92044e8cdb77dbdfed5df7babb5fa8e420fcb4fab2`.
- Compiler: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.
- Reference Lua executable: `dd2f2bb469b8c423292e23a5ab0dea2c4f3ea23658b76d55cd5f296d5d94e91e`.
- Bundled `lstrlib.c`: `e8048c79ae34be0fd29aa8db1a341cd600f5e09b9133b5810910e07b855a32d5`.

## Review and remaining scope

Independent read-only review used the configured `gpt-6.1-sol` reviewer over
`7d6e73ea5ab7ae2996792d695b28d7df8f96dbc0..1c6e33ec0543714a2e6e30e2965410712e9574a2`, with checklist groups A, D, C, R, M and V. It inspected matcher
parity against the supplied C source, API/iterator dispatch, callback suspension
and resumption, collector tracing, reset/error cleanup, scope and artifact
identities, and design correspondence against the relevant existing guidance
and the early implementation rationale. T was inapplicable because no formal
specification, conformance or gate files changed; publication/readiness rules
were superseded by the explicit local-only task instruction. Green suites were
not rerun by the reviewer. No code or runtime correctness finding was reported
within that scope.

The one D3/V2 wording finding overstated callback-arity coverage as "all";
inspection of the authored function replacement cases confirmed representative
arities rather than every integer arity. The claim is corrected above. This is
a prose-only repair, rechecked locally without changing tested VM or case bytes.
The bundled source's absent matcher-depth counter and its exact 32-capture
boundary were identified during the port and are recorded above; no unrelated
defect was found.

No known differential mismatch remains. The complete repository `make check` was not run: this task
explicitly prohibits Cargo and limits work to the Halo VM and pattern experiment.
No network, push, pull request or merge is performed. No universal behavior or
performance claim follows from this finite corpus; the matcher-depth difference
from the bundled reference is explicit above.
