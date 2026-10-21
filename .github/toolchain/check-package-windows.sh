#!/usr/bin/env bash
# No native tool qualifies a Windows package on a machine that holds no other
# compiler. A hosted Windows runner has Rust, LLVM and Visual Studio, and the
# Visual C++ redistributable that would hide a missing runtime DLL. This runner
# installs the exact release archive in a Windows Server Core container, which
# has none of them, with the commands of the Windows section of the getting
# started guide (docs/guide/getting-started.md) and runs the package's compiler
# there; the host only orchestrates and inspects the images that come out.
# The guide's download is replaced by a copy of the candidate archive and of the
# SHA256SUMS file beside it. Every other PowerShell block of the guide runs as
# written, and so do its programs.
# Caller supplies the archive (whose directory holds the release's SHA256SUMS),
# the directory of maintained witnesses (tests/package-fixtures), a disposable
# work directory and the container image whose Windows build matches the
# runner's (process isolation needs that).
# Runs on a Windows host with Git's bash, Python and Docker in Windows
# container mode.
set -euo pipefail
if [ "$#" -ne 4 ]; then
  echo 'usage: check-package-windows.sh ARCHIVE FIXTURES WORK-DIRECTORY IMAGE' >&2
  exit 2
fi
tools=$(cygpath -m "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)")
archive=$(cygpath -m "$1")
fixtures=$(cygpath -m "$2")
mkdir -p "$3/logs"
work=$(cygpath -m "$3")
image="$4"
logs="$work/logs"
# Git's bash rewrites arguments that look like POSIX paths; Windows paths for
# the container must pass through untouched.
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'
mkdir -p "$work/installed here" "$work/unrelated cwd" "$work/out" "$work/tools"
exec > >(tee "$logs/acceptance.log") 2>&1
printf 'Archive: %s\nImage: %s\n' "$archive" "$image"
python -I - "$archive" <<'PYHASH'
import hashlib, pathlib, sys
sha = hashlib.sha256()
with pathlib.Path(sys.argv[1]).open('rb') as stream:
    for block in iter(lambda: stream.read(1024 * 1024), b''):
        sha.update(block)
print('Archive SHA-256:', sha.hexdigest())
PYHASH
# The installation happens in the container, in a directory whose name holds a
# space, away from the repository and the container's own tools.
test "$(basename "$archive")" = whitefootc-windows-x86_64-bundled.zip
sums="$(dirname "$archive")/SHA256SUMS"
test -f "$sums"
guide="$fixtures/../../docs/guide/getting-started.md"
test -f "$guide"
mkdir -p "$work/download"
cp "$archive" "$sums" "$work/download/"
package="$work/installed here/whitefoot"
cp "$fixtures/waiting.wf" "$work/waiting.wf"
cp "$fixtures/stack-exhaustion.wf" "$work/stack-exhaustion.wf"
# A program from the corpus that runs on Windows with the host capabilities it
# has today: generic floating-point code that returns its exit status. The
# README's recursive search is not used: its directory walk reads names as
# bytes of text, and Windows hands the program UTF-16 names.
example="$fixtures/../programs/geometry_vectors.wf"
test -f "$example"
cp "$example" "$work/example.wf"
cat > "$work/builtin-headers.c" <<'C'
#include <stddef.h>
#include <stdatomic.h>
int header_example(void) {
  _Atomic(int) value = 40;
  size_t count = 2;
  return atomic_fetch_add(&value, (int)count) == 40 ? 0 : 1;
}
C
# A program's exit code can exceed 255, which bash cuts to eight bits (a fault
# ends a Windows process with 0xC0000409 or similar), so images run through
# this wrapper, which writes the code to a file.
# The redirection comes first: `echo 1> file` would redirect handle 1.
printf '@echo off\r\n"%%~2"\r\n>"%%~1" echo %%errorlevel%%\r\n' > "$work/tools/run-code.cmd"

