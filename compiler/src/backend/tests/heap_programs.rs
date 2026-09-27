//! Derived recursive cleanup and precise diagnostic regressions on real programs.
use super::{compile, compile_and_run, compile_rejection, emitted_body};

fn derived_drop<'module>(llvm: &'module str, prefix: &str) -> &'module str {
    let start = llvm
        .find(prefix)
        .unwrap_or_else(|| panic!("the module must define {prefix}"));
    let end = llvm[start..]
        .find("\n}\n")
        .map(|offset| start + offset)
        .expect("a definition must close");
    &llvm[start..end]
}

#[test]
fn affine_slot_windows_fill_overwrite_empty_and_release_per_element() {
    let llvm = compile(include_bytes!("../../../../tests/programs/option_slots.wf"));
    // The two slot runs are `Slots<Option<T>, n>` windows built by
    // `slots_new` [OP-13]; the `buffer.vacant.head` tripwire this case once
    // carried retired with the `buffer` storage class and `buffer_vacant`
    // [BLK-2, TYPE-9], and v0.60 has no spelling for it to guard. What the
    // release walk still owes is the one `Some` cell the program leaves in a
    // slot: the window is frame-resident [STOR-1], its element release is
    // derived over the slots in ascending logical index order, and the `Box`
    // it holds is freed after its content [PROV-6, STOR-3, WIN-3].
    assert!(llvm.contains("call ptr @malloc"));
    let helper = derived_drop(&llvm, "define private void @wf.drop.t");
    assert!(helper.contains("call void @free"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn recursively_boxed_tree_executes_with_derived_cleanup() {
    let llvm = compile(include_bytes!(
        "../../../../tests/programs/recursive_tree.wf"
    ));
    // `count` returns in registers, so its recursion is in its
    // destination-form body, which calls the public entry
    // (compiler/src/backend/abi.rs).
    let count = emitted_body(&llvm, "count");
    assert!(
        count
            .lines()
            .any(|line| line.contains("call ") && line.contains(" @wf_count(")),
        "{count}"
    );
    assert!(llvm.contains("call ptr @malloc"));
    assert!(llvm.contains("icmp ne ptr"));
    assert!(llvm.contains("call void @free"));
    // A recursive enum's derived release is one release action per node type
    // that enters itself at the closing edge of its release graph [PROV-6]:
    // the owner deleted the cycle refusal on 2026-09-04 and ruled that the
    // walk may recurse, so the worklist driver that kept the depth off the
    // machine stack (an allocation and an abort on the release path) is gone.
    // Each of the two boxed children is released by the same action, and each
    // block is freed by the action that owns it.
    let entry = derived_drop(&llvm, "define private void @wf.drop.t");
    let name = entry
        .strip_prefix("define private void @")
        .and_then(|rest| rest.split('(').next())
        .expect("a derived release action name");
    assert_eq!(entry.matches(&format!("call void @{name}(")).count(), 2);
    assert_eq!(entry.matches("call void @free(").count(), 2);
    assert!(!llvm.contains("@wf.drop.step."));
    assert!(!llvm.contains("@wf.drop.push"));
    assert!(!llvm.contains("@wf.drop.run"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [OP-4] a byte accessor whose length branch no longer proves the strict
/// bound is a rejection, and the diagnostic renders the residual over the
/// [OP-15] measure read of the range reference the accessor was handed.
///
/// The accessor is written here rather than extracted from
/// `tests/programs/byte_string.wf` by string anchors. v0.60 growth cannot
/// fail [STOR-8], so that program's `Grown` outcome enum and its
/// store-parameter search layer - the two anchors this case used to cut on -
/// have no successor to cut at. The subject is the accessor's obligation, and
/// an inline accessor states it without depending on another package's file.
#[test]
fn the_byte_accessor_without_its_length_branch_is_an_op4_rejection() {
    let source = br#"fn byte_at(s: &[u8], index: u64) -> result: u8 reads(s) {
  let stored = deref(s).len;
  let within = index < stored;
  if within {
    let value = deref(s)[index];
    return value;
  } else {
    return 0_u8;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let bytes = array_filled::<u8, 4>(value: 7_u8);
  let whole = &bytes[0_u64..4_u64];
  let first = byte_at(s: whole, index: 0_u64);
  if first != 7_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let guarded = "  let within = index < stored;";
    let unguarded = "  let within = index <= stored;";
    let source = String::from_utf8(source.to_vec()).expect("test source is UTF-8");
    let stripped = source.replace(guarded, unguarded);
    assert_ne!(stripped, source, "the length branch must have been found");
    let failure = compile_rejection(stripped.as_bytes()).to_string();
    assert!(failure.contains("[OP-4]"), "{failure}");
    assert!(failure.contains("index < deref(s).len"), "{failure}");
}
