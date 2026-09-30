//! [TYPE-9] content-move repairs selected by shape and drop capability.
//! Each offered alternative is applied literally through the shared repair
//! harness. Retire a pair when its diagnostic or distinct control retires.

use super::RepairPair;

pub(super) const CONTENT_MOVES: &[RepairPair] = &[
    RepairPair {
        name: "droppable-slots-content.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type9-neg-move-runtime-capacity-content.wf"
        ),
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move b.inner` with `move b` and keep the receiving value boxed, accessing its content through `.inner`; if the move was intended only to release the content, remove it and let `b` release at scope exit; to release the window explicitly instead, take every element out and consume it, establish `b.inner.len == 0_u64`, and call `free_empty(window: move b)` [OP-14]\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let b = box_slots_new::<u64>(capacity: 4_u64);
  let n = move b;
  let length = n.inner.len;
  return exit_status(code: 0_u8);
}
"#,
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let b = box_slots_new::<u64>(capacity: 4_u64);
  return exit_status(code: 0_u8);
}
"#,
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let b = box_slots_new::<u64>(capacity: 4_u64);
  free_empty(window: move b);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "linear-ring-content-under-field.wf",
        rejected: br#"nodrop struct Token {
  value: u64;
}

struct Holder {
  queue: Box<Ring<Token>>;
}

fn main() -> status: std::process::ExitStatus pure {
  let queue = box_ring_new::<Token>(capacity: 2_u64);
  let holder = Holder(queue: move queue);
  let token = Token(value: 7_u64);
  place_back(window: &holder.queue.inner, value: move token);
  let content = move holder.queue.inner;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move holder.queue.inner` with `move holder.queue` and keep the receiving value boxed, accessing its content through `.inner`; to release the window explicitly instead, take every element out and consume it, establish `holder.queue.inner.len == 0_u64`, and call `free_empty(window: move holder.queue)` [OP-14]\n",
        ],
        repaired: &[
            br#"nodrop struct Token {
  value: u64;
}

struct Holder {
  queue: Box<Ring<Token>>;
}

fn main() -> status: std::process::ExitStatus pure {
  let queue = box_ring_new::<Token>(capacity: 2_u64);
  let holder = Holder(queue: move queue);
  let token = Token(value: 7_u64);
  place_back(window: &holder.queue.inner, value: move token);
  let content = move holder.queue;
  let taken = take_back(window: &content.inner);
  let Token(value: value) = move taken;
  free_empty(window: move content);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"nodrop struct Token {
  value: u64;
}

struct Holder {
  queue: Box<Ring<Token>>;
}

fn main() -> status: std::process::ExitStatus pure {
  let queue = box_ring_new::<Token>(capacity: 2_u64);
  let holder = Holder(queue: move queue);
  let token = Token(value: 7_u64);
  place_back(window: &holder.queue.inner, value: move token);
  let taken = take_back(window: &holder.queue.inner);
  let Token(value: value) = move taken;
  free_empty(window: move holder.queue);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "droppable-array-content.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let data = box_array_filled::<u8>(count: 2_u64, value: 7_u8);
  let content = move data.inner;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move data.inner` with `move data` and keep the receiving value boxed, accessing its content through `.inner`; if the move was intended only to release the content, remove it and let `data` release at scope exit\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let data = box_array_filled::<u8>(count: 2_u64, value: 7_u8);
  let content = move data;
  let length = content.inner.len;
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let data = box_array_filled::<u8>(count: 2_u64, value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "droppable-segments-content.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type9-neg-segments-move-content.wf"
        ),
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move cell.inner` with `move cell` and keep the receiving value boxed, accessing its content through `.inner`; if the move was intended only to release the content, remove it and let `cell` release at scope exit\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let lengths = box_array_filled::<u64>(count: 2_u64, value: 1_u64);
  let made = box_segments_filled::<u64>(lengths: &lengths.inner[0_u64..2_u64], value: 0_u64);
  match move made {
    None() => {
      return exit_status(code: 1_u8);
    }
    Some(value: cell) => {
      let taken = move cell;
      let length = taken.inner.len;
      return exit_status(code: 0_u8);
    }
  }
}
"#,
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let lengths = box_array_filled::<u64>(count: 2_u64, value: 1_u64);
  let made = box_segments_filled::<u64>(lengths: &lengths.inner[0_u64..2_u64], value: 0_u64);
  match move made {
    None() => {
      return exit_status(code: 1_u8);
    }
    Some(value: cell) => {
      return exit_status(code: 0_u8);
    }
  }
}
"#,
        ],
    },
    RepairPair {
        name: "unbounded-array-content.wf",
        rejected: br#"fn keep<T>(cell: Box<Array<T>>) -> result: Box<Array<T>> pure {
  return move cell.inner;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move cell.inner` with `move cell` and keep the receiving value boxed, accessing its content through `.inner`\n",
        ],
        repaired: &[
            br#"fn keep<T>(cell: Box<Array<T>>) -> result: Box<Array<T>> pure {
  return move cell;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "unbounded-segments-content.wf",
        rejected: br#"fn keep<T>(cell: Box<Segments<T>>) -> result: Box<Segments<T>> pure {
  return move cell.inner;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move cell.inner` with `move cell` and keep the receiving value boxed, accessing its content through `.inner`\n",
        ],
        repaired: &[
            br#"fn keep<T>(cell: Box<Segments<T>>) -> result: Box<Segments<T>> pure {
  return move cell;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "drop-bound-slots-content.wf",
        rejected: br#"fn release<T: drop>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  let content = move cell.inner;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move cell.inner` with `move cell` and keep the receiving value boxed, accessing its content through `.inner`; if the move was intended only to release the content, remove it and let `cell` release at scope exit; to release the window explicitly instead, take every element out and consume it, establish `cell.inner.len == 0_u64`, and call `free_empty(window: move cell)` [OP-14]\n",
        ],
        repaired: &[
            br#"fn release<T: drop>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  let content = move cell;
  free_empty(window: move content);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn release<T: drop>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn release<T: drop>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  free_empty(window: move cell);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "unbounded-slots-content.wf",
        rejected: br#"fn release<T>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  let content = move cell.inner;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-9",
        sentences: &[
            "\n  mechanical_fix: replace `move cell.inner` with `move cell` and keep the receiving value boxed, accessing its content through `.inner`; to release the window explicitly instead, take every element out and consume it, establish `cell.inner.len == 0_u64`, and call `free_empty(window: move cell)` [OP-14]\n",
        ],
        repaired: &[
            br#"fn release<T>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  let content = move cell;
  free_empty(window: move content);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn release<T>(cell: Box<Slots<T>>) -> result: unit pure contract {
  requires cell.inner.len == 0_u64;
} {
  free_empty(window: move cell);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
