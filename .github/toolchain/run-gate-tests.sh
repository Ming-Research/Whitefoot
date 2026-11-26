#!/usr/bin/env bash
# Run the gate's compiler tests and conformance cases with the clang of an
# installed bundled package, and nothing else's.
#
# The compiler's tests build and link the programs they run with the toolchain
# the compiler resolves for the executable that runs them (compiler/toolchain-
# resolution): beside a clang with a package manifest, that clang, answering the
# probes from the manifest's recorded answers. So the test executables Cargo
# built are copied into the extracted package's bin directory and run from
# there, against the package's own compiler, while every host clang is denied;
# a test that still starts a host clang fails instead of passing on it.
#
# Linux and macOS run what the gate runs there: the library's tests, and the
# command-line program's tests and every integration target, among them the
# conformance adapter. The gate runs no corpus on Windows (the conformance
# adapter is POSIX-only), whose evidence is the cases the Windows I/O host
# check runs; the filter below is that check's (io-hosts.yml, "Construct and
# run shared Windows program cases").
#
# Usage: run-gate-tests.sh MODE PACKAGE CHECKOUT LOGS
#   MODE      linux, macos or windows
#   PACKAGE   the extracted package root (holding bin/ and toolchain-manifest.json)
#   CHECKOUT  the candidate commit's checkout, whose compiler/ is tested
#   LOGS      a directory for the logs; summary.txt records what ran
set -euo pipefail
if [ "$#" -ne 4 ]; then
  echo 'usage: run-gate-tests.sh MODE PACKAGE CHECKOUT LOGS' >&2
  exit 2
fi
mode="$1"
package="$2"
checkout="$3"
logs="$4"
case "$mode" in linux|macos|windows) ;; *) echo "unknown mode $mode" >&2; exit 2 ;; esac
helpers=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
package=$(cd "$package" && pwd -P)
checkout=$(cd "$checkout" && pwd -P)
mkdir -p "$logs"
logs=$(cd "$logs" && pwd -P)
exe=""
py=python3
if [ "$mode" = windows ]; then
  exe=.exe
  py=python
fi
test -x "$package/bin/whitefootc$exe"
test -f "$package/toolchain-manifest.json"

# What this run stands on: the package's clang and the answers its manifest
# records for it, which the compiler takes instead of probing.
"$package/bin/clang$exe" --version | tee "$logs/clang-version.txt"
"$py" -I - "$package/toolchain-manifest.json" > "$logs/recorded-answers.txt" <<'PY'
import json, sys
print(json.dumps(json.load(open(sys.argv[1]))["clang_facts"], indent=2, sort_keys=True))
PY
cat "$logs/recorded-answers.txt"

# The test executables, built before any host clang is denied: Cargo links
# them with the host's own tools.
selectors=(--lib --bins --test '*')
filters=()
if [ "$mode" = windows ]; then
  selectors=(--test corpus)
  filters=(programs::windows:: programs::network:: programs::file_replacement::
    programs::stop_signals:: programs::driver_count:: programs::host_paths::
    programs::wfgrep_hosts:: conformance::file_replacement:: native_windows::)
fi
cargo test --manifest-path "$checkout/compiler/Cargo.toml" --profile gate --locked --no-run \
  --message-format=json "${selectors[@]}" > "$logs/test-build.json"
"$py" -I - "$logs/test-build.json" > "$logs/test-executables.txt" <<'PY'
import json, sys
paths = []
for line in open(sys.argv[1]):
    if not line.startswith('{'):
        continue
    message = json.loads(line)
    if message.get('reason') == 'compiler-artifact' and message.get('profile', {}).get('test') \
            and message.get('executable'):
        paths.append(message['executable'])
# The library's tests first, as the gate's unit group runs before its corpus.
paths.sort(key=lambda path: (not path.replace('\\', '/').rsplit('/', 1)[-1].startswith('whitefoot-'), path))
print('\n'.join(paths))
PY
test -s "$logs/test-executables.txt"
names=()
while IFS= read -r path; do
  path=$(tr -d '\r' <<< "$path")
  if [ "$mode" = windows ]; then path=$(cygpath -u "$path"); fi
  cp "$path" "$package/bin/"
  names+=("$(basename "$path")")
done < "$logs/test-executables.txt"
printf 'Test executables placed in the package: %s\n' "${names[*]}"

# Deny every host clang, and prove the denial, before anything runs.
wrapper=()
case "$mode" in
  linux)
    for path in $(command -v -a clang 2>/dev/null || true) /usr/bin/clang* /usr/bin/ld.lld* \
        /usr/lib/llvm-*/bin/clang* /usr/lib/llvm-*/bin/ld.lld* /usr/lib/llvm-*/bin/lld; do
      if [ -e "$path" ]; then sudo chmod a-x "$(readlink -f "$path")"; fi
    done
    # `command -v` names a file found on PATH even when it cannot be run, so
    # the proof is an attempt to run it and a look for an executable one.
    IFS=: read -ra entries <<< "$PATH"
    for entry in "${entries[@]}"; do
      if [ -n "$entry" ] && [ -x "$entry/clang" ]; then
        echo "A host clang is still executable: $entry/clang" >&2; exit 1
      fi
    done
    if clang --version > /dev/null 2>&1 || /usr/bin/clang --version > /dev/null 2>&1; then
      echo 'The host clang still runs' >&2; exit 1
    fi
    ;;
  macos)
    bash "$helpers/deny-host-clang-macos.sh" "$logs"
    cat "$logs/isolation.txt"
    if [ "$(sed -n 's/^isolation=//p' "$logs/isolation.txt")" != verified ]; then
      echo 'Host clang denial is unverified, so the run would not show that the packaged clang is the one used' >&2
      exit 1
    fi
    wrapper=(/usr/bin/sandbox-exec -f "$logs/deny-host-clang.sb")
    ;;
  windows)
    IFS=: read -ra entries <<< "$PATH"
    kept=()
    for entry in "${entries[@]}"; do
      if [ -e "$entry/clang.exe" ] || [ -e "$entry/clang" ]; then
        printf 'Dropping %s from PATH: it holds a clang\n' "$entry"
      else
        kept+=("$entry")
      fi
    done
    PATH=$(IFS=:; echo "${kept[*]}")
    export PATH
    if command -v clang; then echo 'A host clang is still reachable' >&2; exit 1; fi
    ;;
esac

# The gate's own environment for its tests, and the windows job's.
if [ "$mode" = windows ]; then
  export CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 RUST_MIN_STACK=8388608
else
  ulimit -c 0
fi
export WHITEFOOT_TEST_COMPILER="$package/bin/whitefootc$exe"
: > "$logs/summary.txt"
failed=0
cd "$checkout/compiler"
for name in "${names[@]}"; do
  arguments=()
  case "$name" in corpus-*) arguments=(${filters[@]+"${filters[@]}"}) ;; esac
  status=0
  ${wrapper[@]+"${wrapper[@]}"} "$package/bin/$name" ${arguments[@]+"${arguments[@]}"} \
    > "$logs/$name.log" 2>&1 || status=$?
  tail -n 30 "$logs/$name.log"
  result=$(grep -E '^test result:' "$logs/$name.log" | tail -n 1 || true)
  printf '%s: exit %s; %s\n' "$name" "$status" "${result:-no test result line}" | tee -a "$logs/summary.txt"
  if [ "$status" -ne 0 ]; then failed=1; fi
done
if [ "$failed" -ne 0 ]; then
  echo 'The gate compiler tests did not all pass with the packaged clang' >&2
  exit 1
fi
echo 'The gate compiler tests passed with the packaged clang.'
