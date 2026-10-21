//! [TYPE-11] a struct's type invariants: their formation, a hand-off
//! before a broken invariant is restored, after the identity a debt was
//! restorable through changed, with no relation naming its anchor, after a
//! `swap` moved a value broken below its own level, of an element whose
//! instance is unproved beside a lost position, or of a value an atomic
//! block's target owns, a construction that does not establish one, and an
//! atomic block that leaves one unproved.

use super::RepairPair;

pub(super) const TYPE_INVARIANTS: &[RepairPair] = &[
    RepairPair {
        name: "range-type-invariant-on-a-generic-struct.wf",
        rejected: br#"struct Zeroes<T> {
  cells: Array<u64, 1>;
  invariant(z): forall zero(k in 0_u64..z^.cells.len): z^.cells[k] == 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: declare the invariant on a struct without generics, or state the relation as a `requires` and `ensures` pair on each function that takes the value\n",
        ],
        repaired: &[br#"struct Zeroes {
  cells: Array<u64, 1>;
  invariant(z): forall zero(k in 0_u64..z^.cells.len): z^.cells[k] == 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "type-invariant-over-a-type-parameter-field.wf",
        rejected: br#"struct Pair<T: Int> {
  first: T;
  second: T;
  invariant ordered(pair): pair.first <= pair.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: relate only fields whose declared types name no type parameter, such as `u64` fields, or the `len` of a field whose type names one, such as a `Slots<T, 8>` field\n",
        ],
        repaired: &[
            br#"struct Pair<T: Int> {
  first: u64;
  second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Pair<T: Int> {
  items: Slots<T, 8>;
  used: u64;
  invariant ordered(pair): pair.used <= pair.items.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "propagation-leaves-a-type-invariant-false.wf",
        rejected: br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn make() -> table: Table pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 10_u64);
  let made = Table(slots: move slots, next: 0_u64);
  return move made;
}

fn peek(t: &Table) -> result: u64 reads(t) {
  let at = t^.next;
  let got = t^.slots[at];
  return got;
}

fn step(t: &Table, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(t.next) {
  set t^.next = 99_u64;
  let inner = propagate outcome;
  set t^.next = 0_u64;
  return Ok<u8, unit>(value: inner);
}

fn main() -> status: std::process::ExitStatus pure {
  let table = make();
  let bad = Err<u8, unit>(error: unit);
  let r = step(t: &table, outcome: bad);
  let got = peek(t: &table);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-9",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: `t^.next < t^.slots.len` is not restored since a write that may have broken it: restore it before this hand-off, writing the fields it relates so the body shows it holds, or replace `t^` whole with a value a construction or a call returns\n",
        ],
        repaired: &[br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn make() -> table: Table pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 10_u64);
  let made = Table(slots: move slots, next: 0_u64);
  return move made;
}

fn peek(t: &Table) -> result: u64 reads(t) {
  let at = t^.next;
  let got = t^.slots[at];
  return got;
}

fn step(t: &Table, outcome: Result<u8, unit>) -> result: Result<u8, unit> writes(t.next) {
  set t^.next = 99_u64;
  set t^.next = 0_u64;
  let inner = propagate outcome;
  return Ok<u8, unit>(value: inner);
}

fn main() -> status: std::process::ExitStatus pure {
  let table = make();
  let bad = Err<u8, unit>(error: unit);
  let r = step(t: &table, outcome: bad);
  let got = peek(t: &table);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#],
    },
    RepairPair {
        name: "a-cursor-moves-before-its-write-is-restored.wf",
        rejected: br#"struct Link {
  v: u64;
  next: Option<Box<Link>>;
  invariant small(link): link.v < 16_u64;
}

fn inspect(link: &Link) -> result: u64 reads(link.v) {
  let got = link^.v;
  return got;
}

fn walk() -> result: u64 pure {
  let end = None<Box<Link>>();
  let tail = Link(v: 1_u64, next: move end);
  let boxed = box_new::<Link>(value: move tail);
  let more = Some<Box<Link>>(value: move boxed);
  let head = Link(v: 2_u64, next: move more);
  let cursor = &head;
  for (i in 0_u64..1_u64) {
    set cursor^.v = 31_u64;
    match cursor^.next {
      Some(value: child) => {
        set cursor = &child^.inner;
      }
      None() => {
        break;
      }
    }
  }
  let got = inspect(link: &head);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = walk();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: a write that may have broken a type invariant at or below `head` went through a position or reference that changed before the invariant was restored, so no relation names it any more: restore the invariant before that position or reference changes, or replace `head` whole with a value a construction or a call returns before this hand-off\n",
        ],
        repaired: &[br#"struct Link {
  v: u64;
  next: Option<Box<Link>>;
  invariant small(link): link.v < 16_u64;
}

fn inspect(link: &Link) -> result: u64 reads(link.v) {
  let got = link^.v;
  return got;
}

fn walk() -> result: u64 pure {
  let end = None<Box<Link>>();
  let tail = Link(v: 1_u64, next: move end);
  let boxed = box_new::<Link>(value: move tail);
  let more = Some<Box<Link>>(value: move boxed);
  let head = Link(v: 2_u64, next: move more);
  let cursor = &head;
  for (i in 0_u64..1_u64) {
    set cursor^.v = 31_u64;
    set cursor^.v = 15_u64;
    match cursor^.next {
      Some(value: child) => {
        set cursor = &child^.inner;
      }
      None() => {
        break;
      }
    }
  }
  let got = inspect(link: &head);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = walk();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#],
    },
    RepairPair {
        name: "a-cursor-writes-below-a-hidden-anchor.wf",
        rejected: br#"struct Wrap {
  inner: Link;
  invariant small(outer): outer.inner.v < 16_u64;
}

struct Link {
  v: u64;
  next: Option<Box<Wrap>>;
}

fn inspect(outer: &Wrap) -> result: u64 reads(outer.inner.v) {
  let got = outer^.inner.v;
  return got;
}

fn walk() -> result: u64 pure {
  let end = None<Box<Wrap>>();
  let last = Link(v: 3_u64, next: move end);
  let wrapped = Wrap(inner: move last);
  let boxed = box_new::<Wrap>(value: move wrapped);
  let more = Some<Box<Wrap>>(value: move boxed);
  let first = Link(v: 2_u64, next: move more);
  let top = Wrap(inner: move first);
  let cursor = &top.inner;
  for (i in 0_u64..1_u64) {
    match cursor^.next {
      Some(value: child) => {
        set cursor = &child^.inner.inner;
      }
      None() => {
        break;
      }
    }
  }
  set cursor^.v = 9_u64;
  set top.inner.v = 1_u64;
  let got = inspect(outer: &top);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = walk();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: a write reached `top.inner` through a position or reference that no relation can name any more, so its type invariants cannot be restored: replace `top.inner` whole with a value a construction or a call returns before this hand-off, or make the write through a place the body still names\n",
        ],
        repaired: &[
            br#"struct Wrap {
  inner: Link;
  invariant small(outer): outer.inner.v < 16_u64;
}

struct Link {
  v: u64;
  next: Option<Box<Wrap>>;
}

fn inspect(outer: &Wrap) -> result: u64 reads(outer.inner.v) {
  let got = outer^.inner.v;
  return got;
}

fn walk() -> result: u64 pure {
  let end = None<Box<Wrap>>();
  let last = Link(v: 3_u64, next: move end);
  let wrapped = Wrap(inner: move last);
  let boxed = box_new::<Wrap>(value: move wrapped);
  let more = Some<Box<Wrap>>(value: move boxed);
  let first = Link(v: 2_u64, next: move more);
  let top = Wrap(inner: move first);
  let cursor = &top.inner;
  for (i in 0_u64..1_u64) {
    match cursor^.next {
      Some(value: child) => {
        set cursor = &child^.inner.inner;
      }
      None() => {
        break;
      }
    }
  }
  set cursor^.v = 9_u64;
  let rest = None<Box<Wrap>>();
  let fresh = Link(v: 1_u64, next: move rest);
  set top.inner = move fresh;
  set top.inner.v = 1_u64;
  let got = inspect(outer: &top);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = walk();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
            br#"struct Wrap {
  inner: Link;
  invariant small(outer): outer.inner.v < 16_u64;
}

struct Link {
  v: u64;
  next: Option<Box<Wrap>>;
}

fn inspect(outer: &Wrap) -> result: u64 reads(outer.inner.v) {
  let got = outer^.inner.v;
  return got;
}

fn walk() -> result: u64 pure {
  let end = None<Box<Wrap>>();
  let last = Link(v: 3_u64, next: move end);
  let wrapped = Wrap(inner: move last);
  let boxed = box_new::<Wrap>(value: move wrapped);
  let more = Some<Box<Wrap>>(value: move boxed);
  let first = Link(v: 2_u64, next: move more);
  let top = Wrap(inner: move first);
  let cursor = &top.inner;
  for (i in 0_u64..1_u64) {
    match cursor^.next {
      Some(value: child) => {
        set cursor = &child^.inner.inner;
      }
      None() => {
        break;
      }
    }
  }
  let named = &top.inner;
  set named^.v = 9_u64;
  set top.inner.v = 1_u64;
  let got = inspect(outer: &top);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = walk();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "an-element-is-handed-off-beside-a-lost-position.wf",
        rejected: br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn handed(at: u64) -> result: u64 pure contract {
  requires at < 4_u64;
} {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  for (j in 0_u64..4_u64) {
    set items[j].value = 31_u64;
  }
  let got = lookup(index: &items[at]);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = handed(at: 1_u64);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: `items[at].value < 16_u64` is not proved: a write at a position not known to differ from `items[at]` may have broken it, so prove it before this hand-off, such as by checking or writing the fields it relates\n",
        ],
        repaired: &[
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn handed(at: u64) -> result: u64 pure contract {
  requires at < 4_u64;
} {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  for (j in 0_u64..4_u64) {
    set items[j].value = 31_u64;
  }
  let got = 0_u64;
  if items[at].value < 16_u64 {
    set got = lookup(index: &items[at]);
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = handed(at: 1_u64);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn handed(at: u64) -> result: u64 pure contract {
  requires at < 4_u64;
} {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  for (j in 0_u64..4_u64) {
    set items[j].value = 31_u64;
  }
  let slot = &items[at];
  set slot^.value = 3_u64;
  let got = lookup(index: slot);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = handed(at: 1_u64);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "an-atomic-block-hands-off-an-unproved-field.wf",
        rejected: br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  single: Index;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    set got = lookup(index: &s^.single);
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let initial = State(single: seed);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: `s^.single.value < 16_u64` is not proved: a value an atomic block's target owns carries no type invariant from the block's entry, so prove it before this hand-off, such as by checking the fields it relates, or replace `s^.single` whole with a value a construction or a call returns\n",
        ],
        repaired: &[
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  single: Index;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    if s^.single.value < 16_u64 {
      set got = lookup(index: &s^.single);
    }
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let initial = State(single: seed);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  single: Index;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    set s^.single = Index(value: 3_u64);
    set got = lookup(index: &s^.single);
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let initial = State(single: seed);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "an-atomic-block-hands-off-a-whole-container.wf",
        rejected: br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  items: Array<Index, 4>;
}

fn first(items: &Array<Index, 4>) -> result: u64 reads(items) {
  let got = items^[0_u64].value;
  return got;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    set got = first(items: &s^.items);
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  let initial = State(items: items);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: a value an atomic block's target owns carries no type invariant from the block's entry, and no relation names each one at or below `s^.items`, so they cannot be proved: hand off one field or element whose invariants the body proves, or replace `s^.items` whole with a value a construction or a call returns before this hand-off\n",
        ],
        repaired: &[
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  items: Array<Index, 4>;
}

fn first(items: &Array<Index, 4>) -> result: u64 reads(items) {
  let got = items^[0_u64].value;
  return got;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    if s^.items[0_u64].value < 16_u64 {
      set got = lookup(index: &s^.items[0_u64]);
    }
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  let initial = State(items: items);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

struct State {
  items: Array<Index, 4>;
}

fn first(items: &Array<Index, 4>) -> result: u64 reads(items) {
  let got = items^[0_u64].value;
  return got;
}

fn lookup(index: &Index) -> result: u64 reads(index) {
  let got = index^.value;
  return got;
}

fn peek(state: Shared<State>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic s = &state {
    let seed = Index(value: 3_u64);
    set s^.items = array_filled::<Index, 4>(value: seed);
    set got = first(items: &s^.items);
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let seed = Index(value: 3_u64);
  let items = array_filled::<Index, 4>(value: seed);
  let initial = State(items: items);
  let state = shared_new::<State>(value: initial);
  let first = shared_share::<State>(shared: &state);
  let got = peek(state: move first);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "a-value-broken-below-its-level-is-swapped.wf",
        rejected: br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

nocopy struct Holder {
  index: Index;
}

fn lookup(holder: &Holder) -> result: u64 reads(holder.index.value) {
  let got = holder^.index.value;
  return got;
}

fn exchange() -> result: u64 pure {
  let first = Index(value: 1_u64);
  let h1 = Holder(index: first);
  let second = Index(value: 2_u64);
  let h2 = Holder(index: second);
  set h1.index.value = 31_u64;
  swap(first: &h1, second: &h2);
  set h2.index.value = 3_u64;
  let got = lookup(holder: &h2);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = exchange();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "FN-8",
        sentences: &[
            "]: UnrestoredTypeInvariant\n",
            "\n  mechanical_fix: a write may have broken a type invariant below a value that moved to `h2`, and no relation names the broken place there, so it cannot be restored: restore the invariant before the value moves, or replace `h2` whole with a value a construction or a call returns before this hand-off\n",
        ],
        repaired: &[
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

nocopy struct Holder {
  index: Index;
}

fn lookup(holder: &Holder) -> result: u64 reads(holder.index.value) {
  let got = holder^.index.value;
  return got;
}

fn exchange() -> result: u64 pure {
  let first = Index(value: 1_u64);
  let h1 = Holder(index: first);
  let second = Index(value: 2_u64);
  let h2 = Holder(index: second);
  set h1.index.value = 31_u64;
  set h1.index.value = 3_u64;
  swap(first: &h1, second: &h2);
  let got = lookup(holder: &h2);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = exchange();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
            br#"struct Index {
  value: u64;
  invariant bounded(index): index.value < 16_u64;
}

nocopy struct Holder {
  index: Index;
}

fn lookup(holder: &Holder) -> result: u64 reads(holder.index.value) {
  let got = holder^.index.value;
  return got;
}

fn exchange() -> result: u64 pure {
  let first = Index(value: 1_u64);
  let h1 = Holder(index: first);
  let second = Index(value: 2_u64);
  let h2 = Holder(index: second);
  set h1.index.value = 31_u64;
  swap(first: &h1, second: &h2);
  let third = Index(value: 3_u64);
  set h2 = Holder(index: third);
  let got = lookup(holder: &h2);
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = exchange();
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "type-invariant-on-an-opaque-struct.wf",
        rejected: br#"opaque struct Span {
  first: u64;
  last: u64;
  invariant ordered(span): span.first <= span.last;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: declare the invariant on a struct that is not `opaque`\n",
        ],
        repaired: &[br#"struct Span {
  first: u64;
  last: u64;
  invariant ordered(span): span.first <= span.last;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "type-invariants-share-a-name.wf",
        rejected: br#"struct Pair {
  first: u64;
  second: u64;
  invariant ordered(pair): pair.first <= pair.second;
  invariant ordered(pair): pair.first <= 10_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: give each type invariant of the struct its own name\n",
        ],
        repaired: &[br#"struct Pair {
  first: u64;
  second: u64;
  invariant ordered(pair): pair.first <= pair.second;
  invariant small(pair): pair.first <= 10_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "type-invariant-is-not-a-comparison.wf",
        rejected: br#"struct Pair {
  first: u64;
  second: u64;
  invariant sums(pair): pair.first +defined pair.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: state the invariant as one comparison between two sides, such as `table.next < table.slots.len`; write two invariants for a conjunction\n",
        ],
        repaired: &[br#"struct Pair {
  first: u64;
  second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "type-invariant-with-two-datums-on-a-side.wf",
        rejected: br#"struct Pair {
  first: u64;
  second: u64;
  invariant bounded(pair): pair.first + pair.second <= 10_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: keep one field or measure of the binder on each side, displaced by a constant, such as `table.next + 1_u64 <= table.slots.len`\n",
        ],
        repaired: &[br#"struct Pair {
  first: u64;
  second: u64;
  invariant bounded(pair): pair.first + 1_u64 <= pair.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "type-invariant-names-no-field.wf",
        rejected: br#"struct Pair {
  first: u64;
  second: u64;
  invariant trivial(pair): 1_u64 <= 2_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: InvalidTypeInvariant\n",
            "\n  mechanical_fix: relate at least one field or measure of the binder, such as `table.next`\n",
        ],
        repaired: &[br#"struct Pair {
  first: u64;
  second: u64;
  invariant small(pair): pair.first <= 2_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "construction-refutes-a-type-invariant.wf",
        rejected: br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<u64, 8>();
  let table = Table(slots: move slots, next: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: UndischargedTypeInvariant\n",
            "\n  mechanical_fix: `0_u64 < slots.len` is false for the operands this construction receives: construct the value from operands that satisfy it, or change the statements that fix those operands\n",
        ],
        repaired: &[br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 1_u64);
  let table = Table(slots: move slots, next: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "atomic-block-leaves-a-type-invariant-unproved.wf",
        rejected: br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn fresh() -> table: Table pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 10_u64);
  let made = Table(slots: move slots, next: 0_u64);
  return move made;
}

fn claim(table: Shared<Table>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic t = &table {
    let at = t^.next;
    set got = t^.slots[at];
    let after = at + 1_u64;
    set t^.next = after;
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let initial = fresh();
  let table = shared_new::<Table>(value: move initial);
  let got = claim(table: move table);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#,
        rule: "TYPE-11",
        sentences: &[
            "]: UndischargedTypeInvariant\n",
            "\n  mechanical_fix: `t^.next < t^.slots.len` is not proved where the block leaves the object's state: restore it before this edge, writing the fields it relates so the block shows it holds, or prove it with an `invariant` whose `use` steps name the facts it follows from\n",
        ],
        repaired: &[br#"struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn fresh() -> table: Table pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 10_u64);
  let made = Table(slots: move slots, next: 0_u64);
  return move made;
}

fn claim(table: Shared<Table>) -> result: u64 pure waits {
  let got = 0_u64;
  atomic t = &table {
    let at = t^.next;
    set got = t^.slots[at];
    let after = at + 1_u64;
    if after == t^.slots.len {
      set t^.next = 0_u64;
    } else {
      set t^.next = after;
    }
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let initial = fresh();
  let table = shared_new::<Table>(value: move initial);
  let got = claim(table: move table);
  let code = cvt.wrap::<u64, u8>(got);
  return std::process::exit_status(code: code);
}
"#],
    },
];
