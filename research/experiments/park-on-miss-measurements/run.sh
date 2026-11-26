#!/bin/sh
# Every line of `research/experiments/park-on-miss-measurements/README.md` that
# a script can produce.
#
# The measurements design section 12 and the plan's "Decided 2026-09-05:
# measured before chosen" ask for, taken with one method: the io-completion-
# bench runner's discipline (`../io-completion-bench/runner.c` -- interleaved
# passes with alternating direction, a median and the observed spread over N
# recorded passes, and every recorded run's bytes compared against the same
# expected line, so a form that computed something else cannot report a time).
#
# It is a measurement and not a gate. `compiler/Makefile` reaches it through
# `park-on-miss-measurements`, outside `check`, and nothing in the repository
# depends on what it prints.
#
#   sh run.sh              every section
#   sh run.sh park compute one or more sections by name
#
# Sections: park compute lanes io chain stacks ledger spin
#
# Slice 4b deleted the compile-time variants of the scheduler core, so the
# sections that swept them -- the per-form gate table and the per-form liveness
# probe -- are gone with the forms, and every section below measures the
# shipped core. The lane slot count survives as the `#if !defined` override of
# `core.h` that it was before the sweep, so the lane sweep still runs.
set -e

# The repository root, derived from this script's own location so that no
# tracked file names anyone's home directory (the `repository-invariants` gate
# of the root Makefile). It is still overridable for a tree checked out
# elsewhere.
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=${ROOT:-$(cd "$HERE/../../.." && pwd)}
WHITEFOOT_SCRATCH_ROOT=${WHITEFOOT_SCRATCH_ROOT:-${TMPDIR:-/tmp}/whitefoot}
WORK=${WORK:-$WHITEFOOT_SCRATCH_ROOT/whitefoot-park-on-miss-measurements}
BUILD=$WORK/build
TREE=$WORK/tree
CLANG=${CLANG:-/usr/bin/clang}
BACKEND=$ROOT/compiler/src/backend
BUNDLE=$ROOT/research/experiments/park-on-miss-measurements
IOBENCH=$ROOT/research/experiments/io-completion-bench
WFC=$ROOT/compiler/target/gate/whitefootc

# The timing protocol. ROUNDS recorded passes after WARMUP unrecorded ones;
# the runner prints the median and the observed minimum and maximum of each.
ROUNDS=${ROUNDS:-9}
WARMUP=${WARMUP:-2}
# The chain and the park micro-benchmark report one number per run, so they are
# repeated here rather than by the runner.
REPEATS=${REPEATS:-7}
# The park micro-benchmark: how many interleaved passes, and how many park and
# publish round trips each one times.
PARK_ROUNDS=${PARK_ROUNDS:-15}
PARK_ITERATIONS=${PARK_ITERATIONS:-200000}
# The many-files tree, at io-completion-bench's own default shape.
FILES=${FILES:-8192}
MAX_KIB=${MAX_KIB:-16}
FILES_EXPECTED=${FILES_EXPECTED:-"17098009301725298919 00000000000071024640"}
# The four-stage chain's shape, from design section 12 item 4.
CHAIN_JOBS=${CHAIN_JOBS:-1000}
CHAIN_THREADS=${CHAIN_THREADS:-8}
CHAIN_SLOTS=${CHAIN_SLOTS:-32}
CHAIN_STACKS=${CHAIN_STACKS:-72}

# The forms measured, as the define each is built with. `shipped` is the core
# as it is compiled everywhere else and takes no define at all; the lane slot
# counts are the one remaining alternative, and they are a constant of `core.h`
# rather than a protocol the section 11 enumerator judges.
ADMITTED_FORMS=${ADMITTED_FORMS:-"shipped"}
PARK_FORMS=${PARK_FORMS:-"shipped"}
LANE_SLOT_COUNTS=${LANE_SLOT_COUNTS:-"2 4 8 16"}

