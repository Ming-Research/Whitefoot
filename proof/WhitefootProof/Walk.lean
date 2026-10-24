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

-- spec ENT-4 6aa92afa17b84194
-- spec ENT-4 0dfae6acbee66db5
-- spec ENT-4 80ca3ec3ee789a59
-- spec ENT-4 60ccf77d200c3d96
-- spec ENT-4 621cc554dfc7ac5c
-- spec ENT-4 94ce336fd1861236
-- spec ENT-4 0ac135032a310cf9
-- spec ENT-4 de2c478666562e29
-- spec ENT-4 f87deb5c1c735e66
-- spec ENT-6 bd2cd10961e1501e
-- spec ENT-6 833026065ac46409
-- spec ENT-6 9edf1f00452ed09a
-- spec ENT-6 b1c67f928185c2d6
-- spec ENT-6 5e52be4760172d16
-- spec ENT-6 d1018e7aa7f6041f
-- spec ENT-6 764d2c6b07f9005e
/-- The Signed Goal query [ENT-4, ENT-6]: whether a goal of sign `s` is
derivable, by contradiction, an established fact in the origin-transport
view, a literal's implicit proof, the Boolean introduction table over its
children, a live definition, a comparison's L0 projection or, for an
ordering, its affine normalization, or an operation-domain predicate's
normalization. -/
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
      (match va.image, vb.image, op' with
        | _, _, .eq | _, _, .ne => false
        | some fa, some fb, _ => (ineqs op' fa fb).all (auto (view env g st))
        | _, _, _ => false)
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

/-- The size of a Goal expression, which bounds a query's fuel. -/
def GExpr.size : GExpr → Nat
  | .index _ off => off.size + 1
  | .infix _ a b | .op2 _ a b | .aff _ a b | .cmp _ a b => a.size + b.size + 1
  | .op1 _ a => a.size + 1
  | _ => 1

-- spec OP-2 072c584ded9ba987
-- spec ENT-3 bb2754f5f1675e6e
-- spec ENT-3 a7cd776116ebf982
-- spec ENT-3 f83d840f83f9d6e3
-- spec ENT-3 b42c8a44d89b1f8c
-- spec ENT-3 2084f3e69dce62fb
-- spec ENT-3 327189f55b5c250e
-- spec ENT-3 2b0404366028f3e5
-- spec ENT-3 cebaa68057b35aef
-- spec ENT-3 bd21e45a863688b8
-- spec ENT-3 54e7ade7ff07a77c
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

-- spec OP-2 b749e9bd9126a614
-- spec ENT-3.S1 010319bcc7e32f4f
-- spec ENT-3.S1 471a1362d45db883
/-- Establish a condition's then-entry or else-entry facts [ENT-3.S1]. -/
def branchState (env : Env) (st : State) (o : Origins) (s : Bool) : State :=
  let st := o.goals.foldl (fun st g => establishGoal env st s g (g.size + 1)) st
  match o.cmp with
  | some (op, a, b) =>
    let (bs, ds) := relFacts (if s then op else op.negate) a b
    st.add env bs ds
  | none => st

-- spec ENT-3 a03af111b84dab86
-- spec ENT-3.S5 72b4d00bbf489a99
-- spec ENT-3.S7 7d57baede0c7cae7
-- spec ENT-6 55c0b60bde3ccdce
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
        let st := { st with quotients := st.quotients ++ [({ q, dividend := fa, divisor := fd } : Division)] }
        match lit with
        | some k => if k > 0 then { st with premises := st.premises ++ [Ineq.le (img.scale k) fa] } else st
        | none => st
      | _, _ => st
    match r.product with
    | some fa => { st with premises := st.premises ++ [Ineq.le img fa] }
    | none => st

-- spec FN-9 dc8c78b15eb05053
-- spec FN-9 c41a9690f56f2e8f
-- spec ENT-2 0f07cfa9989a6ee8
-- spec ENT-3.S5 96db5232821f2958
-- spec ENT-3.S5 031da783167c8ba8
-- spec ENT-3.S5 34b6becc68fcb93b
-- spec ENT-3.S5 f55912d947d2e812
-- spec ENT-3.S7 c886ce025709bb26
-- spec ENT-6 aaae558a8a1cf6f6
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

-- spec FN-1 e9ced632e8466aa3
-- spec FN-9 474d75677adbf145
-- spec FN-9 d0581c5c92c83713
-- spec FN-9 3b3f5941cd3f3ac1
-- spec FN-9 72dc65383b2bc3ea
-- spec FN-9 51a8f7fdc09ba5c6
-- spec ENT-2 261757f83e159d4f
-- spec ENT-2 e7b64879f58fcb34
-- spec FN-9 cda2ca4d680967a7
/-- A return proves each postcondition [FN-9]: the result is the returned
value and each parameter the normalized relation carries is its entry value,
which needs its entry image to be stable. -/
def checkReturn (env : Env) (st : State) (ret : Val) : M Unit := do
  let gen ← get
  if !env.fn.ensures.isEmpty && ret.term.isNone then
    fail "FN-9" "a return whose value is no term or constant, which a postcondition cannot read"
  for r in env.fn.ensures do
    for n in relNames r do
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

/-- The state at a cut point that the edges of its canonical frontier reach:
one edge passes its state on; two or more are joined [ENT-5]. -/
def merge (env : Env) : List State → M (Option State)
  | [] => return none
  | [s] => return some s
  | ss => some <$> joinStates env ss

/-- A loop's head state [ENT-5, INV-1]: the entering state with each binding
of `killed` that is in scope written, each such integer binding given a fresh
atom over its type as its image, and then each invariant established. -/
def loopHead (env : Env) (st : State) (killed : List String)
    (invs : List (String × Relation)) : M State := do
  let mut head := st
  for x in killed do
    if let some v := env.var? x then
      head := head.write v.id
      if let .int t := v.ty then
        head := head.setImage v.id (← typeAtom t)
  return invs.foldl (fun s (_, r) => establishRel env s r) head

mutual
-- spec ENT-2 e1dbb8e4dc9adc24
-- spec ENT-5 a8904d6cd3a8ddaa
-- spec ENT-5 0213f0e9357bf884
-- spec ENT-5 b149846a589e272e
-- spec ENT-5 64537e36dca5782f
-- spec ENT-5 16db13ea9e279f91
-- spec ENT-5 1533734f99b5fdd1
-- spec ENT-5 43c2b72cd3c9443e
-- spec ENT-5 c8724b281fd43439
-- spec ENT-5 81d1f6aefe007fd1
-- spec ENT-5 5149b193b27e6bc9
-- spec ENT-5 f4545a01b74455a1
-- spec ENT-6 dbea7ca9eab89029
/-- Walk a statement list, starting at index `i` of the block at `blk`, from its
entering state. The normal edges leaving a statement reach the next
statement's entry, a cut point where they are joined; at the end of the list
they pass on unjoined, so a merge-only continuation is replaced by its
incoming edges [ENT-5]. Each statement's entering state is recorded, with the
bindings in scope and the atoms minted so far, at its position. -/
def walkList (env : Env) (st : State) (blk : Pos) (i : Nat) (ss : List Stmt) : M Out :=
  match ss with
  | [] => return { normal := [st] }
  | s :: rest => do
    let pos := blk ++ [i]
    let snap : Snapshot := { vars := env.vars, state := st, atoms := (← get).atoms }
    modify fun g => { g with points := g.points ++ [(pos, snap)] }
    let (env', o) ← walkStmt env st pos s
    match rest with
    | [] => return o
    | _ :: _ =>
      match ← merge env' o.normal with
      | none => return o
      | some st' => return o.seq (← walkList env' st' blk (i + 1) rest)
termination_by structural ss

-- spec OP-5 a181a70ccdbfe485
-- spec OP-5 7931a4ac9fcc2e9d
-- spec OP-5 2244a8ebe6e35c43
-- spec OP-5 0c0467fe3e3df709
-- spec OP-5 015a94cd24a9fbb2
-- spec OP-5 04a032d76aa808a8
-- spec OP-5 90bffb29a4559d1e
-- spec OP-5 be93b45a14a33e13
-- spec OP-5 676962aa88f7046c
-- spec FN-9 2b19704cf9266e0b
-- spec INV-1 8f2505939b5b65c6
-- spec INV-1 2e7be49246ee77bc
-- spec INV-1 239600b554c973c6
-- spec INV-1 5c88e6741a3a75e0
-- spec INV-1 112243f049e8edca
-- spec INV-1 e1fbc7739e56b48b
-- spec INV-1 5e2bed457d30db00
-- spec INV-1 ca59951146ed5c65
-- spec INV-1 ffc9dede894a2384
-- spec INV-1 91dec2dea8743224
-- spec INV-1 75cdc1ebaebea14c
-- spec INV-1 c9e44b61a78909ab
-- spec INV-1 8f87af25aae3dcd7
-- spec INV-1 6e71e6ec893922ee
-- spec INV-1 f7ecc12e31eff171
-- spec INV-1 c0986f4943a5aba6
-- spec INV-1 1324494307fcd353
-- spec INV-1 53242470c05acf5a
-- spec INV-1 5c08bf4b34bbc827
-- spec INV-1 363b474ebfd3b00f
-- spec INV-1 6904e63e58391d5e
-- spec INV-1 c0e04663ae4184d6
-- spec INV-1 88bc417515d7176f
-- spec INV-1 cff164cd3ac022fc
-- spec ENT-3 51beaca4e6f7738a
-- spec ENT-3 599970b0de9dee7a
-- spec ENT-5 3de9430bb0118835
-- spec ENT-5 cfc1c375dfcd207f
-- spec ENT-5 875741ec34db74d3
-- spec ENT-5 4ed063b01d91ac45
-- spec ENT-5 a6d59142f82a5e7a
-- spec ENT-5 e94c93727748d972
-- spec ENT-5 905ad2747e7e27aa
-- spec ENT-5 6d262e4f998ae284
-- spec ENT-5 60ca7bf827088fd7
-- spec ENT-5 796454dd702c5619
-- spec ENT-5 d423c42032982b2e
-- spec ENT-5 c18974d7503b2750
-- spec ENT-5 1ae84c8576a083b0
-- spec ENT-5 80c570880c3c70a2
-- spec ENT-5 012b5b11b1b67a36
-- spec ENT-6 bb27ebd455ffc998
-- spec SET-1 fb700bb42c90f36a
/-- Walk one statement, at `pos`: returns the environment after it and its
flow. -/
def walkStmt (env : Env) (st : State) (pos : Pos) (s : Stmt) : M (Env × Out) :=
  match s with
  | .let_ x e => do
    let r ← evalRhs env st e
    let id ← freshId
    let v : Var := { name := x, id, ty := r.val.ty }
    let st ← bindLet env st v r
    return ({ env with vars := v :: env.vars }, { normal := [st] })
  | .set t e => do
    match t with
    | .var x => do
      let some v := env.var? x | fail "TYPE-6" s!"the name `{x}` is not in scope"
      let r ← evalRhs env st e
      return (env, { normal := [← setVar env st v r] })
    | .index b off => do
      let some v := env.var? b | fail "TYPE-6" s!"the name `{b}` is not in scope"
      -- The right-hand side first; the target's subscript is then judged on
      -- its normal continuation, before the commit writes the binding [SET-1].
      let r ← evalRhs env st e
      let st ← match r.call with
        | some (f, args) => publish env st f args none none id
        | none => pure st
      let _ ← evalAtom env st (.index b off)
      return (env, { normal := [st.write v.id] })
  | .call e => do
    let r ← evalRhs env st e
    let st ← match r.call with
      | some (f, args) => publish env st f args none none id
      | none => pure st
    return (env, { normal := [st] })
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
      -- A query path follows each definition link at most once, since a link's
      -- right-hand side reads only earlier bindings; every other step descends
      -- the goal's complete expansion. That bounds the query's depth.
      let fuel (g : GExpr) : Nat := (st.expand g).size + st.links.length + 1
      let holds (sgn : Bool) : Bool :=
        o.goals.all (fun g => signed env gen st (fuel g) sgn g) &&
        (o.cmp.all fun (op, a, b) => derivable st.l0 (relFacts (if sgn then op else op.negate) a b))
      if holds true then fail "OP-5" "the condition is always true"
      if holds false then fail "OP-5" "the condition is always false"
    let ot ← walkList env (branchState env st o true) (pos ++ [0]) 0 thn
    let oe ← walkList env (branchState env st o false) (pos ++ [1]) 0 els
    let normal := (ot.normal ++ oe.normal).map (·.exit env.vars)
    return (env, { normal, breaks := ot.breaks ++ oe.breaks, conts := ot.conts ++ oe.conts })
  | .loop label invs body => do
    let gen ← get
    for (n, r) in invs do
      if !relHolds env gen st r then fail "INV-1" s!"the invariant `{n}` is not proved on entry"
    let id ← freshId
    let killed := (killList { normal := true, loops := [(label, false, true)] } body).eraseDups
    let head ← loopHead env st killed invs
    let lc : LoopCtx := { id, label, headers := invs.map (·.2), outside := env.vars }
    let o ← walkList { env with loops := lc :: env.loops } head (pos ++ [0]) 0 body
    let inputs := (o.normal ++ (o.conts.filter (·.1 == id)).map (·.2)).map (·.exit env.vars)
    let gen ← get
    for s in inputs do
      if !s.contra then
        for (n, r) in invs do
          if !relHolds env gen s r then fail "INV-1" s!"the invariant `{n}` is not preserved"
    let exits := (o.breaks.filter (·.1 == id)).map (·.2.exit env.vars)
    let normal := (← merge env exits).toList
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
    return (env, { normal := [establishRel env st r] })
termination_by structural s
end

/-- Whether a side of a requirement is one operand of shape (a) [ENT-3]: one
name or one literal. -/
def strictSide : Option Lin → Bool
  | some (_, []) => true
  | some (0, [(_, 1)]) => true
  | _ => false

-- spec FN-8 15c207cb24ae7045
-- spec FN-8 2569a9dddaf72dba
-- spec ENT-3.S4 5c0ff9900b220551
-- spec ENT-3.S4 ad2672a8a82e59f9
-- spec ENT-3.S4 0a9b8a6032b2c48a
-- spec ENT-3.S4 7d0b38d368aaacb3
-- spec ENT-3.S4 b1ca56491321758f
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

-- spec FN-9 950a5f281ac705d5
-- spec FN-9 06637cddd1138eb4
-- spec FN-9 13f9ec57ad442ca0
-- spec FN-9 f224d0caa9568643
/-- A postcondition's admitted shape [FN-9]: each side is a constant
displacement of zero or one datum with coefficient one, at most one side also
carries one more parameter with coefficient one, and some side still carries
the result once its coefficients are summed, so a result that cancels does
not count. -/
def ensuresShape (f : FnDecl) (r : Relation) : Judge Unit := do
  if !(relNames r).contains f.result then
    rejectBy "FN-9" "a postcondition that names no result"
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

-- spec CALL-6 998f79c1ceb05a80
-- spec CALL-6 e6d89936daa3de1e
-- spec CALL-6 237835453fecc464
-- spec CALL-6 b64954de34a4f84c
-- spec CALL-6 d7cc58d215f42b2c
-- spec CALL-6 9a0b8784a5b721bd
/-- The consistency of a function's published relations [CALL-6]: the
postconditions over their symbolic datums, each one term, must not be
contradictory under ENT-4. -/
def consistent (p : Program) (f : FnDecl) : Judge Unit := do
  let ty (x : String) : Option IntTy :=
    if x == f.result then (match Static.normTy p f.resultTy with | .int t => some t | _ => none)
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

/-- A function's entry environment and state [ENT-3.S4, FN-8, FN-9]: each
parameter bound to a fresh identity and, for an integer parameter, given a
fresh atom over its type as its entry image; every parameter's entry image
stable; then each requirement established. -/
def entryState (p : Program) (f : FnDecl) : M (Env × State) := do
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
  return (env, st)

mutual
/-- Whether a statement list holds a `return`: a postcondition's selected
returns [FN-9]. -/
def returns (ss : List Stmt) : Bool :=
  match ss with
  | [] => false
  | s :: rest => returnsStmt s || returns rest
termination_by structural ss

def returnsStmt (s : Stmt) : Bool :=
  match s with
  | .ret _ => true
  | .if_ _ thn els _ => returns thn || returns els
  | .loop _ _ body => returns body
  | _ => false
termination_by structural s
end

-- spec FN-8 bb2990933c08779c
-- spec FN-8 1430be0afedd31ac
-- spec FN-8 a79bf222767a72f3
-- spec FN-9 b0a9b84c308b546c
-- spec FN-9 dce09a5a2521286a
-- spec FN-9 c0af4afad7d89490
-- spec ENT-2 14d61dc0f4018056
-- spec ENT-5 47886ab359e36575
-- spec ENT-6 53fe88cf10b86926
/-- A function's fact judgments: its postconditions' shapes and consistency,
then the walk of its body from the entry state of its requirements. -/
def checkFn (p : Program) (f : FnDecl) : M Unit := do
  for r in f.ensures do StateT.lift (ensuresShape f r)
  StateT.lift (consistent p f)
  let (env, st) ← entryState p f
  if !f.ensures.isEmpty && !returns f.body && !st.contra then
    fail "FN-9" s!"`{f.name}` has postconditions and no return to prove them at"
  discard <| walkList env st [] 0 f.body

end Checker

/-- The fragment's checker: the judgments without facts, then every
function's fact judgments. -/
def Program.check (p : Program) : Judge Unit := do
  Static.checkProgram p
  for f in p.fns do
    discard <| (Checker.checkFn p f).run {}

/-- Whether the checker accepts a program. -/
def Program.accepts (p : Program) : Bool :=
  match p.check with
  | .ok _ => true
  | .error _ => false

end Whitefoot
