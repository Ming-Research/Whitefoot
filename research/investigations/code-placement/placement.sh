#!/usr/bin/env bash
# The code-placement experiment of research/investigations/code-placement/DESIGN.md,
# dispatched by .github/workflows/code-placement.yml. Explicit research only:
# no gate runs it.
#
# usage: placement.sh UNALIGNED_TREE UNALIGNED_WFC ALIGNED_TREE ALIGNED_WFC WORK ROUNDS [CPUS]
#
# UNALIGNED_TREE is a checkout whose compiler emits no function alignment and
# whose runtime compiles without it, built into UNALIGNED_WFC; ALIGNED_TREE
# and ALIGNED_WFC are the same for the aligned compiler. Each tree's own
# tests/performance/Makefile builds its objects, so each arm's runtime is
# compiled the way that tree compiles it. From those objects this script links,
# for every compute kernel, four arms at six placements and one null copy:
#
#   U    the unaligned tree's images
#   F    the aligned tree's emitted module with the unaligned tree's runtime
#   FR   the aligned tree's images
#   FRL  FR with the emitted module compiled with -falign-loops=32 as well
#   null a byte-identical copy of U at placement p0
#
#   p0   no padding
#   m16 m32 m48   that many bytes of padding linked ahead of the module, which
#                 moves the module and everything after it
#   r16 r48       that many bytes linked between the module's oracle and
#                 runner objects and the runtime, which moves the runtime only
#
# Every image is verified at widths 1, 2 and 4, then ROUNDS rounds time every
# image at every width, one process each (one warmup and five recorded calls),
# in an order that rotates and alternates direction from round to round. Every
# process is pinned to CPUS (default 0-3) with taskset. WORK/raw.tsv receives
# "arm placement round kernel width call wall_ns cpu_ns" rows, which
# summarize.py reduces.
set -euo pipefail
if [[ $# != 6 && $# != 7 ]]; then
    echo 'usage: placement.sh UNALIGNED_TREE UNALIGNED_WFC ALIGNED_TREE ALIGNED_WFC WORK ROUNDS [CPUS]' >&2
    exit 2
fi
utree=$(cd -- "$1" && pwd); uwfc=$(realpath -- "$2")
atree=$(cd -- "$3" && pwd); awfc=$(realpath -- "$4")
work=$5; rounds=$6; cpus=${7:-0-3}
[[ $rounds =~ ^[1-9][0-9]*$ ]] || { echo 'ROUNDS must be a positive integer' >&2; exit 2; }
[[ ! -e $work ]] || { echo 'WORK must be a fresh directory' >&2; exit 2; }
clang=/usr/bin/clang
kernels=(mandelbrot records fir quadrature stencil)
widths=(1 2 4)
arms=(U F FR FRL)
placements=(p0 m16 m32 m48 r16 r48)
mkdir -p "$work/pads" "$work/images" "$work/logs"

# Each tree builds its own objects with its own compiler and runtime flags.
make -s -C "$utree/tests/performance" build BUILD="$work/u" WFC="$uwfc"
make -s -C "$atree/tests/performance" build BUILD="$work/fr" WFC="$awfc"
native() { # the runtime objects of one tree's build, in its link order
    make -s -C "$1/tests/performance" --no-print-directory BUILD="$2" \
        --eval 'print-native: ; @echo $(NATIVE_OBJECTS)' print-native
}
read -r -a unative <<< "$(native "$utree" "$work/u")"
read -r -a anative <<< "$(native "$atree" "$work/fr")"
optimization=$(make -s -C "$atree/tests/performance" --no-print-directory \
    BUILD="$work/fr" --eval 'print-flags: ; @echo $(NATIVE_OPTIMIZATION_FLAGS)' print-flags)
mkdir -p "$work/frl"
for kernel in "${kernels[@]}"; do
    # shellcheck disable=SC2086
    "$clang" $optimization -falign-loops=32 -Wno-override-module -x ir \
        -c "$work/fr/$kernel.ll" -o "$work/frl/$kernel.o"
done
for bytes in 0 16 32 48; do
    printf '\t.section .note.GNU-stack,"",@progbits\n\t.text\n\t.p2align 4\nwf_placement_pad_%s:\n\t.fill %s,1,0xcc\n' \
        "$bytes" "$bytes" > "$work/pads/pad$bytes.s"
    "$clang" -c "$work/pads/pad$bytes.s" -o "$work/pads/pad$bytes.o"
done

# image ARM PLACEMENT KERNEL: the module, the kernel's oracle and the runner,
# then the runtime, as tests/performance/Makefile links them, with one pad
# object ahead of the module and one ahead of the runtime.
image() {
    local arm=$1 placement=$2 kernel=$3 module objects native before=0 after=0
    case $arm in
        U) module=$work/u/$kernel.o; objects=$work/u; native=("${unative[@]}") ;;
        F) module=$work/fr/$kernel.o; objects=$work/u; native=("${unative[@]}") ;;
        FR) module=$work/fr/$kernel.o; objects=$work/fr; native=("${anative[@]}") ;;
        FRL) module=$work/frl/$kernel.o; objects=$work/fr; native=("${anative[@]}") ;;
    esac
    case $placement in
        m*) before=${placement#m} ;;
        r*) after=${placement#r} ;;
    esac
    "$clang" -O2 "$work/pads/pad$before.o" "$module" "$objects/${kernel}_oracle.o" \
        "$objects/runner.o" "$work/pads/pad$after.o" "${native[@]}" -pthread -lm \
        -o "$work/images/$arm-$placement-$kernel"
}
names=()
for arm in "${arms[@]}"; do
    for placement in "${placements[@]}"; do
        names+=("$arm-$placement")
        for kernel in "${kernels[@]}"; do image "$arm" "$placement" "$kernel"; done
    done
