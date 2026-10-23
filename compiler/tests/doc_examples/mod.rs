use std::any::Any;
use std::path::{Path, PathBuf};

use whitefoot::{CompilerLimits, DiagnosticFormat, SourceInput, check, compile};

use crate::programs::support::build_program;

#[derive(Debug, Eq, PartialEq)]
enum Expectation {
    Run(u8),
    Accept,
    Reject(String),
}

struct Example {
    expectation: Expectation,
    file: String,
}

struct Fence {
    info: String,
    body: String,
    opening: usize,
    closing: usize,
    closed: bool,
    error: Option<&'static str>,
}

#[derive(Clone)]
enum Container {
    Quote,
    List(usize),
}

fn container_start(line: &str) -> Option<(Container, &str)> {
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return None;
    }
    let content = &line[indentation..];
    if let Some(content) = content.strip_prefix('>') {
        return Some((
            Container::Quote,
            content.strip_prefix(' ').unwrap_or(content),
        ));
    }
    let marker_width = match content.as_bytes().first()? {
        b'-' | b'+' | b'*' => 1,
        b'0'..=b'9' => {
            let digits = content.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 9 || !matches!(content.as_bytes().get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let padding = content[marker_width..]
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    if padding == 0 {
        return None;
    }
    // More than four spaces starts indented content after one marker space.
    let padding = if padding <= 4 { padding } else { 1 };
    let width = indentation + marker_width + padding;
    Some((Container::List(width), &line[width..]))
}

fn strip_containers<'a>(mut line: &'a str, containers: &[Container]) -> Option<&'a str> {
    for container in containers {
        match container {
            Container::Quote => {
                let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
                if indentation > 3 {
                    return None;
                }
                line = line[indentation..].strip_prefix('>')?;
                line = line.strip_prefix(' ').unwrap_or(line);
            }
            Container::List(width) => {
                let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
                if indentation < *width && !line.trim().is_empty() {
                    return None;
                }
                line = &line[indentation.min(*width)..];
            }
        }
    }
    Some(line)
}

fn markdown_blank(mut line: &str) -> bool {
    while let Some((Container::Quote, content)) = container_start(line) {
        line = content;
    }
    line.trim().is_empty()
}

fn fence_start(line: &str) -> Option<(u8, usize, usize, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return None;
    }
    let rest = &line[indentation..];
    let marker = *rest.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = rest.bytes().take_while(|byte| *byte == marker).count();
    if width < 3 {
        return None;
    }
    Some((marker, width, indentation, rest[width..].trim()))
}

fn fences(lines: &[&str]) -> Vec<Fence> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    let mut active = Vec::new();
    while cursor < lines.len() {
        let had_list = active
            .iter()
            .any(|container| matches!(container, Container::List(_)));
        let mut containers = active.clone();
        let mut content = loop {
            if let Some(content) = strip_containers(lines[cursor], &containers) {
                break content;
            }
            containers.pop();
        };
        while let Some((container, rest)) = container_start(content) {
            containers.push(container);
            content = rest;
        }
        if !lines[cursor].trim().is_empty() {
            active = containers.clone();
        }
        let Some((marker, width, indentation, info)) = fence_start(content) else {
            // Container forms this small scanner cannot interpret must fail
            // explicitly, rather than silently leave a Whitefoot program unchecked.
            if let Some(position) = content.find(['`', '~']) {
                let prefix = &content[..position];
                if (had_list
                    || !containers.is_empty()
                    || prefix.bytes().any(|byte| !byte.is_ascii_whitespace()))
                    && prefix.bytes().all(|byte| {
                        byte.is_ascii_whitespace()
                            || matches!(
                                byte,
                                b'>' | b'-' | b'+' | b'*' | b'.' | b')' | b'0'..=b'9'
                            )
                    })
                    && let Some((_, _, _, info)) = fence_start(&content[position..])
                    && matches!(
                        info.split_whitespace().next(),
                        Some("whitefoot" | "whitefoot-output")
                    )
                {
                    blocks.push(Fence {
                        info: info.to_owned(),
                        body: String::new(),
                        opening: cursor,
                        closing: cursor,
                        closed: false,
                        error: Some("cannot interpret this Markdown container indentation"),
                    });
                }
            }
            cursor += 1;
            continue;
        };
        // Backticks in a backtick fence's info string are not Markdown fences.
        if marker == b'`' && info.contains('`') {
            cursor += 1;
            continue;
        }
        let opening = cursor;
        let info = info.to_owned();
        let mut body = String::new();
        let mut closed = false;
        cursor += 1;
        while cursor < lines.len() {
            let Some(content) = strip_containers(lines[cursor], &containers) else {
                break;
            };
            if let Some((end_marker, end_width, _, end_info)) = fence_start(content)
                && end_marker == marker
                && end_width >= width
                && end_info.is_empty()
            {
                closed = true;
                break;
            }
            let removed = content
                .bytes()
                .take(indentation)
                .take_while(|byte| *byte == b' ')
                .count();
            body.push_str(&content[removed..]);
            cursor += 1;
        }
        blocks.push(Fence {
            info,
            body,
            opening,
            closing: cursor,
            closed,
            error: None,
        });
        if closed {
            cursor += 1;
        } else {
            active.clear();
        }
    }
    blocks
}

