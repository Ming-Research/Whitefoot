#!/bin/sh
# Crafted tables exercise the actual reducer/verdict, without compiling or
# timing WF. The original fourteen decisions remain, with matrix and precision
# controls for defects found during extraction. Retire with this instrument.
set -eu
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-performance-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
cases=0
cpus=4
all="mandelbrot records fir quadrature stencil"
kernels=$all
# The paired table of the kernels in $1 (default all), every width unremarkable.
matrix() {
    awk -v names="${1:-$all}" 'BEGIN {n=split(names,k," ");for(i=1;i<=n;i++)for(w=1;w<=4;w*=2)print k[i],w,1,0,5,1}' > "$work/table"
}
row() {
    awk -v k="$1" -v w="$2" -v r="$3" -v n="$4" -v c="$5" \
        '{if($1==k && $2==w){$3=r;$4=n;$6=c}print}' "$work/table" > "$work/new"
    mv "$work/new" "$work/table"
}
expect() {
    name=$1; want=$2; pattern=$3; program=${4:-verdict}
    set +e
    awk -v cpus="$cpus" -v kernels="$kernels" -f "$here/$program.awk" "$work/table" > "$work/out" 2>&1
    status=$?
    set -e
    if [ "$status" -ne "$want" ] || ! grep -F -q "$pattern" "$work/out"; then
        echo "FAIL $name: status=$status expected=$want pattern=$pattern" >&2
        cat "$work/out" >&2
        exit 1
    fi
    cases=$((cases + 1))
    echo "PASS $name"
}
matrix
row fir 4 0.96 2 0.99
expect pass 0 'VERDICT: PASS'
matrix; row fir 2 0.94 4 0.995; row fir 4 0.948 5 0.991
expect two-block-fail 1 'VERDICT: FAIL'
matrix; row records 1 0.938 5 0.996
expect one-block-suspect 0 'suspect: records W=1'
matrix; row fir 1 0.94 4 0.995; row records 1 0.951 4 0.998
expect two-kernels-one-block-each 0 '2 suspect(s)'
matrix; row fir 2 0.9 2 1
expect wall-band-without-adverse-pairs 0 'VERDICT: PASS'
matrix; row fir 2 0.985 4 1
expect adverse-pairs-inside-band 0 'VERDICT: PASS'
printf '# no baseline rows\n' > "$work/table"
expect no-twin 2 'REFUSED: missing'
matrix; row fir 4 0.5 5 1; cpus=2
expect oversubscribed-excluded 0 'VERDICT: PASS'
cpus=4; matrix; printf 'fir 8 0.5 5 5 1\n' >> "$work/table"
expect unrecorded-width-ignored 0 'skipped 1 unrecorded/oversubscribed'
printf 'fir 8 0.5 5 5 1\n' > "$work/table"
expect only-unrecorded-widths 2 'REFUSED: missing'
matrix; sed 's/fir 2 1 0 5 1/fir 2 1 0 2 1/' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect under-powered 2 'underpowered paired row'
matrix; row fir 2 1 4 0.5
expect cpu-reported-never-failing 0 'cpu report: fir W=2'
matrix; awk '$2==1' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect one-readable-block-per-kernel 2 'REFUSED: missing mandelbrot W=2'
: > "$work/table"
expect empty-table 2 'REFUSED: missing'

matrix; awk '$1!="stencil"' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect missing-whole-kernel 2 'REFUSED: missing stencil'
matrix; head -1 "$work/table" >> "$work/table"
expect duplicate-width 2 'REFUSED: duplicate kernel/width'
matrix; row fir 1 0.96999999 4 1; row fir 2 0.96999999 4 1
expect unrounded-below-threshold 1 'VERDICT: FAIL'
matrix; row fir 1 0.97000001 4 1; row fir 2 0.97000001 4 1
expect unrounded-above-threshold 0 'VERDICT: PASS'
matrix; cpus=1
expect insufficient-host 2 'at least two available CPUs'
cpus=4

raw() {
    awk -v names="${1:-$all}" 'BEGIN {
        n=split(names,k," ")
        for(i=1;i<=n;i++)for(w=1;w<=4;w*=2)for(a=0;a<2;a++)for(p=0;p<5;p++)for(s=0;s<=5;s++) {
            t=(a && p<4 ? 1100 : 1000)+s
            if(s==0)t=9999999
            print k[i],a?"candidate":"baseline",w,p,s,t,t,17
        }
    }' > "$work/table"
}
raw
expect reduce-paired-process-medians 0 'mandelbrot' reduce
cp "$work/out" "$work/paired"
# Four slower candidate passes must survive both medians; warmup is ignored.
awk '$1=="mandelbrot" && $2==1 {if($3>0.911 || $3<0.909 || $4!=4 || $5!=5)exit 1;found=1}END{if(!found)exit 1}' "$work/paired"
raw; awk '$1!="stencil"' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect reduce-missing-kernel 2 'REFUSED: missing stencil' reduce
raw; awk '$2!="baseline"' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect reduce-missing-arm 2 'REFUSED: missing mandelbrot baseline' reduce
raw; awk '!($1=="fir" && $2=="candidate" && $3==4 && $4==2 && $5==3)' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect reduce-missing-sample 2 'REFUSED: missing fir candidate' reduce
raw; head -1 "$work/table" >> "$work/table"
expect reduce-duplicate-sample 2 'REFUSED: duplicate raw row' reduce
raw; awk 'NR==1{$8=18} {print}' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
expect reduce-output-extent 2 'REFUSED: inconsistent output extent' reduce
printf 'broken\n' > "$work/table"
expect reduce-malformed 2 'REFUSED: invalid raw row' reduce
# Match compare.sh's sequential status propagation: partial reducer output
# must never allow the verdict or a later tee/cat to replace the failure.
if sh -ec 'awk -v cpus=4 -v kernels="$3" -f "$1/reduce.awk" "$2/table" > "$2/paired"; touch "$2/verdict-ran"' sh "$here" "$work" "$all" 2>/dev/null; then
    echo 'FAIL reducer error was swallowed' >&2; exit 1
