#!/usr/bin/env python3
"""Compile small hand-written IR with LLVM 22; never execute generated code."""

import argparse
import json
from pathlib import Path
import re
import subprocess


def instructions(assembly):
    """Count complete function regions, excluding directives, labels, comments."""
    functions = {}
    current = None
    for line in assembly.splitlines():
        label = re.match(r"^([A-Za-z_][A-Za-z_0-9]*):", line)
        if label:
            current = label.group(1)
            functions[current] = []
        if line.startswith(".Lfunc_end"):
            current = None
        instruction = re.match(r"^\s+([A-Za-z][A-Za-z0-9]*)\s*(.*?)\s*$", line)
        if current and instruction:
            functions[current].append(line.strip().split("#", 1)[0].strip())
    return {
        name: {"static_instructions": len(body), "instructions": body}
        for name, body in functions.items()
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--llvm-bin", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parent
    output = args.output.resolve()
    if output == source or source in output.parents:
        parser.error("use a scratch output directory outside the experiment sources")
    output.mkdir(parents=True, exist_ok=True)
    commands = []

    def run(arguments, stem):
        command = [str(value) for value in arguments]
        commands.append(command)
        (output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        result = subprocess.run(command, cwd=source, capture_output=True, text=True)
        (output / (stem + ".stdout.txt")).write_text(result.stdout)
        (output / (stem + ".stderr.txt")).write_text(result.stderr)
        if result.returncode:
            raise SystemExit(f"{command[0]} failed ({result.returncode}): {result.stderr}")
        return result

    versions = {}
    for tool in ("opt", "llc"):
        versions[tool] = run([args.llvm_bin / tool, "--version"], tool + ".version").stdout
        if not re.search(r"version 22\.", versions[tool]):
            raise SystemExit(f"{tool} must be LLVM 22")

    summary = {"versions": versions, "target": "x86_64-unknown-linux-gnu",
               "cpu": "alderlake", "cases": {}}
    for witness in sorted(source.glob("*.ll")):
        optimized = output / (witness.stem + ".opt.ll")
        assembly = output / (witness.stem + ".s")
        optimized_result = run([
            args.llvm_bin / "opt", "-O2", "-pass-remarks=inline",
            "-pass-remarks-missed=inline", "-S", witness.name, "-o", optimized,
        ], witness.stem + ".opt")
        run([
            args.llvm_bin / "llc", "-O2", "-mtriple=x86_64-unknown-linux-gnu",
            "-mcpu=alderlake", optimized, "-o", assembly,
        ], witness.stem + ".llc")
        functions = instructions(assembly.read_text())
        for name in ("caller_partial", "caller_full"):
            body = functions[name]["instructions"]
            # These hand-written witnesses have two return blocks and one
            # straight-line rare path; the untaken return is the only
            # instruction in the body absent from the taken path.
            if body.count("retq") != 2:
                raise SystemExit(f"inspect changed control flow in {witness.name}:{name}")
            count = len(body) - 1
            if any(re.match(r"callq\s+accessor(?:@PLT)?$", line) for line in body):
                count += functions["accessor"]["static_instructions"]
            functions[name]["taken_path_instructions_including_accessor"] = count
        summary["cases"][witness.stem] = {
            "inline_remarks": optimized_result.stderr.splitlines(),
            "functions": functions,
        }
        counts = ", ".join(f"{name}={data['static_instructions']}"
                           for name, data in functions.items())
        print(f"{witness.name}: {counts}")
        for remark in optimized_result.stderr.splitlines():
            print("  " + remark)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    table = ["case\taccessor_static\tpartial_static\tfull_static\tpartial_taken\tfull_taken"]
    for name, case in summary["cases"].items():
        functions = case["functions"]
        table.append("\t".join(map(str, (
            name, functions["accessor"]["static_instructions"],
            functions["caller_partial"]["static_instructions"],
            functions["caller_full"]["static_instructions"],
            functions["caller_partial"]["taken_path_instructions_including_accessor"],
            functions["caller_full"]["taken_path_instructions_including_accessor"],
        ))))
    (output / "summary.tsv").write_text("\n".join(table) + "\n")


if __name__ == "__main__":
    main()
