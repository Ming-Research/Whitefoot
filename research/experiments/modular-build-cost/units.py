#!/usr/bin/env python3
"""Paired module-product cost experiment, called by run.sh --units.

This independent process driver copies formal programs without importing the
experiment into their tests. Python owns process observation and scratch trees,
not compiler judgments. Remove it when this retained-product comparison retires.
Use a guarded invocation; compiler build time is excluded. Results are JSONL.
"""

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time


REPO = Path(__file__).resolve().parents[3]
PROGRAMS = {
    "sha256": "tests/programs/sha256_abc.wf",
    "grow-vector": "tests/programs/containers/grow-vector-program.wf",
    "wfgrep": "tests/programs/wfgrep.wf",
    "hash-map": "tests/programs/containers/hash-map-program.wf",
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def fixture(name, root):
    if name == "queue":
        shutil.copytree(REPO / "research/investigations/modular-compilation/demo", root)
        return "kernel", "inspect", root / "tools/inspect/run.wf", "stored", "saved"
    root.mkdir()
    if name.startswith("chain-"):
        count = int(name.split("-")[1])
        rows = []
        for index in range(count):
            previous = f"pkg::m{index - 1}" if index else None
            rows.append(f"pkg::m{index}: [{previous or ''}];")
            interface, body = [], []
            for function in range(16):
                signature = f"fn f{function}(value: u64) -> result: u64 pure"
                interface.append(f'public {signature} doc "Mixes one dependency value.";')
                value = f"{previous}::f{function}(value: value)" if previous else "value"
                body.append(f"{signature} {{\n  let inner = {value};\n  return inner +wrap 1_u64;\n}}")
            write(root / f"m{index}/module.wfm", "\n\n".join(interface) + "\n")
            write(root / f"m{index}/body.wf", "\n\n".join(body) + "\n")
        dependencies = f"pkg::m{count - 1}, std::process"
        calculation = f"pkg::m{count - 1}::f0(value: 0_u64)"
        body = f"  let observed = {calculation};\n  if observed == {count}_u64 {{\n    return std::process::exit_status(code: 0_u8);\n  }}\n  return std::process::exit_status(code: 1_u8);"
        parameters = ""
    else:
        source = (REPO / PROGRAMS[name]).read_text()
        dependencies = sorted(set(re.findall(r"\b(std(?:::[a-z_]+)+)::[A-Za-z_]+", source)))
        parameters = "inputs: std::process::Inputs" if name == "wfgrep" else ""
        signature = f"fn main({parameters}) -> status: std::process::ExitStatus pure"
        write(root / "work/module.wfm", f'public {signature} doc "Runs the maintained program unchanged.";\n')
        write(root / "work/body.wf", source)
        rows = [f"pkg::work: [{', '.join(dependencies)}];"]
        dependencies = "pkg::work, std::process"
        argument = "inputs: move inputs" if parameters else ""
        body = f"  let observed = pkg::work::main({argument});\n  return move observed;"
    rows += [f"pkg: [{dependencies}];", "", "entry first = pkg::first;", "", "entry second = pkg::second;"]
    write(root / "modules.wfg", "\n".join(rows) + "\n")
    write(root / "module.wfm", "\n\n".join(
        f'public fn {entry}({parameters}) -> status: std::process::ExitStatus pure doc "Runs the experiment entry.";'
        for entry in ("first", "second")) + "\n")
    for entry in ("first", "second"):
        write(root / f"{entry}.wf", f"fn {entry}({parameters}) -> status: std::process::ExitStatus pure {{\n{body}\n}}\n")
    return "first", "second", root / "second.wf", "observed", "answer"


def invoke(command, cwd):
    start = time.perf_counter_ns()
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        child = subprocess.Popen(command, cwd=cwd, stdout=output, stderr=errors)
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
        wall_ms = (time.perf_counter_ns() - start) / 1e6
        output.seek(0)
        errors.seek(0)
        stdout, stderr = output.read(), errors.read()
        if child.returncode:
            raise RuntimeError(f"exit {child.returncode}: {command}\n{stdout.decode()}\n{stderr.decode()}")
    stages = {}
    for name, elapsed in re.findall(rb"WF-STAGE (\w+) (\d+)", stderr):
        key = name.decode()
        stages[key] = stages.get(key, 0) + int(elapsed) / 1e6
    return stdout, wall_ms, int(usage.ru_maxrss), stages


def require_library_reuse(report, workload):
    entry_module = "pkg::tools::inspect" if workload == "queue" else "pkg"
    for field, walked in (("module_bodies", "checked"), ("module_lowerings", "lowered")):
        libraries = [row for row in report[field] if row["module"] not in (entry_module, "prelude")]
        if not libraries or not any(row["reused"] for row in libraries):
            raise RuntimeError(f"{workload}: no observed library reuse in {field}")
        if any(row[walked] for row in libraries):
            raise RuntimeError(f"{workload}: unchanged library work in {field}: {libraries}")


def instrument(tree):
    """Add the same stage observations to an exported baseline or candidate."""
    tree = tree.resolve()
    if tree == REPO:
        raise ValueError("instrument an exported scratch tree, not the working source")
    path = tree / "compiler/src/driver.rs"
    source = path.read_text()
    if "ExperimentStage" in source:
        raise ValueError("the scratch source is already instrumented")
    outcome = "products.as_ref()" if "let outcome = match products.as_ref()" in source else "receipts"
    edits = {
        "    let bundle = match modules {": '    let _source = ExperimentStage::new("source_and_inputs");\n    let bundle = match modules {',
        f"    let outcome = match {outcome} {{": f'    drop(_source);\n    let _check = ExperimentStage::new("formation_and_checking");\n    let outcome = match {outcome} {{',
        "    let checked = match outcome {": "    drop(_check);\n    let checked = match outcome {",
        "    let roots = modules.and(entry).map(|function| [function.id]);": '    let roots = modules.and(entry).map(|function| [function.id]);\n    let _lower = ExperimentStage::new("lowering");',
        "    // What this lowering did with each permission it was given, appended after": '    drop(_lower);\n    let _emit = ExperimentStage::new("emission");\n    // What this lowering did with each permission it was given, appended after',
    }
    for old, new in edits.items():
        if source.count(old) != 1:
            raise ValueError(f"stage boundary is not unique: {old}")
        source = source.replace(old, new)
    source += '''
// Temporary experiment-only observation; never an acceptance input.
pub(crate) struct ExperimentStage(&'static str, std::time::Instant);
std::thread_local! {
    static EXPERIMENT_STAGES: std::cell::RefCell<std::collections::BTreeMap<&'static str, u128>> =
        std::cell::RefCell::new(std::collections::BTreeMap::new());
}
impl ExperimentStage {
    pub(crate) fn new(name: &'static str) -> Self { Self(name, std::time::Instant::now()) }
}
impl Drop for ExperimentStage {
    fn drop(&mut self) {
        let elapsed = self.1.elapsed().as_nanos();
        EXPERIMENT_STAGES.with(|stages| {
            let mut stages = stages.borrow_mut();
            *stages.entry(self.0).or_default() += elapsed;
            if self.0 == "emission" {
                for (name, elapsed) in std::mem::take(&mut *stages) {
                    eprintln!("WF-STAGE {name} {elapsed}");
                }
            }
        });
    }
}
'''
    path.write_text(source)
    # Nested candidate-only observations attribute import costs; they are
    # subsets of the driver stages and must not be added to those totals.
    for relative, functions in {
        "semantic/check/products.rs": {
            "body_product_key": "body_key", "read_body_product": "body_import",
            "write_body_product": "body_retention", "form_retained_identities": "missing_identity_formation",
        },
        "lowering/builder/products.rs": {
            "new": "lowering_product_setup", "key": "lowering_key",
            "read": "lowering_import", "write": "lowering_retention",
        },
        "driver/products.rs": {"new": "body_container_setup", "open": "body_container_open"},
        "semantic/products/identity.rs": {"new": "source_identity_setup"},
        "driver/reads.rs": {"read_declarations": "declaration_reads"},
    }.items():
        path = tree / "compiler/src" / relative
        if not path.exists():
            continue
        source = path.read_text()
        for function, stage in functions.items():
            marker = f"fn {function}("
            if source.count(marker) != 1:
                raise ValueError(f"product stage boundary is not unique: {marker}")
            begin = source.index("{", source.index(marker)) + 1
            source = source[:begin] + f'\n        let _stage = crate::driver::ExperimentStage::new("{stage}");' + source[begin:]
        path.write_text(source)

    path = tree / "compiler/src/semantic/check/products.rs"
    if path.exists():
        source = path.read_text()
        for old, new in [
            (
                "        let entries = Vec::<RetainedIdentity>::read(&mut reader)?;",
                '        let _record = crate::driver::ExperimentStage::new("body_record_decode");\n        let entries = Vec::<RetainedIdentity>::read(&mut reader)?;',
            ),
            (
                "        let mapping = self.map_retained_identities(&entries, identities)?;",
                '        drop(_record);\n        let _mapping = crate::driver::ExperimentStage::new("body_identity_mapping");\n        let mapping = self.map_retained_identities(&entries, identities)?;\n        drop(_mapping);',
            ),
            (
                "        let mut staged_types = self.types.clone();",
                '        let _staging = crate::driver::ExperimentStage::new("body_staging_clone");\n        let mut staged_types = self.types.clone();',
            ),
            (
                "        let staged_identities = identities.staged();",
                "        let staged_identities = identities.staged();\n        drop(_staging);",
            ),
            (
                "            let current = (entry.old.0, *mapping.get(&entry.old)?);",
                '''            let current = (entry.old.0, *mapping.get(&entry.old)?);
            let _kind = crate::driver::ExperimentStage::new(match current.0 {
                IdentityKind::Function => "body_function_inputs",
                IdentityKind::Nominal => "body_nominal_inputs",
                IdentityKind::FunctionReference => "body_reference_inputs",
                _ => "body_other_inputs",
            });''',
            ),
            (
                "            return Some(imported.body);\n        }\n        *self.types = prior;",
                '            let _retirement = crate::driver::ExperimentStage::new("body_prior_retirement");\n            drop(prior);\n            drop(_retirement);\n            return Some(imported.body);\n        }\n        *self.types = prior;',
            ),
        ]:
            if source.count(old) != 1:
                raise ValueError(f"body import boundary is not unique: {old}")
            source = source.replace(old, new)
        marker = "fn decode_body_product("
        begin = source.index("{", source.index(marker)) + 1
        source = source[:begin] + '\n        let _inputs = crate::driver::ExperimentStage::new("body_input_validation");' + source[begin:]
        old = "        let sources = &identities.sources;\n        let origin ="
        if source.count(old) != 1:
            raise ValueError("body payload boundary is not unique")
        source = source.replace(old, '        drop(_inputs);\n        let _payload = crate::driver::ExperimentStage::new("body_payload_import");\n' + old)
        path.write_text(source)

    path = tree / "compiler/src/driver/cache.rs"
    source = path.read_text()
    old = '''        let scoped = self.scoped(material);
        let bytes = std::fs::read(self.record_path(family, &scoped)).ok()?;
        decode(&bytes, &scoped)'''
    new = '''        let _load = crate::driver::ExperimentStage::new(match family {
            "module-bodies" => "cache_body_load",
            "lowered-functions" => "cache_lowering_load",
            "proof-receipts" => "cache_proof_load",
            _ => "cache_other_load",
        });
        let _address = crate::driver::ExperimentStage::new("cache_address");
        let scoped = self.scoped(material);
        let path = self.record_path(family, &scoped);
        drop(_address);
        let _read = crate::driver::ExperimentStage::new("cache_file_read");
        let bytes = std::fs::read(path).ok()?;
        drop(_read);
        let _verify = crate::driver::ExperimentStage::new("cache_record_validation");
        decode(&bytes, &scoped)'''
    if source.count(old) != 1:
        raise ValueError("cache load boundary is not unique")
    path.write_text(source.replace(old, new))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--rounds", type=int, default=7)
    parser.add_argument("--compiler-only", action="store_true", help="measure --emit-llvm instead of native construction; isolates compiler peak RSS")
    parser.add_argument("--stages", action="store_true", help="allow instrumented binaries for attribution, separate from qualification timings")
    parser.add_argument("--require-reuse", action="store_true", help="require the native candidate's edited entry to walk no unchanged library bodies or lowerings")
    parser.add_argument("--workloads", nargs="+", default=["queue", *PROGRAMS, "chain-8", "chain-32"])
    args = parser.parse_args()
    if args.require_reuse and args.compiler_only:
        parser.error("--require-reuse needs the native build report")
    if platform.system() != "Darwin":
        parser.error("this measurement uses Darwin wait4 RSS bytes")
    compilers = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    compiler_hashes = {mode: digest(path) for mode, path in compilers.items()}
    instrumented = {mode: b"WF-STAGE " in path.read_bytes() for mode, path in compilers.items()}
    if any(instrumented.values()) and not args.stages:
        parser.error("stage-instrumented compiler in timing pair; use --stages only for attribution")
    if compilers["baseline"] != compilers["candidate"] and len(set(compiler_hashes.values())) == 1:
        parser.error("different compiler paths contain identical bytes; use the same path explicitly for a null comparison")
    print(json.dumps({"kind": "conditions", "host": platform.platform(), "rounds": args.rounds,
                      "instrumented": instrumented,
                      "compilers": {mode: {"path": str(path), "sha256": compiler_hashes[mode]} for mode, path in compilers.items()},
                      "programs": {name: {"path": path, "sha256": digest(REPO / path)} for name, path in PROGRAMS.items()}}), flush=True)
    with tempfile.TemporaryDirectory(prefix="whitefoot-units-") as scratch:
        scratch = Path(scratch)
        data = scratch / "input.txt"
        data.write_bytes(b"ordinary\nneedle here\nlast\n")
        for workload in args.workloads:
            for round_index in range(args.rounds):
                observations = {}
                order = ("baseline", "candidate") if round_index % 2 == 0 else ("candidate", "baseline")
                for mode in order:
                    tree = scratch / f"{workload}-{round_index}-{mode}"
                    first, second, edited, old, new = fixture(workload, tree)
                    cache = tree / "cache"
                    binary = tree / "program"
                    compiler = str(compilers[mode])
                    for step, entry in (("cold", first), ("warm", first), ("second-entry", second), ("entry-edit", second)):
                        if step == "entry-edit":
                            before = edited.read_text()
                            after = re.sub(rf"\b{old}\b", new, before)
                            if before == after:
                                raise RuntimeError(f"edit did not change {edited}")
                            edited.write_text(after)
                        command = [compiler, "--graph", "modules.wfg", "--entry", entry, "--cache", str(cache)]
                        if args.compiler_only:
                            llvm, wall_ms, peak, stages = invoke([*command, "--emit-llvm"], tree)
                            report, observed, compiler_peak = {}, None, peak
                        else:
                            stdout, wall_ms, peak, stages = invoke([*command, "--report", "-o", str(binary)], tree)
                            report = json.loads(stdout.splitlines()[-1])["build"]
                            if args.require_reuse and mode == "candidate" and step == "entry-edit":
                                require_library_reuse(report, workload)
                            run = subprocess.run([str(binary), *(["needle", data.name] if workload == "wfgrep" else [])], cwd=scratch, capture_output=True)
                            if run.returncode != 0:
                                raise RuntimeError(f"{workload}/{mode}/{step}: program exited {run.returncode}: {run.stderr!r}")
                            observed = (run.returncode, run.stdout, run.stderr)
                            # This untimed check observes a warm emitted-module lookup.
                            llvm, _, compiler_peak, _ = invoke([*command, "--emit-llvm"], tree)
                        key = (step, "runtime")
                        if key in observations and observations[key] != observed:
                            raise RuntimeError(f"{workload}/{step}: baseline/candidate runtime differs")
                        observations[key] = observed
                        key = (step, "llvm")
                        if key in observations and observations[key] != llvm:
                            raise RuntimeError(f"{workload}/{step}: baseline/candidate LLVM differs")
                        observations[key] = llvm
                        print(json.dumps({"kind": "sample", "workload": workload, "round": round_index,
                                          "mode": mode, "step": step, "wall_ms": wall_ms,
                                          "peak_rss_bytes": peak, "warm_emit_peak_rss_bytes": compiler_peak,
                                          "compiler_only": args.compiler_only, "stages_ms": stages,
                                          "cache_bytes": sum(p.stat().st_size for p in cache.rglob("*") if p.is_file()),
                                          "llvm_sha256": hashlib.sha256(llvm).hexdigest(),
                                          "runtime_stdout": observed[1].decode() if observed else None, "report": report}), flush=True)
                    shutil.rmtree(tree)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--instrument":
        instrument(Path(sys.argv[2]))
    else:
        main()
