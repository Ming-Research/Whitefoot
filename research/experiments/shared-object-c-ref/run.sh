#!/usr/bin/env bash
# Runs every arm of the C reference study. Usage: run.sh --out DIR [--ms 5000] [--blocks 3]
# Builds with /usr/bin/clang -O2 -march=native -pthread. Run only on the 14900k runner.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out=""
ms=5000
blocks=3
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --ms) ms="$2"; shift 2 ;;
    --blocks) blocks="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$out" ] || { echo "--out is required" >&2; exit 2; }
mkdir -p "$out/bin"

CLANG=/usr/bin/clang
"$CLANG" -O2 -march=native -pthread -o "$out/bin/cref" "$here/cref.c"
"$CLANG" -O2 -march=native -pthread -o "$out/bin/w7" "$here/w7.c"

lists=("0,2" "0,2,4,6" "0,2,4,6,8,10,12,14" "0-15")
list8="0,2,4,6,8,10,12,14"
arms=(A1 A1t A2 A3 A3r A4 A4r A5a A5b A5c A5d A5e
      A6sl A6sf A6sx A6sc A6pl A6pf A6px A6pc A7n A7s A8)
READER_CPU=16

ncpu() { echo "$1" | tr ',' '\n' | awk -F- 'NF==2{n+=$2-$1+1;next}{n++}END{print n}'; }

raw="$out/raw.tsv"
printf 'arm\tcpus\tblock\tfields\n' > "$raw"

run_cell() {
  local arm="$1" cpus="$2" block="$3" line
  case "$arm" in
    A3)  line=$(taskset -c "$cpus" "$out/bin/w7" "$(ncpu "$cpus")" "$ms") ;;
    A3r) line=$(taskset -c "$cpus" "$out/bin/w7" "$(ncpu "$cpus")" "$ms" "$READER_CPU") ;;
    A4r) line=$("$out/bin/cref" "$arm" "$cpus" "$ms" "$READER_CPU") ;;
    *)   line=$("$out/bin/cref" "$arm" "$cpus" "$ms") ;;
  esac
  echo "[block $block] $arm $cpus: $line"
  printf '%s\t%s\t%s\t%s\n' "$arm" "$cpus" "$block" "$(echo "$line" | tr ' ' '\t')" >> "$raw"
}

count=${#arms[@]}
for ((block = 0; block < blocks; block++)); do
  shift_by=$(( (block * 8) % count ))
  for ((i = 0; i < count; i++)); do
    arm="${arms[$(( (i + shift_by) % count ))]}"
    if [ "$arm" = A8 ]; then
      run_cell A8 "0" "$block"
      run_cell A8 "$list8" "$block"
    else
      for cpus in "${lists[@]}"; do
        run_cell "$arm" "$cpus" "$block"
      done
    fi
  done
done
echo "done: $raw"