done
names+=(null-p0)
for kernel in "${kernels[@]}"; do
    cp "$work/images/U-p0-$kernel" "$work/images/null-p0-$kernel"
done

{
    printf 'unaligned=%s %s\naligned=%s %s\n' "$utree" "$(git -C "$utree" rev-parse HEAD)" \
        "$atree" "$(git -C "$atree" rev-parse HEAD)"
    printf 'rounds=%s cpus=%s images=%s\n' "$rounds" "$cpus" "${#names[@]}"
    uname -a
    "$clang" --version | head -1
    lscpu
    printf '\n'
    for kernel in mandelbrot records; do
        for name in U-p0 U-m16 F-p0 F-m16 FR-p0 FR-m16 FR-r16; do
            printf '%s %s: ' "$name" "$kernel"
            nm -n "$work/images/$name-$kernel" 2>/dev/null |
                awk '/ wf__par_(chunk|seq)_|wf__par_join$/ { printf "%s=%s ", $3, $1 }' || true
            printf '\n'
        done
    done
    sha256sum "$work"/images/*
} > "$work/manifest.txt"

for name in "${names[@]}"; do
    for kernel in "${kernels[@]}"; do
        for width in "${widths[@]}"; do
            env -u WF_SPLIT_WORK WF_WORKERS="$width" taskset -c "$cpus" timeout 60s \
                "$work/images/$name-$kernel" verify > "$work/logs/verify-$name-$kernel-$width.log" 2>&1
        done
    done
done
echo "verified ${#names[@]} images of ${#kernels[@]} kernels at widths ${widths[*]}"

: > "$work/raw.tsv"
count=${#names[@]}
for ((round = 0; round < rounds; round++)); do
    for ((k = 0; k < ${#kernels[@]}; k++)); do
        kernel=${kernels[$(((k + round) % ${#kernels[@]}))]}
        for ((v = 0; v < ${#widths[@]}; v++)); do
            width=${widths[$(((v + round) % ${#widths[@]}))]}
            for ((i = 0; i < count; i++)); do
                if ((round % 2)); then j=$(((count - 1 - i + 7 * round) % count))
                else j=$(((i + 7 * round) % count)); fi
                name=${names[$j]}
                env -u WF_SPLIT_WORK WF_WORKERS="$width" taskset -c "$cpus" timeout 60s \
                    "$work/images/$name-$kernel" measure candidate "$width" 0 2>/dev/null |
                    awk -v arm="${name%%-*}" -v placement="${name#*-}" -v round="$round" \
                        '{ print arm "\t" placement "\t" round "\t" $1 "\t" $3 "\t" $5 "\t" $6 "\t" $7 }' \
                        >> "$work/raw.tsv"
            done
        done
    done
    echo "round $round of $rounds done"
done
