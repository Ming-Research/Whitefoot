#!/usr/bin/env bash
# Builds and runs the uncontended-cost comparison of the shared-take claims
# (research/investigations/shared-take-fairness/DESIGN.md, "Registered
# criterion", part 3, and "Uncontended-cost procedure").
#
#   run.sh --out DIR --build-only | --idle-check | --run
#
# --build-only needs, in the environment:
#   BASE_WFC   the compiler built from the base revision
#   CAND_WFC   the compiler built from the candidate revision
#   BASE_REV, CAND_REV  the two revisions, recorded only
# It builds both programs (take.wf, own.wf) with each compiler from the same
# source, copies each base image to its twin, writes DIR/bin/ and
# DIR/host.txt, and exits 1 when a twin is not byte-identical to its base,
# when a candidate image is byte-identical to its base (the runtime change
# would not reach the program), or when the take program's emitted LLVM calls
# no take entry point.
#
# --idle-check exits 1 unless all CPUs were under 5% busy over 5 s.
#
# --run runs the cells; environment:
#   PILOT      1 runs one block at the 8-P-core list only; otherwise the main
#              run (default: main)
#   BLOCKS     blocks of the main run (default 6)
#   MILLIS     milliseconds each cell lasts (default 3000)
#   CPU_LISTS  taskset CPU lists (default "0 0,2 0,2,4,6 0,2,4,6,8,10,12,14",
#              one hyperthread of each of 1, 2, 4 and 8 P-cores)
# Each cell runs one image with WF_DRIVERS equal to the list's CPU count and
# one context per driver. Within a block every CPU list runs the six arms
# once each; each block starts the arm order one position later than the
# block before, so over six blocks every arm takes every position. It appends
# one row per cell to DIR/raw.tsv:
#   block pos arm program image cpus drivers contexts ops ns ops_per_s check
#   wrong user_frac sys_frac host_busy
# user_frac and sys_frac are the listed CPUs' shares of user (user, nice) and
# system (system, irq, softirq) time between just before and just after the
# cell; host_busy is the busy share of all CPUs over the same interval. Each
# cell's output is kept in DIR/runs/. A cell that fails, reports a driver or
# context count other than the list's, or reports wrong=W other than 0 (a
# context's objects ended at another value than its operation count) leaves
# no row, is reported on standard error and makes the script exit 1 after the
# remaining cells ran. Linux only: it needs taskset and /proc/stat.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

usage() {
    echo "usage: run.sh --out DIR --build-only | --idle-check | --run" >&2
    exit 2
}

out=
action=
while [ $# -gt 0 ]; do
    case $1 in
        --out) out=$2; shift 2 ;;
        --build-only) action=build; shift ;;
        --idle-check) action=idle; shift ;;
        --run) action=run; shift ;;
        *) usage ;;
    esac
done
[ -n "$out" ] && [ -n "$action" ] || usage

# The six arms: program and image. The twin is a byte-for-byte copy of the
# base image, run in its own rotation slot as the noise control.
ARMS="Tb Tc Tt Ob Oc Ot"
program_of() {
    case $1 in
        T*) echo take ;;
        O*) echo own ;;
        *) echo "unknown arm $1" >&2; exit 2 ;;
    esac
}
image_of() {
    case $1 in
        ?b) echo base ;;
        ?c) echo candidate ;;
        ?t) echo twin ;;
        *) echo "unknown arm $1" >&2; exit 2 ;;
    esac
}

# The busy and total jiffies of all CPUs, from /proc/stat's first line.
host_times() {
    awk '/^cpu / { idle = $5 + $6; total = 0; for (i = 2; i <= 9; i++) total += $i; print total - idle, total; exit }' /proc/stat
}

