//! Held contracts preserve native executor preparation across ordinary calls.
use super::*;

const SOURCE: &str = r#"fn read(value: &u64) -> result: u64 reads(value) {
  return value^;
}

fn observe(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  return read(value: value);
}

fn pair(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  let first = observe(value: value);
  let second = observe(value: value);
  return first +wrap second;
}

fn local(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  let first = read(value: value);
  let second = read(value: value);
  return first +wrap second;
}

fn folded(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  let total = 0_u64;
  for (i in 0_u64..8_u64) {
    let part = observe(value: value);
    set total = total +wrap part;
  }
  return total;
}

fn conditional(value: &u64, first: Bool, second: Bool) -> result: unit reads(value) contract {
  requires held(value);
} {
  if first {
    observe(value: value);
  }
  if second {
    observe(value: value);
  }
  return unit;
}

fn apply<fn action(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
}>(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  let first = action(value: value);
  let second = action(value: value);
  return first +wrap second;
}

fn sample(state: &Shared<u64>) -> result: u64 reads(state) waits {
  atomic value = &state^ {
    let first = pair(value: value);
    let second = apply::<fn read>(value: value);
    return first +wrap second;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn has_split(function: &IrFunction) -> bool {
    function.blocks().iter().any(|block| {
        block.instructions().iter().any(|instruction| {
            matches!(
                instruction,
                IrInstruction::Define {
                    operation: IrOperation::LoopSplit { .. },
                    ..
                }
            )
        })
    })
}

#[test]
fn held_direct_formal_conditional_and_loop_calls_stay_on_the_executor() {
    with_checked(SOURCE.as_bytes(), |checked| {
        let pair_permission = checked
            .data
            .permission
            .named("pair")
            .expect("pair judgment");
        assert!(pair_permission.runs.iter().any(|run| {
            run.sites.iter().filter(|site| site.call.is_some()).count() >= 2
        }));
        let loop_permission = checked
            .data
            .permission
            .named("folded")
            .expect("loop judgment");
        assert!(
            loop_permission
                .loops
                .iter()
                .any(|site| site.actualization.is_some())
        );
        let sample = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "sample")
            .expect("atomic owner");
        assert!(sample.held_parameters.is_empty());
        assert!(!sample.held_calls.is_empty());
        let local = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "local")
            .expect("held body");
        assert!(!local.held_parameters.is_empty());
        assert!(local.held_calls.is_empty());
        let program = lower_checked(checked, OverlapLowering::On)
            .expect("held placement lowers without changing acceptance");
        for function in program.functions() {
            assert!(function.overlaps().is_empty(), "{}", function.name());
            assert!(!has_split(function), "{}", function.name());
            assert!(
                !function.name().starts_with("_par_"),
                "{}",
                function.name()
            );
        }
    });

    // Identical executable bodies without the held boundary must still offer
    // ordinary pairs, guarded calls and range splits. This separates executor
    // placement from a fixture whose source permission was never eligible.
    let plain = SOURCE.replace(" contract {\n  requires held(value);\n}", "");
    with_ir_mode(plain.as_bytes(), OverlapLowering::On, |program| {
        assert!(!function(program, "pair").overlaps().is_empty());
        assert!(!function(program, "local").overlaps().is_empty());
        assert!(!function(program, "conditional").overlaps().is_empty());
        assert!(has_split(function(program, "folded")));
        assert!(
            program
                .functions()
                .iter()
                .any(|function| function.name().starts_with("_par_cond_conditional_"))
        );
    });
}
