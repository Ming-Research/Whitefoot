#!/usr/bin/env bash
# The stages of .github/workflows/compiler-compare.yml (README.md beside this
# file), in one place so that a temporary workflow on a work branch can run
# the same commands the manual workflow runs.
#
#   run.sh STAGE
#
# Stages: tools, selftest, arms, hashmap, interp, summary; the workflow
# decides which groups run. The inputs are
# environment variables; OUT, CANDIDATE and RUNNER_KIND (github or 14900k)
# are required, the others default as the workflow's inputs do:
#   BASE            base commit or branch; empty means the merge base with main
#   CPU             the CPU every timed process is pinned to; empty means 2 on
#                   the 14900K and 1 on GitHub
#   SIZES           map sizes of the hash-map cells (default 1000,10000,1000000)
#   MIN_ROUNDS, MAX_ROUNDS   hash-map rounds (default 10 and 40)
#   FLOOR           hash-map noise floor, as compare.py reduce --floor
#   HALO            commit of Halo-wf whose interpreter is built
#   INTERP_MIN_ROUNDS, INTERP_MAX_ROUNDS   interpreter rounds (default 10, 30)
#   INTERP_FLOOR    interpreter noise floor, as interp.py reduce --floor
#   PERF            1 to also run perf stat (14900K only; default 1)
#   PERF_ROUNDS     perf stat rounds (default 5)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
compare=$here/../hash-map-compare/compare.py
interp=$here/interp.py
stage=${1:?usage: run.sh STAGE}
: "${OUT:?OUT is the output directory}"
arms=$OUT/arms
wfc_base=$arms/base/compiler/target/gate/whitefootc
wfc_candidate=$arms/candidate/compiler/target/gate/whitefootc
cpu=${CPU:-}
if [ -z "$cpu" ]; then
    if [ "${RUNNER_KIND:-}" = 14900k ]; then cpu=2; else cpu=1; fi
fi
perf=${PERF:-1}
if [ "${RUNNER_KIND:-}" != 14900k ]; then perf=0; fi

# Pinned inputs of the interpreters group. The Halo commit is the input
# HALO; Silverfir-nano carries the CoreMark module, checked by its git blob.
silverfir_revision=5f248e4420102356be7edadbdb4db2ae105d3270
coremark_blob=70b4d2165e7fbbd797e47b43032fc06302c1774f

case "$stage" in
tools)
    # A runner service starts without the login shell's PATH, where rustup
    # puts cargo; the stage stops on the first missing tool and names it.
    if [ -d "$HOME/.cargo/bin" ]; then
        export PATH="$HOME/.cargo/bin:$PATH"
        if [ -n "${GITHUB_PATH:-}" ]; then echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"; fi
    fi
    wanted=(/usr/bin/clang ld.lld cargo make taskset timeout python3 git perl)
    if [ "$perf" = 1 ]; then wanted+=(perf); fi
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
        ps -eo pid,pcpu,args --sort=-pcpu | head -8
    fi
    if [ "${PERF:-1}" = 1 ] && [ "${RUNNER_KIND:-}" != 14900k ]; then
        echo "perf stat is skipped: it runs on the 14900K only"
    fi
    if [ "$perf" = 1 ]; then
        perf --version
        echo "kernel.perf_event_paranoid=$(cat /proc/sys/kernel/perf_event_paranoid)"
    fi
    ;;

selftest)
    python3 -B "$compare" self-test
    python3 -B "$interp" self-test
    ;;

arms)
    : "${CANDIDATE:?CANDIDATE is the candidate commit or branch}"
    rm -rf "$OUT"
    mkdir -p "$OUT"
    bash "$here/arms.sh" "$CANDIDATE" "${BASE:-}" "$OUT"
    ;;

hashmap)
    dir=$OUT/hashmap
    rm -rf "$dir"
    mkdir -p "$dir"
    cp "$OUT/manifest.txt" "$dir/manifest.txt"
    # The two arms are whole trees of two commits: each observed image links
    # the runtime of its own tree, and equal images are allowed since the
    # same commit twice is the null run.
    python3 -B "$compare" build --base-wfc "$wfc_base" --candidate-wfc "$wfc_candidate" \
        --runtime-root "$arms/candidate" --base-runtime-root "$arms/base" \
        --allow-identical-images --out "$dir"
    python3 -B "$compare" measure --out "$dir" --cpu "$cpu" --sizes "${SIZES:-1000,10000,1000000}" \
        --min-rounds "${MIN_ROUNDS:-10}" --max-rounds "${MAX_ROUNDS:-40}"
    python3 -B "$compare" reduce --out "$dir" --floor "${FLOOR:-}"
    ;;

interp)
    dir=$OUT/interp
    rm -rf "$dir"
    mkdir -p "$dir" "$OUT/inputs"
    cp "$OUT/manifest.txt" "$dir/manifest.txt"
    : "${HALO:?HALO is the commit of Halo-wf to build}"
    rm -rf "$OUT/inputs/halo" "$OUT/inputs/silverfir"
    git clone --quiet https://github.com/Ming-Research/Halo-wf.git "$OUT/inputs/halo"
    git -C "$OUT/inputs/halo" checkout --quiet "$HALO"
    git clone --quiet https://github.com/mbbill/Silverfir-nano "$OUT/inputs/silverfir"
    git -C "$OUT/inputs/silverfir" checkout --quiet "$silverfir_revision"
    module=benchmarks/wasi/coremark/coremark.wasm
    test "$(git -C "$OUT/inputs/silverfir" rev-parse "HEAD:$module")" = "$coremark_blob"
    test "$(git hash-object "$OUT/inputs/silverfir/$module")" = "$coremark_blob"
    {
        echo "halo $(git -C "$OUT/inputs/halo" rev-parse HEAD)"
        echo "silverfir $(git -C "$OUT/inputs/silverfir" rev-parse HEAD)"
    } >> "$dir/manifest.txt"
    python3 -B "$interp" build --base-wfc "$wfc_base" --candidate-wfc "$wfc_candidate" \
        --halo "$OUT/inputs/halo" --silverfir "$OUT/inputs/silverfir" \
        --v2h-generator "$arms/candidate/research/experiments/match-dispatch/wasm/gen.py" --out "$dir"
    # The timed data and the counters are separate acquisitions: a failed
    # perf stat leaves the timed result, which is still reduced.
    status=0
    python3 -B "$interp" time --out "$dir" --cpu "$cpu" \
        --min-rounds "${INTERP_MIN_ROUNDS:-10}" --max-rounds "${INTERP_MAX_ROUNDS:-30}" || status=$?
    if [ "$status" = 0 ] && [ "$perf" = 1 ]; then
        python3 -B "$interp" perf --out "$dir" --cpu "$cpu" --rounds "${PERF_ROUNDS:-5}" || status=$?
    fi
    if [ "$status" = 0 ]; then
        python3 -B "$interp" reduce --out "$dir" --floor "${INTERP_FLOOR:-}" || status=$?
    fi
    exit "$status"
    ;;

summary)
    # Appends each group's summary to the job summary (stdout outside CI).
    {
        for group in hashmap interp; do
            echo "## compiler-compare: $group (${RUNNER_KIND:-?})"
            echo
            echo '```text'
            cat "$OUT/$group/summary.txt" 2>/dev/null || echo "no summary was written (the group did not run or failed)"
            echo '```'
            echo
        done
    } >> "${GITHUB_STEP_SUMMARY:-/dev/stdout}"
    ;;

*)
    echo "unknown stage $stage" >&2
    exit 2
    ;;
esac