# What the guide says, read before anything runs: the PowerShell blocks become
# one script that the container runs from the directory the user works in, and
# the programs, their exit codes and the rejection text come from the same
# document. The download is the one thing the guide does that a candidate that
# is not published yet cannot do; the check requires the block to name the
# release's assets and copies the candidate's files in its place.
python -I - "$guide" "$work" <<'PYGUIDE'
import pathlib, re, sys
guide, work = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
blocks, lines, open_info = [], guide.read_text().splitlines(), None
for line in lines:
    if open_info is None:
        match = re.fullmatch(r'```(\S.*)', line)
        if match:
            open_info, body = match.group(1), []
    elif line == '```':
        blocks.append((open_info, '\n'.join(body) + '\n'))
        open_info = None
    else:
        body.append(line)
assert open_info is None, 'unclosed fence in the guide'
asset = 'whitefootc-windows-x86_64-bundled.zip'
script = ["$ErrorActionPreference = 'Stop'"]
powershell = [body for info, body in blocks if info == 'powershell']
for needle in ('Invoke-WebRequest', 'Get-FileHash', 'Expand-Archive', '$env:PATH',
               'whitefootc first.wf -o first.exe', 'whitefootc checked.wf -o checked.exe'):
    assert any(needle in body for body in powershell), ('the guide has no PowerShell block with', needle)
for number, body in enumerate(powershell, 1):
    if 'Invoke-WebRequest' in body:
        for name in (asset, 'SHA256SUMS', 'releases/download/$release_tag'):
            assert name in body, ('the guide download does not name', name)
        script += [r"Copy-Item 'C:\work\download\%s' ." % asset, r"Copy-Item 'C:\work\download\SHA256SUMS' ."]
    else:
        script.append(body.rstrip('\n'))
    script.append("Write-Output '=== guide PowerShell block %d done ==='" % number)
(work / 'tools' / 'install-from-guide.ps1').write_text('\n'.join(script) + '\n')
codes = []
for info, body in blocks:
    run = re.fullmatch(r'whitefoot run exit=(\d+) file=(\S+)', info)
    reject = re.fullmatch(r'whitefoot reject (\S+) file=(\S+)', info)
    if run or reject:
        # Bytes, not text: a program's line ends are not the platform's.
        (work / 'installed here' / (run or reject).group(2)).write_bytes(body.encode())
        if run:
            codes.append(run.group(1))
(work / 'tools' / 'guide-exit-codes.txt').write_text('\n'.join(codes) + '\n')
outputs = [body for info, body in blocks if info == 'whitefoot-output']
assert len(outputs) == 1, 'the guide shows one rejection text'
(work / 'tools' / 'rejected.expected').write_bytes(outputs[0].encode())
assert codes == ['4', '1'], codes
print('Guide:', guide, '- PowerShell blocks:', len(powershell), '- exit codes:', ' '.join(codes))
PYGUIDE

# The clean machine: a container started from the stock Server Core image. The
# host's own clang is the control that shows the detector can see one.
name=wf-clean-windows
# The hosted image starts its container engine in the background, so the first
# call can come before it listens: give it a bounded time, then fail with the
# engine's own message. One run found no engine after two minutes, so after
# one minute the engine's service state goes to the log and a stopped service
# is started; the bound is five minutes.
for attempt in $(seq 1 60); do
  docker info > /dev/null 2>&1 && break
  if [ "$attempt" -eq 12 ]; then
    powershell.exe -NoProfile -Command \
      'Get-Service docker | Format-List Name,Status,StartType; Start-Service docker' \
      > "$logs/docker-service.txt" 2>&1 || true
    cat "$logs/docker-service.txt"
  fi
  sleep 5
done
docker info > "$logs/docker-info.txt"
docker rm -f "$name" > /dev/null 2>&1 || true
trap 'docker rm -f "$name" > /dev/null 2>&1 || true; restore' EXIT
docker run --detach --name "$name" --isolation process \
  --mount "type=bind,src=$(cygpath -w "$work"),dst=C:\\work" "$image" ping -t localhost > /dev/null
