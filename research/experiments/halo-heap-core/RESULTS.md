# Halo heap core results

2026-10-04. Base revision: `50d8b5eb6078124fcdb51cb7328bfb6ea31e33e0`.
The source digests below identify the tested worktree content. No compiler,
other Halo module, graph registration, specification or design-tree file was
changed. No network, compiler rebuild, push or PR was used.

**Status: module delivery is blocked by the supplied value interface.** The
required `whitefootc --graph lib/halo/modules.wfg --check-modules` exits 1:
`MOD-5 InaccessibleField` at the heap's `Value::Fun(h: h)` construction, with
additional inaccessible-field failures in string/table/value-view operations.
`Value.Num.n`, `Str.h`, `Tab.h`, `Fun.h`, `Builtin.id`, `NumView.Is.x` and
`HandleView.Handle.h` are private in `lib/halo/value/module.wfm`. MOD-5/6
requires public fields or public constructor/accessor functions. This task's
allowed-file boundary excludes that fix. The bound test program therefore
cannot compose yet. Neither diagnostic execution nor a local commit resolves
that blocker.

The explicitly selected diagnostic source-bundle mode checked and built the
actual heap bodies in the same module as the value definitions. Its native
assertion suite and full independent value model exited 0. This removes the
module access boundary solely for the diagnostic; **these are not results of
a successfully checked Halo module**. The run driver continues to return 1
for the module failure and the observed trace differences.

```
Module check exit: 1; seconds: 0.571
Build exit: 0; seconds: 1.790
Native exit: 0; seconds: 0.360
Lua exit: 0; seconds: 0.015
Trace rows: WF 2336; Lua 2336; mismatches 68
```

Both traces contain 2,336 rows: 2,048 LCG steps and 288 dedicated border
observations (six insertion orders, 32 insertions and 16 deletions each).
All 105775 visited key/value pairs passed the Whitefoot dense model's
value, duplicate and missing-key checks, including deletion of the current
cursor. All 200704 lookup observations (one assigned key and all
97 model keys per step) passed. The assertion suite passed, including 128
one-byte reintern checks and the equal-hash/full-byte-inequality witness.
All 288 dedicated insertion-order/hole `#` rows equal PUC. No lookup,
traversal count or traversal weighted-sum field differs from Lua.

## Border differences in the random hole workload

68 rows differ, all in the border column. They are retained below without
changing either oracle or test expectation. Reproducing PUC's internal node
placement is explicitly excluded by VM.md; these differences additionally
show why matching `luaH_getn`'s algorithm does not ensure matching PUC's
selected border for a table with holes when rehash timing differs.

The optional layout probe observes Lua's actual table sizes without changing
its operations. At step 199, Halo has array size 0, node size 64 and 64
occupied retained keys; Lua has array size 1, node size 64, 53 occupied keys
and 45 live hash values. At step 200, Halo's full retained-key node array
rehashes to array size 8, node size 64, with 42 occupied hash keys; Lua
remains array size 1, node size 64, with 54 occupied keys and 46 live hash
values. Halo's `#` is 8 and Lua's is 1. This directly confirms different
sizes at the first mismatch. `ltable.c:newkey` can reuse a nil-valued main
position; the requested Halo layout instead retains every key until rehash.
No lookup or traversal contents changed. All 41 observed layout rows are
available by the README commands; this is an attribution observation, not a
claim that arbitrary PUC hole borders match.

Trace columns: `tag step key value border count weighted-sum`; rows are
one-based positions in the complete output.

