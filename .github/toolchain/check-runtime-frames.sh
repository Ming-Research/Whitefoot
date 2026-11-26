#!/usr/bin/env bash
# No native tool puts a test binary inside an installed package and compares
# the clang it resolves with the package's. The runtime's C units are compiled
# by whichever clang the compiler resolves, and a package's clang is not the
# clang of the gate (Apple's on macOS, an apt build on Linux), so the frame
# test of the candidate's source runs here as the installed compiler would:
# beside the package's clang, under its toolchain manifest, where the
# resolution selects the package.
#
# The test, runtime_frames::every_runtime_c_frame_over_a_page_is_probed in
# compiler/src/bin/whitefootc/runtime_frames.rs, compiles every runtime unit
# with the driver's own arguments and requires a probe in every frame over a
# page. It passes only if exactly that test ran and passed (libtest exits 0
# when a name matches nothing), it compiled with this package's clang by real
# path and by version line, and the package is left as it was found.
#
# usage: check-runtime-frames.sh PACKAGE-DIRECTORY TEST-BINARY
#   PACKAGE-DIRECTORY  the installed package, holding bin/clang and
#                      toolchain-manifest.json
#   TEST-BINARY        the whitefootc test executable built from the
#                      candidate's source (cargo test --bin whitefootc --no-run)
# The caller supplies the host's command wrapper (strace on Linux, the
# host-clang denial sandbox on macOS) by running this script under it.
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo 'usage: check-runtime-frames.sh PACKAGE-DIRECTORY TEST-BINARY' >&2
  exit 2
fi
package="$1"
test_binary="$2"
fail() {
  echo "check-runtime-frames: $1" >&2
  exit 1
}
[ -x "$package/bin/clang" ] || fail "the package holds no executable bin/clang: $package"
[ -f "$package/toolchain-manifest.json" ] || fail "the package holds no toolchain-manifest.json: $package"
[ -x "$test_binary" ] || fail "the frame test is not an executable file: $test_binary"

test_name=runtime_frames::every_runtime_c_frame_over_a_page_is_probed
installed="$package/bin/whitefootc-runtime-frames"
log=$(mktemp)
trap 'rm -f "$installed" "$log"' EXIT
cp "$test_binary" "$installed"
status=0
"$installed" --exact "$test_name" --nocapture > "$log" 2>&1 || status=$?
cat "$log"
if [ "$status" -ne 0 ]; then
  echo "runtime frame test failed with status $status" >&2
  exit 1
fi
# libtest reports a name that matches nothing as zero tests and exits 0.
if ! grep -Eq 'test result: ok\. 1 passed; 0 failed' "$log"; then
  echo 'runtime frame test did not run exactly one passing test' >&2
  exit 1
fi
# grep -c prints 0 and exits 1 when no line matches, which `set -e` must not
# turn into a silent exit.
named=$(grep -c '^runtime frames compiled by ' "$log" || true)
if [ "$named" -ne 1 ]; then
  echo "the test named its clang $named times, not once" >&2
  exit 1
fi
reported=$(grep '^runtime frames compiled by ' "$log")
# The line is "runtime frames compiled by <path>: <first line of --version>".
python3 -I - "$package/bin/clang" "$reported" <<'PY'
import os, subprocess, sys
clang, line = sys.argv[1], sys.argv[2]
rest = line.removeprefix('runtime frames compiled by ')
path, separator, version = rest.partition(': ')
assert separator, ('no version after the clang path', line)
assert os.path.realpath(path) == os.path.realpath(clang), (
    'the runtime was compiled by another clang than the package\'s', path, clang)
expected = subprocess.run([clang, '--version'], capture_output=True, text=True,
                          check=True).stdout.splitlines()[0]
assert version == expected, ('version line differs', version, expected)
print('Runtime frames were compiled by the package clang:', expected)
PY
