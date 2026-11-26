/-!
# Raw lexical formation

[GRAM-1]'s raw formation: the source bytes are partitioned into tokens and
trivia, each cursor taking exactly one maximal form, with no grammar position,
name lookup or decoding consulted. A byte that starts no form is a raw lexical
defect. Literal values are decoded later, by the parser [FORM-5, FORM-7].

The model reads one source record of ASCII bytes as a list of characters.
-/

namespace Whitefoot

-- spec GRAM-1 55d55f6430cf2450
-- spec GRAM-1 3f71fd342320b966
/-- A formed token: its shape kind [GRAM-1] and exact bytes. -/
inductive Tok where
  | lower (s : String)
  | upper (s : String)
  | label (s : String)
  | opname (s : String)
  | oper (s : String)
  | num (s : String)
  | chr (s : String)
  | str (s : String)
  | punct (s : String)
  deriving DecidableEq, Repr, Inhabited

namespace Lexer

def ofChars (cs : List Char) : String :=
  cs.foldl String.push ""

def isLowerCont (c : Char) : Bool :=
  c.isLower || c.isDigit || c == '_'

def isUpperCont (c : Char) : Bool :=
  c.isAlpha || c.isDigit

/-- The six mode suffixes an operation name may carry [GRAM-1, FORM-3]. -/
def modeWords : List String :=
  ["wrap", "defined", "checked", "sat", "strict", "nearest"]

/-- The four suffixes an operator form may carry [GRAM-1]. -/
def operatorSuffixes : List String :=
  ["", "wrap", "defined", "checked", "sat"]

/-- The eight compound punctuation tokens [GRAM-1]. -/
def compounds : List (Char × Char) :=
  [('-', '>'), ('=', '>'), ('.', '.'), ('=', '='), ('!', '='), ('<', '='), ('>', '='),
    (':', ':')]

/-- The single-byte punctuation tokens [GRAM-1]. -/
def singles : List Char :=
  ['(', ')', '{', '}', '[', ']', '<', '>', ',', ':', ';', '.', '=', '&', '^']

/-- The maximal tail of a numeric form: letters, digits, `_` and `.`, a sign
right after `e` or `E`, and no `.` that begins `..`. -/
def numTail : List Char → List Char × List Char
  | [] => ([], [])
  | '.' :: '.' :: rest => ([], '.' :: '.' :: rest)
  | c :: tl@(s :: rest) =>
    if (c == 'e' || c == 'E') && (s == '+' || s == '-') then
      let (t, r) := numTail rest
      (c :: s :: t, r)
    else if c.isAlpha || c.isDigit || c == '_' || c == '.' then
      let (t, r) := numTail tl
      (c :: t, r)
    else
      ([], c :: tl)
  | [c] => if c.isAlpha || c.isDigit || c == '_' || c == '.' then ([c], []) else ([], [c])

