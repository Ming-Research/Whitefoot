#!/usr/bin/env python3
"""Layout tools of the code-placement experiment (DESIGN.md, "The next runs").

usage:
  layout.py rewrite MODE IN_LL OUT_LL OUT_CLAIMS [HOST_LL]
  layout.py loops OBJECT
  layout.py choose OBJECT OUT_CHOICE
  layout.py datadelta IMAGE_FROM IMAGE_TO
  layout.py check IMAGES_TSV
  layout.py lines IMAGE PERF_SCRIPT
  layout.py self-test

`rewrite` writes one arm's module from the aligned emitted module. MODE is
`keep`, `strip` (no alignment), `align=N` (every aligned definition aligned
to N instead), `offset=N` (every aligned definition N bytes past a 64-byte
boundary, through prefix data) or `choice=FILE` (each definition at the
offset `choose` wrote for it). Every definition must carry ` align 64` except
those of HOST_LL, the kernel's appended host adapter, which carry none; the
claims file lists each rewritten definition with the alignment, offset and
crossing-loop count the image must show.

`loops` prints, for each function of an object, its innermost loops of at
most 64 bytes and how many of them cross a 64-byte line at each start offset
0, 16, 32 and 48. A loop is the span from a backward branch's target to the
end of the furthest branch back to it, and it is innermost when no other
such span lies inside it. `choose` writes, per function, the offset with the
fewest crossing loops, the smallest on a tie.

`check` reads placement.sh's images.tsv and refuses the run before any
timing when an image does not have the layout its arm and placement claim:
a module function or runtime function at the wrong offset, a code pad that
did not move the module or runtime by what it claims, a data pad that moved
code or did not move the module's and runtime's data by its size, a `dFR`
image whose data addresses differ from `FR`'s, or a function of a `P` module
whose crossing loops are not the number its offset predicted.

`datadelta` prints by how many bytes the data of IMAGE_TO lies above that of
IMAGE_FROM, the size of the filler `dFR` links. `lines` reduces `perf script
-F sym,symoff` text to samples per function and 64-byte line.

The tools read GNU objdump's output (OBJDUMP, default `objdump`).
"""

import collections
import functools
import os
import re
import subprocess
import sys
import tempfile

LINE = 64
OFFSETS = (0, 16, 32, 48)
ALIGNED = " align 64 {"
NAME = re.compile(r'@("(?:[^"\\]|\\.)*"|[-a-zA-Z$._][-a-zA-Z$._0-9]*)\(')
FUNCTION = re.compile(r"^([0-9a-f]+) <(.+)>:$")
INSTRUCTION = re.compile(r"^\s*([0-9a-f]+):\t([0-9a-f]{2}(?: [0-9a-f]{2})*) *(?:\t(.*))?$")
TARGET = re.compile(r"^([0-9a-f]+)(?: <|$)")
PREFIXES = {"bnd", "notrack", "rep", "repz", "repnz", "ds", "cs", "data16", "addr32", "lock"}
SYMBOL = re.compile(r"^([0-9a-f]+) (.{7}) (\S+)\s+([0-9a-f]+)\s+(?:\.hidden |\.protected |\.internal )?(\S+)$")
SAMPLE = re.compile(r"(\S+)\+0x([0-9a-f]+)")


class Refused(Exception):
    pass


def objdump(*arguments):
    return subprocess.run([os.environ.get("OBJDUMP", "objdump"), *arguments], check=True,
                          capture_output=True, text=True).stdout


# --- rewriting the module ---------------------------------------------------

def function_name(line):
    match = NAME.search(line)
    if not match:
        raise Refused(f"no function name in: {line[:120]}")
    name = match.group(1)
    return name[1:-1] if name.startswith('"') else name


def defined_names(text):
    return [function_name(line) for line in text.splitlines() if line.startswith("define ")]


def read_choice(text):
    choice = {}
    for line in text.splitlines():
        if line.strip():
            name, offset, predicted = line.split()
            choice[name] = (int(offset), int(predicted))
    return choice


