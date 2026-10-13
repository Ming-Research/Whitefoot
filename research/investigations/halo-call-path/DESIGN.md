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

## The join's slots in each arm

The compiler change that follows R1, on the branch `claude/join-slot-local`
(Whitefoot PR #405): a frame slot whose contents never cross a transfer
between the split's parts is the own allocation of each part that uses it,
where before a slot was a part's own only when exactly one part used it
([compiler/match-dispatch-lowering](../../../design/compiler/match-dispatch-lowering.md),
the decision on the frame).

### Question, comparison and rejection criterion

**Written after the run they judge.** The method above asks for these before
the change is measured. They were written after run 38317117641 had
reported, so the criterion below checks a result already seen and predicts
nothing. The one prediction made before that run is R1's: the change would
remove 14 executed instructions per Lua call.

- **Question.** Does keeping the join's slots in each arm remove R1's 14
  frame stores per Lua call, and does Halo's call path then execute fewer
  instructions?
- **Comparison.** fib(32) on Halo-wf `c900a68b2f06`, executed instructions
  per Lua call from callgrind, from this record's workflow at two revisions:
  the base, [run 38311876567](https://github.com/Ming-Research/Whitefoot/actions/runs/38311876567)
  (`f49d0d19c367`, main `abc27fbd3` plus the workflow), and the change,
  [run 38317117641](https://github.com/Ming-Research/Whitefoot/actions/runs/38317117641)
  on the experiment branch `claude/halo-call-path` at `71d9ba663d22`, which
  merges the change at `e0f8615df` into that workflow. The
  change's base, main `1741f1262`, differs from `abc27fbd3` in tests,
  research and CI budgets only, not in compiler source. Both runs used clang
  and LLD 22.1.8 and valgrind 3.22.0; callgrind counts instructions, so the
  hosted runner's speed does not enter.
- **Rejection criterion.** The change is rejected if the `Call` or the
  `Return` arm still executes a store through the frame pointer, where R1
  found the join's `Result` and `Cursor` stores, or if the program executes
  no fewer instructions per Lua call than the base's 862.53.

### Which slots cross a transfer

The transfers are the enclosing function's call of the dispatch function,
which holds the header; a part's guaranteed tail call of the dispatch
function or of an arm; and a part's return, after which nothing reads the
frame, since the loop's blocks leave only by returning and the enclosing
function returns the dispatch function's result. A slot stays in the shared
frame when:

- the enclosing function uses it. It defines the values the loop reads
  from before it, writes every header parameter on its edge into the loop,
  and also
  runs the header instructions hoisted out of the loop and the pinned
  projections, which the loop's value liveness does not place there;
- it holds a header parameter, which every edge into the header writes
  before its transfer and the header or an arm reads after one;
- it holds a value live on entry to an arm, which the dispatch function
  holds before its tail call of the arm. A value live on entry to the header
  needs no rule of its own: the loop's own values reach the header only as
  its parameters, so such a value is defined before the loop and is live on
  entry to every arm that jumps back to the header;
- it is reached through an address: a binding whose address is taken, and
  an indexed family's copied root and block directory. An address derived
  from it, such as a field's address or a range view, is a register value a
  part may pass to the next, and the analysis follows values held in slots,
  not addresses. A range view is always formed from such an address, so no
  view points into a slot the analysis decides;
- it holds a value the enclosing function stores before the loop for the
  parts to read, because no argument register was left for it.

A slot written and read within one edge or one instruction (an edge's
snapshot, the transfer scratch, an array fill's index) is always the part's
own. A waiting call's or a context's result occurs only in a function that
waits, which is emitted whole. The liveness comes from the storage plan's
control-flow graph (`allocations_across` in
`compiler/src/backend/storage.rs`); the per-kind choice is `confined` in
`compiler/src/backend/emitter/dispatch.rs`.

### Result (run 38317117641)

Every number is from `callgrind/summary.txt`, which both runs print in their
logs, divided by fib(32)'s 7,049,155 Lua calls.

| function (the loop's arm by its tag number, or its helper functions) | base: executed instructions per Lua call, the function's callgrind instruction count divided by fib(32)'s 7,049,155 Lua calls, run 38311876567 (this column is the baseline; lower is better, ideal 14 fewer than the whole program's 862.53) | change: the same measure, run 38317117641 (ideal: the base's value, less 7 in each of the `Call` and `Return` arms) | change minus base, instructions per Lua call (baseline 0; R1 predicted -14 in total, which is the ideal) |
|---|---:|---:|---:|
| whole program | 862.53 | 841.53 | -21.00 |
| `Call` arm (63) | 113 | 101 | -12 |
| `Return` arm (65) | 122 | 112 | -10 |
| `SubRK` arm (25) | 53 | 60 | +7 |
| `LtJmpRK` arm (54) | 56 | 53 | -3 |
| `GetUpval` arm (5) | 62 | 61 | -1 |
| `enter_lua`, `finish`, `prepare`, `push_frame`, `ensure_stack` and `native_binding` together | 428 | 428 | 0 |
| everything the workflow does not count by function | 28.53 | 26.53 | -2.00 |

- **The criterion is not met, so the change stands.** The `Call` arm's 7.00
  stores per Lua call through `rbx`, which held the frame pointer, and the
  `Return` arm's 7.00 through `rbp` are gone; each arm's remaining stores
  are through the stack pointer or are ones the base also executed (`Call`:
  `rsp` 8.00 against 7.00, `r15` 1.00 in both; `Return`: `rsp` 18.00 and
  `rax` 1.00 in both). The program executes 21.00 fewer instructions per Lua
  call, 2.4% fewer than the base.
- **14 of the 21 are the predicted stores; the other 7 are not
  attributed.** By kind of instruction: `Call` executes 6 fewer stores (7
  through the frame pointer fewer, 1 on the stack more), 1 more load and 7
  fewer other instructions; `Return` 7 fewer stores, as many loads (43.00)
  and 3 fewer other instructions. In each of the two arms four loads
  through registers other than the stack pointer are gone and the stack
  pointer's loads rose by about as many (`Call`: `rbx` 3.00 and `r14` 1.00
  gone, `rsp` 12.00 to 17.00; `Return`: `rbp` 3.00 and `rbx` 1.00 gone,
  `rsp` 12.00 to 16.00). R2's table puts four loads of the `Step`
  destination, the loop's `step` slot in the frame, in each of these arms,
  so the counts suggest that this slot, too, has become each arm's own
  allocation, which the rule allows when no arm reads the header's
  `Step::Error()`; the disassembly in the run's artifact would confirm it.
  `SubRK` executes 3 more stores, 2 more loads and 2 more other
  instructions; `LtJmpRK` 1 store and 2 other instructions fewer;
  `GetUpval` 1 load fewer. These three arms executed no frame store in the
  base (R1), and counts per base register do not show what moved their
  register and stack assignment in the change. Which instructions
  make up the 7 would be read from the per-instruction counts in the run's
  artifact (`callgrind/counts-*.txt`), which this record did not read; until
  then they are unexplained.
- **The arms' sizes as linked** fell for every hot arm (instructions in
  `disasm/sizes.txt`, base to change: `Call` 194 to 149, `Return` 206 to
  172, `SubRK` 350 to 338, `LtJmpRK` 338 to 290, `GetUpval` 206 to 136).
  These count every path of the arm, most of which fib(32) does not
  execute.

## Call entries for calls made from parts (R3 follow-up)

**Question.** Does sending every ordinary call a split part makes through a
`preserve_most` entry of its callee remove the spill, frame and copy
overhead of kinds (b) to (e) from SubRK, LtJmpRK and GetUpval?

**Change.** The call-entry decision proposed in compiler/match-dispatch-lowering
(Whitefoot#410): a call from the dispatch function or an arm to a
module-defined function returning a value or through a destination names an
internal preserve_mostcc entry that calls the callee with a call-site
alwaysinline; the callee keeps the C convention.

**Comparison.** The halo-call-path workflow (hosted x86-64, pinned LLVM,
Halo-wf c900a68b2f06, full LTO, callgrind of fib(32)), run on a base B (this
branch with main at 58743a931 merged, the base of Whitefoot#410) and on C = B
plus only Whitefoot#410. Both runs build identical Halo sources; the only
difference is the compiler change.

**Measure.** Executed instructions per Lua call of the SubRK, LtJmpRK and
GetUpval arms (re-identified from hot-arms.tsv) and the program total per Lua
call, from callgrind/summary.txt.

**Rejection criterion** (written before C ran). The change is rejected if
the three arms' summed executed instructions per Lua call fall by fewer than
10 from B to C.

**Prior from reading the code.** None of the arms' surviving calls (slow,
heap.intern, unwind, restore_stopped_stack) is a call the arm makes in
source; each lies in a helper the arm calls (fast_instruction_sub_rk,
instruction_sub_rk, instruction_get_upval) that the host inlines into the
arm. The entry for such a helper is an internal single-caller copy of it,
which the host inlines the same way, leaving the inner calls in the C
convention. This predicts no change in the three arms and a rejection; a
reduction would show that the entries keep a helper out of line that the
base inlined.

### Result: runs 38329105320 (B) and 38329170660 (C)

Rejected. fib(32), 7,049,155 Lua calls, executed instructions per Lua call:

| Measure | B | C | C − B |
|---|---|---|---|
| Program total | 841.53 | 845.03 | +3.50 |
| SubRK (arm 25) | 60.0 | 59.0 | −1.0 |
| LtJmpRK (arm 54) | 53.0 | 55.0 | +2.0 |
| GetUpval (arm 5) | 61.0 | 61.0 | 0 |
| Three-arm sum | 174.0 | 175.0 | +1.0 |
| Call (arm 63) / Return (arm 65) | 101.0 / 112.0 | 101.0 / 112.0 | 0 |

The emitted module of C defines eight `preserve_mostcc` entries for the
dispatch helpers (checked_step, fast_instruction_sub_rk, instruction_sub_rk,
fast_instruction_lt_jmp_rk, instruction_lt_jmp_rk, instruction_get_upval,
instruction_call, instruction_return); none survives as a symbol or a
callgrind row: the host inlines every entry into the arm, as the prior
predicted, and the slow calls inside stay C calls. The three-arm sum moved by
+1, far from the −10 the change needed. The runs ran on different hosted CPUs
(EPYC 9V74 and 9V45); instruction counts do not depend on that.

### Result: out-of-line entries, run 38332324517 (D)

The entries became `noinline` `preserve_mostcc` wrappers that call the
original with the C convention. Executed instructions per Lua call, fib(32):

| Measure | B | C | D |
|---|---|---|---|
| Program total | 841.53 | 845.03 | 1150.03 |
| SubRK + LtJmpRK + GetUpval | 174.0 | 175.0 | 137.0 |
| Call (arm 63) | 101.0 | 101.0 | 51.0 |
| Return (arm 65) | 112.0 | 112.0 | 37.0 |
| Stores to the stack in the five hot arms | 43.0 | — | 21.0 |
| `wf__most_*` self instructions | 0 | 0 | 474.5 |

The pre-written arm criterion is met (−37 against −10), but the program is
36.7% slower: every call a part makes now crosses a wrapper, including the
fast-path helpers (`fast_instruction_sub_rk`, `fast_instruction_lt_jmp_rk`,
`checked_step`, `instruction_get_upval`, `instruction_call`,
`instruction_return`) that B inlined into the arms, and each wrapper pushes
and pops about ten registers. The rule as stated, every call from a part
through an entry, cannot tell a call the host would inline from one it
keeps out of line. D ran on an Intel Xeon 6973P-C, B and C on AMD EPYC;
instruction counts do not depend on the host.

### D's arm counts move work, they do not remove it

Each D wrapper holds its callee's inlined body, so the fast-path work left
the arms for the `wf__most_*` rows. Counting each arm with the wrapper it
calls, per Lua call: SubRK 60 → 105, LtJmpRK 53 → 106, GetUpval 61 → 114,
Call 101 → 133, Return 112 → 132. No run yet shows an arm whose fast helpers
stay inlined while only the calls that survive inlining use preserve_most.

B's post-LTO module has 89 `preserve_none` parts with 449 surviving non-tail
calls to 30 callees (426 `fastcc`, 23 C). The hot arms' survivors are the
slow paths (`slow`, `heap.intern`, `unwind`, `restore_stopped_stack`) plus
`prepare`, `collect_if_due` in Call and `finish` in Return. SubRK's
prologue is `push rbp; sub rsp,0xd0` with nine spills in the entry block:
spill code the register allocator hoisted to a point dominating six slow
call sites, which shrink-wrapping cannot move.

### Probe before building: the convention on the surviving calls only

Question: does the preserve_most convention on exactly the calls that
survive host inlining remove the arm-entry preparation, or does the cost
come from spill placement under unknown branch frequency?

Method: one step on this experiment's workflow takes B's post-LTO module
(`precodegen.ll`, save-temps), rewrites text variants, runs host code
generation only, links and runs callgrind on fib(32):

- V0, control: unchanged; must reproduce 841.53 per Lua call exactly.
- V1: in parts, every surviving `call fastcc @f` calls a `preserve_mostcc`
  clone of `f` whose own calls go to clones as well (the convention on
  surviving calls; the ceiling for transitive twins).
- V2: the slow callees (`slow`, `heap.intern`, `unwind`,
  `restore_stopped_stack`) marked `cold`, convention unchanged.
- V3: V1 and V2 together.

Rejection, written before the run: if V1 lowers neither the three-arm sum
(SubRK + LtJmpRK + GetUpval) by at least 10 nor the program total by at
least 10 per Lua call against V0, the convention premise behind every
preserve_most option is rejected. If V2 alone reaches V1's three-arm sum
within 3, the cost is spill placement, not the convention. Also recorded:
SubRK's entry-block stack stores (9 in B), and Call and Return each counted
with their callees.

### Result: probe run 38335991774

Instructions per Lua call (callgrind self counts, fib(32), 7,049,155 Lua
calls; hosted ubuntu-24.04; artifact `halo-probe`, `table.md`). V0
reproduced B's 841.53 exactly, and every variant printed 2178309.

| Measure | V0 | V1 | V2 | V3 |
|---|---|---|---|---|
| SubRK | 60 | 56 | 47 | 47 |
| LtJmpRK | 53 | 49 | 48 | 46 |
| GetUpval | 61 | 58 | 56 | 54 |
| Three-arm sum | 174 | 163 (−11) | 151 (−23) | 147 (−27) |
| Call | 101 | 100 | 101 | 102 |
| Return | 112 | 112 | 112 | 112 |
| Program total | 841.53 | 899.03 (+57.50) | 806.03 (−35.50) | 885.03 (+43.50) |
| Executed in `.most` clones | 0 | 498 | 0 | 501 |

Against the criteria written before the run:

- The rejection rule is not triggered by its letter: V1 lowers the
  three-arm sum by 11, one past the threshold of 10. Its program total rises
  by 57.5 per Lua call, because 15 of the 150 clones run and execute 498
  instructions per call that the inlined originals did not need.
- The spill-placement rule holds: V2 alone, with the convention unchanged,
  lowers the three-arm sum to 151, 12 below V1's 163 rather than merely
  within 3 of it, and lowers the program total by 35.5 (4.2%). V3 adds the
  convention to V2 and gains 4 in the arms while losing 79 in the program.

Conclusion: the arm-entry preparation is spill placement under unknown
branch frequency. Telling code generation which callees are slow removes
more of it than any preserve_most variant, and the convention's clones cost
more than they save. Every preserve_most option (in-place, out-of-line
entries, transitive twins) is rejected on this evidence for this workload.
What remains open is how the compiler learns which callees are slow without
the hand-picked list V2 used (`slow`, `heap.intern`, `unwind`,
`restore_stopped_stack`).

Instrument note: the probe's static count of SubRK entry-block stack
stores reads 0 for every variant, B included, while the disassembly read
above shows nine spills after `sub rsp,0xd0`. The static counter's pattern
does not match this code; the callgrind counts above do not depend on it.

## Which callees are cold: a study

**Question.** The probe's V2 marked four callees `cold` by hand (`slow`,
`heap.intern`, `unwind`, `restore_stopped_stack`) and lowered fib(32) from
841.53 to 806.03 instructions per Lua call. By what general rule, with no
case for any program, can the compiler learn the same thing?

Everything below is read from Halo-wf `c900a68b2f06` (the measured source)
and the compiler at main `58743a931`. Nothing was built or run.

### Where the four calls sit in Halo

Each hot arm calls a helper that the host inlines; the slow calls are inside
those helpers.

| call (inlined into the arm) | how the path to it is selected | what the path ends in |
|---|---|---|
| `slow` in `instruction_sub_rk` / `instruction_lt_jmp_rk` | the arm's `Ok(cursor)` with `cursor.handled` false, then the `Not()` arm of `num_of` (operand not a number) | a normal `Step` result that usually continues the loop (a metamethod or coercion succeeded) |
| `unwind`, `restore_stopped_stack` in `instruction_get_upval` | `if bnot(ok)` (and `vm^.host_stopped`) | `return Err<u64, Step>(...)` |
| the same two in `instruction_sub_rk` / `instruction_lt_jmp_rk` | `if bnot(ok)` after `slow` | `return` of a `Step` (not a `Result`), which the join's `checked_step` may turn into a continue (a `pcall` catch) |
| `heap.intern`, through `raise_text` | `next >= n` in the fast helpers and `instruction_get_upval`; the `None()` arm of `upvalue_handle` | `return Err(...)` for the first; the `None()` arm sets `ok` false and reaches `Err` only through the later `bnot(ok)` test |

Four structural facts follow:

1. `heap.intern` is not cold. It is the string-interning entry, called from
   72 sites in Halo, including string creation in `concat` and table keys.
   What is cold is `raise_text`, its caller on 172 error sites, which always
   returns `False()`. Marking the function `heap.intern` cold, as V2 did, is
   right for fib and is a risk for Halo's `concat` and `string-key` kernels.
   A rule must work on call sites or branches, not on whole callees picked by
   name.
2. Halo signals failure three ways: `Result::Err` (the fast helpers,
   `instruction_get_upval`), a `Bool` flag (`ok`, the result of `slow` and
   `raise_text`), and its own enum (`Step::Error | Stop | Budget`). The
   language knows only the first is a failure [ERR-1, ERR-4].
3. `slow` is not a failure. It is the general, valid path for non-number
   operands (metamethods, string coercion). That it is rare in fib is a
   property of the Lua program being run, not of Halo: a Lua program doing
   vector arithmetic with `__add` runs `slow` on every `SubRK`. Whitefoot's
   source states this expectation only in names (`fast_`, `handled`).
4. `slow` re-enters the interpreter (`callback_values` → `dispatch_step` →
   `run`), so it is in `run`'s recursive call-graph component.

### Why LLVM's static heuristics miss it

LLVM's branch-probability analysis predicts from loops (back edges and exits
likely/unlikely), `unreachable` and `noreturn` successors, calls to `cold`
functions, and comparisons of pointers, zero and floats. On Halo's arms:

- **No seeds.** A C or Rust error path usually ends in a `noreturn` call
  (abort, `longjmp`, panic) that LLVM marks cold and propagates back through
  the blocks before it, and since LLVM 17 `FunctionAttrs` marks a function
  `cold` when every path calls a cold function. Whitefoot has no panic and no
  unwinding [ERR-1]; its failure paths end in an ordinary `ret`. The only
  `noreturn` the compiler emits is the resource-exhaustion abort, which is
  not on these paths. LLVM's propagation has nothing to start from.
- **No loop.** The split dispatch turns the interpreter loop into parts
  chained by guaranteed tail calls (compiler/match-dispatch-lowering). LLVM
  sees no loop in any part, so its loop-exit heuristic, which would weight a
  path leaving `run` against one continuing it, cannot apply.
- **Tags and flags.** The remaining branches are `switch` on an enum tag and
  `i1` flags. Switch successors get equal weight. A zero comparison may get
  LLVM's zero heuristic, which says `x == 0` is unlikely; if `num_of`'s `Is`
  tag is 0 and lowers to `icmp eq ... 0`, that heuristic predicts the number
  path unlikely, the wrong way. This is unverified; it can be read in
  `B.precodegen.ll` from the probe's artifact.

So every branch around the slow calls is roughly 50/50 to the register
allocator, and placing the spills in the entry block looks as cheap as
placing them on the slow path. A Whitefoot construct can carry the missing
weight directly: every conditional in the emitted module is the one
`switch` written for `IrTerminator::Match` in `compiler/src/backend/emitter.rs`
(a Bool `if` is a Match on `i1`), so a `!prof !{!"branch_weights", ...}` on
that `switch` reaches every branch, and branch weights survive host inlining
scaled into the caller. `llvm.expect` is only a front-end spelling that LLVM
lowers to the same metadata; emitting `!prof` directly is simpler. Branch
weights carry no correctness contract: a wrong weight changes code placement,
never behavior. They are therefore not a "fact" under compiler/backend-facts,
whose first decision governs attributes and metadata that promise something,
and they need their own node.

### Approaches

**S1. Result failure edges (structural, grounded in ERR-4).** In each
function, an edge is unlikely when it enters the `Err` arm of a `match` on a
`Result` (an ERR-3 propagation edge included), or when every path from its
target reaches a `return` whose value is constructed as `Err` of the
function's `Result` result (a backward pass over the IR's blocks, no value
reasoning). The weight is LLVM's own `__builtin_expect` ratio, 1:2000. Ground:
ERR-4 classifies `Err` as the expected-failure outcome of an operation, and
ERR-1 removes the panic seeds C and Rust error paths carry; S1 restores in
Whitefoot's terms the seed the language removed.

- Halo's four: catches `unwind` and `restore_stopped_stack` in
  `instruction_get_upval`, `raise_text` (hence its `heap.intern` call) on the
  `next >= n` paths of the fast helpers and `instruction_get_upval`, and the
  arms' own `Err(error: failure)` branches. It misses `slow` and every call
  inside `instruction_sub_rk` / `instruction_lt_jmp_rk`, which return `Step`,
  not `Result`; and it misses `raise_text` in `None()` of `upvalue_handle`
  until LLVM threads the constant `ok` (then LLVM's backward propagation of
  unlikely weights covers it).
- Risk: a program that uses `Err` for a common outcome, such as a polling
  `try_pop` returning `Err` on empty, gets that path laid out cold. The cost
  is the misplacement V2 removed, in reverse, on that one branch; behavior is
  unchanged.
- Generality: any Whitefoot program with `Result` error paths: Firn's error
  replies, v2h's traps, Snowghost's parse failures.
- Size: one analysis over `IrFunction` blocks (about 150 lines) and the
  metadata on the existing `switch` emission (about 60 lines), plus tests;
  a new design node (compiler/branch-weights). No spec change.

**S2. Restored loop exits under the split (structural).** In a split
dispatch loop, an edge is unlikely when every path from its target leaves the
loop (returns from the enclosing function) without reaching the header: the
weight LLVM's loop-exit heuristic would have given the unsplit loop. Ground:
the split is the compiler's own transformation and should not remove what the
whole-function form told the host.

- Halo's four: none directly. In Halo it weights only the join's `Err` arm
  after `checked_step`, since the arms' failure paths pass through the join,
  whose `Ok` side continues.
- Risk: none beyond the unsplit form's; it is LLVM's own prediction.
- Size: small, inside `compiler/src/backend/emitter/dispatch.rs`, which
  already classifies the loop's blocks. No spec change.

**S3. The call heuristic (structural, Ball and Larus).** At a two-way branch
where one successor region reaches the loop's back edge or the function's
return without a call and the other contains a call and does not
post-dominate the branch, the call side is unlikely; ties get no weight;
branches LLVM will still see as loop branches get no weight. This is one of
Ball and Larus's static heuristics; GCC's predictor and Go's
`likelyadjust` implement versions of it, while LLVM does not. At the
Whitefoot level it applies before host inlining.

- Halo's four: all of them in the three arms. In `SubRK` and `LtJmpRK` the
  `cursor.handled` side `continue`s with no call, the other side calls
  `instruction_sub_rk` / `instruction_lt_jmp_rk`, so the whole inlined slow
  helper (`slow`, `unwind`, `restore_stopped_stack`, `raise_text`) lands in
  an unlikely region. In `instruction_get_upval` `Some` (calls `read_upvalue`)
  against `None` (calls `raise_text`) is a tie and gets nothing, while
  `bnot(ok)` and `next >= n` get weights, as under S1.
- Risk: it is a guess with no semantic ground. Its typical failure is a
  branch whose call side is the common one, such as `if is_table { get(...) }
  else { return nil }` on table-heavy input, and a call to a tiny helper that
  the host inlines counts as a call because Whitefoot emits before inlining.
  A wrong weight puts spills and block layout on the hot side. Halo's
  `concat`, `string-key` and `sort` kernels and v2h exercise exactly such
  branches, so the experiment must include them.
- Generality: any program; interpreters with a fast/slow split benefit most.
- Size: about 100 lines beside S1's analysis (the call-containing regions
  and a post-dominance test). No spec change.

