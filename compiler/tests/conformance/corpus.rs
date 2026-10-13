//! The typed reading of `tests/conformance/manifest.jsonl`.
//!
//! The corpus owns its own schema; this module only restates it in Rust types
//! so the adapter can act on it. It deliberately re-derives nothing: an
//! unreadable line, an unknown expectation kind, or an unknown `arrange` key
//! is an error here rather than a skipped case, because a corpus entry the
//! adapter cannot act on must not silently disappear from the run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::json::{self, Value};

/// The verdict space the corpus fixes:
/// `accept | reject(rule) | run(exit) | unsupported`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// The unit reached complete target-independent source acceptance.
    Accept,
    /// The unit violates the named numbered rule, or an unnamed one.
    Reject(Vec<RejectedDiagnostic>),
    /// The program ran to completion with this command status.
    Run(i32),
    /// A compiler or backend capability limit, distinct from source rejection.
    Unsupported(String),
    /// Not one of the corpus's verdicts: the compiler stopped for a reason
    /// the corpus does not model, such as an internal invariant or a resource
    /// ceiling. Kept distinct so such a stop can never be reported as a
    /// language judgment.
    Stopped(String),
}

/// The expectation one manifest line declares.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expectation {
    /// `{"kind":"accept"}`
    Accept,
    /// `{"kind":"reject","rule":R}`
    Reject(String),
    /// `{"kind":"reject","diagnostics":[...]}` in source order.
    RejectDiagnostics(Vec<DiagnosticExpectation>),
    /// `{"kind":"run","exit":N}`
    Run(i32),
    /// `{"kind":"unsupported","why":W}`
    Unsupported,
}

impl Expectation {
    /// Whether satisfying this expectation requires running the program.
    pub const fn needs_execution(&self) -> bool {
        matches!(self, Self::Run(_))
    }

    /// The corpus's own match rule, restated exactly as `runner.py` states it:
    /// a rejection must cite the declared rule, a run must reach the declared
    /// status, and an `unsupported` expectation asserts the verdict kind only,
    /// never a diagnostic's wording.
    pub fn matched_by(&self, verdict: &Verdict) -> bool {
        match (self, verdict) {
            (Self::Accept, Verdict::Accept) => true,
            (Self::Reject(rule), Verdict::Reject(reached)) => {
                reached.len() == 1 && reached[0].rule.as_ref() == Some(rule)
            }
            (Self::RejectDiagnostics(expected), Verdict::Reject(reached)) => {
                expected.len() == reached.len()
                    && expected.iter().zip(reached).all(|(expected, reached)| {
                        reached.rule.as_ref() == Some(&expected.rule)
                            && reached.coordinate == Some(expected.coordinate)
                            && expected.expected.as_ref().is_none_or(|terminals| {
                                reached.expected.as_ref() == Some(terminals)
                            })
                    })
            }
            (Self::Run(exit), Verdict::Run(reached)) => exit == reached,
            (Self::Unsupported, Verdict::Unsupported(_)) => true,
            _ => false,
        }
    }
}

/// The exact DIAG-1 source coordinate, independent of a compiler's tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticCoordinate {
    /// Bundle-order source identity.
    pub source_ordinal: u64,
    /// Inclusive source-local byte offset.
    pub byte_start: u64,
    /// Exclusive source-local byte offset.
    pub byte_end: u64,
}

/// One reported source rejection, with no attribution invented by the adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedDiagnostic {
    /// The reported numbered rule, absent for an uncited stop.
    pub rule: Option<String>,
    /// The exact coordinate, absent for a bundle-root rejection.
    pub coordinate: Option<DiagnosticCoordinate>,
    /// The reported grammar predicates, when a grammar failure carries them.
    pub expected: Option<Vec<String>>,
}

/// One declared rejection; an absent expected set leaves its payload unchecked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticExpectation {
    /// The rule the specification selects.
    pub rule: String,
    /// The exact location the specification selects.
    pub coordinate: DiagnosticCoordinate,
    /// Optional exact expected-terminal evidence.
    pub expected: Option<Vec<String>>,
}

