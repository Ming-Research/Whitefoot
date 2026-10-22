import WhitefootProof.Checker

/-!
# The checker's forward walk

The statement walk of the fragment's checker [ENT-3]: each statement's
obligations are judged in its entering state, its sources establish their
facts, writes and scope exits kill [ENT-5], an `if` takes the join of the
edges reaching its continuation, and a loop is walked once from its head
state [ENT-5, INV-1]. `Program.accepts` runs the judgments without facts and
then this walk over every function.
-/

namespace Whitefoot

namespace Checker

open Fact Aff

/-- A Goal expression's value for a query: a literal's constant, a binding's
term and current image. -/
def gval (env : Env) (st : State) : GExpr → Val
  | .int t v => { term := some (.z, v), image := some (Form.const v), ty := .int t }
  | .var id =>
    (match env.varById? id with
    | some v => (match v.ty with
      | .int t => { term := some (.place id t, 0), image := st.image? id, ty := .int t }
      | ty => { ty })
    | none => { ty := .unit })
  | _ => { ty := .unit }

/-- Whether one alternative of an L0 normalization is refuted: the negation of
one of its facts is derivable. -/
def refutedAlt (s : L0) (alt : List Bound × List Diseq) : Bool :=
  alt.1.any (fun b => s.bound b.b b.a (-b.c - 1)) || alt.2.any (fun d => s.eq d.a d.b d.c)

