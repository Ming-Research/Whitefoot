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

First step: attribution, no compiler change, nothing timed. A temporary
workflow, `.github/workflows/halo-call-path.yml` on the experiment branch
`claude/halo-call-path` at 7517c9d02bb2 (run by a push to that branch, one
hosted x86-64 job, the LLVM major `.github/llvm-major` pins), builds that
branch's compiler, builds Halo-wf `c900a68b2f06`'s bench host with full LTO once
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

## Results (2026-10-12, run 38311876567)

Every number below comes from Whitefoot
[run 38311876567](https://github.com/Ming-Research/Whitefoot/actions/runs/38311876567),
artifact `halo-call-path`. The run built `f49d0d19c367`, which is main
`abc27fbd3` plus only the workflow and this record, with Halo-wf
`c900a68b2f06`, clang and LLD 22.1.8 and valgrind 3.22.0 on a hosted AMD EPYC
7763. Instruction counts are callgrind's executed instructions (`Ir`) divided
by fib(32)'s 7,049,155 Lua calls (`callgrind/summary.txt`,
`callgrind/counts-<function>.txt`). The plain and save-temps binaries are
byte-identical (`manifest.txt`), so the LTO modules are the profiled code's.
The program executes 862.53 instructions per Lua call, Halo-wf's figure
(862.5); PUC Lua 5.1.5 executes 403.0 on the same kernel (Halo-wf run
38236457893). The dispatch ledger splits `run` into 73 arms taking all 11
integer argument registers `preserve_none` offers, keeps no value in the
frame, and dispatches through the tag-indexed table; it prints no handler-word
line for `run`. The five arms each execute once per Lua call (`hot-arms.tsv`:
`Call` arm 63, `Return` arm 65, `SubRK` arm 25, `LtJmpRK` arm 54, `GetUpval`
arm 5).

### R1: the frame stores are the shared-frame placement of the join's slots (H1)

In the emitted module (`ir/call-path.emitted.ll`) every slot of the `Call`
arm is a `getelementptr` into `%wf.frame`, the join's `Result`
(`%wf.slot.35`) and `Cursor` (`%wf.slot.36`) among them; the arm's only
`alloca` is its pin slot. After LTO (`ir/call-path.0.4.opt.ll`)
`checked_step` is inlined into the arm, and the two slots lie at frame offsets
1016 to 1064. The linked `Call` arm stores the `Result` (a tag word and three
payload words, frame offsets 0x3f8 to 0x410) and the `Cursor` (0x418 to
0x428) through the frame pointer it received in `r11` (copied to `rbx`), then
tail-jumps to the next arm with the same three values in registers. The
`Return` arm does the same through `rbp`, holding the frame pointer it
reloads from its stack. In the optimized module the 73 parts hold 1,045
store instructions to these offsets and no load of them, and the enclosing
function reads nothing from the frame after it calls the dispatch function.

| arm | stores to the join's `Result` and `Cursor` frame slots on the executed path, instructions per Lua call (this run; ideal 0) | loads of those slots in any part or the enclosing function, count (H1 predicts 0) | where the stores are |
|---|---:|---:|---|
| `Call` (arm 63) | 7.00 | 0 | `6c21d` to `6c24b`, base `rbx` = incoming frame pointer |
| `Return` (arm 65) | 7.00 | 0 | `6c946` to `6c974`, base `rbp` = frame pointer |

- **H1 survives.** The slots are frame `getelementptr`s in every part, the
  stores reach the `musttail` transfer, and nothing reads them.
- **H2 is rejected.** The `Ok` path stores and transfers. The `Err` path,
  which runs once in the whole run, stores its own `Result` before returning,
  and no path reads what the `Ok` path stored.
- **H3 is rejected.** No part loads those offsets, and neither does the
  enclosing function.
- **H4 is rejected.** `checked_step` has no symbol in the binary and no
  definition after LTO. The stores are the arm's own.
- **The header's `Step::Error()`** costs nothing on the hot arms: `SubRK`,
  `LtJmpRK` and `GetUpval` execute no frame store.

The cost is 14 executed instructions per Lua call. It comes from
compiler/match-dispatch-lowering's decision on the frame, "a frame slot that
exactly one part uses and the enclosing function never uses is that part's
own allocation". The join after the `match` is copied into each of the 73
arms, so its slots have 73 users and stay in the shared frame, whose pointer
every guaranteed tail call passes on. If a slot whose value never crosses a
transfer were each part's own allocation, the host could remove these stores,
as it removed the `match` scrutinee's copy in the measurement that decision
cites. That change narrows the decision and goes to the owner first.

### R2: `enter_lua` and `finish` stay calls, so `Step` returns through memory

