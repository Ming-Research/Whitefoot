#!/usr/bin/env bash
# No native tool qualifies an LLVM/Whitefoot package across isolated Linux and
# SDK-bearing macOS hosts. This shared workflow runner applies the same witness,
# inventory, link-mode and falsifier checks to the exact release archive bytes.
# Caller supplies an archive, maintained witness, disposable work directory,
# and one host mode: linux-absent, linux-wrong-major, or macos.
set -euo pipefail
if [ "$#" -ne 4 ]; then
  echo 'usage: check-package.sh ARCHIVE WITNESS WORK-DIRECTORY MODE' >&2
  exit 2
fi
archive="$1"
witness="$2"
work="$3"
mode="$4"
case "$mode" in linux-absent|linux-wrong-major|macos) ;; *) exit 2 ;; esac
mkdir -p "$work/logs/traces" "$work/unrelated cwd" "$work/driver alias"
work=$(cd "$work" && pwd -P)
logs="$work/logs"
package="$work/relocated package"
mkdir "$package"
# Logs include the package byte identity, installed host inventory and every
# command/status. Neither a compiler build nor a host LLVM tree is mounted.
exec > >(tee "$logs/acceptance.log") 2>&1
printf 'Mode: %s\nArchive: %s\n' "$mode" "$archive"
python3 - "$archive" <<'PYHASH'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
sha = hashlib.sha256()
with path.open('rb') as stream:
    for block in iter(lambda: stream.read(1024 * 1024), b''):
        sha.update(block)
print('Archive SHA-256:', sha.hexdigest())
PYHASH
uname -a
tar -xzf "$archive" -C "$package"
# The witness, tests/package-fixtures/waiting.wf, is reduced from the
# backend test WAITING_PROGRAM: an owned enum crosses a direct waiting call
# and a bound spawn whose result is used, so the build must lower coroutines.
# It exits 0; 1 or 2 identifies a wrong observed result.
cp "$witness" "$work/waiting.wf"
witness="$work/waiting.wf"

python3 - "$package" <<'PY'
import hashlib, json, pathlib, re, sys
root = pathlib.Path(sys.argv[1]).resolve(strict=True)
manifest = json.loads((root / 'toolchain-manifest.json').read_text())
assert manifest['llvm_version'] == '22.1.8', manifest['llvm_version']
assert (root / 'bin/whitefootc').is_file()
assert (root / 'bin/clang').is_file()
assert (root / 'lib/clang/22/include/stddef.h').is_file()
assert (root / 'spec/kernel-spec.md').is_file()
assert (root / 'lib/std/modules.wfg').is_file()
# Verify all trimmed bytes and every relative symlink against the inventory,
# then all regular package files against the release's inner checksums.
for entry in manifest['files']:
    path = root / entry['path']
    path.resolve(strict=True).relative_to(root)
    if entry['type'] == 'symlink':
        assert path.is_symlink(), entry
        target = path.readlink()
        assert not target.is_absolute(), entry
        assert str(target) == entry['link_target'], entry
        payload = str(target).encode(sys.getfilesystemencoding())
    else:
        assert path.is_file() and not path.is_symlink(), entry
        payload = path.read_bytes()
    assert len(payload) == entry['size'], entry['path']
    assert hashlib.sha256(payload).hexdigest() == entry['sha256'], entry['path']
covered = set()
for line in (root / 'SHA256SUMS').read_text().splitlines():
    digest, name = line.split(maxsplit=1)
    name = name.removeprefix('*')
    path = root / name
    path.resolve(strict=True).relative_to(root)
    assert not path.is_symlink() and path.is_file(), name
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    covered.add(path.relative_to(root).as_posix())
actual = {path.relative_to(root).as_posix() for path in root.rglob('*')
          if path.is_file() and not path.is_symlink() and path != root / 'SHA256SUMS'}
