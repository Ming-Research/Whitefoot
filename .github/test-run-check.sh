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
unset WHITEFOOT_CHECK_OWNER WHITEFOOT_TIME_BUDGET_RECORD WHITEFOOT_TIME_BUDGET_FILE
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

# Budgets. The host's own column holds the budget that decides each case, and
# the other columns the opposite value, so a wrong column fails the case.
case "$(uname -s)" in
    Linux) host=linux ;;
    Darwin) host=macos ;;
    *) echo "check runner test: unsupported host $(uname -s)" >&2; exit 1 ;;
esac
row() { # row LABEL HOST-BUDGET OTHER-BUDGET
    for column in linux macos windows; do
        if [ "$column" = "$host" ]; then printf ' %s' "$2"; else printf ' %s' "$3"; fi
    done
}
{
    printf '# test budgets\nlabel linux macos windows\n'
    printf 'within%s\n' "$(row within 600 0)"
    printf 'over%s\n' "$(row over 0 600)"
    printf 'parent%s\n' "$(row parent 600 0)"
    printf 'nowhere%s\n' "$(row nowhere - 600)"
} > "$work/budgets"
WHITEFOOT_TIME_BUDGET_FILE=$work/budgets
export WHITEFOOT_TIME_BUDGET_FILE
record=$work/over-budget

perl "$runner" over true > "$work/report.log" 2>&1
grep -q 'OVER BUDGET: over took' "$work/report.log"
perl "$runner" within true > "$work/within.log" 2>&1
grep -q '== BUDGET within: [0-9.]* s of 600 s' "$work/within.log"
perl "$runner" unlisted true > "$work/unlisted.log" 2>&1
! grep -q 'BUDGET' "$work/unlisted.log"
test ! -e "$record"

# A record collects every overrun; the commands keep their own statuses.
WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$runner" parent sh -c \
    'perl "$1" over true; perl "$1" within true; touch "$2"' sh "$runner" "$work/continued" \
    > "$work/recorded.log" 2>&1
test -f "$work/continued"
grep -q '^  over took [0-9.]* s, over its 0 s' "$record"
! grep -q 'within\|parent' "$record"
test ! -e "$work/lock"
status=0
WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$runner" over sh -c 'exit 17' > "$work/over-failed.log" 2>&1 || status=$?
test "$status" -eq 17
for label in unlisted nowhere; do
    WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$runner" "$label" true > "$work/$label-recorded.log" 2>&1
    grep -q "^  $label has no $host budget" "$record"
done
test "$(grep -c '^  over took' "$record")" -eq 2

status=0
perl "$runner" --budget-verdict "$record" > "$work/verdict.log" 2>&1 || status=$?
test "$status" -eq 1
grep -q 'TIME BUDGETS EXCEEDED' "$work/verdict.log"
: > "$work/empty-record"
perl "$runner" --budget-verdict "$work/empty-record" > "$work/verdict-empty.log" 2>&1
perl "$runner" --budget-verdict "$work/absent-record" > "$work/verdict-absent.log" 2>&1

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
