//! Writer-trial effect-root and reference-kind repairs, carried out literally.

use super::RepairPair;

pub(super) const WRITER_REFERENCES: &[RepairPair] = &[
    RepairPair {
        name: "effect-root-by-value-copy-parameter.wf",
        rejected: br#"fn echo(p: u64) -> result: u64 reads(p) {
  return p;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = echo(p: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "\n  reason: every effect path is rooted at one reference parameter of the same callable, and this root is not one\n",
            "\n  mechanical_fix: `p` is a by-value parameter; an effect root is a reference parameter. Remove this effect entry: the call already records the argument's read or consumption.\n",
        ],
        repaired: &[br#"fn echo(p: u64) -> result: u64 pure {
  return p;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = echo(p: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "effect-root-by-value-owner-parameter.wf",
        rejected: br#"fn consume(pool: Box<u8>) -> result: unit writes(pool) {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pool = box_new::<u8>(value: 3_u8);
  consume(pool: move pool);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "\n  mechanical_fix: `pool` is a by-value parameter; an effect root is a reference parameter. Remove this effect entry: the call already records the argument's read or consumption.\n",
        ],
        repaired: &[br#"fn consume(pool: Box<u8>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pool = box_new::<u8>(value: 3_u8);
  consume(pool: move pool);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "array-borrow-where-range-is-required.wf",
        rejected: br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let copied = first(values: &a);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "\n  expected: &[u8]\n",
            "\n  found: &Array<u8, 4>\n",
            "\n  mechanical_fix: pass a range reference `&a[0_u64..n]`, with `n <= a.len` proved, or `&a[0_u64..a.len]` for the whole array; extend the enclosing effect row to cover any reads through reference parameters [EFF-2]\n",
        ],
        repaired: &[
            br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let n = 2_u64;
  let copied = first(values: &a[0_u64..n]);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let copied = first(values: &a[0_u64..a.len]);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "array-reference-where-range-is-required.wf",
        rejected: br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn relay(a: &Array<u8, 4>) -> result: u8 reads(a) {
  let copied = first(values: a);
  return copied;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let copied = relay(a: &a);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "\n  mechanical_fix: pass a range reference `&a^[0_u64..n]`, with `n <= a^.len` proved, or `&a^[0_u64..a^.len]` for the whole array; extend the enclosing effect row to cover any reads through reference parameters [EFF-2]\n",
        ],
        repaired: &[
            br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn relay(a: &Array<u8, 4>) -> result: u8 reads(a) {
  let n = 2_u64;
  let copied = first(values: &a^[0_u64..n]);
  return copied;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let copied = relay(a: &a);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn first(values: &[u8]) -> result: u8 reads(values) {
  if 0_u64 < values^.len {
    return values^[0_u64];
  }
  return 0_u8;
}

fn relay(a: &Array<u8, 4>) -> result: u8 reads(a) {
  let copied = first(values: &a^[0_u64..a^.len]);
  return copied;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 4>(value: 7_u8);
  let copied = relay(a: &a);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "reference-match-payload-at-copy-argument.wf",
        rejected: br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn echo(value: u64) -> result: u64 pure {
  return value;
}

fn read(packet: &Packet) -> result: u64 reads(packet) {
  match packet^ {
    Data(value: val) => {
      let copied = echo(value: val);
      return copied;
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let packet = Data(value: 7_u64);
  let copied = read(packet: &packet);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-7",
        sentences: &[
            "\n  mechanical_fix: `val` is a reference because the match scrutinee is a reference; read the copy payload with `val^`\n",
        ],
        repaired: &[br#"enum Packet {
  Data(value: u64);
  Idle();
}

fn echo(value: u64) -> result: u64 pure {
  return value;
}

fn read(packet: &Packet) -> result: u64 reads(packet) {
  match packet^ {
    Data(value: val) => {
      let copied = echo(value: val^);
      return copied;
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let packet = Data(value: 7_u64);
  let copied = read(packet: &packet);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "reference-match-payload-before-field-selection.wf",
        rejected: br#"struct Count {
  number: u64;
}

enum Packet {
  Data(value: Count);
  Idle();
}

fn read(packet: &Packet) -> result: u64 reads(packet) {
  match packet^ {
    Data(value: val) => {
      return val.number;
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let count = Count(number: 7_u64);
  let packet = Data(value: count);
  let copied = read(packet: &packet);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-7",
        sentences: &[
            "\n  mechanical_fix: `val` is a reference because the match scrutinee is a reference; read the copy payload with `val^`; write `val^.number` here\n",
        ],
        repaired: &[br#"struct Count {
  number: u64;
}

enum Packet {
  Data(value: Count);
  Idle();
}

fn read(packet: &Packet) -> result: u64 reads(packet) {
  match packet^ {
    Data(value: val) => {
      return val^.number;
    }
    Idle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let count = Count(number: 7_u64);
  let packet = Data(value: count);
  let copied = read(packet: &packet);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
