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

The frame test is `runtime_frames::every_runtime_c_frame_over_a_page_is_probed`
in `compiler/src/bin/whitefootc/runtime_frames.rs` (during the trials, in
`compiler/src/bin/whitefootc.rs`), which compiles each runtime C unit with the
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

## The implementation's runs

The arguments are chosen by one function,
`whitefoot::runtime_stack_probe_arguments` (the target's stack-probe record
and, on Darwin, the helper capability probe of the clang in use), and
`unit_arguments` appends them for the uncached link, the restricted child
units and the cached objects alike. The frame test compiles every unit any
link compiles, the hand-written `ordinary_values.ll` included, with the
driver's arguments for each, and counts a frame over a page or of dynamic size
as probed only when its body holds both halves of one inline method (the
one-page step of the stack pointer and the touch of that page) or references
`___chkstk_darwin`; a lone store of zero at the stack pointer is what an
ordinary local's clearing also compiles to.

Run
[45731597744](https://github.com/Ming-Research/Whitefoot/actions/runs/45731597744)
at `dbf2ad2ac` (jobs 139614682818 on `ubuntu-24.04` with LLVM 22 and
139614682219 on `macos-15`), written as notices; each ablated row empties the
named argument arm and gives `wf_std.text.host_copy_bytes` in
`ordinary_values.ll` a 65536-byte alloca whose address it stores where its
callee reads:

| Compiler | Arguments | Result |
| --- | --- | --- |
| Linux LLVM 22 | `-fstack-clash-protection` | passes; the body steps `subq $4096, %rsp` and stores `movq $0, (%rsp)` |
| Linux LLVM 22, ablated | none | fails naming `wf_file_wait_readiness` (32808, static) and `wf_std.text.host_copy_bytes` (65560, static) |
| Apple clang 17 | `-fstack-check` | passes; the body references `___chkstk_darwin` |
| Apple clang 17, ablated | `-fno-stack-check` | fails naming `wf_file_wait_readiness` (32832) and `wf_std.text.host_copy_bytes` (65584) |
| upstream 22.1.8 as packaged | `-Xclang -fstack-clash-protection` | passes; the body steps `sub sp, sp, #1, lsl #12` and stores `str xzr, [sp]` |
| upstream 22.1.8 as packaged, ablated | none | fails naming `wf_file_wait_readiness` (32832) and `wf_std.text.host_copy_bytes` (65584) |

The Linux definition's body also confirms the x86-64 page step that the plan
had inferred from LLVM's inline-probe lowering. The hand-written definition
stayed unprobed under Apple's clang too, whose driver asks cc1 for
`-fstack-check` by default and was given no probe argument for the IR unit;
the attribute that makes a probe is part of the IR text, which clang does not
add when it compiles IR (read from LLVM's behaviour, not tested further). So
the LLVM unit's frames are covered by this test and by an attribute written in
its text, not by a compile argument.

With the cc1 option in place, `whitefootc` beside the upstream 22.1.8 clang
(a package directory holding the compiler and a `clang` symlink) built and ran
`tests/package-fixtures/waiting.wf` (exit 0) and `stack-exhaustion.wf` (exit
134 with `{"resource":"stack"}`) in the ordinary, `--full-lto`, `--cache` and
`--cache --fragments` link modes, which is the same job's second half.

The acceptance script's comparison (`.github/toolchain/check-runtime-frames.sh`)
was run in the same job on the line the test prints when it runs outside any
package, which names Apple's `/usr/bin/clang`: it fails with "the runtime was
compiled by another clang than the package's", status 1. On the line the test
prints beside the package's clang it passes, status 0.

The acceptance's frame check, through the release workflow's push dry run
(`compiler-release.yml`, which builds and packages the pushed commit as a
bundled package and runs `package-acceptance.yml` on it, publishing nothing).
Every row is a job of the acceptance's qualification, which ends with
`check-runtime-frames.sh` on the installed package: in the stock
`ubuntu:24.04` container on Linux, under the host-clang denial on macOS.

| Commit | Run | Frame check |
| --- | --- | --- |
| `3441d6632` (the implementation) | [45729610580](https://github.com/Ming-Research/Whitefoot/actions/runs/45729610580) | macOS job 139611446773 and Linux jobs 139611446749 and 139611446782 pass it: the test ran once, passed, and printed the installed package's `bin/clang-22`, `clang version 22.1.8 (… ca7933e47d3a…)`; the Linux jobs then failed at their summary line, as above |
| `87e97069c`, the Linux and Darwin-inline arguments emptied | [45731598465](https://github.com/Ming-Research/Whitefoot/actions/runs/45731598465) | fails on every platform, jobs 139617593428 (macOS), 139617593473 and 139617593443 (Linux): `wf_file_wait_readiness` 32832 and 32808 static, unprobed, probes `[]`, test status 101 |
| `536e2a917`, only the three acceptance files on the main before this work | [45731598441](https://github.com/Ming-Research/Whitefoot/actions/runs/45731598441) | fails on every platform, jobs 139617605116 (macOS), 139617605124 and 139617605072 (Linux): `running 0 tests`, "runtime frame test did not run exactly one passing test" |

Defects these runs found in the first trials of the check, each fixed:

- The first ablation of the LLVM unit stored one byte into a 64 KiB alloca.
  SROA shrinks an alloca that only a volatile store reaches to that byte, so
  the unit had no large frame and the test rightly reported none (runs
  [45682401654](https://github.com/Ming-Research/Whitefoot/actions/runs/45682401654)
  and [45729610330](https://github.com/Ming-Research/Whitefoot/actions/runs/45729610330)
  held the verdict `failed=1` for that reason, and for the next point). The
  ablation now keeps the frame by letting its address leave the function.
- That workflow compared the packaged clang by the symlink's path, while the
  resolution prints the resolved one (`clang-22`); the comparison is by real
  path, as the acceptance script's is.
- The frame test named its assembly and report by each unit's stem, so
  `ordinary_values.c` and `ordinary_values.ll` shared both and a unit that left
  no report could have been read through the other's; each unit now writes
  its own `<file name>.s` and `.su`.
- The release dry run of `3441d6632`, run
  [45729610580](https://github.com/Ming-Research/Whitefoot/actions/runs/45729610580),
  ran the frame check green on both packages (macOS under the host-clang
  denial, job 139611446773; Linux in the container, jobs 139611446749 and
  139611446782: the installed package's `clang-22`, one test passed) and then
  failed the two Linux jobs at the summary line, a `printf` whose quoted text
  held an apostrophe.

## Limitations

The runs read assembly; none executes a runtime C frame into the guard page.
Containment by each method rests on the qualification of the same methods
for generated code ([bundled-toolchain qualification](../../investigations/bundled-toolchain/DESIGN.md#qualification-run)).
Only Linux x86-64 and macOS arm64 were compiled; Linux AArch64, x86-64
Darwin and the Windows MinGW target were not. The test compiles each unit
without link-time optimization; a link with `-flto` keeps each definition's
attribute in its bitcode, which was read from LLVM's attribute handling and
not observed here. The four link modes ran with the cc1 option and the
programs behaved, which shows the option does not break them, not that the
linked code carries the probes.
