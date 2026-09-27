//! Program results and host-visible parallel behavior. Compiler permissions,
//! private IR and controlled worker observations live in backend tests.

use super::support::{
    build_program, build_program_from_fragments, compile_program, compile_program_with_overlap,
};
use whitefoot::{FragmentGranularity, module_requires_parallel_runtime};

/// The recursive corpus program publishes one byte sequence whatever the
/// recursion budget cuts, and whatever width it was cut for.
///
/// `adaptive_quadrature.wf` is the corpus's one ordinary recursive component:
/// `adaptive` subdivides until its tolerance is met and hands one half of each
/// subdivision out. Under `--par` that component gets a budget-carrying
/// family, so the tree it evaluates is cut into sequential subtrees at a depth
/// the runtime takes from the pool width — a different depth at one lane than
/// at four — and `--par-recursive-frontier off` is the same program with the
/// family withheld, offering at every node. [PAR-1] fixes every value to the
/// source-order result, so none of that may move a bit, and the reference here
/// is the `--no-overlap` build, the lowering that actualizes nothing at all.
#[test]
fn the_quadrature_program_publishes_the_analytic_integral_under_each_policy() {
    use whitefoot::{
        CompilerLimits, OverlapLowering, RecursionBudget, SourceInput,
        compile_with_permission_ledger,
    };

    let source = include_bytes!("../../../tests/programs/adaptive_quadrature.wf");
    // Analytic Lorentz integral, independent of the program's Simpson walk.
    let exact: f64 = (0..2048)
        .map(|sample| {
            let center = 0.25 + f64::from(sample) / 4096.0;
            let width = 1.0 / 64.0;
            width * (((1.0 - center) / width).atan() - (-center / width).atan())
        })
        .sum();
    let mut reference = None;
    for (mode, omitted_leaves, workers) in [
        (OverlapLowering::Off, 0, [None].as_slice()),
        (OverlapLowering::On, 0, [None].as_slice()),
        (
            OverlapLowering::OnWithoutSmallScalarLeaves {
                maximum_operations: 16,
            },
            2,
            [Some("1"), Some("2"), Some("4")].as_slice(),
        ),
        (
            OverlapLowering::OnWithRecursionBudget {
                budget: RecursionBudget::Off,
                maximum_scalar_leaf_operations: Some(16),
                sequential_refusal: false,
            },
            2,
            [Some("1"), Some("2"), Some("4")].as_slice(),
        ),
    ] {
        let (module, ledger) = compile_with_permission_ledger(
            &[SourceInput::new("adaptive_quadrature.wf", source)],
            CompilerLimits::default(),
            mode,
        )
        .expect("the quadrature program compiles under each execution policy");
        // These observations validate that the requested controls actually
        // differ; they share the compilation needed for the program result.
        assert_eq!(
            ledger
                .iter()
                .filter(|line| line.contains("scalar leaf limit"))
                .count(),
            omitted_leaves
        );
        let has_budget = matches!(
            mode,
            OverlapLowering::On | OverlapLowering::OnWithoutSmallScalarLeaves { .. }
        );
        assert_eq!(
            module.lines().any(|line| line.starts_with("define ")
                && line.contains(" double @wf__par_budget_adaptive(")),
            has_budget
        );
        assert_eq!(
            module.contains("call i64 @wf__par_recursion_budget()"),
            has_budget
        );
        if mode == OverlapLowering::Off {
            assert!(!module_requires_parallel_runtime(&module));
        }
        let program = build_program(&module);
        for &width in workers {
            let output = program.run_with_workers(width);
            assert!(output.status.success(), "{mode:?}/{width:?}: {output:?}");
            assert!(output.stderr.is_empty(), "{mode:?}/{width:?}: {output:?}");
            assert_eq!(output.stdout.len(), 64);
            let bits = std::str::from_utf8(&output.stdout).unwrap();
            let observed = f64::from_bits(u64::from_str_radix(bits, 2).unwrap());
            assert!(
                (observed - exact).abs() <= 1e-8,
                "{mode:?}/{width:?}: {observed} vs {exact}"
            );
            if let Some(bytes) = &reference {
                assert_eq!(
                    &output.stdout, bytes,
                    "{mode:?}/{width:?} moved a result bit"
                );
            } else {
                reference = Some(output.stdout);
            }
        }
    }
}

