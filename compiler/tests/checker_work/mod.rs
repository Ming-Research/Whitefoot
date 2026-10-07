//! The opt-in work instrument uses the ordinary compiler subprocess so the
//! process environment is isolated from concurrently running Rust tests.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use crate::programs::support::fixture_directory;
use crate::support::run_command;

const MATCH: &str = r#"enum Op {
  First();
  Second();
  Third();
}

fn choose(op: Op) -> out: u64 pure {
  doc "Exercise work attribution at a multi-input numeric join.";
  let acc = 0_u64;
  match op {
    First() => {
      set acc = 1_u64;
    }
    Second() => {
      set acc = 2_u64;
    }
    Third() => {
      set acc = 3_u64;
    }
  }
  return acc;
}
"#;

const LOOP: &str = r#"enum Op {
  First();
  Second();
}

fn repeat(op: Op, stop: Bool) -> out: u64 pure {
  doc "Attribute a numeric join inside an ordinary dispatch loop.";
  let acc = 0_u64;
  loop {
    match op {
      First() => {
        set acc = 1_u64;
      }
      Second() => {
        set acc = 2_u64;
      }
    }
    if stop {
      break;
    }
  }
  return acc;
}
"#;

const SINK: &str = r#"enum Op {
  Reset();
  Step();
  Halt();
}

fn repeat(op: Op, limit: u64) -> out: u64 pure {
  doc "Every arm reaches the next loop header directly.";
  let acc = 0_u64;
  loop (
    invariant bounded: acc <= limit
  ) {
    match op {
      Reset() => {
        set acc = 0_u64;
      }
      Step() => {
        if acc < limit {
          set acc = acc + 1_u64;
        }
      }
      Halt() => {
        return acc;
      }
    }
  }
  return acc;
}
"#;

fn check(source: &Path, output: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_whitefootc"));
    command
        .arg("--check")
        .arg(source)
        .env_remove("WHITEFOOT_CHECK_WORK");
    if let Some(output) = output {
        command.env("WHITEFOOT_CHECK_WORK", output);
    }
    run_command(&mut command)
}

fn same_verdict_and_diagnostic(expected: &Output, actual: &Output) {
    assert_eq!(actual.status.code(), expected.status.code());
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stderr, expected.stderr);
}

fn check_counters(text: &str, function: &str) {
    let mut runs = BTreeMap::<u64, BTreeMap<(&str, usize, &str), u64>>::new();
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 7, "malformed work row: {line}");
        assert!(fields[0].parse::<u32>().unwrap() > 0);
        let run = fields[1].parse::<u64>().unwrap();
        let ordinal = fields[4].parse::<usize>().unwrap();
        let value = fields[6].parse::<u64>().unwrap();
        if fields[2] == function {
            assert!(
                runs.entry(run)
                    .or_default()
                    .insert((fields[3], ordinal, fields[5]), value,)
                    .is_none(),
                "duplicate metric: {line}"
            );
        }
    }
    assert!(!runs.is_empty(), "no counters for {function}: {text}");
    for metrics in runs.values() {
        let get = |kind, ordinal, metric| {
            *metrics
                .get(&(kind, ordinal, metric))
                .expect("missing work counter")
        };
        let joins = get("function", 0, "joins");
        let passes = get("function", 0, "join_passes");
        assert!(joins > 0);
        assert!(passes >= joins);
        let mut parent_references = 0;
        let mut intern_calls = 0;
        let mut widest_input = 0;
        for pass in 0..passes as usize {
            let join = get("join", pass, "join");
            assert!(join > 0 && join <= joins);
            widest_input = widest_input.max(get("join", pass, "inputs"));
            let rows = get("join", pass, "union_rows");
            let pairs = get("join", pass, "pairs_evaluated");
            assert!(rows > 0);
            assert_eq!(pairs, rows * rows);
            let retained = get("join", pass, "pairs_retained");
            assert!(retained > 0 && retained <= pairs);
            parent_references += get("join", pass, "parent_references");
            intern_calls += get("join", pass, "intern_calls");
        }
        assert!(widest_input >= 2);
        assert!(parent_references > 0);
        assert!(intern_calls > 0);
        assert!(get("function", 0, "intern_calls") >= intern_calls);
        for kind in ["snapshot", "closure"] {
            assert!(get(kind, 0, "count") > 0);
            assert!(get(kind, 0, "rows_max") > 0);
            assert!(get(kind, 0, "rows_sum") >= get(kind, 0, "rows_max"));
            assert!(get(kind, 0, "cells_max") > 0);
            assert!(get(kind, 0, "cells_sum") >= get(kind, 0, "cells_max"));
        }
        // Seeded states bypass the proof-free probe, so these bodies need
        // not perform one. Even a zero count must have the complete schema.
        for metric in ["count", "rows_sum", "rows_max", "cells_sum", "cells_max"] {
            get("probe", 0, metric);
        }
        get("function", 0, "closure_cache_hits");
    }
}

