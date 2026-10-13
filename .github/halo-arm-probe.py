#!/usr/bin/env python3
"""Temporary hosted-CI arm-preparation probe; remove with its experiment branch.

Invoke one phase under .github/run-check.pl. No compiler build is performed here.
ARM_PROBE_OUT, ARM_HALO_DIR, ARM_COMPILER and ARM_MEASURE are absolute paths.
The historical measure.py supplies its unchanged run output and raw parser; this
probe owns different criteria and never invokes its historical table verdicts.
"""

import argparse
import csv
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
LABELS = ("original", "V0", "V1", "V2")
ARMS = {"SubRK": 25, "LtJmpRK": 54, "GetUpval": 5, "Call": 63, "Return": 65}
CALLS = 7049155
PILOT_CALLS = 150049
FULL_BOUND = 2400
PILOT_BOUND = 90
CLANG = "/usr/bin/clang"
SINK = ROOT / "research/investigations/halo-call-path/probe/sink_initializers.py"
CG_FLAGS = ["valgrind", "--tool=callgrind", "--dump-instr=yes",
            "--collect-jumps=yes", "--dump-line=no", "--compress-strings=no",
            "--compress-pos=no", "--cache-sim=no", "--branch-sim=no"]


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def load(path, default=None):
    return json.loads(path.read_text()) if path.exists() else default


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Probe:
    def __init__(self):
        self.out = self.absolute("ARM_PROBE_OUT")
        self.halo = self.absolute("ARM_HALO_DIR")
        self.compiler = self.absolute("ARM_COMPILER")
        self.measure = self.absolute("ARM_MEASURE")
        self.major = os.environ.get("LLVM_MAJOR", (ROOT / ".github/llvm-major").read_text().strip())
        if not re.fullmatch(r"[0-9]+", self.major):
            raise ValueError("LLVM_MAJOR must be an integer")
        for directory in ("inputs", "images", "ir", "logs", "raw", "pilot", "probe", "mir", "state"):
            (self.out / directory).mkdir(parents=True, exist_ok=True)

    @staticmethod
    def absolute(name):
        path = Path(os.environ[name])
        if not path.is_absolute():
            raise ValueError(name + " must be absolute")
        return path

    def env(self, saved=False):
        env = os.environ.copy()
        env["LC_ALL"] = "C"
        env.pop("CCC_OVERRIDE_OPTIONS", None)
        if saved:
            env["CCC_OVERRIDE_OPTIONS"] = "+-Wl,--save-temps"
        return env

    def command(self, args, name, cwd=ROOT, env=None, bound=1800, stdin_path=None):
        args = list(map(str, args))
        prefix = self.out / "logs" / name
        record = {"argv": args, "cwd": str(cwd), "bound_seconds": bound,
                  "stdin": str(stdin_path) if stdin_path else None,
                  "CCC_OVERRIDE_OPTIONS": (env or {}).get("CCC_OVERRIDE_OPTIONS"),
                  "WHITEFOOT_BRANCH_WEIGHTS": (env or os.environ).get("WHITEFOOT_BRANCH_WEIGHTS")}
        print(name + ": " + " ".join(args), flush=True)
        started = time.monotonic()
        bounded = ["timeout", "--foreground", "--signal=TERM", "--kill-after=10s", str(bound) + "s", *args]
        record["bounded_argv"] = bounded
        with Path(str(prefix) + ".stdout").open("wb") as stdout, Path(str(prefix) + ".stderr").open("wb") as stderr:
            input_stream = None
            try:
                if stdin_path is not None:
                    input_stream = Path(stdin_path).open("rb")
                result = subprocess.run(bounded, cwd=cwd, env=env or self.env(),
                                        stdin=input_stream, stdout=stdout, stderr=stderr)
                record["returncode"] = result.returncode
                record["timed_out"] = result.returncode in (124, 137)
            except OSError as error:
                record["error"] = str(error)
            finally:
                if input_stream is not None:
                    input_stream.close()
        record["acquisition_seconds"] = time.monotonic() - started
        save(Path(str(prefix) + ".command.json"), record)
        with (self.out / "commands.jsonl").open("a") as stream:
            stream.write(json.dumps(record) + "\n")
        if record.get("returncode") != 0 or record.get("timed_out"):
            raise ValueError(name + ": command unavailable: " + json.dumps(record))
        return record, Path(str(prefix) + ".stdout"), Path(str(prefix) + ".stderr")

    def attempt(self, phase, label, action, diagnostic=False):
        try:
            result = action()
            state = {"status": "available", "result": result}
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            state = {"status": "unavailable", "reason": str(error)}
            print(phase + "/" + label + ": " + str(error), flush=True)
        state["diagnostic_only"] = diagnostic
        save(self.out / "state" / (phase + "." + label + ".json"), state)
        return state

    def kernel(self, n, target):
        source = (self.halo / "research/experiments/halo-bench/kernels/fib.lua").read_text()
        source, replaced = re.subn(r"^local N = [0-9]+$", "local N = " + str(n), source, flags=re.M)
        if replaced != 1:
            raise ValueError("fib input must have exactly one local N binding")
        target.write_text(source)

    def build(self):
        self.kernel(24, self.out / "inputs/fib24.lua")
        self.kernel(32, self.out / "inputs/fib32.lua")
        for label, saved in (("original-plain", False), ("original", True)):
            def construct(label=label, saved=saved):
                binary = self.out / "images" / ("halo-" + label)
                self.command([self.compiler, "--graph", "research/experiments/halo-bench/modules.wfg",
                              "--entry", "bench", "--full-lto", "-o", binary],
                             "build-" + label, cwd=self.halo, env=self.env(saved))
                shape = self.disassembly(label, binary)
                return {"binary": str(binary.relative_to(self.out)), "sha256": sha(binary),
                        "shape_sha256": sha(shape)}
            self.attempt("build", label, construct)
        def recover():
            # LLD's .0.5 file is the actual final full-LTO module, not WF's
            # emitted pre-inlining IR or the .0.0 input bitcode.
            files = list((self.out / "images").glob("halo-original*.0.5.precodegen.bc"))
            if len(files) != 1:
                raise ValueError("expected exactly one saved .0.5.precodegen.bc: " + str(files))
            retained = self.out / "ir/original.0.5.precodegen.bc"
            shutil.copy2(files[0], retained)
            target = self.out / "ir/V0.ll"
            self.command(["llvm-dis-" + self.major, retained, "-o", target], "recover-postlto")
            return {"saved_link_bitcode": str(files[0].relative_to(self.out)),
                    "sha256": sha(retained), "llvm_ir": str(target.relative_to(self.out))}
        self.attempt("build", "postlto", recover)
        versions = {}
        for name, args in (("clang", [CLANG, "--version"]), ("opt", ["opt-" + self.major, "--version"]),
                           ("llc", ["llc-" + self.major, "--version"]), ("callgrind", ["valgrind", "--version"]),
                           ("compiler_revision", ["git", "rev-parse", "HEAD"]),
                           ("halo_revision", ["git", "-C", self.halo, "rev-parse", "HEAD"])):
            try:
                _, stdout, _ = self.command(args, "identity-" + name, bound=30)
                versions[name] = stdout.read_text().strip()
            except ValueError as error:
                versions[name] = "unavailable: " + str(error)
        manifest = {"compiler": str(self.compiler), "compiler_sha256": sha(self.compiler),
                    "historical_measure_sha256": sha(self.measure), "llvm_major": self.major,
                    "versions": versions, "counts": {"fib32_lua_calls": CALLS, "fib24_lua_calls": PILOT_CALLS},
                    "pipeline": "plain and save-temps full LTO; unchanged recovered module V0; late hotcoldsplit V1; initializer sink V2",
                    "historical_E2_separate": {"executable_self_per_call": 752.026399,
                                               "SubRK": 49, "LtJmpRK": 55, "GetUpval": 60},
                    "scope": "fib only; hosted elapsed times size acquisition and make no speed claim",
                    "workflow_run": os.environ.get("GITHUB_RUN_ID")}
        save(self.out / "manifest.json", manifest)

    def codegen(self, label):
        source = self.out / "ir" / (label + ".ll")
        obj = self.out / "images" / (label + ".o")
        binary = self.out / "images" / ("halo-" + label)
        self.command([CLANG, "-O2", "-falign-functions=64", "-Wno-override-module", "-Xclang",
                      "-disable-llvm-passes", "-c", "-x", "ir", source, "-o", obj], "codegen-" + label)
        self.link(obj, binary, "link-" + label)
        self.disassembly(label, binary)
        return {"ir_sha256": sha(source), "object_sha256": sha(obj), "binary_sha256": sha(binary)}

    def link(self, obj, binary, label):
        self.command([CLANG, "-pthread", "-fuse-ld=lld", "-O2", "-falign-functions=64", obj,
                      "-lm", "-o", binary], label)

    def disassembly(self, label, binary):
        _, stdout, _ = self.command(["objdump", "-d", "--no-show-raw-insn", "-M", "intel", binary],
                                    "objdump-" + label)
        _, symbols, _ = self.command(["nm", "-S", "--defined-only", binary], "nm-bounds-" + label)
        target = self.out / "probe" / (label + ".objdump.txt")
        shutil.copy2(stdout, target)
        shutil.copy2(symbols, self.out / "probe" / (label + ".nm-bounds.txt"))
        save(self.out / "probe" / (label + ".shape.json"),
             normalized_assembly(target.read_text(), parse_function_bounds(symbols.read_text())))
        return target

    def mir(self, label):
        source = self.out / "ir" / (label + ".ll")
        pei = self.out / "mir" / (label + ".pei.mir")
        obj = self.out / "mir" / (label + ".guard.o")
        binary = self.out / "mir" / ("halo-" + label + "-guard")
        # Attribute-free WF arms need Clang's x86-64/generic default. An
        # unspecified llc CPU instead selects generic CPU with i586 tuning.
        # LLVM 22.1.8: clang/lib/Driver/ToolChains/Arch/X86.cpp:94-96;
        # llvm/lib/Target/X86/X86Subtarget.cpp:227-233.
        options = ["llc-" + self.major, "-O2", "-mcpu=x86-64", "-relocation-model=pic"]
        self.command([*options, "-stop-after=prologepilog", source, "-o", pei], "mir-pei-" + label)
        self.command([*options, "-filetype=obj", source, "-o", obj], "mir-guard-codegen-" + label)
        self.link(obj, binary, "mir-guard-link-" + label)
        self.disassembly(label + "-mir-guard", binary)
        a = load(self.out / "probe" / (label + ".shape.json"))
        b = load(self.out / "probe" / (label + "-mir-guard.shape.json"))
        guard = compare_shapes(a, b)
        mappings = parse_mir(pei.read_text())
        result = {"pipeline_guard": guard, "mappings": mappings,
                  "source_pins": source_pins(source.read_text()),
                  "options": options, "source_target_settings": "module triple and per-function CPU/feature attributes preserved; attribute-free functions explicitly use Clang's x86-64 default CPU"}
        save(self.out / "mir" / (label + ".mapping.json"), result)
        return result

    def variants(self):
        def control():
            self.command(["opt-" + self.major, "-passes=verify", "-verify-each", "-disable-output",
                          self.out / "ir/V0.ll"], "verify-V0")
            return self.codegen("V0")
        control_state = self.attempt("variants", "V0", control)
        def split():
            source, target = self.out / "ir/V0.ll", self.out / "ir/V1.ll"
            self.command(["opt-" + self.major, "-passes=hotcoldsplit,verify", "-verify-each",
                          "-pass-remarks=hotcoldsplit", "-pass-remarks-missed=hotcoldsplit", "-S",
                          source, "-o", target], "hotcoldsplit")
            return self.codegen("V1")
        self.attempt("variants", "V1", split)
        def sink():
            source, target = self.out / "ir/V0.ll", self.out / "ir/V2.ll"
            self.command([sys.executable, SINK, source, target, self.out / "ir/V2.sink.json"], "sink-initializers")
            self.command(["opt-" + self.major, "-passes=verify", "-verify-each", "-disable-output", target], "verify-V2")
            return self.codegen("V2")
        self.attempt("variants", "V2", sink)
        for label in ("V0", "V1", "V2"):
            if load(self.out / "state" / ("variants." + label + ".json"), {}).get("status") == "available":
                self.attempt("mir", label, lambda label=label: self.mir(label), diagnostic=True)
        return 0 if control_state["status"] == "available" else 1

    def acquire(self, label, n, round_number=None):
        pilot = round_number is not None
        name = "fib" + str(n) + "." + label + ("." + str(round_number) if pilot else "")
        prefix = self.out / ("pilot" if pilot else "raw") / name
        raw = Path(str(prefix) + ".out")
        binary = self.out / "images" / ("halo-" + label)
        record, stdout, stderr = self.command([*CG_FLAGS, "--callgrind-out-file=" + str(raw), binary],
                                            name, cwd=self.halo, bound=PILOT_BOUND if pilot else FULL_BOUND,
                                            stdin_path=self.out / "inputs" / ("fib" + str(n) + ".lua"))
        validate_fib(stdout.read_text(), n)
        ir = read_count(raw.read_text(), stderr.read_text())
        result = {"process_Ir": ir, "acquisition_seconds": record["acquisition_seconds"],
                  "raw": str(raw.relative_to(self.out)), "stdout": str(stdout.relative_to(self.out)),
                  "stderr": str(stderr.relative_to(self.out)), "profiler_returncode": record["returncode"]}
        save(Path(str(prefix) + ".json"), result)
        return result

    def pilot(self):
        rounds = [self.attempt("pilot", str(i), lambda i=i: self.acquire("V0", 24, i)) for i in (1, 2)]
        if any(r["status"] != "available" for r in rounds):
            save(self.out / "pilot/plan.json", {"status": "unavailable", "reason": "both pilot runs are required", "rounds": rounds})
            return
        durations = [r["result"]["acquisition_seconds"] for r in rounds]
        scale = CALLS / PILOT_CALLS
        predicted = max(durations) * scale
        save(self.out / "pilot/plan.json", {"status": "available" if predicted <= FULL_BOUND else "unavailable",
             "seconds": durations, "spread_seconds": max(durations) - min(durations),
             "scale_calls32_over_calls24": scale, "estimated_sweep_seconds": len(LABELS) * predicted,
             "estimated_single_run_seconds": predicted, "single_run_bound_seconds": FULL_BOUND,
             "reason": "pilot sizes acquisition only; no elapsed-time performance claim; each full run has its own hard bound"})

    def historical(self):
        spec = importlib.util.spec_from_file_location("historical_arm_measure", self.measure)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def reduce(self, label, result):
        binary = self.out / "images" / ("halo-" + label)
        raw = self.out / result["raw"]
        self.command([sys.executable, self.measure, "run", label, binary, raw, CALLS,
                      self.out / "probe"], "measure-" + label)
        measured = load(self.out / "probe" / (label + ".json"))
        validate_measurement(measured)
        costs, _, functions, edges = self.historical().parse_callgrind(str(raw), str(binary.resolve()))
        if sum(costs.values()) != measured["total"] or sum(functions.values()) != measured["total"]:
            raise ValueError("raw executable SELF reduction disagrees with historical measure output")
        with (self.out / "probe" / (label + ".function-self.tsv")).open("w") as stream:
            writer = csv.writer(stream, delimiter="\t")
            writer.writerow(["symbol", "self_Ir", "lua_calls", "self_Ir_per_Lua_call"])
            for function, count in sorted(functions.items(), key=lambda item: (-item[1], item[0])):
                writer.writerow([function, count, CALLS, format(count / CALLS, ".12f")])
        with (self.out / "probe" / (label + ".callee-inclusive-NONADDITIVE.tsv")).open("w") as stream:
            writer = csv.writer(stream, delimiter="\t")
            writer.writerow(["caller", "callee", "inclusive_edge_Ir_NONADDITIVE", "lua_calls", "Ir_per_Lua_call"])
            for (caller, callee), count in sorted(edges.items(), key=lambda item: (-item[1], item[0])):
                writer.writerow([caller, callee, count, CALLS, format(count / CALLS, ".12f")])
        analysis = {}
        bias = int(measured["bias"], 0)
        mir = load(self.out / "mir" / (label + ".mapping.json"), {})
        for role, arm in measured["arms"].items():
            if "error" in arm:
                analysis[role] = {"status": "unavailable", "reason": arm["error"]}
                continue
            rows = self.historical().disassemble(str(binary), arm["symbol"])
            dynamic = [{"address": hex(address), "instruction": text, "Ir": costs.get(address + bias, 0)}
                       for address, text in rows]
            if sum(row["Ir"] for row in dynamic) != functions.get(arm["symbol"], 0):
                raise ValueError(role + ": exact instruction SELF disagrees with function SELF")
            with (self.out / "probe" / (label + ".arm-" + role + ".exact.tsv")).open("w") as stream:
                writer = csv.writer(stream, delimiter="\t")
                writer.writerow(["address", "instruction", "Ir", "lua_calls", "Ir_per_Lua_call"])
                for row in dynamic:
                    writer.writerow([row["address"], row["instruction"], row["Ir"], CALLS, format(row["Ir"] / CALLS, ".12f")])
            entry = []
            for row in dynamic:
                if re.match(r"^(j\w+|call\w*|ret\w*)\b", row["instruction"]):
                    break
                entry.append(row)
            stores = [dict(row, slot=assembly_store(row["instruction"])) for row in dynamic if assembly_store(row["instruction"])]
            for row in stores:
                row["at_entry"] = any(row["address"] == e["address"] for e in entry)
                row["slot_references"] = [candidate for candidate in dynamic
                    if candidate["address"] != row["address"] and row["slot"]["memory"] in candidate["instruction"]]
            pin = pin_verdict(dynamic, mir, arm["symbol"])
            analysis[role] = {"status": "available", "symbol": arm["symbol"], "entry_instructions": entry,
                              "stack_stores": stores, "pin_initializer": pin,
                              "local_frame": local_frame_verdict(dynamic),
                              "full_entry_frame": entry_frame_verdict(entry),
                              "hot_rbp_saves": [r for r in dynamic if r["Ir"] > 0 and re.match(r"^push\s+rbp$", r["instruction"])]}
        save(self.out / "probe" / (label + ".analysis.json"), analysis)
        return {"executable_self_Ir": measured["total"], "process_Ir": result["process_Ir"],
                "per_function_self": label + ".function-self.tsv", "analysis": label + ".analysis.json"}

    def counts(self):
        plan = load(self.out / "pilot/plan.json", {})
        if plan.get("status") != "available":
            self.attempt("counts", "plan", lambda: unavailable("full sweep unavailable: valid bounded two-run pilot required"))
            return
        for label in LABELS:
            def collect(label=label):
                required = "build.original.json" if label == "original" else "variants." + label + ".json"
                if load(self.out / "state" / required, {}).get("status") != "available":
                    raise ValueError("image construction unavailable; partial products are not valid variants")
                binary = self.out / "images" / ("halo-" + label)
                _, stdout, _ = self.command([binary], "native-" + label, cwd=self.halo, bound=90,
                                            stdin_path=self.out / "inputs/fib32.lua")
                validate_fib(stdout.read_text(), 32)
                return self.reduce(label, self.acquire(label, 32))
            self.attempt("counts", label, collect)

    def report(self):
        data = {label: load(self.out / "probe" / (label + ".json"))
                if load(self.out / "state" / ("counts." + label + ".json"), {}).get("status") == "available"
                else None for label in LABELS}
        analyses = {label: load(self.out / "probe" / (label + ".analysis.json"), {})
                    if data[label] is not None else {} for label in LABELS}
        shapes = {label: load(self.out / "probe" / (label + ".shape.json")) for label in (*LABELS, "original-plain")}
        plain_saved = compare_shapes(shapes["original-plain"], shapes["original"])
        reproduction = compare_shapes(shapes["original"], shapes["V0"])
        if data["original"] is None or data["V0"] is None:
            counts_guard = {"status": "UNAVAILABLE", "reason": "original and V0 exact counts required"}
        else:
            equal = data["original"]["total"] == data["V0"]["total"]
            arm_equal = all(data["original"]["arms"].get(role, {}).get("self_per_call_callgrind_fn") ==
                            data["V0"]["arms"].get(role, {}).get("self_per_call_callgrind_fn") and
                            "error" not in data["original"]["arms"].get(role, {}) and
                            "error" not in data["V0"]["arms"].get(role, {}) for role in ARMS)
            counts_guard = {"status": "PASS" if equal and arm_equal else "FAIL", "executable_self_equal": equal,
                            "five_arm_self_equal": arm_equal}
        attributable = all(g["status"] == "PASS" for g in (plain_saved, reproduction, counts_guard))
        verdicts = {"plain_vs_saved_shapes": plain_saved, "original_vs_V0_shapes": reproduction,
                    "original_vs_V0_counts": counts_guard, "attribution_to_original_available": attributable,
                    "historical_E2": "separate dated baseline: 752.026399 total; SubRK/LtJmpRK/GetUpval 49/55/60; no reproduction claim", "variants": {}}
        lines = ["# Halo arm-preparation probe", "", "Historical E2 is a separate result: 752.026399 executable SELF/Lua call, arms 49/55/60.",
                 "This run uses the current supplied compiler. Hosted elapsed times size acquisition only; no wall-time claim.", "",
                 "Original is the saved-temporary full-LTO image. The plain link checks that saving temporaries preserves its instruction shapes.",
                 "V0 is the untouched true .0.5.precodegen module; V1 runs late hotcoldsplit; V2 sinks only eligible pin initializers.", "",
                 "Reproduction: plain/saved shapes **" + plain_saved["status"] + "**, original/V0 shapes **" + reproduction["status"] +
                 "**, exact counts **" + counts_guard["status"] + "**.",
                 "Variant attribution to original: **" + ("AVAILABLE" if attributable else "UNAVAILABLE — retain the new-pipeline data separately") + "**.", "",
                 "| Image | Executable SELF/Lua call | SubRK SELF | LtJmpRK SELF | GetUpval SELF | Slow-preparation criterion | All-three entry-frame criterion |",
                 "|---|---:|---:|---:|---:|---|---|"]
        for label in LABELS:
            per_arm = {}
            for role in ("SubRK", "LtJmpRK"):
                per_arm[role] = analyses[label].get(role, {}).get("pin_initializer", {"status": "UNAVAILABLE", "reason": "exact count/MIR mapping missing"})
            per_arm["GetUpval"] = analyses[label].get("GetUpval", {}).get("local_frame", {"status": "UNAVAILABLE", "reason": "exact arm data missing"})
            primary = conjunction([v["status"] for v in per_arm.values()])
            full = conjunction([analyses[label].get(role, {}).get("full_entry_frame", {}).get("status", "UNAVAILABLE")
                                for role in ("SubRK", "LtJmpRK", "GetUpval")])
            verdicts["variants"][label] = {"slow_preparation": primary, "per_arm": per_arm, "full_entry_frame": full,
                                        "relative_to_original": "available" if attributable else "unavailable"}
            d = data[label]
            values = [format(d["total_per_call"], ".6f")] if d else ["unavailable"]
            values += [format(d["arms"][r]["self_per_call"], ".6f") if d and "self_per_call" in d["arms"].get(r, {}) else "unavailable"
                       for r in ("SubRK", "LtJmpRK", "GetUpval")]
            lines.append("| " + label + " | " + " | ".join(values + [primary, full]) + " |")
        lines += ["", "Primary PASS requires both known code-pointer pin initializers to have uniquely proven machine identity and exact zero dynamic Ir, plus no hot GetUpval local frame adjustment. Its RBP push is reported separately and allowed by the primary criterion.",
                  "The stronger criterion prohibits entry-frame preparation in all three arms. Smaller frames and new .cold symbols alone prove neither criterion.",
                  "Whole-module guards cover nonzero-sized defined functions, retaining every instruction within the ELF-declared extent. PLT stubs and inter-function padding are outside that scope. RIP displacement and nearest-symbol data comments are relocation annotations, not operand shapes; other constants, frame offsets and control targets remain checked.",
                  "Each named arm also has its own bounded instruction-shape guard. MIR pin provenance requires that arm's exact PASS and unique complete instruction mapping; a failed overall module guard remains failed and continues to withhold whole-program attribution. Unnamed, ambiguous, or missing mappings remain UNVERIFIED.",
                  "Exact integer per-instruction Ir and candidate slot references are in probe/*.analysis.json and *.exact.tsv; rounded historical arm text is diagnostic only.",
                  "Per-function SELF TSVs are additive within each image. Caller/callee inclusive edges are NONADDITIVE; do not sum them with SELF or each other. V0 prepare attribution has no historical base raw comparison and cannot decompose the earlier 73-instruction saving.",
                  "This fib probe does not validate slow/error/resume paths, other-kernel harm or adoption readiness.", ""]
        for label, result in verdicts["variants"].items():
            for role, evidence in result["per_arm"].items():
                lines.append("- " + label + " " + role + ": " + evidence["status"] + "; " + evidence.get("reason", "see explicit mappings and dynamic counts") + ".")
        lines += ["", "Named-arm guards (overall defined-function statuses are retained separately):", "",
                  "| Arm | Original / V0 | V0 Clang / MIR twin | V1 Clang / MIR twin |", "|---|---|---|---|"]
        mir_guards = [load(self.out / "mir" / (label + ".mapping.json"), {}).get("pipeline_guard", {})
                      for label in ("V0", "V1")]
        for role, number in ARMS.items():
            statuses = []
            for guard in (reproduction, *mir_guards):
                found = [v["status"] for name, v in guard.get("arm_guards", {}).items()
                         if name.endswith(".body.arm." + str(number))]
                statuses.append(found[0] if len(found) == 1 else "UNAVAILABLE")
            lines.append("| " + role + " | " + " | ".join(statuses) + " |")
        lines += ["", "Entry evidence (all counts are exact integer Ir over the measured fib32 run):", "",
                  "| Image / arm | Entry local allocation | Hot RBP saves | Entry stack stores: address / Ir | Known pin initializer: address / Ir |",
                  "|---|---|---|---|---|"]
        for label in LABELS:
            for role in ("SubRK", "LtJmpRK", "GetUpval"):
                analysis = analyses[label].get(role, {})
                allocations = [r for r in analysis.get("entry_instructions", [])
                               if re.match(r"^sub\s+rsp,", r["instruction"])]
                allocation = "; ".join(r["instruction"] + " (Ir=" + str(r["Ir"]) + ")" for r in allocations)
                if not allocation:
                    allocation = "none visible" if analysis.get("status") == "available" else "unavailable"
                pushes = "; ".join(r["address"] + " / " + str(r["Ir"]) for r in analysis.get("hot_rbp_saves", [])) or "none"
                stores = "; ".join(r["address"] + " " + r["slot"]["memory"] + " / " + str(r["Ir"])
                                    for r in analysis.get("stack_stores", []) if r["at_entry"]) or "none visible"
                pin = analysis.get("pin_initializer", {})
                mapped = "; ".join(e["instruction"]["address"] + " / " + str(e["instruction"]["Ir"])
                                   for e in pin.get("evidence", [])) or pin.get("status", "unavailable")
                lines.append("| " + label + " / " + role + " | " + allocation + " | " + pushes + " | " + stores + " | " + mapped + " |")
        states = {p.name: load(p) for p in sorted((self.out / "state").glob("*.json"))}
        unavailable_states = {name: state for name, state in states.items()
                              if state["status"] != "available" and not state.get("diagnostic_only")}
        if unavailable_states:
            lines += ["", "Acquisition limitations:"] + ["- " + name + ": " + state["reason"] for name, state in unavailable_states.items()]
        verdicts["acquisition_unavailable"] = unavailable_states
        save(self.out / "verdicts.json", verdicts)
        (self.out / "table.md").write_text("\n".join(lines) + "\n")
        print("\n".join(lines))
        return 0 if attributable and all(data.values()) and not unavailable_states else 1


