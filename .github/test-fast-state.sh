#!/usr/bin/env bash
# Exercise .github/fast-state.sh without cargo: a work's first run copies
# main's directory, a later run moves its sources to the new commit and
# keeps the files that did not change, two runs of one work take turns, a
# work's number is digits, and `clean` removes only what no run used for a day
# and no run holds. The script runs on the 14900K, a Linux machine, with
# util-linux's flock; elsewhere, as on macOS, there is nothing to check.
# The test never sleeps for a set time: it waits on the locks themselves,
# with `flock -n` for a lock given back and /proc/locks for a run blocked on
# one, so that it takes about as long as the script's own runs.
set -euo pipefail
if [ "$(uname -s)" != Linux ] || ! command -v flock > /dev/null; then
    echo "test-fast-state: not Linux with flock, nothing checked"
    exit 0
fi
script=$(cd -- "$(dirname -- "$0")" && pwd)/fast-state.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-fast-state.XXXXXX")
# The CI job's own runner directory and environment file stay out of reach:
# each run of the script below is given a job's own (job).
unset RUNNER_TEMP GITHUB_ENV
export HOME=$work/home
mkdir -p "$HOME" "$work/jobs"
root=$HOME/fast-state/Whitefoot
waiting=

# Runs fast-state.sh as the job $1. Each job has its own runner directory and
# environment file, as each job on a runner has.
job() {
    local name=$1
    shift
    mkdir -p "$work/jobs/$name"
    RUNNER_TEMP=$work/jobs/$name GITHUB_ENV=$work/jobs/$name/env bash "$script" "$@"
}

# Waits, for at most five seconds, until no process holds the lock file $1.
free() {
    for _ in $(seq 100); do
        flock -n "$1" true && return 0
        sleep 0.05
    done
    echo "the lock $1 was not given back" >&2
    return 1
}

# Waits, for at most five seconds, until a process is blocked on the lock file
# $1. /proc/locks lists such a process under the lock's holder, as a line that
# starts "->" and names the file's inode.
blocked() {
    local inode
    inode=$(stat -c %i "$1")
    for _ in $(seq 100); do
        grep -q -e "-> FLOCK .*:$inode " /proc/locks && return 0
        sleep 0.05
    done
    echo "no run is blocked on the lock $1" >&2
    return 1
}

# The job $1 gives back its lock, and the test waits until the lock of the
# work $2 is free.
release() {
    job "$1" release
    free "$root/$2.lock"
}

# On a runner, the end of a job ends the process that holds its lock. Here
# the test gives back every lock a run still holds, and waits for it, before
# it removes the directory that the process watches.
cleanup() {
    exec 8>&-
    if [ -n "$waiting" ]; then kill "$waiting" 2> /dev/null || :; fi
    for dir in "$work"/jobs/*/; do
        if [ -d "$dir" ]; then touch "$dir/fast-state-release"; fi
    done
    for lock in "$root"/*.lock; do
        if [ -e "$lock" ]; then free "$lock" || :; fi
    done
    rm -rf "$work"
}
trap cleanup EXIT

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

job first take 205 "$work/checkout" > /dev/null
test "$(cat "$root/205/target/built")" = built
test "$(git -C "$root/205/src" rev-parse HEAD)" = "$first"
grep -qx "FAST_STATE=$root/205" "$work/jobs/first/env"
grep -qx "CARGO_TARGET_DIR=$root/205/target" "$work/jobs/first/env"
if flock -n "$root/205.lock" true; then
    echo "a taken work's lock was free" >&2
    exit 1
fi
release first 205

# The next commit: only the changed file is written again, and what the last
# run left in the sources is gone.
touch -t 202001010000 "$root/205/src/kept"
echo left > "$root/205/src/left"
echo two > changed
git commit --quiet -am two
second=$(git rev-parse HEAD)
job next take 205 "$work/checkout" > /dev/null
test "$(git -C "$root/205/src" rev-parse HEAD)" = "$second"
test "$(cat "$root/205/src/changed")" = two
test -z "$(find "$root/205/src/kept" -newer "$root/205/src/changed")"
test -n "$(find "$root/205/src/kept" -mmin +1440)"
test ! -e "$root/205/src/left"

# A second run of the work waits for the first. It is blocked on the work's
# lock, and while the test runs a person's run, which has no work number, the
# second run does not go on. Once the first run gives the lock back, the
# second run goes on and holds the lock.
job waiting take 205 "$work/checkout" > /dev/null &
waiting=$!
blocked "$root/205.lock"

job manual take "" "$work/checkout" > /dev/null
test -d "$root/manual/src/.git"
release manual manual

kill -0 "$waiting"
test ! -s "$work/jobs/waiting/env"
job next release
wait "$waiting"
waiting=
if flock -n "$root/205.lock" true; then
    echo "the run that waited does not hold the work's lock" >&2
    exit 1
fi
grep -qx "FAST_STATE=$root/205" "$work/jobs/waiting/env"
release waiting 205

status=0
job escape take ../escape "$work/checkout" > /dev/null 2>&1 || status=$?
test "$status" -ne 0
test ! -e "$root/../escape"

# Unused for a day and free: removed. Recent, held or main's: kept. The test
# holds the lock of 302 itself, on a descriptor of its own.
mkdir -p "$root/301" "$root/302"
touch -t 202001010000 "$root/205/used" "$root/301/used" "$root/302/used" "$root/main/used"
exec 8> "$root/302.lock"
flock -n 8
bash "$script" clean > /dev/null
exec 8>&-
test ! -e "$root/205"
test ! -e "$root/301"
test -d "$root/302"
test -d "$root/manual"
test -d "$root/main"
echo "test-fast-state: ok"