fi
test ! -e "$work/verdict-ran"
cases=$((cases + 1))

# The kernels are named by the caller, not by the scripts: an empty list, a
# repeated name or a name with no rows is refused, and a subset needs only its
# own matrix.
kernels=""; matrix
expect verdict-no-kernels 2 'REFUSED: no kernels named'
expect reduce-no-kernels 2 'REFUSED: no kernels named' reduce
kernels="fir fir"; matrix fir
expect verdict-repeated-kernel 2 'REFUSED: invalid or repeated kernel name fir'
kernels="mandelbrot bogus"; matrix mandelbrot
expect verdict-kernel-without-rows 2 'REFUSED: missing bogus W=1'
kernels="fir"; matrix fir; row fir 1 0.94 4 0.995; row fir 4 0.95 5 0.99
expect verdict-subset-fails 1 'VERDICT: FAIL'
kernels="fir"; matrix fir
expect verdict-subset-passes 0 'VERDICT: PASS'
kernels="fir"; matrix "fir stencil"
expect verdict-row-outside-subset 2 'REFUSED: invalid or underpowered paired row'
kernels="fir"; raw fir
expect reduce-subset 0 'fir' reduce
kernels=$all

# decide.awk: the null control's table, then the comparison's.
decide() {
    name=$1; want=$2; pattern=$3
    set +e
    awk -v cpus="$cpus" -v kernels="$kernels" -f "$here/decide.awk" "$work/null" "$work/table" > "$work/out" 2>&1
    status=$?
    set -e
    if [ "$status" -ne "$want" ] || ! grep -F -q "$pattern" "$work/out"; then
        echo "FAIL $name: status=$status expected=$want pattern=$pattern" >&2
        cat "$work/out" >&2
        exit 1
    fi
    cases=$((cases + 1))
    echo "PASS $name"
}
matrix; cp "$work/table" "$work/null"; row fir 1 0.94 4 1; row fir 2 0.95 5 1
decide decide-fails-on-a-clean-null 0 'decide: fir FAIL comparison=2 null=0'
matrix; row fir 4 0.95 4 1; cp "$work/table" "$work/null"; matrix; row fir 1 0.94 4 1; row fir 2 0.95 5 1
decide decide-indeterminate-on-an-adverse-null 0 'decide: fir indeterminate comparison=2 null=1'
matrix; row fir 4 0.5 3 1; cp "$work/table" "$work/null"; matrix; row fir 1 0.94 4 1; row fir 2 0.95 5 1
decide decide-null-without-adverse-pairs-is-clean 0 'decide: fir FAIL comparison=2 null=0'
matrix; row fir 4 0.95 4 1; cp "$work/table" "$work/null"; matrix; row fir 1 0.94 4 1
decide decide-one-adverse-width-passes 0 'decide: fir pass comparison=1 null=1'
matrix; cp "$work/table" "$work/null"
decide decide-quiet-passes 0 'decide: mandelbrot pass comparison=0 null=0'
# Two CPUs leave out width 4: an adverse width 4 in the null control is not read.
cpus=2; kernels="fir"; matrix fir; row fir 4 0.95 4 1; cp "$work/table" "$work/null"; matrix fir; row fir 1 0.94 4 1; row fir 2 0.95 5 1
decide decide-widths-follow-the-cpus 0 'decide: fir FAIL comparison=2 null=0'
cpus=4; kernels=$all
matrix; cp "$work/table" "$work/null"; awk '$1!="stencil"' "$work/table" > "$work/new"; mv "$work/new" "$work/table"
decide decide-missing-kernel 2 'REFUSED: missing stencil W=1 in table 2'
matrix; cp "$work/table" "$work/null"; : > "$work/table"
decide decide-missing-comparison-table 2 'REFUSED: two tables expected'
matrix; cp "$work/table" "$work/null"; head -1 "$work/table" >> "$work/table"
decide decide-duplicate-width 2 'REFUSED: duplicate kernel/width'
kernels=""; matrix; cp "$work/table" "$work/null"
decide decide-no-kernels 2 'REFUSED: no kernels named'
kernels=$all

