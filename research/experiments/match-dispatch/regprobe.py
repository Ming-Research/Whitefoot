import subprocess, re
def spills(cc, target, n, ty):
    params = ", ".join(f"{ty} %a{i}" for i in range(n))
    op = "fadd" if ty == "double" else "add"
    body = []; prev = "%a0"
    for i in range(1, n):
        body.append(f"  %s{i} = {op} {ty} {prev}, %a{i}"); prev = f"%s{i}"
    ir = f"define {cc} {ty} @f({params}) {{\n" + "\n".join(body) + f"\n  ret {ty} {prev}\n}}\n"
    open("rp.ll","w").write(ir)
    asm = subprocess.run(["clang","-target",target,"-O2","-S","-o","-","-x","ir","rp.ll"],capture_output=True,text=True).stdout
    return bool(re.search(r"\[sp|\[x29|\(%rsp\)|\(%rbp\)|%rsp\)", asm))
for cc in ["preserve_nonecc", ""]:
    for t in ["aarch64-apple-darwin","aarch64-unknown-linux-gnu","x86_64-unknown-linux-gnu","x86_64-pc-windows-msvc"]:
        res = []
        for ty in ["i64","double"]:
            last = 0
            for n in range(1, 40):
                if spills(cc, t, n, ty): break
                last = n
            res.append(f"{ty}={last}")
        print(f"{cc or 'C':16} {t:28} " + " ".join(res))
