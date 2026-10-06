"""Argument registers per calling convention and target, for
compiler/match-dispatch-lowering's register budget.

    CLANG=clang-19 python3 regprobe.py

The first table is the largest n for which a function of n `i64` (or n
`double`) parameters reads no argument from the stack. The second is the
largest n for which a dispatch part's own shape still compiles with no
argument on the stack: n integer parameters, the next handler loaded from a
global table by a tag the first parameter points at, every parameter
recomputed, and a guaranteed tail call (`musttail`) through that loaded
address passing all n on. The second can be smaller, because the indirect
call's target and the table's address need registers of their own while
every argument is live.
"""

import os
import re
import subprocess

CLANG = os.environ.get("CLANG", "clang")
TARGETS = [
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
]
STACK = re.compile(r"\[sp|\[x29|\(%rsp\)|\(%rbp\)|%rsp\)")


def compile_ir(target, ir):
    """Returns the assembly, or None when the compiler refuses the module."""
    with open("rp.ll", "w") as module:
        module.write(ir)
    result = subprocess.run(
        [CLANG, "-target", target, "-O2", "-S", "-o", "-", "-x", "ir", "rp.ll"],
        capture_output=True,
        text=True,
    )
    return result.stdout if result.returncode == 0 else None


def spills(cc, target, n, ty):
    params = ", ".join(f"{ty} %a{i}" for i in range(n))
    op = "fadd" if ty == "double" else "add"
    body = []
    prev = "%a0"
    for i in range(1, n):
        body.append(f"  %s{i} = {op} {ty} {prev}, %a{i}")
        prev = f"%s{i}"
    ir = f"define {cc} {ty} @f({params}) {{\n" + "\n".join(body) + f"\n  ret {ty} {prev}\n}}\n"
    asm = compile_ir(target, ir)
    return asm is None or bool(STACK.search(asm))


def indirect_fits(cc, target, n):
    """Whether a part with n integer parameters, the first the code
    pointer, compiles and passes all of them on, each recomputed so that
    every one is live at the call, through a table-loaded guaranteed tail
    call without touching the stack."""
    params = ", ".join(["ptr %code"] + [f"i64 %a{i}" for i in range(1, n)])
    body = "".join(f"  %b{i} = add i64 %a{i}, %index\n" for i in range(1, n))
    args = ", ".join(["ptr %next_code"] + [f"i64 %b{i}" for i in range(1, n)])
    ir = (
        # An external table, so the optimizer cannot resolve the target and
        # turn the transfer into a loop.
        "@table = external global [256 x ptr]\n\n"
        f"define {cc} i64 @part({params}) {{\n"
        "  %tag = load i8, ptr %code\n"
        "  %index = zext i8 %tag to i64\n"
        "  %slot = getelementptr [256 x ptr], ptr @table, i64 0, i64 %index\n"
        "  %next = load ptr, ptr %slot\n"
        "  %next_code = getelementptr i8, ptr %code, i64 1\n"
        + body
        + f"  %r = musttail call {cc} i64 %next({args})\n"
        "  ret i64 %r\n"
        "}\n"
    )
    asm = compile_ir(target, ir)
    return asm is not None and not STACK.search(asm)


def largest(fits):
    last = 0
    for n in range(1, 40):
        if not fits(n):
            break
        last = n
    return last


print(subprocess.run([CLANG, "--version"], capture_output=True, text=True).stdout.splitlines()[0])
print("direct, no argument on the stack:")
for cc in ["preserve_nonecc", ""]:
    for target in TARGETS:
        res = []
        for ty in ["i64", "double"]:
            res.append(f"{ty}={largest(lambda n: not spills(cc, target, n, ty))}")
        print(f"  {cc or 'C':16} {target:28} " + " ".join(res))
print("table-loaded guaranteed tail call, integer arguments:")
for cc in ["preserve_nonecc", ""]:
    for target in TARGETS:
        print(f"  {cc or 'C':16} {target:28} i64={largest(lambda n: indirect_fits(cc, target, n))}")
