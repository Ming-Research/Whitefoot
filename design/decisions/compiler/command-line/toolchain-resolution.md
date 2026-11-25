# Toolchain resolution

Which clang, linker and SDK the compiler runs, how a packaged installation is told from a source build, and the one Windows target.

### decision sibling-clang-marks-a-packaged-install
The compiler uses the `clang` beside it; a `toolchain-manifest.json` above `bin/` marks a package, where a missing clang is a toolchain error, and records that clang's answers; else the host default.
- rejected: an agent proposed inferring source-build mode from a missing clang alone; the owner rejected it, because deleting a component must not turn a qualified bundle into a compiler that depends on the host.
- rejected: an agent proposed requiring every source build to stage a release layout; the owner rejected it, because existing source builds and small releases still use their configured host toolchain.
- replaced: A manifest that only recognized a package and decided no behavior was on main; the owner changed it, because the package's clang is fixed at release, so its probe answers are fixed and can be recorded there.

### decision one-resolved-toolchain-for-every-step
The resolved toolchain serves native compilation, runtime C construction, capability probes, stack reports, every link and the tests' own clang commands; a packaged probe failure is a toolchain error.
- rejected: an agent proposed resolving clang separately at each invocation; the owner rejected it, because independent selection can probe one LLVM consumer and emit to another.
- rejected: an agent proposed treating a package's probe failures as syntax rejection; the owner rejected it, because an unusable selected tool is different from an unsupported form.
- replaced: Starting the host's default clang directly in tests and test helpers was on main; the owner changed it, because a test run in a package would then compile with another clang than the package's.

### decision a-package-takes-its-clangs-answers-from-its-manifest
A packaged compiler takes the probe answers recorded in its manifest when the recorded clang version is the package clang's own, and otherwise probes and says why in a note and in `--report`.
- replaced: Every process of a packaged compiler probing its own clang, with the manifest deciding nothing, was on main; the owner changed it, because the package's clang is fixed, so the answers are fixed, and the release can show that clang compiles the tests.

### decision packaged-linux-links-with-bundled-lld
In packaged mode on Linux, every ordinary, full-LTO and cached ThinLTO link passes the bundled `ld.lld` alias by its absolute path through `--ld-path`, with no host fallback.
- rejected: an agent proposed canonicalizing the alias to the generic `lld`; the owner rejected it, because lld's dispatcher selects the ELF driver by the executable's name.
- rejected: an agent proposed relying on PATH for the linker; the owner rejected it, because a partial bundle must not silently use another LLVM installation.

### decision packaged-macos-uses-the-system-sdk-and-linker
In packaged mode on macOS, the compiler takes the SDK from a valid `SDKROOT` or from `xcrun`, finds Apple's linker with `xcrun`, and passes them by `-isysroot` and `--ld-path`.
- rejected: an agent proposed substituting `ld64.lld` for Apple's linker; the owner rejected it, because the selected link contract keeps Apple's linker with the bundled libLTO.

### decision native-cache-keys-name-the-toolchain
Native-cache toolchain identity includes the resolved clang path and its first version line, the selected linker path, and the selected macOS SDK path.
- rejected: an agent proposed keying reused runtime and native products only by clang's version or path; the owner rejected it, because the linker and SDK choices affect native construction even when clang's version line is unchanged.

### decision windows-targets-mingw-only
Windows x86-64 programs target `x86_64-w64-windows-gnu`, linked by LLD's MinGW driver, as the single Windows ABI, whatever Rust environment built whitefootc.
- replaced: The MSVC target, taken from the Rust environment whitefootc was built for, was on main; the owner changed it, because it needs the Visual Studio linker, CRT libraries and Windows SDK, which a package may not carry, so users would first install gigabytes of Microsoft tools.