/-- The Signed Goal query [ENT-4, ENT-6]: whether a goal of sign `s` is
derivable, by contradiction, an established fact in the origin-transport
view, a literal's implicit proof, the Boolean introduction table over its
children, a live definition, a comparison's L0 projection or its affine
normalization, or an operation-domain predicate's normalization. -/
def signed (env : Env) (g : Gen) (st : State) : Nat → Bool → GExpr → Bool
  | 0, _, _ => false
  | fuel + 1, s, e =>
    st.contra || st.present s e ||
    match e with
    | .bool b => b == s
    | .op1 .bnot a => signed env g st fuel (!s) a
    | .op2 .band a b =>
      if s then signed env g st fuel s a && signed env g st fuel s b
      else signed env g st fuel s a || signed env g st fuel s b
    | .op2 .bor a b =>
      if s then signed env g st fuel s a || signed env g st fuel s b
      else signed env g st fuel s a && signed env g st fuel s b
    | .var id =>
      (match st.links.find? (·.1 == id) with
      | some (_, rhs) => signed env g st fuel s rhs
      | none => false)
    | .infix (.compare op) a b =>
      let op' := if s then op else op.negate
      let va := gval env st a
      let vb := gval env st b
      (match projection env e with
        | some (_, x, y) => derivable st.l0 (relFacts op' x y)
        | none => false) ||
      (match va.image, vb.image with
        | some fa, some fb =>
          let ts := ineqs op' fa fb
          !ts.isEmpty && ts.all (auto (view env g st))
        | _, _ => false)
    | .infix (.arith o .defined) a b => domain (.arith o .exact) a b s
    | .infix (.divide o .defined) a b => domain (.divide o .exact) a b s
    | _ => false
where
  domain (op : InfixOp) (a b : GExpr) (s : Bool) : Bool :=
    let va := gval env st a
    let vb := gval env st b
    match va.ty with
    | .int t =>
      let q := domainQuery .unit op t va vb
      if s then discharged env g st { q with goal := none }
      else q.l0.any fun cs => cs.any fun alts => alts.all (refutedAlt st.l0)
    | _ => false

/-- The size of a Goal expression, the fuel of a query over it. -/
def GExpr.size : GExpr → Nat
  | .index _ off => off.size + 1
  | .infix _ a b | .op2 _ a b | .aff _ a b | .cmp _ a b => a.size + b.size + 1
  | .op1 _ a => a.size + 1
  | _ => 1

/-- A condition's goal-origin set and comparison origin [ENT-3]: a comparison
of terms or constants has both; an operation-domain predicate, a Boolean row
or a literal has a goal origin; a bare `Bool` binding has its live
initializer's comparison origin or operation-domain goal. Each goal comes
with its complete origin expansion when that differs. -/
def origins (env : Env) (st : State) (c : Expr) (gexpr : Option GExpr) : Origins :=
  let withExpansion (g : GExpr) : List GExpr :=
    let e := st.expand g
    if e == g then [g] else [g, e]
  match c, gexpr with
  | .infix _ (.compare _) _, some g =>
    { goals := withExpansion g, cmp := projection env g }
  | .infix _ (.arith _ .defined) _, some g | .infix _ (.divide _ .defined) _, some g
  | .op _ _, some g | .bool _, some g => { goals := withExpansion g }
  | .atom (.var _), some (.var id) =>
    (match st.links.find? (·.1 == id) with
    | some (_, rhs) => (match rhs with
      | .infix (.compare _) _ _ => { cmp := projection env rhs }
      | .infix (.arith _ .defined) _ _ | .infix (.divide _ .defined) _ _
      | .op1 (.unary _ .defined) _ => { goals := withExpansion rhs }
      | _ => {})
    | none => {})
  | _, _ => {}

/-- Establish a condition's then-entry or else-entry facts [ENT-3.S1]. -/
def branchState (env : Env) (st : State) (o : Origins) (s : Bool) : State :=
  let st := o.goals.foldl (fun st g => establishGoal env st s g (g.size + 1)) st
  match o.cmp with
  | some (op, a, b) =>
    let (bs, ds) := relFacts (if s then op else op.negate) a b
    st.add env bs ds
  | none => st

/-- Bind a `let`'s value [ENT-3.S5, ENT-3.S7, ENT-6]: the copy or literal
equality, the operation row's facts, the image, the captured division, a
matched product's bound and the origin link. -/
def bindLet (env : Env) (st : State) (x : Var) (r : Rhs) : M State := do
  match x.ty with
  | .int t => do
    let tx := Term.place x.id t
    let env' := { env with vars := x :: env.vars }
    let copy : List Bound := match r.val.term with
      | some (p, k) => [⟨tx, p, k⟩, ⟨p, tx, -k⟩]
      | none => []
    let st := if r.call.isSome then st else st.add env' (copy ++ r.facts tx)
    let img ← match r.val.image with
      | some f => pure f
      | none => typeAtom t
    let st := (st.setImage x.id img)
    let st := capture st img r
    let st := match r.val.gexpr, r.call with
      | some g, none => { st with links := (x.id, g) :: st.links }
      | _, _ => st
    match r.call with
    | some (f, args) => publish env' st f args (some (tx, img)) none id
    | none => return st
  | _ =>
    match r.val.gexpr, r.call with
    | some g, none => return { st with links := (x.id, g) :: st.links }
    | _, some (f, args) => publish env st f args none none id
    | _, _ => return st
where
  /-- A division bound directly captures its images; a matched product
  inherits its dividend's bound [ENT-3.S7]. -/
  capture (st : State) (img : Form) (r : Rhs) : State :=
    let st := match r.quot, img.xs with
      | some (fa, fd, lit), [(q, 1)] =>
        let st := { st with quotients := st.quotients ++ [⟨q, fa, fd⟩] }
        match lit with
        | some k => if k > 0 then { st with premises := st.premises ++ [Ineq.le (img.scale k) fa] } else st
        | none => st
      | _, _ => st
    match r.product with
    | some fa => { st with premises := st.premises ++ [Ineq.le img fa] }
    | none => st

/-- A whole-binding `set` [SET-1, ENT-3.S5, ENT-5, ENT-6]: a fragment-typed
right-hand side is evaluated to a commit value that receives the same image
a `let` would, the target's facts die, and the target equals the commit value
and denotes the right-hand side's image. A call's result is published at the
target after the kill. -/
def setVar (env : Env) (st : State) (x : Var) (r : Rhs) : M State := do
  match x.ty, r.call with
  | .int t, some (f, args) => do
    let img := r.val.image.getD (Form.const 0)
    let st ← publish env st f args (some (.place x.id t, img)) (some x.id) (·.write x.id)
    return st.setImage x.id img
  | .int t, none => do
    let v := Term.commit (← freshId) t
    let copy : List Bound := match r.val.term with
      | some (p, k) => [⟨v, p, k⟩, ⟨p, v, -k⟩]
      | none => []
    let st := st.add env (copy ++ r.facts v)
    let st := st.write x.id
    let st := st.add env [⟨.place x.id t, v, 0⟩, ⟨v, .place x.id t, 0⟩]
    let img ← match r.val.image with
      | some f => pure f
      | none => typeAtom t
    return bindLet.capture (st.setImage x.id img) img r
  | _, some (f, args) => publish env st f args none none (·.write x.id)
  | _, none => return st.write x.id

/-- A return proves each postcondition [FN-9]: the result is the returned
value and each parameter its entry value, which needs its entry image to be
stable. -/
def checkReturn (env : Env) (st : State) (ret : Val) : M Unit := do
  let gen ← get
  for r in env.fn.ensures do
    let names := Affine.names r.lhs ++ Affine.names r.rhs
    for n in names do
      if n != env.fn.result then
        if let some (_, id) := env.params.find? (·.1 == n) then
          if !st.stable.contains id && !st.contra then
            fail "FN-9" s!"the entry value of `{n}` is not available at a return"
    let looks (x : String) : Option (Term × Int) :=
      if x == env.fn.result then ret.term else lookT env x
    let looki (x : String) : Option Form :=
      if x == env.fn.result then ret.image else lookI env st x
    if !(memberQueries r looks looki).all (discharged env gen st) then
      fail "FN-9" s!"a postcondition of `{env.fn.name}` is not proved at a return"

/-- Combine the flows of a statement and the rest of its list. -/
def Out.seq (o o' : Out) : Out :=
  { normal := o'.normal, breaks := o.breaks ++ o'.breaks, conts := o.conts ++ o'.conts }

/-- The continuation state of the edges reaching one point: one edge passes
its state on; two or more are joined [ENT-5]. -/
def merge (env : Env) : List State → M (Option State)
  | [] => return none
  | [s] => return some s
  | ss => some <$> joinStates env ss

mutual
/-- Walk a statement list from its entering state. -/
def walkList (env : Env) (st : State) (ss : List Stmt) : M Out :=
  match ss with
  | [] => return { normal := some st }
  | s :: rest => do
    let (env', o) ← walkStmt env st s
    match o.normal with
    | none => return o
    | some st' => return o.seq (← walkList env' st' rest)
termination_by structural ss

/-- Walk one statement: returns the environment after it and its flow. -/
def walkStmt (env : Env) (st : State) (s : Stmt) : M (Env × Out) :=
  match s with
  | .let_ x e => do
    let r ← evalRhs env st e
    let id ← freshId
    let v : Var := { name := x, id, ty := r.val.ty }
    let st ← bindLet env st v r
    return ({ env with vars := v :: env.vars }, { normal := some st })
  | .set t e => do
    match t with
    | .var x => do
      let some v := env.var? x | fail "TYPE-6" s!"the name `{x}` is not in scope"
      let r ← evalRhs env st e
      return (env, { normal := some (← setVar env st v r) })
    | .index b off => do
      let some v := env.var? b | fail "TYPE-6" s!"the name `{b}` is not in scope"
      let _ ← evalAtom env st (.index b off)
      let r ← evalRhs env st e
      let st ← match r.call with
        | some (f, args) => publish env st f args none none (·.write v.id)
        | none => pure (st.write v.id)
      return (env, { normal := some st })
  | .call e => do
    let r ← evalRhs env st e
    let st ← match r.call with
      | some (f, args) => publish env st f args none none id
      | none => pure st
    return (env, { normal := some st })
  | .ret e => do
    let r ← evalRhs env st e
    let st ← match r.call with
      | some (f, args) => publish env st f args none none id
      | none => pure st
    checkReturn env st r.val
    return (env, {})
  | .if_ c thn els _ => do
    let r ← evalRhs env st c
    let st ← match r.call with
      | some (f, args) => publish env st f args none none id
      | none => pure st
    let o : Origins := if r.call.isSome then {} else origins env st c r.val.gexpr
    let gen ← get
    if !st.contra && (!o.goals.isEmpty || o.cmp.isSome) then
      let holds (sgn : Bool) : Bool :=
        o.goals.all (fun g => signed env gen st (g.size + 1) sgn g) &&
        (o.cmp.all fun (op, a, b) => derivable st.l0 (relFacts (if sgn then op else op.negate) a b))
      if holds true then fail "OP-5" "the condition is always true"
      if holds false then fail "OP-5" "the condition is always false"
    let ot ← walkList env (branchState env st o true) thn
    let oe ← walkList env (branchState env st o false) els
    let exits := (ot.normal.toList ++ oe.normal.toList).map (·.exit env.vars)
    let normal ← merge env exits
    return (env, { normal, breaks := ot.breaks ++ oe.breaks, conts := ot.conts ++ oe.conts })
  | .loop label invs body => do
    let gen ← get
    for (n, r) in invs do
      if !relHolds env gen st r then fail "INV-1" s!"the invariant `{n}` is not proved on entry"
    let id ← freshId
    let killed := (killList { normal := true, loops := [(label, false, true)] } body).eraseDups
    let mut head := st
    for x in killed do
      if let some v := env.var? x then
        head := head.write v.id
        if let .int t := v.ty then
          head := head.setImage v.id (← typeAtom t)
    head := invs.foldl (fun s (_, r) => establishRel env s r) head
    let lc : LoopCtx := { id, label, headers := invs.map (·.2), outside := env.vars }
    let o ← walkList { env with loops := lc :: env.loops } head body
    let inputs := (o.normal.toList ++ (o.conts.filter (·.1 == id)).map (·.2)).map (·.exit env.vars)
    let gen ← get
    for s in inputs do
      if !s.contra then
        for (n, r) in invs do
          if !relHolds env gen s r then fail "INV-1" s!"the invariant `{n}` is not preserved"
    let exits := (o.breaks.filter (·.1 == id)).map (·.2.exit env.vars)
    let normal ← merge env exits
    return (env, { normal, breaks := o.breaks.filter (·.1 != id), conts := o.conts.filter (·.1 != id) })
  | .brk label => do
    let some l := target env.loops label | fail "TYPE-6" "a break outside its loop"
    return (env, { breaks := [(l.id, st)] })
  | .cont label => do
    let some l := target env.loops label | fail "TYPE-6" "a continue outside its loop"
    return (env, { conts := [(l.id, st)] })
  | .invariant n r => do
    let gen ← get
    if !relHolds env gen st r then fail "INV-1" s!"the invariant `{n}` is not proved"
    return (env, { normal := some (establishRel env st r) })
termination_by structural s
end

/-- Whether a side of a requirement is one operand of shape (a) [ENT-3]: one
name or one literal. -/
def strictSide : Option Lin → Bool
  | some (_, []) => true
  | some (0, [(_, 1)]) => true
  | _ => false

/-- Establish a requirement at body entry [ENT-3.S4, FN-8]: its goal; its
comparison when each side is one name or literal; otherwise, for an ordering,
its normalized affine inequality over the entry images. -/
def establishRequire (env : Env) (st : State) (r : Relation) : State :=
  let st := match relG (fun x => (env.var? x).map (GExpr.var ·.id)) r with
    | some g => { st with goals := st.goals ++ [(true, g)] }
    | none => st
  if strictSide (lin r.lhs) && strictSide (lin r.rhs) then
    match linTerm (lookT env) ((lin r.lhs).getD (0, [])), linTerm (lookT env) ((lin r.rhs).getD (0, [])) with
    | some a, some b => let (bs, ds) := relFacts r.op a b; st.add env bs ds
    | _, _ => st
  else
    match (lin r.lhs).bind (linImage (lookI env st)), (lin r.rhs).bind (linImage (lookI env st)) with
    | some fa, some fb =>
      if r.op == .eq || r.op == .ne then st
      else { st with premises := st.premises ++ ineqs r.op fa fb }
    | _, _ => st

/-- A postcondition's admitted shape [FN-9]: each side is a constant
displacement of zero or one datum with coefficient one, and at most one side
also carries one more parameter with coefficient one. -/
def ensuresShape (f : FnDecl) (r : Relation) : Judge Unit := do
  let side (a : Affine) : Judge Nat :=
    match lin a with
    | some l => (match l.clean.2 with
      | [] => .ok 0
      | [(_, 1)] => .ok 1
      | [(_, 1), (_, 1)] => .ok 2
      | _ => rejectBy "FN-9" "a postcondition side outside the admitted shapes")
    | none => rejectBy "FN-9" "a postcondition side outside the admitted shapes"
  let a ← side r.lhs
  let b ← side r.rhs
  if a == 2 && b == 2 then rejectBy "FN-9" "both sides of a postcondition carry an additional parameter"

/-- The consistency of a function's published relations [CALL-6]: the
postconditions over their symbolic datums, each one term, must not be
contradictory under ENT-4. -/
def consistent (p : Program) (f : FnDecl) : Judge Unit := do
  let ty (x : String) : Option IntTy :=
    if x == f.result then match Static.normTy p f.resultTy with | .int t => some t | _ => none
    else (f.params.find? (·.name == x)).bind fun prm => match Static.normTy p prm.ty with
      | .int t => some t
      | _ => none
  let idx (x : String) : Nat :=
    if x == f.result then 0 else ((f.params.findIdx? (·.name == x)).getD 0) + 1
  let look (x : String) : Option ((Term × Int) × Form) := do
    return ((.datum (idx x) (← ty x), 0), Form.const 0)
  let facts := f.ensures.filterMap fun r => publish.pubRel r look
  let s := close [] (facts.flatMap (·.1)) (facts.flatMap (·.2.1))
  if s.contra then rejectBy "CALL-6" s!"the postconditions of `{f.name}` are contradictory"

/-- A function's fact judgments: its postconditions' shapes and consistency,
then the walk of its body from the entry state of its requirements. -/
def checkFn (p : Program) (f : FnDecl) : M Unit := do
  for r in f.ensures do StateT.lift (ensuresShape f r)
  StateT.lift (consistent p f)
  let mut vars : List Var := []
  let mut st : State := {}
  for prm in f.params do
    let id ← freshId
    let ty := Static.normTy p prm.ty
    vars := { name := prm.name, id, ty } :: vars
    if let .int t := ty then
      st := st.setImage id (← typeAtom t)
  let env : Env := { prog := p, fn := f, vars, params := vars.map (fun v => (v.name, v.id)), loops := [] }
  st := { st with stable := vars.map (·.id), l0 := close env.live [] [] }
  for r in f.requires do
    st := establishRequire env st r
  let _ ← walkList env st f.body

end Checker

/-- The fragment's checker: the judgments without facts, then every
function's fact judgments. -/
def Program.check (p : Program) : Judge Unit := do
  Static.checkProgram p
  for f in p.fns do
    let _ ← (Checker.checkFn p f).run {}

/-- Whether the checker accepts a program. -/
def Program.accepts (p : Program) : Bool :=
  match p.check with
  | .ok _ => true
  | .error _ => false

end Whitefoot
