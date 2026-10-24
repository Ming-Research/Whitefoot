//! Handler-bearing enum values that cross between the two worlds of one
//! `--par` module (compiler/parallel-lowering/two-worlds): the ordinary
//! computation keeps its single-thread layout and handler addresses, and the
//! split computation dispatches by its own tag table. Every fixture builds a
//! value once and lets both dispatchers read it.

use super::parallel::function_body;
use super::system::with_mutated_ir_lowering;
use super::{
    BoundedOutput, build_executable, compile_and_run, emit, emit_with_overlap, emitted_body,
    test_directory,
};
use crate::OverlapLowering;
use std::collections::BTreeSet;
use std::process::Command;

/// `spin` is the only function that loops over a `match` on `Step`, so the
/// ordinary plan gives `Step` exactly one handler word and the split world
/// has one dispatch family of its own: equal counts, different targets.
/// `Owner` reaches `Step` only through a variant payload, an exclusively
/// owned `Box` cell and a struct field, and has no word itself.
const STEP_LIBRARY: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Step {
  First(value: u64);
  Second(value: u64);
  Third(value: u64);
}

struct Frame {
  step: Step;
  bias: u64;
}

enum Owner {
  Boxed(frame: Box<Frame>);
  Empty();
}

fn spin(step: &Step, count: u64) -> result: u64 reads(step) {
  let remaining = count;
  loop {
    match step^ {
      First(value: number) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        return number^;
      }
      Second(value: number) => {
        return number^;
      }
      Third(value: number) => {
        break;
      }
    }
  }
  return 0_u64;
}

fn paired_spin(step: &Step) -> result: u64 reads(step) {
  let first = spin(step: step, count: 2_u64);
  let second = spin(step: step, count: 3_u64);
  return first +wrap second;
}

fn weigh(owner: &Owner) -> result: u64 reads(owner) {
  match owner^ {
    Boxed(frame: stored) => {
      let spun = spin(step: &stored^.inner.step, count: 2_u64);
      return spun +wrap stored^.inner.bias;
    }
    Empty() => {
      return 0_u64;
    }
  }
}

fn paired_owners(left: &Owner, right: &Owner) -> result: u64 reads(left), reads(right) {
  let first = weigh(owner: left);
  let second = weigh(owner: right);
  return first +wrap second;
}
"#;

/// The same borrowed `Step` is dispatched by both offered calls, each after
/// continuing through its own word: 5 + 5.
const BORROWED_MAIN: &str = r#"
fn main() -> status: ExitStatus pure {
  let step = Step::First(value: 5_u64);
  let total = paired_spin(step: &step);
  match cvt.checked::<u64, u8>(total) {
    Ok(value: code) => {
      return exit_status(code: code);
    }
    Err(error: problem) => {
      return exit_status(code: 255_u8);
    }
  }
}
"#;

/// Two `Step`s owned through `Owner`, a `Box<Frame>` and a `Frame` field:
/// (5 + 10) + (7 + 20).
const OWNED_MAIN: &str = r#"
fn main() -> status: ExitStatus pure {
  let first_step = Step::First(value: 5_u64);
  let first_frame = Frame(step: first_step, bias: 10_u64);
  let first_box = box_new::<Frame>(value: first_frame);
  let left = Owner::Boxed(frame: move first_box);
  let second_step = Step::Second(value: 7_u64);
  let second_frame = Frame(step: second_step, bias: 20_u64);
  let second_box = box_new::<Frame>(value: second_frame);
  let right = Owner::Boxed(frame: move second_box);
  let total = paired_owners(left: &left, right: &right);
  match cvt.checked::<u64, u8>(total) {
    Ok(value: code) => {
      return exit_status(code: code);
    }
    Err(error: problem) => {
      return exit_status(code: 255_u8);
    }
  }
}
"#;

