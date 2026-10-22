import WhitefootProof.Syntax

/-!
# The fragment's interpreter

A small-step machine over the syntax tree. A configuration is a stack of call
frames; each frame holds its function, its bindings in scope, the entry values
of its parameters, a continuation of statement lists, block scopes and loop
markers, and where its result goes in its caller. `step` takes one statement or
one administrative move (leaving a block, repeating a loop body, returning);
`run` takes at most a given number of steps.

The machine stops in one of four ways:

- `done` with the entry's result;
- `stuck`, in one of the five states the specification gives no meaning: an
  exact operation whose domain query is false, a subscript outside its base, a
  read of a binding with no value, a call whose requirement is false, and a
  return whose postcondition is false. Each is the state the rules' static
  obligations exclude, so an accepted program never reaches one;
- `illFormed`, where a step would need a typing or naming judgment the static
  rules decide and the program fails it, such as adding a `u8` to a `u64`;
- `outOfSteps`, when the budget runs out.

Each definition is marked with the specification lines that give its step its
meaning. Contracts are read over the mathematical integers [FN-8] and checked
at the call and at the return, so that a false contract is a state of the
machine; the specification erases them [FN-9], and the machine's checks are
how the model says where the rules give a step no meaning.
-/

namespace Whitefoot

-- spec OP-2 fbe253f1ec4a8a2c
/-- A runtime value: an integer of a closed integer type, a `Bool`, `unit`, an
`Array<T, N>` of integers, or the host type `ExitStatus` carrying its code. -/
inductive Value where
  | int (t : IntTy) (value : Int)
  | bool (b : Bool)
  | unit
  | array (t : IntTy) (elems : List Int)
  | status (code : Int)
  deriving DecidableEq, Repr

/-- The five states the specification gives no meaning. -/
inductive Stuck where
  /-- An exact operation whose domain query is false [OP-2]. -/
  | domain
  /-- A subscript whose offset is not below its base's length [OP-4]. -/
  | bounds
  /-- A read or write of a binding that has no value. -/
  | noValue
  /-- A call whose requirement is false [FN-8]. -/
  | requires
  /-- A return whose postcondition is false [FN-9]. -/
  | ensures
  deriving DecidableEq, Repr

/-- Why an evaluation stops before its step completes. -/
inductive Halt where
  | stuck (s : Stuck)
  | illFormed (what : String)
  deriving DecidableEq, Repr

abbrev Eval := Except Halt

def ill {α : Type} (what : String) : Eval α := .error (.illFormed what)

def stuck {α : Type} (s : Stuck) : Eval α := .error (.stuck s)

/-- The bindings in scope, newest first. -/
abbrev Env := List (String × Value)

namespace Env

def get (env : Env) (x : String) : Eval Value :=
  match env.find? (·.1 == x) with
  | some (_, v) => .ok v
  | none => stuck .noValue

/-- Replace the value of the newest binding named `x`. -/
def put (env : Env) (x : String) (v : Value) : Eval Env :=
  match env with
  | [] => stuck .noValue
  | (y, w) :: rest =>
    if y == x then .ok ((x, v) :: rest) else do return (y, w) :: (← put rest x v)

/-- The bindings that were in scope when the environment had `depth` of them. -/
def restore (env : Env) (depth : Nat) : Env :=
  env.drop (env.length - depth)

end Env

namespace Value

-- spec TYPE-4 00c71ec1cfe67231
/-- Whether a value has a written type, exactly: there are no implicit
conversions. -/
def hasTy (p : Program) : Value → Ty → Bool
  | .int t _, .int t' => t == t'
  | .bool _, .bool => true
  | .unit, .unit => true
  | .array t es, .array t' n => t == t' && es.length == n
  | .status _, .named path => p.isExitStatus path
  | _, _ => false

/-- Whether two values have one type. -/
def sameTy : Value → Value → Bool
  | .int t _, .int t' _ => t == t'
  | .bool _, .bool _ => true
  | .unit, .unit => true
  | .array t es, .array t' es' => t == t' && es.length == es'.length
  | .status _, .status _ => true
  | _, _ => false

end Value

-- spec OP-4 f570fbbafdb636d5
-- spec OP-4 89228ce1fa7785fd
-- spec OP-4 352083e8b1804823
-- spec OP-4 1cdd1364a8ad0ca8
/-- An atom's value. An element subscript reads element `i` of an `Array`
whose length exceeds `i`; any other offset selects no element. -/
def evalAtom (env : Env) : Atom → Eval Value
  | .int t v => .ok (.int t v)
  | .unit => .ok .unit
  | .var x => env.get x
  | .index x off => do
    let base ← env.get x
    let i ← evalAtom env off
    match base, i with
    | .array t es, .int .u64 k =>
      if k < es.length then .ok (.int t (es.getD k.toNat 0)) else stuck .bounds
    | .array _ _, _ => ill "a subscript offset is not a u64"
    | _, _ => ill "a subscripted place is not an array"