fn parse_example(info: &str) -> Result<Example, String> {
    let mut words = info.split_whitespace();
    if words.next() != Some("whitefoot") {
        return Err(format!("unknown Whitefoot fence info {info:?}"));
    }
    let mut expectation = None;
    let mut file = None;
    while let Some(word) = words.next() {
        let outcome = match word {
            "run" => {
                let value = words
                    .next()
                    .and_then(|word| word.strip_prefix("exit="))
                    .ok_or_else(|| "run requires exit=N (0..255)".to_owned())?;
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err("run requires exit=N (0..255)".to_owned());
                }
                let status = value
                    .parse::<u8>()
                    .map_err(|_| "run requires exit=N (0..255)".to_owned())?;
                Some(Expectation::Run(status))
            }
            "reject" => {
                let rule = words
                    .next()
                    .filter(|word| {
                        !matches!(*word, "accept" | "run" | "reject") && !word.contains('=')
                    })
                    .ok_or_else(|| "reject requires a rule ID".to_owned())?;
                Some(Expectation::Reject(rule.to_owned()))
            }
            attribute if attribute.starts_with("file=") => {
                let name = &attribute[5..];
                if name.is_empty()
                    || matches!(name, "." | "..")
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
                {
                    return Err("file= must name a nonempty ASCII basename".to_owned());
                }
                if file.replace(name.to_owned()).is_some() {
                    return Err("duplicate file= attribute".to_owned());
                }
                None
            }
            _ => return Err(format!("unknown attribute {word:?}")),
        };
        if let Some(outcome) = outcome
            && expectation.replace(outcome).is_some()
        {
            return Err("at most one outcome is allowed; found two outcomes".to_owned());
        }
    }
    Ok(Example {
        expectation: expectation.unwrap_or(Expectation::Accept),
        file: file.unwrap_or_else(|| "example.wf".to_owned()),
    })
}

