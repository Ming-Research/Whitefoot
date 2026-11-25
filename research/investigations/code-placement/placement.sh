#!/usr/bin/env bash
# The code-placement experiment of research/investigations/code-placement/DESIGN.md
# ("The next runs"), dispatched by .github/workflows/compute-bench.yml with
# `experiment: placement`. Explicit research only: no gate runs it.
#
# usage: placement.sh TREE WFC WORK ROUNDS [CPUS]
#        placement.sh sample WORK [CPUS]
#
# TREE's tests/performance/Makefile builds the five compute kernels with the
# compiler WFC; every arm is that build with a different layout, so the arms
# differ in layout alone. layout.py rewrites the emitted module per arm:
#
#   U    no function aligned: ` align 64` removed from every definition of the
#        module and ordinary_values.ll, the C runtime compiled without
#        -falign-functions
#   UR   U's module and ordinary_values.ll over the aligned C runtime
#   F    the aligned module over U's runtime
#   FR   the build as the compiler ships it
#   O16 O32 O48   FR with every module function that many bytes past a
#                 64-byte boundary (prefix data); diagnostic
#   L32 L64       FR with the module, ordinary_values.ll and the C runtime
#                 compiled with -falign-loops=32 or 64 as well
#   A32  functions aligned to 32 bytes instead, module, ordinary_values.ll
#        and the C runtime alike
#   P    FR with each module function at the offset of 0, 16, 32 or 48 that
#        leaves the fewest of its short innermost loops across a line
#   null a byte-identical copy of U at p0
#
# Code placements, a pad of never-executed code linked ahead of the module
# (m) or ahead of the runtime (r): p0 m16 m32 m48 r16 r48, and m1024 m2048
# for every arm whose module is aligned. Data placements, linked first in
# .data, for records only: FR at p0 with 64, 1024 or 2048 bytes (d64 d1024
# d2048), and U at p0 with the bytes that give its data FR's addresses (dFR).
# records runs every arm and placement; the other kernels run U, FR, L32,
# L64, A32 and P at the code placements; every kernel has its null.
#
# layout.py check refuses the run before any timing when an image does not
# have the layout its arm and placement claim. Every image is verified at
# widths 1, 2 and 4, then ROUNDS rounds time every image at every width, one
# process each (one warmup and five recorded calls), in an order that rotates
# and alternates direction from round to round, every process pinned to CPUS
# (default 0-3). WORK/raw.tsv receives "arm placement round kernel width call
# wall_ns cpu_ns" rows, which summarize.py reduces; WORK/manifest.txt holds
# the toolchain, the module objects' digests, the loop tables and the images'
# digests, WORK/nm/ and WORK/disassembly/ the images' symbols and the FR
# modules' code.
#
# `sample` then samples records under the software clock, FR at p0 and the
# fastest U placement of the run at each width, into WORK/lines/.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
layout() { python3 -I "$here/layout.py" "$@"; }
widths=(1 2 4)

