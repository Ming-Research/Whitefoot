//! Generate matched source forms and record current compiler verdicts.
//! Run through the command in DESIGN.md; all outputs go to its scratch path.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PRODUCER: &str = r#"fn selected(value: own i32) -> result: own Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}"#;

const GUARD: &str = r#"fn guard(left: own i32, right: own i32) -> result: own unit pure contract {
  requires left == right;
} {
  return unit;
}"#;

const ENTRY: &str = r#"fn main() -> status: own ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#;

struct Case {
    name: &'static str,
    expected_rule: Option<&'static str>,
    source: String,
}

fn indent(source: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn matched(scrutinee: &str, success: &str) -> String {
    format!(
        "match {scrutinee} {{\n  Ok(value: payload) => {{\n{}\n  }}\n  Err(error: problem) => {{\n  }}\n}}",
        indent(success, 4)
    )
}

fn consumer(body: &str) -> String {
    format!(
        "{PRODUCER}\n\n{GUARD}\n\nfn consumer(input: own i32, choose: own Bool) -> result: own Result<unit, Overflow> pure {{\n{}\n  return Ok<unit, Overflow>(value: unit);\n}}\n\n{ENTRY}",
        indent(body, 2)
    )
}

fn wrapper(body: &str) -> String {
    format!(
        "{PRODUCER}\n\nfn relay(input: own i32) -> result: own Result<i32, Overflow> pure contract {{\n  ensures when Ok(value: payload): payload == input;\n}} {{\n{}\n}}\n\n{ENTRY}",
        indent(body, 2)
    )
}

fn indexed(body: &str) -> String {
    format!(
        "fn bounded(count: own u64) -> result: own Result<u64, Overflow> pure contract {{\n  requires count > 0_u64;\n  ensures when Ok(value: index): index < count;\n}} {{\n  return Ok<u64, Overflow>(value: 0_u64);\n}}\n\nfn read(run: &Slots<u8, 4>) -> result: own Result<unit, Overflow> reads(run) contract {{\n  requires deref(run).len > 0_u64;\n}} {{\n{}\n  return Ok<unit, Overflow>(value: unit);\n}}\n\n{ENTRY}",
        indent(body, 2)
    )
}

fn cases() -> Vec<Case> {
    let use_payload = "guard(left: payload, right: input);";
    let named = matched("outcome", use_payload);
    let mut cases = vec![
        Case {
            name: "direct-match",
            expected_rule: None,
            source: consumer(&matched("selected(value: input)", use_payload)),
        },
        Case {
            name: "named-match",
            expected_rule: Some("FN-8"),
            source: consumer(&format!("let outcome = selected(value: input);\n{named}")),
        },
        Case {
            name: "copied-match",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let outcome = selected(value: input);\nlet retained = outcome;\n{}",
                matched("retained", use_payload)
            )),
        },
        Case {
            name: "direct-propagate",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let payload = propagate selected(value: input);\n{use_payload}"
            )),
        },
        Case {
            name: "named-propagate",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let outcome = selected(value: input);\nlet payload = propagate outcome;\n{use_payload}"
            )),
        },
        Case {
            name: "value-match-delivery",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let payload = match selected(value: input) {{\n  Ok(value: retained) => {{\n    give retained;\n  }}\n  Err(error: problem) => {{\n    return Err<unit, Overflow>(error: problem);\n  }}\n}}\n{use_payload}"
            )),
        },
        Case {
            name: "direct-unrelated-statement",
            expected_rule: None,
            source: consumer(&matched(
                "selected(value: input)",
                &format!("let unrelated = 0_i32;\n{use_payload}"),
            )),
        },
        Case {
            name: "direct-stale-scalar",
            expected_rule: Some("FN-8"),
            source: consumer(&matched(
                "selected(value: input)",
                &format!("set input = 0_i32;\n{use_payload}"),
            )),
        },
        Case {
            name: "overwritten-outcome",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let outcome = selected(value: input);\nset outcome = Ok<i32, Overflow>(value: 0_i32);\n{named}"
            )),
        },
        Case {
            name: "mixed-join",
            expected_rule: Some("FN-8"),
            source: consumer(&format!(
                "let outcome = if choose {{\n  give selected(value: input);\n}} else {{\n  give Ok<i32, Overflow>(value: 0_i32);\n}}\n{named}"
            )),
        },
        Case {
            name: "wrapper-direct-return",
            expected_rule: Some("FN-9"),
            source: wrapper("return selected(value: input);"),
        },
        Case {
            name: "wrapper-named-return",
            expected_rule: Some("FN-9"),
            source: wrapper("let outcome = selected(value: input);\nreturn outcome;"),
        },
        Case {
            name: "wrapper-rebuild",
            expected_rule: None,
            source: wrapper(
                "match selected(value: input) {\n  Ok(value: retained) => {\n    return Ok<i32, Overflow>(value: retained);\n  }\n  Err(error: problem) => {\n    return Err<i32, Overflow>(error: problem);\n  }\n}",
            ),
        },
    ];
    let owned_source = consumer(&format!(
        "let outcome = selected(value: input);\nlet retained = move outcome;\n{}",
        matched("retained", use_payload)
    ))
    .replace("Overflow", "Problem");
    cases.push(Case {
        name: "moved-affine-outcome",
        expected_rule: Some("FN-8"),
        source: format!("nocopy enum Problem {{\n  Failed();\n}}\n\n{owned_source}"),
    });
    let identity = "fn identity(value: own i32) -> result: own i32 pure contract {\n  ensures result == value;\n} {\n  return value;\n}";
    cases.push(Case {
        name: "plain-result-copy-control",
        expected_rule: None,
        source: format!(
            "{identity}\n\n{}",
            consumer("let produced = identity(value: input);\nlet payload = produced;\nguard(left: payload, right: input);")
        ),
    });
    cases.push(Case {
        name: "value-if-delivery-control",
        expected_rule: None,
        source: format!(
            "{identity}\n\n{}",
            consumer("let produced = identity(value: input);\nlet payload = if choose {\n  give produced;\n} else {\n  give produced;\n}\nguard(left: payload, right: input);")
        ),
    });
    cases.extend([
        Case {
            name: "index-direct-match",
            expected_rule: None,
            source: indexed(&matched(
                "bounded(count: deref(run).len)",
                "let observed = deref(run)[payload];",
            )),
        },
        Case {
            name: "index-named-match",
            expected_rule: Some("OP-4"),
            source: indexed(&format!(
                "let outcome = bounded(count: deref(run).len);\n{}",
                matched("outcome", "let observed = deref(run)[payload];")
            )),
        },
        Case {
            name: "index-direct-propagate",
            expected_rule: Some("OP-4"),
            source: indexed("let index = propagate bounded(count: deref(run).len);\nlet observed = deref(run)[index];"),
        },
    ]);
    cases
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 6 && args[3] == "--compare" {
        return compare_cost(&args[1], &args[4], Path::new(&args[5]), Path::new(&args[2]));
    }
    if args.len() == 4 && args[3] == "--scale" {
        return scale(&args[1], Path::new(&args[2]));
    }
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--candidate")) {
        return Err("usage: probe COMPILER SCRATCH_DIRECTORY [--candidate|--scale|--compare OTHER_COMPILER REPOSITORY]".into());
    }
    let scratch = Path::new(&args[2]);
    fs::create_dir_all(scratch)?;
    let mut results = String::from("case,expected,exit,matched\n");
    let mut failures = Vec::new();
    let mut inputs = cases();
    if args.len() == 4 {
        for case in &mut inputs {
            if !matches!(
                case.name,
                "direct-stale-scalar" | "overwritten-outcome" | "mixed-join"
            ) {
                case.expected_rule = None;
            }
        }
        inputs.extend(candidate_cases());
    }
    for case in inputs {
        let source = scratch.join(format!("{}.wf", case.name));
        fs::write(&source, case.source)?;
        let output = Command::new(&args[1])
            .arg("--diagnostic-format")
            .arg("json")
            .arg("--emit-llvm")
            .arg("-o")
            .arg(scratch.join(format!("{}.ll", case.name)))
            .arg(&source)
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        // One JSON object per stop: the cited rule and the semantic stage.
        let matched = match case.expected_rule {
            None => output.status.success(),
            Some(rule) => {
                output.status.code() == Some(1)
                    && stderr.contains(&format!("\"rule\":\"{rule}\""))
                    && stderr.contains("\"stage\":\"Semantics\"")
            }
        };
        fs::write(
            scratch.join(format!("{}.stderr", case.name)),
            &output.stderr,
        )?;
        fs::write(
            scratch.join(format!("{}.stdout", case.name)),
            &output.stdout,
        )?;
        results.push_str(&format!(
            "{},{},{},{}\n",
            case.name,
            case.expected_rule.unwrap_or("accept"),
            output
                .status
                .code()
                .map_or("signal".to_owned(), |n| n.to_string()),
            matched
        ));
        if !matched {
            failures.push(format!("{}: {}\n{stderr}", case.name, output.status));
        }
    }
    fs::write(scratch.join("results.csv"), &results)?;
    print!("{results}");
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    Ok(())
}