# judge.sh, with a stub for the measuring command and for the stage labels. The
# stub answers the nth measurement with the table the test put as tables/n, and
# records the kernels it was asked for (calls) and every stage label (labels).
judge_dir=$work/j
cat > "$work/stub-measure.sh" <<'STUB'
#!/bin/sh
j=$WF_TEST_J
n=$(( $(cat "$j/count") + 1 ))
echo "$n" > "$j/count"
echo "$n KERNELS=$KERNELS" >> "$j/calls"
mkdir -p "$3"
cp "$j/tables/$n" "$3/paired.tsv"
exit "$(cat "$j/tables/$n.status" 2>/dev/null || echo 0)"
STUB
cat > "$work/stub-check.sh" <<'STUB'
#!/bin/sh
echo "$1" >> "$WF_TEST_J/labels"
shift
exec "$@"
STUB
printf 'processor\t: 0\nmodel name\t: Test CPU 9000\n' > "$work/cpuinfo"
# judge_setup NAME...: the candidate's images of these kernels differ from the baseline's.
judge_setup() {
    rm -rf "$judge_dir"; mkdir -p "$judge_dir/base" "$judge_dir/cand" "$judge_dir/tables"
    for k in $all; do echo "image of $k" > "$judge_dir/base/$k"; cp "$judge_dir/base/$k" "$judge_dir/cand/$k"; done
    for k in "$@"; do echo "changed $k" > "$judge_dir/cand/$k"; done
    echo 0 > "$judge_dir/count"; : > "$judge_dir/calls"; : > "$judge_dir/labels"
}
# put N KERNELS: table N of a quiet matrix of these kernels, then edited by row.
put() { matrix "$2"; }
save() { cp "$work/table" "$judge_dir/tables/$1"; }
judge_run() {
    name=$1; want=$2; pattern=$3
    set +e
    WF_PERF_CPUS=4 WF_PERF_MEASURE="sh $work/stub-measure.sh" WF_PERF_RUN_CHECK="sh $work/stub-check.sh" \
        WF_TEST_J="$judge_dir" WHITEFOOT_CPUINFO="$work/cpuinfo" KERNELS="$all" \
        bash "$here/judge.sh" "$judge_dir/base" "$judge_dir/cand" "$judge_dir/out" > "$work/out" 2>&1
    status=$?
    set -e
    if [ "$status" -ne "$want" ] || ! grep -F -q "$pattern" "$work/out"; then
        echo "FAIL $name: status=$status expected=$want pattern=$pattern" >&2
        cat "$work/out" >&2
        exit 1
    fi
}
judge_pass() { cases=$((cases + 1)); echo "PASS $1"; }
# In the order of the stub's tables: 1 null, 2 comparison; 3 retest null, 4 retest comparison.
adverse() { row "$1" 1 0.94 4 1; row "$1" 2 0.95 5 1; }
labels_are() { [ "$(tr '\n' ' ' < "$judge_dir/labels" | sed 's/ *$//')" = "$1" ]; }

judge_setup
judge_run judge-identical-images-are-not-timed 0 'VERDICT: PASS -- 5 image(s) identical, 0 timed'
labels_are '' || { echo 'FAIL judge-identical-images-are-not-timed: a stage ran' >&2; exit 1; }
test "$(wc -l < "$judge_dir/out/identical.txt" | tr -d ' ')" = 5
grep -F -q 'cpu model: Test CPU 9000' "$work/out"
grep -F -q 'stencil: identical image, not timed, pass' "$work/out"
judge_pass judge-identical-images-are-not-timed

# The comparison of run 38389055091's stencil when its code is not the same:
# 0.9076 and 0.9421 with four of five pairs, and a clean null control.
judge_setup stencil
put 1 stencil; save 1
put 2 stencil; row stencil 1 0.907591 4 0.9; row stencil 2 0.942127 4 0.93; row stencil 4 1.05207 2 1.05; save 2
echo 1 > "$judge_dir/tables/2.status"
judge_run judge-a-changed-kernel-is-timed-and-fails 1 'VERDICT: FAIL -- stencil (round 1)'
labels_are 'performance-null performance-comparison' || { echo 'FAIL judge-a-changed-kernel: stages' >&2; exit 1; }
grep -F -q '2 KERNELS=stencil' "$judge_dir/calls"
grep -F -q 'mandelbrot: identical image, not timed, pass' "$work/out"
judge_pass judge-a-changed-kernel-is-timed-and-fails

# The same kernel 10% slower than the unchanged one still fails, 0.825 and 0.857.
judge_setup stencil
put 1 stencil; save 1
put 2 stencil; row stencil 1 0.825 5 0.82; row stencil 2 0.857 5 0.85; save 2
judge_run judge-a-changed-kernel-10-percent-slower-fails 1 'VERDICT: FAIL -- stencil (round 1)'
judge_pass judge-a-changed-kernel-10-percent-slower-fails

judge_setup stencil
put 1 stencil; save 1
put 2 stencil; save 2
judge_run judge-a-changed-kernel-that-is-quiet-passes 0 'VERDICT: PASS -- 4 image(s) identical, 1 timed'
judge_pass judge-a-changed-kernel-that-is-quiet-passes

# An adverse null control with an adverse comparison is no verdict: the kernel is
# measured once more, alone, and the retest decides.
retest_tables() {
    put 1 stencil; row stencil 4 0.95 4 1; save 1
    put 2 stencil; adverse stencil; save 2
}
judge_setup stencil; retest_tables
put 3 stencil; save 3
put 4 stencil; save 4
judge_run judge-indeterminate-retest-passes 0 'VERDICT: PASS'
labels_are 'performance-null performance-comparison performance-retest-null performance-retest-comparison' || { echo 'FAIL judge-indeterminate-retest-passes: stages' >&2; exit 1; }
grep -F -q '3 KERNELS=stencil' "$judge_dir/calls"
grep -F -q 'round 1: stencil indeterminate' "$work/out"
judge_pass judge-indeterminate-retest-passes