if [[ ${1:-} == sample ]]; then
    [[ $# == 2 || $# == 3 ]] || { echo 'usage: placement.sh sample WORK [CPUS]' >&2; exit 2; }
    work=$(realpath -- "$2"); cpus=${3:-0-3}
    perf=$(command -v perf || true)
    if [[ -z $perf ]] || ! "$perf" --version > /dev/null 2>&1; then
        perf=$(ls /usr/lib/linux-tools/*/perf /usr/lib/linux-tools-*/perf 2> /dev/null | head -1)
    fi
    [[ -n $perf ]] || { echo 'no perf binary' >&2; exit 1; }
    mkdir -p "$work/lines"
    image_of() { awk -F '\t' -v a="$1" -v p="$2" '$1 == "records" && $2 == a && $3 == p { print $4 }' \
        "$work/images.tsv"; }
    for width in "${widths[@]}"; do
        fastest=$(python3 -I "$here/summarize.py" fastest "$work/raw.tsv" records "$width" U)
        for name in "FR p0" "U $fastest"; do
            read -r arm placement <<< "$name"
            image=$(image_of "$arm" "$placement")
            data=$work/lines/perf-$width-$arm-$placement.data
            "$perf" record -q -e cpu-clock:u -F 4999 -o "$data" -- bash -c '
                for _ in $(seq 20); do
                    env -u WF_SPLIT_WORK WF_WORKERS="$1" taskset -c "$2" "$3" measure candidate "$1" 0 > /dev/null
                done' sample "$width" "$cpus" "$image"
            "$perf" script -i "$data" -F dso,sym,symoff 2> /dev/null | grep -F "($image)" \
                > "$work/lines/script-$width-$arm-$placement.txt" || true
            layout lines "$image" "$work/lines/script-$width-$arm-$placement.txt" \
                > "$work/lines/records-W$width-$arm-$placement.txt"
            rm -f "$data"
        done
    done
    exit 0
fi

if [[ $# != 4 && $# != 5 ]]; then
    echo 'usage: placement.sh TREE WFC WORK ROUNDS [CPUS]' >&2
    exit 2
fi
tree=$(cd -- "$1" && pwd); wfc=$(realpath -- "$2")
work=$3; rounds=$4; cpus=${5:-0-3}
[[ $rounds =~ ^[1-9][0-9]*$ ]] || { echo 'ROUNDS must be a positive integer' >&2; exit 2; }
[[ ! -e $work ]] || { echo 'WORK must be a fresh directory' >&2; exit 2; }
mkdir -p "$work"; work=$(realpath -- "$work")
clang=/usr/bin/clang
kernels=(mandelbrot records fir quadrature stencil)
perfdir=$tree/tests/performance
fixtures=$tree/tests/programs/compute
values_ll=$tree/compiler/src/backend/ordinary_values.ll
mkdir -p "$work/pads" "$work/images" "$work/logs" "$work/nm" "$work/disassembly" "$work/loops"

# The build as the compiler ships it: emitted modules, their objects, the
# oracles, the runner and the runtime.
make -s -C "$perfdir" build BUILD="$work/fr" WFC="$wfc"
shipped=$(make -s -C "$perfdir" --no-print-directory BUILD="$work/fr" \
    --eval 'print-flags: ; @echo $(NATIVE_OPTIMIZATION_FLAGS)' print-flags)
[[ $shipped == '-O2 -falign-functions=64' ]] || {
    echo "the runtime's flags are '$shipped', not the ones this harness's arms are defined against" >&2; exit 1; }

# runtime DIR FLAGS VALUES_LL: the C runtime compiled with FLAGS into
# WORK/DIR/native, and ordinary_values.ll as VALUES_LL gives it compiled with
# the same flags; prints the objects in the Makefile's link order.
runtime() {
    local dir=$1 flags=$2 values=$3 objects
    read -r -a objects <<< "$(make -s -C "$perfdir" --no-print-directory BUILD="$work/$dir" \
        --eval 'print-native: ; @echo $(NATIVE_OBJECTS)' print-native)"
    make -s -C "$perfdir" BUILD="$work/$dir" WFC="$wfc" NATIVE_OPTIMIZATION_FLAGS="$flags" "${objects[@]}" >&2
    # shellcheck disable=SC2086
    "$clang" $flags -Wno-override-module -x ir -c "$values" -o "$work/$dir/values.o"
    printf '%s\n' "${objects[@]/%native\/ordinary_values_ir.o/values.o}"
}
c_only() { grep -v -e '/values\.o$' -e '\.a$'; }
join() { paste -sd, -; }

mkdir -p "$work/u" "$work/a32"
layout rewrite strip "$values_ll" "$work/u/ordinary_values.ll" "$work/u/values.claims"
layout rewrite align=32 "$values_ll" "$work/a32/ordinary_values.ll" "$work/a32/values.claims"
declare -A rt rtclaim
rt[FR]=$(runtime fr '-O2 -falign-functions=64' "$values_ll" | join)
rt[U]=$(runtime u '-O2' "$work/u/ordinary_values.ll" | join)
rt[L32]=$(runtime l32 '-O2 -falign-functions=64 -falign-loops=32' "$values_ll" | join)
rt[L64]=$(runtime l64 '-O2 -falign-functions=64 -falign-loops=64' "$values_ll" | join)
rt[A32]=$(runtime a32 '-O2 -falign-functions=32' "$work/a32/ordinary_values.ll" | join)
rtclaim[FR]="64@$(tr , '\n' <<< "${rt[FR]}" | grep -v '\.a$' | join)"
rtclaim[U]=none
rtclaim[L32]="64@$(tr , '\n' <<< "${rt[L32]}" | grep -v '\.a$' | join)"
rtclaim[L64]="64@$(tr , '\n' <<< "${rt[L64]}" | grep -v '\.a$' | join)"
rtclaim[A32]="32@$(tr , '\n' <<< "${rt[A32]}" | grep -v '\.a$' | join)"
# UR: U's ordinary_values.ll over the aligned C runtime.
rt[UR]=$(tr , '\n' <<< "${rt[FR]}" | sed "s#^$work/fr/values.o\$#$work/u/values.o#" | join)
rtclaim[UR]="64@$(tr , '\n' <<< "${rt[FR]}" | c_only | join)"

# module ARM KERNEL: the arm's module object and claims file.
declare -A flags=([FR]='-O2 -falign-functions=64' [U]='-O2'
    [L32]='-O2 -falign-functions=64 -falign-loops=32' [L64]='-O2 -falign-functions=64 -falign-loops=64'
    [A32]='-O2 -falign-functions=32' [O16]='-O2 -falign-functions=64' [O32]='-O2 -falign-functions=64'
    [O48]='-O2 -falign-functions=64' [P]='-O2 -falign-functions=64')
declare -A mode=([FR]=keep [U]=strip [L32]=keep [L64]=keep [A32]=align=32 [O16]=offset=16
    [O32]=offset=32 [O48]=offset=48)
for kernel in "${kernels[@]}"; do
    objdump -d --insn-width=16 "$work/fr/$kernel.o" > "$work/disassembly/$kernel.txt"
    layout loops "$work/fr/$kernel.o" > "$work/loops/$kernel.txt"
    layout choose "$work/fr/$kernel.o" "$work/loops/$kernel.choice"
    for arm in FR U L32 L64 A32 O16 O32 O48 P; do
        dir=$work/arm-$arm; mkdir -p "$dir"
        if [[ $arm == P ]]; then m=choice=$work/loops/$kernel.choice; else m=${mode[$arm]}; fi
        layout rewrite "$m" "$work/fr/$kernel.ll" "$dir/$kernel.ll" "$dir/$kernel.claims" \
            "$fixtures/${kernel}_host.ll"
        # shellcheck disable=SC2086
        "$clang" ${flags[$arm]} -Wno-override-module -x ir -c "$dir/$kernel.ll" -o "$dir/$kernel.o"
    done
done
cmp -s "$work/arm-FR/records.o" "$work/fr/records.o" || {
    echo "FR's module object is not the Makefile's" >&2; exit 1; }

module_of() { # ARM KERNEL -> "object claims"
    case $1 in
        U|UR|null) echo "$work/arm-U/$2.o none" ;;
        F) echo "$work/arm-FR/$2.o $work/arm-FR/$2.claims" ;;
        *) echo "$work/arm-$1/$2.o $work/arm-$1/$2.claims" ;;
    esac
}
runtime_of() { # ARM -> "objects claim"
    case $1 in
        U|F|null) echo "${rt[U]} ${rtclaim[U]}" ;;
        UR|L32|L64|A32) echo "${rt[$1]} ${rtclaim[$1]}" ;;
        *) echo "${rt[FR]} ${rtclaim[FR]}" ;;
    esac
}