/// The migrated tree/window/spine family has independent semantic results;
/// sequential/parallel agreement alone would permit two identical mistakes.
/// Each source constructs one ordinary and one parallel image, then executes
/// one process per selected width. Scheduling-path evidence lives in compiler
/// observers, so these cases do not retry until a steal happens by chance.
#[test]
fn tree_window_and_deep_spine_preserve_their_independent_results() {
    fn mix(left: u64, right: u64) -> u64 {
        left.rotate_left(13) ^ right ^ ((u128::from(right) * 2_654_435_761) >> 64) as u64
    }
    fn fold(values: &[u64]) -> u64 {
        if values.len() == 1 {
            return values[0];
        }
        let middle = values.len() / 2;
        mix(fold(&values[..middle]), fold(&values[middle..]))
    }
    let leaves: Vec<_> = (1..=32).collect();
    let tree = fold(&leaves).to_le_bytes();
    let mut spine_values = vec![1.0009765625_f64];
    for index in 0..4_000 {
        spine_values.push(spine_values[index] * 1.0009765625);
    }
    let mut spine = spine_values[4_000];
    for &value in spine_values[..4_000].iter().rev() {
        spine += value * 0.5;
    }
    let spine = spine.to_bits().to_le_bytes();
    for (name, expected) in [
        ("parallel/tree.wf", tree),
        ("parallel/window.wf", tree),
        ("parallel/spine.wf", spine),
    ] {
        let plain = build_program(&compile_program(name));
        let output = plain.run_with_workers(Some("1"));
        assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
        assert_eq!(output.stdout, expected, "{name}");
        assert!(output.stderr.is_empty(), "{name}: {output:?}");
        let parallel = build_program(&compile_program_with_overlap(name));
        for workers in ["1", "2", "4", "8"] {
            let output = parallel.run_with_workers(Some(workers));
            assert_eq!(
                output.status.code(),
                Some(0),
                "{name}/{workers}: {output:?}"
            );
            assert_eq!(output.stdout, expected, "{name}/{workers}");
            assert!(output.stderr.is_empty(), "{name}/{workers}: {output:?}");
        }
        if name == "parallel/tree.wf" {
            let source = super::support::read_program(name);
            let module = whitefoot::compile_with_overlap(
                &[whitefoot::SourceInput::new(name, &source)],
                whitefoot::CompilerLimits::default(),
                whitefoot::OverlapLowering::OnWithRecursionBudget {
                    budget: whitefoot::RecursionBudget::Pinned(
                        std::num::NonZeroU8::new(2).unwrap(),
                    ),
                    maximum_scalar_leaf_operations: None,
                    sequential_refusal: false,
                },
            )
            .expect("pinned-budget tree must compile");
            let pinned = build_program(&module);
            for workers in ["1", "4"] {
                let output = pinned.run_with_workers(Some(workers));
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "pinned/{workers}: {output:?}"
                );
                assert_eq!(output.stdout, expected);
                assert!(output.stderr.is_empty());
            }
        }
    }
}

/// Every iteration of both loops adds one to the same element: the range the
/// body cuts is `[i..i+1]` of `w = values.inner[last - i..n]`, so each call
/// names `values.inner[last]` although its endpoints alone look like a
/// per-iteration partition of `w`. The first loop forms that range at the
/// call and the second binds it first.
const SHIFTING_ORIGIN_PROGRAM: &str = r#"fn bump(output: &[u64], mark: u64) -> result: u64 writes(output) {
  let count = deref(output).len;
  for (x in 0_u64..count) {
    let old = deref(output)[x];
    let next = old +wrap mark;
    set deref(output)[x] = next;
  }
  return count;
}

fn shifted_inline(values: &Box<Array<u64>>, n: u64) -> result: unit writes(values) contract {
  requires 1_u64 <= n;
  requires n <= deref(values).inner.len;
} {
  let last = n - 1_u64;
  for (i in 0_u64..n) {
    let lo = last - i;
    let w = &deref(values).inner[lo..n];
    let hi = i + 1_u64;
    let painted = bump(output: &deref(w)[i..hi], mark: 1_u64);
  }
  return unit;
}

