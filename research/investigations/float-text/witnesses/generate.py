"""Generator and independent oracle for the float-text witness programs.

Experiment E2 of ../DESIGN.md compiles four small Whitefoot programs (W1 to
W4, here split into eight files so that one refusal does not hide the next,
plus w2d, a restated form of w2c, the minimal witnesses w1c, w2e and w2f of
the gaps E2 found, and w2g and w2h, which vary w2f one change at a time)
to learn whether the checker accepts the arithmetic that exact decimal
conversion needs. Every expected value in a witness is computed here with
Python's exact integer arithmetic, from the definition of the operation and
never from the output of any witness.

    python3 -I generate.py            # write the witness files beside this script
    python3 -I generate.py --check    # fail unless the files equal the generator output
    python3 -I generate.py --expect   # print "name<TAB>accept|reject|open" per witness

Standard library only.
"""
import os
import struct
import sys
from fractions import Fraction

HERE = os.path.dirname(os.path.abspath(__file__))
MASK64 = (1 << 64) - 1

HEADER = (
    "alias ExitStatus = std::process::ExitStatus;\n"
    "alias exit_status = std::process::exit_status;\n"
)


def u64(value):
    return f"{value}_u64"


def u8(value):
    return f"{value}_u8"


def fnv(data):
    state = 14695981039346656037
    for byte in data:
        state = ((state ^ byte) * 1099511628211) & MASK64
    return state


def check_block(actual, wanted, code):
    return (
        f"  if {actual} == {wanted} {{\n"
        f"  }} else {{\n"
        f"    return exit_status(code: {u8(code)});\n"
        f"  }}\n"
    )


def bound_block(actual, limit, code):
    return (
        f"  if {actual} <= {limit} {{\n"
        f"  }} else {{\n"
        f"    return exit_status(code: {u8(code)});\n"
        f"  }}\n"
    )


def finish():
    return "  return exit_status(code: 0_u8);\n}\n"


# ---------------------------------------------------------------------------
# W1: the 128-bit product and the exponent-indexed table
# ---------------------------------------------------------------------------

TABLE_LENGTH = 617
TABLE_OFFSET = 324
LOG_SCALE = 1262611
LOG_SHIFT = 22


def power_entry(index):
    """The 64-bit normalized significand of 10^(index - 324), truncated."""
    k = index - TABLE_OFFSET
    if k >= 0:
        x = 10 ** k
        width = x.bit_length()
        value = x >> (width - 64) if width > 64 else x << (64 - width)
    else:
        d = 10 ** (-k)
        value = (1 << (d.bit_length() + 63)) // d
    assert 1 << 63 <= value < 1 << 64
    return value


POWER = [power_entry(i) for i in range(TABLE_LENGTH)]

W1_BITS = [
    0x3FF0000000000000,
    0x3FB999999999999A,
    0x0000000000000001,
    0x7FEFFFFFFFFFFFFF,
    0x0010000000000000,
    0x7FF0000000000000,
    0x44B52D02C7E14AF6,
    struct.unpack("<Q", struct.pack("<d", 123456.789))[0],
]


