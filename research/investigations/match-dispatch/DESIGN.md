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

The writer may equally make each arm end in `return musttail run(vm, ...)`;
the guaranteed self-tail transfer [FN-10] already lowers to a jump back to the
function entry, so both spellings reach the compiler as the same loop.

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
3. Write a wasm 2.0 interpreter in Whitefoot that runs CoreMark, following
   Silverfir-nano's interpreter design (predecoded folded cells, an
   accumulator, register-resident locals as loop-carried state), and compare
   it with Silverfir-nano on the same CoreMark module.
