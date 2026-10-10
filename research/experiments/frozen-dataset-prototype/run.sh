#!/bin/sh
# Explicit boundary probe, never a dependency of the canonical gate.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${OUT:-${RUNNER_TEMP:-/tmp}/frozen-dataset-prototype}
WFC=${WFC:-$ROOT/compiler/target/gate/whitefootc}
mkdir -p "$OUT"
printf 'program\tphase\texit_status\n' > "$OUT/results.tsv"

control_compiled=0
control_ran=0
for program in control revoke-rejected; do
    if timeout 300 "$WFC" -o "$OUT/$program" \
        "$ROOT/research/experiments/frozen-dataset-prototype/$program.wf" \
        > "$OUT/$program-compile.log" 2>&1; then
        compiled=0
    else
        compiled=$?
    fi
    cat "$OUT/$program-compile.log"
    printf '%s\tcompile\t%s\n' "$program" "$compiled" >> "$OUT/results.tsv"
    printf '%s compile exit status: %s\n' "$program" "$compiled"
    if [ "$program" = revoke-rejected ]; then
        printf '%s\trun\tnot-run-negative-source\n' "$program" >> "$OUT/results.tsv"
        printf 'Inspect the diagnostic: expected SHARE-2 at set bytes^ = move empty;\n'
        continue
    fi
    control_compiled=$compiled
    if [ "$compiled" -ne 0 ]; then
        printf 'control\trun\tnot-run-compile-failed\n' >> "$OUT/results.tsv"
        continue
    fi
    if WF_DRIVERS=1 WF_WORKERS=1 timeout 60 "$OUT/control" \
        > "$OUT/control-run.log" 2>&1; then
        control_ran=0
    else
        control_ran=$?
    fi
    cat "$OUT/control-run.log"
    printf 'control\trun\t%s\n' "$control_ran" >> "$OUT/results.tsv"
    printf 'control run exit status: %s\n' "$control_ran"
done

if [ "$control_compiled" -ne 0 ]; then
    exit "$control_compiled"
fi
if [ "$control_ran" -ne 0 ]; then
    exit "$control_ran"
fi
printf 'Prototype stopped at reserve ownership boundary; no library acceptance result.\n'
exit 80
