#!/bin/sh
# CI only: prebuilt arms, two drivers on two physical performance cores.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${OUT:-/tmp/guard-fairness}
K=${K:-1000}
CPUS=${CPUS:-2,4}
PATH_DIAGNOSTICS=${PATH_DIAGNOSTICS:-0}
DIAGNOSTIC_OUT=${DIAGNOSTIC_OUT:-/tmp/guard-fairness-counters}
[ "$PATH_DIAGNOSTICS" = 0 ] || [ "$PATH_DIAGNOSTICS" = 1 ] || { printf 'PATH_DIAGNOSTICS must be 0 or 1\n' >&2; exit 1; }
[ "$K" = 1000 ] || { printf 'registered comparison requires K=1000\n' >&2; exit 1; }
[ "$CPUS" = 2,4 ] || { printf 'registered comparison requires CPUs 2,4\n' >&2; exit 1; }
mkdir -p "$OUT/raw"
cmp "$OUT/baseline" "$OUT/twin"
printf 'revision,%s\n' "$(git -C "$ROOT" rev-parse HEAD)"
uname -srvmo
lscpu
for cpu in $(printf '%s' "$CPUS" | tr ',' ' '); do
    printf 'cpu,%s,siblings,%s\n' "$cpu" "$(cat /sys/devices/system/cpu/cpu$cpu/topology/thread_siblings_list)"
done
sha256sum "$OUT/baseline" "$OUT/age" "$OUT/oldest" "$OUT/twin"
python3 "$ROOT/research/experiments/guard-fairness/summarize.py" init "$OUT" 0
printf 'path_diagnostics,%s\n' "$PATH_DIAGNOSTICS"
if [ "$PATH_DIAGNOSTICS" = 1 ]; then
    [ "$OUT" != "$DIAGNOSTIC_OUT" ] || { printf 'diagnostic and timing paths must differ\n' >&2; exit 1; }
    mkdir -p "$DIAGNOSTIC_OUT/raw"
    # Both constructions start with exactly the original emitted probe.
    cmp "$OUT/probe.ll" "$DIAGNOSTIC_OUT/probe.ll"
    sha256sum "$DIAGNOSTIC_OUT/baseline" "$DIAGNOSTIC_OUT/age" "$DIAGNOSTIC_OUT/oldest" "$DIAGNOSTIC_OUT/probe-diagnostic.ll"
    python3 "$ROOT/research/experiments/guard-fairness/summarize.py" init "$DIAGNOSTIC_OUT" 1
fi

run_one() (
    pass=$1 arm=$2 n=$3 rounds=$4
    diagnostic=${5:-0}
    images="$OUT"
    if [ "$diagnostic" = 1 ]; then images="$DIAGNOSTIC_OUT"; fi
    record="$images/raw/$pass-$arm-$n"
    printf 'pass,%s,arm,%s,N,%s,K,%s,cpus,%s,diagnostic,%s\n' "$pass" "$arm" "$n" "$rounds" "$CPUS" "$diagnostic"
    # Read the child's status directly: no tee pipeline can conceal a failure.
    # A pilot over ten seconds stops expansion to the fixed full scale.
    limit=120s
    case "$pass" in pilot*) limit=10s ;; esac
    WF_DRIVERS=2 timeout "$limit" taskset -c "$CPUS" "$images/$arm" "$n" "$rounds" > "$record.stdout" 2> "$record.stderr"
    cat "$record.stdout" "$record.stderr"
    python3 "$ROOT/research/experiments/guard-fairness/summarize.py" record \
        "$record" "$pass" "$arm" "$n" "$rounds" "$diagnostic" "$images"
)

# Two smallest useful pilots expose spread before the fixed scale. Retain
# their timings, but exclude them from the registered acceptance decision.
for arm in oldest baseline age twin; do
    run_one pilot1 "$arm" 2 100
    run_one pilot2 "$arm" 2 100
    if [ "$PATH_DIAGNOSTICS" = 1 ] && [ "$arm" != twin ]; then
        run_one pilot1 "$arm" 2 100 1
        run_one pilot2 "$arm" 2 100 1
    fi
done
for n in 2 8 50; do
    for arm in oldest baseline age twin; do
        run_one 1 "$arm" "$n" "$K"
        # A distinct counter-enabled process follows each matching timing
        # cell. Deleting diagnostic executions recovers the original order.
        if [ "$PATH_DIAGNOSTICS" = 1 ] && [ "$arm" != twin ]; then
            run_one diagnostic "$arm" "$n" "$K" 1
        fi
    done
done
for n in 50 8 2; do
    for arm in twin age baseline oldest; do run_one 2 "$arm" "$n" "$K"; done
done
python3 "$ROOT/research/experiments/guard-fairness/summarize.py" judge "$OUT/measurements.csv" "$K" > "$OUT/verdict.txt"
cat "$OUT/verdict.txt"
if [ "$PATH_DIAGNOSTICS" = 1 ]; then
    python3 "$ROOT/research/experiments/guard-fairness/summarize.py" diagnostics \
        "$OUT/measurements.csv" "$DIAGNOSTIC_OUT/measurements.csv" "$K"
fi
