#!/usr/bin/env python3
"""Writes the Whitefoot wasm interpreter (stage 3 of the match-dispatch
investigation) to the path given as the first argument.

The repetitive part, the operation enum, the opcode tables and one handler
and helper function per numeric, load and store operation, is generated here
from the tables below; interp_head.wf (the interpreter function's header and
its control, variable and call handlers) and interp_tail.wf (the loader, the
translator and the WASI driver) are written by hand. With --count the
interpreter also counts its dispatches and prints the count on standard
error when _start returns, for attributing a change to the dispatch count;
with --profile it counts each operation kind, in the order --names prints.
With --inline each handler's body is written into its arm, except a body
that delivers a value from a match, which stays a helper call. The forms
that pass a value to a later operation in the interpreter function's acc
parameter (the ACC table) are always written into their arms.

    python3 gen.py interp.wf [--count | --profile] [--inline]
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
# The i32 loads and stores that also take their address as the sum of two
# slots, an i32.add just emitted for the address folded into them.
INDEXED = ["I32Load", "I32Load8S", "I32Load8U", "I32Load16S", "I32Load16U", "I32Store", "I32Store8", "I32Store16"]
STORES = [(0x36, "I32Store", 4), (0x37, "I64Store", 8), (0x38, "F32Store", 4),
          (0x39, "F64Store", 8), (0x3a, "I32Store8", 1), (0x3b, "I32Store16", 2),
          (0x3c, "I64Store8", 1), (0x3d, "I64Store16", 2), (0x3e, "I64Store32", 4)]


# Compare-and-branch operations: a br_if on an i32 comparison computed just
# before it, or an if on its negation (opcode, name, operator, signedness).
FUSED = [(0x46, "Eq", "==", "u"), (0x47, "Ne", "!=", "u"), (0x48, "LtS", "<", "i"),
         (0x49, "LtU", "<", "u"), (0x4a, "GtS", ">", "i"), (0x4b, "GtU", ">", "u"),
         (0x4c, "LeS", "<=", "i"), (0x4d, "LeU", "<=", "u"), (0x4e, "GeS", ">=", "i"),
         (0x4f, "GeU", ">=", "u")]
NEGATE = {0x46: 0x47, 0x47: 0x46, 0x48: 0x4e, 0x4e: 0x48, 0x49: 0x4f, 0x4f: 0x49,
          0x4a: 0x4c, 0x4c: 0x4a, 0x4b: 0x4d, 0x4d: 0x4b, 0x45: 0xa8, 0xa8: 0x45}

ARGS = "code: code, funcs: funcs, brtab: brtab, table: table, consts: consts, stack: stack, mem: mem, globals: globals"

# The frame contract every handler and helper relies on: the 65536 slots
# from fp lie inside the stack, so an operand slot (a u16) needs no check.
FRAME = "  requires fp + 65536_u64 <= stack^.inner.len;"
KEEPS = "  ensures stack^.inner.len == entry(stack)^.inner.len;"


def tail(pc, ind, acc="acc"):
    """The transfer to the next operation: set the loop variables that
    change and continue the interpreter loop."""
    p = " " * ind
    sets = [f"{p}set pc = {pc};"] if pc != "pc" else []
    sets += [f"{p}set acc = {acc};"] if acc != "acc" else []
    return sets + [f"{p}continue;"]


def advance(ind, acc="acc"):
    p = " " * ind
    return [f"{p}let next = pc + 1_u64;", f"{p}if next < n {{"] + tail("next", ind + 2, acc) + [f"{p}}}"]


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
        if ch.isupper() and i > 0 and not name[i - 1].isupper():
            out += "_"
        elif ch.isupper() and i > 0 and i + 1 < len(name) and name[i + 1].islower() and name[i - 1].isupper():
            out += "_"
        out += ch.lower()
    return "op_" + out


def slot(var, field):
    return [f"let {var}i = cvt::<u16, u64>({field});", f"let {var}t = fp + {var}i;"]


def numeric_arm(name, ins, out, lines):
    fn = snake(name)
    trapping = any("trap(" in l for l in lines)
    body = [l.replace("return trap(code: 2_u32, pc: pc);", "return False();") for l in lines]
    res = "ok: Bool" if trapping else "r: unit"
    params = "d: u16, a: u16" + (", b: u16" if len(ins) == 2 else "")
    h = [f"fn {fn}(stack: &Box<Array<u64>>, fp: u64, {params}) -> {res} writes(stack.inner) contract {{", FRAME, KEEPS, "} {"]
    b = slot("a", "a") + ["let xw = stack^.inner[at];"] + decode("x", "xw", ins[0])
    if len(ins) == 2:
        b += slot("b", "b") + ["let yw = stack^.inner[bt];"] + decode("y", "yw", ins[1])
    b += body + encode(out) + slot("d", "d") + ["set stack^.inner[dt] = zw;"]
    b.append("return True();" if trapping else "return unit;")
    HELPERS.extend(h + ["  " + l for l in b] + ["}", ""])
    fields = "d: dv, a: av" + (", b: bv" if len(ins) == 2 else "")
    args = "stack: stack, fp: fp, d: dv^, a: av^" + (", b: bv^" if len(ins) == 2 else "")
    o = [f"    {name}({fields}) => {{"]
    if trapping:
        o.append(f"      let ok = {fn}({args});")
        o.append("      if ok {")
        o += advance(8)
        o.append("      }")
        o.append("      return trap(code: 2_u32, pc: pc);")
    else:
        o.append(f"      {fn}({args});")
        o += advance(6)
        o.append("      return trap(code: 1_u32, pc: pc);")
    o.append("    }")
    return o


def memory_address(nbytes):
    return ["let a32 = cvt.wrap::<u64, u32>(aw);", "let a64 = cvt::<u32, u64>(a32);",
            "let o64 = cvt::<u32, u64>(offset);", "let ea = a64 + o64;",
            f"let lim = ea + {nbytes}_u64;"]


def indexed_address(nbytes):
    return ["let bw = stack^.inner[bt];", "let x32 = cvt.wrap::<u64, u32>(aw);", "let y32 = cvt.wrap::<u64, u32>(bw);",
            "let a32 = x32 +wrap y32;"] + memory_address(nbytes)[1:]


def load_arm(name, nbytes, signed, out, indexed=False):
    fn = snake(name)
    second = "b: u16, " if indexed else ""
    h = [f"fn {fn}(stack: &Box<Array<u64>>, mem: &Box<Array<u8>>, fp: u64, d: u16, a: u16, {second}offset: u32) -> ok: Bool reads(mem), writes(stack.inner) contract {{",
         FRAME, KEEPS, "} {"]
    address = slot("a", "a") + (slot("b", "b") if indexed else []) + ["let aw = stack^.inner[at];"]
    address += indexed_address(nbytes) if indexed else memory_address(nbytes)
    b = address + ["if lim <= mem^.inner.len {"]
    inner = load_value(nbytes, signed, out)
    inner += slot("d", "d") + ["set stack^.inner[dt] = zw;", "return True();"]
    b += ["  " + l for l in inner] + ["}", "return False();"]
    HELPERS.extend(h + ["  " + l for l in b] + ["}", ""])
    pat, arg = ("b: bv, ", "b: bv^, ") if indexed else ("", "")
    o = [f"    {name}(d: dv, a: av, {pat}o: ov) => {{",
         f"      let ok = {fn}(stack: stack, mem: mem, fp: fp, d: dv^, a: av^, {arg}offset: ov^);", "      if ok {"]
    o += advance(8)
    o += ["      }", "      return trap(code: 3_u32, pc: pc);", "    }"]
    return o


def load_value(nbytes, signed, out):
    """The lines of a load reading nbytes from ea and binding the slot word zw."""
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
    return inner


def store_arm(name, nbytes, indexed=False):
    fn = snake(name)
    second = "b: u16, " if indexed else ""
    h = [f"fn {fn}(stack: &Box<Array<u64>>, mem: &Box<Array<u8>>, fp: u64, a: u16, {second}v: u16, offset: u32) -> ok: Bool reads(stack), writes(mem.inner) contract {{",
         FRAME, "  ensures mem^.inner.len == entry(mem)^.inner.len;", "} {"]
    address = slot("a", "a") + (slot("b", "b") if indexed else []) + slot("v", "v") + ["let aw = stack^.inner[at];", "let vw = stack^.inner[vt];"]
    address += indexed_address(nbytes) if indexed else memory_address(nbytes)
    b = address + ["if lim <= mem^.inner.len {"]
    inner = store_value(nbytes)
    inner.append("return True();")
    b += ["  " + l for l in inner] + ["}", "return False();"]
    HELPERS.extend(h + ["  " + l for l in b] + ["}", ""])
    pat, arg = ("b: bv, ", "b: bv^, ") if indexed else ("", "")
    o = [f"    {name}(a: av, {pat}v: vv, o: ov) => {{",
         f"      let ok = {fn}(stack: stack, mem: mem, fp: fp, a: av^, {arg}v: vv^, offset: ov^);", "      if ok {"]
    o += advance(8)
    o += ["      }", "      return trap(code: 3_u32, pc: pc);", "    }"]
    return o



def store_value(nbytes):
    """The lines of a store writing the low nbytes of vw at ea."""
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
    return inner


def fused_arm(name, op, sign):
    fn = snake(name)
    ty = sign + "32"
    h = [f"fn {fn}(stack: &Box<Array<u64>>, fp: u64, a: u16, b: u16) -> taken: Bool reads(stack) contract {{", FRAME, "} {"]
    b = slot("a", "a") + ["let xw = stack^.inner[at];"] + decode("x", "xw", ty)
    b += slot("b", "b") + ["let yw = stack^.inner[bt];"] + decode("y", "yw", ty)
    b += [f"let z = x {op} y;", "return z;"]
    HELPERS.extend(h + ["  " + l for l in b] + ["}", ""])
    o = [f"    {name}(a: av, b: bv, t: tv) => {{",
         f"      let taken = {fn}(stack: stack, fp: fp, a: av^, b: bv^);",
         "      let next = pc + 1_u64;", "      if taken {", "        set next = cvt::<u32, u64>(tv^);", "      }",
         "      if next < n {"] + tail("next", 8) + ["      }", "      return trap(code: 1_u32, pc: pc);", "    }"]
    return o



# ---- the accumulator ------------------------------------------------------
# A value passes in the interpreter function's acc parameter instead of a
# frame slot from the operation that computed it to a later operation
# reading it. The translator (interp_tail.wf's acc_feed) tracks what acc
# holds on the fall-through path: a local, and the candidate, the newest
# operation whose result could go to acc, which it rewrites in place when a
# reader of that result appears. A form's letters name what moves: D its
# result goes to acc only (the temporary's one reader reads acc), SD its
# result goes to slot d and to acc (wasmi's SlotAndReg forms: a local.set
# taking the result, or a temporary a branch may also read), A (B, V, C, S,
# T) its a (b, v, c, s, t) operand comes from acc. Every form keeps its base
# operation's fields, so the compare and address fusions and the patching of
# forward branches carry a form; the field a form reads from acc holds the
# sentinel 65535, never a slot.
ACC = {}
for nm in ["I32Add", "I32Mul", "I32And", "I32Or", "I32Xor"]:
    ACC[nm] = ["A", "D", "AD"]
for nm in ["I32Sub", "I32Shl", "I32ShrU", "I32ShrS", "I32GtS"]:
    ACC[nm] = ["A", "B", "D", "AD", "BD"]
for nm in ["I32Eq", "I32Ne", "I32Eqz", "BrI32Eq", "BrI32Ne"]:
    ACC[nm] = ["A"]
for nm in ["LtS", "LtU", "GtU", "LeS", "LeU", "GeS", "GeU"]:
    ACC["I32" + nm] = ["A", "B"]
for nm in ["LtS", "LtU", "GtS", "GtU", "LeS", "LeU", "GeS", "GeU"]:
    ACC["BrI32" + nm] = ["A", "B"]
for nm in ["I32Extend8S", "I32Extend16S"]:
    ACC[nm] = ["A", "D", "AD"]
for nm in ["I32Load", "I32Load8U", "I32Load8S", "I32Load16U", "I32Load16S"]:
    ACC[nm] = ["A", "D", "AD"]
    ACC[nm + "Ix"] = ["A", "D", "AD"]
for nm in ["I32Store", "I32Store8", "I32Store16"]:
    ACC[nm] = ["A", "V"]
    ACC[nm + "Ix"] = ["A", "V"]
ACC["BrIf"] = ["C"]
ACC["BrUnless"] = ["C"]
ACC["Select"] = ["C", "D", "CD"]
# A copy into a local from acc: a local.set of a value acc holds, or of the
# candidate's temporary; a pair of copies with either source in acc.
ACC["Copy"] = ["S"]
ACC["Copy2"] = ["S", "T"]
# The operations with SD forms: one for the slot-reading form and one for
# each form reading an operand from acc.
SLOT_ACC = ["I32Add", "I32Sub", "I32Mul", "I32And", "I32Or", "I32Xor", "I32Shl", "I32ShrU", "I32ShrS"]
for nm in ["I32Load", "I32Load8U", "I32Load8S", "I32Load16U", "I32Load16S"]:
    SLOT_ACC += [nm, nm + "Ix"]
for nm in SLOT_ACC:
    ACC[nm] = ACC[nm] + [f + "SD" for f in ["", "A", "B"] if f == "" or f in ACC[nm]]
# Operations whose a and b may trade places: a b operand from acc becomes
# the A form with the operands swapped (an indexed address is a + b).
SWAPS = {"I32Add", "I32Mul", "I32And", "I32Or", "I32Xor", "I32Eq", "I32Ne", "BrI32Eq", "BrI32Ne",
         "I32LoadIx", "I32Load8UIx", "I32Load8SIx", "I32Load16UIx", "I32Load16SIx",
         "I32StoreIx", "I32Store8Ix", "I32Store16Ix"}
# Branches take acc only through the sentinel the translator records for
# them, since a forward branch is rebuilt when its target is known.
BRANCHES = {"BrIf", "BrUnless"} | {"BrI32" + nm for _, nm, _, _ in FUSED}
FIELD = {"A": "a", "B": "b", "V": "v", "C": "c", "S": "s", "T": "t"}
SENT = "65535_u16"


def acc_inputs(name):
    return [l for l in "ABVCST" if l in ACC[name]]


def base_form(name):
    """The operation and form a variant's name combines, the form empty for an operation."""
    for b in ACC:
        for f in ACC[b]:
            if name == b + f:
                return b, f
    return name, ""