/// Explicit research only: source construction precedes each timed compiler
/// invocation; no build, linker or executable run is included. The CLI still
/// performs parsing, checking and LLVM emission, so this is compilation cost,
/// not an isolated entailment timer. The same sources are valid at the baseline.
fn scale(compiler: &str, scratch: &Path) -> Result<(), Box<dyn Error>> {
    if std::env::consts::OS != "macos" {
        return Err("this measurement uses macOS /usr/bin/time -l RSS bytes".into());
    }
    fs::create_dir_all(scratch)?;
    let mut csv = String::from("axis,size,repeat,source_bytes,wall_ms,peak_rss_bytes\n");
    for axis in ["copies", "outcomes", "joins"] {
        for count in [4, 8, 16, 32] {
            let source = scale_source(axis, count);
            let path = scratch.join(format!("{axis}-{count}.wf"));
            fs::write(&path, &source)?;
            for repeat in 0..3 {
                let start = std::time::Instant::now();
                let output = Command::new("/usr/bin/time")
                    .arg("-l")
                    .arg(compiler)
                    .arg("--emit-llvm")
                    .arg("-o")
                    .arg(scratch.join(format!("{axis}-{count}.ll")))
                    .arg(&path)
                    .output()?;
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                let stderr = String::from_utf8_lossy(&output.stderr);
                fs::write(
                    scratch.join(format!("{axis}-{count}-{repeat}.stderr")),
                    &output.stderr,
                )?;
                if !output.status.success() {
                    return Err(format!("{axis}/{count}: {stderr}").into());
                }
                let rss = stderr
                    .lines()
                    .find(|line| line.contains("maximum resident set size"))
                    .and_then(|line| line.split_whitespace().next())
                    .ok_or("missing peak RSS")?
                    .parse::<u64>()?;
                let row = format!(
                    "{axis},{count},{repeat},{},{wall_ms:.3},{rss}\n",
                    source.len()
                );
                print!("{row}");
                csv.push_str(&row);
                fs::write(scratch.join("scale.csv"), &csv)?;
            }
        }
    }
    Ok(())
}

