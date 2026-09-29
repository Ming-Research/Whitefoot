#!/bin/sh
# Checks each probe of research/experiments/monitor-invariants with the
# worktree's compiler and compares its verdict with the one RESULTS.md
# records: "accepted", or the first diagnostic's rule and disposition. A
# changed verdict is printed and the script exits 1, so a proof-engine change
# that widens or narrows what the probes show is noticed.
#
#   sh research/experiments/monitor-invariants/run.sh
set -u

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
WHITEFOOTC=${WHITEFOOTC:-$ROOT/compiler/target/debug/whitefootc}

expected() {
    case $1 in
        queue-count | bank-values | bank-snapshot | ghost-counters | field-premise-copied)
            echo accepted ;;
        queue-missed-update) echo "INV-1 Refuted" ;;
        queue-split-transaction | bank-snapshot-without-bridge) echo "INV-1 Unproved" ;;
        bank-field-premise | bank-values-equality-premise | ghost-counters-unbounded | field-premise-direct)
            echo "OP-2 Unproved" ;;
        *) echo unknown ;;
    esac
}

status=0
for probe in "$HERE"/probes/*.wf; do
    name=$(basename "$probe" .wf)
    output=$("$WHITEFOOTC" --check "$probe" 2>&1)
    if [ -z "$output" ]; then
        verdict=accepted
    else
        rule=$(printf '%s\n' "$output" | sed -n '1s/.*error\[\([A-Z0-9-]*\)\].*/\1/p')
        disposition=$(printf '%s\n' "$output" | sed -n 's/^ *disposition: //p' | head -1)
        verdict="$rule $disposition"
    fi
    want=$(expected "$name")
    if [ "$verdict" = "$want" ]; then
        echo "ok      $name: $verdict"
    else
        echo "CHANGED $name: $verdict (recorded: $want)"
        status=1
    fi
done
exit $status