def unavailable(reason):
    raise ValueError(reason)


def validate_fib(text, n):
    expected = {24: "46368", 32: "2178309"}[n]
    if text.strip() != expected:
        raise ValueError("fib(" + str(n) + ") stdout must be exactly " + expected + "; got " + repr(text))


def read_count(raw, stderr):
    events = re.findall(r"^events:\s*(.*?)\s*$", raw, re.M)
    summaries = re.findall(r"^summary:\s*([0-9]+)\s*$", raw, re.M)
    collected = re.findall(r"^==[0-9]+==\s+Collected\s*:\s*([0-9]+)\s*$", stderr, re.M)
    if len(events) != 1 or events[0].split() != ["Ir"]:
        raise ValueError("callgrind must report exactly one events: Ir field")
    if len(summaries) != 1 or len(collected) != 1 or int(summaries[0]) <= 0 or summaries[0] != collected[0]:
        raise ValueError("missing, empty or inconsistent callgrind summary/Collected Ir")
    return int(summaries[0])


def validate_measurement(measured):
    if not measured or measured.get("lua_calls") != CALLS or measured.get("total", 0) <= 0:
        raise ValueError("missing executable SELF or incorrect Lua-call normalization")
    for role in ARMS:
        arm = measured.get("arms", {}).get(role, {})
        if not arm.get("symbol") or "error" in arm:
            raise ValueError(role + ": actual arm evidence missing")
        for key in ("self_per_call", "self_per_call_callgrind_fn"):
            value = arm.get(key)
            if not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
                raise ValueError(role + ": positive finite executable arm SELF required")


