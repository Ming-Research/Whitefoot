#!/usr/bin/env bash
# Time wf_file_wait_readiness (completion/file_posix.c) compiled under each
# variant's probe arguments, the variants run one after another in rotating
# order within every round so that drift of the machine reaches each alike, and
# print a median of the paired per-round ratios to the first variant. Also
# prints, for each variant, how many page steps and touches the function's
# assembly holds and whether that assembly is the first variant's.
# Written for macOS's /bin/bash 3.2 as well.
#
# usage: wait-bench.sh CLANG SYSROOT LABEL NAME=ARGUMENTS...
#   CLANG      the clang to run
#   SYSROOT    the macOS SDK for an upstream clang, or empty
#   LABEL      a name for this set
#   NAME=ARGS  one variant: its name and the probe arguments it compiles with,
#              split on spaces; the first variant is the baseline
# ROUNDS (default 15) rounds per count. Run from the repository root; the C
# source is read from the directory of this script.
#
# The unit is compiled as the driver compiles a runtime unit (-std=c11
# -pthread -I… -O2 -falign-functions=64), on Linux also with
# -ffunction-sections, so that the linker, which is told to drop what main
# cannot reach, leaves out every other function of the unit and with it their
# references to the rest of the runtime (a Mach-O linker drops them by symbol).
set +e
clang="$1"; sysroot_path="$2"; label="$3"; shift 3
rounds="${ROUNDS:-15}"
src=compiler/src/backend
here=$(cd "$(dirname "$0")" && pwd)
out="${TMPDIR:-/tmp}/wait-bench-$label"
rm -rf "$out"; mkdir -p "$out"
sysroot=()
if [ -n "$sysroot_path" ]; then sysroot=(-isysroot "$sysroot_path"); fi
common=(-std=c11 -pthread -I "$src" -I "$src/completion")
if [ "$(uname -s)" = Darwin ]; then
  link=(-Wl,-dead_strip)
  sections=()
else
  link=(-no-pie -Wl,--gc-sections -Wl,--unresolved-symbols=ignore-all)
  sections=(-ffunction-sections)
fi
pin=()
if command -v taskset > /dev/null 2>&1; then pin=(taskset -c 1); fi

"$clang" ${sysroot[@]+"${sysroot[@]}"} "${common[@]}" -O2 -c -o "$out/bench.o" "$here/wait-bench.c" || exit 1
names=()
for spec in "$@"; do
  name="${spec%%=*}"; args="${spec#*=}"
  names+=("$name")
  # shellcheck disable=SC2086
  "$clang" ${sysroot[@]+"${sysroot[@]}"} "${common[@]}" -O2 -falign-functions=64 ${sections[@]+"${sections[@]}"} $args \
    -c -o "$out/$name.o" "$src/completion/file_posix.c" || exit 1
  # shellcheck disable=SC2086
  "$clang" ${sysroot[@]+"${sysroot[@]}"} "${common[@]}" -O2 -falign-functions=64 ${sections[@]+"${sections[@]}"} $args \
    -S -o "$out/$name.s" "$src/completion/file_posix.c" || exit 1
  "$clang" ${sysroot[@]+"${sysroot[@]}"} -pthread ${link[@]+"${link[@]}"} -o "$out/$name" "$out/bench.o" "$out/$name.o" || exit 1
  steps=$(awk -v a='wf_file_wait_readiness:' -v b='_wf_file_wait_readiness:' '
    $1 == a || $1 == b { inside = 1 }
    inside && /\.cfi_endproc/ { exit }
    inside && (/subq[ \t]+\$4096, %rsp/ || /sub[ \t]+sp, sp, #1, lsl #12/ || /___chkstk_darwin/) { n++ }
    END { print n + 0 }' "$out/$name.s")
  same=no; if cmp -s "$out/$name.s" "$out/${names[0]}.s"; then same=yes; fi
  echo "variant $name: arguments [$args], page steps or helper calls in the function: $steps, assembly equal to ${names[0]}'s: $same"
done

table="$out/times.tsv"; : > "$table"
for count in 1 64 4096; do
  case "$count" in 1) calls=300000 ;; 64) calls=100000 ;; *) calls=3000 ;; esac
  round=0
  while [ "$round" -lt "$rounds" ]; do
    round=$((round + 1))
    total=${#names[@]}
    shift_by=$(( (round - 1) % total ))
    i=0
    while [ "$i" -lt "$total" ]; do
      name="${names[$(( (i + shift_by) % total ))]}"
      ns=$(${pin[@]+"${pin[@]}"} "$out/$name" "$count" "$calls") || { echo "!!! $name failed at count $count"; exit 1; }
      printf '%s\t%s\t%s\t%s\n' "$count" "$round" "$name" "$ns" >> "$table"
      i=$((i + 1))
    done
  done
done

python3 -I - "$table" "${names[0]}" <<'PY'
import collections, statistics, sys
rows = collections.defaultdict(dict)   # (count, round) -> {variant: ns}
order = []
for line in open(sys.argv[1]):
    count, rnd, name, ns = line.split('\t')
    rows[(int(count), int(rnd))][name] = float(ns)
    if name not in order:
        order.append(name)
base = sys.argv[2]
print('entries  variant            median ns/call   paired ratio to %s (median, min..max)' % base)
for count in sorted({c for c, _ in rows}):
    rounds = [rows[(c, r)] for c, r in sorted(rows) if c == count]
    for name in order:
        values = [r[name] for r in rounds]
        ratios = [r[name] / r[base] for r in rounds]
        print('%7d  %-17s %14.1f   %.4f (%.4f..%.4f)' % (
            count, name, statistics.median(values), statistics.median(ratios), min(ratios), max(ratios)))
PY