if [ "$action" = idle ]; then
    read -r busy_before total_before < <(host_times)
    sleep 5
    read -r busy_after total_after < <(host_times)
    busy=$(awk -v bb="$busy_before" -v tb="$total_before" -v ba="$busy_after" -v ta="$total_after" \
        'BEGIN { printf "%.4f", (ba - bb) / (ta - tb) }')
    echo "load: $(cat /proc/loadavg); busy share of all CPUs over 5 s: $busy"
    ps -eo pid,pcpu,args --sort=-pcpu | head -n 8
    if awk -v busy="$busy" 'BEGIN { exit !(busy > 0.05) }'; then
        echo "the host is not idle; stop its other work first" >&2
        exit 1
    fi
    exit 0
fi

if [ "$action" = build ]; then
    : "${BASE_WFC:?BASE_WFC names the base compiler}"
    : "${CAND_WFC:?CAND_WFC names the candidate compiler}"
    mkdir -p "$out/bin" "$out/llvm"
    failed=0
    calls() {
        local count
        count=$(grep -c "$1" "$2" || true)
        echo "${count:-0}"
    }
    llvm_record=
    for program in take own; do
        "$BASE_WFC" --full-lto --graph "$here/modules.wfg" --entry "w_$program" -o "$out/bin/${program}_base"
        "$CAND_WFC" --full-lto --graph "$here/modules.wfg" --entry "w_$program" -o "$out/bin/${program}_candidate"
        cp "$out/bin/${program}_base" "$out/bin/${program}_twin"
        if ! cmp -s "$out/bin/${program}_base" "$out/bin/${program}_twin"; then
            echo "$program: the twin is not byte-identical to the base image" >&2
            failed=1
        fi
        if cmp -s "$out/bin/${program}_base" "$out/bin/${program}_candidate"; then
            echo "$program: the candidate image is byte-identical to the base image, so the runtime change did not reach it" >&2
            failed=1
        fi
        for side in base candidate; do
            if [ "$side" = base ]; then wfc=$BASE_WFC; else wfc=$CAND_WFC; fi
            ll="$out/llvm/${program}_$side.ll"
            "$wfc" --emit-llvm --graph "$here/modules.wfg" --entry "w_$program" -o "$ll"
            take=$(calls 'call void @wf__shared_take(' "$ll")
            group=$(calls '@wf__atomic_group_\(acquire\|take\)(' "$ll")
            acquire=$(calls '@wf__shared_acquire(' "$ll")
            llvm_record="$llvm_record
  $program $side: wf__shared_take=$take wf__atomic_group_acquire/take=$group wf__shared_acquire=$acquire"
            if [ "$program" = take ] && [ $((take + group)) -eq 0 ]; then
                echo "take ($side): the emitted LLVM calls neither wf__shared_take nor a group acquisition, so the program does not reach the take" >&2
                failed=1
            fi
        done
        if cmp -s "$out/llvm/${program}_base.ll" "$out/llvm/${program}_candidate.ll"; then
            llvm_record="$llvm_record
  $program: base and candidate emit identical LLVM"
        else
            llvm_record="$llvm_record
  $program: base and candidate emit different LLVM"
        fi
    done
    clang=${CLANG:-/usr/bin/clang}
    {
        echo "kernel: $(uname -srvmo)"
        echo "cpus: $(nproc)"
        echo "model: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"
        echo "load: $(cat /proc/loadavg)"
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unavailable)"
        echo "no_turbo: $(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || echo unavailable)"
        echo "base revision: ${BASE_REV:-unnamed}"
        echo "candidate revision: ${CAND_REV:-unnamed}"
        echo "base compiler: $(sha256sum "$BASE_WFC" | awk '{print $1}')"
        echo "candidate compiler: $(sha256sum "$CAND_WFC" | awk '{print $1}')"
        echo "clang: $("$clang" --version | head -n 1)"
        echo "images (sha256):"
        for program in take own; do
            for image in base twin candidate; do
                echo "  ${program}_$image $(sha256sum "$out/bin/${program}_$image" | awk '{print $1}')"
            done
        done
        echo "emitted LLVM call sites:$llvm_record"
        echo "cpu map (CPU, core, max MHz):"
        lscpu -e=CPU,CORE,MAXMHZ 2>/dev/null || echo unavailable
    } | tee "$out/host.txt"
    exit "$failed"
