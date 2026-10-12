# Halo's Lua call path under the split dispatch

Halo-wf filed three code-generation requests from one callgrind profile of
its `fib` kernel (Halo-wf `f11e9b3ed340`, the bench host of `c900a68b2f06`
built by release `wf-d837678c6905` with full LTO and clang 22.1.8, fib(32),
7,049,155 Lua calls;
[run 38236457893](https://github.com/Ming-Research/Halo-wf/actions/runs/38236457893)).
Halo executes 862.5 instructions per Lua call where PUC Lua 5.1.5 executes
403.0. The requests name three costs; this investigation attributes each to
the lowering that produces it before any compiler change is designed.

This record is separate from [the match-dispatch
investigation](../match-dispatch/DESIGN.md) because one of the three
questions is the result ABI ([compiler/result-registers](../../../design/compiler/result-registers.md)
and [compiler/payload-enum-layout](../../../design/compiler/payload-enum-layout.md)),
not the dispatch, and because its yardstick is one interpreter's call path
rather than v2h's CoreMark.

## The loop as Halo writes it

`run` in Halo's `lib/halo/vm/dispatch.wf` is `loop { match code^.inner[pc] }`
with `pc`, `base` and `kbase` as loop variables. Before the `match` it binds
`let step = Step::Error();`. Hot arms whose fast path succeeds end in
`continue` with a new `pc`; every other arm sets `step` from a helper
(`Call` from `instruction_call`, `Return` from `instruction_return`) and falls
to one join after the `match`, which calls `checked_step(step)` and matches its
`Result<Cursor, Outcome>`: `Ok` sets `pc`, `base` and `kbase` from the
`Cursor`, `Err` returns the `Outcome`. `Step` is
`Jump(pc, base, kbase) | Done(count) | Error | Stop | Budget`: two payload
variants whose product exceeds three integer leaves, so it is laid out as a
union and is memory-only, as is `Result<Cursor, Outcome>`.

What the split does with this loop, read from
`compiler/src/backend/emitter/dispatch.rs` at this branch's base:

- **The join is copied into every arm.** An arm's function holds every block
  reachable from its target without passing the header, so each arm that can
  fall off the `match` carries its own copy of the `checked_step` call and
  the `Result` match.
- **A slot used by more than one part lives in the shared frame.**
  `emit_dispatch_parts` makes a slot a part's own `alloca` only when exactly
  one part uses it and the enclosing function does not; every other slot is a
  `getelementptr` into `%wf.frame`, the enclosing function's frame, whose
  pointer every part passes on in its guaranteed tail call
  ([compiler/match-dispatch-lowering](../../../design/compiler/match-dispatch-lowering.md),
  the decision on the frame). The slots of the join's `step`, its `Result`
  and its `cursor` are used by every arm holding a copy of the join.
- **The header runs at the end of every arm.** The dispatch function, which
  holds the header's instructions, is always inlined into each arm's tail
  transfer, so `let step = Step::Error();` executes on every dispatch,
  including dispatches of arms that never read `step`.

## The three questions

### R1: dead `Result` and `Cursor` stores at the join

**Observed by Halo.** On the `Call` path and on the `Return` path, 7 stores
per Lua call write the whole `Result` and the extracted `Cursor` into the
shared frame; the `Ok` side then uses `pc`, `base` and `kbase` from registers
and nothing reads the stores back. Seven is a tag and three words of the
`Result` plus the three words of the `Cursor`, which is consistent with H1
below but does not establish it.

**Question.** Which lowering keeps these stores?

**Hypotheses and the observation that separates them**, all read from the
artifact of the workflow below:

| hypothesis | prediction in the IR and the code | refuted by |
|---|---|---|
| H1. The join's slots are in the shared frame because several arms use them, and LLVM keeps stores to memory a tail-called part may read. | In each arm's emitted prelude the `Result` and `Cursor` slots are `getelementptr ... %wf.frame`, not `alloca`; in the module after LTO the stores remain on the `Ok` path up to the `musttail` call, with no load of those addresses after them in the arm; in the machine code they address the register holding the frame pointer, not `rsp`. | either slot is an `alloca` in the arm, or a load in the same arm reads the stored words. |
| H2. The `Result` is materialized for the `Err` path, which reads it, and the stores were not sunk into that path. | The `Err` block of the arm's join loads the slot; the stores precede the branch on the tag. | no load of the slot on the `Err` path (with H1's evidence, H2 is then not the cause). |
| H3. A frame-wide store of the transferred cursor serves a cold reader elsewhere (a resume after a host call, the enclosing function after the loop, `note_raise`'s `pc`). | The stored frame offsets are loaded by another part, the dispatch function or the enclosing function before being stored again. | in every part and the enclosing function, each of those offsets is stored before any load of it. |
| H4. The slot's address escapes to an out-of-line callee (`checked_step` returning through it as destination, or `enter_lua`/`finish` writing `step` there). | After LTO the arm still calls that function with the slot as an argument; the stores sit in the callee. | `checked_step` is inlined into the arm and the stores are the arm's own. |

The header's `Step::Error()` initialization is read the same way: if its
stores reach the frame on every dispatch of `SubRK`, `LtJmpRK` and
`GetUpval`, it is the same mechanism as H1 on the hot arms.

**What the design already decides.** If H1 holds, the frame write is what
compiler/match-dispatch-lowering's frame decision prescribes: a slot used by
more than one part is reached through the shared frame. That decision's own
ground is that "storage a value keeps across parts must outlive them"; if H1
holds, these slots keep nothing across a transfer, since every part writes them before
reading them. Counting the parts that use a slot is the rule as written, and a
copied join makes a value that lives within one part's execution look
shared. Narrowing the rule, for example to a slot whose value never crosses a
transfer between parts, changes that decision and goes to the owner before
implementation.

### R2: the four-word `Step` returned through memory

**Observed by Halo.** `enter_lua` and `finish` return `Step::Jump` through the
caller's destination slot, about 16 memory operations per Lua call.

**Question.** After LTO, are `enter_lua` and `finish` still calls from the
`Call` and `Return` arms (through `instruction_call`, `prepare` and
`instruction_return`), and where does the destination point?

- **If they remain calls:** the stores are the callee's writes of the `Step`
  and the loads are the caller's reads, which a register return would
  remove. Two approved decisions require the memory return:
  [compiler/result-registers](../../../design/compiler/result-registers.md)
  returns at most three integer leaves in registers on every target, and
  [compiler/payload-enum-layout](../../../design/compiler/payload-enum-layout.md)
  keeps a union-laid-out enum memory-only, returned through a destination.
  `Step::Jump` needs a tag and three words. Changing either is the status
  board item on returning small union enums in registers, whose card is
  pending.
- **If they are inlined:** the return ABI no longer applies, and the memory
  operations are writes of `step`'s slot. If that slot is a `getelementptr`
  into the shared frame, R2 is R1's mechanism and a register return would
  not remove it.

The count per Lua call of the destination's stores and loads in each
function, from the callgrind join, says how much each case accounts for.

### R3: loop state spilled in hot arms

**Observed by Halo.** The split passes 11 of `preserve_none`'s 12 integer
argument registers on x86-64 as loop state, the twelfth holding the handler
address ([compiler/match-dispatch-lowering](../../../design/compiler/match-dispatch-lowering.md),
the decision on the convention). `SubRK` (18 of 53 instructions), `LtJmpRK`
(19 of 56) and `GetUpval` (12 of 62) store and reload 3 to 6 loop-state
values, about 110 instructions per Lua call, and their frame setup is not
moved to cold paths.

**Question.** Do the spills execute on the fast path because the fast path
itself needs more registers, or because of a call it does not take?

- **Pressure on the fast path:** the `rsp`-relative stores and reloads
  execute once per execution of the arm (from the callgrind counts) with no
  call between them. Only a different register assignment removes them;
  this is the case the parked study of pinning the state in one function
  with empty inline-assembly register constraints addresses (the status board item on measuring whether interpreter state can stay
  in registers without the calling convention; its design and its 2%
  criterion against `preserve_none` tail calls are on the board, not yet in
  this research record).
- **Around a call:** the spills surround a call to an ordinary C-convention
  helper (the slow path's `instruction_sub_rk` or `instruction_lt_jmp_rk`, or
  a helper the fast path calls), across which only the six C callee-saved
  registers survive. If they execute on the fast path while the call is on
  the slow path, the cause is spill and prologue placement (shrink-wrapping),
  and the candidates are moving the slow path into a cold part of its own or
  marking it cold; pinning registers in one function would not remove a
  spill around a C call.
- **Prologue alone:** a stack adjustment or `push` executed on every arm
  execution with spills only on the slow path is a frame-setup placement
  failure of the same kind.

Nothing in the design tree decides where LLVM places spills inside an arm.
The register budget decision fixes which values are parameters; it does not
require spilling a parameter on a path that makes no call.

## Method

First step: attribution, no compiler change, nothing timed. The temporary
workflow `.github/workflows/halo-call-path.yml` (manual dispatch, one hosted
x86-64 job, the LLVM major `.github/llvm-major` pins) builds this branch's
compiler, builds Halo-wf `c900a68b2f06`'s bench host with full LTO once
plainly and once with lld keeping its LTO temporaries, and records:

- the module Whitefoot emits, the merged module before LTO optimization and
  the module after it, each cut down to `run`'s enclosing body, its dispatch
  function, the `Call`, `Return`, `SubRK`, `LtJmpRK` and `GetUpval` arms
  (named by the one helper each calls) and the call path's helpers;
- the dispatch ledger, and whether the two binaries are identical;
- objdump of those functions as linked;
- callgrind of fib(32) with every instruction's executions per Lua call and,
  per function, executed instructions, stores and loads per Lua call by base
  register, so stack spills (`rsp`) and frame stores are counted apart.

Each question is answered by the observations its section names. A compiler
change that follows gets its own question, comparison and rejection
criterion in this record before it is measured; its timing runs on the
14900K against a twin of its base.
