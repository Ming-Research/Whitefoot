#!/usr/bin/env bash
# The stages of .github/workflows/interpreter-compare.yml (README.md beside
# this file), in one place so that a temporary workflow on a work branch can
# run the same commands the manual workflow runs.
#
#   run.sh STAGE
#
# Stages: inputs, perf-setup, tools, selftest, build, time, sample, summary.
# The inputs are environment variables; OUT, CANDIDATE and RUNNER_KIND (github
# or 14900k) are required, the others default as the workflow's inputs do:
#   BASE            base commit or branch; empty means the candidate's merge
#                   base with main
#   CPU             the CPU every timed process is pinned to; empty means 2 on
#                   the 14900K and 1 on GitHub
#   MODE            time (interleaved timing) or sample (per-instruction CPU
#                   samples); default time
#   ITERATIONS      CoreMark iterations; default 2000
#   ROUNDS          timed rounds; default 10; fewer than 6 is a trial that
#                   judges nothing
#   FLOOR           noise floor, as `coremark=spread`, as interp.py reduce
#   RESIDUAL_LIMIT  seconds; a workload whose median wall time minus user+sys
#                   CPU time is above it is invalid data (default: printed
#                   only)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
compare=$here/../compiler-compare
interp=$compare/interp.py
samples=$here/handler_samples.py
stage=${1:?usage: run.sh STAGE}
: "${OUT:?OUT is the output directory}"
mode=${MODE:-time}
iterations=${ITERATIONS:-2000}
rounds=${ROUNDS:-10}
cpu=${CPU:-}
if [ -z "$cpu" ]; then
    if [ "${RUNNER_KIND:-}" = 14900k ]; then cpu=2; else cpu=1; fi
fi
wfc=$OUT/candidate/compiler/target/gate/whitefootc
generator=research/experiments/match-dispatch/wasm
silverfir_revision=5f248e4420102356be7edadbdb4db2ae105d3270
coremark_blob=70b4d2165e7fbbd797e47b43032fc06302c1774f

# check_inputs: the workflow's inputs, each refused with its reason before any
# build. A mode other than time or sample, an iteration count or round count
# that is not a positive integer, a floor that is not coremark=spread and a
# limit that is not a number would each end in a late failure or in a run that
# measures something else.
check_inputs() {
    local mode=$1 iterations=$2 rounds=$3 floor=$4 limit=$5
    case "$mode" in time | sample) ;; *)
        echo "mode '$mode': the modes are time and sample" >&2
        return 1
        ;;
    esac
    case "$iterations" in '' | *[!0-9]* | 0*)
        echo "iterations '$iterations' is not a positive integer" >&2
        return 1
        ;;
    esac
    case "$rounds" in '' | *[!0-9]* | 0*)
        echo "rounds '$rounds' is not a positive integer" >&2
        return 1
        ;;
    esac
    if [ -n "$floor" ] && ! [[ "$floor" =~ ^coremark=[0-9]+(\.[0-9]+)?$ ]]; then
        echo "floor '$floor' is not coremark=SPREAD" >&2
        return 1
    fi
    if [ -n "$limit" ] && ! [[ "$limit" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
        echo "residual limit '$limit' is not a number of seconds" >&2
        return 1
    fi
}

case "$stage" in
inputs)
    check_inputs "$mode" "$iterations" "$rounds" "${FLOOR:-}" "${RESIDUAL_LIMIT:-}"
    ;;

perf-setup)
    # A hosted runner has no perf for its kernel and keeps profiling closed.
    sudo apt-get update -qq
    sudo apt-get install -y -qq "linux-tools-$(uname -r)" || sudo apt-get install -y -qq linux-tools-generic
    sudo sysctl -w kernel.perf_event_paranoid=1
    perf --version
    ;;

tools)
    if [ -d "$HOME/.cargo/bin" ]; then
        export PATH="$HOME/.cargo/bin:$PATH"
        if [ -n "${GITHUB_PATH:-}" ]; then echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"; fi
    fi
    wanted=(/usr/bin/clang ld.lld cargo make taskset python3 git perl nm)
    if [ "$mode" = sample ]; then wanted+=(perf); fi
    missing=
    for tool in "${wanted[@]}"; do
        command -v "$tool" >/dev/null || missing="$missing $tool"
    done
    if [ -n "$missing" ]; then
        echo "missing:$missing"
        exit 1
    fi
    /usr/bin/clang --version
    nproc
    lscpu
    if [ "${RUNNER_KIND:-}" = 14900k ]; then
        echo "load: $(cat /proc/loadavg)"
        ps -eo pid,pcpu,args --sort=-pcpu | sed -n 1,8p
    fi
    if [ "$mode" = sample ]; then
        perf --version
        echo "kernel.perf_event_paranoid=$(cat /proc/sys/kernel/perf_event_paranoid)"
    fi
    ;;

