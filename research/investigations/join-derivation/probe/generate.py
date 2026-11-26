"""Writes the Whitefoot programs of the compiler probe
(research/investigations/join-derivation/DESIGN.md): CHAIN(k), NEST(k) and
GUARDED(k), one file per size, into the directory given as the argument."""

import pathlib
import sys

HEAD = """fn at_most(value: u64, limit: u64) -> result: unit pure contract {
  requires value <= limit;
} {
  return unit;
}

fn below(value: u64, limit: u64) -> result: unit pure contract {
  requires value < limit;
} {
  return unit;
}
"""

TAIL = """
fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"""


def flags(k):
    return "".join(f", c{i}: Bool" for i in range(1, k + 1))


def chain(k):
    body = ["  let total = start;"]
    for i in range(1, k + 1):
        body += [f"  if c{i} {{", "    set total = total + 1_u64;", "  }"]
    body += [f"  at_most(value: total, limit: {100 + k}_u64);", "  return unit;"]
    return (
        f"fn probe(start: u64{flags(k)}) -> result: unit pure contract {{\n"
        "  requires start <= 100_u64;\n} {\n" + "\n".join(body) + "\n}\n"
    )


def nest(k):
    body = ["  let total = start;"]
    for i in range(1, k + 1):
        pad = "  " * i
        body += [f"{pad}if c{i} {{", f"{pad}  set total = total + 1_u64;"]
    for i in range(k, 0, -1):
        body.append("  " * i + "}")
    body += [f"  at_most(value: total, limit: {100 + k}_u64);", "  return unit;"]
    return (
        f"fn probe(start: u64{flags(k)}) -> result: unit pure contract {{\n"
        "  requires start <= 100_u64;\n} {\n" + "\n".join(body) + "\n}\n"
    )


def guarded(k):
    body = ["  let i = start;"]
    for j in range(1, k + 1):
        body += [
            f"  let next{j} = i + 1_u64;",
            f"  let more{j} = next{j} < limit;",
            f"  if more{j} {{",
            f"    set i = next{j};",
            "  }",
        ]
    body += ["  below(value: i, limit: limit);", "  return unit;"]
    return (
        "fn probe(start: u64, limit: u64) -> result: unit pure contract {\n"
        "  requires start < limit;\n} {\n" + "\n".join(body) + "\n}\n"
    )


FAMILIES = {"chain": chain, "nest": nest, "guarded": guarded}
SIZES = [1, 2, 3, 4, 6, 8, 10, 12, 14, 16, 18]


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, make in FAMILIES.items():
        for k in SIZES:
            (out / f"{name}-{k:02}.wf").write_text(HEAD + "\n" + make(k) + TAIL)


if __name__ == "__main__":
    main()
