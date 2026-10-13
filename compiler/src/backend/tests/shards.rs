//! Backend obligations that the source conformance verdicts do not inspect.

use super::{compile, compile_and_run, system::with_ir};
use crate::{IrNominalKind, IrType};

const WHOLE_CALLEE: &[u8] = include_bytes!(
    "../../../../tests/conformance/cases/share-pos-shards-whole-to-callee.wf"
);
const OWNED: &[u8] = include_bytes!(
    "../../../../tests/conformance/cases/share-pos-shards-owned-buffers.wf"
);

#[test]
fn shards_emit_inline_ticket_paths_and_no_watch_protocol() {
    let llvm = compile(WHOLE_CALLEE);
    assert!(llvm.contains("@wf__map_user = external thread_local global i32"));
    assert!(llvm.contains("load i32, ptr @wf__map_user"));
    assert!(
        llvm.contains("@wf__context_boundary = external thread_local global { ptr, ptr, i64 }")
    );
    assert!(llvm.contains(".next = add i32 "));
    assert!(llvm.contains(".threshold = icmp uge i32 "));
    assert!(llvm.contains(".identity = and i64 "));
    assert!(llvm.contains(".check = or i1 "));
    assert!(llvm.contains("call i32 @wf__context_pass_counted("));
    let lines = llvm.lines().collect::<Vec<_>>();
    let resumes = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("wf.atomic.pass.") && line.ends_with(".resume:"))
        .collect::<Vec<_>>();
    assert!(!resumes.is_empty());
    for (index, label) in resumes {
        let done = label.trim_end_matches(".resume:");
        assert_eq!(
            lines[index + 1].trim(),
            format!("br label %{done}.done"),
            "resume must reach acquisition without counting this pass again"
        );
    }
    let boundary = llvm.find("call i32 @wf__context_pass_counted(").unwrap();
    let selection = llvm.find("load i32, ptr @wf__map_user").unwrap();
    assert!(
        boundary < selection,
        "choose the executor's shard after possible migration"
    );
    assert!(
        llvm.lines().any(|line| line.contains("atomicrmw add ptr")
            && line.contains("i64 1 monotonic"))
    );
    assert!(
        llvm.lines()
            .any(|line| line.contains("load atomic i64") && line.contains("acquire"))
    );
    assert!(
        llvm.lines()
            .any(|line| line.contains("store atomic i64") && line.contains("release"))
    );
    assert!(llvm.contains("call void @wf__shards_wait("));
    for forbidden in [
        "call void @wf__watch_",
        "call i32 @wf__watch_",
        "call i32 @wf__shared_acquire(",
        "call void @wf__shared_take(",
        "call void @wf__shared_unlock(",
        "call i32 @wf__driver_index(",
    ] {
        assert!(!llvm.contains(forbidden), "shards emitted {forbidden}");
    }
    // Scalar T has no derived cleanup, so its final release is one free.
    assert!(llvm.contains("call void @wf__shards_free("));
    assert!(!llvm.contains("shards.walk:"));
    with_ir(WHOLE_CALLEE, |program| {
        let state = program
            .nominals()
            .iter()
            .find_map(|nominal| match nominal.kind() {
                IrNominalKind::Shared {
                    state: state @ IrType::Shards { .. },
                    ..
                } => Some(*state),
                _ => None,
            })
            .expect("the fixture has a shards state");
        assert_eq!(
            crate::backend::emitter::llvm_type(program, state).unwrap(),
            "{ i64 }"
        );
        assert_eq!(crate::IrMeasure::Length.descriptor_field(state), Some(0));
    });
}

#[test]
fn owned_shards_release_each_element_before_the_object() {
    let llvm = compile(OWNED);
    assert!(llvm.contains("shards.walk:"));
    assert!(llvm.contains("%shards.index = phi i64 [ 0, %state ]"));
    assert!(llvm.contains("%shards.next = add i64 %shards.index, 1"));
    assert!(llvm.contains("%shards.value = getelementptr inbounds i8"));
    assert!(llvm.contains("call void @wf__shards_free("));
    let body = llvm
        .split_once("shards.body:")
        .expect("an owned shard has a cleanup body")
        .1
        .split_once("shards.step:")
        .expect("cleanup returns to the element walk")
        .0;
    assert!(body.contains("call void @wf__heap_give("));
}

#[test]
fn whole_shards_hold_absorbs_aliases_and_selected_aliases_share_one_slot() {
    let source = br#"fn zero() -> result: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let stats = shared_shards_new::<u64, fn zero>();
  let other = shared_share::<Shards<u64>>(shared: &stats);
  let whole = shared_share::<Shards<u64>>(shared: &stats);
  let mine = any_shard();
  atomic first = &stats[mine], second = &other[mine] {
    set first^ = 2_u64;
    set second^ = second^ +wrap 3_u64;
  }
  let total = 0_u64;
  atomic part = &stats[mine], all = &whole {
    set part^ = part^ +wrap 7_u64;
    for @shard (i in 0_u64..all^.len) {
      set total = total +wrap all^[i];
    }
  }
  let code = cvt.wrap::<u64, u8>(total);
  return std::process::exit_status(code: code);
}
"#;
    let llvm = compile(source);
    assert_eq!(compile_and_run(&llvm).status.code(), Some(12));
}
