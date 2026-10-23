import WhitefootProof.Syntax

/-!
# The checker's judgments without facts

The first half of the fragment's checker: literal ranges [FORM-7], name
resolution [TYPE-6, GRAM-11], statement-local typing [TYPE-5, OP-1, OP-2,
OP-4, OP-5], the shape of a contract [FN-8], and statement reachability and
completion over the structural control-flow graph [FN-1]. Each judgment fails
with the rule a conforming checker cites. A program that passes them never makes the interpreter stop with
`illFormed`: every name resolves, every operand has the type its operation
takes, and every loop exit resolves. The fact judgments (`Checker.lean`) run
after these succeed.
-/

namespace Whitefoot

/-- A rejection: the rule the checker cites and what fails. -/
structure Rejection where
  rule : String
  msg : String
  deriving DecidableEq, Repr

/-- A judgment that either succeeds or rejects the program. -/
abbrev Judge := Except Rejection

def rejectBy {α : Type} (rule msg : String) : Judge α := .error ⟨rule, msg⟩

namespace Static

/-- The function records the prelude contributes to every module [PRE-1]: each
enters the lexical IDENT domain as a callable [TYPE-6]. -/
def preludeFunctions : List String := [
  "append", "array_filled", "box_array_filled", "box_new", "box_paged_new",
  "box_ring_new", "box_segments_filled", "box_slots_new", "claim_killed",
  "claim_written", "decision_cell_new", "decision_cell_read", "decision_cell_share",
  "free_empty", "frozen_new", "frozen_share", "grow", "grow_paged", "insert_at",
  "key_set_bytes", "key_set_insert", "key_set_new", "key_set_read_key", "map_clear",
  "map_count", "map_scan", "map_scan_within", "paged_page_len", "place_back",
  "place_front", "release", "remove_at", "ring_new", "shared_close", "shared_map_new",
  "shared_new", "shared_read", "shared_read_share", "shared_share", "slots_from_array",
  "slots_into_array", "slots_new", "split_off", "swap", "take_back", "take_front"]

/-- Whether an alias names a module rather than a function or a type: such an
alias is in both use classes of the lexical IDENT domain [TYPE-6]. -/
def isModulePath : List String → Bool
  | ["std"] | ["std", "process"] => true
  | _ => false

/-- The module aliases of a program, which compete with every lexical
declaration of their spelling [TYPE-6]. -/
def moduleAliases (p : Program) : List String :=
  (p.aliases.filter fun (n, target) => isModulePath target && n.front.isLower).map (·.1)

/-- A written type with its alias resolved, so that two spellings of one type
compare equal. -/
def normTy (p : Program) : Ty → Ty
  | .named path => .named (p.expand path)
  | t => t

def exitStatusTy : Ty := .named ["std", "process", "ExitStatus"]

/-- The values in scope, innermost first: each name with its type. -/
abbrev Scope := List (String × Ty)

/-- The type of the value a name declares in scope; a name nothing in scope
declares does not resolve. -/
def lookup (sc : Scope) (x : String) : Judge Ty :=
  match sc.find? (·.1 == x) with
  | some (_, t) => .ok t
  | none => rejectBy "TYPE-6" s!"the name `{x}` is not in scope"

-- spec FORM-7 c21702627a33020e
-- spec FORM-7 a1daa343146d3542
/-- A typed integer literal's value lies in its type [FORM-7]. The parser
already rejects a literal outside it; the checker judges the tree, which need
not come from the parser. -/
def literalOk (t : IntTy) (v : Int) : Judge Unit :=
  if decide (t.Mem v) then .ok () else rejectBy "FORM-7" s!"the literal {v} lies outside its type"

/-- An atom's type: a literal carries its type, a name has its binding's type,
and an element subscript selects an element of an `Array` by a `u64` offset. -/
def atomTy (sc : Scope) : Atom → Judge Ty
  | .int t v => do literalOk t v; .ok (.int t)
  | .unit => .ok .unit
  | .var x => lookup sc x
  | .index b off => do
    let bt ← lookup sc b
    let ot ← atomTy sc off
    match bt with
    | .array t _ =>
      if ot == .int .u64 then .ok (.int t)
      else rejectBy "TYPE-5" s!"the subscript of `{b}` is not a u64"
    | _ => rejectBy "OP-4" s!"`{b}` is not an Array"