# The idle spin's sweep: `WF_SCHED_IDLE_SPIN_ROUNDS` and
# `WF_SCHED_IDLE_YIELD_ROUNDS` of `compiler/src/backend/sched/core.h`, as the
# pair of defines each form is built with. `spin-0-0` is the form the tree
# shipped before the spin existed -- the idle window parks the moment its last
# look misses -- and is the reference every line here is read against.
# The spin rounds carry two points below the coarse grid this sweep started
# from (0, 256, 1024, 4096, 16384), because the first run showed the cost of a
# round where the design did not expect one: a round takes the core's one mutex
# to look at the ready list, so the interesting region is nearer 16 than 256.
SPIN_ROUNDS=${SPIN_ROUNDS:-"0 16 64 256 1024 4096 16384"}
SPIN_YIELDS=${SPIN_YIELDS:-"0 16 64"}
spin_forms() {
    for rounds in $SPIN_ROUNDS; do
        for yields in $SPIN_YIELDS; do
            printf 'spin-%s-%s ' "$rounds" "$yields"
        done
    done
}
SPIN_FORMS=${SPIN_FORMS:-$(spin_forms)}
# The mixed program's tree: two files of MIXED_KIB KiB, which is what
# `programs/windows_runtime_mixed.wf` reads (4096 blocks of 4096 bytes).
MIXED_KIB=${MIXED_KIB:-16384}
# The I/O line is a best-of, as design section 12 item 4 reads it.
SPIN_IO_ROUNDS=${SPIN_IO_ROUNDS:-7}

define_of() {
    case $1 in
    shipped) printf '' ;;
    lane-slots-*) printf -- '-DWF_SCHED_LANE_SLOTS=%su' "${1#lane-slots-}" ;;
    spin-*-*)
        pair=${1#spin-}
        printf -- '-DWF_SCHED_IDLE_SPIN_ROUNDS=%su -DWF_SCHED_IDLE_YIELD_ROUNDS=%su' \
            "${pair%%-*}" "${pair##*-}"
        ;;
    *) echo "run.sh: unknown form $1" >&2; exit 2 ;;
    esac
}

# One emitted module linked against one form of the runtime, which is how the
# plan's own worker-start measurement was taken: the module is compiled once
# and only the C the link compiles differs, so the pair is a statement about
# the runtime and not about two programs.
#
# `statistics_observer.c` joins every link, because the stacks section reads
# the core's refusal counters through it and a counter read at exit cannot move
# a timing. It writes one line to the diagnostic channel and nothing to the
# output channel, which is what the runner compares.
link_form() {
    module=$1
    output=$2
    shift 2
    # Compile the audited child closure separately so compiler-inserted libc,
    # stack and sanitizer hooks cannot enter it. Other runtime units retain
    # the measurement's original flags.
    for unit in snapshot_export fork_capsule; do
        "$CLANG" -std=c11 -pthread -O2 -I "$BACKEND" -I "$BACKEND/completion" \
            "$@" -fno-builtin -fno-stack-protector -fno-sanitize=all \
            -c "$BACKEND/completion/$unit.c" -o "$output.$unit.o"
    done
    "$CLANG" -std=c11 -pthread -I "$BACKEND" -I "$BACKEND/completion" \
        -I "$BACKEND/sched" "$@" \
        -x c "$BACKEND/wf_floor.c" \
        -x c "$BACKEND/sched/core.c" \
        -x c "$BACKEND/sched/prim_host.c" \
        -x c "$BACKEND/sched/entry.c" \
        -x c "$BACKEND/completion/runtime.c" \
        -x c "$BACKEND/completion/wait_host.c" \
        -x c "$BACKEND/completion/file_adapter.c" \
        -x c "$BACKEND/completion/file_posix.c" \
        -x c "$BACKEND/completion/bridge.c" \
        -x c "$BACKEND/completion/stop_signals.c" \
        -x c "$BACKEND/completion/linux_io_uring.c" \
        -x c "$BACKEND/keyed_table.c" \
        -x c "$BACKEND/heap.c" \
        -x c "$BACKEND/ordinary_values.c" \
        -x c "$BUNDLE/statistics_observer.c" \
        -x ir "$BACKEND/ordinary_values.ll" \
        -x none "$output.snapshot_export.o" "$output.fork_capsule.o" \
        -x ir "$module" -Wno-override-module -O2 -lm -o "$output"
}

prepare() {
    mkdir -p "$BUILD"
    ( cd "$ROOT/compiler" && cargo build --profile gate --bin whitefootc --locked --offline )
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror "$IOBENCH/runner.c" -o "$BUILD/runner"
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror "$IOBENCH/gen.c" -o "$BUILD/gen"
}

# ----------------------------------------------------------------- park