-- spec OP-2 5503711c8aacc135
-- spec OP-2 cde0d6cacd62e4e1
-- spec OP-2 ce096ef35f4647b5
-- spec OP-2 822c3ccaf78ce417
-- spec OP-2 68823f08ba6f4e02
-- spec OP-2 0a955796e48be770
-- spec OP-2 7f1f67bf143f2482
/-- An infix operation over two operand values of one integer type. An exact
form outside its domain has no result; the comparisons and domain queries are
total. Value equality of two `Bool`s compares them. -/
def evalInfix (op : InfixOp) : Value → Value → Eval Value
  | .int t x, .int t' y =>
    if t != t' then ill "operands of two integer types" else
    match op with
    | .arith o .exact =>
      match o.exactForm t x y with
      | some r => .ok (.int t r)
      | none => stuck .domain
    | .arith o .wrap => .ok (.int t (o.wrapForm t x y))
    | .arith o .defined => .ok (.bool (o.defined t x y))
    | .divide o .exact =>
      match o.exactForm t x y with
      | some r => .ok (.int t r)
      | none => stuck .domain
    | .divide o .defined => .ok (.bool (o.defined t x y))
    | .divide _ .wrap => ill "there is no wrap division or remainder"
    | .compare c => .ok (.bool (c.eval x y))
  | .bool x, .bool y =>
    match op with
    | .compare .eq => .ok (.bool (x == y))
    | .compare .ne => .ok (.bool (x != y))
    | _ => ill "an integer operation on Bool operands"
  | _, _ => ill "an infix operation on operands it does not take"

-- spec OP-2 b5dc1d3d54f9afb7
-- spec OP-2 b2f9211f8c7d02cf
-- spec OP-2 1276e30b81cfa6c8
/-- A table operation called by name. Negation and absolute value take one
operand of a signed type; `band`, `bor`, `bxor` and `bnot` take `Bool`s. -/
def evalOp : TableOp → List Value → Eval Value
  | .unary u m, [.int t x] =>
    if !t.signed then ill "negation or absolute value of an unsigned type" else
    match m with
    | .exact =>
      match u.exactForm t x with
      | some r => .ok (.int t r)
      | none => stuck .domain
    | .wrap =>
      match u.wrapForm t x with
      | some r => .ok (.int t r)
      | none => ill "a wrap form the rules give no result"
    | .defined => .ok (.bool (u.defined t x))
  | .band, [.bool a, .bool b] => .ok (.bool (a && b))
  | .bor, [.bool a, .bool b] => .ok (.bool (a || b))
  | .bxor, [.bool a, .bool b] => .ok (.bool (a != b))
  | .bnot, [.bool a] => .ok (.bool (!a))
  | _, _ => ill "a table operation with operands it does not take"

/-- The value of an expression that calls no function of the program:
`exit_status` makes the status that reports its code, and `array_filled`
an `Array` whose every element is its value. -/
def evalExpr (p : Program) (env : Env) : Expr → Eval Value
  | .atom a => evalAtom env a
  | .infix a op b => do evalInfix op (← evalAtom env a) (← evalAtom env b)
  | .op o as => do evalOp o (← as.mapM (evalAtom env))
  | .bool b => .ok (.bool b)
  | .call path targs as =>
    match p.callee? path, targs, as with
    | some .exitStatus, [], [("code", a)] => do
      match ← evalAtom env a with
      | .int .u8 c => .ok (.status c)
      | _ => ill "exit_status takes a u8 code"
    | some .arrayFilled, [.ty (.int t), .nat n], [("value", a)] => do
      match ← evalAtom env a with
      | .int t' v => if t == t' then .ok (.array t (List.replicate n v)) else
          ill "array_filled's value is not of its element type"
      | _ => ill "array_filled's value is not an integer"
    | some (.fn _), _, _ => ill "a function call outside a call position"
    | _, _, _ => ill "a call whose callee or arguments do not resolve"

