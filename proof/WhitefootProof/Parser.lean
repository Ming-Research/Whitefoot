import WhitefootProof.Syntax
import WhitefootProof.Lexer

/-!
# The fragment's parser

A recursive-descent parser over [GRAM-1]'s formed tokens that follows the
productions of [GRAM-2] to [GRAM-5]. Every grammar decision reads at most two
tokens, as [GRAM-1] guarantees is enough. Where a production has an
alternative outside the fragment, the parser stops with `outside` naming the
construct; where the source derives from no production, or a literal is not
well formed [FORM-7], it stops with `reject`. A program that parses is then
resolved: every callee and type must name a function of the program or one of
the prelude and host declarations the fragment uses, and no function may call
itself through a chain of calls, since the fragment's calls are non-recursive.

The parser is `partial`: nothing is proved about it. The proofs are stated over
the tree it produces.
-/

namespace Whitefoot

/-- Why a source is not a fragment program. -/
inductive ParseError where
  /-- The source uses a construct outside the fragment. -/
  | outside (what : String)
  /-- The rules reject the source before name resolution: a raw lexical defect,
  a token sequence no production derives, or an ill-formed literal. -/
  | reject (rule msg : String)
  deriving Repr, Inhabited

namespace Parser

/-- A parser: the token array to read, and the position as state. -/
abbrev P := StateT Nat (ReaderT (Array Tok) (Except ParseError))

def fail {α : Type} (e : ParseError) : P α := throw e

def outside {α : Type} (what : String) : P α := fail (.outside what)

def reject {α : Type} (rule msg : String) : P α := fail (.reject rule msg)

/-- The token `k` places ahead of the cursor. -/
def peekAt (k : Nat) : P (Option Tok) := do
  let ts ← read
  return ts[(← get) + k]?

def peek : P (Option Tok) := peekAt 0

def advance : P Unit := modify (· + 1)

def atEnd : P Bool := do return decide ((← read).size ≤ (← get))

def show? : Option Tok → String
  | some (.lower s) | some (.upper s) | some (.opname s) | some (.oper s) | some (.num s)
  | some (.punct s) => s!"`{s}`"
  | some (.label s) => s!"`@{s}`"
  | some (.chr s) => s!"`{s}`"
  | some (.str _) => "a STRING"
  | none => "the end of the source"

/-- Consume one exact token, or reject where the production needs it. -/
def expect (t : Tok) (rule : String) : P Unit := do
  let next ← peek
  if next == some t then advance else
    reject rule s!"expected {show? (some t)}, found {show? next}"

def isTok (t : Tok) : P Bool := do return (← peek) == some t

def punct (s : String) : Tok := .punct s

def kw (s : String) : Tok := .lower s

/-- Every lowercase spelling of a fixed grammar atom: none of them is an IDENT
[FORM-3]. -/
def keywords : List String :=
  ["alias", "apart", "atomic", "binding", "break", "const", "continue", "contract", "copy",
    "define", "doc", "drop", "else", "ensures", "entry", "enum", "f32", "f64", "fn", "for",
    "forall", "give", "held", "i16", "i32", "i64", "i8", "if", "in", "interface",
    "invariant", "is", "let", "loop", "match", "move", "musttail", "no_heap", "nocopy",
    "nodrop", "not", "opaque", "package", "pkg", "program", "propagate", "public", "pure",
    "readonly", "reads", "requires", "return", "set", "spawn", "std", "struct", "times",
    "u16", "u32", "u64", "u8", "unchanged", "unit", "use", "waits", "when", "writes"]

def isIdent : Option Tok → Bool
  | some (.lower s) => !keywords.contains s
  | _ => false

def ident (rule : String) : P String := do
  match ← peek with
  | some (.lower s) =>
    if keywords.contains s then reject rule s!"expected an IDENT, found `{s}`"
    else do advance; return s
  | t => reject rule s!"expected an IDENT, found {show? t}"

def typeid (rule : String) : P String := do
  match ← peek with
  | some (.upper s) => do advance; return s
  | t => reject rule s!"expected a TYPEID, found {show? t}"

def intTy? : String → Option IntTy
  | "i8" => some .i8 | "i16" => some .i16 | "i32" => some .i32 | "i64" => some .i64
  | "u8" => some .u8 | "u16" => some .u16 | "u32" => some .u32 | "u64" => some .u64
  | _ => none

