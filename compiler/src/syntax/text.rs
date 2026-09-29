//! Text items: the interior of a character literal and of a STRING
//! [FORM-5], with the one spelling [FORM-7] gives each value.
//!
//! Both quoted forms share one item grammar and differ only in which quote
//! delimits them, so terminal membership, the check-time canonical judgment
//! and the UTF-8 encoding of a STRING constant all read items through this
//! one decoder rather than each re-reading escapes.

/// The quote delimiting a character literal.
pub(crate) const CHARACTER_QUOTE: u8 = b'\'';
/// The quote delimiting a STRING.
pub(crate) const STRING_QUOTE: u8 = b'"';

/// The largest Unicode scalar value; a larger H is not read further.
const SCALAR_MAXIMUM: u32 = 0x10_ffff;

/// One text item of a quoted token, located by its byte offsets within the
/// token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TextItem {
    /// The item's first byte, counted from the token's opening quote.
    pub(crate) start: usize,
    /// One past the item's last byte.
    pub(crate) end: usize,
    /// The number the item denotes, or `None` for a `\u{H}` whose H exceeds
    /// every scalar value; a surrogate is kept here and refused by
    /// [`TextItem::scalar`].
    pub(crate) value: Option<u32>,
}

impl TextItem {
    /// The Unicode scalar value the item denotes, if it denotes one: at most
    /// 0x10FFFF and not a surrogate [FORM-7].
    pub(crate) fn scalar(self) -> Option<char> {
        self.value.and_then(char::from_u32)
    }

    /// Whether the item is written in its value's one spelling [FORM-7]:
    /// it denotes a scalar value and its bytes are that value's spelling
    /// inside the given quote.
    pub(crate) fn is_canonical(self, token: &[u8], quote: u8) -> bool {
        self.scalar().is_some_and(|scalar| {
            token.get(self.start..self.end) == Some(canonical_spelling(scalar, quote).as_bytes())
        })
    }
}

/// A quoted token read as its text items [FORM-5].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuotedText {
    /// The items between the quotes, in source order.
    pub(crate) items: Vec<TextItem>,
    /// The bytes after the closing quote: a character literal's `_TYPE`
    /// suffix, and nothing for a STRING.
    pub(crate) suffix_start: usize,
}

/// Reads a token that opens with `quote` as a sequence of text items up to
/// its closing quote, or `None` when the bytes are not that shape: a raw
/// byte outside U+0020..U+007E, the quote or `\` unescaped, an escape other
/// than `\\`, `\n`, `\t`, `\r`, the quote escape and `\u{H}` with H one or
/// more lowercase hexadecimal digits, or no closing quote.
///
/// This is the shape [FORM-5] decides at terminal membership; whether an
/// item is its value's one spelling is [FORM-7]'s check-time judgment,
/// [`TextItem::is_canonical`].
pub(crate) fn quoted_text(token: &[u8], quote: u8) -> Option<QuotedText> {
    if token.first() != Some(&quote) {
        return None;
    }
    let mut items = Vec::new();
    let mut cursor = 1;
    loop {
        let byte = *token.get(cursor)?;
        if byte == quote {
            return Some(QuotedText {
                items,
                suffix_start: cursor + 1,
            });
        }
        let start = cursor;
        let value = if byte == b'\\' {
            let follower = *token.get(cursor + 1)?;
            cursor += 2;
            match follower {
                b'\\' => Some(u32::from(b'\\')),
                b'n' => Some(u32::from(b'\n')),
                b't' => Some(u32::from(b'\t')),
                b'r' => Some(u32::from(b'\r')),
                b'u' => {
                    let (value, end) = hexadecimal_escape(token, cursor)?;
                    cursor = end;
                    value
                }
                _ if follower == quote => Some(u32::from(quote)),
                _ => return None,
            }
        } else if (0x20..=0x7e).contains(&byte) {
            cursor += 1;
            Some(u32::from(byte))
        } else {
            return None;
        };
        items.push(TextItem {
            start,
            end: cursor,
            value,
        });
    }
}

/// Reads the `{H}` completing a `\u` escape that starts at `cursor`: its
/// value, `None` above every scalar value, and the offset after `}`.
fn hexadecimal_escape(token: &[u8], cursor: usize) -> Option<(Option<u32>, usize)> {
    if token.get(cursor) != Some(&b'{') {
        return None;
    }
    let mut end = cursor + 1;
    let mut value = Some(0_u32);
    while let Some(digit) = token.get(end).and_then(|byte| lowercase_hex_digit(*byte)) {
        value = value
            .and_then(|value| value.checked_mul(16))
            .and_then(|value| value.checked_add(digit))
            .filter(|value| *value <= SCALAR_MAXIMUM);
        end += 1;
    }
    if end == cursor + 1 || token.get(end) != Some(&b'}') {
        return None;
    }
    Some((value, end + 1))
}

fn lowercase_hex_digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a') + 10),
        _ => None,
    }
}

