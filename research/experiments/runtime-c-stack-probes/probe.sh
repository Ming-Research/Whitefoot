#!/usr/bin/env bash
# Compile every runtime C unit the driver compiles on a POSIX host, with the
# driver's arguments, under one candidate probe argument set, and list each
# frame over a page or of dynamic size with the stack lines of its body, the
# options the driver hands to cc1, and the probe attributes clang gives
# wf_file_wait_readiness. Written for macOS's /bin/bash 3.2 as well.
#
# usage: probe.sh CLANG SYSROOT LABEL [ARGUMENT ...]
#   CLANG    the clang to run
#   SYSROOT  the macOS SDK for an upstream clang, or empty
#   LABEL    a name for the variant; ARGUMENTs are the candidate arguments
# Run from the repository root. Output goes to stdout; scratch to $TMPDIR.
set +e
clang="$1"; sysroot_path="$2"; label="$3"; shift 3
src=compiler/src/backend
units="wf_floor.c ordinary_values.c heap.c sched/core.c sched/prim_host.c sched/entry.c
  completion/runtime.c completion/wait_host.c completion/file_adapter.c completion/file_posix.c
  completion/bridge.c completion/snapshot_export.c completion/fork_capsule.c
  completion/stop_signals.c completion/linux_io_uring.c keyed_table.c"
sysroot=()
if [ -n "$sysroot_path" ]; then sysroot=(-isysroot "$sysroot_path"); fi
out="${TMPDIR:-/tmp}/runtime-c-stack-probes/$label"
rm -rf "$out"; mkdir -p "$out"
echo "================ $label: $* ================"
echo "--- driver line for completion/file_posix.c:"
"$clang" "${sysroot[@]}" -std=c11 -pthread -I "$src" -I "$src/completion" -O2 -falign-functions=64 "$@" \
  -S -o /dev/null "$src/completion/file_posix.c" -### 2>&1 | tr ' ' '\n' | grep -E 'stack|probe|chkstk' | sort -u
echo "--- probe attributes of wf_file_wait_readiness:"
"$clang" "${sysroot[@]}" -std=c11 -pthread -I "$src" -I "$src/completion" -O2 -falign-functions=64 "$@" \
  -S -emit-llvm -o "$out/file_posix.ll" "$src/completion/file_posix.c" 2>&1 | head -5
group=$(grep -E '^define .*@wf_file_wait_readiness\(' "$out/file_posix.ll" | grep -oE '#[0-9]+' | head -1)
grep -E "^attributes $group " "$out/file_posix.ll" | grep -oE '"(probe-stack|stack-probe-size)"="[^"]*"' || echo "(none)"
for unit in $units; do
  extra=()
  case "$unit" in
    completion/snapshot_export.c|completion/fork_capsule.c) extra=(-fno-builtin -fno-stack-protector -fno-sanitize=all) ;;
  esac
  base="$out/$(echo "${unit%.c}" | tr / _)"
  if ! "$clang" "${sysroot[@]}" -std=c11 -pthread -I "$src" -I "$src/completion" -O2 -falign-functions=64 \
      "${extra[@]}" "$@" -S -fstack-usage -o "$base.s" "$src/$unit" 2> "$base.err"; then
    echo "!!! $unit failed:"; cat "$base.err"; continue
  fi
  if [ -s "$base.err" ]; then echo "--- $unit warnings:"; head -3 "$base.err"; fi
  [ -f "$base.su" ] || continue
  awk -F '\t' '$2 > 4096 || $3 != "static"' "$base.su" | while IFS="$(printf '\t')" read -r where bytes kind; do
    name="${where##*:}"
    echo "### $unit $name $bytes $kind"
    awk -v a="$name:" -v b="_$name:" '
      $1 == a || $1 == b { inside = 1 }
      inside && /\.cfi_endproc/ { exit }
      inside && /sp|chkstk|xzr/ { print; if (++shown == 30) exit }' "$base.s"
  done
done
"$clang" "${sysroot[@]}" -O2 -falign-functions=64 -Wno-override-module -x ir "$@" -S -fstack-usage \
  -o "$out/ordinary_values_ir.s" "$src/ordinary_values.ll" 2> /dev/null \
  && awk -F '\t' '$2 > 4096 || $3 != "static" { print "### ordinary_values.ll " $0 }' "$out/ordinary_values_ir.su"
exit 0
