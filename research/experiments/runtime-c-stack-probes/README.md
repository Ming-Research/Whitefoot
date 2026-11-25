# Stack probes in the runtime's C frames over a page

Every function the compiler generates carries a stack-probe attribute, so a
frame larger than the guard page touches each page on its way down and a
stack exhaustion ends in the floor's `{"resource":"stack"}` record. The
runtime's own C units are compiled by clang with the driver's arguments
(`unit_arguments` in `compiler/src/bin/whitefootc.rs`), which named no
stack-probe argument. This bundle records which runtime C frames exceed a
page on each gate and package compiler, which probes each compiler puts in
them under each candidate argument, and whether the repository's frame test
reads them.

## Question

1. Which runtime C frames exceed 4096 bytes, or have a dynamic size, on
   Linux x86-64 and macOS arm64?
2. Linux, LLVM 22 (the gate's `/usr/bin/clang` and the Linux bundle's major):
   does the driver's `-fstack-clash-protection` put an inline probe in each
   such frame, and is there none without it?
3. macOS arm64, Apple's clang (the macOS gate's `/usr/bin/clang`): does the
   unflagged compile already probe each such frame, by which method, and is
   `-fstack-check` / `-fno-stack-check` the switch that turns it on and off?
4. macOS arm64, upstream clang 22.1.8 (the macOS bundle's clang): is there
   any probe without an argument; does the driver forward
   `-fstack-clash-protection` on Darwin; does the cc1 option
   `-Xclang -fstack-clash-protection` give inline probes?

## Comparison and rejection criteria

Written before the results were read. [`probe.sh`](probe.sh) compiles every
runtime C unit the driver compiles on a POSIX host with the driver's
arguments (`-std=c11 -pthread -I… -O2 -falign-functions=64`, and the
restricted child arguments for the two fork-child units) plus one variant's
arguments, under `-S -fstack-usage`, and prints for each frame over 4096
bytes or of dynamic size the stack lines of its assembly body, the
stack-probe options the driver hands to cc1, and the `probe-stack`
attribute clang gives `wf_file_wait_readiness`. A frame counts as probed when
its body touches each page it allocates: `movq $0, (%rsp)` in a page loop on
x86-64, `str xzr, [sp]` on AArch64, or a call to `___chkstk_darwin`.

- The Linux argument `-fstack-clash-protection` is rejected if a frame over a
  page lacks a probe with it, or if the frame shows a probe without it (then
  the frame test could not tell the two apart).
- Leaving Apple's clang to its default, or naming `-fstack-check` for it, is
  rejected if a frame over a page lacks `___chkstk_darwin` in either compile,
  and `-fstack-check` is rejected as the switch if `-fno-stack-check` still
  leaves the call in place.
- `-Xclang -fstack-clash-protection` for upstream clang on Darwin is
  rejected if the driver's own `-fstack-clash-protection` already gives the
  probes (the driver flag would then do), or if the cc1 option leaves a frame
  over a page without `str xzr, [sp]`.

## Method

The probe matrix and the frame test ran in the temporary workflow
`tmp-runtime-probes.yml` of the work branch of item 208 (its step "Frames and
probes under each variant" is this script inline), on GitHub's hosted runners:
`ubuntu-24.04` with LLVM 22 from apt.llvm.org as the gate installs it,
`macos-15` with its `/usr/bin/clang`, and `macos-15` with the official
LLVM 22.1.8 macOS arm64 archive that `.github/toolchain/llvm-assets.json`
pins, its digest checked, and the SDK from `xcrun`. To repeat a variant from
the repository root:

```sh
bash research/experiments/runtime-c-stack-probes/probe.sh /usr/bin/clang "" clash-driver -fstack-clash-protection
```
