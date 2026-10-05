#!/bin/sh
# Build every E0 variant of vm.c into OUT (default ./out).
#   switch, goto, tail (table), tailpn (table, preserve_none),
#   cell (handler offset in the cell), cellpn
# each under the four access forms: checked, u8, u8v (u8 without the fetch
# comparison), raw.
# Plus counting builds (one per access form; dispatch counts do not depend on
# the dispatch shape) and a padded control of tailpn-checked.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?usage: build.sh OUT_DIR}
cc=${CC:-clang}
mkdir -p "$out"
flags="-O2 -std=gnu11 -Wall -Wextra -Werror -Wno-unused-label -Wno-unused-parameter"

build() { # name, extra flags...
    name=$1; shift
    $cc $flags "$@" -o "$out/$name" "$here/vm.c"
}

for access in 1 2 4 3; do
    case $access in 1) a=checked ;; 2) a=u8 ;; 4) a=u8v ;; 3) a=raw ;; esac
    build "switch-$a" -DDISPATCH=1 -DACCESS=$access
    build "goto-$a" -DDISPATCH=2 -DACCESS=$access
    build "tail-$a" -DDISPATCH=3 -DACCESS=$access
    build "tailpn-$a" -DDISPATCH=3 -DPRESERVE_NONE -DACCESS=$access
    build "cell-$a" -DDISPATCH=4 -DACCESS=$access
    build "cellpn-$a" -DDISPATCH=4 -DPRESERVE_NONE -DACCESS=$access
    build "count-$a" -DDISPATCH=1 -DACCESS=$access -DCOUNT
done
build "tailpn-checked-pad64" -DDISPATCH=3 -DPRESERVE_NONE -DACCESS=1 -DPAD=64
build "tailpn-checked-pad2048" -DDISPATCH=3 -DPRESERVE_NONE -DACCESS=1 -DPAD=2048

# E1 (vm1.c): accumulator and pinned locals; the mode is a run-time argument.
build1() { # name, extra flags...
    name=$1; shift
    $cc $flags "$@" -o "$out/$name" "$here/vm1.c"
}
for access in 2 4 3; do
    case $access in 2) a=u8 ;; 4) a=u8v ;; 3) a=raw ;; esac
    build1 "e1-switch-$a" -DDISPATCH=1 -DACCESS=$access
    build1 "e1-goto-$a" -DDISPATCH=2 -DACCESS=$access
    build1 "e1-tailpn-$a" -DDISPATCH=3 -DACCESS=$access
    build1 "e1-cellpn-$a" -DDISPATCH=4 -DACCESS=$access
done
build1 "e1-count" -DDISPATCH=1 -DACCESS=2 -DCOUNT
for access in 2 4 3; do
    case $access in 2) a=u8 ;; 4) a=u8v ;; 3) a=raw ;; esac
    build1 "e1hb-tailpn-$a" -DDISPATCH=3 -DACCESS=$access -DHANDLER_BASE_PARAM
    build1 "e1hb-cellpn-$a" -DDISPATCH=4 -DACCESS=$access -DHANDLER_BASE_PARAM
done

# Optional inputs of the Silverfir-nano comparison and stage 2:
#   WASI_SDK=<wasi-sdk directory>  builds wasm/kernels.wasm as kernels.wasm;
#   WHITEFOOTC=<whitefootc>        builds wf/vm.wf once per kernel as
#                                  ${WF_NAME:-wfsplit}-<kernel>.
if [ -n "${WASI_SDK:-}" ]; then
    "$WASI_SDK/bin/clang" --sysroot="$WASI_SDK/share/wasi-sysroot" -O2 \
        -o "$out/kernels.wasm" "$here/wasm/kernels.c"
fi
if [ -n "${WHITEFOOTC:-}" ]; then
    name=${WF_NAME:-wfsplit}
    i=0
    for kernel in loop fib sieve mandel; do
        sed "s/  let which = 0_u64;/  let which = ${i}_u64;/" "$here/wf/vm.wf" > "$out/vm-$kernel.wf"
        "$WHITEFOOTC" "$out/vm-$kernel.wf" -o "$out/$name-$kernel"
        i=$((i + 1))
    done
fi
