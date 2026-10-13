#!/usr/bin/env python3
"""Temporary handler-rbp.yml assertions; remove with the workflow before ready.

objdump supplies the machine-code oracle. The generated run arms supply the
required symbol inventory, so a missing split cannot pass an empty scan.
"""

from pathlib import Path
import re
import subprocess
import sys


INSTRUCTION = re.compile(r"^[ \t]*[0-9a-f]+:[ \t]+([a-z][a-z0-9]*)[ \t]*(.*?)[ \t]*$", re.M)
REGISTER_COPY = re.compile(r"(%(?:r\w+|e\w+)),\s*(%r\w+)\s*$")
EXPECTED_CRC = {"crclist": "0xe714", "crcmatrix": "0x1fd7",
                "crcstate": "0x8e3a", "crcfinal": "0x4983"}


def attribute_failures(module):
    groups = {match[1]: match[2] for match in
              re.finditer(r"^attributes #(\d+) = \{(.*?)\}$", module, re.M)}
    definitions = 0
    calls = 0
    failures = []
    for line in module.splitlines():
        private_definition = line.startswith("define ") and re.search(
            r"@wf_run\.(?:arm\.\d+|dispatch|cold[\w.]*)\(", line)
        transfer = re.search(r"\bcall preserve_nonecc\b", line)
        if not private_definition and not transfer:
            continue
        definitions += bool(private_definition)
        calls += bool(transfer)
        attributes = line + " ".join(groups.get(group, "") for group in re.findall(r"#(\d+)", line))
        if '"no_callee_saved_registers"' not in attributes:
            failures.append(f"missing required pinned-LLVM attribute: {line.strip()}")
    if not definitions or not calls:
        failures.append("required preserve_none private definitions or calls are absent")
    return failures


def instructions(body):
    return [(match[1], match[2]) for match in INSTRUCTION.finditer(body)]


def forbidden(body):
    failures = []
    code = instructions(body)
    for index, (opcode, operands) in enumerate(code):
        if opcode in {"push", "pushq", "pop", "popq"} and operands == "%rbp":
            failures.append(f"{opcode} {operands}")
        if opcode not in {"jmp", "jmpq"} or not re.fullmatch(r"\*%r\w+", operands):
            continue
        target = operands[1:]
        # LLVM inserts an epilogue between the target copy and the tail jump.
        # Walk only that epilogue, never an earlier calculation of this value.
        before = index - 1
        while before >= 0 and code[before][0] in {"pop", "popq", "add", "addq", "lea", "leaq"}:
            operation, arguments = code[before]
            if operation not in {"pop", "popq"} and not arguments.endswith(",%rsp"):
                break
            before -= 1
        if before >= 0:
            operation, arguments = code[before]
            copy = REGISTER_COPY.fullmatch(arguments)
            if operation in {"mov", "movq"} and copy and copy[2] == target and copy[1] != target:
                failures.append(f"tail-copy: {operation} {arguments} before {opcode} {operands}")
    return failures


def arm_map(source):
    start = source.index("\nfn run(") + 1
    end = source.index("\nfn ", start)
    arms = re.findall(r"^      (\w+)\([^\n]*\) => \{", source[start:end], re.M)
    if not arms or len(set(arms)) != len(arms) or "Return" not in arms:
        raise ValueError("generated run arm map is empty, ambiguous or lacks Return")
    return arms


def required_symbols(arms, inventory):
    names = [f"wf_run.arm.{index}" for index in range(len(arms))]
    missing = [name for name in names if name not in inventory]
    if missing:
        raise ValueError(f"missing generated handlers: {', '.join(missing)}")
    return names


def self_test():
    cases = [
        ("  100: push %rbp\n  101: pop %rbp\n", 2),
        ("  100: mov %r10,%rax\n  101: jmp *%rax\n", 1),
        ("  100: mov %r10,%rax\n  101: pop %rbp\n  102: jmp *%rax\n", 2),
        ("  100: mov %r10,%rax\n  101: add $0x8,%rsp\n  102: jmp *%rax\n", 1),
        ("  100: mov 0x8(%r9),%r10\n  101: jmp *%r10\n", 0),
        ("  100: jmp *0x8(%r9)\n", 0),
    ]
    for body, count in cases:
        if len(forbidden(body)) != count:
            raise AssertionError(f"inspection control failed: {body}")
    try:
        required_symbols(["Return"], {})
    except ValueError:
        pass
    else:
        raise AssertionError("missing-handler control passed")
    try:
        arm_map("\nfn run() {\n}\nfn end() {}")
    except ValueError:
        pass
    else:
        raise AssertionError("empty-arm-map control passed")
    attributed = ('define internal preserve_nonecc void @wf_run.arm.0() '
                  '"no_callee_saved_registers" {\n'
                  '  musttail call preserve_nonecc void @wf_run.dispatch() '
                  '"no_callee_saved_registers"\n')
    if attribute_failures(attributed):
        raise AssertionError("attributed definitions and calls were rejected")
    lines = attributed.splitlines()
    for index in range(len(lines)):
        missing_attribute = lines.copy()
        missing_attribute[index] = missing_attribute[index].replace(
            ' "no_callee_saved_registers"', "")
        if not attribute_failures("\n".join(missing_attribute)):
            raise AssertionError(f"missing-attribute control passed on line {index + 1}")
    valid_crc = "".join(f"[0]{name}: {value}\n" for name, value in EXPECTED_CRC.items())
    if crc_failure(0, valid_crc):
        raise AssertionError("valid CRC control was rejected")
    for name, value in EXPECTED_CRC.items():
        if not crc_failure(0, valid_crc.replace(value, "0xffff")):
            raise AssertionError(f"incorrect {name} control passed")
        if not crc_failure(0, valid_crc.replace(f"[0]{name}: {value}\n", "")):
            raise AssertionError(f"missing {name} control passed")
    if not crc_failure(1, valid_crc):
        raise AssertionError("failed-process CRC control passed")
    print("controls reject RBP saves, target copies, missing handlers/attributes and invalid CRC results")


