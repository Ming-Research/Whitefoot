#!/usr/bin/env python3
"""Writes the Whitefoot wasm interpreter (stage 3 of the match-dispatch
investigation) to the path given as the first argument.

The repetitive part, the operation enum, the opcode tables and one handler
and helper function per numeric, load and store operation, is generated here
from the tables below; interp_head.wf (the interpreter function's header and
its control, variable and call handlers) and interp_tail.wf (the loader, the
translator and the WASI driver) are written by hand. With --count the
interpreter also counts its dispatches and prints the count on standard
error when _start returns, for attributing a change to the dispatch count.

    python3 gen.py interp.wf [--count]
    whitefootc interp.wf -o wasm-interp
    ./wasm-interp coremark.wasm 0x0 0x0 0x66 2000
"""

import os
import sys

# ---- numeric operations ---------------------------------------------------
# (opcode, variant, input types, output type, compute lines)
# Inputs are bound to x (and y); compute binds z.
N = []

def add(code, name, ins, out, *lines):
    N.append((code, name, ins, out, list(lines)))

def trap_match(src, ty):
    return [f"let z = match {src} {{",
            f"  Ok(value: v) => {{",
            f"    give v;",
            f"  }}",
            f"  Err(error: problem) => {{",
            f"    return trap(code: 2_u32, pc: pc);",
            f"  }}",
            f"}}"]

# comparisons
add(0x45, "I32Eqz", ["u32"], "bool", "let z = x == 0_u32;")
for code, nm, op, sg in [(0x46, "Eq", "==", "u"), (0x47, "Ne", "!=", "u"),
                         (0x48, "LtS", "<", "i"), (0x49, "LtU", "<", "u"),
                         (0x4a, "GtS", ">", "i"), (0x4b, "GtU", ">", "u"),
                         (0x4c, "LeS", "<=", "i"), (0x4d, "LeU", "<=", "u"),
                         (0x4e, "GeS", ">=", "i"), (0x4f, "GeU", ">=", "u")]:
    add(code, "I32" + nm, [sg + "32", sg + "32"], "bool", f"let z = x {op} y;")
add(0x50, "I64Eqz", ["u64"], "bool", "let z = x == 0_u64;")
for code, nm, op, sg in [(0x51, "Eq", "==", "u"), (0x52, "Ne", "!=", "u"),
                         (0x53, "LtS", "<", "i"), (0x54, "LtU", "<", "u"),
                         (0x55, "GtS", ">", "i"), (0x56, "GtU", ">", "u"),
                         (0x57, "LeS", "<=", "i"), (0x58, "LeU", "<=", "u"),
                         (0x59, "GeS", ">=", "i"), (0x5a, "GeU", ">=", "u")]:
    add(code, "I64" + nm, [sg + "64", sg + "64"], "bool", f"let z = x {op} y;")
for base, w in [(0x5b, "32"), (0x61, "64")]:
    for i, (nm, f) in enumerate([("Eq", "feq"), ("Ne", "fne"), ("Lt", "flt"),
                                 ("Gt", "fgt"), ("Le", "fle"), ("Ge", "fge")]):
        add(base + i, f"F{w}{nm}", ["f" + w, "f" + w], "bool", f"let z = {f}(x, y);")

