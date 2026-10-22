import WhitefootProof.Static
import WhitefootProof.Facts
import WhitefootProof.Affine

/-!
# The fragment's checker

`Program.accepts` runs the judgments without facts (`Static.lean`) and then,
for each function, the fact judgments of section 15: one forward walk of the
body [ENT-3] that carries the fact state, establishes each source's facts at
its stated point, kills facts on writes and scope exits [ENT-5], joins states
at merges and forms each loop's head state, and submits every proof
obligation to the one numeric disposition [MSR-4]: the domain of each exact
integer operation [OP-2], each subscript's bound [OP-4], each requirement at a
call [FN-8], each postcondition at a return [FN-9], each invariant [INV-1],
and each condition's redundancy [OP-5].

The state has four parts: the closed L0 relations (`Facts.lean`), the signed
opaque goals, the affine images and premises (`Affine.lean`), and the
bookkeeping of origin links, captured divisions and parameter entry-image
stability. The walk visits every statement once; a loop body is walked once
from its head state.
-/

namespace Whitefoot

namespace Affine

/-- The names an affine expression reads. -/
def names : Whitefoot.Affine → List String
  | .atom (.var x) => [x]
  | .atom _ => []
  | .add a b | .sub a b | .mul a b => names a ++ names b

end Affine

namespace Checker

open Fact Aff

/-- A Goal expression [ENT-2]: a typed tree over binding datums and literals.
A contract clause's `+`, `-` and `*` are the mathematical operations
`aff` [FN-8, MSR-5]. -/
inductive GExpr where
  | int (t : IntTy) (v : Int)
  | bool (b : Bool)
  | unit
  | var (id : Nat)
  | index (base : Nat) (off : GExpr)
  | infix (op : InfixOp) (a b : GExpr)
  | op1 (o : TableOp) (a : GExpr)
  | op2 (o : TableOp) (a b : GExpr)
  | aff (op : ArithOp) (a b : GExpr)
  | cmp (op : CmpOp) (a b : GExpr)
  deriving DecidableEq, Repr, Inhabited

namespace GExpr

/-- Whether the expression reads binding `x`: its support [ENT-5]. -/
def reads (x : Nat) : GExpr → Bool
  | .var id => id == x
  | .index b off => b == x || off.reads x
  | .infix _ a b | .op2 _ a b | .aff _ a b | .cmp _ a b => a.reads x || b.reads x
  | .op1 _ a => a.reads x
  | _ => false

/-- Replace every binding datum by `f`'s expression for it, if any. -/
def subst (f : Nat → Option GExpr) : GExpr → GExpr
  | .var id => (f id).getD (.var id)
  | .index b off => .index b (off.subst f)
  | .infix o a b => .infix o (a.subst f) (b.subst f)
  | .op1 o a => .op1 o (a.subst f)
  | .op2 o a b => .op2 o (a.subst f) (b.subst f)
  | .aff o a b => .aff o (a.subst f) (b.subst f)
  | .cmp o a b => .cmp o (a.subst f) (b.subst f)
  | g => g

end GExpr

/-- A captured division [ENT-3.S7]: its quotient's atom and the images of its
dividend and divisor. -/
structure Quot where
  q : Nat
  dividend : Form
  divisor : Form
  deriving Repr

/-- The fact state at one point. -/
structure State where
  l0 : L0 := {}
  /-- Established signed opaque goals [ENT-2]. -/
  goals : List (Bool × GExpr) := []
  /-- Each live integer binding's current image [ENT-6]. -/
  images : List (Nat × Form) := []
  /-- The immutable images of call datums [ENT-3.S13]. -/
  data : List (Term × Form) := []
  /-- The automatic affine premises, in establishment order [ENT-6]. -/
  premises : List Ineq := []
  /-- Parameters whose entry image is still stable [FN-9]. -/
  stable : List Nat := []
  /-- Live origin links: each binding with its defining right-hand side [ENT-3]. -/
  links : List (Nat × GExpr) := []
  quotients : List Quot := []
  deriving Repr, Inhabited

/-- A binding in scope. -/
structure Var where
  name : String
  id : Nat
  ty : Ty
  deriving Repr

/-- An enclosing loop: its identity, label, proved header relations and the
bindings in scope where it starts. -/
structure LoopCtx where
  id : Nat
  label : Option String
  headers : List Relation
  outside : List Var

/-- What the walk carries down a body. -/
structure Env where
  prog : Program
  fn : FnDecl
  vars : List Var
  params : List (String × Nat)
  loops : List LoopCtx

/-- The compiler-owned identities the walk mints: bindings, commit values,
call datums and atoms share one counter. Each atom has an interval; a join's
delta atoms are listed apart [ENT-6]. -/
structure Gen where
  next : Nat := 0
  atoms : List (Nat × Int × Int) := []
  deltas : List Nat := []

abbrev M := StateT Gen Judge

def fail {α : Type} (rule msg : String) : M α := StateT.lift (rejectBy rule msg)

def freshId : M Nat := do
  let g ← get
  set { g with next := g.next + 1 }
  return g.next

def freshAtom (lo hi : Int) : M Form := do
  let x ← freshId
  modify fun g => { g with atoms := (x, lo, hi) :: g.atoms }
  return Form.atom x

/-- A fresh atom over a type's complete interval [ENT-6]. -/
def typeAtom (t : IntTy) : M Form := freshAtom t.minVal t.maxVal

namespace Env

def var? (env : Env) (x : String) : Option Var := env.vars.find? (·.name == x)

def varById? (env : Env) (id : Nat) : Option Var := env.vars.find? (·.id == id)

/-- The term of an integer binding. -/
def term? (v : Var) : Option Term :=
  match v.ty with
  | .int t => some (.place v.id t)
  | _ => none

/-- The live terms, whose implicit facts every closure includes. -/
def live (env : Env) : List Term := env.vars.filterMap term?

end Env

namespace State

