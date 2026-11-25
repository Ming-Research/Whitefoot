# Profiler unwinding

The unwind rules the compiler emits and the layout of the Linux link, which let a sampling profiler follow a Whitefoot call stack to each caller.

### decision every-emitted-function-carries-unwind-tables
Every function the compiler emits carries LLVM's `uwtable` attribute, so even a leaf that cannot throw has unwind rules a profiler can follow to its caller.
- replaced: Unwind tables only for functions that can unwind by exception was on main; the owner changed it, because a hot leaf had none and its samples lost their callers, while the tables cost 208 bytes of `.eh_frame` in the witness (data: `research/experiments/unwind-resume/README.md`).
- rejected: an agent proposed keeping frame pointers for profiling; the owner rejected it, because they take a register from every function, for a need the unwind tables meet without changing the emitted code.

### decision the-linux-lto-link-keeps-code-at-its-file-offset
On Linux, the LTO link passes `-z separate-code` to LLD, so the code segment's virtual address equals its file offset.
- replaced: LLD's default layout was on main; the owner changed it, because under it perf 6.17 found no caller in an LLD-linked C program, while `-z separate-code` or the default linker found every one, at 992 bytes more in the witness (data: `research/experiments/unwind-resume/README.md`).
- rejected: an agent proposed linking with a different linker; the owner rejected it, because that would replace the one LLD the toolchain pins.
- rejected: an agent proposed only documenting the profiler's limit for downstream projects; the owner rejected it, because perf 6.17 finds no caller above an LLD-linked program's resume frame, and the fix costs 992 bytes in the witness (data: `research/experiments/unwind-resume/README.md`).