**S4. Re-entrant callees (structural), rejected.** Mark a call from a loop
body cold when the callee reaches the loop's own function (`slow` →
`callback_values` → `run`). Rejected: a tree-walking interpreter re-enters
its `eval` on every node, and a bytecode interpreter's call opcode may
re-enter on every native call, so the rule marks the hottest calls cold. It
also leaves `unwind` and `raise_text`, which do not re-enter, unmarked.

**S5. Callee-level propagation.** Downward: a function every one of whose
call sites lies in an unlikely region gets the `cold` attribute (section
placement, smaller inlining budget at its sites). Upward: LLVM's
`FunctionAttrs` already does it once seeds exist. S5 adds nothing to spill
placement, which V2's effect is, beyond what S1 to S3's weights already give
the caller, and it would never mark `heap.intern` (hot sites elsewhere),
which is correct. It is an optional follow-up, not a candidate on its own.

**P. Profile-guided optimization.** Instrument the emitted modules
(`-fprofile-generate`), run a training workload, rebuild with
`-fprofile-use`. No compiler change is needed to test it: the halo-call-path
workflow already passes options to every clang command through
`CCC_OVERRIDE_OPTIONS`.

- Halo's four: yes for any training run that resembles the measured one; it
  sees actual counts, including that `slow` is rare in fib, which no static
  rule can know.
