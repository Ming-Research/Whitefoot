//! Atomic target and map-selection repairs under amendment S.
use super::RepairPair;
pub(super) const SHARED_MAPS: &[RepairPair] = &[
    RepairPair {
        name: "map-placement-field.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/type9-neg-map-field.wf"),
        rule: "TYPE-9",
        sentences: &[
            "]: InvalidRestrictedTypePlacement\n",
            "a `ConcurrentHashMap<V>` is only ever the state of a shared object: write `Shared<ConcurrentHashMap<V>>`",
        ],
        repaired: &[include_bytes!(
            "../../../../tests/conformance/cases/share-pos-map-entry-beside-an-object.wf"
        )],
    },
    RepairPair {
        name: "map-placement-type-argument.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type9-neg-map-type-argument-in-a-result.wf"
        ),
        rule: "TYPE-9",
        sentences: &[
            "]: InvalidRestrictedTypePlacement\n",
            "this type argument would place `ConcurrentHashMap<u8>` as an inline value; make a shared map with `shared_map_new::<V>(capacity: n)`",
        ],
        repaired: &[include_bytes!(
            "../../../../tests/conformance/cases/type9-pos-map-type-argument-behind-a-handle.wf"
        )],
    },
    RepairPair {
        name: "map-key-set-moved.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/share-neg-map-key-moved.wf"),
        rule: "SHARE-2",
        sentences: &["name the key set without `move`: the statement reads it when it begins"],
        repaired: &[include_bytes!(
            "../../../../tests/conformance/cases/share-pos-map-key-set-entries.wf"
        )],
    },
    RepairPair {
        name: "map-mixed-entry-and-whole.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-target-handle-twice.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicTargetNotShared\n",
            "hold a handle whole once, or name its entries with entry and key-set targets",
        ],
        repaired: &[
            br#"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic t = &store {
    set t^[name] = Some<u8>(value: 1_u8);
    set n = map_count::<u8>(map: t);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-map-two-keys-one-entry.wf"
            ),
        ],
    },
    RepairPair {
        name: "map-whole-target-unused.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/share-neg-target-unused.wf"),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicBindingUnused\n",
            "remove the target, or the whole statement when no target is used",
        ],
        repaired: &[
            br#"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let tally = shared_new::<u8>(value: 0_u8);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic c = &tally {
    set c^ = 1_u8;
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
        name: "map-subscript-offset-not-a-key.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-map-subscript-offset-not-a-key.wf"
        ),
        rule: "OP-4",
        sentences: &[
            "]: TableOffsetNotKey\n",
            "\n  mechanical_fix: index a map by a key, a `&[u8]` range such as `&bytes[start..end]`, or borrow the entries under a `KeySet` as `&t^[keys]`\n",
        ],
        repaired: &[
            br#"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  atomic t = &store {
    set t^[name] = Some<u8>(value: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-pos-map-whole-target-entries-over-set.wf"
            ),
        ],
    },
    RepairPair {
        name: "map-entries-place-not-borrowed.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-map-entries-place-not-borrowed.wf"
        ),
        rule: "OP-4",
        sentences: &[
            "]: TableEntriesNotBorrowed\n",
            "\n  mechanical_fix: write `&t^[keys]`: the entries under a key set are reached only as a reference of kind `&Entries<V>`\n",
        ],
        repaired: &[br#"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let name = &names[0_u64..1_u64];
  let second = &names[1_u64..2_u64];
  let n = 0_u64;
  let keys = key_set_new(capacity: 0_u64);
  atomic t = &store {
    let x = &t^[keys];
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "atomic-guard-reads-wall-clock.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-guard-reads-wall-clock.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "]: AtomicGuardReadsSelfChanging\n",
            "\n  mechanical_fix: wait for the time outside the guard: call `std::time::sleep_until` with the deadline before the statement, or spawn a context that sleeps until the deadline and then fires a cancellation source, and give the statement a target on that source's `cancel_state` whose `fired` the guard reads\n",
        ],
        repaired: &[
            br#"struct Progress {
  n: u64;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(cwd: directory, handles: factory, clock: clock, ..) = move inputs;
  let std::fs::Directory(read: directory_read, write: directory_write) = move directory;
  std::fs::close_directory_write(factory: &factory, directory: move directory_write);
  std::fs::close_directory(factory: &factory, directory: move directory_read);
  let start = std::time::now(clock: &clock);
  let deadline = std::time::instant_after(instant: start, nanoseconds: 1000000_u64);
  let never = std::time::cancel_never();
  std::time::sleep_until(deadline: deadline, cancel: &never);
  std::time::close_cancel_watch(watch: move never);
  let initial = Progress(n: 0_u64);
  let state = shared_new::<Progress>(value: initial);
  atomic g = &state {
    set g^.n = 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            include_bytes!(
                "../../../../tests/conformance/cases/share-run-guard-deadline-through-a-cancel-watch.wf"
            ),
        ],
    },
];
