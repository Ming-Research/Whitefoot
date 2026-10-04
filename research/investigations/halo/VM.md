# Halo VM core design

Designed by a Fable consultation on 2026-10-04 from the investigation's
inputs ([DESIGN.md](DESIGN.md)), checked against the compiler with the
witness programs in [witnesses/vm/](witnesses/vm/); every verdict below was
re-run on the v0.89 compiler and on main's v0.90 compiler with the same
result. This document is the proposal the implementation follows; the
owner's open rulings are listed at the end.

## 0. The finding that shapes the dispatch loop

The interpreter cannot be the loop

```wf
loop (invariant fetch: pc < n, invariant frame: base + 256_u64 <= stack^.inner.len) {
  match code^.inner[pc] { ... }
}
```

The arms change different loop variables, and the body's paths join before
the backedge, where only facts identical on every input survive: an arm
that sets `base = t` under `t <= room` carries `t <= room` and `base == t`,
an arm that leaves `base` carries the assumed `base <= room`, and the join
keeps neither, so `witnesses/vm/ts.wf` and `s1_core.wf` are refused with
`INV-1 UndischargedLoopInvariant ... obligation: Backedge`. The minimal
witness has one invariant and two paths (`witnesses/vm/join.wf`; recorded
in docs/todo.md under checker precision). `t3.wf` passes when every path
sets both variables; `u7.wf` passes by re-checking `base <= room` at run
time before `set pc`, two comparisons per dispatch. (The consultation first
attributed this to the header batch as a whole; a loop with two invariants
whose body sets only one variable is accepted, which refutes that.)

The interpreter is therefore the guaranteed self-tail call [FN-10], whose
parameters are never set, so the entry requirements hold in every arm:

```wf
fn run<interface Host<E>>(code: &Box<Slots<Cell>>, consts: &Box<Slots<Value>>,
                          stack: &Box<Slots<Value>>, vm: &Vm, env: &E,
                          pc: u64, base: u64, kbase: u64) -> r: Outcome
  reads(code), reads(consts), writes(stack), writes(vm), writes(env) contract {
  requires pc < code^.inner.len;
  requires base + 256_u64 <= stack^.inner.len;
  requires kbase + 256_u64 <= consts^.inner.len;
}
// every arm ends:
//   return musttail run::<Host<E>>(code: code, ..., pc: next, base: base, kbase: kbase);
```

A changed argument is proved by its guard, an unchanged one by the
requirement. `witnesses/vm/s2_tail.wf` (558 lines: Move, LoadK, AddRR with
slow path, fused LtJmp, ForLoop with budget exit, table get and set, an
open or closed upvalue, Call and Return with frame records, a host call
through a generic interface) and `mt.wf` are accepted. The spelling must be
`run::<Host<E>>`: FN-6 refuses a bare `run(...)` (polymorphic recursion) and
FN-2 refuses `run::<Host>`. Whether the match-dispatch lowering recognizes
this spelling as it recognizes `loop { match }` is being confirmed with that
work; if not, the loop form with two run-time comparisons is the fallback,
and only the arm epilogues differ.

Checker facts the implementers need, each observed:

- no wildcard arm: every variant is listed, so type tests go through small
  view functions (`num_of(v) -> NumView { Is(x) | Not() }`);
- a call or construction is not an atom: bind it with `let` first;
- `match` on a place reached through a reference binds references to the
  payload fields (read them with `^`); on an owned local it binds copies;
- a fact about a field path (`frame.base`) does not enter arithmetic
  proofs: bind the field to a local first;
- a callee declared `writes(stack)` kills `stack^.inner.len` facts unless it
  `ensures stack^.inner.len == entry(stack)^.inner.len`, which an interface
  member may declare and the checker then uses;
- `writes(stack.inner.filled)` does not license `set stack^.inner[i]`;
  `writes(stack.inner)` or `writes(stack.inner[i])` does;
- `cvt::<f64, u64>` needs a `cvt.defined` guard, and `x + 256_u64` needs an
  overflow guard or a fact.

## 1. Values

```wf
enum Value {
  Nil(); False(); True();
  Num(n: f64);
  Str(h: u32);      // string slab index
  Tab(h: u32);      // table slab index
  Fun(h: u32);      // Lua closure slab index
  Builtin(id: u32); // library (< 4096) or host (>= 4096) function
}
```