def table_index(bits):
    """floor(e * log10(2)) + 324 from the biased exponent field, by definition."""
    biased = (bits >> 52) & 2047
    e = biased - 1075
    exact = Fraction(e) * Fraction(30102999566398119521, 10 ** 20)
    index = (exact.numerator // exact.denominator) + TABLE_OFFSET
    assert 0 <= index < TABLE_LENGTH
    assert index == ((e * LOG_SCALE) >> LOG_SHIFT) + TABLE_OFFSET
    return index


def scaled_value(bits):
    mantissa = (bits & ((1 << 52) - 1)) + (1 << 52)
    entry = POWER[table_index(bits)]
    wide = mantissa * entry
    return (wide >> 64) + ((wide >> 63) & 1)


def table_text():
    body = ", ".join(u64(v) for v in POWER)
    return f"const power_table: Array<u64, {TABLE_LENGTH}> =[{body}];\n"


TABLE_ENTRY = f"""fn table_entry(bits: u64) -> result: u64 pure {{
  doc "The table row for a binary64 bit pattern: the biased exponent field gives e, and e times 1262611 shifted right by 22 is floor(e * log10(2)) for the exponents that occur, so adding 324 indexes the table.";
  let shifted = ishr(bits, 52_u32);
  let biased = iand(shifted, 2047_u64);
  let widened = cvt::<u64, i64>(biased);
  let exponent = widened - 1075_i64;
  let scaled = exponent * {LOG_SCALE}_i64;
  let floor_log = ishr(scaled, {LOG_SHIFT}_u32);
  let offset = floor_log + {TABLE_OFFSET}_i64;
  let index = cvt::<i64, u64>(offset);
  let row = power_table[index];
  return row;
}}
"""


def w1a():
    text = HEADER + "\n" + table_text() + "\n" + TABLE_ENTRY + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Looks the table up for eight bit patterns and compares each row with the one computed outside the compiler.";\n'
    for number, bits in enumerate(W1_BITS, start=1):
        text += f"  let entry_{number} = table_entry(bits: {u64(bits)});\n"
        text += check_block(f"entry_{number}", u64(POWER[table_index(bits)]), number)
    return text + finish()


PRODUCT = """fn product(left: u64, right: u64) -> (high: u64, low: u64) pure contract {
  ensures high <= left;
  ensures high <= right;
} {
  doc "The 128-bit product of two words as its high and low words.";
  let low_word = left *wrap right;
  let high_word = imulhi(left, right);
  return high_word, low_word;
}
"""

SCALED = """fn scaled(bits: u64) -> result: u64 pure {
  doc "The 53-bit significand times the table row, rounded to the high word: the high word of a product with a 53-bit factor is at most that factor, so adding the rounding bit cannot overflow.";
  let fraction = iand(bits, 4503599627370495_u64);
  let mantissa = fraction + 4503599627370496_u64;
  let row = table_entry(bits: bits);
  let (high, low) = product(left: mantissa, right: row);
  let half = ishr(low, 63_u32);
  let rounded = high + half;
  return rounded;
}
"""


def w1b():
    text = HEADER + "\n" + table_text() + "\n" + TABLE_ENTRY + "\n" + PRODUCT + "\n" + SCALED + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Scales eight bit patterns by their table rows and compares each result with the exact integer product computed outside the compiler.";\n'
    for number, bits in enumerate(W1_BITS, start=1):
        text += f"  let result_{number} = scaled(bits: {u64(bits)});\n"
        text += check_block(f"result_{number}", u64(scaled_value(bits)), number)
    return text + finish()


# W1c: the w1b gap apart from the table. The high word of a 64-by-64-bit
# product is at most either factor.

HIGH_WORD = """fn high_word(left: u64, right: u64) -> result: u64 pure contract {
  ensures result <= left;
} {
  doc "The high word of the 128-bit product of two words, which is at most either word.";
  let high = imulhi(left, right);
  return high;
}
"""

W1C_CASES = [
    (MASK64, MASK64),
    (1 << 53, MASK64),
    (3, 5),
    (1 << 63, 4),
]


def w1c():
    text = HEADER + "\n" + HIGH_WORD + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Takes four high words and compares each with the one computed outside the compiler.";\n'
    for number, (left, right) in enumerate(W1C_CASES, start=1):
        text += f"  let high_{number} = high_word(left: {u64(left)}, right: {u64(right)});\n"
        text += check_block(f"high_{number}", u64((left * right) >> 64), number)
    return text + finish()


# ---------------------------------------------------------------------------
# W2: the decimal digit buffer
# ---------------------------------------------------------------------------

DECIMAL = """struct Decimal {
  doc "A nonnegative integer as decimal digits, most significant first: count digits of an array of 800, each at most nine. Digits past the 800 held are dropped and noted in truncated.";
  digits: Array<u8, 800>;
  count: u64;
  truncated: Bool;
  invariant count_ceiling(decimal): decimal.count <= 800_u64;
  invariant(decimal): forall below_ten(k in 0_u64..decimal^.digits.len): decimal^.digits[k] <= 9_u8;
}

fn new_decimal() -> result: Decimal pure {
  let digits = array_filled::<u8, 800>(value: 0_u8);
  let clean = False();
  return Decimal(digits: digits, count: 0_u64, truncated: clean);
}

fn append_digit(decimal: &Decimal, digit: u8) -> result: unit writes(decimal) contract {
  requires digit <= 9_u8;
} {
  doc "Appends a digit, or notes a nonzero one past the 800 held as truncated.";
  let count = decimal^.count;
  if count < 800_u64 {
    set decimal^.digits[count] = digit;
    set decimal^.count = count + 1_u64;
  } else if digit != 0_u8 {
    set decimal^.truncated = True();
  }
  return unit;
}

fn digest(decimal: &Decimal) -> result: u64 reads(decimal) {
  doc "FNV-1a over the held digits.";
  let count = decimal^.count;
  let hash = 14695981039346656037_u64;
  for (index in 0_u64..count) {
    let digit = decimal^.digits[index];
    let wide = cvt::<u8, u64>(digit);
    let mixed = ixor(hash, wide);
    set hash = mixed *wrap 1099511628211_u64;
  }
  return hash;
}
"""

FORALL_SMALL = "invariant forall small(k in 0_u64..decimal^.digits.len): decimal^.digits[k] <= 9_u8"

MULTIPLY = f"""fn multiply(decimal: &Decimal, shift: u32) -> result: u64 writes(decimal) contract {{
  requires shift <= 60_u32;
}} {{
  doc "Multiplies the held integer by two to the shift, in place and from the least significant digit; the carry out of the most significant digit is returned. Each step is at most nine times 2^60 plus a carry of at most 2^60.";
  let multiplier = ishl(1_u64, shift);
  let count = decimal^.count;
  let remaining = count;
  let carry = 0_u64;
  loop @digits (
    invariant within: remaining <= count,
    invariant carry_bound: carry <= 1152921504606846976_u64,
    {FORALL_SMALL}
  ) {{
    if 0_u64 < remaining {{
    }} else {{
      break @digits;
    }}
    let index = remaining - 1_u64;
    let digit = decimal^.digits[index];
    invariant digit_small: digit <= 9_u8 {{
      use small(index);
    }}
    let wide = cvt::<u8, u64>(digit);
    let scaled = wide * multiplier;
    let total = scaled + carry;
    let low = total % 10_u64;
    let high = total / 10_u64;
    let narrow = cvt::<u64, u8>(low);
    set decimal^.digits[index] = narrow;
    set carry = high;
    set remaining = index;
  }}
  return carry;
}}
"""

SHIFT_RIGHT = f"""fn shift_right(decimal: &Decimal, shift: u32) -> result: u64 writes(decimal) contract {{
  requires shift <= 60_u32;
}} {{
  doc "Divides the held integer by two to the shift in place, schoolbook from the most significant digit: the running remainder times ten plus the next digit is shifted right to give the next quotient digit, and its low bits are the new remainder. The remainder at the end is returned.";
  let multiplier = ishl(1_u64, shift);
  let mask = multiplier - 1_u64;
  let count = decimal^.count;
  let remainder = 0_u64;
  for (
    index in 0_u64..count,
    invariant remainder_bound: remainder <= 1152921504606846975_u64,
    {FORALL_SMALL}
  ) {{
    let digit = decimal^.digits[index];
    invariant digit_small: digit <= 9_u8 {{
      use small(index);
    }}
    let wide = cvt::<u8, u64>(digit);
    let scaled = remainder * 10_u64;
    let total = scaled + wide;
    let quotient = ishr(total, shift);
    let kept = iand(total, mask);
    let narrow = cvt::<u64, u8>(quotient);
    set decimal^.digits[index] = narrow;
    set remainder = kept;
  }}
  return remainder;
}}
"""

SHIFT_FRONT = f"""fn shift_front(decimal: &Decimal, delta: u64) -> result: unit writes(decimal) contract {{
  requires decimal^.count + delta <= 800_u64;
}} {{
  doc "Moves the held digits delta places toward the end, from the last digit backward, and fills the vacated front with zeros.";
  let count = decimal^.count;
  let total = count + delta;
  let remaining = count;
  loop @copy (
    invariant within: remaining <= count,
    {FORALL_SMALL}
  ) {{
    if 0_u64 < remaining {{
    }} else {{
      break @copy;
    }}
    let index = remaining - 1_u64;
    let digit = decimal^.digits[index];
    let target = index + delta;
    set decimal^.digits[target] = digit;
    set remaining = index;
  }}
  for (
    position in 0_u64..delta,
    {FORALL_SMALL}
  ) {{
    set decimal^.digits[position] = 0_u8;
  }}
  set decimal^.count = total;
  return unit;
}}
"""

# W2c's function with the bound its requirement implies restated as an
# invariant, which the stored field needs (DESIGN.md, E2 results).
SHIFT_FRONT_RESTATED = SHIFT_FRONT.replace(
    "  let total = count + delta;\n",
    "  let total = count + delta;\n  invariant fits: total <= 800_u64;\n",
)
assert SHIFT_FRONT_RESTATED != SHIFT_FRONT

LONG_DIGITS = "3141592653589793238462643383279502884197"

W2_MULTIPLY_CASES = [
    ("1", 1),
    ("99999", 5),
    ("18446744073709551615", 60),
    (LONG_DIGITS, 37),
    ("5", 0),
]

W2_DIVIDE_CASES = [
    ("1", 1),
    ("99999", 5),
    ("18446744073709551615", 60),
    (LONG_DIGITS, 37),
    ("5", 0),
]

W2_FRONT_CASES = [
    ("12345", 3),
    ("1", 0),
    (LONG_DIGITS, 7),
    ("90", 798),
]


def decimal_digest(digits):
    return fnv([int(c) for c in digits])


def load_digits(name, digits):
    text = f"  let {name} = new_decimal();\n"
    for char in digits:
        text += f"  append_digit(decimal: &{name}, digit: {u8(int(char))});\n"
    return text


def w2_main(doc, cases, call, outcome):
    """Main for the in-place operations; outcome(digits, arg) -> (result, digits)."""
    text = "fn main() -> status: ExitStatus pure {\n"
    text += f'  doc "{doc}";\n'
    for number, (digits, arg) in enumerate(cases, start=1):
        result, after = outcome(digits, arg)
        name = f"decimal_{number}"
        text += load_digits(name, digits)
        if call == "unit":
            # append_digit publishes nothing about the count, so the caller
            # tests the room shift_front requires before calling it.
            text += f"  let held_{number} = {name}.count;\n"
            text += bound_block(f"held_{number}", u64(800 - arg), number * 10 + 4)
            text += f"  let done_{number} = shift_front(decimal: &{name}, delta: {u64(arg)});\n"
        else:
            text += f"  let result_{number} = {call}(decimal: &{name}, shift: {arg}_u32);\n"
            text += check_block(f"result_{number}", u64(result), number * 10 + 1)
        text += f"  let digest_{number} = digest(decimal: &{name});\n"
        text += check_block(f"digest_{number}", u64(decimal_digest(after)), number * 10 + 2)
        text += f"  let count_{number} = {name}.count;\n"
        text += check_block(f"count_{number}", u64(len(after)), number * 10 + 3)
    return text + finish()


def multiply_outcome(digits, shift):
    width = len(digits)
    product = int(digits) << shift
    carry = product // 10 ** width
    assert carry <= 1 << 60
    return carry, str(product % 10 ** width).zfill(width)


def divide_outcome(digits, shift):
    width = len(digits)
    number = int(digits)
    return number & ((1 << shift) - 1), str(number >> shift).zfill(width)


def front_outcome(digits, delta):
    return None, "0" * delta + digits


def w2a():
    return (HEADER + "\n" + DECIMAL + "\n" + MULTIPLY + "\n"
            + w2_main("Multiplies decimals by powers of two and compares the carry, the digits and the count with big-integer arithmetic done outside the compiler.",
                      W2_MULTIPLY_CASES, "multiply", multiply_outcome))


def w2b():
    return (HEADER + "\n" + DECIMAL + "\n" + SHIFT_RIGHT + "\n"
            + w2_main("Divides decimals by powers of two and compares the remainder, the quotient digits and the count with big-integer arithmetic done outside the compiler.",
                      W2_DIVIDE_CASES, "shift_right", divide_outcome))


FRONT_DOC = "Checks that each decimal has the room the move needs, moves its digits toward the end and compares the digest and the count with the expected string."


def w2c():
    return (HEADER + "\n" + DECIMAL + "\n" + SHIFT_FRONT + "\n"
            + w2_main(FRONT_DOC, W2_FRONT_CASES, "unit", front_outcome))


def w2d():
    return (HEADER + "\n" + DECIMAL + "\n" + SHIFT_FRONT_RESTATED + "\n"
            + w2_main(FRONT_DOC, W2_FRONT_CASES, "unit", front_outcome))


# W2e: the w2b gap apart from the decimal buffer. The quotient of a total
# below ten divisors is below ten, which needs cancelling the divisor.

QUOTIENT_BOUND = """fn leading_digit(total: u64, divisor: u64) -> result: u64 pure contract {
  requires 1_u64 <= divisor;
  requires divisor <= 1152921504606846976_u64;
  requires total < 10_u64 * divisor;
  ensures result <= 9_u64;
} {
  doc "The quotient of a total below ten divisors by the divisor, which is below ten.";
  let quotient = total / divisor;
  return quotient;
}
"""

W2E_CASES = [
    (0, 1),
    (9, 1),
    (99, 10),
    (37, 4),
    (10 * (1 << 60) - 1, 1 << 60),
]


def w2e():
    text = HEADER + "\n" + QUOTIENT_BOUND + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Divides five totals and compares each quotient with the one computed outside the compiler.";\n'
    for number, (total, divisor) in enumerate(W2E_CASES, start=1):
        assert 1 <= divisor <= 1 << 60 and total < 10 * divisor
        text += f"  let digit_{number} = leading_digit(total: {u64(total)}, divisor: {u64(divisor)});\n"
        text += check_block(f"digit_{number}", u64(total // divisor), number)
    return text + finish()


# W2f: the w2c gap apart from the decimal buffer. The requirement bounds the
# sum, and the postcondition states that bound over the field it is stored in.

COUNTER = """struct Counter {
  doc "One count.";
  count: u64;
}

fn advance(counter: &Counter, delta: u64) -> result: unit writes(counter) contract {
  requires counter^.count + delta <= 800_u64;
  ensures counter^.count <= 800_u64;
} {
  doc "Adds delta to the count; the requirement bounds the sum that is stored.";
  let count = counter^.count;
  let total = count + delta;
  set counter^.count = total;
  return unit;
}
"""

W2F_CASES = [
    (5, 7),
    (0, 800),
    (800, 0),
    (399, 401),
]


# W2g adds w2c's count type invariant to the counter, and w2h drops the
# postcondition: each changes one thing, to find why the requirement bounds
# the copied count in w2c but not in w2f (DESIGN.md, E2 results).
COUNTER_BOUNDED = COUNTER.replace(
    "  count: u64;\n}",
    "  count: u64;\n  invariant count_ceiling(counter): counter.count <= 800_u64;\n}",
)
assert COUNTER_BOUNDED != COUNTER
COUNTER_UNSTATED = COUNTER.replace("  ensures counter^.count <= 800_u64;\n", "")
assert COUNTER_UNSTATED != COUNTER


def counter_witness(program):
    text = HEADER + "\n" + program + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Advances four counters after checking their room and compares each count with the sum computed outside the compiler.";\n'
    for number, (start, delta) in enumerate(W2F_CASES, start=1):
        assert start + delta <= 800
        text += f"  let counter_{number} = Counter(count: {u64(start)});\n"
        text += f"  let held_{number} = counter_{number}.count;\n"
        text += bound_block(f"held_{number}", u64(800 - delta), number * 10 + 1)
        text += f"  advance(counter: &counter_{number}, delta: {u64(delta)});\n"
        text += f"  let count_{number} = counter_{number}.count;\n"
        text += check_block(f"count_{number}", u64(start + delta), number * 10 + 2)
    return text + finish()


def w2f():
    return counter_witness(COUNTER)


def w2g():
    return counter_witness(COUNTER_BOUNDED)


def w2h():
    return counter_witness(COUNTER_UNSTATED)


# ---------------------------------------------------------------------------
# W3: writing a computed-length text into a window of the caller's bytes
# ---------------------------------------------------------------------------

WRITE_HEAD = """enum WriteError {
  TooSmall();
}

fn write_decimal(destination: &[u8], start: u64, end: u64, value: u64) -> result: Result<u64, WriteError> writes(destination) contract {
  requires start <= end;
  requires end <= destination^.len;
} {
  doc "%s";
  let length = 1_u64;
  let rest = value / 10_u64;
  for (
    step in 0_u64..19_u64,
    invariant grown: length <= step + 1_u64
  ) {
    if rest != 0_u64 {
      set length = length + 1_u64;
      set rest = rest / 10_u64;
    }
  }
"""

WRITE_WINDOW = WRITE_HEAD % (
    "Writes the decimal text of a value into the window destination[start..end) when the text fits, and reports TooSmall when it does not. The window is a range reference, so its length is end minus start and every subscript is below the length the single test establishes."
) + """  let window = &destination^[start..end];
  let room = window^.len;
  if length <= room {
    let position = length;
    let remaining = value;
    loop @emit (
      invariant placed: position <= length
    ) {
      if 0_u64 < position {
      } else {
        break @emit;
      }
      let at = position - 1_u64;
      let low = remaining % 10_u64;
      let digit = cvt::<u64, u8>(low);
      let byte = digit + 48_u8;
      set window^[at] = byte;
      set remaining = remaining / 10_u64;
      set position = at;
    }
    return Ok<u64, WriteError>(value: length);
  }
  let error = WriteError::TooSmall();
  return Err<u64, WriteError>(error: error);
}
"""

WRITE_INDEX = WRITE_HEAD % (
    "The same write spelled with plain indices: one test that end minus start is at least the length, and the position start plus at for each digit."
) + """  let room = end - start;
  if length <= room {
    let position = length;
    let remaining = value;
    loop @emit (
      invariant placed: position <= length
    ) {
      if 0_u64 < position {
      } else {
        break @emit;
      }
      let at = position - 1_u64;
      let index = start + at;
      let low = remaining % 10_u64;
      let digit = cvt::<u64, u8>(low);
      let byte = digit + 48_u8;
      set destination^[index] = byte;
      set remaining = remaining / 10_u64;
      set position = at;
    }
    return Ok<u64, WriteError>(value: length);
  }
  let error = WriteError::TooSmall();
  return Err<u64, WriteError>(error: error);
}
"""

BUFFER_DIGEST = """fn buffer_digest(buffer: &[u8]) -> result: u64 reads(buffer) {
  doc "FNV-1a over every byte of a window.";
  let count = buffer^.len;
  let hash = 14695981039346656037_u64;
  for (index in 0_u64..count) {
    let byte = buffer^[index];
    let wide = cvt::<u8, u64>(byte);
    let mixed = ixor(hash, wide);
    set hash = mixed *wrap 1099511628211_u64;
  }
  return hash;
}
"""

W3_SIZE = 32
W3_CASES = [
    (0, 3, 8),
    (18446744073709551615, 2, 30),
    (12345, 0, 4),
    (7, 5, 5),
    (1000000, 10, 17),
    (999999, 31, 32),
]


def w3_model(value, start, end):
    buffer = bytearray(W3_SIZE)
    text = str(value).encode()
    if len(text) <= end - start:
        buffer[start:start + len(text)] = text
        return len(text), bytes(buffer)
    return None, bytes(buffer)


def w3_main(write_text):
    text = HEADER + "\n" + write_text + "\n" + BUFFER_DIGEST + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Writes six values into a 32-byte buffer at different windows and compares the outcome and the whole buffer with the text computed outside the compiler.";\n'
    for number, (value, start, end) in enumerate(W3_CASES, start=1):
        written, image = w3_model(value, start, end)
        text += f"  let buffer_{number} = array_filled::<u8, {W3_SIZE}>(value: 0_u8);\n"
        text += f"  let window_{number} = &buffer_{number}[0_u64..{u64(W3_SIZE)}];\n"
        text += f"  let outcome_{number} = write_decimal(destination: window_{number}, start: {u64(start)}, end: {u64(end)}, value: {u64(value)});\n"
        text += f"  match outcome_{number} {{\n"
        text += "    Ok(value: length) => {\n"
        if written is None:
            text += f"      return exit_status(code: {u8(number * 10 + 1)});\n"
        else:
            text += f"      if length == {u64(written)} {{\n      }} else {{\n        return exit_status(code: {u8(number * 10 + 1)});\n      }}\n"
        text += "    }\n"
        text += "    Err(error: failure) => {\n"
        if written is not None:
            text += f"      return exit_status(code: {u8(number * 10 + 2)});\n"
        text += "    }\n  }\n"
        text += f"  let view_{number} = &buffer_{number}[0_u64..{u64(W3_SIZE)}];\n"
        text += f"  let digest_{number} = buffer_digest(buffer: view_{number});\n"
        text += check_block(f"digest_{number}", u64(fnv(image)), number * 10 + 3)
    return text + finish()


def w3a():
    return w3_main(WRITE_WINDOW)


def w3b():
    return w3_main(WRITE_INDEX)


# ---------------------------------------------------------------------------
# W4: the decimal-text scanner
# ---------------------------------------------------------------------------

SCAN = """fn scan(input: &[u8], start: u64) -> (mantissa: u64, dropped: u64, fraction: u64, next: u64) reads(input) contract {
  requires start <= input^.len;
} {
  doc "Reads digits with at most one point from start: a digit joins the mantissa while another cannot overflow it, and is otherwise counted as dropped; fraction counts the digits kept after the point; next is the first byte not consumed.";
  let length = input^.len;
  let index = start;
  let mantissa = 0_u64;
  let dropped = 0_u64;
  let fraction = 0_u64;
  let seen_point = False();
  loop @text (
    invariant ahead: start <= index,
    invariant inside: index <= length,
    invariant fraction_seen: fraction <= index,
    invariant dropped_seen: dropped <= index
  ) {
    if index < length {
    } else {
      break @text;
    }
    let byte = input^[index];
    let following = index + 1_u64;
    if 48_u8 <= byte {
      if byte <= 57_u8 {
        let digit = byte - 48_u8;
        let wide = cvt::<u8, u64>(digit);
        if mantissa < 1844674407370955161_u64 {
          let scaled = mantissa * 10_u64;
          set mantissa = scaled + wide;
          if seen_point {
            set fraction = fraction + 1_u64;
          }
        } else {
          set dropped = dropped + 1_u64;
        }
        set index = following;
        continue @text;
      }
    }
    if byte == 46_u8 {
      if seen_point {
        break @text;
      }
      set seen_point = True();
      set index = following;
      continue @text;
    }
    break @text;
  }
  return mantissa, dropped, fraction, index;
}
"""

W4_CASES = [
    ("123.4500xyz", 0),
    ("x1", 1),
    ("0.000000000000000000000001", 0),
    ("18446744073709551615999.5", 0),
    ("1.2.3", 0),
    (".5", 0),
    ("99999999999999999999", 0),
    ("abc", 3),
]


def scan_model(text, start):
    mantissa = dropped = fraction = 0
    seen_point = False
    index = start
    while index < len(text):
        c = text[index]
        if "0" <= c <= "9":
            if mantissa < 1844674407370955161:
                mantissa = mantissa * 10 + int(c)
                if seen_point:
                    fraction += 1
            else:
                dropped += 1
            index += 1
        elif c == ".":
            if seen_point:
                break
            seen_point = True
            index += 1
        else:
            break
    return mantissa, dropped, fraction, index


def w4():
    text = HEADER + "\n"
    for number, (sample, _) in enumerate(W4_CASES, start=1):
        body = ", ".join(u8(ord(c)) for c in sample)
        text += f"const sample_{number}: Array<u8, {len(sample)}> =[{body}];\n\n"
    text += SCAN + "\n"
    text += "fn main() -> status: ExitStatus pure {\n"
    text += '  doc "Scans eight samples and compares all four results of each with a model written outside the compiler.";\n'
    for number, (sample, start) in enumerate(W4_CASES, start=1):
        mantissa, dropped, fraction, nxt = scan_model(sample, start)
        text += f"  let input_{number} = &sample_{number}[0_u64..{u64(len(sample))}];\n"
        text += (f"  let (mantissa_{number}, dropped_{number}, fraction_{number}, next_{number}) = "
                 f"scan(input: input_{number}, start: {u64(start)});\n")
        text += check_block(f"mantissa_{number}", u64(mantissa), number * 10 + 1)
        text += check_block(f"dropped_{number}", u64(dropped), number * 10 + 2)
        text += check_block(f"fraction_{number}", u64(fraction), number * 10 + 3)
        text += check_block(f"next_{number}", u64(nxt), number * 10 + 4)
    return text + finish()


# ---------------------------------------------------------------------------

WITNESSES = [
    ("w1a_table_lookup", w1a, "accept"),
    # Language gap: the imulhi fact row computes its bounds in checked i128,
    # so for two full-width u64 words it overflows and establishes neither
    # bound nor the high <= factor relations (DESIGN.md, E2 results).
    ("w1b_product_and_scale", w1b, "reject"),
    # W1b's gap apart from the table.
    ("w1c_high_word_bound", w1c, "reject"),
    ("w2a_left_shift", w2a, "open"),
    # Language gap: no rule bounds ishr(total, shift) by total's bound in units
    # of 2^shift, so the quotient digit's bound below ten is unprovable
    # (DESIGN.md, E2 results).
    ("w2b_right_shift", w2b, "reject"),
    # Candidate language gap: a field reached through a reference has no
    # affine image, so the sum bound the requirement gives does not reach the
    # count invariant over the stored field (DESIGN.md, E2 results).
    ("w2c_front_shift", w2c, "reject"),
    # W2c with that bound restated as an invariant.
    ("w2d_front_shift_restated", w2d, "accept"),
    # W2b's gap apart from the decimal buffer.
    ("w2e_quotient_bound", w2e, "reject"),
    # W2c's candidate gap apart from the decimal buffer; refused earlier, at
    # the overflow check of the copied count (DESIGN.md, E2 results).
    ("w2f_field_bound", w2f, "reject"),
    # Predicted: the type invariant bounds the copied count, so the refusal
    # moves to FN-9 at the return, as in w2c.
    ("w2g_field_bound_invariant", w2g, "reject"),
    # Predicted: the postcondition is not the difference, so OP-2 stays.
    ("w2h_field_sum", w2h, "reject"),
    ("w3a_window_write", w3a, "open"),
    ("w3b_index_write", w3b, "open"),
    ("w4_scan", w4, "open"),
]


def main(argv):
    mode = argv[1] if len(argv) > 1 else "write"
    if mode == "--expect":
        for name, _, expected in WITNESSES:
            print(f"{name}\t{expected}")
        return 0
    status = 0
    for name, build, _ in WITNESSES:
        path = os.path.join(HERE, name + ".wf")
        text = build()
        if mode == "--check":
            try:
                with open(path, encoding="utf-8") as handle:
                    current = handle.read()
            except FileNotFoundError:
                current = None
            if current != text:
                print(f"{name}.wf differs from the generator output", file=sys.stderr)
                status = 1
        elif mode == "write":
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
        else:
            print("usage: generate.py [--check | --expect]", file=sys.stderr)
            return 2
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))