assert covered == actual, ('checksum coverage', covered ^ actual)
for component in ('llvm', 'clang', 'lld', 'compiler-rt'):
    path = root / 'share/licenses' / component / 'LICENSE.TXT'
    text = path.read_text()
    assert 'Apache License' in text and 'LLVM Exceptions' in text, component
print('Verified package files:', len(actual), 'toolchain inventory:', len(manifest['files']))
print('Source LLVM:', manifest['source_asset'], manifest['source_sha256'])
print('Host prerequisites:', json.dumps(manifest['host_dependencies'], indent=2))
print('Minimum GLIBC symbol version:', manifest['minimum_glibc_version'])
PY

# The invocation alias and cwd live outside the package. current_exe must lead
# the resolver back to the moved real binary, never to this alias directory.
ln -s "$package/bin/whitefootc" "$work/driver alias/whitefootc"
compiler="$work/driver alias/whitefootc"
cd "$work/unrelated cwd"
wrapper=()
isolation=verified
host_log="$logs/host-clang-invocations.txt"
: > "$host_log"
if [ "$mode" != macos ]; then
  dpkg-query -W > "$logs/installed-packages.txt"
  cat "$logs/installed-packages.txt"
  echo 'PATH clang inventory:'
  which -a clang || true
  if [ "$mode" = linux-absent ]; then
    if command -v clang; then echo 'Host clang unexpectedly installed' >&2; exit 1; fi
    if compgen -G '/usr/lib/llvm-*' >/dev/null || compgen -G '/usr/bin/clang*' >/dev/null; then
      echo 'Host clang/LLVM paths unexpectedly installed' >&2; exit 1
    fi
    if grep -E '^((clang|llvm|lld)(-[0-9]+)?|libclang[^[:space:]]*)[[:space:]]' "$logs/installed-packages.txt"; then
      echo 'Clang/LLVM packages unexpectedly installed' >&2; exit 1
    fi
  else
    # Wrap the canonical clang-18 binary; both PATH and /usr/bin/clang,
    # clang-18 and /usr/lib/llvm-18/bin aliases consequently log every call.
    host=$(readlink -f /usr/bin/clang-18)
    test -x "$host"
    mv "$host" "$host.host-original"
    cat > "$host" <<WRAPPER
#!/bin/sh
printf '%s\\n' "\$0 \$*" >> '$host_log'
exec '$host.host-original' "\$@"
WRAPPER
    chmod +x "$host"
    ln -sf /usr/bin/clang-18 /usr/bin/clang
    export PATH="/usr/bin:$PATH"
    test "$(command -v clang)" = /usr/bin/clang
    /usr/bin/clang --version > "$logs/host-clang-version.txt"
    test -s "$host_log" # Falsifier: the detector must see an actual host call.
    host_major=$(/usr/bin/clang -dumpversion | cut -d. -f1)
    bundle_major=$("$package/bin/clang" -dumpversion | cut -d. -f1)
    test "$host_major" = 18
    test "$bundle_major" = 22
    test "$host_major" != "$bundle_major"
    cat "$logs/host-clang-version.txt"
    : > "$host_log"
  fi
  ldd "$package/bin/clang" > "$logs/clang-dependencies.txt"
  ldd "$package/bin/ld.lld" > "$logs/lld-dependencies.txt"
  cat "$logs/clang-dependencies.txt" "$logs/lld-dependencies.txt"
  if grep -E '/usr/lib/llvm-|not found' "$logs/clang-dependencies.txt" "$logs/lld-dependencies.txt"; then
    echo 'Package uses missing or host LLVM libraries' >&2; exit 1
  fi
