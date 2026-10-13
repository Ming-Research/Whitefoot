#!/usr/bin/env bash
# Builds the arms of research/investigations/handle-count with two compilers
# and records the structural check of the handle count.
#
#   build.sh --pr WHITEFOOTC --base WHITEFOOTC --out DIR
#
# Writes DIR/bin/ (pr_value, pr_ref, pr_ref2, base_value, base_ref),
# DIR/host.txt, and fails unless the emitted LLVM shows what the experiment
# presupposes: see "Structural check" in the investigation's README.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
pr=
base=
out=
while [ $# -gt 0 ]; do
    case $1 in
        --pr) pr=$2; shift 2 ;;
        --base) base=$2; shift 2 ;;
        --out) out=$2; shift 2 ;;
        *) echo "usage: build.sh --pr WHITEFOOTC --base WHITEFOOTC --out DIR" >&2; exit 2 ;;
    esac
done
[ -n "$pr" ] && [ -n "$base" ] && [ -n "$out" ] || { echo "usage: build.sh --pr WHITEFOOTC --base WHITEFOOTC --out DIR" >&2; exit 2; }

mkdir -p "$out/bin" "$out/ll"

# build COMPILER ENTRY BINARY
build() {
    "$1" --full-lto --graph "$here/modules.wfg" --entry "$2" -o "$out/bin/$3"
}
# calls COMPILER ENTRY NAME: the count of handle-count calls in the emitted module
calls() {
    "$1" --emit-llvm --graph "$here/modules.wfg" --entry "$2" -o "$out/ll/$3.ll"
    grep -c 'call void @wf__shared_share(' "$out/ll/$3.ll" || true
}

build "$pr" handle_value pr_value
build "$pr" handle_reference pr_ref
build "$pr" handle_reference pr_ref2
build "$base" handle_value base_value
build "$base" handle_reference base_ref

pr_value=$(calls "$pr" handle_value pr_value)
pr_ref=$(calls "$pr" handle_reference pr_ref)
base_value=$(calls "$base" handle_value base_value)
base_ref=$(calls "$base" handle_reference base_ref)

{
    echo "kernel: $(uname -srvmo)"
    echo "cpus: $(nproc)"
    echo "model: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"
    echo "load: $(cat /proc/loadavg)"
    echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unavailable)"
    echo "no_turbo: $(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || echo unavailable)"
    echo "clang: $(/usr/bin/clang --version | head -n 1)"
    echo "pr compiler revision: ${PR_REV:-unnamed}"
    echo "base compiler revision: ${BASE_REV:-unnamed}"
    echo "wf__shared_share calls: pr_value=$pr_value pr_ref=$pr_ref base_value=$base_value base_ref=$base_ref"
    echo "binaries:"
    (cd "$out/bin" && sha256sum pr_value pr_ref pr_ref2 base_value base_ref)
    echo "cpu map (CPU, core, max MHz):"
    lscpu -e=CPU,CORE,MAXMHZ 2>/dev/null || echo unavailable
} | tee "$out/host.txt"

# The experiment presupposes: the base counts the value handle (base_value
# above base_ref), the base counts no handle in R (base_ref = pr_ref), and the
# PR counts none in V (pr_value = pr_ref). A violation means an arm is not what
# its name says, or a compiler is not the revision it was named for.
status=0
if [ "$pr_value" != "$pr_ref" ]; then
    echo "structural check failed: the PR compiler counts $pr_value calls in V and $pr_ref in R; they must agree" >&2
    status=1
fi
if [ "$base_ref" != "$pr_ref" ]; then
    echo "structural check failed: R has $base_ref calls under the base compiler and $pr_ref under the PR's; R must be unchanged" >&2
    status=1
fi
if [ "$base_value" -le "$base_ref" ]; then
    echo "structural check failed: the base compiler counts $base_value calls in V and $base_ref in R; V must have more" >&2
    status=1
fi
exit "$status"