A tag and an 8-byte payload, 16 bytes, PUC 5.1's `TValue` size. The Lua
stack is one `Box<Slots<Value>>`, Nil-filled, with the standing fact
`base + 256 <= stack.len`, kept at calls by growing the stack on the slow
path as Lua's `EXTRA_STACK` does; operands are `u8`, so register accesses
need no comparison. Slots are authoritative and nothing lives only in a
machine register across a dispatch in slice 1, which also makes the
collector's roots precise; an accumulator is a later loop-carried
parameter.

Rejected: NaN-boxing (every access would decode tags by hand with
`reinterpret`, outside `match` lowering and the collector's types; PUC 5.1,
the oracle, moves the same 16 bytes). Rejected: generational handles as in
E1 (a precise collector never leaves a dangling handle; the generation would
cost a load and compare on every table and string access). A `live` flag is
kept and checked, and a test-build verifier checks handles after every
collection.

## 2. Heap

Halo owns its slabs instead of `std::collections::slab`, whose payload is
reached through visit and edit members returning `Result` with a per-access
window check: acceptable for allocation (E1), not inside `GETTABLE`. Each
object kind is a `Box<Slots<Cell>>` with a free list; a cell is
`{ live: Bool; mark: u32; next_free: u32; payload }`.

- **Strings** `{ hash: u64; bytes: Box<Slots<u8>> }`, all interned as in
  Lua 5.1, so equality is handle equality. Lua 5.1's sampled hash; Halo's
  own open-addressing intern table of `u32` handles; interned strings are
  weak (sweep removes them).
- **Tables** `{ array; nodes; node_count; meta; readonly; flags }`, nodes
  `{ key: Value; val: Value }`, open addressing with linear probing. A
  removed key keeps its node with a Nil value until rehash, so `next` stays
  stable when a traversal assigns nil. Rehash sizes the array part by Lua
  5.1's `computesizes`. Integral numeric keys in range use the array part,
  `-0` is `0`, NaN and nil keys raise. `#` is Lua 5.1's `luaH_getn` exactly.
  `next` walks the array part, then the nodes. `readonly` is Redis 7's
  readonly table: setting one raises "Attempt to modify a readonly table".
  The hash part is revised to PUC's own (open ruling H1).
- **Closures** `{ proto: u32; upvals }`; prototypes belong to the compiled
  script and die with `SCRIPT FLUSH`.
- **Upvalues** `enum Upval { Open(slot: u64); Closed(v: Value) }`; the open
  ones are kept sorted by slot and closed by `CLOSE`, `RETURN` and error
  unwinding.

Collector: stop-the-world mark and sweep with an explicit gray stack and
epoch marks (E1). Roots: the stack below the current frame's
`base + maxstack`, frame records, open upvalues, globals, the registry,
every script's constant pool, and values the host pins. Collection happens
only inside allocation, never at a dispatch; an allocating helper takes the
values it must keep as arguments and returns the new handle, and a handler
never holds an unrooted handle across a second allocation. Trigger:
`bytes_since_gc > max(1 MiB, live_bytes_after_last_gc)`. Byte accounting
answers G5: past `memory_limit`, collect once, then raise "not enough
memory". Not incremental, no weak tables, no `__gc` in slice 1.

## 3. Calls

One value stack with overlapping frames and one frame-record stack:

```wf
struct Frame {
  func: u64; base: u64; return_pc: u64; kbase: u64;
  closure: u32; nresults: i64; activation: u32; flags: u8;  // PROTECTED, TAIL
}
```