def parse_function_bounds(text):
    bounds = {}
    for line in text.splitlines():
        entry = re.fullmatch(r"([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+([tTwW])\s+(.+)", line.strip())
        if entry:
            bounds[entry[4]] = {"start": int(entry[1], 16), "size": int(entry[2], 16)}
    return bounds


def is_probe_arm(symbol):
    return bool(re.fullmatch(r"wf_package\.halo\.vm\.run\$instance\$[0-9a-f]+\.body\.arm\.(25|54|5|63|65)", symbol))


def normalized_assembly(text, bounds=None):
    # Function extents, not mnemonic filtering, separate body instructions from
    # linker/assembler padding. An in-body nop or int3 remains significant.
    bounds = bounds or {}
    functions = {name: [] for name, extent in bounds.items() if extent["size"] > 0}
    current, excluded, arm_symbols = None, set(), set()
    for line in text.splitlines():
        heading = re.match(r"^[0-9a-f]+ <(.+)>:$", line)
        if heading:
            current = heading[1]
            if is_probe_arm(current):
                arm_symbols.add(current)
            if current not in functions:
                excluded.add(current)
            continue
        instruction = re.match(r"^\s*([0-9a-f]+):\s*(.+)$", line)
        if current not in functions or instruction is None:
            continue
        address, extent = int(instruction[1], 16), bounds[current]
        if not extent["start"] <= address < extent["start"] + extent["size"]:
            continue
        value = instruction[2]
        # Preserve symbol+offset targets, immediate constants and stack offsets.
        # Remove only absolute control-target addresses and RIP relocations.
        value = re.sub(r"\b[0-9a-f]+\s+(?=<[^>]+>)", "", value)
        if "#" in value:
            before, comment = value.split("#", 1)
            if re.search(r"\[rip(?:[+-]0x[0-9a-f]+)?\]", before):
                before = re.sub(r"\[rip(?:[+-]0x[0-9a-f]+)?\]", "[rip+relocation]", before)
                if re.search(r"(?:^|\s)(?:j\w+|call\w*)\s", before):
                    # Indirect control transfers still carry a target identity.
                    # Instruction prefixes may precede their actual mnemonic.
                    comment = re.sub(r"^\s*[0-9a-f]+\s+(?=<[^>]+>)", "", comment)
                    value = before + "# " + comment.strip()
                else:
                    # Objdump's nearest visible data symbol need not own the
                    # anonymous constant. Its name changes with link layout.
                    value = before
            else:
                value = before + "# " + comment.strip()
        functions[current].append(re.sub(r"\s+", " ", value).strip())
    arm_symbols.update(name for name in bounds if is_probe_arm(name))
    return {"functions": functions, "bounds": bounds, "arm_symbols": sorted(arm_symbols),
            "excluded_unbounded_symbols": sorted(excluded),
            "missing_instruction_bodies": sorted(name for name, body in functions.items() if not body),
            "scope": "nonzero-sized defined functions from nm -S; all in-bound instructions retained"}


