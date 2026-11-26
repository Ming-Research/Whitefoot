# Getting started

Whitefoot checks a program's safety before it builds an executable. This
guide installs the compiler, runs a small program, and follows one rejection
to a working repair.

## Install

Choose a bundled release, whose tag is `wf-<12 hex digits>-bundled`, on the
[GitHub releases page](https://github.com/Ming-Research/Whitefoot/releases).
Set its exact tag once, replacing the example below:

```sh
release_tag='wf-<12 hex digits>-bundled'
```

The package carries its own clang and lld toolchain. You do not need a
separate host clang installation or Rust. It also contains the
specification, standard library interfaces, licenses,
`toolchain-manifest.json`, and checksums for the extracted files.

### Linux x86-64

Ubuntu 24.04 is the qualified Linux host. Other distributions are
unqualified. Install the libraries the bundled toolchain needs:

```sh
sudo apt-get install libc6-dev libgcc-13-dev libstdc++6 zlib1g libxml2 libicu74 liblzma5
```

Download the Linux archive and its published checksum file, then verify it:

```sh
curl -LO "https://github.com/Ming-Research/Whitefoot/releases/download/${release_tag}/whitefootc-linux-x86_64-bundled.tar.gz"
curl -LO "https://github.com/Ming-Research/Whitefoot/releases/download/${release_tag}/SHA256SUMS"
sha256sum -c --ignore-missing SHA256SUMS
```

The check must report `whitefootc-linux-x86_64-bundled.tar.gz: OK` before
you extract it:

```sh
mkdir -p whitefoot
tar -xzf whitefootc-linux-x86_64-bundled.tar.gz -C whitefoot
```

### macOS arm64

Install the Command Line Tools for the SDK and the system linker:

```sh
xcode-select --install
```

Download the macOS archive and its published checksum file, then verify it:

```sh
curl -LO "https://github.com/Ming-Research/Whitefoot/releases/download/${release_tag}/whitefootc-macos-arm64-bundled.tar.gz"
curl -LO "https://github.com/Ming-Research/Whitefoot/releases/download/${release_tag}/SHA256SUMS"
shasum -a 256 -c --ignore-missing SHA256SUMS
```

The check must report `whitefootc-macos-arm64-bundled.tar.gz: OK` before
you extract it:

```sh
mkdir -p whitefoot
tar -xzf whitefootc-macos-arm64-bundled.tar.gz -C whitefoot
```

If you downloaded through a browser instead, remove the quarantine
attribute after extraction so the unsigned binaries can run:

```sh
xattr -dr com.apple.quarantine whitefoot
```

### Put the compiler on PATH on Linux and macOS

The archive has no enclosing directory, so extraction puts `bin/` directly
under `whitefoot/`. On either platform:

```sh
export PATH="$PWD/whitefoot/bin:$PATH"
whitefootc --help
```

Keep the extracted package together: `bin/whitefootc` uses the bundled
toolchain beside it. Add the `PATH` setting, with an absolute installation
path, to your shell's startup file if you want it in later sessions.

### Windows x86-64

Windows Server 2022 is the qualified Windows host. Other Windows releases
are unqualified. The package needs no Visual Studio, Windows SDK, Visual C++
runtime, Rust or LLVM: it carries its toolchain, and the programs it builds
import only Windows's own libraries. The commands below are for PowerShell.
Set the release tag once, replacing the example:

```powershell
$release_tag = 'wf-<12 hex digits>-bundled'
```

Download the Windows archive and its published checksum file. The progress
display makes downloads in Windows PowerShell very slow, so turn it off first:

```powershell
$ProgressPreference = 'SilentlyContinue'
$base = "https://github.com/Ming-Research/Whitefoot/releases/download/$release_tag"
Invoke-WebRequest "$base/whitefootc-windows-x86_64-bundled.zip" -OutFile whitefootc-windows-x86_64-bundled.zip
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
```

Verify the archive against the checksum file:

```powershell
$archive = 'whitefootc-windows-x86_64-bundled.zip'
$expected = (Get-Content SHA256SUMS | Where-Object { $_ -like "*$archive" }).Substring(0, 64)
$actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
if ($actual -eq $expected) { "${archive}: OK" } else { "${archive}: FAILED" }
```

The check must report `whitefootc-windows-x86_64-bundled.zip: OK` before
you extract it. The archive has no enclosing directory, so extraction puts
`bin\` directly under `whitefoot\`:

```powershell
Expand-Archive $archive -DestinationPath whitefoot
```

Put the compiler on the `PATH` of this PowerShell session:

```powershell
$env:PATH = "$PWD\whitefoot\bin;$env:PATH"
whitefootc --help
```

Keep the extracted package together: `bin\whitefootc.exe` uses the bundled
toolchain beside it. To keep the compiler on `PATH` in later sessions, add
the absolute path of `whitefoot\bin` to your user `PATH` in the Windows
settings.

### Build from source

For a source build, follow the prerequisites and commands in the
[README's Try it section](../../README.md#try-it).

## Your first program

Save this program as `first.wf`. It adds two bytes modulo 256 and returns
the result as its exit status. Since 250 + 10 is 260, the result is 4.

```whitefoot run exit=4 file=first.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const bytes: Array<u8, 2> =[250_u8, 10_u8];

fn main() -> status: ExitStatus pure {
  let total = 0_u8;
  for (i in 0_u64..bytes.len) {
    set total = total +wrap bytes[i];
  }
  return exit_status(code: total);
}
```

- The two `alias` lines give short names to the process library's result
  type and its constructor.
- `const bytes` declares an array of two unsigned 8-bit integers. `250_u8`
  is a typed literal; every numeric literal names its type.
- `fn main()` declares the entry function. `-> status: ExitStatus` names
  its result and its type. `pure` is the empty effect annotation: this
  function accesses no state through reference parameters. It can still
  read constants and change its own locals.
- `let total` creates a local value. `set total` assigns a new value to it.
- `for` visits indices from zero up to, but excluding, `bytes.len`. Indices
  use `u64`, so the loop begins at `0_u64`. The loop's bounds prove that
  `bytes[i]` is in range.
- `+wrap` adds modulo the integer type's range: modulo 256 for `u8`.
- `return` ends the function. `exit_status(code: total)` constructs the
  process result, with its argument named `code`.

Compile and run it:

```sh
whitefootc first.wf -o first
./first
echo "$?"
```

The last command prints `4`. Run it immediately after the program: `$?`
holds the preceding command's exit status. This example uses the status to
show a value; zero usually means success in a command-line tool.

In PowerShell on Windows, name the executable with `.exe`; `$LASTEXITCODE`
holds the exit status of the last program it ran:

```powershell
whitefootc first.wf -o first.exe
.\first.exe
echo $LASTEXITCODE
```

The last command prints `4` here too.

Copy the source exactly. Whitefoot requires canonical spacing, two spaces
per indentation level, and a final newline. It has no comments. These
rules are specified by FORM-1 and FORM-2 in the
[specification](../../spec/kernel-spec.md#2-canonical-form).

## A rejection

Replace only `+wrap` with `+` and save the program as `rejected.wf`:

```whitefoot reject OP-2 file=rejected.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const bytes: Array<u8, 2> =[250_u8, 10_u8];

fn main() -> status: ExitStatus pure {
  let total = 0_u8;
  for (i in 0_u64..bytes.len) {
    set total = total + bytes[i];
  }
  return exit_status(code: total);
}
```

```whitefoot-output
rejected.wf:9:17: error[OP-2]: UndischargedIntegerDomainObligation
  source:     set total = total + bytes[i];
  marker:                 ^^^^^^^^^^^^^^^^
  residual: total +defined bytes[i]
  disposition: Unproved
  mechanical_fix: `total +defined bytes[i]` is not proved here: when facts that reach the operation imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the operation with `if total +defined bytes[i]` where skipping it is the intended behavior; or write the `+wrap`, `+checked` or `+sat` form
```

You can request just the compiler's check:

```sh
whitefootc --check rejected.wf
```

The command is the same in PowerShell.

Bare `+` means an exact sum. The compiler must prove that each addition
fits in a `u8`, whose maximum is 255. This loop supplies no such proof. In
fact, the second addition would produce 260.

- `rejected.wf:9:17` locates the operation by file, line and column.
  `error[OP-2]` names the violated specification rule.
- `UndischargedIntegerDomainObligation` is the kind of rejection: the
  compiler could not establish the integer operation's domain.
- `source` repeats the source line. `marker` underlines the operation.
- `residual` states the goal left open. `+defined` asks whether this exact
  addition fits; it does not compute the sum.
- `disposition: Unproved` means the facts available to the checker do not
  settle that goal. It does not mean the program would be safe to run.
- `mechanical_fix` lists ways to establish the goal or select another
  arithmetic meaning. Choose one that matches what the program should do.

The first program wanted a modular sum, so `+wrap` was the right operation.
If you want an exact sum and a reported failure when it cannot fit, guard
the addition instead. Save this complete repair as `checked.wf`:

```whitefoot run exit=1 file=checked.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const bytes: Array<u8, 2> =[250_u8, 10_u8];

fn main() -> status: ExitStatus pure {
  let total = 0_u8;
  for (i in 0_u64..bytes.len) {
    if total +defined bytes[i] {
      set total = total + bytes[i];
    } else {
      return exit_status(code: 1_u8);
    }
  }
  return exit_status(code: 0_u8);
}
```

```sh
whitefootc checked.wf -o checked
./checked
echo "$?"
```

In PowerShell on Windows:

```powershell
whitefootc checked.wf -o checked.exe
.\checked.exe
echo $LASTEXITCODE
```

Either prints `1`: the first addition fits, and the second takes the failure
branch. The condition proves the exact `+` safe in its then-block. The
branch is part of the program's chosen behavior. Whitefoot adds no hidden
overflow check or trap to the exact operation.

## Where next

- The [README](../../README.md) introduces safety, performance and the
  language's basic forms. Its [Try it section](../../README.md#try-it)
  lists compiler options and source-build steps.
- [Integers](../articles/integers.md) covers exact, wrapping, checked and
  saturating arithmetic, and explicit conversions.
- [What a rejection tells you](../articles/what-a-rejection-tells-you.md)
  explains diagnostics and how to use their repair suggestions.
- [Proofs without a solver, by hand](../articles/proofs-by-hand.md)
  follows a loop invariant through the compiler's proof checks.
- The [kernel specification](../../spec/kernel-spec.md) defines syntax,
  semantics and the numbered rules cited in rejections.