selftest)
    python3 -B "$interp" self-test
    python3 -B "$samples" self-test
    # check_inputs takes what the workflow's inputs may hold and refuses the rest.
    failed=0
    check_inputs time 2000 10 "" "" 2>/dev/null || { echo "FAIL inputs: the defaults are refused"; failed=1; }
    check_inputs sample 20 2 coremark=0.012 0.05 2>/dev/null || { echo "FAIL inputs: a trial is refused"; failed=1; }
    for case in "wall 2000 10 '' ''" "time 0 10 '' ''" "time 20x 10 '' ''" "time 2000 0 '' ''" "time 2000 10 fib=0.01 ''" \
        "time 2000 10 coremark=x ''" "time 2000 10 '' abc"; do
        eval "set -- $case"
        if check_inputs "$@" 2>/dev/null; then echo "FAIL inputs: '$case' is accepted"; failed=1; fi
    done
    if [ "$failed" != 0 ]; then exit 1; fi
    echo "inputs check: passed"
    ;;

build)
    : "${CANDIDATE:?CANDIDATE is the candidate commit or branch}"
    rm -rf "$OUT"
    mkdir -p "$OUT/src" "$OUT/inputs"
    git fetch --no-tags origin '+refs/heads/*:refs/remotes/origin/*'
    resolve() {
        git rev-parse --verify -q "origin/$1^{commit}" || git rev-parse --verify "$1^{commit}"
    }
    candidate=$(resolve "$CANDIDATE")
    if [ -n "${BASE:-}" ]; then
        base=$(resolve "$BASE")
    else
        base=$(git merge-base origin/main "$candidate")
    fi
    # Both sources of the interpreter come from their own commit; a commit
    # without the generator cannot be compared, and is named.
    for pair in "base $base" "candidate $candidate"; do
        set -- $pair
        if ! git cat-file -e "$2:$generator/gen.py" 2>/dev/null; then
            echo "the $1 commit $2 has no $generator/gen.py" >&2
            exit 1
        fi
        mkdir -p "$OUT/src/$1"
        git archive "$2" "$generator" | tar -x -C "$OUT/src/$1"
    done
    # One compiler builds both: the candidate's.
    git worktree prune
    git worktree add --detach "$OUT/candidate" "$candidate"
    cargo fetch --locked --manifest-path "$OUT/candidate/compiler/Cargo.toml"
    make -C "$OUT/candidate/compiler" build
    {
        echo "instrument $(git rev-parse HEAD)"
        echo "candidate $candidate"
        echo "base $base"
        if [ "$candidate" = "$base" ]; then
            echo "null run: the candidate and the base are the same commit"
        fi
        echo "compiler $candidate (builds both sources)"
        echo "clang $(/usr/bin/clang --version | head -1)"
        echo "cpu model $(lscpu | sed -n 's/^Model name: *//p')"
        echo "mode $mode"
    } > "$OUT/manifest.txt"
    git clone --quiet https://github.com/mbbill/Silverfir-nano "$OUT/silverfir"
    git -C "$OUT/silverfir" checkout --quiet "$silverfir_revision"
    module=benchmarks/wasi/coremark/coremark.wasm
    test "$(git -C "$OUT/silverfir" rev-parse "HEAD:$module")" = "$coremark_blob"
    test "$(git hash-object "$OUT/silverfir/$module")" = "$coremark_blob"
    echo "silverfir $(git -C "$OUT/silverfir" rev-parse HEAD)" >> "$OUT/manifest.txt"
    python3 -B "$interp" build --base-wfc "$wfc" --candidate-wfc "$wfc" --workloads coremark \
        --silverfir "$OUT/silverfir" --iterations "$iterations" \
        --base-generator "$OUT/src/base/$generator/gen.py" \
        --candidate-generator "$OUT/src/candidate/$generator/gen.py" --out "$OUT"
    ;;

time)
    trial=
    if [ "$rounds" -lt 6 ]; then trial=--trial; fi
    echo "time cpu $cpu rounds $rounds ${trial:-judged}" >> "$OUT/manifest.txt"
    status=0
    python3 -B "$interp" time --out "$OUT" --cpu "$cpu" --workloads coremark \
        --min-rounds "$rounds" --max-rounds "$rounds" $trial || status=$?
    if [ "$status" = 0 ]; then
        python3 -B "$interp" reduce --out "$OUT" --workloads coremark --floor "${FLOOR:-}" \
            ${RESIDUAL_LIMIT:+--residual-limit "$RESIDUAL_LIMIT"} || status=$?
    fi
    exit "$status"
    ;;

sample)
    echo "sample cpu $cpu" >> "$OUT/manifest.txt"
    python3 -B "$samples" sample --out "$OUT" --cpu "$cpu"
    ;;

summary)
    {
        echo "## interpreter-compare: $mode (${RUNNER_KIND:-?})"
        echo
        echo '```text'
        if [ "$mode" = sample ]; then
            cat "$OUT/manifest.txt" "$OUT/sample-summary.txt" 2>/dev/null || echo "no summary was written (the run failed)"
        else
            cat "$OUT/summary.txt" 2>/dev/null || echo "no summary was written (the run failed)"
        fi
        echo '```'
    } >> "${GITHUB_STEP_SUMMARY:-/dev/stdout}"
    ;;

*)
    echo "unknown stage $stage" >&2
    exit 2
    ;;
esac
