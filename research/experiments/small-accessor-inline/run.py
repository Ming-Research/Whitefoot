#!/usr/bin/env python3
"""Compile small hand-written and extracted IR with LLVM 22; never execute generated code."""

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
        label = re.match(r"^([A-Za-z_][\w.$]*):", line)
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
    for tool in ("opt", "llc", "llvm-extract", "llvm-dis", "llvm-nm", "llvm-lto2"):
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
    snowghost(args.llvm_bin, source / "snowghost", output / "snowghost", run, versions)


def defined_functions(module):
    """Names of the functions a textual module defines, in order."""
    return re.findall(r"^define [^@]*@([\w.$]+)\(", module.read_text(), re.M)


def assembly_counts(path):
    return {name: data["static_instructions"]
            for name, data in instructions(path.read_text()).items()}


def snowghost(llvm_bin, source, output, run, versions):
    """Compile extracted Snowghost accessors whole and as per-function ThinLTO fragments.

    The whole-module pass is `opt -O2`; the fragment pass mirrors
    `whitefootc --cache --fragments function`: one module per function,
    the ThinLTO pre-link pipeline, then an in-process ThinLTO link whose
    import decisions use each function's summary instruction count.
    """
    output.mkdir(parents=True, exist_ok=True)
    cases = {}
    controls = {"styles-emitted-import-200": ("styles-emitted", ["--import-instr-limit=200"])}
    witnesses = {path.stem: path for path in sorted(source.glob("*.ll"))}
    plans = [(stem, stem, []) for stem in witnesses]
    plans += [(name, stem, extra) for name, (stem, extra) in controls.items()]
    for name, stem, extra in plans:
        witness = witnesses[stem].resolve()
        case = {"witness": witness.name, "lto_options": extra}
        if not extra:
            optimized = output / (stem + ".opt.ll")
            whole = run([llvm_bin / "opt", "-O2", "-pass-remarks=inline",
                         "-pass-remarks-missed=inline", "-S", witness, "-o", optimized],
                        "snowghost-" + stem + ".whole")
            assembly = output / (stem + ".whole.s")
            run([llvm_bin / "llc", "-O2", "-mtriple=x86_64-unknown-linux-gnu",
                 "-mcpu=alderlake", optimized, "-o", assembly], "snowghost-" + stem + ".whole.llc")
            case["whole_remarks"] = whole.stderr.splitlines()
            case["whole_static_instructions"] = assembly_counts(assembly)
        fragments = output / (name + ".fragments")
        fragments.mkdir(exist_ok=True)
        (output / (name + ".thin.s")).write_text("")
        summary_instructions, bitcode = {}, []
        for function in defined_functions(witness):
            fragment = fragments / (function + ".ll")
            code = fragments / (function + ".bc")
            run([llvm_bin / "llvm-extract", "--func=" + function, "-S", witness, "-o", fragment],
                "snowghost-" + name + "-" + function + ".extract")
            run([llvm_bin / "opt", "-passes=thinlto-pre-link<O2>", "--thinlto-bc", fragment,
                 "-o", code], "snowghost-" + name + "-" + function + ".prelink")
            listing = run([llvm_bin / "llvm-dis", code, "-o", "-"],
                          "snowghost-" + name + "-" + function + ".dis").stdout
            match = re.search(r'name: "' + re.escape(function) + r'".*?insts: (\d+)', listing)
            summary_instructions[function] = int(match.group(1))
            bitcode.append(code)
        resolutions = []
        for code in bitcode:
            for kind, flag in (("px", "--defined-only"), ("", "--undefined-only")):
                symbols = run([llvm_bin / "llvm-nm", "-j", flag, code],
                              "snowghost-" + name + "-nm").stdout.split()
                resolutions += ["-r=" + str(code) + "," + symbol + "," + kind for symbol in symbols]
        linked = fragments / "link"
        thin = run([llvm_bin / "llvm-lto2", "run", *extra, "-O2", "--thinlto-threads=1",
                    "--save-temps", "-pass-remarks=inline", "-pass-remarks-missed=inline",
                    "-o", linked, *resolutions, *bitcode], "snowghost-" + name + ".thinlto")
        imports, thin_counts = {}, {}
        for task in sorted(fragments.glob("link.*.3.import.bc")):
            listing = run([llvm_bin / "llvm-dis", task, "-o", "-"],
                          "snowghost-" + name + ".imports").stdout
            owned = re.findall(r"^define (?!available_externally)[^@]*@([\w.$]+)\(", listing, re.M)
            imported = re.findall(r"^define available_externally [^@]*@([\w.$]+)\(", listing, re.M)
            for function in owned:
                imports[function] = imported
            final = Path(str(task).replace(".3.import.bc", ".5.precodegen.bc"))
            assembly = Path(str(task).replace(".3.import.bc", ".s"))
            run([llvm_bin / "llc", "-O2", "-mtriple=x86_64-unknown-linux-gnu",
                 "-mcpu=alderlake", final, "-o", assembly], "snowghost-" + name + ".thin.llc")
            thin_counts.update(assembly_counts(assembly))
            with (output / (name + ".thin.s")).open("a") as combined:
                combined.write(assembly.read_text())
        case.update({"summary_instructions": summary_instructions, "imports": imports,
                     "thinlto_remarks": thin.stderr.splitlines(),
                     "thinlto_static_instructions": thin_counts})
        cases[name] = case
        print(f"snowghost {name}: summary instructions {summary_instructions}")
        for remark in case.get("whole_remarks", []) + case["thinlto_remarks"]:
            print("  " + remark)
    (output / "summary.json").write_text(json.dumps(
        {"versions": versions, "cases": cases}, indent=2) + "\n")
    table = ["case\tfunction\tsummary_instructions\timports\twhole_static\tthin_static"]
    for name, case in cases.items():
        for function, count in case["summary_instructions"].items():
            table.append("\t".join(map(str, (
                name, function, count, " ".join(case["imports"].get(function, [])) or "-",
                case.get("whole_static_instructions", {}).get(function, "-"),
                case["thinlto_static_instructions"].get(function, "-")))))
    (output / "summary.tsv").write_text("\n".join(table) + "\n")


if __name__ == "__main__":
    main()
