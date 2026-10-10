//! [TYPE-2, PRE-1] prelude opaque repairs use forming and access functions.
//! Keep these pairs wired to the shared acceptance and contradiction checks;
//! retire a pair when its repair or distinct observation retires.
use super::RepairPair;

pub(super) const PRELUDE_OPAQUE: &[RepairPair] = &[
    RepairPair {
        name: "frozen-constructed.wf",
        rejected: br#"fn probe() -> result: Frozen<u8> pure {
  return Frozen<u8>(inner: 7_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: replace this construction with `frozen_new::<T>(value: v)`, passing a copy value bare or an affine value with `move` [SHARE-1]\n",
        ],
        repaired: &[
            br#"fn probe() -> result: Frozen<u8> pure {
  return frozen_new::<u8>(value: 7_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe(v: Box<u8>) -> result: Frozen<Box<u8>> pure {
  return frozen_new::<Box<u8>>(value: move v);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "shared-constructed.wf",
        rejected: br#"fn probe() -> result: Shared<u8> pure {
  return Shared<u8>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: replace this construction with `shared_new::<T>(value: v)`, passing a copy value bare or an affine value with `move` [SHARE-1]\n",
        ],
        repaired: &[
            br#"fn probe() -> result: Shared<u8> pure {
  return shared_new::<u8>(value: 7_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe(v: Box<u8>) -> result: Shared<Box<u8>> pure {
  return shared_new::<Box<u8>>(value: move v);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "shared-read-constructed.wf",
        rejected: br#"fn probe(h: Shared<u8>) -> result: SharedRead<u8> pure {
  return SharedRead<u8>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: replace this construction with `shared_read::<T>(shared: &h)` for an existing Shared<T> handle [SHARE-1]\n",
        ],
        repaired: &[br#"fn probe(h: Shared<u8>) -> result: SharedRead<u8> pure {
  return shared_read::<u8>(shared: &h);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "frozen-taken-apart.wf",
        rejected: br#"fn probe(h: Frozen<u8>) -> result: u8 pure {
  let Frozen(inner: x) = move h;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: keep the handle formed by `frozen_new` intact; replace this destructuring with reads of copy-typed parts through `h.inner` or borrows into `&h.inner`, adapting uses of the destructured bindings to those reads or references [SHARE-1]\n",
        ],
        repaired: &[
            br#"fn probe(h: Frozen<u8>) -> result: u8 pure {
  let x = h.inner;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe(h: Frozen<u8>) -> result: u8 pure {
  let x = &h.inner;
  return x^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "shared-taken-apart.wf",
        rejected: br#"fn probe(h: Shared<u8>) -> result: u8 pure {
  let Shared() = move h;
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: keep the handle formed by `shared_new` intact; replace this destructuring with an atomic target to read the state, adapting uses of the destructured bindings to those reads and marking the enclosing function `waits` [SHARE-1, SHARE-2]\n",
        ],
        repaired: &[br#"fn probe(h: Shared<u8>) -> result: u8 pure waits {
  let byte = 0_u8;
  atomic state = &h {
    set byte = state^;
  }
  return byte;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "sharedread-taken-apart.wf",
        rejected: br#"fn probe(h: SharedRead<u8>) -> result: u8 pure {
  let SharedRead() = move h;
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: keep the readonly handle formed by `shared_read` intact; replace this destructuring with a readonly atomic target to read the state, adapting uses of the destructured bindings to those reads and marking the enclosing function `waits` [SHARE-1, SHARE-2]\n",
        ],
        repaired: &[br#"fn probe(h: SharedRead<u8>) -> result: u8 pure waits {
  let byte = 0_u8;
  atomic state = &h {
    set byte = state^;
  }
  return byte;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "concurrent-map-constructed.wf",
        rejected: br#"fn probe() -> result: unit pure {
  ConcurrentHashMap<u8>();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: replace this construction with `shared_map_new::<V>(capacity: n)` and reach the map through an atomic target in a function marked `waits` [SHARE-1, SHARE-2]\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let count = 0_u64;
  atomic state = &store {
    set count = map_count::<u8>(map: state);
  }
  let code = cvt.wrap::<u64, u8>(count);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
    RepairPair {
        name: "concurrent-map-taken-apart.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let count = 0_u64;
  atomic state = &store {
    let ConcurrentHashMap() = move state^;
  }
  let code = cvt.wrap::<u64, u8>(count);
  return std::process::exit_status(code: code);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: remove this destructuring statement and reach the map through an atomic target over a handle formed by `shared_map_new` [SHARE-1, SHARE-2]\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let count = 0_u64;
  atomic state = &store {
    set count = map_count::<u8>(map: state);
  }
  let code = cvt.wrap::<u64, u8>(count);
  return std::process::exit_status(code: code);
}
"#,
        ],
    },
];