- Risk: the profile is the workload. Trained on fib it would lay out
  `slow` cold for a metamethod-heavy Lua program. It is also a second build
  and a training run per project, a build input that the cache identity must
  include (compiler/build-inputs), and hard to collect on cross-compiled
  embedded targets.
- Generality: universal, and the upper bound on what weights can buy.
- Against this project's goals: it hides rather than finds a gap. A Halo
  built with PGO against PUC Lua 5.1.5 built without it is not a like-for-like
  comparison, and the result would not say whether the language or the
  compiler can express the expectation. It belongs as an experiment ceiling
  and an optional build mode, not as the answer.
- No spec change.

**H. A writer's frequency hint (language change).** The proposal, stated so
the owner can rule on it, not adopted here:

```
rare fn slow<interface Host<E>>(code: &Box<Slots<Cell>>, ...) -> r: Bool ... {
  ...
}
```

Semantics: `rare` before `fn` states the writer's expectation that a call to
this function executes much less often than the code around its call sites.
It changes no judgment, no acceptance verdict and no observable behavior; it
is not a fact source [SCOPE-2]; an implementation may use it only for code
placement (branch weights on the paths to its calls, the callee's own
placement). Halo would write it on `slow`, `raise_text`, `unwind` and
`restore_stopped_stack` (not on `heap.intern`).

