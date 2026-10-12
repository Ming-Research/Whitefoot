#!/usr/bin/env bash
# Builds and runs the shared-counter arms (README.md beside this file), ported
# from the runtime-contention harness of Firn-wf (README.md, "Source").
#
#   run.sh --out DIR [--build-only | --skip-build]
#
# Environment:
#   WHITEFOOTC  the Whitefoot compiler to build the arms with (required unless
#               --skip-build)
#   PILOT       1 runs one block at the 8-P-core list only; otherwise the main
#               run (default: main)
#   BLOCKS      blocks of the main run (default 6, provisional)
#   CONTEXTS    contexts or threads each arm starts (default 64)
#   MILLIS      milliseconds each cell lasts (default 5000)
#   ARMS        arms to run, in rotation order (default
#               "W0 W1 W2 W2t W2r W6 W3 W3rd W5 W5p")
#   CPU_LISTS   taskset CPU lists (default "0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15")
#
# The build writes DIR/bin/ and DIR/host.txt. The run appends one row per cell
# to DIR/raw.tsv:
#   block pos arm cpus drivers contexts ops ns ops_per_s check reads read_ns
#   read_max_ns user_frac sys_frac
# where user_frac and sys_frac are the shares of the listed CPUs' time that
# /proc/stat counts as user (user and nice) and system (system, irq and
# softirq) between just before and just after the cell. Each cell's standard
# output and error are kept in DIR/runs/. A cell that fails, or reports a
# driver count other than the list's, is reported on standard error, leaves no
# row, and makes the script exit 1 after the remaining cells ran.
# Linux only: it needs taskset and /proc/stat.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

out=
build=1
run=1
while [ $# -gt 0 ]; do
    case $1 in
        --out) out=$2; shift 2 ;;
        --build-only) run=0; shift ;;
        --skip-build) build=0; shift ;;
        *) echo "usage: run.sh --out DIR [--build-only | --skip-build]" >&2; exit 2 ;;
    esac
done
[ -n "$out" ] || { echo "usage: run.sh --out DIR [--build-only | --skip-build]" >&2; exit 2; }

PILOT=${PILOT:-0}
BLOCKS=${BLOCKS:-6}
CONTEXTS=${CONTEXTS:-64}
MILLIS=${MILLIS:-5000}
ARMS=${ARMS:-"W0 W1 W2 W2t W2r W6 W3 W3rd W5 W5p"}
CPU_LISTS=${CPU_LISTS:-"0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15"}
if [ "$PILOT" = 1 ]; then
    BLOCKS=1
    CPU_LISTS="0,2,4,6,8,10,12,14"
fi

mkdir -p "$out/bin" "$out/runs"

# The binary of an arm, and the entry or define it is built from.
binary_of() {
    case $1 in
        W0) echo w_base ;;
        W1) echo w_private ;;
        W2) echo w_shared ;;
        W2t) echo w_shared_twin ;;
        W2r) echo w_shared_param ;;
        W6) echo w_read_shared ;;
        W3) echo w_shards ;;
        W3rd) echo w_shards_reader ;;
        W5) echo w5 ;;
        W5p) echo w5p ;;
        *) echo "unknown arm $1" >&2; exit 2 ;;
    esac
}

if [ "$build" = 1 ]; then
    : "${WHITEFOOTC:?WHITEFOOTC names the compiler to build the arms with}"
    clang=${CLANG:-/usr/bin/clang}
    for arm in $ARMS; do
        bin=$(binary_of "$arm")
        case $arm in
            W5) "$clang" -O2 -pthread -o "$out/bin/$bin" "$here/w5.c" ;;
            W5p) "$clang" -O2 -pthread -DPADDED=1 -o "$out/bin/$bin" "$here/w5.c" ;;
            W2t) "$WHITEFOOTC" --full-lto --graph "$here/modules.wfg" --entry w_shared -o "$out/bin/$bin" ;;
            *) "$WHITEFOOTC" --full-lto --graph "$here/modules.wfg" --entry "$bin" -o "$out/bin/$bin" ;;
        esac
    done

    # Each atomic statement whose handle is not a read-only reference parameter
    # counts the object's handles up with a call of wf__shared_share at its
    # start. The counts are recorded, not enforced: W2 and W2r should differ by
    # the one in the statement, and W3 shows whether a reference into an array
    # element is counted.
    share_calls() {
        "$WHITEFOOTC" --emit-llvm --graph "$here/modules.wfg" --entry "$1" -o "$out/bin/$1.ll" ||
            { echo unavailable; return 0; }
        grep -c 'call void @wf__shared_share(' "$out/bin/$1.ll" || true
    }
    share_record="wf__shared_share calls: shared=$(share_calls w_shared) shared_param=$(share_calls w_shared_param) shards=$(share_calls w_shards) shards_reader=$(share_calls w_shards_reader)"

    twin_record="W2 and its twin: $(sha256sum "$out/bin/w_shared" "$out/bin/w_shared_twin" | awk '{print $1}' | tr '\n' ' ')"
    {
        echo "kernel: $(uname -srvmo)"
        echo "cpus: $(nproc)"
        echo "model: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"
        echo "load: $(cat /proc/loadavg)"
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unavailable)"
        echo "no_turbo: $(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || echo unavailable)"
        echo "compiler release: ${WF_RELEASE:-unnamed}"
        echo "clang: $("$clang" --version | head -n 1)"
        echo "$share_record"
        echo "$twin_record"
        echo "contexts: $CONTEXTS  millis: $MILLIS  blocks: $BLOCKS  pilot: $PILOT"
        echo "arms: $ARMS"
        echo "cpu lists: $CPU_LISTS"
        echo "cpu map (CPU, core, max MHz):"
        lscpu -e=CPU,CORE,MAXMHZ 2>/dev/null || echo unavailable
    } | tee "$out/host.txt"
