#!/bin/sh
# Exercise the gate wrapper's ownership and exit-status boundaries cheaply.
set -eu
runner=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/run-check.pl
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-check-test.XXXXXX")
holder=
cleanup() {
    if [ -n "$holder" ]; then wait "$holder" 2>/dev/null || :; fi
    rm -rf "$work"
}
trap cleanup EXIT
unset WHITEFOOT_CHECK_OWNER WHITEFOOT_TIME_BUDGET_RECORD WHITEFOOT_TIME_BUDGET_FILE WHITEFOOT_CPUINFO
WHITEFOOT_CHECK_LOCK_DIR=$work/lock
export WHITEFOOT_CHECK_LOCK_DIR

status=0
perl "$runner" failure sh -c 'exit 17' > "$work/failure.log" 2>&1 || status=$?
test "$status" -eq 17
test ! -e "$work/lock"

perl "$runner" parent perl "$runner" nested true > "$work/nested.log" 2>&1
test ! -e "$work/lock"

perl "$runner" holder sh -c 'touch "$1"; sleep 2' sh "$work/ready" > "$work/holder.log" 2>&1 &
holder=$!
attempt=0
while [ ! -f "$work/ready" ]; do
    attempt=$((attempt + 1))
    test "$attempt" -lt 100
    sleep 0.02
done
status=0
perl "$runner" competing true > "$work/competing.log" 2>&1 || status=$?
test "$status" -eq 75
grep -q 'already owned' "$work/competing.log"
wait "$holder"
holder=
test ! -e "$work/lock"

# Parallelism stays Cargo's and the test harness's own default unless named.
(
    unset CARGO_BUILD_JOBS RUST_TEST_THREADS
    perl "$runner" defaults sh -c 'test -z "${CARGO_BUILD_JOBS+set}${RUST_TEST_THREADS+set}"'
) > "$work/defaults.log" 2>&1
(
    CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1
    export CARGO_BUILD_JOBS RUST_TEST_THREADS
    perl "$runner" chosen sh -c 'test "$CARGO_BUILD_JOBS:$RUST_TEST_THREADS" = 1:1'
) > "$work/chosen.log" 2>&1
test ! -e "$work/lock"

status=0
WHITEFOOT_CHECK_TIMEOUT=1 perl "$runner" timeout sh -c \
    'sleep 30 & echo $! > "$1"; wait' sh "$work/child" \
    > "$work/timeout.log" 2>&1 || status=$?
test "$status" -eq 124
test ! -e "$work/lock"
if kill -0 "$(cat "$work/child")" 2>/dev/null; then exit 1; fi

perl "$runner" signal perl "$runner" nested sh -c \
    'sleep 30 & echo $! > "$1"; wait' sh "$work/signal-child" \
    > "$work/signal.log" 2>&1 &
holder=$!
attempt=0
while [ ! -f "$work/signal-child" ]; do
    attempt=$((attempt + 1))
    test "$attempt" -lt 100
    sleep 0.02
done
kill -TERM "$holder"
status=0
wait "$holder" || status=$?
holder=
test "$status" -eq 143
test ! -e "$work/lock"
if kill -0 "$(cat "$work/signal-child")" 2>/dev/null; then exit 1; fi

status=0
perl "$runner" orphan sh -c 'sleep 30 & echo $! > "$1"' sh "$work/orphan" \
    > "$work/orphan.log" 2>&1 || status=$?
test "$status" -eq 1
test ! -e "$work/lock"
if kill -0 "$(cat "$work/orphan")" 2>/dev/null; then exit 1; fi

# Emulate the platform seen by the wrapper, so every CI host checks Linux,
# macOS and Windows budget policy without changing the real command's host.
run_host() {
    platform=$1
    shift
    perl -e '$^O = shift @ARGV; my $runner = shift @ARGV; do $runner; die $@ || $!' "$platform" "$runner" "$@"
}
cat > "$work/budgets" <<'BUDGETS'
# Different class/host values expose selection of the wrong column.
cpu fast  Test Fast CPU
cpu slow  Test Slow CPU
label fast slow macos windows
within 600 0 0 0
over 0 600 0 0
parent 600 0 0 0
nowhere - 600 - -
timeout 0 600 0 0
malformed 1.5 600 1.5 1.5
unknown-max 0 600 0 0
BUDGETS
printf 'model name\t: Test Fast CPU\n' > "$work/cpuinfo"
WHITEFOOT_TIME_BUDGET_FILE=$work/budgets
WHITEFOOT_CPUINFO=$work/cpuinfo
export WHITEFOOT_TIME_BUDGET_FILE WHITEFOOT_CPUINFO
record=$work/over-budget

