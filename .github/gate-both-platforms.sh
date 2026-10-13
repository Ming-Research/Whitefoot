#!/bin/sh
# Require every correctness group green on Linux and macOS for a revision,
# read from the jobs of gate.yml's runs for that revision. Only jobs of those
# runs count (`filter=latest` drops a job superseded by a re-run attempt), and
# the latest completed job of a name decides, so a re-run run supersedes an
# earlier one. Each `<group> (<os>)` must have concluded success.
#
# GitHub's API is sometimes flaky, and a listing taken while jobs are being
# recorded can lack some of them, so a flaky answer must not read as a red
# gate:
# - every `gh api` call is tried up to 5 times, waiting 1, 2, 4 and 8 times
#   GATE_RETRY_WAIT seconds (default 5: 5, 10, 20, 40 s) between tries; a call
#   that fails every time fails the step, naming the call;
# - when an expected job has no entry at all, everything (the runs and their
#   jobs) is listed again after GATE_RELIST_WAIT seconds (default 30), up to 4
#   listings in all, before judging; a job still without an entry after the
#   last listing fails as "no gate job";
# - a job with an entry that did not succeed (failure, cancelled, timed_out)
#   is a real result: it fails the step at once, with no further listing, after
#   every group's line is printed.
#
# It reads GH_TOKEN, HEAD_SHA, GATE_GROUPS (a JSON array of group names) and
# GITHUB_REPOSITORY, uses RUNNER_TEMP (or a fresh temporary directory) for
# scratch files, and queries GitHub with `gh`. .github/test-gate-both-platforms.sh
# exercises it.
set -eu

wait_retry=${GATE_RETRY_WAIT:-5}
wait_relist=${GATE_RELIST_WAIT:-30}
attempts=5
listings=4

groups=$(printf '%s\n' "$GATE_GROUPS" | jq -er '.[]')
test -n "$groups"

work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/gate-both-platforms.XXXXXX")
trap 'rm -rf "$work"' EXIT

# api LABEL ARGS...: gh api with retries; the answer goes to $work/answer.
api() {
    label=$1
    shift
    try=1
    delay=$wait_retry
    while :; do
        if gh api "$@" > "$work/answer" 2> "$work/error"; then
            return 0
        fi
        if [ "$try" -ge "$attempts" ]; then
            cat "$work/error" >&2
            echo "::error::gh api call failed $attempts times: $label"
            exit 1
        fi
        echo "gh api call failed (try $try of $attempts): $label; retrying in ${delay}s" >&2
        sleep "$delay"
        try=$((try + 1))
        delay=$((delay * 2))
    done
}

# list: rebuild $work/jobs.tsv from the runs of gate.yml for the revision.
list() {
    api "list gate.yml runs for $HEAD_SHA" --paginate \
        "repos/$GITHUB_REPOSITORY/actions/workflows/gate.yml/runs?head_sha=$HEAD_SHA&per_page=100" \
        --jq '.workflow_runs[].id'
    runs=$(cat "$work/answer")
    : > "$work/jobs.tsv"
    for run in $runs; do
        api "list jobs of run $run" --paginate \
            "repos/$GITHUB_REPOSITORY/actions/runs/$run/jobs?filter=latest&per_page=100" \
            --jq '.jobs[] | select(.completed_at != null) | [.completed_at, .name, .conclusion] | @tsv'
        cat "$work/answer" >> "$work/jobs.tsv"
    done
    sort -r "$work/jobs.tsv" > "$work/sorted.tsv"
}

# judge: print a line per expected job; set absent (no entry) and failed
# (entry without success) to the counts.
judge() {
    absent=0
    failed=0
    for group in $groups; do
        for os in ubuntu-24.04 macos-15; do
            name="$group ($os)"
            found=$(awk -F '\t' -v name="$name" '$2 == name && !seen { seen = 1; print "x" $3 }' "$work/sorted.tsv")
            if [ -z "$found" ]; then
                echo "MISSING $name: no gate job"
                absent=$((absent + 1))
            elif [ "$found" = xsuccess ]; then
                echo "ok      $name"
            else
                echo "MISSING $name: ${found#x}"
                failed=$((failed + 1))
            fi
        done
    done
}

count=1
while :; do
    list
    echo "Listing $count of $listings:"
    judge
    if [ "$failed" -ne 0 ] || [ "$absent" -eq 0 ] || [ "$count" -ge "$listings" ]; then
        break
    fi
    echo "$absent expected job(s) have no entry yet; listing again in ${wait_relist}s" >&2
    sleep "$wait_relist"
    count=$((count + 1))
done

if [ "$failed" -ne 0 ] || [ "$absent" -ne 0 ]; then
    echo "::error::$HEAD_SHA is not green on both platforms"
    exit 1
fi