def word(var, field, letter, form):
    """Binds {var}w to the operand in field, from acc when the form reads it there."""
    if letter in form:
        return [f"let {var}w = acc;"]
    return slot(var, f"{field}^") + [f"let {var}w = stack^.inner[{var}t];"]


def result(form, ind):
    """Stores zw in slot d, unless the form leaves it in acc only (D), sets
    acc to it for a form leaving it there (D, SD), and advances."""
    p = " " * ind
    store = [p + l for l in slot("d", "dv^") + ["set stack^.inner[dt] = zw;"]]
    if form.endswith("SD"):
        return store + advance(ind, "zw")
    if "D" in form:
        return advance(ind, "zw")
    return store + advance(ind)


def acc_numeric_arm(name, ins, out, lines, form, fields):
    binds = ", ".join(f"{f}: {f}v" for f in fields)
    b = word("x", "av", "A", form) + decode("x", "xw", ins[0])
    if len(ins) == 2:
        b += word("y", "bv", "B", form) + decode("y", "yw", ins[1])
    b += lines
    b += (["let zw = 0_u64;", "if z {", "  set zw = 1_u64;", "}"] if out == "bool" else encode(out))
    o = [f"    {name}{form}({binds}) => {{"] + ["      " + l for l in b] + result(form, 6)
    return o + ["      return trap(code: 1_u32, pc: pc);", "    }"]


