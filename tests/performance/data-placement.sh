#!/usr/bin/env bash
# compute-regression's data-placement control (README.md, "Data placement").
# Linux only; a results directory must be fresh.
#
#   control BASELINE CANDIDATE RESULTS
#       Records both arms' mapping calls per kernel. For each kernel whose arms
#       map differently, launches both at the 32 positions and compares them in
#       order of time (positions.awk), and launches both again when a rank is
#       unlike. Writes RESULTS/verdict.txt and RESULTS/aside.txt, the kernels
#       judge.sh then neither times nor judges. Exit 0 whether or not a kernel
#       is set aside, 2 on a refusal.
#   qualify CANDIDATE RESULTS
#       The control on the candidate against itself must find every kernel
#       mapping alike; with the candidate arm at position 1 every kernel's
#       record must hold the runner's idle mapping and, without it, still
#       differ, as the later mappings lie lower. Only then are all kernels
#       launched at the 32 positions, which must set none aside. Writes
#       RESULTS/verdict.txt; exit 1 when an expectation fails, 2 on a refusal.
#
# Those two start each unit of fixed work below under run-check.pl, so every
# run of one label is one amount of work and its budget row bounds it:
#   record BASELINE CANDIDATE RESULTS [shifted]    performance-data-layout
#   round BASELINE CANDIDATE RESULTS KERNEL ROUND  performance-data-round-KERNEL
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
usage() {
    echo 'usage: data-placement.sh control BASELINE CANDIDATE RESULTS | qualify CANDIDATE RESULTS' >&2
    exit 2
}
refuse() {
    echo "REFUSED: $*" >&2
    exit 2
}
[[ $# -ge 1 ]] || usage
[[ $(uname -s) == Linux ]] || refuse 'the data-placement control requires the Linux host'
# The kernels are the Makefile's, as compare.sh and judge.sh read them; KERNELS
# in the environment names fewer.
read -r -a kernels <<< "${KERNELS:-$(sed -n 's/^KERNELS := //p' "$here/Makefile")}"
((${#kernels[@]} > 0)) || refuse 'no kernels named'
machine=$(uname -m)

unit() {
    local label=$1
    shift
    WHITEFOOT_CHECK_TIMEOUT=300 perl "$here/../../.github/run-check.pl" "$label" \
        bash "$here/data-placement.sh" "$@"
}

# One launch per kernel and arm at one worker without address randomization,
# its mapping calls traced up to the runner's exit before teardown.
record() {
    local baseline=$1 candidate=$2 results=$3 shifted=${4:-} kernel arm directory key
    local -a position
    [[ -z $shifted || $shifted == shifted ]] || usage
    mkdir -p "$results/layout"
    : > "$results/layout.new"
    for kernel in "${kernels[@]}"; do
        for arm in baseline candidate; do
            directory=${!arm}
            key=$results/layout/$kernel-$arm
            position=()
            if [[ $arm == candidate && -n $shifted ]]; then position=(WF_PERFORMANCE_POSITION=1); fi
            env -u WF_SPLIT_WORK WF_WORKERS=1 "${position[@]}" timeout 60s setarch "$machine" -R \
                strace -f -e trace=mmap,munmap,mremap -o "$key.strace" "$directory/$kernel" layout 1 \
                > "$key.log" 2>&1 || { cat "$key.log" >&2; refuse "the traced launch of $kernel $arm failed"; }
            awk -f "$here/layout.awk" "$key.strace" > "$key.unsorted" ||
                { cat "$key.unsorted" >&2; refuse "the trace of $kernel $arm"; }
            LC_ALL=C sort "$key.unsorted" > "$key.calls"
        done
        if cmp -s "$results/layout/$kernel-baseline.calls" "$results/layout/$kernel-candidate.calls"; then
            echo "$kernel alike"
        else
            echo "$kernel differ"
        fi >> "$results/layout.new"
    done
    mv "$results/layout.new" "$results/layout.txt"
    cat "$results/layout.txt"
}

# Both arms of one kernel at the 32 positions, one launch each at one worker
# without address randomization, the arm order alternating by position. Each
# launch is an ordinary measured process; its row is the median of its five
# measured calls.
round() {
    local baseline=$1 candidate=$2 results=$3 kernel=$4 round=$5 position arm directory key
    local -a arms
    [[ " ${kernels[*]} " == *" $kernel "* && ($round == 1 || $round == 2) ]] || usage
    mkdir -p "$results/logs"
    for ((position = 0; position < 32; position++)); do
        arms=(baseline candidate)
        if ((position % 2)); then arms=(candidate baseline); fi
        for arm in "${arms[@]}"; do
            directory=${!arm}
            key=$results/logs/$kernel-$round-$position-$arm
            env -u WF_SPLIT_WORK WF_WORKERS=1 WF_PERFORMANCE_POSITION="$position" timeout 60s \
                setarch "$machine" -R "$directory/$kernel" measure "$arm" 1 0 \
                > "$key.tsv" 2> "$key.log" || { cat "$key.log" >&2; refuse "the launch of $kernel $arm at $position failed"; }
            cat "$key.tsv" >> "$results/raw.tsv"
            awk -v kernel="$kernel" -v arm="$arm" -v position="$position" -v round="$round" '
                NF != 8 || $1 != kernel || $2 != arm || $3 != "1" || $4 != "0" ||
                    $5 !~ /^[0-5]$/ || ($5 in seen) || $6 !~ /^[0-9]+$/ || $6 + 0 <= 0 { bad = 1; exit 2 }
                { seen[$5] = 1; if ($5 != "0") wall[++n] = $6 }
                END {
                    if (bad || n != 5) exit 2
                    for (i = 2; i <= 5; i++) {
                        x = wall[i]
                        for (j = i - 1; j >= 1 && wall[j] + 0 > x + 0; j--) wall[j + 1] = wall[j]
                        wall[j + 1] = x
                    }
                    print kernel, arm, position, round, wall[3]
                }' "$key.tsv" >> "$results/rows.tsv" || refuse "the rows of $kernel $arm at $position"
        done
    done
}

# The control in RESULTS, its verdict in RESULTS/verdict.txt and its list in
# RESULTS/aside.txt.
control() {
    local baseline=$1 candidate=$2 results=$3
    [[ ! -e $results ]] || refuse 'results must be a fresh directory'
    mkdir -p "$results"
    unit performance-data-layout record "$baseline" "$candidate" "$results" || refuse 'the mapping record'
    positions "$baseline" "$candidate" "$results"
}

# The rounds of the kernels RESULTS/layout.txt reads as differing, and what
# positions.awk reads from them.
positions() {
    local baseline=$1 candidate=$2 results=$3 kernel word
    : > "$results/rows.tsv"
    for kernel in $(awk '$2 == "differ" { print $1 }' "$results/layout.txt"); do
        unit "performance-data-round-$kernel" round "$baseline" "$candidate" "$results" "$kernel" 1 ||
            refuse "round 1 of $kernel"
    done
    awk -v stage=first -v kernels="${kernels[*]}" -f "$here/positions.awk" "$results/layout.txt" "$results/rows.tsv" \
        > "$results/again.txt" || { cat "$results/again.txt" >&2; refuse 'the first round'; }
    while read -r kernel word; do
        [[ $word == again ]] || refuse "unreadable line of again.txt: $kernel $word"
        unit "performance-data-round-$kernel" round "$baseline" "$candidate" "$results" "$kernel" 2 ||
            refuse "round 2 of $kernel"
    done < "$results/again.txt"
    awk -v stage=final -v kernels="${kernels[*]}" -f "$here/positions.awk" "$results/layout.txt" "$results/rows.tsv" \
        > "$results/verdict.txt" || { cat "$results/verdict.txt" >&2; refuse 'the rounds'; }
    awk -v stage=aside -v kernels="${kernels[*]}" -f "$here/positions.awk" "$results/layout.txt" "$results/rows.tsv" \
        > "$results/aside.txt" || { cat "$results/aside.txt" >&2; refuse 'the rounds'; }
    cat "$results/verdict.txt"
}

# The runner's own idle mapping at position 1 (runner.c), as layout.awk writes
# it. A record of the candidate there differs from one at position 0 by this
# call in any case; the qualification requires each kernel's record to differ
# without it too, as it does when every later mapping lies 2 MiB lower.
own_mapping='mmap(NULL, 2097152, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_NORESERVE, -1, 0) = '
qualify() {
    local candidate=$1 results=$2 failed=0 alike moved=0 aside= kernel calls
    [[ ! -e $results ]] || refuse 'results must be a fresh directory'
    mkdir -p "$results/moved"
    control "$candidate" "$candidate" "$results/same"
    unit performance-data-layout record "$candidate" "$candidate" "$results/moved" shifted ||
        refuse 'the mapping record'
    alike=$(awk '$2 == "alike"' "$results/same/layout.txt" | wc -l)
    for kernel in "${kernels[@]}"; do
        calls=$results/moved/layout/$kernel
        grep -v -F -- "$own_mapping" "$calls-candidate.calls" > "$calls-candidate.later" || true
        if [[ $(grep -c -F -- "$own_mapping" "$calls-candidate.calls") == 1 &&
              $(grep -c -F -- "$own_mapping" "$calls-baseline.calls") == 0 ]] &&
            ! cmp -s "$calls-candidate.later" "$calls-baseline.calls"; then
            moved=$((moved + 1))
        fi
    done
    if ((alike == ${#kernels[@]} && moved == ${#kernels[@]})); then
        positions "$candidate" "$candidate" "$results/moved"
        aside=$(cat "$results/moved/aside.txt")
    fi
    {
        echo 'data-placement qualification: the candidate against itself'
        if ((alike == ${#kernels[@]})); then
            echo 'PASS: every kernel maps alike'
        else
            echo "FAIL: $((${#kernels[@]} - alike)) kernel(s) map differently"; failed=1
        fi
        if ((moved == ${#kernels[@]})); then
            echo 'PASS: with the candidate arm at position 1, every kernel maps its later calls elsewhere'
        else
            echo "FAIL: with the candidate arm at position 1, $((${#kernels[@]} - moved)) kernel(s) lack the idle mapping or a later call elsewhere"
            failed=1
        fi
        if ((failed)); then
            echo 'the 32 positions were not launched: a record check failed'
        else
            if [[ -z $aside ]]; then
                echo 'PASS: the 32 positions set no kernel aside'
            else
                echo "FAIL: the 32 positions set aside: $aside"; failed=1
            fi
            printf '\nthe candidate against itself at the 32 positions:\n'
            cat "$results/moved/verdict.txt"
        fi
    } > "$results/verdict.txt"
    cat "$results/verdict.txt"
    return "$failed"
}

command=$1
shift
case $command in
    control) [[ $# == 3 ]] || usage; control "$@" ;;
    qualify) [[ $# == 2 ]] || usage; qualify "$@" ;;
    record) [[ $# == 3 || $# == 4 ]] || usage; record "$@" ;;
    round) [[ $# == 5 ]] || usage; round "$@" ;;
    *) usage ;;
esac