/// The one spelling of a scalar value inside the given quote [FORM-7]: the
/// raw byte for printable ASCII other than `\` and the quote, `\\`, the quote
/// escape, `\n`, `\t` and `\r` for their five values, and `\u{H}` in lowercase
/// hexadecimal without leading zeros for every other value.
pub(crate) fn canonical_spelling(scalar: char, quote: u8) -> String {
    let value = u32::from(scalar);
    match value {
        0x5c => "\\\\".to_owned(),
        0x0a => "\\n".to_owned(),
        0x09 => "\\t".to_owned(),
        0x0d => "\\r".to_owned(),
        _ if value == u32::from(quote) => format!("\\{scalar}"),
        0x20..=0x7e => scalar.to_string(),
        _ => format!("\\u{{{value:x}}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{CHARACTER_QUOTE, STRING_QUOTE, TextItem, canonical_spelling, quoted_text};

    fn values(token: &[u8], quote: u8) -> Option<Vec<Option<u32>>> {
        quoted_text(token, quote).map(|text| text.items.iter().map(|item| item.value).collect())
    }

    #[test]
    fn items_decode_every_escape_form() {
        assert_eq!(
            values(br#""a\\\"\n\t\r\u{e9}\u{0}""#, STRING_QUOTE),
            Some(vec![
                Some(0x61),
                Some(0x5c),
                Some(0x22),
                Some(0x0a),
                Some(0x09),
                Some(0x0d),
                Some(0xe9),
                Some(0),
            ])
        );
        assert_eq!(values(br"'\''_u8", CHARACTER_QUOTE), Some(vec![Some(0x27)]));
        assert_eq!(values(br"'\t'_u8", CHARACTER_QUOTE), Some(vec![Some(0x09)]));
        assert_eq!(
            values(br"'\r'_u32", CHARACTER_QUOTE),
            Some(vec![Some(0x0d)])
        );
        assert_eq!(
            quoted_text(br"'a'_u32", CHARACTER_QUOTE).map(|text| text.suffix_start),
            Some(3)
        );
        assert_eq!(values(br#""""#, STRING_QUOTE), Some(vec![]));
    }

    #[test]
    fn a_value_above_every_scalar_decodes_to_none() {
        assert_eq!(values(br#""\u{110000}""#, STRING_QUOTE), Some(vec![None]));
        assert_eq!(
            values(br#""\u{fffffffffffffffff}""#, STRING_QUOTE),
            Some(vec![None])
        );
        let item = |value| TextItem {
            start: 0,
            end: 0,
            value,
        };
        assert_eq!(item(None).scalar(), None);
        assert_eq!(item(Some(0xd800)).scalar(), None);
        assert_eq!(item(Some(0x10_ffff)).scalar(), Some('\u{10ffff}'));
    }

    #[test]
    fn shape_refuses_what_form5_refuses() {
        for (token, quote) in [
            (br#""\u{}""#.as_slice(), STRING_QUOTE),
            (br#""\u{E9}""#, STRING_QUOTE),
            (br#""\u41""#, STRING_QUOTE),
            (br#""\'""#, STRING_QUOTE),
            (br#"'\"'_u8"#, CHARACTER_QUOTE),
            (br"'\x09'_u8", CHARACTER_QUOTE),
            (br#""\0""#, STRING_QUOTE),
            (b"'\x7f'_u8", CHARACTER_QUOTE),
            (br"'a", CHARACTER_QUOTE),
        ] {
            assert_eq!(quoted_text(token, quote), None, "{token:?}");
        }
    }

    #[test]
    fn each_value_has_one_spelling_per_quote() {
        assert_eq!(canonical_spelling('a', CHARACTER_QUOTE), "a");
        assert_eq!(canonical_spelling('\'', CHARACTER_QUOTE), "\\'");
        assert_eq!(canonical_spelling('\'', STRING_QUOTE), "'");
        assert_eq!(canonical_spelling('"', CHARACTER_QUOTE), "\"");
        assert_eq!(canonical_spelling('"', STRING_QUOTE), "\\\"");
        assert_eq!(canonical_spelling('\\', STRING_QUOTE), "\\\\");
        assert_eq!(canonical_spelling('\n', STRING_QUOTE), "\\n");
        assert_eq!(canonical_spelling('\t', STRING_QUOTE), "\\t");
        assert_eq!(canonical_spelling('\r', CHARACTER_QUOTE), "\\r");
        assert_eq!(canonical_spelling('\u{b}', STRING_QUOTE), "\\u{b}");
        assert_eq!(canonical_spelling('\0', STRING_QUOTE), "\\u{0}");
        assert_eq!(canonical_spelling('\u{7f}', STRING_QUOTE), "\\u{7f}");
        assert_eq!(canonical_spelling('é', STRING_QUOTE), "\\u{e9}");
    }

    #[test]
    fn canonical_items_are_exactly_the_one_spellings() {
        let canonical = |token: &[u8], quote| {
            quoted_text(token, quote)
                .expect("shape")
                .items
                .iter()
                .all(|item| item.is_canonical(token, quote))
        };
        assert!(canonical(br#""it's \"x\"\n\t\r\u{e9}\u{0}""#, STRING_QUOTE));
        assert!(canonical(br"'\''_u8", CHARACTER_QUOTE));
        assert!(canonical(br#"'"'_u8"#, CHARACTER_QUOTE));
        assert!(!canonical(br#""\u{41}""#, STRING_QUOTE));
        assert!(!canonical(br#""\u{0e9}""#, STRING_QUOTE));
        assert!(!canonical(br#""\u{00}""#, STRING_QUOTE));
        assert!(!canonical(br#""\u{a}""#, STRING_QUOTE));
        assert!(!canonical(br#""\u{9}""#, STRING_QUOTE));
        assert!(!canonical(br"'\u{d}'_u8", CHARACTER_QUOTE));
        assert!(!canonical(br#""\u{d800}""#, STRING_QUOTE));
        assert!(!canonical(br#""\u{110000}""#, STRING_QUOTE));
        assert!(!canonical(br"'\u{27}'_u8", CHARACTER_QUOTE));
    }
}
