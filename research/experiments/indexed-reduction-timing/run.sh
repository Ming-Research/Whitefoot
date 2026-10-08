#!/usr/bin/env bash
# The timing experiment of research/experiments/indexed-reduction-timing/README.md,
# dispatched by .github/workflows/indexed-reduction-timing.yml. Explicit
# research only: no gate runs it.
#
# usage: run.sh probe|measure WFC WORK [ROUNDS]
#
# WFC is a built whitefootc; WORK a fresh directory. `probe` runs 3 rounds so
# the spread can be seen before a scale is chosen; `measure` runs ROUNDS rounds.
# Environment: CPUS (default 0-7) is the taskset list every timed process is
# pinned to, WORKERS (default 8) the worker count of the parallel build, and
# REPS (default 20) the positive histogram repetition count.
#
# For each of the two programs (histogram.wf, 256 cells over u8 keys, and
# histogram_4096.wf, 4096 cells over u16 keys masked to twelve bits) it builds
#
#   seq    the plain build, run at WF_WORKERS=1
#   par8   the `--par` build, run at WF_WORKERS=$WORKERS
#   twin   a byte copy of seq, run exactly as seq: the noise control
#   par1   the `--par` build run at WF_WORKERS=1: the parallel build's
#          sequential world, which separates the cost of carrying the
#          parallel lowering from the benefit of workers
#
# Every process generates ten million keys, runs K histograms over the same
# keys, folds every digest into a checksum, and writes one little-endian word.
# Each build in each round runs with K=0 and K=REPS. WORK/raw.tsv receives
# "cells round build workers k wall_ns checksum" rows; summarize.py computes
# (wall_ns at K=REPS - wall_ns at K=0) / REPS for each build and round.
# One unrecorded pass checks every build's checksum against seq for the same
# cell count and K; any later difference also ends in a nonzero status.
set -euo pipefail
if [[ $# != 3 && $# != 4 ]]; then
    echo 'usage: run.sh probe|measure WFC WORK [ROUNDS]' >&2
    exit 2
fi
mode=$1; wfc=$(realpath -- "$2"); work=$3; rounds=${4:-}
case $mode in
    probe) rounds=3 ;;
    measure) [[ $rounds =~ ^[1-9][0-9]*$ ]] || { echo 'measure needs a positive ROUNDS' >&2; exit 2; } ;;
    *) echo 'MODE must be probe or measure' >&2; exit 2 ;;
esac
[[ ! -e $work ]] || { echo 'WORK must be a fresh directory' >&2; exit 2; }
cpus=${CPUS:-0-7}
workers=${WORKERS:-8}
reps=${REPS:-20}
if [[ ! $reps =~ ^[1-9][0-9]{0,19}$ ]] ||
   [[ ${#reps} == 20 && $reps > 18446744073709551615 ]]; then
    echo 'REPS must be a positive decimal u64 with at most twenty digits' >&2
    exit 2
fi
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cells_list=(256 4096)
builds=(seq par8 twin par1)
mkdir -p "$work/bin" "$work/logs"
work=$(cd -- "$work" && pwd)

pinned=$(taskset -c "$cpus" nproc)
if ((pinned < workers)); then
    echo "warning: CPUS=$cpus gives $pinned processors for WORKERS=$workers" >&2
fi

for cells in "${cells_list[@]}"; do
    if [[ $cells == 256 ]]; then program=$here/histogram.wf; else program=$here/histogram_$cells.wf; fi
    "$wfc" "$program" -o "$work/bin/seq-$cells"
    "$wfc" --par "$program" -o "$work/bin/par-$cells"
    cp "$work/bin/seq-$cells" "$work/bin/twin-$cells"
done

{
    printf 'mode=%s rounds=%s reps=%s cpus=%s workers=%s pinned=%s\n' "$mode" "$rounds" "$reps" "$cpus" "$workers" "$pinned"
    git -C "$here" rev-parse HEAD 2>/dev/null || true
    uname -a
    /usr/bin/clang --version | head -1
    lscpu 2>/dev/null || true
    sha256sum "$work"/bin/* "$here"/histogram*.wf
} > "$work/manifest.txt"

# run_one CELLS BUILD K [REPORT]: sets wall and sum for one process.
run_one() {
    local cells=$1 build=$2 k=$3 binary w start end
    case $build in
        seq) binary=seq; w=1 ;;
        par8) binary=par; w=$workers ;;
        twin) binary=twin; w=1 ;;
        par1) binary=par; w=1 ;;
    esac
    start=$(date +%s%N)
    env -u WF_SPLIT_WORK -u WF_SCHED_REPORT WF_WORKERS="$w" ${4:+WF_SCHED_REPORT=2} \
        taskset -c "$cpus" timeout 120s "$work/bin/$binary-$cells" "$k" \
        > "$work/out.bin" 2> "$work/err.txt"
    end=$(date +%s%N)
    wall=$((end - start))
    [[ $(stat -c %s "$work/out.bin") == 8 ]] || { echo "$build-$cells at K=$k: output is not 8 bytes" >&2; return 1; }
    sum=$(od -An -v -w8 -tu8 -N8 "$work/out.bin" | tr -d '[:space:]')
    return 0
}

declare -A reference
failed=0
check_sum() {
    local cells=$1 build=$2 k=$3 key="$1,$3"
    if [[ $sum != "${reference[$key]}" ]]; then
        echo "CHECKSUM MISMATCH: $build at $cells cells and K=$k gave $sum, seq gave ${reference[$key]}" >&2
        failed=1
    fi
}

for cells in "${cells_list[@]}"; do
    for build in "${builds[@]}"; do
        for k in 0 "$reps"; do
            run_one "$cells" "$build" "$k"
            if [[ $build == seq ]]; then reference["$cells,$k"]=$sum; fi
            check_sum "$cells" "$build" "$k"
        done
    done
    run_one "$cells" par8 "$reps" report
    check_sum "$cells" par8 "$reps"
    cp "$work/err.txt" "$work/logs/sched-report-$cells.txt"
    echo "par8 $cells cells: $(cat "$work/err.txt")"
done
if ((failed)); then exit 1; fi
echo "verified: every build's checksum equals seq's at K=0 and K=$reps"

printf 'cells\tround\tbuild\tworkers\tk\twall_ns\tchecksum\n' > "$work/raw.tsv"
for ((round = 0; round < rounds; round++)); do
    for cells in "${cells_list[@]}"; do
        for build in "${builds[@]}"; do
            for k in 0 "$reps"; do
                run_one "$cells" "$build" "$k"
                case $build in par8) w=$workers ;; *) w=1 ;; esac
                printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$cells" "$round" "$build" "$w" "$k" "$wall" "$sum" >> "$work/raw.tsv"
                check_sum "$cells" "$build" "$k"
            done
        done
    done
    echo "round $round of $rounds done"
done

python3 -I "$here/summarize.py" "$work/raw.tsv" > "$work/summary.txt"
cat "$work/summary.txt"
if ((failed)); then exit 1; fi