def rewrite(mode, text, host_names=()):
    """Returns the rewritten module and its claims, (name, modulus, offset,
    predicted crossings or None) per rewritten definition."""
    choice = None
    if mode.startswith("choice="):
        with open(mode.split("=", 1)[1], encoding="utf-8") as handle:
            choice = read_choice(handle.read())
    out, claims, aligned = [], [], 0
    for line in text.splitlines(keepends=True):
        body = line.rstrip("\n")
        if not body.startswith("define "):
            out.append(line)
            continue
        name = function_name(body)
        if not body.endswith(ALIGNED):
            if name in host_names:
                out.append(line)
                continue
            raise Refused(f"definition without align 64: {name}")
        aligned += 1
        head = body[: -len(ALIGNED)]
        if mode == "keep":
            new, claim = body, (name, 64, 0, None)
        elif mode == "strip":
            new, claim = head + " {", None
        elif mode.startswith("align="):
            modulus = int(mode.split("=", 1)[1])
            new, claim = f"{head} align {modulus} {{", (name, modulus, 0, None)
        elif mode.startswith("offset="):
            offset = int(mode.split("=", 1)[1])
            new, claim = with_offset(head, offset), (name, 64, offset, None)
        elif choice is not None:
            offset, predicted = choice.get(name, (0, None))
            new, claim = with_offset(head, offset), (name, 64, offset, predicted)
        else:
            raise Refused(f"unknown mode {mode}")
        out.append(new + "\n")
        if claim:
            claims.append(claim)
    if not aligned:
        raise Refused("the module has no aligned definition")
    result = "".join(out)
    if mode == "strip" and ALIGNED in "".join(l for l in result.splitlines() if l.startswith("define ")):
        raise Refused("the stripped module still has an aligned definition")
    return result, claims


def with_offset(head, offset):
    if offset not in OFFSETS:
        raise Refused(f"offset {offset} is not one of {OFFSETS}")
    if offset == 0:
        return head + ALIGNED
    return f"{head} align 64 prefix [{offset} x i8] zeroinitializer {{"


def format_claims(claims):
    return "".join(f"{name} {modulus} {offset} {'-' if predicted is None else predicted}\n"
                   for name, modulus, offset, predicted in claims)


def read_claims(text):
    claims = []
    for line in text.splitlines():
        if line.strip():
            name, modulus, offset, predicted = line.split()
            claims.append((name, int(modulus), int(offset), None if predicted == "-" else int(predicted)))
    return claims


# --- loops ------------------------------------------------------------------

def functions(disassembly):
    """{name: (start, [(address, length, mnemonic, target or None)])} from
    `objdump -d --insn-width=16`."""
    result, current = {}, None
    for line in disassembly.splitlines():
        header = FUNCTION.match(line)
        if header:
            current = header.group(2)
            result[current] = (int(header.group(1), 16), [])
            continue
        match = INSTRUCTION.match(line)
        if not match or current is None:
            continue
        address = int(match.group(1), 16)
        length = len(match.group(2).split())
        words = (match.group(3) or "").split()
        while words and words[0] in PREFIXES:
            words = words[1:]
        mnemonic = words[0] if words else ""
        target = None
        if (mnemonic.startswith("j") or mnemonic.startswith("loop")) and len(words) > 1:
            found = TARGET.match(" ".join(words[1:]))
            if found:
                target = int(found.group(1), 16)
        result[current][1].append((address, length, mnemonic, target))
    return result


def innermost_loops(start, instructions):
    """Innermost loops as (offset from start, length), shortest first."""
    furthest = {}
    for address, length, _, target in instructions:
        if target is not None and start <= target <= address:
            end = address + length
            furthest[target] = max(furthest.get(target, 0), end)
    spans = sorted(furthest.items())
    inner = [(a, b) for a, b in spans
             if not any((c, d) != (a, b) and a <= c and d <= b for c, d in spans)]
    return [(a - start, b - a) for a, b in inner]


