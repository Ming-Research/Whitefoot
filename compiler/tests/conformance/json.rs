//! The smallest JSON reader that can read `tests/conformance/manifest.jsonl`.
//!
//! The corpus is deliberately toolchain-agnostic data, so the adapter reads
//! the same bytes `runner.py` reads rather than a Rust-side copy of the
//! corpus. This closed manifest format needs no additional parser dependency: it
//! reads exactly the closed value set one manifest line uses — object, array,
//! string, unsigned integer, and `true` — and refuses everything else instead
//! of guessing. A manifest line the reader cannot read is a corpus defect the
//! adapter must report, never silently skip.

use std::collections::BTreeMap;

/// One JSON value in the subset the manifest uses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// A member-keyed object.
    Object(BTreeMap<String, Value>),
    /// An ordered array.
    Array(Vec<Value>),
    /// A decoded string; the manifest's own byte strings are hex inside this.
    String(String),
    /// A nonnegative integer; the manifest writes no negative or fractional number.
    Integer(u64),
    /// The literal `true`; the manifest writes no other boolean.
    True,
}

impl Value {
    /// Returns the named member of an object value.
    pub fn get(&self, member: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members.get(member),
            _ => None,
        }
    }

    /// Returns the string contents of a string value.
    pub fn string(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// Returns the elements of an array value.
    pub fn array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(elements) => Some(elements),
            _ => None,
        }
    }

    /// Returns the value of an integer value.
    pub const fn integer(&self) -> Option<u64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }
}

/// Reads one complete JSON value, rejecting trailing bytes.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        cursor: 0,
    };
    let value = reader.value()?;
    reader.skip_space();
    if reader.cursor != reader.bytes.len() {
        return Err(format!("trailing bytes at offset {}", reader.cursor));
    }
    Ok(value)
}

struct Reader<'text> {
    bytes: &'text [u8],
    cursor: usize,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while matches!(
            self.bytes.get(self.cursor),
            Some(b' ' | b'\t' | b'\n' | b'\r')
        ) {
            self.cursor += 1;
        }
    }

    fn peek(&mut self) -> Result<u8, String> {
        self.skip_space();
        self.bytes
            .get(self.cursor)
            .copied()
            .ok_or_else(|| "unexpected end of value".to_owned())
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek()? != byte {
            return Err(format!(
                "expected {:?} at offset {}",
                char::from(byte),
                self.cursor
            ));
        }
        self.cursor += 1;
        Ok(())
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Value::String),
            b't' => {
                if self.bytes[self.cursor..].starts_with(b"true") {
                    self.cursor += 4;
                    Ok(Value::True)
                } else {
                    Err(format!("unknown literal at offset {}", self.cursor))
                }
            }
            byte if byte.is_ascii_digit() => self.integer(),
            byte => Err(format!(
                "unsupported value {:?} at offset {}",
                char::from(byte),
                self.cursor
            )),
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut members = BTreeMap::new();
        if self.peek()? == b'}' {
            self.cursor += 1;
            return Ok(Value::Object(members));
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            let value = self.value()?;
            if members.insert(key.clone(), value).is_some() {
                return Err(format!("repeated member {key:?}"));
            }
            match self.peek()? {
                b',' => self.cursor += 1,
                b'}' => {
                    self.cursor += 1;
                    return Ok(Value::Object(members));
                }
                byte => {
                    return Err(format!(
                        "expected ',' or '}}' at offset {}, found {:?}",
                        self.cursor,
                        char::from(byte)
                    ));
                }
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut elements = Vec::new();
        if self.peek()? == b']' {
            self.cursor += 1;
            return Ok(Value::Array(elements));
        }
        loop {
            elements.push(self.value()?);
            match self.peek()? {
                b',' => self.cursor += 1,
                b']' => {
                    self.cursor += 1;
                    return Ok(Value::Array(elements));
                }
                byte => {
                    return Err(format!(
                        "expected ',' or ']' at offset {}, found {:?}",
                        self.cursor,
                        char::from(byte)
                    ));
                }
            }
        }
    }

    fn integer(&mut self) -> Result<Value, String> {
        let start = self.cursor;
        while matches!(self.bytes.get(self.cursor), Some(byte) if byte.is_ascii_digit()) {
            self.cursor += 1;
        }
        let digits = std::str::from_utf8(&self.bytes[start..self.cursor])
            .map_err(|error| error.to_string())?;
        if matches!(self.bytes.get(self.cursor), Some(b'.' | b'e' | b'E')) {
            return Err(format!("unsupported non-integer number at offset {start}"));
        }
        digits
            .parse()
            .map(Value::Integer)
            .map_err(|error| format!("integer at offset {start}: {error}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut text = String::new();
        loop {
            let byte = *self
                .bytes
                .get(self.cursor)
                .ok_or_else(|| "unterminated string".to_owned())?;
            self.cursor += 1;
            match byte {
                b'"' => return Ok(text),
                b'\\' => {
                    let escape = *self
                        .bytes
                        .get(self.cursor)
                        .ok_or_else(|| "unterminated escape".to_owned())?;
                    self.cursor += 1;
                    match escape {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'/' => text.push('/'),
                        b'b' => text.push('\u{8}'),
                        b'f' => text.push('\u{c}'),
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        b'u' => text.push(self.escaped_scalar()?),
                        other => {
                            return Err(format!("unknown escape {:?}", char::from(other)));
                        }
                    }
                }
                _ => {
                    // The manifest is UTF-8, so an unescaped byte is copied
                    // through by finding the complete character it starts.
                    let start = self.cursor - 1;
                    let rest = std::str::from_utf8(&self.bytes[start..])
                        .map_err(|error| format!("manifest is not UTF-8: {error}"))?;
                    let character = rest
                        .chars()
                        .next()
                        .expect("a nonempty rest has a character");
                    self.cursor = start + character.len_utf8();
                    text.push(character);
                }
            }
        }
    }

    /// Decodes one `\uXXXX` escape, joining a surrogate pair when present.
    fn escaped_scalar(&mut self) -> Result<char, String> {
        let first = self.hex_quad()?;
        let scalar = if (0xd800..0xdc00).contains(&first) {
            if !self.bytes[self.cursor..].starts_with(b"\\u") {
                return Err("high surrogate without a low surrogate".to_owned());
            }
            self.cursor += 2;
            let second = self.hex_quad()?;
            if !(0xdc00..0xe000).contains(&second) {
                return Err("high surrogate followed by a non-low surrogate".to_owned());
            }
            0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00)
        } else {
            first
        };
        char::from_u32(scalar).ok_or_else(|| format!("escape {scalar:#x} is not a scalar value"))
    }

    fn hex_quad(&mut self) -> Result<u32, String> {
        let end = self.cursor + 4;
        let digits = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| "truncated \\u escape".to_owned())?;
        let text = std::str::from_utf8(digits).map_err(|error| error.to_string())?;
        let value =
            u32::from_str_radix(text, 16).map_err(|error| format!("\\u escape: {error}"))?;
        self.cursor = end;
        Ok(value)
    }
}
