#!/usr/bin/env python3
"""Temporary dispatch-only cost protocol; removed with its calling workflow.

Python owns matched process timing, TCP lifetime and retained partial results;
the native client keeps the existing independent byte oracle. No native tool
combines these observations or rebinds the same inputs across three revisions.
"""

import csv
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import statistics
import subprocess
import tempfile
import time
import traceback


ARMS = ("base", "twin", "branch")
ORDERS = (ARMS, ("twin", "branch", "base"), ("branch", "base", "twin"))
DRIVERS = (1, 4, 8, 16)
SPAWN_FIXTURES = {"spawn": "context_starts", "spawn-concurrent": "context_starts_concurrent"}
FIELDS = ("workload", "drivers", "cpus", "connections", "round", "order", "arm",
          "work", "exchanges", "bytes", "seconds", "status")


def integer(name, default, minimum, maximum):
    value = os.environ.get(name, str(default))
    if not re.fullmatch(r"[0-9]+", value):
        raise ValueError(f"{name} must be an integer")
    number = int(value)
    if not minimum <= number <= maximum:
        raise ValueError(f"{name} must be in {minimum}..{maximum}")
    return number


def cpu_list(value):
    if not re.fullmatch(r"[0-9]+(?:-[0-9]+)?(?:,[0-9]+(?:-[0-9]+)?)*", value):
        raise ValueError("expected a numeric CPU list")
    chosen = []
    for part in value.split(","):
        ends = [int(item) for item in part.split("-")]
        first, last = ends[0], ends[-1]
        if first > last or last - first > 1024:
            raise ValueError("invalid CPU range")
        chosen.extend(range(first, last + 1))
    if len(set(chosen)) != len(chosen):
        raise ValueError("a CPU list must not repeat a CPU")
    return chosen


def topology_masks(value, cpuinfo, record):
    """Qualify eight physical P cores before assigning any panel that name."""
    record["requested"] = value
    record["allowed_affinity"] = sorted(os.sched_getaffinity(0))
    hypervisor = Path("/sys/hypervisor/type")
    record["hypervisor_type"] = hypervisor.read_text().strip() if hypervisor.exists() else None
    if record["hypervisor_type"] or re.search(r"(?m)^flags\s*:.*\bhypervisor\b", cpuinfo):
        raise RuntimeError("P-core panels require exposed physical hardware, not virtual CPUs")
    root = Path("/sys/devices/system/cpu")
    performance = Path("/sys/devices/cpu_core/cpus")
    if not performance.is_file():
        raise RuntimeError("missing cpu_core/cpus: cannot identify physical performance cores")
    record["performance_cpus"] = performance.read_text().strip()
    record["online_cpus"] = (root / "online").read_text().strip()
    p_cpus = set(cpu_list(record["performance_cpus"]))
    online = set(cpu_list(record["online_cpus"]))
    available = set(record["allowed_affinity"])
    record["cpus"] = {}
    cores = {}
    for cpu in sorted(p_cpus):
        topology = root / f"cpu{cpu}" / "topology"
        try:
            facts = {name: (topology / name).read_text().strip()
                     for name in ("physical_package_id", "core_id", "thread_siblings_list")}
            kind = topology / "core_type"
            facts["core_type"] = kind.read_text().strip() if kind.exists() else None
            record["cpus"][str(cpu)] = facts
            package, core = int(facts["physical_package_id"]), int(facts["core_id"])
            if package < 0 or core < 0:
                raise ValueError("missing physical package/core identity")
            cores.setdefault((package, core), []).append(cpu)
        except (OSError, ValueError) as error:
            raise RuntimeError(f"CPU {cpu}: incomplete physical topology: {error}") from error
    if len(cores) != 8 or len(p_cpus) != 16 or len({key[0] for key in cores}) != 1:
        raise RuntimeError("14900K panels require exactly eight P cores with two threads each on one package")
    if not p_cpus <= online or not p_cpus <= available:
        raise RuntimeError("all sixteen P-core threads must be online and allowed by runner affinity")
    for pair in cores.values():
        if len(pair) != 2 or any(
                set(cpu_list(record["cpus"][str(cpu)]["thread_siblings_list"])) != set(pair)
                for cpu in pair):
            raise RuntimeError("P-core topology must expose reciprocal two-thread physical sibling pairs")
        types = {record["cpus"][str(cpu)]["core_type"] for cpu in pair}
        if len(types) != 1:
            raise RuntimeError("P-core siblings disagree on the exposed core type")
    pairs = sorted(cores.values(), key=min)
    if value == "auto":
        chosen = [pair[0] for pair in pairs] + [pair[1] for pair in pairs]
    else:
        chosen = cpu_list(value)
        if len(chosen) != 16 or set(chosen) != p_cpus:
            raise ValueError("DEADLOCK_P_CORE_CPUS must name all sixteen qualified P-core threads")
        chosen_pairs = [{chosen[index], chosen[index + 8]} for index in range(8)]
        if {frozenset(pair) for pair in chosen_pairs} != {frozenset(pair) for pair in pairs}:
            raise ValueError("first eight CPUs must name distinct P cores; last eight must be their siblings in order")
    record["ordered_p_core_threads"] = chosen
    masks = {drivers: ",".join(map(str, chosen[:drivers])) for drivers in DRIVERS}
    record["panel_masks"] = masks
    record["qualification"] = "cpu_core PMU membership plus package/core IDs and reciprocal SMT siblings"
    return masks