def digitsValue (cs : List Char) : Nat :=
  cs.foldl (fun n c => 10 * n + (c.toNat - '0'.toNat)) 0

-- spec FORM-5 1095ae64c71de4f0
-- spec FORM-7 c21702627a33020e
-- spec FORM-7 a1daa343146d3542
/-- A numeric-form token in literal position: a decimal integer `-?d_T` whose
value lies in T, with no leading zero and no `-0` [FORM-5, FORM-7]. A float
literal is outside the fragment. -/
def intLiteral (s : String) : P Atom := do
  let cs := s.toList
  let (neg, body) := match cs with
    | '-' :: rest => (true, rest)
    | _ => (false, cs)
  let digits := body.takeWhile Char.isDigit
  let rest := body.dropWhile Char.isDigit
  match rest with
  | '_' :: suffix =>
    match intTy? (Lexer.ofChars suffix) with
    | some t =>
      let n : Int := digitsValue digits
      let value := if neg then -n else n
      if digits.isEmpty then reject "FORM-5" s!"`{s}` is not a literal"
      else if digits.length > 1 && digits.head? == some '0' then
        reject "FORM-7" s!"`{s}` has a leading zero"
      else if neg && n == 0 then reject "FORM-7" s!"`{s}`: `-0` is written `0`"
      else if neg && !t.signed then reject "FORM-7" s!"`{s}`: a `-` needs a signed type"
      else if !decide (t.Mem value) then reject "FORM-7" s!"`{s}` lies outside its type"
      else return .int t value
    | none =>
      if suffix == ['f', '3', '2'] || suffix == ['f', '6', '4'] then outside "float literals"
      else outside s!"the literal `{s}`"
  | _ =>
    if rest.any (fun c => c == '.' || c == 'e' || c == 'E') then outside "float literals"
    else reject "FORM-5" s!"`{s}` is not a literal"

-- spec OP-2 a361e882fc6dbb5e
-- spec OP-2 2549511d4fc76a0b
-- spec OP-2 e887ad5c7088061b
/-- The `infix_op` and `compare_op` tokens [GRAM-5], with the fragment's
reading of each. -/
def infixOp? : Option Tok → Option (P InfixOp)
  | some (.oper s) =>
    let arith (op : ArithOp) : String → P InfixOp
      | "" => pure (.arith op .exact)
      | "wrap" => pure (.arith op .wrap)
      | "defined" => pure (.arith op .defined)
      | "checked" => outside "checked operations"
      | _ => outside "saturating operations"
    let divide (op : DivOp) : String → P InfixOp
      | "" => pure (.divide op .exact)
      | "defined" => pure (.divide op .defined)
      | "checked" => outside "checked operations"
      | m => reject "GRAM-5" s!"`{s}` is not an infix_op: there is no {m} form"
    match s.toList with
    | '+' :: m => some (arith .add (Lexer.ofChars m))
    | '-' :: m => some (arith .sub (Lexer.ofChars m))
    | '*' :: m => some (arith .mul (Lexer.ofChars m))
    | '/' :: m => some (divide .div (Lexer.ofChars m))
    | '%' :: m => some (divide .rem (Lexer.ofChars m))
    | _ => none
  | some (.punct "==") => some (pure (.compare .eq))
  | some (.punct "!=") => some (pure (.compare .ne))
  | some (.punct "<") => some (pure (.compare .lt))
  | some (.punct "<=") => some (pure (.compare .le))
  | some (.punct ">") => some (pure (.compare .gt))
  | some (.punct ">=") => some (pure (.compare .ge))
  | _ => none

def compareOp? : Option Tok → Option CmpOp
  | some (.punct "==") => some .eq
  | some (.punct "!=") => some .ne
  | some (.punct "<") => some .lt
  | some (.punct "<=") => some .le
  | some (.punct ">") => some .gt
  | some (.punct ">=") => some .ge
  | _ => none

/-- The dotless operation names of [OP-1]'s table: an IDENT callee with one of
these spellings names that operation, never a function. -/
def dotlessOperations : List String :=
  ["ineg", "iabs", "band", "bor", "bxor", "bnot", "cvt", "iand", "ior", "ixor", "inot",
    "ishl", "ishr", "irotl", "irotr", "ipopcount", "iclz", "ictz", "ibswap", "imulhi", "imin",
    "imax", "reinterpret", "feq", "flt", "fle", "fgt", "fge", "fne", "fneg", "fabs",
    "fcopysign", "fmin", "fmax", "ffloor", "fceil", "ftrunc", "froundeven", "frem", "finf",
    "fnan"]

