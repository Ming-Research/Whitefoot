# Text literals

## Question

Whitefoot had no way to write text. STRING existed only in `doc` entries, so
every message was an `Array<u8, N>` of decimal bytes and every character
comparison a bare number. Should the language admit text literals, and in
which form, given the surface-form principle that each semantic construct
has exactly one spelling (`design/language/surface-form.md`, first decision)?

## Evidence

Snowghost, a web renderer written in Whitefoot, is text-heavy: CSS, HTML and a
JavaScript interpreter. Its writers reported a 47-array oracle driver whose
every message was a list of decimal bytes, character tests written as bare
numbers (`47_u8` for `/`), and a hand-counted message length that was wrong.
The maintained corpus shows the same shape: the eight messages of
`tests/programs/raw_deflate_boundary.wf` were 33 to 64 decimal entries each.
No measurement is needed to select the form; the question is one of spelling.

## Owner rulings

The owner approved adding text literals (decision card #18) and ruled (card
#21) that the one-spelling principle is read per construct: a text literal is
its own construct, used to denote text, and a number keeps its numeric
literal. `'a'_u8` and `97_u8` are both legal, each the one canonical spelling
of its own construct, and each literal still has exactly one canonical
spelling of its interior.

On reviewing the decision cards the owner approved the design and added the
escapes `\t` (U+0009) and `\r` (U+000D) to the escape set before merge. Tab
and carriage return occur in ordinary text such as tab-separated rows and
CRLF line ends, and each value keeps one spelling: `\u{9}` and `\u{d}` become
noncanonical, as `\u{a}` is beside `\n`.

## Design

- **Character literal** `'C'_TYPE`, TYPE `u8` or `u32`, suffix mandatory as on
  every numeric literal. C is one text item: a raw printable ASCII byte other
  than `'` and `\`, one of `\\`, `\'`, `\n`, `\t`, `\r`, or `\u{H}` with H
  lowercase hexadecimal. It is an integer literal of TYPE, so it is legal
  wherever one is, including a `cvalue`, a contract clause and an affine
  factor.
- **One spelling per value.** The raw byte for printable ASCII other than `\`
  and the delimiting quote; `\\`, the quote escape, `\n`, `\t` and `\r` for
  their five values; `\u{H}` without leading zeros for every other value. A
  `u8` character is at most 0x7F, so it is always ASCII and never a UTF-8
  byte; `u32` admits every Unicode scalar value.
- **Byte-string constant.** STRING gains `\u{H}` under the same rule (with
  `"` as its quote) and becomes a `cvalue` of `Array<u8, N>`, denoting the
  UTF-8 encoding of its scalar values; N must equal the byte length. It is
  not a member of `literal`, so it never appears in an expression. Source
  stays ASCII.

## Rejected alternatives

- **Force the character spelling for printable ASCII values**, making `97_u8`
  illegal (owner, card #21): rejected because arithmetic on byte values would
  have to be written as text, `'a'_u8` where a number is meant, which makes
  numeric code odd and reads one construct, the integer, as two.
- **Standard-library named constants only** (owner, card #21): rejected
  because it names characters but gives no way to write a message.
- **Uppercase or leading-zero hexadecimal, or decimal escapes such as
  `\x2f`**: each would give one value two spellings.

## Interaction with canonical form

Each value has exactly one spelling inside each quote, so FORM-1's one
spelling holds per construct. The shape of an item (which escapes exist,
lowercase hexadecimal) is decided at terminal membership [FORM-5], as the
float grammar decides `1.0E2_f64`; which spelling a value takes, whether it is
a scalar value, and whether a `u8` holds it are value judgments at check time
[FORM-7], as the shortest float spelling is. A `doc` STRING is checked by the
same rule although its value is never used.

## Lexing

Before v0.78 a `'` began no token and was a raw FORM-1 defect, the REGIONID
form having retired at v0.60; no token, label or operator uses it, and inside
a STRING it stays an ordinary raw byte. A character form ends at the first
unescaped `'` and then takes the maximal `[A-Za-z0-9_]*` suffix, as a numeric
form takes its suffix; each quoted form escapes only its own quote, so `'"'`
and `"'"` need no escape and `\"` in a character form, or `\'` in a STRING, is
a raw lexical defect. The `{H}` of `\u{H}` is ordinary interior bytes at raw
formation, so the raw scanner decodes nothing.

## Diagnostics

A noncanonical item cites FORM-7 at the item and repairs to the value's one
spelling, the raw `A` for `\u{41}`; an escape denoting no scalar
value has no spelling to repair to. A `u8` character above 0x7F repairs to the
`u32` character or, up to 0xFF, the decimal byte. A STRING whose byte length
differs from N cites CONST-2 and states the byte length. A wrong suffix, an
empty or two-item character form, and uppercase hexadecimal fail terminal
membership [FORM-5], which carries no repair in this compiler, as for every
other token that fails membership. A STRING in expression position is a
grammar rejection at the position that expected an expression.

Conformance cases `form5-*character*`, `form5-*string*`, `form7-*character*`,
`form7-*string*`, `form7-*doc*` and `const2-*string*` record each verdict.
