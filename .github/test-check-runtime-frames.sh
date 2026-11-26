#!/bin/bash
# Run .github/toolchain/check-runtime-frames.sh against stand-ins for an
# installed package (a clang that prints a version line, a manifest) and for
# the candidate's frame-test binary (a script that prints what libtest and the
# test print), and check each verdict. The script's own words are the
# reference: it passes only if exactly the one named test ran and passed, the
# test names its clang once, that clang is the package's by real path and by
# version line, and the package is left as it was found. Every failure must
# also name its reason on stderr.
set -u
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script=$here/toolchain/check-runtime-frames.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-check-runtime-frames.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0

package=$work/package
mkdir -p "$package/bin" "$work/elsewhere" "$work/another/bin"
cat > "$package/bin/clang" <<'EOF'
#!/bin/sh
echo 'Stand-in clang version 22.1.8'
echo 'Target: x86_64'
EOF
chmod +x "$package/bin/clang"
echo '{}' > "$package/toolchain-manifest.json"
ln -s "$package/bin/clang" "$work/elsewhere/clang-link"
cp "$package/bin/clang" "$work/another/bin/clang"

# The stand-in test binary. FAKE_MODE selects what it prints. Beside the
# package's clang (its own directory holds `clang`) it reports that clang the
# way the driver's resolution would; run from anywhere else it reports the
# host's, as the resolution does when no sibling clang exists.
cat > "$work/frame-test" <<'EOF'
#!/bin/sh
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -x "$dir/clang" ]; then clang=$dir/clang; else clang=/usr/bin/clang; fi
version='Stand-in clang version 22.1.8'
ok='test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'
case "$FAKE_MODE" in
  green) echo "runtime frames compiled by $clang: $version"; echo "$ok" ;;
  symlink) echo "runtime frames compiled by $FAKE_LINK: $version"; echo "$ok" ;;
  other-clang) echo "runtime frames compiled by $FAKE_OTHER: $version"; echo "$ok" ;;
  other-version) echo "runtime frames compiled by $clang: Apple clang version 17.0.0"; echo "$ok" ;;
  no-version) echo "runtime frames compiled by $clang"; echo "$ok" ;;
  no-line) echo "$ok" ;;
  twice) echo "runtime frames compiled by $clang: $version"; echo "runtime frames compiled by $clang: $version"; echo "$ok" ;;
  zero-tests) echo 'running 0 tests'; echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out' ;;
  eleven) echo "runtime frames compiled by $clang: $version"; echo 'test result: ok. 11 passed; 0 failed; 0 ignored' ;;
  two-failed) echo "runtime frames compiled by $clang: $version"; echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored' ;;
  failed-exit-zero) echo "runtime frames compiled by $clang: $version"; echo 'test result: FAILED. 1 passed; 1 failed; 0 ignored' ;;
  red) echo "runtime frames compiled by $clang: $version"; echo 'runtime frames over a page without a stack probe: wf_file_wait_readiness' >&2; echo 'test result: FAILED. 0 passed; 1 failed'; exit 101 ;;
  arguments) echo "runtime frames compiled by $clang: $version"; echo "arguments: $*"; echo "$ok" ;;
esac
EOF
chmod +x "$work/frame-test"

check() { # label expected-status stderr-must-name-a-reason
  label=$1
  expected=$2
  reason=$3
  shift 3
  "$@" > "$work/out" 2> "$work/err"
  status=$?
  problems=
  if [ "$expected" = pass ] && [ "$status" -ne 0 ]; then
    problems="expected success, got status $status"
  elif [ "$expected" = fail ] && [ "$status" -eq 0 ]; then
    problems='expected failure, got success'
  elif [ "$expected" = usage ] && [ "$status" -ne 2 ]; then
    problems="expected usage status 2, got $status"
  fi
  if [ "$expected" != pass ] && [ "$reason" = reason ] && [ ! -s "$work/err" ]; then
    problems="$problems; a failure named no reason on stderr"
  fi
  if [ -e "$package/bin/whitefootc-runtime-frames" ]; then
    problems="$problems; the copy of the test binary was left in the package"
  fi
  if [ -n "$problems" ]; then
    echo "FAIL $label:$problems" >&2
    sed 's/^/  out: /' "$work/out" >&2
    sed 's/^/  err: /' "$work/err" >&2
    failures=$((failures + 1))
  fi
}