# Design section 12 item 2: one park and one publish through the shipped core,
# in nanoseconds, beside this host's own condition-variable park-and-wake,
# which is the figure the design's 2.2 microsecond number names on another
# machine. The locked form of section 6, the claim-protocol variant and the
# weak-order variant stood beside it here until slice 4b removed them; the
# README's section 3 records what each was and why it went.
section_park() {
    echo "== park: one park and one publish through the core, nanoseconds =="
    park_sweep
    echo "-- this host's own reference primitives --"
    make -C "$ROOT/research/experiments/park-on-miss-switch-cost" CC="$CLANG" run
}

# The round trip over whatever `$PARK_FORMS` names, so that the idle-spin
# section below takes the same measurement over its own forms with the same
# discipline rather than a second one of its own.
park_sweep() {
    for form in $PARK_FORMS; do
        define=$(define_of "$form")
        # shellcheck disable=SC2086
        "$CLANG" -std=c11 -O2 -g -Wall -Wextra -Werror -Wpedantic -pthread \
            -I "$BACKEND/sched" $define \
            "$BUNDLE/park_publish.c" "$BACKEND/sched/core.c" \
            "$BACKEND/sched/prim_host.c" -o "$BUILD/park-publish-$form"
    done
    : > "$WORK/park-samples.txt"
    # Interleaved, and reversed on every other pass, for the reason the
    # io-completion-bench runner interleaves: this host drifts over the minutes
    # a table takes, and a grouped schedule turns that drift into a difference
    # between rows.
    pass=0
    while [ "$pass" -lt "$PARK_ROUNDS" ]; do
        for mode in settled racing; do
            if [ $((pass % 2)) -eq 0 ]; then
                order=$PARK_FORMS
            else
                order=$(printf '%s\n' $PARK_FORMS | tac | tr '\n' ' ')
            fi
            for form in $order; do
                line=$( ( "$BUILD/park-publish-$form" "$PARK_ITERATIONS" "$mode" ) 2>&1 ) || {
                    printf '%s %s FAULT %s\n' "$form" "$mode" "$line" \
                        >> "$WORK/park-samples.txt"
                    continue
                }
                printf '%s %s %s\n' "$form" "$mode" \
                    "$(printf '%s' "$line" | sed -n 's/.*roundtrip_ns=\([0-9]*\).*/\1/p')" \
                    >> "$WORK/park-samples.txt"
            done
        done
        pass=$((pass + 1))
    done
    printf '%-16s %-8s %10s %10s %6s\n' form mode best_ns median_ns n
    awk '$3 != "FAULT" { key = $1 " " $2; value[key] = value[key] " " $3; seen[key] = 1 }
         END {
             for (key in seen) {
                 count = split(value[key], sample, " ");
                 for (i = 2; i <= count; i++) {
                     for (j = 1; j <= count - i + 1; j++) {
                         if (sample[j] + 0 > sample[j + 1] + 0) {
                             swap = sample[j]; sample[j] = sample[j + 1]; sample[j + 1] = swap;
                         }
                     }
                 }
                 middle = (count % 2) ? sample[(count + 1) / 2] \
                     : (sample[count / 2] + sample[count / 2 + 1]) / 2;
                 split(key, part, " ");
                 printf "%-16s %-8s %10s %10s %6s\n", part[1], part[2], sample[1], middle, count;
             }
         }' "$WORK/park-samples.txt" | sort
    if grep -q FAULT "$WORK/park-samples.txt"; then
        echo "-- a form that could not complete a run --"
        grep FAULT "$WORK/park-samples.txt" | sort -u
    fi
}


# -------------------------------------------------------------- compute

# Design section 12 item 1's first half: park and resume at a compute miss on a
# compute-only program, at every worker count, so slice 4 has one table.
compute_plan() {
    program=$1
    for form in $ADMITTED_FORMS; do
        for workers in 1 2 4 8; do
            printf '%s.w%s\tWF_WORKERS=%s\t%s/%s-%s\n' \
                "$form" "$workers" "$workers" "$BUILD" "$program" "$form"
        done
    done
}