| Row | Halo | Lua |
| --- | --- | --- |
| 201 | `0 200 68 201 8 47 324106` | `0 200 68 201 1 47 324106` |
| 202 | `0 201 17 202 8 47 326231` | `0 201 17 202 1 47 326231` |
| 203 | `0 202 90 0 8 47 326231` | `0 202 90 0 1 47 326231` |
| 204 | `0 203 28 204 8 48 331943` | `0 203 28 204 1 48 331943` |
| 205 | `0 204 68 205 8 48 332215` | `0 204 68 205 1 48 332215` |
| 206 | `0 205 72 0 8 48 332215` | `0 205 72 0 1 48 332215` |
| 207 | `0 206 34 207 8 48 335003` | `0 206 34 207 1 48 335003` |
| 208 | `0 207 29 208 8 48 337033` | `0 207 29 208 1 48 337033` |
| 209 | `0 208 9 209 10 49 338914` | `0 208 9 209 1 49 338914` |
| 210 | `0 209 4 210 10 49 338982` | `0 209 4 210 1 49 338982` |
| 211 | `0 210 1 0 10 48 338798` | `0 210 1 0 0 48 338798` |
| 212 | `0 211 44 212 10 48 343506` | `0 211 44 212 0 48 343506` |
| 213 | `0 212 39 213 10 49 351813` | `0 212 39 213 0 49 351813` |
| 214 | `0 213 8 0 5 48 350333` | `0 213 8 0 0 48 350333` |
| 215 | `0 214 76 215 5 48 354133` | `0 214 76 215 0 48 354133` |
| 216 | `0 215 14 216 5 48 354553` | `0 215 14 216 0 48 354553` |
| 217 | `0 216 19 217 5 49 358676` | `0 216 19 217 0 49 358676` |
| 218 | `0 217 56 218 5 50 370884` | `0 217 56 218 0 50 370884` |
| 219 | `0 218 86 0 5 45 327212` | `0 218 86 0 0 45 327212` |
| 220 | `0 219 6 220 7 46 328532` | `0 219 6 220 0 46 328532` |
| 221 | `0 220 0 221 7 47 328532` | `0 220 0 221 0 47 328532` |
| 222 | `0 221 89 0 7 47 328532` | `0 221 89 0 0 47 328532` |
| 223 | `0 222 51 223 7 48 339905` | `0 222 51 223 0 48 339905` |
| 224 | `0 223 24 224 7 49 345281` | `0 223 24 224 0 49 345281` |
| 225 | `0 224 2 225 7 50 345731` | `0 224 2 225 0 50 345731` |
| 226 | `0 225 45 226 7 51 355901` | `0 225 45 226 0 51 355901` |
| 227 | `0 226 87 0 7 51 355901` | `0 226 87 0 79 51 355901` |
| 228 | `0 227 72 228 7 52 372317` | `0 227 72 228 79 52 372317` |
| 229 | `0 228 46 229 7 52 379125` | `0 228 46 229 79 52 379125` |
| 230 | `0 229 58 0 7 52 379125` | `0 229 58 0 79 52 379125` |
| 231 | `0 230 87 231 7 53 399222` | `0 230 87 231 79 53 399222` |
| 232 | `0 231 90 232 7 54 420102` | `0 231 90 232 79 54 420102` |
| 233 | `0 232 22 233 7 54 421950` | `0 232 22 233 79 54 421950` |
| 234 | `0 233 67 234 7 54 426439` | `0 233 67 234 79 54 426439` |
| 235 | `0 234 5 0 7 53 425439` | `0 234 5 0 79 53 425439` |
| 236 | `0 235 19 236 7 53 425800` | `0 235 19 236 79 53 425800` |
| 237 | `0 236 16 237 7 54 429592` | `0 236 16 237 79 54 429592` |
| 238 | `0 237 45 0 7 53 419422` | `0 237 45 0 79 53 419422` |
| 239 | `0 238 23 239 7 53 422435` | `0 238 23 239 79 53 422435` |
| 240 | `0 239 54 240 7 54 435395` | `0 239 54 240 79 54 435395` |
| 241 | `0 240 79 241 7 54 449457` | `0 240 79 241 79 54 449457` |
| 242 | `0 241 90 242 7 54 450357` | `0 241 90 242 79 54 450357` |
| 243 | `0 242 65 0 7 53 443142` | `0 242 65 0 64 53 443142` |
| 244 | `0 243 7 244 7 53 443639` | `0 243 7 244 64 53 443639` |
| 245 | `0 244 8 245 8 54 445599` | `0 244 8 245 64 54 445599` |
| 246 | `0 245 16 0 8 53 441807` | `0 245 16 0 64 53 441807` |
| 247 | `0 246 21 247 8 54 446994` | `0 246 21 247 64 54 446994` |
| 248 | `0 247 75 248 8 55 465594` | `0 247 75 248 64 55 465594` |
| 249 | `0 248 78 249 8 56 485016` | `0 248 78 249 64 56 485016` |
| 250 | `0 249 82 250 8 45 357979` | `0 249 82 250 64 45 357979` |
| 251 | `0 250 41 0 8 44 354945` | `0 250 41 0 64 44 354945` |
| 252 | `0 251 37 252 8 45 364269` | `0 251 37 252 64 45 364269` |
| 253 | `0 252 85 253 8 45 368859` | `0 252 85 253 64 45 368859` |
| 254 | `0 253 5 0 8 45 368859` | `0 253 5 0 64 45 368859` |
| 255 | `0 254 16 255 8 46 372939` | `0 254 16 255 64 46 372939` |
| 256 | `0 255 25 256 8 46 377139` | `0 255 25 256 64 46 377139` |
| 257 | `0 256 17 257 8 46 378074` | `0 256 17 257 64 46 378074` |
| 258 | `0 257 11 258 8 47 380912` | `0 257 11 258 64 47 380912` |
| 259 | `0 258 31 0 8 47 380912` | `0 258 31 0 64 47 380912` |
| 260 | `0 259 43 260 8 47 387749` | `0 259 43 260 64 47 387749` |
| 261 | `0 260 54 261 8 48 401843` | `0 260 54 261 64 48 401843` |
| 262 | `0 261 10 0 8 47 400623` | `0 261 10 0 64 47 400623` |
| 263 | `0 262 8 263 8 47 400767` | `0 262 8 263 64 47 400767` |
| 264 | `0 263 32 264 8 48 409215` | `0 263 32 264 64 48 409215` |
| 265 | `0 264 89 265 8 49 432800` | `0 264 89 265 64 49 432800` |
| 266 | `0 265 69 266 8 50 451154` | `0 265 69 266 64 50 451154` |
| 267 | `0 266 65 0 8 50 451154` | `0 266 65 0 64 50 451154` |
| 268 | `0 267 58 268 8 51 466698` | `0 267 58 268 64 51 466698` |

