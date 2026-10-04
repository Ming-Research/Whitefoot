//! Table grant and key-subscript repairs, carried out through the shared harness.

use super::RepairPair;

pub(super) const TABLE_GRANTS: &[RepairPair] = &[
    RepairPair {
        name: "table-owned-borrow-without-reference.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-owned-borrow-without-reference.wf"
        ),
        rule: "OP-4",
        sentences: &[
            "]: TableNeedsReference\n",
            "\n  mechanical_fix: form a reference first, `let t = &local.map;`, and index `t^[key]`\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
}

const bytes: Array<u8, 1> =[97_u8];

fn main() -> status: std::process::ExitStatus pure {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let local = Store(map: move table);
  let key = &bytes[0_u64..1_u64];
  let t = &local.map;
  let slot = &t^[key];
  set slot^ = Some<u8>(value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "table-reached-through-a-box.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-reached-through-a-box.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTableNotGranted\n",
            "\n  mechanical_fix: move the table to a state field reached through no `Box` and no enum payload, then bind it whole as `t = &s^.field` in the header and reach it through `t`\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table);
  let store = shared_new::<Store>(value: move state);
  let counted = 0_u64;
  atomic s = &store, t = &s^.map {
    set counted = keyed_table_count::<u8>(table: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "table-state-payload-alias.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-state-payload-alias.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTableNotGranted\n",
            "\n  mechanical_fix: move the table to a state field reached through no `Box` and no enum payload, then bind it whole as `t = &s^.field` in the header and reach it through `t`\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table);
  let store = shared_new::<Store>(value: move state);
  let counted = 0_u64;
  atomic s = &store, t = &s^.map {
    set counted = keyed_table_count::<u8>(table: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "table-whole-binding-through-a-box.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-whole-binding-through-a-box.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTargetNotShared\n",
            "\n  mechanical_fix: write a table binding as `name = &s^.table[key]` for one entry, `name = &s^.table[keys]` for the entries under a key set, or `name = &s^.table` for the table whole, where `s` is the statement's binding and `table` a field of type `KeyedTable<V>` reached through no `Box`\n",
        ],
        repaired: &[
            br#"struct Store {
  map: KeyedTable<u8>;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic s = &store, t = &s^.map {
    set n = keyed_table_count::<u8>(table: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-table-entries-start-none.wf"
            ),
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-table-key-set-entries.wf"
            ),
        ],
    },
    RepairPair {
        name: "table-mixed-entry-and-whole.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-mixed-entry-and-whole.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTableBoundTwice\n",
            "\n  mechanical_fix: name each table once: a whole binding `t = &s^.table` when the block computes its keys, reaching entries as `t^[key]` and `&t^[keys]`, or entry bindings for keys known before the statement\n",
        ],
        repaired: &[
            br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic s = &store, t = &s^.map {
    set t^[name] = Some<u8>(value: 1_u8);
    set n = keyed_table_count::<u8>(table: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-table-two-keys-one-entry.wf"
            ),
        ],
    },
    RepairPair {
        name: "table-reached-without-binding.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-reached-without-binding.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTableNotGranted\n",
            "\n  mechanical_fix: add a whole binding `t = &s^.table` to the header and reach the table through `t`; a statement's header names every table its guard and block reach\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let counted = 0_u64;
  atomic s = &store, t = &s^.map {
    set counted = keyed_table_count::<u8>(table: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "table-row-names-table-without-binding.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-row-names-table-without-binding.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicRowReachesTable\n",
            "\n  mechanical_fix: add a whole binding for each table the callee's row reaches through this argument, or pass the parts the callee needs: `&s^.field` for a field, an entry binding for an entry\n",
        ],
        repaired: &[
            br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

const names: Array<u8, 1> =[97_u8];

fn touch(env: &Store) -> result: unit writes(env) {
  set env^.count = env^.count +wrap 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  atomic s = &store, t = &s^.map {
    set t^[name] = Some<u8>(value: 1_u8);
    touch(env: s);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

fn touch(env: &u8) -> result: unit writes(env) {
  set env^ = 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  atomic s = &store {
    touch(env: &s^.count);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Store {
  map: KeyedTable<u8>;
}

const names: Array<u8, 1> =[97_u8];

fn touch(env: &Option<u8>) -> result: unit writes(env) {
  set env^ = Some<u8>(value: 1_u8);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  atomic s = &store, slot = &s^.map[name] {
    touch(env: slot);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "table-whole-binding-unused.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-whole-binding-unused.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicBindingUnused\n",
            "\n  mechanical_fix: remove the table binding, or the whole statement when nothing in it reaches the state; an atomic statement holds what its header names, so a binding nothing uses holds a table or an entry for nothing\n",
        ],
        repaired: &[
            br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic s = &store {
    set s^.count = 1_u8;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "table-subscript-offset-not-a-key.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-subscript-offset-not-a-key.wf"
        ),
        rule: "OP-4",
        sentences: &[
            "]: TableOffsetNotKey\n",
            "\n  mechanical_fix: index a table by a key, a `&[u8]` range such as `&bytes[start..end]`, or borrow the entries under a `KeySet` as `&t^[keys]`\n",
        ],
        repaired: &[
            br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic s = &store, t = &s^.map {
    set t^[name] = Some<u8>(value: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-table-whole-binding-entries-over-set.wf"
            ),
        ],
    },
    RepairPair {
        name: "table-entries-place-not-borrowed.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-table-entries-place-not-borrowed.wf"
        ),
        rule: "OP-4",
        sentences: &[
            "]: TableEntriesNotBorrowed\n",
            "\n  mechanical_fix: write `&t^[keys]`: the entries under a key set are reached only as a reference of kind `&KeyedEntries<V>`\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
  count: u8;
}

const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let state = Store(map: move table, count: 0_u8);
  let store = shared_new::<Store>(value: move state);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  let keys = key_set_new(capacity: 0_u64);
  atomic s = &store, t = &s^.map {
    let x = &t^[keys];
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "table-subscript-needs-reference.wf",
        rejected: br#"struct Store {
  map: KeyedTable<u8>;
}

const bytes: Array<u8, 1> =[97_u8];

fn main() -> status: std::process::ExitStatus pure {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let local = Store(map: move table);
  let key = &bytes[0_u64..1_u64];
  set local.map[key] = Some<u8>(value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "]: TableNeedsReference\n",
            "\n  mechanical_fix: form a reference first, `let t = &local.map;`, and index `t^[key]`\n",
        ],
        repaired: &[br#"struct Store {
  map: KeyedTable<u8>;
}

const bytes: Array<u8, 1> =[97_u8];

fn main() -> status: std::process::ExitStatus pure {
  let table = keyed_table_new::<u8>(capacity: 0_u64);
  let local = Store(map: move table);
  let key = &bytes[0_u64..1_u64];
  let t = &local.map;
  set t^[key] = Some<u8>(value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