- Halo's four: yes, by the writer's word; it is the information fib's
  coldness of `slow` actually rests on.
- Risk: an unchecked claim. A wrong hint costs what a wrong S3 weight costs,
  and nothing reports it; a profile could audit hints later, but that is
  more machinery.
- Fit with the constitution: it serves "the language must provide the
  capabilities needed to meet its target projects' performance
  requirements", and the dispatch decision's objection to a source marker
  (acceptance would depend on the host) does not apply, since `rare` decides
  nothing. It conflicts with AGENTS.md's rule that surface names label checked
  invariants: `rare` labels an expectation that no checker can verify. The
  only precedent for unchecked surface text is `doc`, which labels
  documentation and claims nothing about execution. `cold` itself is
  excluded by the same paragraph's ban on backend terms naming lowering
  consequences. Adopting H therefore needs the owner to admit a class of
  non-judgment hint items, or it is rejected.

Minimal semantic witness of the gap H would close, apart from Halo:

```
enum Num { Is(x: f64), Not() }

fn sub(a: Value, b: Value) -> r: Value {
  match num_of(v: a) {
    Is(x: n) => { ... return Value::Num(n: d); }
    Not() => { return general_sub(a: a, b: b); }
  }
}
```

Both arms are total, neither is a failure, both return the same type. Which
one runs more often depends only on the values the program receives. No
type, effect, contract or proof distinguishes them, so no rule grounded in
the checked program can rank them; only a guess (S3), a measurement (P) or
the writer's statement (H) can.

