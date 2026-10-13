#!/usr/bin/env bash
# Builds and runs the arms of Q1, Q2 and Q3 of research/investigations/
# sequence-take/DESIGN.md section 7 (written before any measurement); the
# shapes follow research/experiments/shared-counters/ on the counter study's
# branch.
#
#   run.sh --out DIR [--build-only | --skip-build]
#
# Environment:
#   WHITEFOOTC      the Whitefoot compiler for the Q2 or Q3 arms (without it only Q1
#                   is built and run)
#   WF_RELEASE      a name for that compiler's revision, recorded in host.txt
#   KINDS           "q1 q2" (default), q1, q2 or q3; Q3 includes its Q2 controls
#   MODE            main (default), pilot or smoke. pilot: one block at the
#                   8-P-core list, timed, with the projected main duration
#                   written to pilot.txt. smoke: short cells on a host that is
#                   not the 14900K, no verdict, checks the wiring and that the
#                   correctness runs tell T2 from Tbad
#   BLOCKS          blocks of the main run (default 6)
#   MILLIS          milliseconds of each timed cell (default 5000)
#   CHECK_MILLIS    milliseconds of each correctness run (default 1000)
#   CPU_LISTS       taskset lists (default "0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15")
#   CHECK_LIST      the list of the correctness runs (default the 8-P-core one)
#   CALIB_CPU       the CPU of the calibration (default 0)
#   W_LIST          nanoseconds of private work per operation (default
#                   "100 200 400 800")
#   CONTEXTS        contexts of the Q2 arms (default 64)
#   CHECK_ATTEMPTS  runs of Tbad's correctness run allowed to show an N3
#                   failure (default 1; smoke 5, because N3 needs two contexts
#                   whose statements are ordered against their numbers)
#   REQUIRE_IDLE    1 (default except smoke): the host must be under 5% busy
#                   before the run and before each block
#   CLANG           the C compiler (default /usr/bin/clang)
#
# Output in DIR: bin/, host.txt, calib.tsv (the calibrated step counts), raw.tsv
#   block pos kind arm w cpus drivers contexts ops ns ops_per_s check
# correct.tsv (block arm attempts status verdict expect line), idle.tsv and,
# for a pilot, pilot.txt. A cell that fails, or reports a driver count other
# than its list's, is reported, leaves no row and makes the script exit 1 after
# the remaining cells ran. Linux only: it needs taskset and /proc/stat.
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

MODE=${MODE:-main}
BLOCKS=${BLOCKS:-6}
MILLIS=${MILLIS:-5000}
CHECK_MILLIS=${CHECK_MILLIS:-1000}
KINDS=${KINDS:-"q1 q2"}
if [ "$KINDS" = q3 ]; then
    CPU_LISTS=${CPU_LISTS:-"0 0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15"}
else
    CPU_LISTS=${CPU_LISTS:-"0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15"}
fi
CHECK_LIST=${CHECK_LIST:-0,2,4,6,8,10,12,14}
CALIB_CPU=${CALIB_CPU:-0}
W_LIST=${W_LIST:-"100 200 400 800"}
CONTEXTS=${CONTEXTS:-64}
CHECK_ATTEMPTS=${CHECK_ATTEMPTS:-1}
REQUIRE_IDLE=${REQUIRE_IDLE:-1}
clang=${CLANG:-/usr/bin/clang}
case $MODE in
    main | pilot | smoke) ;;
    *) echo "MODE must be main, pilot or smoke" >&2; exit 2 ;;
esac
if [ "$MODE" = pilot ]; then
    BLOCKS=1
    CPU_LISTS=$CHECK_LIST
fi
if [ "$MODE" = smoke ]; then
    REQUIRE_IDLE=0
    CHECK_ATTEMPTS=5
fi

