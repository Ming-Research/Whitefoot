#!/usr/bin/env python3
"""CI-only, same-source Firn network panel; invoked by hash-shrink CI.

Measures client-observed depth-one latency, including network/client overhead.
Internal atomic-hold duration and bucket capacity are not observable over RESP.
The threshold case predicts capacity from the pinned source's growth rule.
No verdict here establishes zero slowdown or application compatibility.
"""

import argparse
import contextlib
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import signal
import socket
import statistics
import subprocess
import threading
import time

FIRN_REVISION = "c6f6058fb17277d4d44d364a45efad1eae01a22b"
VARIANTS = ("base", "twin", "head")
CASES = ("sadd_fresh", "spop_sparse", "spop_threshold", "spop_full_drain",
         "lrange_100", "spop_contention")


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


class Resp:
    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=120)
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.reader = self.sock.makefile("rb")

    def close(self):
        self.reader.close()
        self.sock.close()

    def send(self, *parts):
        values = [part if isinstance(part, bytes) else str(part).encode() for part in parts]
        self.sock.sendall(b"*%d\r\n" % len(values) + b"".join(
            b"$%d\r\n" % len(value) + value + b"\r\n" for value in values))

    def read(self):
        line = self.reader.readline()
        if not line.endswith(b"\r\n"):
            raise RuntimeError("truncated RESP reply")
        tag, body = line[:1], line[1:-2]
        if tag == b"-":
            raise RuntimeError("Firn error: " + body.decode(errors="replace"))
        if tag == b"+":
            return body
        if tag == b":":
            return int(body)
        if tag == b"$":
            length = int(body)
            if length == -1:
                return None
            value = self.reader.read(length)
            if len(value) != length or self.reader.read(2) != b"\r\n":
                raise RuntimeError("truncated RESP bulk")
            return value
        if tag == b"*":
            length = int(body)
            return None if length == -1 else [self.read() for _ in range(length)]
        raise RuntimeError("unsupported RESP tag " + repr(tag))

    def call(self, *parts):
        self.send(*parts)
        return self.read()


def expect(actual, expected):
    if actual != expected:
        raise RuntimeError(f"unexpected reply: {actual!r}; expected {expected!r}")


@contextlib.contextmanager
def server(binary, cpus, directory):
    directory.mkdir(parents=True, exist_ok=True)
    with socket.socket() as available:
        available.bind(("127.0.0.1", 0))
        port = available.getsockname()[1]
    env = dict(os.environ, WF_DRIVERS=str(len(cpus)))
    command = ["taskset", "-c", ",".join(map(str, cpus)), str(binary), str(port), "0", "-", "0"]
    write_json(directory / "command.json", command)
    with (directory / "server.log").open("wb") as log:
        child = subprocess.Popen(command, cwd=directory, env=env, stdout=log,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        connection = None
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if child.poll() is not None:
                    raise RuntimeError(f"Firn exited {child.returncode}: {directory / 'server.log'}")
                try:
                    connection = Resp(port)
                    expect(connection.call("PING"), b"PONG")
                    break
                except OSError:
                    if connection:
                        connection.close()
                        connection = None
                    time.sleep(0.025)
            if connection is None:
                raise RuntimeError("Firn startup timed out")
            yield connection, port
        finally:
            if connection:
                connection.close()
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=5)


def populate(client, count):
    # A single first member fixes sadd_body's initial capacity at four.
    expect(client.call("SADD", "members", "m0"), 1)
    for first in range(1, count, 512):
        stop = min(count, first + 512)
        expect(client.call("SADD", "members", *[f"m{i}" for i in range(first, stop)]), stop - first)
    expect(client.call("SCARD", "members"), count)


def retain(client, count, remaining):
    for first in range(0, count - remaining, 512):
        stop = min(count - remaining, first + 512)
        expect(client.call("SREM", "members", *[f"m{i}" for i in range(first, stop)]), stop - first)
    expect(client.call("SCARD", "members"), remaining)


def predicted_capacity(count):
    # Pinned sadd_body initializes max(first-command members, 4); first has one.
    # With unique insertions, no tombstones or ceiling contact: double when
    # the prior live count reaches C - floor(C/4). Ceiling is 2**32.
    capacity = 4
    while count > capacity - capacity // 4:
        capacity *= 2
    return capacity


def timed(client, command):
    start = time.perf_counter_ns()
    result = client.call(*command)
    end = time.perf_counter_ns()
    return result, {"start_ns": start, "end_ns": end, "latency_ns": end - start}


