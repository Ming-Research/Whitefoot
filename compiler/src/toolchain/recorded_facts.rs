//! The clang answers a package records in its `toolchain-manifest.json`.
//!
//! A bundled release fixes the clang a package carries, so the answers the
//! capability probes would find on it are fixed too. The release recipe runs
//! the probes once, against that clang, and writes the answers under the key
//! `clang_facts`. A compiler running from the package takes them instead of
//! probing, when the manifest's `clang_version` is the first line of the
//! package clang's `--version`, and probes as before when anything differs.
//!
//! The manifest is read with a small JSON reader: the crate takes no JSON
//! dependency, and the reader only has to hold the manifest's shape (objects,
//! arrays, strings, numbers, literals) to find one object in it.

use super::ToolchainFacts;

/// One JSON value; numbers keep no value because no answer is a number.
#[derive(Debug, PartialEq)]
pub(super) enum Json {
    Null,
    Bool(bool),
    Number,
    Text(String),
    /// Elements are checked as they are read and not kept: no answer is one.
    Array,
    Object(Vec<(String, Json)>),
}

impl Json {
    fn member(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members
                .iter()
                .find_map(|(name, value)| (name == key).then_some(value)),
            _ => None,
        }
    }
}

/// How deeply values may nest; the manifest nests three levels.
const DEPTH_LIMIT: usize = 64;

struct Reader<'text> {
    bytes: &'text [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.at))
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            self.fail(&format!("expected '{}'", char::from(byte)))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            self.fail("unknown literal")
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > DEPTH_LIMIT {
            return self.fail("nesting too deep");
        }
        self.skip_space();
        match self.peek() {
            None => self.fail("unexpected end"),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Text),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.fail("unexpected character"),
        }
    }

    /// The digits at the cursor, and how many there were.
    fn digits(&mut self) -> usize {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - start
    }

    /// A number as RFC 8259 writes it: `-? (0 | [1-9][0-9]*) (. [0-9]+)?
    /// ([eE] [+-]? [0-9]+)?`. What follows it is the caller's to refuse, so
    /// `01`, `1-2` and `1+2` stop at the second part and fail there.
    fn number(&mut self) -> Result<Json, String> {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return self.fail("malformed number"),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if self.digits() == 0 {
                return self.fail("malformed number");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return self.fail("malformed number");
            }
        }
        Ok(Json::Number)
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0_u32;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|byte| char::from(byte).to_digit(16))
                .map_or_else(|| self.fail("malformed \\u escape"), Ok)?;
            code = code * 16 + digit;
            self.at += 1;
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut collected = Vec::new();
        loop {
            let Some(byte) = self.peek() else {
                return self.fail("unterminated string");
            };
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return self.fail("unterminated escape");
                    };
                    self.at += 1;
                    let ch = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                } else {
                                    return self.fail("unpaired surrogate escape");
                                }
                            }
                            char::from_u32(code).map_or_else(
                                || self.fail("unpaired surrogate escape"),
                                Ok,
                            )?
                        }
                        _ => return self.fail("unknown escape"),
                    };
                    let mut buffer = [0_u8; 4];
                    collected.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                }
                0x00..=0x1f => return self.fail("control character in string"),
                _ => collected.push(byte),
            }
        }
        String::from_utf8(collected).map_or_else(|_| self.fail("string is not UTF-8"), Ok)
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'[')?;
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array);
        }
        loop {
            self.value(depth + 1)?;
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array);
                }
                _ => return self.fail("expected ',' or ']'"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            // Readers disagree on which of two equal names wins (this one
            // would take the first, Python's `json` the last), so a manifest
            // that holds two is not read at all.
            if members.iter().any(|(name, _)| *name == key) {
                return self.fail(&format!("duplicate key \"{key}\""));
            }
            self.skip_space();
            self.expect(b':')?;
            members.push((key, self.value(depth + 1)?));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                _ => return self.fail("expected ',' or '}'"),
            }
        }
    }
}

/// The JSON value `text` holds, or why it holds none.
pub(super) fn parse(text: &str) -> Result<Json, String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at == reader.bytes.len() {
        Ok(value)
    } else {
        reader.fail("trailing text")
    }
}

/// Whether the package must record the Darwin stack-probe answer.
const RECORDS_DARWIN_ANSWER: bool = cfg!(target_os = "macos");