has_kind() {
    case " $KINDS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

mkdir -p "$out/bin" "$out/runs"

if [ "$build" = 1 ]; then
    "$clang" -O2 -pthread -o "$out/bin/q1" "$here/q1.c"
    "$clang" -O2 -pthread -o "$out/bin/q1_twin" "$here/q1.c"
    wf_record="whitefootc: not used (Q2 not built)"
    twin_record="T0 and its twin: not built"
    share_record="wf__shared_share calls: not recorded"
    if has_kind q2 || has_kind q3; then
        : "${WHITEFOOTC:?WHITEFOOTC names the compiler to build the Whitefoot arms with}"
        wf_graph="$here/modules.wfg"
        entries="t0 t2 c_t2 c_tbad"
        if has_kind q3; then
            # Bind Q3 and its controls as one module without making the Q2
            # source graph name types that older released compilers lack.
            mkdir -p "$out/source-q3"
            cp "$here/harness.wf" "$here/timed.wf" "$here/correct.wf" "$out/source-q3/"
            cp "$here/q3/atomic.wf" "$out/source-q3/"
            cat "$here/module.wfm" "$here/q3/module.wfm" > "$out/source-q3/module.wfm"
            cat "$here/modules.wfg" "$here/q3/modules.wfg" > "$out/source-q3/modules.wfg"
            wf_graph="$out/source-q3/modules.wfg"
            entries="$entries t1 t1off tp tc c_t1"
        fi
        for entry in $entries; do
            "$WHITEFOOTC" --full-lto --graph "$wf_graph" --entry "$entry" -o "$out/bin/$entry"
        done
        "$WHITEFOOTC" --full-lto --graph "$wf_graph" --entry t0 -o "$out/bin/t0_twin"
        if has_kind q3; then
            "$WHITEFOOTC" --full-lto --graph "$wf_graph" --entry t1 -o "$out/bin/t1_twin"
        fi
        wf_record="whitefootc: $("$WHITEFOOTC" --version 2>&1 | head -n 1)"
        twin_record="T0 and its twin: $(sha256sum "$out/bin/t0" "$out/bin/t0_twin" | awk '{print $1}' | tr '\n' ' ')"
        share_calls() {
            "$WHITEFOOTC" --emit-llvm --graph "$wf_graph" --entry "$1" -o "$out/bin/$1.ll" ||
                { echo unavailable; return 0; }
            grep -c 'call void @wf__shared_share(' "$out/bin/$1.ll" || true
        }
        share_record="wf__shared_share calls in the emitted code: t0=$(share_calls t0) t2=$(share_calls t2)"
        if has_kind q3; then
            twin_record="$twin_record; T1 and its twin: $(sha256sum "$out/bin/t1" "$out/bin/t1_twin" | awk '{print $1}' | tr '\n' ' ')"
            share_record="$share_record t1=$(share_calls t1) t1off=$(share_calls t1off) tp=$(share_calls tp) tc=$(share_calls tc)"
        fi
    fi
    {
        echo "kernel: $(uname -srvmo)"
        echo "cpus: $(nproc)"
        echo "model: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"
        echo "load: $(cat /proc/loadavg)"
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unavailable)"
        echo "no_turbo: $(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || echo unavailable)"
        echo "compiler revision: ${WF_RELEASE:-unnamed}"
        echo "$wf_record"
        echo "clang: $("$clang" --version | head -n 1)"
        echo "q1 and its twin: $(sha256sum "$out/bin/q1" "$out/bin/q1_twin" | awk '{print $1}' | tr '\n' ' ')"
        echo "$twin_record"
        echo "$share_record"
        echo "mode: $MODE  kinds: $KINDS  millis: $MILLIS  check millis: $CHECK_MILLIS  blocks: $BLOCKS  contexts: $CONTEXTS"
        echo "cpu lists: $CPU_LISTS  check list: $CHECK_LIST  calibration cpu: $CALIB_CPU  w list: $W_LIST"
        echo "cpu map (CPU, core, max MHz):"
        lscpu -e=CPU,CORE,MAXMHZ 2>/dev/null || echo unavailable
    } | tee "$out/host.txt"
fi

[ "$run" = 1 ] || exit 0

expand_list() {
    echo "$1" | tr ',' '\n' | while IFS=- read -r first last; do
        seq "$first" "${last:-$first}"
    done
}

cpu_times() {
    awk -v cpus="$(expand_list "$1" | tr '\n' ' ')" '
        BEGIN { n = split(cpus, list, " "); for (i = 1; i <= n; i++) want["cpu" list[i]] = 1 }
        $1 in want {
            user += $2 + $3; sys += $4 + $7 + $8
            for (f = 2; f <= 9; f++) total += $f
        }
        END { print user, sys, total }' /proc/stat
}