else
  test "$(uname -m)" = arm64
  sw_vers > "$logs/host.txt"
  sdk=$(/usr/bin/xcrun --sdk macosx --show-sdk-path)
  linker=$(/usr/bin/xcrun --sdk macosx --find ld)
  test -d "$sdk"
  test -x "$linker"
  printf 'System SDK: %s\nSystem linker: %s\n' "$sdk" "$linker" | tee -a "$logs/host.txt"
  otool -L "$package/bin/clang" > "$logs/clang-dependencies.txt"
  otool -L "$package/lib/libLTO.dylib" > "$logs/lto-dependencies.txt"
  cat "$logs/clang-dependencies.txt" "$logs/lto-dependencies.txt"
  # Deny every visible host clang path and its canonical target. SDK/ld remain
  # available. Never equate PATH hiding or a failed sandbox launch with denial.
  selected_host_clang=$(/usr/bin/xcrun --sdk macosx --find clang)
  python3 - "$logs" "$selected_host_clang" <<'PY'
import glob, json, os, pathlib, sys
logs = pathlib.Path(sys.argv[1])
patterns = [sys.argv[2], '/usr/bin/clang*', '/Library/Developer/CommandLineTools/usr/bin/clang*',
            '/Applications/Xcode*.app/Contents/Developer/Toolchains/*/usr/bin/clang*']
patterns += [str(pathlib.Path(part) / 'clang*') for part in os.environ['PATH'].split(':') if part]
paths = set()
for pattern in patterns:
    for name in glob.glob(pattern):
        path = pathlib.Path(name)
        if path.is_file() and os.access(path, os.X_OK):
            paths.update((str(path.absolute()), str(path.resolve(strict=True))))
assert '/usr/bin/clang' in paths, 'No host clang to prove denial against'
(logs / 'host-clangs.txt').write_text(''.join(path + '\n' for path in sorted(paths)))
profile = '(version 1)\n(allow default)\n(deny process-exec\n'
profile += ''.join('  (literal ' + json.dumps(path) + ')\n' for path in sorted(paths))
(logs / 'deny-host-clang.sb').write_text(profile + ')\n')
PY
  profile="$logs/deny-host-clang.sb"
  if [ ! -x /usr/bin/sandbox-exec ] || ! /usr/bin/sandbox-exec -f "$profile" /usr/bin/true; then
    isolation=unverified
    echo 'UNVERIFIED: hosted runner cannot execute the isolation profile'
  else
    while IFS= read -r host; do
      # The baseline proves the path executes, not that it is a compiler:
      # tools such as clang-cache exit nonzero on --version but still run.
      # Only the shell's cannot-execute statuses mean it never ran.
      status=0
      "$host" --version > "$logs/denial-baseline.txt" 2>&1 || status=$?
      if [ "$status" -eq 126 ] || [ "$status" -eq 127 ]; then
        isolation=unverified
        printf 'UNVERIFIED: host baseline cannot execute: %s\n' "$host"
        break
      fi
      printf 'Prove host denial: %s\n' "$host"
      /usr/bin/sandbox-exec -f "$profile" "$host" --version > "$logs/denial-attempt.txt" 2>&1 || true
      if ! grep -Fq "execvp() of '$host' failed: Operation not permitted" "$logs/denial-attempt.txt"; then
        isolation=unverified
        printf 'UNVERIFIED: sandbox did not refuse executing host clang: %s\n' "$host"
        break
      fi
      cat "$logs/denial-attempt.txt"
      printf '%s: baseline succeeded; sandbox invocation failed\n' "$host" >> "$logs/denial-results.txt"
    done < "$logs/host-clangs.txt"
  fi
  if [ "$isolation" = verified ]; then
    wrapper=(/usr/bin/sandbox-exec -f "$profile")
  else
    echo 'Basic package checks continue; host-clang independence remains UNVERIFIED.'
  fi
fi