/// One fixture object placed under the invocation's initial directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fixture {
    /// The path's exact bytes, relative to the initial directory.
    pub path: Vec<u8>,
    /// A regular file's exact contents, or `None` for a directory.
    pub bytes: Option<Vec<u8>>,
}

/// The invocation one `run` case needs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Arrangement {
    /// The complete native argument vector, position 0 included.
    pub argv: Option<Vec<Vec<u8>>>,
    /// The standard input body; absent means an empty standard input.
    pub stdin: Option<Vec<u8>>,
    /// Fixtures created under the initial directory before the invocation.
    pub files: Vec<Fixture>,
    /// Opaque sink labels for the redirected streams.
    pub redirect: BTreeMap<String, String>,
}

/// One corpus case.
#[derive(Clone, Debug)]
pub struct Case {
    /// The case id, which also names its `.wf` source.
    pub id: String,
    /// The verdict the specification requires.
    pub expect: Expectation,
    /// The prose the manifest line carries, when it carries any. It is
    /// context for a report and never selects a path.
    pub reason: Option<String>,
    /// The invocation arrangement, present only on an executed case.
    pub arrange: Option<Arrangement>,
}

impl Case {
    /// Reads this case's canonical source bytes.
    pub fn source(&self) -> Vec<u8> {
        let path = cases_directory().join(format!("{}.wf", self.id));
        std::fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read case source {}: {error}", path.display()))
    }

    /// The logical path the compiler sees, which is the case's own file name.
    pub fn logical_path(&self) -> String {
        format!("{}.wf", self.id)
    }

    /// The package root of a module-form case: the case's own directory,
    /// which holds its `modules.wfg` and one directory per module [MOD-2].
    /// `None` for an ordinary one-record case.
    pub fn module_root(&self) -> Option<PathBuf> {
        let root = cases_directory().join(&self.id);
        root.join("modules.wfg").is_file().then_some(root)
    }
}

/// The conformance corpus directory, reached from the compiler package.
pub fn corpus_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package must live directly under the repository root")
        .join("tests")
        .join("conformance")
}

fn cases_directory() -> PathBuf {
    corpus_directory().join("cases")
}

/// Reads every case line of the manifest, ignoring coverage annotations.
///
/// Annotations carry `covered_by` instead of `id` and describe corpus
/// coverage rather than a program, so they have no verdict to check.
pub fn load() -> Vec<Case> {
    let path = corpus_directory().join("manifest.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut cases = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value = json::parse(line)
            .unwrap_or_else(|error| panic!("manifest line {}: {error}", number + 1));
        if value.get("id").is_none() {
            continue;
        }
        cases.push(
            case(&value).unwrap_or_else(|error| panic!("manifest line {}: {error}", number + 1)),
        );
    }
    cases
}

fn case(value: &Value) -> Result<Case, String> {
    let id = value
        .get("id")
        .and_then(Value::string)
        .ok_or("id must be a string")?
        .to_owned();
    let expect = expectation(value.get("expect").ok_or("missing expect")?)?;
    // `runnable` is the only status a manifest line may carry, and it is the
    // default when the line omits the key. The readiness axis is gone: a case
    // whose declared verdict the toolchain does not reach is a defect, not a
    // status. Any other value is a manifest error, exactly as
    // `tests/conformance/runner.py` reports it.
    match value.get("status").and_then(Value::string) {
        None | Some("runnable") => {}
        Some(other) => return Err(format!("{id}: invalid status {other:?}")),
    }
    let reason = value
        .get("reason")
        .and_then(Value::string)
        .map(ToOwned::to_owned);
    let arrange = value.get("arrange").map(arrangement).transpose()?;
    Ok(Case {
        id,
        expect,
        reason,
        arrange,
    })
}

