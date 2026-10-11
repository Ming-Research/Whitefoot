//! Recursive EFF-2 repairs, applied as exact row substitutions by the
//! shared DIAG-1 harness. Keep these pairs while this diagnostic exists.

use super::RepairPair;

pub(super) const RECURSIVE_EFFECTS: &[RepairPair] = &[
    // The firn-reported Frozen shape remains a stability regression: reference
    // match dispatch already exhibits reads(root.inner), including on main.
    RepairPair {
        name: "recursive-frozen-row.wf",
        rejected: br#"enum Node {
  Empty();
  Leaf(key: u64, byte: u8);
  Branch(zero: Frozen<Node>, one: Frozen<Node>);
}

fn map_lookup_at(root: &Frozen<Node>, key: u64, mask: u64) -> result: Option<u8> reads(root.inner.Leaf.key), reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Empty() => {
      return None<u8>();
    }
    Leaf(key: found_key, byte: found_byte) => {
      if found_key^ == key {
        return Some<u8>(value: found_byte^);
      } else {
        return None<u8>();
      }
    }
    Branch(zero: zero_child, one: one_child) => {
      let selected = iand(key, mask);
      let next_mask = ishr(mask, 1_u32);
      if selected == 0_u64 {
        return map_lookup_at(root: zero_child, key: key, mask: next_mask);
      } else {
        return map_lookup_at(root: one_child, key: key, mask: next_mask);
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let zero_value = Node::Leaf(key: 0_u64, byte: 7_u8);
  let zero_node = frozen_new::<Node>(value: move zero_value);
  let one_value = Node::Leaf(key: 1_u64, byte: 9_u8);
  let one_node = frozen_new::<Node>(value: move one_value);
  let branch_value = Node::Branch(zero: move zero_node, one: move one_node);
  let root_node = frozen_new::<Node>(value: move branch_value);
  let found = map_lookup_at(root: &root_node, key: 0_u64, mask: 1_u64);
  match found {
    Some(value: found_value) => {
      return std::process::exit_status(code: found_value);
    }
    None() => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"enum Node {
  Empty();
  Leaf(key: u64, byte: u8);
  Branch(zero: Frozen<Node>, one: Frozen<Node>);
}

fn map_lookup_at(root: &Frozen<Node>, key: u64, mask: u64) -> result: Option<u8> reads(root.inner) {
  match &root^.inner {
    Empty() => {
      return None<u8>();
    }
    Leaf(key: found_key, byte: found_byte) => {
      if found_key^ == key {
        return Some<u8>(value: found_byte^);
      } else {
        return None<u8>();
      }
    }
    Branch(zero: zero_child, one: one_child) => {
      let selected = iand(key, mask);
      let next_mask = ishr(mask, 1_u32);
      if selected == 0_u64 {
        return map_lookup_at(root: zero_child, key: key, mask: next_mask);
      } else {
        return map_lookup_at(root: one_child, key: key, mask: next_mask);
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let zero_value = Node::Leaf(key: 0_u64, byte: 7_u8);
  let zero_node = frozen_new::<Node>(value: move zero_value);
  let one_value = Node::Leaf(key: 1_u64, byte: 9_u8);
  let one_node = frozen_new::<Node>(value: move one_value);
  let branch_value = Node::Branch(zero: move zero_node, one: move one_node);
  let root_node = frozen_new::<Node>(value: move branch_value);
  let found = map_lookup_at(root: &root_node, key: 0_u64, mask: 1_u64);
  match found {
    Some(value: found_value) => {
      return std::process::exit_status(code: found_value);
    }
    None() => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#],
    },
    // Box preserves the same dispatch-covered row as Frozen; neither pair
    // depends on recursive read widening to obtain reads(root.inner).
    RepairPair {
        name: "recursive-box-row.wf",
        rejected: br#"enum Node {
  Empty();
  Leaf(key: u64, byte: u8);
  Branch(zero: Box<Node>, one: Box<Node>);
}

fn map_lookup_at(root: &Box<Node>, key: u64, mask: u64) -> result: Option<u8> reads(root.inner.Leaf.key), reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Empty() => {
      return None<u8>();
    }
    Leaf(key: found_key, byte: found_byte) => {
      if found_key^ == key {
        return Some<u8>(value: found_byte^);
      } else {
        return None<u8>();
      }
    }
    Branch(zero: zero_child, one: one_child) => {
      let selected = iand(key, mask);
      let next_mask = ishr(mask, 1_u32);
      if selected == 0_u64 {
        return map_lookup_at(root: zero_child, key: key, mask: next_mask);
      } else {
        return map_lookup_at(root: one_child, key: key, mask: next_mask);
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let zero_value = Node::Leaf(key: 0_u64, byte: 7_u8);
  let zero_node = box_new::<Node>(value: move zero_value);
  let one_value = Node::Leaf(key: 1_u64, byte: 9_u8);
  let one_node = box_new::<Node>(value: move one_value);
  let branch_value = Node::Branch(zero: move zero_node, one: move one_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = map_lookup_at(root: &root_node, key: 0_u64, mask: 1_u64);
  match found {
    Some(value: found_value) => {
      return std::process::exit_status(code: found_value);
    }
    None() => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"enum Node {
  Empty();
  Leaf(key: u64, byte: u8);
  Branch(zero: Box<Node>, one: Box<Node>);
}

fn map_lookup_at(root: &Box<Node>, key: u64, mask: u64) -> result: Option<u8> reads(root.inner) {
  match &root^.inner {
    Empty() => {
      return None<u8>();
    }
    Leaf(key: found_key, byte: found_byte) => {
      if found_key^ == key {
        return Some<u8>(value: found_byte^);
      } else {
        return None<u8>();
      }
    }
    Branch(zero: zero_child, one: one_child) => {
      let selected = iand(key, mask);
      let next_mask = ishr(mask, 1_u32);
      if selected == 0_u64 {
        return map_lookup_at(root: zero_child, key: key, mask: next_mask);
      } else {
        return map_lookup_at(root: one_child, key: key, mask: next_mask);
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let zero_value = Node::Leaf(key: 0_u64, byte: 7_u8);
  let zero_node = box_new::<Node>(value: move zero_value);
  let one_value = Node::Leaf(key: 1_u64, byte: 9_u8);
  let one_node = box_new::<Node>(value: move one_value);
  let branch_value = Node::Branch(zero: move zero_node, one: move one_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = map_lookup_at(root: &root_node, key: 0_u64, mask: 1_u64);
  match found {
    Some(value: found_value) => {
      return std::process::exit_status(code: found_value);
    }
    None() => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#],
    },
    // An empty child window gives the recursive node a finite leaf. The
    // forwarded formal index keeps the child path statically nameable, while
    // the descriptor read children.len cannot cover children[index].
    // Main's observed repair prints ordinals below the index, so applying it
    // literally fails FORM-5. Even with those names restored, substituting its
    // unwidened row would project another child suffix on the next check.
    RepairPair {
        name: "recursive-indexed-read-row.wf",
        rejected: br#"struct Node {
  byte: u8;
  children: Slots<Box<Node>, 1>;
}

fn sum_path(root: &Box<Node>, index: u64) -> result: u8 reads(root.inner.byte), reads(root.inner.children.len) {
  let byte = root^.inner.byte;
  if index < root^.inner.children.len {
    let child = &root^.inner.children[index];
    let suffix = sum_path(root: child, index: index);
    let total = byte +wrap suffix;
    return total;
  } else {
    return byte;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_children = slots_new::<Box<Node>, 1>();
  let leaf_value = Node(byte: 7_u8, children: move leaf_children);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let root_children = slots_new::<Box<Node>, 1>();
  place_back(window: &root_children, value: move leaf_node);
  let root_value = Node(byte: 9_u8, children: move root_children);
  let root_node = box_new::<Node>(value: move root_value);
  let total = sum_path(root: &root_node, index: 0_u64);
  return std::process::exit_status(code: total);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner.byte), reads(root.inner.children[index]), reads(root.inner.children.len)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner.byte), reads(root.inner.children[index]), reads(root.inner.children.len)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"struct Node {
  byte: u8;
  children: Slots<Box<Node>, 1>;
}

fn sum_path(root: &Box<Node>, index: u64) -> result: u8 reads(root.inner.byte), reads(root.inner.children[index]), reads(root.inner.children.len) {
  let byte = root^.inner.byte;
  if index < root^.inner.children.len {
    let child = &root^.inner.children[index];
    let suffix = sum_path(root: child, index: index);
    let total = byte +wrap suffix;
    return total;
  } else {
    return byte;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_children = slots_new::<Box<Node>, 1>();
  let leaf_value = Node(byte: 7_u8, children: move leaf_children);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let root_children = slots_new::<Box<Node>, 1>();
  place_back(window: &root_children, value: move leaf_node);
  let root_value = Node(byte: 9_u8, children: move root_children);
  let root_node = box_new::<Node>(value: move root_value);
  let total = sum_path(root: &root_node, index: 0_u64);
  return std::process::exit_status(code: total);
}
"#],
    },
    // Recursion forwards the same root, so no growing argument suffix can
    // widen away the indexed Box field. Its repair must retain the field name.
    RepairPair {
        name: "recursive-indexed-field-row.wf",
        rejected: br#"struct Item {
  byte: u8;
}

fn sum_repeated(root: &Slots<Box<Item>, 1>, index: u64, remaining: u64) -> result: u8 reads(root.len) {
  if index < root^.len {
    let byte = root^[index].inner.byte;
    if remaining > 0_u64 {
      let next = remaining - 1_u64;
      let suffix = sum_repeated(root: root, index: index, remaining: next);
      let total = byte +wrap suffix;
      return total;
    } else {
      return byte;
    }
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(byte: 7_u8);
  let node = box_new::<Item>(value: item);
  let items = slots_new::<Box<Item>, 1>();
  place_back(window: &items, value: move node);
  let total = sum_repeated(root: &items, index: 0_u64, remaining: 1_u64);
  return std::process::exit_status(code: total);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root[index].inner.byte), reads(root.len)\n",
            "\n  mechanical_fix: declare the row as `reads(root[index].inner.byte), reads(root.len)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"struct Item {
  byte: u8;
}

fn sum_repeated(root: &Slots<Box<Item>, 1>, index: u64, remaining: u64) -> result: u8 reads(root[index].inner.byte), reads(root.len) {
  if index < root^.len {
    let byte = root^[index].inner.byte;
    if remaining > 0_u64 {
      let next = remaining - 1_u64;
      let suffix = sum_repeated(root: root, index: index, remaining: next);
      let total = byte +wrap suffix;
      return total;
    } else {
      return byte;
    }
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(byte: 7_u8);
  let node = box_new::<Item>(value: item);
  let items = slots_new::<Box<Item>, 1>();
  place_back(window: &items, value: move node);
  let total = sum_repeated(root: &items, index: 0_u64, remaining: 1_u64);
  return std::process::exit_status(code: total);
}
"#],
    },
    RepairPair {
        name: "mutually-recursive-rows.wf",
        rejected: br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup_first(root: &Box<Node>) -> result: u8 reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Leaf(byte: first_byte) => {
      return first_byte^;
    }
    Branch(next: first_child) => {
      return lookup_second(root: first_child);
    }
  }
}

fn lookup_second(root: &Box<Node>) -> result: u8 reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Leaf(byte: second_byte) => {
      return second_byte^;
    }
    Branch(next: second_child) => {
      return lookup_first(root: second_child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_value = Node::Leaf(byte: 7_u8);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let branch_value = Node::Branch(next: move leaf_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = lookup_first(root: &root_node);
  return std::process::exit_status(code: found);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner)`, which covers every access the body makes and no other; also declare the row of `lookup_second` as `reads(root.inner)`\n",
        ],
        repaired: &[br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup_first(root: &Box<Node>) -> result: u8 reads(root.inner) {
  match &root^.inner {
    Leaf(byte: first_byte) => {
      return first_byte^;
    }
    Branch(next: first_child) => {
      return lookup_second(root: first_child);
    }
  }
}

fn lookup_second(root: &Box<Node>) -> result: u8 reads(root.inner) {
  match &root^.inner {
    Leaf(byte: second_byte) => {
      return second_byte^;
    }
    Branch(next: second_child) => {
      return lookup_first(root: second_child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_value = Node::Leaf(byte: 7_u8);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let branch_value = Node::Branch(next: move leaf_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = lookup_first(root: &root_node);
  return std::process::exit_status(code: found);
}
"#],
    },
    RepairPair {
        name: "generic-mutually-recursive-rows.wf",
        rejected: br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup_first<T>(root: &Box<Node>) -> result: u8 reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Leaf(byte: first_byte) => {
      return first_byte^;
    }
    Branch(next: first_child) => {
      return lookup_second::<T>(root: first_child);
    }
  }
}

fn lookup_second<U>(root: &Box<Node>) -> result: u8 reads(root.inner.Leaf.byte) {
  match &root^.inner {
    Leaf(byte: second_byte) => {
      return second_byte^;
    }
    Branch(next: second_child) => {
      return lookup_first::<U>(root: second_child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_value = Node::Leaf(byte: 7_u8);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let branch_value = Node::Branch(next: move leaf_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = lookup_first::<u64>(root: &root_node);
  return std::process::exit_status(code: found);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner)`, which covers every access the body makes and no other; also declare the row of `lookup_second` as `reads(root.inner)`\n",
        ],
        repaired: &[br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup_first<T>(root: &Box<Node>) -> result: u8 reads(root.inner) {
  match &root^.inner {
    Leaf(byte: first_byte) => {
      return first_byte^;
    }
    Branch(next: first_child) => {
      return lookup_second::<T>(root: first_child);
    }
  }
}

fn lookup_second<U>(root: &Box<Node>) -> result: u8 reads(root.inner) {
  match &root^.inner {
    Leaf(byte: second_byte) => {
      return second_byte^;
    }
    Branch(next: second_child) => {
      return lookup_first::<U>(root: second_child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let leaf_value = Node::Leaf(byte: 7_u8);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let branch_value = Node::Branch(next: move leaf_node);
  let root_node = box_new::<Node>(value: move branch_value);
  let found = lookup_first::<u64>(root: &root_node);
  return std::process::exit_status(code: found);
}
"#],
    },
    RepairPair {
        name: "recursive-write-separation.wf",
        rejected: br#"struct Node {
  byte: u8;
  other: u8;
  next: Option<Box<Node>>;
}

fn store(root: &Box<Node>, value: &u8) -> result: unit reads(root.inner.next), reads(value), writes(root.inner.byte) {
  set root^.inner.byte = value^;
  match &root^.inner.next {
    Some(value: child) => {
      let supplied = &child^.inner.other;
      return store(root: child, value: supplied);
    }
    None() => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let empty = None<Box<Node>>();
  let leaf_value = Node(byte: 0_u8, other: 7_u8, next: move empty);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let child_value = Some<Box<Node>>(value: move leaf_node);
  let root_value = Node(byte: 0_u8, other: 0_u8, next: move child_value);
  let root_node = box_new::<Node>(value: move root_value);
  let supplied = 9_u8;
  let stored = store(root: &root_node, value: &supplied);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(root.inner.next), reads(value), writes(root.inner.byte), writes(root.inner.next.Some.value)\n",
            "\n  mechanical_fix: declare the row as `reads(root.inner.next), reads(value), writes(root.inner.byte), writes(root.inner.next.Some.value)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"struct Node {
  byte: u8;
  other: u8;
  next: Option<Box<Node>>;
}

fn store(root: &Box<Node>, value: &u8) -> result: unit reads(root.inner.next), reads(value), writes(root.inner.byte), writes(root.inner.next.Some.value) {
  set root^.inner.byte = value^;
  match &root^.inner.next {
    Some(value: child) => {
      let supplied = &child^.inner.other;
      return store(root: child, value: supplied);
    }
    None() => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let empty = None<Box<Node>>();
  let leaf_value = Node(byte: 0_u8, other: 7_u8, next: move empty);
  let leaf_node = box_new::<Node>(value: move leaf_value);
  let child_value = Some<Box<Node>>(value: move leaf_node);
  let root_value = Node(byte: 0_u8, other: 0_u8, next: move child_value);
  let root_node = box_new::<Node>(value: move root_value);
  let supplied = 9_u8;
  let stored = store(root: &root_node, value: &supplied);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "recursive-rotated-parameter-rows.wf",
        rejected: br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup(first: &Box<Node>, second: &Box<Node>) -> result: u8 reads(first.inner.Leaf.byte) {
  match &first^.inner {
    Leaf(byte: found) => {
      return found^;
    }
    Branch(next: child) => {
      return lookup(first: second, second: child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let child_value = Node::Leaf(byte: 7_u8);
  let child_node = box_new::<Node>(value: move child_value);
  let first_value = Node::Branch(next: move child_node);
  let first_node = box_new::<Node>(value: move first_value);
  let second_value = Node::Leaf(byte: 9_u8);
  let second_node = box_new::<Node>(value: move second_value);
  let found = lookup(first: &first_node, second: &second_node);
  return std::process::exit_status(code: found);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: reads(first.inner), reads(second.inner)\n",
            "\n  mechanical_fix: declare the row as `reads(first.inner), reads(second.inner)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"enum Node {
  Leaf(byte: u8);
  Branch(next: Box<Node>);
}

fn lookup(first: &Box<Node>, second: &Box<Node>) -> result: u8 reads(first.inner), reads(second.inner) {
  match &first^.inner {
    Leaf(byte: found) => {
      return found^;
    }
    Branch(next: child) => {
      return lookup(first: second, second: child);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let child_value = Node::Leaf(byte: 7_u8);
  let child_node = box_new::<Node>(value: move child_value);
  let first_value = Node::Branch(next: move child_node);
  let first_node = box_new::<Node>(value: move first_value);
  let second_value = Node::Leaf(byte: 9_u8);
  let second_node = box_new::<Node>(value: move second_value);
  let found = lookup(first: &first_node, second: &second_node);
  return std::process::exit_status(code: found);
}
"#],
    },
];

/// The repaired fixtures change only the row printed by the diagnostic.
/// Pin the unchanged acyclic repair alongside them using its existing pair.
#[test]
fn recursive_repairs_are_exact_substitutions_and_acyclic_text_is_unchanged() {
    for pair in RECURSIVE_EFFECTS {
        assert_eq!(pair.rule, "EFF-2", "{}", pair.name);
        assert_eq!(pair.repaired.len(), 1, "{}", pair.name);
        let rejected = std::str::from_utf8(pair.rejected).expect("UTF-8 fixture");
        let row = pair.sentences[0]
            .strip_prefix("\n  expected_row: ")
            .and_then(|text| text.strip_suffix('\n'))
            .expect("pinned expected row");
        let mut fix = format!(
            "\n  mechanical_fix: declare the row as `{row}`, which covers every access the body makes and no other"
        );
        if pair.name.ends_with("mutually-recursive-rows.wf") {
            fix.push_str(&format!(
                "; also declare the row of `lookup_second` as `{row}`"
            ));
        }
        fix.push('\n');
        assert_eq!(pair.sentences[1], fix, "{}", pair.name);
        let applied = if pair.name.ends_with("mutually-recursive-rows.wf") {
            rejected.replace("reads(root.inner.Leaf.byte)", row)
        } else if pair.name == "recursive-indexed-read-row.wf" {
            rejected.replacen(
                "reads(root.inner.byte), reads(root.inner.children.len)",
                row,
                1,
            )
        } else if pair.name == "recursive-indexed-field-row.wf" {
            rejected.replacen("reads(root.len)", row, 1)
        } else if pair.name == "recursive-write-separation.wf" {
            rejected.replacen(
                "reads(root.inner.next), reads(value), writes(root.inner.byte)",
                row,
                1,
            )
        } else if pair.name == "recursive-rotated-parameter-rows.wf" {
            rejected.replacen("reads(first.inner.Leaf.byte)", row, 1)
        } else {
            rejected.replacen(
                "reads(root.inner.Leaf.key), reads(root.inner.Leaf.byte)",
                row,
                1,
            )
        };
        assert_eq!(applied.as_bytes(), pair.repaired[0], "{}", pair.name);
    }
    let pair = super::REPAIRS
        .iter()
        .find(|pair| pair.name == "declared-row-is-narrower-than-the-body.wf")
        .expect("existing non-recursive EFF-2 pair");
    let failure = super::super::check(
        &[crate::SourceInput::new(pair.name, pair.rejected)],
        super::CompilerLimits::default(),
    )
    .expect_err("the original row is rejected");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = format!("{failure}\n");
    assert!(
        rendered.contains("\n  expected_row: reads(data.len)\n"),
        "{rendered}"
    );
    assert!(rendered.contains(pair.sentences[0]), "{rendered}");
}

/// A suffix row can still overlap another argument below the recursive child.
/// EFF-2 has a finite row, but DIAG-1 must not offer it when EFF-5 refuses it.
#[test]
fn recursive_row_refused_by_call_separation_has_no_row_repair() {
    let source = br#"struct Node {
  byte: u8;
  other: u8;
  next: Option<Box<Node>>;
}

fn store(root: &Box<Node>, value: &u8) -> result: unit reads(root.inner.next), reads(value), writes(root.inner.byte) {
  set root^.inner.byte = value^;
  match &root^.inner.next {
    Some(value: child) => {
      match &child^.inner.next {
        Some(value: grandchild) => {
          let supplied = &grandchild^.inner.other;
          return store(root: child, value: supplied);
        }
        None() => {
          return unit;
        }
      }
    }
    None() => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = super::super::check(
        &[crate::SourceInput::new(
            "recursive-row-no-repair.wf",
            source,
        )],
        super::CompilerLimits::default(),
    )
    .expect_err("the declared row misses recursive writes");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = format!("{failure}\n");
    assert!(rendered.contains("]: EffectMismatch\n"), "{rendered}");
    assert!(!rendered.contains("expected_row:"), "{rendered}");
    assert!(!rendered.contains("mechanical_fix:"), "{rendered}");
    let repaired = std::str::from_utf8(source)
        .expect("UTF-8 fixture")
        .replacen(
            "reads(root.inner.next), reads(value), writes(root.inner.byte)",
            "reads(root.inner.next), reads(value), writes(root.inner.byte), writes(root.inner.next.Some.value)",
            1,
        );
    let refused = super::super::check(
        &[crate::SourceInput::new(
            "recursive-row-refused.wf",
            repaired.as_bytes(),
        )],
        super::CompilerLimits::default(),
    )
    .expect_err("the suffix write overlaps the separate grandchild read");
    assert_eq!(refused.rule_id(), Some("EFF-5"));
}

/// Later defects cannot erase an acyclic EFF-2 repair that main already prints.
#[test]
fn acyclic_repair_text_survives_an_unrelated_later_rejection() {
    let source = br#"fn read(value: &u8) -> result: u8 pure {
  return value^;
}

fn write(left: &u8, right: &u8) -> result: unit reads(right), writes(left) {
  set left^ = right^;
  return unit;
}

fn bad() -> result: unit pure {
  let value = 0_u8;
  let ignored = write(left: &value, right: &value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = super::super::check(
        &[crate::SourceInput::new(
            "acyclic-row-before-overlapping-call.wf",
            source,
        )],
        super::CompilerLimits::default(),
    )
    .expect_err("the first body's declared row is too narrow");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = format!("{failure}\n");
    assert!(rendered.contains(
        "\n  expected_row: reads(value)\n  found_row: pure\n  missing: [reads(value)]\n  extra: []\n  mechanical_fix: declare the row as `reads(value)`, which covers every access the body makes and no other\n"
    ), "{rendered}");
}
