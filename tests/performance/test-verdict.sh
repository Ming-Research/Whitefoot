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
matrix() {
    awk 'BEGIN {split("mandelbrot records fir quadrature stencil",k);for(i=1;i<=5;i++)for(w=1;w<=4;w*=2)print k[i],w,1,0,5,1}' > "$work/table"
}
row() {
    awk -v k="$1" -v w="$2" -v r="$3" -v n="$4" -v c="$5" \
        '{if($1==k && $2==w){$3=r;$4=n;$6=c}print}' "$work/table" > "$work/new"
    mv "$work/new" "$work/table"
}
expect() {
    name=$1; want=$2; pattern=$3; program=${4:-verdict}
    set +e
    awk -v cpus="$cpus" -f "$here/$program.awk" "$work/table" > "$work/out" 2>&1
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
    awk 'BEGIN {
        split("mandelbrot records fir quadrature stencil",k)
        for(i=1;i<=5;i++)for(w=1;w<=4;w*=2)for(a=0;a<2;a++)for(p=0;p<5;p++)for(s=0;s<=5;s++) {
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
if sh -ec 'awk -v cpus=4 -f "$1/reduce.awk" "$2/table" > "$2/paired"; touch "$2/verdict-ran"' sh "$here" "$work" 2>/dev/null; then
    echo 'FAIL reducer error was swallowed' >&2; exit 1
fi
test ! -e "$work/verdict-ran"
cases=$((cases + 1))

fail() {
    echo "FAIL $1" >&2
    cat "$work/out" >&2
    exit 1
}
pass() {
    cases=$((cases + 1))
    echo "PASS $1"
}

# A kernel the data-placement control sets aside is not judged; the others are.
expect_aside() {
    name=$1; list=$2; want=$3; pattern=$4
    set +e
    awk -v cpus="$cpus" -v aside="$list" -f "$here/verdict.awk" "$work/table" > "$work/out" 2>&1
    status=$?
    set -e
    [ "$status" -eq "$want" ] && grep -F -q "$pattern" "$work/out" ||
        fail "$name: status=$status expected=$want pattern=$pattern"
    pass "$name"
}
matrix; row stencil 1 0.9 5 1; row stencil 2 0.9 5 1
expect_aside set-aside-kernel-passes 'stencil' 0 \
    'VERDICT: PASS -- 0 kernel(s) adverse at two widths; 1 kernel(s) not judged (data placement)'
grep -F -q 'not judged: stencil (data placement)' "$work/out" || fail 'set-aside-kernel-line'
test "$(grep -c '^stencil .* not judged (data placement)$' "$work/out")" -eq 3 || fail 'set-aside-kernel-rows'
matrix; row stencil 1 0.9 5 0.5
expect_aside set-aside-suspect-reports-nothing 'stencil' 0 '0 suspect(s); 0 cpu report(s)'
row stencil 2 0.9 5 1; row fir 1 0.9 5 1; row fir 2 0.9 5 1
expect_aside judged-kernel-still-fails 'stencil' 1 \
    'VERDICT: FAIL -- 1 kernel(s) adverse at two widths; 1 kernel(s) not judged (data placement)'
expect_aside unknown-kernel-set-aside 'stencil sorting' 2 'REFUSED: unknown kernel set aside: sorting'
expect_aside kernel-set-aside-twice 'stencil stencil' 2 'REFUSED: kernel set aside twice: stencil'
matrix; row records 1 0.938 5 0.996; row fir 2 1 4 0.5
awk -v cpus="$cpus" -f "$here/verdict.awk" "$work/table" > "$work/want" 2>&1
expect_aside empty-aside-list '' 0 'suspect: records W=1'
cmp -s "$work/want" "$work/out" || fail 'an empty list changes the verdict'

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
    for kernel in mandelbrot records fir quadrature stencil; do
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
    awk -v stage="$1" -f "$here/positions.awk" "$work/layout" "$work/rows" > "$work/out" 2>&1
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
placement_layout stencil; placement_rows stencil 1 '' ''; grep '^stencil baseline 3 ' "$work/rows" >> "$work/rows"
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
echo "performance instrument: $cases cases PASS"
