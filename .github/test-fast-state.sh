#!/usr/bin/env bash
# Exercise .github/fast-state.sh without cargo: a work's first run copies
# main's directory, a later run moves its sources to the new commit and
# keeps the files that did not change, two runs of one work take turns, a
# work's number is digits, and `clean` removes only what no run used for a day
# and no run holds. The script runs on the 14900K, a Linux machine, with
# util-linux's flock; elsewhere, as on macOS, there is nothing to check.
set -euo pipefail
if [ "$(uname -s)" != Linux ] || ! command -v flock > /dev/null; then
    echo "test-fast-state: not Linux with flock, nothing checked"
    exit 0
fi
script=$(cd -- "$(dirname -- "$0")" && pwd)/fast-state.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-fast-state.XXXXXX")
trap 'touch "$RUNNER_TEMP/fast-state-release" 2>/dev/null || :; sleep 0.3; rm -rf "$work"' EXIT
export HOME=$work/home RUNNER_TEMP=$work/temp GITHUB_ENV=$work/env
mkdir -p "$HOME" "$RUNNER_TEMP"
root=$HOME/fast-state/Whitefoot

git init --quiet "$work/checkout"
cd "$work/checkout"
git config user.email test@example.invalid
git config user.name test
echo one > kept
echo one > changed
git add kept changed
git commit --quiet -m one
first=$(git rev-parse HEAD)

# Main's build, as `refresh` leaves it.
mkdir -p "$root/main/target"
git clone --quiet "$work/checkout" "$root/main/src"
echo built > "$root/main/target/built"

release() {
    bash "$script" release
    for _ in $(seq 100); do
        flock -n "$root/$1.lock" true && return 0
        sleep 0.05
    done
    echo "the lock of $1 was not given back" >&2
    exit 1
}

: > "$GITHUB_ENV"
bash "$script" take 205 "$work/checkout" > /dev/null
test "$(cat "$root/205/target/built")" = built
test "$(git -C "$root/205/src" rev-parse HEAD)" = "$first"
grep -qx "FAST_STATE=$root/205" "$GITHUB_ENV"
grep -qx "CARGO_TARGET_DIR=$root/205/target" "$GITHUB_ENV"
if flock -n "$root/205.lock" true; then
    echo "a taken work's lock was free" >&2
    exit 1
fi
release 205

# The next commit: only the changed file is written again, and what the last
# run left in the sources is gone.
touch -t 202001010000 "$root/205/src/kept"
echo left > "$root/205/src/left"
echo two > changed
git commit --quiet -am two
second=$(git rev-parse HEAD)
bash "$script" take 205 "$work/checkout" > /dev/null
test "$(git -C "$root/205/src" rev-parse HEAD)" = "$second"
test "$(cat "$root/205/src/changed")" = two
test -z "$(find "$root/205/src/kept" -newer "$root/205/src/changed")"
test -n "$(find "$root/205/src/kept" -mmin +1440)"
test ! -e "$root/205/src/left"

# A second run of the work waits for the first.
status=0
timeout 1 bash "$script" take 205 "$work/checkout" > /dev/null 2>&1 || status=$?
test "$status" -eq 124
release 205

bash "$script" take "" "$work/checkout" > /dev/null
test -d "$root/manual/src/.git"
release manual

status=0
bash "$script" take ../escape "$work/checkout" > /dev/null 2>&1 || status=$?
test "$status" -ne 0
test ! -e "$root/../escape"

# Unused for a day and free: removed. Recent, held or main's: kept.
mkdir -p "$root/301" "$root/302"
touch -t 202001010000 "$root/205/used" "$root/301/used" "$root/302/used" "$root/main/used"
flock "$root/302.lock" sleep 2 &
while flock -n "$root/302.lock" true; do sleep 0.05; done
bash "$script" clean > /dev/null
test ! -e "$root/205"
test ! -e "$root/301"
test -d "$root/302"
test -d "$root/manual"
test -d "$root/main"
wait
echo "test-fast-state: ok"