# integer arithmetic
for w, base, mask in [("32", 0x67, "31"), ("64", 0x79, "63")]:
    u, s = "u" + w, "i" + w
    cnt_out = "u32" if w == "32" else "u64"
    for i, (nm, f) in enumerate([("Clz", "iclz"), ("Ctz", "ictz"), ("Popcnt", "ipopcount")]):
        if w == "32":
            add(base + i, f"I{w}{nm}", [u], "u32", f"let z = {f}(x);")
        else:
            add(base + i, f"I{w}{nm}", [u], "u64", f"let c = {f}(x);", "let z = cvt::<u32, u64>(c);")
    b = base + 3
    add(b + 0, f"I{w}Add", [u, u], u, "let z = x +wrap y;")
    add(b + 1, f"I{w}Sub", [u, u], u, "let z = x -wrap y;")
    add(b + 2, f"I{w}Mul", [u, u], u, "let z = x *wrap y;")
    add(b + 3, f"I{w}DivS", [s, s], s, "let q = x /checked y;", *trap_match("q", s))
    add(b + 4, f"I{w}DivU", [u, u], u, "let q = x /checked y;", *trap_match("q", u))
    add(b + 5, f"I{w}RemS", [s, s], s,
        f"let minus = y == -1_{s};",
        f"let z = if minus {{",
        f"  give 0_{s};",
        f"}} else {{",
        f"  let q = x %checked y;",
        f"  match q {{",
        f"    Ok(value: v) => {{",
        f"      give v;",
        f"    }}",
        f"    Err(error: problem) => {{",
        f"      return trap(code: 2_u32, pc: pc);",
        f"    }}",
        f"  }}",
        f"}}")
    add(b + 6, f"I{w}RemU", [u, u], u, "let q = x %checked y;", *trap_match("q", u))
    add(b + 7, f"I{w}And", [u, u], u, "let z = iand(x, y);")
    add(b + 8, f"I{w}Or", [u, u], u, "let z = ior(x, y);")
    add(b + 9, f"I{w}Xor", [u, u], u, "let z = ixor(x, y);")
    cnt = (f"let m = iand(y, {mask}_{u});" if w == "32" else
           f"let m64 = iand(y, {mask}_{u});")
    cnt2 = [] if w == "32" else ["let m = cvt.wrap::<u64, u32>(m64);"]
    add(b + 10, f"I{w}Shl", [u, u], u, cnt, *cnt2, "let z = ishl.wrap(x, m);")
    add(b + 11, f"I{w}ShrS", [s, u], s, cnt, *cnt2, "let z = ishr.wrap(x, m);")
    add(b + 12, f"I{w}ShrU", [u, u], u, cnt, *cnt2, "let z = ishr.wrap(x, m);")
    add(b + 13, f"I{w}Rotl", [u, u], u, cnt, *cnt2, "let z = irotl(x, m);")
    add(b + 14, f"I{w}Rotr", [u, u], u, cnt, *cnt2, "let z = irotr(x, m);")

# float arithmetic
for w, base in [("32", 0x8b), ("64", 0x99)]:
    f = "f" + w
    for i, (nm, op) in enumerate([("Abs", "fabs"), ("Neg", "fneg"), ("Ceil", "fceil"),
                                  ("Floor", "ffloor"), ("Trunc", "ftrunc"),
                                  ("Nearest", "froundeven"), ("Sqrt", "fsqrt.strict")]):
        add(base + i, f"F{w}{nm}", [f], f, f"let z = {op}(x);")
    b = base + 7
    for i, (nm, op) in enumerate([("Add", "fadd.strict"), ("Sub", "fsub.strict"),
                                  ("Mul", "fmul.strict"), ("Div", "fdiv.strict"),
                                  ("Min", "fmin"), ("Max", "fmax"),
                                  ("Copysign", "fcopysign")]):
        add(b + i, f"F{w}{nm}", [f, f], f, f"let z = {op}(x, y);")

# conversions
add(0xa7, "I32WrapI64", ["u64"], "u32", "let z = cvt.wrap::<u64, u32>(x);")
for code, nm, src, dst in [(0xa8, "I32TruncF32S", "f32", "i32"), (0xa9, "I32TruncF32U", "f32", "u32"),
                           (0xaa, "I32TruncF64S", "f64", "i32"), (0xab, "I32TruncF64U", "f64", "u32"),
                           (0xae, "I64TruncF32S", "f32", "i64"), (0xaf, "I64TruncF32U", "f32", "u64"),
                           (0xb0, "I64TruncF64S", "f64", "i64"), (0xb1, "I64TruncF64U", "f64", "u64")]:
    add(code, nm, [src], dst, "let t = ftrunc(x);", f"let c = cvt.checked::<{src}, {dst}>(t);",
        *trap_match("c", dst))