/// The facts `manifest` records for the clang whose first `--version` line is
/// `version_line`, or why they cannot be taken: the manifest is not JSON, has
/// no `clang_facts`, records another clang, or lacks or garbles an answer.
/// Taken all together or not at all, so one bad answer never mixes with
/// probed ones.
pub(super) fn recorded(manifest: &str, version_line: &str) -> Result<ToolchainFacts, String> {
    let document =
        parse(manifest).map_err(|error| format!("toolchain-manifest.json is not valid JSON: {error}"))?;
    let facts = document
        .member("clang_facts")
        .ok_or_else(|| "toolchain-manifest.json records no clang_facts".to_owned())?;
    let text = |key: &str| match facts.member(key) {
        Some(Json::Text(value)) => Ok(value.as_str()),
        Some(_) => Err(format!("clang_facts.{key} is not a string")),
        None => Err(format!("clang_facts.{key} is missing")),
    };
    let flag = |key: &str| match facts.member(key) {
        Some(Json::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("clang_facts.{key} is not true or false")),
        None => Err(format!("clang_facts.{key} is missing")),
    };
    let recorded_version = text("clang_version")?;
    if recorded_version != version_line {
        return Err(format!(
            "toolchain-manifest.json records clang \"{recorded_version}\", but the package's clang reports \"{version_line}\""
        ));
    }
    let no_capture_attribute = match text("no_capture_attribute")? {
        "captures(none)" => "captures(none)",
        "nocapture" => "nocapture",
        other => {
            return Err(format!(
                "clang_facts.no_capture_attribute is \"{other}\", not captures(none) or nocapture"
            ));
        }
    };
    let coro_end_result = match text("coro_end_result")? {
        "void" => "void",
        "i1" => "i1",
        other => {
            return Err(format!(
                "clang_facts.coro_end_result is \"{other}\", not void or i1"
            ));
        }
    };
    let preserve_none = flag("preserve_none")?;
    let darwin_stack_probe_helper = if RECORDS_DARWIN_ANSWER {
        flag("darwin_stack_probe_helper")?
    } else {
        false
    };
    Ok(ToolchainFacts {
        preserve_none,
        no_capture_attribute,
        coro_end_result,
        darwin_stack_probe_helper,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "clang version 22.1.8";

    fn manifest(version: &str, extra: &str) -> String {
        let darwin = if RECORDS_DARWIN_ANSWER {
            ",\"darwin_stack_probe_helper\":false"
        } else {
            ""
        };
        format!(
            "{{\"llvm_version\":\"22.1.8\",\"files\":[{{\"path\":\"bin/cl\\u0061ng\",\"size\":12,\"x\":[1,-2.5e3,null,true]}}],\
             \"clang_facts\":{{\"clang_version\":\"{version}\",\"preserve_none\":true,\
             \"no_capture_attribute\":\"captures(none)\",\"coro_end_result\":\"void\"{darwin}{extra}}}}}"
        )
    }

    #[test]
    fn a_manifest_shaped_document_is_read_through_its_nested_values() {
        let facts = recorded(&manifest(LINE, ""), LINE).expect("recorded answers");
        assert!(facts.preserve_none);
        assert_eq!(facts.no_capture_attribute, "captures(none)");
        assert_eq!(facts.coro_end_result, "void");
        assert!(!facts.darwin_stack_probe_helper);
    }

    #[test]
    fn escapes_and_surrogate_pairs_decode() {
        let parsed = parse("{\"a\":\"x\\n\\u00e9\\ud83d\\ude00\\\"\"}").expect("valid");
        assert_eq!(
            parsed.member("a"),
            Some(&Json::Text("x\n\u{e9}\u{1f600}\"".to_owned()))
        );
    }

    #[test]
    fn truncated_and_malformed_documents_are_refused() {
        let whole = manifest(LINE, "");
        for cut in [10, whole.len() / 2, whole.len() - 1] {
            assert!(parse(&whole[..cut]).is_err(), "cut at {cut}");
        }
        for bad in ["", "{", "{\"a\" 1}", "[1,]", "{\"a\":tru}", "{} {}", "\"\\x\"", "-"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(DEPTH_LIMIT + 8);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn another_clang_version_is_refused_naming_both() {
        let error = recorded(&manifest("clang version 21.1.0", ""), LINE).expect_err("mismatch");
        assert!(
            error.contains("clang version 21.1.0") && error.contains(LINE),
            "{error}"
        );
    }

    /// RFC 8259's number grammar: what is not a number makes the manifest not
    /// JSON, so no record in it is taken.
    #[test]
    fn a_number_outside_the_json_grammar_is_not_trusted() {
        for good in [
            "0", "-0", "7", "-12", "0.5", "10.25", "1e3", "1E+3", "-2.5e-3", "0e0",
        ] {
            let text = manifest(LINE, &format!(",\"n\":{good}"));
            if let Err(error) = recorded(&text, LINE) {
                panic!("{good} is a JSON number, but the record was refused: {error}");
            }
        }
        for bad in [
            "1-2", "01", "-01", "1.", "1e", "1e+", "1+2", "-", "--1", "+1", ".5", "1.e2",
            "0x10",
        ] {
            let text = manifest(LINE, &format!(",\"n\":{bad}"));
            let Err(error) = recorded(&text, LINE) else {
                panic!("{bad} is not a JSON number, but the record was taken");
            };
            assert!(error.contains("not valid JSON"), "{bad}: {error}");
        }
    }

    /// Two equal names would be read as the first by this reader and as the
    /// last by Python's, so the manifest is refused.
    #[test]
    fn a_duplicated_key_is_not_trusted() {
        let text = manifest(LINE, ",\"preserve_none\":false");
        let Err(error) = recorded(&text, LINE) else {
            panic!("a manifest with two preserve_none answers was taken");
        };
        assert!(
            error.contains("not valid JSON") && error.contains("duplicate key \"preserve_none\""),
            "{error}"
        );
    }

    #[test]
    fn a_missing_or_garbled_answer_names_its_key() {
        let without = manifest(LINE, "").replace("\"preserve_none\":true,", "");
        let error = recorded(&without, LINE).expect_err("missing");
        assert!(error.contains("preserve_none"), "{error}");
        let wrong_type = manifest(LINE, "").replace("\"preserve_none\":true", "\"preserve_none\":\"yes\"");
        let error = recorded(&wrong_type, LINE).expect_err("wrong type");
        assert!(error.contains("preserve_none"), "{error}");
        let wrong_word = manifest(LINE, "").replace("\"void\"", "\"maybe\"");
        let error = recorded(&wrong_word, LINE).expect_err("wrong word");
        assert!(error.contains("coro_end_result"), "{error}");
        let none = "{\"llvm_version\":\"22.1.8\"}";
        let error = recorded(none, LINE).expect_err("no clang_facts");
        assert!(error.contains("clang_facts"), "{error}");
    }
}
