#!/bin/sh
# Runs every arm Makefile built in BUILD at one and four workers, ROUNDS times,
# rotating the arm order each round and alternating the worker order, and
# prints one row per run: round, arm, workers, wall nanoseconds.
#   sh measure.sh BUILD ROUNDS > runs.tsv
set -eu
build=$1
rounds=$2
arms="emitted twin direct zero plain"
printf 'round\tarm\tworkers\tns\n'
round=1
while [ "$round" -le "$rounds" ]; do
    shift_by=$(( (round - 1) % 5 ))
    order=$(echo $arms | awk -v k="$shift_by" '{ for (i = 0; i < NF; i++) printf "%s ", $(((i + k) % NF) + 1) }')
    if [ $((round % 2)) -eq 1 ]; then widths="1 4"; else widths="4 1"; fi
    for arm in $order; do
        for workers in $widths; do
            start=$(date +%s%N)
            WF_WORKERS=$workers "$build/$arm"
            end=$(date +%s%N)
            printf '%s\t%s\t%s\t%s\n' "$round" "$arm" "$workers" $((end - start))
        done
    done
    round=$((round + 1))
done
