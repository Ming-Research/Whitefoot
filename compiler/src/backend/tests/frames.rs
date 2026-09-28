//! Waiting functions lowered to resumable frames
//! (design/compiler/waiting-contexts.md).
//!
//! A call of a waiting function transfers into the callee's frame and the
//! callee transfers back when it returns, each by a resume placed before a
//! suspension, which the coroutine split must turn into a tail call. The
//! entry runs on the floor's 1 GiB stack, so each observation below uses a
//! count at which a transfer that kept even a few dozen bytes of native stack
//! would exhaust it and end in the stack record instead of the exit status.

use super::{compile, compile_and_run};

/// A hundred million calls of a waiting function that returns at once: every
/// call is a transfer into the callee and one back, so the native stack
/// would grow by two frames per call if the transfers were not tail calls,
/// past the floor's 1 GiB at five bytes a frame.
#[test]
fn returning_waiting_calls_do_not_grow_the_native_stack() {
    let llvm = compile(
        br#"fn step(value: u64) -> result: u64 pure waits {
  return value +wrap 1_u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let total = 0_u64;
  let index = 0_u64;
  loop @spin {
    if index >= 100000000_u64 {
      break @spin;
    }
    let next = step(value: index);
    set total = total +wrap next;
    set index = index +wrap 1_u64;
  }
  if total == 5000000050000000_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#,
    );
    assert!(llvm.contains("define ptr @wf_step(ptr %wf.result, ptr %wf.coro.parent"));
    let output = compile_and_run(&llvm);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}

/// A waiting recursion a million calls deep: every level's frame comes from
/// the context's arena, last in, first out across many chunks, and the
/// native stack stays at one resumed frame.
#[test]
fn a_deep_waiting_recursion_keeps_its_frames_in_the_arena() {
    let llvm = compile(
        br#"fn depth(remaining: u64) -> result: u64 pure waits {
  if remaining == 0_u64 {
    return 0_u64;
  }
  let fewer = remaining -wrap 1_u64;
  let below = depth(remaining: fewer);
  return below +wrap 1_u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let reached = depth(remaining: 1000000_u64);
  if reached == 1000000_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#,
    );
    let output = compile_and_run(&llvm);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}
