# Text literals

Character literals, the spellings inside a quote, and byte-string constants.

### decision character-literals-are-integers
A character literal `'C'_TYPE`, with suffix `u8` or `u32`, is an integer literal of that type holding C's scalar value, and a `u8` character holds at most 0x7F.
- rejected: an agent proposed forcing the character spelling on printable byte values, so that `97_u8` is illegal where `'a'_u8` can be written; the owner rejected it, because arithmetic on byte values would then be written as text, and numeric code would read oddly.
- rejected: an agent proposed standard-library constants for characters as the only relief; the owner rejected it, because such constants name characters but give no way to write a message.

### decision each-scalar-has-one-quoted-spelling
Each scalar value has one spelling inside a quote: the raw byte for printable ASCII, `\\`, the escaped quote, `\n`, `\t` and `\r`, and lowercase `\u{H}` without leading zeros for all others.
- rejected: an agent proposed a `\x` byte escape, or uppercase or zero-padded hexadecimal; the owner rejected it, because a second escape family would give one value two spellings.
- rejected: an agent proposed `\u{9}` and `\u{d}` as the only spellings of tab and carriage return; the owner rejected it, because tab and carriage return occur in ordinary text, such as tab-separated rows and CRLF line ends, so each takes a named escape.

### decision strings-are-byte-array-constants
A STRING is the value of an `Array<u8, N>` constant holding its UTF-8 encoding, with N required to equal the byte length, and it is never an expression.
- rejected: an agent proposed a string expression or a string type; the owner rejected it, because a message is static data, which the constant array already stores.
- rejected: an agent proposed inferring the array length from its STRING; the owner rejected it, because every type is written in full, and a declared length the checker checks states the storage while the repair supplies the count.

### decision text-values-are-judged-at-check-time
A text item's shape is decided when the token is classified, while whether it is its value's one spelling, a scalar value, and in range for `u8` is decided at check time, with a repair.
- rejected: an agent proposed rejecting every non-canonical item while forming tokens; the owner rejected it, because no repair is given there, while only the value judgments have a spelling to repair to, as with the float literal.