### Recommendation

Build S1 with S2 now: both are grounded (ERR-4's failure classification and
the split's own loop), cheap, need no spec change and cannot mark a
non-failure path cold by design. Measure S3 on top of it as the general
candidate for valid-but-rare paths such as `slow`, and keep it only if it
passes the harm criteria below. Do not adopt PGO as the answer; run it once
as the ceiling. Bring H to the owner only if the experiment shows that the
part of V2's gain S1 cannot reach is large and S3 fails its harm criteria;
that card's witness is the one above.

### Experiments, criteria written before measuring

All on the halo-call-path workflow (hosted x86-64, the pinned LLVM, Halo-wf
`c900a68b2f06`, full LTO, callgrind), instructions per Lua call or per
kernel; the 14900K for any timing, with the base's twin as noise control.

**E1. Which of the four carries the gain (no compiler change).** Add
variants to `probe/variants.py` on B's post-LTO module: V2s (`slow` cold
only), V2e (`unwind` and `restore_stopped_stack` cold only, the calls S1
mostly reaches), V2i (`heap.intern` cold only). Prediction: V2s gives at
least 60% of V2's 35.5 instructions per Lua call (at most 820.2), V2e plus
V2i together at most 40%. If V2e alone reaches 80% of V2 (at most 813.1),
S3 and H are not needed for Halo's fib and E2 stops after S1. Also run V2
on `concat` and `string-key`: if V2 raises either kernel's instruction count
by more than 1%, function-level `cold` on `heap.intern` is confirmed harmful
and stays excluded from every rule. (V2e marks `unwind` cold at the
`Step`-returning sites too, which S1 would not reach; E1 bounds S1 from
above.)

