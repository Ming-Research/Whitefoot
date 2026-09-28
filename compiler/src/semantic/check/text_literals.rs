//! Check-time judgments on text literals: a character literal's value and
//! type [FORM-5, FORM-7], the one spelling of every text item, including a
//! `doc` entry's, and a STRING constant's bytes [CONST-2].

use crate::syntax::NodeId;
use crate::syntax::terminal::TerminalPredicate;
use crate::syntax::text::{
    CHARACTER_QUOTE, STRING_QUOTE, TextItem, canonical_spelling, quoted_text,
};
use crate::{Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

use super::super::model::{CheckedType, CheckedValue, IntegerType};
use super::{CheckStop, DeclarationInventory};

/// The largest value a `u8` character literal holds, so that it is ASCII
/// and never mistaken for one byte of a UTF-8 encoding [FORM-7].
const ASCII_MAXIMUM: u32 = 0x7f;

impl DeclarationInventory<'_> {
    /// A character literal's value [FORM-5]: its one text item, which
    /// [FORM-7] requires in its value's one spelling, denoting a scalar
    /// value that a `u8` holds only up to 0x7F.
    pub(super) fn parse_character_literal(
        &self,
        node: NodeId,
        literal: usize,
    ) -> Result<CheckedValue, CheckStop> {
        let bytes = self.tree.token_bytes(literal)?;
        let text = quoted_text(bytes, CHARACTER_QUOTE)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let [item] = text.items.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let ty = match &bytes[text.suffix_start..] {
            b"_u8" => IntegerType::U8,
            b"_u32" => IntegerType::U32,
            _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        };
        let scalar = self.text_item_scalar(node, literal, *item, CHARACTER_QUOTE)?;
        let value = u32::from(scalar);
        if ty == IntegerType::U8 && value > ASCII_MAXIMUM {
            let character = format!("'{}'_u32", canonical_spelling(scalar, CHARACTER_QUOTE));
            let mechanical_fix = if value <= u32::from(u8::MAX) {
                format!(
                    "a `u8` character is ASCII, at most 0x7F: write `{character}` for the character, or `{value}_u8` for the byte"
                )
            } else {
                format!("a `u8` character is ASCII, at most 0x7F: write `{character}`")
            };
            return self.issue_node(
                SemanticRule::Form7,
                node,
                SemanticIssueKind::NonAsciiByteCharacter { mechanical_fix },
            );
        }
        Ok(CheckedValue::Integer {
            ty,
            bits: u64::from(value),
        })
    }

    /// The bytes a STRING `cvalue` defines for an `Array<u8, N>` whose
    /// declared type `expected` and length N the caller has read [CONST-2]:
    /// the UTF-8 encoding of its items' scalar values, each item in its one
    /// spelling [FORM-7], and exactly N of them.
    pub(super) fn parse_string_constant(
        &self,
        node: NodeId,
        literal: usize,
        expected: CheckedType,
        declared_length: u64,
    ) -> Result<CheckedValue, CheckStop> {
        let bytes = self.tree.token_bytes(literal)?;
        let text = quoted_text(bytes, STRING_QUOTE)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut encoded = String::new();
        for item in &text.items {
            encoded.push(self.text_item_scalar(node, literal, *item, STRING_QUOTE)?);
        }
        let byte_length =
            u64::try_from(encoded.len()).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
        if byte_length != declared_length {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::TextLengthMismatch {
                    declared_length,
                    byte_length,
                    mechanical_fix: format!(
                        "this text is {byte_length} bytes in UTF-8: write `Array<u8, {byte_length}>` where this array's type is declared, or change the text to {declared_length} bytes"
                    ),
                },
            );
        }
        Ok(CheckedValue::Array {
            ty: expected,
            elements: encoded
                .bytes()
                .map(|byte| CheckedValue::Integer {
                    ty: IntegerType::U8,
                    bits: u64::from(byte),
                })
                .collect(),
        })
    }

    /// Every `doc` entry's text items in their one spelling [FORM-7]: a
    /// `doc` STRING is not otherwise evaluated, and its items obey the same
    /// rule as a constant's.
    pub(super) fn check_documentation_text(&self) -> Result<(), CheckStop> {
        for doc in self
            .tree
            .descendants_with(self.tree.root(), Production::Doc)?
        {
            let literal = self
                .tree
                .direct_token_with(doc, TerminalPredicate::String)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let text = quoted_text(self.tree.token_bytes(literal)?, STRING_QUOTE)
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            for item in text.items {
                self.text_item_scalar(doc, literal, item, STRING_QUOTE)?;
            }
        }
        Ok(())
    }

    /// The scalar value of one text item written in that value's one
    /// spelling, or the [FORM-7] rejection at the item, with a repair
    /// giving the spelling where the item denotes a scalar value.
    fn text_item_scalar(
        &self,
        node: NodeId,
        literal: usize,
        item: TextItem,
        quote: u8,
    ) -> Result<char, CheckStop> {
        let bytes = self.tree.token_bytes(literal)?;
        if item.is_canonical(bytes, quote) {
            return item
                .scalar()
                .ok_or_else(|| SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let coordinate = self
            .tree
            .token_subcoordinate(literal, item.start, item.end)?;
        let kind = match item.scalar() {
            Some(scalar) => {
                let written = String::from_utf8_lossy(&bytes[item.start..item.end]);
                SemanticIssueKind::InvalidTextItem {
                    reason: "each character has exactly one spelling: the printable ASCII byte itself, `\\\\`, `\\n`, `\\t`, `\\r` or the escaped quote, and `\\u{H}` in lowercase hexadecimal without leading zeros for every other value",
                    mechanical_fix: Some(format!(
                        "write `{}` in place of `{written}`",
                        canonical_spelling(scalar, quote)
                    )),
                }
            }
            None => SemanticIssueKind::InvalidTextItem {
                reason: "`\\u{H}` must denote a Unicode scalar value, at most 0x10FFFF and outside the surrogates 0xD800..0xDFFF",
                mechanical_fix: None,
            },
        };
        self.issue_at(SemanticRule::Form7, node, coordinate, kind)
    }
}
