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
/// `accept | reject(rule, optional position) | run(exit) | unsupported`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// The unit reached complete target-independent source acceptance.
    Accept,
    /// The unit violates the named numbered rule, with its source and span
    /// when the adapter has the complete diagnostic record.
    Reject(Option<String>, Option<(String, Span)>),
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

    /// The verdict-kind portion of the corpus's match rule: a rejection must
    /// cite the declared rule, a run must reach the declared
    /// status, and an `unsupported` expectation asserts the verdict kind only,
    /// never a diagnostic's wording. `Case::matches_span` adds an opted-in
    /// position assertion to this verdict check.
    pub fn matched_by(&self, verdict: &Verdict) -> bool {
        match (self, verdict) {
            (Self::Accept, Verdict::Accept) => true,
            (Self::Reject(rule), Verdict::Reject(Some(cited), _)) => rule == cited,
            (Self::Run(exit), Verdict::Run(reached)) => exit == reached,
            (Self::Unsupported, Verdict::Unsupported(_)) => true,
            _ => false,
        }
    }
}

/// A checked half-open byte interval in a case's single source record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    /// First byte of the interval.
    pub start: u64,
    /// First byte after the interval.
    pub end: u64,
}

impl Span {
    /// Read the same closed span object the Python manifest validator accepts.
    pub fn read(value: &Value) -> Result<Self, String> {
        let Value::Object(fields) = value else {
            return Err("span must be an object".to_owned());
        };
        if fields.len() != 2 || !fields.contains_key("start") || !fields.contains_key("end") {
            return Err("span fields must be exactly start and end".to_owned());
        }
        let start = value
            .get("start")
            .and_then(Value::integer)
            .ok_or("span start must be an integer")?;
        let end = value
            .get("end")
            .and_then(Value::integer)
            .ok_or("span end must be an integer")?;
        if start > end {
            return Err("span start must not exceed end".to_owned());
        }
        Ok(Self { start, end })
    }
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
    /// Optional position assertion, in this case's single `.wf` source.
    pub span: Option<Span>,
    /// The prose the manifest line carries, when it carries any. It is
    /// context for a report and never selects a path.
    pub reason: Option<String>,
    /// The invocation arrangement, present only on an executed case.
    pub arrange: Option<Arrangement>,
}

impl Case {
    /// A rule-only case leaves the position unconstrained. An opted-in case
    /// requires its own source and span from that same rejection.
    pub fn matches_span(&self, verdict: &Verdict) -> bool {
        self.span.is_none_or(|expected| {
            matches!(verdict, Verdict::Reject(_, Some((file, actual)))
                if *file == self.logical_path() && *actual == expected)
        })
    }

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

/// Selects manifest positions for this invocation, retaining their order.
/// An ordinary invocation without a case shard checks the complete list.
pub fn case_shard(cases: Vec<Case>) -> Vec<Case> {
    let value = std::env::var_os("WHITEFOOT_CASE_SHARD");
    let (index, count) =
        parse_case_shard(value.as_deref()).unwrap_or_else(|error| panic!("{error}"));
    let total = cases.len();
    let selected = select_case_shard(cases, (index, count));
    println!(
        "case shard {index}/{count}: {} of {total} cases",
        selected.len()
    );
    selected
}

fn parse_case_shard(value: Option<&std::ffi::OsStr>) -> Result<(usize, usize), String> {
    let Some(value) = value else {
        return Ok((1, 1));
    };
    let invalid = || format!("WHITEFOOT_CASE_SHARD must be i/N with 1 <= i <= N, got {value:?}");
    let value = value.to_str().ok_or_else(invalid)?;
    let (index, count) = value.split_once('/').ok_or_else(invalid)?;
    if [index, count].iter().any(|part| {
        part.is_empty() || part.starts_with('0') || !part.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return Err(invalid());
    }
    let index = index.parse::<usize>().map_err(|_| invalid())?;
    let count = count.parse::<usize>().map_err(|_| invalid())?;
    if index > count {
        return Err(invalid());
    }
    Ok((index, count))
}

fn select_case_shard<T>(cases: Vec<T>, (index, count): (usize, usize)) -> Vec<T> {
    cases
        .into_iter()
        .enumerate()
        .filter_map(|(position, case)| (position % count == index - 1).then_some(case))
        .collect()
}

#[test]
fn case_shards_cover_each_manifest_position_once_in_manifest_order() {
    use std::ffi::OsStr;

    let cases = vec!["z", "a", "t", "b", "r", "c", "p", "d", "n", "e", "l"];
    for count in 1..=5 {
        let mut union = Vec::new();
        for index in 1..=count {
            let value = format!("{index}/{count}");
            let shard = parse_case_shard(Some(OsStr::new(&value))).unwrap();
            assert_eq!(shard, (index, count));
            let selected = select_case_shard(cases.clone(), shard);
            let expected = cases
                .iter()
                .copied()
                .skip(index - 1)
                .step_by(count)
                .collect::<Vec<_>>();
            assert_eq!(selected, expected, "case shard {index}/{count}");
            union.extend(selected);
        }
        let mut expected = cases.clone();
        expected.sort_unstable();
        union.sort_unstable();
        assert_eq!(union, expected, "case shards 1..={count}");
    }
    assert_eq!(select_case_shard(cases.clone(), (1, 1)), cases);
}

#[test]
fn case_shards_default_to_the_whole_list_and_allow_empty_partitions() {
    use std::ffi::OsStr;

    assert_eq!(parse_case_shard(None), Ok((1, 1)));
    assert_eq!(parse_case_shard(Some(OsStr::new("1/1"))), Ok((1, 1)));
    assert_eq!(parse_case_shard(Some(OsStr::new("3/3"))), Ok((3, 3)));
    assert_eq!(select_case_shard(Vec::<usize>::new(), (1, 1)), vec![]);
    assert_eq!(select_case_shard(vec![0, 1], (3, 3)), vec![]);
    assert_eq!(select_case_shard(vec![0, 1, 2], (3, 3)), vec![2]);
    let maximum = format!("{0}/{0}", usize::MAX);
    assert_eq!(
        parse_case_shard(Some(OsStr::new(&maximum))),
        Ok((usize::MAX, usize::MAX))
    );
}

#[test]
fn invalid_case_shards_report_the_variable_and_its_required_range() {
    use std::ffi::OsStr;

    for value in [
        "",
        "0/1",
        "1/0",
        "2/1",
        "1",
        "1/2/3",
        "01/2",
        "1/02",
        "-1/2",
        "+1/2",
        "1/ 2",
        "1.0/2",
        "１/2",
        "1/9999999999999999999999999999999999999999",
    ] {
        let error = parse_case_shard(Some(OsStr::new(value))).unwrap_err();
        assert!(error.contains("WHITEFOOT_CASE_SHARD must be i/N with 1 <= i <= N"));
        assert!(error.contains(&format!("{value:?}")));
    }
}

#[cfg(unix)]
#[test]
fn nonunicode_case_shards_fail_instead_of_selecting_the_whole_list() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let error = parse_case_shard(Some(OsStr::from_bytes(&[0xff]))).unwrap_err();
    assert!(error.contains("WHITEFOOT_CASE_SHARD must be i/N with 1 <= i <= N"));
}

fn case(value: &Value) -> Result<Case, String> {
    let id = value
        .get("id")
        .and_then(Value::string)
        .ok_or("id must be a string")?
        .to_owned();
    let expect = expectation(value.get("expect").ok_or("missing expect")?)?;
    let span = value
        .get("expect")
        .and_then(|expect| expect.get("span"))
        .map(Span::read)
        .transpose()?;
    if let Some(span) = span {
        if !matches!(expect, Expectation::Reject(_)) {
            return Err("only a reject expectation may carry a span".to_owned());
        }
        let directory = cases_directory();
        if directory.join(&id).join("modules.wfg").is_file() {
            return Err("span requires a single .wf source".to_owned());
        }
        let source = std::fs::read(directory.join(format!("{id}.wf")))
            .map_err(|error| format!("cannot read span source: {error}"))?;
        if span.end > u64::try_from(source.len()).map_err(|error| error.to_string())? {
            return Err("span exceeds source byte length".to_owned());
        }
    }
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
        span,
        reason,
        arrange,
    })
}