/-- The fragment's reading of a table operation's name. -/
def tableOp (name : String) : P TableOp :=
  match name with
  | "ineg" => pure (.unary .neg .exact)
  | "ineg.wrap" => pure (.unary .neg .wrap)
  | "ineg.defined" => pure (.unary .neg .defined)
  | "iabs" => pure (.unary .abs .exact)
  | "iabs.wrap" => outside "`iabs.wrap`, whose result the rules give only at MIN(T)"
  | "iabs.defined" => pure (.unary .abs .defined)
  | "band" => pure .band
  | "bor" => pure .bor
  | "bxor" => pure .bxor
  | "bnot" => pure .bnot
  | _ => outside s!"the operation `{name}`"

/-- Whether the next tokens begin a `call` rather than an `atom`: an OPNAME, a
`musttail` or `spawn`, a qualified path, a constructor, or an IDENT followed
by `(` or `::`. -/
def startsCall : P Bool := do
  match ← peek with
  | some (.opname _) | some (.upper _) => return true
  | some (.lower "musttail") | some (.lower "spawn") | some (.lower "pkg")
  | some (.lower "std") => return true
  | some (.lower s) =>
    if keywords.contains s then return false
    let t ← peekAt 1
    return t == some (punct "(") || t == some (punct "::")
  | _ => return false

-- spec GRAM-3 616a6403114b8bdf
-- spec GRAM-3 010bad98e365de0e
-- spec GRAM-3 e48d0bd3f4ae1882
-- spec GRAM-3 326f5efccfb46958
-- spec GRAM-3 bc3d34d20d190968
-- spec GRAM-3 fc47c53108f6b200
mutual

/-- A `type` [GRAM-3]. -/
partial def type : P Ty := do
  match ← peek with
  | some (.lower "unit") => do advance; return .unit
  | some (.lower "f32") | some (.lower "f64") => outside "floats"
  | some (.lower s) =>
    match intTy? s with
    | some t => do advance; return .int t
    | none =>
      if s == "pkg" || s == "std" || isIdent (some (.lower s)) then
        let path ← typePath
        if (← isTok (punct "<")) then outside "generic nominal types" else return .named path
      else reject "GRAM-3" s!"expected a type, found `{s}`"
  | some (.upper "Bool") => do
    advance
    if (← isTok (punct "<")) then reject "GRAM-3" "`Bool` takes no type arguments"
    else return .bool
  | some (.upper "Array") => do
    advance
    let args ← targs
    match args with
    | [.ty (.int t), .nat n] => return .array t n
    | [.ty _, .nat _] => outside "arrays of non-integer elements"
    | _ => outside "runtime-capacity and generic arrays"
  | some (.upper s) => do
    advance
    if (← isTok (punct "<")) then outside "generic nominal types" else return .named [s]
  | t => reject "GRAM-3" s!"expected a type, found {show? t}"

/-- A `type_path`: `std::process::ExitStatus` [GRAM-3]. -/
partial def typePath : P (List String) := do
  let first ← match ← peek with
    | some (.lower s) => do advance; pure s
    | t => reject "GRAM-3" s!"expected a type path, found {show? t}"
  expect (punct "::") "GRAM-3"
  let rec segments : P (List String) := do
    match ← peek with
    | some (.upper s) => do advance; return [s]
    | some (.lower s) => do
      if !isIdent (some (.lower s)) then reject "GRAM-3" s!"`{s}` cannot be a path segment"
      advance
      expect (punct "::") "GRAM-3"
      return s :: (← segments)
    | t => reject "GRAM-3" s!"expected a path segment, found {show? t}"
  return first :: (← segments)

/-- `targs` [GRAM-3]: types and decimal constants. -/
partial def targs : P (List TArg) := do
  expect (punct "<") "GRAM-3"
  let rec items : P (List TArg) := do
    let arg ← targ
    if (← isTok (punct ",")) then do advance; return arg :: (← items)
    else do expect (punct ">") "GRAM-3"; return [arg]
  items

