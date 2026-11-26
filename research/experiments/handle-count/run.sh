#!/usr/bin/env bash
# Runs the arms of research/investigations/handle-count in interleaved blocks.
# It builds nothing: build.sh made the binaries.
#
#   run.sh --out DIR --bins DIR
#
# Environment:
#   BLOCKS    blocks (default 12)
#   MILLIS    milliseconds each arm-run lasts (default 1000)
#   CPU_LIST  taskset CPU list; one context per CPU, one driver per CPU
#             (default "0,2,4,6,8,10,12,14", the 14900K's 8 P-cores)
#
# Arms, by the binary in --bins: PRV pr_value, PRR pr_ref, BV base_value,
# PRR2 pr_ref2 (the twin of PRR). Block b rotates the order by b positions;
# odd blocks run it backwards, so each pair of arms runs in both orders equally
# often. Appends one row per run to DIR/raw.tsv:
#   block pos arm contexts drivers ops ns ops_per_s check
# A run that fails, whose witness name is not its arm's, whose contexts or
# drivers differ from the list's, or whose check differs from ops, is reported
# on standard error, leaves no row, and makes the script exit 1 after the other
# runs. Standard output and error of each run are kept in DIR/runs/.
# Linux only (taskset).
set -eu

out=
bins=
while [ $# -gt 0 ]; do
    case $1 in
        --out) out=$2; shift 2 ;;
        --bins) bins=$2; shift 2 ;;
        *) echo "usage: run.sh --out DIR --bins DIR" >&2; exit 2 ;;
    esac
done
[ -n "$out" ] && [ -n "$bins" ] || { echo "usage: run.sh --out DIR --bins DIR" >&2; exit 2; }

BLOCKS=${BLOCKS:-12}
MILLIS=${MILLIS:-1000}
CPU_LIST=${CPU_LIST:-0,2,4,6,8,10,12,14}
ARMS="PRV PRR BV PRR2"

binary_of() {
    case $1 in
        PRV) echo pr_value ;;
        PRR) echo pr_ref ;;
        BV) echo base_value ;;
        PRR2) echo pr_ref2 ;;
    esac
}
witness_of() {
    case $1 in
        PRV | BV) echo value ;;
        PRR | PRR2) echo ref ;;
    esac
}

expand_list() {
    echo "$1" | tr ',' '\n' | while IFS=- read -r first last; do
        seq "$first" "${last:-$first}"
    done
}
drivers=$(expand_list "$CPU_LIST" | wc -l | tr -d ' ')

mkdir -p "$out/runs"
raw="$out/raw.tsv"
printf 'block\tpos\tarm\tcontexts\tdrivers\tops\tns\tops_per_s\tcheck\n' > "$raw"
failed=0
read -r -a arms <<< "$ARMS"
count=${#arms[@]}
for block in $(seq 0 $((BLOCKS - 1))); do
    order=
    for pos in $(seq 0 $((count - 1))); do
        next=${arms[$(((pos + block) % count))]}
        if [ $((block % 2)) = 1 ]; then
            order="$next $order"
        else
            order="$order $next"
        fi
    done
    pos=0
    for arm in $order; do
        pos=$((pos + 1))
        bin=$(binary_of "$arm")
        name="b$block-$pos-$arm"
        status=0
        WF_DRIVERS=$drivers timeout $((MILLIS / 1000 + 120)) \
            taskset -c "$CPU_LIST" "$bins/$bin" "$drivers" "$MILLIS" \
            > "$out/runs/$name.out" 2> "$out/runs/$name.err" || status=$?
        line=$(cat "$out/runs/$name.out")
        echo "$name: $line (status $status)"
        row=$(echo "$line" | awk -v want="$(witness_of "$arm")" -v drivers="$drivers" '
            {
                for (i = 1; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
                if (v["witness"] != want || v["contexts"] != drivers || v["drivers"] != drivers) exit 1
                if (v["ops"] + 0 == 0 || v["ns"] + 0 == 0 || v["check"] != v["ops"]) exit 1
                printf "%s\t%s\t%s\t%s\t%.0f\t%s\n", v["contexts"], v["drivers"], v["ops"], v["ns"], v["ops"] / (v["ns"] / 1e9), v["check"]
            }') || row=
        if [ "$status" -ne 0 ] || [ -z "$row" ]; then
            echo "$name: status $status; the line is missing, names another arm, or its contexts, drivers or check are wrong" >&2
            failed=1
            continue
        fi
        printf '%s\t%s\t%s\t%s\n' "$block" "$pos" "$arm" "$row" >> "$raw"
    done
done
cat "$raw"
exit "$failed"