pad() { # SECTION BYTES: one pad object of that many bytes
    local file=$work/pads/$1$2
    if [[ $1 == text ]]; then
        printf '\t.section .note.GNU-stack,"",@progbits\n\t.text\n\t.p2align 4\nwf_placement_pad_%s:\n\t.fill %s,1,0xcc\n' \
            "$2" "$2" > "$file.s"
    else
        printf '\t.section .note.GNU-stack,"",@progbits\n\t.data\nwf_placement_data_pad_%s:\n\t.fill %s,1,0\n' \
            "$2" "$2" > "$file.s"
    fi
    "$clang" -c "$file.s" -o "$file.o"
}
for bytes in 0 16 32 48 1024 2048; do pad text "$bytes"; done
for bytes in 0 64 1024 2048; do pad data "$bytes"; done

# image ARM PLACEMENT KERNEL: the module, the kernel's oracle and the runner,
# then the runtime, as tests/performance/Makefile links them, with one data
# pad first, one code pad ahead of the module and one ahead of the runtime.
# Image paths have one length within a kernel.
: > "$work/images.tsv"
declare -A names count
image() {
    local arm=$1 placement=$2 kernel=$3 before=0 after=0 data=0 module claims objects claim index path
    read -r module claims <<< "$(module_of "$arm" "$kernel")"
    read -r objects claim <<< "$(runtime_of "$arm")"
    case $placement in
        m*) before=${placement#m} ;;
        r*) after=${placement#r} ;;
        dFR) data=$dfr ;;
        d*) data=${placement#d} ;;
    esac
    [[ -e $work/pads/data$data.o ]] || pad data "$data"
    index=$(printf '%03d' "${count[$kernel]:-0}"); count[$kernel]=$((10#$index + 1))
    path=$work/images/$kernel/$index; mkdir -p "$work/images/$kernel"
    local runtime_list; IFS=, read -r -a runtime_list <<< "$objects"
    "$clang" -O2 "$work/pads/data$data.o" "$work/pads/text$before.o" "$module" \
        "$work/fr/${kernel}_oracle.o" "$work/fr/runner.o" "$work/pads/text$after.o" \
        "${runtime_list[@]}" -pthread -lm -o "$path"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$kernel" "$arm" "$placement" "$path" "$module" \
        "$claims" "$claim" "$objects" >> "$work/images.tsv"
    names[$kernel]+="$arm-$placement-$index "
}
code_placements() {
    case $1 in
        U|UR) echo p0 m16 m32 m48 r16 r48 ;;
        *) echo p0 m16 m32 m48 r16 r48 m1024 m2048 ;;
    esac
}
dfr=0
image U p0 records
image FR p0 records
dfr=$(layout datadelta "$work/images/records/000" "$work/images/records/001")
[[ $dfr =~ ^[0-9]+$ ]] || { echo "FR's data lies below U's by ${dfr#-} bytes; dFR cannot be linked" >&2; exit 1; }
for kernel in "${kernels[@]}"; do
    if [[ $kernel == records ]]; then
        arms=(U UR F FR O16 O32 O48 L32 L64 A32 P)
    else
        arms=(U FR L32 L64 A32 P)
    fi
    for arm in "${arms[@]}"; do
        for placement in $(code_placements "$arm"); do
            [[ $kernel == records && $placement == p0 && ( $arm == U || $arm == FR ) ]] ||
                image "$arm" "$placement" "$kernel"
        done
    done
    if [[ $kernel == records ]]; then
        for placement in d64 d1024 d2048; do image FR "$placement" records; done
        image U dFR records
    fi
    image null p0 "$kernel"
    cp "$(awk -F '\t' -v k="$kernel" '$1 == k && $2 == "U" && $3 == "p0" { print $4 }' "$work/images.tsv")" \
        "$(awk -F '\t' -v k="$kernel" '$1 == k && $2 == "null" { print $4 }' "$work/images.tsv")"