build_compute() {
    # Retain this manual experiment's historical single 800-repetition batch.
    # The formal source now defaults to one correctness invocation. Stage the
    # explicit benchmark defaults here; this build-local file is not a fixture.
    sed -e 's/let repetitions = 1_u64;/let repetitions = 800_u64;/' \
        -e 's/let initial_seed = 65.9375_f64;/let initial_seed = 16.0_f64;/' \
        "$ROOT/tests/programs/par_layout.wf" > "$BUILD/par_layout-benchmark.wf"
    "$WFC" --par --emit-llvm -o "$BUILD/par_layout.ll" "$BUILD/par_layout-benchmark.wf"
    "$WFC" --par --emit-llvm -o "$BUILD/grid.ll" "$BUNDLE/programs/grid_split.wf"
    for form in "$@"; do
        define=$(define_of "$form")
        # shellcheck disable=SC2086
        link_form "$BUILD/par_layout.ll" "$BUILD/par_layout-$form" $define
        # shellcheck disable=SC2086
        link_form "$BUILD/grid.ll" "$BUILD/grid-$form" $define
    done
}

section_compute() {
    echo "== compute: par_layout and the grid loop split, W=1,2,4,8 =="
    build_compute $ADMITTED_FORMS
    for program in par_layout grid; do
        expected=$("$BUILD/$program-shipped" 2>/dev/null)
        compute_plan "$program" > "$WORK/plan-$program.txt"
        echo "-- $program, expected [$expected], rounds=$ROUNDS warmup=$WARMUP --"
        "$BUILD/runner" "$WORK/plan-$program.txt" "$ROUNDS" "$WARMUP" "$expected" \
            2> "$WORK/runner-$program.err"
    done
}

# ---------------------------------------------------------------- lanes

# Design section 12 item 6's first half: the lane slot count, at the worker
# counts where a lane is contended.
section_lanes() {
    echo "== lanes: WF_SCHED_LANE_SLOTS at 2, 4, 8, 16 against the shipped 64 =="
    forms="shipped"
    for count in $LANE_SLOT_COUNTS; do
        forms="$forms lane-slots-$count"
    done
    build_compute $forms
    for program in par_layout grid; do
        expected=$("$BUILD/$program-shipped" 2>/dev/null)
        {
            for form in $forms; do
                for workers in 4 8; do
                    printf '%s.w%s\tWF_WORKERS=%s\t%s/%s-%s\n' \
                        "$form" "$workers" "$workers" "$BUILD" "$program" "$form"
                done
            done
        } > "$WORK/plan-lanes-$program.txt"
        echo "-- $program, expected [$expected], rounds=$ROUNDS warmup=$WARMUP --"
        "$BUILD/runner" "$WORK/plan-lanes-$program.txt" "$ROUNDS" "$WARMUP" "$expected" \
            2> "$WORK/runner-lanes-$program.err"
    done
}

# ------------------------------------------------------------------- io

# Design section 12 item 4's program half: the many-files workload at the
# default helper policy and at four helpers.
section_io() {
    echo "== io: many_files_wide at the default policy and at WF_IO_HELPERS=4 =="
    rm -rf "$TREE"
    mkdir -p "$TREE"
    "$BUILD/gen" "$TREE" "$FILES" "$MAX_KIB"
    "$WFC" --emit-llvm -o "$BUILD/wide.ll" "$IOBENCH/programs/many_files_wide.wf"
    for form in $ADMITTED_FORMS; do
        define=$(define_of "$form")
        # shellcheck disable=SC2086
        link_form "$BUILD/wide.ll" "$BUILD/wide-$form" $define
    done
    {
        for form in $ADMITTED_FORMS; do
            printf '%s.default\t\t%s/wide-%s\n' "$form" "$BUILD" "$form"
            printf '%s.h4\tWF_IO_HELPERS=4\t%s/wide-%s\n' "$form" "$BUILD" "$form"
        done
    } > "$WORK/plan-io.txt"
    echo "-- many_files_wide, expected [$FILES_EXPECTED], rounds=$ROUNDS warmup=$WARMUP --"
    ( cd "$TREE" && "$BUILD/runner" "$WORK/plan-io.txt" "$ROUNDS" "$WARMUP" \
        "$FILES_EXPECTED" 2> "$WORK/runner-io.err" )
}


# ------------------------------------------------------------------ spin