def acc_load_arm(name, nbytes, signed, out, form, indexed):
    pat = "b: bv, " if indexed else ""
    b = ["let offset = ov^;"] + word("a", "av", "A", form)
    if indexed:
        b += slot("b", "bv^") + indexed_address(nbytes)
    else:
        b += memory_address(nbytes)
    o = [f"    {name}{form}(d: dv, a: av, {pat}o: ov) => {{"] + ["      " + l for l in b]
    o += ["      if lim <= mem^.inner.len {"] + ["        " + l for l in load_value(nbytes, signed, out)]
    o += result(form, 8) + ["      }", "      return trap(code: 3_u32, pc: pc);", "    }"]
    return o


def acc_store_arm(name, nbytes, form, indexed):
    pat = "b: bv, " if indexed else ""
    b = ["let offset = ov^;"] + word("a", "av", "A", form)
    if indexed:
        b += slot("b", "bv^")
    b += word("v", "vv", "V", form)
    b += indexed_address(nbytes) if indexed else memory_address(nbytes)
    o = [f"    {name}{form}(a: av, {pat}v: vv, o: ov) => {{"] + ["      " + l for l in b]
    o += ["      if lim <= mem^.inner.len {"] + ["        " + l for l in store_value(nbytes)]
    o += advance(8) + ["      }", "      return trap(code: 3_u32, pc: pc);", "    }"]
    return o


