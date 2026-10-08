# Interpreter dispatch through `match`

Whitefoot has no function values and no dynamic dispatch; runtime dispatch
over a closed set is `match` [FN-5]. A bytecode interpreter written in
Whitefoot is therefore a loop around a `match`:

```wf
fn run(vm: &mut Vm, code: &Array<Cell>, frame: &mut Array<u64>) -> u64 {
  let mut pc: u32 = 0;
  loop {
    match code[pc] {
      Add(d, a, b) => { frame[d] = frame[a] + frame[b]; pc = pc + 1; }
      BrIf(c, t)   => { if frame[c] != 0 { pc = t; } else { pc = pc + 1; } }
      Ret(r)       => { return frame[r]; }
    }
  }
}
```

The writer writes this loop and no tail calls; the compiler produces the
tail calls. Stages 2 and 3 below measured interpreters whose arms each end
in a `musttail` self call instead. That spelling was taken to reach the
compiler as the same loop without a check, and the work no longer uses it:
[Stage 4](#stage-4-loop--match-) makes `loop { match }` itself compile to
tail calls.

The question is how the compiler lowers such a loop, and whether a Whitefoot
interpreter compiled that way can match or exceed Silverfir-nano's
interpreter, whose dispatch chain is generated assembly. The consumers are
Halo, the Lua 5.1 engine for firn ([Halo investigation, G4 and
E2/E3](../halo/DESIGN.md)), and a Whitefoot wasm 2.0 interpreter running
CoreMark against Silverfir-nano on the same module.

## What the compiler emits today

One LLVM function. The loop header loads the tag and switches on it; every
arm branches back to the header:

```llvm
header:
  %tag = load i32, ptr %cell
  switch i32 %tag, label %invalid.tag [ i32 0, label %Add  i32 1, label %BrIf ... ]
invalid.tag:
  unreachable          ; was `call void @abort()` before this branch
Add:
  ...
  br label %header
```

Whether LLVM duplicates the dispatch into each arm (threaded code) or keeps
one shared indirect branch is its choice, and register allocation runs over
the whole function, so the state the arms share (pc, frame base, memory base
and length) competes with every arm's temporaries.

The abort default cost a range comparison and branch before the jump table on
every dispatch; it is now `unreachable` (compiler/backend-facts).

## Proposed lowering

Each arm becomes its own function. Every value live at the loop header
becomes a parameter, so it travels in a register from handler to handler, and
each arm ends with its own dispatch:

```llvm
define i64 @run(ptr %vm, ptr %code, ptr %frame) {
  %spill = alloca %run.spill                 ; state beyond the register budget
  %r = call i64 @run.dispatch(ptr %vm, ptr %code, ptr %frame, i32 0, ptr %spill)
  ret i64 %r
}

define i64 @run.Add(ptr %vm, ptr %code, ptr %frame, i32 %pc, ptr %spill) {
  ; the arm's body
  %pc1 = add i32 %pc, 1
  %tag = load <cell[pc1].tag>
  %h   = load ptr, <@run.handlers[tag]>
  %r   = musttail call i64 %h(ptr %vm, ptr %code, ptr %frame, i32 %pc1, ptr %spill)
  ret i64 %r
}
```

An arm that returns delivers its value as an ordinary return; `musttail`
carries it back to `run`'s caller without a frame per dispatched instruction.

- **State beyond the registers** (owner direction, 2026-10-04). A handler
  takes K register parameters plus one pointer to a spill block allocated in
  `run`'s own stack frame. K comes from the target's argument registers under
  the convention used: the default C convention gives 8 on arm64, 6 on
  x86-64 System V and 4 on Windows x64, and `preserve_none`, where LLVM
  offers it, raises K and removes callee-saved register saves from handlers.
  The stack rather than a global, because Halo re-enters `run` for nested
  Lua calls and several contexts may run interpreters at once. Which values
  get registers follows Silverfir-nano's finding that a register pays where
  it breaks a binding loop-carried dependency chain, not where references
  are most numerous.
- **Tag to handler** (open; measured in E0). A: the tag stays dense and the
  dispatch loads `handlers[tag]`, two dependent loads per dispatch. B: the
  tag's value is the handler's offset from a base, one load per dispatch, as
  Silverfir-nano stores the handler address in each cell; every other
  `match` on that enum then sees sparse tags and loses its dense jump table,
  and the tag widens to 32 bits.
- **Inside an arm.** A non-tail call is an ordinary call; the state in
  registers is saved around it, so it belongs in cold arms or is inlined.
  Owned bindings of the arm are released before the transfer, as [FN-10]
  already places releases. A loop in a waiting function, lowered as a
  coroutine frame, is outside the first version.

## Evidence carried over from Silverfir-nano

Silverfir-nano's interpreter decisions and measurements are in that
repository's `mcts_mem/silverfir/interpreter.md` and its `interpreter/`
subtree (read at commit `42396c9d`). The findings this work depends on,
all measured on Apple M-series unless stated:

- Removing one dispatch is worth about one cycle, removing one instruction
  from every handler 0.03 to 0.09 cycles, so dispatch count dominates; that
  is the bytecode designer's lever (folding moves and constants into
  operands, fused compare and branch), not the compiler's.
- A dispatch is bound by load latency: about four memory operations each,
  mostly loads of the instruction stream. Loading a taken branch's target
  handler at handler entry, off the critical path, was worth 4.8% of
  CoreMark cycles.
- One accumulator register for a value consumed by the next instruction was
  worth 29%; the first register-resident local 15 to 16%, the second about
  4% on CoreMark.
- A predictable indirect branch and a bounds check against a length already
  in a register cost nothing measurable.
- The indirect-branch predictor memorises a dispatch sequence up to about
  256 to 320 dispatches and degrades sharply beyond; replicating handlers
  does not recover it.
- Handler placement alone moves results by up to about 20%, through address
  bits at and above the page that address-space randomization changes per
  launch; a single launch measures which way that draw fell, not the code.

## E0: lowering shapes in C

Before the compiler changes, E0 measures what the candidate shapes reach when
LLVM compiles them, using one small interpreter written in C. The handler
semantics are written once and compiled into each variant, so the variants
differ only in dispatch and state carriage.

- **Interpreter.** A register machine with fixed 16-byte cells (opcode,
  three 16-bit operands, a 32-bit immediate) and about twenty opcodes:
  moves, integer and float arithmetic, fused compare-and-branch, jumps,
  bounds-checked 32-bit memory loads and stores, call and return over a
  frame stack.
