#!/usr/bin/env bash
# Explicit Linux measurement entry, consumed by compute-regression only.
# This is never called by make check or routine correctness CI.
set -euo pipefail
if [[ $# != 3 && $# != 4 ]]; then
    echo 'usage: compare.sh BASELINE_IMAGES CANDIDATE_IMAGES RESULTS [slow]' >&2
    exit 2
fi
[[ $(uname -s) == Linux ]] || { echo 'paired verdict requires the Linux host' >&2; exit 2; }
[[ $# == 3 || $4 == slow ]] || exit 2
here=$(cd -- "$(dirname -- "$0")" && pwd)
baseline=$1; candidate=$2; results=$3
[[ ! -e $results ]] || { echo 'results must be a fresh directory' >&2; exit 2; }
mkdir -p "$results/logs"
cpus=$(nproc)
((cpus >= 2)) || { echo 'at least two available CPUs required' >&2; exit 2; }
widths=(1 2)
if ((cpus >= 4)); then widths+=(4); fi
kernels=(mandelbrot records fir quadrature stencil)
{
    printf 'cpus=%s widths=%s passes=5 warmups=1 calls=5\n' "$cpus" "${widths[*]}"
    printf 'baseline=%s\ncandidate=%s\ncontrol=%s\n' "$baseline" "$candidate" "${4:-none}"
    uname -a
    for arm in "$baseline" "$candidate"; do
        sha256sum "${kernels[@]/#/$arm/}"
    done
} > "$results/manifest.txt"
# Verify both actual emissions and their own runtimes before any paired timing.
for kernel in "${kernels[@]}"; do
    for arm in baseline candidate; do
        directory=${!arm}
        for width in "${widths[@]}"; do
            env -u WF_SPLIT_WORK WF_WORKERS="$width" timeout 60s "$directory/$kernel" verify \
                > "$results/logs/verify-$kernel-$arm-$width.log" 2>&1
        done
    done
done
: > "$results/raw.tsv"
for pass in {0..4}; do
    for ((j=0;j<${#kernels[@]};j++)); do
        kernel=${kernels[$(((j + pass) % ${#kernels[@]}))]}
        for ((v=0;v<${#widths[@]};v++)); do
            width=${widths[$(((v + pass) % ${#widths[@]}))]}
            arms=(baseline candidate)
            if (((pass + j + v) % 2)); then arms=(candidate baseline); fi
            for arm in "${arms[@]}"; do
                directory=${!arm}; control=()
                if [[ $arm == candidate && ${4:-} == slow ]]; then control=(slow); fi
                key=$kernel-$arm-$width-$pass
                echo "measure $key"
                env -u WF_SPLIT_WORK WF_WORKERS="$width" timeout 60s "$directory/$kernel" \
                    measure "$arm" "$width" "$pass" "${control[@]}" \
                    > "$results/logs/$key.tsv" 2> "$results/logs/$key.log"
                cat "$results/logs/$key.tsv" >> "$results/raw.tsv"
            done
        done
    done
done
awk -v cpus="$cpus" -f "$here/reduce.awk" "$results/raw.tsv" > "$results/paired.tsv"
# Do not pipe a decision through tee without preserving its exit status.
status=0
awk -v cpus="$cpus" -f "$here/verdict.awk" "$results/paired.tsv" > "$results/verdict.txt" || status=$?
cat "$results/verdict.txt"
exit "$status"