def acc_fused_arm(name, op, sign, form):
    ty = sign + "32"
    b = word("x", "av", "A", form) + decode("x", "xw", ty) + word("y", "bv", "B", form) + decode("y", "yw", ty)
    b += [f"let taken = x {op} y;", "let next = pc + 1_u64;", "if taken {", "  set next = cvt::<u32, u64>(tv^);", "}"]
    o = [f"    {name}{form}(a: av, b: bv, t: tv) => {{"] + ["      " + l for l in b]
    return o + ["      if next < n {"] + tail("next", 8) + ["      }", "      return trap(code: 1_u32, pc: pc);", "    }"]


def acc_control_arms():
    o = []
    for name, test in [("BrIf", "!="), ("BrUnless", "==")]:
        o += [f"    {name}C(t: tv, c: cv) => {{", "      let next = pc + 1_u64;", f"      if acc {test} 0_u64 {{",
              "        set next = cvt::<u32, u64>(tv^);", "      }", "      if next < n {"] + tail("next", 8)
        o += ["      }", "      return trap(code: 1_u32, pc: pc);", "    }"]
    for form in ACC["Select"]:
        b = word("c", "cv", "C", form) + ["let pick = bv^;", "if cw != 0_u64 {", "  set pick = av^;", "}",
                                          "let si = cvt::<u16, u64>(pick);"]
        if "D" in form:
            b += ["let st = fp + si;", "let zw = stack^.inner[st];"]
            end = advance(6, "zw")
        else:
            b += ["let di = cvt::<u16, u64>(dv^);", "move_slot(stack: stack, fp: fp, s: si, d: di);"]
            end = advance(6)
        o += [f"    Select{form}(d: dv, a: av, b: bv, c: cv) => {{"] + ["      " + l for l in b] + end
        o += ["      return trap(code: 1_u32, pc: pc);", "    }"]
    from_acc = lambda d: [f"let {d}i = cvt::<u16, u64>({d}v^);", f"let {d}t = fp + {d}i;", f"set stack^.inner[{d}t] = acc;"]
    from_slot = lambda d, s: [f"let {s}i = cvt::<u16, u64>({s}v^);", f"let {s}t = fp + {s}i;", f"let {s}w = stack^.inner[{s}t];",
                              f"let {d}i = cvt::<u16, u64>({d}v^);", f"let {d}t = fp + {d}i;", f"set stack^.inner[{d}t] = {s}w;"]
    moves = {"CopyS": ("(d: dv, s: sv)", from_acc("d")),
             "Copy2S": ("(d: dv, s: sv, e: ev, t: tv)", from_acc("d") + from_slot("e", "t")),
             "Copy2T": ("(d: dv, s: sv, e: ev, t: tv)", from_slot("d", "s") + from_acc("e"))}
    for name in [b + f for b in ["Copy", "Copy2"] for f in ACC[b]]:
        pattern, b = moves[name]
        o += [f"    {name}{pattern} => {{"] + ["      " + l for l in b] + advance(6)
        o += ["      return trap(code: 1_u32, pc: pc);", "    }"]
    return o


