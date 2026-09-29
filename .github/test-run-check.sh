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
unset WHITEFOOT_CHECK_OWNER WHITEFOOT_TIME_BUDGETS WHITEFOOT_TIME_BUDGET_FILE
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

processors=$(getconf _NPROCESSORS_ONLN)
(
    unset CARGO_BUILD_JOBS RUST_TEST_THREADS JOBS
    perl "$runner" defaults sh -c 'test "$CARGO_BUILD_JOBS:$RUST_TEST_THREADS:$JOBS" = "$1:$1:$1"' sh "$processors"
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
! kill -0 "$(cat "$work/child")" 2>/dev/null

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
! kill -0 "$(cat "$work/signal-child")" 2>/dev/null

status=0
perl "$runner" orphan sh -c 'sleep 30 & echo $! > "$1"' sh "$work/orphan" \
    > "$work/orphan.log" 2>&1 || status=$?
test "$status" -eq 1
test ! -e "$work/lock"
! kill -0 "$(cat "$work/orphan")" 2>/dev/null

# Budgets: a zero budget is exceeded by any stage, a large one by none.
printf '# test budgets\nlabel linux macos windows\nwithin 600 600 600\nover 0 0 0\nparent 600 600 600\nnowhere - - -\n' \
    > "$work/budgets"
WHITEFOOT_TIME_BUDGET_FILE=$work/budgets
export WHITEFOOT_TIME_BUDGET_FILE

perl "$runner" over true > "$work/report.log" 2>&1
grep -q 'OVER BUDGET: over took' "$work/report.log"
perl "$runner" unlisted true > "$work/unlisted.log" 2>&1
! grep -q 'BUDGET' "$work/unlisted.log"

WHITEFOOT_TIME_BUDGETS=enforce perl "$runner" within true > "$work/within.log" 2>&1
grep -q '== BUDGET within: 0 s of 600 s' "$work/within.log"

status=0
WHITEFOOT_TIME_BUDGETS=enforce perl "$runner" parent sh -c \
    'perl "$1" over true; perl "$1" within true; touch "$2"' sh "$runner" "$work/continued" \
    > "$work/enforce.log" 2>&1 || status=$?
test "$status" -eq 3
test -f "$work/continued"
grep -q '^  over took [0-9]* s, over its 0 s' "$work/enforce.log"
! grep -q '^  within' "$work/enforce.log"
test ! -e "$work/lock"

for label in unlisted nowhere; do
    status=0
    WHITEFOOT_TIME_BUDGETS=enforce perl "$runner" "$label" true > "$work/$label-enforced.log" 2>&1 || status=$?
    test "$status" -eq 3
    grep -q "^  $label has no [a-z]* budget" "$work/$label-enforced.log"
done

status=0
WHITEFOOT_TIME_BUDGETS=enforce perl "$runner" over sh -c 'exit 17' > "$work/over-failed.log" 2>&1 || status=$?
test "$status" -eq 17
grep -q 'TIME BUDGETS EXCEEDED' "$work/over-failed.log"

status=0
WHITEFOOT_TIME_BUDGETS=strict perl "$runner" within true > "$work/mode.log" 2>&1 || status=$?
test "$status" -ne 0
grep -q 'must be report or enforce' "$work/mode.log"
test ! -e "$work/lock"
echo 'check runner: status, nesting, exclusion, limits, timeout, cancellation, orphan cleanup and time budgets pass'
