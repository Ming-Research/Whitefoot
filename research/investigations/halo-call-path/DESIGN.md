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

The pre-written arm criterion is met (−37 against −10), but the program executes
36.7% more instructions: every call a part makes now crosses a wrapper, including the
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

### Result: E2 instruction counts and 14900K timing

The [instruction-count run](https://github.com/Ming-Research/Whitefoot/actions/runs/38352516256)
used base compiler `c28b0145e777fa6159ba7134fe1d9591c24466b1` (the
merge-base with main), and C1 and C2 compiler
`b385c5c03b219d0f37218f169cb4caa30c45b851`, selecting S1+S2 and
S1+S2+S3 respectively. Its manifest records main
`b9c6235e340ba127260e67089dc87dbff7b00362`, Halo
`c900a68b2f06af0de2adccfd0dd348d49af184de`, the historical probe
`14eb8eb181fd5fa57cb9f13382ad31bf858c9ae4` (its `probe/measure.py`),
silverfir `5f248e4420102356be7edadbdb4db2ae105d3270` for v2h, and
CoreMark input blob `70b4d2165e7fbbd797e47b43032fc06302c1774f`.
The hosted x86-64 run used LLVM 22, Halo full LTO and uncached builds;
v2h compiled directly at the default optimizing level, with separate fresh
caches. The run's `table.md`, `manifest.txt` and `probe/` artifact supply
the counts and disassembly quoted here. Executable self instructions per
Lua call exclude other process instructions; harm comparisons use whole
process callgrind Ir. fib(32) printed 2178309 and made 7,049,155 Lua calls.

| Executed instructions per Lua call | Base | C1 | C2 |
|---|---:|---:|---:|
| fib executable self, approximate headline | 841.6 | 825.6 | 752.1 |
| fib executable self, artifact precision | 841.525130 | 825.525150 | 752.026399 |
| SubRK self | 59.999991 | 49.999993 | 48.999993 |
| LtJmpRK self | 53.000000 | 55.000000 | 55.000000 |
| GetUpval self | 60.999991 | 59.999991 | 59.999991 |
| Three-arm self sum | 173.999983 | 164.999984 | 163.999985 |
| Call including ordinary callees | 436.002442 | 433.002441 | 362.002499 |

The headline figures are approximations; the criteria use the artifact
precision. C1 saves 15.999980 instructions per Lua call against the measured
base, exceeding its minimum saving of 5. C2 saves 89.498731 against base
(and 73.498751 against C1), below its ceiling of 814.9 instructions per
Lua call and exceeding the item's minimum saving of 25. The measured base
is not the historical B compiler, but its 841.525130 is 0.004870 below
historical B's 841.53. Translating the published thresholds by that
baseline difference gives 836.525130 for C1 and 814.895130 for C2; both
still pass. The written criteria above remain unchanged.

| Other Halo kernel (input size) | C1 whole-process Ir vs base | C2 whole-process Ir vs C1 |
|---|---:|---:|
| binary-trees (12) | −0.305961% | −0.383889% |
| concat (200000) | −1.688624% | +0.446522% |
| integer-table (1000000) | +0.582242% | −1.301094% |
| loop (5000000) | +0.693972% | −2.065575% |
| sort (100000) | −0.328093% | −0.194585% |
| string-key (200000) | −3.284816% | −0.883528% |
| v2h CoreMark (200 iterations) | +0.144406% | +0.050732% |

fib's whole-process changes are −1.901125% for C1 against base and
−8.902442% for C2 against C1. The worst increase among the six other Halo
kernels is loop's +0.693972% for C1 against base and concat's +0.446522%
for C2 against C1, both below the 1% harm bound. CoreMark rises 0.14% for
C1 against base and 0.05% for C2 against C1, also below 1%, **at 200
iterations only**. The attempted 2000-iteration callgrind run
[38350625447](https://github.com/Ming-Research/Whitefoot/actions/runs/38350625447)
on the 16 GB hosted runner already held 20,303,183,872 anonymous mmap
bytes (over 20 GB) and failed to acquire another 29,360,132,096 bytes.
The 200-iteration result does not validate the full 2000-iteration
instruction-count harm criterion. Native timing below uses 2000 iterations.

#### Arm-entry evidence

All three arms execute no ordinary callee on fib's measured fast path;
their self and inclusive counts agree. The artifact's automatic stack-store
counter reports zero even for base's visible stores, the same instrument
limitation noted in the earlier probe. The entry-store counts below come
from reading the disassembly, before its first conditional jump; entry
instruction counts exclude that jump as the probe does. A frame or a store
at entry executes on every arm invocation, including the measured fast path.

| Arm | Base → C2 executed self instructions per Lua call | Base → C2 entry instructions | Base → C2 entry stack stores (excluding push) | Base → C2 frame allocation after push |
|---|---:|---:|---:|---|
| SubRK | 60 → 49 | 32 → 14 | 9 → 5 | 0xd0 → 0x80 bytes |
| LtJmpRK | 53 → 55 | 26 → 27 | 6 → 6 | 0xc0 → 0xc0 bytes |
| GetUpval | 61 → 60 | 9 → 8 | 2 → 0 | 0x40 → 0x80 bytes |

In `base.arm-SubRK.txt`, entry `5e040: push rbp` and
`5e041: sub rsp,0xd0` build a frame. Of nine entry stores, four slots are
read or passed by address only on the unexecuted paths: `5e058: mov QWORD
PTR [rsp+0x60],r8` (address at `5e206` and `5e2e0`, read at `5e588`),
`5e0c3: mov QWORD PTR [rsp+0x68],r8` (read at `5e592`),
`5e0be: mov QWORD PTR [rsp+0x70],r11` (read at `5e5c2`), and
`5e0b1: mov QWORD PTR [rsp+0x78],rbx` (read at `5e5e4`). Those consumers
all count 0.000 executions per Lua call. In `C2.arm-SubRK.txt`,
`5f980: push rbp; 5f981: sub rsp,0x80` still builds a frame; five entry
stores remain. One is still slow-path-only: `5f996: mov QWORD PTR
[rsp+0x28],r8`, whose address is passed at `5fa7a: lea r8,[rsp+0x28]`
and read at `5fabc: mov rsi,QWORD PTR [rsp+0x28]`, both counting 0.000.
Other entry spills have fast-path reads, for example the `r11` store at
`5f988` is reloaded at `5fa54`, both counting 1.000. **SubRK does not meet
the no-entry-preparation condition**, despite saving 11 executed instructions
per Lua call.

In `base.arm-LtJmpRK.txt`, `65240: push rbp; 65241: sub rsp,0xc0`
builds a frame. Two of six entry stores have only slow-path consumers:
`65255: mov QWORD PTR [rsp+0x58],r8` (address at `65412`, read at
`656a7`) and `652a7: mov QWORD PTR [rsp+0x78],rsi` (read at `656f6`),
all consumers counting 0.000. In `C2.arm-LtJmpRK.txt`,
`66680: push rbp; 66681: sub rsp,0xc0` remains, with six entry stores.
`66693: mov QWORD PTR [rsp+0x18],r8` still prepares a slow-path-only
slot: reads at `667d1`, `668c2`, `66ad7`, and address formation at
`66869: lea rdi,[rsp+0x18]` all count 0.000. The new byte spill
`6669d: mov BYTE PTR [rsp+0x7],al` is **not** slow-path-only:
`66701: cmp BYTE PTR [rsp+0x7],0x0` executes 1.000 times per Lua call.
**LtJmpRK does not meet the condition**; its executed count rises by 2.

In `base.arm-GetUpval.txt`, `58980: push rbp; 58981: sub rsp,0x40`
builds a frame. Both entry stores serve the fast path:
`58985: mov QWORD PTR [rsp+0x8],r11` and
`58990: mov QWORD PTR [rsp],rdi` reload at `58b63` and `58b5c`,
respectively, all counting 1.000. No slow-path-only entry store is visible.
`C2.arm-GetUpval.txt` has no entry stack store; stores
`5b886: mov QWORD PTR [rsp],r10` and
`5b88a: mov QWORD PTR [rsp+0x8],r11` now lie after the error branch
and count 0.000. But `5b800: push rbp; 5b801: sub rsp,0x80`
still builds a frame on the hot path, twice base's allocation.
**GetUpval removes entry stores but does not meet the complete condition**:
frame preparation remains, and it saves only 1 executed instruction per
Lua call. None of the three arms establishes the separate completion
condition of no frame or slow-path-only preparation at entry. These verdicts
are for these images and fib paths, not a claim about every input or target.

#### 14900K wall time and selection

The [14900K timing run](https://github.com/Ming-Research/Whitefoot/actions/runs/38366457857)
used base compiler `c28b0145e777fa6159ba7134fe1d9591c24466b1` and
C2 compiler `0793e49124c16e6d19bed5109168f9508766fc37`; its manifest
records main `ad8ea57f7cbb0bfd099c9924f7b52639e172e04f`. Halo,
silverfir and the CoreMark input blob are the same revisions as above.
This candidate revision differs from the instruction-count run; these are
results for the two recorded revisions, not measurements of the final
review-fix revision. The timing manifest identifies Ubuntu x86-64 on
MBSDESKTOP and clang 22.1.8. The run pinned execution to logical CPU 2,
used one warm-up round and seven interleaved B/twin/C2 rounds, rotating
order each round. The twin is a byte copy of B's images. fib(32) validated
output 2178309; native v2h CoreMark used 2000 iterations and validated CRC.
The run's timing summary and manifest supply these figures; displayed
seconds are rounded, while percentages use the underlying measurements.

| Workload | Image | Median wall s | Min wall s | Max wall s | Measured rounds |
|---|---|---:|---:|---:|---:|
| fib(32) | B | 0.3032 | 0.3028 | 0.3034 | 7 |
| fib(32) | twin | 0.3032 | 0.3028 | 0.3043 | 7 |
| fib(32) | C2 | 0.3031 | 0.3028 | 0.3034 | 7 |
| CoreMark (2000) | B | 0.4025 | 0.4024 | 0.4032 | 7 |
| CoreMark (2000) | twin | 0.4027 | 0.4025 | 0.4031 | 7 |
| CoreMark (2000) | C2 | 0.4026 | 0.4025 | 0.4031 | 7 |

| Workload | Absolute twin/B median difference / B median | B min–max range / B median | C2/B median ratio | C2 wall-time change vs B | Literal criterion |
|---|---:|---:|---:|---:|---|
| fib(32) | 0.001% | 0.188% | 0.99958 | 0.042% less | Pass |
| CoreMark (2000) | 0.062% | 0.188% | 1.00046 | 0.046% more | Pass |

By the run's literal definition of the twin's spread (the absolute
difference between its median and B's median), fib passes: 0.042% is
greater than 0.001%. CoreMark's 0.046% increase is below the twin's
0.062% median difference, so it passes too. That median difference is too
narrow a noise measure: two medians can agree while the individual runs
vary more. Base's own min–max range is 0.188%, about 0.19%, for both
workloads, larger than either effect. The wall-time changes are not
distinguishable from noise on this run; it establishes no wall-time gain
from the instruction reduction.

On 2026-10-13 the owner selected **C2, all three rules**, to keep and
finish despite this wall-time result, on the instruction-count evidence.
This settles policy selection, without asserting that the full
2000-iteration instruction-count criterion or the arm-entry condition met.
Before merge, one `perf stat` run on the 14900K must explain why fewer
instructions did not shorten wall time; that explanation is pending.
Reopen the policy if a measurement on any target shows wall-time harm
beyond its measured noise, the C2/C1 instruction harm exceeds 1% on a
Halo kernel or v2h CoreMark, or the pending explanation contradicts the
instruction-reduction grounds. The evidence here covers the listed
workloads, sizes, compiler revisions and x86-64 settings only.

#### Why the arm entries still prepare

This is a static attribution of the supplied E2 artifacts, not a new
code-generation or timing result. Artifact line numbers below refer to
`ir/C2.ll`, `probe/*.json` and `probe/*.arm-*.txt` from the
[E2 instruction-count run](https://github.com/Ming-Research/Whitefoot/actions/runs/38352516256),
as supplied for this investigation. The scratch copies are not repository
inputs. The three functions have the common symbol prefix
`wf_package.halo.vm.run$instance$2ead40aebe81bc32.body.arm.` and suffixes
`25` (SubRK), `54` (LtJmpRK) and `5` (GetUpval).

**Evidence boundary.** Despite its description as optimized pre-codegen IR,
the supplied `ir/C2.ll` is an earlier module: it still calls
`fast_instruction_sub_rk` at line 163468 and the always-inlined dispatch at
163549, while the measured SubRK machine path contains neither call.
GetUpval likewise still calls `instruction_get_upval` at 159996. Its only
numbered function attribute group is `"probe-stack"="inline-asm"`
(198886). These bytes show what Whitefoot handed to LLVM, not which
metadata survived full LTO. A true post-LTO module and machine-pass dump
are missing. Consequently, neither “LLVM ignored the right weights” nor
“eleven live states make every remaining spill unavoidable” follows from
these artifacts.

**Where the other instructions went.** Positive savings below mean base
minus C2, in executed instructions per Lua call. JSON lines 5, 11, 23,
35, 47–51 and 61–65 in each variant give these totals. The helper rows
are inclusive contributions charged to the indicated caller, not helper
self counts; they are added to arm self counts exactly once.

| Disjoint contribution | Base | C1 | C2 | Base − C2 |
|---|---:|---:|---:|---:|
| SubRK self | 59.999991 | 49.999993 | 48.999993 | 10.999998 |
| LtJmpRK self | 53.000000 | 55.000000 | 55.000000 | −2.000000 |
| GetUpval self | 60.999991 | 59.999991 | 59.999991 | 1.000000 |
| Call self | 101.000014 | 98.000014 | 100.000014 | 1.000000 |
| `prepare` subtree called by Call | 335.002427 | 335.002427 | 262.002485 | 72.999943 |
| Return self | 112.000015 | 109.000015 | 109.000015 | 3.000000 |
| `finish` subtree called by Return | 93.000006 | 93.000006 | 93.000006 | 0.000000 |
| Remaining executable self, by subtraction | 26.522684 | 25.522703 | 24.023894 | 2.498790 |
| Total | 841.525130 | 825.525150 | 752.026399 | 89.498731 |

Thus the saving outside the three hot arms is **79.498733**: about 73
in the `prepare` subtree, 1 in Call itself, 3 in Return itself and
2.498790 elsewhere. C1 leaves the `prepare` contribution unchanged;
C2 accounts for its entire reduction. Call's own code actually costs
2 more instructions than C1. The JSON contains only these five arms and
their listed callees: it cannot identify the remaining arms or divide
`prepare` between its own code and nested helpers. Naming that residual,
or calling the 73 a `prepare` self saving, would invent evidence. A raw
per-function callgrind self table is needed to finish that decomposition.

The Call disassembly also locates a small gain outside entry preparation:
base's five-instruction normal-result jump-table dispatch at lines 89–93
becomes `test al,al; jne` in C2 at 108–109. The ordinary `prepare` call
still executes 1.000 times per Lua call (base line 79, C2 line 94).
This explains a local three-instruction saving, not the entire net Call
delta: C2's frame grows from 0x60 to 0x70 and its allocation changes too.

**Every C2 entry store.** The table gives store address → hot read address,
with line numbers in the corresponding annotated disassembly. Counts
rounded to 1.000 identify the measured fast path. A displayed 0.000
cannot by itself rule out rare executions rounded away; the classifications
also follow the numeric-success CFG. They are not claims that the other
paths are unreachable for other Lua inputs.

| Arm and entry slot | Evidence | Classification |
|---|---|---|
| SubRK `[rsp+0x20] = r11` | `5f988` → `5fa54`, lines 5, 47 | Hot save/reload; `r11d` is overwritten at `5f9a0` for an operand. |
| SubRK `[rsp] = r15` | `5f98d` → `5fa0f`, lines 6, 33 | Hot save/reload; `r15` becomes the constant address at `5f9d2`. |
| SubRK `[rsp+0x18] = r12` | `5f991` → `5fa4c`, lines 7, 45 | Hot save/reload; `r12` becomes the constant storage pointer at `5f9ca`. |
| SubRK `[rsp+0x28] = r8` | `5f996`, line 8; only address/read at `5fa7a`, `5fabc`, lines 57, 78 | Slow-only addressable pinned-pointer slot. |
| SubRK `[rsp+0x10] = r14` | `5f9ae` → `5fa28`, lines 13, 38 | Hot read, but no intervening hot definition of `r14`; this pair is redundant on the measured fast trace. |
| LtJmpRK `[rsp+0x70] = rdi` | `6668e` → `66749`, lines 7, 50 | Hot save/reload; `edi` is overwritten by the jump target at `666ab`. |
| LtJmpRK `[rsp+0x18] = r8` | `66693`, line 8; consumers `667d1`, `66869`, `668c2`, `66ad7` all count zero | Slow-only addressable pinned-pointer slot. |
| LtJmpRK `[rsp+0x7] = al` | `6669d` → `66701`, lines 10, 35 | Hot operand byte; `eax` is overwritten at `666a1`. |
| LtJmpRK `[rsp+0x68] = rcx` | `666b2` → `66754`, lines 15, 53 | Hot save/reload; `rcx` is reused for an address and the comparison. |
| LtJmpRK `[rsp+0x8] = r13` | `666ba` → `6673f`, lines 17, 48 | Hot read with no intervening hot definition of `r13`; redundant on this trace. |
| LtJmpRK `[rsp+0x10] = r14` | `666d5` → `66744`, lines 23, 49 | Hot read with no intervening hot definition of `r14`; redundant on this trace. |
| GetUpval | No entry stack data store, lines 3–11 | Later spills at `5b886`/`5b88a`, lines 36–37, execute only on the error path. |

The three pairs that reload an unchanged register demonstrate why a hot
consumer alone is insufficient evidence of necessary register pressure.
Cold clobbers and joined register-allocation regions can affect the hot
allocation. The other saves protect actual hot clobbers in this allocation,
but that still is not a lower-bound proof that another allocation must spill.

The two slow-only stores have a more direct cause than register pressure.
SubRK's input IR explicitly emits
`%wf.pin.8 = alloca ptr; store ptr %v25, ptr %wf.pin.8` at 163439–163440;
LtJmpRK does the same at 167920–167921. The slow calls pass that slot's
address at 163541 and 168027; `checked_step` also consumes it at 163449
and 167930. `%v25` is the hoisted code pointer. The general dispatch
emitter initializes such local reference slots in each part's prelude
([dispatch.rs](../../../compiler/src/backend/emitter/dispatch.rs),
lines 1466–1476). LLVM retained an existing entry initializer after inlining
removed its hot address users; it did not invent these two stores to spill
eleven parameters. GetUpval emits the same initializer at 159970–159971,
but its machine code eliminates the slot, passing the code pointer in
`rbp` directly to `checked_step` (`5b920`, disassembly line 70).

**Frames and shrink-wrapping.** SubRK and LtJmpRK have real hot stack
accesses before their first conditional jump. Their existing frames must
therefore be available at entry for this generated allocation. The full
0x80/0xc0 allocations also accommodate cold temporaries; their sizes are
not a measured minimum for the hot path. Merely sinking the single
slow-only initializer cannot remove their remaining hot saves.

GetUpval's 0x80 local allocation has no hot data access at all. Its cold
slots include the two saved states, interning output, `checked_step`
input/output and `unwind` output (`5b886`–`5b938`, lines 36–76).
However, `push rbp` is not slow-only: the hot code overwrites `rbp` with
the code pointer at `5b808`, reads through it at `5b99a`, and uses it to
form the next instruction and transfer arguments at `5b9cb`/`5b9e1`.
LLVM 22's `preserve_none` still saves RBP:
[X86RegisterInfo.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/X86/X86RegisterInfo.cpp#L257-L258)
selects the list whose sole member is RBP in
[X86CallingConv.td](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/X86/X86CallingConv.td#L1079).
The hot tail goes through `add rsp,0x80; pop rbp` at `5b96e`/`5b975`
(lines 87–89). Separating cold storage could remove the 0x80 adjustment
without necessarily removing this save/restore pair.

Shrink-wrapping moves the save/restore region subject to dominance of
frame and callee-saved-register uses; it does not give each path its own
independently sized frame. These hot uses block moving the whole prologue
cold. The generic algorithm's
[frame/register-use test](https://llvm.org/doxygen/ShrinkWrap_8cpp_source.html)
explains the constraint; LLVM 22's
[x86 enablement](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/X86/X86FrameLowering.cpp#L3627-L3640)
does not categorically exclude this Linux convention. A machine-pass dump
would establish the actual pass decision. Turning shrink-wrapping on is
not evidence that these hot uses cease to constrain it.

**The weights reached the arm boundary, but not uniformly the numeric
tests.** Metadata is inline here: the initial zero is the impossible
default switch edge, followed by the listed cases, not a measured count.

| C2 IR location | Weight and meaning |
|---|---|
| SubRK 163471–163474, 163497–163500 | `0,2000,1`: successful helper result, then successful fast continuation, preferred over error/fallback. The continuation tail is at 163549. |
| LtJmpRK 167955–167958, 167981–167984 | `0,2000,1`: same outer preference; slow helper at 168027, fast tail at 168035. |
| GetUpval 159999–160002, 159983–159986 | `0,2000,1`: success preferred over error, including the copied `checked_step` join. |
| SubRK helper 64627–64630 and 64653–64656 | `0,1,2000`: numeric case **less** likely than non-number. `num_of` returns tag 0 for a number (7655–7663), tag 1 otherwise. |
| LtJmpRK helper 65611–65614 and 65640–65643 | Numeric tag tests have no `!prof`. |
| LtJmpRK helper 65626–65630 | `!numeric_ready` true is weighted 2000, false 1. The true successor returns “not handled” at 65682–65701; the numeric case sets ready=true at 65661. This also points opposite to fib. |
| GetUpval's `upvalue_handle` helper 120234–120237, 120270–120273, 120302–120305 | The three bounds/error tests have no `!prof`; outer success weighting does not directly annotate these tests. |

Therefore increasing every ratio would not necessarily help: it could
strengthen contradictory local predictions. The outer weights are present,
but the supplied IR cannot show how inlining and CFG simplification
reconciled them with the helper weights before register allocation. The
entry initializer and the hot register/frame uses are established causes;
the exact contribution of these conflicting predictions remains unverified.

**General changes that could remove the preparation.** These are candidates,
not adopted compiler decisions or source-language rules.

| Candidate | Expected effect on these arms | Cost, risk and discriminating probe |
|---|---|---|
| Sink local pinned-reference initialization to the paths needing its address, after inlining | Directly targets SubRK's `[rsp+0x28]` and LtJmpRK's `[rsp+0x18]`. GetUpval has already eliminated its pin slot. | Smallest causal test: move only these stores in a copy of true post-LTO IR, before every surviving address consumer, preserving all-path initialization. Compare codegen with its untouched twin. A pre-inlining move is insufficient while the fast helper itself still takes the address; later optimization may hoist the store again. Does not promise frame removal. |
| LLVM IR HotColdSplitting, late after inlining | A genuinely separate function can own cold locals and reduce allocator coupling. Best frame-size candidate is GetUpval; SubRK/LtJmpRK may retain hot spills. | Extra call/return and argument/result traffic on cold paths; parameter or extraction restrictions may reject the region. Count moved allocations and remaining entry stores, not just new `.cold` symbols. |
| Compiler emits a cold continuation as a separate dispatch part | Can place the pin initializer, slow temporaries and copied join wholly in the cold part, leaving hot allocation independent. | Larger general lowering change. Preserve the dispatch signature/convention and `musttail`; pass live values, never addresses into a destroyed hot frame. Splits inside inlined helpers require access to their CFG; splitting only the existing outer fallback leaves helper-internal errors. Keep cold parts from being inlined back. A hand-transformed post-LTO arm tests feasibility before compiler work. |
| Mark selected surviving slow **call sites** `cold` | Could improve frequency/inlining decisions for SubRK's fallback, LtJmpRK's `slow`, and GetUpval's error helpers, without marking every use of string interning cold. | Still one frame, still hot stack/RBP uses. It does not directly sink pin stores, and conflicting explicit weights need inspection. Compare a call-site-only variant separately, preserving calling conventions and recording inlining changes. |
| MachineFunctionSplitter | Moves cold machine blocks into separate text sections; it does not create an independent activation/frame for these arms. | Primarily a code-locality probe, with possible extra jumps. Branch hints alone are not the function profile it normally requires; expect no useful split on this unprofiled module. Do not select it as the frame-removal mechanism. |

LLVM 22's
[HotColdSplitting implementation](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Transforms/IPO/HotColdSplitting.cpp)
uses a default cold probability threshold of 1/100 and a four-parameter
limit, counts inputs and outputs in its profitability penalty, recognizes
cold call sites, and rejects `noinline` source functions. Its static
metadata analysis handles conditional branches, not switches: 1:2000
meets its threshold if it survives onto a conditional branch, but the
supplied switches alone do not establish recognition. Eleven loop states can make a
large extraction unattractive. Its
[CodeExtractor](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Transforms/Utils/CodeExtractor.cpp#L155-L162)
refuses regions containing `musttail`, and its allocation sinking depends
on eligible uses/lifetimes. Extracting only helper calls may leave their
address-taken slots in the hot caller; outputs may introduce new caller
allocations. A successful extraction is therefore not a successful
frame-removal result.

The ordinary LLVM 22
[pass pipeline](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Passes/PassBuilderPipelines.cpp#L2062-L2064)
adds HotColdSplitting late in full LTO only when explicitly enabled; `-O2`
alone does not select it. Whitefoot supplies `-O2` in
[driver.rs](../../../compiler/src/driver.rs) and full LTO in
[whitefootc.rs](../../../compiler/src/bin/whitefootc.rs). Enabling the pass
at the final LTO stage is a plausible small integration if the isolated
probe succeeds; running it only before inlining tests a different boundary.
The [`cold` attribute](https://llvm.org/docs/LangRef.html#function-attributes)
is a frequency hint, not a new calling convention. LLVM 22's
[MachineFunctionSplitter](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/CodeGen/MachineFunctionSplitter.cpp#L121-L132)
requires function profile data except for its optional EH treatment and
groups blocks into sections rather than separate stack activations.

**Next probe and prior criterion (written before execution).** First obtain
the actual C2 post-LTO/pre-codegen module from the same E2 link with saved
temporaries. Retain an untouched twin and all original target/codegen
options. Inspect the resulting numeric branches, `!prof` and computed
block frequencies. Reproduce the recorded arm shapes with the untouched
module before comparing transformations; if it does not reproduce them,
stop attribution to E2 and label the new pipeline a separate experiment.

Then try late HotColdSplitting, with the initializer-sinking variant as the
small causal control. **Success for slow preparation** means both slow-only
pin stores execute zero times on the fast path and GetUpval's cold 0x80
local allocation is absent there; its hot RBP save is counted separately.
**Success for the stronger original arm-entry condition** still means no
entry frame preparation in all three arms. A smaller frame or a `.cold`
symbol alone does not pass either condition. Record each arm independently;
if splitting happens but the stores/allocations remain, reject “enable
this pass alone” as the remedy. If no region is extracted, inspect the
rejection and distinguish missing coldness recognition, profitability
and `musttail`/lifetime legality;
it does not refute explicit cold-part emission. In all variants require
valid IR and unchanged fast, slow, error and resumed-dispatch behavior.

The following commands describe a hosted x86-64 CI probe with LLVM 22.1.8;
none ran on this Mac. Here `C2.precodegen.ll` means the recovered optimized
module, **not** the supplied `ir/C2.ll`. The shown target/PIC/O2 options
must match the recovered link; carry through any additional original CPU,
feature and relocation settings. Run each heavy command under the usual
CI stage wrapper, for example:

```sh
perl .github/run-check.pl cold-split-opt opt-22 \
  -passes='hotcoldsplit,verify' -verify-each \
  -pass-remarks=hotcoldsplit -pass-remarks-missed=hotcoldsplit \
  -S C2.precodegen.ll -o C2.hcs.ll
perl .github/run-check.pl cold-split-base llc-22 \
  -O2 -mtriple=x86_64-unknown-linux-gnu -relocation-model=pic \
  C2.precodegen.ll -o C2.control.s
perl .github/run-check.pl cold-split-codegen llc-22 \
  -O2 -mtriple=x86_64-unknown-linux-gnu -relocation-model=pic \
  C2.hcs.ll -o C2.hcs.s
```

The LLVM 22 [pass registry](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Passes/PassRegistry.def#L87)
names the textual pass `hotcoldsplit`; `-hot-cold-split` is the separate
option enabling it in a default pipeline. Start with this one codegen
pair, not a benchmark matrix. A separately
labelled diagnostic variant can raise `-hotcoldsplit-max-params=16` only
after the default refuses for that reason; do not silently replace the
default result. The layout-only control uses `llc-22
-split-machine-functions` with the same input/options (the LLVM 22
[command flag](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/CodeGen/CommandFlags.cpp)
is `-split-machine-functions`, not `-enable-machine-function-splitter`).
An emitted-module-only exploratory probe can use
`opt-22 -passes='default<O2>,hotcoldsplit,verify' -S ir/C2.ll -o C2.hcs.ll`,
but lacks full-LTO runtime context and cannot establish an E2 delta.

If the assembly criterion passes, link identically and use a short
correctness/count pilot before fib(32). Preserve the original stronger
condition separately; for candidate adoption, also require total fib
instructions to fall against its reproduced C2 control and no greater
than 1% instruction increase on the other six Halo kernels or v2h
CoreMark at the declared input size. Any wall-time claim still requires
the 14900K interleaved base/twin protocol; none follows from a smaller
frame or fewer instructions alone.

**Confidence.** High (4/5) in the classified stores, hot RBP constraint,
pin-initializer origin and the 73-instruction `prepare`-subtree attribution.
Moderate (3/5) that outlining plus moving cold-owned initialization will
remove the avoidable preparation without new hot spills. The missing
post-LTO metadata/MIR, extraction legality and residual function counts
prevent a stronger causal or performance claim. No compiler, specification,
design-tree or test change is selected by this subsection.

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
owner. E2 above records the subsequent S1 + S2 + S3 compiler results and the
owner's selection of C2; it does not select a profile-guided build mode.

### Result: S3 against a call-free return, and the rule restricted to a loop continuation

Question. S3 as first selected (compiler revision 9e5b6689f) marks the
call-containing side of a two-way branch unlikely against any side that
returns without a call. R1 (revision a3f254c) marks it only inside a split
dispatch loop and only against a side that reaches the restored header
without a call on every path. Does S3 slow the ordinary `HashMap` removal,
and what does R1 keep of S3's gain on Halo's `fib`? The removal slowdown was
first seen in a hosted remove-then-insert timing and then explained with the
inlining record; no criterion was written before those runs, so the S3
findings below are exploratory. S3 counts as unfit as a general rule if
`hash_map_find` is no longer inlined into the removal, or remove-then-insert
is slower than the unweighted compiler beyond the twin's noise.

**Inlining record** ([run 40052494556](https://github.com/Ming-Research/Whitefoot/actions/runs/40052494556),
`-Rpass=inline`, `hash_map_find` into `hash_map_remove`, cost 155). The
unweighted compiler and R1 inline it at threshold 225; S3 declines at
threshold 45, the smaller budget the host gives a call site it believes
rarely runs. Machine code agrees: with S3, `wf_remove_one` and
`hash_map_remove` keep a call to `hash_map_find`; with the unweighted
compiler and R1 they do not. S3's weight comes from `if before == 0 { return
None }`, a call-free return on an empty map, set against the continuation
that calls `find`.

**Remove-then-insert on hosted runners** (descriptive, pinned to one
CPU; median over rounds of candidate/base nanoseconds per operation, with the
twin of base as control; a cell is judged a regression only when its 95%
interval lies wholly above 1 and the twin's interval holds 1).

| Run | CPU | S3 at 1,000 | S3 at 10,000 | R1 at 1,000 | R1 at 10,000 |
|---|---|---:|---:|---:|---:|
| [40034526904](https://github.com/Ming-Research/Whitefoot/actions/runs/40034526904) | AMD EPYC 7763 | 1.0283 | 1.0350 | 0.9928 | 0.9946 |
| [40049776146](https://github.com/Ming-Research/Whitefoot/actions/runs/40049776146) | AMD EPYC 9V45 | 1.0001 | 0.9962 | 0.9813 | 0.9765 |
| [40050822548](https://github.com/Ming-Research/Whitefoot/actions/runs/40050822548) | AMD EPYC 7763 | 1.0309 | 1.0347 | 0.9974 | 0.9916 |
| [40051610729](https://github.com/Ming-Research/Whitefoot/actions/runs/40051610729) | AMD EPYC 9V74 | 1.0075 | 0.9939 | 0.9695 | 0.9844 |
| [40052494556](https://github.com/Ming-Research/Whitefoot/actions/runs/40052494556) | Intel Xeon Platinum 8370C | 1.0305 | 1.0370 | 0.9947 | 0.9919 |
| [40094855753](https://github.com/Ming-Research/Whitefoot/actions/runs/40094855753) | Intel Xeon 6973P-C | 1.0146 | 1.0112 | 0.9984 | 0.9950 |

The compiler code of R1 is the same in all six runs (`compiler/` does not
differ between revisions 83f096a, 1e9a782 and a3f254c). S3 is slower on
three of the five CPU models (1.1% to 3.7%) and unchanged on the two newer
EPYC models; R1 is never slower on this cell. The criterion holds for S3.

The same runs show `remove` at 10,000 elements (candidate/base) moving the
other way for R1:

| CPU | S3 | R1 |
|---|---:|---:|
| AMD EPYC 7763 (two runs) | 1.0110, 1.0113 | 0.9763, 0.9800 |
| AMD EPYC 9V74 | 1.1106 | 1.2294 [1.2008, 1.2412] |
| AMD EPYC 9V45 | 1.1276 | 1.2546 [1.2504, 1.2681] |
| Intel Xeon Platinum 8370C | 1.0040 | not a regression |
| Intel Xeon 6973P-C | not a regression | not a regression |

On the two newer EPYC models R1 is 22.9% and 25.5% slower on this cell, as it
is on the 14900K (18.5% below), and on EPYC 9V45 `remove` at 1,000 elements
is 5.9% slower (1.0586). S3 is also slower on these EPYC models there
(11.1% and 12.8%), for a different reason: it does not inline `find`. Other
hosted regressions of R1, all small and descriptive: lookup-hit at 1,000 and
10,000 elements 2.1% each on the Xeon 6973P-C (S3: 1.7%, 1.6%), lookup-hit at
1,000 elements 1.8% and insert-growing at 10,000 elements 1.5% on the Xeon
8370C, insert-presized at 10,000 elements 1.05% on the EPYC 9V74. The
position control below was made on the 14900K only; the EPYC runs have no
control and no counters beyond the timings.

**Halo instruction counts** (run 40052494556, Xeon Platinum 8370C, callgrind;
the three compilers build the same Halo-wf draft port to the current
prelude, because the previously pinned Halo does not compile with current
`main`; CoreMark's wasm interpreter source does not compile with the current
compiler, so it is not counted). Counts are deterministic.

| Instructions per Lua call on fib(32) | main | S3 | R1 |
|---|---:|---:|---:|
| executable self | 793.03 | 752.03 | 778.03 |
| gain against main | | 41.00 | 15.00 |

| Whole-process instructions | main | S3 vs main | R1 vs main |
|---|---:|---:|---:|
| fib | 5,590,683,495 | −5.17% | −1.89% |
| loop | 725,669,348 | −2.07% | −2.07% |
| string-key | 114,663,382 | −3.30% | −1.04% |
| concat | 361,909,460 | +1.50% | −1.05% |
| binary-trees | 5,152,150,048 | +2.32% | −0.37% |
| sort | 282,805,499 | −0.17% | +0.11% |
| integer-table | 676,176,945 | −0.44% | +1.18% |

R1 keeps 15.00 of S3's 41.00 instructions per Lua call, short of the
item's 25. Only R1's integer-table (+1.18%) and sort (+0.11%) rise; S3
raises concat and binary-trees, which R1 lowers. The log of item
wf-req-halo-dispatch-arm-spills-own attributes 25 of S3's 41 instructions to
the small guard function `native_binding` no longer being called. Why S3
changes that is read from the IR, not confirmed with an inlining record: of
that function's three guards, S3 marks the continuing direction of two
(bounds and released) unlikely and, correctly, the Lua-callback side of the
third, so it predicts the first two backwards. That is the same shape as the
removal's early return, and here it happens to help. This reading is a
hypothesis, not a result.

**14900K** ([run 40091755274](https://github.com/Ming-Research/Whitefoot/actions/runs/40091755274),
instrument 05e4c58, candidate = R1 at a3f254c, base = merge base 6d220b46e,
Halo df3eb9ec; one twin of base per arm; clang 22.1.8). Wall time over 10
rounds, ratio candidate/base (twin in brackets): fib 1.0011 [0.9978,
1.0022] (twin 1.0003), CoreMark 1.0004 [0.9986, 1.0013] (twin 1.0001); both
judged no change. `perf stat`, user mode, medians of five rounds:

| Candidate/base | fib | CoreMark | fib twin |
|---|---:|---:|---:|
| instructions | 0.98109 | 1.00150 | 1.00000 |
| cycles | 0.97412 | 0.99979 | 1.00037 |
| branches | 1.03755 | 1.00213 | 1.00000 |
| branch misses | 0.87430 | 0.94149 | 1.00389 |
| L1D loads | 0.95938 | 1.00292 | 1.00000 |

fib's IPC rises from 5.433 to 5.472 and its branch-miss rate falls from
0.207% to 0.174%. Fewer instructions and 2.6% fewer cycles do not appear as
less wall time: the five-round `perf stat` and the ten-round wall-time ratio
disagree, and this run does not explain why. The run measured R1; the
approved S3 has no 14900K timing. The `HashMap` benchmark on the same
compilers (21 cells; floor from the run itself): improvement 4, no change
11, not counted 3, regression 3.

| Cell | Candidate/base | 95% interval | Twin deviation (floor) |
|---|---:|---|---|
| remove 10,000 | 1.1849 | [1.1760, 1.1929] | 0.0035 (0.0136) |
| remove 1,000,000 | 1.0135 | [1.0097, 1.0164] | 0.0029 (0.0107) |
| insert-presized 10,000 | 1.0231 | [1.0093, 1.0311] | 0.0063 (0.0114) |
| insert-growing 10,000 | 0.9495 | [0.9407, 0.9593] | |
| remove-insert 1,000 | 0.9829 | [0.9801, 0.9869] | |
| remove-insert 10,000 | 0.9787 | [0.9751, 0.9819] | |
| insert-growing 1,000 | 0.9915 | [0.9771, 0.9977] | |

Memory cells are identical on both sides. The remove-insert cells that S3
slowed on the hosted runners are not slower under R1 on the 14900K.

**Position control for remove 10,000** ([run 40096747209](https://github.com/Ming-Research/Whitefoot/actions/runs/40096747209),
instrument 3c73453). The candidate is the unweighted merge base with one
driver option, `-mllvm -align-all-nofallthru-blocks=5`, which aligns blocks
without a fall-through predecessor to 32 bytes and changes no weight. The
compiler and image digests differ from base; the twin equals base. Criterion,
written in the log of item wf-req-halo-dispatch-arm-spills-own before the
run and not in this record: if the
option also makes remove 10,000 slower by at least 5%, that cell is
sensitive to code position and R1's 18.5% cannot be charged to the weights
alone. Result: remove 10,000 is 1.1957 [1.1800,
1.2055] (twin deviation 0.0035, floor 0.0136), remove 1,000,000 1.0131; the
same option also lowers iterate 10,000 to 0.2044 [0.2027, 0.2068] and
iterate 1,000 to 0.7357. Over 21 cells: improvement 9, no change 8,
regression 4. The criterion is met.

The perturbation does not execute more instructions
([run 40097164563](https://github.com/Ming-Research/Whitefoot/actions/runs/40097164563),
hosted, callgrind, remove at 10,000, one warm-up and two sampled passes):

| Compiler | Instructions | Conditional branches |
|---|---:|---:|
| merge base | 242,079,010 | 29,864,949 |
| merge base with block alignment | 242,077,920 | 29,864,944 |
| R1 | 234,788,709 | 29,865,362 |

With alignment the four affected functions differ from base only in padding
instructions. R1 executes 3.0% fewer instructions than base. Its
`hash_map_remove`, `hash_map_find` and `wf_remove_one` have the same
instructions as base's; only the copy inlined into the timing loop differs,
in register allocation (the loop counter lives on the stack in one build and
in a register in the other). Remove-then-insert calls the separate
`wf_remove_one` and does not slow.

This is consistent with the [code-placement
investigation](../code-placement/DESIGN.md#exploratory-local-readings),
whose exploratory local readings spread one kernel by 32, 36 and 42 percent
at widths 1, 2 and 4 by moving identical code with a pad of 0 to 48 bytes
linked ahead of it.

Not settled. Whether R1's 18.5% on remove 10,000 and the alignment option's
19.6% have the same mechanism (loop placement or the changed register
allocation); why that cell's placement in base is fast (no hardware counters
of that cell exist); the 26-instruction difference between S3 and R1 on
`fib` (above); the `perf stat` and wall-time disagreement on `fib`; S3's
timing on the 14900K.