busy_share() {
    local ib tb ia ta
    read -r ib tb < <(awk '/^cpu / {idle = $5 + $6; total = 0; for (i = 2; i <= NF; i++) total += $i; print idle, total}' /proc/stat)
    sleep "$1"
    read -r ia ta < <(awk '/^cpu / {idle = $5 + $6; total = 0; for (i = 2; i <= NF; i++) total += $i; print idle, total}' /proc/stat)
    awk -v ib="$ib" -v tb="$tb" -v ia="$ia" -v ta="$ta" 'BEGIN { printf "%.4f", 1 - (ia - ib) / (ta - tb) }'
}

idle_tsv="$out/idle.tsv"
printf 'block\tbusy\n' > "$idle_tsv"
idle_check() {
    local busy
    busy=$(busy_share 3)
    printf '%s\t%s\n' "$1" "$busy" >> "$idle_tsv"
    echo "idle check before block $1: busy share $busy"
    if [ "$REQUIRE_IDLE" = 1 ] && awk -v busy="$busy" 'BEGIN { exit !(busy > 0.05) }'; then
        echo "the host is not idle (busy share $busy over 3 s)" >&2
        ps -eo pid,pcpu,args --sort=-pcpu | head -n 8 >&2
        exit 3
    fi
}

started_at=$(date +%s)

if has_kind q1; then
    printf 'w\tsteps\tns_per_op\n' > "$out/calib.tsv"
    for w in $W_LIST; do
        for pass in 1 2 3; do
            taskset -c "$CALIB_CPU" "$out/bin/q1" calibrate "$w" | tee -a "$out/runs/calibrate-$w.out"
        done
        sed -n 's/^target_ns=[0-9]* steps=\([0-9]*\) ns_per_op=\(.*\)$/\1 \2/p' "$out/runs/calibrate-$w.out" |
            sort -n | sed -n '2p' | awk -v w="$w" '{ printf "%s\t%s\t%s\n", w, $1, $2 }' >> "$out/calib.tsv"
    done
    cat "$out/calib.tsv"
fi

steps_of() {
    awk -v w="$1" '$1 == w { print $2 }' "$out/calib.tsv"
}

units=()
if has_kind q1; then
    for w in $W_LIST; do
        for arm in S0 S0t S1 S1t S2 S3 S4; do
            units+=("q1:$arm:$w")
        done
    done
fi
if has_kind q3; then
    for arm in T0 T0t T2 T1 T1t T1off TP TC; do
        units+=("q3:$arm:0")
    done
elif has_kind q2; then
    for arm in T0 T0t T2; do
        units+=("q2:$arm:0")
    done
fi
count=${#units[@]}

raw="$out/raw.tsv"
printf 'block\tpos\tkind\tarm\tw\tcpus\tdrivers\tcontexts\tops\tns\tops_per_s\tcheck\n' > "$raw"
correct="$out/correct.tsv"
printf 'block\tarm\tattempts\tstatus\tverdict\texpect\tline\n' > "$correct"
failed=0

run_cell() {
    local block=$1 pos=$2 unit=$3 list=$4 drivers=$5
    local kind arm w bin threads before after status line seen name
    IFS=: read -r kind arm w <<< "$unit"
    name="b$block-$drivers-$pos-$arm-$w"
    before=$(cpu_times "$list")
    status=0
    case $arm in
        S0 | S1 | S2 | S3 | S4) bin="$out/bin/q1" ;;
        S0t) bin="$out/bin/q1_twin" ;;
        S1t) bin="$out/bin/q1_twin" ;;
        T0) bin="$out/bin/t0" ;;
        T0t) bin="$out/bin/t0_twin" ;;
        T2) bin="$out/bin/t2" ;;
        T1) bin="$out/bin/t1" ;;
        T1t) bin="$out/bin/t1_twin" ;;
        T1off) bin="$out/bin/t1off" ;;
        TP) bin="$out/bin/tp" ;;
        TC) bin="$out/bin/tc" ;;
        *) echo "unknown arm $arm" >&2; exit 2 ;;
    esac
    if [ "$kind" = q1 ]; then
        timeout $((MILLIS / 1000 + 120)) taskset -c "$list" "$bin" "${arm%t}" "$MILLIS" "$(steps_of "$w")" \
            > "$out/runs/$name.out" 2> "$out/runs/$name.err" || status=$?
    else
        WF_DRIVERS=$drivers WF_SCHED_REPORT=2 timeout $((MILLIS / 1000 + 120)) \
            taskset -c "$list" "$bin" "$CONTEXTS" "$MILLIS" \
            > "$out/runs/$name.out" 2> "$out/runs/$name.err" || status=$?
    fi
    after=$(cpu_times "$list")
    line=$(cat "$out/runs/$name.out")
    echo "$name: $line (status $status)"
    seen=$(echo "$line" | sed -n 's/.* drivers=\([0-9]*\) .*/\1/p')
    if [ "$status" -ne 0 ] || [ "$seen" != "$drivers" ]; then
        echo "$name: status $status, drivers $seen where $drivers were set" >&2
        failed=1
        return 0
    fi
    echo "$line" | awk -v block="$block" -v pos="$pos" -v kind="$kind" -v arm="$arm" -v w="$w" -v list="$list" -v drivers="$drivers" '
        {
            for (i = 1; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
            rate = v["ops_per_s"]
            if (rate == "?") rate = sprintf("%.0f", v["ops"] / (v["ns"] / 1e9))
            printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", block, pos, kind, arm, w, list, drivers, v["contexts"], v["ops"], v["ns"], rate, v["check"]
        }' >> "$raw"
}