def crossing(loops, start_offset):
    """How many loops of at most a line cross a line at this start offset."""
    return sum(1 for offset, length in loops if length <= LINE and
               (start_offset + offset) // LINE != (start_offset + offset + length - 1) // LINE)


def loop_table(disassembly):
    table = {}
    for name, (start, instructions) in functions(disassembly).items():
        loops = [loop for loop in innermost_loops(start, instructions) if loop[1] <= LINE]
        table[name] = (start, loops, {s: crossing(loops, s) for s in OFFSETS})
    return table


def choose(table):
    choice = {}
    for name, (_, loops, counts) in table.items():
        best = min(OFFSETS, key=lambda s: (counts[s], s))
        choice[name] = (best, counts[best])
    return choice


def format_table(table):
    lines = []
    for name, (start, loops, counts) in sorted(table.items()):
        if not loops:
            continue
        spans = ", ".join(f"{offset:#x}+{length}" for offset, length in loops)
        crossings = " ".join(f"{s}:{counts[s]}" for s in OFFSETS)
        lines.append(f"{name} start={start % LINE} loops [{spans}] crossing at start {crossings}")
    return "\n".join(lines) + "\n" if lines else "no innermost loop of at most 64 bytes\n"


# --- symbol tables ----------------------------------------------------------

def symbols(table_text):
    """{name: (section, address, is_function)} from `objdump -t`; a name
    defined more than once maps to None."""
    seen = {}
    for line in table_text.splitlines():
        match = SYMBOL.match(line)
        if not match:
            continue
        address, flags, section, _, name = match.groups()
        if section in ("*UND*", "*ABS*") or flags[5] == "d" or flags[6] == "f":
            continue
        entry = (section, int(address, 16), flags[6] == "F")
        seen[name] = None if name in seen else entry
    return seen


def defined(object_tables, sections):
    """Names the objects' tables define in an input section the predicate
    accepts, functions or data alike."""
    names = set()
    for table in object_tables:
        for name, entry in symbols(table).items():
            if entry and sections(entry[0]):
                names.add(name)
    return names


def code_section(section):
    return section == ".text"


def data_section(section):
    return section.startswith((".data", ".bss")) and not section.startswith(".data.rel.ro")


def shift(names, before, after, sections):
    """The set of address differences of the names in both images' sections."""
    differences = set()
    for name in names:
        a, b = before.get(name), after.get(name)
        if a and b and a[0] in sections and b[0] in sections:
            differences.add(b[1] - a[1])
    return differences


def data_delta(before_text, after_text):
    before, after = symbols(before_text), symbols(after_text)
    differences = shift([n for n, e in before.items() if e and e[0] == ".data"], before, after, {".data"})
    if len(differences) != 1:
        raise Refused(f"the .data symbols do not move by one amount: {sorted(differences)}")
    return differences.pop()


# --- the image check --------------------------------------------------------

# One image per row: its kernel, arm and placement, its path, its module
# object, the module's claims file or `none`, the runtime's claim as
# `MODULUS@OBJECT,...` for the runtime objects compiled aligned or `none`,
# and every runtime object, comma-separated.
Row = collections.namedtuple("Row", "kernel arm placement image module claims runtime_claim runtime")


def read_rows(text):
    rows = []
    for number, line in enumerate(text.splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 8:
            raise Refused(f"images.tsv row {number} has {len(fields)} fields")
        kernel, arm, placement, image, module, claims, runtime_claim, runtime = fields
        rows.append(Row(kernel, arm, placement, image, module, claims, runtime_claim,
                        [path for path in runtime.split(",") if path]))
    return rows


def runtime_claim(text):
    """(modulus, objects) of a runtime claim, or None."""
    if text == "none":
        return None
    modulus, objects = text.split("@", 1)
    return int(modulus), [path for path in objects.split(",") if path]


def expected_shift(placement, aligned):
    """The module's or runtime's move a code pad claims: exactly its size, or
    0 or 64 for a pad under a line ahead of aligned code."""
    size = int(placement[1:])
    if aligned and size < LINE:
        return {0, LINE}
    return {size}


def check_rows(rows, image_table, object_table, read_text, object_disassembly):
    """Every problem with the images' layouts. The callables return the
    `objdump -t` text of an image or object, a file's text and an object's
    disassembly."""
    problems = []
    by_key = {(row.kernel, row.arm, row.placement): row for row in rows}
    images = {}

    def table(path):
        if path not in images:
            images[path] = symbols(image_table(path))
        return images[path]

    for row in rows:
        here = table(row.image)
        name = f"{row.arm} {row.placement} {row.kernel}"
        module_names = defined([object_table(row.module)], code_section)
        runtime_names = defined([object_table(path) for path in row.runtime], code_section)
        claims = read_claims(read_text(row.claims)) if row.claims != "none" else []
        present = 0
        for function, modulus, offset, _ in claims:
            entry = here.get(function)
            if not entry:
                continue
            present += 1
            if entry[1] % modulus != offset:
                problems.append(f"{name}: {function} starts at {entry[1] % modulus} modulo {modulus}, "
                                f"not {offset}")
        if claims and not present:
            problems.append(f"{name}: no claimed module function is in the image")
        claimed_runtime = runtime_claim(row.runtime_claim)
        if claimed_runtime:
            modulus, objects = claimed_runtime
            for function in sorted(defined([object_table(path) for path in objects], code_section)):
                entry = here.get(function)
                if entry and entry[2] and entry[0] == ".text" and entry[1] % modulus:
                    problems.append(f"{name}: runtime function {function} starts at "
                                    f"{entry[1] % modulus} modulo {modulus}")
        if row.placement == "p0":
            continue
        reference = by_key.get((row.kernel, "U" if row.placement == "dFR" else row.arm, "p0"))
        if reference is None:
            problems.append(f"{name}: no p0 image to compare with")
            continue
        there = table(reference.image)
        if row.placement[0] in "mr":
            moved = shift(module_names, there, here, {".text"})
            runtime_moved = shift(runtime_names, there, here, {".text"})
            if row.placement[0] == "m":
                expected = expected_shift(row.placement, bool(claims))
                if len(moved) != 1 or not moved <= expected:
                    problems.append(f"{name}: the module moved by {sorted(moved)}, not one of {sorted(expected)}")
            else:
                expected = expected_shift(row.placement, claimed_runtime is not None)
                if moved != {0}:
                    problems.append(f"{name}: the module moved by {sorted(moved)} under a runtime pad")
                if len(runtime_moved) != 1 or not runtime_moved <= expected:
                    problems.append(f"{name}: the runtime moved by {sorted(runtime_moved)}, "
                                    f"not one of {sorted(expected)}")
        elif row.placement[0] == "d":
            data_names = defined([object_table(row.module)] + [object_table(path) for path in row.runtime],
                                 data_section)
            text_moved = shift(module_names | runtime_names, there, here, {".text"})
            if text_moved != {0}:
                problems.append(f"{name}: code moved by {sorted(text_moved)} under a data pad")
            if row.placement == "dFR":
                data_reference = by_key.get((row.kernel, "FR", "p0"))
                if data_reference is None:
                    problems.append(f"{name}: no FR p0 image to take the data addresses from")
                    continue
                data_moved = shift(data_names, table(data_reference.image), here, {".data", ".bss"})
                expected = {0}
            else:
                data_moved = shift(data_names, there, here, {".data", ".bss"})
                expected = {int(row.placement[1:])}
            if data_moved != expected:
                problems.append(f"{name}: the data moved by {sorted(data_moved)}, not {sorted(expected)}"
                                + (" against FR p0" if row.placement == "dFR" else ""))
        else:
            problems.append(f"{name}: unknown placement")
    checked = set()
    for row in rows:
        if row.claims == "none" or (row.module, row.claims) in checked:
            continue
        checked.add((row.module, row.claims))
        claims = read_claims(read_text(row.claims))
        if not any(predicted is not None for _, _, _, predicted in claims):
            continue
        table_now = loop_table(object_disassembly(row.module))
        for function, _, offset, predicted in claims:
            if predicted is None or function not in table_now:
                continue
            start, loops, _ = table_now[function]
            actual = crossing(loops, start % LINE)
            if start % LINE != offset or actual != predicted:
                problems.append(f"{row.arm} {row.kernel}: {function} starts at {start % LINE} with "
                                f"{actual} crossing loop(s); its choice was {offset} with {predicted}")
    return problems


# --- samples per line -------------------------------------------------------

def line_samples(table_text, script_text):
    table = symbols(table_text)
    counts = collections.Counter()
    total = 0
    for line in script_text.splitlines():
        match = SAMPLE.search(line)
        total += 1
        if not match:
            continue
        name, offset = match.group(1), int(match.group(2), 16)
        entry = table.get(name)
        if not entry:
            continue
        address = entry[1] + offset
        counts[(name, address // LINE * LINE, entry[1])] += 1
    return counts, total


def format_lines(counts, total, top=40):
    lines = [f"{total} samples; samples per function and 64-byte line, the line as its offset from "
             f"the function's start and its address modulo 4096"]
    for (name, line, start), count in counts.most_common(top):
        lines.append(f"{count:8d} {count / max(total, 1):6.1%} {name} +{line - start:#06x} "
                     f"line {line % 4096:#06x}")
    return "\n".join(lines) + "\n"


# --- the command line -------------------------------------------------------

def main(arguments):
    if not arguments:
        sys.exit(__doc__)
    command, rest = arguments[0], arguments[1:]
    try:
        if command == "rewrite" and len(rest) in (4, 5):
            mode, source, target, claims_path = rest[:4]
            host = defined_names(open(rest[4], encoding="utf-8").read()) if len(rest) == 5 else ()
            text, claims = rewrite(mode, open(source, encoding="utf-8").read(), set(host))
            with open(target, "w", encoding="utf-8") as out:
                out.write(text)
            with open(claims_path, "w", encoding="utf-8") as out:
                out.write(format_claims(claims))
        elif command == "loops" and len(rest) == 1:
            sys.stdout.write(format_table(loop_table(objdump("-d", "--insn-width=16", rest[0]))))
        elif command == "choose" and len(rest) == 2:
            choice = choose(loop_table(objdump("-d", "--insn-width=16", rest[0])))
            with open(rest[1], "w", encoding="utf-8") as out:
                out.writelines(f"{name} {offset} {predicted}\n" for name, (offset, predicted) in sorted(choice.items()))
        elif command == "datadelta" and len(rest) == 2:
            print(data_delta(objdump("-t", rest[0]), objdump("-t", rest[1])))
        elif command == "check" and len(rest) == 1:
            rows = read_rows(open(rest[0], encoding="utf-8").read())
            tables = functools.lru_cache(maxsize=None)(lambda path: objdump("-t", path))
            problems = check_rows(rows, tables, tables, lambda p: open(p, encoding="utf-8").read(),
                                  lambda p: objdump("-d", "--insn-width=16", p))
            for problem in problems:
                print(f"LAYOUT: {problem}")
            if problems:
                sys.exit(f"REFUSED: {len(problems)} layout problem(s)")
            print(f"layout check: {len(rows)} images as their arms and placements claim")
        elif command == "lines" and len(rest) == 2:
            counts, total = line_samples(objdump("-t", rest[0]), open(rest[1], encoding="utf-8").read())
            sys.stdout.write(format_lines(counts, total))
        elif command == "self-test" and not rest:
            self_test()
        else:
            sys.exit(__doc__)
    except Refused as refusal:
        sys.exit(f"REFUSED: {refusal}")


# --- self-test --------------------------------------------------------------

def disassembly(functions_spec):
    """objdump -d text for {name: (start, [(offset, length, mnemonic, target offset or None)])}."""
    lines = []
    for name, (start, instructions) in functions_spec.items():
        lines.append(f"{start:016x} <{name}>:")
        for offset, length, mnemonic, target in instructions:
            raw = " ".join(["90"] * length)
            operand = f" {start + target:x} <{name}+{target:#x}>" if target is not None else ""
            lines.append(f"  {start + offset:x}:\t{raw:<47}\t{mnemonic}{operand}")
    return "\n".join(lines) + "\n"


def symbol_table(entries):
    """objdump -t text for [(name, section, address, is_function)]."""
    return "".join(f"{address:016x} {'g     F' if function else 'g     O'} {section}\t"
                   f"{16:016x} {name}\n" for name, section, address, function in entries) + \
        "0000000000000000 l    df *ABS*\t0000000000000000 crt.c\n"


def self_test():
    failures = []

    def expect(label, actual, wanted):
        if actual != wanted:
            failures.append(f"{label}: got {actual!r}, wanted {wanted!r}")

    # The October table: a 54-byte loop at 0x110 and a 21-byte loop at 0x170
    # cross at start 0, 16 and 32 and none at 48; a longer outer loop around
    # them is not innermost; a forward jump and a branch before the start are
    # not loops.
    hot = {"hot": (0x1000, [(0x100, 4, "mov", None), (0x110, 50, "add", None),
                            (0x142, 4, "jne", 0x110), (0x170, 17, "add", None),
                            (0x181, 4, "jb", 0x170), (0x185, 2, "jmp", 0x1f0),
                            (0x190, 2, "jmp", 0x100), (0x192, 2, "jmp", -0x10)])}
    table = loop_table(disassembly(hot))
    expect("loops", table["hot"][1], [(0x110, 0x36), (0x170, 0x15)])
    expect("crossings", table["hot"][2], {0: 2, 16: 1, 32: 1, 48: 1})
    expect("choose", choose(table), {"hot": (48, 0)})
    tie = {"tie": (0, [(0x0, 6, "add", None), (0x6, 2, "jne", 0x0)])}
    expect("choose on a tie", choose(loop_table(disassembly(tie)))["tie"][0], 0)
    prefixed = "0000000000000000 <p>:\n  10:\t90                                             \tnop\n" \
               "  11:\tf2 75 fc                                       \tbnd jne 10 <p+0x10>\n"
    expect("a prefixed branch", loop_table(prefixed)["p"][1], [(0x10, 4)])

    # Rewriting: strip, align, offset and choice, and the refusals.
    module = ("declare void @ext()\n"
              "define internal fastcc void @wf__a(ptr %x) #0 align 64 {\n  ret void\n}\n"
              "define void @\"wf.b\"() #1 align 64 {\n  ret void\n}\n"
              "define void @wf_bench_records(ptr %c) {\n  ret void\n}\n")
    host = {"wf_bench_records"}
    stripped, claims = rewrite("strip", module, host)
    expect("strip leaves no alignment", " align 64 " in stripped, False)
    expect("strip claims nothing", claims, [])
    _, claims = rewrite("keep", module, host)
    expect("keep claims", claims, [("wf__a", 64, 0, None), ("wf.b", 64, 0, None)])
    text, claims = rewrite("align=32", module, host)
    expect("align=32", text.count(" align 32 {"), 2)
    text, _ = rewrite("offset=16", module, host)
    expect("offset=16", text.count("align 64 prefix [16 x i8] zeroinitializer {"), 2)
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as handle:
        handle.write("wf__a 48 0\n")
    text, claims = rewrite(f"choice={handle.name}", module, host)
    os.unlink(handle.name)
    expect("choice", claims, [("wf__a", 64, 48, 0), ("wf.b", 64, 0, None)])
    expect("choice text", "@wf__a(ptr %x) #0 align 64 prefix [48 x i8] zeroinitializer {" in text, True)
    for label, source, hosts in (("an unaligned definition", module, set()),
                                 ("no aligned definition", "define void @f() {\n}\n", {"f"})):
        try:
            rewrite("keep", source, hosts)
            failures.append(f"rewrite accepted {label}")
        except Refused:
            pass

    # The image check on synthetic tables: each refusal it exists for.
    module_obj = symbol_table([("wf__hot", ".text", 0x0, True), ("wf__cold", ".text", 0x40, True),
                               ("wf_state", ".data", 0x0, False), ("wf_counts", ".bss", 0x0, False)])
    runtime_obj = symbol_table([("wf_rt_run", ".text", 0x0, True), ("wf_rt_queue", ".bss", 0x0, False)])
    claims_text = "wf__hot 64 0 -\nwf__cold 64 0 -\n"
    p_claims = "wf__hot 64 48 0\n"

    def image(hot_at, rt_at, data_at):
        return symbol_table([("wf__hot", ".text", hot_at, True), ("wf__cold", ".text", hot_at + 0x40, True),
                             ("wf_rt_run", ".text", rt_at, True),
                             ("wf_state", ".data", data_at, False), ("wf_counts", ".bss", data_at + 0x100, False),
                             ("wf_rt_queue", ".bss", data_at + 0x140, False)])

    good = {"U/p0": image(0x1010, 0x2010, 0x5000), "U/m16": image(0x1020, 0x2020, 0x5000),
            "U/dFR": image(0x1010, 0x2010, 0x6000),
            "FR/p0": image(0x1040, 0x2040, 0x6000), "FR/m16": image(0x1080, 0x2080, 0x6000),
            "FR/m1024": image(0x1440, 0x2440, 0x6000), "FR/r16": image(0x1040, 0x2080, 0x6000),
            "FR/d64": image(0x1040, 0x2040, 0x6040), "P/p0": image(0x1070, 0x2040, 0x6000)}

    def rows_for(images):
        rows = []
        for key in images:
            arm, placement = key.split("/")
            aligned = arm != "U"
            claims = "p.claims" if arm == "P" else "fr.claims" if aligned else "none"
            rows.append(Row("records", arm, placement, key, "module.o", claims,
                            "64@runtime.o" if aligned else "none", ["runtime.o"]))
        return rows

    texts = {"fr.claims": claims_text, "p.claims": p_claims}
    p_disassembly = disassembly({"wf__hot": (0x30, [(0x00, 4, "add", None), (0x04, 2, "jne", 0)])})
    # the P function above starts at 48 with a 6-byte loop at its start: no crossing.

    def run(images, disassembly_text=p_disassembly):
        return check_rows(rows_for(images), images.__getitem__,
                          {"module.o": module_obj, "runtime.o": runtime_obj}.__getitem__,
                          texts.__getitem__, lambda _: disassembly_text)

    expect("a good layout", run(good), [])
    cases = {
        "a function at the wrong offset": ("FR/p0", image(0x1050, 0x2040, 0x6000), "starts at 16 modulo 64"),
        "a code pad that did not move the module": ("FR/m1024", image(0x1040, 0x2040, 0x6000),
                                                    "the module moved by [0], not one of [1024]"),
        "a data pad that moved code": ("FR/d64", image(0x1080, 0x2080, 0x6040), "code moved by [64]"),
        "a data pad that moved data by less": ("FR/d64", image(0x1040, 0x2040, 0x6000),
                                               "the data moved by [0], not [64]"),
        "dFR data unequal to FR": ("U/dFR", image(0x1010, 0x2010, 0x6040), "against FR p0"),
        "a runtime pad that moved the module": ("FR/r16", image(0x1080, 0x2080, 0x6000),
                                                "the module moved by [64] under a runtime pad"),
    }
    for label, (key, table_text, wanted) in cases.items():
        broken = dict(good)
        broken[key] = table_text
        problems = run(broken)
        if not any(wanted in problem for problem in problems):
            failures.append(f"check missed {label}: {problems}")
    crossing_p = disassembly({"wf__hot": (0x30, [(0x00, 30, "add", None), (0x1e, 2, "jne", 0)])})
    problems = run(good, crossing_p)
    if not any("its choice was 48 with 0" in problem for problem in problems):
        failures.append(f"check missed a P function whose loops cross: {problems}")

    expect("data delta", data_delta(good["U/p0"], good["FR/p0"]), 0x1000)
    counts, total = line_samples(good["FR/p0"], "  wf__hot+0x3\n  wf__hot+0x45\n  wf__hot+0x7\n [unknown]\n")
    expect("line samples", (dict(counts), total),
           ({("wf__hot", 0x1040, 0x1040): 2, ("wf__hot", 0x1080, 0x1040): 1}, 4))

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        sys.exit(f"layout.py self-test: {len(failures)} failure(s)")
    print("layout.py self-test: all cases pass")


if __name__ == "__main__":
    main(sys.argv[1:])
