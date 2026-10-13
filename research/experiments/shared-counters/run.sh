#!/usr/bin/env bash
# Builds and runs the shared-counter arms (README.md beside this file), ported
# from the runtime-contention harness of Firn-wf (README.md, "Source").
#
#   run.sh --out DIR [--build-only | --skip-build]
#
# Environment:
#   WHITEFOOTC  the Whitefoot compiler to build the arms with (required unless
#               --skip-build)
#   W4n256 and W4nmax select diagnostic compiler copies with atomic-boundary
#   yield thresholds 256 and 1048576. They compile W4's unchanged source only;
#   W4 and its twin keep the unpatched threshold 64.
#   PILOT       1 runs one block at the 8-P-core list only; otherwise the main
#               run (default: main)
#   BLOCKS      blocks of the main run (default 12, registered)
#   CONTEXTS    contexts or threads each arm starts (default 64)
#   MILLIS      milliseconds each cell lasts (default 5000)
#   ARMS        arms to build and run, in rotation order (default
#               "W0 W1 W2 W2t W2r W6 W3 W3rd W4 W4t W4r W4rd M1 M2 M3 M4 A1 W5 W5p"); the C model's run
#               (DESIGN.md section 8.6) is ARMS="W5p W5pN W7 W7t W7rd", which
#               builds with clang alone and needs no WHITEFOOTC
#   CPU_LISTS   taskset CPU lists (default "0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15")
#   ORDER       rotate: each block starts the arm order one position later
#               than the block before (section 8.3); alternate: the
#               same rotation, run backwards in odd blocks, so that each pair
#               of arms runs in both orders equally often (default, section 8.6)
#   READER_CPU  the CPU W7rd's reader thread is pinned to, outside every CPU
#               list (default 16, the first E-core of the i9-14900K)
#   CLANG       the C compiler (default /usr/bin/clang)
#
# W5pN, W7, W7t and W7rd start one thread per CPU of the list; every other
# arm starts CONTEXTS contexts or threads.
#
# The build writes DIR/bin/ and DIR/host.txt. The run appends one row per cell
# to DIR/raw.tsv:
#   block pos arm cpus drivers contexts ops ns ops_per_s check reads read_ns
#   read_max_ns user_frac sys_frac read_med_ns read_p99_ns read_hist_ns
# where user_frac and sys_frac are the shares of the listed CPUs' time that
# /proc/stat counts as user (user and nice) and system (system, irq and
# softirq) between just before and just after the cell, and a field an arm
# does not print is 0, except an absent histogram is blank and W4rd's absent
# scalar percentiles are blank (its histogram supplies them in the reducer).
# Each cell's standard output and error are kept in
# DIR/runs/. A cell that fails, or reports a driver count other than the
# list's, is reported on standard error, leaves no row, and makes the script
# exit 1 after the remaining cells ran.
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
BLOCKS=${BLOCKS:-12}
CONTEXTS=${CONTEXTS:-64}
MILLIS=${MILLIS:-5000}
ARMS=${ARMS:-"W0 W1 W2 W2t W2r W6 W3 W3rd W4 W4t W4r W4rd M1 M2 M3 M4 A1 W5 W5p"}
CPU_LISTS=${CPU_LISTS:-"0,2 0,2,4,6 0,2,4,6,8,10,12,14 0-15"}
ORDER=${ORDER:-alternate}
READER_CPU=${READER_CPU:-16}
case $ORDER in
    rotate | alternate) ;;
    *) echo "ORDER must be rotate or alternate" >&2; exit 2 ;;
esac
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
        W4) echo w_sharded ;;
        W4t) echo w_sharded_twin ;;
        W4n256) echo w_sharded_n256 ;;
        W4nmax) echo w_sharded_nmax ;;
        W4r) echo w_sharded_param ;;
        W4rd) echo w_sharded_reader ;;
        M1) echo w_max_exclusive ;;
        M2) echo w_max_read ;;
        M3) echo w_max_cache ;;
        M4) echo w_max_sharded ;;
        A1) echo w_aof ;;
        W5) echo w5 ;;
        W5p | W5pN) echo w5p ;;
        W7 | W7rd) echo w7 ;;
        W7t) echo w7_twin ;;
        *) echo "unknown arm $1" >&2; exit 2 ;;
    esac
}

# Twins have distinct output paths and repeat the build of the same entry.
entry_of() {
    case $1 in
        W2t) echo w_shared ;;
        W4t | W4n256 | W4nmax) echo w_sharded ;;
        *) binary_of "$1" ;;
    esac
}

is_whitefoot() {
    case $1 in
        W5 | W5p | W5pN | W7 | W7t | W7rd) return 1 ;;
        *) return 0 ;;
    esac
}