**E2. The rules (experiment branch, compiler change).** C1 = B + S1 + S2;
C2 = C1 + S3. Workloads: fib(32) per Lua call, and every kernel of Halo's
bench set (binary-trees, concat, fib, integer-table, loop, sort, string-key)
in total instructions; v2h CoreMark through `compute-regression.yml`.
Rejection, per change:

- C1 is rejected if fib(32) falls by fewer than 5 instructions per Lua call
  against B (841.53), or any kernel's count rises by more than 1%.
- C2 is rejected if fib(32) is above 814.9 (less than 75% of V2's 35.5
  gain), or any Halo kernel or v2h's CoreMark instruction count rises by more
  than 1% against C1.
- A change passing both then gets 14900K timing on fib and CoreMark,
  interleaved against B with a twin of B; it is rejected if it is not faster
  than B beyond the twin's spread on fib, or slower beyond it on CoreMark.

**E3. Profile ceiling (no compiler change).** B built with
`-fprofile-generate`, trained (a) on fib(32) and (b) on the six other
kernels, then rebuilt with `-fprofile-use`; measured on fib(32). Prediction:
(a) at or below V2's 806.03. If (b) recovers less than half of V2's gain
(above 823.8), a profile does not transfer across Lua workloads, which
confirms that `slow`'s coldness is a property of the input and that P is not
a default build mode. E3 decides nothing about S1 to S3; it calibrates how
far they are from what counts can reach.