fn expectation(value: &Value) -> Result<Expectation, String> {
    match value.get("kind").and_then(Value::string) {
        Some("accept") => Ok(Expectation::Accept),
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

#[test]
fn span_schema_requires_exact_ordered_integer_endpoints() {
    for text in [r#"{"start":0,"end":3}"#, r#"{"start":3,"end":3}"#] {
        assert!(Span::read(&json::parse(text).expect("JSON")).is_ok());
    }
    for text in [
        r#"{}"#,
        r#"{"start":0}"#,
        r#"{"start":2,"end":1}"#,
        r#"{"start":true,"end":1}"#,
        r#"{"start":0,"end":"1"}"#,
        r#"{"start":0,"end":1,"file":"sample.wf"}"#,
    ] {
        assert!(
            Span::read(&json::parse(text).expect("JSON")).is_err(),
            "{text}"
        );
    }
}

#[test]
fn span_assertions_require_the_same_rejections_rule_source_and_interval() {
    let span = Span { start: 12, end: 40 };
    let case = Case {
        id: "sample".to_owned(),
        expect: Expectation::Reject("PROV-6".to_owned()),
        span: Some(span),
        reason: None,
        arrange: None,
    };
    let verdict = |rule: &str, position| Verdict::Reject(Some(rule.to_owned()), position);
    let matches = |reached: &Verdict| case.expect.matched_by(reached) && case.matches_span(reached);
    assert!(matches(&verdict(
        "PROV-6",
        Some(("sample.wf".to_owned(), span))
    )));
    for reached in [
        verdict("PROV-6", None),
        verdict("PROV-6", Some(("another.wf".to_owned(), span))),
        verdict(
            "PROV-6",
            Some(("sample.wf".to_owned(), Span { start: 20, end: 26 })),
        ),
        verdict("TYPE-5", Some(("sample.wf".to_owned(), span))),
        Verdict::Accept,
    ] {
        assert!(!matches(&reached), "{reached:?}");
    }
    let mut unconstrained = case;
    unconstrained.span = None;
    assert!(unconstrained.matches_span(&verdict("PROV-6", None)));
}

#[test]
fn manifest_span_assertions_require_single_source_rejections_in_bounds() {
    for (text, expected) in [
        (
            r#"{"id":"sample","expect":{"kind":"accept","span":{"start":0,"end":1}}}"#,
            "only a reject",
        ),
        (
            r#"{"id":"fn9-pos-recursive-cross-module-written","expect":{"kind":"reject","rule":"FN-9","span":{"start":0,"end":1}}}"#,
            "single .wf source",
        ),
        (
            r#"{"id":"prov6-neg-copy-bound-call-span","expect":{"kind":"reject","rule":"PROV-6","span":{"start":0,"end":999999}}}"#,
            "exceeds source byte length",
        ),
    ] {
        let error = case(&json::parse(text).expect("JSON")).expect_err("invalid span assertion");
        assert!(error.contains(expected), "{error}");
    }
}