add(0xac, "I64ExtendI32S", ["i32"], "i64", "let z = cvt::<i32, i64>(x);")
for code, nm, src, dst, how in [(0xb2, "F32ConvertI32S", "i32", "f32", "cvt.nearest"),
                                (0xb3, "F32ConvertI32U", "u32", "f32", "cvt.nearest"),
                                (0xb4, "F32ConvertI64S", "i64", "f32", "cvt.nearest"),
                                (0xb5, "F32ConvertI64U", "u64", "f32", "cvt.nearest"),
                                (0xb6, "F32DemoteF64", "f64", "f32", "cvt.nearest"),
                                (0xb7, "F64ConvertI32S", "i32", "f64", "cvt"),
                                (0xb8, "F64ConvertI32U", "u32", "f64", "cvt"),
                                (0xb9, "F64ConvertI64S", "i64", "f64", "cvt.nearest"),
                                (0xba, "F64ConvertI64U", "u64", "f64", "cvt.nearest"),
                                (0xbb, "F64PromoteF32", "f32", "f64", "cvt")]:
    add(code, nm, [src], dst, f"let z = {how}::<{src}, {dst}>(x);")
add(0xc0, "I32Extend8S", ["u32"], "i32", "let b = cvt.wrap::<u32, u8>(x);",
    "let s = reinterpret::<u8, i8>(b);", "let z = cvt::<i8, i32>(s);")
add(0xc1, "I32Extend16S", ["u32"], "i32", "let b = cvt.wrap::<u32, u16>(x);",
    "let s = reinterpret::<u16, i16>(b);", "let z = cvt::<i16, i32>(s);")
add(0xc2, "I64Extend8S", ["u64"], "i64", "let b = cvt.wrap::<u64, u8>(x);",
    "let s = reinterpret::<u8, i8>(b);", "let z = cvt::<i8, i64>(s);")
add(0xc3, "I64Extend16S", ["u64"], "i64", "let b = cvt.wrap::<u64, u16>(x);",
    "let s = reinterpret::<u16, i16>(b);", "let z = cvt::<i16, i64>(s);")
add(0xc4, "I64Extend32S", ["u64"], "i64", "let b = cvt.wrap::<u64, u32>(x);",
    "let s = reinterpret::<u32, i32>(b);", "let z = cvt::<i32, i64>(s);")

# saturating truncation (0xfc 0..7)
FC = []
LIMITS = {"i32": ("-2147483648_i32", "2147483647_i32"), "u32": ("0_u32", "4294967295_u32"),
          "i64": ("-9223372036854775808_i64", "9223372036854775807_i64"),
          "u64": ("0_u64", "18446744073709551615_u64")}
for sub, nm, src, dst in [(0, "I32TruncSatF32S", "f32", "i32"), (1, "I32TruncSatF32U", "f32", "u32"),
                          (2, "I32TruncSatF64S", "f64", "i32"), (3, "I32TruncSatF64U", "f64", "u32"),
                          (4, "I64TruncSatF32S", "f32", "i64"), (5, "I64TruncSatF32U", "f32", "u64"),
                          (6, "I64TruncSatF64S", "f64", "i64"), (7, "I64TruncSatF64U", "f64", "u64")]:
    lo, hi = LIMITS[dst]
    FC.append((sub, nm, [src], dst, [
        "let t = ftrunc(x);",
        f"let c = cvt.checked::<{src}, {dst}>(t);",
        "let z = match c {",
        "  Ok(value: v) => {",
        "    give v;",
        "  }",
        "  Err(error: problem) => {",
        "    let nan = fne(x, x);",
        f"    let zero = 0.0_{src};",
        "    let neg = flt(x, zero);",
        "    if nan {",
        f"      give 0_{dst};",
        "    } else if neg {",
        f"      give {lo};",
        "    } else {",
        f"      give {hi};",
        "    }",
        "  }",
        "}"]))

# ---- memory operations ----------------------------------------------------
# (opcode, variant, bytes, signed, result type)
LOADS = [(0x28, "I32Load", 4, False, "u32"), (0x29, "I64Load", 8, False, "u64"),
         (0x2a, "F32Load", 4, False, "u32"), (0x2b, "F64Load", 8, False, "u64"),
         (0x2c, "I32Load8S", 1, True, "u32"), (0x2d, "I32Load8U", 1, False, "u32"),
         (0x2e, "I32Load16S", 2, True, "u32"), (0x2f, "I32Load16U", 2, False, "u32"),
         (0x30, "I64Load8S", 1, True, "u64"), (0x31, "I64Load8U", 1, False, "u64"),
         (0x32, "I64Load16S", 2, True, "u64"), (0x33, "I64Load16U", 2, False, "u64"),
         (0x34, "I64Load32S", 4, True, "u64"), (0x35, "I64Load32U", 4, False, "u64")]