judge_setup stencil; retest_tables
put 3 stencil; save 3
put 4 stencil; adverse stencil; save 4
judge_run judge-retest-fails-with-a-clean-null 1 'VERDICT: FAIL -- stencil (round 2)'
judge_pass judge-retest-fails-with-a-clean-null

judge_setup stencil; retest_tables
put 3 stencil; row stencil 1 0.96 4 1; save 3
put 4 stencil; adverse stencil; save 4
judge_run judge-still-indeterminate-is-inconclusive 3 'VERDICT: INCONCLUSIVE -- stencil still indeterminate after the retest'
grep -F -q '::warning::INCONCLUSIVE: stencil' "$work/out"
judge_pass judge-still-indeterminate-is-inconclusive

# A kernel that failed decides the job: nothing is retested, and a failure in the
# retest outranks a kernel that stays indeterminate.
judge_setup records stencil
put 1 "records stencil"; row stencil 4 0.95 4 1; save 1
put 2 "records stencil"; adverse records; adverse stencil; save 2
judge_run judge-a-failure-outranks-an-indeterminate-kernel 1 'VERDICT: FAIL -- records (round 1)'
grep -F -q 'round 1: stencil indeterminate' "$work/out"
test "$(cat "$judge_dir/count")" = 2
judge_pass judge-a-failure-outranks-an-indeterminate-kernel

judge_setup records stencil
put 1 "records stencil"; row records 4 0.95 4 1; row stencil 4 0.95 4 1; save 1
put 2 "records stencil"; adverse records; adverse stencil; save 2
put 3 "records stencil"; row stencil 4 0.95 4 1; save 3
put 4 "records stencil"; adverse records; adverse stencil; save 4
judge_run judge-a-retest-failure-outranks-an-indeterminate-kernel 1 'VERDICT: FAIL -- records (round 2)'
grep -F -q '3 KERNELS=records stencil' "$judge_dir/calls"
grep -F -q 'round 2: stencil indeterminate' "$work/out"
judge_pass judge-a-retest-failure-outranks-an-indeterminate-kernel

# A measurement that did not run is refused, not read as a pass.
judge_setup stencil
put 1 stencil; save 1; echo 2 > "$judge_dir/tables/1.status"
judge_run judge-a-measurement-that-did-not-run-is-refused 2 'REFUSED: the null measurement of round 1 did not run'
judge_pass judge-a-measurement-that-did-not-run-is-refused

judge_setup stencil
mkdir -p "$judge_dir/out"
judge_run judge-results-directory-must-be-fresh 2 'results must be a fresh directory'
judge_pass judge-results-directory-must-be-fresh

fail() {
    echo "FAIL $1" >&2
    cat "$work/out" >&2
    exit 1
}
pass() {
    cases=$((cases + 1))
    echo "PASS $1"
}

