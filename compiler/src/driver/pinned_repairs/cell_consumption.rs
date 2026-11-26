//! [TYPE-2, PROV-6, OP-14] taking a cell apart must not hide its linear
//! consumption obligation. Each alternative is checked by the repair harness.

use super::RepairPair;

pub(super) const CELL_CONSUMPTION: &[RepairPair] = &[
    RepairPair {
        name: "linear-cell-binding-nothing.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let content = move cell.inner;`, which frees the cell [WIN-3], then move `content` into a function that consumes it or return it\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(value: Token) -> result: unit pure {
  let Token(value: byte) = move value;
  return unit;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let content = move cell.inner;
  consume(value: move content);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>) -> result: Token pure {
  let content = move cell.inner;
  return move content;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-cell-content-name-is-a-parameter.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>, content: u8) -> result: unit pure {
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let content1 = move cell.inner;`, which frees the cell [WIN-3], then move `content1` into a function that consumes it or return it\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(value: Token) -> result: unit pure {
  let Token(value: byte) = move value;
  return unit;
}

fn finish(cell: Box<Token>, content: u8) -> result: unit pure {
  let content1 = move cell.inner;
  consume(value: move content1);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>, content: u8) -> result: Token pure {
  let content1 = move cell.inner;
  return move content1;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-cell-content-name-reserves-the-declaration-inventory.wf",
        rejected: br#"alias content3 = std::process::exit_status;

nodrop struct Token {
  value: u8;
}

fn content() -> result: unit pure {
  return unit;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let content1 = box_new::<u8>(value: 1_u8);
  let previous = move content1;
  let Box(..) = move cell;
  let content2 = 0_u8;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let content4 = move cell.inner;`, which frees the cell [WIN-3], then move `content4` into a function that consumes it or return it\n"],
        repaired: &[
            br#"alias content3 = std::process::exit_status;

nodrop struct Token {
  value: u8;
}

fn content() -> result: unit pure {
  return unit;
}

fn consume(value: Token) -> result: unit pure {
  let Token(value: byte) = move value;
  return unit;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let content1 = box_new::<u8>(value: 1_u8);
  let previous = move content1;
  let content4 = move cell.inner;
  consume(value: move content4);
  let content2 = 0_u8;
  return unit;
}
"#,
            br#"alias content3 = std::process::exit_status;

nodrop struct Token {
  value: u8;
}

fn content() -> result: unit pure {
  return unit;
}

fn finish(cell: Box<Token>) -> result: Token pure {
  let content1 = box_new::<u8>(value: 1_u8);
  let previous = move content1;
  let content4 = move cell.inner;
  let content2 = 0_u8;
  return move content4;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-cell-binding-content.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let Box(inner: taken) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let taken = move cell.inner;`, which frees the cell [WIN-3], then move `taken` into a function that consumes it or return it\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(value: Token) -> result: unit pure {
  let Token(value: byte) = move value;
  return unit;
}

fn finish(cell: Box<Token>) -> result: unit pure {
  let taken = move cell.inner;
  consume(value: move taken);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Token>) -> result: Token pure {
  let taken = move cell.inner;
  return move taken;
}
"#,
        ],
    },
    RepairPair {
        name: "box-name-on-linear-parameter.wf",
        rejected: br#"fn finish<T>(value: T) -> result: unit pure {
  let Box(inner: taken) = move value;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: the operand is a linear value, not a cell whose content this statement can select: replace this statement by moving `value` whole into a function that consumes it or returning it\n"],
        repaired: &[
            br#"fn consume<T>(value: T) -> result: T pure {
  return move value;
}

fn finish<T>(value: T) -> result: T pure {
  return consume::<T>(value: move value);
}
"#,
            br#"fn finish<T>(value: T) -> result: T pure {
  return move value;
}
"#,
        ],
    },
    RepairPair {
        name: "runtime-linear-slots-binding-nothing.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Slots<Token>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and a runtime-capacity content never leaves it: replace this statement by taking every element out and consuming it, establish `cell.inner.len == 0_u64`, and call `free_empty(window: move cell)` [OP-14]\n"],
        repaired: &[br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Slots<Token>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  let token = take_back(window: &cell.inner);
  let Token(value: byte) = move token;
  free_empty(window: move cell);
  return unit;
}
"#],
    },
    RepairPair {
        name: "runtime-ring-binding-nothing.wf",
        rejected: br#"fn finish() -> result: unit pure {
  let cell = box_ring_new::<u8>(capacity: 2_u64);
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and a runtime-capacity content never leaves it: replace this statement by taking every element out and consuming it, establish `cell.inner.len == 0_u64`, and call `free_empty(window: move cell)` [OP-14]\n"],
        repaired: &[br#"fn finish() -> result: unit pure {
  let cell = box_ring_new::<u8>(capacity: 2_u64);
  free_empty(window: move cell);
  return unit;
}
"#],
    },
    RepairPair {
        name: "runtime-paged-binding-nothing.wf",
        rejected: br#"fn finish() -> result: unit pure {
  let cell = box_paged_new::<u8>(capacity: 2_u64);
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and a runtime-capacity content never leaves it: replace this statement by taking every element out and consuming it, establish `cell.inner.len == 0_u64`, and call `free_empty(window: move cell)` [OP-14]\n"],
        repaired: &[br#"fn finish() -> result: unit pure {
  let cell = box_paged_new::<u8>(capacity: 2_u64);
  free_empty(window: move cell);
  return unit;
}
"#],
    },
    RepairPair {
        name: "runtime-segments-binding-nothing.wf",
        rejected: br#"fn finish(cell: Box<Segments<u8>>) -> result: unit pure {
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and this runtime-capacity content never leaves it: replace this statement by moving `cell` whole into a function that consumes it or returning it\n"],
        repaired: &[
            br#"fn consume(cell: Box<Segments<u8>>) -> result: unit pure {
  return unit;
}

fn finish(cell: Box<Segments<u8>>) -> result: unit pure {
  consume(cell: move cell);
  return unit;
}
"#,
            br#"fn finish(cell: Box<Segments<u8>>) -> result: Box<Segments<u8>> pure {
  return move cell;
}
"#,
        ],
    },
    RepairPair {
        name: "runtime-array-binding-nothing.wf",
        rejected: br#"fn finish(cell: Box<Array<u8>>) -> result: unit pure {
  let Box(..) = move cell;
  return unit;
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and this runtime-capacity content never leaves it: replace this statement by moving `cell` whole into a function that consumes it or returning it\n"],
        repaired: &[
            br#"fn consume(cell: Box<Array<u8>>) -> result: unit pure {
  return unit;
}

fn finish(cell: Box<Array<u8>>) -> result: unit pure {
  consume(cell: move cell);
  return unit;
}
"#,
            br#"fn finish(cell: Box<Array<u8>>) -> result: Box<Array<u8>> pure {
  return move cell;
}
"#,
        ],
    },
];