fn shifted_bound(values: &Box<Array<u64>>, n: u64) -> result: unit writes(values) contract {
  requires 1_u64 <= n;
  requires n <= deref(values).inner.len;
} {
  let last = n - 1_u64;
  for (i in 0_u64..n) {
    let lo = last - i;
    let w = &deref(values).inner[lo..n];
    let hi = i + 1_u64;
    let u = &deref(w)[i..hi];
    let painted = bump(output: u, mark: 1_u64);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let n = 200000_u64;
  let last = n - 1_u64;
  let values = box_array_filled::<u64>(count: n, value: 0_u64);
  if n <= values.inner.len {
    shifted_inline(values: &values, n: n);
  }
  if last < values.inner.len {
    let once = values.inner[last];
    if once == n {
    } else {
      return std::process::exit_status(code: 1_u8);
    }
  }
  if n <= values.inner.len {
    shifted_bound(values: &values, n: n);
  }
  if last < values.inner.len {
    let twice = values.inner[last];
    let expected = n + n;
    if twice == expected {
      return std::process::exit_status(code: 0_u8);
    }
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 3_u8);
}
"#;

/// [PAR-2] a range formed from a source bound inside the loop body is no
/// proved range reference, so under the shipped `--par` policy both loops stay
/// source-ordered and no update to the shared element is lost at any width.
/// A grant here was observed to lose updates at two or more workers in every
/// run. The oracle is the source-order count, not a second build.
#[test]
fn a_range_cut_from_a_per_iteration_origin_keeps_every_update() {
    let parallel = build_program(&super::support::compile_sources_with_cli_parallel_defaults(
        &[("shifting_origin.wf", SHIFTING_ORIGIN_PROGRAM.as_bytes())],
    ));
    for workers in ["1", "2", "4", "8"] {
        let output = parallel.run_with_workers(Some(workers));
        assert_eq!(
            output.status.code(),
            Some(0),
            "workers={workers}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "workers={workers}: {output:?}");
    }
}

/// One ordinary and one parallel build cover the real default-grain fold and
/// unhooked runtime reports. The reference is independent Rust arithmetic.
#[test]
fn range_fold_preserves_bytes_and_ordinary_runtime_reports() {
    let expected = (0_u64..400_000)
        .fold(0_u64, |sum, seed| {
            let mixed = (0..24).fold(seed, |state, _| {
                (state.rotate_left(27) ^ state.wrapping_mul(6364136223846793005))
                    .wrapping_add(1442695040888963407)
            });
            sum.wrapping_add(mixed)
        })
        .to_le_bytes();
    let plain = build_program(&compile_program("parallel/range_fold.wf"));
    let reference = plain.run_with_workers(Some("1"));
    assert!(reference.status.success());
    assert_eq!(reference.stdout, expected);
    assert!(reference.stderr.is_empty());
    let parallel = build_program(&compile_program_with_overlap("parallel/range_fold.wf"));
    for workers in [None, Some("0"), Some("1"), Some("2"), Some("4"), Some("8")] {
        let output = parallel.run_with_workers(workers);
        assert!(output.status.success(), "workers={workers:?}: {output:?}");
        assert_eq!(output.stdout, expected, "workers={workers:?}");
        assert!(output.stderr.is_empty());
    }
    for workers in ["1", "4"] {
        for report in ["0", "1", "2"] {
            let output = parallel.run_with_settings(
                Some(workers),
                &[("WF_SPLIT_WORK", "60000"), ("WF_SCHED_REPORT", report)],
            );
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, expected);
            if report == "2" {
                let text = String::from_utf8_lossy(&output.stderr);
                assert_eq!(text.lines().count(), 1, "{text}");
                assert!(
                    text.starts_with(&format!("compute: threads={workers} ")),
                    "{text}"
                );
                let started = if workers == "1" { "0" } else { "3" };
                assert!(
                    text.contains(&format!("workers_started={started} ")),
                    "{text}"
                );
            } else {
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
    let disabled = parallel.run_with_settings(Some("4"), &[("WF_SPLIT_WORK", "0")]);
    assert!(disabled.status.success());
    assert_eq!(disabled.stdout, expected);
    for (name, value, expected_error) in [
        (
            "WF_SCHED_REPORT",
            "3",
            "whitefoot scheduler: WF_SCHED_REPORT must be an integer from 0 through 2\n",
        ),
        (
            "WF_SPLIT_WORK",
            "no",
            "whitefoot scheduler: WF_SPLIT_WORK must be an integer from 0 through 1000000000\n",
        ),
    ] {
        let output = parallel.run_with_settings(Some("4"), &[(name, value)]);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            output.stdout.is_empty(),
            "configuration must be refused before WF output"
        );
        assert_eq!(String::from_utf8_lossy(&output.stderr), expected_error);
    }
}

/// The parallel lowering linked from the link fragments a modular build
/// splits it into [MOD-8] publishes the same results: its thunks, recursion
/// budget entries and runtime fallbacks each keep one definition across the
/// fragment boundaries.
#[test]
fn layout_linked_from_its_fragments_preserves_both_results() {
    let expected = b"420a993efa7437a1 41fa962893d45299\n";
    let llvm = compile_program_with_overlap("par_layout.wf");
    for granularity in [FragmentGranularity::Function, FragmentGranularity::Module] {
        let program = build_program_from_fragments(&llvm, granularity);
        for workers in [None, Some("1"), Some("2")] {
            let output = program.run_with_workers(workers);
            assert!(
                output.status.success(),
                "{granularity:?} workers={workers:?}: {output:?}"
            );
            assert_eq!(
                output.stdout, expected,
                "{granularity:?} workers={workers:?}"
            );
            assert!(
                output.stderr.is_empty(),
                "{granularity:?} workers={workers:?}"
            );
        }
    }
}

/// Each fold overwrites its prior output without reading it. The former
/// 800-pass fixture published only the final exactly representable seed
/// (16 + 799/16 = 65.9375). Check both complete established result words.
#[test]
fn layout_preserves_both_results_without_benchmark_repetition() {
    let expected = b"420a993efa7437a1 41fa962893d45299\n";
    let plain = build_program(&compile_program("par_layout.wf"));
    let output = plain.run_with_workers(Some("1"));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
    let parallel = build_program(&compile_program_with_overlap("par_layout.wf"));
    for workers in [None, Some("1"), Some("2"), Some("4")] {
        let output = parallel.run_with_workers(workers);
        assert!(output.status.success(), "workers={workers:?}: {output:?}");
        assert_eq!(output.stdout, expected, "workers={workers:?}");
        assert!(output.stderr.is_empty());
    }
}