fn check_example(example: &Example, body: &str, expected_output: Option<&str>) -> Vec<String> {
    let inputs = [SourceInput::new(&example.file, body.as_bytes())];
    if let Expectation::Run(status) = &example.expectation {
        let module = match crate::support::timed("whitefoot-compile", || {
            compile(&inputs, CompilerLimits::default())
        }) {
            Ok(module) => module,
            Err(failure) => {
                return vec![format!(
                    "expected run exit={status}; actual compilation failure:\n{}",
                    failure.render(DiagnosticFormat::Text)
                )];
            }
        };
        let program = build_program(&crate::support::sanitized(module));
        let output = program.run_with_settings(None, &[]);
        return if output.status.code() == Some(i32::from(*status)) {
            Vec::new()
        } else {
            vec![format!(
                "expected run exit={status}; actual status {} (code {:?}), stdout {:?}, stderr {:?}",
                output.status,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )]
        };
    }
    let result = crate::support::timed("whitefoot-check", || {
        check(&inputs, CompilerLimits::default())
    });
    match (&example.expectation, result) {
        (Expectation::Reject(rule), Err(failure)) => {
            let mut failures = Vec::new();
            // The CLI's Stop::Compilation delegates to this renderer and its
            // eprintln adds one LF. Preserve that LF and every displayed byte.
            let actual_output = format!("{}\n", failure.render(DiagnosticFormat::Text));
            if failure.rule_id() != Some(rule.as_str()) {
                failures.push(format!(
                    "expected reject {rule}; actual first rule {:?}\n{actual_output}",
                    failure.rule_id()
                ));
            }
            if let Some(expected_output) = expected_output
                && expected_output != actual_output
            {
                failures.push(format!(
                    "expected whitefoot-output bytes {expected_output:?}; actual bytes {actual_output:?}\nactual rejection:\n{actual_output}"
                ));
            }
            failures
        }
        (Expectation::Reject(rule), Ok(_)) => {
            vec![format!("expected reject {rule}; actual accepted")]
        }
        (expected, Err(failure)) => vec![format!(
            "expected {expected:?}; actual compilation failure:\n{}",
            failure.render(DiagnosticFormat::Text)
        )],
        (_, Ok(())) => Vec::new(),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else {
        "non-text panic"
    }
}

fn check_document(path: &Path, text: &str) -> Vec<String> {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let blocks = fences(&lines);
    let mut failures = Vec::new();
    let mut attached_output = None;
    for (index, block) in blocks.iter().enumerate() {
        let location = format!("{}:{}", path.display(), block.opening + 1);
        if let Some(error) = block.error {
            failures.push(format!(
                "{location}: expected a readable Whitefoot fence; actual {error}"
            ));
            continue;
        }
        let mut words = block.info.split_whitespace();
        let kind = words.next();
        if kind != Some("whitefoot")
            && let Some(attribute) = words.next()
        {
            failures.push(format!(
                "{location}: expected no attributes on {:?} fence; actual {attribute:?}",
                kind.unwrap_or("")
            ));
            continue;
        }
        if kind == Some("whitefoot-output") {
            if attached_output != Some(index) {
                failures.push(format!(
                    "{location}: expected whitefoot-output immediately after a whitefoot reject block; actual unattached output"
                ));
            }
            if !block.closed {
                failures.push(format!("{location}: expected closing fence; actual end of file"));
            }
            continue;
        }
        if kind != Some("whitefoot") {
            continue;
        }
        if !block.closed {
            failures.push(format!("{location}: expected closing fence; actual end of file"));
        }
        let example = match parse_example(&block.info) {
            Ok(example) => example,
            Err(error) => {
                failures.push(format!(
                    "{location}: expected valid whitefoot attributes; actual {error}"
                ));
                continue;
            }
        };
        let next = blocks.get(index + 1).filter(|next| {
            block.closed
                && next.info == "whitefoot-output"
                && lines[block.closing + 1..next.opening]
                    .iter()
                    .all(|line| markdown_blank(line))
        });
        let expected_output = match (&example.expectation, next) {
            (Expectation::Reject(_), Some(output)) => {
                attached_output = Some(index + 1);
                Some(output.body.as_str())
            }
            _ => None,
        };
        let result =
            std::panic::catch_unwind(|| check_example(&example, &block.body, expected_output));
        match result {
            Ok(errors) => failures.extend(
                errors
                    .into_iter()
                    .map(|error| format!("{location}: {error}")),
            ),
            Err(payload) => failures.push(format!(
                "{location}: expected {:?}; actual compiler/build/run panic: {}",
                example.expectation,
                panic_message(payload.as_ref())
            )),
        }
    }
    failures
}

fn documentation_files(root: &Path, failures: &mut Vec<String>) -> Vec<PathBuf> {
    let mut files = vec![root.join("README.md"), root.join("README.zh-CN.md")];
    let mut directories = vec![root.join("docs").join("guide")];
    while let Some(directory) = directories.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                failures.push(format!(
                    "{}: cannot scan documentation: {error}",
                    directory.display()
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    failures.push(format!(
                        "{}: cannot read directory entry: {error}",
                        directory.display()
                    ));
                    continue;
                }
            };
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn documentation_examples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package lives directly under the repository root");
    let mut failures = Vec::new();
    for path in documentation_files(root, &mut failures) {
        let relative = path
            .strip_prefix(root)
            .expect("documentation is in the repository");
        match std::fs::read_to_string(&path) {
            Ok(text) => failures.extend(check_document(relative, &text)),
            Err(error) => failures.push(format!(
                "{}: cannot read documentation: {error}",
                relative.display()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "documentation examples failed ({} failures):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn fence_outcomes_default_to_check_and_attributes_are_closed() {
    for (info, expectation, file) in [
        ("whitefoot run exit=0", Expectation::Run(0), "example.wf"),
        (
            "whitefoot run exit=255 file=first.wf",
            Expectation::Run(255),
            "first.wf",
        ),
        ("whitefoot", Expectation::Accept, "example.wf"),
        ("whitefoot file=library", Expectation::Accept, "library"),
        (
            "whitefoot file=bad.wf reject OP-4",
            Expectation::Reject("OP-4".to_owned()),
            "bad.wf",
        ),
    ] {
        let example = parse_example(info).unwrap_or_else(|error| panic!("{info}: {error}"));
        assert_eq!(example.expectation, expectation, "{info}");
        assert_eq!(example.file, file, "{info}");
    }
    for info in [
        "whitefoot accept",
        "whitefoot run exit=0 reject OP-4",
        "whitefoot reject OP-4 run exit=0",
        "whitefoot run",
        "whitefoot run exit=-1",
        "whitefoot run exit=256",
        "whitefoot reject",
        "whitefoot typo=value",
        "whitefoot file=../first.wf",
        "whitefoot file=first.wf file=second.wf",
        "whitefootish",
    ] {
        assert!(parse_example(info).is_err(), "{info} must fail");
    }
}

#[test]
fn markdown_fences_keep_exact_bytes_and_do_not_extract_nested_blocks() {
    let text = concat!(
        "````text\n```whitefoot\nnot a program\n```\n````\n\n",
        "~~~whitefoot reject OP-4\nsource\n~~~\n\n",
        "  ```whitefoot-output\n  first.wf:1:1: error[OP-4]: Kind\n  ```\n",
    );
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let blocks = fences(&lines);
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].info, "text");
    assert_eq!(blocks[0].body, "```whitefoot\nnot a program\n```\n");
    assert_eq!(blocks[1].info, "whitefoot reject OP-4");
    assert_eq!(blocks[1].body, "source\n");
    assert_eq!(blocks[2].body, "first.wf:1:1: error[OP-4]: Kind\n");
    assert!(blocks.iter().all(|block| block.closed));

    let invalid = "```whitefoot unexpected=value\nsource\n```\n\n~~~whitefoot-output\nwrong\n~~~\n";
    let failures = check_document(Path::new("fixture.md"), invalid);
    assert_eq!(failures.len(), 2, "every malformed block is reported");
    assert!(failures[0].contains("fixture.md:1"));
    assert!(failures[0].contains("unknown attribute"));
    assert!(failures[1].contains("fixture.md:5"));
    assert!(failures[1].contains("unattached output"));

    let unclosed = fences(&["```whitefoot\n", "source\n"]);
    assert_eq!(unclosed.len(), 1);
    assert!(!unclosed[0].closed);
}

#[test]
fn markdown_containers_preserve_program_and_output_contents() {
    for text in [
        "- ```whitefoot\n  source\n  ```\n",
        "1. ```whitefoot\n   source\n   ```\n",
        "- A list item.\n\n  ```whitefoot\n  source\n  ```\n",
        "- A list item.\n\n    ```whitefoot\n    source\n    ```\n",
        "> ```whitefoot\n> source\n> ```\n",
        "> - ```whitefoot\n>   source\n>   ```\n",
        "- > ```whitefoot\n  > source\n  > ```\n",
    ] {
        let lines = text.split_inclusive('\n').collect::<Vec<_>>();
        let blocks = fences(&lines);
        assert_eq!(blocks.len(), 1, "{text}");
        assert_eq!(blocks[0].info, "whitefoot", "{text}");
        assert_eq!(blocks[0].body, "source\n", "{text}");
        assert!(blocks[0].closed, "{text}");
        assert!(blocks[0].error.is_none(), "{text}");
    }
    let quoted = concat!(
        "> ```whitefoot reject OP-4\n> source\n> ```\n>\n",
        "> ```whitefoot-output\n> exact output\n> ```\n",
    );
    let lines = quoted.split_inclusive('\n').collect::<Vec<_>>();
    let blocks = fences(&lines);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[1].body, "exact output\n");
    assert!(
        lines[blocks[0].closing + 1..blocks[1].opening]
            .iter()
            .all(|line| markdown_blank(line))
    );
    assert!(!markdown_blank("> intervening prose\n"));
    assert!(!markdown_blank("- \n"));
    assert!(!markdown_blank("> - \n"));

    let indented = "    ```whitefoot\n    source\n    ```\n";
    assert!(fences(&indented.split_inclusive('\n').collect::<Vec<_>>()).is_empty());

    let failures = check_document(Path::new("fixture.md"), "- A list item.\n\t```whitefoot\n");
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("fixture.md:2"));
    assert!(failures[0].contains("cannot interpret this Markdown container indentation"));

    let failures = check_document(Path::new("fixture.md"), ">\t```whitefoot\n");
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("cannot interpret this Markdown container indentation"));
}

