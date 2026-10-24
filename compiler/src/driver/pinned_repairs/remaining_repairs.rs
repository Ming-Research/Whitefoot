//! Constructor, ownership, placement and scope-exit repairs, carried out
//! literally. Keep each source with its repair while that repair is printed.

use super::RepairPair;

pub(super) const REMAINING_REPAIRS: &[RepairPair] = &[
    RepairPair {
        name: "ring-constructor.wf",
        rejected: br#"fn make() -> result: Ring<u8, 2> pure {
  return Ring<u8, 2>();
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Ring<u8, 2> pure {
  return ring_new::<u8, 2>();
}
"#],
    },
    RepairPair {
        name: "paged-constructor.wf",
        rejected: br#"fn make() -> result: Box<Paged<u8>> pure {
  return Paged<u8>();
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Box<Paged<u8>> pure {
  return box_paged_new::<u8>(capacity: 2_u64);
}
"#],
    },
    RepairPair {
        name: "segments-constructor.wf",
        rejected: br#"fn make() -> result: Box<Segments<u8>> pure {
  return Segments<u8>();
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Box<Segments<u8>> pure {
  let lengths = array_filled::<u64, 2>(value: 3_u64);
  return box_segments_filled::<u8>(lengths: &lengths[0_u64..2_u64], value: 0_u8);
}
"#],
    },
    RepairPair {
        name: "joined-argument-overlaps-own-row.wf",
        rejected: br#"struct Node {
  byte: u8;
  next: Option<Box<Node>>;
}

fn touch(root: &Box<Node>) -> result: unit reads(root.inner.next), writes(root.inner.byte) {
  set root^.inner.byte = 0_u8;
  match &root^.inner.next {
    Some(value: unused) => {
      return unit;
    }
    None() => {
      return unit;
    }
  }
}

fn select(root: &Box<Node>, flag: Bool) -> result: unit writes(root.inner) {
  match &root^.inner.next {
    Some(value: child) => {
      let selected = if flag {
        give root;
      } else {
        give child;
      }
      touch(root: selected);
      return unit;
    }
    None() => {
      return unit;
    }
  }
}
"#,
        rule: "EFF-5",
        sentences: &["\n  mechanical_fix: these two entries of the callee's row may reach overlapping places through one argument, and no position this call passes separates them: replace the callee's row entries at or below their common path with one `writes` entry of that path\n"],
        repaired: &[br#"struct Node {
  byte: u8;
  next: Option<Box<Node>>;
}

fn touch(root: &Box<Node>) -> result: unit writes(root.inner) {
  set root^.inner.byte = 0_u8;
  match &root^.inner.next {
    Some(value: unused) => {
      return unit;
    }
    None() => {
      return unit;
    }
  }
}

fn select(root: &Box<Node>, flag: Bool) -> result: unit writes(root.inner) {
  match &root^.inner.next {
    Some(value: child) => {
      let selected = if flag {
        give root;
      } else {
        give child;
      }
      touch(root: selected);
      return unit;
    }
    None() => {
      return unit;
    }
  }
}
"#],
    },
    RepairPair {
        name: "unnamed-row-positions.wf",
        rejected: br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit reads(window.len), writes(window[first]), writes(window[second]) {
  let length = window^.len;
  if first < length {
    if second < length {
      set window^[first] = 1_u8;
      set window^[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let indices = array_filled::<u64, 2>(value: 0_u64);
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  write_two(window: &window, first: indices[0_u64], second: indices[1_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &["\n  mechanical_fix: these two entries of the callee's row may reach overlapping places through one argument, and no fact names the positions this call passes for them: replace the callee's row entries at or below their common path with one `writes` entry of that path\n"],
        repaired: &[br#"fn write_two(window: &Slots<u8, 2>, first: u64, second: u64) -> result: unit writes(window) {
  let length = window^.len;
  if first < length {
    if second < length {
      set window^[first] = 1_u8;
      set window^[second] = 2_u8;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let indices = array_filled::<u64, 2>(value: 0_u64);
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  write_two(window: &window, first: indices[0_u64], second: indices[1_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "linear-fixed-boxed-slots-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Slots<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `cell` whole into a function that consumes it, or return it\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(cell: Box<Slots<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  let window = move cell.inner;
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}

fn finish(cell: Box<Slots<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  consume(cell: move cell);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Slots<Token, 2>>) -> result: Box<Slots<Token, 2>> pure contract {
  requires cell.inner.len == 1_u64;
} {
  return move cell;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-fixed-boxed-ring-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Ring<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `cell` whole into a function that consumes it, or return it\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(cell: Box<Ring<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  let window = move cell.inner;
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}

fn finish(cell: Box<Ring<Token, 2>>) -> result: unit pure contract {
  requires cell.inner.len == 1_u64;
} {
  consume(cell: move cell);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(cell: Box<Ring<Token, 2>>) -> result: Box<Ring<Token, 2>> pure contract {
  requires cell.inner.len == 1_u64;
} {
  return move cell;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-inline-fixed-slots-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Slots<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `window` whole into a function that consumes it, or return it; or take every element out and consume it, prove its length is zero, and call `free_empty(window: move window)`\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(window: Slots<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}

fn finish(window: Slots<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  consume(window: move window);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Slots<Token, 2>) -> result: Slots<Token, 2> pure contract {
  requires window.len == 1_u64;
} {
  return move window;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Slots<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-inline-fixed-ring-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Ring<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `window` whole into a function that consumes it, or return it; or take every element out and consume it, prove its length is zero, and call `free_empty(window: move window)`\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn consume(window: Ring<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}

fn finish(window: Ring<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  consume(window: move window);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Ring<Token, 2>) -> result: Ring<Token, 2> pure contract {
  requires window.len == 1_u64;
} {
  return move window;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn finish(window: Ring<Token, 2>) -> result: unit pure contract {
  requires window.len == 1_u64;
} {
  let token = take_back(window: &window);
  let Token(value: byte) = move token;
  free_empty(window: move window);
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-boxed-window-at-exit.wf",
        rejected: br#"fn pass<T>(window: Box<Slots<T>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
} {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `window` whole into a function that consumes it, or return it; or take every element out and consume it, prove its length is zero, and call `free_empty(window: move window)`\n"],
        repaired: &[
            br#"fn consume<T>(window: Box<Slots<T>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
} {
  free_empty(window: move window);
  return unit;
}

fn pass<T>(window: Box<Slots<T>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
} {
  consume::<T>(window: move window);
  return unit;
}
"#,
            br#"fn pass<T>(window: Box<Slots<T>>) -> result: Box<Slots<T>> pure contract {
  requires window.inner.len == 0_u64;
} {
  return move window;
}
"#,
            br#"fn pass<T>(window: Box<Slots<T>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
} {
  free_empty(window: move window);
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "array-constructor.wf",
        rejected: br#"fn make() -> result: Array<u8, 2> pure {
  return Array<u8, 2>();
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Array<u8, 2> pure {
  return array_filled::<u8, 2>(value: 0_u8);
}
"#],
    },
    RepairPair {
        name: "slots-constructor.wf",
        rejected: br#"fn make() -> result: Slots<u8, 2> pure {
  return Slots<u8, 2>();
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Slots<u8, 2> pure {
  return slots_new::<u8, 2>();
}
"#],
    },
    RepairPair {
        name: "box-constructor.wf",
        rejected: br#"fn make() -> result: Box<u8> pure {
  return Box<u8>(inner: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &["\n  mechanical_fix: build it with a construction function [OP-13]\n"],
        repaired: &[br#"fn make() -> result: Box<u8> pure {
  return box_new::<u8>(value: 0_u8);
}
"#],
    },
    RepairPair {
        name: "bare-affine-place.wf",
        rejected: br#"nocopy struct Parcel {
  value: u8;
}

fn pass(p: Parcel) -> result: Parcel pure {
  return p;
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  binding: p\n",
            "\n  ty: Parcel\n",
            "\n  reason: `p` has type Parcel; its type Parcel is declared non-copy\n",
            "\n  mechanical_fix: write `move p`\n",
        ],
        repaired: &[br#"nocopy struct Parcel {
  value: u8;
}

fn pass(p: Parcel) -> result: Parcel pure {
  return move p;
}
"#],
    },
    RepairPair {
        name: "copy-place-moved.wf",
        rejected: br#"fn pass(p: u8) -> result: u8 pure {
  return move p;
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  binding: p\n",
            "\n  ty: u8\n",
            "\n  reason: `p` has type u8; every part is copy\n",
            "\n  mechanical_fix: write `p` without `move`\n",
        ],
        repaired: &[br#"fn pass(p: u8) -> result: u8 pure {
  return p;
}
"#],
    },
    RepairPair {
        name: "runtime-slots-inline.wf",
        rejected: br#"fn make() -> result: Slots<u8> pure {
  return slots_new::<u8, 2>();
}
"#,
        rule: "TYPE-9",
        sentences: &["\n  mechanical_fix: wrap it in a Box, or write the constant-capacity form\n"],
        repaired: &[
            br#"fn make() -> result: Box<Slots<u8>> pure {
  return box_slots_new::<u8>(capacity: 2_u64);
}
"#,
            br#"fn make() -> result: Slots<u8, 2> pure {
  return slots_new::<u8, 2>();
}
"#,
        ],
    },
    RepairPair {
        name: "type9-neg-segments-parameter.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/type9-neg-segments-parameter.wf"),
        rule: "TYPE-9",
        sentences: &["\n  mechanical_fix: wrap it in a Box and pass a reference to its content\n"],
        repaired: &[br#"fn count(segments: &Segments<u64>) -> result: u64 reads(segments.len) {
  return segments^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let lengths = array_filled::<u64, 2>(value: 3_u64);
  let segments = box_segments_filled::<u64>(lengths: &lengths[0_u64..2_u64], value: 7_u64);
  let n = count(segments: &segments.inner);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "paged-inline-parameter.wf",
        rejected: br#"fn count(values: Paged<u64>) -> result: u64 pure {
  return 0_u64;
}
"#,
        rule: "TYPE-9",
        sentences: &["\n  mechanical_fix: wrap it in a Box and pass a reference to its content\n"],
        repaired: &[br#"fn count(values: &Paged<u64>) -> result: u64 reads(values.len) {
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_paged_new::<u64>(capacity: 2_u64);
  let n = count(values: &values.inner);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "linear-type-parameter-at-exit.wf",
        rejected: br#"fn pass<T>(value: T) -> result: unit pure {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `value` whole into a function that consumes it, or return it\n"],
        repaired: &[
            br#"fn sink<T>(value: T) -> result: T pure {
  return move value;
}

fn pass<T>(value: T) -> result: T pure {
  return sink::<T>(value: move value);
}
"#,
            br#"fn pass<T>(value: T) -> result: T pure {
  return move value;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-host-handle-at-exit.wf",
        rejected: br#"fn finish(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> result: unit pure {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: when its module provides a consuming close or finish function, move `file` whole into it; for a `ReadFile`, call `std::fs::close_read` with `file: move file` and a `HandleFactory` reference as `factory`, declaring that reference's write effect and marking the function `waits`; otherwise move `file` whole into a function that consumes it, or return it\n"],
        repaired: &[
            br#"fn finish(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> result: unit writes(factory) waits {
  let closed = std::fs::close_read(factory: factory, file: move file);
  return unit;
}
"#,
            br#"fn finish(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> result: std::fs::ReadFile pure {
  return move file;
}
"#,
            br#"fn consume(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> result: unit writes(factory) waits {
  let closed = std::fs::close_read(factory: factory, file: move file);
  return unit;
}

fn finish(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> result: unit writes(factory) waits {
  consume(factory: factory, file: move file);
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-enum-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

enum Package {
  First(token: Token);
  Second(token: Token);
}

fn pass(value: Package) -> result: unit pure {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `value` whole into a function that consumes it, or return it; or take it apart with `match move value` and consume every linear payload in each arm\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

enum Package {
  First(token: Token);
  Second(token: Token);
}

fn sink(value: Package) -> result: Package pure {
  return move value;
}

fn pass(value: Package) -> result: Package pure {
  return sink(value: move value);
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

enum Package {
  First(token: Token);
  Second(token: Token);
}

fn pass(value: Package) -> result: Package pure {
  return move value;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

enum Package {
  First(token: Token);
  Second(token: Token);
}

fn pass(value: Package) -> result: unit pure {
  match move value {
    First(token: first) => {
      let Token(value: a) = move first;
    }
    Second(token: second) => {
      let Token(value: b) = move second;
    }
  }
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "linear-struct-at-exit.wf",
        rejected: br#"nodrop struct Token {
  value: u8;
}

fn pass(value: Token) -> result: unit pure {
  return unit;
}
"#,
        rule: "PROV-6",
        sentences: &["\n  mechanical_fix: move `value` whole into a function that consumes it, or return it; or destructure the whole struct with `let N(f: a, ...) = move value;`, replacing `N` and `f` with its declared struct and field names and consuming every linear field\n"],
        repaired: &[
            br#"nodrop struct Token {
  value: u8;
}

fn sink(value: Token) -> result: unit pure {
  let Token(value: taken) = move value;
  return unit;
}

fn pass(value: Token) -> result: unit pure {
  sink(value: move value);
  return unit;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn pass(value: Token) -> result: Token pure {
  return move value;
}
"#,
            br#"nodrop struct Token {
  value: u8;
}

fn pass(value: Token) -> result: unit pure {
  let Token(value: taken) = move value;
  return unit;
}
"#,
        ],
    },
    RepairPair {
        name: "propagate-postcondition-unproved.wf",
        rejected: br#"fn step(place: &u64, replacement: u64, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(place) contract {
  ensures place^ <= 8_u64;
} {
  set place^ = replacement;
  let value = propagate outcome;
  set place^ = 0_u64;
  return Ok<u8, unit>(value: value);
}
"#,
        rule: "FN-9",
        sentences: &["\n  mechanical_fix: the postcondition is not proved where this `propagate` leaves the function: when its related places admit these writes, establish it before the `propagate` by writing values that satisfy it into those places; or, for a bound on a writable integer place without a subscript, when available facts prove a suitable value's bound, bind that value with an ordinary `let`, prove the bound over that binding with an `invariant` (adding `use` steps only when AUTO does not already prove its target), and assign the binding to the place before the `propagate`, keeping every place the relation reads unchanged through the early return; or state a postcondition every exit satisfies\n"],
        repaired: &[
            br#"fn step(place: &u64, replacement: u64, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(place) contract {
  ensures place^ <= 8_u64;
} {
  set place^ = replacement;
  set place^ = 0_u64;
  let value = propagate outcome;
  return Ok<u8, unit>(value: value);
}
"#,
            br#"fn step(place: &u64, replacement: u64, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(place) contract {
  requires replacement <= 8_u64;
  ensures place^ <= 8_u64;
} {
  set place^ = replacement;
  let snapshot = place^;
  invariant ready: snapshot <= 8_u64;
  set place^ = snapshot;
  let value = propagate outcome;
  set place^ = 0_u64;
  return Ok<u8, unit>(value: value);
}
"#,
            br#"fn step(place: &u64, replacement: u64, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(place) contract {
  ensures place^ <= 18446744073709551615_u64;
} {
  set place^ = replacement;
  let value = propagate outcome;
  set place^ = 0_u64;
  return Ok<u8, unit>(value: value);
}
"#,
        ],
    },
    RepairPair {
        // [ENT-2] a field below an element is a clause (b) term only when
        // every offset is a clause (a) or (c) term; an element-read offset
        // is neither, so the endpoint is bound first.
        name: "writable-len-field-at-an-element-offset-endpoint.wf",
        rejected: br#"struct Span {
  len: u64;
}

fn total(spans: &Array<Span, 2>, slots: &Array<u64, 2>) -> result: u64 reads(spans), reads(slots) contract {
  requires slots^[0_u64] < 2_u64;
} {
  let count = 0_u64;
  for (i in 0_u64..spans^[slots^[0_u64]].len) {
    set count = count +wrap 1_u64;
  }
  return count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "ENT-2",
        sentences: &[
            "\n  mechanical_fix: bind the computed u64 value with one preceding ordinary let and use that term as the endpoint\n",
        ],
        repaired: &[br#"struct Span {
  len: u64;
}

fn total(spans: &Array<Span, 2>, slots: &Array<u64, 2>) -> result: u64 reads(spans), reads(slots) contract {
  requires slots^[0_u64] < 2_u64;
} {
  let count = 0_u64;
  let length = spans^[slots^[0_u64]].len;
  for (i in 0_u64..length) {
    set count = count +wrap 1_u64;
  }
  return count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

/// The public-field repair is selected in an interface, which a single
/// source cannot spell [MOD-5]. Both alternatives use the same RepairPair
/// with the other module records retained, and the same contradiction oracle.
#[test]
fn public_type_invariant_fields_have_working_repairs() {
    use crate::driver::{CompilationFailureKind, CompilerLimits};
    use crate::{SourceInput, SourceRole};

    let pair = RepairPair {
        name: "pairs/module.wfm",
        rejected: include_bytes!("../../../../tests/conformance/cases/type11-neg-writable-public-field/pairs/module.wfm"),
        rule: "TYPE-11",
        sentences: &["\n  mechanical_fix: write the field `public readonly`, so that code outside the declaring module reads it and only that module writes it, or make it private\n"],
        repaired: &[
            br#"public struct Pair {
  public readonly first: u64;
  public readonly second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}
"#,
            br#"public struct Pair {
  first: u64;
  public readonly second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}
"#,
        ],
    };
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::pairs: [];\npkg: [pkg::pairs, std::process];\n\nentry app = pkg::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the fixture graph is well formed");
    let module = |path: &[&str]| {
        graph
            .modules()
            .iter()
            .position(|module| module.path() == path)
            .and_then(crate::ModuleId::from_index)
            .expect("the fixture module is registered")
    };
    let root = module(&[]);
    let pairs = module(&["pairs"]);
    let supporting = [
        SourceInput::new(
            "module.wfm",
            b"public fn main() -> status: std::process::ExitStatus pure doc \"Runs the fixture.\";\n",
        )
        .in_module(root, SourceRole::Interface),
        SourceInput::new(
            "main.wf",
            b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        )
        .in_module(root, SourceRole::Implementation),
    ];
    let inputs = |source| {
        let mut inputs = supporting.to_vec();
        inputs.push(SourceInput::new(pair.name, source).in_module(pairs, SourceRole::Interface));
        inputs
    };
    let failure = crate::driver::check_module_program(
        &graph,
        &inputs(pair.rejected),
        CompilerLimits::default(),
    )
    .expect_err("the public writable field is refused");
    let rendered = format!("{failure}\n");
    assert_eq!(
        failure.kind(),
        CompilationFailureKind::Source,
        "{}: {rendered}",
        pair.name
    );
    assert_eq!(failure.rule_id(), Some(pair.rule), "{}: {rendered}", pair.name);
    for sentence in pair.sentences {
        assert!(rendered.contains(sentence), "{rendered}");
    }
    for source in pair.repaired {
        let inputs = inputs(source);
        crate::driver::check_module_program(&graph, &inputs, CompilerLimits::default())
            .expect("each field repair is accepted in its module");
        let inputs = crate::driver::with_library_records(&graph, &inputs);
        let contradictions = crate::driver::with_checked_program(
            &inputs,
            Some(graph.modules()),
            CompilerLimits::default(),
            |checked, _| Ok(checked.contradictory_successes(&["main".to_owned()])),
        )
        .expect("the repaired composition is accepted");
        assert!(contradictions.is_empty(), "{contradictions:?}");
    }
}

/// A foreign private field admits whole ownership transfer, but no source
/// repair that binds that field in this module [MOD-5].
#[test]
fn foreign_private_linear_fields_are_not_destructuring_repairs() {
    use crate::driver::{CompilationFailureKind, CompilerLimits};
    use crate::{SourceInput, SourceRole};

    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::parts: [];\npkg: [pkg::parts];\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the fixture graph is well formed");
    let module = |path: &[&str]| {
        graph
            .modules()
            .iter()
            .position(|module| module.path() == path)
            .and_then(crate::ModuleId::from_index)
            .expect("the fixture module is registered")
    };
    let root = module(&[]);
    let parts = module(&["parts"]);
    for (interface, body, pair) in [
        (
            br#"public nodrop struct Token {
  value: u8;
}

public fn consume(value: Token) -> result: unit pure doc "Consumes the token.";
"#
            .as_slice(),
            br#"fn consume(value: Token) -> result: unit pure {
  let Token(value: byte) = move value;
  return unit;
}
"#
            .as_slice(),
            RepairPair {
                name: "private-struct.wf",
                rejected: br#"fn finish(value: pkg::parts::Token) -> result: unit pure {
  return unit;
}
"#,
                rule: "PROV-6",
                sentences: &["\n  mechanical_fix: move `value` whole into a function that consumes it, or return it\n"],
                repaired: &[
                    br#"fn finish(value: pkg::parts::Token) -> result: unit pure {
  pkg::parts::consume(value: move value);
  return unit;
}
"#,
                    br#"fn finish(value: pkg::parts::Token) -> result: pkg::parts::Token pure {
  return move value;
}
"#,
                ],
            },
        ),
        (
            br#"public enum Parcel {
  Full(token: Token);
}

public nodrop struct Token {
  value: u8;
}

public fn consume(value: Parcel) -> result: unit pure doc "Consumes the parcel.";
"#
            .as_slice(),
            br#"fn consume(value: Parcel) -> result: unit pure {
  match move value {
    Full(token: carried_token) => {
      let Token(value: byte) = move carried_token;
    }
  }
  return unit;
}
"#
            .as_slice(),
            RepairPair {
                name: "private-enum.wf",
                rejected: br#"fn finish(value: pkg::parts::Parcel) -> result: unit pure {
  return unit;
}
"#,
                rule: "PROV-6",
                sentences: &["\n  mechanical_fix: move `value` whole into a function that consumes it, or return it\n"],
                repaired: &[
                    br#"fn finish(value: pkg::parts::Parcel) -> result: unit pure {
  pkg::parts::consume(value: move value);
  return unit;
}
"#,
                    br#"fn finish(value: pkg::parts::Parcel) -> result: pkg::parts::Parcel pure {
  return move value;
}
"#,
                ],
            },
        ),
    ] {
        let inputs = |source| {
            vec![
                SourceInput::new("module.wfm", b"\n").in_module(root, SourceRole::Interface),
                SourceInput::new("parts/module.wfm", interface)
                    .in_module(parts, SourceRole::Interface),
                SourceInput::new("parts/consume.wf", body)
                    .in_module(parts, SourceRole::Implementation),
                SourceInput::new(pair.name, source).in_module(root, SourceRole::Implementation),
            ]
        };
        let failure = crate::driver::check_module_program(
            &graph,
            &inputs(pair.rejected),
            CompilerLimits::default(),
        )
        .expect_err("the linear value is unconsumed");
        let rendered = format!("{failure}\n");
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{}: {rendered}",
            pair.name
        );
        assert_eq!(failure.rule_id(), Some(pair.rule), "{}: {rendered}", pair.name);
        for sentence in pair.sentences {
            assert!(rendered.contains(sentence), "{rendered}");
        }
        for source in pair.repaired {
            let inputs = inputs(source);
            crate::driver::check_module_program(&graph, &inputs, CompilerLimits::default())
                .expect("whole ownership transfer respects field privacy");
            let inputs = crate::driver::with_library_records(&graph, &inputs);
            let contradictions = crate::driver::with_checked_program(
                &inputs,
                Some(graph.modules()),
                CompilerLimits::default(),
                |checked, _| {
                    Ok(checked.contradictory_successes(&[
                        "finish".to_owned(),
                        "consume".to_owned(),
                    ]))
                },
            )
            .expect("the repaired composition is accepted");
            assert!(contradictions.is_empty(), "{contradictions:?}");
        }
    }
}
