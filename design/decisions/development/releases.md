# Releases

How projects outside this repository get the compiler: where they live, the releases they pin, what a release holds, how a bundled release is qualified, and when releases are removed.

### decision downstream-projects-have-their-own-repositories
Whitefoot programs that are projects of their own, such as Firn, Halo and Snowghost, live in their own repositories with their own gates; this repository keeps the language, compiler and library.
- rejected: an agent proposed path-filtered application lanes in this repository's gate; the owner rejected it, because the applications would stay under this repository's merge rules, and the Redis and Lua reference tooling would enter its CI.
- rejected: an agent proposed one repository for every application; the owner rejected it, because Halo is a general Lua engine meant to stand alone, and every Firn push would run Halo's gate.

### decision downstreams-pin-a-published-release
A downstream pins by name a release made on request for a main commit whose gate passed, `wf-` and 12 hex digits, or for an unmerged commit, `wf-exp-`, pinned only on an experiment branch.
- rejected: an agent proposed a release per specification version; the owner rejected it, because the compiler changes within one version, so a downstream pinned to a version would get every such change.
- rejected: an agent proposed a submodule of this repository in each downstream that builds the compiler; the owner rejected it, because the downstream would compile this repository in its own CI and carry its whole tree for one executable.

### decision releases-carry-their-language-files
A release also holds `whitefoot-language.tar.gz`, with its commit's specification and standard-library interface files, listed in the manifest and in `SHA256SUMS`.
- replaced: A release that carries no documents, with writers reading this repository at the release's commit, was on main; the owner changed it, because writers then depend on this repository's layout and network access to it, which an external user lacks.
- rejected: an agent proposed a copy of the newest specification kept in Whitefoot-kit; the owner rejected it, because each downstream pins a different release, so the newest specification differs from the one its compiler takes.

### decision releases-are-made-by-hand-and-expire
A release is made only by hand, and a weekly run removes releases more than 30 days old, except the newest lightweight release of a main commit and bundled releases of main commits.
- rejected: an agent proposed a release of every main commit, or a daily one; the owner rejected it, because a release is made when a downstream needs a commit, so each release already has a user.
- rejected: an agent proposed keeping every release; the owner rejected it, because the owner moves every downstream to the newest release from time to time, so older releases lose their users.

### decision the-gate-builds-no-downstream-program
This repository's gate builds no downstream program; a compiler change that breaks one shows when the periodic upgrade moves that downstream to a newer release.
- rejected: an agent proposed a scheduled run of each downstream against main's newest compiler; the owner rejected it, because an agent's periodic upgrade runs the same downstream gates and adapts the code in the change that moves the pin.

### decision lightweight-releases-stay-the-default
The release workflow's `bundle_toolchain` input defaults to false, and a lightweight release's Linux compiler uses the gate's pinned LLVM major, recorded as `linux_llvm_major` for Whitefoot-kit.
- rejected: an agent proposed making every release carry its toolchain; the owner rejected it, because existing downstreams take frequent small releases and already install that toolchain, and existing tags have extraction consumers.

### decision bundled-releases-carry-their-toolchain
With `bundle_toolchain=true`, a `-bundled` release holds Linux x86-64, macOS arm64 and Windows x86-64 packages of compiler, clang, lld, headers, runtime objects and a manifest recording clang's answers.
- replaced: Releases that use the host's clang, a bundled clang being deferred to the first formal release, was on main; the owner changed it, because the first release for outside users must work once installed, which the host's clang cannot promise.

### decision the-release-records-its-clangs-answers
The release asks the staged clang the compiler's probes once, with the staged compiler, and writes the answers and the clang's version line into the package manifest before sealing the archive.
- rejected: an agent proposed a separate command-line option that prints the answers; the owner rejected it, because it adds a user-visible switch, while the compiler's `--report` already says where its answers came from.
- replaced: A package that probed its clang in every process, though no release step had asked that clang anything, was on main; the owner changed it, because the answers are fixed once the package is.

### decision windows-bundle-uses-llvm-mingw
The Windows package is a zip holding a statically linked `whitefootc.exe` and a trimmed, pinned llvm-mingw toolchain, and it exists only in bundled mode.
- rejected: an agent proposed a Windows package completed by the user's Visual Studio Build Tools; the owner rejected it, because the user would first have to install several gigabytes of Visual Studio components, so the package would not work once installed.
- rejected: an agent proposed redistributing the Windows SDK and Visual C++ headers and libraries in the package; the owner rejected it, because their license does not allow it.
- rejected: an agent proposed shipping Microsoft's Visual C++ runtime DLLs beside a dynamically linked compiler; the owner rejected it, because static linking gives the same result with no extra files to track.
- rejected: an agent proposed a tar.gz Windows archive like the other platforms; the owner rejected it, because Windows Explorer cannot open it, so users would need an extra command-line step.

### decision bundled-releases-publish-after-acceptance
A bundled release is published only after the acceptance workflow passes every required check on the exact candidate archive bytes; the workflow's own revision supplies the checks.
- rejected: an agent proposed taking the acceptance files from the candidate commit; the owner rejected it, because a candidate commit may predate those files.
- rejected: an agent proposed publishing before acceptance, or counting a trimming smoke test as qualification; the owner rejected it, because only a check of the delivered archive shows that it finds its own toolchain and keeps native behavior.

### decision acceptance-runs-the-gate-tests-with-the-packaged-clang
Acceptance runs the candidate's gate compiler tests and conformance cases with each archive's own clang, test executables placed in the extracted package and every host clang denied.
- rejected: an agent proposed an environment variable naming the clang for the compiler and its tests; the owner rejected it, because it gives every user a toolchain switch and does not test the installed layout.
- replaced: A release whose acceptance never ran the compiler tests with the packaged clang was on main; the owner changed it, because users received a compiler and clang pair no test had run.

### decision windows-package-is-qualified-in-a-clean-container
The Windows archive is installed by the commands of the getting started guide inside a stock Windows Server Core container, and its compiler builds and runs the checks there.
- rejected: an agent proposed hiding the runner's clang and qualifying on the hosted Windows runner; the owner rejected it, because the runner holds Rust, LLVM, Visual Studio and the Visual C++ runtime, so a package that used any of them would still pass.
- rejected: an agent proposed running the documentation example test against the package's compiler; the owner rejected it, because that test needs Rust, which the container does not hold.

### decision unix-packages-are-qualified-by-the-guide
The Linux and macOS archives are installed by the commands of their getting started section, in a stock Ubuntu container or a sandbox denying the host's clang, and the guide's programs are run.
- rejected: an agent proposed extracting the archive by the check's own command and comparing the guide's text with it; the owner rejected it, because equal text does not show that the commands work, and it builds none of the guide's programs.
- rejected: an agent proposed running the documentation example test against the package's compiler; the owner rejected it, because it needs Rust and a host linker, so the check would no longer show a clean machine.

### decision bundled-main-releases-never-expire
The weekly removal never removes a bundled release of a main commit by age, and that release never takes the newest release's place, which stays with the newest lightweight release.
- rejected: an agent proposed the 30-day rule unchanged for bundled releases; the owner rejected it, because an external user's pinned release would vanish after a month, and a bundled release in the newest place would let downstreams' pinned release expire.
- rejected: an agent proposed a separate, longer retention window for bundled releases; the owner rejected it, because it adds a number to choose and still ends in an expiry the external user meets.