-- spec FN-8 5154cd12897bf13a
-- spec FN-8 3f865dc3f5237e05
-- spec GRAM-6 2cb9c41badbae3c8
/-- An affine expression over the mathematical integers: its `+`, `-` and `*`
perform no operation and have no domain. -/
def evalAffine (env : Env) : Affine → Eval Int
  | .atom a => do
    match ← evalAtom env a with
    | .int _ v => .ok v
    | _ => ill "an affine term that is not an integer"
  | .add x y => do return (← evalAffine env x) + (← evalAffine env y)
  | .sub x y => do return (← evalAffine env x) - (← evalAffine env y)
  | .mul x y => do return (← evalAffine env x) * (← evalAffine env y)

/-- Whether a relation between two affine expressions holds. -/
def Relation.holds (env : Env) (r : Relation) : Eval Bool := do
  return r.op.eval (← evalAffine env r.lhs) (← evalAffine env r.rhs)

/-- Whether every relation of a list holds. -/
def allHold (env : Env) : List Relation → Eval Bool
  | [] => .ok true
  | r :: rs => do
    if (← r.holds env) then allHold env rs else return false

/-- What follows a statement in its frame. -/
inductive Kont where
  /-- The rest of a block. -/
  | stmts (ss : List Stmt)
  /-- The end of a block: the bindings it declared leave scope. -/
  | scope (depth : Nat)
  /-- The end of a loop body: the body runs again. -/
  | loop (label : Option String) (body : List Stmt) (depth : Nat)
  deriving Repr

/-- Where a callee's result goes in its caller. -/
inductive Dest where
  /-- A `let` binds it. -/
  | bind (x : String)
  /-- A `set` writes it to a whole binding. -/
  | assign (x : String)
  /-- A `set` writes it to an element whose offset the target evaluated. -/
  | assignAt (x : String) (offset : Nat)
  /-- An expression statement discards it. -/
  | discard
  /-- A `return` returns it from the caller. -/
  | ret
  /-- It is the entry's result, which ends the run. -/
  | exit
  deriving DecidableEq, Repr

/-- One call's state. -/
structure Frame where
  fn : FnDecl
  env : Env
  entry : Env
  kont : List Kont
  dest : Dest
  deriving Repr

/-- The machine's state: the call frames, the running one first. -/
structure Config where
  frames : List Frame
  deriving Repr

/-- The result of one step. -/
inductive Step where
  | next (c : Config)
  | done (v : Value)
  | halt (h : Halt)

/-- How a run ends. -/
inductive Outcome where
  | done (v : Value)
  | stuck (s : Stuck)
  | illFormed (what : String)
  | outOfSteps
  deriving DecidableEq, Repr

def Step.ofEval {α : Type} (e : Eval α) (k : α → Step) : Step :=
  match e with
  | .ok a => k a
  | .error h => .halt h

-- spec FN-8 511e3847cbdb14c5
-- spec FN-8 bdea1ed7072175b6
-- spec PROG-3 3f894a05d8320644
/-- Enter a function with argument values named as it names its parameters,
in order and with its parameter types [GRAM-11]. Its requirements are read
over the arguments; a call whose requirement is false is not transferred. -/
def enter (p : Program) (g : FnDecl) (args : List (String × Value)) (dest : Dest)
    (callers : List Frame) : Step :=
  if args.map (·.1) != g.params.map (·.name) then .halt (.illFormed "argument names") else
  if !(args.zip g.params).all (fun (a, prm) => a.2.hasTy p prm.ty) then
    .halt (.illFormed "argument types") else
  let env := args.reverse
  Step.ofEval (allHold env g.requires) fun ok =>
    if !ok then .halt (.stuck .requires) else
    .next { frames := { fn := g, env, entry := env, kont := [.stmts g.body], dest } :: callers }

/-- Evaluate a statement's right-hand side. A call of a function of the
program enters it, sending its result to `dest`; any other expression's value
goes to `fin`. -/
def evalRhs (p : Program) (f : Frame) (callers : List Frame) (e : Expr) (dest : Dest)
    (k : List Kont) (fin : Value → Step) : Step :=
  match e with
  | .call path _ as =>
    match p.callee? path with
    | some (.fn g) =>
      Step.ofEval (as.mapM fun (n, a) => do return (n, ← evalAtom f.env a)) fun args =>
        enter p g args dest ({ f with kont := k } :: callers)
    | _ => Step.ofEval (evalExpr p f.env e) fin
  | _ => Step.ofEval (evalExpr p f.env e) fin

