#!/bin/sh
# Runs the guard-fairness probe with one or two compilers, interleaved, on
# two drivers pinned to distinct physical performance cores. CI only:
#   BASE_WFC=... WFC=... sh research/experiments/guard-fairness/run.sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
WFC=${WFC:-"$ROOT/compiler/target/gate/whitefootc"}
BASE_WFC=${BASE_WFC:-}
OUT=${OUT:-/tmp/guard-fairness}
K=${K:-1000}
PASSES=${PASSES:-2}
CPUS=${CPUS:-2,4}
mkdir -p "$OUT"
arms=candidate
"$WFC" --full-lto "$ROOT/research/experiments/guard-fairness/probe.wf" -o "$OUT/candidate"
if [ -n "$BASE_WFC" ]; then
    "$BASE_WFC" --full-lto "$ROOT/research/experiments/guard-fairness/probe.wf" -o "$OUT/base"
    arms="base candidate"
fi
printf 'revision,%s\n' "$(git -C "$ROOT" rev-parse HEAD)"
uname -srvmo
for cpu in $(echo "$CPUS" | tr ',' ' '); do
    printf 'cpu,%s,siblings,%s\n' "$cpu" "$(cat /sys/devices/system/cpu/cpu$cpu/topology/thread_siblings_list 2>/dev/null || echo unknown)"
done
for arm in $arms; do
    printf 'pilot,%s,N=2,K=100\n' "$arm"
    WF_DRIVERS=2 taskset -c "$CPUS" "$OUT/$arm" 2 100
done
pass=1
while [ "$pass" -le "$PASSES" ]; do
    order='2 8 50'
    if [ $((pass % 2)) -eq 0 ]; then order='50 8 2'; fi
    for n in $order; do
        for arm in $arms; do
            printf 'pass,%s,arm,%s,N,%s,K,%s,cpus,%s\n' "$pass" "$arm" "$n" "$K" "$CPUS"
            WF_DRIVERS=2 taskset -c "$CPUS" "$OUT/$arm" "$n" "$K"
        done
    done
    pass=$((pass + 1))
done