/-- Contradictory: L0 derives `t - t <= -1`, or both signs of one goal are
established [ENT-4]. -/
def contra (st : State) : Bool :=
  st.l0.contra || st.goals.any fun (s, g) => st.goals.contains (!s, g)

def add (st : State) (env : Env) (bs : List Bound) (ds : List Diseq := []) : State :=
  { st with l0 := st.l0.add env.live bs ds }

def image? (st : State) (id : Nat) : Option Form := (st.images.find? (·.1 == id)).map (·.2)

def setImage (st : State) (id : Nat) (f : Form) : State :=
  { st with images := (id, f) :: st.images.filter (·.1 != id) }

/-- A write of binding `x` [ENT-5]: every fact, goal and origin link whose
support holds x dies, and a parameter's entry image stops being stable. -/
def write (st : State) (x : Nat) : State :=
  { st with l0 := st.l0.kill (· == x),
            goals := st.goals.filter (fun (_, g) => !g.reads x),
            links := st.links.filter (fun (b, e) => b != x && !e.reads x),
            stable := st.stable.filter (· != x) }

/-- Leave the scope of every binding outside `keep` [ENT-5]: their facts,
goals, links and images go. -/
def exit (st : State) (keep : List Var) : State :=
  let dead (x : Nat) : Bool := !keep.any (·.id == x)
  { st with l0 := st.l0.kill dead,
            goals := st.goals.filter (fun (_, g) => !(dead_any g dead)),
            links := st.links.filter (fun (b, e) => !dead b && !(dead_any e dead)),
            images := st.images.filter (fun (b, _) => !dead b) }
where
  dead_any (g : GExpr) (dead : Nat → Bool) : Bool :=
    match g with
    | .var id => dead id
    | .index b off => dead b || dead_any off dead
    | .infix _ a b | .op2 _ a b | .aff _ a b | .cmp _ a b => dead_any a dead || dead_any b dead
    | .op1 _ a => dead_any a dead
    | _ => false

/-- An origin expansion [ENT-3]: every datum with a live link replaced by its
right-hand side, to a fixed point. -/
def expand (st : State) (g : GExpr) : GExpr :=
  (List.range (st.links.length + 1)).foldl (fun g _ =>
    g.subst fun id => (st.links.find? (·.1 == id)).map (·.2)) g

/-- Whether a signed goal is present in the origin-transport view [ENT-4.OT]:
an established goal of that sign whose complete expansion is the goal's own. -/
def present (st : State) (s : Bool) (g : GExpr) : Bool :=
  let eg := st.expand g
  st.goals.any fun (s', g') => s' == s && (g' == g || st.expand g' == eg)

end State

/-- A relation between two term operands, each a term plus a constant
offset: `(a + ka) op (b + kb)`, as L0 facts [ENT-2]. -/
def relFacts (op : CmpOp) (a : Term × Int) (b : Term × Int) : List Bound × List Diseq :=
  let (ta, ka) := a
  let (tb, kb) := b
  match op with
  | .le => ([⟨ta, tb, kb - ka⟩], [])
  | .lt => ([⟨ta, tb, kb - ka - 1⟩], [])
  | .ge => ([⟨tb, ta, ka - kb⟩], [])
  | .gt => ([⟨tb, ta, ka - kb - 1⟩], [])
  | .eq => ([⟨ta, tb, kb - ka⟩, ⟨tb, ta, ka - kb⟩], [])
  | .ne => ([], [⟨ta, tb, kb - ka⟩])

/-- The exact negation of a comparison [ENT-3.S1]. -/
def _root_.Whitefoot.CmpOp.negate : CmpOp → CmpOp
  | .le => .gt | .lt => .ge | .ge => .lt | .gt => .le | .eq => .ne | .ne => .eq

/-- Whether a relation's facts are derivable [ENT-4]. -/
def derivable (s : L0) (fs : List Bound × List Diseq) : Bool :=
  fs.1.all (fun b => s.bound b.a b.b b.c) && fs.2.all (fun d => s.diseq d.a d.b d.c)

/-- What an atom denotes for the facts: its L0 term with a constant offset (a
literal is Z plus its value), its current image, its Goal expression and its
type. -/
structure Val where
  term : Option (Term × Int) := none
  image : Option Form := none
  gexpr : Option GExpr := none
  ty : Ty

/-- What a numeric goal submits to the disposition [MSR-4]: its exact signed
identity, its finite L0 normalization, and its affine targets, each with the
L0 term of its right-hand side when it has one. -/
structure Query where
  goal : Option GExpr := none
  /-- Components, each proved by any one of its alternatives. -/
  l0 : Option (List (List (List Bound × List Diseq))) := none
  affine : Option (List (Ineq × Option (Form × Term))) := none
  /-- A multiplication of two nonconstant operands, for the interval product. -/
  product : Option (Val × Val × IntTy) := none

/-- The candidates of the closed L0-to-affine index [ENT-6]: Z, each call
datum with its captured image, each sum term with its operands' images, and
each live integer binding with its current image. -/
def candidates (env : Env) (st : State) : List (Term × Form) :=
  let sums := (st.l0.terms.filter fun t => match t with | .sum _ _ => true | _ => false)
  let imageOf (t : Term) : Option Form :=
    match t with
    | .place id _ => st.image? id
    | .datum _ _ => (st.data.find? (·.1 == t)).map (·.2)
    | _ => none
  let sumImages : List (Term × Form) := sums.filterMap fun t => match t with
    | .sum a b => do return (t, (← imageOf a).add (← imageOf b))
    | _ => none
  let bindings : List (Term × Form) := env.vars.reverse.filterMap fun v => do
    let t ← Env.term? v
    return (t, ← st.image? v.id)
  (Term.z, Form.const 0) :: st.data ++ sumImages ++ bindings

/-- The view an affine query reads. -/
def view (env : Env) (g : Gen) (st : State) : View :=
  { contra := st.contra,
    interval := fun x => ((g.atoms.find? (·.1 == x)).map (·.2)).getD (-(2 ^ 127), 2 ^ 127 - 1),
    premises := st.premises,
    images := l0Images st.l0 (candidates env st) }