# The bounded spin before the idle park: `WF_SCHED_IDLE_SPIN_ROUNDS` and
# `WF_SCHED_IDLE_YIELD_ROUNDS` of `compiler/src/backend/sched/core.h`, swept
# against `spin-0-0`, which is the form the tree carried before the spin
# existed -- the idle window parks the moment its last look misses. The
# README's "section 12 addendum: the idle spin" is the table.
#
# Four lines, all under the method above. The mixed program in the three
# builds the Windows qualification bench compares, because that bench is the
# judge the spin exists for; the two compute programs of section 12 item 1,
# which the spin can move because a compute miss whose stolen child finishes
# soon then finds the READY stack with no kernel wake; the many-files I/O
# workload, where a spin standing in front of the drain would show; and the
# park and publish round trip. The runner prints child user and system CPU
# beside every wall time, which is the column that says whether a form won its
# wall time by burning a core.

# The mixed program of `../io-completion-bench/programs/windows_runtime_mixed.wf`
# in the bench's three builds: the fully sequential reference (`--no-overlap`),
# the completion-only control (no flag), and the unified contender (`--par`).
build_mixed() {
    "$WFC" --no-overlap --emit-llvm -o "$BUILD/mixed-seq.ll" "$IOBENCH/programs/windows_runtime_mixed.wf"
    "$WFC" --emit-llvm -o "$BUILD/mixed-iocp.ll" "$IOBENCH/programs/windows_runtime_mixed.wf"
    "$WFC" --par --emit-llvm -o "$BUILD/mixed-full.ll" "$IOBENCH/programs/windows_runtime_mixed.wf"
    for form in "$@"; do
        define=$(define_of "$form")
        for build in seq iocp full; do
            # shellcheck disable=SC2086
            link_form "$BUILD/mixed-$build.ll" "$BUILD/mixed-$build-$form" $define
        done
    done
}

section_spin() {
    echo "== spin: the idle window's bounded spin, swept =="
    echo "-- forms: $SPIN_FORMS --"
    # Every line is read against the first form named, which is `spin-0-0` in
    # the sweep above: the form the tree carried before the spin existed.
    spin_reference=$(printf '%s\n' $SPIN_FORMS | head -1)

    echo "== spin/mixed: windows_runtime_mixed.wf in the bench's three builds, WF_WORKERS=4 =="
    build_mixed $SPIN_FORMS
    rm -rf "$WORK/mixedtree"
    mkdir -p "$WORK/mixedtree"
    "$BUILD/gen" "$WORK/mixedtree" 2 "$MIXED_KIB" fixed
    mixed_expected=$( cd "$WORK/mixedtree" && "$BUILD/mixed-seq-$spin_reference" f00000.dat f00001.dat )
    {
        for form in $SPIN_FORMS; do
            for build in seq iocp full; do
                printf '%s.%s\tWF_WORKERS=4\t%s/mixed-%s-%s\tf00000.dat\tf00001.dat\n' \
                    "$build" "$form" "$BUILD" "$build" "$form"
            done
        done
    } > "$WORK/plan-spin-mixed.txt"
    echo "-- mixed, expected [$mixed_expected], rounds=$ROUNDS warmup=$WARMUP --"
    ( cd "$WORK/mixedtree" && "$BUILD/runner" "$WORK/plan-spin-mixed.txt" "$ROUNDS" "$WARMUP" \
        "$mixed_expected" 2> "$WORK/runner-spin-mixed.err" )

    echo "== spin/compute: par_layout and the grid loop split, W=1,2,4,8 =="
    build_compute $SPIN_FORMS
    for program in par_layout grid; do
        expected=$("$BUILD/$program-$spin_reference" 2>/dev/null)
        {
            for form in $SPIN_FORMS; do
                for workers in 1 2 4 8; do
                    printf '%s.w%s\tWF_WORKERS=%s\t%s/%s-%s\n' \
                        "$form" "$workers" "$workers" "$BUILD" "$program" "$form"
                done
            done
        } > "$WORK/plan-spin-$program.txt"
        echo "-- $program, expected [$expected], rounds=$ROUNDS warmup=$WARMUP --"
        "$BUILD/runner" "$WORK/plan-spin-$program.txt" "$ROUNDS" "$WARMUP" "$expected" \
            2> "$WORK/runner-spin-$program.err"
    done

    echo "== spin/io: many_files_wide at the default policy =="
    rm -rf "$TREE"
    mkdir -p "$TREE"
    "$BUILD/gen" "$TREE" "$FILES" "$MAX_KIB"
    "$WFC" --emit-llvm -o "$BUILD/wide.ll" "$IOBENCH/programs/many_files_wide.wf"
    for form in $SPIN_FORMS; do
        define=$(define_of "$form")
        # shellcheck disable=SC2086
        link_form "$BUILD/wide.ll" "$BUILD/wide-$form" $define
    done
    {
        for form in $SPIN_FORMS; do
            printf '%s.default\t\t%s/wide-%s\n' "$form" "$BUILD" "$form"
        done
    } > "$WORK/plan-spin-io.txt"
    echo "-- many_files_wide, expected [$FILES_EXPECTED], rounds=$SPIN_IO_ROUNDS warmup=$WARMUP --"
    ( cd "$TREE" && "$BUILD/runner" "$WORK/plan-spin-io.txt" "$SPIN_IO_ROUNDS" "$WARMUP" \
        "$FILES_EXPECTED" 2> "$WORK/runner-spin-io.err" )

    # Twenty-one forms and two modes, so each pass times a quarter of the
    # round trips section 3 times; the number is a per-round-trip average and
    # the spread over PARK_ROUNDS passes is what the table reports either way.
    echo "== spin/park: one park and one publish through the core, nanoseconds =="
    PARK_FORMS=$SPIN_FORMS
    PARK_ITERATIONS=${SPIN_PARK_ITERATIONS:-50000}
    park_sweep
}