### What this study does not settle

- Whether branch weights on the paths to a call reproduce what the callee's
  `cold` attribute did in V2. Both feed the same block frequencies, but V2
  also shrank inlining budgets at those sites; E2 measures the weights
  themselves.
- The direction of LLVM's zero heuristic on `num_of`'s tag test (above).
- Whether `prepare`, called on every Lua call, reaches `run`; S4's rejection
  does not depend on it.
- Whether the 1:2000 weight is too strong for S3's guesses; a weaker ratio
  is the first thing to try if C2 fails only on harm.

### Result: E1 and E3, run 38338456381

Instructions per Lua call on fib(32) (callgrind, hosted ubuntu-24.04,
artifact `halo-probe`); harm kernels as whole-run instruction totals at the
reduced sizes. V0 reproduced 841.53.

| Variant | Three-arm sum | Program total | concat vs V0 | string-key vs V0 |
|---|---|---|---|---|
| V0 | 174 | 841.53 | — | — |
| V2 (all four cold) | 151 | 806.03 | −2.68% | −4.84% |
| V2s (`slow` only) | 156 | 820.53 | −1.20% | −3.63% |
| V2e (`unwind`, `restore_stopped_stack`) | 177 | 844.53 | −0.11% | +0.00% |
| V2i (`heap.intern` only) | 163 | 822.03 | −1.04% | −0.69% |
| PGOa (profile from fib) | 118.58 | 523.61 | — | — |
| PGOb (profile from the six other kernels) | 436.00 | 626.52 | — | — |