#[test]
fn only_whitefoot_opts_in_and_other_fences_cannot_carry_attributes() {
    let ignored = concat!(
        "```wf\nnot a program\n```\n",
        "```wf-output\nnot an expected rejection\n```\n",
        "```whitefootish\nnot a program\n```\n",
        "```text\nnot a program\n```\n",
        "```sh\nnot a program\n```\n",
        "```\nnot a program\n```\n",
    );
    assert!(check_document(Path::new("fixture.md"), ignored).is_empty());
    let malformed = concat!(
        "```text file=first.wf\nnot a program\n```\n",
        "```whitefoot-output file=first.wf\nnot output\n```\n",
        "```wf run exit=4\nnot a program\n```\n",
    );
    let failures = check_document(Path::new("fixture.md"), malformed);
    assert_eq!(failures.len(), 3);
    assert!(failures[0].contains("fixture.md:1"));
    assert!(failures[1].contains("fixture.md:4"));
    assert!(failures[2].contains("fixture.md:7"));
    assert!(failures.iter().all(|error| error.contains("no attributes")));
}

#[test]
fn bare_whitefoot_checks_a_library_without_building_an_executable() {
    let source = include_str!("../../../tests/conformance/cases/fn1-pos-library-without-entry.wf");
    let text = format!("```whitefoot\n{source}```\n");
    let failures = check_document(Path::new("fixture.md"), &text);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