fn expectation(value: &Value) -> Result<Expectation, String> {
    match value.get("kind").and_then(Value::string) {
        Some("accept") => Ok(Expectation::Accept),
        Some("reject") if value.get("diagnostics").is_some() => {
            let Value::Object(fields) = value else {
                return Err("reject expectation must be an object".to_owned());
            };
            if fields.len() != 2 {
                return Err("reject expectation needs exactly one of rule, diagnostics".to_owned());
            }
            let diagnostics = value
                .get("diagnostics")
                .and_then(Value::array)
                .filter(|diagnostics| !diagnostics.is_empty())
                .ok_or("reject diagnostics must be a nonempty array")?
                .iter()
                .map(diagnostic_expectation)
                .collect::<Result<Vec<_>, _>>()?;
            if diagnostics.windows(2).any(|pair| {
                (pair[0].coordinate.source_ordinal, pair[0].coordinate.byte_start)
                    > (pair[1].coordinate.source_ordinal, pair[1].coordinate.byte_start)
            }) {
                return Err("diagnostics must be in source ordinal and byte order".to_owned());
            }
            Ok(Expectation::RejectDiagnostics(diagnostics))
        }
        Some("reject") => value
            .get("rule")
            .and_then(Value::string)
            .map(|rule| Expectation::Reject(rule.to_owned()))
            .ok_or_else(|| "reject expectation needs a rule".to_owned()),
        Some("run") => value
            .get("exit")
            .and_then(Value::integer)
            .and_then(|exit| i32::try_from(exit).ok())
            .map(Expectation::Run)
            .ok_or_else(|| "run expectation needs an integer exit".to_owned()),
        Some("unsupported") => Ok(Expectation::Unsupported),
        other => Err(format!("invalid expectation kind {other:?}")),
    }
}