# The threads or contexts an arm starts on a list of DRIVERS CPUs: the C
# model and its comparison W5pN one thread per CPU, as the planned runtime
# has one shard writer per driver (DESIGN.md section 8.6); the rest CONTEXTS.
threads_of() {
    case $1 in
        W5pN | W7 | W7t | W7rd) echo "$2" ;;
        *) echo "$CONTEXTS" ;;
    esac
}

# Arguments after the thread count and the milliseconds.
extra_of() {
    case $1 in
        W7rd) echo "$READER_CPU" ;;
        *) echo ;;
    esac
}

# Whether ARMS names arm $1.
has_arm() {
    case " $ARMS " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

for arm in $ARMS; do binary_of "$arm" > /dev/null; done

# Diagnostic variants must have their unchanged-source baseline in the run.
if has_arm W4n256 || has_arm W4nmax; then
    has_arm W4 || { echo "yield variants require W4 in ARMS" >&2; exit 2; }
fi

# Compiler selection also applies to retained LLVM, so attribution never uses
# the baseline's code in place of a variant's code.
compiler_of() {
    case $1 in
        W4n256) echo "$variant_root/n256/compiler/target/gate/whitefootc" ;;
        W4nmax) echo "$variant_root/nmax/compiler/target/gate/whitefootc" ;;
        *) echo "$WHITEFOOTC" ;;
    esac
}

if [ "$build" = 1 ]; then
    clang=${CLANG:-/usr/bin/clang}
    variant_record="$out/yield-variants.txt"
    : > "$variant_record"
    if has_arm W4n256 || has_arm W4nmax; then
        : "${WHITEFOOTC:?WHITEFOOTC names the unpatched compiler}"
        checkout=$(git -C "$here" rev-parse --show-toplevel)
        # WHITEFOOTC was built from this checkout; fail rather than comparing
        # it against variant copies from a different tracked source revision.
        git -C "$checkout" diff --quiet HEAD -- compiler lib spec tests || {
            echo "variant builds require unchanged tracked compiler inputs" >&2; exit 1;
        }
        variant_root=$(mktemp -d "${TMPDIR:-/tmp}/shared-counters-yields.XXXXXX")
        trap 'rm -rf "$variant_root"' EXIT
        git -C "$checkout" archive --format=tar --output="$variant_root/source.tar" HEAD
        for arm in W4n256 W4nmax; do
            has_arm "$arm" || continue
            case $arm in
                W4n256) threshold=256; copy="$variant_root/n256" ;;
                W4nmax) threshold=1048576; copy="$variant_root/nmax" ;;
            esac
            mkdir "$copy"
            tar -xf "$variant_root/source.tar" -C "$copy"
            python3 - "$copy" "$threshold" <<'PY'
import sys
from pathlib import Path

def replace_once(text, old, new):
    count = text.count(old)
    if count != 1:
        raise ValueError(f"expected exactly one {old!r}, found {count}")
    return text.replace(old, new, 1)

# The patch guard must reject a missing or duplicated site, rather than silently
# making a mislabeled diagnostic. No compiler is run by these string controls.
for sample in ("absent", "site site"):
    try:
        replace_once(sample, "site", "changed")
    except ValueError:
        pass
    else:
        raise AssertionError("threshold patch guard accepted a wrong site count")
assert replace_once("site", "site", "changed") == "changed"

root = Path(sys.argv[1])
threshold = int(sys.argv[2])
assert threshold in (256, 1048576)
patches = {
    "compiler/src/backend/emitter/shared.rs": [
        ("icmp uge i32 %{prefix}.next, 64", f"icmp uge i32 %{{prefix}}.next, {threshold}")],
    "compiler/src/backend/completion/bridge.c": [
        ("#define WF_CONTEXT_YIELD_PASSES 64u", f"#define WF_CONTEXT_YIELD_PASSES {threshold}u"),
        ("_Static_assert(WF_CONTEXT_YIELD_PASSES == 64u,", f"_Static_assert(WF_CONTEXT_YIELD_PASSES == {threshold}u,"),
        ('"inline atomic boundary compares the pass count with 64"',
         f'"inline atomic boundary compares the pass count with {threshold}"')],
}
# Check every site before writing either file, so the patch is all-or-nothing.
changed = {}
for relative, replacements in patches.items():
    path = root / relative
    text = path.read_text()
    for old, new in replacements:
        text = replace_once(text, old, new)
    changed[path] = text
for path, text in changed.items():
    path.write_text(text)