def compare_shapes(left, right):
    if not left or not right or not left.get("bounds") or not right.get("bounds"):
        return {"status": "UNAVAILABLE", "reason": "both disassemblies and explicit ELF function bounds required", "arm_guards": {}}
    a, b = left["functions"], right["functions"]
    symbols = sorted(set(a) | set(b))
    changed = [symbol for symbol in symbols if a.get(symbol) != b.get(symbol)]
    arms = sorted(set(left["arm_symbols"]) | set(right["arm_symbols"]))
    arm_guards = {}
    for arm in arms:
        bounded = all(shape["bounds"].get(arm, {}).get("size", 0) > 0 and shape["functions"].get(arm)
                      for shape in (left, right))
        arm_guards[arm] = {"status": "UNAVAILABLE" if not bounded else "PASS" if a[arm] == b[arm] else "FAIL",
                           "bounds_present": bool(bounded), "left_bounds": left["bounds"].get(arm),
                           "right_bounds": right["bounds"].get(arm)}
    complete = len(arms) == 5 and all(v["bounds_present"] for v in arm_guards.values())
    arm_guard = complete and all(v["status"] == "PASS" for v in arm_guards.values())
    missing = {"left": left["missing_instruction_bodies"], "right": right["missing_instruction_bodies"]}
    status = "UNAVAILABLE" if not complete or any(missing.values()) else "FAIL" if changed else "PASS"
    return {"status": status, "changed_symbols": changed, "arm_guards": arm_guards,
            "five_arm_shapes_equal": arm_guard, "missing_instruction_bodies": missing,
            "scope": left["scope"], "normalization": "body extents checked; addresses/RIP data annotations removed; constants, frame offsets, control-symbol targets and in-body padding instructions retained"}


