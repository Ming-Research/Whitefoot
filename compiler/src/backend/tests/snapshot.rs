//! Target qualification and the supplied-definition ABI, after source checking.
use super::system::with_ir_layout;
use crate::backend::emission::{FunctionBody, Module, Parameter, Signature};
use crate::backend::emitter::emit_llvm_with_layout;
use crate::target::TargetLayout;
use crate::{BackendFailure, OverlapLowering};

const FORWARD: &str = r#"fn forward(reserve: std::snapshot::Reserve, map: &ConcurrentHashMap<u64>, cut: &u64) -> result: Result<std::snapshot::Job, std::snapshot::CreateError> reads(map), reads(cut) contract {
  requires held(map, cut);
} {
  return std::snapshot::capture_export::<u64>(reserve: move reserve, map: map, cut: cut);
}
"#;

/// Capture, preparation, read translation, finish and abort all need zeroed
/// result storage. Their independently emitted bodies must share declarations
/// with each other and with ordinary emission and frame/runtime declarations.
#[test]
fn snapshot_and_ordinary_bodies_share_each_link_declaration_once() {
    let source = format!(
        "{}\n{FORWARD}",
        include_str!("../../../../tests/conformance/cases/snapshot-pos-job-lifecycle.wf")
    );
    let target = TargetLayout::for_triple("x86_64-unknown-linux-gnu").expect("qualified target");
    with_ir_layout(source.as_bytes(), OverlapLowering::Off, target, |program| {
        let mut llvm = emit_llvm_with_layout(program, target).expect("snapshot definitions emit");
        // The source's ordinary array_filled zero initialization currently
        // lowers to stores. Exercise an independently constructed ordinary
        // memset body too, without inventing a new source lowering path.
        let mut ordinary = Module::default();
        ordinary.declare(Signature::new(
            "llvm.memset.p0.i64",
            "void",
            vec![
                Parameter::unnamed("ptr"),
                Parameter::unnamed("i8"),
                Parameter::unnamed("i64"),
                Parameter::unnamed("i1 immarg"),
            ],
        ));
        let mut body = FunctionBody::default();
        body.open_block("entry".into());
        body.instructions(
            "  call void @llvm.memset.p0.i64(ptr %buffer, i8 0, i64 8, i1 false)\n  ret void\n",
            &["llvm.memset.p0.i64"],
        );
        ordinary.define(
            Signature::new(
                "wf.ordinary.zero_storage",
                "void",
                vec![Parameter::named("ptr", "%buffer")],
            )
            .define(body, "")
            .expect("ordinary zeroing body"),
        );
        llvm.model.append(ordinary);
        let rendered = llvm.model.render();
        let mut declarations = std::collections::BTreeSet::new();
        for line in rendered.lines().filter(|line| line.starts_with("declare ")) {
            let symbol = line
                .split_once('@')
                .expect("named declaration")
                .1
                .split_once('(')
                .expect("function declaration")
                .0;
            assert!(declarations.insert(symbol), "duplicate declaration: {line}");
        }
        for symbol in [
            "llvm.memset.p0.i64",
            "wf_snapshot_prepare",
            "wf_snapshot_discard",
            "wf_snapshot_capture",
            "wf_snapshot_read_start",
            "wf_snapshot_read_finish",
            "wf__completion_snapshot_finish_submit",
            "wf__completion_file_join",
            "wf_snapshot_take_outcome",
        ] {
            assert!(declarations.contains(symbol), "missing declaration: {symbol}");
        }
        assert!(llvm.model.repeated_declaration().is_none());
    });
}

#[test]
fn accepted_snapshot_source_has_an_explicit_target_capability_boundary() {
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        crate::target::WINDOWS_X86_64_TRIPLE,
    ] {
        let target = TargetLayout::for_triple(triple).expect("ordinary supported target");
        // This helper performs the ordinary source judgment before lowering;
        // every row must reach emission, including unqualified snapshot hosts.
        with_ir_layout(
            FORWARD.as_bytes(),
            OverlapLowering::Off,
            target,
            |program| {
                let emitted = emit_llvm_with_layout(program, target);
                if triple == "x86_64-unknown-linux-gnu" {
                    let llvm = emitted.expect("qualified capsule target");
                    assert!(llvm.contains("define void @wf_std.snapshot.capture_export.typed."));
                    assert!(llvm.contains("call ptr @wf_snapshot_capture("));
                } else {
                    assert_eq!(emitted, Err(BackendFailure::SnapshotTargetUnsupported));
                }
            },
        );
    }
}

#[test]
fn distinct_value_instances_supply_all_owned_shapes_without_callbacks() {
    let aggregate = r#"struct Node {
  next: Slots<Box<Node>, 1>;
  value: u64;
}

enum Choice {
  Small(code: u8);
  Large(left: u64, right: u64);
}

struct Values {
  unit_value: unit;
  flag: Bool;
  choice: Choice;
  signed_value: i16;
  floating_value: f64;
  fixed: Array<u8, 3>;
  array: Box<Array<u8>>;
  slots: Slots<u16, 4>;
  ring: Ring<u32, 4>;
  dynamic_ring: Box<Ring<u32>>;
  segments: Box<Segments<u64>>;
  pages: Box<Paged<u8>>;
  wide_pages: Box<Paged<u64>>;
  hash: std::collections::hash_map::HashMap<u32, u64, 99>;
  ordered: std::collections::ordered_map::OrderedMap<u32, u64, 99>;
  tree: Slots<Box<Node>, 1>;
}

fn aggregate(reserve: std::snapshot::Reserve, map: &ConcurrentHashMap<Values>, cut: &u64) -> result: Result<std::snapshot::Job, std::snapshot::CreateError> reads(map), reads(cut) contract {
  requires held(map, cut);
} {
  return std::snapshot::capture_export::<Values>(reserve: move reserve, map: map, cut: cut);
}
"#;
    let source = format!("{FORWARD}\n{aggregate}");
    let target = TargetLayout::for_triple("x86_64-unknown-linux-gnu").expect("qualified target");
    with_ir_layout(source.as_bytes(), OverlapLowering::Off, target, |program| {
        let llvm = emit_llvm_with_layout(program, target).expect("typed metadata emits");
        assert_eq!(
            llvm.matches("define void @wf_std.snapshot.capture_export.typed.")
                .count(),
            2
        );
        for (shape, wire_tag) in [
            (0, 8),
            (1, 16),
            (2, 17),
            (3, 32),
            (4, 33),
            (5, 34),
            (6, 35),
            (7, 36),
            (8, 37),
            (9, 48),
            (10, 49),
        ] {
            assert!(
                llvm.contains(&format!("{{ i64 {shape}, i64 {wire_tag},")),
                "missing descriptor shape {shape}"
            );
        }
        assert!(
            !llvm.contains("declare void @wf_std.snapshot.capture_export"),
            "every instance has its supplied definition"
        );
        let pages = llvm.lines()
            .filter(|line| line.contains("{ i64 7, i64 36,"))
            .collect::<Vec<_>>();
        assert_eq!(pages.len(), 2);
        for elements_per_page in [4096, 512] {
            assert!(pages.iter().any(|line| line.contains(&format!("i64 {elements_per_page} }}"))),
                "page geometry must use the u8/u64 payload stride ceiling");
        }
        // Fields are offset/type pairs and all traversal code is in the
        // supplied native runtime; no emitted encoder can call source code.
        assert!(!llvm.contains("define void @wf.snapshot."));
    });
}
