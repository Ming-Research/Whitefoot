#!/bin/sh
# The check of placement.sh: `place` run on a fabricated sysfs shaped like the
# i9-14900K (CPUs 0-15 the performance cores, adjacent CPUs hyperthread
# siblings, CPUs 16-31 efficiency cores of one CPU each), against the
# placements the rule's own description gives. It needs python3 and a POSIX
# shell, and measures nothing.
#
#   sh placement-check.sh
set -e

HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
. "$HERE/placement.sh"

FAKE=$WORK/devices
mkdir -p "$FAKE/cpu_core" "$FAKE/cpu_atom" "$FAKE/system/cpu"
echo 0-15 > "$FAKE/cpu_core/cpus"
echo 16-31 > "$FAKE/cpu_atom/cpus"
echo 0-31 > "$FAKE/system/cpu/online"
cpu=0
while [ "$cpu" -lt 32 ]; do
    topology=$FAKE/system/cpu/cpu$cpu/topology
    mkdir -p "$topology"
    echo 0 > "$topology/physical_package_id"
    if [ "$cpu" -lt 16 ]; then echo $((cpu / 2)) > "$topology/core_id"; else echo "$cpu" > "$topology/core_id"; fi
    cpu=$((cpu + 1))
done
export PLACE_SYSFS_DEVICES=$FAKE

expect() {
    name=$1; wanted=$2; shift 2
    status=0
    got=$("$@" 2>/dev/null) || status=$?
    if [ "$got" != "$wanted" ]; then
        echo "FAIL $name: got '$got' (status $status), wanted '$wanted'" >&2
        exit 1
    fi
    echo "ok   $name"
}

placed() { place "$@" >/dev/null && echo "$SERVER_CPUS|$CLIENT_CPUS|$CLIENT_THREADS"; }
refused() { status=0; (place "$@") >/dev/null 2>&1 || status=$?; echo "refused $status"; }

# The rule's description: n = 2 gives server 2,4 and the client the other
# performance cores' CPUs, 6-15.
expect "two servers skip CPU 0's core" "2,4|6,7,8,9,10,11,12,13,14,15|10" placed 2 16
# CPU 0's core is the OS's, so eight servers do not fit on eight cores, and fit when it is kept.
expect "eight servers do not fit without CPU 0's core" "refused 2" refused 8 16
expect "eight servers fit when CPU 0's core is kept" "0,2,4,6,8,10,12,14|16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31|16" \
    env PLACE_KEEP_CPU0=1 SERVER_CPU_LIST= CLIENT_CPU_LIST=16-31 sh -c ". '$HERE/placement.sh'; place 8 64 >/dev/null && echo \"\$SERVER_CPUS|\$CLIENT_CPUS|\$CLIENT_THREADS\""
# One client list for every n.
expect "a client list given stays the same for one server" "2|16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31|16" \
    env CLIENT_CPU_LIST=16-31 sh -c ". '$HERE/placement.sh'; place 1 64 >/dev/null && echo \"\$SERVER_CPUS|\$CLIENT_CPUS|\$CLIENT_THREADS\""
# A client CPU on a server's physical core is refused.
expect "a client on a server's core is refused" "refused 1" \
    env SERVER_CPU_LIST=2 CLIENT_CPU_LIST=3 sh -c ". '$HERE/placement.sh'; (place 1 8) >/dev/null 2>&1 && echo ok || echo \"refused \$?\""
# Two servers on one core are refused.
expect "two servers on one core are refused" "refused 1" \
    env SERVER_CPU_LIST=2,3 CLIENT_CPU_LIST=16-31 sh -c ". '$HERE/placement.sh'; (place 2 8) >/dev/null 2>&1 && echo ok || echo \"refused \$?\""

echo "placement-check: every case behaved"