def assembly_store(text):
    match = re.match(r"^(?:v?mov\w*|stos\w*)\s+(?:[A-Z]+ PTR )?(\[(rsp|rbp)([+-]0x[0-9a-f]+)?\]),\s*(\w+)$", text)
    if not match:
        return None
    return {"memory": match[1], "base": match[2], "offset": int(match[3] or "0", 16), "source": match[4]}


def parse_mir(text):
    mappings = {}
    for document in re.split(r"^---\s*$", text, flags=re.M):
        name = re.search(r"^name:\s*['\"]?([^\n'\"]+)", document, re.M)
        if not name:
            continue
        stores = []
        for line in document.splitlines():
            provenance = re.search(r"\binto %ir\.(wf\.pin\.8(?:\.[-\w.]+)?)(?=[\s,)])", line)
            operands = re.search(r"\bMOV64mr\s+\$(rsp|rbp),\s*1,\s*\$noreg,\s*(-?[0-9]+),\s*\$noreg,.*?\$(\w+)\s*(?:::|$)", line)
            if provenance and operands:
                stores.append({"pin": provenance[1], "base": operands[1], "offset": int(operands[2]),
                               "source": operands[3], "mir_instruction": line.strip()})
        mappings[name[1].strip()] = stores
    return mappings


def source_pins(text):
    """Names of explicit code-pointer pin candidates; not machine-slot proof."""
    result = {}
    current = None
    for line in text.splitlines():
        name = re.match(r'^define\b.*?@("[^"]+"|[-\w.$]+)\(', line)
        if name:
            current = name[1].strip('"')
            result[current] = []
        if current is not None:
            pin = re.match(r"\s*%(wf\.pin\.8(?:\.[-\w.]+)?)\s*=\s*alloca\s+ptr\b", line)
            if pin:
                result[current].append(pin[1])
        if line.strip() == "}":
            current = None
    return result