def acc_functions(fields_of):
    """The translator's view of the forms: acc_norm turns an operation with
    the sentinel in a field into the form reading acc there; acc_input
    turns one reading slot s into that form; acc_dest gives the form
    leaving the result in acc, acc_slot_dest the form writing slot d and
    acc; has_sentinel finds a sentinel no form took."""
    w = []
    w.append("fn acc_norm(op: Op) -> o: Op pure {")
    w.append('  doc "The form of op reading acc where a field holds the sentinel 65535, or op itself.";')
    w.append("  match op {")
    for name, fields in fields_of:
        if name not in ACC:
            w += [f"    {name}(..) => {{", "      return op;", "    }"]
            continue
        binds = ", ".join(f"{f}: x_{f}" for f in fields)
        w.append(f"    {name}({binds}) => {{")
        for letter in acc_inputs(name):
            f = FIELD[letter]
            inits = ", ".join(f"{g}: x_{g}" for g in fields)
            w += [f"      if x_{f} == {SENT} {{", f"        let o = Op::{name}{letter}({inits});", "        return o;", "      }"]
        if name in SWAPS:
            inits = ", ".join({"a": "a: x_b", "b": "b: x_a"}.get(g, f"{g}: x_{g}") for g in fields)
            w += [f"      if x_b == {SENT} {{", f"        let o = Op::{name}A({inits});", "        return o;", "      }"]
        w += ["      return op;", "    }"]
    w += ["  }", "}", ""]
    w.append("fn acc_input(op: Op, s: u16) -> found: Option<Op> pure {")
    w.append('  doc "The form of op taking the operand it reads from slot s from acc instead, or None.";')
    w.append("  match op {")
    for name, fields in fields_of:
        if name not in ACC or name in BRANCHES:
            w += [f"    {name}(..) => {{", "      return None<Op>();", "    }"]
            continue
        binds = ", ".join(f"{f}: x_{f}" for f in fields)
        w.append(f"    {name}({binds}) => {{")
        reads = [FIELD[l] for l in acc_inputs(name)] + (["b"] if name in SWAPS else [])
        for f in reads:
            inits = ", ".join((f"{g}: {SENT}" if g == f else f"{g}: x_{g}") for g in fields)
            w += [f"      if x_{f} == s {{", f"        let o = Op::{name}({inits});", "        let n = acc_norm(op: o);",
                  "        return Some<Op>(value: n);", "      }"]
        w += ["      return None<Op>();", "    }"]
    w += ["  }", "}", ""]
    for fn, doc, suffix in [("acc_dest", "The form of op leaving its result in acc instead of slot d, or None.", "D"),
                            ("acc_slot_dest", "The form of op writing slot d and leaving its result in acc as well, or None.", "SD")]:
        w.append(f"fn {fn}(op: Op) -> found: Option<Op> pure {{")
        w.append(f'  doc "{doc}";')
        w.append("  match op {")
        for name, fields in fields_of:
            base, form = base_form(name)
            target = form + suffix
            if base in ACC and "D" not in form and target in ACC[base]:
                binds = ", ".join(f"{f}: x_{f}" for f in fields)
                w += [f"    {name}({binds}) => {{", f"      let o = Op::{base}{target}({binds});", "      return Some<Op>(value: o);", "    }"]
            else:
                w += [f"    {name}(..) => {{", "      return None<Op>();", "    }"]
        w += ["  }", "}", ""]
    w.append("fn has_sentinel(op: Op) -> yes: Bool pure {")
    w.append('  doc "Whether an operation reading only slots holds the sentinel where it reads one.";')
    w.append("  match op {")
    for name, fields in fields_of:
        if name not in ACC:
            w += [f"    {name}(..) => {{", "      return False();", "    }"]
            continue
        binds = ", ".join(f"{f}: x_{f}" for f in fields)
        w.append(f"    {name}({binds}) => {{")
        for f in [FIELD[l] for l in acc_inputs(name)] + (["b"] if name in SWAPS else []):
            w += [f"      if x_{f} == {SENT} {{", "        return True();", "      }"]
        w += ["      return False();", "    }"]
    w += ["  }", "}", ""]
    return w


def inline_helpers(arms, helpers):
    """The --inline variant: each arm carries its helper's body in place of
    the call, the helper's parameters bound by `let` from the call's
    arguments; `return True()` becomes the arm's advance, `return False()`
    its trap, and a comparison's `return z;` binds `taken`."""
    import re
    bodies = {}
    text = "\n".join(helpers)
    for match in re.finditer(r"^fn (op_\w+)\((.*?)\) -> (\w+): \w+ .*?\{\n(?:.*?\n)*?\} \{\n((?:  .*\n)*?)\}\n", text, re.M):
        name, params, result, body = match.group(1), match.group(2), match.group(3), match.group(4)
        bodies[name] = (result, [line[2:] for line in body.rstrip("\n").split("\n")])
    out = []
    kept = set()
    i = 0
    while i < len(arms):
        line = arms[i]
        call = re.search(r"(op_\w+)\((.*)\);$", line)
        if not call or call.group(1) not in bodies:
            out.append(line)
            i += 1
            continue
        name, arguments = call.group(1), call.group(2)
        result, body = bodies[name]
        if any("give " in b for b in body):
            # A body that delivers a value from a match stays a call: the
            # checker's delivery of such values grows with the function
            # (docs/todo.md, "Checking a function with many value deliveries").
            out.append(line)
            kept.add(name)
            i += 1
            continue
        indent = line[: len(line) - len(line.lstrip())]
        binds = []
        for argument in arguments.split(", "):
            formal, actual = argument.split(": ", 1)
            if formal != actual:
                binds.append(f"{indent}let {formal} = {actual};")
        if result == "ok":
            # let ok = op(...); if ok { ADVANCE } return trap(...);
            opener = arms[i + 1]
            closer = opener[: len(opener) - len(opener.lstrip())] + "}"
            j = i + 2
            advance = []
            while arms[j] != closer:
                advance.append(arms[j][2:])
                j += 1
            failure = arms[j + 1].strip()
            out += binds
            for b in body:
                if b.strip() == "return True();":
                    pad = b[: len(b) - len(b.lstrip())]
                    out += [indent + pad + a[len(indent):] for a in advance]
                elif b.strip() == "return False();":
                    pad = b[: len(b) - len(b.lstrip())]
                    out.append(indent + pad + failure)
                else:
                    out.append(indent + b)
            if body and body[-1].strip() == "return True();":
                out.append(indent + failure)
            i = j + 2
        elif result == "taken":
            out += binds
            for b in body:
                if b.strip() == "return z;":
                    out.append(indent + "let taken = z;")
                else:
                    out.append(indent + b)
            i += 1
        else:
            out += binds
            out += [indent + b for b in body if b.strip() != "return unit;"]
            i += 1
    return out, kept