echo 'Host control (the runner has a compiler on PATH, the container must not):'
command -v clang || echo 'no clang on the runner PATH'
cexec_at() { # directory label command... : run in the container from a directory
  local directory="$1" label="$2" status=0
  shift 2
  printf 'Command (%s):' "$label"
  printf ' %q' "$@"
  printf '\n'
  docker exec --workdir "$directory" "$name" "$@" > "$logs/$label.txt" 2>&1 || status=$?
  cat "$logs/$label.txt"
  printf 'Exit (%s): %s\n' "$label" "$status"
  return "$status"
}
cexec() { # label command... : run in the container from an unrelated directory
  cexec_at 'C:\work\unrelated cwd' "$@"
}
cexec container-version cmd /c ver
cexec container-program-files cmd /c dir /b 'C:\Program Files' || true
cexec container-program-files-x86 cmd /c dir /b 'C:\Program Files (x86)' || true
for absent in clang cargo rustc link gcc git python bash; do
  if cexec "container-where-$absent" where "$absent"; then
    echo "$absent is installed in the container" >&2
    exit 1
  fi
done
# The Visual C++ runtime is what a package built with Visual C++ would need.
for runtime in vcruntime140.dll msvcp140.dll; do
  if cexec "container-no-$runtime" cmd /c dir /b "C:\\Windows\\System32\\$runtime"; then
    echo "$runtime is present in the container" >&2
    exit 1
  fi
done

# The installation, by the guide's own commands from the directory the user
# works in; it leaves first.exe and checked.exe there and prints their exit
# codes.
cexec_at 'C:\work\installed here' install-from-guide powershell -NoProfile -ExecutionPolicy Bypass \
  -File 'C:\work\tools\install-from-guide.ps1'
grep -F 'whitefootc-windows-x86_64-bundled.zip: OK' "$logs/install-from-guide.txt"
printed=$(tr -d '\r' < "$logs/install-from-guide.txt" | grep -x -E '[0-9]+' | tr '\n' ' ')
test "$printed" = "$(tr '\n' ' ' < "$work/tools/guide-exit-codes.txt")"
test -f "$work/installed here/first.exe"
test -f "$work/installed here/checked.exe"
cp "$work/installed here/first.exe" "$work/out/guide-first.exe"
cp "$work/installed here/checked.exe" "$work/out/guide-checked.exe"
test -f "$package/bin/whitefootc.exe"

# The same inventory check as the Linux package: every trimmed byte against the
# manifest, every regular file against the release's inner checksums.
python -I - "$package" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1]).resolve(strict=True)
manifest = json.loads((root / 'toolchain-manifest.json').read_text())
assert manifest['llvm_version'] == '22.1.8', manifest['llvm_version']
assert manifest['platform'] == 'windows-x86_64', manifest['platform']
for required in ('bin/whitefootc.exe', 'bin/clang.exe', 'bin/clang-22.exe', 'bin/ld.lld.exe',
                 'lib/clang/22/include/stddef.h', 'spec/kernel-spec.md', 'lib/std/modules.wfg',
                 'lib/clang/22/lib/windows/libclang_rt.builtins-x86_64.a'):
    assert (root / required).is_file(), required
for entry in manifest['files']:
    path = root / entry['path']
    assert entry['type'] == 'file' and path.is_file() and not path.is_symlink(), entry
    payload = path.read_bytes()
    assert len(payload) == entry['size'], entry['path']
    assert hashlib.sha256(payload).hexdigest() == entry['sha256'], entry['path']
covered = set()
for line in (root / 'SHA256SUMS').read_text().splitlines():
    digest, name = line.split(maxsplit=1)
    name = name.removeprefix('*')
    path = root / name
    assert path.is_file() and not path.is_symlink(), name
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    covered.add(path.relative_to(root).as_posix())
actual = {path.relative_to(root).as_posix() for path in root.rglob('*')
          if path.is_file() and path != root / 'SHA256SUMS'}
assert covered == actual, ('checksum coverage', covered ^ actual)
for component in ('llvm', 'clang', 'lld', 'compiler-rt', 'libcxx', 'libunwind'):
    text = (root / 'share/licenses' / component / 'LICENSE.TXT').read_text()
    assert 'Apache License' in text and 'LLVM Exceptions' in text, component
for name in manifest['licenses']:
    assert (root / name).is_file(), name
print('Verified package files:', len(actual), 'toolchain inventory:', len(manifest['files']))
print('Source LLVM:', manifest['source_asset'], manifest['source_sha256'])
print('System dependencies:', json.dumps(manifest['host_dependencies'], indent=2))
PY