# ---------------------------------------------------------------- chain

# Design section 12 item 4: the four-stage chain in C on io_uring, in the four
# shapes, with the dependent stage's in-flight depth beside the wall time.
section_chain() {
    echo "== chain: the four-stage chain, $CHAIN_JOBS files, $CHAIN_THREADS threads =="
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic -pthread \
        -I "$IOBENCH" -I "$BACKEND/sched" \
        "$IOBENCH/chain.c" "$BACKEND/sched/core.c" "$BACKEND/sched/prim_host.c" \
        -o "$BUILD/chain"
    rm -rf "$WORK/chaintree"
    "$BUILD/gen" "$WORK/chaintree" "$CHAIN_JOBS" "$MAX_KIB"
    for repeat in $(seq 1 "$REPEATS"); do
        for shape in nested compensate switch pipeline; do
            "$BUILD/chain" "$WORK/chaintree" "$shape" "$CHAIN_JOBS" \
                "$CHAIN_THREADS" "$CHAIN_SLOTS" "$CHAIN_STACKS" | grep '^chain: shape'
        done
    done
}

# --------------------------------------------------------------- stacks

# Design section 12 item 5: the smallest stack count at which the pool stops
# refusing, and what a refusal costs below it. A refusal is the fourth line of
# the rule taken because the pool had no stack; the core counts it, and
# `statistics_observer.c` prints the count at exit.
section_stacks() {
    echo "== stacks: the WF_STACKS sweep, refusals and wall time =="
    build_compute shipped
    for program in par_layout grid; do
        for workers in 4 8; do
            floor=$((workers + 1))
            for stacks in $floor $((floor + 1)) $((floor + 3)) $((floor + 7)) \
                          $((floor + 11)) $((floor + 15)) $((floor + 23)) \
                          $((floor + 31)); do
                line=$(WF_WORKERS=$workers WF_STACKS=$stacks \
                    "$BUILD/$program-shipped" 2>&1 >/dev/null | grep sched-statistics)
                start=$(date +%s%N)
                WF_WORKERS=$workers WF_STACKS=$stacks "$BUILD/$program-shipped" \
                    >/dev/null 2>/dev/null
                end=$(date +%s%N)
                printf '%-12s W=%s STACKS=%-3s wall_ms=%s %s\n' \
                    "$program" "$workers" "$stacks" \
                    "$(( (end - start) / 1000000 ))" "$line"
            done
        done
    done
}

# --------------------------------------------------------------- ledger