# The mapping record: layout.awk's calls, sorted, compared as data-placement.sh
# record compares two launches.
layout_calls() {
    set +e
    awk -f "$here/layout.awk" "$1" > "$work/out" 2>&1
    status=$?
    set -e
    LC_ALL=C sort "$work/out" > "$2"
}
expect_layout() {
    layout_calls "$2" "$work/first"; first=$status
    layout_calls "$3" "$work/second"
    [ "$first" -eq 0 ] && [ "$status" -eq 0 ] || fail "$1: a trace was refused"
    if cmp -s "$work/first" "$work/second"; then got=same; else got=differ; fi
    [ "$got" = "$4" ] || { cat "$work/first" "$work/second" > "$work/out"; fail "$1: $got, expected $4"; }
    pass "$1"
}
expect_layout_refused() {
    layout_calls "$2" "$work/first"
    [ "$status" -eq 2 ] && grep -F -q "$3" "$work/out" || fail "$1: status=$status pattern=$3"
    pass "$1"
}
cat > "$work/trace-a" <<'EOF'
4001  mmap(NULL, 8192, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x7ffff7fbd000
4001  mmap(NULL, 1073745920, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK, -1, 0) = 0x7fffb7dff000
4002  mmap(NULL, 134217728, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_NORESERVE, -1, 0 <unfinished ...>
4001  munmap(0x7ffff7fbd000, 8192 <unfinished ...>
4002  <... mmap resumed> )                 = 0x7fffa8000000
4001  <... munmap resumed>) = 0
4002  munmap(0x7fffac000000, 67108864) = 0
4002  mremap(0x7fffa7000000, 33558528, 67112960, MREMAP_MAYMOVE) = 0x7fff9c000000
4001  --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=4003} ---
4002  +++ exited with 0 +++
4001  +++ exited with 0 +++
EOF
cat > "$work/trace-b" <<'EOF'
913  mmap(NULL, 8192, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x7ffff7fbd000
913  mmap(NULL, 1073745920, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK, -1, 0) = 0x7fffb7dff000
913  munmap(0x7ffff7fbd000, 8192) = 0
120944 mmap(NULL, 134217728, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_NORESERVE, -1, 0) = 0x7fffa8000000
120944 munmap(0x7fffac000000, 67108864 <unfinished ...>
913  +++ exited with 0 +++
120944 <... munmap resumed>) = 0
120944 mremap(0x7fffa7000000, 33558528, 67112960, MREMAP_MAYMOVE) = 0x7fff9c000000
120944 +++ exited with 0 +++
EOF
expect_layout layout-ignores-pids-and-interleaving "$work/trace-a" "$work/trace-b" same
{ cat "$work/trace-a"; echo '4002  mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x7ffff7fbc000'; } > "$work/trace-c"
expect_layout layout-one-more-mapping "$work/trace-a" "$work/trace-c" differ
sed 's/= 0x7fffa8000000/= 0x7fffa6000000/' "$work/trace-a" > "$work/trace-c"
expect_layout layout-same-call-elsewhere "$work/trace-a" "$work/trace-c" differ
: > "$work/trace-c"
expect_layout_refused layout-empty-trace "$work/trace-c" 'REFUSED: no mapping call'
{ cat "$work/trace-a"; echo 'garbled line'; } > "$work/trace-c"
expect_layout_refused layout-unreadable-line "$work/trace-c" 'REFUSED: unreadable trace line'
{ cat "$work/trace-a"; echo '4002  <... munmap resumed>) = 0'; } > "$work/trace-c"
expect_layout_refused layout-resumed-without-start "$work/trace-c" 'REFUSED: unreadable trace line'

# Two arms' 32 positions in order of time (positions.awk). An arm is given as
# FIRST-LAST:FACTOR,...: each position's time is 1000 ns times its factor.
placement_layout() {
    : > "$work/layout"
    for kernel in $kernels; do
        state=alike
        for differ in "$@"; do if [ "$kernel" = "$differ" ]; then state=differ; fi; done
        echo "$kernel $state" >> "$work/layout"
    done
    : > "$work/rows"
}
placement_rows() {
    awk -v kernel="$1" -v round="$2" -v baseline="$3" -v candidate="$4" '
        function factor(spec, position,   n, i, part, range, bounds) {
            n = split(spec, part, ",")
            for (i = 1; i <= n; i++) {
                split(part[i], range, ":")
                if (split(range[1], bounds, "-") == 1) bounds[2] = bounds[1]
                if (position >= bounds[1] + 0 && position <= bounds[2] + 0) return range[2] + 0
            }
            return 1
        }
        BEGIN {
            for (p = 0; p < 32; p++) {
                printf "%s baseline %d %d %.10g\n", kernel, p, round, 1000 * factor(baseline, p)
                printf "%s candidate %d %d %.10g\n", kernel, p, round, 1000 * factor(candidate, p)
            }
        }' >> "$work/rows"
}
positions() {
    set +e
    awk -v stage="$1" -v kernels="$kernels" -f "$here/positions.awk" "$work/layout" "$work/rows" > "$work/out" 2>&1
    status=$?
    set -e
}
# NAME STAGE STATUS OUTPUT: OUTPUT is a printf format, the whole output expected.
expect_positions() {
    positions "$2"
    printf "$4" > "$work/want"
    [ "$status" -eq "$3" ] && cmp -s "$work/want" "$work/out" ||
        fail "$1: status=$status expected=$3 output=$4"
    pass "$1"
}
expect_positions_match() {
    positions "$2"
    [ "$status" -eq "$3" ] && grep -F -q "$4" "$work/out" || fail "$1: status=$status expected=$3 pattern=$4"
    pass "$1"
}
placement_layout
expect_positions every-kernel-maps-alike first 0 ''
placement_layout stencil; placement_rows stencil 1 '' ''
expect_positions flat-arms first 0 ''
placement_layout stencil; placement_rows stencil 1 30:1.3 30:1.3
expect_positions slow-at-one-position-in-both first 0 ''
placement_layout stencil; placement_rows stencil 1 30:1.3 29:1.3
expect_positions profile-moved-by-one-position first 0 ''
placement_layout stencil; placement_rows stencil 1 0-31:1 0-31:1.1
expect_positions candidate-slower-everywhere first 0 ''
placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3
expect_positions slow-at-other-positions first 0 'stencil again\n'
placement_layout stencil; placement_rows stencil 1 '' 5:1.2
expect_positions one-slow-launch first 0 'stencil again\n'
placement_layout stencil; placement_rows stencil 1 5:1.15 ''
expect_positions one-slow-launch-of-the-baseline first 0 'stencil again\n'
placement_layout stencil; placement_rows stencil 1 '' 7:1.0999999
expect_positions unrounded-below-the-line first 0 ''
placement_layout stencil; placement_rows stencil 1 '' 7:1.1000001
expect_positions unrounded-above-the-line first 0 'stencil again\n'
placement_layout fir stencil; placement_rows fir 1 '' ''; placement_rows stencil 1 30:1.3 6-20:1.3
expect_positions only-the-unlike-kernel-again first 0 'stencil again\n'

placement_layout stencil
expect_positions_match differing-kernel-without-rows first 2 'REFUSED: stencil baseline round 1 lacks position 0'
placement_layout; placement_rows stencil 1 '' ''
expect_positions_match rows-of-a-kernel-that-maps-alike first 2 'REFUSED: stencil maps alike but has a row'
placement_layout stencil; placement_rows stencil 1 '' ''
grep -v '^fir ' "$work/layout" > "$work/new"; mv "$work/new" "$work/layout"
expect_positions_match layout-lacks-a-kernel first 2 'REFUSED: the layout lacks fir'
placement_layout stencil; placement_rows stencil 1 '' ''; echo 'sorting differ' >> "$work/layout"
expect_positions_match layout-names-an-unknown-kernel first 2 'REFUSED: unreadable layout line 6'
placement_layout stencil; placement_rows stencil 1 '' ''; echo 'fir alike' >> "$work/layout"
expect_positions_match layout-names-a-kernel-twice first 2 'REFUSED: the layout names fir twice'
placement_layout stencil; placement_rows stencil 1 '' ''
grep -v '^stencil candidate 17 ' "$work/rows" > "$work/new"; mv "$work/new" "$work/rows"
expect_positions_match missing-position first 2 'REFUSED: stencil candidate round 1 lacks position 17'
placement_layout stencil; placement_rows stencil 1 '' ''
grep '^stencil baseline 3 ' "$work/rows" > "$work/new"; cat "$work/new" >> "$work/rows"
expect_positions_match duplicate-position first 2 'REFUSED: duplicate row stencil baseline 3 1'
placement_layout stencil; placement_rows stencil 1 '' ''
grep -v '^stencil candidate ' "$work/rows" > "$work/new"; mv "$work/new" "$work/rows"
expect_positions_match missing-arm first 2 'REFUSED: stencil candidate round 1 lacks position 0'
placement_layout stencil; placement_rows stencil 1 '' ''; echo 'stencil candidate 4 1 0' >> "$work/rows"
expect_positions_match nonpositive-time first 2 'REFUSED: unreadable row'
placement_layout stencil; placement_rows stencil 1 '' ''; placement_rows stencil 2 '' ''
expect_positions_match second-round-before-the-first-is-read first 2 'REFUSED: a second round before the first is read'
placement_layout stencil; placement_rows stencil 1 '' ''
expect_positions_match unknown-stage none 2 'REFUSED: stage must be first, final or aside'
# The kernels are the caller's, as for reduce.awk and verdict.awk: none is
# refused, and a layout of the kernels named is read whatever they are.
kernels=""; placement_layout
expect_positions_match positions-no-kernels first 2 'REFUSED: no kernels named'
kernels="fir stencil"; placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3
expect_positions positions-of-the-kernels-named first 0 'stencil again\n'
kernels=$all

placement_layout stencil
placement_rows stencil 1 30:1.3 6-20:1.3; placement_rows stencil 2 30:1.25 6-20:1.35
expect_positions_match unlike-in-both-rounds final 0 'ASIDE: stencil'
grep -q '^stencil  *aside ' "$work/out" || fail 'unlike-in-both-rounds names stencil aside'
test "$(awk '$1=="stencil" && ($2=="round") && NF==39' "$work/out" | wc -l)" -eq 4 ||
    fail 'unlike-in-both-rounds prints four series of 32 quotients'
expect_positions unlike-in-both-rounds-aside aside 0 'stencil\n'
placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3; placement_rows stencil 2 '' ''
expect_positions_match unlike-once final 0 'ASIDE: none'
grep -q '^stencil  *judged ' "$work/out" || fail 'unlike-once judges stencil'
! grep -q '^stencil round ' "$work/out" || fail 'unlike-once prints no series'
expect_positions unlike-once-aside aside 0 '\n'
placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3; placement_rows stencil 2 6-20:1.3 30:1.3
expect_positions_match unlike-in-opposite-directions final 0 'ASIDE: none'
grep -q '^stencil  *judged ' "$work/out" || fail 'unlike-in-opposite-directions judges stencil'
placement_layout fir stencil; placement_rows fir 1 '' ''; placement_rows stencil 1 30:1.3 6-20:1.3
placement_rows stencil 2 30:1.3 6-20:1.3
expect_positions_match judged-alike-and-aside final 0 'ASIDE: stencil'
grep -q '^fir  *judged ' "$work/out" && grep -q '^mandelbrot  *alike$' "$work/out" ||
    fail 'judged-alike-and-aside reads every kernel'
placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3; placement_rows stencil 2 30:1.3 6-20:1.3
grep -v '^stencil baseline 9 2 ' "$work/rows" > "$work/new"; mv "$work/new" "$work/rows"
expect_positions_match second-round-lacks-a-row final 2 'REFUSED: stencil baseline round 2 lacks position 9'
placement_layout fir stencil; placement_rows fir 1 '' ''; placement_rows stencil 1 30:1.3 6-20:1.3
placement_rows stencil 2 30:1.3 6-20:1.3; placement_rows fir 2 '' ''
expect_positions_match second-round-of-another-kernel final 2 'REFUSED: fir has a second round but was not launched again'
placement_layout stencil; placement_rows stencil 1 30:1.3 6-20:1.3
expect_positions_match second-round-missing aside 2 'REFUSED: stencil baseline round 2 lacks position 0'

# The data-placement control end to end: data-placement.sh on stand-in images
# under stand-in strace, setarch and run-check.pl, so nothing is compiled,
# traced or timed. The stand-in image writes its trace and its times from files
# beside it: TRACE, PROFILE lines `FIRST LAST PERCENT` of positions and SLOW a
# percent at every position. Its six samples are 5000, 105, 101, 103, 102
# and 104 percent of the rest, so a launch's median of samples 1 to 5 is 103.
# The expectations are the plan's rule (README.md, "Data placement"): a kernel
# is launched at the 32 positions only when its arms map differently and set
# aside when two rounds are unlike in one direction; each record and each
# kernel's round is a stage of its own budget row. A qualification that passes
# is left out: its five rounds would double this instrument's time.
if [ "$(uname -s)" = Linux ]; then
dp=$work/dp
mkdir -p "$dp/tree/tests/performance" "$dp/tree/.github" "$dp/bin"
cp "$here/data-placement.sh" "$here/positions.awk" "$here/layout.awk" "$here/Makefile" "$dp/tree/tests/performance/"
cat > "$dp/tree/.github/run-check.pl" <<'EOF'
my $label = shift @ARGV;
open my $log, '>>', $ENV{FAKE_STAGES} or die "stage log: $!\n";
print $log "$label\n";
close $log;
exec { $ARGV[0] } @ARGV or die "execute $ARGV[0]: $!\n";
EOF
cat > "$dp/bin/setarch" <<'EOF'
#!/bin/sh
[ -n "$1" ] && [ "$2" = -R ] || { echo "stand-in setarch: $*" >&2; exit 3; }
shift 2
exec "$@"
EOF
cat > "$dp/bin/strace" <<'EOF'
#!/bin/sh
follow= out= traced=
while [ $# -gt 0 ]; do
    case $1 in
        -f) follow=1; shift ;;
        -e) traced=$2; shift 2 ;;
        -o) out=$2; shift 2 ;;
        -*) echo "stand-in strace: option $1" >&2; exit 3 ;;
        *) break ;;
    esac