STORES = [(0x36, "I32Store", 4), (0x37, "I64Store", 8), (0x38, "F32Store", 4),
          (0x39, "F64Store", 8), (0x3a, "I32Store8", 1), (0x3b, "I32Store16", 2),
          (0x3c, "I64Store8", 1), (0x3d, "I64Store16", 2), (0x3e, "I64Store32", 4)]

ARGS = "code: code, funcs: funcs, brtab: brtab, table: table, stack: stack, mem: mem, globals: globals"

def tail(pc, sp, fp, ind):
    p = " " * ind
    return [f"{p}return musttail run({ARGS}, pc: {pc}, sp: {sp}, fp: {fp});"]

def advance(sp, fp, ind):
    p = " " * ind
    return [f"{p}let next = pc + 1_u64;", f"{p}if next < n {{"] + tail("next", sp, fp, ind + 2) + [f"{p}}}"]

def decode(var, word, ty):
    if ty == "u32":
        return [f"let {var} = cvt.wrap::<u64, u32>({word});"]
    if ty == "i32":
        return [f"let {var}u = cvt.wrap::<u64, u32>({word});", f"let {var} = reinterpret::<u32, i32>({var}u);"]
    if ty == "u64":
        return [f"let {var} = {word};"]
    if ty == "i64":
        return [f"let {var} = reinterpret::<u64, i64>({word});"]
    if ty == "f32":
        return [f"let {var}u = cvt.wrap::<u64, u32>({word});", f"let {var} = reinterpret::<u32, f32>({var}u);"]
    if ty == "f64":
        return [f"let {var} = reinterpret::<u64, f64>({word});"]
    raise ValueError(ty)

def encode(ty):
    if ty == "u32":
        return ["let zw = cvt::<u32, u64>(z);"]
    if ty == "i32":
        return ["let zu = reinterpret::<i32, u32>(z);", "let zw = cvt::<u32, u64>(zu);"]
    if ty == "u64":
        return ["let zw = z;"]
    if ty == "i64":
        return ["let zw = reinterpret::<i64, u64>(z);"]
    if ty == "f32":
        return ["let zu = reinterpret::<f32, u32>(z);", "let zw = cvt::<u32, u64>(zu);"]
    if ty == "f64":
        return ["let zw = reinterpret::<f64, u64>(z);"]
    if ty == "bool":
        return ["let zw = if z {", "  give 1_u64;", "} else {", "  give 0_u64;", "}"]
    raise ValueError(ty)


HELPERS = []

def snake(name):
    out = ""
    for i, ch in enumerate(name):
        if ch.isupper() and i > 0 and not name[i-1].isupper():
            out += "_"
        elif ch.isupper() and i > 0 and i + 1 < len(name) and name[i+1].islower() and name[i-1].isupper():
            out += "_"
        out += ch.lower()
    return "op_" + out

def traps(lines):
    return any("trap(" in l for l in lines)

def numeric_arm(name, ins, out, lines):
    fn = snake(name)
    trapping = traps(lines)
    body = [l.replace("return trap(code: 2_u32, pc: pc);", "return False();") for l in lines]
    h = []
    res = "ok: Bool" if trapping else "r: unit"
    if len(ins) == 1:
        h.append(f"fn {fn}(stack: &Box<Array<u64>>, at: u64) -> {res} writes(stack) contract {{")
        h.append("  requires at < stack^.inner.len;")
        h.append("  ensures stack^.inner.len == entry(stack)^.inner.len;")
        h.append("} {")
        b = ["let xw = stack^.inner[at];"] + decode("x", "xw", ins[0]) + body + encode(out) + ["set stack^.inner[at] = zw;"]
    else:
        h.append(f"fn {fn}(stack: &Box<Array<u64>>, at: u64) -> {res} writes(stack) contract {{")
        h.append("  requires at >= 1_u64;")
        h.append("  requires at < stack^.inner.len;")
        h.append("  ensures stack^.inner.len == entry(stack)^.inner.len;")
        h.append("} {")
        b = ["let below = at - 1_u64;", "let xw = stack^.inner[below];", "let yw = stack^.inner[at];"] + decode("x", "xw", ins[0]) + decode("y", "yw", ins[1]) + body + encode(out) + ["set stack^.inner[below] = zw;"]
    b.append("return True();" if trapping else "return unit;")
    h += ["  " + l for l in b]
    h.append("}")
    h.append("")
    HELPERS.extend(h)
    o = [f"    {name}() => {{"]
    need = len(ins)
    o.append(f"      if sp >= {need}_u64 {{")
    o.append("        let s1 = sp - 1_u64;")
    ns = "sp" if need == 1 else "s1"
    if trapping:
        o.append(f"        let ok = {fn}(stack: stack, at: s1);")
        o.append("        if ok {")
        o += advance(ns, "fp", 10)
        o.append("        }")
        o.append("        return trap(code: 2_u32, pc: pc);")
    else:
        o.append(f"        {fn}(stack: stack, at: s1);")
        o += advance(ns, "fp", 8)
    o.append("      }")
    o.append("      return trap(code: 1_u32, pc: pc);")
    o.append("    }")
    return o