# Design section 12 items 6 and 7: record memory per frame, and the ledger's
# chain bound per hand-out entry.
#
# The "before" column is the record size the tree carried at 92b19e1, read from
# that commit rather than remembered, and applied to a scratch copy of the
# compiler so that the tree itself is untouched. Only the emitter's constant
# matters to the ledger, which reads the emitted module's frames and not the
# runtime's.
section_ledger() {
    echo "== ledger: record bytes per frame, before and after =="
    before=$(git -C "$ROOT" show 92b19e1:compiler/src/backend/emitter/completion.rs \
        | sed -n 's/^pub(crate) const COMPLETION_RECORD_BYTES: u64 = \([0-9]*\);/\1/p')
    after=$(sed -n 's/^pub(crate) const COMPLETION_RECORD_BYTES: u64 = \([0-9]*\);/\1/p' \
        "$BACKEND/emitter/completion.rs")
    beforeheader=$(git -C "$ROOT" show 92b19e1:compiler/src/backend/completion/contract.h \
        | sed -n 's/^#define WF_COMPLETION_RECORD_BYTES \([0-9]*\)u/\1/p')
    afterheader=$(sed -n 's/^#define WF_COMPLETION_RECORD_BYTES \([0-9]*\)u/\1/p' \
        "$BACKEND/completion/contract.h")
    echo "record bytes: before=$before (header $beforeheader) after=$after (header $afterheader)"
    rm -rf "$WORK/before"
    mkdir -p "$WORK/before"
    cp -a "$ROOT/compiler" "$WORK/before/compiler"
    rm -rf "$WORK/before/compiler/target"
    # The crate includes the active specification from beside it, so the copy
    # needs the repository around it as well as the directory.
    ln -s "$ROOT/spec" "$WORK/before/spec"
    ln -s "$ROOT/tests" "$WORK/before/tests"
    sed -i "s/^pub(crate) const COMPLETION_RECORD_BYTES: u64 = $after;/pub(crate) const COMPLETION_RECORD_BYTES: u64 = $before;/" \
        "$WORK/before/compiler/src/backend/emitter/completion.rs"
    sed -i "s/^#define WF_COMPLETION_RECORD_BYTES ${afterheader}u/#define WF_COMPLETION_RECORD_BYTES ${before}u/" \
        "$WORK/before/compiler/src/backend/completion/contract.h"
    ( cd "$WORK/before/compiler" && cargo build --profile gate --bin whitefootc --offline )
    beforewfc=$WORK/before/compiler/target/gate/whitefootc
    printf '%-28s %10s %10s %10s\n' program "main($before)" "main($after)" growth
    for source in "$ROOT"/tests/programs/*.wf; do
        name=$(basename "$source" .wf)
        old=$("$beforewfc" --stack-ledger -o /dev/null "$source" 2>/dev/null \
            | awk '$2 == "chain" && $3 == "wf__main_body" { print $4 }')
        new=$("$WFC" --stack-ledger -o /dev/null "$source" 2>/dev/null \
            | awk '$2 == "chain" && $3 == "wf__main_body" { print $4 }')
        if [ -z "$old" ] || [ -z "$new" ]; then
            continue
        fi
        printf '%-28s %10s %10s %10s\n' "$name" "$old" "$new" "$((new - old))"
    done
    echo "== ledger: the deepest chain bound, before and after =="
    for source in "$ROOT"/tests/programs/*.wf; do
        name=$(basename "$source" .wf)
        old=$("$beforewfc" --stack-ledger -o /dev/null "$source" 2>/dev/null \
            | awk '$2 == "chain" { if ($4 + 0 > deepest) deepest = $4 + 0 } END { print deepest + 0 }')
        new=$("$WFC" --stack-ledger -o /dev/null "$source" 2>/dev/null \
            | awk '$2 == "chain" { if ($4 + 0 > deepest) deepest = $4 + 0 } END { print deepest + 0 }')
        printf '%-28s %10s %10s %10s\n' "$name" "$old" "$new" "$((new - old))"
    done
    echo "== ledger: the chain bound per hand-out entry, across tests/programs =="
    printf '%-28s %-22s %10s\n' program "hand-out entry" bytes
    for source in "$ROOT"/tests/programs/*.wf; do
        name=$(basename "$source" .wf)
        "$WFC" --par --stack-ledger -o /dev/null "$source" 2>/dev/null \
            | awk -v program="$name" \
                '$2 == "chain" && $3 ~ /^wf__par_thunk_/ { printf "%-28s %-22s %10s\n", program, $3, $4 }'
    done
}

sections=${*:-park compute lanes io chain stacks ledger spin}
prepare
for section in $sections; do
    case $section in
    park) section_park ;;
    spin) section_spin ;;
    compute) section_compute ;;
    lanes) section_lanes ;;
    io) section_io ;;
    chain) section_chain ;;
    stacks) section_stacks ;;
    ledger) section_ledger ;;
    *) echo "run.sh: unknown section $section" >&2; exit 2 ;;
    esac
done