done
layout check "$work/images.tsv"

{
    printf 'tree=%s %s\nwfc=%s\n' "$tree" "$(git -C "$tree" rev-parse HEAD)" "$wfc"
    printf 'rounds=%s cpus=%s images=%s dFR=%s\n' "$rounds" "$cpus" "$(wc -l < "$work/images.tsv")" "$dfr"
    uname -a
    "$clang" --version | head -1
    lscpu
    printf '\nmodule objects\n'
    sha256sum "$work"/arm-*/*.o "$work"/*/values.o
    for kernel in "${kernels[@]}"; do
        printf '\nloops of the FR module of %s\n' "$kernel"
        cat "$work/loops/$kernel.txt"
        printf 'P offsets: %s\n' "$(awk '$2 != 0 { printf "%s=%s ", $1, $2 }' "$work/loops/$kernel.choice")"
    done
    printf '\nimages\n'
    cat "$work/images.tsv"
    sha256sum "$work"/images/*/*
} > "$work/manifest.txt"
while IFS=$'\t' read -r kernel arm placement path _; do
    nm -n "$path" > "$work/nm/$kernel-$arm-$placement.txt"
done < "$work/images.tsv"

for kernel in "${kernels[@]}"; do
    for entry in ${names[$kernel]}; do
        for width in "${widths[@]}"; do
            env -u WF_SPLIT_WORK WF_WORKERS="$width" taskset -c "$cpus" timeout 60s \
                "$work/images/$kernel/${entry##*-}" verify > "$work/logs/verify-$kernel-$entry-$width.log" 2>&1
        done
    done
done
echo "verified $(wc -l < "$work/images.tsv") images at widths ${widths[*]}"

: > "$work/raw.tsv"
for ((round = 0; round < rounds; round++)); do
    for ((k = 0; k < ${#kernels[@]}; k++)); do
        kernel=${kernels[$(((k + round) % ${#kernels[@]}))]}
        read -r -a list <<< "${names[$kernel]}"
        total=${#list[@]}
        for ((v = 0; v < ${#widths[@]}; v++)); do
            width=${widths[$(((v + round) % ${#widths[@]}))]}
            for ((i = 0; i < total; i++)); do
                if ((round % 2)); then j=$(((total - 1 - i + 7 * round) % total))
                else j=$(((i + 7 * round) % total)); fi
                entry=${list[$j]}; label=${entry%-*}
                env -u WF_SPLIT_WORK WF_WORKERS="$width" taskset -c "$cpus" timeout 60s \
                    "$work/images/$kernel/${entry##*-}" measure candidate "$width" 0 2> /dev/null |
                    awk -v arm="${label%%-*}" -v placement="${label#*-}" -v round="$round" \
                        '{ print arm "\t" placement "\t" round "\t" $1 "\t" $3 "\t" $5 "\t" $6 "\t" $7 }' \
                        >> "$work/raw.tsv"
            done
        done
    done
    echo "round $round of $rounds done"
done
