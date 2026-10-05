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
- What works: register operands typed `u8` with 256 slots of headroom
  above the current frame's base (one `requires base + 256 <= regs.len`,
  kept at calls by growing the stack, as Lua's `EXTRA_STACK` does), so the
  type bounds the index (`witnesses/w3d_u8.wf`, accepted); a
  dispatch loop whose fetch is proved by `invariant inside: pc < n`, at one
  comparison per jump or fall-through (`witnesses/w3g_jumps.wf`, accepted).
- Need: the bytecode verifier's facts (operands within the frame, jump
  targets within the code, a final `RETURN`) used by the dispatch loop
  without a comparison per instruction. Silverfir measured a bounds check
  against a length already in a register as free and one that loads the
  length as not free, so the cost of the workaround is to be measured
  before this gap is ranked.
- Measured (match-dispatch E0, C register-machine interpreter, M1 Pro,
  geomean of six kernels, [RESULTS.md on PR #217 at `c6c16a9cd`](https://github.com/Ming-Research/Whitefoot/blob/c6c16a9cd/research/experiments/match-dispatch/RESULTS.md)):
  checked frame indexes cost 4.0% over `u8` operands, and the remaining
  fetch comparison 2.3%; what this gap could buy beyond the workaround is
  that 2.3%. The further 9.0% between `u8` without checks and raw pointers
  is the index representation, a lowering matter rather than a proof one.

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

## Selected direction (owner, 2026-10-04)

Two outside consultations analysed the gaps independently: GPT-6 Astra and,
without Astra's answer, Fable. Fable's analysis was taken further; the owner
approved its direction:

- **The header grants, the block reaches at most the grant, the
  implementation holds anything in between.** An atomic statement's header
  names every part its block may form a path to; meaning stays SHARE-3's
  (exclusive access to the whole state at one point); what is actually
  locked lies between the block's footprint and the whole state, which is
  where lazy taking, escalation after patience and SHARE-3's liberty to hold
  less already live.
- **A, an explicit whole-table binding** answers G1:

  ```wf
  atomic s = &store^.state, t = &s^.map {     // the table, whole
    let e = t^[key];                           // a key computed in the block
    let es = &t^[keys];                        // entries of a KeySet formed in the block
  }
  ```

  A bare `s` grants the state's non-table fields, which are its one
  non-table lock unit; a table is reachable only through its header binding,
  by entry, by key set or whole; one binding per table, so entries and the
  whole of one table in one header are refused. Passing `s` to a callee is
  admitted when the callee's row lies within the grant, so a script host's
  `writes(env)` requires the table granted whole and the hold that
  serializes the server is visible in the header. Today's implicit whole
  hold (any path to a table outside an entry binding) is retired; firn's
  DBSIZE gains `t = &s^.map`.
- **B, a deadline on the statement** answers G3:
  `atomic ... until d { ... } else { ... }` takes effect before the clock
  reaches `d`, or runs the else block having executed nothing of its guard
  or block, as a host operation's `DeadlinePassed` transfers nothing. Only a
  statement with a deadline takes every unit its block can reach before the
  block; all others keep the lazy rule.
- **C, no change for G2**: a block that ends having written nothing of its
  state has no effect, which is Redis's own condition for `SCRIPT KILL`.
  Firn runs a script in budgeted slices; a slice that ends before the
  script's first write returns, firn checks for a kill between statements
  and runs the script again from the start with a doubled budget; a script
  that has written runs to its end. Keys a script did not declare are found
  the same way: before the first write, the key joins the `KeySet` and the
  script reruns.
- **G4, G5, G6: no language change.** G4's measured price is 2.3% (above);
  G5 is the engine's own byte accounting and a heap call stack; G6 is a
  numeric library (fdlibm or musl lineage) with specified results, not a
  host binding to the platform's libm.

A and B are one amendment of GRAM-4, SHARE-1, SHARE-2, SHARE-3, WAIT-2 and
OP-4, designed with firn's KeySet redesign (insertion-order keys, lock order
private to the runtime), which lands first.

## VM core

The engine's design, from values to the work order and its falsifiers, is
[VM.md](VM.md).

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

- **E1, the heap.** Question: does a heap of generational slab handles
  with a stop-the-world mark-and-sweep collector cost too much for Lua-shaped
  allocation? First workload: binary-trees (build and check complete trees
  of two-field tables, discard them), the standard allocation benchmark.
  Three programs run the same algorithm: C with `malloc`/`free` (the floor),
  Whitefoot over the handle heap with its collector, and the same algorithm
  as a Lua script run by Redis 7.0.15's own Lua 5.1. Criterion, stated
  before measuring: the handle heap is viable for Halo if the Whitefoot
  program takes at most 3 times the C program's time at depths 14 to 16 and
  less time than Redis's Lua; above 3 times, the cost of each handle check
  is attributed before the design proceeds. Short runs first; more
  repetitions only if the spread across three runs exceeds 10%.
  ([experiments/halo-heap](../../experiments/halo-heap/))
  Result (2026-10-04, M1 Pro under shared load): met at every depth.
  Depth 16 medians: C 0.38 s, Whitefoot 0.37 s, Redis Lua 2.55 s; peak RSS
  5.3, 31.6 and 79.1 MiB. Collection ran only at safe points between tree
  constructions, so the collector's cost inside a running interpreter, with
  roots on the Lua stack, is not yet measured
  ([RESULTS.md](../../experiments/halo-heap/RESULTS.md)).
- **E2, dispatch.** With the match-dispatch work: the Halo instruction set as
  the benchmark VM.
- **E3, checked operands.** The same loop with checked and with `u8`-bounded
  operand access, to price G4.

- **Oracle corpus.** 80 EVAL scripts (Lua 5.1 core semantics, the Redis
  scripting API, application scripts, the codec libraries) with the replies
  Redis 7.0.15 gives, in a RESP-typed form; the same runner pointed at firn
  compares a Halo build ([experiments/halo-oracle](../../experiments/halo-oracle/)).

Each experiment states its falsifier before it runs.