fi

PILOT=${PILOT:-0}
BLOCKS=${BLOCKS:-6}
MILLIS=${MILLIS:-3000}
CPU_LISTS=${CPU_LISTS:-"0 0,2 0,2,4,6 0,2,4,6,8,10,12,14"}
if [ "$PILOT" = 1 ]; then
    BLOCKS=1
    CPU_LISTS="0,2,4,6,8,10,12,14"
fi
mkdir -p "$out/runs"
{
    echo "pilot: $PILOT  blocks: $BLOCKS  millis: $MILLIS"
    echo "arms: $ARMS"
    echo "cpu lists: $CPU_LISTS"
} | tee -a "$out/host.txt"

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
printf 'block\tpos\tarm\tprogram\timage\tcpus\tdrivers\tcontexts\tops\tns\tops_per_s\tcheck\twrong\tuser_frac\tsys_frac\thost_busy\n' > "$raw"
failed=0
read -r -a arms <<< "$ARMS"
count=${#arms[@]}
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
            program=$(program_of "$arm")
            image=$(image_of "$arm")
            name="b$block-$drivers-$pos-$arm"
            before=$(cpu_times "$list")
            host_before=$(host_times)
            status=0
            WF_DRIVERS=$drivers timeout $((MILLIS / 1000 + 60)) \
                taskset -c "$list" "$out/bin/${program}_$image" "$drivers" "$MILLIS" \
                > "$out/runs/$name.out" 2> "$out/runs/$name.err" < /dev/null || status=$?
            after=$(cpu_times "$list")
            host_after=$(host_times)
            line=$(cat "$out/runs/$name.out")
            echo "$name: $line (status $status)"
            seen_drivers=$(echo "$line" | sed -n 's/.* drivers=\([0-9]*\) .*/\1/p')
            seen_contexts=$(echo "$line" | sed -n 's/.* contexts=\([0-9]*\) .*/\1/p')
            seen_wrong=$(echo "$line" | sed -n 's/.* wrong=\([0-9]*\)$/\1/p')
            if [ "$status" -ne 0 ] || [ "$seen_drivers" != "$drivers" ] || [ "$seen_contexts" != "$drivers" ] || [ "$seen_wrong" != 0 ]; then
                echo "$name: status $status, drivers $seen_drivers and contexts $seen_contexts where $drivers were set, wrong $seen_wrong" >&2
                failed=1
                continue
            fi
            echo "$line" | awk -v block="$block" -v pos="$pos" -v arm="$arm" -v program="$program" \
                -v image="$image" -v list="$list" -v drivers="$drivers" -v before="$before" -v after="$after" \
                -v host_before="$host_before" -v host_after="$host_after" '
                {
                    for (i = 1; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
                    split(before, b, " "); split(after, a, " ")
                    split(host_before, hb, " "); split(host_after, ha, " ")
                    total = a[3] - b[3]
                    rate = v["ops_per_s"]
                    if (rate == "?") rate = sprintf("%.0f", v["ops"] / (v["ns"] / 1e9))
                    user = 0; sys = 0; busy = 0
                    if (total > 0) { user = (a[1] - b[1]) / total; sys = (a[2] - b[2]) / total }
                    if (ha[2] > hb[2]) busy = (ha[1] - hb[1]) / (ha[2] - hb[2])
                    printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%.4f\t%.4f\t%.4f\n", block, pos, arm,
                        program, image, list, drivers, v["contexts"], v["ops"], v["ns"], rate, v["check"],
                        v["wrong"], user, sys, busy
                }' >> "$raw"
        done
    done
done
cat "$raw"
exit "$failed"