/-- One `targ` [GRAM-3]. -/
partial def targ : P TArg := do
  match ← peek with
  | some (.num s) =>
    if s.toList.all Char.isDigit then
      if (← infixOp? <$> peekAt 1).isSome then outside "computed const arguments"
      else do advance; return .nat (digitsValue s.toList)
    else reject "GRAM-3" s!"`{s}` is not a const argument"
  | some (.lower "fn") => outside "function arguments"
  | some (.lower s) =>
    if isIdent (some (.lower s)) && (← peekAt 1) != some (punct "::") then
      outside "const-generic arguments"
    else return .ty (← type)
  | _ => return .ty (← type)

end

-- spec GRAM-5 e79cfb23ed89bd34
-- spec GRAM-5 90dd8f7117070e81
-- spec GRAM-5 c1abce14ecb76af5
-- spec GRAM-5 8e3f7d4ce1ba3460
-- spec GRAM-5 65cc6c30d10078db
-- spec FORM-5 79a659b49ac34a6b
/-- An `atom` [GRAM-5]: a literal, or a place made of an IDENT and at most
one element subscript. -/
partial def atom : P Atom := do
  match ← peek with
  | some (.num s) => do advance; intLiteral s
  | some (.chr _) => outside "character literals"
  | some (.lower "unit") => do advance; return .unit
  | some (.lower "move") => outside "moves"
  | some (.lower "entry") => outside "entry-state places"
  | some (.punct "&") => outside "borrows"
  | some (.lower _) =>
    let name ← ident "GRAM-5"
    match ← peek with
    | some (.punct "[") => do
      advance
      let offset ← atom
      if (← isTok (punct "..")) then outside "range steps"
      expect (punct "]") "GRAM-5"
      match ← peek with
      | some (.punct "[") | some (.punct ".") | some (.punct "^") =>
        outside "places with more than one step"
      | _ => return .index name offset
    | some (.punct ".") => outside "field and payload steps"
    | some (.punct "^") => outside "references"
    | _ => return .var name
  | t => reject "GRAM-5" s!"expected an atom, found {show? t}"

/-- A `place` in `set` target position [SET-1]. -/
def target : P Target := do
  match ← atom with
  | .var x => return .var x
  | .index x i => return .index x i
  | _ => reject "GRAM-5" "a `set` target is a place"

-- spec GRAM-5 116c62d1ea9d440e
-- spec GRAM-5 b19d6b2cdfd31b55
-- spec GRAM-5 cf90b3b46d7ed69f
/-- The operand list of a call: empty, `atom_list`, or `fieldinit_list`; the
second token decides [GRAM-5]. Positional operands come back with empty
names. -/
partial def args : P (Bool × List (String × Atom)) := do
  expect (punct "(") "GRAM-5"
  if (← isTok (punct ")")) then do advance; return (true, [])
  let named := isIdent (← peek) && (← peekAt 1) == some (punct ":")
  let rec items : P (List (String × Atom)) := do
    let item ← if named then do
        let n ← ident "GRAM-5"
        expect (punct ":") "GRAM-5"
        pure (n, ← atom)
      else do pure ("", ← atom)
    if (← isTok (punct ",")) then do advance; return item :: (← items)
    else do expect (punct ")") "GRAM-5"; return [item]
  return (named, ← items)

/-- A `callee_path` after `pkg::`, `std::` or an IDENT and `::` [GRAM-5]. -/
partial def calleePath : P (List String) := do
  match ← peek with
  | some (.lower s) =>
    if !isIdent (some (.lower s)) then reject "GRAM-5" s!"`{s}` cannot be a callee segment"
    advance
    if (← isTok (punct "::")) && (← peekAt 1) != some (punct "<") then do
      advance
      return s :: (← calleePath)
    else return [s]
  | some (.upper _) => outside "member calls of nominal types"
  | t => reject "GRAM-5" s!"expected a callee path, found {show? t}"