/-- The interior of a character or STRING form up to its closing quote: raw
bytes `0x20`..`0x7e` other than `\` and the quote, or a two-byte escape.
Returns the interior and the rest after the closing quote. -/
def quoted (q : Char) : List Char → Option (List Char × List Char)
  | [] => none
  | c :: rest =>
    if c == q then
      some ([], rest)
    else if c == '\\' then
      match rest with
      | e :: rest' =>
        if e == '\\' || e == 'n' || e == 't' || e == 'r' || e == 'u' || e == q then
          (quoted q rest').map fun (body, r) => (c :: e :: body, r)
        else
          none
      | [] => none
    else if 0x20 ≤ c.toNat && c.toNat ≤ 0x7e then
      (quoted q rest).map fun (body, r) => (c :: body, r)
    else
      none

-- spec GRAM-1 b36c9bea493138d8
-- spec GRAM-1 14b053231a9aa466
-- spec GRAM-1 e0022814b2781ff0
-- spec GRAM-1 57bf3724cc057509
-- spec GRAM-1 562dbfd8a1520424
-- spec GRAM-1 37eeb71f72a29c92
-- spec GRAM-1 20e1d617d2cdabff
-- spec GRAM-1 24832fbb0049c771
-- spec GRAM-1 e54c75a23baf9076
-- spec GRAM-1 6668c55d33060772
-- spec GRAM-1 9b3b249ac9d9c946
-- spec GRAM-1 68e5c4b859b303f0
-- spec GRAM-1 2fbbe0c327cfa5f1
-- spec GRAM-1 82f48e0f8eeb3352
-- spec GRAM-1 f100096735cdeabe
-- spec GRAM-1 6c54d72bb4854630
-- spec GRAM-1 862889a8b68b6670
-- spec GRAM-1 bcfe1cb9d8d8165d
-- spec GRAM-1 987a7ee4f27a0e35
/-- One maximal form at the cursor: a token, or `none` for trivia, with the
rest; `.error` for a raw lexical defect. -/
def form : List Char → Except String (Option Tok × List Char)
  | [] => .error "no input"
  | c :: rest =>
    if c == ' ' then
      .ok (none, rest.dropWhile (· == ' '))
    else if c == '\n' then
      .ok (none, rest)
    else if c.isLower then
      let base := c :: rest.takeWhile isLowerCont
      let after := rest.dropWhile isLowerCont
      let opname := modeWords.findSome? fun w =>
        match after with
        | '.' :: tail =>
          if tail.take w.length == w.toList &&
              !((tail.drop w.length).head?.map isLowerCont' |>.getD false) then
            some (base ++ '.' :: w.toList, tail.drop w.length)
          else
            none
        | _ => none
      match opname with
      | some (cs, r) => .ok (some (.opname (ofChars cs)), r)
      | none => .ok (some (.lower (ofChars base)), after)
    else if c.isUpper then
      .ok (some (.upper (ofChars (c :: rest.takeWhile isUpperCont))), rest.dropWhile isUpperCont)
    else if c == '@' then
      match rest with
      | d :: rest' =>
        if d.isLower then
          .ok (some (.label (ofChars (d :: rest'.takeWhile isLowerCont))),
            rest'.dropWhile isLowerCont)
        else
          .error "a label sigil not followed by a lowercase letter"
      | [] => .error "a label sigil at the end of the input"
    else if c.isDigit || (c == '-' && (rest.head?.map Char.isDigit |>.getD false)) then
      let (t, r) := numTail rest
      .ok (some (.num (ofChars (c :: t))), r)
    else if c == '+' || c == '*' || c == '/' || c == '%' ||
        (c == '-' && rest.head? != some '>') then
      let suffix := rest.takeWhile Char.isLower
      if operatorSuffixes.contains (ofChars suffix) then
        .ok (some (.oper (ofChars (c :: suffix))), rest.dropWhile Char.isLower)
      else
        .error s!"the operator suffix {ofChars suffix} is not in the infix_op list"
    else if c == '\'' || c == '"' then
      match quoted c rest with
      | some (body, r) =>
        if c == '\'' then
          .ok (some (.chr (ofChars ('\'' :: body ++ '\'' :: r.takeWhile isUpperCont'))),
            r.dropWhile isUpperCont')
        else
          .ok (some (.str (ofChars body)), r)
      | none => .error "an unterminated or ill-formed quoted form"
    else
      match rest with
      | d :: rest' =>
        if compounds.contains (c, d) then
          .ok (some (.punct (ofChars [c, d])), rest')
        else if c == '!' then
          .error "a `!` not followed by `=`"
        else if singles.contains c then
          .ok (some (.punct (ofChars [c])), rest)
        else
          .error s!"the byte {c.toNat} starts no token"
      | [] =>
        if singles.contains c then
          .ok (some (.punct (ofChars [c])), [])
        else
          .error s!"the byte {c.toNat} starts no token"
where
  /-- An ASCII letter, digit or `_`: what may not follow an operation name. -/
  isLowerCont' (c : Char) : Bool := c.isAlpha || c.isDigit || c == '_'
  /-- The suffix of a character form: `[A-Za-z0-9_]`. -/
  isUpperCont' (c : Char) : Bool := c.isAlpha || c.isDigit || c == '_'

/-- The tokens of a source record, or the first raw lexical defect. Each
`form` consumes at least one byte, so `fuel` bytes bound the forms. -/
def tokensAux : Nat → List Char → Array Tok → Except String (Array Tok)
  | _, [], acc => .ok acc
  | 0, _, _ => .error "the token count exceeds the input length"
  | fuel + 1, cs, acc =>
    match form cs with
    | .error e => .error e
    | .ok (tok, rest) =>
      tokensAux fuel rest (match tok with | some t => acc.push t | none => acc)

-- spec GRAM-1 825147d90102bd09
/-- Raw lexical formation of one source record. -/
def tokens (src : String) : Except String (Array Tok) :=
  let cs := src.toList
  tokensAux (cs.length + 1) cs #[]

end Lexer

end Whitefoot