`CALL` on a Lua function pushes a frame (vararg protos move their fixed
parameters up, Lua 5.1's layout) and grows the stack when
`new_base + 256 > stack.len`; on a builtin it runs the library function in
place with no frame. `RETURN` copies results to the popped frame's `func`,
adjusts to `nresults` and closes upvalues. `TAILCALL` replaces the frame.
`pcall` pushes a PROTECTED frame and continues the dispatch; `error` and
every raise unwind to the nearest PROTECTED frame of the current
activation, or return `Outcome::Error`. Lua-to-Lua calls, `pcall`, `error`
and host calls never use the machine stack. Library functions that call
back into Lua (`table.sort` with a comparator, `__tostring`, metamethods
reached from the slow path) re-enter `run` as PUC's `luaD_call` does,
bounded at 200 nestings ("C stack overflow"; open ruling H2).

## 4. Instructions

One cell is one enum variant, its operands as fields: a tag, up to three
`u8` and one `u32`, 8 bytes, each operand one load. Lua's `RK` operands
become separate variants (`AddRR`, `AddRK`, `AddKR`, `AddKK`, and likewise
for the other arithmetic and comparison families), about 80 variants.
Compare and test instructions are fused with their `JMP`
(`LtJmpRK(a, b, c, target)`, `TestJmp`), as Lua 5.1 always emits them in
pairs; `FORLOOP` and `TFORLOOP` already are. No other fusion until a
dynamic census of the corpus. Constants are `kbase + k` with `k: u8`, the
script's pool padded with 256 Nils so every prototype's `kbase` satisfies
the requirement; a prototype's 257th and later constants load through
`LoadKx` with a comparison. Branch targets are absolute cell indexes, one
comparison per taken jump or fall-through, the measured 2.3%.

A verifier still runs once per compiled script for Lua-level correctness
(jump targets inside the prototype, registers below `maxstack`, a final
`Return`); its facts do not reach the dispatch loop (G4).

## 5. Dispatch

Eight parameters, the arm64 argument-register budget (E0: one more costs
8%). The budget counter lives in `vm`, decremented only at `ForLoop`,
`TForLoop`, a backward `Jmp`, `Call` and `TailCall`; at zero the handler
saves `pc`, `base` and `kbase` in `vm` and returns `Outcome::Budget`.
Each handler implements the common case (both operands numbers; a table
with an in-range integer key or a present string key and no metatable
involved) and otherwise calls the one shared slow executor, which performs
coercion, metamethods, hash misses, `__index` chains and readonly refusal
and writes the destination slot itself; it returns `Err` after raising
into `vm.error`. Outcomes: `Done(count)`, `Error`, `Budget`, `HostStopped`.

## 6. Embedding

```wf
interface Host<E> {
  fn call(env: &E, vm: &Vm, stack: &Box<Slots<Value>>, func: u64, argc: u64, id: u32)
    -> r: HostOutcome writes(env), writes(vm), writes(stack) contract {
      ensures stack^.inner.len == entry(stack)^.inner.len;
    };
}
enum HostOutcome { Returned(count: u64); Raised(); Stop(); }
```

A host function's arguments are at `stack[func+1 .. func+1+argc)` and its
results go from `stack[func]`. `Stop` ends the run with `HostStopped` and
the stack intact. Halo exports to the host: `intern`, `string_of` (an index
and length the host turns into a slice of the string's public readonly
bytes, since no function returns a reference), `number_of`, `truthy`,
`kind`, `new_table`, `table_append`, `table_set`, `table_get`,
`table_border`, `table_next`, `set_global` (bypassing readonly, as Redis
sets `KEYS` and `ARGV` from C), `pin`/`unpin`, `error_value` and
`format_error` producing Redis's `@user_script:LINE: msg`. Script cache:
`compile` returns a `ScriptId` or a compile error with PUC's text;
`forget_all` is `SCRIPT FLUSH`. Running: `start`, `resume`, `reset`. The
slice protocol of the selected direction (C) is the host's: on `Budget`
before its first write it resets, leaves the atomic statement, checks for a
kill and restarts with a doubled budget; an undeclared key before the first
write is a `Stop` from its own `call`. The engine knows nothing of keys or
writes. Globals protection is installed by the host through the ordinary
library, as Redis does from C.

## 7. Compiler

Lua 5.1's own architecture: a lexer, then parsing and code generation in
one pass (`llex`, `lparser`, `lcode`), the expression-descriptor machine,
`freereg` register discipline and jump lists. Output: one `Script` whose
prototypes' code and constants are appended to script-wide arrays, jump
targets absolute, the pool padded. Constant folding exactly as `lcode.c`'s
`constfolding`. Number lexing with PUC's `luaO_str2d` semantics; PUC's exact
error texts, since firn relays compile errors. Rejected: an abstract syntax
tree, which would make register allocation and jump patching a second
design and loosen parity with PUC's emission.

## 8. Packages and slice 1

```
pkg::value:   []                                             // Value, Cell, Proto, views
pkg::number:  [pkg::value]                                   // %.14g, str2number, fmod, pow
pkg::heap:    [pkg::value]                                   // slabs, strings, tables, closures, GC
pkg::lex:     [pkg::value, pkg::number]
pkg::compile: [pkg::value, pkg::number, pkg::lex, pkg::heap] // parser, codegen, verifier
pkg::vm:      [pkg::value, pkg::heap, pkg::number]           // run, slow executor, calls, unwind
pkg::lib:     [pkg::value, pkg::heap, pkg::vm, pkg::number]  // base, string, table, math
pkg::embed:   [pkg::value, pkg::heap, pkg::vm, pkg::lib, pkg::compile]
```

Slice 1 includes every Lua 5.1 opcode; numbers, strings, tables, closures,
varargs, multiple returns and tail calls; metatables (`__index`,
`__newindex`, `__call`, arithmetic, `__concat`, `__eq`, `__lt`, `__le`);
`pcall` and `error` with values; readonly tables; the base library; the
non-pattern string functions including `string.format`; the table library;
the math library with Redis's deterministic random; the collector, the byte
limit, the budget and the embedding interface. Excluded and recorded: Lua
patterns, `cjson` and `cmsgpack` (their own packages), `bit`, `struct`,
coroutines, `loadstring`, `__gc`, weak tables, `pairs` order parity.

## 9. Work order

L marks design-critical work kept with the lead; R marks routine work for a
coding agent.

1. L values and cells (`pkg::value`).
2. R number library: `%.14g`, `str2number`, `fmod`, `pow` (musl lineage),
   checked against PUC's output on 10,000 doubles.
3. L heap core: slabs, intern table, tables with `#` and `next`, upvalues,
   closures, byte accounting; property tests against a model.
4. L collector with its test-build verifier; a seeded "forgot a root" fault
   must make the verifier fail.
5. R lexer: PUC 5.1 tokens and error texts; token dumps equal PUC's.
6. L code generation skeleton (expression descriptors, registers, jumps,
   fusion, pools, verifier), then R the remaining statement and expression
   forms; cell listings equal `luac -l` modulo fusion.
7. L dispatch core (signature, ten representative handlers, slow executor,
   frames, unwinding, budget), then R the remaining handlers from that
   pattern; a budget of 1 suspends every loop and resumes to the same
   result.
8. R library functions of slice 1.
9. L embedding (engine, host interface, API, slice protocol), with a test
   host over an in-memory map running the oracle corpus.
10. R firn binding (host over `KeyedEntries`, reply conversion, `EVAL`,
    `EVALSHA`, `SCRIPT`); the lead reviews the atomic statements.
11. L measurement harness before any tuning.

## 10. Falsifiers, stated before measuring

- F1: the Lua 5.1 test files `constructs calls closure nextvar vararg
  strings math sort errors`, with I/O, coroutine, load and pattern sections
  removed by a recorded script, pass completely.
- F2: the [oracle corpus](../../experiments/halo-oracle/) replies equal
  Redis 7.0.15's, error texts included, except outputs depending on `pairs`
  order over non-sequence keys, which the harness sorts.
- F3: every corpus script under budgets 1, 7 and 1000 gives the unbudgeted
  reply; a `Stop` before the first write leaves the store unchanged.
- F4: a script allocating without bound under a 64 MiB limit fails with
  "not enough memory" and the host survives; the collector verifier finds
  nothing with collection forced at every allocation.
- P1 (with the current `match` lowering): Halo's median is at most PUC
  5.1.5's on `fib(30)`, a 1e8 numeric loop, 1e7 table integer fill and read,
  1e6 string-key reads, 1e6 short concatenations, `table.sort` of 1e6
  numbers and binary-trees depth 16; above 1.5 times on `fib` or the loop,
  the value width and handle checks are attributed first.
- P2 (after the per-arm lowering): at most 0.6 times PUC on the numeric
  kernels and 0.8 times on the table and string kernels.
- P3: the budget costs under 1% on the loop kernel.

## Open rulings

- H4. The platform reference is Redis 7.0.15 on x86-64 Linux with glibc,
  firn's own reference (it already prints a negative NaN as `-nan`, as
  glibc does). The number library formats NaN with its sign accordingly;
  its parser and the oracle corpus were checked against macOS builds and
  are rechecked on the Linux runner before release (docs/todo.md).

- H1 (revised during implementation). The first heap, open addressing as
  section 2 says, gave a different `#` from Redis's Lua on 68 of 2,336
  traced tables with holes: PUC reuses a nil-valued main position and
  rehashes only when its `lastfree` pointer runs out, so array sizes differ
  over time and `luaH_getn` picks another border. Recommended and being
  implemented: the hash part ports `ltable.c` exactly (chained scatter
  table, Brent's variation, `lastfree`, PUC's number and string hashes), so
  `#` and `pairs` order equal Redis's for number, string and boolean keys;
  only tables and functions used as keys traverse in another order, since
  PUC hashes them by address.
- H2. Library callbacks re-enter `run` up to 200 nestings, as PUC does.
  Recommended: accept; measure the stack use once the lowering exists.
- H3. A loop invariant is lost where a guarded update joins an untouched
  path (`witnesses/vm/join.wf`): the join keeps only identical facts. The
  program is sound, so this is a precision gap, recorded in docs/todo.md.
  Slice 1 does not depend on it.
