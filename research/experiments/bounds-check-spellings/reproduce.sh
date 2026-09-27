#!/bin/sh
# Writes what this record reads into OUTPUT_DIR: the x86-64 assembly of every
# Rust spelling in spellings.rs at -C opt-level=2 and 3, the Whitefoot squeeze
# at clang -O2 (the level whitefootc links with), and the rejection of that
# function without its invariant. Run it on an x86-64 host.
# usage: reproduce.sh OUTPUT_DIR
# WHITEFOOTC names the compiler; the default is this checkout's release build.
set -eu
[ $# -eq 1 ] || { echo "usage: $0 OUTPUT_DIR" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out=$1
whitefootc=${WHITEFOOTC:-$root/compiler/target/release/whitefootc}
case $whitefootc in /*) ;; */*) whitefootc=$(pwd)/$whitefootc ;; esac
mkdir -p "$out/without-invariant"
for level in 2 3; do
  rustc --edition 2021 --crate-type lib -C opt-level="$level" --emit asm \
    --remap-path-prefix "$here/=" -o "$out/spellings-O$level.s" \
    "$here/spellings.rs"
done
"$whitefootc" --emit-llvm -o "$out/squeeze.ll" "$here/squeeze.wf"
clang -O2 -S -x ir -Wno-override-module -o "$out/squeeze.s" "$out/squeeze.ll"
# Without the invariant, canonical layout puts the loop header on one line.
awk 'NR == 3 { print "  for (i in 0_u64..deref(buf).len) {"; next }
     NR >= 4 && NR <= 6 { next }
     { print }' "$here/squeeze.wf" > "$out/without-invariant/squeeze.wf"
if (cd "$out/without-invariant" && "$whitefootc" squeeze.wf -o squeeze) \
    2> "$out/rejection.txt"; then
  echo "squeeze.wf without its invariant was accepted" >&2
  exit 1
fi
{ rustc --version; clang --version | head -n 1; } > "$out/versions.txt"
echo "wrote spellings-O2.s spellings-O3.s squeeze.ll squeeze.s rejection.txt versions.txt to $out"