-- spec OP-2 7dbfa2db3467236c
-- spec OP-2 4ab9e3d81edb2369
/-- An infix operation's result type: an arithmetic row over two operands of
one integer type has that type and its `.defined` form `Bool`; a comparison
of two integers of one type, or `==` and `!=` of two `Bool`s, has type `Bool`.
-/
def infixTy (op : InfixOp) (ta tb : Ty) : Judge Ty :=
  match op, ta, tb with
  | .compare _, .int s, .int t =>
    if s == t then .ok .bool else rejectBy "TYPE-5" "a comparison of two integer types"
  | .compare .eq, .bool, .bool | .compare .ne, .bool, .bool => .ok .bool
  | .compare _, _, _ => rejectBy "TYPE-5" "a comparison of operands it does not take"
  | .divide _ .wrap, _, _ => rejectBy "OP-1" "there is no wrap division or remainder"
  | .arith _ m, .int s, .int t | .divide _ m, .int s, .int t =>
    if s != t then rejectBy "TYPE-5" "an operation on two integer types"
    else if m == .defined then .ok .bool else .ok (.int s)
  | _, _, _ => rejectBy "TYPE-5" "an integer operation on operands that are not integers"

-- spec OP-2 fac7a2bd6bf7ab80
/-- A table operation's result type: negation and absolute value take one
signed integer, `band`, `bor` and `bxor` two `Bool`s and `bnot` one. -/
def opTy (o : TableOp) (ts : List Ty) : Judge Ty :=
  match o, ts with
  | .unary _ m, [.int t] =>
    if !t.signed then rejectBy "OP-1" "negation or absolute value of an unsigned type"
    else if m == .defined then .ok .bool else .ok (.int t)
  | .unary _ _, [_] => rejectBy "TYPE-5" "negation or absolute value of a value that is not an integer"
  | .band, [.bool, .bool] | .bor, [.bool, .bool] | .bxor, [.bool, .bool]
  | .bnot, [.bool] => .ok .bool
  | .band, [_, _] | .bor, [_, _] | .bxor, [_, _] | .bnot, [_] =>
    rejectBy "TYPE-5" "a Boolean operation on operands that are not Bool"
  | _, _ => rejectBy "OP-1" "a table operation with the wrong number of operands"

-- spec GRAM-11 75ffabdce81ea617
-- spec GRAM-11 5f21c4ab83cca24d
-- spec GRAM-11 0c6bdaffe5e5254c
/-- A call's arguments must be named as its callee names its parameters, in
declared order, each once [GRAM-11]. -/
def namedArgs (names : List String) (args : List (String × Atom)) : Judge Unit :=
  if args.map (·.1) == names then .ok ()
  else rejectBy "GRAM-11" s!"the arguments are not named ({", ".intercalate names}) in order"

-- spec TYPE-5 46d0a358c40a73b0
/-- Each argument's type is its parameter's declared type exactly [TYPE-5]. -/
def argTypes (sc : Scope) (p : Program) : List (Atom × Ty) → Judge Unit
  | [] => .ok ()
  | (a, t) :: rest => do
    let ta ← atomTy sc a
    if ta != normTy p t then rejectBy "TYPE-5" "an argument's type is not its parameter's"
    argTypes sc p rest

