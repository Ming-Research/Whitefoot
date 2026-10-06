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