-- spec SET-1 88d43b4983637822
-- spec SET-1 072b9a0395f8c983
/-- Write a value into a frame's binding or one element of it; the value has
the type of what it replaces [TYPE-5]. -/
def write (f : Frame) (x : String) (offset : Option Nat) (v : Value) : Eval Frame := do
  let old ← f.env.get x
  let new ← match offset, old, v with
    | none, _, _ => if old.sameTy v then pure v else ill "a set of another type"
    | some i, .array t es, .int t' w =>
      if t == t' then pure (.array t (es.set i w)) else ill "an element set of another type"
    | some _, _, _ => ill "an element set of a value that is not an array element"
  return { f with env := (← f.env.put x new) }

-- spec FN-9 1a8c5bd424397cf5
-- spec FN-9 18262fff0ab222b1
-- spec FN-9 8de8aba69e3f17bf
-- spec FN-9 cda2ca4d680967a7
-- spec FN-1 1dde9a33051df474
-- spec FN-1 ab4d2f4320f57cfd
-- spec PROG-3 7ec63686cc236224
/-- Return a value from a frame: it has the declared result type, every
postcondition holds of it and of the parameters' entry values, and it goes
where the caller sends it. The entry's result ends the run. -/
def doReturn (p : Program) (v : Value) : List Frame → Step
  | [] => .halt (.illFormed "a return with no frame")
  | f :: callers =>
    if !v.hasTy p f.fn.resultTy then .halt (.illFormed "a return of another type") else
    match allHold ((f.fn.result, v) :: f.entry) f.fn.ensures with
    | .error h => .halt h
    | .ok false => .halt (.stuck .ensures)
    | .ok true =>
      match f.dest, callers with
      | .exit, _ => .done v
      | .ret, _ => doReturn p v callers
      | .bind x, g :: rest => .next { frames := { g with env := (x, v) :: g.env } :: rest }
      | .assign x, g :: rest =>
        Step.ofEval (write g x none v) fun g => .next { frames := g :: rest }
      | .assignAt x i, g :: rest =>
        Step.ofEval (write g x (some i) v) fun g => .next { frames := g :: rest }
      | .discard, g :: rest => .next { frames := g :: rest }
      | _, [] => .halt (.illFormed "a return with no caller")

/-- The continuation after the loop a `break` or `continue` resolves to: the
innermost loop, or the one with its label. Returns the loop's marker and what
follows it. -/
def findLoop (label : Option String) :
    List Kont → Option (Option String × List Stmt × Nat × List Kont)
  | [] => none
  | .loop l body d :: rest =>
    if label.isNone || label == l then some (l, body, d, rest) else findLoop label rest
  | _ :: rest => findLoop label rest

-- spec FN-1 8ceb7eb381b298a6
/-- `let x = e;` binds the value of `e`. -/
def stepLet (p : Program) (f : Frame) (callers : List Frame) (x : String) (e : Expr)
    (k : List Kont) : Step :=
  evalRhs p f callers e (.bind x) k fun v =>
    .next { frames := { f with env := (x, v) :: f.env, kont := k } :: callers }

-- spec SET-1 bc13c6f37f130a8b
-- spec SET-1 aa4513a8bf22a571
-- spec SET-1 b09526b422e10fb4
-- spec SET-1 296476a9dbac8acd
-- spec SET-1 117df770313c404b
-- spec OP-4 84dd7d280b494166
/-- `set p = e;` evaluates the target place, its subscript offset included,
then the right-hand side, then writes once. -/
def stepSet (p : Program) (f : Frame) (callers : List Frame) (t : Target) (e : Expr)
    (k : List Kont) : Step :=
  let commit (x : String) (offset : Option Nat) (v : Value) : Step :=
    Step.ofEval (write { f with kont := k } x offset v) fun g => .next { frames := g :: callers }
  match t with
  | .var x =>
    Step.ofEval (f.env.get x) fun _ =>
      evalRhs p f callers e (.assign x) k (commit x none)
  | .index x off =>
    Step.ofEval (evalAtom f.env (.index x off)) fun _ =>
      Step.ofEval (evalAtom f.env off) fun i =>
        match i with
        | .int _ i => evalRhs p f callers e (.assignAt x i.toNat) k (commit x (some i.toNat))
        | _ => .halt (.illFormed "a subscript offset is not a u64")

-- spec FN-1 1b8bfc7d20670172
-- spec OP-5 af38c43524906cf8
-- spec OP-5 d6475f1046091841
-- spec OP-5 f72d932bf4fcbb17
-- spec GRAM-6 fa3c95e89a631bdd
-- spec GRAM-6 b50dca70cef91d5c
/-- `if c { … } else { … }` enters the block its `Bool` condition selects; an
absent `else` block is empty, so a false condition goes on to the next
statement. -/
def stepIf (p : Program) (f : Frame) (callers : List Frame) (c : Expr) (thn els : List Stmt)
    (k : List Kont) : Step :=
  Step.ofEval (evalExpr p f.env c) fun v =>
    match v with
    | .bool b =>
      let kont : List Kont := .stmts (if b then thn else els) :: .scope f.env.length :: k
      .next { frames := { f with kont } :: callers }
    | _ => .halt (.illFormed "an if condition that is not a Bool")