run_host linux over true > "$work/report.log" 2>&1
grep -q 'OVER BUDGET: enforced: over took' "$work/report.log"
run_host linux within true > "$work/within.log" 2>&1
grep -q '== BUDGET within: [0-9.]* s of 600 s (linux) ==' "$work/within.log"
run_host linux unlisted true > "$work/unlisted.log" 2>&1
if grep -q 'BUDGET' "$work/unlisted.log"; then exit 1; fi
test ! -e "$record"

# A record collects every overrun; nested commands keep their own statuses.
WHITEFOOT_TIME_BUDGET_RECORD=$record run_host linux parent sh -c \
    'perl -e "$3" "$1" over true; perl -e "$3" "$1" within true; touch "$2"' \
    sh "$runner" "$work/continued" '$^O = "linux"; do shift @ARGV; die $@ || $!' \
    > "$work/recorded.log" 2>&1
test -f "$work/continued"
grep -q '^  ENFORCED: over took [0-9.]* s, over its 0 s' "$record"
if grep -q 'within\|parent' "$record"; then exit 1; fi
test ! -e "$work/lock"
status=0
WHITEFOOT_TIME_BUDGET_RECORD=$record run_host linux over sh -c 'exit 17' > "$work/over-failed.log" 2>&1 || status=$?
test "$status" -eq 17
for label in unlisted nowhere; do
    WHITEFOOT_TIME_BUDGET_RECORD=$record run_host linux "$label" true > "$work/$label-recorded.log" 2>&1
    grep -q "^  ENFORCED: $label has no linux budget" "$record"
done
test "$(grep -c '^  ENFORCED: over took' "$record")" -eq 2
WHITEFOOT_TIME_BUDGET_RECORD=$record run_host linux over true > "$work/over-passed.log" 2>&1
test "$(grep -c '^  ENFORCED: over took' "$record")" -eq 3
WHITEFOOT_TIME_BUDGET_RECORD=$record run_host linux malformed true > "$work/malformed.log" 2>&1
grep -q "malformed needs seconds or - for fast" "$record"

# The second mapped model selects its own column without a fallback note.
printf 'model name\t: Test Slow CPU\n' > "$work/slow-cpuinfo"
WHITEFOOT_CPUINFO=$work/slow-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$work/slow-record \
    run_host linux unknown-max true > "$work/slow.log" 2>&1
grep -q '== BUDGET unknown-max: [0-9.]* s of 600 s (linux) ==' "$work/slow.log"
test ! -s "$work/slow-record"

# Unknown models use the largest Linux class value and record a note even
# when the selected budget is met. Notes alone never fail the verdict.
printf 'model name\t: Test Unmapped CPU\n' > "$work/unknown-cpuinfo"
WHITEFOOT_CPUINFO=$work/unknown-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$work/unknown-record \
    run_host linux unknown-max true > "$work/unknown.log" 2>&1
grep -q '== BUDGET unknown-max: [0-9.]* s of 600 s (linux) ==' "$work/unknown.log"
grep -q '== BUDGET NOTE: Linux CPU model "Test Unmapped CPU"' "$work/unknown.log"
grep -q '^  NOTE: Linux CPU model "Test Unmapped CPU".*slowest Linux class budget (600 s)' "$work/unknown-record"
perl "$runner" --budget-verdict "$work/unknown-record" > "$work/unknown-verdict.log" 2>&1

# A known mapping without a column and unreadable cpuinfo use the same
# fallback. A mapped '-' was checked above and must never fall back.
{ printf 'cpu omitted  Test Omitted CPU\n'; cat "$work/budgets"; } > "$work/omitted-budgets"
printf 'model name\t: Test Omitted CPU\n' > "$work/omitted-cpuinfo"
WHITEFOOT_TIME_BUDGET_FILE=$work/omitted-budgets WHITEFOOT_CPUINFO=$work/omitted-cpuinfo \
    WHITEFOOT_TIME_BUDGET_RECORD=$work/omitted-record run_host linux unknown-max true > "$work/omitted.log" 2>&1
grep -q '^  NOTE: Linux CPU model "Test Omitted CPU".*(600 s)' "$work/omitted-record"
WHITEFOOT_CPUINFO=$work/no-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$work/no-cpuinfo-record \
    run_host linux unknown-max true > "$work/no-cpuinfo.log" 2>&1
