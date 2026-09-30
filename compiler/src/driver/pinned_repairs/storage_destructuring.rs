//! [TYPE-2, TYPE-9] storage-shape destructuring repairs and the programs
//! obtained by reading the named fields instead. The normative refusals live
//! in conformance; this family pins the compiler's concrete repair wording.
//! Keep these pairs with the shared repair harness while this repair exists.

use super::RepairPair;

pub(super) const STORAGE_DESTRUCTURING: &[RepairPair] = &[
    RepairPair {
        name: "array-taken-apart-through-reference.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-array-destructuring-let.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: replace this statement with `let count = values^.len;`\n",
        ],
        repaired: &[
            br#"fn length(values: &Array<u64, 4>) -> result: u64 reads(values.len) {
  let count = values^.len;
  return count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "slots-taken-apart.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-slots-destructuring-let.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: replace this statement with `let count = values.len; let limit = values.cap;`\n",
        ],
        repaired: &[br#"fn capacity(values: Slots<u64, 4>) -> result: u64 pure {
  let count = values.len;
  let limit = values.cap;
  return limit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "ring-content-taken-apart.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-ring-destructuring-let.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: replace this statement with `let count = values.inner.len; let limit = values.inner.cap; let start = values.inner.head;`\n",
        ],
        repaired: &[br#"fn head(values: Box<Ring<u64>>) -> result: u64 pure {
  let count = values.inner.len;
  let limit = values.inner.cap;
  let start = values.inner.head;
  return start;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "segments-content-taken-apart-through-reference.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-segments-destructuring-let.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: replace this statement with `let count = values^.inner.len;`\n",
        ],
        repaired: &[
            br#"fn length(values: &Box<Segments<u64>>) -> result: u64 reads(values.inner.len) {
  let count = values^.inner.len;
  return count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "storage-taken-apart-binding-nothing.wf",
        rejected: br#"fn capacity(values: Slots<u64, 4>) -> result: u64 pure {
  let Slots(..) = move values;
  return values.cap;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: remove this statement and keep using `values` directly\n",
        ],
        repaired: &[br#"fn capacity(values: Slots<u64, 4>) -> result: u64 pure {
  return values.cap;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "storage-taken-apart-with-rest.wf",
        rejected: br#"fn capacity(values: Slots<u64, 4>) -> result: u64 pure {
  let Slots(cap: limit, ..) = move values;
  return limit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: storage shapes expose their measures as readonly fields [TYPE-9]: replace this statement with `let limit = values.cap;`\n",
        ],
        repaired: &[br#"fn capacity(values: Slots<u64, 4>) -> result: u64 pure {
  let limit = values.cap;
  return limit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