Against the criteria written before the run:

- V2s ≤ 820.2: fails by 0.33 (820.53). `slow` carries 21.0 of V2's 35.5,
  59%, just under the 60% predicted.
- V2e ≤ 813.1: fails (844.53, +3.0). Marking the error-path callees cold
  gains nothing on fib, so S1 alone, which reaches only those, cannot meet
  the item's 25-instruction condition; S3 (or H) stays in play.
- `heap.intern` harm: not harmful. V2 lowers both kernels (concat −2.7%,
  string-key −4.8%), and V2i alone lowers them too, so the study's warning
  that marking `heap.intern` cold would slow concatenation and string keys
  is not borne out at function level on these kernels.
- E3 (a) ≤ 806.03: holds by far (523.61, −37.8%).
- E3 (b) above 823.8 would show profiles do not transfer: not confirmed.
  A profile trained only on the other six kernels gives 626.52 (−25.6%),
  so most of the gain transfers across Lua workloads on this interpreter.

Reading: on fib, profile-guided optimization removes several times what
any cold marking does, and much of it survives training on different
programs. The arm table shows PGO rearranges the dispatch code wholesale
(SubRK and Call fold into their neighbours; LtJmpRK drops to 24), so its
gain is not the spill placement this item addresses alone. The study's
objections to PGO as a default stand (a build input, a training set, and an
unfair comparison against PUC Lua built without one); whether an opt-in
profile-guided build mode is worth offering is a separate question for the
owner. For this item, the next step is E2: S1 + S2 + S3 in the compiler.
