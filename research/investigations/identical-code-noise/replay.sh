#!/bin/bash
# Replays the new judgment (tests/performance/decide.awk) on the retained
# artifacts of past compute-regression runs, one directory per run
# (`gh run download <id> -n compute-regression-ubuntu-24.04 -D DIR/<id>`).
#   replay.sh identity DIR   a kernel whose images match is not timed; the artifacts
#                            predate the path prefix map, so "match" is the replay
#                            normalisation below
#   replay.sh all DIR        every kernel is treated as having different code
# One line per run: the run id, the CPU, the kernels treated as identical, and each
# timed kernel's decision (pass, FAIL, or indeterminate = it would be retested),
# then the totals.
#
# Replay normalisation: the executables of every retained run differ because
# native/keyed_table.o embeds the source path (see README.md). Two runs'
# kernels are taken as the same program when the module, oracle and runner objects
# and every native object are byte-identical, keyed_table.o excepted, which may
# differ in at most 130 bytes of an object of the same size, as the path does.
set -eo pipefail
mode=$1; dir=$2
here=$(cd -- "$(dirname -- "$0")" && pwd)
decide=$here/../../../tests/performance/decide.awk
kernels="mandelbrot records fir quadrature stencil"
same_program() {
    local a=$1/performance-baseline b=$1/performance-candidate k=$2 f
    for f in "$k.o" "${k}_oracle.o" runner.o; do cmp -s "$a/$f" "$b/$f" || return 1; done
    for f in $(cd "$a/native" && find . -type f -name '*.o' -o -type f -name '*.a'); do
        if [[ $f == ./keyed_table.o ]]; then
            [[ $(wc -c < "$a/native/$f") == $(wc -c < "$b/native/$f") ]] || return 1
            (( $(cmp -l "$a/native/$f" "$b/native/$f" | wc -l) <= 130 )) || return 1
        else
            cmp -s "$a/native/$f" "$b/native/$f" || return 1
        fi
    done
}
total=0; pass=0; fail=0; indeterminate=0; identical_runs=0; timed_kernels=0
for d in "$dir"/*; do
    r=$d/performance-results
    [[ -f $r/null/paired.tsv && -f $r/comparison/paired.tsv && -d $d/performance-baseline/native ]] || continue
    cpus=$(sed -n 's/^cpus=\([0-9]*\) .*/\1/p' "$r/comparison/manifest.txt")
    cpu=$(grep -m1 'Model name' "$r/identity.txt" | sed 's/.*: *//; s/ [0-9]*-Core.*//')
    identical=(); timed=()
    for k in $kernels; do
        if [[ $mode == identity ]] && same_program "$d" "$k"; then identical+=("$k"); else timed+=("$k"); fi
    done
    total=$((total + 1)); decisions=""
    if ((${#timed[@]} == 0)); then identical_runs=$((identical_runs + 1)); pass=$((pass + 1)); decisions="all identical, nothing timed"
    else
        for t in null comparison; do
            awk -v ks="${timed[*]}" 'BEGIN{n=split(ks,a," ");for(i=1;i<=n;i++)m[a[i]]=1} $1 in m' "$r/$t/paired.tsv" > "$d/replay-$t.tsv"
        done
        decisions=$(awk -v cpus="$cpus" -v kernels="${timed[*]}" -f "$decide" "$d/replay-null.tsv" "$d/replay-comparison.tsv" | sed 's/^decide: //; s/ comparison=.*//' | tr '\n' ',')
        timed_kernels=$((timed_kernels + ${#timed[@]}))
        case $decisions in
            *FAIL*) fail=$((fail + 1)) ;;
            *indeterminate*) indeterminate=$((indeterminate + 1)) ;;
            *) pass=$((pass + 1)) ;;
        esac
    fi
    printf '%s | %s | identical: %s | %s\n' "$(basename "$d")" "$cpu" "${identical[*]:-none}" "$decisions"
done
echo "runs=$total pass=$pass fail=$fail indeterminate=$indeterminate all-identical-runs=$identical_runs timed-kernel-judgments=$timed_kernels"