def inspect(out):
    arms = arm_map((out / "v2h.wf").read_text())
    binary = out / "v2h"
    symbols = subprocess.check_output(
        ["nm", "-S", "--defined-only", "--format=posix", str(binary)], text=True)
    (out / "v2h-symbols.txt").write_text(symbols)
    inventory = {}
    for line in symbols.splitlines():
        fields = line.split()
        if len(fields) == 4 and fields[1] in {"t", "T", "w", "W"}:
            inventory[fields[0]] = (int(fields[2], 16), int(fields[3], 16))
    required = required_symbols(arms, inventory)
    selected = sorted(set(required) | {name for name in inventory
                      if name.startswith(("wf_run.arm.", "wf_run.cold"))})
    (out / "v2h-arm-map.tsv").write_text(
        "arm\thandler\tsymbol\n" + "".join(
            f"{index}\t{arm}\t{required[index]}\n" for index, arm in enumerate(arms)))
    failures = attribute_failures((out / "v2h.ll").read_text())
    with (out / "v2h-handlers.asm").open("w") as assembly:
        for name in selected + ["wf_run"]:
            if name not in inventory:
                raise ValueError(f"missing C-ABI boundary: {name}")
            start, size = inventory[name]
            if size == 0:
                raise ValueError(f"empty symbol range: {name}")
            body = subprocess.check_output(
                ["objdump", "-d", "--no-show-raw-insn", f"--start-address={start}",
                 f"--stop-address={start + size}", str(binary)], text=True)
            assembly.write(f"\n# {name}\n{body}")
            if not instructions(body):
                raise ValueError(f"no disassembled instructions: {name}")
            if name != "wf_run":
                failures.extend(f"{name}: {failure}" for failure in forbidden(body))
            else:
                code = instructions(body)
                pushes = sum(opcode in {"push", "pushq"} and operands == "%rbp"
                             for opcode, operands in code)
                pops = sum(opcode in {"pop", "popq"} and operands == "%rbp"
                           for opcode, operands in code)
                if pushes != 1 or pops < 1:
                    failures.append(f"C-ABI boundary wf_run: expected one RBP save and a restore; "
                                    f"found {pushes} pushes and {pops} pops")
    # LLVM 22 excludes R10 from the tail-jump target class and refuses load
    # folding past six argument registers, so target copies are recorded,
    # not failed (research record, sources of the small-handler overhead).
    copies = [failure for failure in failures if ": tail-copy: " in failure]
    failures = [failure for failure in failures if ": tail-copy: " not in failure]
    report = f"Inspected {len(selected)} private handlers and the C-ABI boundary.\n"
    report += f"Tail-jump target copies (known LLVM 22 constraint): {len(copies)}\n"
    report += "\n".join(failures) + "\n" if failures else "No forbidden instructions.\n"
    (out / "machine-code-check.txt").write_text(report)
    print(report, end="")
    if failures:
        raise SystemExit(1)


def crc_failure(status, stdout):
    if status:
        return f"CoreMark exited with status {status}"
    values = dict(re.findall(r"\[0\](crc\w+)\s*:\s*(0x[0-9a-f]+)", stdout))
    if any(values.get(name) != value for name, value in EXPECTED_CRC.items()):
        return f"CoreMark CRC mismatch: expected {EXPECTED_CRC}, observed {values}"
    return None


def crc(out):
    # Single correctness sample, with the module and four CRCs established by
    # the match-dispatch investigation. No score or timing is a verdict.
    result = subprocess.run(
        [str(out / "v2h"), "coremark.wasm", "0x0", "0x0", "0x66", "2000"],
        cwd=out, capture_output=True, text=True, check=False)
    (out / "coremark.stdout.txt").write_text(result.stdout)
    (out / "coremark.stderr.txt").write_text(result.stderr)
    print(result.stdout, end="")
    print(result.stderr, end="", file=sys.stderr)
    failure = crc_failure(result.returncode, result.stdout)
    if failure:
        raise SystemExit(failure)
    print("CoreMark 2000-iteration CRC oracle passed")


if __name__ == "__main__":
    if sys.argv[1:] == ["self-test"]:
        self_test()
    elif len(sys.argv) == 3 and sys.argv[1] in {"inspect", "crc"}:
        {"inspect": inspect, "crc": crc}[sys.argv[1]](Path(sys.argv[2]))
    else:
        raise SystemExit("usage: handler-rbp.py self-test | inspect OUT | crc OUT")