/-- An operand's interval for an operation row [ENT-3.S7]: a literal's value,
an admitted term's closed bounds through Z, otherwise its type's interval. -/
def operandInterval (st : State) (t : IntTy) (v : Val) : Int × Int :=
  match v.term with
  | some (.z, k) => (k, k)
  | some (x, k) =>
    (match st.l0.interval x with
    | some (lo, hi) => (lo + k, hi + k)
    | none => (t.minVal, t.maxVal))
  | none => (t.minVal, t.maxVal)

/-- The least and greatest of a list of values. -/
def hull (vs : List Int) : Option (Int × Int) :=
  match vs with
  | [] => none
  | v :: rest => some (rest.foldl min v, rest.foldl max v)

/-- The interval product [MSR-4]: a multiplication of two nonconstant
operands is in its type when the four products of the operands' closed L0 or
type intervals all are. -/
def intervalProduct (st : State) (a b : Val) (t : IntTy) : Bool :=
  let (a0, a1) := operandInterval st t a
  let (b0, b1) := operandInterval st t b
  [a0 * b0, a0 * b1, a1 * b0, a1 * b1].all fun v => decide (t.minVal ≤ v ∧ v ≤ t.maxVal)

/-- The one numeric disposition [MSR-4], true when the goal is discharged:
contradiction; the exact signed fact; the closed L0 state; `AUTO` over the
affine targets, each also by the affine-left / L0-right bridge or, for an
upper bound on a captured quotient, the quotient bound. -/
def discharged (env : Env) (g : Gen) (st : State) (q : Query) : Bool :=
  st.contra ||
  (q.goal.any fun G => st.present true G) ||
  (q.l0.any fun cs => cs.all fun alts => alts.any (derivable st.l0)) ||
  (q.affine.any fun ts => ts.all fun (t, br) =>
    let v := view env g st
    auto v t || bridge v br || quotient v t) ||
  (q.product.any fun (a, b, t) => intervalProduct st a b t)
where
  bridge (v : View) : Option (Form × Term) → Bool
    | none => false
    | some (lhs, r) => (candidates env st).any fun (m, fm) =>
      m != r && match st.l0.tightest m r with
        | some c => auto v (Ineq.le (lhs.sub fm) (Form.const (-c)))
        | none => false
  quotient (v : View) (t : Ineq) : Bool :=
    match t.xs with
    | [(q, 1)] => st.quotients.any fun d =>
      d.q == q && auto v (Ineq.le (d.dividend.sub (d.divisor.scale (t.c + 1))) (Form.const (-1)))
    | _ => false

def mk1 (f : GExpr → GExpr) (a : Option GExpr) : Option GExpr := a.map f

def mk2 (f : GExpr → GExpr → GExpr) (a b : Option GExpr) : Option GExpr :=
  match a, b with
  | some x, some y => some (f x y)
  | _, _ => none

/-- A linear form over named operands: a constant and coefficients. -/
abbrev Lin := Int × List (String × Int)

def linAdd (a b : Lin) : Lin :=
  (a.1 + b.1, b.2.foldl (fun acc (x, k) =>
    match acc.find? (·.1 == x) with
    | some _ => acc.map fun (y, j) => if y == x then (y, j + k) else (y, j)
    | none => acc ++ [(x, k)]) a.2)

def linScale (k : Int) (a : Lin) : Lin := (k * a.1, a.2.map fun (x, j) => (x, k * j))

/-- An affine expression's linear form; `none` for a product with no constant
side. -/
def lin : Affine → Option Lin
  | .atom (.int _ v) => some (v, [])
  | .atom (.var x) => some (0, [(x, 1)])
  | .atom _ => none
  | .add a b => do return linAdd (← lin a) (← lin b)
  | .sub a b => do return linAdd (← lin a) (linScale (-1) (← lin b))
  | .mul a b => do
    let la ← lin a
    let lb ← lin b
    if la.2.all (·.2 == 0) then some (linScale la.1 lb)
    else if lb.2.all (·.2 == 0) then some (linScale lb.1 la) else none

/-- A linear form with its zero coefficients removed. -/
def Lin.clean (l : Lin) : Lin := (l.1, l.2.filter (·.2 != 0))

/-- A side whose form is one operand with coefficient one plus a constant, or
a constant alone, as a term operand. -/
def linTerm (look : String → Option (Term × Int)) (l : Lin) : Option (Term × Int) :=
  match l.clean.2 with
  | [] => some (.z, l.1)
  | [(x, 1)] => do let (t, k) ← look x; return (t, k + l.1)
  | _ => none

/-- A linear form's image. -/
def linImage (look : String → Option Form) (l : Lin) : Option Form :=
  l.2.foldlM (fun acc (x, k) => do return acc.add ((← look x).scale k)) (Form.const l.1)

/-- A relation's normalized affine components [INV-1, ENT-6]: `a <= b`, `a < b`
as `a + 1 <= b`, `>=` and `>` reversed, and `==` as both bounds; a
disequality has none. -/
def ineqs (op : CmpOp) (a b : Form) : List Ineq :=
  match op with
  | .le => [Ineq.le a b]
  | .lt => [Ineq.le (a.add (Form.const 1)) b]
  | .ge => [Ineq.le b a]
  | .gt => [Ineq.le (b.add (Form.const 1)) a]
  | .eq => [Ineq.le a b, Ineq.le b a]
  | .ne => []

/-- The right-hand L0 term of each component of a relation, for the bridge:
the side a component bounds from above. -/
def bridges (op : CmpOp) (a b : Form) (ta tb : Option (Term × Int)) :
    List (Ineq × Option (Form × Term)) :=
  let up (lhs : Form) (rhs : Option (Term × Int)) : Option (Form × Term) :=
    match rhs with
    | some (t, k) => if t == .z then none else some (lhs.sub (Form.const k), t)
    | none => none
  match op with
  | .le => [(Ineq.le a b, up a tb)]
  | .lt => [(Ineq.le (a.add (Form.const 1)) b, up (a.add (Form.const 1)) tb)]
  | .ge => [(Ineq.le b a, up b ta)]
  | .gt => [(Ineq.le (b.add (Form.const 1)) a, up (b.add (Form.const 1)) ta)]
  | .eq => [(Ineq.le a b, up a tb), (Ineq.le b a, up b ta)]
  | .ne => []

