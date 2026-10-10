#!/bin/sh
# Explicit research invocation; never a dependency of the canonical gate.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${OUT:-${RUNNER_TEMP:-/tmp}/frozen-dataset-witness}
WFC=${WFC:-$ROOT/compiler/target/gate/whitefootc}
mkdir -p "$OUT"
printf 'program\tphase\texit_status\n' > "$OUT/results.tsv"

# Preserve the command's own status, including timeout or signal termination.
if timeout 300 "$WFC" -o "$OUT/witness" \
    "$ROOT/research/experiments/frozen-dataset-witness/witness.wf" \
    > "$OUT/compile.log" 2>&1; then
    compiled=0
else
    compiled=$?
fi
cat "$OUT/compile.log"
printf 'witness\tcompile\t%s\n' "$compiled" >> "$OUT/results.tsv"
printf 'witness compile exit status: %s\n' "$compiled"
if [ "$compiled" -ne 0 ]; then
    printf 'witness\trun\tnot-run\n' >> "$OUT/results.tsv"
    printf 'witness run: not run (compilation did not succeed)\n'
    exit "$compiled"
fi

# One small correctness run, no --par and no performance measurement.
if WF_DRIVERS=2 WF_WORKERS=1 timeout 60 "$OUT/witness" \
    > "$OUT/run.log" 2>&1; then
    ran=0
else
    ran=$?
fi
cat "$OUT/run.log"
printf 'witness\trun\t%s\n' "$ran" >> "$OUT/results.tsv"
printf 'witness run exit status: %s\n' "$ran"
exit "$ran"
