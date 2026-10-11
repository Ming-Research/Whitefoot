#!/bin/sh
# CI only: prebuilt arms, two drivers on two physical performance cores.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${OUT:-/tmp/guard-fairness}
K=${K:-1000}
CPUS=${CPUS:-2,4}
DIAGNOSTIC=${DIAGNOSTIC:-0}
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
printf 'pass,arm,contexts,rounds,count,p50_ns,p99_ns,max_ns,hold_ns_mean,runtime_ns,checksum\n' > "$OUT/measurements.csv"
printf 'pass,arm,contexts,rounds,count,woken_per_acquisition,spurious_per_acquisition,refused_locks_per_acquisition,clock_reads_per_acquisition\n' > "$OUT/counters.csv"

run_one() {
    pass=$1 arm=$2 n=$3 rounds=$4
    record="$OUT/raw/$pass-$arm-$n"
    printf 'pass,%s,arm,%s,N,%s,K,%s,cpus,%s\n' "$pass" "$arm" "$n" "$rounds" "$CPUS"
    # Read the child's status directly: no tee pipeline can conceal a failure.
    # A pilot over ten seconds stops expansion to the fixed full scale.
    limit=120s
    case "$pass" in pilot*) limit=10s ;; esac
    WF_DRIVERS=2 timeout "$limit" taskset -c "$CPUS" "$OUT/$arm" "$n" "$rounds" > "$record.stdout" 2> "$record.stderr"
    cat "$record.stdout" "$record.stderr"
    python3 "$ROOT/research/experiments/guard-fairness/summarize.py" record \
        "$record" "$pass" "$arm" "$n" "$rounds" "$DIAGNOSTIC" "$OUT"
}

# Two smallest useful pilots expose spread before the fixed scale. Retain
# their timings, but exclude them from the registered acceptance decision.
for arm in oldest baseline age twin; do
    run_one pilot1 "$arm" 2 100
    run_one pilot2 "$arm" 2 100
done
if [ "$DIAGNOSTIC" = 1 ]; then
    for n in 2 8 50; do
        for arm in oldest baseline age; do run_one diagnostic "$arm" "$n" "$K"; done
    done
    printf 'diagnostic_only,no_acceptance_judgment\n'
else
    for n in 2 8 50; do
        for arm in oldest baseline age twin; do run_one 1 "$arm" "$n" "$K"; done
    done
    for n in 50 8 2; do
        for arm in twin age baseline oldest; do run_one 2 "$arm" "$n" "$K"; done
    done
    python3 "$ROOT/research/experiments/guard-fairness/summarize.py" judge "$OUT/measurements.csv" "$K" > "$OUT/verdict.txt"
    cat "$OUT/verdict.txt"
fi