/-- A relation over named operands, as one query: its L0 facts when each side
is one term, its affine components over the operands' images. -/
def relQuery (goal : Option GExpr) (r : Relation) (looks : String → Option (Term × Int))
    (looki : String → Option Form) : Query :=
  match lin r.lhs, lin r.rhs with
  | some la, some lb =>
    let ta := linTerm looks la
    let tb := linTerm looks lb
    let l0 : Option (List (List (List Bound × List Diseq))) := do return [[relFacts r.op (← ta) (← tb)]]
    let affine : Option (List (Ineq × Option (Form × Term))) := do
      let fa ← linImage looki la
      let fb ← linImage looki lb
      return bridges r.op fa fb ta tb
    { goal, l0, affine }
  | _, _ => { goal }

/-- Each member of a relation as its own query: one per normalized bound, as
an invariant batch and a postcondition submit them [INV-1, FN-9]. -/
def memberQueries (r : Relation) (looks : String → Option (Term × Int))
    (looki : String → Option Form) : List Query :=
  match r.op with
  | .eq => [relQuery none { r with op := .le } looks looki, relQuery none { r with op := .ge } looks looki]
  | _ => [relQuery none r looks looki]

/-- The current term and image of each name in scope. -/
def lookT (env : Env) (x : String) : Option (Term × Int) := do
  let v ← env.var? x
  return (← Env.term? v, 0)

def lookI (env : Env) (st : State) (x : String) : Option Form := do
  let v ← env.var? x
  st.image? v.id

/-- An affine expression's Goal tree over the bindings it names. -/
def affG (look : String → Option GExpr) : Affine → Option GExpr
  | .atom (.int t v) => some (.int t v)
  | .atom (.var x) => look x
  | .atom _ => none
  | .add a b => do return .aff .add (← affG look a) (← affG look b)
  | .sub a b => do return .aff .sub (← affG look a) (← affG look b)
  | .mul a b => do return .aff .mul (← affG look a) (← affG look b)

def relG (look : String → Option GExpr) (r : Relation) : Option GExpr := do
  return .cmp r.op (← affG look r.lhs) (← affG look r.rhs)

/-- The facts a proved relation establishes [ENT-3.S16, ENT-6]: each bound
whose normalized source form is a difference bound of two terms is an L0 fact,
and each normalized component is an automatic affine premise over the
current images. -/
def establishRel (env : Env) (st : State) (r : Relation) : State :=
  let q := relQuery none r (lookT env) (lookI env st)
  let st := match q.l0 with
    | some [[(bs, ds)]] => if r.op == .ne then st else st.add env bs ds
    | _ => st
  match q.affine with
  | some ts => { st with premises := st.premises ++ (ts.map (·.1)).filter fun i => !st.premises.contains i }
  | none => st

/-- The join of affine images [ENT-6]: an image identical on every input is
kept; otherwise, when every input has one identical nonconstant form beside
its delta atoms, that form plus one fresh delta atom over the inputs'
constant intervals; otherwise a fresh atom over the type. -/
def joinImage (t : IntTy) (fs : List Form) : M Form := do
  match fs with
  | [] => typeAtom t
  | f :: rest =>
    if rest.all (· == f) then return f
    let g ← get
    let norm (f : Form) : Form × Int × Int :=
      f.xs.foldl (fun (base, lo, hi) (x, a) =>
        if g.deltas.contains x then
          let (l, h) := ((g.atoms.find? (·.1 == x)).map (·.2)).getD (0, 0)
          (base, lo + min (a * l) (a * h), hi + max (a * l) (a * h))
        else ({ base with xs := addTerm base.xs x a }, lo, hi)) ({}, f.c, f.c)
    let ns := fs.map norm
    match ns with
    | (b, _, _) :: _ =>
      if ns.all (·.1 == b) then
        let lo := ns.foldl (fun acc (_, l, _) => min acc l) (ns.headD ({}, 0, 0)).2.1
        let hi := ns.foldl (fun acc (_, _, h) => max acc h) (ns.headD ({}, 0, 0)).2.2
        let d ← freshAtom lo hi
        modify fun g => { g with deltas := (d.xs.headD (0, 0)).1 :: g.deltas }
        return b.add d
      else typeAtom t
    | [] => typeAtom t

/-- The members of `xs` every state of `rest` also holds. -/
def commonTo {α : Type} [BEq α] (rest : List State) (f : State → List α) (xs : List α) : List α :=
  xs.filter fun x => rest.all fun s => (f s).contains x

/-- Whether every component of a relation is proved in a state. -/
def relHolds (env : Env) (g : Gen) (st : State) (r : Relation) : Bool :=
  (memberQueries r (lookT env) (lookI env st)).all (discharged env g st)

/-- The join of the states reaching one merge [ENT-5, ENT-6]: the L0 join, the
signed goals, links and premises identical on every non-contradictory input,
stability where every input keeps it, each binding's joined image; then the
transport of every enclosing loop's header relation that each such input
proves. -/
def joinStates (env : Env) (inputs : List State) : M State := do
  let live := inputs.filter (!·.contra)
  match live with
  | [] => return { (inputs.headD {}) with l0 := L0.bottom }
  | first :: rest =>
    let all := first :: rest
    let mut images : List (Nat × Form) := []
    for v in env.vars do
      if let .int t := v.ty then
        let fs := all.filterMap (·.image? v.id)
        if fs.length == all.length then images := (v.id, ← joinImage t fs) :: images
    let joined : State := {
      l0 := Fact.join env.live (all.map (·.l0)),
      goals := commonTo rest (·.goals) first.goals,
      images,
      data := (all.flatMap (·.data)).eraseDups,
      premises := commonTo rest (·.premises) first.premises,
      stable := commonTo rest (·.stable) first.stable,
      links := commonTo rest (·.links) first.links,
      quotients := first.quotients ++ rest.flatMap (·.quotients) }
    let g ← get
    let headers := env.loops.reverse.flatMap (·.headers)
    let proved := headers.filter fun r => all.all fun s => relHolds env g s r
    return proved.foldl (establishRel env) joined

