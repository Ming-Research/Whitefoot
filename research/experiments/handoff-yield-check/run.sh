#!/bin/sh
# The 14900K measurements of research/investigations/driver-handoff/DESIGN.md,
# "Plan for the 14900K measurements", items 1 to 3: the ready context's and
# the polling shape's latencies (latency.py), and the fixed cost of an
# uncontended atomic statement, shared-take-overhead's `own` program against
# the C reference (c_reference.c). Firn's throughput, item 4, is io-bench.yml's
# firn comparison.
#
#   BASE_WFC, CAND_WFC   compilers built from the base and candidate revisions
#   BASE_REV, CAND_REV   the two revisions, recorded only
#   OUT                  the output directory
#
# Linux only, with taskset and a C compiler. Exits nonzero when a build or a
# fixed-cost cell fails; latency.py judges nothing.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$HERE/../../.." && pwd)
: "${BASE_WFC:?BASE_WFC names the base compiler}"
: "${CAND_WFC:?CAND_WFC names the candidate compiler}"
: "${OUT:?OUT names the output directory}"
mkdir -p "$OUT/bin"
for side in base candidate; do
    if [ "$side" = base ]; then wfc=$BASE_WFC; else wfc=$CAND_WFC; fi
    "$wfc" -o "$OUT/bin/ready_$side" "$ROOT/tests/programs/ready_beside_computations.wf"
    "$wfc" -o "$OUT/bin/poll_$side" "$ROOT/tests/programs/lone_timer_writer.wf"
    "$wfc" -o "$OUT/bin/turn_$side" "$ROOT/tests/programs/poll_contexts.wf"
done
${CC:-cc} -std=c11 -O2 -o "$OUT/bin/c_reference" "$HERE/c_reference.c"

# The fixed cost: one context on one CPU and one per driver on two P-cores,
# six blocks of the six arms, then the C reference six times each way on
# CPU 0, three seconds a run as the arms run.
bash "$ROOT/research/experiments/shared-take-overhead/run.sh" --out "$OUT/fixed" --idle-check ||
    echo "the host was not idle before the runs; see the load above"
bash "$ROOT/research/experiments/shared-take-overhead/run.sh" --out "$OUT/fixed" --build-only
CPU_LISTS="0 0,2" BLOCKS=6 MILLIS=3000 \
    bash "$ROOT/research/experiments/shared-take-overhead/run.sh" --out "$OUT/fixed" --run
: >"$OUT/c_reference.txt"
for block in 1 2 3 4 5 6; do
    for way in plain locked; do
        taskset -c 0 "$OUT/bin/c_reference" "$way" 3000 | sed "s/^/block=$block /" | tee -a "$OUT/c_reference.txt"
    done
done

python3 "$HERE/latency.py" "$OUT"
