#!/bin/sh
# CI only; compile each runtime arm from the checkout's same source.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
WFC=${WFC:-"$ROOT/compiler/target/gate/whitefootc"}
OUT=${OUT:-/tmp/guard-fairness}
COUNTERS=${COUNTERS:-0}
CLANG=${CLANG:-/usr/bin/clang}
mkdir -p "$OUT"
"$WFC" --emit-llvm "$ROOT/research/experiments/guard-fairness/probe.wf" -o "$OUT/probe.ll"
llvm="$OUT/probe.ll"
if [ "$COUNTERS" = 1 ]; then
    # Only attribution images observe the return statement's first lock
    # attempt. Timing images keep the compiler's original IR byte for byte.
    llvm="$OUT/probe-diagnostic.ll"
    python3 "$ROOT/research/experiments/guard-fairness/tag-acquire.py" \
        "$OUT/probe.ll" "$llvm" wf_contend 2 4 wf_hold_work
fi
for arm in baseline age oldest; do
    case "$arm" in baseline) mode=0 ;; age) mode=1 ;; oldest) mode=2 ;; esac
    make -f "$ROOT/research/experiments/guard-fairness/Makefile" \
        BUILD="$OUT/build-$arm" LLVM="$llvm" IMAGE="$OUT/$arm" \
        CLANG="$CLANG" MODE="$mode" COUNTERS="$COUNTERS"
done
# The noise control is a copy of the baseline executable, not a rebuild.
cp "$OUT/baseline" "$OUT/twin"
cmp "$OUT/baseline" "$OUT/twin"
printf 'revision,%s\ncounters,%s\n' "$(git -C "$ROOT" rev-parse HEAD)" "$COUNTERS"
sha256sum "$WFC" "$OUT/probe.ll" "$OUT/baseline" "$OUT/age" "$OUT/oldest" "$OUT/twin"
if [ "$COUNTERS" = 1 ]; then sha256sum "$llvm"; fi
"$CLANG" --version
ld.lld --version