/-- How a statement list ends: the state at its normal exit, if reachable,
and the states on its `break` and `continue` edges by loop identity. -/
structure Out where
  normal : Option State := none
  breaks : List (Nat × State) := []
  conts : List (Nat × State) := []

/-- The loop a `break` or `continue` resolves to [TYPE-6]. -/
def target (loops : List LoopCtx) : Option String → Option LoopCtx
  | none => loops.head?
  | some l => loops.find? (·.label == some l)

/-- Reachability of a loop's next iteration head over the structural graph
[FN-1]: `normal` says whether the end of the current list reaches it, and
each enclosing loop inside the target, innermost first, whether its `break`
and its `continue` do. -/
structure Reach where
  normal : Bool
  loops : List (Option String × Bool × Bool)

def Reach.find (k : Reach) : Option String → Option (Bool × Bool)
  | none => (k.loops.head?).map (·.2)
  | some l => (k.loops.find? (·.1 == some l)).map (·.2)

mutual
/-- Whether running a statement list can reach the target loop's head. -/
def reachList (k : Reach) (ss : List Stmt) : Bool :=
  match ss with
  | [] => k.normal
  | s :: rest => reachStmt { k with normal := reachList k rest } s
termination_by structural ss

/-- Whether running one statement can reach the target loop's head. A nested
loop reaches it when its body does with the loop's own head reaching nothing,
the least fixed point of its body's equation. -/
def reachStmt (k : Reach) (s : Stmt) : Bool :=
  match s with
  | .ret _ => false
  | .brk l => ((k.find l).map (·.1)).getD false
  | .cont l => ((k.find l).map (·.2)).getD false
  | .if_ _ thn els _ => reachList k thn || reachList k els
  | .loop l _ body => reachList { normal := false, loops := (l, k.normal, false) :: k.loops } body
  | _ => k.normal
termination_by structural s
end

mutual
/-- The bindings a continuing kill event of the target loop writes [ENT-5]: a
`set` whose edge can reach a later iteration head without leaving the loop. -/
def killList (k : Reach) (ss : List Stmt) : List String :=
  match ss with
  | [] => []
  | s :: rest => killStmt { k with normal := reachList k rest } s ++ killList k rest
termination_by structural ss

def killStmt (k : Reach) (s : Stmt) : List String :=
  match s with
  | .set t _ =>
    if k.normal then [match t with | .var x => x | .index x _ => x] else []
  | .if_ _ thn els _ => killList k thn ++ killList k els
  | .loop l _ body =>
    let h := reachStmt k s
    killList { normal := h, loops := (l, k.normal, h) :: k.loops } body
  | _ => []
termination_by structural s
end

/-- A Bool condition's origins [ENT-3]: its goal-origin set and its comparison
origin, each as what the then-entry establishes. -/
structure Origins where
  goals : List GExpr := []
  cmp : Option (CmpOp × (Term × Int) × (Term × Int)) := none

/-- A goal's comparison projection: a comparison of two integer operands that
are terms or constants. -/
def projection (env : Env) : GExpr → Option (CmpOp × (Term × Int) × (Term × Int))
  | .infix (.compare op) a b => do return (op, ← operand a, ← operand b)
  | _ => none
where
  operand : GExpr → Option (Term × Int)
    | .int _ v => some (.z, v)
    | .var id => do return (← Env.term? (← env.varById? id), 0)
    | _ => none

/-- The signed decomposition of an established goal [ENT-3]: `+band` gives
both children, `-bor` both children negated, `bnot` its child with the other
sign; each member also establishes its comparison projection. -/
def establishGoal (env : Env) (st : State) (s : Bool) (g : GExpr) : Nat → State
  | 0 => st
  | fuel + 1 =>
    let st := if st.goals.contains (s, g) then st else { st with goals := st.goals ++ [(s, g)] }
    let st := match projection env g with
      | some (op, a, b) => let (bs, ds) := relFacts (if s then op else op.negate) a b; st.add env bs ds
      | none => st
    match g, s with
    | .op2 .band a b, true | .op2 .bor a b, false =>
      establishGoal env (establishGoal env st s a fuel) s b fuel
    | .op1 .bnot a, _ => establishGoal env st (!s) a fuel
    | _, _ => st

