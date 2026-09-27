#!/usr/bin/env bash
# Builds quicksort.wf without and with --par, then times each build RUNS times
# (default 7) and prints every wall time in seconds and the best one per build.
# usage: measure.sh OUTPUT_DIR [RUNS]
# WHITEFOOTC names the compiler; the default is this checkout's release build.
# Run it under the host verification lock so that no other heavy command
# competes for the processors, for example from the repository root:
#   perl .github/run-check.pl par-quicksort \
#     research/experiments/par-quicksort/measure.sh <scratch-root>/par-quicksort
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: $0 OUTPUT_DIR [RUNS]" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out=$1
runs=${2:-7}
whitefootc=${WHITEFOOTC:-$root/compiler/target/release/whitefootc}
mkdir -p "$out"
echo "processors: $(getconf _NPROCESSORS_ONLN)"
if [ -r /proc/cpuinfo ]; then grep -m 1 'model name' /proc/cpuinfo; fi
"$whitefootc" "$here/quicksort.wf" -o "$out/quicksort-seq"
"$whitefootc" --par "$here/quicksort.wf" -o "$out/quicksort-par"
TIMEFORMAT=%3R
# Each run must exit 0: main returns 1 when the sorted check fails.
time_runs() {
  local label=$1 best= seconds run
  shift
  for ((run = 1; run <= runs; run++)); do
    seconds=$({ time "$@" > /dev/null; } 2>&1)
    echo "$label, run $run: $seconds s"
    if [ -z "$best" ] || awk "BEGIN { exit !($seconds < $best) }"; then
      best=$seconds
    fi
  done
  echo "$label, best of $runs: $best s"
}
time_runs "sequential build" "$out/quicksort-seq"
time_runs "--par build, WF_WORKERS=1" env WF_WORKERS=1 "$out/quicksort-par"
time_runs "--par build, WF_WORKERS=4" env WF_WORKERS=4 "$out/quicksort-par"