After LTO neither function is inlined. `prepare`, which the `Call` arm calls,
tail-calls `enter_lua` with its own destination. The arm set that destination
to frame offset 40 (`%wf.slot.2`, the loop's `step`). The `Return` arm calls
`finish` with the same slot (`ir/call-path.0.4.opt.ll`). Each callee writes
the `Jump` tag word and three payload words there (`enter_lua` at `2feb1` to
`2fec5`, `finish` at `2f3ac` to `2f3c0`). The arm reads the tag and the three
words back from frame offsets 40 to 64 to run the inlined `checked_step`.

| function | executed instructions per Lua call (this run) | executions per Lua call | `Step` stores into the destination, per Lua call (ideal 0 with a register return) | `Step` loads by the calling arm, per Lua call (ideal 0) |
|---|---:|---:|---:|---:|
| `enter_lua` (from `prepare`, from the `Call` arm) | 190.00 | 1.00 | 4.00 | 4.00 (`Call` arm, `6c178`, `6c1b2`, `6c1c3`, `6c20b`) |
| `finish` (from the `Return` arm) | 93.00 | 1.00 | 4.00 | 4.00 (`Return` arm, `6c8ce`, `6c8e1`, `6c8f2`, `6c934`) |

The "remain calls" branch holds: 16 memory operations per Lua call, Halo-wf's
figure. Two approved decisions require them. compiler/result-registers' first
decision returns a result in registers only when it has at most three integer
leaves, and compiler/payload-enum-layout's fourth decision keeps a
union-laid-out enum memory-only, returned through a destination. `Step::Jump`
needs a tag and three words. That the destination is a shared-frame slot
(R1's mechanism) adds nothing here, because an out-of-line callee writes
whatever slot it is given.

### R3: most of the hot arms' spills serve their slow paths

`SubRK`, `LtJmpRK` and `GetUpval` execute no call: every `call` in them
counts 0. Yet each sets up a stack frame and stores to the stack on every
execution. Reading them instruction by instruction
(`callgrind/counts-...arm.{25,54,5}.txt`) sorts the overhead into five kinds:

- (a) a loop-state value stored at entry and reloaded before the tail jump,
  because the executed path used its register for a temporary;
- (b) a value stored at entry that only the slow path reads; in `SubRK` and
  `LtJmpRK` one of these is the pin slot holding the code run's pointer,
  which emission stores in every part's prelude (compiler/match-dispatch-lowering,
  the decision on pinned references);
- (c) the stack frame: `push rbp`, `sub rsp`, `add rsp`, `pop rbp`;
- (d) copies of loop-state values into `rbx`, `rbp` or `r10` and back;
- (e) the address of the join's `Result` slot, formed on the executed path
  and used only by the slow path.

| arm | executed instructions per Lua call (this run) | (a) spilled and reloaded on the executed path | (b) stored for the slow path only | (c) stack frame | (d) register copies | (e) slow-path slot address | overhead, instructions per Lua call (Halo-wf's figure in brackets; ideal 0) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `SubRK` (arm 25) | 53 | 6 (`rdi`, `r13`, `r14`) | 3 (`r8` twice, `rsi`) | 4 | 4 | 1 | 18 (18) |
| `LtJmpRK` (arm 54) | 56 | 7 (`rdi`, `rcx`, `r8`, a byte temporary) | 3 (`r8`, `r13`, `rsi`) | 4 | 4 | 1 | 19 (19) |
| `GetUpval` (arm 5) | 62 | 5 (`r8` reloaded twice, `rdi`) | 0 | 4 | 3 | 0 | 12 (12) |
| total | 171 | 18 | 6 | 12 | 11 | 2 | 49 |

- **Kinds (b) to (e), 31 instructions per Lua call, are spills around calls
  the executed path does not make.** The slow paths call `slow`,
  `heap.intern`, `unwind` and `restore_stopped_stack` under the C convention.
  A value live across those calls must be in one of the six registers they
  preserve, or on the stack. LLVM placed the frame, the spills and the copies
  at the arm's entry, which every execution runs.
- **Kind (a), 18 instructions per Lua call, is pressure on the executed
  path.** All 11 loop-state values must be back in their argument registers
  at the tail jump. That leaves `rax`, `rbx`, `rbp` and `r10`, plus the
  cursor's register once its operands are read, for temporaries. This run
  does not show whether those would suffice once the slow paths no longer
  claim registers.
- **The `Call` and `Return` arms differ: they call on every execution**
  (`prepare` and `finish`), so their stack traffic is spilling around a hot
  call. `Call` executes 7 stack stores, 12 stack loads and 7 pushes of stack
  arguments, since `prepare` takes twelve parameters. `Return` executes 18
  stack stores, 8 of which copy the popped call frame that `finish` takes by
  value, and 12 stack loads (`callgrind/summary.txt`).

No design decision places these spills; compiler/match-dispatch-lowering's
convention decision fixes 11 integer parameters, and where the frame and
spills go inside an arm is LLVM's choice. The candidate this points to is
moving each arm's slow path out of the arm, into a part of its own entered by
a tail call with the loop state as arguments, so the arm itself calls
nothing. That would make up to the 31 instructions per Lua call of kinds (b)
to (e) unnecessary in these three arms, and kind (a) may shrink with it. The
status board study of pinning the state in one function with inline-assembly
constraints addresses kind (a) only.

### What the data does not settle

- The savings above count instructions the named change would make
  unnecessary, read off this run. No changed compiler was measured, and they
  are instruction counts on a hosted guest, not time.
- Whether a four-word `Step` can return in registers at all: LLVM returns at
  most three integer leaves in registers on x86-64
  ([result-register investigation](../result-registers/DESIGN.md#demotion-probe)).
  Removing R2's 16 operations therefore needs a narrower `Step`
  representation, or `enter_lua` and `finish` inlined together with R1's
  part-local slots, or a different convention for such calls.
- `storage_parts` gives a block that several arms reach only the last arm's
  identity. This run does not bear on it: the emitter places the join's
  slots by its count of using parts, which the data confirms, not through
  `storage_parts`.
- No handler word for Halo's `Cell`: the ledger prints the table dispatch,
  and every arm loads its next handler from the tag-indexed table. Why the
  composition-wide plan did not select `Cell` was not examined here.
