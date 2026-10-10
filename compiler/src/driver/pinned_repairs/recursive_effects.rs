//! Recursive EFF-2 repairs, applied as exact row substitutions by the
//! shared DIAG-1 harness. Keep these pairs while this diagnostic exists.

use super::RepairPair;

pub(super) const RECURSIVE_EFFECTS: &[RepairPair] = &[
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
];

/// The repaired fixtures change only the row printed by the diagnostic.
/// Pin the unchanged acyclic repair alongside them using its existing pair.
#[test]
fn recursive_repairs_are_exact_substitutions_and_acyclic_text_is_unchanged() {
    for pair in RECURSIVE_EFFECTS {
        let rejected = std::str::from_utf8(pair.rejected).expect("UTF-8 fixture");
        let applied = if pair.name.ends_with("mutually-recursive-rows.wf") {
            rejected.replace("reads(root.inner.Leaf.byte)", "reads(root.inner)")
        } else {
            rejected.replacen(
                "reads(root.inner.Leaf.key), reads(root.inner.Leaf.byte)",
                "reads(root.inner)",
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