done
[ -n "$follow" ] && [ -n "$out" ] && [ "$traced" = trace=mmap,munmap,mremap ] ||
    { echo 'stand-in strace: -f, -o FILE and the mapping calls are required' >&2; exit 3; }
FAKE_STRACE_OUT=$out
export FAKE_STRACE_OUT
exec "$@"
EOF
cat > "$dp/image" <<'EOF'
#!/bin/sh
dir=${0%/*}
kernel=${0##*/}
if [ -n "${WF_SPLIT_WORK+set}" ]; then echo 'stand-in image: WF_SPLIT_WORK is set' >&2; exit 3; fi
position=${WF_PERFORMANCE_POSITION:-0}
case ${1:-} in
verify)
    echo "$kernel oracle PASS: compared=7" ;;
layout)
    [ $# -eq 2 ] && [ "${WF_WORKERS:-}" = "$2" ] && [ -n "${FAKE_STRACE_OUT:-}" ] ||
        { echo 'stand-in image: layout needs its width, WF_WORKERS and a trace' >&2; exit 3; }
    {
        if [ "$position" != 0 ]; then
            echo "4001  mmap(NULL, $((position * 2097152)), PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_NORESERVE, -1, 0) = 0x7fff00000000"
        fi
        cat "$dir/$kernel.trace"
    } > "$FAKE_STRACE_OUT" ;;
measure)
    [ $# -eq 4 ] && [ "${WF_WORKERS:-}" = "$3" ] || { echo "stand-in image: measure $*" >&2; exit 3; }
    percent=100
    profile=$dir/$kernel.profile
    if [ -f "$profile" ]; then
        while read -r first last factor; do
            if [ "$position" -ge "$first" ] && [ "$position" -le "$last" ]; then percent=$factor; fi
        done < "$profile"
    fi
    slow=100
    if [ -f "$dir/$kernel.slow" ]; then read -r slow < "$dir/$kernel.slow"; fi
    sample=0
    for share in 5000 105 101 103 102 104; do
        wall=$((slow * percent * share))
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t7\n' "$kernel" "$2" "$3" "$4" "$sample" "$wall" "$wall"
        sample=$((sample + 1))
    done ;;
*)
    echo "stand-in image: $*" >&2; exit 3 ;;
