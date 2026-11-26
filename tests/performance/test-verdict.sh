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

echo "performance instrument: $cases cases PASS"