def pin_verdict(rows, mir, symbol):
    if not any(row["Ir"] > 0 for row in rows):
        return {"status": "UNVERIFIED", "reason": "arm has no measured executable activity; missing data cannot prove coldness"}
    arm_guard = mir.get("pipeline_guard", {}).get("arm_guards", {}).get(symbol, {})
    if arm_guard.get("status") != "PASS" or not arm_guard.get("bounds_present"):
        return {"status": "UNVERIFIED", "reason": "MIR codegen must reproduce this bounded arm before its provenance maps machine instructions"}
    mappings = mir.get("mappings", {}).get(symbol, [])
    candidates = mir.get("source_pins", {}).get(symbol, [])
    if len(candidates) != 1 or any(mapping["pin"] != candidates[0] for mapping in mappings):
        return {"status": "UNVERIFIED", "reason": "post-LTO code-pointer pin source identity is absent or ambiguous", "source_candidates": candidates, "mappings": mappings}
    if not mappings:
        return {"status": "UNVERIFIED", "reason": "no explicit final MIR code-pointer pin store; absence of a source initializer does not identify possible allocator spills"}
    evidence = []
    for mapping in mappings:
        matches = [row for row in rows if assembly_store(row["instruction"]) and
                   all(assembly_store(row["instruction"])[key] == mapping[key] for key in ("base", "offset", "source"))]
        if len(matches) != 1:
            return {"status": "UNVERIFIED", "reason": "named MIR pin store has no unique base/offset/source match in final assembly", "mappings": mappings}
        evidence.append({"mapping": mapping, "instruction": matches[0]})
    hot = any(item["instruction"]["Ir"] > 0 for item in evidence)
    return {"status": "FAIL" if hot else "PASS", "reason": "explicit named MIR stores uniquely mapped; " + ("a pin initializer executes on fib" if hot else "every mapped pin initializer has exact zero Ir"), "evidence": evidence}


def local_frame_verdict(rows):
    adjustments = [row for row in rows if (re.match(r"^(sub|add)\s+rsp,", row["instruction"])
                   or re.match(r"^enter\b", row["instruction"])) and row["Ir"] > 0]
    unknown = [row for row in rows if row["Ir"] > 0 and
               (re.match(r"^(lea|mov|and)\s+rsp,", row["instruction"]) or
                (re.match(r"^push\b", row["instruction"]) and
                 not re.fullmatch(r"push\s+rbp\s*", row["instruction"])) or
                re.search(r"\[rsp(?:[+\-\]])", row["instruction"]) or
                re.match(r"^mov\s+rbp,rsp$", row["instruction"]))]
    active = any(row["Ir"] > 0 for row in rows)
    status = "FAIL" if adjustments else "UNVERIFIED" if unknown or not active else "PASS"
    return {"status": status, "reason": "hot local stack adjustments remain" if adjustments else
            "hot stack access/address formation could be a red-zone local or incoming argument; local ownership unproved, or arm activity missing" if unknown or not active else
            "exact dynamic counts show no hot local stack allocation/deallocation; RBP save is separate",
            "hot_adjustments": adjustments, "unclassified": unknown}