-- spec TYPE-5 71de3eb8364d5a30
-- spec TYPE-5 6e86d72b3113476a
-- spec GRAM-11 f68dcba2f70185cd
-- spec GRAM-11 6a6500d3934371e2
/-- An expression's type. A call of a function of the program has its declared
result type, `exit_status` makes an `ExitStatus` from a `u8` code, and
`array_filled::<T, n>` an `Array<T, n>` from a value of type T. -/
def exprTy (p : Program) (sc : Scope) : Expr → Judge Ty
  | .atom a => atomTy sc a
  | .infix a op b => do infixTy op (← atomTy sc a) (← atomTy sc b)
  | .op o as => do opTy o (← as.mapM (atomTy sc))
  | .bool _ => .ok .bool
  | .call path targs as =>
    match p.callee? path, targs with
    | some (.fn f), [] => do
      namedArgs (f.params.map (·.name)) as
      argTypes sc p ((as.map (·.2)).zip (f.params.map (·.ty)))
      .ok (normTy p f.resultTy)
    | some .exitStatus, [] => do
      namedArgs ["code"] as
      argTypes sc p ((as.map (·.2)).zip [.int .u8])
      .ok exitStatusTy
    | some .arrayFilled, [.ty (.int t), .nat n] => do
      namedArgs ["value"] as
      argTypes sc p ((as.map (·.2)).zip [.int t])
      .ok (.array t n)
    | some (.fn _), _ => rejectBy "OP-1" "a nongeneric function takes no written argument"
    | _, _ => rejectBy "TYPE-6" s!"the callee `{"::".intercalate path}` does not resolve"

-- spec FN-8 ef741fd6cc43a493
-- spec FN-8 50b1578b2547bad0
-- spec FN-9 71118b8660d37bed
-- spec INV-1 97e537c1a0782695
-- spec INV-1 4d9283cbf102401b
-- spec INV-1 c9ca441f8ba0930e
-- spec INV-1 ef0dc8739ff62c37
-- spec INV-1 e0e9940924b8cec0
-- spec MSR-4 5d22615f4e699a0a
/-- An affine expression's names resolve to integer values in scope; a `*` has
one direct literal operand [INV-1]. `rule` is the rule that owns the position:
INV-1 for an invariant, FN-8 for a requirement, FN-9 for a postcondition. -/
def affineOk (rule : String) (sc : Scope) : Affine → Judge Unit
  | .atom (.int t v) => literalOk t v
  | .atom (.var x) =>
    (match sc.find? (·.1 == x) with
    | some (_, .int _) => .ok ()
    | some _ => rejectBy rule s!"`{x}` is not an integer value"
    | none => rejectBy rule s!"the name `{x}` is not in scope")
  | .atom _ => rejectBy rule "an affine factor that is not a name or an integer literal"
  | .add a b | .sub a b => do affineOk rule sc a; affineOk rule sc b
  | .mul a b => do
    affineOk rule sc a
    affineOk rule sc b
    match a, b with
    | .atom (.int _ _), _ | _, .atom (.int _ _) => .ok ()
    | _, _ => rejectBy rule "a product with no direct literal operand"

-- spec INV-1 96a91d8f58236079
-- spec INV-1 82e8e94035dab07f
-- spec INV-1 11ff1d49c75d3157
-- spec INV-1 1f717d964c34370d
/-- An invariant's relation is `==`, `<=`, `<`, `>=` or `>` over two affine
expressions whose names are live integer values [INV-1]. -/
def invariantOk (sc : Scope) (r : Relation) : Judge Unit := do
  if r.op == .ne then rejectBy "INV-1" "an invariant cannot be a disequality"
  affineOk "INV-1" sc r.lhs
  affineOk "INV-1" sc r.rhs

/-- The loops a `break` or `continue` may resolve to, innermost first: each
loop's label, if it writes one. -/
abbrev Loops := List (Option String)

/-- A `break` or `continue` resolves to the innermost enclosing loop, or with
a label to the innermost enclosing loop that writes it [TYPE-6]. -/
def resolveLoop (loops : Loops) (label : Option String) : Judge Unit :=
  match label with
  | none => if loops.isEmpty then rejectBy "TYPE-6" "a break or continue outside a loop" else .ok ()
  | some l =>
    if loops.contains (some l) then .ok ()
    else rejectBy "TYPE-6" s!"no enclosing loop is labeled `{l}`"

/-- What the statement judgment carries down a body: the values in scope, the
lexical names a new value may not shadow, the enclosing loops, and the live
invariant names. -/
structure Ctx where
  scope : Scope
  /-- Spellings no new value may use: the parameters, the module aliases and
  every enclosing live binding [TYPE-6]. -/
  taken : List String
  loops : Loops
  invariants : List String

/-- A new value declaration may not use a spelling a live competing entry
holds [TYPE-6]. -/
def fresh (c : Ctx) (x : String) : Judge Unit :=
  if c.taken.contains x then rejectBy "TYPE-6" s!"`{x}` shadows a live declaration" else .ok ()