fi

[ "$run" = 1 ] || exit 0

# The CPU numbers a taskset list names, one per line: "0-3,8" gives 0 1 2 3 8.
expand_list() {
    echo "$1" | tr ',' '\n' | while IFS=- read -r first last; do
        seq "$first" "${last:-$first}"
    done
}

# The listed CPUs' user, system and total jiffies, summed, from /proc/stat.
cpu_times() {
    awk -v cpus="$(expand_list "$1" | tr '\n' ' ')" '
        BEGIN { n = split(cpus, list, " "); for (i = 1; i <= n; i++) want["cpu" list[i]] = 1 }
        $1 in want {
            user += $2 + $3; sys += $4 + $7 + $8
            for (f = 2; f <= 9; f++) total += $f
        }
        END { print user, sys, total }' /proc/stat
}

raw="$out/raw.tsv"
printf 'block\tpos\tarm\tcpus\tdrivers\tcontexts\tops\tns\tops_per_s\tcheck\treads\tread_ns\tread_max_ns\tuser_frac\tsys_frac\n' > "$raw"
failed=0
read -r -a arms <<< "$ARMS"
count=${#arms[@]}
# Each block starts its arm order one position later than the block before.
for block in $(seq 0 $((BLOCKS - 1))); do
    order=
    for pos in $(seq 0 $((count - 1))); do
        order="$order ${arms[$(((pos + block) % count))]}"
    done
    for list in $CPU_LISTS; do
        drivers=$(expand_list "$list" | wc -l | tr -d ' ')
        pos=0
        for arm in $order; do
            pos=$((pos + 1))
            bin=$(binary_of "$arm")
            name="b$block-$drivers-$pos-$arm"
            before=$(cpu_times "$list")
            status=0
            WF_DRIVERS=$drivers WF_SCHED_REPORT=2 timeout $((MILLIS / 1000 + 120)) \
                taskset -c "$list" "$out/bin/$bin" "$CONTEXTS" "$MILLIS" \
                > "$out/runs/$name.out" 2> "$out/runs/$name.err" || status=$?
            after=$(cpu_times "$list")
            line=$(cat "$out/runs/$name.out")
            echo "$name: $line (status $status)"
            seen=$(echo "$line" | sed -n 's/.* drivers=\([0-9]*\) .*/\1/p')
            if [ "$status" -ne 0 ] || [ "$seen" != "$drivers" ]; then
                echo "$name: status $status, drivers $seen where $drivers were set" >&2
                failed=1
                continue
            fi
            echo "$line" | awk -v block="$block" -v pos="$pos" -v arm="$arm" -v list="$list" \
                -v drivers="$drivers" -v before="$before" -v after="$after" '
                {
                    for (i = 1; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
                    split(before, b, " "); split(after, a, " ")
                    total = a[3] - b[3]
                    rate = v["ops_per_s"]
                    if (rate == "?") rate = sprintf("%.0f", v["ops"] / (v["ns"] / 1e9))
                    user = 0; sys = 0
                    if (total > 0) { user = (a[1] - b[1]) / total; sys = (a[2] - b[2]) / total }
                    printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%.4f\t%.4f\n", block, pos, arm, list,
                        drivers, v["contexts"], v["ops"], v["ns"], rate, v["check"], v["reads"],
                        v["read_ns"], v["read_max_ns"], user, sys
                }' >> "$raw"
        done
    done
done
cat "$raw"
exit "$failed"