def run_case(client, port, case, members, remaining, requests):
    samples, waits = [], []
    setup_start = time.perf_counter()
    details = {}
    if case == "lrange_100":
        expected = [str(i).encode() for i in range(100)]
        expect(client.call("RPUSH", "items", *expected), 100)
    elif case != "sadd_fresh":
        populate(client, members)
        if case == "spop_threshold":
            capacity = predicted_capacity(members)
            remaining = capacity // 8
            details = {"predicted_capacity_before": capacity,
                       "capacity_observed": False, "remaining_before": remaining,
                       "predicted_head_capacity_after": max(8, 4 * (remaining - 1))}
        if case == "spop_full_drain":
            remaining = members
        else:
            retain(client, members, remaining)
    setup_seconds = time.perf_counter() - setup_start
    measure_start = time.perf_counter_ns()
    if case == "sadd_fresh":
        for number in range(requests):
            reply, sample = timed(client, ("SADD", "members", f"m{number}"))
            expect(reply, 1)
            samples.append(sample)
        expect(client.call("SCARD", "members"), requests)
    elif case in ("spop_sparse", "spop_threshold"):
        count = 1 if case == "spop_threshold" else min(requests, remaining)
        seen = set()
        expected_members = {f"m{i}".encode() for i in range(members - remaining, members)}
        for _ in range(count):
            reply, sample = timed(client, ("SPOP", "members"))
            if reply not in expected_members or reply in seen:
                raise RuntimeError("SPOP returned absent or duplicate member")
            seen.add(reply)
            samples.append(sample)
        expect(client.call("SCARD", "members"), remaining - count)
    elif case == "lrange_100":
        for _ in range(requests):
            reply, sample = timed(client, ("LRANGE", "items", 0, 99))
            expect(reply, expected)
            samples.append(sample)
    else:
        # Competing depth-one SCARD calls share the same set. The send signal
        # schedules competition after the drain request is sent; timestamps
        # record network overlap, which cannot prove overlap inside the lock.
        errors = []
        sent = threading.Event()
        ready = threading.Event()
        stop = threading.Event()

        def competitor():
            other = None
            try:
                other = Resp(port)
                ready.set()
                if not sent.wait(timeout=120):
                    raise RuntimeError("drain request was not sent")
                for _ in range(requests):
                    reply, sample = timed(other, ("SCARD", "members"))
                    if not isinstance(reply, int) or not 0 <= reply <= remaining:
                        raise RuntimeError("invalid competing SCARD result")
                    sample["reply"] = reply
                    waits.append(sample)
                    if stop.is_set():
                        break
            except BaseException as error:
                errors.append(str(error))
            finally:
                if other:
                    other.close()

        worker = threading.Thread(target=competitor) if case == "spop_contention" else None
        if worker:
            worker.start()
        try:
            if worker and not ready.wait(timeout=10):
                raise RuntimeError("competing connection failed to become ready")
            start = time.perf_counter_ns()
            client.send("SPOP", "members", remaining)
            sent.set()
            reply = client.read()
            end = time.perf_counter_ns()
            samples.append({"start_ns": start, "end_ns": end, "latency_ns": end - start})
        finally:
            stop.set()
            sent.set()
            if worker:
                worker.join(timeout=125)
                if worker.is_alive():
                    raise RuntimeError("competing client failed to stop")
        if errors:
            raise RuntimeError("competing client: " + "; ".join(errors))
        expected_members = {f"m{i}".encode() for i in range(members - remaining, members)}
        if not isinstance(reply, list) or len(reply) != remaining or set(reply) != expected_members:
            raise RuntimeError("full drain returned an incorrect member set")
        expect(client.call("SCARD", "members"), 0)
        for wait in waits:
            wait["overlaps_drain_network_interval"] = wait["start_ns"] < end and wait["end_ns"] > start
    measure_end = time.perf_counter_ns()
    span = samples[-1]["end_ns"] - samples[0]["start_ns"]
    # Service-time sum is independent of untimed validation and Python gaps.
    # Span is retained to expose those gaps rather than hiding client overhead.
    elapsed = sum(sample["latency_ns"] for sample in samples)
    return dict(setup_seconds=setup_seconds, samples=samples, competing_samples=waits,
                latency_sum_ns=elapsed, elapsed_ns=span,
                client_span_including_validation_ns=measure_end - measure_start,
                request_count=len(samples), throughput_rps=len(samples) * 1e9 / span,
                p99_ns=(percentile([sample["latency_ns"] for sample in samples], 0.99)
                        if len(samples) >= 100 else None),
                latency_sample_count=len(samples), p99_qualified=False,
                tail_note="Empirical quantile only; fewer than 100 samples leaves p99 absent. No registered tail acceptance is established.",
                max_ns=max(sample["latency_ns"] for sample in samples),
                internal_hold_ns=None, internal_hold_observable=False, **details)


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def interval(ratios):
    if len(ratios) < 2:
        return {"pairs": len(ratios), "geomean": ratios[0] if ratios else None,
                "low95": None, "high95": None}
    logs = [math.log(ratio) for ratio in ratios]
    # Conservative t critical values, two-sided 95%, exact through df=10,
    # then the lower enclosing df bucket; no normal approximation for tiny n.
    table = {1: 12.706, 2: 4.303, 3: 3.182, 4: 2.776, 5: 2.571,
             6: 2.447, 7: 2.365, 8: 2.306, 9: 2.262, 10: 2.228,
             20: 2.086, 30: 2.042, 60: 2.000, 120: 1.980}
    df = len(logs) - 1
    critical = table[max(key for key in table if key <= df)]
    center = statistics.mean(logs)
    radius = critical * statistics.stdev(logs) / math.sqrt(len(logs))
    return dict(pairs=len(ratios), median=statistics.median(ratios),
                geomean=math.exp(center), low95=math.exp(center - radius),
                high95=math.exp(center + radius))