esac
EOF
chmod +x "$dp/bin/setarch" "$dp/bin/strace" "$dp/image"
cat > "$dp/trace" <<'EOF'
4001  mmap(NULL, 8192, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x7ffff7fbd000
4001  mmap(NULL, 1073745920, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK, -1, 0) = 0x7fffb7dff000
4002  mmap(NULL, 134217728, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_NORESERVE, -1, 0) = 0x7fffa8000000
4002  +++ exited with 0 +++
4001  +++ exited with 0 +++
EOF
thread='4001  mmap(NULL, 8392704, PROT_NONE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_STACK, -1, 0) = 0x7fffb75fe000'
dp_images() {
    mkdir -p "$1"
    for kernel in mandelbrot records fir quadrature stencil; do
        cp "$dp/image" "$1/$kernel"
        cp "$dp/trace" "$1/$kernel.trace"
    done
}
# B: the merge base, stencil slow at positions 6 to 20. S: one more mapping in
# fir and stencil, stencil slow at position 30 and 20 percent slower everywhere.
# A: S's times with B's mappings.
dp_images "$dp/b"; echo '6 20 130' > "$dp/b/stencil.profile"
dp_images "$dp/s"; echo '30 30 130' > "$dp/s/stencil.profile"; echo 120 > "$dp/s/stencil.slow"
echo "$thread" >> "$dp/s/fir.trace"; echo "$thread" >> "$dp/s/stencil.trace"
dp_images "$dp/a"; echo '30 30 130' > "$dp/a/stencil.profile"; echo 120 > "$dp/a/stencil.slow"
# NAME STATUS COMMAND...: the command under the stand-ins, its output in $work/out.
dp_run() {
    name=$1; want=$2; shift 2
    : > "$dp/stages"
    set +e
    FAKE_STAGES=$dp/stages PATH="$dp/bin:$PATH" WF_SPLIT_WORK=1 "$@" > "$work/out" 2>&1
    status=$?
    set -e
    [ "$status" -eq "$want" ] || fail "$name: status=$status expected=$want"
}
# NAME FILE FORMAT: FILE holds exactly the printf FORMAT.
dp_file() {
    printf "$3" > "$work/want"
    cmp -s "$work/want" "$2" || { cat "$2" >> "$work/out"; fail "$1: $2"; }
}
dp_has() {
    grep -E -q "$3" "$2" || { cat "$2" >> "$work/out"; fail "$1: $2 lacks $3"; }
}
control="$dp/tree/tests/performance/data-placement.sh"

dp_run control-sets-aside-the-placed-kernel 0 bash "$control" control "$dp/b" "$dp/s" "$dp/data-s"
dp_file control-sets-aside-the-placed-kernel "$dp/data-s/layout.txt" \
    'mandelbrot alike\nrecords alike\nfir differ\nquadrature alike\nstencil differ\n'
dp_file control-sets-aside-the-placed-kernel "$dp/data-s/aside.txt" 'stencil\n'
dp_file control-sets-aside-the-placed-kernel "$dp/stages" \
    'performance-data-layout\nperformance-data-round-fir\nperformance-data-round-stencil\nperformance-data-round-stencil\n'
dp_has control-sets-aside-the-placed-kernel "$dp/data-s/verdict.txt" '^fir +judged round 1: 0 unlike'
dp_has control-sets-aside-the-placed-kernel "$dp/data-s/verdict.txt" '^stencil +aside '
dp_has control-sets-aside-the-placed-kernel "$dp/data-s/verdict.txt" '^ASIDE: stencil$'
test "$(grep -c -E '^stencil round [12] (baseline|candidate) median ' "$dp/data-s/verdict.txt")" -eq 4 ||
    fail 'control-sets-aside-the-placed-kernel: four series of stencil quotients'
test "$(wc -l < "$dp/data-s/rows.tsv")" -eq 192 || fail 'control-sets-aside-the-placed-kernel: 192 rows'
dp_has control-sets-aside-the-placed-kernel "$dp/data-s/rows.tsv" '^stencil baseline 0 1 1030000$'
dp_has control-sets-aside-the-placed-kernel "$dp/data-s/rows.tsv" '^stencil candidate 30 2 1606800$'
pass control-sets-aside-the-placed-kernel

dp_run control-launches-nothing-for-alike-mappings 0 bash "$control" control "$dp/b" "$dp/a" "$dp/data-a"
dp_file control-launches-nothing-for-alike-mappings "$dp/data-a/aside.txt" '\n'
dp_file control-launches-nothing-for-alike-mappings "$dp/stages" 'performance-data-layout\n'
dp_has control-launches-nothing-for-alike-mappings "$dp/data-a/verdict.txt" '^ASIDE: none$'
pass control-launches-nothing-for-alike-mappings

# The kernels are the Makefile's, as compare.sh and judge.sh read them, and
# KERNELS in the environment names fewer.
dp_run control-records-the-kernels-named 0 env KERNELS='fir stencil' bash "$control" control "$dp/b" "$dp/a" "$dp/data-named"
dp_file control-records-the-kernels-named "$dp/data-named/layout.txt" 'fir alike\nstencil alike\n'
pass control-records-the-kernels-named

# The stand-in image at a position adds only the runner's own idle mapping and
# leaves every later mapping where it was, so its records at position 1 differ
# by that one call alone. qualify must then fail before it launches a round: a
# record that sees only the extra call would not see a startup that maps the
# same calls in another order.
dp_run qualify-needs-the-later-mappings-moved 1 bash "$control" qualify "$dp/b" "$dp/qualify-b"
dp_has qualify-needs-the-later-mappings-moved "$dp/qualify-b/verdict.txt" '^PASS: every kernel maps alike$'
dp_has qualify-needs-the-later-mappings-moved "$dp/qualify-b/verdict.txt" \
    '^FAIL: with the candidate arm at position 1, 5 kernel\(s\) lack the idle mapping or a later call elsewhere$'
dp_has qualify-needs-the-later-mappings-moved "$dp/qualify-b/verdict.txt" \
    '^the 32 positions were not launched: a record check failed$'
dp_file qualify-needs-the-later-mappings-moved "$dp/stages" 'performance-data-layout\nperformance-data-layout\n'
pass qualify-needs-the-later-mappings-moved
else
    echo 'SKIP the data-placement control end to end: it runs on Linux only'
fi
echo "performance instrument: $cases cases PASS"