run_correctness() {
    local block=$1 arm=$2 bin=$3 attempts=$4
    local drivers n status=0 line verdict expect
    drivers=$(expand_list "$CHECK_LIST" | wc -l | tr -d ' ')
    for n in $(seq 1 "$attempts"); do
        status=0
        WF_DRIVERS=$drivers WF_SCHED_REPORT=2 timeout 600 taskset -c "$CHECK_LIST" "$bin" "$CONTEXTS" "$CHECK_MILLIS" \
            > "$out/runs/b$block-check-$arm-$n.out" 2> "$out/runs/b$block-check-$arm-$n.err" || status=$?
        line=$(cat "$out/runs/b$block-check-$arm-$n.out")
        echo "check b$block $arm attempt $n: $line (status $status)"
        [ "$arm" = c_tbad ] || break
        [ $((status & 4)) -ne 4 ] || break
    done
    verdict=$(echo "$line" | sed -n 's/.* verdict=\([0-9]*\).*/\1/p')
    if [ "$arm" != c_tbad ]; then
        if [ "$status" = 0 ] && [ "$verdict" = 0 ]; then expect=ok; else expect=BAD; fi
    else
        if [ -n "$verdict" ] && [ $((verdict & 5)) -eq 5 ] && [ "$status" = "$verdict" ]; then expect=ok; else expect=BAD; fi
    fi
    [ "$expect" = ok ] || failed=1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$block" "$arm" "$n" "$status" "${verdict:-none}" "$expect" "$line" >> "$correct"
}

for block in $(seq 0 $((BLOCKS - 1))); do
    idle_check "$block"
    order=()
    for pos in $(seq 0 $((count - 1))); do
        order+=("${units[$(((pos + block) % count))]}")
    done
    for list in $CPU_LISTS; do
        drivers=$(expand_list "$list" | wc -l | tr -d ' ')
        pos=0
        for unit in "${order[@]}"; do
            pos=$((pos + 1))
            run_cell "$block" "$pos" "$unit" "$list" "$drivers"
        done
    done
    if has_kind q2 || has_kind q3; then
        if has_kind q3; then
            run_correctness "$block" c_t1 "$out/bin/c_t1" 1
        fi
        run_correctness "$block" c_t2 "$out/bin/c_t2" 1
        run_correctness "$block" c_tbad "$out/bin/c_tbad" "$CHECK_ATTEMPTS"
    fi
done

elapsed=$(($(date +%s) - started_at))
echo "cells and checks took $elapsed s"
if [ "$MODE" = pilot ]; then
    {
        echo "pilot: one block at the list $CPU_LISTS took $elapsed s including calibration"
        lists=4
        if has_kind q3; then lists=5; fi
        echo "projection of the main run ($BLOCKS pilot block scaled to 6 blocks, $lists lists): about $((elapsed * 6 * lists)) s"
    } | tee "$out/pilot.txt"
fi
cat "$raw" "$correct"
exit "$failed"