/// The program's exit status at every pool size. `0` and `1` are the
/// opt-out, where each offered call runs at its ordinary fallback; `4`
/// starts a pool that can take them. The status is the source-order value on
/// all three, so a value read through the wrong world's address fails here.
fn at_every_worker_count(module: &str, expected: i32) {
    let directory = test_directory();
    let executable = build_executable(module, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the handler-transfer fixture");
        assert_eq!(
            output.status.code(),
            Some(expected),
            "WF_WORKERS={workers}: {output:?}"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Both worlds give `Step` one word, yet the word holds the ordinary
/// family's address while the split family dispatches by its own table.
#[test]
fn equal_handler_word_counts_keep_each_world_on_its_own_targets() {
    let source = format!("{STEP_LIBRARY}{BORROWED_MAIN}");
    with_mutated_ir_lowering(source.as_bytes(), OverlapLowering::On, |program| {
        let target = crate::target::TargetLayout::host().unwrap();
        let parallel =
            crate::backend::emitter::prepare_dispatch_layout(program, target, false).unwrap();
        let ordinary = crate::backend::emitter::prepare_dispatch_layout(
            program.canonical_sequential.as_deref().unwrap(),
            target,
            false,
        )
        .unwrap();
        let step = program
            .nominals()
            .iter()
            .find(|n| n.name() == "Step")
            .unwrap()
            .id();
        assert_eq!(ordinary.program.nominal(step).unwrap().handler_words, 1);
        assert_eq!(parallel.program.nominal(step).unwrap().handler_words, 1);
        assert!(
            !program
                .functions()
                .iter()
                .find(|f| f.name() == "paired_spin")
                .unwrap()
                .overlaps()
                .is_empty(),
            "the witness must have permitted offers"
        );
    });
    let module = emit_with_overlap(source.as_bytes());
    let table = module
        .lines()
        .find(|line| line.starts_with("@wf_spin.dispatch.table = "))
        .expect("the split family has a table of its own");
    for arm in 0..3 {
        assert!(
            table.contains(&format!("ptr @wf_spin.arm.{arm}")),
            "the split table binds split handlers: {table}"
        );
    }
    assert!(!table.contains("@wf__par_seq_"), "{table}");
    // A value carries the ordinary family's addresses and no split one.
    assert!(module.contains("ptr @wf__par_seq_spin.arm."), "{module}");
    assert!(!module.contains("store ptr @wf_spin.arm."), "{module}");
    let split = function_body(&module, "@wf_spin.dispatch");
    assert!(
        split.contains("getelementptr inbounds [3 x ptr], ptr "),
        "the split dispatch indexes its own table: {split}"
    );
    let ordinary = function_body(&module, "@wf__par_seq_spin.dispatch");
    assert!(
        ordinary.contains("load ptr") && !ordinary.contains("[3 x ptr]"),
        "the ordinary dispatch calls the stored word: {ordinary}"
    );
    assert!(
        function_body(&module, "@wf_paired_spin").contains("@wf__par_acquire_lane("),
        "the pair must be offered"
    );
    let reference = compile_and_run(&emit(source.as_bytes()));
    assert_eq!(reference.status.code(), Some(10), "{reference:?}");
    at_every_worker_count(&module, 10);
}

/// The values the dispatchers read are reached through an enum payload, a
/// `Box` and a struct, none of which holds a handler word itself.
#[test]
fn handler_cells_nested_in_owned_payloads_cross_worlds_at_every_worker_count() {
    let source = format!("{STEP_LIBRARY}{OWNED_MAIN}");
    with_mutated_ir_lowering(source.as_bytes(), OverlapLowering::On, |program| {
        let target = crate::target::TargetLayout::host().unwrap();
        let parallel =
            crate::backend::emitter::prepare_dispatch_layout(program, target, false).unwrap();
        let ordinary = crate::backend::emitter::prepare_dispatch_layout(
            program.canonical_sequential.as_deref().unwrap(),
            target,
            false,
        )
        .unwrap();
        for (name, words) in [("Step", 1), ("Frame", 0), ("Owner", 0)] {
            let id = program
                .nominals()
                .iter()
                .find(|n| n.name() == name)
                .unwrap()
                .id();
            assert_eq!(ordinary.program.nominal(id).unwrap().handler_words, words);
            assert_eq!(parallel.program.nominal(id).unwrap().handler_words, words);
        }
        assert!(
            !program
                .functions()
                .iter()
                .find(|f| f.name() == "paired_owners")
                .unwrap()
                .overlaps()
                .is_empty(),
            "the witness must have permitted offers"
        );
    });
    let module = emit_with_overlap(source.as_bytes());
    assert!(
        emitted_body(&module, "paired_owners").contains("@wf__par_acquire_lane("),
        "the owners must be offered"
    );
    assert!(module.contains("ptr @wf__par_seq_spin.arm."), "{module}");
    assert!(!module.contains("store ptr @wf_spin.arm."), "{module}");
    let reference = compile_and_run(&emit(source.as_bytes()));
    assert_eq!(reference.status.code(), Some(42), "{reference:?}");
    at_every_worker_count(&module, 42);
}

/// The entrance check compares every representation reachable from a
/// crossing signature, so a disagreement about one nominal is found through
/// each kind of owner and container above it.
#[test]
fn crossing_validation_reaches_handler_cells_through_owners_and_containers() {
    let source = format!("{STEP_LIBRARY}{OWNED_MAIN}");
    with_mutated_ir_lowering(source.as_bytes(), OverlapLowering::On, |program| {
        let canonical = program.canonical_sequential.as_deref().unwrap();
        let step = program
            .nominals()
            .iter()
            .find(|n| n.name() == "Step")
            .unwrap()
            .id();
        let roots = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<BTreeSet<_>>()
        };
        let validate = crate::backend::emitter::validate_world_crossovers;
        validate(
            program,
            canonical,
            &roots(&["main", "spin", "paired_spin", "weigh", "paired_owners"]),
        )
        .expect("one checked source has one representation in both worlds");
        // The ordinary plan gave Step a word that the other world lacks.
        let mut disagreeing = canonical.clone();
        disagreeing.nominals[step.index()].handler_words = 1;
        for (name, path) in [
            ("spin", "a reference to the cell itself"),
            ("paired_spin", "a reference to the cell itself"),
            ("weigh", "a variant payload, a Box and a struct field"),
            ("paired_owners", "a variant payload, a Box and a struct field"),
        ] {
            assert_eq!(
                validate(program, &disagreeing, &roots(&[name])),
                Err(crate::backend::BackendFailure::InvalidIr),
                "{name} reaches Step through {path}"
            );
        }
        validate(program, &disagreeing, &roots(&["main"]))
            .expect("a signature that reaches no Step is unaffected");
    });
}
