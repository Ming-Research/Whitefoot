#!/bin/sh
# The check of echo-scale-summary.sh: it feeds the summary fabricated samples
# and requires the exit status and the reason each one must produce, so a
# summary that stopped rejecting a bad pass, or that judged a doubling by the
# wrong rule, fails here instead of passing a curve it should not.
#
#   sh echo-scale-summary-check.sh
#
# It measures nothing and needs only a POSIX shell and awk.
set -e

HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# make_samples FILE WF_FACTOR URING_FACTOR EPOLL_FACTOR [BUSY]: five passes of
# each line at 1, 2, 4 and 8 cores, the rate of each line multiplying by its
# factor at every doubling from 100000 round trips a second at one core.
make_samples() {
    awk -v wf="$2" -v uring="$3" -v epoll="$4" -v busy="${5:-40}" 'BEGIN {
        factor["wf"] = wf; factor["uring"] = uring; factor["epoll"] = epoll
        for (line in factor) {
            for (cores = 1; cores <= 8; cores *= 2) {
                rate = 100000
                for (c = 1; c < cores; c *= 2) rate *= factor[line]
                drivers = (line == "wf") ? cores : "-"
                for (pass = 1; pass <= 5; pass++)
                    printf "%s\t%d\t%d\t%.1f\t50\t200\t90000\t%s\t%s\t0\t0\t0\n", \
                        line, cores, pass, rate * (1 + (pass - 3) / 100), busy, drivers
            }
        }
    }' > "$1"
}

expect() {
    name=$1; wanted_status=$2; wanted_text=$3; file=$4
    status=0
    output=$(sh "$HERE/echo-scale-summary.sh" "$file") || status=$?
    if [ "$status" != "$wanted_status" ]; then
        echo "FAIL $name: exit status $status, wanted $wanted_status" >&2
        echo "$output" >&2
        exit 1
    fi
    if ! echo "$output" | grep -q "$wanted_text"; then
        echo "FAIL $name: the output lacks '$wanted_text'" >&2
        echo "$output" >&2
        exit 1
    fi
    echo "ok   $name"
}

# A curve of 1.9 per doubling reaches the step.
make_samples "$WORK/good.tsv" 1.9 1.9 1.9
expect "1.9 per doubling reaches" 0 "RESULT: reached" "$WORK/good.tsv"

# A curve of 1.3 per doubling does not, while the control's 1.9 shows the
# harness could carry more: a miss.
make_samples "$WORK/miss.tsv" 1.3 1.9 1.9
expect "1.3 per doubling beside a control of 1.9 is a miss" 4 "RESULT: miss" "$WORK/miss.tsv"

# The same 1.3 beside a control that is also 1.3 is the harness's limit.
make_samples "$WORK/limited.tsv" 1.3 1.3 1.3
expect "1.3 beside a control of 1.3 is harness-limited" 3 "RESULT: harness-limited" "$WORK/limited.tsv"

# A pass whose busiest client CPU passed the limit is rejected.
make_samples "$WORK/busy.tsv" 1.9 1.9 1.9 90
expect "a client busier than the limit is rejected" 2 "is over 80%" "$WORK/busy.tsv"

# A pass whose server exited nonzero is rejected.
make_samples "$WORK/server.tsv" 1.9 1.9 1.9
awk -F'\t' -v OFS='\t' 'NR == 7 { $10 = 1 } { print }' "$WORK/server.tsv" > "$WORK/server-bad.tsv"
expect "a nonzero server exit is rejected" 2 "server exit 1" "$WORK/server-bad.tsv"

# A pass whose load generator failed is rejected, whatever its other fields say.
awk -F'\t' -v OFS='\t' 'NR == 9 { $11 = 1 } { print }' "$WORK/server.tsv" > "$WORK/load-bad.tsv"
expect "a nonzero load generator exit is rejected" 2 "load generator exit 1" "$WORK/load-bad.tsv"

# A Whitefoot pass at four cores that ran one driver is rejected, though it
# echoed every byte.
awk -F'\t' -v OFS='\t' '$1 == "wf" && $2 == 4 && $3 == 2 { $9 = 1 } { print }' "$WORK/server.tsv" > "$WORK/drivers-bad.tsv"
expect "a wrong driver count is rejected" 2 "ran 1 drivers, not 4" "$WORK/drivers-bad.tsv"

# One core may report no driver line at all, as the runtime prints none.
awk -F'\t' -v OFS='\t' '$1 == "wf" && $2 == 1 { $9 = 0 } { print }' "$WORK/server.tsv" > "$WORK/one-core.tsv"
expect "one core with no driver line is accepted" 0 "RESULT: reached" "$WORK/one-core.tsv"

# Fewer valid passes than required leaves a cell without evidence.
awk -F'\t' '!($1 == "wf" && $2 == 8 && $3 >= 3)' "$WORK/server.tsv" > "$WORK/few.tsv"
expect "too few passes are rejected" 2 "fewer than 5" "$WORK/few.tsv"

# Without the control line a miss cannot be told from the harness's limit.
awk -F'\t' '$1 != "uring"' "$WORK/server.tsv" > "$WORK/no-control.tsv"
expect "a missing control is rejected" 2 "no uring control line" "$WORK/no-control.tsv"

# A row cut short is rejected, not read as zeros.
awk -F'\t' -v OFS='\t' 'NR == 5 { print $1, $2, $3; next } { print }' "$WORK/server.tsv" > "$WORK/short.tsv"
expect "a short row is rejected" 2 "has 3 fields" "$WORK/short.tsv"

echo "echo-scale-summary-check: every case behaved"