fn scale_source(axis: &str, count: usize) -> String {
    let mut body = String::from("let outcome0 = selected(value: input);\n");
    for index in 1..=count {
        let previous = index - 1;
        body.push_str(&match axis {
            "copies" => format!("let outcome{index} = outcome{previous};\n"),
            "outcomes" => format!("let outcome{index} = selected(value: input);\n"),
            "joins" => format!(
                "let outcome{index} = if choose {{\n  give outcome{previous};\n}} else {{\n  give selected(value: input);\n}}\n"
            ),
            _ => unreachable!(),
        });
    }
    // Both compilers can accept this source without the transported relation.
    // The correctness probes separately require the recovered proof authority.
    body.push_str(&matched(
        &format!("outcome{count}"),
        "let observed = payload;",
    ));
    consumer(&body)
}

/// Explicit research comparison; never called by correctness CI. Construction
/// precedes timing. One warmup is followed by five alternating paired rounds.
fn compare_cost(
    before: &str,
    after: &str,
    repository: &Path,
    scratch: &Path,
) -> Result<(), Box<dyn Error>> {
    if std::env::consts::OS != "macos" {
        return Err("this measurement uses macOS /usr/bin/time -l RSS bytes".into());
    }
    fs::create_dir_all(scratch)?;
    let repository = repository.canonicalize()?;
    let scratch = scratch.canonicalize()?;
    let programs: &[(&str, &[&str])] = &[
        ("dense-control", &["containers/dense.wf"]),
        ("grayscale", &["grayscale_pixels.wf"]),
        ("telemetry", &["telemetry_packet.wf"]),
        ("prefix-expression", &["prefix_expression.wf"]),
        ("wfgrep", &["wfgrep.wf"]),
        (
            "raw-deflate",
            &[
                "raw_deflate.wf",
                "raw_deflate_dynamic.wf",
                "raw_deflate_dynamic_decode.wf",
                "raw_deflate_vectors.wf",
            ],
        ),
    ];
    let mut workloads: Vec<(String, Vec<PathBuf>)> = programs
        .iter()
        .map(|(name, sources)| {
            (
                name.to_string(),
                sources
                    .iter()
                    .map(|path| repository.join("tests/programs").join(path))
                    .collect(),
            )
        })
        .collect();
    for axis in ["copies", "outcomes", "joins"] {
        for count in [4, 8, 16, 32] {
            let name = format!("{axis}-{count}");
            let path = scratch.join(format!("{name}.wf"));
            fs::write(&path, scale_source(axis, count))?;
            workloads.push((name, vec![path]));
        }
    }
    let mut csv =
        String::from("workload,compiler,round,warmup,wall_ms,peak_rss_bytes,source_bytes\n");
    for (name, paths) in workloads {
        let bytes: u64 = paths
            .iter()
            .map(fs::metadata)
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(|metadata| metadata.len())
            .sum();
        let mut samples: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
        let mut memory = [0_u64; 2];
        for round in 0..=5 {
            let order = if round % 2 == 0 { [0, 1] } else { [1, 0] };
            for index in order {
                let compiler = [before, after][index];
                let side = ["before", "after"][index];
                let started = std::time::Instant::now();
                let output = Command::new("/usr/bin/time")
                    .arg("-l")
                    .arg(compiler)
                    .arg("--emit-llvm")
                    .arg("-o")
                    .arg(scratch.join(format!("{name}-{side}.ll")))
                    .args(&paths)
                    .output()?;
                let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
                let stderr = String::from_utf8_lossy(&output.stderr);
                fs::write(
                    scratch.join(format!("{name}-{side}-{round}.stderr")),
                    &output.stderr,
                )?;
                if !output.status.success() {
                    return Err(format!("{name}/{side}: {stderr}").into());
                }
                let rss = stderr
                    .lines()
                    .find(|line| line.contains("maximum resident set size"))
                    .and_then(|line| line.split_whitespace().next())
                    .ok_or("missing peak RSS")?
                    .parse::<u64>()?;
                csv.push_str(&format!(
                    "{name},{side},{round},{},{wall_ms:.3},{rss},{bytes}\n",
                    round == 0
                ));
                fs::write(scratch.join("comparison.csv"), &csv)?;
                if round > 0 {
                    samples[index].push(wall_ms);
                    memory[index] = memory[index].max(rss);
                }
            }
            if fs::read(scratch.join(format!("{name}-before.ll")))?
                != fs::read(scratch.join(format!("{name}-after.ll")))?
            {
                return Err(format!("{name}: emitted LLVM differs in round {round}").into());
            }
        }
        for values in &mut samples {
            values.sort_by(f64::total_cmp);
        }
        println!(
            "{name}: {:.3} [{:.3}, {:.3}] -> {:.3} [{:.3}, {:.3}] ms, {:.2} -> {:.2} MiB",
            samples[0][2],
            samples[0][0],
            samples[0][4],
            samples[1][2],
            samples[1][0],
            samples[1][4],
            memory[0] as f64 / 1_048_576.0,
            memory[1] as f64 / 1_048_576.0
        );
    }
    Ok(())
}

