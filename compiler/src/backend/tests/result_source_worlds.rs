//! Result-source world exclusions and signature-only caller fragments.

use super::owned_places::retain_calls;
use super::{
    BoundedOutput, build_executable, compile_and_run, emit_lowered, emitted_function,
    test_directory,
};
use std::process::Command;

const TYPES: &str = r#"struct Item {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
}

struct Holder {
  tag: u64;
  item: Item;
}
"#;

// Both results are needed, and every-call overlap lowering keeps this pair's
// offer. Reaching it gives a cyclic caller a sequential clone and a budget
// family even when that caller has no overlapping group of its own.
const PAIR: &str = r#"
fn identity(n: u64) -> result: u64 pure {
  return n;
}

fn pair(n: u64) -> result: u64 pure {
  let left = identity(n: n);
  let right = identity(n: n);
  let zero = right -wrap n;
  return left +wrap zero;
}
"#;

fn world_program() -> String {
    format!(
        "{TYPES}{PAIR}{}",
        r#"
fn item_of(holder: &Holder) -> result: Item reads(holder) {
  let item = holder^.item;
  return item;
}

fn walk(holder: &Holder, n: u64) -> result: u64 reads(holder) {
  if n == 0_u64 {
    let item = item_of(holder: holder);
    let first = item.a;
    return first;
  }
  let below_n = n -wrap 1_u64;
  let below = walk(holder: holder, n: below_n);
  let paired = pair(n: below);
  return paired;
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(a: 11_u64, b: 12_u64, c: 13_u64, d: 14_u64);
  let holder = Holder(tag: 9_u64, item: item);
  let observed = walk(holder: &holder, n: 2_u64);
  if observed != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    )
}

fn assert_public_leaf(body: &str) {
    assert!(body.contains("call void @wf_item_of("), "{body}");
    assert!(!body.contains("@wf_item_of.source("), "{body}");
}

fn assert_runs(module: &str) {
    // Retain the boundary as well as checking the normal optimized program.
    // Inlining must not make an invalid ABI boundary disappear from coverage.
    for native in [module.to_owned(), retain_calls(module)] {
        let output = compile_and_run(&native);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

fn assert_world_runs(module: &str) {
    for native in [module.to_owned(), retain_calls(module)] {
        let directory = test_directory();
        let executable = build_executable(&native, &directory);
        // One worker chooses the clone world; four choose the ordinary
        // budget-carrying world. Both must compute the leaf's original value.
        for workers in ["1", "4"] {
            let output = Command::new(&executable)
                .env("WF_WORKERS", workers)
                .bounded_output()
                .expect("run the result-source world fixture");
            assert_eq!(
                output.status.code(),
                Some(0),
                "workers={workers}: {output:?}"
            );
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "workers={workers}: {output:?}"
            );
        }
        std::fs::remove_file(executable).expect("remove world fixture executable");
        std::fs::remove_dir(directory).expect("remove world fixture directory");
    }
}

#[test]
fn clone_and_grain_callers_keep_an_ordinary_leafs_public_entry() {
    let module = emit_lowered(world_program().as_bytes(), crate::OverlapLowering::On);
    assert!(module.contains("@wf__par_seq_walk("), "{module}");
    assert!(!module.contains("@wf__par_seq_item_of("), "{module}");
    // Testing the callee symbol alone misses this case: the leaf has no
    // clone, so the clone caller still names the ordinary public symbol.
    assert_public_leaf(emitted_function(&module, "_par_seq_walk"));
    assert!(module.contains("@wf__par_budget_walk("), "{module}");
    assert!(!module.contains("@wf__par_budget_item_of("), "{module}");
    // An out-of-component call carries no budget and names the ordinary
    // symbol. It must nevertheless obey the caller's grain-world exclusion.
    assert_public_leaf(emitted_function(&module, "_par_budget_walk"));
    assert_world_runs(&module);
}

fn lower(source: &str, overlap: crate::OverlapLowering) -> crate::LlvmModule {
    crate::compile_with_overlap(
        &[crate::SourceInput::new("test.wf", source.as_bytes())],
        crate::CompilerLimits::default(),
        overlap,
    )
    .expect("the fragment fixture compiles")
}

fn defining_fragment(module: &crate::LlvmModule, name: &str) -> String {
    let symbol = format!(" @wf_{name}(");
    crate::split_module(module, crate::FragmentGranularity::Function)
        .expect("the module splits")
        .into_iter()
        .find(|fragment| {
            fragment
                .lines()
                .any(|line| line.starts_with("define ") && line.contains(&symbol))
        })
        .unwrap_or_else(|| panic!("no fragment defines {name}"))
}

fn budget_program(body: &str) -> String {
    format!(
        r#"{TYPES}{PAIR}
fn item_of(holder: &Holder, n: u64) -> result: Item reads(holder) {{
{body}
}}

fn read_item_of(holder: &Holder) -> result: u64 reads(holder) {{
  let item = item_of(holder: holder, n: 1_u64);
  let first = item.a;
  let last = item.d;
  return first +wrap last;
}}

fn sum_item(item: Item) -> result: u64 pure {{
  return item.a +wrap item.d;
}}

fn consume_item_of(holder: &Holder) -> result: u64 reads(holder) {{
  let item = item_of(holder: holder, n: 1_u64);
  return sum_item(item: item);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let item = Item(a: 11_u64, b: 12_u64, c: 13_u64, d: 14_u64);
  let holder = Holder(tag: 9_u64, item: item);
  let observed = read_item_of(holder: &holder);
  if observed != 25_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  let consumed = consume_item_of(holder: &holder);
  if consumed != 25_u64 {{
    return std::process::exit_status(code: 2_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn a_callee_entering_or_leaving_a_budget_family_keeps_its_callers_fragment() {
    let ordinary = budget_program("  let item = holder^.item;\n  return item;");
    let budgeted = budget_program(
        r#"  if n == 0_u64 {
    let item = holder^.item;
    return item;
  }
  let below_n = n -wrap 1_u64;
  let below = item_of(holder: holder, n: below_n);
  let first = below.a;
  let paired = pair(n: first);
  return Item(a: paired, b: below.b, c: below.c, d: below.d);"#,
    );
    let modules = [
        lower(&ordinary, crate::OverlapLowering::On),
        lower(&budgeted, crate::OverlapLowering::On),
    ];
    assert!(
        !modules[0].as_str().contains("@wf__par_budget_item_of("),
        "{}",
        modules[0].as_str()
    );
    assert!(
        modules[1].as_str().contains("@wf__par_budget_item_of("),
        "{}",
        modules[1].as_str()
    );
    let callers = modules
        .iter()
        .map(|module| defining_fragment(module, "read_item_of"))
        .collect::<Vec<_>>();
    // Both callers must actually name .source. Comparing two public calls
    // would never inspect the budget source entry's exported declaration.
    for caller in &callers {
        assert!(caller.contains("call ptr @wf_item_of.source("), "{caller}");
        assert!(
            caller.lines().any(|line| line.starts_with("declare ")
                && line.contains(" nonnull align 8 ptr @wf_item_of.source(")),
            "{caller}"
        );
    }
    // Adding recursion changes only item_of's body, and removing it is the
    // reverse edit. Empty budget parameter facts must not leak into a caller.
    assert_eq!(callers[0], callers[1]);
    // An own-parameter consumer requires the public destination call. Its
    // fragment must also keep the callee's signature facts when recursion
    // changes only the implementation behind the ordinary public symbol.
    let consumers = modules
        .iter()
        .map(|module| defining_fragment(module, "consume_item_of"))
        .collect::<Vec<_>>();
    for consumer in &consumers {
        assert_public_leaf(emitted_function(consumer, "consume_item_of"));
        assert!(
            consumer
                .lines()
                .any(|line| line.starts_with("declare void @wf_item_of(")
                    && line.contains("ptr noalias nonnull")
                    && line.contains("dereferenceable(40)")),
            "{consumer}"
        );
    }
    assert_eq!(consumers[0], consumers[1]);
    for module in &modules {
        assert_runs(module.as_str());
    }
}

fn consumer_program(body: &str) -> String {
    format!(
        r#"{TYPES}
fn item_of(holder: &Holder) -> result: Item reads(holder) {{
  let item = holder^.item;
  return item;
}}

fn consume(item: Item) -> result: u64 pure {{
{body}
}}

fn read_item_of(holder: &Holder) -> result: u64 reads(holder) {{
  let item = item_of(holder: holder);
  let observed = consume(item: item);
  return observed;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let item = Item(a: 11_u64, b: 12_u64, c: 13_u64, d: 14_u64);
  let holder = Holder(tag: 9_u64, item: item);
  let observed = read_item_of(holder: &holder);
  let original = holder.item.a;
  if observed != 25_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  if original != 11_u64 {{
    return std::process::exit_status(code: 2_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn a_by_value_consumer_starting_to_mutate_keeps_its_callers_fragment() {
    let immutable =
        consumer_program("  let first = item.a;\n  let last = item.d;\n  return first +wrap last;");
    let mutating = consumer_program(
        "  let first = item.a;\n  let last = item.d;\n  set item.a = 99_u64;\n  let changed = item.a;\n  if changed != 99_u64 {\n    return 0_u64;\n  }\n  return first +wrap last;",
    );
    let modules = [
        lower(&immutable, crate::OverlapLowering::Off),
        lower(&mutating, crate::OverlapLowering::Off),
    ];
    let callers = modules
        .iter()
        .map(|module| defining_fragment(module, "read_item_of"))
        .collect::<Vec<_>>();
    for caller in &callers {
        // Consumer body inspection formerly admitted the immutable arm and
        // rejected the mutating arm. Neither signature proves an immutable
        // own parameter, so both now preserve the ordinary value boundary.
        assert_public_leaf(emitted_function(caller, "read_item_of"));
    }
    assert_eq!(callers[0], callers[1]);
    for module in &modules {
        assert_runs(module.as_str());
    }
}