def entry_frame_verdict(entry):
    preparation = [row for row in entry if row["Ir"] > 0 and
                   (re.match(r"^(push|enter)\b", row["instruction"]) or re.match(r"^(sub|add|lea|mov|and)\s+rsp,", row["instruction"]))]
    accesses = [row for row in entry if row["Ir"] > 0 and re.search(r"\[rsp(?:[+\-\]])", row["instruction"])]
    active = any(row["Ir"] > 0 for row in entry)
    return {"status": "FAIL" if preparation else "UNVERIFIED" if accesses or not active else "PASS",
            "reason": "entry frame preparation remains" if preparation else
            "entry stack access/address formation may use local red-zone storage or incoming arguments; ownership unproved" if accesses else
            "no counted entry frame preparation" if active else "entry activity missing", "preparation": preparation,
            "unclassified_stack_accesses": accesses}


def conjunction(statuses):
    for status in ("FAIL", "UNAVAILABLE", "UNVERIFIED"):
        if status in statuses:
            return status
    return "PASS" if statuses and all(s == "PASS" for s in statuses) else "UNVERIFIED"


def self_test():
    """Fixed classifier fixtures; only run by CI, no application benchmark."""
    good = [{"address": "0x10", "instruction": "mov    QWORD PTR [rsp+0x28],r8", "Ir": 0},
            {"address": "0x20", "instruction": "push   rbp", "Ir": 7}]
    mapping = {"pipeline_guard": {"status": "PASS", "arm_guards": {"arm": {"status": "PASS", "bounds_present": True}}},
               "source_pins": {"arm": ["wf.pin.8"]},
               "mappings": {"arm": [{"pin": "wf.pin.8", "base": "rsp", "offset": 40, "source": "r8"}]}}
    if pin_verdict(good, mapping, "arm")["status"] != "PASS":
        raise ValueError("good exact named cold pin fixture must pass")
    bad = [dict(good[0], Ir=1), good[1]]
    if pin_verdict(bad, mapping, "arm")["status"] != "FAIL":
        raise ValueError("one executed named pin store must fail")
    if pin_verdict(good, {}, "arm")["status"] != "UNVERIFIED":
        raise ValueError("missing provenance must stay unverified")
    if pin_verdict(good + [dict(good[0], address="0x30")], mapping, "arm")["status"] != "UNVERIFIED":
        raise ValueError("ambiguous mapping must stay unverified")
    absent = [dict(row, Ir=0) for row in good]
    if pin_verdict(absent, mapping, "arm")["status"] != "UNVERIFIED":
        raise ValueError("all-zero missing arm activity must not prove pin coldness")
    if local_frame_verdict(absent)["status"] != "UNVERIFIED" or entry_frame_verdict(absent)["status"] != "UNVERIFIED":
        raise ValueError("all-zero frame fixtures must not pass")
    for separator in (" ", "   ", "\t", " \t  "):
        rbp_only = [{"address": "0x20", "instruction": "push" + separator + "rbp", "Ir": 7},
                    {"address": "0x30", "instruction": "pop" + separator + "rbp", "Ir": 7}]
        if local_frame_verdict(rbp_only)["status"] != "PASS":
            raise ValueError("RBP-only push/pop must pass local-allocation criterion regardless of whitespace")
        if entry_frame_verdict(rbp_only[:1])["status"] != "FAIL":
            raise ValueError("RBP push must fail stronger full-frame criterion")
        if local_frame_verdict([dict(row, Ir=0) for row in rbp_only])["status"] != "UNVERIFIED":
            raise ValueError("all-zero RBP-only activity must stay unverified")
        for operand in ("rax", "rbp1"):
            other_push = {"address": "0x40", "instruction": "push" + separator + operand, "Ir": 1}
            if local_frame_verdict(rbp_only + [other_push])["status"] != "UNVERIFIED":
                raise ValueError("non-RBP push must stay unverified regardless of whitespace")
    for instruction in ("sub    rsp,0x80", "add    rsp,0x80"):
        if local_frame_verdict(rbp_only + [{"address": "0x40", "instruction": instruction, "Ir": 1}])["status"] != "FAIL":
            raise ValueError("hot local allocation/deallocation must fail")
    if local_frame_verdict([])["status"] != "UNVERIFIED":
        raise ValueError("missing arm must not pass")
    red_zone = [{"address": "0x50", "instruction": "mov    QWORD PTR [rsp-0x8],r8", "Ir": 1}]
    if local_frame_verdict(red_zone)["status"] != "UNVERIFIED" or entry_frame_verdict(red_zone)["status"] != "UNVERIFIED":
        raise ValueError("red-zone access must not pass merely because sub rsp is absent")
    mir = '---\nname: arm\nbody: |\n  MOV64mr $rsp, 1, $noreg, 40, $noreg, killed renamable $r8 :: (store (s64) into %ir.wf.pin.8)\n...\n'
    if parse_mir(mir)["arm"][0]["offset"] != 40:
        raise ValueError("MIR named-store fixture must retain final offset")
    if parse_mir(mir.replace("wf.pin.8", "wf.pin.80"))["arm"]:
        raise ValueError("a different pin number must not match pin 8")
    def shape_fixture(base):
        lines = []
        for index, number in enumerate(ARMS.values()):
            name = "wf_package.halo.vm.run$instance$abc.body.arm." + str(number)
            address = base + index * 64
            lines += [format(address, "x") + " <" + name + ">:",
                      " " + format(address, "x") + ": sub rsp,0x80",
                      " " + format(address + 4, "x") + ": lea rax,[rip+" + hex(base) + "] # " + format(base + 512, "x") + " <global>",
                      " " + format(address + 8, "x") + ": jne " + format(address + 16, "x") + " <" + name + "+0x10>"]
        return "\n".join(lines)
    def shape_bounds(base):
        return {"wf_package.halo.vm.run$instance$abc.body.arm." + str(number):
                {"start": base + index * 64, "size": 24} for index, number in enumerate(ARMS.values())}
    left = normalized_assembly(shape_fixture(4096), shape_bounds(4096))
    right = normalized_assembly(shape_fixture(8192).replace("<global>", "<different_nearest_symbol+0x40>"), shape_bounds(8192))
    if compare_shapes(left, right)["status"] != "PASS":
        raise ValueError("relocated body with different nearest-data annotations must reproduce")
    pad_address = 8192 + (len(ARMS) - 1) * 64 + 32
    padded = shape_fixture(8192) + "\n " + format(pad_address, "x") + ": int3"
    if compare_shapes(left, normalized_assembly(padded, shape_bounds(8192)))["status"] != "PASS":
        raise ValueError("padding outside declared function extent must not change its body shape")
    in_body_address = 8192 + (len(ARMS) - 1) * 64 + 16
    for opcode in ("nop", "int3"):
        in_body = shape_fixture(8192) + "\n " + format(in_body_address, "x") + ": " + opcode
        if compare_shapes(left, normalized_assembly(in_body, shape_bounds(8192)))["status"] != "FAIL":
            raise ValueError("an added in-body nop/int3 must fail reproduction")
    stack_body = shape_fixture(8192) + "\n " + format(in_body_address, "x") + ": mov QWORD PTR [rsp+0x28],r8"
    stack_left = normalized_assembly(stack_body, shape_bounds(8192))
    stack_right = normalized_assembly(stack_body.replace("[rsp+0x28]", "[rsp+0x30]"), shape_bounds(8192))
    if compare_shapes(stack_left, stack_right)["status"] != "FAIL":
        raise ValueError("changed in-body stack offset must fail reproduction")
    for mnemonic in ("call", "jmp", "bnd jmp", "notrack jmp"):
        indirect = shape_fixture(8192) + "\n " + format(in_body_address, "x") + ": " + mnemonic + " QWORD PTR [rip+0x20] # 3000 <callee>"
        moved = indirect.replace("[rip+0x20] # 3000", "[rip+0x40] # 4000")
        wrong_target = moved.replace("<callee>", "<different_callee>")
        indirect_left = normalized_assembly(indirect, shape_bounds(8192))
        if compare_shapes(indirect_left, normalized_assembly(moved, shape_bounds(8192)))["status"] != "PASS":
            raise ValueError("relocated indirect control transfer with same target must reproduce")
        if compare_shapes(indirect_left, normalized_assembly(wrong_target, shape_bounds(8192)))["status"] != "FAIL":
            raise ValueError("changed indirect RIP call/jump target must fail reproduction")
    for before, after in (("sub rsp,0x80", "sub rsp,0x40"), ("lea rax,", "mov rax,"),
                          ("[rip+0x2000]", "[rsp+0x28]"), ("+0x10>", "+0x14>")):
        changed = normalized_assembly(shape_fixture(8192).replace(before, after, 1), shape_bounds(8192))
        if compare_shapes(left, changed)["status"] != "FAIL":
            raise ValueError("changed in-body opcode, constant, frame operand or control target must fail reproduction")
    if compare_shapes(left, {})["status"] != "UNAVAILABLE":
        raise ValueError("missing disassembly must not reproduce")
    if compare_shapes(left, normalized_assembly(shape_fixture(8192)))["status"] != "UNAVAILABLE":
        raise ValueError("missing function bounds must not reproduce")
    symbol_rows = "\n".join(format(v["start"], "x") + " " + format(v["size"], "x") + " t " + k
                            for k, v in shape_bounds(8192).items())
    if parse_function_bounds(symbol_rows) != shape_bounds(8192):
        raise ValueError("nm bounds fixture must preserve each declared extent")
    incomplete_bounds = shape_bounds(8192)
    incomplete_bounds.pop(next(iter(incomplete_bounds)))
    if compare_shapes(left, normalized_assembly(shape_fixture(8192), incomplete_bounds))["status"] != "UNAVAILABLE":
        raise ValueError("missing an owned arm's bounds must not reproduce")
    truncated_body = normalized_assembly(shape_fixture(8192), shape_bounds(8192))
    truncated_body["functions"][next(iter(truncated_body["functions"]))] = []
    truncated_body["missing_instruction_bodies"] = [next(iter(truncated_body["functions"]))]
    if compare_shapes(left, truncated_body)["status"] != "UNAVAILABLE":
        raise ValueError("missing a bounded function body must not reproduce")
    extra_bounds = dict(shape_bounds(8192), other={"start": 65536, "size": 1})
    extra_left = normalized_assembly(shape_fixture(8192) + "\n10000 <other>:\n 10000: ret", extra_bounds)
    extra_right = normalized_assembly(shape_fixture(8192) + "\n10000 <other>:\n 10000: nop", extra_bounds)
    unrelated = compare_shapes(extra_left, extra_right)
    arm_name = "wf_package.halo.vm.run$instance$abc.body.arm.25"
    if unrelated["status"] != "FAIL" or unrelated["arm_guards"][arm_name]["status"] != "PASS":
        raise ValueError("unrelated function mismatch must remain globally failed with unchanged arm guard passing")
    scoped_mapping = {"pipeline_guard": unrelated, "source_pins": {arm_name: ["wf.pin.8"]},
                      "mappings": {arm_name: mapping["mappings"]["arm"]}}
    if pin_verdict(good, scoped_mapping, arm_name)["status"] != "PASS":
        raise ValueError("own-arm exact guard may validate its pin mapping despite unrelated module failure")
    own_changed = normalized_assembly(shape_fixture(8192).replace("sub rsp,0x80", "sub rsp,0x40", 2), shape_bounds(8192))
    scoped_mapping["pipeline_guard"] = compare_shapes(right, own_changed)
    if pin_verdict(good, scoped_mapping, arm_name)["status"] != "UNVERIFIED":
        raise ValueError("changed owned arm must prevent pin provenance")
    if read_count("events: Ir\nsummary: 7\n", "==1== Collected : 7\n") != 7:
        raise ValueError("valid count fixture rejected")
    for raw, stderr in (("events: Ir Dr\nsummary: 7\n", "==1== Collected : 7\n"),
                        ("events: Ir\nsummary: 7\n", "==1== Collected : 8\n"),
                        ("events: Ir\n", "==1== Collected : 7\n")):
        try:
            read_count(raw, stderr)
        except ValueError:
            continue
        raise ValueError("wrong/missing raw-count fixture must fail")
    validate_fib("2178309\n", 32)
    try:
        validate_fib("2178310\n", 32)
    except ValueError:
        pass
    else:
        raise ValueError("wrong fib stdout must fail")
    measured = {"lua_calls": CALLS, "total": CALLS * 5, "arms": {
        role: {"symbol": role, "self_per_call": 1, "self_per_call_callgrind_fn": 1} for role in ARMS}}
    validate_measurement(measured)
    for invalid in ({}, dict(measured, total=0), dict(measured, lua_calls=1),
                    dict(measured, arms={}), dict(measured, arms={role: dict(arm, self_per_call=0)
                                                               for role, arm in measured["arms"].items()})):
        try:
            validate_measurement(invalid)
        except ValueError:
            continue
        raise ValueError("missing/zero/misnormalized arm measurement must fail")
    print("PASS: representative good, hot-store, frame, missing, ambiguous, profiler-integrity and output fixtures")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("self-test", "build", "variants", "pilot", "counts", "report"))
    args = parser.parse_args()
    if args.phase == "self-test":
        self_test()
        Probe().command([sys.executable, SINK, "--self-test"], "sink-self-test", bound=90)
        return 0
    probe = Probe()
    result = getattr(probe, args.phase)()
    return result if isinstance(result, int) else 0


if __name__ == "__main__":
    sys.exit(main())