- **Kernels.** A tight counting loop (few handlers, predictable), recursive
  Fibonacci (call and return), a sieve (memory), Mandelbrot (float), and a
  generated loop body of more than 1,024 dispatches with shuffled opcodes
  (beyond the predictor's memorisation).
- **Variants.** (1) `switch` in a loop; (2) computed goto; (3) `musttail`
  handlers with table lookup (A) under the default convention; (4) the same
  under `preserve_none`; (5) `musttail` with the handler offset in the cell
  (B). Each also in three operand-access forms: checked against the frame
  length, `u8` operands with 256 slots of frame headroom (no check, accepted
  by today's checker per the Halo witnesses), and unchecked.
- **Measurement.** Cycles and instructions retired per process from
  `/usr/bin/time -l`, divided by the dispatch count from a separate counting
  build. Fixed work per kernel. Each binary is launched at least ten times,
  variants interleaved, and the median taken. A null comparison of one
  variant against itself with unreachable padding inserted before its code
  gives the noise floor.

Criteria, fixed before any measurement:

- **Shape selection.** The compiler's target shape is the variant with the
  lowest geometric mean of median cycles per dispatch over the five kernels.
  One variant beats another only when the difference exceeds the larger of
  2% and the null comparison's spread.
- **Falsifier of the proposed lowering.** If variant 3 or 4 does not beat
  variant 1 under that rule, E0 does not justify per-arm functions, and the
  direction returns to the owner before any compiler change.
- **Tag to handler.** B goes to a decision card only if it beats A by at
  least 3% on the geometric mean and loses more than 2% on no kernel.
- **Calling convention.** The gap between variants 3 and 4 prices the
  targets without `preserve_none`; it is reported, not used to select.
- **Operand checks.** The differences between checked, `u8` and unchecked
  access are reported to the Halo investigation for G4; they select nothing
  here.

## E0 outcome

Measured on an M1 Pro ([results](../../experiments/match-dispatch/RESULTS.md)):

- Per-arm functions under `preserve_none` are 15% (checked) and 21% (`u8`)
  below the switch loop, so the proposed lowering stands; computed goto in
  one function does not reach it once the checked form's state is live.
- Passing more state than the convention's argument registers costs 8% with
  one parameter over and 43% with two, which makes the spill block a
  requirement of the lowering, not a refinement.
- Option B stays open: it misses the recorded 3% on the five-kernel test,
  but the dispatch-floor kernel added afterwards shows a 20% reduction of
  dispatch cost once no frame round trip hides it. It is re-measured in the
  wasm interpreter, which has an accumulator and register-resident locals.
- For Halo's G4: frame-index checks cost 4.0% over `u8` operands, the fetch
  comparison 2.3%, and the index representation itself 9.0% over pointers;
  the last is a candidate compiler improvement (carrying a derived address
  instead of a loop-carried index), to be designed separately.

The same work run on Silverfir-nano's interpreter on the same core
([comparison](../../experiments/match-dispatch/RESULTS.md#the-same-work-on-silverfir-nanos-interpreter))
costs about 2.0 to 2.25 cycles per dispatch against E0's 3.6 to 4.5, with
dispatch counts within 20%: the best `u8` variant takes 1.5 to 2.3 times
Silverfir-nano's cycles. E0's interpreter keeps every value in frame memory;
Silverfir-nano keeps an accumulator and its hottest locals in registers. The
dispatch shape is the smaller lever; reaching Silverfir-nano needs the
interpreter's hot values carried as loop state, which the per-arm lowering
keeps in registers.

## E1 outcome

E1 adds Silverfir-nano's accumulator and two pinned locals to E0's
interpreter ([results](../../experiments/match-dispatch/RESULTS.md#e1-silverfir-nanos-register-residency-in-the-same-interpreter)):

- With both, the unchecked C form reaches 1.16-1.37x of Silverfir-nano's
  cycles on the same work, and the WF-expressible `u8` form 1.26-1.79x.
- Passing the handler base along the chain brings the unchecked form's
  instruction count to Silverfir-nano's on `loop` while its cycles stay 1.37x;
  the residue is the dispatch's latency structure (a writeback load of the
  next cell, no early load of the next handler), which LLVM chooses.
- Three consequences for the lowering. The handler base travels as a hidden
  parameter rather than being rematerialised per handler. A loop-carried
  index whose every use addresses one array (`code[pc]`, `regs[base + a]`)
  is the largest remaining cost of the expressible form, 10-30% over
  pointers, so carrying the derived address beside or instead of the index
  is the next lowering candidate. And the accumulator and pinned locals are
  the interpreter writer's loop-carried values, which the per-arm lowering
  keeps in registers only if the register budget admits them.

## Stage 2: the lowering in the compiler

Implemented as compiler/match-dispatch-lowering in
`compiler/src/backend/emitter/dispatch.rs`. The recogniser takes the first
block, in block order, that ends in a `match` over a nominal enum with at
least two targets taking no parameters, whose loop is entered only through
it and left only by returning or jumping back to it. The emitter keeps the
blocks before the loop in the enclosing function, whose entry into the
header becomes a call of the dispatch function and a return of its
result; it then emits the dispatch function (the header, always inlined,
ending in a table transfer) and one function per arm, whose edges back to
the header become guaranteed tail calls of the dispatch function. All parts
share one parameter list under `preserve_none`; the enclosing function's
frame is passed by pointer, and a slot only one part uses becomes that
part's own allocation.

Measured on the WF port of E0's interpreter
([results](../../experiments/match-dispatch/RESULTS.md#stage-2-the-whitefoot-interpreter-under-the-compilers-lowering)):
10-17% fewer cycles than the same compiler emitting the loop whole on three
kernels and 6% on the fourth, and 1.11-1.35x of the C `u8` form.

The parts use `preserve_none` where a build-time probe finds it (LLVM 19 and
later) and the C convention otherwise. Past the convention's argument
registers the values the loop cannot change wait in the frame; a loop that
still does not fit is emitted whole. The enclosing function computes the
loop's invariant loads once and passes the handler table's address where a
register is left. `whitefootc --dispatch-ledger` reports each loop's
verdict. Derived addresses for loop-carried indices remain in
`docs/todo.md` under "Interpreter dispatch lowering".

## Later stages

1. Done as stage 2: the lowering with its register budget; values the loop
   cannot change move to the frame past the registers, and invariant loads
   are hoisted into the enclosing function.
2. Done as stage 2: the Whitefoot interpreter measured under both
   emissions against E0's C forms.
3. Stage 3, below: a wasm interpreter in Whitefoot running CoreMark against
   Silverfir-nano on the same module.

## Stage 3: a wasm interpreter running CoreMark

The question is how far a wasm interpreter written in Whitefoot, compiled
by the stage-2 lowering, is from Silverfir-nano's interpreter on the same
CoreMark module, and which part of the gap belongs to the interpreter's
design and which to the compiler. wasmi, a tail-calling interpreter in
plain Rust with a register-form bytecode, scores close to Silverfir-nano,
so a tail-calling interpreter needs no generated assembly to come near it.

The module is Silverfir-nano's `benchmarks/wasi/coremark/coremark.wasm`
(49,968 bytes, 92 functions, 118 distinct opcodes counting the `0xfc`
prefixes separately: integer and float arithmetic, sign extension,
saturating truncation, `memory.copy` and `memory.fill`), importing eight
WASI functions: `args_get`, `args_sizes_get`, `clock_time_get`,
`fd_close`, `fd_fdstat_get`, `fd_seek`, `fd_write` and `proc_exit`.

The interpreter is built in two steps:

- **v1, a direct stack machine.** The loader decodes the module's sections;
  a translator rewrites each function body into one array of a Whitefoot
  `enum` of operations with branch targets, stack heights and callee
  indices resolved; the value stack and the locals share one array of
  `u64`, the stack pointer and frame base travel as loop-carried state, and
  linear memory is an array of bytes. A host call leaves the interpreter
  function and returns to a driver that performs it, since a waiting
  function is not split [compiler/match-dispatch-lowering's provisional
  exclusions]. The interpreter implements the opcodes the module uses and
  the loader rejects any other; safety comes from Whitefoot's checks on
  every access, not from a wasm validator, so a malformed module reaches an
  error, never undefined behavior.
- **v2, measured steps toward the fast designs.** Each step is one
  mechanism from Silverfir-nano (an accumulator for the top of the stack,
  locals resident in registers, folded operations) or wasmi (register-form
  operations), measured alone so its effect is attributed.

Measurement: on the M1 Pro, Silverfir-nano's interpreter
(`sf-nano-cli --interp`) and the Whitefoot interpreter run the same module
with the same arguments, alternating launches, and each side's score is the
median of the launches' CoreMark scores.

Criteria, fixed before the first measurement:

- **Correctness.** A run counts only when CoreMark reports correct
  operation: every CRC it validates matches.
- **v1 position.** v1's score is reported as a ratio to Silverfir-nano's;
  there is no threshold. The prediction, written before measuring, is
  0.3-0.5x, a stack machine executing every `local.get` and `local.set`
  as an operation.
- **A v2 step** is kept when it raises the median score by at least 2%
  over the previous step, twice the 1% the E0 null comparison measured as
  layout noise; each step records its predicted effect before it is
  measured.
- **Compiler attribution.** A gap that profiles to code the compiler emits
  around a handler (reloads, spills, checks it could have proved) rather
  than to the interpreter's design goes to `docs/todo.md` as a compiler
  item with the profile as evidence.

## Stage 3 v1 outcome

v1 runs the module correctly (every CRC CoreMark validates matches) and
scores 0.223x Silverfir-nano's interpreter, below the predicted 0.3-0.5x
([results](../../experiments/match-dispatch/RESULTS.md#v1-the-direct-stack-machine)).
At 3.94 cycles and 18.2 instructions per dispatch, the per-dispatch cost is
already near E0's tail-call forms; the gap is the dispatch count, one per
wasm operation including every local access and constant, and the
instructions each handler spends on operand-stack checks and frame traffic.
The v2 steps therefore start with the mechanisms that remove dispatches:
locals and constants folded into operands (Silverfir-nano's `MovSlot` and
`MovConst` and its folded arithmetic, or wasmi's register form), then an
accumulator for the top of the stack.

Writing v1 also found that checking one function grows faster than its size
(`docs/todo.md`, "Checking one function grows faster than its size"): the
interpreter function checks only with each handler's body in a function of
its own.

## Stage 3 v2 so far

Register-form operations on frame slots (v2a), constants in frame slots
(v2b), fused compare-and-branch operations (v2c), the stack box kept across
helper calls (v2d), handler bodies written into their arms (v2e), fewer
copies (v2f), address additions folded into loads and stores (v2g) and an
accumulator register (v2h) bring the interpreter to 0.519x of
Silverfir-nano, 2971.8 against 5730.7 in one run
([results](../../experiments/match-dispatch/RESULTS.md#v2h-an-accumulator-register)).
Each step since v2b removed dispatches or instructions per dispatch; at
about 4.2 cycles for each of 506 million dispatches the remaining cost is
still both the number of dispatches and each handler's instructions
beyond its operation, among them the next cell's address recomputed from
its index on every dispatch (`docs/todo.md`, "Interpreter dispatch
lowering"). Like Silverfir-nano's translator, v2h passes a value from the
operation that computes it to the next one, its single consumer, in an
accumulator register instead of a frame slot, for 135 million of the 506
million dispatches. The checker's growth with function size still limits
the interpreter's form: a handler that delivers a value from a `match`
stays a helper function.

## Stage 3 on x86-64

Every stage-3 result so far is from the M1 Pro, where the convention without
callee-saved registers, `preserve_none`, passes 24 integer arguments in
registers. On x86-64 it passes 12
([argument registers](../../experiments/match-dispatch/RESULTS.md#argument-registers)),
and the C convention passes 6. v2h's split loop takes 16. Below that budget,
compiler/match-dispatch-lowering moves the values the loop cannot change into
frame slots, and emits the loop whole when the parts still do not fit. The
convention is chosen by a build-time probe of `/usr/bin/clang`, and LLVM 19
added `preserve_none`. So the form depends on the host's clang:

- the 14900K runner has clang 18 and so builds parts under the C convention;
- an x86-64 host with clang 19 or later uses `preserve_none`.

The question: how does the lowering emit v2h on x86-64 under each
convention, and how far is the interpreter from Silverfir-nano's on the same
x86-64 host?

**Method.**
- **Interpreter:** v2h as `wasm/gen.py` writes it at the measured revision,
  compiled by that revision's `whitefootc`.
- **Emission:** `--dispatch-ledger` reports whether the dispatch loop split
  and otherwise the first condition it failed.
- **Workload:** the CoreMark 2K performance run, `0x0 0x0 0x66 2000`.
- **Comparison:** Silverfir-nano at `5f248e44`, release `sf-nano-cli
  --interp`, alternating launches with `wasm/coremark.py`, medians.
- **Hosts:**
  - the 14900K, with its clang 18, for timing;
  - a GitHub-hosted `ubuntu-24.04` runner with clang 19 as `/usr/bin/clang`,
    for the `preserve_none` emission. Its timing indicates only, since the
    runner's processor is not chosen.

**Prediction, written before measuring.**
- **Clang 18 (C convention):** the loop does not fit the 6 registers even
  with the values it cannot change in the frame, so it is emitted whole,
  and the ratio to Silverfir-nano falls well below the M1's 0.519.
- **Clang 19 (`preserve_none`):** the loop splits with some values in the
  frame. The ratio stays near the M1's, 0.40 to 0.52.

**What decides the next step.** The rule is fixed now.
- **Split with `preserve_none` and ratio at least 0.45:** x86-64 needs
  nothing of its own beyond a clang that has the convention. The next
  lowering change is the loop-carried index carried as an address
  (`docs/todo.md`, "A loop-carried index is recomputed into an address in
  every arm").
- **Not split under `preserve_none`, or ratio below 0.45 with the parts
  reading frame slots on the hot path:** the next change is the x86-64
  register budget, meaning which values the parts carry in registers.
- **Clang 18 only:** the C convention's result is recorded as the cost of a
  host without the convention. Whether the 14900K gets a clang with it goes
  to the owner, because the runner is shared with other repositories.

## Stage 3 on x86-64 outcome

**The first `preserve_none` build failed.** With clang 19, the lowering
split v2h's loop into parts taking all 12 integer registers it counted for
x86-64. The build then failed in register allocation: the parts' transfer,
a guaranteed tail call through a table-loaded address, needs one of those
registers for the address. `regprobe.py` now measures that transfer itself.
It gives 11 on x86-64 Linux, 12 on Windows x64 and 24 on AArch64, under
clang 19 and 20
([argument registers](../../experiments/match-dispatch/RESULTS.md#argument-registers)).
The budget is corrected to those counts.

**After the correction, the loop splits under both conventions**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-on-x86-64)):
- `preserve_none` on a hosted EPYC: 11 registers and 4 values in the frame,
  0.771 of Silverfir-nano. This is indicative only, since the processor is
  not chosen.
- The C convention on the 14900K under clang 18: 6 registers and 9 values in
  the frame, 0.560 of Silverfir-nano.

The measured interpreter splits on both hosts, and each ratio clears the
threshold the rule set. These runs compare Whitefoot with Silverfir-nano on
different processors and toolchains. They do not isolate what the split, or
either convention, costs on x86-64, and the M1's 0.519 was measured on
another processor.

By the rule fixed before measuring, the loop splits under `preserve_none`
with a ratio of at least 0.45. So the next lowering change is the
loop-carried index carried as an address. The 14900K's clang 18 has no
`preserve_none`, so timing that form there needs a newer clang on the host,
which the runner's other users share.

## Stage 3: the code cursor

Every dispatch of v2h computes its next operation's address from an index.
An arm computes `next = pc + 1` or a branch target, tests `next < n`, and
transfers. The header then addresses `code^.inner[pc]`, the hoisted
`Slots<Op>` payload plus `pc` times the `Op` stride, before it loads the
tag. E1 put the C interpreter's index form 1.1 to 1.3 times above its
pointer form with every other mechanism equal, almost all of it computing
`code[pc]` and `regs[base + a]` from indices
([E1](../../experiments/match-dispatch/RESULTS.md#e1-silverfir-nanos-register-residency-in-the-same-interpreter)).
The frame half, `regs[base + a]`, was measured in the compiler and refused
(compiler/match-dispatch-lowering, derived frame-slot addresses: +0.9%).
This section takes the other half.

**Question.** How many of each dispatch's instructions go to turning `pc`
into the next operation's address and checking it? Which way of carrying
the address instead would remove them, on x86-64 and AArch64?

**Candidates.**
- **Cursor beside the index.** The parts carry `&code^.inner[pc]` in
  addition to `pc`, and the arms advance both. It costs one more argument
  register, which on x86-64 is one of 11.
- **Cursor instead of the index.** The parts carry only the address. The
  rare uses of `pc` itself recover it as the cursor's distance from the
  payload divided by the stride: a trap's report, a call's return address,
  a branch's relative target. The bounds test becomes an end-pointer
  comparison.
- **A power-of-two `Op` stride.** Padding `Op` lets an index become an
  address in one shifted add. This interacts with the enum tag width
  (`docs/todo.md`, "An enum's tag is an `i32` whatever its variant count").
- **None.** If the attribution below finds few instructions, the index stays.

**First step: attribution, no compiler change.** Compile v2h at main with
the gate's pinned LLVM on x86-64 (`preserve_none`), and with macOS's clang
on AArch64. Disassemble the hot arms: `I32Add`, `I32AddA`, `I32AddAD`,
`LocalGet`-shaped copies, `BrIf` and the compare-and-branch fusions. Count
each arm's instructions by role: the operation, operand and result slots,
the next operation's address, the bounds test, and the transfer.

**Rule for the next step, fixed now.**
- **Average at least 2 instructions per hot arm** on the address and the
  bounds test: build the cursor-instead-of-index candidate in an
  experiment branch. Judge it by CoreMark against its base under the
  stage-3 criterion: adopt if the median score rises at least 2%.
- **Fewer:** record the attribution and close this candidate.

**Attribution**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-the-code-cursor)).
Every hot arm ends in the same dispatch steps, in an order of its own.
Forming the next operation's address and testing it takes 7
of the 21 instructions `I32Add` executes on x86-64: the next index, a
compare and a branch, then a move, a shift and two additions that turn the
index into an address. On AArch64 it takes 4 of 19, the shift folding into
one addition. Both exceed the rule's 2, so the next step is the
cursor-instead-of-index candidate.

Three findings bear on how to build it:
- **The arms already hold the cursor.** Each arm receives the matched
  element's address, the header value it reads the operation's fields
  through. The dispatch forms the next element's address again from the
  next index, not from that address. On v2h a cursor beside the index
  would therefore cost no register; the cursor instead of the index also
  frees the index's register.
- **`Op` is already 16 bytes.** A power-of-two stride is in place, and
  x86-64 addressing scales an index by at most 8, so the shift stays.
  That candidate is closed.
- **x86-64 forms what a free register would hold.** With the index's
  register freed, the handler table's address could become a parameter
  again: every x86-64 arm forms it with `leaq table(%rip)`. Every arm that
  reads a frame slot also reloads the stack's payload pointer from the
  enclosing frame. On AArch64 both stay in registers.

**Building the candidate.** The arms already hold the address, so the
candidate is built and measured in two steps.
- **A, the cursor beside the index.** The lowering recognises a header that
  addresses the matched element by a carried index into a run of slots
  whose address the loop cannot change, when the arms read the element
  through that address. The parts carry the address in the parameter that
  held it as a header value, so the parameter list is unchanged. Entering
  the loop, the enclosing function forms the address from the entry's
  index. An edge back to the header moves the address the arm received by
  `next - pc` elements, which the host folds to one addition when
  `next = pc + 1`. The header uses the address it receives. The moved
  address is exactly the element's: the edge passes `next` to a header
  whose indexing of it was proved, so `next` lies inside the run.
- **B, the cursor instead of the index.** A, and the index is no part's
  parameter. A part that reads it recovers it as the address's distance
  from the run's first element divided by the element's size. A
  comparison of `pc + 1` with the run's length becomes a comparison of
  the moved address with the run's end address, which the enclosing
  function computes once and the parts receive. Only a step of one
  element is compared that way: `pc` is below the length, so the moved
  address is at most the end address, and no address outside the run and
  its end is formed.

**Predictions**, from the attribution's `I32Add` path, before measuring:
- **A:** 3 fewer instructions per sequential arm on x86-64, where the
  move, the shift and two additions become one addition, and 1 fewer on
  AArch64, where the shifted addition and the tag's load become one
  pre-indexed load. Branch arms keep about their counts.
- **B:** 2 fewer again per sequential arm on both, the index's increment
  and move. A branch arm whose next index joins `pc + 1` and a target pays
  2 or 3 instructions to recover `pc`.

**Criterion, fixed before measuring.**
- **Correctness.** Every launch reports CoreMark's CRCs, and the gate
  passes.
- **Measurement.** CoreMark 2K, the branch's compiler against its merge
  base's on v2h, 7 alternating launches, medians.
- **Host.** The 14900K decides, with the gate's pinned LLVM and
  `preserve_none`. Its `/usr/bin/clang` stays at 18 until the downstream
  projects pin a release built with the pinned major. Until then, the
  14900K under clang 18, the hosted x86-64 runner and macos-15 (AArch64)
  are indications only.
- **Adoption.** A is adopted if its median is at least 2% above the
  base's. B's further changes are adopted if B's median is at least 2%
  above A's. If A falls short and B is at least 2% above the base, B is
  adopted whole.

**Outcome**
([results](../../experiments/match-dispatch/RESULTS.md#a-and-b-on-the-14900k)).
On the 14900K with the pinned LLVM, A raised the median score 13.0% over
the base, above it in all 7 pairs, so A is adopted. A repeat with a twin
of the base gave 13.3%, with the twin within 1% of the base. B scored 1.6% below A,
below it in all 7 pairs, so its further changes are refused. B fell short
for three reasons:
- B's sequential arms saved one instruction, not the predicted two: the
  moved address still passes through a move into its carried register.
- Its branch arms recover `pc` on every dispatch.
- The end address took the index's register, so no register was freed
  for the handler table's address.

The hosted runners agree that A gains and disagree on B, within their
spreads. B's implementation stays on the branch
`claude/stage3-cursor-instead`.

## Stage 3: x86-64 register pressure

With the code cursor, x86-64's `I32Add` runs 18 instructions. Two kinds of
them exist because the parts have more values than registers. Every arm
that reads a frame slot first reloads a value kept in the enclosing frame
(`addq 0xc8(%r11)`). Every arm also forms the handler table's address
(`leaq table(%rip)`), since no register is left to pass it. The split keeps
4 values in the frame, choosing first those that the fewest arms read,
regardless of how often those arms run (compiler/match-dispatch-lowering).

**Question.** Which values are kept in the frame? Which of them do the hot
arms read on every dispatch? Would keeping a different set in registers
remove those reloads, or leave a register for the table's address?

**First step: attribution, no compiler change.** Emit v2h's module on
x86-64 with the gate's pinned LLVM at this branch. From it:
- list the parts' parameters and the values kept in the frame, with the
  number of arms that read each;
- identify the value the hot arms reload;
- count the hot arms' instructions spent on reloads and on forming the
  table's address.

**Rule for the next step, fixed now.**
- **The hot arms reload a kept value on every dispatch:** build a
  candidate that changes which values the frame keeps. Judge it by CoreMark
  on the 14900K against its base, with a twin of the base as the noise
  control, and adopt it if the median rises at least 2%.
- **Otherwise:** record the attribution and close this step.

**Attribution**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-x86-64-register-pressure)).
The hot arms reload a kept value on every dispatch: the stack's element
address, which 129 of the 318 arms read. Meanwhile three values that at
most two arms read hold registers. So the rule's first branch applies.
Both causes are defects of the general lowering, not of this program:
- **Reads through replaced projections are not counted.** The spill order
  counts the arms that name a value, while an arm reaches a box the loop
  keeps through its own projection, which emission replaces by the hoisted
  one.
- **An unread address stays a parameter.** With the code cursor, the run's
  address remains a part parameter although no part reads it.

**The candidate.** Count an arm as reading a value when it reads any
projection that emission replaces by that value, or hands such a value to
a callee through a part's pin slot. Leave out of the parts' parameters a
hoisted value that no instruction emitted in a part reads.

**Prediction:** on x86-64 the stack's element address moves to a
register, removing the reload from every arm that reads a frame slot. The
register freed by the code's element address keeps one more value out of
the frame, or passes the handler table's address.

**Criterion, fixed before measuring**, extended by the owner's direction
that this lowering serves Lua and other interpreters, not this one:
- **Measurement on two interpreters, the 14900K deciding:**
  - the stage-3 wasm interpreter on CoreMark 2K;
  - Halo's Lua interpreter on its `fib` and `loop` kernels, built from
    Halo-wf at a commit pinned to a release built with LLVM 22.
- **Each comparison:** the branch's compiler against its base, with a
  twin of the base, 7 interleaved launches, medians.
- **Adoption:** if CoreMark's median rises at least 2% and no Halo kernel
  falls more than 2%.
- **The code cursor itself** is measured on the same Halo kernels against
  its own base, as a check that it serves another interpreter.

**Outcome**
([results](../../experiments/match-dispatch/RESULTS.md#the-register-pressure-candidate-and-the-code-cursor-on-halo)).
- **The candidate is not adopted under the rule.** CoreMark rose 1.3% on
  the 14900K, short of 2%. No Halo kernel moved.
- **The code cursor slows Halo.** Halo's `loop` is 5.7% slower with the
  cursor, in every pair, and its `fib` about 1% slower, in 6 of 7 pairs,
  against a twin within 0.2%. The cursor, adopted on the wasm interpreter
  alone, does not serve Halo as built.

**Edges by their step.** An edge back to the header moves the received
address only where its index is the received index moved by a constant,
`pc + k`, which the host folds to one addition. Every other edge forms the
element's address from the run, as the header did before the cursor. The
parts then keep the run's address wherever an arm has such an edge.

**Prediction:**
- **Halo:** its edges take their index from helpers, so they form the
  address from the run as before the cursor, and its times return to the
  pre-cursor ones.
- **The wasm interpreter:** its sequential arms keep the moved address,
  and its branch arms form theirs from the run, one subtraction fewer
  than with the cursor. CoreMark stays at the cursor's score.

**Criterion, fixed before measuring**, on the 14900K against a twin, 7
interleaved launches:
- **Adopted** if neither Halo kernel is more than 1% slower than before
  the cursor (`e1708490c`), and CoreMark is no more than 2% below the
  cursor's score (`3a260446c`).
- **Otherwise** the cursor's adoption is reopened.

**Outcome**
([results](../../experiments/match-dispatch/RESULTS.md#edges-by-their-step)).
Edges by their step meet the criterion:
- **Halo:** `loop` matches its time before the cursor (1.000) and `fib`
  is within 0.5% (1.005, slower in every pair, within the 1% allowed).
- **CoreMark:** 2.4% above the cursor, above it in every pair, against a
  twin at 1.000.

So the cursor stays, with this rule for its edges. The measured branch
also carries the register-pressure candidate. Alone, that candidate gave
1.3% against its 2% rule; in this branch the two together give the 2.4%.
It is kept as a correction rather than adopted as a gain: the recorded
spill order spills the values the fewest arms read first, and the
implementation had missed reads through projections and pins.

## Stage 3: the gap to Silverfir-nano

The stage-3 wasm interpreter is the yardstick for this lowering because its
ceiling is known. Silverfir-nano's interpreter is close to the best this
dispatch shape reaches, and wasmi, a Rust interpreter dispatching by tail
calls, is reported at about 80% of it. A Whitefoot interpreter compiled
from the same design should reach that too. Halo's Lua interpreter remains
the check that a change does not cost another interpreter. Where Halo
cannot reach the wasm interpreter's shape, the gap points at Halo's
implementation, as its next index taken from helpers does.

**Question.** On the 14900K with the pinned LLVM, how far is v2h, compiled
by main, from Silverfir-nano on CoreMark, and from wasmi where it runs the
same module? Where do the remaining instructions per dispatch go?

**First step: the ratio, no compiler change.** Run v2h compiled by main,
a twin of it, Silverfir-nano `5f248e44` (`sf-nano-cli --interp`) and
wasmi's command-line runner where it installs. Use CoreMark 2K with 7
interleaved launches and medians. Then compare the hot arms' machine code
with Silverfir-nano's handlers, role by role, as the code cursor's
attribution did.

**Use of the result.** The ratio sets how much room is left. The
attribution orders the next lowering candidates by the instructions they
would remove from the hot arms. Each candidate keeps its own rule, written
before it is measured: the wasm interpreter on the 14900K decides, and
Halo must not regress.

**Outcome of the measurement**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-the-gap-to-silverfir-nano)).
On the 14900K, v2h scores 0.692 of Silverfir-nano and wasmi 0.848. v2h
makes the fewest dispatches of the three. The whole gap is in each
dispatch: v2h's `I32Add` runs 17 instructions on x86-64 where wasmi's
corresponding handler runs 6. At wasmi's cost per dispatch, v2h would
score about 0.97 of Silverfir-nano. So the room left is about 1.4 times,
all of it in the code each dispatch runs.

## Stage 3: binding what wasmi and Silverfir-nano bind

The owner set closing this gap as the evidence that the lowering is at
its best. Even a straight copy of wasmi's design should score about like
wasmi. The means are to bind values to registers across handlers as
wasmi and Silverfir-nano do. Their contracts, against v2h's parts:

| value | wasmi | Silverfir-nano, x86-64 | v2h on x86-64 |
|---|---|---|---|
| next operation | cell pointer, the cell holding its handler's address | cell pointer, handler address in the cell, next handler word preloaded | element address and index; tag, then a handler table |
| frame | stack pointer (an address) | frame base register | frame index plus the stack's element address |
| linear memory | base and length in registers | base and length in registers | element address in a register, length read from memory at each access |
| accumulators | integer and two float registers | integer and float accumulators, two locals of each kind | the integer `acc` |
| bound of the next operation | none (validated code) | none | `pc + 1 < n` at every dispatch |

Each row v2h lacks is a lowering mechanism, since the language has no
pointers or code addresses. The order follows the instructions each would
remove from the hot arms. Each one is a candidate with its own rule:
- **1. The handler's address in the element.** A split dispatch loop's
  matched element carries its arm's address, so the dispatch loads the
  next handler from the element it moves to, instead of a tag and then a
  table entry. This saves about two instructions and a dependent load per
  dispatch. Its first step measures the upper bound with a prototype, not
  the final representation. The representation (where the address lives,
  who writes it, and an enum matched by several loops) is designed only if
  the bound clears the rule.
- **2. The frame as an address.** The frame index used as the base of
  slot accesses travels as the address of its first slot, as the code
  cursor carries the matched element. This saves a base computation per
  slot-reading arm, and one register.
- **3. The memory's length in a register.** A run's length that the loop
  reads on every access, and changes only in some arms, travels in a
  register those arms update.
- **4. The index, once registers allow.** Revisited after 1 to 3.

The bound of the next operation is a language question, whether the
proof can show the code cannot fall off its end. It is outside these
lowering steps.

**Criterion for each candidate, fixed before it is measured.** CoreMark 2K
on the 14900K with the pinned LLVM, against its base with a twin, 7
interleaved launches. Adopted if the median rises at least 2%, and neither
of Halo's `fib` and `loop` kernels is more than 2% slower. The goal is
wasmi's 0.848 of Silverfir-nano, then Silverfir-nano itself.

**Outcome of candidate 1**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-the-handlers-address-in-the-element)).
The prototype scores 1.101 of its base on the 14900K, ahead in all 7
launch pairs, and its `I32Add` runs 15 instructions instead of 17. The
bound clears the rule, so the representation is designed next.

Halo's `fib` and `loop` stay within 0.5% of the base. Halo's interpreter
did not receive the word, though, so that result checks nothing about the
mechanism itself. The language's layout ceilings
([OP-9](../../../spec/kernel-spec.md)) bound each stored enum by its
product layout, alignment included. Halo's `Cell` has only `u8` and `u32`
fields, so its alignment ceiling is 4, and an 8-byte-aligned word cannot
enter it.

**The representation's open choices.**
- **Where the word lives:**
  - in every value of the enum, as in the prototype;
  - or only in the enum's run storage, where the stride grows and values
    elsewhere keep their size.
- **Who writes it:** every construction in the first case, every store
  into a run in the second.
- **Its width and alignment:**
  - an address at pointer alignment, which only an enum whose ceiling is
    8-byte aligned admits;
  - an address at the ceiling's alignment, which Halo's `Cell` admits at
    20 bytes instead of 12;
  - a 32-bit offset from a base, 16 bytes for `Cell`, at the cost of
    adding the base at each dispatch.
- **An enum matched by several split loops:** which loop's arms the word
  names.
- **Fragment builds:** a construction in one fragment names an arm in
  another, so arms cannot stay internal to their module.

**Next experiment: the word in a 4-byte-aligned `Cell`.** The question is
whether the word helps an interpreter like Halo's, whose cells are small
and 4-byte aligned. The prototype places the 8-byte address at the enum's
ceiling alignment where that is below 8, loading and storing it with that
alignment. Base, prototype, twin, and Halo's `fib` and `loop` on the
14900K, 7 interleaved launches.
- **For the representation:** if either kernel gains at least 2% and
  neither loses more than 2%, the representation must cover such enums,
  and the address and offset forms are compared next.
- **Otherwise:** this form gives a Lua-like interpreter nothing, and the
  card says so.
- **CoreMark:** must stay within its twin's spread, since `Op` is already
  8-byte aligned.

**Outcome of the word in a 4-byte-aligned `Cell`**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-the-handlers-address-in-the-element)).
Against candidates 1 and 2 alone, Halo's `fib` takes 2.3% less time and
`loop` 1.8%, while the twin stays within 0.2%. That clears the criterion,
so the representation must cover enums whose ceiling is below pointer
alignment. Comparing the address with a 32-bit offset remains open.

CoreMark is unchanged, as `Op` is already 8-byte aligned. With this
commit (`8a93bbb90`) it scores 5970.1, 1.099 of main, the same as
candidates 1 and 2 alone
([run 37579981158](https://github.com/Ming-Research/Whitefoot/actions/runs/37579981158)).

**Outcome of candidate 2**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-the-frame-as-an-address)).
- **The mechanism works:** the frame base leaves every slot access.
- **The time does not move:** CoreMark scores 0.994 of candidate 1 alone on
  the 14900K, below the rule.
- **Halo is not covered:** its interpreter reaches frame slots only inside
  helpers, so it does not receive the mechanism.

The candidate is not adopted.

## Stage 3: where the time goes

Candidate 1 removed one dependent load from the dispatch chain and gained
10%. Candidate 2 removed a cheap instruction from every slot access and
gained nothing. So counting instructions does not rank the remaining
candidates. The owner asked for an attribution of the whole gap before more
candidates. wasmi is the reference, because it is compiled by LLVM from
Rust: whatever wasmi's code achieves, a Whitefoot interpreter's code should
reach.

**Question.** Of the cycles v2h spends beyond wasmi, which part is due to
each of the following?
- the lowering of `match` to tail calls;
- the language's proof obligations, such as the test of the next index
  against the code's length;
- the interpreter's own design: which values live in frame slots and which
  in registers, and how calls build frames.

**Method.**
- **Counterfactual time, not instruction counts.** Remove one cost and
  measure cycles or score: the bounds tests by rewriting them to true in
  the emitted LLVM, separate dispatch per branch outcome, and each
  candidate.
- **Cycles per comparable handler.** For v2h and wasmi, from sampled time
  shares, per-handler counts and total cycles on the M5.
- **Machines.** The M5 has hardware counters; the 14900K remains the
  yardstick.

**Outcome**
([results](../../experiments/match-dispatch/RESULTS.md#stage-3-where-v2hs-time-goes-against-wasmi)).
- **Dispatch is no longer v2h's bottleneck.**
  - On the M5, candidates 1 and 2 with the bounds tests removed save 2.4%
    of cycles. v2h still spends 3.61 cycles per dispatch, against wasmi's
    2.32.
  - Separate dispatch per branch outcome saves nothing.
- **The bounds tests no longer cost time.**
  - On the 14900K they cost 3.7% on main but nothing once candidate 1
    shortens the dispatch chain.
  - On the M5 they never cost anything.
  - A language mechanism that removes them would not be justified by
    performance on this interpreter.
- **The remaining cycles go to v2h's design.**
  - A value read from a frame slot that the operation before it has just
    written: `I32AddD` takes 4.7 cycles where `I32AddAD`, reading the
    accumulator, takes 1.85.
  - Calls take 28 cycles against wasmi's 15.6.
  - wasmi's translator keeps more values in its integer register (`ireg`).
    v2h's accumulator holds a value only when the next operation is its
    one consumer.

**Next step, awaiting the owner.** Adopt wasmi's choices in v2h's
translator: keep the values wasmi keeps in registers in loop parameters,
and build call frames as wasmi does. Then compare with wasmi again. That
changes the interpreter, not the compiler.
- **If v2h then comes within a few percent of wasmi:** the lowering is at
  its best for this interpreter shape.
- **If it does not:** the remaining difference is again attributed by
  counterfactuals.

Two constraints carry into the design:
- on x86-64, v2h's split loop already takes all 11 integer argument
  registers of `preserve_none`;
- on x86-64, wasmi's handlers take 7 integer arguments under `sysv64`, of
  which only 6 can travel in registers.

## Stage 4: `loop { match }`

An interpreter is written as a loop around a `match`, with no tail calls in
its source. The compiler's back end turns that loop into one function per
arm with guaranteed tail calls between them. Every measurement so far used
interpreters written with a `musttail` self call ending each arm, so
whether `loop { match }` reaches the split lowering at all is untested.

**Known obstacles, each to be fixed rather than avoided:**
- **[INV-1] at a join.** An invariant over loop variables that different
  arms update is lost where their paths join before the back edge
  (`docs/todo.md`, "A loop invariant is lost where a guarded update joins
  an untouched path").
- **Check time.** Checking one large function grows faster than its size
  (`docs/todo.md`, "Checking one function grows faster than its size").
  This applies to any interpreter written as one function.
- **One back edge.** A `match` inside a loop reaches the back edge through
  one join after the `match`. Each arm needs its own transfer, so the
  lowering must give each arm the dispatch that follows that join.

**First step: what the compiler does today.** Rewrite two interpreters as
`loop { match }`: the four-instruction interpreter of the dispatch tests,
and v2h, with `gen.py` writing the loop form. Record, for each:
- the checker's verdict, and every refusal's rule and witness;
- the dispatch ledger's verdict for the loop, and its reason if it is not
  split;
- for a split loop, the emitted parts against the `musttail` spelling's.

This changes no compiler. Each obstacle found then gets its own design,
and a change to a language rule goes to the owner first. After that,
v2h's gap to Silverfir-nano and wasmi is measured again in the loop form.

**Outcome of the first step.** The compiler was built from `8a93bbb90` on
the owner's M5. Two interpreters were written as `loop { match }`:
- the four-instruction test interpreter, by hand;
- v2h, by a one-off rewrite of `gen.py`'s output. It turns each arm's
  `return musttail run(..., pc: X, fp: F, acc: A)` into assignments, and
  moves the statements that followed a failed guard into its `else`.

The findings, in the order they block:

1. **The back end already splits `loop { match }`.** The test interpreter
   checks, splits into 4 arm functions and runs correctly. The arms fall
   off the `match` to one back edge, and each still ends in its own
   guaranteed tail call.
2. **In a loop, a call that writes a box blocks a bound derived from the
   box's length.** After such a call, even one whose `ensures` preserves
   the length, `fp + 4 <= stack^.inner.len` still holds. It re-proves the
   invariant at the back edge and admits `stack^.inner[fp]`. But
   `4 <= stack^.inner.len`, which drops the non-negative `fp`, is no longer
   derived. In v2h the frame contract is the loop invariant
   `fp + 65536 <= stack^.inner.len`, so after a helper writes a frame slot
   `stack^.inner.len - 65536` is refused [OP-2]. Minimal witness, compiler
   `8a93bbb90`:

   ```wf
   fn touch(stack: &Box<Array<u64>>) -> r: unit writes(stack.inner) contract {
     ensures stack^.inner.len == entry(stack)^.inner.len;
   } {
     if 0_u64 < stack^.inner.len {
       set stack^.inner[0_u64] = 1_u64;
     }
     return unit;
   }

   fn walk(stack: &Box<Array<u64>>, fp0: u64, k: u64) -> r: u64 writes(stack.inner) contract {
     requires fp0 + 4_u64 <= stack^.inner.len;
   } {
     let fp = fp0;
     loop (
       invariant fb: fp + 4_u64 <= stack^.inner.len
     ) {
       touch(stack: stack);
       let room = stack^.inner.len - 4_u64;
       set stack^.inner[fp] = room;
       if room == 7_u64 {
         return room;
       }
     }
     return 0_u64;
   }
   ```

   | variant | verdict |
   |---|---|
   | the witness, with the call | refused [OP-2] on `room` |
   | the call under `if k != 0_u64` | refused [OP-2] on `room` |
   | no call | accepted |
   | the same fact as a `requires` of a loop-free function, then the call | accepted |
   | the fact `4 <= stack^.inner.len` from an `if` guard, then the call | accepted |
   | the fact `fp + 4 <= stack^.inner.len` from an `if` guard without a loop, then the call | accepted |
   | the invariant `4 <= stack^.inner.len`, then the call | accepted |
   | the call, then only the back edge and `stack^.inner[fp]`, no `room` | accepted |
   | `fp` an immutable parameter instead of a loop variable | refused [OP-2] on `room` |

   The refusal needs a loop, a call writing the box, and a bound that drops
   a term of the invariant. Every arm of v2h that writes a frame slot
   through a helper meets it.

   **Cause: the checker falls short of the specification.** After the
   call, closed L0 holds `L = D`, where `D` is the call's datum for the
   length. The loop header's affine theorem gives `fp - D <= -4` through
   `D`'s image. [MSR-4]'s step 6, the affine-left / L0-right bridge,
   combines the two into `4 <= L`.

   The checker never takes that step for this bound. It builds the lower
   target of a subtraction's integer domain without its right-hand term
   (`right: None`, `compiler/src/semantic/entailment/flow/prover.rs`, in
   the subtraction's normalization), and `numeric_affine_proof` returns
   before the bridge when that term is absent.

   The fix is in the checker: submit each finite component with its
   right-hand term through the complete numeric disposition. No rule
   changes.
3. **[INV-1] at a join.** An arm that updates a loop variable under a guard
   (`set fp = nf` after `nf + 4 <= len`), beside arms that leave it alone,
   fails the invariant `fp + 4 <= len` at the back edge, even when the other
   arms do nothing else. Without the update it is accepted. This is the
   join of `docs/todo.md`'s INV-1 entry, and every interpreter whose calls
   and returns change a frame base meets it.
4. **Check time grows with the cube of a join's width.**

   | arms | `loop { match }` | `musttail` spelling | no loop, one join after the `match` | the same, one `set` per arm |
   |---:|---:|---:|---:|---:|
   | 40 | 0.11 s | 0.04 s | 0.05 s | — |
   | 80 | 0.51 s | 0.07 s | 0.26 s | 0.03 s |
   | 160 | 3.59 s | 0.14 s | 1.84 s | 0.20 s |
   | 320 | 35.0 s | 0.33 s | 18.7 s | 1.53 s |
   | 640 | — | — | — | 15.3 s |

   - **The source:** the join of the `match`'s arms, in loops or not. A
     `match` whose every arm returns has no such join.
   - **Even the barest arms grow so:** an arm that only sets one variable
     to a constant still grows about tenfold per doubling.
   - **Where the time goes in v2h's loop spelling:** the entailment state's
     `DerivationLedger::intern` and `join_at_once`.
5. **No `continue`.** An arm that leaves early for the next operation must
   nest everything after that point in an `else`. The `musttail` spelling
   used `return musttail` as a `continue` carrying new values.
6. **The cursor's step through the join after the `match`.** Each arm's new
   `pc` reaches the back edge through one join after the `match`. The step
   analysis sees that join's value, not the arm's own `pc + 1`, so each arm
   forms the cursor from the run (`madd` on AArch64) instead of stepping
   it. The fix is in the back end: read each arm's own incoming value at
   that join.

A floating loop variable travels in a floating register of the split: the
four-instruction interpreter with an `f64` accumulator reports "9 integer
and 1 floating" registers and runs correctly.

Findings 2 to 5 concern the checker and the language; finding 6 is a
lowering fix. Each one's design and the plan follow from the analyses of
the checker, the language and the back end.

## Stage 4: the gaps and the plan

The goal is that an interpreter written as `loop { match }` compiles to the
shape wasmi's and Silverfir-nano's handlers have and runs about as fast,
with every gap closed in the language or the compiler and none routed
around.

The inventory comes from three analyses run on the owner's instruction,
of the checker, the language and the back end. They used the compiler
built from `8a93bbb90`, wasmi 2.0.0's source and AArch64 code, and
Silverfir-nano's generated AArch64 handlers (`5f248e44`). Each witness in
[`witnesses/`](witnesses/) was rechecked with that compiler and has the
verdict listed below. Each is promoted into a compiler or conformance test
by the change that fixes it.

### The checker

**C1. A bound the specification derives, after a box-writing call in a
loop.** A checker defect: [MSR-4]'s step 6 is skipped for a subtraction's
lower bound, as finding 2 above details. The fix routes every finite
integer-domain component, with its right-hand term, through the complete
numeric disposition, across all operations rather than a subtraction
special case. No rule changes.
- `bound-after-call.wf`: refused [OP-2].
- `bound-no-call.wf`, `bound-requires.wf`: accepted.

**C2. A join loses a relation the writer stated as a loop invariant.**
Every input of the join proves the relation, but the joined state does
not hold it. Under [ENT-5]'s join, which takes the arms' exits, and
[ENT-6], which gives differing values a fresh image, this is what the
specification prescribes. It takes four forms:

| form | refused | accepted control |
|---|---|---|
| a scalar updated under a guard in one arm, untouched in another | `join-min.wf`, [INV-1] | `join-only-set.wf`, `join-only-keep.wf` |
| a box length after a call made in only one arm | `helper-min.wf`, [INV-1] | `helper-all.wf` |
| the relation needed after the join, before the back edge | `helper-post-use.wf`, [OP-4] | — |
| two variables updated together | `correlated-cache.wf`, [INV-1] | `correlated-one.wf` |

A ten-operation interpreter with calls, returns, memory and a frame
contract meets it (`rich.wf`; `rich-no-helper.wf` passes). So does v2h.

Proving the invariant on each edge into the back edge's join covers only
the first and second forms. The proposed rule (Q137) keeps the ordinary
join and transports the loop header's written relations across it:
- for each active header relation whose operands are live there, check it
  on every input with the existing disposition;
- where every input proves it, publish it over the joined values;
- at the back edge, prove the next header batch on each incoming edge.

This adds automatic facts, so [ENT-5], [ENT-6] and [INV-1] change. Its
cost is a fixed number of existing queries per relation and per input, not
an enumeration of paths.

**C3. Check time grows with the cube of a join's width** (finding 4).
- **Cause:** the checker joins every continuing exit at once, and the join
  builds every pair of the union of the inputs' rows, with a parent list
  over all inputs (`entailment/state.rs`): O(arms × rows²).
- **The specification:** it fixes the join's result, not this
  construction.
- **Fix, investigated first (Q140):** after C2's per-edge back-edge proof,
  build a join only where something reads it, and make the join exact but
  demanded, memoizing pairs.
- **Target, fixed before measuring:** linear growth in arms at fixed arm
  size, no more than 2.5 times per doubling. The 320-arm synthetic loop
  takes 35 s today.

### The language

**L1. No `continue`** (`continue.wf`, [FORM-1]; [GRAM-6] excludes it).
The proposal (Q138) is `continue;` and `continue @label;` with the current
values of the loop variables:
- it adds a back edge carrying [INV-1]'s and [RANGE-3]'s obligations;
- releases follow [STOR-3]'s edge rule;
- a counted loop takes its increment;
- [GIVE-1] counts it only as an escape from an enclosing initializer.

A value-carrying transfer is refused: plain assignment already lowers to
the edge's values.

**L2. Code validated once cannot be used.**
- **What exists:** a translator can prove `forall j: targets[j] < n`
  (`validate-targets.wf` accepted).
- **What is missing:** the executor cannot instantiate that fact where it
  reads a target ([RANGE-4], `range-use.wf`), and a range fact cannot name
  an enum payload's field ([RANGE-1], `range-payload.wf`).
- **The proposal (Q139):**
  - an explicit `use R(i)` of an active range clause inside a local
    invariant;
  - read provenance tying the value read to the version of the run it was
    read from;
  - range facts over a guarded variant's integer fields;
  - a source-order proof-event model, so that range and affine proofs
    compose without cycles.
- **Prior decision:** `design/language/checks-and-proofs/range-facts.md`
  refused exactly these, preferring guards. The proposal reopens it on a
  new consumer, immutable translated code.
- **Measured value:** none in time (the bounds counterfactual). The value
  is that a validated program needs no per-dispatch test and that a
  translator error is caught.

**L3. A validator returning `unit` cannot state its guarantee.**
`validate-targets-unit.wf` is refused [FN-9], while the same validator
returning a count is accepted. The proposal, part of Q139: a success route
selects a range postcondition whatever the payload's type, the payload
supplying only the terms it has.

**L4. Operations a complete wasm interpreter needs.** These go on demand,
not in this plan: a saturating float-to-integer conversion, and a 128-bit
vector type.

**What needs no language change:**
- The interpreter's state in registers is its loop variables, `f32` and
  `f64` included.
- Register, slot and immediate operand forms and superinstructions are
  ordinary variants.
- Dispatch through handler addresses stays a lowering.

### The back end

**B1. The cursor's step through the join after the `match`** (finding 6).
While emitting each arm, read the header's argument through the
predecessors that arm reaches:
- an equal step through a join keeps the step;
- mixed steps take a pointer phi of each predecessor's cursor;
- arbitrary targets form the cursor from the run.

**B2. Parameters that only travel.** `natural-loop.wf` passes its
`count0` and the enclosing frame through every arm unread. The fix is
liveness over real observations, applied to the dispatcher, the arms and
the entering call before the registers are counted.

**B3. The handler word's representation (Q135).**
- **Placement:** a compiler-private pointer word in every value of a
  selected enum, aligned at most to the enum's ceiling.
- **Several loops:** one word per dispatch family that fits the ceiling;
  an enum with more families than fit keeps the tag and the table.
- **Fragments:** a composition-wide layout plan, with hidden cross-fragment
  symbols.
- **Writers:** constructors write it, copies keep it, and replacing a whole
  value rewrites it.
- **Measured alternatives:** a 32-bit offset.

**B4. Interpreter state and register pressure (Q141).**
- **What fails today:** a loop whose changing values exceed the registers
  is not split at all (`loop-pressure.wf`: 37 integer registers needed,
  24 available). The spill order counts reading arms, not frequency, and
  only unchanging values spill.
- **The plan:** B2's liveness first. Then memory base and length carried
  as values that effects refresh. Then cold changing values kept in the
  enclosing frame, with spill choice weighted by frequency.

**B5. Frame-relative addressing has to rewrite uses.** On AArch64 the
frame-cursor prototype's `Copy` rebuilds the stack base
(`sub x8, x1, x23, lsl #3`) and adds the index back. Candidate 2 is not
adopted; reopen it only with a changed mechanism.

**B6. Load the next handler early**, within each arm, where its address
is valid and the code cannot change before the transfer.

**B7. Calls and returns.** Separate the interpreter's choices (copying
constants into every frame, the metadata's width) from lowering:
- carry the callee frame's address from the caller's;
- keep dead state out of helper calls;
- weigh an inline zero fill against the call to the C library.

Matching wasmi's call cost is worth about 2% of cycles.

**B8. Tag width and payload packing (Q142).**
- **Tag:** sized by the variant count, with fields packed under the
  ceilings.
- **Gain:** Halo's `Cell` shrinks from 12 bytes to 8.
- **Measurement:** alone, then with B3.

**B9. Bounds tests.** Guest memory tests stay. Code-validity tests go
only with L2's proof.

**B10. Split eligibility.**
- **Current scope:** only a function's first dispatch loop splits, and
  waiting functions, overlap groups and synthesized functions are emitted
  whole.
- **Plan:** widen this as natural witnesses require.

### The interpreter's own design (Q143)

wasmi's handlers keep values in registers and use immediates where v2h
reads frame slots, and its calls copy no constants. On the M5 those are
where v2h's remaining cycles go. They are expressible in WF today, so the
parity program changes v2h's translator to make the same choices. Any
form WF then refuses or compiles poorly becomes a gap above. Compiler and
interpreter changes are measured separately.

### Decided so far

The owner decided on 2026-10-07 and 2026-10-08:
- **Approved:** Q135 (A: a handler word only in enums a split loop
  matches, the layout plan a lowering input), Q137 (option B), Q138 (A),
  Q139 (A, all three parts), Q140, Q141 (A, with the register classes
  below), Q142 (A), Q143 (A) and Q145 (A with A2: a read instantiates range
  facts).
- **Moved earlier:** range facts used per element in ordinary proofs (Q139's
  first part and Q145 A2) leave phase 5 and follow the checker-completion
  change, owned by the checker-facts line.
- **Open:** Q144, the fork grain.

**R1. A run-time test the checker can decide is a redundant source form
(Q145).** An `if` whose condition the automatic derivation proves, or
refutes, at that point is rejected with a repair, as a redundant proof
block already is. It sweeps from programs the tests their proofs make dead,
and its yield grows with C2's and L2's facts. `docs/todo.md` holds the
entry.

**Closure evaluation.** Beyond C3's join, the owner proposed computing the
closure backward from each obligation within the closure space, instead
of closing every program point forward:
- a bound is a shortest path in a point's difference-bound graph;
- at a join it is the weakest of the predecessors' answers, memoized.

The results stay those [ENT-4] and [ENT-5] fix, with no SMT. Q140's
counters decide between demanded joins and this broader evaluation;
`docs/todo.md`'s check-time entry holds the criteria.

### Plan

| phase | work | decisions | exit |
|---|---|---|---|
| 1. The natural form checks | C1; C2's rule and per-edge back-edge proof; C3's demanded joins and the closure evaluation below; L1; R1 | Q137, Q138, Q140, Q145 | v2h written by `gen.py` as `loop { match }` checks in seconds, splits and runs CoreMark correctly. The `musttail` spelling leaves `gen.py` and the dispatch tests. |
| 2. Lowering on the natural form | B1, B2, B3, B4 | Q135, Q141 | The natural form reproduces the handler word's 1.10 on the 14900K, and Halo is not slower. |
| 3. The interpreter's design | v2h takes wasmi's register, immediate and call choices | Q143 | Each refusal it meets is a gap above. |
| 4. Layout and scheduling | B8, B6, B7 | Q142 | Each step under the measurement rule. |
| 5. Validated code | L2, L3, and R1's reading of range facts at a read if chosen | Q139, Q145 A2 | The executor carries no per-dispatch code test. |
| 6. The residual | attribute what remains against wasmi, then nano, per handler | — | Parity or an explained limit. |

The measurement rule throughout: CoreMark on the 14900K gains at least
2% per step, and Halo's `fib` and `loop` are not more than 2% slower.
Halo's interpreter, in Halo-wf, also uses the `musttail` spelling; its
move to `loop { match }` follows phase 1.

### Phase 1 outcome

Observed on [PR #270](https://github.com/Ming-Research/Whitefoot/pull/270)
at 8e33ca298, on an Apple M5:

- **The exit is reached.** v2h generated by `gen.py` as `loop { match }`
  checks in 7.6 s, splits into 318 arm functions, and runs CoreMark to the
  final CRC wasmi and Silverfir-nano produce (0x4983). Before C2 it was
  refused after 12.0 s at `let room = stack^.inner.len - 65536_u64;`.
- **Done:** C1 (two defects: the finite components used L0 only, and a
  measure operand had no term), L1, C2 with Q146 option A pending the
  owner, the dispatch tests written as `loop { match }`.
- **Part of B2 fell out of the tests' move.** Lowering hands every binding
  in scope to each join, so values the loop never reads counted as read and
  were carried. With joins read only through read parameters, the test
  interpreter takes 8 integer registers as its `musttail` spelling did, not
  11, and v2h takes 16 instead of 19.
- **R1 is not in #270.** Deciding by any one origin can reject a test whose
  deletion breaks a later proof
  ([witness](https://github.com/Ming-Research/Whitefoot/blob/claude/natural-loop-r1/research/investigations/redundant-tests/origin-publication.wf)),
  so the rule now decides only when every fact the branch would establish
  is derivable. It still rejects a decided test in 133 conformance cases
  and 15 programs. The migration waits for Q147, Q150, Q151 and Q152 on
  branch `claude/natural-loop-r1`.
- **C3 is not done.** One `set` per arm checks in 0.24 s, 1.55 s and 15.1 s
  at 160, 320 and 640 arms, as before: C2 removed only the joins that served
  induction.
- **Found:** a disequality with a constant does not tighten a bound (Q148);
  the old checker accepted a false counted next-slot invariant, which
  ENT-2(j) now refuses.

## Phase 2: lowering on the natural form

### Baseline

Observed on the 14900K with the gate's pinned LLVM, at main `1a6a96c5b`
plus only the temporary run on `claude/natural-lowering`
([run 37769271380](https://github.com/Ming-Research/Whitefoot/actions/runs/37769271380)):
CoreMark 2000 iterations, 7 interleaved launches per engine, every final
CRC 0x4983.

| engine | median score | of nano | spread |
|---|---:|---:|---:|
| v2h, `loop { match }` | 5449.6 | 0.698 | 0.8% |
| its twin | 5449.6 | 0.698 | 1.6% |
| v2h, the earlier `musttail` spelling (`gen.py` at `3142e15c6`) | 5405.4 | 0.692 | 1.4% |
| wasmi 2.0.0 | 6622.5 | 0.848 | 1.7% |
| Silverfir-nano `5f248e44` | 7812.5 | 1.000 | 0.8% |

Both spellings were compiled by the same compiler. The natural form now
splits as the `musttail` spelling did, into 318 arms over all 11 integer
argument registers with 4 unchanging values in the frame, and scores the
same within the spread. So phase 2 starts from the earlier attribution
([where the time goes](#stage-3-where-the-time-goes)) unchanged: the gap to
wasmi is 0.698 to 0.848. Compiling and checking take 23 s for the natural
form and 6.3 s for the `musttail` spelling on that host.

### The register classes (B4, Q141)

The owner directed B4 to start from what wasmi and Silverfir-nano keep in
registers, and to find general features a compiler can recognize in any
`loop { match }`, not wasm-specific ones; without such features their
register binding is out of reach. Read from wasmi 2.0.0's tail-call
handler signature and Silverfir-nano's x86-64 and AArch64 generators (source
only, nothing run):

| class | wasmi | Silverfir-nano | feature in a `loop { match }` |
|---|---|---|---|
| dispatch state | `ip`; the next handler, from the cell | `pc`; the next handler word, preloaded | the matched sequence's cursor `seq[i]`, `i` loop-carried |
| loop state few arms change | `sp`, the frame base; `instance`, changed at cross-instance calls | `frame`; `code_base`, changed at calls and returns | a loop-carried value most arms read and only some arms write |
| storage shape | `mem0`, `mem0_len`, refreshed after growth and host calls | `mem_base`, `mem_len`, refreshed on chain re-entry | a base or length of storage that only effects on that storage change |
| hot data | `ireg`, `freg32`, `freg64` | the accumulators and two pinned locals, chosen by its linker from the bytecode | the interpreter writer's own loop variables |
| invariant context | `store` | the entry state pointer | a value the loop never changes |

Owner's ruling: the compiler recognizes the dispatch state, loop state few
arms change, storage shape and invariant context by these features; hot
data is the interpreter's design, written as loop variables, which get
registers first; when values exceed the registers, values on the
loop-carried dependency chain keep their registers before invariant ones,
instead of ranking by frequency. The source reading widened the second
class from frame bases to any loop state few arms change, because wasmi's
instance pointer and nano's code base share the frame base's feature.

### Next: the handler word (B3, Q135 A)

Candidate 1's prototype gave 1.101 on the 14900K in the `musttail`
spelling; reproducing it on the natural form is the phase's exit. The
representation follows Q135 A: a compiler-private word in every value of
an enum that a split dispatch loop matches, written by its constructors,
kept by copies and rewritten by whole-value replacement, aligned at most to
the enum's ceiling so a 4-byte-aligned `Cell` admits it.

**Criterion, fixed before measuring.** CoreMark 2K on the 14900K against
main with a twin, 7 interleaved launches: adopted if the natural form's
median rises at least 2% and neither of Halo's `fib` and `loop`, once Halo
is written as `loop { match }`, is more than 2% slower. A result below 2%
rejects this representation on the natural form and reopens the choice
between an address and a 32-bit offset.

### Outcome of the handler word on the natural form

Observed on the 14900K with the gate's pinned LLVM
([run 37789304542](https://github.com/Ming-Research/Whitefoot/actions/runs/37789304542)):
the branch at `d63883029` against its merge base `1a6a96c5b`, both compiling
the same `gen.py` output, CoreMark 2000 iterations, 7 interleaved launches,
every final CRC 0x4983.

| engine | median score | of nano | spread |
|---|---:|---:|---:|
| v2h, `loop { match }`, the branch's compiler | 5988.0 | 0.763 | 0.6% |
| v2h, `loop { match }`, the merge base's compiler | 5449.6 | 0.695 | 1.1% |
| its twin | 5449.6 | 0.695 | 0.8% |
| v2h, the `musttail` spelling, the branch's compiler | 6006.0 | 0.766 | 1.5% |
| wasmi 2.0.0 | 6644.5 | 0.847 | 0.7% |
| Silverfir-nano `5f248e44` | 7843.1 | 1.000 | 0.4% |

The natural form gains 1.099 over its base, ahead in all 7 launch pairs,
with the twin equal to the base: the criterion's CoreMark half is met, and
the phase's exit value, the prototype's 1.10, is reproduced on the natural
form. The criterion's Halo half waits for Halo's interpreter written as
`loop { match }`. The remaining gap to wasmi is 0.763 to 0.847.

## Phase 3: the interpreter's design follows wasmi

The owner approved (Q143) that v2h's translator adopt wasmi's register,
immediate and call choices, as interpreter changes measured one at a time;
a form Whitefoot then refuses or compiles poorly is a gap to report. A
source reading of wasmi 2.0.0's translator and executor against `gen.py`
ordered the candidates:

1. A result that `local.set` or `local.tee` takes goes to the local's slot
   and to the accumulator in one operation (wasmi's `SlotAndReg` forms), so
   the next operation reading that local reads the accumulator; this
   targets the slot written and read back by the next operation (`I32AddD`
   4.7 cycles against `I32AddAD` 1.85 in the attribution).
2. Immediate operands for `i32` arithmetic, compare-branches and stores,
   instead of constants in frame slots.
3. Calls that copy no constants and carry the callee's entry in the
   operation.
4. The accumulator kept across operations until overwritten, with every
   integer operation having its accumulator forms.
5. Branches on `and` and on not-`and`.

Each step: CoreMark 2K on the 14900K against its base with a twin, adopted
at +2% or more.

**Step 0, observed** (hosted runner, run 37792262615, the branch's compiler
at `9b9f14ac3`):
- `I32Load`'s arm reads its four bytes with one 32-bit load after one
  bounds test: the host optimizer combines `gen.py`'s byte reads, so the
  per-byte spelling costs nothing and is not a gap.
- Its dispatch is the handler word's: the next element's word loaded, the
  cursor advanced, an indirect jump; the next-index bounds test remains.
- Dispatch counts on CoreMark 2K, highest first: `I32Add` 48.4M, `Copy`
  32.0M, `I32Load` 30.0M, `BrIf` 29.9M, `I32LoadD` 21.2M, `BrI32Ne` 20.9M,
  `Copy2` 17.0M, `I32Store` 15.3M; `Call` 3.0M. Calls are about 0.6% of
  dispatches, so step 3 is worth little on CoreMark; step 1 goes first.