#[test]
fn checker_work_is_opt_in_and_preserves_verdicts() {
    let directory = fixture_directory();
    let negative = MATCH
        .replace("fn choose(op: Op)", "fn choose(op: Op, bump: u64)")
        .replace("  return acc;", "  let next = acc + bump;\n  return next;");
    for (index, (source, function, accepts)) in [
        (MATCH, "choose", true),
        (LOOP, "repeat", true),
        (negative.as_str(), "choose", false),
    ]
    .into_iter()
    .enumerate()
    {
        let path = directory.path().join(format!("case-{index}.wf"));
        let counters = directory.path().join(format!("case-{index}.tsv"));
        std::fs::write(&path, source).unwrap();
        let disabled = check(&path, None);
        assert_eq!(disabled.status.success(), accepts, "{disabled:?}");
        if !accepts {
            assert!(String::from_utf8_lossy(&disabled.stderr).contains("OP-2"));
        }
        assert!(!counters.exists(), "unset instrumentation created output");
        let enabled = check(&path, Some(&counters));
        same_verdict_and_diagnostic(&disabled, &enabled);
        let text = std::fs::read_to_string(&counters).expect("enabled counters were not written");
        check_counters(&text, function);
        // No append from an invocation with the variable absent, even when a
        // previous process used that output path.
        same_verdict_and_diagnostic(&disabled, &check(&path, None));
        assert_eq!(std::fs::read_to_string(&counters).unwrap(), text);
    }
    // An unavailable output is an observation failure, not a source verdict.
    let path = directory.path().join("case-0.wf");
    let disabled = check(&path, None);
    let unavailable = check(&path, Some(directory.path()));
    assert_eq!(unavailable.status.code(), disabled.status.code());
    assert_eq!(unavailable.stdout, disabled.stdout);
    assert!(
        String::from_utf8_lossy(&unavailable.stderr).contains("checker work output unavailable")
    );
}

/// The input counts of one function's numeric joins that combine at least one
/// state, per analysis run. A loop without a `break` still joins its empty
/// exit set; that join has no input and is not counted.
fn joined_inputs(text: &str, function: &str) -> BTreeMap<u64, Vec<u64>> {
    let mut runs = BTreeMap::<u64, Vec<u64>>::new();
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields[2] != function {
            continue;
        }
        let entry = runs.entry(fields[1].parse::<u64>().unwrap()).or_default();
        if fields[3] == "join" && fields[5] == "inputs" {
            let inputs = fields[6].parse::<u64>().unwrap();
            if inputs > 0 {
                entry.push(inputs);
            }
        }
    }
    runs
}

/// [ENT-5, INV-1] Arms that all end at the loop's next header are each proved
/// on their own edge, so no numeric join combines them. The same loop with one
/// shared statement after the `match` joins its three inputs once, at the
/// canonical frontier, instead of once per nested merge. Before per-edge
/// induction both programs were refused: the guarded update's join lost
/// `acc <= limit`.
#[test]
fn an_induction_sink_joins_nothing_and_a_shared_suffix_joins_its_frontier_once() {
    let directory = fixture_directory();
    let suffix = SINK
        .replace(
            "Every arm reaches the next loop header directly.",
            "Every arm reaches one shared statement before the next loop header.",
        )
        .replace(
            "        return acc;\n      }\n    }\n  }",
            "        return acc;\n      }\n    }\n    let seen = acc;\n  }",
        );
    assert_ne!(suffix, SINK);
    for (index, (source, expected)) in [(SINK, vec![]), (suffix.as_str(), vec![3])]
        .into_iter()
        .enumerate()
    {
        let path = directory.path().join(format!("sink-{index}.wf"));
        let counters = directory.path().join(format!("sink-{index}.tsv"));
        std::fs::write(&path, source).unwrap();
        let output = check(&path, Some(&counters));
        assert!(output.status.success(), "{output:?}");
        let text = std::fs::read_to_string(&counters).expect("counters were written");
        let runs = joined_inputs(&text, "repeat");
        assert!(!runs.is_empty(), "no counters for repeat: {text}");
        for inputs in runs.values() {
            assert_eq!(inputs, &expected, "{text}");
        }
    }
}
