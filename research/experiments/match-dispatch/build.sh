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