## Scope and remaining uncertainty

- Collector marking/sweeping and memory-limit enforcement are deferred as
  requested. Cells expose `live`, epoch `mark`, payload and `next_free`;
  slab indices and free heads are public for iteration. `free_string` removes
  the weak entry and repairs its probe cluster without rebuilding the table.
- VM.md section 2 contains no numeric per-kind accounting estimates in this
  revision. The implemented logical estimates are string 24 + byte capacity,
  table 48 + 16 × array capacity + 32 × node capacity, closure 24 + 4 ×
  upvalue capacity, and upvalue 32. Slab/intern reserve is excluded. This
  assumption was presented to the owner; its numeric values remain unconfirmed.
  `bytes_since_gc` records allocations and positive resize growth; freeing
  subtracts from `bytes` only.
- `read_only` spells the requested `readonly` flag because `readonly` is a
  language keyword. The flag is left to callers as requested. Error enum
  variants carry the specified error meanings in the public function docs.
- Silent parts follow `lstring.c`'s unsigned-int sampled-hash wrapping,
  `ltable.c`'s flags and rehash/size rules (MAXBITS 26), and `luaH_getn`'s
  MAX_INT (`INT_MAX - 2`) fallback. Comments use Whitefoot `doc`, since FORM-4
  has no source comments. No alternate collector, node chaining or sorted
  open-upvalue list was added.
- Raw allocations return `Option<u32>` at handle/accounting exhaustion;
  value constructors return Nil at that representational boundary. These
  explicit boundaries precede the later collector's allocation/limit policy.
- The comparison checks two deliberately wrong traces, a changed field and
  a missing row, and confirms equivalent space/tab tokenization. Both wrong
  traces are detected. No repository gate, target/platform qualification,
  collection verifier or large-capacity exhaustion run is claimed.

## Review

Separate read-only `gpt-6-sol` review against the base revision checked A,
D, C, R, M and V and design correspondence, followed by a narrow review of
its repair. No remaining finding was reported within that scope. The reviewer
read the heap sources, value interface, experiment source/driver, relevant VM
sections, checklist and design procedure, and ran `git diff --check`; it did
not rerun green suites. A raw public string allocator could create an equal
string outside canonical interning. That path is now module-private, while
public `alloc_string` takes bytes through `intern`; a public-path assertion
passed in the final diagnostic run. A readonly concern was withdrawn because
the task explicitly assigns that check to callers. The known private-interface
blocker, numeric-estimate assumption and hole-border differences remain.

The source-size sample passed in 1.57 seconds. `make static` was attempted and
stopped at its host-wide verification lock, whose recorded PID was 59413 for
an unrelated command. The sandbox refused `ps` inspection, so the lock was
left untouched; the full static group did not run. `git diff --check` passed.
No compiler Cargo command or full repository `make check` was run.

Found along the way: the canonical-allocation API gap was fixed and tested;
the existing private value payloads remain recorded here under the allowed
file boundary rather than edited in an excluded module. The border-layout
finding and missing numeric estimates are recorded above. No specification
rule changed, and no design-tree/log entry was written.

## Tested source identities

```
lib/halo/heap/module.wfm 0932045ed7f13862ea3adef4c721f2506fef9ff5969fa35045b2b8940712e220
lib/halo/heap/slabs.wf a47902df781b9359285c797cc3285761c49cdc072bdeebc1c2de5e3646c76318
lib/halo/heap/strings.wf 617abfee802da7a3e9f7cb20b0de1f4e6154c79598755948329ca0dd36a23484
lib/halo/heap/tables.wf 55475076e4017bc02217a97dea96897e4ac5f73d6b31b6aced2562b45ca9e8bf
research/experiments/halo-heap-core/test/module.wfm 32cfb0ec38068b052b0dac9786564b97eeb3dbdb4118555b3e043ec62383fbe8
research/experiments/halo-heap-core/test/trace.wf 8879dbc6b699bbc498d4d3bf065cbbc8f45b8a2269ea8be51683036a41061f5f
research/experiments/halo-heap-core/reference.lua f217da56336e3340839d6ddc7af28c683e365b0e67263bd5c7a77f00e2ae8490
research/experiments/halo-heap-core/run.py 5eff605e9baf0bec45d7b41c822cf030437ee05c607ca7c1d887435058964954
```

Oracle layout observer source:

```
research/experiments/halo-heap-core/layout_probe.c e4d2265459ea044193d9b8b96c1556da6b62419fc42104667012e8b3ce2daa33
```
