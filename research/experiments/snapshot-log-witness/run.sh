#!/bin/sh
# CI-only expressibility/correctness witness; no performance comparison.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
HERE=$ROOT/research/experiments/snapshot-log-witness
OUT=${OUT:-${RUNNER_TEMP:-/tmp}/snapshot-log-witness}
WFC=${WFC:-$ROOT/compiler/target/gate/whitefootc}
mkdir -p "$OUT"

# Preserve the compiler's actual status and entire diagnostic. A timeout,
# crash or link failure is not evidence of a source-language rejection.
for mode in repro-single repro repro-map-only correct wrong; do
    code=0
    case "$mode" in
        repro-single|repro|repro-map-only) set -- "$HERE/$mode.wf" ;;
        *) set -- "$HERE/protocol.wf" "$HERE/$mode.wf" ;;
    esac
    timeout 180 "$WFC" -o "$OUT/$mode" "$@" \
        >"$OUT/$mode.compile.log" 2>&1 || code=$?
    printf 'compile\t%s\texit=%s\n' "$mode" "$code"
    cat "$OUT/$mode.compile.log"
    if [ "$code" -ne 0 ]; then
        printf 'STOP: inspect the diagnostic and minimize the rejected operation; do not substitute a workaround.\n'
        exit 2
    fi
done

printf 'mode\trun\texit\twall_seconds\n' >"$OUT/results.tsv"
correct_mismatches=0
wrong_mismatches=0
interleaved_correct=0
uninterleaved_correct=0
unexpected=0
repro_unexpected=0
map_only_unexpected=0
single_unexpected=0

sample() {
    mode=$1
    repetition=$2
    code=0
    # /usr/bin/time preserves its child's exit status; no pipeline masks it.
    /usr/bin/time -p -o "$OUT/$mode.$repetition.time" \
        env WF_DRIVERS="${DRIVERS:-2}" WF_WORKERS=1 timeout 30 "$OUT/$mode" \
        >"$OUT/$mode.$repetition.stdout" 2>"$OUT/$mode.$repetition.stderr" || code=$?
    seconds=$(awk '$1 == "real" { print $2 }' "$OUT/$mode.$repetition.time")
    printf '%s\t%s\t%s\t%s\tdrivers=%s\n' "$mode" "$repetition" "$code" "$seconds" "${DRIVERS:-2}"
    printf '%s\t%s\t%s\t%s\n' "$mode" "$repetition" "$code" "$seconds" >>"$OUT/results.tsv"
    case "$mode:$code" in
        repro-single:0|repro:0|repro-map-only:0) ;;
        repro-single:*) single_unexpected=$((single_unexpected + 1)); cat "$OUT/$mode.$repetition.stderr" ;;
        repro:*) repro_unexpected=$((repro_unexpected + 1)); cat "$OUT/$mode.$repetition.stderr" ;;
        repro-map-only:*) map_only_unexpected=$((map_only_unexpected + 1)); cat "$OUT/$mode.$repetition.stderr" ;;
        correct:0) interleaved_correct=$((interleaved_correct + 1)) ;;
        correct:10) uninterleaved_correct=$((uninterleaved_correct + 1)) ;;
        correct:70) correct_mismatches=$((correct_mismatches + 1)) ;;
        wrong:70) wrong_mismatches=$((wrong_mismatches + 1)) ;;
        wrong:0|wrong:10) ;;
        *) unexpected=$((unexpected + 1)); cat "$OUT/$mode.$repetition.stderr" ;;
    esac
}

# These fixed, small programs isolate acquisition from the export protocol.
# Keep all 20 pairs even on timeout, and still run the witness afterward.
# Their failures are separate from the witness's probe/extension decision.
# A single context (no spawn) isolates self-waiting from cross-context
# contention; map-only with one driver separates driver count.
for repetition in 1 2 3 4 5; do
    sample repro-single "$repetition"
done
DRIVERS=1
for repetition in 1 2 3; do
    sample repro-map-only "d1-$repetition"
done
unset DRIVERS
repetition=1
while [ "$repetition" -le 5 ]; do
    sample repro "$repetition"
    sample repro-map-only "$repetition"
    repetition=$((repetition + 1))
done

# Start with three small samples of each executable and inspect their spread
# before extending to the pre-registered N=20 per mode. Timings size this job;
# they are neither performance results nor source-acceptance limits.
for repetition in 1 2 3; do
    sample correct "$repetition"
    sample wrong "$repetition"
done
awk 'NR > 1 && ($1 == "correct" || $1 == "wrong") {
    if (!($1 in low) || $4 < low[$1]) low[$1] = $4;
    if (!($1 in high) || $4 > high[$1]) high[$1] = $4;
} END { for (mode in low) printf "probe %s wall_seconds min=%s max=%s\n", mode, low[mode], high[mode] }' \
    "$OUT/results.tsv"
if [ "$unexpected" -eq 0 ]; then
    repetition=4
    while [ "$repetition" -le 20 ]; do
        sample correct "$repetition"
        sample wrong "$repetition"
        repetition=$((repetition + 1))
    done
fi

{
    printf 'single_unexpected=%s\nrepro_unexpected=%s\nmap_only_unexpected=%s\n' "$single_unexpected" "$repro_unexpected" "$map_only_unexpected"
    printf 'correct_mismatches=%s\nwrong_mismatches=%s\n' "$correct_mismatches" "$wrong_mismatches"
    printf 'interleaved_correct=%s\nuninterleaved_correct=%s\nunexpected=%s\n' \
        "$interleaved_correct" "$uninterleaved_correct" "$unexpected"
} >"$OUT/summary.txt"
cat "$OUT/summary.txt"
if [ "$repro_unexpected" -ne 0 ] || [ "$map_only_unexpected" -ne 0 ] || \
   [ "$unexpected" -ne 0 ] || [ "$correct_mismatches" -ne 0 ] || \
   [ "$wrong_mismatches" -eq 0 ] || [ "$interleaved_correct" -eq 0 ]; then
    printf 'FAIL or inconclusive: compare summary with the pre-registered criteria.\n'
    exit 1
fi