def runtime_environment(drivers):
    env = os.environ.copy()
    for name in list(env):
        if name.startswith("WF_"):
            del env[name]
    env["WF_DRIVERS"] = str(drivers)
    return env


class Protocol:
    def __init__(self, out):
        self.started = time.perf_counter()
        self.out = out
        self.logs = out / "logs"
        self.logs.mkdir(parents=True, exist_ok=True)
        self.rows = []
        self.builds = []
        self.manifest = {"status": "incomplete", "tcp_panel": "included"}
        self.worktrees = []
        self.failure = None

    def note(self, text):
        print(text, flush=True)
        with (self.out / "protocol.log").open("a") as log:
            log.write(text + "\n")

    def save(self):
        self.manifest["total_elapsed_seconds"] = time.perf_counter() - self.started
        self.write("manifest.json", json.dumps(self.manifest, indent=2) + "\n")
        for name, fields, rows in [
            ("raw.tsv", FIELDS, self.rows),
            ("build-times.tsv", ("arm", "stage", "seconds", "status"), self.builds),
        ]:
            stream = io.StringIO(newline="")
            writer = csv.DictWriter(stream, fields, delimiter="\t")
            writer.writeheader()
            writer.writerows(rows)
            self.write(name, stream.getvalue())

    def write(self, name, text):
        temporary = self.out / (name + ".tmp")
        temporary.write_text(text)
        os.replace(temporary, self.out / name)

    def command(self, argv, label, cwd=None, env=None, timeout=1800, arm=None, stage=None):
        self.note(f"{label}: {json.dumps(list(map(str, argv)))}")
        started = time.perf_counter()
        code = "not-started"
        elapsed = None
        result = None
        try:
            result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, timeout=timeout)
            elapsed = time.perf_counter() - started
            code = result.returncode
            (self.logs / f"{label}.stdout").write_bytes(result.stdout)
            (self.logs / f"{label}.stderr").write_bytes(result.stderr)
            if code != 0:
                raise RuntimeError(f"{label} exited {code}; see retained logs")
            return result
        except subprocess.TimeoutExpired as error:
            elapsed = time.perf_counter() - started
            code = "timeout"
            (self.logs / f"{label}.stdout").write_bytes(error.stdout or b"")
            (self.logs / f"{label}.stderr").write_bytes(error.stderr or b"")
            raise RuntimeError(f"{label} exceeded {timeout}s") from error
        finally:
            elapsed = elapsed if elapsed is not None else time.perf_counter() - started
            self.last_status = code
            self.last_duration = elapsed
            with (self.out / "commands.jsonl").open("a") as record:
                record.write(json.dumps({"label": label, "argv": list(map(str, argv)),
                                         "status": code, "seconds": elapsed}) + "\n")
            if stage is not None:
                self.builds.append({"arm": arm or "shared", "stage": stage,
                                    "seconds": elapsed, "status": code})
                self.save()

    def git(self, *arguments):
        return subprocess.check_output(["git", *arguments]).decode().strip()

    def prepare(self):
        if platform.system() != "Linux":
            raise RuntimeError("this protocol requires the Linux 14900K runner")
        lock = Path(os.environ.get("WHITEFOOT_CHECK_LOCK_DIR", ""))
        owner = os.environ.get("WHITEFOOT_CHECK_OWNER", "")
        if not owner.isdigit() or not lock.is_dir() or (lock / "pid").read_text().strip() != owner:
            raise RuntimeError("invoke through run-check.pl: the host-wide lock is required")
        os.kill(int(owner), 0)
        model = Path("/proc/cpuinfo").read_text()
        if "14900K" not in model:
            raise RuntimeError("precise comparison requires the i9-14900K host")
        self.manifest["cpuinfo"] = model
        self.mode = os.environ.get("DEADLOCK_MODE", "pilot")
        if self.mode not in ("pilot", "full"):
            raise ValueError("DEADLOCK_MODE must be pilot or full")
        rounds = integer("DEADLOCK_ROUNDS", 9, 3, 99)
        if rounds % 3:
            raise ValueError("full rounds must be a multiple of three")
        work = integer("DEADLOCK_WORK", 10, 1, 1000)
        self.rounds, self.work = (3, 1) if self.mode == "pilot" else (rounds, work)
        self.manifest["topology"] = {}
        self.masks = topology_masks(os.environ.get("DEADLOCK_P_CORE_CPUS", "auto"),
                                    model, self.manifest["topology"])
        self.threads = integer("DEADLOCK_CLIENT_THREADS", 4, 1, 64)
        self.payload = integer("DEADLOCK_BYTES", 64, 1, 65536)
        exchanges = integer("DEADLOCK_EXCHANGES", 100, 2, 100000)
        self.exchanges = 2 if self.mode == "pilot" else exchanges
        self.timeout = integer("DEADLOCK_SAMPLE_TIMEOUT", 120, 1, 1800)
        extra = os.environ.get("DEADLOCK_TCP_1024", "false")
        if extra not in ("true", "false"):
            raise ValueError("DEADLOCK_TCP_1024 must be true or false")
        self.connections = [64, 1024] if extra == "true" else [64]
        client_source = os.environ["DEADLOCK_NETLOAD_SOURCE"]
        source_path = Path(client_source)
        if source_path.is_absolute() or ".." in source_path.parts or not client_source.endswith(".c"):
            raise ValueError("DEADLOCK_NETLOAD_SOURCE must be a repository-relative C source")
        for tool in ("git", "rustup", "rustc", "cargo", "taskset", "/usr/bin/clang", "ld.lld"):
            if shutil.which(tool) is None:
                raise RuntimeError(f"missing host tool: {tool}")
        self.command(["rustup", "default", "stable"], "rustup", stage="toolchain")
        os.environ["RUSTUP_TOOLCHAIN"] = "stable"
        tools = {}
        for tool, args in [("rustc", ["-vV"]), ("cargo", ["--version"]),
                           ("/usr/bin/clang", ["--version"]), ("ld.lld", ["--version"])]:
            tools[tool] = self.command([tool, *args], f"version-{Path(tool).name}").stdout.decode()
        self.command(["git", "fetch", "--no-tags", "origin",
                      "+refs/heads/main:refs/remotes/origin/main"], "fetch-main")
        candidate = self.git("rev-parse", "--verify", os.environ["DEADLOCK_CANDIDATE"] + "^{commit}")
        main = self.git("rev-parse", "origin/main")
        base = self.git("merge-base", main, candidate)
        self.manifest.update({"mode": self.mode, "rounds": self.rounds, "work": self.work,
                              "requested_rounds": rounds, "requested_work": work,
                              "panel_cpus": self.masks, "drivers": list(DRIVERS), "client_threads": self.threads,
                              "tcp_bytes": self.payload, "tcp_exchanges": self.exchanges,
                              "tcp_churn_exchanges": 1, "connections": self.connections,
                              "tcp_placement": "Client threads share each server mask; client/server oversubscription is retained identically across arms.",
                              "sample_timeout": self.timeout, "candidate": candidate,
                              "fetched_main": main, "merge_base": base,
                              "toolchain": tools, "compiler_profile": "gate",
                              "cargo_build_jobs": os.environ.get("CARGO_BUILD_JOBS", "Cargo default"),
                              "native_flags": ["--par"], "host": platform.uname()._asdict(),
                              "run_id": os.environ.get("GITHUB_RUN_ID"),
                              "criterion": "median(branch/base-1) > 0.01 + max(abs(twin/base-1))",
                              "limitation": "Wall samples include process launch; TCP includes server start, client and shutdown. No warmup samples are discarded."})
        bundle = self.out / "bundle"
        bundle.mkdir(exist_ok=True)
        sources = ["tests/programs/context_starts.wf", "tests/programs/context_starts_concurrent.wf",
                   "tests/programs/tcp_contexts.wf",
                   "tests/programs/driver_count.wf", client_source]
        self.manifest["sources"] = {}
        for path in sources:
            data = subprocess.check_output(["git", "show", f"{candidate}:{path}"])
            target = bundle / Path(path).name
            target.write_bytes(data)
            self.manifest["sources"][path] = hashlib.sha256(data).hexdigest()
        self.scratch = Path(tempfile.mkdtemp(prefix="deadlock-sites-", dir=os.environ["RUNNER_TEMP"]))
        self.executables = {}
        self.manifest["images"] = {}
        for arm in ARMS:
            revision = candidate if arm == "branch" else base
            tree = self.scratch / arm
            self.command(["git", "worktree", "add", "--detach", str(tree), revision], f"checkout-{arm}")
            self.worktrees.append(tree)
            build_env = os.environ.copy()
            build_env.update({"CARGO_TARGET_DIR": str(self.scratch / f"target-{arm}"),
                              "CARGO_INCREMENTAL": "0", "RUSTFLAGS": "",
                              "RUSTC_WRAPPER": "", "RUSTC_WORKSPACE_WRAPPER": ""})
            build_env.pop("CARGO_ENCODED_RUSTFLAGS", None)
            build_env.pop("CARGO_BUILD_TARGET", None)
            manifest = str(tree / "compiler/Cargo.toml")
            self.command(["cargo", "fetch", "--locked", "--manifest-path", manifest],
                         f"fetch-{arm}", env=build_env, arm=arm, stage="dependencies")
            self.command(["cargo", "build", "--manifest-path", manifest, "--profile", "gate",
                          "--bin", "whitefootc", "--locked", "--offline"],
                         f"compiler-{arm}", env=build_env, arm=arm, stage="compiler")
            compiler = Path(build_env["CARGO_TARGET_DIR"]) / "gate/whitefootc"
            self.manifest["images"][arm] = {"revision": revision,
                                                   "compiler_sha256": hashlib.sha256(compiler.read_bytes()).hexdigest()}
            self.executables[arm] = {}
            for fixture in (*SPAWN_FIXTURES.values(), "tcp_contexts", "driver_count"):
                image = self.scratch / f"{arm}-{fixture}"
                self.command([str(compiler), "--par", str(bundle / f"{fixture}.wf"), "-o", str(image)],
                             f"native-{arm}-{fixture}", cwd=tree, env=runtime_environment(1),
                             arm=arm, stage=f"native-{fixture}")
                self.executables[arm][fixture] = image
                self.manifest["images"][arm][fixture + "_sha256"] = hashlib.sha256(image.read_bytes()).hexdigest()
        self.client = self.scratch / "netload"
        self.command(["/usr/bin/clang", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                      "-pthread", str(bundle / source_path.name), "-o", str(self.client)],
                     "native-client", stage="native-client")
        self.manifest["client_sha256"] = hashlib.sha256(self.client.read_bytes()).hexdigest()
        for drivers in DRIVERS:
            for arm in ARMS:
                result = self.command(["taskset", "-c", self.masks[drivers],
                                       str(self.executables[arm]["driver_count"])],
                                      f"preflight-{arm}-{drivers}", env=runtime_environment(drivers),
                                      timeout=self.timeout)
                expected = (f"{drivers:02d}\n" * 2).encode()
                if result.stdout != expected or result.stderr:
                    raise RuntimeError(f"{arm}/{drivers}: actual runtime driver-count preflight failed")
        self.manifest["preflight"] = "all three images published exactly 1, 4, 8 and 16 drivers before and after spawn"
        self.save()

    def listening(self, port, server):
        inodes = set()
        for descriptor in Path(f"/proc/{server.pid}/fd").iterdir():
            try:
                target = os.readlink(descriptor)
            except FileNotFoundError:
                continue
            if target.startswith("socket:["):
                inodes.add(target[8:-1])
        for table in ("/proc/net/tcp", "/proc/net/tcp6"):
            for line in Path(table).read_text().splitlines()[1:]:
                columns = line.split()
                if columns[3] == "0A" and int(columns[1].split(":")[1], 16) == port and columns[9] in inodes:
                    return True
        return False

    def tcp_batch(self, arm, drivers, connections, exchanges, label):
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        argv = ["taskset", "-c", self.masks[drivers], str(self.executables[arm]["tcp_contexts"]),
                str(port), str(connections)]
        client_argv = ["taskset", "-c", self.masks[drivers], str(self.client), str(port), str(connections),
                       str(exchanges), str(self.payload), "--threads", str(self.threads)]
        self.note(f"{label}: server={json.dumps(argv)} client={json.dumps(client_argv)}")
        server = None
        details = {"label": label, "arm": arm, "drivers": drivers, "port": port,
                   "connections": connections, "exchanges": exchanges, "bytes": self.payload,
                   "server_status": None, "client_status": None}
        started = None
        elapsed = None
        client_stdout, client_stderr = b"", b""
        try:
            with (self.logs / f"{label}.server.stdout").open("wb") as stdout, \
                 (self.logs / f"{label}.server.stderr").open("wb") as stderr:
                started = time.perf_counter()
                deadline = time.monotonic() + self.timeout
                server = subprocess.Popen(argv, env=runtime_environment(drivers), stdout=stdout, stderr=stderr)
                while True:
                    if server.poll() is not None:
                        raise RuntimeError(f"{label}: server exited before listening: {server.returncode}")
                    if self.listening(port, server):
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{label}: listener deadline")
                    time.sleep(0.001)
                try:
                    client = subprocess.run(client_argv, capture_output=True,
                                            timeout=max(0.001, deadline - time.monotonic()))
                except subprocess.TimeoutExpired as error:
                    details["client_status"] = "timeout"
                    client_stdout, client_stderr = error.stdout or b"", error.stderr or b""
                    raise
                details["client_status"] = client.returncode
                client_stdout, client_stderr = client.stdout, client.stderr
                if client.returncode != 0:
                    raise RuntimeError(f"{label}: client exited {client.returncode}")
                server.wait(timeout=max(0.001, deadline - time.monotonic()))
                elapsed = time.perf_counter() - started
                details["server_status"] = server.returncode
                if server.returncode != 0:
                    raise RuntimeError(f"{label}: server exited {server.returncode}")
                fields = client_stdout.decode().strip().split("\t")
                metrics = dict(field.split("=", 1) for field in fields)
                if len(metrics) != len(fields) or int(metrics["roundtrips"]) != connections * exchanges:
                    raise RuntimeError(f"{label}: incomplete client work")
                if client_stderr or any(not math.isfinite(float(value)) for value in metrics.values()):
                    raise RuntimeError(f"{label}: invalid client metrics")
                details["client_metrics"] = metrics
            if (self.logs / f"{label}.server.stdout").stat().st_size or (self.logs / f"{label}.server.stderr").stat().st_size:
                raise RuntimeError(f"{label}: unexpected server output")
        finally:
            if server is not None:
                if server.poll() is None:
                    server.terminate()
                    try:
                        server.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        server.kill()
                        server.wait()
                details["server_status"] = server.returncode
            if elapsed is None:
                elapsed = time.perf_counter() - started if started is not None else 0
            self.last_duration = elapsed
            details["seconds"] = elapsed
            details["server_argv"] = argv
            details["client_argv"] = client_argv
            (self.logs / f"{label}.client.stdout").write_bytes(client_stdout)
            (self.logs / f"{label}.client.stderr").write_bytes(client_stderr)
            with (self.out / "tcp-batches.jsonl").open("a") as record:
                record.write(json.dumps(details) + "\n")

    def sample(self, workload, arm, drivers, connections, exchanges, round_index, order):
        label = f"{workload}-{drivers}-{connections}-{round_index}-{arm}"
        row = dict(zip(FIELDS, (workload, drivers, self.masks[drivers], connections, round_index, order, arm,
                               self.work, exchanges, self.payload if connections else 0, 0, "incomplete")))
        self.rows.append(row)
        self.save()
        self.summary()
        try:
            for batch in range(self.work):
                self.last_duration = 0
                try:
                    if workload in SPAWN_FIXTURES:
                        result = self.command(["taskset", "-c", self.masks[drivers],
                                               str(self.executables[arm][SPAWN_FIXTURES[workload]])],
                                              f"{label}-{batch}", env=runtime_environment(drivers),
                                              timeout=self.timeout)
                        if result.stdout or result.stderr:
                            raise RuntimeError(f"{label}: unexpected spawn fixture output")
                    else:
                        self.tcp_batch(arm, drivers, connections, exchanges, f"{label}-{batch}")
                finally:
                    row["seconds"] += self.last_duration
                    self.save()
            row["status"] = "valid"
        except Exception:
            row["status"] = "failed"
            raise
        finally:
            self.save()
            self.summary()

    def measure(self):
        panels = [(workload, 0, 0) for workload in SPAWN_FIXTURES]
        for connections in self.connections:
            panels.extend([("tcp-exchange", connections, self.exchanges), ("tcp-churn", connections, 1)])
        for workload, connections, exchanges in panels:
            for drivers in DRIVERS:
                for round_index in range(self.rounds):
                    order = ORDERS[round_index % 3]
                    for position, arm in enumerate(order):
                        self.sample(workload, arm, drivers, connections, exchanges, round_index, position)

    def summary(self):
        lines = ["# Deadlock-sites cost", "", f"Status: {self.manifest['status']}.", ""]
        if self.failure:
            lines.extend([f"Failure: {self.failure}", "", "Partial rows are retained; no incomplete panel has a verdict.", ""])
        if "panel_cpus" in self.manifest:
            lines.extend(["Qualified P-core placement (same mask for every arm and TCP client):", "",
                          "| Drivers | Physical P cores | Threads per core | CPU mask |",
                          "|---:|---:|---:|---|"])
            for drivers in DRIVERS:
                lines.append(f"| {drivers} | {min(drivers, 8)} | {2 if drivers == 16 else 1} | {self.masks[drivers]} |")
            lines.append("")
        lines.extend(["Build/dependency/native compilation times are separate in build-times.tsv.",
                      f"Elapsed protocol: {time.perf_counter() - self.started:.3f}s; recorded build stages: {sum(row['seconds'] for row in self.builds):.3f}s; measured native runtime: {sum(row['seconds'] for row in self.rows):.3f}s.",
                      "All source bytes, toolchain facts, commands, exits and TCP client metrics are retained.",
                      "Wall time includes launch costs; TCP also includes listener readiness, client start and server shutdown.",
                      "TCP client threads share the server mask, so the two processes oversubscribe it; every arm uses identical placement.",
                      "Pilot observations select later full work; pilot never gives a cost verdict. No warmups are discarded.",
                      "Both spawn fixtures use every driver panel. Spawn rejection: median(branch/base − 1) > 0.01 + max(abs(twin/base − 1)).",
                      "Finite within-bound evidence requires median(overhead) + twin spread ≤ 0.01; other nonrejected full panels are inconclusive.", "",
                      "| Panel | Drivers | Min sample s | Median branch/base − 1 | Twin spread | Rejection threshold | Result |",
                      "|---|---:|---:|---:|---:|---:|---|"])
        ratios = []
        rejected = False
        keys = sorted({(row["workload"], row["drivers"], row["connections"]) for row in self.rows})
        for key in keys:
            rows = [row for row in self.rows if (row["workload"], row["drivers"], row["connections"]) == key]
            overheads, twins = [], []
            for round_index in range(self.rounds):
                matched = [row for row in rows if row["round"] == round_index]
                if len(matched) != 3 or any(row["status"] != "valid" for row in matched):
                    continue
                timed = {row["arm"]: row["seconds"] for row in matched}
                if set(timed) != set(ARMS):
                    continue
                overhead = timed["branch"] / timed["base"] - 1
                twin = timed["twin"] / timed["base"] - 1
                overheads.append(overhead)
                twins.append(twin)
                ratios.append({"workload": key[0], "drivers": key[1], "connections": key[2],
                               "round": round_index, "branch_base_ratio": timed["branch"] / timed["base"],
                               "twin_base_ratio": timed["twin"] / timed["base"],
                               "overhead": overhead, "twin_overhead": twin,
                               "wall_seconds": timed,
                               "wall_roundtrips_per_second": {
                                   arm: self.work * key[2] * rows[0]["exchanges"] / timed[arm]
                                   for arm in ARMS
                               } if key[2] else None})
            if len(rows) != self.rounds * 3 or len(overheads) != self.rounds:
                lines.append(f"| {key[0]} {key[2]} | {key[1]} | — | — | — | — | incomplete |")
                continue
            median, spread = statistics.median(overheads), max(map(abs, twins))
            if self.mode == "pilot":
                result = "pilot: no verdict"
            elif key[0] not in SPAWN_FIXTURES:
                result = "TCP evidence; outside spawn veto"
            elif median > 0.01 + spread:
                result = "reject"
                rejected = True
            elif median + spread <= 0.01:
                result = "within recorded noise bound"
            else:
                result = "inconclusive"
            threshold = f"{0.01 + spread:.6%}" if key[0] in SPAWN_FIXTURES else "outside veto"
            lines.append(f"| {key[0]} {key[2]} | {key[1]} | {min(row['seconds'] for row in rows):.6f} | {median:.6%} | {spread:.6%} | {threshold} | {result} |")
        lines.extend(["", "Matched rounds (wall ratios; TCP rates include startup and shutdown, unlike the client exchange-only metrics):", "",
                      "| Panel | Drivers | Round | Branch/base | Twin/base | Base rt/s | Twin rt/s | Branch rt/s |",
                      "|---|---:|---:|---:|---:|---:|---:|---:|"])
        for ratio in ratios:
            rates = ratio["wall_roundtrips_per_second"]
            rate_cells = " | ".join(f"{rates[arm]:.3f}" if rates else "—" for arm in ARMS)
            lines.append(f"| {ratio['workload']} {ratio['connections']} | {ratio['drivers']} | {ratio['round']} | {ratio['branch_base_ratio']:.9f} | {ratio['twin_base_ratio']:.9f} | {rate_cells} |")
        self.write("ratios.json", json.dumps(ratios, indent=2) + "\n")
        self.write("summary.md", "\n".join(lines) + "\n")
        return rejected

    def cleanup(self):
        for tree in reversed(self.worktrees):
            result = subprocess.run(["git", "worktree", "remove", "--force", str(tree)], capture_output=True)
            if result.returncode:
                self.note(f"scratch worktree cleanup failed: {tree}: {result.stderr.decode(errors='replace')}")
        if hasattr(self, "scratch") and not any(tree.exists() for tree in self.worktrees):
            shutil.rmtree(self.scratch)


def main():
    out = Path(os.environ["DEADLOCK_OUT"])
    out.mkdir(parents=True, exist_ok=True)
    protocol = Protocol(out)
    code = 0
    try:
        protocol.save()
        protocol.summary()
        protocol.prepare()
        protocol.measure()
        protocol.manifest["status"] = "complete"
    except Exception as error:
        code = 1
        protocol.failure = str(error)
        protocol.manifest["status"] = "failed"
        (out / "failure.log").write_text(traceback.format_exc())
        protocol.note(f"FAILED: {error}")
    finally:
        protocol.save()
        rejected = protocol.summary()
        protocol.cleanup()
    return code or (2 if rejected else 0)


if __name__ == "__main__":
    raise SystemExit(main())
