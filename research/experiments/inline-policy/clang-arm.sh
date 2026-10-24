#!/usr/bin/env bash
# The host clang of one arm of the inline-policy experiment (README.md beside
# this file). whitefootc selects a `clang` sibling of its own executable
# (design/compiler/toolchain-resolution.md), so a copy of one compiler binary
# placed beside this script, named `clang`, compiles and links with the arm's
# settings and nothing else differs between arms.
#
# The arm is read from arm.conf in the same directory, a shell fragment that
# may set:
#   ARM_OPT       optimization level that replaces -O2 (default 2)
#   ARM_LLVM      LLVM options, each passed as -mllvm to every compile and as
#                 -Wl,-mllvm to every LTO link
#   ARM_LTO       1 to turn an ordinary compile-and-link into a full-LTO one
#                 (lld), which internalizes every symbol but the entry points
#   ARM_REMARKS   a file that collects the inline remarks of every invocation
#   ARM_CLANG     the real clang (default /usr/bin/clang)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
ARM_OPT=2
ARM_LLVM=
ARM_LTO=0
ARM_REMARKS=
ARM_CLANG=/usr/bin/clang
if [ -f "$here/arm.conf" ]; then
    # shellcheck disable=SC1091
    . "$here/arm.conf"
fi

args=()
compile_only=0
lto=0
for argument in "$@"; do
    case "$argument" in
    -O2) argument="-O$ARM_OPT" ;;
    -c | -S | -E | --version | -dumpversion) compile_only=1 ;;
    -flto=*) lto=1 ;;
    esac
    args+=("$argument")
done
links=$((1 - compile_only))
if [ "$ARM_LTO" = 1 ] && [ "$links" = 1 ] && [ "$lto" = 0 ]; then
    args+=(-flto=full -fuse-ld=lld)
    lto=1
fi
for option in $ARM_LLVM; do
    args+=(-mllvm "$option")
    if [ "$links" = 1 ] && [ "$lto" = 1 ]; then
        args+=("-Wl,-mllvm,$option")
    fi
done
if [ -z "$ARM_REMARKS" ]; then
    exec "$ARM_CLANG" "${args[@]}"
fi
# A per-module compile prints its remarks; an LTO link writes the remarks of
# its link-time optimization, whose decisions are the final ones, to a YAML
# file of its own beside the collected text (one per ThinLTO backend task);
# lld takes these two options' values as separate arguments.
if [ "$links" = 1 ] && [ "$lto" = 1 ]; then
    exec "$ARM_CLANG" "${args[@]}" "-Wl,--opt-remarks-filename,$ARM_REMARKS.lto.$$.yaml" \
        -Wl,--opt-remarks-passes,inline
fi
printf '### invocation %s\n' "$*" >> "$ARM_REMARKS"
exec "$ARM_CLANG" "${args[@]}" -Rpass=inline -Rpass-missed=inline 2>> "$ARM_REMARKS"