# Every Linux compiler invocation has an execution trace, including version
# queries and all capability probes; both output and status stay independent of
# tee. macOS compiler subprocesses inherit the validated denial sandbox.
run() {
  local label="$1" status=0
  shift
  printf 'Command (%s):' "$label"
  printf ' %q' "$@"
  printf '\n'
  if [ "$mode" = macos ]; then
    # macOS bash 3.2 treats an empty array as unset under set -u.
    ${wrapper[@]+"${wrapper[@]}"} "$@" > "$logs/$label.txt" 2>&1 || status=$?
  else
    strace -f -s 8192 -e trace=execve -o "$logs/traces/$label.txt" \
      "$@" > "$logs/$label.txt" 2>&1 || status=$?
  fi
  cat "$logs/$label.txt"
  printf 'Exit (%s): %s\n' "$label" "$status"
  return "$status"
}
run bundle-version "$package/bin/clang" --version
# Match the resource ablation with a positive that disables host header
# fallback. This tests the final archive's resources, not the input subset.
cat > "$work/builtin-headers.c" <<'C'
#include <stddef.h>
#include <stdatomic.h>
int header_example(void) {
  _Atomic(int) value = 40;
  size_t count = 2;
  return atomic_fetch_add(&value, (int)count) == 40 ? 0 : 1;
}
C
header_flags=(-std=c11 -nostdlibinc)
if [ "$mode" = macos ]; then header_flags+=(-isysroot "$sdk"); fi
run builtin-headers-control "$package/bin/clang" ${header_flags[@]+"${header_flags[@]}"} \
  -c "$work/builtin-headers.c" -o "$work/builtin-headers.o"
run llvm "$compiler" --emit-llvm -o "$work/waiting.ll" "$witness"
# A declaration does not witness execution of coroutine lowering.
grep -E '(call|invoke)[[:space:]].*@llvm\.coro\.' "$work/waiting.ll"
if [ "$mode" = macos ]; then
  # Both probe attributes must belong to one attribute group selected by the
  # real fourth capability probe, not merely appear somewhere in the module.
  grep -E '^attributes #[0-9]+ = \{.*"probe-stack"="inline-asm".*"stack-probe-size"="4096"' "$work/waiting.ll"
  if grep -F '"probe-stack"="__chkstk_darwin"' "$work/waiting.ll"; then
    echo 'Upstream bundle unexpectedly selected Darwin helper probing' >&2; exit 1
  fi
fi
run ordinary-build "$compiler" -o "$work/ordinary" "$witness"
run ordinary-run "$work/ordinary"
run full-lto-build "$compiler" --full-lto -o "$work/full-lto" "$witness"
run full-lto-run "$work/full-lto"
for kind in objects fragments; do
  extra=()
  if [ "$kind" = fragments ]; then extra=(--fragments module); fi
  for temperature in cold warm; do
    run "$kind-$temperature-build" "$compiler" --cache "$work/cache-$kind" \
      ${extra[@]+"${extra[@]}"} --report -o "$work/$kind-$temperature" "$witness"
    run "$kind-$temperature-run" "$work/$kind-$temperature"
  done
  python3 - "$logs" "$kind" <<'PY'
import json, pathlib, sys
logs, kind = pathlib.Path(sys.argv[1]), sys.argv[2]
# The report's final JSON line contains build observations; diagnostics are
# retained in the same log without making a substring a cache-hit proof.
def report(temperature):
    rows = (logs / f'{kind}-{temperature}-build.txt').read_text().splitlines()
    return next(json.loads(row)['build'] for row in reversed(rows) if row.startswith('{"build":'))
cold, warm = report('cold'), report('warm')
assert cold['objects_compiled'] > 0 and cold['objects_reused'] == 0, cold
assert warm['objects_reused'] > 0 and warm['objects_compiled'] == 0, warm
if kind == 'fragments':
    assert cold['fragments'] > 0 and warm['fragments'] > 0, (cold, warm)
print(kind, 'cold/warm cache observations:', cold, warm)
PY
 done