/-- One statement of the running frame, followed by `k`. -/
def stepStmt (p : Program) (f : Frame) (callers : List Frame) (s : Stmt) (k : List Kont) :
    Step :=
  match s with
  | .let_ x e => stepLet p f callers x e k
  | .set t e => stepSet p f callers t e k
  | .call e => evalRhs p f callers e .discard k fun _ =>
      .next { frames := { f with kont := k } :: callers }
  | .ret e => evalRhs p f callers e .ret k fun v => doReturn p v (f :: callers)
  | .if_ c thn els _ => stepIf p f callers c thn els k
  | .loop label _ body => stepLoop f callers label body k
  | .brk label => stepBreak f callers label k
  | .cont label => stepContinue f callers label k
  | .invariant _ _ => .next { frames := { f with kont := k } :: callers }
where
  -- spec FN-1 e4a2e0019ae50357
  -- spec FN-1 5e402322980da8ac
  /-- `loop { … }` enters its body; the end of the body enters it again. -/
  stepLoop (f : Frame) (callers : List Frame) (label : Option String) (body : List Stmt)
      (k : List Kont) : Step :=
    let kont : List Kont := .stmts body :: .loop label body f.env.length :: k
    .next { frames := { f with kont } :: callers }
  -- spec FN-1 a3703e88ae515ee0
  -- spec FN-1 1d8e64f64176a68a
  /-- `break` leaves its loop for the statement after it. -/
  stepBreak (f : Frame) (callers : List Frame) (label : Option String) (k : List Kont) : Step :=
    match findLoop label k with
    | some (_, _, d, rest) =>
      .next { frames := { f with env := f.env.restore d, kont := rest } :: callers }
    | none => .halt (.illFormed "a break outside its loop")
  -- spec FN-1 05ca73acf9655ecd
  /-- `continue` enters its loop's body again. -/
  stepContinue (f : Frame) (callers : List Frame) (label : Option String) (k : List Kont) :
      Step :=
    match findLoop label k with
    | some (l, body, d, rest) =>
      let kont : List Kont := .stmts body :: .loop l body d :: rest
      .next { frames := { f with env := f.env.restore d, kont } :: callers }
    | none => .halt (.illFormed "a continue outside its loop")

-- spec FN-1 825e55475a4a1460
-- spec FN-1 c6cd91bdc57c73cc
-- spec FN-1 1072efe921994617
-- spec FN-1 726d0abfaca9ee67
-- spec FN-1 f7d2803bf009af80
/-- One step of the machine. The end of a block goes on to what follows it;
a function body has no normal exit, so reaching its end is ill formed. -/
def step (p : Program) (c : Config) : Step :=
  match c.frames with
  | [] => .halt (.illFormed "no frame")
  | f :: callers =>
    match f.kont with
    | [] => .halt (.illFormed "a function body falls through")
    | .stmts [] :: k => .next { frames := { f with kont := k } :: callers }
    | .scope d :: k => .next { frames := { f with env := f.env.restore d, kont := k } :: callers }
    | .loop l body d :: k =>
      let kont : List Kont := .stmts body :: .loop l body d :: k
      .next { frames := { f with env := f.env.restore d, kont } :: callers }
    | .stmts (s :: ss) :: k => stepStmt p f callers s (.stmts ss :: k)

/-- At most `n` steps from a configuration. -/
def runFrom (p : Program) : Nat → Config → Outcome
  | 0, _ => .outOfSteps
  | n + 1, c =>
    match step p c with
    | .next c' => runFrom p n c'
    | .done v => .done v
    | .halt (.stuck s) => .stuck s
    | .halt (.illFormed w) => .illFormed w

-- spec PROG-3 3f894a05d8320644
/-- Run the program's entry `main`, which takes no argument, for at most
`steps` steps. -/
def Program.run (p : Program) (steps : Nat) : Outcome :=
  match p.fn? "main" with
  | none => .illFormed "no main"
  | some f =>
    match enter p f [] .exit [] with
    | .next c => runFrom p steps c
    | .done v => .done v
    | .halt (.stuck s) => .stuck s
    | .halt (.illFormed w) => .illFormed w

end Whitefoot