-- spec GRAM-5 59341df70b21cda7
-- spec GRAM-5 aad310ce5a597312
-- spec GRAM-5 3580b76d096ba20a
-- spec GRAM-5 30c2aa98540ed97d
/-- A `call` [GRAM-5]: a table operation with positional operands, a function
call with named arguments [GRAM-11], or the `Bool` constructors. -/
partial def call : P Expr := do
  match ← peek with
  | some (.lower "musttail") => outside "tail calls"
  | some (.lower "spawn") => outside "spawned calls"
  | some (.opname s) => do
    advance
    let op ← tableOp s
    let (named, as) ← args
    if named && !as.isEmpty then reject "GRAM-11" s!"`{s}` takes positional operands"
    return .op op (as.map (·.2))
  | some (.upper s) => do
    advance
    if s != "True" && s != "False" then outside "construction"
    let (_, as) ← args
    if !as.isEmpty then reject "GRAM-8" s!"`{s}()` has no fields"
    return .bool (s == "True")
  | some (.lower s) => do
    if dotlessOperations.contains s then
      advance
      let op ← tableOp s
      let (named, as) ← args
      if named && !as.isEmpty then reject "GRAM-11" s!"`{s}` takes positional operands"
      return .op op (as.map (·.2))
    let path ← if s == "pkg" || s == "std" then do
        advance
        expect (punct "::") "GRAM-5"
        pure (s :: (← calleePath))
      else calleePath
    let ts ← if (← isTok (punct "::")) then do advance; targs else pure []
    let (named, as) ← args
    if !named && !as.isEmpty then
      reject "GRAM-11" s!"a call of `{"::".intercalate path}` names its arguments"
    return .call path ts as
  | t => reject "GRAM-5" s!"expected a call, found {show? t}"

-- spec GRAM-5 a3b1ce8e8256e482
-- spec GRAM-5 2d79c6dbb117a9d4
-- spec GRAM-5 3c3f225f76b1c08b
-- spec GRAM-5 7f0ecf45b6cbc706
-- spec GRAM-5 710f4a218925b785
-- spec GRAM-5 bfc4c4543e97b328
-- spec GRAM-5 cf0b87c4e965c4ba
-- spec GRAM-5 ef8657ec7b8bd511
-- spec GRAM-1 4cf65da241275fbe
/-- An `expr` [GRAM-5]: a call, or an atom with an optional infix tail whose
right operand is one atom. -/
partial def expr : P Expr := do
  if (← startsCall) then call
  else
    let lhs ← atom
    match infixOp? (← peek) with
    | some op => do
      advance
      let o ← op
      return .infix lhs o (← atom)
    | none => return .atom lhs

-- spec GRAM-4 c3a298c9dadcbff3
-- spec GRAM-4 27006d302111f3b2
-- spec GRAM-4 83d24ea69e2cb264
-- spec GRAM-4 9a211d65cd2a5ad7
mutual

/-- An `affine_expr` [GRAM-4]. -/
partial def affine : P Affine := do
  let first ← affineTerm
  let rec rest (acc : Affine) : P Affine := do
    match ← peek with
    | some (.oper "+") => do advance; rest (.add acc (← affineTerm))
    | some (.oper "-") => do advance; rest (.sub acc (← affineTerm))
    | _ => return acc
  rest first

/-- An `affine_term` [GRAM-4]. -/
partial def affineTerm : P Affine := do
  let first ← affineFactor
  if (← isTok (.oper "*")) then do advance; return .mul first (← affineFactor)
  else return first

/-- An `affine_factor` [GRAM-4]. -/
partial def affineFactor : P Affine := do
  if (← isTok (punct "(")) then do
    advance
    let a ← affine
    expect (punct ")") "GRAM-4"
    return a
  else if (← startsCall) then outside "calls in contract and invariant expressions"
  else return .atom (← atom)

end

-- spec GRAM-5 856d71f7894fc65d
-- spec GRAM-5 d9e2c24da760413d
-- spec GRAM-5 b946aacbece3343d
/-- A `clause_expr` of a contract [GRAM-5] in the fragment's comparison form. -/
def clause : P Relation := do
  if (← isTok (kw "forall")) then outside "range clauses"
  let lhs ← affine
  match compareOp? (← peek) with
  | some op => do advance; return { lhs, op, rhs := (← affine) }
  | none =>
    match ← peek with
    | some (.lower "unchanged") => outside "`unchanged` clauses"
    | some (.oper _) => outside "domain-query clauses"
    | _ => outside "Bool-valued clauses"

/-- An invariant's `IDENT ":" affine_expr compare_op affine_expr` [GRAM-4]. -/
def invariantBody : P (String × Relation) := do
  if (← isTok (kw "forall")) then outside "range clauses"
  let name ← ident "GRAM-4"
  expect (punct ":") "GRAM-4"
  let lhs ← affine
  match compareOp? (← peek) with
  | some op => do advance; return (name, { lhs, op, rhs := (← affine) })
  | none => reject "GRAM-4" "an invariant compares two affine expressions"