/-- The judgments of one statement list, with whether its entry is reachable:
returns whether its normal exit is reachable and how many `break`s it holds
whose target loop is reachable from them, by target depth. A statement whose
entry is unreachable is rejected [FN-1].

`breaks` counts the reachable `break` edges leaving to each enclosing loop:
index 0 is the innermost. -/
structure Flow where
  normal : Bool
  breaks : List Nat
  deriving Repr

/-- Add two flows' break counts elementwise. -/
def addBreaks : List Nat → List Nat → List Nat
  | [], bs => bs
  | as, [] => as
  | a :: as, b :: bs => (a + b) :: addBreaks as bs

/-- A break to the loop at depth `d`, innermost 0. -/
def breakAt : Nat → List Nat
  | 0 => [1]
  | d + 1 => 0 :: breakAt d

/-- The depth of the loop a label resolves to. -/
def loopDepth (loops : Loops) : Option String → Nat
  | none => 0
  | some l => (loops.findIdx? (· == some l)).getD 0

mutual
-- spec FN-1 d52eed4970a914b5
-- spec FN-1 93fbb6e9617a6cb7
-- spec FN-1 a8f9d6296fddd5f0
/-- Check a statement list from an entry whose reachability is `live`. -/
def stmts (p : Program) (f : FnDecl) (c : Ctx) (live : Bool) (ss : List Stmt) : Judge Flow :=
  match ss with
  | [] => .ok { normal := live, breaks := [] }
  | s :: rest => do
    if !live then rejectBy "FN-1" "a statement no path from the function's entry reaches"
    let (c', fl) ← stmt p f c s
    let fr ← stmts p f c' fl.normal rest
    .ok { normal := fr.normal, breaks := addBreaks fl.breaks fr.breaks }
termination_by structural ss

-- spec SET-1 c9aeb4e2926e47d3
-- spec SET-1 e6168fc62d2685c5
-- spec FN-1 3383434bcf16aa80
-- spec FN-1 117a5a0177832bfe
-- spec FN-1 9382d2575bc551e3
-- spec OP-5 b9afb72ab2a98cdf
-- spec TYPE-5 b5cd6822f7e891be
-- spec TYPE-5 1b82ee6b361d365b
-- spec TYPE-5 d410749e22a1df1b
-- spec TYPE-5 90c92218ba569076
-- spec INV-1 f64f7344179bc86c
-- spec INV-1 a2376b1ecd2c93e5
-- spec INV-1 b4528fe71667ebf9
-- spec INV-1 9c854e87624bb8e5
-- spec ENT-5 b197069fcb21e417
/-- Check one reachable statement: returns the context for the statements
after it and its flow. -/
def stmt (p : Program) (f : FnDecl) (c : Ctx) (s : Stmt) : Judge (Ctx × Flow) :=
  match s with
  | .let_ x e => do
    fresh c x
    let t ← exprTy p c.scope e
    .ok ({ c with scope := (x, t) :: c.scope, taken := x :: c.taken },
      { normal := true, breaks := [] })
  | .set t e => do
    let root := match t with
      | .var x => x
      | .index b _ => b
    if (c.scope.find? (·.1 == root)).isNone then
      rejectBy "SET-1" s!"the target `{root}` resolves to no binding in scope"
    let target ← match t with
      | .var x => lookup c.scope x
      | .index b off => atomTy c.scope (.index b off)
    let te ← exprTy p c.scope e
    if te != target then rejectBy "TYPE-5" "a set of a value of another type"
    .ok (c, { normal := true, breaks := [] })
  | .call e => do
    match e with
    | .call _ _ _ => discard (exprTy p c.scope e)
    | _ => rejectBy "TYPE-5" "an expression statement that is not a call"
    .ok (c, { normal := true, breaks := [] })
  | .ret e => do
    let te ← exprTy p c.scope e
    if te != normTy p f.resultTy then rejectBy "FN-1" "a return of a value of another type"
    .ok (c, { normal := false, breaks := [] })
  | .if_ cond thn els _ => do
    let tc ← exprTy p c.scope cond
    if tc != .bool then rejectBy "OP-5" "an if condition that is not a Bool"
    let a ← stmts p f c true thn
    let b ← stmts p f c true els
    .ok (c, { normal := a.normal || b.normal, breaks := addBreaks a.breaks b.breaks })
  | .loop label invs body => do
    if let some l := label then
      if c.loops.contains (some l) then rejectBy "TYPE-6" s!"the label `{l}` shadows a live label"
    let names := invs.map (·.1)
    if names.eraseDups.length != names.length then
      rejectBy "INV-1" "two invariants of one header share a name"
    for n in names do
      if c.invariants.contains n then rejectBy "TYPE-6" s!"the invariant `{n}` shadows a live one"
    for (_, r) in invs do invariantOk c.scope r
    let inner := { c with loops := label :: c.loops, invariants := names ++ c.invariants }
    let fb ← stmts p f inner true body
    let exits := fb.breaks.headD 0
    .ok (c, { normal := exits > 0, breaks := fb.breaks.drop 1 })
  | .brk label => do
    resolveLoop c.loops label
    .ok (c, { normal := false, breaks := breakAt (loopDepth c.loops label) })
  | .cont label => do
    resolveLoop c.loops label
    .ok (c, { normal := false, breaks := [] })
  | .invariant n r => do
    if c.invariants.contains n then rejectBy "TYPE-6" s!"the invariant `{n}` shadows a live one"
    invariantOk c.scope r
    .ok ({ c with invariants := n :: c.invariants }, { normal := true, breaks := [] })
termination_by structural s
end

-- spec FN-1 4fc8064295ff593a
-- spec FN-8 e9fa53d76339eda6
-- spec FN-8 5815d9a878353f32
-- spec FN-9 4915fe73fda60469
-- spec FN-9 d31c4e246221ef7a
-- spec FN-9 ced9537ad03f5d04
-- spec FN-9 a4b5ca93e9458548
-- spec TYPE-5 1368c102317b63af
-- spec TYPE-5 de1f592ea4f18a8d
/-- A function's declaration: distinct parameter names that no module alias
holds, a result name distinct from them [FN-9], a nonempty contract block
[FN-8] whose clauses name parameters (and, in a postcondition, the result),
and a body whose normal exit is unreachable [FN-1]. -/
def checkFn (p : Program) (f : FnDecl) : Judge Unit := do
  let mods := moduleAliases p
  let names := f.params.map (·.name)
  if names.eraseDups.length != names.length then rejectBy "TYPE-6" "two parameters share a name"
  for n in names do
    if mods.contains n then rejectBy "TYPE-6" s!"the parameter `{n}` shadows a module alias"
  if names.contains f.result || mods.contains f.result then
    rejectBy "FN-9" s!"the result name `{f.result}` competes with another declaration"
  if f.contract && f.requires.isEmpty && f.ensures.isEmpty then
    rejectBy "FN-8" "an empty contract block"
  let params : Scope := f.params.reverse.map fun prm => (prm.name, normTy p prm.ty)
  for r in f.requires do
    affineOk "FN-8" params r.lhs
    affineOk "FN-8" params r.rhs
  match normTy p f.resultTy with
  | .int t =>
    for r in f.ensures do
      affineOk "FN-9" ((f.result, .int t) :: params) r.lhs
      affineOk "FN-9" ((f.result, .int t) :: params) r.rhs
  | _ => if !f.ensures.isEmpty then rejectBy "FN-9" "a postcondition of a result that is not an integer"
  let c : Ctx := { scope := params, taken := names ++ mods, loops := [], invariants := [] }
  let fl ← stmts p f c true f.body
  if fl.normal then rejectBy "FN-1" s!"the body of `{f.name}` falls through"

/-- The program's declarations: no function takes the spelling of a prelude
function, a module alias or another alias [TYPE-6], and every function passes
its judgments. -/
def checkProgram (p : Program) : Judge Unit := do
  for f in p.fns do
    if preludeFunctions.contains f.name then
      rejectBy "TYPE-6" s!"`{f.name}` redeclares a prelude function"
    if p.aliases.any (·.1 == f.name) then
      rejectBy "TYPE-6" s!"`{f.name}` redeclares an alias"
  for f in p.fns do checkFn p f

end Static

end Whitefoot
