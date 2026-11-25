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

The frame test is `tests::every_runtime_c_frame_over_a_page_is_probed` in
`compiler/src/bin/whitefootc.rs`, which compiles each runtime C unit with the
driver's own `unit_arguments` and reads the same report and assembly. On
macOS it also ran against upstream clang 22.1.8 as the bundle selects it: a
copy of the test binary in a directory beside a `clang` symlink to the
pinned archive's clang, which the toolchain resolution then takes as a
packaged clang (its compiles carry `-isysroot` with the SDK `xcrun` finds,
as the packaged mode adds).

## Result

Measured on 2026-11-25 on hosted runners: `ubuntu-24.04` with Ubuntu clang
22.1.8 from apt.llvm.org; `macos-15` (Xcode 16.4) with Apple clang 17.0.0
(clang-1700.0.13.5); `macos-15` with upstream clang 22.1.8 (commit
`ca7933e47d3a`). The probe matrix is run
[45616283880](https://github.com/Ming-Research/Whitefoot/actions/runs/45616283880)
(commit `9991ad8de`, jobs 139212402634, 139212403079 and 139212403198), each
job's probe lines also kept as its notice.

1. One runtime C frame exceeds a page and none has a dynamic size, on both
   hosts and under every variant: `wf_file_wait_readiness` in
   `completion/file_posix.c`, whose `struct pollfd polled[4096]` makes a
   32808-byte frame on Linux x86-64 and a 32832-byte frame on macOS arm64.
   The allocator unit `heap.c` and the hand-written `ordinary_values.ll` have
   none. The two 4096-entry arrays in `completion/bridge.c` are fields of a
   driver record, not locals.
2. Linux LLVM 22: unflagged, the definition has no `probe-stack` attribute
   and the 32 KiB frame is allocated with no store into it. With
   `-fstack-clash-protection` the driver hands the option to cc1, the
   definition carries `"probe-stack"="inline-asm"`, and the body walks the
   frame a page at a time with `movq $0, (%rsp)`.
3. Apple clang 17: its driver hands `-fstack-check` to cc1 unasked, the
   definition carries `"probe-stack"="__chkstk_darwin"` and the body calls
   `___chkstk_darwin` through the GOT before it moves `sp` by 32 KiB.
   `-fstack-check` gives the same; `-fno-stack-check` leaves no attribute and
   no call. The driver's `-fstack-clash-protection` is reported as an unused
   argument and changes nothing; `-Xclang -fstack-clash-protection` replaces
   the helper with inline probing.
4. Upstream clang 22.1.8 on Darwin: unflagged, no attribute and no probe.
   The driver's `-fstack-clash-protection` is reported as an unused argument
   and changes nothing, as its `RenderSCPOptions` forwards the option only
   for FreeBSD, Linux and Fuchsia targets
   ([Clang.cpp at llvmorg-22.1.8](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/clang/lib/Driver/ToolChains/Clang.cpp)).
   `-fstack-check` is accepted without a word and changes nothing.
   `-Xclang -fstack-clash-protection` gives `"probe-stack"="inline-asm"` and
   a page loop of `sub sp, sp, #1, lsl #12` and `str xzr, [sp]`.

No rejection criterion is met: the Linux flag probes the frame and its
absence leaves it unprobed; Apple's helper is there by default and with
`-fstack-check`, and `-fno-stack-check` removes it; upstream Darwin needs the
cc1 option, which probes the frame, while its driver's flag does nothing.

The frame test, on the same day:

| Compiler and arguments | Run, job | Result |
| --- | --- | --- |
| Linux LLVM 22, no probe argument | [45612683884](https://github.com/Ming-Research/Whitefoot/actions/runs/45612683884), 139200011375 | fails (exit 101) |
| Linux LLVM 22, `-fstack-clash-protection` | [45612684106](https://github.com/Ming-Research/Whitefoot/actions/runs/45612684106), 139200011343 | passes |
| Apple clang 17, no argument | [45612683884](https://github.com/Ming-Research/Whitefoot/actions/runs/45612683884), 139200011234 | passes |
| Apple clang 17, `-fno-stack-check` | [45614872788](https://github.com/Ming-Research/Whitefoot/actions/runs/45614872788), 139207555727 | fails: `wf_file_wait_readiness 32832 static`, unprobed |
| upstream 22.1.8 as packaged, `-fno-stack-check` (ignored) | same job | fails: `wf_file_wait_readiness 32832 static`, unprobed |
| Apple clang 17, `-Xclang -fstack-clash-protection` | [45614872796](https://github.com/Ming-Research/Whitefoot/actions/runs/45614872796), 139207554717 | passes |
| upstream 22.1.8 as packaged, `-Xclang -fstack-clash-protection` | same job | passes |

The first row's failure message is in its job log, which was not read: the
API that serves logs refused this work's token for its rate limit that day,
and the job's annotations hold only the exit status. The first three rows ran at commits `f6f2cb899` and `40ace2e97`, the rest
at `587911df0` and `1d5a54f1a`; each commit names its arguments. Before
those commits the test matched a function's label against the whole line,
while clang writes the label with a comment after it (`name:  # @name`), so
every body it read was empty and every large frame counted as unprobed
whatever the arguments: the red of the earlier trial at `92152974b` with
`-fstack-clash-protection` was that defect, not a missing probe.

## Limitations

The runs read assembly; none executes a runtime C frame into the guard page.
Containment by each method rests on the qualification of the same methods
for generated code ([bundled-toolchain qualification](../../investigations/bundled-toolchain/DESIGN.md#qualification-run)).
Only Linux x86-64 and macOS arm64 were compiled; Linux AArch64, x86-64
Darwin and the Windows MinGW target were not. The test compiles each unit
without link-time optimization; a link with `-flto` keeps each definition's
attribute in its bitcode, which was read from LLVM's attribute handling and
not observed here.