expect_failure() {
  local label="$1" needle="$2"
  shift 2
  if run "$label" "$@"; then
    echo "$label unexpectedly succeeded" >&2
    exit 1
  fi
  grep -F "$needle" "$logs/$label.txt"
}
# Each negative starts from a complete package that just compiled this exact
# witness, and removes one component. The package marker remains in place.
missing="$work/missing builtin headers"
cp -a "$package" "$missing"
rm -rf "$missing/lib/clang/22/include"
expect_failure missing-headers stddef.h "$missing/bin/clang" ${header_flags[@]+"${header_flags[@]}"} \
  -c "$work/builtin-headers.c" -o "$work/missing-headers.o"
grep -E 'fatal error:.*stddef.h.*(not found|No such)' "$logs/missing-headers.txt"
missing="$work/missing clang"
cp -a "$package" "$missing"
rm "$missing/bin/clang"
expect_failure missing-clang "$missing/bin/clang" \
  "$missing/bin/whitefootc" --diagnostic-format json -o "$work/missing-clang-output" "$witness"
grep -E '"category"[[:space:]]*:[[:space:]]*"Toolchain"' "$logs/missing-clang.txt"
if [ "$mode" != macos ]; then
  missing="$work/missing lld"
  cp -a "$package" "$missing"
  rm "$missing/bin/ld.lld"
  expect_failure missing-lld "$missing/bin/ld.lld" \
    "$missing/bin/whitefootc" --diagnostic-format json -o "$work/missing-lld-output" "$witness"
  grep -E '"category"[[:space:]]*:[[:space:]]*"Toolchain"' "$logs/missing-lld.txt"
  # Verify actual successful executable paths, not merely selected flags. All
  # clang (including capability probes) and linker executions must use this
  # package; generic/system ld would refute bundled linker selection.
  python3 - "$package" "$logs/traces" <<'PY'
import ast, pathlib, re, sys
root, traces = pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2])
clang, lld = [], []
for path in traces.glob('*.txt'):
    if path.stem.startswith('missing-'):
        continue
    for line in path.read_text().splitlines():
        match = re.search(r'execve\(("(?:\\.|[^"\\])*")\s*,.*\)\s*=\s*0\s*$', line)
        if not match:
            continue
        executable = pathlib.Path(ast.literal_eval(match.group(1)))
        name = executable.name
        if re.fullmatch(r'clang(?:-[0-9]+)?', name):
            executable.resolve(strict=True).relative_to(root)
            clang.append(str(executable))
        if name in ('ld', 'ld.lld', 'lld') or re.fullmatch(r'ld\.lld-[0-9]+', name):
            assert executable == root / 'bin/ld.lld', executable
            lld.append(str(executable))
assert len(clang) > 4, ('Missing compiler/probe execution evidence', clang)
assert lld, 'No bundled ld.lld execution found'
print('Actual bundled clang executions:', len(clang), sorted(set(clang)))
print('Actual bundled ELF linker executions:', len(lld), sorted(set(lld)))
PY
  test ! -s "$host_log"
else
  missing="$work/missing libLTO"
  cp -a "$package" "$missing"
  python3 - "$missing/lib/libLTO.dylib" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
target = path.resolve(strict=True)
if path.is_symlink():
    path.unlink()
target.unlink()
PY
  expect_failure missing-lto libLTO \
    "$missing/bin/whitefootc" --full-lto -o "$work/missing-lto-output" "$witness"
  grep -Ei 'error|missing|not found|No such|cannot|could not|unable' "$logs/missing-lto.txt"
  expect_failure invalid-sdk SDKROOT env SDKROOT="$work/not an SDK" \
    "$compiler" --diagnostic-format json -o "$work/invalid-sdk-output" "$witness"
  grep -E '"category"[[:space:]]*:[[:space:]]*"Toolchain"' "$logs/invalid-sdk.txt"
fi
printf 'isolation=%s\n' "$isolation" > "$logs/result.txt"
cp "$work/waiting.ll" "$logs/waiting.ll"
echo 'All executable package checks passed.'