/-- The `header_invariant` list of a `loop`, after its `(` [GRAM-4]. -/
partial def headerInvariants : P (List (String × Relation)) := do
  expect (kw "invariant") "GRAM-4"
  let inv ← invariantBody
  if (← isTok (punct ",")) then do advance; return inv :: (← headerInvariants)
  else do expect (punct ")") "GRAM-4"; return [inv]

def label? : P (Option String) := do
  match ← peek with
  | some (.label s) => do advance; return some s
  | _ => return none

-- spec GRAM-4 e2d5fcb9c3a07717
-- spec GRAM-4 ea60d8cc51520ece
-- spec GRAM-4 1f7c8de34ef52b51
-- spec GRAM-4 1b3b983ab4d8bb00
-- spec GRAM-4 f9928d7375a57876
-- spec GRAM-4 31c9fc09cd64216c
-- spec GRAM-4 1d2792628d50588c
-- spec GRAM-4 a614eee3fa28e32c
-- spec GRAM-4 abb006370c9a1567
-- spec GRAM-4 a91cf5797cbb7491
-- spec GRAM-4 c9048c699b172a5d
-- spec GRAM-4 377cdde2162a1534
-- spec GRAM-4 f0c5999704d47c3f
-- spec GRAM-4 147e4e81097b886f
-- spec GRAM-4 d1106fab2ae24378
-- spec GRAM-4 a1598c08c5283f64
-- spec GRAM-4 55f1d9e8a8cf71f5
-- spec GRAM-4 dccff615ca049f81
mutual

/-- A `stmt` [GRAM-4]. -/
partial def stmt : P Stmt := do
  match ← peek with
  | some (.lower "let") => do
    advance
    if !isIdent (← peek) then
      match ← peek with
      | some (.punct "(") => outside "result-list bindings"
      | some (.upper _) | some (.lower "pkg") | some (.lower "std") =>
        outside "destructuring bindings"
      | t => reject "GRAM-4" s!"expected a binding, found {show? t}"
    let name ← ident "GRAM-4"
    expect (punct "=") "GRAM-4"
    match ← peek with
    | some (.lower "propagate") => outside "`propagate`"
    | some (.lower "match") => outside "`match`"
    | some (.lower "if") => outside "value `if`"
    | _ =>
      let e ← expr
      expect (punct ";") "GRAM-4"
      return .let_ name e
  | some (.lower "set") => do
    advance
    let t ← target
    expect (punct "=") "GRAM-4"
    let e ← expr
    expect (punct ";") "GRAM-4"
    return .set t e
  | some (.lower "return") => do
    advance
    let e ← expr
    if (← isTok (punct ",")) then outside "result lists"
    expect (punct ";") "GRAM-4"
    return .ret e
  | some (.lower "loop") => do
    advance
    let lbl ← label?
    let invs ← if (← isTok (punct "(")) then do advance; headerInvariants else pure []
    return .loop lbl invs (← block)
  | some (.lower "break") => do
    advance
    let lbl ← label?
    expect (punct ";") "GRAM-4"
    return .brk lbl
  | some (.lower "continue") => do
    advance
    let lbl ← label?
    expect (punct ";") "GRAM-4"
    return .cont lbl
  | some (.lower "if") => ifStmt
  | some (.lower "invariant") => do
    advance
    let (name, rel) ← invariantBody
    if (← isTok (punct "{")) then outside "proof uses"
    expect (punct ";") "GRAM-4"
    return .invariant name rel
  | some (.lower "for") => outside "counted loops"
  | some (.lower "match") => outside "`match`"
  | some (.lower "give") => outside "`give`"
  | some (.lower "atomic") => outside "atomic statements"
  | _ =>
    if !(← startsCall) then reject "GRAM-4" s!"expected a statement, found {show? (← peek)}"
    let e ← call
    expect (punct ";") "GRAM-4"
    return .call e

/-- An `if_stmt` [GRAM-4]. -/
partial def ifStmt : P Stmt := do
  expect (kw "if") "GRAM-4"
  let c ← expr
  let thn ← block
  if (← isTok (kw "else")) then do
    advance
    if (← isTok (kw "if")) then return .if_ c thn [← ifStmt] true
    else return .if_ c thn (← block) true
  else return .if_ c thn [] false

/-- `"{" stmt* "}"` [GRAM-4]. -/
partial def block : P (List Stmt) := do
  expect (punct "{") "GRAM-4"
  let rec items : P (List Stmt) := do
    if (← isTok (punct "}")) then do advance; return []
    else return (← stmt) :: (← items)
  items