def summarize(records):
    output = []
    for case in CASES:
        for cores in (1, 2):
            for cohort in sorted({row["cohort"] for row in records}):
                rows = {(row["round"], row["variant"]): row for row in records
                        if row["case"] == case and row["cores"] == cores and row["cohort"] == cohort}
                rounds = sorted({key[0] for key in rows})
                for metric in ("elapsed_ns", "p99_ns", "max_ns"):
                    comparisons = {}
                    for variant in ("twin", "head"):
                        ratios = [rows[r, variant][metric] / rows[r, "base"][metric]
                                  for r in rounds if (r, variant) in rows and (r, "base") in rows
                                  and rows[r, variant][metric] is not None and rows[r, "base"][metric] is not None]
                        comparisons[variant + "_over_base"] = interval(ratios)
                    twin = comparisons["twin_over_base"]
                    precise = (twin.get("median") is not None and 0.99 <= twin["median"] <= 1.01
                               and twin["low95"] is not None and twin["high95"] - twin["low95"] <= 0.02)
                    pooled = {}
                    for variant in VARIANTS:
                        samples = [sample["latency_ns"] for (r, name), row in rows.items()
                                   if name == variant for sample in row["samples"]]
                        pooled[variant] = dict(sample_count=len(samples), median_ns=statistics.median(samples),
                                               max_ns=max(samples), empirical_p99_ns=(percentile(samples, 0.99)
                                                                                     if len(samples) >= 100 else None),
                                               p99_qualified=False)
                    output.append(dict(case=case, cores=cores, cohort=cohort, metric=metric,
                                       pooled_latency=pooled,
                                       twin_precision_sufficient=precise,
                                       **comparisons))
    return output


def select_panel_scale(args, pilot):
    rows = [json.loads(line) for line in (args.output / "firn-pilot-raw.jsonl").read_text().splitlines()]
    size_ratio = args.members / pilot["members"]
    request_ratio = args.requests / pilot["requests"]
    projected = 0.0
    components = []
    for row in rows:
        preparation = row["setup_seconds"] * size_ratio
        if row["case"] == "spop_full_drain":
            # Source-model extrapolation, not a measurement of the large case:
            # retained capacity times harmonic empty-scan growth during drain.
            factor = size_ratio * math.log(args.members) / math.log(pilot["members"])
        elif row["case"] in ("spop_threshold", "spop_contention"):
            factor = size_ratio
        elif row["case"] == "spop_sparse":
            factor = size_ratio * request_ratio
        else:
            preparation = row["setup_seconds"]
            factor = request_ratio
        measurement = row["elapsed_ns"] / 1e9 * factor
        projected += preparation + measurement
        components.append(dict(case=row["case"], cores=row["cores"], variant=row["variant"],
                               preparation_seconds=preparation, measurement_seconds=measurement))
    measured_work = sum(row["setup_seconds"] + row["elapsed_ns"] / 1e9 for row in rows)
    overhead = max(0, pilot["elapsed_seconds"] - measured_work)
    projected = (projected + overhead) * args.rounds / pilot["rounds"]
    selection = dict(projected_seconds=projected, maximum_automatic_seconds=1800,
                     allowed=projected <= 1800, pilot_twin_precision_sufficient=pilot["twin_precision_sufficient"],
                     members=args.members, remaining=args.remaining, requests=args.requests, rounds=args.rounds,
                     basis="Pilot preparation scaled by N; drain by N log N; threshold/contention by N; sparse by N and request ratio. Source-model estimate, not an upper bound.",
                     uncertainty="Hosted variance and nonlinear behavior can invalidate this estimate; imprecise results remain unresolved.",
                     components=components)
    write_json(args.output / "firn-panel-selection.json", selection)
    if not selection["allowed"]:
        raise RuntimeError(f"projected Firn panel {projected:.0f}s exceeds automatic 1800s limit; inspect firn-panel-selection.json before selecting a smaller panel")
    return selection