fn candidate_cases() -> Vec<Case> {
    let use_payload = "guard(left: payload, right: input);";
    let mut cases = Vec::new();
    for (name, body, expected_rule) in [
        (
            "saved-copy-after-replacement",
            format!(
                "let outcome = selected(value: input);\nlet retained = outcome;\nset outcome = Ok<i32, Overflow>(value: 0_i32);\n{}",
                matched("retained", use_payload)
            ),
            None,
        ),
        (
            "set-call-result",
            format!(
                "let outcome = Ok<i32, Overflow>(value: 0_i32);\nset outcome = selected(value: input);\n{}",
                matched("outcome", use_payload)
            ),
            None,
        ),
        (
            "set-copied-result",
            format!(
                "let outcome = Ok<i32, Overflow>(value: 0_i32);\nlet retained = selected(value: input);\nset outcome = retained;\n{}",
                matched("outcome", use_payload)
            ),
            None,
        ),
        (
            "same-relation-join",
            format!(
                "let outcome = if choose {{\n  give selected(value: input);\n}} else {{\n  give selected(value: input);\n}}\n{}",
                matched("outcome", use_payload)
            ),
            None,
        ),
        (
            "constructor-through-binding",
            format!(
                "let outcome = Ok<i32, Overflow>(value: input);\n{}",
                matched("outcome", use_payload)
            ),
            None,
        ),
        (
            "delayed-stale-scalar",
            format!(
                "let outcome = selected(value: input);\nset input = 0_i32;\n{}",
                matched("outcome", use_payload)
            ),
            Some("FN-8"),
        ),
        (
            "distinct-outcomes-isolated",
            format!(
                "let retained = selected(value: input);\nlet outcome = selected(value: 0_i32);\n{}",
                matched("outcome", use_payload)
            ),
            Some("FN-8"),
        ),
        (
            "unselected-guard-isolated",
            "let outcome = selected(value: input);\nguard(left: input, right: 0_i32);".to_owned(),
            Some("FN-8"),
        ),
        (
            "loop-replaced-outcome",
            format!(
                "let outcome = selected(value: input);\nfor (counter in 0_u64..2_u64) {{\n{}\n  set outcome = Ok<i32, Overflow>(value: 0_i32);\n}}",
                indent(&matched("outcome", use_payload), 2)
            ),
            Some("FN-8"),
        ),
        (
            "loop-local-production",
            format!(
                "for (counter in 0_u64..2_u64) {{\n  let outcome = selected(value: input);\n{}\n  set input = 0_i32;\n}}",
                indent(&matched("outcome", use_payload), 2)
            ),
            None,
        ),
        (
            "loop-unchanged-outcome",
            format!(
                "let outcome = selected(value: input);\nfor (counter in 0_u64..2_u64) {{\n{}\n}}",
                indent(&matched("outcome", use_payload), 2)
            ),
            None,
        ),
        (
            "alias-replaces-outcome",
            format!(
                "let outcome = selected(value: input);\nlet alias = &outcome;\nset deref(alias) = Ok<i32, Overflow>(value: 0_i32);\n{}",
                matched("outcome", use_payload)
            ),
            Some("FN-8"),
        ),
        (
            "alias-replaces-source-of-copy",
            format!(
                "let outcome = selected(value: input);\nlet retained = outcome;\nlet alias = &outcome;\nset deref(alias) = Ok<i32, Overflow>(value: 0_i32);\n{}",
                matched("retained", use_payload)
            ),
            None,
        ),
    ] {
        cases.push(Case {
            name,
            expected_rule,
            source: consumer(&body),
        });
    }
    let weaker = "fn smaller() -> result: own Result<i32, Overflow> pure contract {\n  ensures when Ok(value: payload): payload < 8_i32;\n} {\n  return Ok<i32, Overflow>(value: 0_i32);\n}\n\nfn larger() -> result: own Result<i32, Overflow> pure contract {\n  ensures when Ok(value: payload): payload < 10_i32;\n} {\n  return Ok<i32, Overflow>(value: 0_i32);\n}\n\nfn bounded(value: own i32) -> result: own unit pure contract {\n  requires value < 10_i32;\n} {\n  return unit;\n}\n\nfn consumer(choose: own Bool) -> result: own Result<unit, Overflow> pure {\n  let outcome = if choose {\n    give smaller();\n  } else {\n    give larger();\n  }\n  let value = propagate outcome;\n  bounded(value: value);\n  return Ok<unit, Overflow>(value: unit);\n}\n\n";
    cases.push(Case {
        name: "weaker-bound-join",
        expected_rule: None,
        source: format!("{weaker}{ENTRY}"),
    });
    cases.push(Case {
        name: "stronger-bound-join-refused",
        expected_rule: Some("FN-8"),
        source: format!(
            "{}{ENTRY}",
            weaker.replace("requires value < 10_i32;", "requires value < 8_i32;")
        ),
    });
    cases.push(Case {
        name: "wrapper-stronger-contract-refused",
        expected_rule: Some("FN-9"),
        source: wrapper("return selected(value: input);").replace(
            "ensures when Ok(value: payload): payload == input;",
            "ensures when Ok(value: payload): payload < input;",
        ),
    });
    cases
}
