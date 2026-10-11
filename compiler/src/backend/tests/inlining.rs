//! Frontend inlining keeps layout transfers without charging their expansion.

use super::{compile, compile_and_run, emitted_body, emitted_function, host_optimized_module};
use crate::backend::emitter::LlvmModule;
use crate::backend::{FragmentGranularity, split_module};

const SOURCE: &[u8] = br#"struct Pair {
  a: u64;
  b: u64;
}

struct Wide {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
}

fn copied(value: Wide) -> result: Wide pure {
  return value;
}

fn pair(seed: u64) -> result: Pair pure {
  let next = seed +wrap 1_u64;
  return Pair(a: seed, b: next);
}

fn loop_scalar(count: u64) -> result: u64 pure {
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap i;
  }
  return total;
}

fn pair_via_loop(count: u64) -> result: Pair pure {
  let total = loop_scalar(count: count);
  return Pair(a: total, b: count);
}

fn loop_pair(count: u64) -> result: Pair pure {
  let total = 0_u64;
  for (i in 0_u64..count) {
    set total = total +wrap i;
  }
  return Pair(a: total, b: count);
}

fn main() -> status: std::process::ExitStatus pure {
  let input = Wide(a: 3_u64, b: 5_u64, c: 7_u64, d: 11_u64);
  let output = copied(value: input);
  let p = pair(seed: 13_u64);
  let sum = loop_scalar(count: 4_u64);
  let q = loop_pair(count: 4_u64);
  let indirect = pair_via_loop(count: 5_u64);
  if output.a != 3_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if output.d != 11_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if p.b != 14_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if sum != 6_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if q.a != 6_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if q.b != 4_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if indirect.a != 10_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

fn forced(body: &str) -> bool {
    body.lines()
        .next()
        .expect("definition header")
        .contains(" alwaysinline ")
}

#[test]
fn bounded_inlining_preserves_granules_and_cyclic_result_bodies() {
    let module = compile(SOURCE);
    let copied = emitted_function(&module, "copied");
    assert!(forced(copied), "{copied}");
    assert!(copied.contains("; layout-bounded transfer"), "{copied}");
    assert!(!copied.contains("@llvm.memmove"), "{copied}");
    for name in ["pair", "loop_scalar"] {
        let function = emitted_function(&module, name);
        assert!(forced(function), "{function}");
    }
    assert!(forced(emitted_body(&module, "pair")));
    assert!(!forced(emitted_function(&module, "loop_pair")));
    assert!(!forced(emitted_body(&module, "loop_pair")));
    assert!(!forced(emitted_function(&module, "pair_via_loop")));
    assert!(!forced(emitted_body(&module, "pair_via_loop")));
    // The actual host optimization must remove forced call boundaries,
    // including the register-result wrapper/body pair.
    let optimized = host_optimized_module(&module);
    for name in ["copied", "pair", "pair.body", "loop_scalar"] {
        assert!(
            !optimized
                .lines()
                .any(|line| line.contains(" call ") && line.contains(&format!("@wf_{name}("))),
            "{name} remained called: {optimized}"
        );
    }
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn bounded_inline_attributes_survive_serialization_and_fragments() {
    let inputs = [crate::SourceInput::new("inlining-fragments.wf", SOURCE)];
    // Fragment emission chooses a partitionable layout before LLVM emission;
    // the ordinary whole-program handler-word layout cannot be split later.
    let module = crate::compile_for_emission(
        &inputs,
        crate::CompilerLimits::default(),
        crate::OverlapLowering::Off,
        None,
        true,
    )
    .expect("the fixture is compiled for partitionable layouts")
    .0;
    let restored = LlvmModule::decode(&module.encode()).expect("decode");
    assert_eq!(restored.as_str(), module.as_str());
    for granularity in [FragmentGranularity::Module, FragmentGranularity::Function] {
        let fragments = split_module(&module, granularity).expect("original module splits");
        let cached = split_module(&restored, granularity).expect("restored module splits");
        assert_eq!(fragments, cached);
        for (name, expected) in [
            ("copied", true),
            ("pair", true),
            ("pair.body", true),
            ("loop_scalar", true),
            ("loop_pair", false),
            ("loop_pair.body", false),
            ("pair_via_loop", false),
            ("pair_via_loop.body", false),
        ] {
            let marker = format!("@wf_{name}(");
            let headers = fragments
                .iter()
                .flat_map(|fragment| fragment.lines())
                .filter(|line| line.starts_with("define ") && line.contains(&marker))
                .collect::<Vec<_>>();
            assert_eq!(headers.len(), 1, "{name}: {headers:?}");
            assert_eq!(
                headers[0].contains(" alwaysinline "),
                expected,
                "{}",
                headers[0]
            );
        }
    }
}

#[test]
fn backend_policy_uses_emitted_world_for_overlap_and_callee_summaries() {
    super::system::with_mutated_ir(SOURCE, |program| {
        let index = program
            .functions
            .iter()
            .position(|function| function.name() == "copied")
            .expect("copied");
        let selected = |program: &crate::IrProgram| {
            super::super::emitter::inlining::InlinePlan::build(program)
                .expect("plan")
                .contains(index)
        };
        assert!(selected(program));
        program.functions[index].name = "unrelated_spelling".to_owned();
        assert!(selected(program));
        program.functions[index].waits = true;
        assert!(!selected(program));
        program.functions[index].waits = false;
        program.functions[index].overlaps.push(crate::IrOverlap {
            members: Vec::new(),
        });
        assert!(!selected(program));
        use crate::backend::emitter::inlining::InlinePlan;
        assert!(
            InlinePlan::build_sequential(program)
                .expect("sequential plan")
                .contains(index)
        );
        // Synthesized chunks stay outside the bounded source-body policy in
        // both worlds; ignoring overlap is not a general eligibility bypass.
        program.functions[index].synthesis = Some(crate::IrSynthesis::Chunk);
        assert!(
            !InlinePlan::build_sequential(program)
                .expect("sequential plan")
                .contains(index)
        );

        let looping = program
            .functions
            .iter()
            .position(|function| function.name() == "loop_scalar")
            .expect("loop_scalar");
        let wrapper = program
            .functions
            .iter()
            .position(|function| function.name() == "pair_via_loop")
            .expect("pair_via_loop");
        program.functions[looping].overlaps.push(crate::IrOverlap {
            members: Vec::new(),
        });
        let ordinary = InlinePlan::build(program).expect("ordinary plan");
        let sequential = InlinePlan::build_sequential(program).expect("sequential plan");
        assert!(!ordinary.contains(looping));
        assert!(sequential.contains(looping));
        // The ordinary wrapper calls an unselected callee; its loop is not
        // absorbed early. The clone's selected callee introduces that loop,
        // so its register-result wrapper must retain independent simplification.
        assert!(ordinary.contains(wrapper));
        assert!(!sequential.contains(wrapper));
    });
}