def load_arm(name, nbytes, signed, out):
    fn = snake(name)
    h = [f"fn {fn}(stack: &Box<Array<u64>>, mem: &Box<Array<u8>>, at: u64, offset: u32) -> ok: Bool reads(mem), writes(stack) contract {{",
         "  requires at < stack^.inner.len;", "  ensures stack^.inner.len == entry(stack)^.inner.len;", "} {"]
    b = ["let aw = stack^.inner[at];",
         "let a32 = cvt.wrap::<u64, u32>(aw);", "let a64 = cvt::<u32, u64>(a32);",
         "let o64 = cvt::<u32, u64>(offset);", "let ea = a64 + o64;",
         f"let lim = ea + {nbytes}_u64;", "if lim <= mem^.inner.len {"]
    inner = []
    for i in range(nbytes):
        if i == 0:
            inner.append("let b0 = mem^.inner[ea];")
        else:
            inner.append(f"let e{i} = ea + {i}_u64;")
            inner.append(f"let b{i} = mem^.inner[e{i}];")
    acc = "u64" if nbytes == 8 else ("u32" if nbytes == 4 else ("u16" if nbytes == 2 else "u8"))
    if nbytes == 1:
        inner.append("let v = b0;")
    else:
        inner.append(f"let v0 = cvt::<u8, {acc}>(b0);")
        prev = "v0"
        for i in range(1, nbytes):
            inner.append(f"let w{i} = cvt::<u8, {acc}>(b{i});")
            inner.append(f"let t{i} = ishl.wrap(w{i}, {8 * i}_u32);")
            inner.append(f"let v{i} = ior({prev}, t{i});")
            prev = f"v{i}"
        inner.append(f"let v = {prev};")
    if signed:
        st = {"u8": "i8", "u16": "i16", "u32": "i32"}[acc]
        dst = "i32" if out == "u32" else "i64"
        inner.append(f"let sv = reinterpret::<{acc}, {st}>(v);")
        inner.append(f"let wv = cvt::<{st}, {dst}>(sv);")
        if dst == "i32":
            inner.append("let uv = reinterpret::<i32, u32>(wv);")
            inner.append("let zw = cvt::<u32, u64>(uv);")
        else:
            inner.append("let zw = reinterpret::<i64, u64>(wv);")
    else:
        inner.append("let zw = v;" if acc == "u64" else f"let zw = cvt::<{acc}, u64>(v);")
    inner.append("set stack^.inner[at] = zw;")
    inner.append("return True();")
    b += ["  " + l for l in inner] + ["}", "return False();"]
    h += ["  " + l for l in b] + ["}", ""]
    HELPERS.extend(h)
    o = [f"    {name}(o: ov) => {{", "      if sp >= 1_u64 {", "        let s1 = sp - 1_u64;",
         f"        let ok = {fn}(stack: stack, mem: mem, at: s1, offset: ov^);", "        if ok {"]
    o += advance("sp", "fp", 10)
    o += ["        }", "        return trap(code: 3_u32, pc: pc);", "      }",
          "      return trap(code: 1_u32, pc: pc);", "    }"]
    return o