/-- The finite L0 normalization and affine targets of an integer domain
[MSR-4], for an exact operation on two operands: for add, subtract and
multiply with a constant operand, two bounds on the other operand; for
division and remainder, `d != Z` and, for a signed type, `n != min(T)` or
`d != -1`; then the result's range over the operands' images. -/
def domainQuery (goal : GExpr) (op : InfixOp) (t : IntTy) (a b : Val) : Query :=
  let m := t.minVal
  let mx := t.maxVal
  let ground (ok : Bool) : List (List Bound × List Diseq) :=
    if ok then [([], [])] else [([⟨.z, .z, -1⟩], [])]
  let range (r : Form) : List (Ineq × Option (Form × Term)) :=
    [(Ineq.le r (Form.const mx), none), (Ineq.le (Form.const m) r, none)]
  let scaled (x : Term) (c : Int) : List (List (List Bound × List Diseq)) :=
    if c > 0 then [[([⟨x, .z, mx.fdiv c⟩], [])], [([⟨.z, x, (-m).fdiv c⟩], [])]]
    else if c == 0 then [ground true]
    else [[([⟨x, .z, m.fdiv c⟩], [])], [([⟨.z, x, (-mx).fdiv c⟩], [])]]
  let l0 : Option (List (List (List Bound × List Diseq))) :=
    match op, a.term, b.term with
    | .arith .add _, some (ta, ka), some (tb, kb) =>
      if ta == .z && tb == .z then some [ground (decide (m ≤ ka + kb ∧ ka + kb ≤ mx))]
      else if tb == .z then some [[([⟨ta, .z, mx - kb - ka⟩], [])], [([⟨.z, ta, kb + ka - m⟩], [])]]
      else if ta == .z then some [[([⟨tb, .z, mx - ka - kb⟩], [])], [([⟨.z, tb, ka + kb - m⟩], [])]]
      else none
    | .arith .sub _, some (ta, ka), some (tb, kb) =>
      if ta == .z && tb == .z then some [ground (decide (m ≤ ka - kb ∧ ka - kb ≤ mx))]
      else if tb == .z then some [[([⟨ta, .z, mx + kb - ka⟩], [])], [([⟨.z, ta, ka - kb - m⟩], [])]]
      else if ta == .z then some [[([⟨tb, .z, ka - kb - m⟩], [])], [([⟨.z, tb, mx - ka + kb⟩], [])]]
      else none
    | .arith .mul _, some (ta, ka), some (tb, kb) =>
      if ta == .z && tb == .z then some [ground (decide (m ≤ ka * kb ∧ ka * kb ≤ mx))]
      else if tb == .z && ka == 0 then some (scaled ta kb)
      else if ta == .z && kb == 0 then some (scaled tb ka)
      else none
    | .divide _ _, some (tn, kn), some (td, kd) =>
      let nonzero : List (List Bound × List Diseq) :=
        if td == .z then ground (kd != 0) else [([], [⟨td, .z, -kd⟩])]
      let notMin : List (List Bound × List Diseq) :=
        if tn == .z then ground (kn != m) else [([], [⟨tn, .z, m - kn⟩])]
      let notMinusOne : List (List Bound × List Diseq) :=
        if td == .z then ground (kd != -1) else [([], [⟨td, .z, -1 - kd⟩])]
      if t.signed then some [nonzero, notMin ++ notMinusOne] else some [nonzero]
    | .divide _ _, none, some (td, kd) =>
      let nonzero : List (List Bound × List Diseq) :=
        if td == .z then ground (kd != 0) else [([], [⟨td, .z, -kd⟩])]
      let notMinusOne : List (List Bound × List Diseq) :=
        if td == .z then ground (kd != -1) else [([], [⟨td, .z, -1 - kd⟩])]
      if t.signed then some [nonzero, notMinusOne] else some [nonzero]
    | _, _, _ => none
  let affine : Option (List (Ineq × Option (Form × Term))) :=
    match op, a.image, b.image with
    | .arith .add _, some fa, some fb => some (range (fa.add fb))
    | .arith .sub _, some fa, some fb => some (range (fa.sub fb))
    | .arith .mul _, some fa, some fb =>
      if fa.isConst then some (range (fb.scale fa.c))
      else if fb.isConst then some (range (fa.scale fb.c)) else none
    | .divide _ _, some _, some fb =>
      if !t.signed then some [(Ineq.le (Form.const 1) fb, none)] else none
    | _, _, _ => none
  let product := match op, a.image, b.image with
    | .arith .mul _, some fa, some fb => if fa.isConst || fb.isConst then none else some (a, b, t)
    | _, _, _ => none
  { goal := some goal, l0, affine, product }

/-- A divisor interval's nonzero parts [ENT-3.S7]: its negative part and its
nonnegative part from one. -/
def divisorParts (b : Int × Int) : List (Int × Int) :=
  (if b.1 ≤ -1 then [(b.1, min b.2 (-1))] else []) ++ (if 1 ≤ b.2 then [(max b.1 1, b.2)] else [])

/-- An integer operation row's result bounds and relations [ENT-3.S7], as
facts about the result term r: the row's result interval, limited to the type,
through Z, and its offset or order relations to each operand that is a term
other than a literal. -/
def opFacts (st : State) (op : InfixOp) (t : IntTy) (a b : Val) (r : Term) : List Bound :=
  if st.contra then [] else
  let (a0, a1) := operandInterval st t a
  let (b0, b1) := operandInterval st t b
  let limit (lo hi : Int) : List Bound :=
    if fits lo && fits hi then [⟨r, .z, min hi t.maxVal⟩, ⟨.z, r, -(max lo t.minVal)⟩] else []
  let rel (v : Val) (lo hi : Int) : List Bound :=
    match v.term with
    | some (x, k) => if x == .z then [] else [⟨r, x, hi + k⟩, ⟨x, r, -(lo + k)⟩]
    | none => []
  let le (v : Val) : List Bound :=
    match v.term with
    | some (x, k) => if x == .z then [] else [⟨r, x, k⟩]
    | none => []
  let inType (lo hi : Int) : Bool := decide (t.minVal ≤ lo ∧ hi ≤ t.maxVal)
  match op with
  | .arith .add m =>
    let (lo, hi) := (a0 + b0, a1 + b1)
    if m == .wrap && !inType lo hi then [] else limit lo hi ++ rel a b0 b1 ++ rel b a0 a1
  | .arith .sub m =>
    let (lo, hi) := (a0 - b1, a1 - b0)
    if m == .wrap && !inType lo hi then [] else limit lo hi ++ rel a (-b1) (-b0)
  | .arith .mul m =>
    match hull [a0 * b0, a0 * b1, a1 * b0, a1 * b1] with
    | some (lo, hi) =>
      if m == .wrap && !inType lo hi then [] else
      if [a0 * b0, a0 * b1, a1 * b0, a1 * b1].all fits then limit lo hi else []
    | none => []
  | .divide .div _ =>
    let qs := (divisorParts (b0, b1)).flatMap fun (d0, d1) =>
      [a0.tdiv d0, a0.tdiv d1, a1.tdiv d0, a1.tdiv d1]
    let rels := if a0 ≥ 0 && b0 ≥ 0 then le a else []
    match hull qs with
    | some (lo, hi) => limit lo hi ++ rels
    | none => []
  | .divide .rem _ =>
    let d : Int := max b0.natAbs b1.natAbs
    let lo := min 0 (max a0 (1 - d))
    let hi := max 0 (min a1 (d - 1))
    let rels := if a0 ≥ 0 && b0 ≥ 0 then
        le a ++ (match b.term with
          | some (x, k) => if x == .z then [] else [⟨r, x, k - 1⟩]
          | none => [])
      else []
    limit lo hi ++ rels
  | _ => []