package_c='C:\work\installed here\whitefoot'
compiler="$package_c\\bin\\whitefootc.exe"
run_image() { # label executable : exit code in $logs/label.code, output in label.out/err
  local label="$1" exe="$2"
  printf 'Run (%s): %s\n' "$label" "$exe"
  docker exec --workdir 'C:\work\unrelated cwd' "$name" 'C:\work\tools\run-code.cmd' \
    "C:\\work\\logs\\$label.code" "$exe" > "$logs/$label.out" 2> "$logs/$label.err" || true
  report_run "$label"
}
report_run() { # label : read the exit code the wrapper wrote and show the run
  local label="$1"
  code=$(tr -d '\r\n ' < "$logs/$label.code")
  cat "$logs/$label.out" "$logs/$label.err"
  printf 'Exit (%s): %s\n' "$label" "$code"
}
restore() { # the negative tests hide one component at a time
  for hidden in "$package"/*.hidden "$package"/*/*.hidden "$package"/lib/clang/22/*.hidden; do
    [ -e "$hidden" ] && mv "$hidden" "${hidden%.hidden}"
  done
  return 0
}

cexec bundle-version "$package_c\\bin\\clang.exe" --version
grep -F 'clang version 22.1.8' "$logs/bundle-version.txt"
# clang reports its installation directory with forward slashes.
grep -iF "InstalledDir: ${package_c//\\//}/bin" "$logs/bundle-version.txt"
# Match the resource ablation with a positive that disables host header
# fallback. This tests the final archive's resources, not the input subset.
cexec builtin-headers-control "$package_c\\bin\\clang.exe" -std=c11 -nostdlibinc \
  -c 'C:\work\builtin-headers.c' -o 'C:\work\out\builtin-headers.o'
cexec llvm "$compiler" --emit-llvm -o 'C:\work\out\waiting.ll' 'C:\work\waiting.wf'
# A declaration does not witness execution of coroutine lowering.
grep -E '(call|invoke)[[:space:]].*@llvm\.coro\.' "$work/out/waiting.ll"
grep -F 'target triple = "x86_64-w64-windows-gnu"' "$work/out/waiting.ll"
cexec ordinary-build "$compiler" -o 'C:\work\out\ordinary.exe' 'C:\work\waiting.wf'
run_image ordinary-run 'C:\work\out\ordinary.exe'
test "$code" = 0
# An ordinary program built and run from the package alone.
cexec example-build "$compiler" -o 'C:\work\out\example.exe' 'C:\work\example.wf'
run_image example-run 'C:\work\out\example.exe'
test "$code" = 0
# The guide's rejection: refused, with the text the guide shows.
if cexec_at 'C:\work\installed here' guide-rejected "$compiler" --check rejected.wf; then
  echo 'the guide rejected.wf was accepted' >&2
  exit 1
fi
tr -d '\r' < "$logs/guide-rejected.txt" | diff - "$work/tools/rejected.expected"
# Stack exhaustion ends in the floor's abort and its one record only if every
# large frame is probed into the guard, so this observes the probing method the
# packaged clang selected: an unprobed frame would end in a host fault with no
# record. The CRT's abort ends a Windows process with 3 or with the fast-fail
# status 0xC0000409, which cmd's errorlevel prints as a signed 32-bit number.
# The package builds the image in the container, but the image runs on the
# runner: ending a process that holds the floor's 1 GiB of committed stack by
# a crash takes about twenty minutes inside a process-isolated container and
# one second on the runner (a dedicated probe measured both, with an
# exit-based end taking a second in either), and the property under test is
# which method the compiler chose, not where the image runs. The ordinary
# images already run in the container, and the import check below covers this
# image like the others.
cexec stack-build "$compiler" -o 'C:\work\out\stack.exe' 'C:\work\stack-exhaustion.wf'
cmd /c "$(cygpath -w "$work/tools/run-code.cmd")" "$(cygpath -w "$logs/stack-run.code")" \
  "$(cygpath -w "$work/out/stack.exe")" > "$logs/stack-run.out" 2> "$logs/stack-run.err" || true
report_run stack-run
case "$code" in 3 | 3221226505 | -1073740791) ;; *) echo "stack witness ended with $code" >&2; exit 1 ;; esac
test "$(tr -d '\r' < "$logs/stack-run.err")" = '{"resource":"stack"}'
test ! -s "$logs/stack-run.out"
cexec full-lto-build "$compiler" --full-lto -o 'C:\work\out\full-lto.exe' 'C:\work\waiting.wf'
run_image full-lto-run 'C:\work\out\full-lto.exe'
test "$code" = 0
for kind in objects fragments; do
  extra=()
  if [ "$kind" = fragments ]; then extra=(--fragments module); fi
  for temperature in cold warm; do
    cexec "$kind-$temperature-build" "$compiler" --cache "C:\\work\\out\\cache-$kind" \
      ${extra[@]+"${extra[@]}"} --report -o "C:\\work\\out\\$kind-$temperature.exe" 'C:\work\waiting.wf'
    run_image "$kind-$temperature-run" "C:\\work\\out\\$kind-$temperature.exe"
    test "$code" = 0
  done
  python -I "$tools/check-cache-report.py" "$logs" "$kind"
done

# Every image imports only Windows itself: the Universal C Runtime and system
# DLLs, never Visual C++'s runtime or a MinGW runtime DLL that a clean machine
# lacks. The host's llvm-readobj only reads the images.
readobj=$(command -v llvm-readobj || echo '/c/Program Files/LLVM/bin/llvm-readobj.exe')
test -x "$readobj"
for image in guide-first guide-checked ordinary example stack full-lto objects-cold objects-warm fragments-cold fragments-warm; do
  "$readobj" --coff-imports "$work/out/$image.exe" | sed -n 's/^ *Name: //p' | tr -d '\r' | sort -u > "$logs/$image.imports"
  echo "--- $image imports ---"
  cat "$logs/$image.imports"
  test -s "$logs/$image.imports"
  if grep -i -E '^(vcruntime|msvcp|concrt|vccorlib|libgcc|libstdc|libwinpthread|libc\+\+|libunwind)' "$logs/$image.imports"; then
    echo "$image imports a runtime a clean machine does not have" >&2
    exit 1
  fi
  grep -i -E '^(api-ms-win-crt-|ucrtbase)' "$logs/$image.imports" > /dev/null
done

# Each negative removes one component from the installed package, in place,
# and puts it back afterwards; the package just compiled this exact witness.
expect_failure() {
  local label="$1" needle="$2"
  shift 2
  if cexec "$label" "$@"; then
    echo "$label unexpectedly succeeded" >&2
    exit 1
  fi
  grep -F "$needle" "$logs/$label.txt"
}
mv "$package/lib/clang/22/include" "$package/lib/clang/22/include.hidden"
expect_failure missing-headers stddef.h "$package_c\\bin\\clang.exe" -std=c11 -nostdlibinc \
  -c 'C:\work\builtin-headers.c' -o 'C:\work\out\missing-headers.o'
grep -E 'fatal error:.*stddef.h.*(not found|No such)' "$logs/missing-headers.txt"
restore
mv "$package/bin/clang.exe" "$package/bin/clang.exe.hidden"
expect_failure missing-clang 'clang.exe' "$compiler" --diagnostic-format json \
  -o 'C:\work\out\missing-clang.exe' 'C:\work\waiting.wf'
grep -E '"category"[[:space:]]*:[[:space:]]*"Toolchain"' "$logs/missing-clang.txt"
restore
mv "$package/bin/ld.lld.exe" "$package/bin/ld.lld.exe.hidden"
expect_failure missing-lld 'ld.lld.exe' "$compiler" --diagnostic-format json \
  -o 'C:\work\out\missing-lld.exe' 'C:\work\waiting.wf'
grep -E '"category"[[:space:]]*:[[:space:]]*"Toolchain"' "$logs/missing-lld.txt"
restore
# The package is whole again: one more build from it.
cexec after-restore "$compiler" -o 'C:\work\out\after-restore.exe' 'C:\work\waiting.wf'
printf 'isolation=verified\n' > "$logs/result.txt"
echo 'All executable package checks passed.'
