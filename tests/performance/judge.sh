#!/usr/bin/env bash
# The hosted paired comparison's judgment, consumed by compute-regression only.
# usage: judge.sh BASELINE_IMAGES CANDIDATE_IMAGES RESULTS   (RESULTS must not exist)
#
# 1. A kernel whose two executables are byte-identical is the same program: it
#    is not verified or timed and counts as passing (the images are built with
#    the same embedded paths, tests/performance/Makefile).
# 2. The other kernels are timed. Round 1 is the null control (the candidate's
#    image against itself) and the comparison, each its own run-check stage.
#    decide.awk gives each kernel `pass`, `FAIL`, or `indeterminate`: the
#    comparison is adverse at two widths while the kernel's own null control also
#    shows an adverse width, so the host's noise reaches the threshold.
# 3. If no kernel failed and some are indeterminate, those kernels alone are
#    measured once more as a retest round (stages performance-retest-*), and
#    decided the same way. A kernel that failed in either round fails the job
#    whatever the others did; indeterminate after the retest is INCONCLUSIVE.
# Exit status: 0 pass, 1 a kernel failed, 3 inconclusive, 2 refused or a
# measurement failed to run. RESULTS/verdict.txt is the one record of the decision.
#
# Seams for the tests of this script: WF_PERF_MEASURE (the measuring command,
# compare.sh by default), WF_PERF_RUN_CHECK (the command prefix that labels a
# stage, run-check.pl by default), WF_PERF_CPUS (the CPU count) and KERNELS.
set -eo pipefail
[[ $# == 3 ]] || { echo 'usage: judge.sh BASELINE_IMAGES CANDIDATE_IMAGES RESULTS' >&2; exit 2; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
baseline=$1; candidate=$2; results=$3
[[ ! -e $results ]] || { echo 'results must be a fresh directory' >&2; exit 2; }
measure=${WF_PERF_MEASURE:-"bash $here/compare.sh"}
run_check=${WF_PERF_RUN_CHECK:-"perl $here/../../.github/run-check.pl"}
cpus=${WF_PERF_CPUS:-$(nproc)}
read -r -a kernels <<< "${KERNELS:-$(sed -n 's/^KERNELS := //p' "$here/Makefile")}"
((${#kernels[@]} > 0)) || { echo 'no kernels named' >&2; exit 2; }
mkdir -p "$results"
cpu_model=$(sh "$here/cpu-model.sh")

identical=(); timed=()
for kernel in "${kernels[@]}"; do
    if cmp -s "$baseline/$kernel" "$candidate/$kernel"; then identical+=("$kernel"); else timed+=("$kernel"); fi
done
: > "$results/identical.txt"
for kernel in "${identical[@]}"; do echo "$kernel" >> "$results/identical.txt"; done

lines=()      # the decision's lines, in order
failed=()     # kernels that failed, with the round
open=()       # kernels still indeterminate

# One round over the kernels given: its null control, its comparison, its decision.
# A measurement that did not run (status above 1) refuses the whole judgment.
round() {
    local number=$1 prefix=$2; shift 2
    local directory="$results/round$number" status
    for step in null comparison; do
        status=0
        if [[ $step == null ]]; then
            KERNELS="$*" $run_check "$prefix-null" $measure "$candidate" "$candidate" "$directory/null" || status=$?
        else
            KERNELS="$*" $run_check "$prefix-comparison" $measure "$baseline" "$candidate" "$directory/comparison" || status=$?
        fi
        # Status 1 is the verdict of that one table; decide.awk reads the table.
        ((status <= 1)) || { echo "REFUSED: the $step measurement of round $number did not run (status $status)" >&2; exit 2; }
    done
    status=0
    awk -v cpus="$cpus" -v kernels="$*" -f "$here/decide.awk" \
        "$directory/null/paired.tsv" "$directory/comparison/paired.tsv" > "$directory/decision.txt" || status=$?
    ((status == 0)) || { cat "$directory/decision.txt" >&2; exit 2; }
    while read -r _ kernel result rest; do
        lines+=("round $number: $kernel $result ($rest)")
        case $result in
            FAIL) failed+=("$kernel (round $number)") ;;
            indeterminate) open+=("$kernel") ;;
        esac
    done < "$directory/decision.txt"
}

if ((${#timed[@]} > 0)); then
    round 1 performance "${timed[@]}"
    if ((${#failed[@]} == 0 && ${#open[@]} > 0)); then
        retest=("${open[@]}"); open=()
        round 2 performance-retest "${retest[@]}"
    fi
fi

if ((${#failed[@]} > 0)); then verdict=FAIL; status=1
elif ((${#open[@]} > 0)); then verdict=INCONCLUSIVE; status=3
else verdict=PASS; status=0; fi
{
    echo 'rule: a kernel fails when its wall ratio is below 0.97 with >=4/5 lower pairs at two widths and its own'
    echo 'null control shows no such width; with one, the kernel is indeterminate and measured once more.'
    echo "cpu model: $cpu_model"
    for kernel in "${identical[@]}"; do echo "$kernel: identical image, not timed, pass"; done
    ((${#timed[@]} == 0)) || echo "timed: ${timed[*]}"
    ((${#lines[@]} == 0)) || printf '%s\n' "${lines[@]}"
    case $verdict in
        PASS) echo "VERDICT: PASS -- ${#identical[@]} image(s) identical, ${#timed[@]} timed" ;;
        FAIL) echo "VERDICT: FAIL -- ${failed[*]}" ;;
        *) echo "VERDICT: INCONCLUSIVE -- ${open[*]} still indeterminate after the retest" ;;
    esac
} > "$results/verdict.txt"
cat "$results/verdict.txt"
if [[ $verdict == INCONCLUSIVE ]]; then
    echo "::warning::INCONCLUSIVE: ${open[*]} could not be judged: the host's null control reads at the threshold in both rounds"
    if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
        echo "INCONCLUSIVE: ${open[*]} could not be judged (null control at the threshold in both rounds); no regression verdict." >> "$GITHUB_STEP_SUMMARY"
    fi
fi
exit "$status"
