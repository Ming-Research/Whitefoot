#!/usr/bin/env bash
# The data-placement experiment's measurements on one host
# (research/investigations/data-placement/README.md, "Method").
#
# usage: probe.sh IMAGES RESULTS
#
# IMAGES holds one directory of the five regression images per revision, each
# linked with shift.o: b (the stop-signals merge base), s (the receiver thread
# started before the entry), h (this branch) and hd (h with its static data
# moved by a pad), and the alias program. A missing revision is skipped.
# RESULTS must not exist. No measurement's failure stops the others; every
# campaign's exit status is recorded in RESULTS/statuses.txt.
set -uo pipefail
[[ $# == 2 ]] || { echo 'usage: probe.sh IMAGES RESULTS' >&2; exit 2; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
root=$(cd -- "$here/../../.." && pwd)
images=$(cd -- "$1" && pwd)
[[ ! -e $2 ]] || { echo 'results must be a fresh directory' >&2; exit 2; }
mkdir -p "$2/arms" "$2/trace"
results=$(cd -- "$2" && pwd)
kernels=(mandelbrot records fir quadrature stencil)
machine=$(uname -m)

have() { [[ -x $images/$1/stencil ]]; }

# arm NAME REVISION PREFIX...: a directory whose five "images" run REVISION's
# real images behind PREFIX, so compare.sh times the same image under one more
# setting. The wrapper replaces itself with the image before anything is timed.
arm() {
    local name=$1 revision=$2 kernel
    shift 2
    mkdir -p "$results/arms/$name"
    for kernel in "${kernels[@]}"; do
        printf '#!/bin/sh\nexec %s "%s" "$@"\n' "$*" "$images/$revision/$kernel" \
            > "$results/arms/$name/$kernel"
        chmod +x "$results/arms/$name/$kernel"
    done
}

# campaign NAME BASELINE CANDIDATE: one compare.sh campaign, its verdict shown.
campaign() {
    local status=0 start=$SECONDS
    bash "$root/tests/performance/compare.sh" "$2" "$3" "$results/$1" \
        > "$results/$1.out" 2>&1 || status=$?
    echo "== $1: exit $status, $((SECONDS - start)) s" | tee -a "$results/statuses.txt"
    if [[ -f $results/$1/verdict.txt ]]; then
        grep -E '^(mandelbrot|records|fir|quadrature|stencil) |^VERDICT|^REFUSED' \
            "$results/$1/verdict.txt"
    else
        tail -n 5 "$results/$1.out"
    fi
}

# trace NAME COUNT REVISION PREFIX...: COUNT launches of REVISION's stencil at
# one worker, each with its mappings traced. Only mmap and munmap stop the
# process, four times inside each timed call.
trace() {
    local name=$1 count=$2 revision=$3 i
    shift 3
    for ((i = 0; i < count; i++)); do
        env -u WF_SPLIT_WORK WF_WORKERS=1 \
            strace -f --seccomp-bpf -e trace=mmap,munmap -o "$results/trace/$name-$i.strace" \
            "$@" "$images/$revision/stencil" measure baseline 1 0 \
            > "$results/trace/$name-$i.tsv" 2> "$results/trace/$name-$i.log" \
            || echo "trace $name-$i failed" | tee -a "$results/statuses.txt"
    done
    echo "== trace $name: $count launches"
}

{
    lscpu | grep -E '^(Model name|CPU\(s\)|Thread|Core|L1d|L2|L3)'
    uname -r
    getconf GNU_LIBC_VERSION
    printf 'stack limit (KiB): %s\n' "$(ulimit -s)"
    printf 'randomize_va_space: %s\n' "$(cat /proc/sys/kernel/randomize_va_space)"
    printf 'transparent_hugepage: %s\n' "$(cat /sys/kernel/mm/transparent_hugepage/enabled)"
} > "$results/host.txt" 2>&1
cat "$results/host.txt"

# The two arrays' distance alone, on this processor. The first series keeps the
# low address bits the traced regression images had; the second starts from an
# address whose bits 12 to 27 are zero, so the distance is the bit difference.
if [[ -x $images/alias ]]; then
    {
        echo '# low distance fastest_ns median_ns'
        real=600009fff000
        for distance in 2001000 7fff000 8000000 8001000 7ffe000 61ff000 6bff000 7dff000 \
            3fff000 4000000 2200000 a001000 10000000 40000000 47a00000 47c00000 46ff0000; do
            "$images/alias" "$real" "$distance"
        done
        zero=600000000000
        for pattern in 0 1000 2000 100000 8000000 8001000 4002000 2004000 108000 210000 \
            420000 840000 1080000; do
            "$images/alias" "$zero" "$(printf '%x' $((0x10000000 + 0x$pattern)))"
        done
    } > "$results/alias.txt" 2>&1
    cat "$results/alias.txt"
fi

# The static-data pad must move data symbols and no code symbol.
if have h && have hd; then
    for kernel in "${kernels[@]}"; do
        symbols() { nm "$images/$1/$kernel" | awk -v types="$2" 'NF == 3 && $2 ~ types { print $3, $1 }' | sort; }
        code=$(comm -3 <(symbols h '^[Tt]$') <(symbols hd '^[Tt]$') | wc -l)
        data=$(comm -3 <(symbols h '^[DdBb]$') <(symbols hd '^[DdBb]$') | wc -l)
        echo "$kernel: code symbol lines that differ: $code; data symbol lines that differ: $data"
    done > "$results/datapad.txt" 2>&1
    cat "$results/datapad.txt"
fi

for revision in b s h; do
    have "$revision" || continue
    arm "$revision-thread" "$revision" env WF_DATA_SHIFT=thread
    arm "$revision-map8" "$revision" env WF_DATA_SHIFT=map:8392704
    arm "$revision-arena1" "$revision" env GLIBC_TUNABLES=glibc.malloc.arena_max=1
done
if have h; then
    arm h-touch h env WF_DATA_SHIFT=touch
    arm h-map1 h env WF_DATA_SHIFT=map:1052672
    arm h-map34 h env WF_DATA_SHIFT=map:35651584
fi

# Campaigns, the ones the criterion reads first.
have h && campaign null-h "$images/h" "$images/h"
have b && have s && campaign bs "$images/b" "$images/s"
for move in thread arena1 map8; do
    for revision in b s h; do
        have "$revision" && campaign "$revision-$move" "$images/$revision" "$results/arms/$revision-$move"
    done
done
if have h; then
    for move in touch map1 map34; do
        campaign "h-$move" "$images/h" "$results/arms/h-$move"
    done
    have hd && campaign h-datapad "$images/h" "$images/hd"
fi

# Traced launches: which mappings each launch got, beside its time.
for revision in b s h; do
    have "$revision" || continue
    trace "$revision-none" 24 "$revision"
    trace "$revision-thread" 16 "$revision" env WF_DATA_SHIFT=thread
    trace "$revision-arena1" 6 "$revision" env GLIBC_TUNABLES=glibc.malloc.arena_max=1
    # Without address randomization a launch's mappings are fixed, and an idle
    # mapping of k times 2 MiB ahead of them moves the entry stack by that much:
    # 32 steps cover every position within the allocator's 64 MiB heap alignment.
    for ((k = 0; k < 32; k++)); do
        trace "$revision-fixed-$k" 1 "$revision" \
            env WF_DATA_SHIFT=map:$((k * 2097152)) setarch "$machine" -R
    done
done

python3 -B "$here/reduce.py" "$results" > "$results/summary.txt" 2>&1 \
    || echo 'reduce.py failed' | tee -a "$results/statuses.txt"
cat "$results/summary.txt"