def count_dispatches(program):
    """The --count variant: a counter cell passed to the interpreter function
    and incremented at every dispatch, printed when _start returns."""
    sig = ("globals: &Box<Slots<u64>>, pc0: u64, fp0: u64, acc0: u64) -> r: Outcome reads(code), reads(funcs), "
           "reads(brtab), reads(table), reads(consts), writes(stack), writes(mem), writes(globals) contract {")
    assert sig in program
    program = program.replace(sig, sig.replace("pc0: u64,", "counter: &Box<Array<u64>>, pc0: u64,")
                              .replace("writes(globals) contract", "writes(globals), writes(counter) contract"))
    top = "  ) {\n    match code^.inner[pc] {"
    assert top in program
    program = program.replace(top, "  ) {\n    if 0_u64 < counter^.inner.len {\n"
                              "      let seen = counter^.inner[0_u64];\n"
                              "      set counter^.inner[0_u64] = seen +wrap 1_u64;\n    }\n    match code^.inner[pc] {")
    program = program.replace("globals: &globals, pc0: pc, fp0: fp, acc0: 0_u64);",
                              "globals: &globals, counter: &counter, pc0: pc, fp0: fp, acc0: 0_u64);")
    program = program.replace("  let max_pages = info.max_pages;\n  loop @drive {",
                              "  let max_pages = info.max_pages;\n"
                              "  let counter = box_array_filled::<u64>(count: 1_u64, value: 0_u64);\n  loop @drive {")
    done = "      Done() => {\n        return 0_u8;\n      }"
    assert done in program
    program = program.replace(done, """      Done() => {
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


def profile_dispatches(program, variants):
    """The --profile variant: one counter per operation kind, incremented at
    every dispatch and printed, one 20-digit count per line in enum order,
    when _start returns; gen.py --names prints the names in the same order."""
    program = count_dispatches(program)
    total = len(variants)
    program = program.replace("let counter = box_array_filled::<u64>(count: 1_u64, value: 0_u64);",
                              f"let counter = box_array_filled::<u64>(count: {total}_u64, value: 0_u64);")
    old = ("    if 0_u64 < counter^.inner.len {\n      let seen = counter^.inner[0_u64];\n"
           "      set counter^.inner[0_u64] = seen +wrap 1_u64;\n    }\n")
    assert old in program
    program = program.replace(old, "    let kind = op_index(op: code^.inner[pc]);\n    if kind < counter^.inner.len {\n"
                              "      let seen = counter^.inner[kind];\n      set counter^.inner[kind] = seen +wrap 1_u64;\n    }\n")
    old = "        if 0_u64 < counter.inner.len {\n          let total = counter.inner[0_u64];"
    assert old in program
    program = program.replace(old, "        let kinds = counter.inner.len;\n        for (kind in 0_u64..kinds) {\n          let total = counter.inner[kind];")
    index = ["fn op_index(op: Op) -> kind: u64 pure {", '  doc "The position of an operation\'s kind in the enum.";', "  match op {"]
    for i, (name, fields, dest) in enumerate(variants):
        index += [f"    {name}(..) => {{", f"      return {i}_u64;", "    }"]
    index += ["  }", "}", ""]
    return program.replace("fn run(", "\n".join(index) + "\nfn run(", 1)

# ---- emit -----------------------------------------------------------------
# Control, variable and call operations; their handlers are in interp_head.wf.
CONTROL = ["Unreachable()", "Jump(t: u32)", "Br(t: u32, s: u16, d: u16)", "BrIf(t: u32, c: u16)",
           "BrIfMove(c: u16, e: u32)", "BrUnless(t: u32, c: u16)", "BrTable(c: u16, start: u32, count: u32)",
           "Return(s: u16, k: u16, l: u16)", "Call(f: u32, at: u16)", "CallIndirect(canon: u32, at: u16, i: u16)",
           "Host(id: u16, at: u16)", "Select(d: u16, a: u16, b: u16, c: u16)", "Copy(d: u16, s: u16)", "Copy2(d: u16, s: u16, e: u16, t: u16)",
           "GlobalGet(d: u16, i: u32)", "GlobalSet(s: u16, i: u32)", "MemorySize(d: u16)",
           "MemoryGrow(d: u16, s: u16)", "MemoryCopy(d: u16, s: u16, n: u16)",
           "MemoryFill(d: u16, v: u16, n: u16)", "Const(d: u16, v: u64)"]
DEST = {"Select", "Copy", "GlobalGet", "MemorySize", "MemoryGrow", "Const"}

out = []
w = out.append

variants = []
for v in CONTROL:
    name, fields = v[:-1].split("(")
    variants.append((name, [f.split(": ")[0] for f in fields.split(", ")] if fields else [], name in DEST))
w("enum Op {")
for v in CONTROL:
    w(f"  {v};")
for code, name, *_ in LOADS:
    w(f"  {name}(d: u16, a: u16, o: u32);")
    variants.append((name, ["d", "a", "o"], True))
for code, name, _ in STORES:
    w(f"  {name}(a: u16, v: u16, o: u32);")
    variants.append((name, ["a", "v", "o"], False))
for code, name, *_ in LOADS:
    if name in INDEXED:
        w(f"  {name}Ix(d: u16, a: u16, b: u16, o: u32);")
        variants.append((name + "Ix", ["d", "a", "b", "o"], True))
for code, name, _ in STORES:
    if name in INDEXED:
        w(f"  {name}Ix(a: u16, b: u16, v: u16, o: u32);")
        variants.append((name + "Ix", ["a", "b", "v", "o"], False))
for code, name, ins, *_ in N:
    if len(ins) == 2:
        w(f"  {name}(d: u16, a: u16, b: u16);")
        variants.append((name, ["d", "a", "b"], True))
    else:
        w(f"  {name}(d: u16, a: u16);")
        variants.append((name, ["d", "a"], True))
for sub, name, *_ in FC:
    w(f"  {name}(d: u16, a: u16);")
    variants.append((name, ["d", "a"], True))
for code, nm, _, _ in FUSED:
    w(f"  BrI32{nm}(a: u16, b: u16, t: u32);")
    variants.append((f"BrI32{nm}", ["a", "b", "t"], False))
declared = {line.strip().split("(")[0]: line.strip()[len(line.strip().split("(")[0]):] for line in out[1:]}
for name, fields, dest in list(variants):
    for form in ACC.get(name, []):
        w(f"  {name}{form}{declared[name]}")
        variants.append((name + form, fields, dest and "D" not in form))
w("}")
w("")

w("fn numeric_op(code: u8, d: u16, a: u16, b: u16) -> found: Option<Op> pure {")
w('  doc "The operation of a one-byte numeric opcode writing slot d from a (and b), or None for an opcode that is not one.";')
for code, name, ins, *_ in N:
    w(f"  if code == {code}_u8 {{")
    if len(ins) == 2:
        w(f"    let o = Op::{name}(d: d, a: a, b: b);")
    else:
        w(f"    let o = Op::{name}(d: d, a: a);")
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
w("fn saturating_op(sub: u64, d: u16, a: u16) -> found: Option<Op> pure {")
w('  doc "The operation of a saturating truncation, 0xfc 0 through 7.";')
for sub, name, *_ in FC:
    w(f"  if sub == {sub}_u64 {{")
    w(f"    let o = Op::{name}(d: d, a: a);")
    w(f"    return Some<Op>(value: o);")
    w("  }")
w("  return None<Op>();")
w("}")
w("")
w("fn load_op(code: u8, first: u16, second: u16, offset: u32) -> found: Option<Op> pure {")
w('  doc "The operation of a load (first the destination, second the address) or a store (first the address, second the value).";')
for code, name, *_ in LOADS:
    w(f"  if code == {code}_u8 {{")
    w(f"    let o = Op::{name}(d: first, a: second, o: offset);")
    w(f"    return Some<Op>(value: o);")
    w("  }")
for code, name, _ in STORES:
    w(f"  if code == {code}_u8 {{")
    w(f"    let o = Op::{name}(a: first, v: second, o: offset);")
    w(f"    return Some<Op>(value: o);")
    w("  }")
w("  return None<Op>();")
w("}")
w("")
w("fn with_dest(op: Op, slot: u16) -> found: Option<Op> pure {")
w('  doc "The operation writing slot instead of its own destination, or None for one that writes no slot.";')
w("  match op {")
for name, fields, dest in variants:
    if not dest:
        w(f"    {name}(..) => {{")
        w("      return None<Op>();")
        w("    }")
        continue
    binds = ", ".join(f"{f}: x_{f}" for f in fields)
    inits = ", ".join(("d: slot" if f == "d" else f"{f}: x_{f}") for f in fields)
    w(f"    {name}({binds}) => {{")
    w(f"      let o = Op::{name}({inits});")
    w("      return Some<Op>(value: o);")
    w("    }")
w("  }")
w("}")
w("")

w("fn indexed_op(code: u8, first: u16, a: u16, b: u16, offset: u32) -> found: Option<Op> pure {")
w('  doc "The i32 load (first the destination) or store (first the value) at address a + b, or None for an operation without that form.";')
for code, name, *_ in LOADS:
    if name in INDEXED:
        w(f"  if code == {code}_u8 {{")
        w(f"    let o = Op::{name}Ix(d: first, a: a, b: b, o: offset);")
        w("    return Some<Op>(value: o);")
        w("  }")
for code, name, _ in STORES:
    if name in INDEXED:
        w(f"  if code == {code}_u8 {{")
        w(f"    let o = Op::{name}Ix(a: a, b: b, v: first, o: offset);")
        w("    return Some<Op>(value: o);")
        w("  }")
w("  return None<Op>();")
w("}")
w("")
w("fn indexed(code: u8) -> yes: Bool pure {")
w('  doc "Whether the load or store opcode has a form taking its address as a sum.";')
for code, name, *_ in LOADS + STORES:
    if name in INDEXED:
        w(f"  if code == {code}_u8 {{")
        w("    return True();")
        w("  }")
w("  return False();")
w("}")
w("")
w("fn add_operands(op: Op) -> (found: Bool, a: u16, b: u16) pure {")
w('  doc "An i32.add\'s operand slots, found false for any other operation.";')
w("  match op {")
for name, fields, dest in variants:
    if name in ("I32Add", "I32AddA"):
        w(f"    {name}(d: x_d, a: x_a, b: x_b) => {{")
        w("      let yes = True();")
        w("      return yes, x_a, x_b;")
        w("    }")
    else:
        w(f"    {name}(..) => {{")
        w("      let no = False();")
        w("      return no, 0_u16, 0_u16;")
        w("    }")
w("  }")
w("}")
w("")

w("fn compare_code(op: Op) -> (code: u64, a: u16, b: u16) pure {")
w('  doc "An i32 comparison\'s opcode and operand slots, 0x45 for eqz, or zero for any other operation.";')
w("  match op {")
compares = {f"I32{nm}": c for c, nm, _, _ in FUSED}
compares["I32Eqz"] = 0x45
for name in list(compares):
    for form in ACC.get(name, []):
        compares[name + form] = compares[name]
for name, fields, dest in variants:
    if name in compares:
        binds = ", ".join(f"{f}: x_{f}" for f in fields)
        second = "x_b" if "b" in fields else "0_u16"
        w(f"    {name}({binds}) => {{")
        w(f"      return {compares[name]}_u64, x_a, {second};")
        w("    }")
    else:
        w(f"    {name}(..) => {{")
        w("      return 0_u64, 0_u16, 0_u16;")
        w("    }")
w("  }")
w("}")
w("")
w("fn negate_compare(code: u64) -> negated: u64 pure {")
w('  doc "The comparison true exactly when code\'s is false; 0xa8 stands for a value tested nonzero.";')
for a, b in NEGATE.items():
    w(f"  if code == {a}_u64 {{")
    w(f"    return {b}_u64;")
    w("  }")
w("  return 0_u64;")
w("}")
w("")
w("fn fused_branch(code: u64, a: u16, b: u16, t: u32) -> op: Op pure {")
w('  doc "The branch to t taken when comparison code holds of slots a and b: eqz is BrUnless, nonzero is BrIf.";')
for c, nm, _, _ in FUSED:
    w(f"  if code == {c}_u64 {{")
    w(f"    let o = Op::BrI32{nm}(a: a, b: b, t: t);")
    w("    return acc_norm(op: o);")
    w("  }")
w("  if code == 69_u64 {")
w("    let o = Op::BrUnless(t: t, c: a);")
w("    return acc_norm(op: o);")
w("  }")
w("  let o = Op::BrIf(t: t, c: a);")
w("  return acc_norm(op: o);")
w("}")
w("")
for line in acc_functions([(name, fields) for name, fields, _ in variants]):
    w(line)

OPS = "\n".join(out).strip("\n")

arms = []
for code, name, nbytes, signed, outty in LOADS:
    arms += load_arm(name, nbytes, signed, outty)
for code, name, nbytes in STORES:
    arms += store_arm(name, nbytes)
for code, name, nbytes, signed, outty in LOADS:
    if name in INDEXED:
        arms += load_arm(name + "Ix", nbytes, signed, outty, indexed=True)
for code, name, nbytes in STORES:
    if name in INDEXED:
        arms += store_arm(name + "Ix", nbytes, indexed=True)
for code, name, ins, outty, lines in N:
    arms += numeric_arm(name, ins, outty, lines)
for sub, name, ins, outty, lines in FC:
    arms += numeric_arm(name, ins, outty, lines)
for code, nm, op, sign in FUSED:
    arms += fused_arm(f"BrI32{nm}", op, sign)
for code, name, nbytes, signed, outty in LOADS:
    for suffix, indexed in [("", False), ("Ix", True)]:
        for form in ACC.get(name + suffix, []):
            arms += acc_load_arm(name + suffix, nbytes, signed, outty, form, indexed)
for code, name, nbytes in STORES:
    for suffix, indexed in [("", False), ("Ix", True)]:
        for form in ACC.get(name + suffix, []):
            arms += acc_store_arm(name + suffix, nbytes, form, indexed)
for code, name, ins, outty, lines in N:
    for form in ACC.get(name, []):
        arms += acc_numeric_arm(name, ins, outty, lines, form, ["d", "a", "b"] if len(ins) == 2 else ["d", "a"])
for code, nm, op, sign in FUSED:
    for form in ACC.get(f"BrI32{nm}", []):
        arms += acc_fused_arm(f"BrI32{nm}", op, sign, form)
arms += acc_control_arms()
if "--inline" in sys.argv:
    arms, kept = inline_helpers(arms, HELPERS)
    import re as _re
    chunks = [chunk.strip("\n") for chunk in "\n".join(HELPERS).split("\n\n") if chunk.strip()]
    HELPERS = ["\n\n".join(chunk for chunk in chunks
                             if (_re.match(r"fn (op_\w+)\(", chunk) or [None, None])[1] in kept)]
here = os.path.dirname(os.path.abspath(__file__))
head = open(os.path.join(here, "interp_head.wf")).read()
tail_text = open(os.path.join(here, "interp_tail.wf")).read().strip("\n")
aliases_end = head.index("\n\n") + 2
helper_text = "\n".join(HELPERS).strip("\n")
program = (head[:aliases_end] + OPS + "\n\n" + (helper_text + "\n\n" if helper_text else "")
           + head[aliases_end:].rstrip("\n") + "\n" + "\n".join(("  " + line) if line else line for line in "\n".join(arms).strip("\n").split("\n"))
           + "\n    }\n  }\n  return trap(code: 1_u32, pc: pc);\n}\n\n" + tail_text + "\n")
if "--names" in sys.argv:
    print("\n".join(name for name, _, _ in variants))
    sys.exit(0)
if "--profile" in sys.argv:
    program = profile_dispatches(program, variants)
elif "--count" in sys.argv:
    program = count_dispatches(program)
open(sys.argv[1], "w").write(program)
