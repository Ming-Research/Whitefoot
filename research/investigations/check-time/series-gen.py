import sys
n=int(sys.argv[1]); loop=len(sys.argv)>2
out=["enum Op {"]+[f"  V{i}();" for i in range(n)]+["}",""]
out+=["fn f(op: Op) -> r: u64 pure {", '  doc "One constant assignment per arm, joined after the match.";', "  let v = 0_u64;"]
ind="  "
if loop:
    out+=["  loop {"]; ind="    "
out+=[ind+"match op {"]
for i in range(n):
    out+=[ind+f"  V{i}() => {{", ind+f"    set v = {i}_u64;", ind+"  }"]
out+=[ind+"}"]
if loop:
    out+=["    if v == 0_u64 {","      break;","    }","  }"]
out+=["  return v;","}","","fn main() -> status: std::process::ExitStatus pure {",'  doc "Checks the function.";',"  return std::process::exit_status(code: 0_u8);","}",""]
print("\n".join(out),end="")