/-- A negation's or absolute value's result bounds and relation [ENT-3.S7]. -/
def unaryFacts (st : State) (u : UnaryOp) (m : Mode) (t : IntTy) (a : Val) (r : Term) : List Bound :=
  if st.contra then [] else
  let (a0, a1) := operandInterval st t a
  if m == .wrap && !(a0 > t.minVal) then [] else
  let limit (lo hi : Int) : List Bound := [⟨r, .z, min hi t.maxVal⟩, ⟨.z, r, -(max lo t.minVal)⟩]
  match u with
  | .neg => limit (-a1) (-a0)
  | .abs =>
    let (lo, hi) := if a0 ≥ 0 then (a0, a1) else if a1 < 0 then (-a1, -a0) else (0, max (-a0) a1)
    limit lo hi ++ (match a.term with
      | some (x, k) => if x == .z then [] else [⟨x, r, -k⟩]
      | none => [])

/-- What a right-hand side denotes: its value, the S7 facts it establishes on
its result, a division it captures, a dividend bound a matched product
inherits, and a call to publish once its destination is known. -/
structure Rhs where
  val : Val
  facts : Term → List Bound := fun _ => []
  quot : Option (Form × Form × Option Int) := none
  product : Option Form := none
  call : Option (FnDecl × List (String × Val)) := none

/-- Evaluate an atom: its subscripts' bounds are proved [OP-4]. -/
def evalAtom (env : Env) (st : State) : Atom → M Val
  | .int t v => return { term := some (.z, v), image := some (Form.const v), gexpr := some (.int t v), ty := .int t }
  | .unit => return { gexpr := some .unit, ty := .unit }
  | .var x => do
    let some v := env.var? x | fail "TYPE-6" s!"the name `{x}` is not in scope"
    match v.ty with
    | .int t => return { term := some (.place v.id t, 0), image := st.image? v.id, gexpr := some (.var v.id), ty := .int t }
    | ty => return { gexpr := some (.var v.id), ty }
  | .index b off => do
    let some v := env.var? b | fail "TYPE-6" s!"the name `{b}` is not in scope"
    let o ← evalAtom env st off
    let .array t n := v.ty | fail "OP-4" s!"`{b}` is not an Array"
    let g ← get
    let q : Query := {
      l0 := o.term.map fun (x, k) => [[([⟨x, .z, (n : Int) - 1 - k⟩], [])]],
      affine := o.image.map fun f => [(Ineq.le f (Form.const ((n : Int) - 1)), none)] }
    if !discharged env g st q then fail "OP-4" s!"the subscript of `{b}` is not proved below its length"
    let img ← typeAtom t
    return { image := some img, gexpr := o.gexpr.map (.index v.id ·), ty := .int t }

