# A bundled native toolchain for Whitefoot's first external release

Status: research and proposed implementation plan, written before measurement.
The owner has committed to bundling clang and lld at the gate's LLVM major and
to preserving macOS stack probing with upstream LLVM. This document selects
candidates and rejection checks; it does not claim an implemented or qualified
package. No compiler, specification, gate, or approval log changes accompany it.

## Question and fixed requirements

How can the release that ships with Firn build native programs independently
of the host's clang, while retaining stack containment on macOS and the
existing ordinary, full-LTO, and cached ThinLTO build paths?

The two completion conditions are:

1. Each external compiler package contains its own clang and lld, matching
   [the gate's major pin](../../../.github/llvm-major), currently 22. The
   packaged compiler and its capability probes invoke that clang. A program
   that exercises coroutine lowering compiles, links, and returns its expected
   result both without an installed host clang and with a different host
   clang major.
2. Upstream LLVM 22.1.8, or a subsequently qualified version at the pinned
   major, generates Darwin native programs without the reported unsupported
   probing error. Large-frame containment and recursion-exhaustion floor
   tests pass, and native code retains probes. This is a prerequisite for the
   macOS package, not an optional package optimization.

The [constitution's safety constraint](../../../docs/constitution.md#safety)
rules out removing probes: a frame can cross a guard page and write into
another mapping. Resource exhaustion being outside source outcomes in
[the specification](../../../spec/kernel-spec.md) does not permit corruption.
No language-rule change is proposed.

Initial release scope recommended here is the existing Linux x86-64 and
macOS arm64 package pair. x86-64 Darwin probing is investigated separately;
Windows packaging is designed below but adding its release lane remains an
owner choice. Bundling a compiler is not bundling the operating system's SDK,
C library, or native development libraries.

## Repository evidence and affected responsibilities

The starting tree is `af7665be672d29912d878dd5da8c3932bb6f296c`. Source inspection
establishes the following; the reported LLVM failure comes from the task and
was not reproduced on this workstation.

| Owner in the repository | Present behavior and consequence |
| --- | --- |
| [Target layout](../../../compiler/src/target.rs), `TargetLayout::for_triple` | Both Darwin architectures select `__chkstk_darwin`; Linux selects `inline-asm`; Windows MSVC selects `__chkstk`. Backend capability is currently conflated with target ABI. |
| [Emitter](../../../compiler/src/backend/emitter.rs), `emit_module`'s attribute group, and [fragments](../../../compiler/src/backend/fragments.rs), `split_emission` | Generated definitions share the target probe attribute; fragment emission retains referenced attribute groups. Any replacement must cover helpers and fragments as well as source functions. |
| [Toolchain selection](../../../compiler/src/toolchain.rs), `clang_executable`, `ToolchainFacts::probe` | Unix uses `/usr/bin/clang`, Windows uses PATH `clang`. Three once-per-process probes select `preserve_nonecc`, `captures(none)` versus `nocapture`, and the result type of `llvm.coro.end`. |
| [Native driver](../../../compiler/src/bin/whitefootc.rs), `compile_executable`, `link_cached_objects`, `print_stack_ledger`, `host_compiler_identity` | Native builds, runtime C compilation, assembly/stack reports, and cache identity use the shared clang selector. macOS LTO uses the system linker; other LTO paths request lld. Cache identity currently includes clang path and version output. |
| [Exhaustion floor](../../../compiler/src/backend/wf_floor.c) | Fault classification depends on probe geometry: host page size plus red-zone allowance. Enlarging that band to conceal a missing probe would misclassify wild faults. |
| [Exhaustion tests](../../../compiler/src/backend/tests/exhaustion.rs) and [stack-ledger tests](../../../compiler/src/backend/tests/stack_ledger.rs) | They check IR attachment, a large frame on a guarded stack, a deliberately unprobed negative control, and successful/aborting recursion around the measured boundary on entry and worker stacks. The macOS assembly recognizer currently searches for `chkstk`. |
| [Release workflow](../../../.github/workflows/compiler-release.yml) | Packages contain only `whitefootc`; the language documents are a separate archive. Linux x86-64 uses apt.llvm.org; macOS arm64 uses Apple clang. The smoke program is `geometry_vectors`, not a coroutine witness. |

The [downstream-release decision](../../../design/compiler/downstream-releases.md)
explicitly deferred bundling until the first formal release. That condition
has now arrived. Its package-size estimate was not a measurement. The
[backend-facts decision](../../../design/compiler/backend-facts.md) deliberately
probes the actual consumer because Apple and upstream version numbers do not
identify accepted LLVM syntax. The [floor decision](../../../design/compiler/resource-exhaustion-floor.md)
requires probes on every generated function. The
[verification decision](../../../design/compiler/verification.md) and
[incremental-compilation decision](../../../design/compiler/incremental-compilation.md)
also matter: qualify the selected LLVM in CI and retain existing linking
behavior. These are historical grounds, not substitutes for the specification.

## Darwin stack probing: source evidence and candidates

All upstream implementation references below are pinned to `llvmorg-22.1.8`.
Reading a lowering path establishes a candidate, not a successful macOS run.

### Supported forms

| Form | Upstream AArch64 Darwin | Upstream x86-64 Darwin | Link-time requirement |
| --- | --- | --- | --- |
| `"probe-stack"="inline-asm"` | Accepted by `AArch64FunctionInfo::AArch64FunctionInfo`; uses inline probing. | Accepted by `X86TargetLowering::hasInlineStackProbe`; selects inline lowering. | No external stack-probe helper. Ordinary system libraries are still needed. |
| `"stack-probe-size"="4096"` | Sets the interval, but does not enable probing by itself on non-Windows. | Sets the interval/threshold used by probing, but does not enable it alone on Darwin. | None by itself; pair it with an enabled probe method. |
| `"probe-stack"="__chkstk_darwin"` | Rejected: non-Windows accepts only the nonempty value `inline-asm`. This explains the supplied failure. | Treated as a requested helper name, not rejected by the same AArch64 check. | A definition with exactly the backend's probe ABI, register preservation, and symbol spelling. Do not assume a normal C function is sufficient. |
| Another helper name, or `"inline"` | A nonempty value other than `inline-asm` is rejected. | A non-special value is a helper name, so `inline` is not an alias for inline probing. | The corresponding helper must exist and implement the special ABI. |
| No method, or probe suppression | Does not establish the required protection. | Does not establish the required protection. | Unacceptable as a production remedy. |

Sources: [AArch64MachineFunctionInfo.cpp, constructor, lines 116–145](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/AArch64/AArch64MachineFunctionInfo.cpp#L116-L145);
[X86ISelLowering.cpp, `hasInlineStackProbe`, `getStackProbeSymbolName`, `getStackProbeSize`](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/X86/X86ISelLowering.cpp#L58478-L58526).
Both default to 4096 bytes; AArch64 first consults a module `stack-probe-size`
flag when the function attribute is absent. Explicit 4096 makes
the intended maximum probe interval inspectable and avoids relying on a
default. It is deliberately no larger than the smallest supported guard page;
a 16 KiB host page does not require increasing the interval to 16 KiB.
The [LLVM language reference's function attributes](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/docs/LangRef.rst#L2251)
describe `stack-probe-size` as controlling probes already enabled by a method
or ABI; inlining can propagate a method and merges probe sizes conservatively.

[AArch64FrameLowering.cpp, `inlineStackProbeFixed` and `inlineStackProbe`](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/AArch64/AArch64FrameLowering.cpp#L3535-L3632)
expand fixed and variable allocation pseudos: fixed frames use unrolled or
looping stack descent with stores, plus residual handling. On x86,
[X86FrameLowering.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/X86/X86FrameLowering.cpp#L593),
`emitStackProbe`, `emitStackProbeInline`, and `emitStackProbeCall` distinguish
inline walks from helper calls. The helper path supplies size through the
backend's register convention; resolving a symbol alone is not an ABI proof.
Neither source path supplies a portable Darwin helper implementation for us.

For the current Apple toolchain, the repository uses `__chkstk_darwin` and
expects a helper call. That is evidence of the maintained path, not a proof
that every Apple compiler supports the same attributes. Apple's exact shipping
backend source and the helper's supported ABI/availability across deployment
targets were not verified here. The upstream
[Darwin probing proposal](https://github.com/llvm/llvm-project/pull/210812)
is still draft in the consulted source; it is not part of 22.1.8 and cannot
justify its helper behavior. No guarantee is made for every future version.
Apple's [libpthread `pthread_asm.s`](https://github.com/apple-oss-distributions/libpthread/blob/main/src/pthread_asm.s),
`_thread_chkstk_darwin` / `____chkstk_darwin`, contains arm64 and x86-64
system-side routines, including stack-bound checks and alternate-stack page
walks. This is not evidence for the public link symbol or deployment baseline
needed by a particular clang. A helper need not touch every page if its
qualified boundary check provides equivalent containment; the inline candidate
must perform the page walk it promises.

### Choice and rejection checks

| Candidate | Benefit and cost | Deciding comparison | Reject when |
| --- | --- | --- | --- |
| Inline probing with explicit 4096 for both upstream Darwin architectures; retain the qualified Apple helper while Apple remains supported (**recommended initial plan**) | Fits upstream source without a new assembly runtime. Requires a toolchain-aware emission fact rather than OS-only selection; two qualified producer families remain temporarily. | Compile the same large-frame IR with upstream and Apple, inspect the selected native method, then run the unchanged behavioral floor oracles. | Missing inline page touches, failed helper containment, backend error, incorrect unwind/frame accounting, changed fault classification, or failed positive/negative control. |
| One inline form for upstream and Apple | Removes vendor-specific probe selection. Apple's support and behavior are unverified. | Give the identical inline+4096 module to both exact compilers on arm64 and x86-64, including ordinary, large, aligned, and variable frames and LTO. | Any supported Apple toolchain rejects it, ignores it, or fails containment/recursion tests. Adopt only after the whole declared support matrix passes. |
| Keep the Darwin helper everywhere by upgrading or patching LLVM | May align with future Darwin conventions, but adds a different version qualification or a maintained LLVM fork. | Identify a released implementation, check its helper ABI and deployment requirements, and repeat the full native checks. | Unreleased-only support, missing helper, ABI mismatch, or inability to stay on the owner's pinned major. Not needed for the inline candidate. |
| Write a Whitefoot probe helper | Owns delicate architecture-specific pre-prologue assembly, unwinding, and Darwin ABI evolution; does not bypass AArch64's rejection of helper forms. | First establish backend support for invoking it, then independently check preservation and every guard crossing. | Unsupported invocation, ABI uncertainty, or any containment failure. Not recommended while inline lowering exists. |

The proposed upstream attribute group is an LLVM IR fragment, not a complete
program:

```llvm
attributes #0 = { "probe-stack"="inline-asm" "stack-probe-size"="4096" }
```

The initial Apple path retains its existing helper attribute until qualified
evidence permits consolidation. Probe selection must use the resolved backend
toolchain and target together; parsing an Apple marketing major alone is
insufficient. A small source-backed toolchain profile, checked by native CI,
is preferable to introducing an unproved custom stack routine. Failure to
qualify a toolchain is a toolchain error, never a fallback to unprobed code.

### Preserve the independent floor oracles

The existing macOS `chkstk` substring test would reject valid inline probing.
Update it to inspect the chosen architecture's descent-and-touch sequence,
including loop termination and stride, while retaining the runtime assertion:
the probed large frame aborts with the stack record, the ablated frame retains
the host protection signal with no record. An IR attribute alone is not proof.

Keep the negative control's isolation of the fill helper from inlining. Its
removal of the selected function's probe attribute is essential on upstream
AArch64: the non-Windows constructor does not use `no-stack-arg-probe` to
suppress an explicit method. Do not add a module-wide method that silently
re-enables this negative control. Check ordinary small-frame code, entry and
worker recursion boundaries, helper definitions, fragments, and post-LTO code.
Review C runtime prologues and SDK calls separately: adding an IR attribute
does not automatically instrument runtime C translation units. Keep the
floor's classification band unchanged unless a separately justified geometry
change is required; passing a wider band is not containment evidence.
The floor comment currently describes probe stride as the host page size;
with explicit 4096 on a host with larger pages, revise that explanation to
state the actual upper bound. Do not change the signal policy merely to
preserve the wording.

## Toolchain acquisition, contents, and installation contract

### Acquisition alternatives

The [official LLVM 22.1.8 asset listing](https://github.com/llvm/llvm-project/releases/expanded_assets/llvmorg-22.1.8)
advertises these compressed download sizes. They are publisher metadata, not
local measurements or estimates of a trimmed Whitefoot archive:

| Platform | Published input | Listed compressed size |
| --- | --- | --- |
| Linux x86-64 | `LLVM-22.1.8-Linux-X64.tar.xz` | 1.81 GB |
| Linux arm64 | `LLVM-22.1.8-Linux-ARM64.tar.xz` | 1.65 GB |
| macOS arm64 | `LLVM-22.1.8-macOS-ARM64.tar.xz` | 1.38 GB |
| Windows x86-64 | `clang+llvm-22.1.8-x86_64-pc-windows-msvc.tar.xz` | 822 MB |
| Windows x86-64 installer | `LLVM-22.1.8-win64.exe` | 434 MB |
| macOS x86-64 | No corresponding prebuilt asset listed | Not available from this listing |

| Source | Cost and recommended use | Check that decides it | Reject when |
| --- | --- | --- | --- |
| Official release archive (**first candidate on current platforms**) | Large acquisition download but no LLVM build; trim once in release CI and cache the verified source artifact. Exact patch and checksum are selectable. | Extract in CI, inventory dependencies and resources, relocate the selected subset, then run clean-host, coroutine, floor and all link-mode checks. | Missing needed component, unrelocatable binary, unsupported host baseline, or failure of a required check. |
| Versioned packages from [apt.llvm.org](https://apt.llvm.org/) | Fits the existing Linux gate and can reduce downloaded components. Debian packages are an installation layout, not automatically a relocatable SDK. No macOS/Windows source. | Pin exact package versions/hashes; extract clang, lld, resources/compiler-rt, LLVM/Clang shared dependencies; inspect paths and re-run on a host without those packages installed. | Hidden `/usr/lib/llvm-*` dependency, missing library closure, incompatible baseline, or only a floating package set can be reproduced. |
| Build upstream LLVM from a pinned source tag | Controls architectures, distribution contents and baseline. Highest CI/storage/maintenance cost; possible fallback for Intel macOS or unsuitable official binaries. | Use LLVM's [distribution build mechanism](https://llvm.org/docs/BuildingADistribution.html), enable clang/lld/compiler-rt and required LTO support, qualify the installed tree by the same tests. | It cannot deliver the platform/ABI contract, or prebuilt inputs already meet it without this maintenance. |

Do not silently switch sources after a qualification failure: record the
failure and select the next candidate. Match major 22 to the gate; additionally
record the exact patch, platform asset, hash, and package recipe. A major pin
alone does not freeze the bytes, dependencies, or future backend behavior.

Expected trimmed size is **an engineering estimate only**: roughly 150–400 MiB
compressed for Linux x86-64, 200–500 MiB for macOS arm64 including libLTO, and
150–400 MiB for a future Windows x86-64 package. These broad ranges allow for
shared-library versus monolithic builds and duplicate code across clang/lld.
No archive was downloaded or inventoried. They are planning allowances, not
an acceptance ceiling; the earlier 100–200 MB estimate is not established.
Measure the actual `.tar.gz`/Windows archive after trimming, identify its
largest retained components, and revisit acquisition if it exceeds the range.
Do not delete required runtimes to fit a guessed size.

### Proposed relocatable layout

```text
package/
  bin/whitefootc[.exe]
  bin/clang[.exe]          # include its real versioned target, if an alias
  bin/lld[.exe]
  bin/ld.lld              # ELF driver alias where applicable
  bin/ld64.lld            # Mach-O alias; shipped, not selected on macOS
  bin/lld-link.exe        # Windows alias when that platform ships
  lib/clang/22/include/   # retain the complete builtin header directory
  lib/clang/22/lib/...    # target compiler-rt/builtins, preserving input layout
  lib/libLTO.dylib        # macOS system-linker LTO integration
  lib/... or bin/...      # actual shared-library/DLL dependency closure
  share/licenses/...
  toolchain-manifest.json
```

The input distribution may use target-specific resource paths; preserve them
and verify clang's resource-dir result after relocation rather than flattening
them. Preserve aliases inside the package, never links back to the build
machine. Include external `cc1` components if that distribution uses them.
The driver normally includes its frontend and integrated assembler; do not
assume this for an uninspected distribution. Compiler-rt builtins may be needed
for operations emitted by native lowering even when the smoke test needs none.

Keep builtin headers because the embedded Whitefoot runtime is compiled from
C; `stddef.h`, `stdatomic.h`, and target intrinsics can come from clang's
resource directory. System headers/startup objects and libc remain host
prerequisites. Full LLVM development headers, static development libraries,
debuggers, tests, documentation, and unrelated language frontends are not
required by this compiler's current invocation path. C++ headers are not
needed to compile the C runtime, but clang/lld binaries themselves may depend
on a C++ runtime shared library, which belongs in the dependency inventory.

On Linux retain or explicitly require the selected executable's ELF dependency
closure and a C development environment (libc headers, startup objects, and
the driver-selected support libraries). A container with `libc6-dev` and any
required GCC runtime/development support but **no clang** satisfies this
boundary; a libc-only image may not. Choosing compiler-rt over the driver's
default support library is a separate link change, not implied by bundling it.
The minimum glibc and OS versions must be established from the chosen binaries
and clean-host execution; Ubuntu 24.04 as the current build host does not
establish a broader compatibility promise.

Ship LLVM's [Apache-2.0 WITH LLVM-exception license](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/LICENSE.TXT),
the applicable [compiler-rt license](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/compiler-rt/LICENSE.TXT),
and notices/licenses for every retained third-party component, including any
zlib, zstd, XML, terminal, or C++ runtime library actually included. Preserve
copyrights and upstream notices. The exact list follows the selected artifact's
inventory; it has not been verified by inspecting archive contents. Apple SDK
and Microsoft SDK/CRT redistribution are outside this proposed bundle.

### Resolution and capability probes

Resolve `current_exe()` once, use its directory's `clang[.exe]`, and carry a
single resolved toolchain object through native compilation, runtime C builds,
capability probes, stack reports, full LTO and fragment links. Handle paths as
OS paths and arguments, not shell strings; test spaces, relocation and an
invocation through a symlink. A missing bundled clang or lld reports the
expected path and an incomplete-package error; it must not search PATH or fall
back to `/usr/bin/clang`. Explicitly select the packaged ELF/COFF linker by
absolute path so PATH cannot select another major. macOS is the exception
fixed by the commitment: its selected linker is the system one.

Source-build CI needs to stage the same layout around the compiler and test
executables, or use an explicit internal toolchain configuration. Do not make
an absent package trigger an implicit host fallback just to preserve developer
convenience. The exact internal API is for implementation, not a public option
to bypass the release contract.

Bundling makes the three syntax/ABI answers constant **for a particular
qualified artifact and target**, not automatically constants of the Whitefoot
binary or LLVM major. Recommendation: retain the existing once-per-process
probes and point all three at the resolved bundled executable. Cost: three
small compiler invocations when native emission first needs the facts. The
alternative is packaging-time generated facts, coupled to a verified artifact
identity; compare them with live probes on every shipped platform and reject
that alternative on any disagreement or an independently replaceable toolchain.
It adds no needed evidence for the first release. In particular, a coroutine
intrinsic declaration alone does not validate a call's result type.

An inability to execute a required probe or create its files must be reported
as an environment/toolchain failure in the packaged path, distinguished from
an actually unsupported optional form. Today's conservative fallback on launch
failure is a related implementation concern, not evidence that the bundled
toolchain accepts the selected form. Retain source judgment independently.
Include the exact toolchain identity and relevant SDK/linker selection in
native-cache identity so changing the bundle or SDK cannot reuse incompatible
runtime/native products merely because clang's version string stayed equal.

## macOS SDK, linker, and host prerequisites

Require Apple's Command Line Tools or a selected Xcode installation. Apple's
[installation documentation](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools/)
states that CLT contains the macOS SDK and toolchain also shipped with Xcode.
Upstream clang does not replace SDK headers, `.tbd` system library interfaces,
or Apple's linker.

Proposed resolution: honor a valid explicit `SDKROOT` for a macOS SDK;
otherwise call `/usr/bin/xcrun --sdk macosx --show-sdk-path`. Resolve Apple's
linker with `/usr/bin/xcrun --sdk macosx --find ld`, respecting the selected
developer directory. Pass `-isysroot <SDK>` to every relevant C/IR/link command
and `--ld-path=<absolute-system-ld>` when linking. Validate an explicitly
invalid SDKROOT with a clear diagnostic instead of quietly switching SDKs.
Do not assume an upstream driver installed outside Xcode invokes xcrun itself.

Source grounds: LLVM 22.1.8
[Darwin.cpp, `Darwin::AddDeploymentTarget`](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/clang/lib/Driver/ToolChains/Darwin.cpp)
uses an explicit sysroot and otherwise considers a valid absolute `SDKROOT`;
[ToolChain.cpp, `ToolChain::GetLinkerPath`](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/clang/lib/Driver/ToolChain.cpp)
supports explicit linker selection. Inspect `clang -###` in CI to prove the
actual arguments and executable paths, then prove them with a real link.

Preserve system ld64, as the owner specified. Shipping lld's Mach-O driver
does not authorize using it. That alternative offers LLVM-matched LTO inside
lld but changes link semantics, flags and platform qualification; it is
rejected for this scope. If system linking fails, bring the concrete failure
back rather than silently substituting lld.

**The package must also preserve LTO.** In
[Darwin.cpp, `darwin::Linker::AddLinkArgs`, lines 238–254](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/clang/lib/Driver/ToolChains/Darwin.cpp#L238-L254),
upstream clang passes system ld64 a path to its sibling `../lib/libLTO.dylib`
for applicable linker versions; the source explains that mismatched LLVM
versions will not work. Bundle that library and its dynamic dependencies.
Check ordinary, full-LTO and cached ThinLTO links against each declared
minimum SDK/linker pair. Passing one non-LTO smoke test cannot qualify the
package. No compatibility result for upstream 22.1.8 libLTO with a particular
Apple linker was obtained here.

If SDK or linker discovery fails, stop before the first C compile with a
toolchain diagnostic naming the missing component, selected developer path,
and failed discovery command. Suggest installing CLT with
`xcode-select --install` or selecting an existing Xcode/CLT installation;
do not install it automatically. Missing bundled clang and missing Apple SDK
are different errors. Minimum macOS, CLT and deployment-target versions remain
to be qualified; SDK version alone is not the deployment target.

There is a literal acceptance-boundary issue: installing ordinary CLT also
installs Apple clang. A completely CLT-free Mac lacks the required SDK/linker.
Prove independence from clang on macOS by denying execution of all host clangs
while retaining the SDK/linker, and record this separately from the literal
clang-absent Linux container. Hiding PATH alone does not block `/usr/bin/clang`.
The owner should confirm this interpretation before calling macOS acceptance
complete; it does not weaken the system SDK/linker requirement.

## Windows boundary

The existing target is x86-64 MSVC, not MinGW. If Windows joins the first
release, package upstream `clang.exe`, the resource tree, `lld-link.exe` and
their DLL dependencies at the same major. Retain the MSVC `__chkstk` contract;
the Darwin change must not alter it. LLVM's
[Windows toolchain documentation](https://clang.llvm.org/docs/UsersManual.html#clang-cl)
describes the Visual Studio environment and MSVC/SDK integration. Bundled LLVM
does not provide the Windows SDK/UCRT/MSVC libraries and headers. Require a
qualified Visual Studio Build Tools/SDK installation or its configured build
environment; do not promise a compiler-only unzip works on a bare Windows host.

The check is a Windows host with those libraries/headers but no host clang,
then a different-major clang first on PATH: build and run the coroutine witness
and all supported link modes with the package. Missing SDK and missing packaged
DLLs must produce distinct actionable failures. Cost is a new build, archive,
host-qualification and CI lane; the current release workflow has none. Defer
that lane unless Firn's first release requires Windows, while leaving the
layout and resolver portable. Linux arm64 and Intel macOS similarly require
their own package and native qualification before becoming release promises.

## Acceptance experiments and CI cost

These are plans, not commands run or passing results. Run only in CI, with
heavy commands under `.github/run-check.pl`; begin with one tiny IR object
probe, one native link and one focused floor case before scaling. Record their
actual durations separately from toolchain acquisition and Rust construction.
Do not raise a budget, drop a check, or use a research fixture in the ordinary
gate to make the experiment green.
Each stage's deciding result is also its acceptance evidence. Reuse that run
when the accepted code and package remain applicable; repeat it only after a
change that could alter its result. The final evidence review does not require
another run solely to mark acceptance complete.

| Stage | Evidence required and falsifier | Cost shape |
| --- | --- | --- |
| Darwin candidate screen | On hosted arm64 macOS with exact upstream 22.1.8, compile the current helper attribute to capture its failure; compile inline+4096 to object and assembly. Cross-emit the x86-64 candidate, then run it on a native Intel runner before claiming Intel runtime support. A frame without a retained touch sequence fails. | One LLVM download/cache restore per host; a handful of tiny compilations. Cross-emission does not replace native execution. |
| Floor qualification | Run `backend::tests::exhaustion` and `backend::tests::stack_ledger`, including `a_frame_larger_than_the_guard_region_is_still_reported` and `the_reported_ceiling_is_the_measured_one`, plus the native floor runtime group. Preserve positive/negative controls and entry/worker recursion. Reject backend errors, wrong signals/records, lost probing or an ablation that still probes. | Reuse one Rust gate-profile test build; focused cases first, then the existing groups. No new performance benchmark. |
| Coroutine package smoke | Extract the actual final archive into a fresh path. Compile a current-language waiting call and bound spawn whose result is used, verify emitted `llvm.coro.*` calls, link and run with a fixed expected result. Repeat ordinary/full-LTO/cached-fragment paths with cold and reused cache. An optimized-away or noncoroutine smoke program fails the witness check. | Several small native builds/runs; one package extraction. |
| Linux no clang | In a clean runtime/development container with no clang/LLVM packages or host toolchain mounts, copy only the package and maintained witness. Inventory installed packages; verify no host clang paths exist. Build/link/run; trace executed paths to prove bundled clang/lld handled probes and compilation. Missing resources/dependencies fail. | Reuse the release compiler; one small container per declared baseline. No Rust/LLVM rebuild inside it. |
| Wrong host major | Repeat with a known different LLVM major, record both toolchain identities, assert the majors differ, set host clang first on PATH, and instrument calls. Host `/usr/bin/clang` must also be different on Linux. Success requires expected program output and zero host-clang invocations, not just a successful build. | A second container image on Linux; use the existing Apple clang on macOS only when its LLVM lineage establishes a different major. Otherwise provision a known different-major upstream host clang; an Apple marketing version or vendor difference alone is insufficient. |
| macOS host-clang isolation | Retain real CLT SDK/ld, deny host-clang execution using a validated disposable-runner sandbox/fixture, then run the package witness. First prove the denial by an attempted host invocation. Invalid SDKROOT/missing selected SDK or ld must fail with the intended diagnostic. | One hosted macOS job; denial mechanism still needs a feasibility check. If a hosted runner cannot enforce it, report unverified and use an isolated CI VM, not a PATH-only substitute. |
| Relocation and completeness | Move the unpacked tree to a path containing spaces, invoke through a symlink and from an unrelated cwd. Remove bundled clang, linker, resource input, and macOS libLTO individually in disposable copies; each exercised missing component must fail, never use a host replacement. Inspect dynamic dependency paths. | Small additional invocations reusing artifacts; LTO negative case needed for libLTO. |
| Final release qualification | Gate the exact implementation revision on both platforms; dry-run the release workflow and qualify its exact archives before publication. Record compiler/package hashes, LLVM patch/major, SDK/linker, selected test cases and exit status. | Existing full gate plus package jobs; publishing remains a later owner action. |

The task calls the witness “await.” In this tree the source form is `waits`
plus ordinary waiting calls or `spawn`, with bound-result use producing the
join; the backend represents the operation as `ContextAwait`. Do not invent
an `await` token or use a hand-written LLVM module as the end-to-end source
witness. [The existing waiting-program case](../../../compiler/src/backend/tests/payload_enums.rs),
`WAITING_PROGRAM` / `union_enums_cross_waiting_calls_and_contexts`, supplies a
current-language example to reduce into a maintained package fixture. Retain
checks for both coroutine IR and the program result. The fixture belongs with
formal tests, not under this investigation.

For planning only, allow tens of runner-minutes for the first uncached macOS
and Linux qualification jobs, with download and Rust build likely larger than
the small smoke cases; this is not a measured estimate or a timeout rule.
The current budget table lists macOS `compiler/test-build-unit` at 300 s and
`compiler/test-unit` at 175 s, references for existing stages, not predictions
for this toolchain. Source-building LLVM is materially larger and must begin
with a scoped CI feasibility sample. New stage budgets require actual CI
evidence and the repository's budget procedure. No precision timing on this
MacBook or a new 14900K measurement is needed for this design.

## Recommended ordered delivery plan

1. **Set the release support boundary.** Confirm Linux x86-64/macOS arm64,
   system SDK/linker prerequisites, and macOS host-clang isolation semantics.
   Select exact 22.1.8 input artifacts and provisional OS baselines. Deliver
   owner decisions and a reproducible toolchain inventory plan.
2. **Qualify the Darwin probe candidate first.** In temporary hosted CI,
   establish the reported helper failure and inline candidate behavior on
   upstream LLVM; compare the maintained Apple path. Deliver disassembly,
   large-frame positive/negative controls and recursion-floor results.
   Any failure blocks the macOS bundle; do not suppress probing.
3. **Implement the qualified probe choice.** Move backend-specific probe
   facts out of the OS-only assumption, retain all generated-definition and
   fragment coverage, and update the native recognizer without weakening its
   oracle. Deliver passing floor/ledger/runtime groups and the full gate.
4. **Construct relocatable packages.** In parallel with probe work after
   step 1, inventory and trim official assets, retaining resources, lld,
   shared dependencies, licenses and macOS libLTO. Deliver archives with
   measured sizes, hashes, exact versions and declared host prerequisites.
   Reject/replace the acquisition candidate if relocation fails.
5. **Unify compiler toolchain resolution.** Use the sibling clang for every
   probe and native path, explicit packaged lld where applicable, and explicit
   macOS SDK/system linker. Retain dynamic capability probes and coherent
   native-cache identity. Deliver intended diagnostics and no-host-fallback
   negative cases, including ordinary/full/ThinLTO success.
6. **Prove release acceptance with the actual archives.** After steps 3–5,
   run coroutine witnesses in clang-absent and wrong-major environments,
   macOS isolation, relocation and package-completeness checks. Deliver the
   exact CI artifacts/results and every remaining platform limitation.
7. **Integrate the release contract and review.** Update downstream-release,
   backend-facts, exhaustion-floor and verification decisions where their
   grounds change; bring release notes/manifest/consumer extraction guidance
   current. File Whitefoot-kit/Firn changes to their responsible sessions.
   Run independent review and the exact-revision gate. Obtain owner approvals,
   then record approvals and prepare the release for owner-authorized delivery.

No upstream or downstream repository is changed by this investigation. A
single compiler-only tar extraction consumer will need adaptation for the new
layout; keep that dependency visible rather than retaining a second package
path that continues to use host clang.

## Owner decisions needed before implementation

These questions refine the two settled commitments; bundling and keeping
probes are not being reopened.

| Question | Options and recommendation | Ground and confidence |
| --- | --- | --- |
| Which platforms ship with Firn first? | **A: existing Linux x86-64/macOS arm64 pair (recommended)**. B: add Windows x86-64 now, accepting its SDK, packaging and CI lane. C: also add Intel macOS/Linux arm64 with native qualification. | Current release scope is concrete; Firn's desired public support matrix remains the owner's decision. 3/5. |
| Which probing policy should we qualify first? | **A: upstream inline+4096, retain Apple's qualified helper initially (recommended)**. B: require one inline form for Apple and upstream before delivery. C: wait for an upstream helper implementation. | Exact upstream source supports A; runtime qualification and Apple-common-form evidence remain outstanding. 4/5 for choosing the experiment, not for claiming safety validation. |
| Which toolchain source should package CI try first? | **A: exact official archives, trimmed and dependency-checked (recommended)**. B: apt.llvm.org extraction for Linux and official macOS. C: build selected LLVM distributions ourselves. | Available assets reduce new build responsibility; actual portability and final size are unknown. 3/5. |
| Does macOS acceptance mean no dependency on an executable host clang while retaining CLT/Xcode's SDK and ld? | **A: yes; prove invocation denial and retain the system SDK/linker (recommended)**. B: require literally no host clang files, using an isolated CI image retaining only the necessary Apple development components. | Ordinary CLT includes clang. PATH hiding cannot prove either option. A is testable in principle; the isolation mechanism remains unverified. 4/5 for the boundary, 2/5 for hosted enforcement. |
| Should capability probes become packaged constants now? | A: generate facts and bind them to exact artifacts. **B: keep the three runtime probes against sibling clang (recommended)**. | This preserves the existing semantic consumer check without a second fact-production pipeline. No startup-cost measurement justifies changing it. 4/5. |

## Unverified work and findings disposition

- **Not run:** any compiler build, clang invocation, test, package extraction,
  download, measurement or CI workflow. The investigation is source research
  only; no acceptance condition is marked complete.
- **Safety qualification outstanding:** Darwin inline native execution,
  post-LTO probing, aligned/variable frames, unwinding and stack-ledger
  correspondence; Apple inline support; exact Apple helper ABI/deployment
  availability; runtime C prologue coverage. These belong in probe qualification,
  not a suppression workaround.
- **Package qualification outstanding:** archive member/dependency inventories,
  final compressed sizes, minimum OS/glibc/CLT versions, resource layouts,
  macOS libLTO/system-ld compatibility, symlinks and macOS distribution signing
  behavior, Windows SDK/CRT/DLL requirements, and the complete retained-license
  inventory. These belong in packaging CI before a public support claim.
- **Acceptance mechanics outstanding:** macOS invocation-denial feasibility,
  a native Intel macOS runner if that target is promised, and the reduced
  maintained coroutine witness. No Linux container result can establish the
  Darwin floor behavior.
- **Found along the way, included in the proposed work:** macOS's helper-only
  native recognizer; macOS libLTO dependency; release-workflow comments claiming
  build-time capability facts despite the current runtime probes; native cache
  identity and consumer extraction assumptions. No unrelated repair was made.
