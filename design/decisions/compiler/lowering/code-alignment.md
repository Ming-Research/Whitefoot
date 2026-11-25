# Code alignment

Where the code of each compiled function starts, so that a change to one function does not move the time of another.

### decision every-compiled-function-starts-on-64-bytes
Every function the compiler emits, every definition of `ordinary_values.ll` and every function of the C runtime starts on a 64-byte boundary.
- replaced: Leaving each function where the linker put it was on main; the owner changed it, because one runtime import moved every function 16 bytes and an unchanged kernel failed the regression check; a fixed layout ties times to code (data: `research/investigations/code-placement/DESIGN.md`).
- replaced: Leaving function alignment out of the driver was on main; the owner changed it, because the controls behind that choice passed `-falign-functions=64` to the module's LLVM, which clang ignores, so no kernel function was aligned.
- rejected: an agent proposed aligning only the functions that run hot; the owner rejected it, because a kernel's time depends on where several of its functions fall, and which ones are hot is not known when the module is emitted.
- rejected: an agent proposed `-mbranches-within-32B-boundaries`; the owner rejected it, because it pads branches for one processor family's erratum and leaves each function's start where it falls, and the hosts that showed the effect are not of that family.
- rejected: an agent proposed measuring in a way that does not depend on placement and leaving the compiler as it was; the owner rejected it, because a program would still change speed by tens of percent when an unrelated import moved it.

### decision each-function-is-aligned-not-only-each-module
Each function is padded to the boundary, not only the start of each module's code.
- rejected: an agent proposed aligning only each module's code section; the owner rejected it, because a change to one function's size still moves every later function of that module within its line.

### decision the-module-states-its-own-function-alignment
The emitter writes `align 64` on each definition of the module, rather than passing an option to each host invocation.
- rejected: an agent proposed the internal LLVM option `-align-all-functions` at every host invocation; the owner rejected it, because the module reaches the host by the driver's link, by `--emit-llvm` users and by LTO, and an attribute on the definition reaches them all from one place.

### decision the-function-boundary-is-64-bytes-on-every-target
The function boundary is 64 bytes on every target.
- rejected: an agent proposed a boundary chosen for each target; the owner rejected it, because 64 bytes is the line of the x86-64 hosts where the effect was measured, and arm64's 128-byte lines cannot be measured on the hosted macOS runner.

### decision loop-headers-keep-llvm-alignment
Loop headers keep LLVM's own alignment; the compiler passes no `-falign-loops`.
- rejected: an agent proposed compiling the module with `-falign-loops=32`; the owner rejected it, because it met the experiment's selection rule on no host, and its one large gain came with a placement dependence that run did not locate (data: `research/investigations/code-placement/DESIGN.md`).