def identities(args):
    root = args.firn_root
    revision = git(root, "rev-parse", "HEAD")
    if revision != FIRN_REVISION:
        raise RuntimeError("Firn source revision differs from the preregistered revision")
    if git(root, "status", "--porcelain", "--untracked-files=all"):
        raise RuntimeError("Firn source or submodule checkout has changes or untracked files")
    submodules = subprocess.check_output(["git", "-C", str(root), "submodule", "status", "--recursive"], text=True).rstrip("\n")
    if any(line.startswith(("-", "+", "U")) for line in submodules.splitlines()):
        raise RuntimeError("Firn submodules are missing or differ from their pins")
    compilers = {variant: getattr(args, variant + "_compiler") for variant in VARIANTS}
    return dict(firn_revision=revision, submodules=submodules,
                compiler_sha256={name: digest(path) for name, path in compilers.items()},
                compilers={name: str(path) for name, path in compilers.items()},
                build_options=["--full-lto", "--graph", "firn/modules.wfg", "--entry", "firn"],
                cache="none: --full-lto and --cache are mutually exclusive",
                helper_sha256=digest(Path(__file__)))


def harness_integrity():
    """Small CI checks of protocol/oracle faults before any costly build."""
    detected = []
    try:
        expect(2, 1)
    except RuntimeError:
        detected.append("wrong-insertion-count")
    for label, wire in (("server-error", b"-ERR deliberate fault\r\n"),
                        ("truncated-bulk", b"$4\r\nab\r\n")):
        client = Resp.__new__(Resp)
        client.reader = io.BytesIO(wire)
        try:
            client.read()
        except RuntimeError:
            detected.append(label)
    if len(detected) != 3:
        raise RuntimeError("harness failed to detect a representative wrong/missing reply")
    client = Resp.__new__(Resp)
    client.reader = io.BytesIO(b"*2\r\n$2\r\nm0\r\n:7\r\n")
    expect(client.read(), [b"m0", 7])
    equal = interval([1, 1, 1])
    expect((equal["geomean"], equal["low95"], equal["high95"]), (1, 1, 1))
    expect([predicted_capacity(n) for n in (3, 4, 24, 25)], [4, 8, 32, 64])
    return {"detected_faults": detected, "valid_reply": True, "equal_ratios": True,
            "growth_boundaries": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("firn-root", "base-compiler", "twin-compiler", "head-compiler", "output"):
        parser.add_argument("--" + name, required=True, type=lambda value: Path(value).resolve())
    parser.add_argument("--phase", required=True, choices=("pilot", "panel"))
    for name in ("members", "remaining", "requests", "rounds"):
        parser.add_argument("--" + name, required=True, type=int)
    args = parser.parse_args()
    if os.environ.get("CI", "").lower() != "true" or platform.system() != "Linux":
        parser.error("this build/measurement helper runs only in Linux CI")
    if args.members < 32 or not 0 < args.remaining < args.members or args.requests < 1 or args.rounds < 2:
        parser.error("need members>=32, 0<remaining<members, requests>=1, rounds>=2")
    if args.phase == "panel" and (args.members != 1000000 or args.remaining != 1000 or args.rounds < 4):
        parser.error("panel requires members=1000000, remaining=1000 and rounds>=4")
    args.output.mkdir(parents=True, exist_ok=True)
    write_json(args.output / f"firn-{args.phase}-harness-integrity.json", harness_integrity())
    identity = identities(args)
    identity_path = args.output / "firn-identities.json"
    binaries = {name: args.output / "build" / name / "firn" for name in VARIANTS}
    if args.phase == "pilot":
        build_times = {}
        for name, binary in binaries.items():
            binary.parent.mkdir(parents=True, exist_ok=True)
            command = [str(getattr(args, name + "_compiler")), "--full-lto", "--graph",
                       str(args.firn_root / "firn/modules.wfg"), "--entry", "firn", "-o", str(binary)]
            write_json(binary.parent / "command.json", command)
            start = time.perf_counter()
            with (binary.parent / "build.log").open("wb") as log:
                child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                         start_new_session=True)
                try:
                    status = child.wait(timeout=1800)
                    if status:
                        raise RuntimeError(f"{name} Firn build failed ({status}): {binary.parent / 'build.log'}")
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait(timeout=5)
            build_times[name] = time.perf_counter() - start
        identity["binary_sha256"] = {name: digest(path) for name, path in binaries.items()}
        identity["build_seconds"] = build_times
        write_json(identity_path, identity)
    else:
        saved = json.loads(identity_path.read_text())
        if any(saved.get(key) != value for key, value in identity.items()):
            raise RuntimeError("source/compiler/helper identity changed since pilot")
        if saved["binary_sha256"] != {name: digest(path) for name, path in binaries.items()}:
            raise RuntimeError("Firn binary changed since pilot")
        pilot = json.loads((args.output / "firn-pilot-scale.json").read_text())
        if not pilot["complete"]:
            raise RuntimeError("pilot did not complete")
        selection = select_panel_scale(args, pilot)
    available = sorted(os.sched_getaffinity(0))
    if len(available) < 2:
        raise RuntimeError("the preregistered panel requires at least two available CPUs")
    configuration = {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()}
    configuration.update(available_cpus=available, host=platform.platform(),
                         cpuinfo=Path("/proc/cpuinfo").read_text(),
                         precision="hosted exploratory; not precise 14900K acceptance",
                         random_seed="Firn does not expose RNG seed; identical member/input order, draws not controlled",
                         limitations=["client overhead included", "no direct internal hold observation",
                                      "threshold capacity inferred from pinned source, not observed over RESP",
                                      "one threshold/drain request per independent process; p99 needs many rounds"])
    if args.phase == "panel":
        configuration["pilot_scale"] = pilot
        configuration["panel_selection"] = selection
        configuration["scale_selection"] = "Fixed registered million-to-thousand target; explicit requested rounds. Pilot spread retained; imprecision never grants a pass."
    write_json(args.output / f"firn-{args.phase}-configuration.json", configuration)
    records = []
    started = time.perf_counter()
    with (args.output / f"firn-{args.phase}-raw.jsonl").open("w", buffering=1) as raw:
        for round_number in range(args.rounds):
            cohort = 0 if args.phase == "pilot" else int(round_number >= args.rounds // 2)
            order = VARIANTS[round_number % 3:] + VARIANTS[:round_number % 3]
            for cores in (1, 2):
                cpus = available[:cores]
                client_cpus = available[cores:] or available
                os.sched_setaffinity(0, client_cpus)
                for case in CASES:
                    for name in order:
                        directory = args.output / "servers" / f"{args.phase}-{round_number}-{cores}-{case}-{name}"
                        print(f"{args.phase} round={round_number} cores={cores} case={case} variant={name}", flush=True)
                        with server(binaries[name], cpus, directory) as (client, port):
                            row = run_case(client, port, case, args.members, args.remaining, args.requests)
                        row.update(phase=args.phase, round=round_number, cohort=cohort, cores=cores,
                                   case=case, variant=name, server_cpus=cpus, client_cpus=client_cpus,
                                   separate_client_cpus=not bool(set(cpus) & set(client_cpus)))
                        records.append(row)
                        raw.write(json.dumps(row) + "\n")
    summary = summarize(records)
    write_json(args.output / f"firn-{args.phase}-summary.json", summary)
    scale = dict(complete=True, elapsed_seconds=time.perf_counter() - started,
                 members=args.members, remaining=args.remaining, requests=args.requests, rounds=args.rounds,
                 max_setup_seconds=max(row["setup_seconds"] for row in records),
                 setup_seconds_total=sum(row["setup_seconds"] for row in records),
                 twin_precision_sufficient=all(row["twin_precision_sufficient"] for row in summary),
                 summary_file=f"firn-{args.phase}-summary.json",
                 acceptance="unresolved: report cohort ratios, twin variation and hosted limitations")
    write_json(args.output / f"firn-{args.phase}-scale.json", scale)
    write_json(args.output / f"firn-{args.phase}-artifact-hashes.json", {
        path.name: digest(path) for path in sorted(args.output.glob(f"firn-{args.phase}-*"))
        if path.is_file() and not path.name.endswith("artifact-hashes.json")
    })
    os.sched_setaffinity(0, available)


if __name__ == "__main__":
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    main()