end

/-- `doc STRING ;` [GRAM-2], which carries no semantics. -/
def doc : P Unit := do
  if (← isTok (kw "doc")) then
    advance
    match ← peek with
    | some (.str _) => advance
    | t => reject "GRAM-2" s!"expected a STRING, found {show? t}"
    expect (punct ";") "GRAM-2"

-- spec GRAM-2 560af56ef6d377ed
-- spec GRAM-2 6dcd5005201b5525
-- spec GRAM-2 f74da57a7d3e305b
/-- A `contract_block` [GRAM-2]: its requirements and its postconditions. -/
partial def contract : P (List Relation × List Relation) := do
  expect (kw "contract") "GRAM-2"
  expect (punct "{") "GRAM-2"
  if (← isTok (kw "define")) then outside "contract definitions"
  let rec requires : P (List Relation) := do
    if (← isTok (kw "requires")) then do
      advance
      if (← isTok (kw "held")) then outside "held requirements"
      let c ← clause
      expect (punct ";") "GRAM-2"
      return c :: (← requires)
    else return []
  let rec ensures : P (List Relation) := do
    if (← isTok (kw "ensures")) then do
      advance
      if (← isTok (kw "when")) then outside "routed postconditions"
      let c ← clause
      expect (punct ";") "GRAM-2"
      return c :: (← ensures)
    else return []
  let rs ← requires
  let es ← ensures
  expect (punct "}") "GRAM-2"
  return (rs, es)

-- spec GRAM-2 60c1c5a42de1c14e
-- spec GRAM-2 4ab85b4ea7bfda3f
-- spec GRAM-2 33be63670f7be798
-- spec GRAM-2 4846b289cb657f89
-- spec GRAM-2 f33b22da46e90f11
-- spec GRAM-2 155ad6ddbb4c8b8d
/-- A `fn_decl` with a body [GRAM-2]. -/
partial def fnDecl : P FnDecl := do
  expect (kw "fn") "GRAM-2"
  let name ← ident "GRAM-2"
  if (← isTok (punct "<")) then outside "generic functions"
  expect (punct "(") "GRAM-2"
  let rec params : P (List Param) := do
    let n ← ident "GRAM-2"
    expect (punct ":") "GRAM-2"
    if (← isTok (punct "&")) then outside "reference parameters"
    let p : Param := { name := n, ty := (← type) }
    if (← isTok (punct ",")) then do advance; return p :: (← params)
    else do expect (punct ")") "GRAM-2"; return [p]
  let ps ← if (← isTok (punct ")")) then do advance; pure [] else params
  expect (punct "->") "GRAM-2"
  if (← isTok (punct "(")) then outside "result lists"
  let result ← ident "GRAM-2"
  expect (punct ":") "GRAM-2"
  let resultTy ← type
  match ← peek with
  | some (.lower "pure") => advance
  | some (.lower "reads") | some (.lower "writes") => outside "effect rows"
  | t => reject "EFF-1" s!"expected an effect row, found {show? t}"
  if (← isTok (kw "waits")) then outside "waiting functions"
  let (rs, es) ← if (← isTok (kw "contract")) then contract else pure ([], [])
  match ← peek with
  | some (.punct ";") | some (.lower "doc") => outside "functions without a source body"
  | _ => pure ()
  expect (punct "{") "GRAM-2"
  doc
  let rec items : P (List Stmt) := do
    if (← isTok (punct "}")) then do advance; return []
    else return (← stmt) :: (← items)
  let body ← items
  return { name, params := ps, result, resultTy, requires := rs, ensures := es, body }

-- spec GRAM-2 565f16f1b7627f60
/-- An `alias_decl` [GRAM-2]. -/
partial def aliasDecl : P (String × List String) := do
  expect (kw "alias") "GRAM-2"
  let name ← match ← peek with
    | some (.upper s) => do advance; pure s
    | t => if isIdent t then ident "GRAM-2" else
        reject "GRAM-2" s!"expected an alias name, found {show? t}"
  expect (punct "=") "GRAM-2"
  let first ← match ← peek with
    | some (.lower s) =>
      if s == "pkg" || s == "std" || isIdent (some (.lower s)) then do advance; pure s
      else reject "GRAM-2" s!"expected a module root, found `{s}`"
    | t => reject "GRAM-2" s!"expected a module root, found {show? t}"
  let rec segments : P (List String) := do
    if (← isTok (punct "::")) then do
      advance
      match ← peek with
      | some (.upper s) | some (.lower s) => do advance; return s :: (← segments)
      | t => reject "GRAM-2" s!"expected a path segment, found {show? t}"
    else do expect (punct ";") "GRAM-2"; return []
  return (name, first :: (← segments))