def store_arm(name, nbytes):
    fn = snake(name)
    h = [f"fn {fn}(stack: &Box<Array<u64>>, mem: &Box<Array<u8>>, at: u64, offset: u32) -> ok: Bool reads(stack), writes(mem) contract {{",
         "  requires at >= 1_u64;", "  requires at < stack^.inner.len;", "  ensures mem^.inner.len == entry(mem)^.inner.len;", "} {"]
    b = ["let below = at - 1_u64;", "let vw = stack^.inner[at];", "let aw = stack^.inner[below];",
         "let a32 = cvt.wrap::<u64, u32>(aw);", "let a64 = cvt::<u32, u64>(a32);",
         "let o64 = cvt::<u32, u64>(offset);", "let ea = a64 + o64;",
         f"let lim = ea + {nbytes}_u64;", "if lim <= mem^.inner.len {"]
    inner = []
    for i in range(nbytes):
        if i == 0:
            inner.append("let b0 = cvt.wrap::<u64, u8>(vw);")
            inner.append("set mem^.inner[ea] = b0;")
        else:
            inner.append(f"let r{i} = ishr.wrap(vw, {8 * i}_u32);")
            inner.append(f"let b{i} = cvt.wrap::<u64, u8>(r{i});")
            inner.append(f"let e{i} = ea + {i}_u64;")
            inner.append(f"set mem^.inner[e{i}] = b{i};")
    inner.append("return True();")
    b += ["  " + l for l in inner] + ["}", "return False();"]
    h += ["  " + l for l in b] + ["}", ""]
    HELPERS.extend(h)
    o = [f"    {name}(o: ov) => {{", "      if sp >= 2_u64 {", "        let s1 = sp - 1_u64;",
         "        let s2 = sp - 2_u64;",
         f"        let ok = {fn}(stack: stack, mem: mem, at: s1, offset: ov^);", "        if ok {"]
    o += advance("s2", "fp", 10)
    o += ["        }", "        return trap(code: 3_u32, pc: pc);", "      }",
          "      return trap(code: 1_u32, pc: pc);", "    }"]
    return o


def count_dispatches(program):
    """The --count variant: a counter cell passed to the interpreter function
    and incremented at every dispatch, printed when _start returns."""
    sig = ("globals: &Box<Slots<u64>>, pc: u64, sp: u64, fp: u64) -> r: Outcome reads(code), reads(funcs), "
           "reads(brtab), reads(table), writes(stack), writes(mem), writes(globals) contract {")
    assert sig in program
    program = program.replace(sig, sig.replace("pc: u64,", "counter: &Box<Array<u64>>, pc: u64,")
                              .replace("writes(globals) contract", "writes(globals), writes(counter) contract"))
    program = program.replace("globals: globals, pc:", "globals: globals, counter: counter, pc:")
    program = program.replace("  let n = code^.inner.len;\n  match code^.inner[pc] {",
                              "  let n = code^.inner.len;\n  if 0_u64 < counter^.inner.len {\n"
                              "    let seen = counter^.inner[0_u64];\n"
                              "    set counter^.inner[0_u64] = seen +wrap 1_u64;\n  }\n  match code^.inner[pc] {")
    program = program.replace("globals: &globals, pc: pc, sp: sp, fp: fp);",
                              "globals: &globals, counter: &counter, pc: pc, sp: sp, fp: fp);")
    program = program.replace("  let max_pages = info.max_pages;\n  loop @drive {",
                              "  let max_pages = info.max_pages;\n"
                              "  let counter = box_array_filled::<u64>(count: 1_u64, value: 0_u64);\n  loop @drive {")
    done = "      Done(sp: final_sp) => {\n        return 0_u8;\n      }"
    assert done in program
    program = program.replace(done, """      Done(sp: final_sp) => {
        if 0_u64 < counter.inner.len {
          let total = counter.inner[0_u64];
          let digits = box_array_filled::<u8>(count: 21_u64, value: 48_u8);
          let rest = total;
          for (d in 0_u64..20_u64) {
            let place = 19_u64 - d;
            let digit = rest % 10_u64;
            let glyph = cvt.wrap::<u64, u8>(digit);
            let shown = glyph +wrap 48_u8;
            if place < digits.inner.len {
              set digits.inner[place] = shown;
            }
            set rest = rest / 10_u64;
          }
          if 20_u64 < digits.inner.len {
            set digits.inner[20_u64] = 10_u8;
          }
          let none = None<Instant>();
          let dl = digits.inner.len;
          match write_once(factory: files, output: err, source: &digits.inner[0_u64..dl], start: 0_u64, end: dl, deadline: none) {
            Ok(value: wrote) => {
            }
            Err(error: problem) => {
            }
          }
        }
        return 0_u8;
      }""")
    return program