grep -q '^  NOTE: Linux CPU model "<unavailable>".*(600 s)' "$work/no-cpuinfo-record"

# macOS overruns remain visible but do not enforce; missing rows/values and
# malformed tables still fail there. Linux and Windows overruns enforce.
WHITEFOOT_TIME_BUDGET_RECORD=$work/macos-record run_host darwin over true > "$work/macos.log" 2>&1
grep -q '== OVER BUDGET: report-only: over took' "$work/macos.log"
grep -q '^  REPORT-ONLY: over took' "$work/macos-record"
perl "$runner" --budget-verdict "$work/macos-record" > "$work/macos-verdict.log" 2>&1
grep -q 'REPORT-ONLY: over took' "$work/macos-verdict.log"
for label in unlisted nowhere malformed; do
    WHITEFOOT_TIME_BUDGET_RECORD=$work/macos-missing run_host darwin "$label" true > "$work/macos-$label.log" 2>&1
done
grep -q '^  ENFORCED: unlisted has no macos budget' "$work/macos-missing"
grep -q '^  ENFORCED: nowhere has no macos budget' "$work/macos-missing"
grep -q '^  ENFORCED: .*malformed needs seconds or - for macos' "$work/macos-missing"
status=0
perl "$runner" --budget-verdict "$work/macos-missing" > "$work/macos-missing-verdict.log" 2>&1 || status=$?
test "$status" -eq 1
WHITEFOOT_TIME_BUDGET_RECORD=$work/linux-record run_host linux over true > "$work/linux.log" 2>&1
WHITEFOOT_TIME_BUDGET_RECORD=$work/windows-record run_host MSWin32 over true > "$work/windows.log" 2>&1
for platform in linux windows; do
    grep -q '^  ENFORCED: over took' "$work/$platform-record"
    status=0
    perl "$runner" --budget-verdict "$work/$platform-record" > "$work/$platform-verdict.log" 2>&1 || status=$?
    test "$status" -eq 1
done

# A cancelled stage has no budget verdict.
status=0
WHITEFOOT_CHECK_TIMEOUT=1 WHITEFOOT_TIME_BUDGET_RECORD=$work/timeout-record \
    run_host linux timeout sleep 5 > "$work/timeout-budget.log" 2>&1 || status=$?
test "$status" -eq 124
test ! -s "$work/timeout-record"

# A record that cannot be written stops the command before it runs; the
# relative case runs in the scratch directory, where a regression would leave
# its file.
for bad in relative-record "$work/no-such-directory/record"; do
    status=0
    (cd "$work" && WHITEFOOT_TIME_BUDGET_RECORD=$bad perl "$runner" within touch "$work/ran") \
        > "$work/bad-record.log" 2>&1 || status=$?
    test "$status" -ne 0
    test ! -e "$work/ran"
done
test ! -e "$work/relative-record"

status=0
perl "$runner" --budget-verdict "$record" > "$work/verdict.log" 2>&1 || status=$?
test "$status" -eq 1
grep -q 'TIME BUDGETS EXCEEDED' "$work/verdict.log"
: > "$work/empty-record"
perl "$runner" --budget-verdict "$work/empty-record" > "$work/verdict-empty.log" 2>&1
perl "$runner" --budget-verdict "$work/absent-record" > "$work/verdict-absent.log" 2>&1
for arguments in "''" "'$record' extra"; do
    status=0
    eval "perl \"\$runner\" --budget-verdict $arguments" > "$work/verdict-usage.log" 2>&1 || status=$?
    test "$status" -ne 0
    test "$status" -ne 1
    grep -q usage "$work/verdict-usage.log"
done

# An unreadable table never changes a status; with a record it is recorded.
status=0
WHITEFOOT_TIME_BUDGET_FILE=$work/missing perl "$runner" within sh -c 'exit 17' > "$work/missing.log" 2>&1 || status=$?
test "$status" -eq 17
grep -q 'cannot read' "$work/missing.log"
WHITEFOOT_TIME_BUDGET_FILE=$work/missing WHITEFOOT_TIME_BUDGET_RECORD=$work/missing-record \
    perl "$runner" within true > "$work/missing-recorded.log" 2>&1
grep -q 'cannot read' "$work/missing-record"
test ! -e "$work/lock"
echo 'check runner: status, nesting, exclusion, limits, timeout, cancellation, orphan cleanup and time budgets pass'