-- spec GRAM-2 eefa4b69271b1b9d
-- spec GRAM-2 2febf647050d5a4c
-- spec GRAM-2 af5c23223f183ecc
/-- A `program` [GRAM-2] of `alias` and `fn` items. -/
partial def program : P Program := do
  let rec items (aliases : List (String × List String)) (fns : List FnDecl) : P Program := do
    if (← atEnd) then return { aliases := aliases.reverse, fns := fns.reverse }
    match ← peek with
    | some (.lower "alias") => items ((← aliasDecl) :: aliases) fns
    | some (.lower "fn") => items aliases ((← fnDecl) :: fns)
    | some (.lower "public") => outside "public items"
    | some (.lower "struct") | some (.lower "opaque") | some (.lower "nocopy")
    | some (.lower "nodrop") | some (.lower "enum") => outside "nominal type declarations"
    | some (.lower "interface") | some (.lower "binding") => outside "interfaces and bindings"
    | some (.lower "const") => outside "named consts"
    | some (.lower "program") => outside "the no-heap declaration"
    | t => reject "GRAM-2" s!"expected an item, found {show? t}"
  items [] []

end Parser

namespace Resolve


def checkTy (p : Program) : Ty → Except ParseError Unit
  | .named path =>
    if p.isExitStatus path then .ok () else .error (.outside s!"the type `{"::".intercalate path}`")
  | _ => .ok ()

def checkCall (p : Program) : Expr → Except ParseError (List String)
  | .call path targs _ =>
    match p.callee? path with
    | some (.fn f) => .ok [f.name]
    | some _ => do
      for a in targs do
        if let .ty t := a then checkTy p t
      .ok []
    | none => .error (.outside s!"the callee `{"::".intercalate path}`")
  | _ => .ok []

/-- The functions a statement list calls, or the first name that does not
resolve within the fragment. -/
partial def calls (p : Program) : List Stmt → Except ParseError (List String)
  | [] => .ok []
  | s :: rest => do
    let here ← match s with
      | .let_ _ e | .set _ e | .call e | .ret e => checkCall p e
      | .if_ c thn els _ => do
        let a ← checkCall p c
        let b ← calls p thn
        let d ← calls p els
        pure (a ++ b ++ d)
      | .loop _ _ body => calls p body
      | _ => pure []
    return here ++ (← calls p rest)

/-- Whether `f` reaches itself through calls. -/
partial def reaches (graph : List (String × List String)) (seen : List String) :
    String → String → Bool
  | target, f =>
    let next := (graph.find? (·.1 == f)).map (·.2) |>.getD []
    next.any fun g => g == target || (!seen.contains g && reaches graph (g :: seen) target g)

end Resolve

/-- Name resolution over a parsed program: duplicate functions are rejected,
names outside the fragment's declarations and recursive calls are outside the
fragment, and the entry `main` takes no parameter. -/
def Program.resolve (p : Program) : Except ParseError Unit := do
  let names := p.fns.map (·.name)
  if names.eraseDups.length != names.length then
    .error (.reject "TYPE-6" "two functions share a name")
  let mut graph : List (String × List String) := []
  for f in p.fns do
    for prm in f.params do Resolve.checkTy p prm.ty
    Resolve.checkTy p f.resultTy
    graph := (f.name, ← Resolve.calls p f.body) :: graph
  for f in p.fns do
    if Resolve.reaches graph [] f.name f.name then
      .error (.outside "recursive calls")
  match p.fn? "main" with
  | some f => if f.params.isEmpty then .ok () else .error (.outside "entries with parameters")
  | none => .error (.outside "programs without `main`")

/-- Lex, parse and resolve one source record. -/
def parseProgram (src : String) : Except ParseError Program := do
  let toks ← match Lexer.tokens src with
    | .ok ts => .ok ts
    | .error e => .error (.reject "GRAM-1" e)
  let (p, _) ← (Parser.program.run 0).run toks
  p.resolve
  return p

end Whitefoot