# ---- emit -----------------------------------------------------------------
out = []
w = out.append

w("enum Op {")
for v in ["Unreachable()", "Jump(t: u32)", "Br(t: u32, h: u32, k: u32)", "BrIf(t: u32, h: u32, k: u32)",
          "BrIfJ(t: u32)", "BrUnless(t: u32)", "BrTable(start: u32, count: u32)",
          "Return(k: u32, l: u32)", "Call(f: u32)", "CallIndirect(canon: u32)", "Host(id: u32)",
          "Drop()", "Select()", "LocalGet(i: u32)", "LocalSet(i: u32)", "LocalTee(i: u32)",
          "GlobalGet(i: u32)", "GlobalSet(i: u32)", "MemorySize()", "MemoryGrow()",
          "MemoryCopy()", "MemoryFill()", "I32Const(v: u32)", "I64Const(v: u64)"]:
    w(f"  {v};")
for code, name, *_ in LOADS:
    w(f"  {name}(o: u32);")
for code, name, _ in STORES:
    w(f"  {name}(o: u32);")
for code, name, *_ in N:
    w(f"  {name}();")
for sub, name, *_ in FC:
    w(f"  {name}();")
w("}")
w("")

w("fn numeric_op(code: u8) -> found: Option<Op> pure {")
w('  doc "The operation of a one-byte numeric opcode, or None for an opcode that is not one.";')
for code, name, *_ in N:
    w(f"  if code == {code}_u8 {{")
    w(f"    let o = Op::{name}();")
    w(f"    return Some<Op>(value: o);")
    w("  }")
w("  return None<Op>();")
w("}")
w("")
w("fn numeric_inputs(code: u8) -> count: u64 pure {")
w('  doc "How many operands a one-byte numeric opcode pops; it pushes one result.";')
for code, name, ins, *_ in N:
    if len(ins) == 2:
        w(f"  if code == {code}_u8 {{")
        w("    return 2_u64;")
        w("  }")
w("  return 1_u64;")
w("}")
w("")
w("fn saturating_op(sub: u64) -> found: Option<Op> pure {")
w('  doc "The operation of a saturating truncation, 0xfc 0 through 7.";')
for sub, name, *_ in FC:
    w(f"  if sub == {sub}_u64 {{")
    w(f"    let o = Op::{name}();")
    w(f"    return Some<Op>(value: o);")
    w("  }")
w("  return None<Op>();")
w("}")
w("")
w("fn load_op(code: u8, offset: u32) -> found: Option<Op> pure {")
w('  doc "The operation of a load or store opcode with its offset.";')
for code, name, *_ in LOADS:
    w(f"  if code == {code}_u8 {{")
    w(f"    let o = Op::{name}(o: offset);")
    w(f"    return Some<Op>(value: o);")
    w("  }")
for code, name, _ in STORES:
    w(f"  if code == {code}_u8 {{")
    w(f"    let o = Op::{name}(o: offset);")
    w(f"    return Some<Op>(value: o);")
    w("  }")
w("  return None<Op>();")
w("}")
w("")

OPS = "\n".join(out).strip("\n")

# The interpreter's arms go to a second file.
arms = []
for code, name, nbytes, signed, outty in LOADS:
    arms += load_arm(name, nbytes, signed, outty)
for code, name, nbytes in STORES:
    arms += store_arm(name, nbytes)
for code, name, ins, outty, lines in N:
    arms += numeric_arm(name, ins, outty, lines)
for sub, name, ins, outty, lines in FC:
    arms += numeric_arm(name, ins, outty, lines)
here = os.path.dirname(os.path.abspath(__file__))
head = open(os.path.join(here, "interp_head.wf")).read()
tail = open(os.path.join(here, "interp_tail.wf")).read().strip("\n")
aliases_end = head.index("\n\n") + 2
program = (head[:aliases_end] + OPS + "\n\n" + "\n".join(HELPERS).strip("\n") + "\n\n"
           + head[aliases_end:].rstrip("\n") + "\n" + "\n".join(arms).strip("\n") + "\n  }\n}\n\n" + tail + "\n")
if "--count" in sys.argv:
    program = count_dispatches(program)
open(sys.argv[1], "w").write(program)
