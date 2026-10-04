# Halo: a Lua engine in Whitefoot

## Purpose

Firn's deployment milestone needs Redis scripting (`EVAL`, `EVALSHA`), and
the owner selected a Lua interpreter written in Whitefoot rather than an
embedded native engine ([design/language/firn](../../../design/language/firn.md)).
Halo is that engine: a general Lua 5.1 engine with an embedding interface,
which firn hosts. The owner's rulings of 2026-10-04:

- Halo lives in `lib/halo`; the codecs Redis scripts use live in `lib/json`
  and `lib/msgpack` as general libraries, and each is its own package that
  firn binds ([MOD-11]). Halo's `cjson`/`cmsgpack` bindings, `bit` and
  `struct` sit in Halo.
- The language baseline is Lua 5.1, which Redis scripts are written against.
- Halo's own source never names Redis; firn supplies `redis.call`, `KEYS`,
  `ARGV`, the reply conversions and the script cache.

This document states what the engine needs from the language before any
mechanism is chosen. Every gap below has a minimal witness program in
[witnesses/](witnesses/), checked with the compiler at `db07796a9`
(spec v0.89; its rules used here equal v0.90's).

## What a Redis script needs

```lua
-- a rate limiter, typical of application scripts
local n = redis.call('INCR', KEYS[1])
if n == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
return n
```

- The whole script is atomic: no other command observes a partial effect.
- A script may touch keys it did not declare in `KEYS` outside cluster mode;
  Redis documents declaring them but does not enforce it.
- An error ends the script; writes already made stay (no rollback).
- After `busy-reply-threshold` (5 s by default) other clients receive
  `BUSY`, and `SCRIPT KILL` ends the script only if it has not written yet;
  a script that has written can only be stopped by `SHUTDOWN NOSAVE`.

## Interpreter inputs

The owner's Silverfir-nano wasm interpreter records what made its dispatch
fast; the parts that transfer to a register-based Lua VM:

- Dispatch count dominates: on Apple M4 one dispatch costs about 2 cycles,
  and removing one is worth 10–50 times more than removing an instruction
  from every handler. Operands carry either a frame slot or a constant, and
  compare-and-branch is fused (Lua's `EQ`/`LT`/`LE` followed by `JMP`).
- Fixed-size instruction cells, each operand readable with one load; branch
  targets are cell indices.
- An accumulator register carrying producer-to-consumer values was worth
  +29%, one pinned local +16%. Both require the interpreter state to stay
  in machine registers from handler to handler, which is what the separate
  match-dispatch work (another session) is to deliver.
- Handlers implement the common case and leave for one shared slow executor
  on anything else. In Lua the slow path usually succeeds (a metamethod),
  so it writes the destination slot itself.
- Frame slots are the authoritative values and registers only cache them,
  which also gives a precise collector its roots.
- A budget decremented only at loop back-edges and calls, exiting to the
  slow path at zero, is how an interrupt check stays cheap.

## Capability gaps

Each gap is stated by the smallest program a sound language could accept,
the rule that refuses it today, and why Halo or firn needs it.

### G1. A key chosen during the atomic statement

```wf
atomic s = &h {
  let k = script_computes_a_key(s^.meta);
  // read or write the entry of s^.map under k
}
```

- Today: keyed entries are reached only through an entry binding written in
  the statement's header, whose key is read when the statement begins
  (SHARE-2); a table is no indexable base (SHARE-1, OP-4). Even a statement
  holding the whole state cannot index the table by a runtime key
  (`witnesses/w0b.wf`: `OP-4 TypeMismatch`).
- What works: keys known before the statement, as a `KeySet`, with the host
  callback receiving the entries and the state's other fields
  (`witnesses/w0d.wf`, `w0e_meta.wf`, accepted).
- Need: scripts that read keys they compute, or that were not declared, and
  `KEYS`/`SCAN`, which need iteration over the table (already recorded as an
  open language decision).

### G2. Observing another context during an atomic statement

```wf
atomic s = &h {
  loop {
    if stop_requested(&kill) { break; }   // set by another connection
    step(s);
  }
}
```

- Today: no atomic statement nests in another and no waiting call runs in
  one (SHARE-2, `witnesses/w1.wf`: `SHARE-2 WaitInsideAtomic`), and every
  read in a statement sees one snapshot (SHARE-3). Contexts communicate only
  through atomic statements and the host (WAIT-2).
- What works: a script that reads the clock and stops itself at a deadline
  (`witnesses/w1_clock.wf`, accepted); `now` writes its clock but does not
  wait.
- Need: `SCRIPT KILL` from another connection. Redis permits it only before
  the script's first write, when ending the script is indistinguishable
  from the script never having run.

### G3. Waiting with a bound

```wf
atomic s = &h until deadline else { reply_busy(); }
```

- Today: an atomic statement waits until it takes effect (WAIT-2), a join
  until its context completes (WAIT-3), and a guard writes nothing, so it
  cannot read the clock (`witnesses/w2.wf`: `SHARE-2 AtomicGuardWrites`).
  Only host I/O functions take `deadline: Option<Instant>`.
- Need: a connection waiting behind a long script answers `BUSY` instead of
  waiting without bound. SHARE-3 lets an implementation hold less than the
  statement's state, so a script whose keys are declared blocks only
  commands on those keys.

### G4. A fact checked once and used at every access

```wf
fn run(code: &Code, regs: &Box<Slots<Value>>, base: u64)
  requires forall(i in 0..code.len): code.a[i] < code.frame
  requires base + code.frame <= regs.len
{
  ... regs[base + code.a[pc]] ...   // no check: the quantified fact bounds it
}
```

- Today: a type invariant reaches data by field and `Box` projections only,
  with no subscript or quantifier (TYPE-11), and is established at
  parameters, results and atomic blocks, never at an element read
  (`witnesses/w3e_elem_inv.wf`: `OP-4 UndischargedBoundsObligation`). A
  range fact can be written but no ordinary obligation consumes it (RANGE-2;
  `witnesses/w3b.wf`: `OP-2 UndischargedIntegerDomainObligation`).
- What works: register operands typed `u8` with 256 slots of headroom per
  frame, so the type bounds the index (`witnesses/w3d_u8.wf`, accepted); a
  dispatch loop whose fetch is proved by `invariant inside: pc < n`, at one
  comparison per jump or fall-through (`witnesses/w3g_jumps.wf`, accepted).
- Need: the bytecode verifier's facts (operands within the frame, jump
  targets within the code, a final `RETURN`) used by the dispatch loop
  without a comparison per instruction. Silverfir measured a bounds check
  against a length already in a register as free and one that loads the
  length as not free, so the cost of the workaround is to be measured
  before this gap is ranked.

### G5. Exhausting the heap

- Today: allocation is total; heap exhaustion terminates the program and
  returns no failure (STOR-8, SCOPE-3). Stack exhaustion is also outside the
  source model.
- Need: a script that allocates without bound must fail alone, not stop
  firn. The engine can count its own bytes and refuse past a limit, and its
  calls can live on a heap stack instead of the machine stack. Whether a
  language-level answer is needed depends on whether firn's own `maxmemory`
  needs one too.

### G6. Floating-point functions

- Today: `ffloor`, `fceil`, `ftrunc`, `frem`, `fsqrt.strict`, `ffma.strict`
  and bit reinterpretation exist (OP-1); there is no `pow`, `exp`, `log` or
  trigonometric primitive.
- Need: Lua's `^` and `math.*`. These are library or host functions; matching
  the C library's results bit for bit is the hard part.

## Settled without a language change

- Recursive values (`witnesses/x_value.wf`, accepted). Lua tables need
  shared identity and cycles, which owned `Box` values cannot give (TYPE-9,
  REF-3), so objects live in slabs addressed by generational handles and the
  engine collects them.
- A host callback through a generic interface with an environment reference
  (`witnesses/w0d.wf`), the pattern the standard library's slab uses.
- Number formatting and parsing (firn already formats `%.17g` and parses
  like `strtod` in Whitefoot) and string hashing.

## Planned experiments

- **E1, the heap.** A slab-and-handle heap with a mark-and-sweep collector,
  measured on allocation-heavy Lua-shaped workloads (tables of tables,
  string building, closures) against Redis's Lua. Start with a short probe
  and extend only if the spread is too wide.
- **E2, dispatch.** With the match-dispatch work: the Halo instruction set as
  the benchmark VM.
- **E3, checked operands.** The same loop with checked and with `u8`-bounded
  operand access, to price G4.

Each experiment states its falsifier before it runs.