/-- Evaluate a right-hand side whose operands are atoms. Each exact integer
operation's domain is proved [OP-2]; a call of a function of the program has
each requirement proved in the caller's state [FN-8]. -/
def evalRhs (env : Env) (st : State) (e : Expr) : M Rhs := do
  match e with
  | .atom a => return { val := ← evalAtom env st a }
  | .bool b => return { val := { gexpr := some (.bool b), ty := .bool } }
  | .infix a op b => do
    let va ← evalAtom env st a
    let vb ← evalAtom env st b
    let g := mk2 (GExpr.infix op) va.gexpr vb.gexpr
    match op, va.ty with
    | .compare _, _ | .arith _ .defined, _ | .divide _ .defined, _ =>
      return { val := { gexpr := g, ty := .bool } }
    | _, .int t => do
      let gen ← get
      let exact := match op with | .arith _ .exact | .divide _ .exact => true | _ => false
      if exact then
        let goalOp := match op with
          | .arith o _ => InfixOp.arith o .defined
          | .divide o _ => .divide o .defined
          | o => o
        let goal := (mk2 (GExpr.infix goalOp) va.gexpr vb.gexpr).getD .unit
        if !discharged env gen st (domainQuery goal op t va vb) then
          fail "OP-2" s!"the domain of an exact `{repr op}` is not proved"
      let image ← match op, va.image, vb.image with
        | .arith .add .exact, some fa, some fb => pure (fa.add fb)
        | .arith .sub .exact, some fa, some fb => pure (fa.sub fb)
        | .arith .mul .exact, some fa, some fb =>
          if fa.isConst then pure (fb.scale fa.c) else if fb.isConst then pure (fa.scale fb.c)
          else typeAtom t
        | _, _, _ => typeAtom t
      let quot := match op, va.term, vb.term, va.image, vb.image with
        | .divide .div .exact, some _, some (td, kd), some fa, some fd =>
          if t.signed then none else some (fa, fd, if td == .z then some kd else none)
        | _, _, _, _, _ => none
      let product := match op, va.image, vb.image with
        | .arith .mul .exact, some fa, some fb =>
          if fa.isConst || fb.isConst then none else
          (st.quotients.find? fun d => (Form.atom d.q == fa && d.divisor == fb) ||
            (Form.atom d.q == fb && d.divisor == fa)).map (·.dividend)
        | _, _, _ => none
      return { val := { image := some image, gexpr := g, ty := .int t },
               facts := opFacts st op t va vb, quot, product }
    | _, _ => return { val := { gexpr := g, ty := .bool } }
  | .op o as => do
    let vs ← as.mapM (evalAtom env st)
    match o, vs with
    | .unary u m, [va] =>
      match va.ty, m with
      | .int t, .defined => return { val := { gexpr := mk1 (GExpr.op1 o) va.gexpr, ty := .bool } }
      | .int t, _ => do
        if m == .exact then
          let gen ← get
          let goal := (mk1 (GExpr.op1 (.unary u .defined)) va.gexpr).getD .unit
          let q : Query := { goal := some goal,
            l0 := va.term.map fun (x, k) => if x == .z then
                [[if k != t.minVal then ([], []) else ([⟨.z, .z, -1⟩], [])]]
              else [[([], [⟨x, .z, t.minVal - k⟩])]],
            affine := va.image.map fun f => match u with
              | .neg => [(Ineq.le (f.scale (-1)) (Form.const t.maxVal), none),
                         (Ineq.le (Form.const t.minVal) (f.scale (-1)), none)]
              | .abs => [(Ineq.le (Form.const (t.minVal + 1)) f, none)] }
          if !discharged env gen st q then fail "OP-2" "the domain of an exact negation or absolute value is not proved"
        let image ← match u, m, va.image with
          | .neg, .exact, some f => pure (f.scale (-1))
          | _, _, _ => typeAtom t
        return { val := { image := some image, gexpr := mk1 (GExpr.op1 o) va.gexpr, ty := .int t },
                 facts := unaryFacts st u m t va }
      | ty, _ => return { val := { ty } }
    | _, [va] => return { val := { gexpr := mk1 (GExpr.op1 o) va.gexpr, ty := .bool } }
    | _, [va, vb] =>
      return { val := { gexpr := mk2 (GExpr.op2 o) va.gexpr vb.gexpr, ty := .bool } }
    | _, _ => fail "OP-1" "a table operation with the wrong number of operands"
  | .call path targs as => do
    match env.prog.callee? path, targs with
    | some (.fn f), _ => do
      let args ← as.mapM fun (n, a) => do return (n, ← evalAtom env st a)
      let gen ← get
      for r in f.requires do
        let looks (x : String) : Option (Term × Int) := (args.find? (·.1 == x)).bind (·.2.term)
        let looki (x : String) : Option Form := (args.find? (·.1 == x)).bind (·.2.image)
        let lookg (x : String) : Option GExpr := (args.find? (·.1 == x)).bind (·.2.gexpr)
        if !discharged env gen st (relQuery (relG lookg r) r looks looki) then
          fail "FN-8" s!"a requirement of `{f.name}` is not proved at the call"
      let ty := Static.normTy env.prog f.resultTy
      let image ← match ty with
        | .int t => some <$> typeAtom t
        | _ => pure none
      return { val := { image, ty }, call := some (f, args) }
    | some .exitStatus, _ => do
      let _ ← as.mapM fun (_, a) => evalAtom env st a
      return { val := { ty := Static.exitStatusTy } }
    | some .arrayFilled, [.ty (.int t), .nat n] => do
      let _ ← as.mapM fun (_, a) => evalAtom env st a
      return { val := { ty := .array t n } }
    | _, _ => fail "TYPE-6" s!"the callee `{"::".intercalate path}` does not resolve"

/-- Publish a call's verified postconditions [ENT-3.S12, ENT-3.S13, CALL-6]:
each operand mints a call datum equal to its pre-transfer term, then, after
`kill` applies the statement's own kills, each relation is established with
its result at `dest` and each parameter at its datum. A relation naming the
result needs a destination; one over a formal whose actual is a receiver
`skip` excludes is not published. -/
def publish (env : Env) (st : State) (f : FnDecl) (args : List (String × Val))
    (dest : Option (Term × Form)) (skip : Option Nat) (kill : State → State) : M State := do
  let mut st := st
  let mut subs : List (String × (Term × Int) × Form) := []
  for (n, v) in args do
    match v.term, v.image, v.ty with
    | some (.z, k), some img, _ => subs := (n, (.z, k), img) :: subs
    | some (x, k), some img, .int t =>
      let d := Term.datum (← freshId) t
      let dimg := img
      st := { st.add env [⟨d, x, k⟩, ⟨x, d, -k⟩] with data := st.data ++ [(d, dimg)] }
      subs := (n, (d, 0), dimg) :: subs
    | _, _, _ => pure ()
  st := kill st
  for r in f.ensures do
    let names := (Affine.names r.lhs ++ Affine.names r.rhs)
    if names.contains f.result && dest.isNone then continue
    if let some x := skip then
      if names.any fun n => (args.find? (·.1 == n)).any fun (_, v) =>
        v.term.any (fun (t, _) => t.support.contains x) then continue
    let look (x : String) : Option ((Term × Int) × Form) :=
      if x == f.result then dest.map fun (t, img) => ((t, 0), img)
      else (subs.find? (·.1 == x)).map (·.2)
    match pubRel r look with
    | some (bs, ds, prems) =>
      st := { st.add env bs ds with premises := st.premises ++ prems }
    | none => pure ()
  return st
where
  /-- A relation side as a term plus constant, with a sum term for a side that
  carries an additional parameter, and its image. -/
  side (look : String → Option ((Term × Int) × Form)) (l : Lin) : Option ((Term × Int) × Form) :=
    match l.clean.2 with
    | [] => some ((.z, l.1), Form.const l.1)
    | [(x, 1)] => do
      let ((t, k), img) ← look x
      return ((t, k + l.1), img.add (Form.const l.1))
    | [(x, 1), (y, 1)] => do
      let ((t, k), i) ← look x
      let ((u, j), i') ← look y
      return ((.sum t u, k + j + l.1), (i.add i').add (Form.const l.1))
    | _ => none
  pubRel (r : Relation) (look : String → Option ((Term × Int) × Form)) :
      Option (List Bound × List Diseq × List Ineq) := do
    let la ← lin r.lhs
    let lb ← lin r.rhs
    let (ta, ia) ← side look la
    let (tb, ib) ← side look lb
    let (bs, ds) := relFacts r.op ta tb
    let hasSum := (match ta.1 with | .sum _ _ => true | _ => false) ||
      (match tb.1 with | .sum _ _ => true | _ => false)
    return (bs, ds, if hasSum then ineqs r.op ia ib else [])

end Checker

end Whitefoot
