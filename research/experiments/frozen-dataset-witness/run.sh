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
    "$ROOT/research/experiments/frozen-dataset-witness/heap_report.wf" \
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
    ran=0
else
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
fi

# An independent diagnostic follows even a failed witness. Its status never
# turns that failure into success; its output is not a new witness verdict.
if timeout 300 "$WFC" -o "$OUT/diag" \
    "$ROOT/research/experiments/frozen-dataset-witness/diag.wf" \
    "$ROOT/research/experiments/frozen-dataset-witness/heap_report.wf" \
    > "$OUT/diag-compile.log" 2>&1; then
    diag_compiled=0
else
    diag_compiled=$?
fi
cat "$OUT/diag-compile.log"
printf 'diag\tcompile\t%s\n' "$diag_compiled" >> "$OUT/results.tsv"
printf 'diag compile exit status: %s\n' "$diag_compiled"
if [ "$diag_compiled" -ne 0 ]; then
    printf 'diag\trun\tnot-run\n' >> "$OUT/results.tsv"
    printf 'diag run: not run (compilation did not succeed)\n'
    diag_ran=0
else
    if WF_DRIVERS=2 WF_WORKERS=1 timeout 60 "$OUT/diag" \
        > "$OUT/diag-run.log" 2>&1; then
        diag_ran=0
    else
        diag_ran=$?
    fi
    cat "$OUT/diag-run.log"
    printf 'diag\trun\t%s\n' "$diag_ran" >> "$OUT/results.tsv"
    printf 'diag run exit status: %s\n' "$diag_ran"
fi

if [ "$compiled" -ne 0 ]; then
    exit "$compiled"
fi
if [ "$ran" -ne 0 ]; then
    exit "$ran"
fi
if [ "$diag_compiled" -ne 0 ]; then
    exit "$diag_compiled"
fi
exit "$diag_ran"
