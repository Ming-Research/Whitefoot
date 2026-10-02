//! The repairs whose wording the layout stage's writers needed: a collision
//! with a top-level declaration, offering a rename on either side [TYPE-6],
//! and a bound whose facts a covering call removed, naming the call [OP-4].
//! Keep these programs while the corresponding repairs are printed.

use super::RepairPair;

pub(super) const LAYOUT_FRICTION: &[RepairPair] = &[
    // -------------------------------------------------------------------
    // [TYPE-6] a local beside a top-level declaration of its domain and
    // class: either one may be renamed.
    // -------------------------------------------------------------------
    RepairPair {
        name: "collision-with-top-level.wf",
        rejected: br#"const width: u32 = 8_u32;

fn half() -> result: u32 pure {
  let width = 4_u32;
  return width;
}

fn main() -> status: std::process::ExitStatus pure {
  let seen = half();
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[
            "\n  mechanical_fix: a top-level declaration is live in every function of its module, and a file's alias in every function of its file, whichever file declares it and wherever, so no parameter, local, or alias there may take its spelling in its domain; rename this declaration, or rename the top-level declaration or alias at the origin listed\n",
        ],
        repaired: &[
            br#"const width: u32 = 8_u32;

fn half() -> result: u32 pure {
  let narrow = 4_u32;
  return narrow;
}

fn main() -> status: std::process::ExitStatus pure {
  let seen = half();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"const full_width: u32 = 8_u32;

fn half() -> result: u32 pure {
  let width = 4_u32;
  return width;
}

fn main() -> status: std::process::ExitStatus pure {
  let seen = half();
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    // -------------------------------------------------------------------
    // [OP-4, ENT-5] a bound whose facts a call's covering row removed: the
    // call is named, and narrowing its row, stating the length unchanged in
    // its `ensures`, or a guard each discharge it.
    // -------------------------------------------------------------------
    RepairPair {
        name: "bounds-after-a-covering-call.wf",
        rejected: br#"struct Block {
  y: i32;
  height: i32;
}

struct Ctx {
  blocks: Box<Slots<Block>>;
  width: i32;
}

fn bump(context: &Ctx) -> result: unit writes(context) {
  set context^.width = context^.width +sat 1_i32;
  return unit;
}

fn after_a_call(context: &Ctx, at: u64) -> result: i32 writes(context) {
  if at < context^.blocks.inner.len {
    bump(context: context);
    let y = context^.blocks.inner[at].y;
    return y;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let block = Block(y: 5_i32, height: 1_i32);
  let blocks = box_slots_new::<Block>(capacity: 1_u64);
  place_back(window: &blocks.inner, value: block);
  let context = Ctx(blocks: move blocks, width: 0_i32);
  let y = after_a_call(context: &context, at: 0_u64);
  if y != 5_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  mechanical_fix: `at < context^.blocks.inner.len` is not proved here, but facts about `context^.blocks.inner.len` that held before the call to `bump` at line 18 would prove it, and its row, which writes `context^`, removed them: where `bump` leaves `context^.blocks.inner.len` unchanged, narrow the entry of its row that covers it to the paths its body writes, or state `context^.blocks.inner.len` unchanged in its `ensures`; or guard the access with `if at < context^.blocks.inner.len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[
            br#"struct Block {
  y: i32;
  height: i32;
}

struct Ctx {
  blocks: Box<Slots<Block>>;
  width: i32;
}

fn bump(context: &Ctx) -> result: unit writes(context.width) {
  set context^.width = context^.width +sat 1_i32;
  return unit;
}

fn after_a_call(context: &Ctx, at: u64) -> result: i32 writes(context) {
  if at < context^.blocks.inner.len {
    bump(context: context);
    let y = context^.blocks.inner[at].y;
    return y;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let block = Block(y: 5_i32, height: 1_i32);
  let blocks = box_slots_new::<Block>(capacity: 1_u64);
  place_back(window: &blocks.inner, value: block);
  let context = Ctx(blocks: move blocks, width: 0_i32);
  let y = after_a_call(context: &context, at: 0_u64);
  if y != 5_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Block {
  y: i32;
  height: i32;
}

struct Ctx {
  blocks: Box<Slots<Block>>;
  width: i32;
}

fn bump(context: &Ctx) -> result: unit writes(context) contract {
  ensures context^.blocks.inner.len == entry(context)^.blocks.inner.len;
} {
  set context^.width = context^.width +sat 1_i32;
  return unit;
}

fn after_a_call(context: &Ctx, at: u64) -> result: i32 writes(context) {
  if at < context^.blocks.inner.len {
    bump(context: context);
    let y = context^.blocks.inner[at].y;
    return y;
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let block = Block(y: 5_i32, height: 1_i32);
  let blocks = box_slots_new::<Block>(capacity: 1_u64);
  place_back(window: &blocks.inner, value: block);
  let context = Ctx(blocks: move blocks, width: 0_i32);
  let y = after_a_call(context: &context, at: 0_u64);
  if y != 5_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Block {
  y: i32;
  height: i32;
}

struct Ctx {
  blocks: Box<Slots<Block>>;
  width: i32;
}

fn bump(context: &Ctx) -> result: unit writes(context) {
  set context^.width = context^.width +sat 1_i32;
  return unit;
}

fn after_a_call(context: &Ctx, at: u64) -> result: i32 writes(context) {
  if at < context^.blocks.inner.len {
    bump(context: context);
    if at < context^.blocks.inner.len {
      let y = context^.blocks.inner[at].y;
      return y;
    }
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let block = Block(y: 5_i32, height: 1_i32);
  let blocks = box_slots_new::<Block>(capacity: 1_u64);
  place_back(window: &blocks.inner, value: block);
  let context = Ctx(blocks: move blocks, width: 0_i32);
  let y = after_a_call(context: &context, at: 0_u64);
  if y != 5_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
