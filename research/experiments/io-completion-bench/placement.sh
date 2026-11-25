# The CPU placement of the compare, scale and quick modes of redis-bench.sh and
# of the scale mode of linux-net-bench.sh: one rule, sourced by both.
#
# `place` puts n server CPUs on n distinct physical cores, one logical CPU to
# a core, performance cores only where the host lists them
# (/sys/devices/cpu_core/cpus), in CPU-number order, skipping the physical core
# of CPU 0, which the OS and the runner keep. The client gets every logical CPU
# of the remaining such cores, up to the count asked. On the native
# i9-14900K, whose adjacent CPU numbers are hyperthread siblings and whose
# CPUs 16-31 are efficiency cores, n = 2 gives server 2,4 and client 6-15.
# SERVER_CPU_LIST and CLIENT_CPU_LIST, comma-separated CPU numbers or ranges,
# override the two lists; the server list's first n CPUs are used. A client
# CPU that shares a physical core with a server CPU, or two server CPUs on one
# core, is refused. Each placement is printed as a placement line. The rule is
# Firn-wf's benchmark placement (its design node firn/measurement-sessions).
#
# PLACE_KEEP_CPU0=1 keeps the physical core of CPU 0 eligible, for a run that
# needs every performance core (n = 8 on the i9-14900K, which has eight).

# Sets SERVER_CPUS, CLIENT_CPUS and CLIENT_THREADS for n servers and at most
# the given client count; returns 2 when the host has too few cores for n.
place() {
    lists=$(python3 - "$1" "$2" "${SERVER_CPU_LIST:-}" "${CLIENT_CPU_LIST:-}" "${PLACE_KEEP_CPU0:-}" <<'PY'
import sys
from pathlib import Path
n, wanted, server_list, client_list = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
keep_cpu0 = sys.argv[5] == "1"
def parse(text):
    cpus = []
    for part in text.strip().split(","):
        if part:
            first, _, last = part.partition("-")
            cpus.extend(range(int(first), int(last or first) + 1))
    return cpus
root = Path("/sys/devices")
listed = root / "cpu_core" / "cpus"
online = root / "system" / "cpu" / "online"
eligible = parse(listed.read_text()) if listed.exists() else parse(online.read_text())
def core(cpu):
    topology = root / "system" / "cpu" / f"cpu{cpu}" / "topology"
    try:
        return ((topology / "physical_package_id").read_text().strip(),
                (topology / "core_id").read_text().strip())
    except OSError:
        return ("cpu", str(cpu))
cores = {}
for cpu in sorted(eligible):
    cores.setdefault(core(cpu), []).append(cpu)
reserved = None if keep_cpu0 else core(0)
ordered = [cpus for key, cpus in cores.items() if key != reserved]
if server_list:
    servers = parse(server_list)[:n]
else:
    servers = [cpus[0] for cpus in ordered[:n]]
if client_list:
    clients = parse(client_list)[:wanted]
else:
    clients = [cpu for cpus in ordered[n:] for cpu in cpus][:wanted]
server_cores = {core(cpu) for cpu in servers}
if len(servers) < n or not clients:
    sys.exit(2)
if len(server_cores) != len(servers) or server_cores & {core(cpu) for cpu in clients}:
    sys.exit("placement: a server CPU shares a physical core with another server or the client")
print(",".join(map(str, servers)), ",".join(map(str, clients)), len(clients))
PY
    ) || return $?
    set -- $lists
    SERVER_CPUS=$1
    CLIENT_CPUS=$2
    CLIENT_THREADS=$3
    echo "placement,$n servers,server=$SERVER_CPUS,client=$CLIENT_CPUS"
}