print(f"yield threshold {threshold}: emitter, bridge define, assertion and message patched exactly once")
PY
            # This runs inside the workflow's existing images lock and budget.
            # Separate targets prevent either compiler from reusing the other's
            # runtime/emitter build; the copies are deleted at script exit.
            CARGO_TARGET_DIR="$copy/compiler/target" cargo build \
                --manifest-path "$copy/compiler/Cargo.toml" --profile gate \
                --bin whitefootc --locked --offline
            {
                echo "$arm: threshold=$threshold; checkout=$(git -C "$checkout" rev-parse HEAD)"
                echo "$arm compiler SHA-256: $(sha256sum "$(compiler_of "$arm")" | awk '{print $1}')"
            } >> "$variant_record"
        done
        echo "W4nmax is diagnostic: 16384x lower nominal yield frequency than W4, not zero yields; actual switch counts are not instrumented." >> "$variant_record"
    else
        echo "Yield diagnostic variants: not built" >> "$variant_record"
    fi
    built=" "
    for arm in $ARMS; do
        bin=$(binary_of "$arm")
        case $built in *" $bin "*) continue ;; esac
        built="$built$bin "
        case $arm in
            W5) "$clang" -O2 -pthread -o "$out/bin/$bin" "$here/w5.c" ;;
            W5p | W5pN) "$clang" -O2 -pthread -DPADDED=1 -o "$out/bin/$bin" "$here/w5.c" ;;
            # W7t is W7's source built a second time into its own binary.
            W7 | W7t | W7rd) "$clang" -O2 -pthread -o "$out/bin/$bin" "$here/w7.c" ;;
            *)
                : "${WHITEFOOTC:?WHITEFOOTC names the compiler to build the Whitefoot arms with}"
                "$(compiler_of "$arm")" --full-lto --graph "$here/modules.wfg" --entry "$(entry_of "$arm")" -o "$out/bin/$bin" ;;
        esac
    done

    if has_arm W4n256 || has_arm W4nmax; then
        "$WHITEFOOTC" --full-lto --graph "$here/modules.wfg" --entry w_sharded \
            -o "$out/bin/w_sharded_unpatched_control"
        cmp -s "$out/bin/w_sharded" "$out/bin/w_sharded_unpatched_control" || {
            echo "W4 differs from its unpatched same-compiler rebuild" >&2; exit 1;
        }
        echo "W4 is byte-identical to the unpatched same-compiler rebuild: $(sha256sum "$out/bin/w_sharded" | awk '{print $1}')" >> "$variant_record"
        for arm in W4n256 W4nmax; do
            has_arm "$arm" || continue
            bin=$(binary_of "$arm")
            comparison=0
            cmp -s "$out/bin/w_sharded" "$out/bin/$bin" || comparison=$?
            [ "$comparison" = 1 ] || {
                echo "$arm must differ from W4 (cmp status $comparison; 0 means identical, 2 means a comparison error)" >&2; exit 1;
            }
            echo "$arm differs from W4: $(sha256sum "$out/bin/$bin" | awk '{print $1}')" >> "$variant_record"
        done
    fi

    # Count static call sites, not runtime executions. Retain calls are direct;
    # release calls occur inside compiler-generated wf.drop.t.* helpers. Emit
    # each selected arm, including each twin, to its own retained LLVM file.
    # Function-scoped counts distinguish hot workers from spawn-time sharing
    # and cleanup; drop-helper call sites also include scope-exit cleanup.
    counts="$out/handle-counts.txt"
    : > "$counts"
    for arm in $ARMS; do
        is_whitefoot "$arm" || continue
        bin=$(binary_of "$arm")
        "$(compiler_of "$arm")" --emit-llvm --graph "$here/modules.wfg" --entry "$(entry_of "$arm")" -o "$out/bin/$bin.ll"
        awk -v arm="$arm" -v llvm="bin/$bin.ll" '
            /^define / {
                name = $0; sub(/^.*@/, "", name); sub(/\(.*/, "", name)
                functions[++n] = name
            }
            /call void @wf__shared_share\(/ { share++; shares[name]++ }
            /call i32 @wf__shared_release\(/ { release++; releases[name]++ }
            /call void @wf[.]drop[.]t[.]/ { drops[name]++ }
            END {
                printf "LLVM handle-count call sites %s (%s): wf__shared_share=%d wf__shared_release=%d\n", arm, llvm, share, release
                for (i = 1; i <= n; i++) {
                    name = functions[i]
                    printf "  %s: wf__shared_share=%d wf__shared_release=%d wf.drop.t.*=%d\n", name, shares[name], releases[name], drops[name]
                }
            }' "$out/bin/$bin.ll" >> "$counts"
    done
    if [ ! -s "$counts" ]; then
        echo "LLVM handle-count call sites: not recorded (no Whitefoot arm selected)" >> "$counts"
    else
        echo "Skip-confirm: W4 worker should have zero retain calls. W2 also keeps its value handle live under the current general skip; W2 is not a retaining-statement falsifier. Whole-program release counts include teardown." >> "$counts"
    fi

    twin_record="W2 and its twin: not built"
    if has_arm W2 && has_arm W2t; then
        twin_record="W2 and its twin: $(sha256sum "$out/bin/w_shared" "$out/bin/w_shared_twin" | awk '{print $1}' | tr '\n' ' ')"
    fi
    model_twin_record="W7 and its twin: not built"
    if has_arm W7t && { has_arm W7 || has_arm W7rd; }; then
        model_twin_record="W7 and its twin: $(sha256sum "$out/bin/w7" "$out/bin/w7_twin" | awk '{print $1}' | tr '\n' ' ')"
    fi
    shard_twin_record="W4 and its twin: not built"
    if has_arm W4 && has_arm W4t; then
        shard_twin_record="W4 and its twin: $(sha256sum "$out/bin/w_sharded" "$out/bin/w_sharded_twin" | awk '{print $1}' | tr '\n' ' ')"
    fi
    {
        echo "kernel: $(uname -srvmo)"
        echo "cpus: $(nproc)"
        echo "model: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"
        echo "load: $(cat /proc/loadavg)"
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unavailable)"
        echo "no_turbo: $(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || echo unavailable)"
        echo "compiler release: ${WF_RELEASE:-unnamed}"
        if [ -n "${WHITEFOOTC:-}" ]; then
            echo "compiler SHA-256: $(sha256sum "$WHITEFOOTC" | awk '{print $1}')"
        fi
        echo "clang: $("$clang" --version | head -n 1)"
        cat "$counts"
        cat "$variant_record"
        echo "$twin_record"
        echo "$model_twin_record"
        echo "$shard_twin_record"
        echo "contexts: $CONTEXTS  millis: $MILLIS  blocks: $BLOCKS  pilot: $PILOT  order: $ORDER"
        echo "arms: $ARMS"
        echo "cpu lists: $CPU_LISTS"
        if has_arm W7rd; then
            echo "W7rd reader cpu: $READER_CPU (its core and clock are in the cpu map below)"
        fi
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
printf 'block\tpos\tarm\tcpus\tdrivers\tcontexts\tops\tns\tops_per_s\tcheck\treads\tread_ns\tread_max_ns\tuser_frac\tsys_frac\tread_med_ns\tread_p99_ns\tread_hist_ns\n' > "$raw"
failed=0
read -r -a arms <<< "$ARMS"
count=${#arms[@]}
# Each block starts its arm order one position later than the block before;
# with ORDER=alternate, odd blocks run that order backwards.
for block in $(seq 0 $((BLOCKS - 1))); do
    order=
    for pos in $(seq 0 $((count - 1))); do
        next=${arms[$(((pos + block) % count))]}
        if [ "$ORDER" = alternate ] && [ $((block % 2)) = 1 ]; then
            order="$next $order"
        else
            order="$order $next"
        fi
    done
    for list in $CPU_LISTS; do
        drivers=$(expand_list "$list" | wc -l | tr -d ' ')
        pos=0
        for arm in $order; do
            pos=$((pos + 1))
            bin=$(binary_of "$arm")
            threads=$(threads_of "$arm" "$drivers")
            extra=$(extra_of "$arm")
            name="b$block-$drivers-$pos-$arm"
            before=$(cpu_times "$list")
            status=0
            # $extra is empty or one number, so it is left unquoted.
            # shellcheck disable=SC2086
            WF_DRIVERS=$drivers WF_SCHED_REPORT=2 timeout $((MILLIS / 1000 + 120)) \
                taskset -c "$list" "$out/bin/$bin" "$threads" "$MILLIS" $extra \
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
                    if (!("read_med_ns" in v)) v["read_med_ns"] = arm == "W4rd" ? "" : 0
                    if (!("read_p99_ns" in v)) v["read_p99_ns"] = arm == "W4rd" ? "" : 0
                    split(before, b, " "); split(after, a, " ")
                    total = a[3] - b[3]
                    rate = v["ops_per_s"]
                    if (rate == "?") rate = sprintf("%.0f", v["ops"] / (v["ns"] / 1e9))
                    user = 0; sys = 0
                    if (total > 0) { user = (a[1] - b[1]) / total; sys = (a[2] - b[2]) / total }
                    printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%.4f\t%.4f\t%s\t%s\t%s\n", block, pos, arm, list,
                        drivers, v["contexts"], v["ops"], v["ns"], rate, v["check"], v["reads"],
                        v["read_ns"], v["read_max_ns"], user, sys, v["read_med_ns"], v["read_p99_ns"], v["read_hist_ns"]
                }' >> "$raw"
        done
    done
done
cat "$raw"
exit "$failed"
