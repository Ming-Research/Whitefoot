"""Machine-code identity checks of the call-grain-args investigation.

Criterion 4 compares the --par LLVM of the five formal kernels between main and
the change. Criterion 5 compares, in the change's --par object, the functions of
the one-thread path (the sequential clones, `wf__par_seq_X`) with the
sequential build's `wf_X`, by normalized disassembly of the objects. Both checks
carry controls that must fail the comparison they could vacuously pass.
"""
import difflib
import re
import subprocess

CLONE = "wf__par_seq_"
SKIP = re.compile(r"^wf_(bench_|fixture_|release_)")
HEADER = re.compile(r"^[0-9a-f]+ <([^>]+)>:$")
ADDRESS = re.compile(r"^\s*[0-9a-f]+:\s?")
TARGET = re.compile(r"\b[0-9a-f]+ (<[^>]+>)")
LOCAL = re.compile(r"\.L[A-Za-z_]*\d+(?:_\d+)?")


def normalize(line):
    """One disassembly line without addresses, with sequential-clone names mapped back."""
    line = ADDRESS.sub("", line.rstrip())
    line = TARGET.sub(r"\1", line)
    line = line.replace(CLONE, "wf_")
    return LOCAL.sub(".L#", line)


def disassemble(path, objdump="objdump"):
    """symbol -> normalized lines of an object file, trailing padding removed."""
    text = subprocess.run([objdump, "-d", "-r", "--no-show-raw-insn", str(path)], check=True,
                          capture_output=True, text=True).stdout
    return parse(text)


def parse(text):
    functions, current = {}, None
    for line in text.splitlines():
        match = HEADER.match(line)
        if match:
            current = match.group(1)
            functions[current] = []
        elif current is not None and line.strip() and not line.startswith(("Disassembly", "/")):
            functions[current].append(line)
    result = {}
    for name, lines in functions.items():
        body = [normalize(x) for x in lines]
        while body and re.fullmatch(r"\s*(nop\w*|xchg\s+%ax,%ax|data16.*|cs nopw.*)(\s.*)?", body[-1]):
            body.pop()
        result[name] = body
    return result


def compare_clones(sequential, parallel, required):
    """Compare each sequential wf_X with the parallel object's clone of it.

    sequential and parallel map symbol -> normalized lines. A function with no
    clone is compared with the parallel object's own wf_X (the call chain
    reaches no overlapped code). Returns (pairs, failures): pairs maps the
    symbol to one of identical, different, missing, and failures lists the
    reasons the check fails. `required` symbols must be present and identical.
    """
    pairs, failures = {}, []
    for name in sorted(sequential):
        if not name.startswith("wf_") or name.startswith(CLONE) or SKIP.match(name):
            continue
        twin = CLONE + name[3:]
        counterpart = twin if twin in parallel else name if name in parallel else None
        if counterpart is None:
            pairs[name] = "missing"
            failures.append(f"{name}: no counterpart in the parallel object")
        elif parallel[counterpart] == sequential[name]:
            pairs[name] = "identical"
        else:
            pairs[name] = "different"
            failures.append(f"{name}: differs from {counterpart}")
    for name in required:
        if pairs.get(name) != "identical":
            failures.append(f"{name}: required and not identical ({pairs.get(name, 'absent')})")
    return pairs, failures


def first_difference(a, b, context=3, limit=40):
    return "\n".join(list(difflib.unified_diff(a, b, "sequential", "clone", lineterm="", n=context))[:limit])


def same_bytes(a, b):
    with open(a, "rb") as x, open(b, "rb") as y:
        return x.read() == y.read()


def compile_llvm(wfc, source, flags, output, ledger=False):
    """Compile to LLVM text; returns the ledger text (empty when not asked)."""
    command = [wfc, *flags, *(["--par-ledger"] if ledger else []), "--emit-llvm", str(source), "-o", str(output)]
    return subprocess.run(command, check=True, capture_output=True, text=True).stdout


def llvm_checks(base_wfc, cand_wfc, kernels, fixtures, scratch):
    """Criterion 4 and its controls over the kernel sources in `fixtures`.

    per kernel: base twice (determinism), base vs candidate (the claim), base
    --par vs base sequential (a comparison that must report a difference). The
    controls refuse a check that cannot tell: empty output, or two
    different paths that always compare equal.
    """
    results, scratch = {}, scratch
    scratch.mkdir(parents=True, exist_ok=True)
    for kernel in kernels:
        source = fixtures / f"{kernel}.wf"
        files = {}
        for label, wfc, flags in (("base1", base_wfc, ["--par"]), ("base2", base_wfc, ["--par"]),
                                  ("cand", cand_wfc, ["--par"]), ("seq", base_wfc, [])):
            out = scratch / f"{kernel}.{label}.ll"
            compile_llvm(wfc, source, flags, out)
            files[label] = out
        sizes = {k: v.stat().st_size for k, v in files.items()}
        has_code = all("define " in v.read_text() for v in files.values())
        results[kernel] = dict(
            sizes=sizes, has_code=has_code,
            deterministic=same_bytes(files["base1"], files["base2"]),
            identical=same_bytes(files["base1"], files["cand"]),
            control_detects_difference=not same_bytes(files["base1"], files["seq"]))
    return results
