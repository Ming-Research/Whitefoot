import WhitefootProof.Integers

/-!
# The fragment's syntax tree

The source tree of the first phase's fragment: integer and `Bool` values,
`let` and whole-binding or element `set`, `if`, `loop` with `break` and
`continue`, local `Array<T, N>` of an integer T built by `array_filled`,
element subscripts, calls with named arguments and `requires` and `ensures`
contracts, and `return` of one result. [GRAM-1] maps every production to one
node kind; this tree keeps a node for each production the fragment uses,
except where a production only groups others (`stmt`, `expr`'s factoring,
`affine_term`), and folds `alias` items into one table.

The tree is shaped for the proofs that follow: one plain inductive per syntactic
class, statements nested by `List Stmt`, and no positions. The parser
(`Parser.lean`) produces it and the interpreter (`Interpreter.lean`) runs it;
`research/investigations/spec-proof/DESIGN.md` records why it has this shape.
-/

namespace Whitefoot

/-- A type the fragment writes [GRAM-3]. `named` is a TYPEID or a path that
name resolution maps through the program's aliases, such as `ExitStatus`. -/
inductive Ty where
  | int (t : IntTy)
  | unit
  | bool
  | array (elem : IntTy) (n : Nat)
  | named (path : List String)
  deriving DecidableEq, Repr

/-- An `atom` [GRAM-5]: a typed integer literal, `unit`, a binding, or one
element subscript of a binding. -/
inductive Atom where
  | int (t : IntTy) (value : Int)
  | unit
  | var (name : String)
  | index (base : String) (offset : Atom)
  deriving DecidableEq, Repr

/-- A `set` target place [SET-1]: a whole binding or one of its elements. -/
inductive Target where
  | var (name : String)
  | index (base : String) (offset : Atom)
  deriving DecidableEq, Repr

/-- The mode of an integer infix operator [OP-1]: `+`, `+wrap`, `+defined`. -/
inductive Mode where
  | exact | wrap | defined
  deriving DecidableEq, Repr

/-- An `infix_op` or `compare_op` [GRAM-5] the fragment admits. -/
inductive InfixOp where
  | arith (op : ArithOp) (mode : Mode)
  | divide (op : DivOp) (mode : Mode)
  | compare (op : CmpOp)
  deriving DecidableEq, Repr

/-- A table operation called by name with positional operands [OP-1, GRAM-11]. -/
inductive TableOp where
  | unary (op : UnaryOp) (mode : Mode)
  | band | bor | bxor | bnot
  deriving DecidableEq, Repr

/-- A type or constant argument of a call [GRAM-3]: `array_filled::<u8, 4>`. -/
inductive TArg where
  | ty (t : Ty)
  | nat (n : Nat)
  deriving DecidableEq, Repr

/-- An `expr` [GRAM-5]. -/
inductive Expr where
  | atom (a : Atom)
  | infix (lhs : Atom) (op : InfixOp) (rhs : Atom)
  | op (o : TableOp) (args : List Atom)
  /-- A function call with named arguments [GRAM-11]; name resolution selects a
  function of the program or a prelude or host function. -/
  | call (callee : List String) (targs : List TArg) (args : List (String × Atom))
  /-- A nullary constructor call of the prelude `Bool`: `True()` or `False()`. -/
  | bool (b : Bool)
  deriving DecidableEq, Repr

/-- An `affine_expr` [GRAM-4], read over the mathematical integers. -/
inductive Affine where
  | atom (a : Atom)
  | add (lhs rhs : Affine)
  | sub (lhs rhs : Affine)
  | mul (lhs rhs : Affine)
  deriving DecidableEq, Repr

/-- A comparison between two affine expressions: a `clause_expr` of a contract
[GRAM-2], or the relation of a header invariant or `invariant_stmt` [GRAM-4]. -/
structure Relation where
  lhs : Affine
  op : CmpOp
  rhs : Affine
  deriving DecidableEq, Repr

/-- A `stmt` [GRAM-4]. -/
inductive Stmt where
  | let_ (name : String) (rhs : Expr)
  | set (target : Target) (rhs : Expr)
  /-- An expression statement: a call whose result is discarded. -/
  | call (e : Expr)
  | ret (e : Expr)
  /-- `if c { … } else { … }`; an absent `else` is an empty list, and
  `else if` is an `else` block holding one `if`. -/
  | if_ (cond : Expr) (thn els : List Stmt) (hasElse : Bool)
  | loop (label : Option String) (invariants : List (String × Relation)) (body : List Stmt)
  | brk (label : Option String)
  | cont (label : Option String)
  | invariant (name : String) (rel : Relation)
  deriving Repr

/-- A `param` of value mode [GRAM-2]. -/
structure Param where
  name : String
  ty : Ty
  deriving DecidableEq, Repr

/-- A `fn_decl` with a body [GRAM-2]: one result, the effect `pure`, and an
optional contract of `requires` and `ensures` clauses. -/
structure FnDecl where
  name : String
  params : List Param
  result : String
  resultTy : Ty
  /-- Whether the declaration writes a `contract_block`. -/
  contract : Bool
  requires : List Relation
  ensures : List Relation
  body : List Stmt
  deriving Repr

/-- A source bundle's one module [PROG-2]: its `alias` items and its functions. -/
structure Program where
  aliases : List (String × List String)
  fns : List FnDecl
  deriving Repr

namespace Program

/-- The function the program declares under a name. -/
def fn? (p : Program) (name : String) : Option FnDecl :=
  p.fns.find? (·.name == name)

/-- A written path with a leading alias replaced by its target. -/
def expand (p : Program) : List String → List String
  | [name] => match p.aliases.find? (·.1 == name) with
    | some (_, target) => target
    | none => [name]
  | path => path

end Program

/-- What a resolved callee names. -/
inductive Callee where
  /-- A function of the program. -/
  | fn (f : FnDecl)
  /-- The host function `std::process::exit_status`. -/
  | exitStatus
  /-- The prelude function `array_filled`. -/
  | arrayFilled

namespace Program

/-- The declaration a written callee path names. -/
def callee? (p : Program) (path : List String) : Option Callee :=
  match path with
  | [name] =>
    match p.fn? name with
    | some f => some (.fn f)
    | none =>
      match p.expand [name] with
      | ["std", "process", "exit_status"] => some .exitStatus
      | ["array_filled"] => some .arrayFilled
      | _ => none
  | ["std", "process", "exit_status"] => some .exitStatus
  | _ => none

/-- Whether a written type names the host type `std::process::ExitStatus`. -/
def isExitStatus (p : Program) (path : List String) : Bool :=
  p.expand path == ["std", "process", "ExitStatus"]

end Program

end Whitefoot