fn diagnostic_expectation(value: &Value) -> Result<DiagnosticExpectation, String> {
    let Value::Object(fields) = value else {
        return Err("diagnostic must be an object".to_owned());
    };
    if fields.keys().any(|field| {
        !matches!(field.as_str(), "rule" | "source_ordinal" | "byte_start" | "byte_end" | "expected")
    }) {
        return Err("diagnostic contains an unknown field".to_owned());
    }
    let integer = |field| {
        value
            .get(field)
            .and_then(Value::integer)
            .ok_or_else(|| format!("diagnostic {field} must be a nonnegative integer"))
    };
    let coordinate = DiagnosticCoordinate {
        source_ordinal: integer("source_ordinal")?,
        byte_start: integer("byte_start")?,
        byte_end: integer("byte_end")?,
    };
    if coordinate.byte_start > coordinate.byte_end {
        return Err("diagnostic byte_start must not exceed byte_end".to_owned());
    }
    let rule = value
        .get("rule")
        .and_then(Value::string)
        .ok_or("diagnostic needs a rule")?
        .to_owned();
    let expected = value
        .get("expected")
        .map(|terminals| {
            terminals
                .array()
                .filter(|terminals| !terminals.is_empty())
                .ok_or("diagnostic expected must be a nonempty array")?
                .iter()
                .map(|terminal| {
                    terminal
                        .string()
                        .map(ToOwned::to_owned)
                        .ok_or_else(|| "diagnostic expected terminal must be a string".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    if expected.as_ref().is_some_and(|terminals| {
        terminals.iter().enumerate().any(|(position, terminal)| {
            terminal.is_empty() || terminals[..position].contains(terminal)
        })
    }) {
        return Err("diagnostic expected must contain distinct nonempty terminal labels".to_owned());
    }
    Ok(DiagnosticExpectation {
        rule,
        coordinate,
        expected,
    })
}

#[test]
fn diagnostic_expectations_detect_omissions_reordering_and_changed_payloads() {
    let value = json::parse(
        r#"{"kind":"reject","diagnostics":[{"rule":"GRAM-3","source_ordinal":0,"byte_start":10,"byte_end":11,"expected":["pure","reads","writes"]},{"rule":"FORM-1","source_ordinal":0,"byte_start":30,"byte_end":31}]}"#,
    )
    .expect("diagnostic manifest row");
    let expected = expectation(&value).expect("valid diagnostic expectation");
    let Expectation::RejectDiagnostics(diagnostics) = &expected else {
        panic!("diagnostic-list expectation");
    };
    let reached = diagnostics
        .iter()
        .map(|diagnostic| RejectedDiagnostic {
            rule: Some(diagnostic.rule.clone()),
            coordinate: Some(diagnostic.coordinate),
            expected: diagnostic.expected.clone(),
        })
        .collect::<Vec<_>>();
    assert!(expected.matched_by(&Verdict::Reject(reached.clone())));
    assert!(!expected.matched_by(&Verdict::Reject(reached[..1].to_vec())));
    let mut extra = reached.clone();
    extra.push(reached[1].clone());
    assert!(!expected.matched_by(&Verdict::Reject(extra)));
    let mut reordered = reached.clone();
    reordered.reverse();
    assert!(!expected.matched_by(&Verdict::Reject(reordered)));
    let mut wrong_set = reached.clone();
    wrong_set[0]
        .expected
        .as_mut()
        .expect("expected set")
        .push(";".to_owned());
    assert!(!expected.matched_by(&Verdict::Reject(wrong_set)));
    let legacy = Expectation::Reject("GRAM-3".to_owned());
    assert!(legacy.matched_by(&Verdict::Reject(reached[..1].to_vec())));
    assert!(!legacy.matched_by(&Verdict::Reject(reached)));
}

#[test]
fn malformed_diagnostic_manifest_rows_are_rejected() {
    for text in [
        r#"{"kind":"reject","diagnostics":[]}"#,
        r#"{"kind":"reject","diagnostics":[{}]}"#,
        r#"{"kind":"reject","rule":"GRAM-3","diagnostics":[]}"#,
        r#"{"kind":"reject","diagnostics":[{"rule":"GRAM-3","source_ordinal":0,"byte_start":2,"byte_end":1}]}"#,
        r#"{"kind":"reject","diagnostics":[{"rule":"GRAM-3","source_ordinal":0,"byte_start":1,"byte_end":2,"expected":["pure","pure"]}]}"#,
        r#"{"kind":"reject","diagnostics":[{"rule":"GRAM-3","source_ordinal":0,"byte_start":1,"byte_end":2,"ignored":1}]}"#,
        r#"{"kind":"reject","diagnostics":[{"rule":"GRAM-3","source_ordinal":0,"byte_start":3,"byte_end":4},{"rule":"FORM-1","source_ordinal":0,"byte_start":1,"byte_end":2}]}"#,
    ] {
        let value = json::parse(text).expect("structurally valid JSON");
        assert!(expectation(&value).is_err(), "admitted malformed row {text}");
    }
}

fn arrangement(value: &Value) -> Result<Arrangement, String> {
    let argv = value
        .get("argv")
        .map(|argv| {
            argv.array()
                .ok_or_else(|| "argv must be an array".to_owned())?
                .iter()
                .map(hex_bytes)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let stdin = value.get("stdin").map(hex_bytes).transpose()?;
    let files = value
        .get("files")
        .map(|files| {
            files
                .array()
                .ok_or_else(|| "files must be an array".to_owned())?
                .iter()
                .map(fixture)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let mut redirect = BTreeMap::new();
    if let Some(Value::Object(streams)) = value.get("redirect") {
        for (stream, sink) in streams {
            let sink = sink
                .string()
                .ok_or_else(|| format!("redirect {stream} needs a sink name"))?;
            redirect.insert(stream.clone(), sink.to_owned());
        }
    }
    Ok(Arrangement {
        argv,
        stdin,
        files,
        redirect,
    })
}

fn fixture(value: &Value) -> Result<Fixture, String> {
    let path = hex_bytes(value.get("path").ok_or("fixture needs a path")?)?;
    let bytes = match (value.get("bytes"), value.get("directory")) {
        (Some(content), None) => Some(hex_bytes(content)?),
        (None, Some(Value::True)) => None,
        _ => return Err("fixture needs exactly one of bytes, directory".to_owned()),
    };
    Ok(Fixture { path, bytes })
}

/// Decodes one manifest byte string.
///
/// Every byte string in the corpus is lowercase hex so a non-UTF-8 argument
/// or path is stated exactly rather than through an encoding the reader has
/// to guess.
fn hex_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let text = value.string().ok_or("byte string must be a string")?;
    if text.len() % 2 != 0 {
        return Err(format!("byte string {text:?} has an odd length"));
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            if !digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!("byte string {text:?} is not lowercase hex"));
            }
            u8::from_str_radix(digits, 16).map_err(|error| error.to_string())
        })
        .collect()
}
