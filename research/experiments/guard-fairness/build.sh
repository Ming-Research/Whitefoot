#!/bin/sh
# CI only; compile each runtime arm from the checkout's same source.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
# Only the measured revision's runtime has the arms: a later runtime ignores
# WF_GUARD_MODE, so every arm built there would be the same program.
REVISION=faccdca4e883d4f18c2b4fab52be50e8e6b8e703
if [ "$(git -C "$ROOT" rev-parse HEAD)" != "$REVISION" ]; then
    printf 'build.sh builds the arms only at revision %s, whose runtime has them;
' "$REVISION" >&2
    printf 'check it out first, e.g. git worktree add <dir> %s, and run its build.sh
' "$REVISION" >&2
    exit 1
fi
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
# The registered arms (DESIGN.md, "Registered experiment"): the baseline,
# candidate A (wake tests, mode 3), and the non-voting references A1 (mode 3
# waking every watch whose test holds below the threshold) and oldest-only
# handoff with its ordered queue (mode 2). The emitted probe registers its
# guard's wake test and passes each release's write flag; modes 0 to 2 ignore
# the test, and every probe statement writes its object, so the baseline
# wakes exactly as main's runtime does.
for arm in baseline candidate a1 oldest; do
    eligible=0
    case "$arm" in baseline) mode=0 ;; candidate) mode=3 ;; a1) mode=3 eligible=1 ;; oldest) mode=2 ;; esac
    make -f "$ROOT/research/experiments/guard-fairness/Makefile" \
        BUILD="$OUT/build-$arm" LLVM="$llvm" IMAGE="$OUT/$arm" \
        CLANG="$CLANG" MODE="$mode" ELIGIBLE="$eligible" COUNTERS="$COUNTERS"
done
# The noise control is a copy of the baseline executable, not a rebuild.
cp "$OUT/baseline" "$OUT/twin"
cmp "$OUT/baseline" "$OUT/twin"
printf 'revision,%s\ncounters,%s\n' "$(git -C "$ROOT" rev-parse HEAD)" "$COUNTERS"
sha256sum "$WFC" "$OUT/probe.ll" "$OUT/baseline" "$OUT/candidate" "$OUT/a1" "$OUT/oldest" "$OUT/twin"
if [ "$COUNTERS" = 1 ]; then sha256sum "$llvm"; fi
"$CLANG" --version
ld.lld --version