run() { FAKE_MODE=$1 FAKE_LINK=$work/elsewhere/clang-link FAKE_OTHER=$work/another/bin/clang \
  bash "$script" "$package" "$work/frame-test"; }

check 'beside the package clang, one test passes' pass reason run green
grep -q 'Runtime frames were compiled by the package clang: Stand-in clang version 22.1.8' "$work/out" || {
  echo 'FAIL green: the success line did not name the package clang' >&2; failures=$((failures + 1)); }
check 'a symlink to the package clang is that clang by real path' pass reason run symlink
check 'a clang elsewhere with the same text is another clang' fail reason run other-clang
check 'a version line that differs fails' fail reason run other-version
check 'a clang path with no version fails' fail reason run no-version
check 'a test that names no clang fails' fail reason run no-line
check 'a test that names its clang twice fails' fail reason run twice
check 'a name that matches nothing (0 tests, exit 0) fails' fail reason run zero-tests
check 'eleven passed is not one passed' fail reason run eleven
check 'a failed result with exit 0 fails' fail reason run failed-exit-zero
check 'a result of one failed test fails' fail reason run two-failed
check 'a red test (status 101) fails' fail reason run red
grep -q 'wf_file_wait_readiness' "$work/out" "$work/err" || {
  echo 'FAIL red: the test output that names the unprobed frame was not shown' >&2; failures=$((failures + 1)); }

# The test runs only the one named test, with --exact.
FAKE_MODE=arguments bash "$script" "$package" "$work/frame-test" > "$work/out" 2>&1
grep -q 'arguments: --exact runtime_frames::every_runtime_c_frame_over_a_page_is_probed --nocapture' "$work/out" || {
  echo 'FAIL arguments: the test was not run by its exact name' >&2; cat "$work/out" >&2; failures=$((failures + 1)); }

# The copy is placed beside the clang, so the resolution selects the package:
# running it from outside the package gives the host clang, which must fail.
cp "$work/frame-test" "$work/elsewhere/frame-test"
FAKE_MODE=green "$work/elsewhere/frame-test" > "$work/direct" 2>&1
grep -q '/usr/bin/clang' "$work/direct" || {
  echo 'FAIL stand-in: a test run outside the package must report the host clang' >&2; failures=$((failures + 1)); }

# The package is left as found, even on a failure (checked by every case
# above), and a pre-existing file of another name stays.
touch "$package/bin/keep"
run green > /dev/null 2>&1
[ -e "$package/bin/keep" ] || { echo 'FAIL: the script removed a file it did not create' >&2; failures=$((failures + 1)); }

# Arguments and the package's shape.
check 'no arguments is a usage error' usage reason bash "$script"
check 'one argument is a usage error' usage reason bash "$script" "$package"
check 'three arguments is a usage error' usage reason bash "$script" "$package" "$work/frame-test" extra
mkdir -p "$work/bare/bin"
cp "$package/bin/clang" "$work/bare/bin/clang"
check 'a package without its manifest fails' fail reason \
  env FAKE_MODE=green bash "$script" "$work/bare" "$work/frame-test"
check 'a missing test binary fails' fail reason \
  env FAKE_MODE=green bash "$script" "$package" "$work/no-such-binary"

if [ "$failures" -ne 0 ]; then
  echo "$failures check-runtime-frames case(s) failed" >&2
  exit 1
fi
echo 'check-runtime-frames: all cases behave'
