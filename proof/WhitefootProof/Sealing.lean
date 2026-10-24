import WhitefootProof.Integers

/-!
# Sealed type invariants

The sealing rules of affine type invariants, TYPE-11.S1 to TYPE-11.S7 of the
specification amendment that firn-req-invariant-through-fields makes, as a
component of the checker and of the run it judges. Until that amendment
merges, its rules are on its branch; `research/investigations/spec-proof/
DESIGN.md` ("Sealed type invariants") maps each of their sentences to a
definition here or gives the reason it is outside the component.

The checker's side is the sealing state of one function body at one point —
its debts, and each reference variable's members [REF-1] — and the judgments
the rules make of it: the rows of TYPE-11.S3, the hand-offs of TYPE-11.S4 with
the restoration and satisfaction of TYPE-11.S5, the merges and loop headers of
TYPE-11.S6, and the instances of TYPE-11.S7. Its places are resolved places
[OWN-7] of integers, structs, `Box`es, `Array`s and enums. It reads two kinds of
judgment it does not make: the fact core's [ENT-2, ENT-5, MSR-4] — whether two
offsets are proved distinct, whether an instance is proved, which terms a kill
batch kills — named by `Core`, and what a call writes [EFF-2, EFF-5], which a
call event lists.

The run's side is a store of values at roots — local bindings, by-value
parameters and the referents of reference parameters — and the path each
reference variable names. A value is a tree whose children are a struct's
fields, a `Box`'s content, an `Array`'s elements or the payload of the variant
an enum holds; the declared types of the roots say which, so validity
[TYPE-11.S1] is read at a place's declared type. `Sem` gives the terms their
values and states what the proofs assume of the fact core and of a callee.
-/

namespace Whitefoot.Seal

/-! ## Lists -/

/-- The element at an index. -/
def nth {α : Type} : List α → Nat → Option α
  | [], _ => none
  | a :: _, 0 => some a
  | _ :: l, n + 1 => nth l n

/-- The list with the element at an index replaced; the list itself when the
index is past its end. -/
def upd {α : Type} : List α → Nat → α → List α
  | [], _, _ => []
  | _ :: l, 0, b => b :: l
  | a :: l, n + 1, b => a :: upd l n b

/-! ## Runtime values and places -/

/-- A step of a runtime place [OWN-7]: a struct field, the content of a `Box`,
an element of an `Array`, or a field of the payload of an enum's variant. -/
inductive CStep where
  | field (i : Nat)
  | inner
  | elem (k : Nat)
  | payload (variant field : Nat)
  deriving DecidableEq, Repr

/-- The child of a value that a step selects. -/
def CStep.child : CStep → Nat
  | .field i => i
  | .inner => 0
  | .elem k => k
  | .payload _ i => i

/-- What a node value is: a struct, a `Box`, an `Array`, or an enum. -/
inductive VKind where
  | strct
  | box
  | array
  | enum
  deriving DecidableEq, Repr

/-- Whether a step selects below a node of this kind holding this tag: a field
below a struct, the content below a `Box`, an element below an `Array`, and a
payload field below an enum holding that variant. -/
def CStep.fits : CStep → VKind → Nat → Bool
  | .field _, k, _ => k == .strct
  | .inner, k, _ => k == .box
  | .elem _, k, _ => k == .array
  | .payload v _, k, tag => k == .enum && v == tag

/-- A runtime value: an integer, or a node of a kind whose children are a
struct's fields, a `Box`'s content, an `Array`'s elements, or the payload
fields of the variant an enum holds, its tag. -/
inductive Val where
  | int (v : Int)
  | node (kind : VKind) (tag : Nat) (cs : List Val)

/-- The child of a value that a step selects, if the step selects below it. -/
def Val.child : Val → CStep → Option Val
  | .int _, _ => none
  | .node k t cs, s => if s.fits k t then nth cs s.child else none

/-- The value with the child a step selects replaced. -/
def Val.replace : Val → CStep → Val → Val
  | .int x, _, _ => .int x
  | .node k t cs, s, c => .node k t (upd cs s.child c)

/-- The value at a path below a value, if the path exists. -/
def Val.get : Val → List CStep → Option Val
  | v, [] => some v
  | v, s :: p =>
    match Val.child v s with
    | some c => Val.get c p
    | none => none

/-- The value with the value at a path below it replaced, if the path exists. -/
def Val.set : Val → List CStep → Val → Option Val
  | _, [], w => some w
  | v, s :: p, w =>
    match Val.child v s with
    | some c =>
      match Val.set c p w with
      | some c' => some (Val.replace v s c')
      | none => none
    | none => none

/-- A runtime place: a root and the steps below it. -/
structure CPlace where
  root : String
  path : List CStep
  deriving DecidableEq, Repr

/-- A runtime place lies at or below another: one root, and the other's steps
begin it. -/
def CPlace.Below (π ρ : CPlace) : Prop :=
  π.root = ρ.root ∧ ∃ s, ρ.path ++ s = π.path

/-- Two runtime places are apart: neither lies at or below the other. -/
def CPlace.Apart (π ρ : CPlace) : Prop :=
  ¬ π.Below ρ ∧ ¬ ρ.Below π

/-- The run the component reads: the value each live root holds, and the path
each live reference variable names [REF-1]. A root is a local binding, a
by-value parameter, or the referent of a reference parameter. -/
structure Run where
  vals : String → Option Val
  refs : String → Option CPlace

/-- A function with its result at one argument replaced. -/
def override {α : Type} (g : String → Option α) (x : String) (a : Option α) :
    String → Option α :=
  fun y => if y = x then a else g y

/-- The value at a runtime place, if the place exists. -/
def Run.get (r : Run) (π : CPlace) : Option Val :=
  match r.vals π.root with
  | some v => Val.get v π.path
  | none => none

/-- The run with the value at a runtime place replaced, if the place exists. -/
def Run.set (r : Run) (π : CPlace) (w : Val) : Option Run :=
  match r.vals π.root with
  | some v =>
    match Val.set v π.path w with
    | some v' => some { r with vals := override r.vals π.root (some v') }
    | none => none
  | none => none

/-! ## Types and type invariants -/

/-- The type of a place [TYPE-11.S1]: a fragment integer, a struct, a `Box`,
an `Array` of a length, or an enum. -/
inductive STy where
  | int
  | strct (name : String)
  | box (t : STy)
  | array (t : STy) (n : Nat)
  | enum (name : String)
  deriving DecidableEq, Repr

/-- One side of an affine type invariant [TYPE-11]: one binder datum displaced
by a constant, or a constant alone. The datum is the projection of field
selections and `Box` content steps that reaches it from the binder. -/
inductive Side where
  | datum (proj : List CStep) (k : Int)
  | const (k : Int)
  deriving DecidableEq, Repr

/-- An affine type invariant: its name and its comparison [TYPE-11]. -/
structure Inv where
  name : String
  lhs : Side
  op : CmpOp
  rhs : Side
  deriving DecidableEq, Repr

/-- A struct declaration: its fields' types in order, and its affine type
invariants. -/
structure StructDecl where
  name : String
  fields : List STy
  invs : List Inv
  deriving Repr

/-- An enum declaration: the types of each variant's payload fields. -/
structure EnumDecl where
  name : String
  variants : List (List STy)
  deriving Repr

/-- The declarations a body's types name. -/
structure Decls where
  structs : List StructDecl
  enums : List EnumDecl
  deriving Repr

/-- An affine type invariant, named by its struct and its own name. -/
structure InvId where
  strct : String
  name : String
  deriving DecidableEq, Repr

/-- What a step selects, apart from its offset: the part of a step a type
reads. -/
inductive Kind where
  | field (i : Nat)
  | inner
  | elem
  | payload (variant field : Nat)
  deriving DecidableEq, Repr

/-- What a runtime step selects. -/
def CStep.kind : CStep → Kind
  | .field i => .field i
  | .inner => .inner
  | .elem _ => .elem
  | .payload v i => .payload v i

namespace Decls

def struct? (D : Decls) (s : String) : Option StructDecl :=
  D.structs.find? (·.name == s)

def enum? (D : Decls) (e : String) : Option EnumDecl :=
  D.enums.find? (·.name == e)

/-- The type of the part of a value that one step selects. -/
def stepTy (D : Decls) : STy → Kind → Option STy
  | .strct s, .field i =>
    match D.struct? s with
    | some d => nth d.fields i
    | none => none
  | .box t, .inner => some t
  | .array t _, .elem => some t
  | .enum e, .payload v i =>
    match D.enum? e with
    | some d =>
      match nth d.variants v with
      | some fs => nth fs i
      | none => none
    | none => none
  | _, _ => none

/-- The type of the part of a value that a list of steps selects. -/
def walk (D : Decls) : STy → List Kind → Option STy
  | t, [] => some t
  | t, k :: ks =>
    match D.stepTy t k with
    | some t' => walk D t' ks
    | none => none

/-- The affine type invariants of a type: those its struct declares. -/
def invsOf (D : Decls) : STy → List InvId
  | .strct s =>
    match D.struct? s with
    | some d => d.invs.map fun i => ⟨s, i.name⟩
    | none => []
  | _ => []

/-- The declaration of an affine type invariant. -/
def inv? (D : Decls) (I : InvId) : Option Inv :=
  match D.struct? I.strct with
  | some d => d.invs.find? (·.name == I.name)
  | none => none

end Decls

/-- The value one side of an invariant takes over a value of its struct: its
datum's integer displaced by its constant. -/
def Side.eval : Side → Val → Option Int
  | .datum p k, v =>
    match Val.get v p with
    | some (.int x) => some (x + k)
    | _ => none
  | .const k, _ => some k

/-- The binder data one side reads. -/
def Side.data : Side → List (List CStep)
  | .datum p _ => [p]
  | .const _ => []

/-- The binder data an invariant reads: its read set below its binder
[TYPE-11.S1]. -/
def Inv.data (I : Inv) : List (List CStep) :=
  I.lhs.data ++ I.rhs.data

/-- The invariant holds of a value of its struct: its comparison is true of
the value's binder data. -/
def Inv.HoldsOf (I : Inv) (v : Val) : Prop :=
  ∀ a b, I.lhs.eval v = some a → I.rhs.eval v = some b → I.op.eval a b = true

/-! ## Resolved places, anchor shapes and debts -/

/-- An index position of a resolved place or an anchor shape [TYPE-11.S2]: a
captured offset [REF-1], a term of the fact core, or the unknown position
`[?]`, which overlaps every position of its run [OWN-7]. -/
inductive Ix (T : Type) where
  | cap (t : T)
  | unknown
  deriving DecidableEq, Repr

/-- A step of a resolved place [OWN-7]. In the component every step selects a
value, so each adds one level [TYPE-11.S2]; range, page, `Segments` and window
steps, which add none, are outside it. -/
inductive SStep (T : Type) where
  | field (i : Nat)
  | inner
  | elem (ix : Ix T)
  | payload (variant field : Nat)
  deriving DecidableEq, Repr

/-- A resolved place, or an anchor shape [REF-1, TYPE-11.S2]: its root and the
steps below it. -/
structure Shape (T : Type) where
  root : String
  steps : List (SStep T)
  deriving DecidableEq, Repr

/-- Where a spelled place starts: a binding, or the referent `q^` of a
reference variable [REF-1]. -/
inductive Base where
  | var (x : String)
  | deref (q : String)
  deriving DecidableEq, Repr

/-- A place as the body spells it: its start and the steps after it, each
index position a captured offset. -/
structure Place (T : Type) where
  base : Base
  steps : List (SStep T)
  deriving DecidableEq, Repr

/-- The reference and projection a debt records [TYPE-11.S2]. -/
structure Rec (T : Type) where
  ref : String
  proj : List (SStep T)
  deriving DecidableEq, Repr

/-- A debt [TYPE-11.S2]: an anchor shape with one affine type invariant of its
type, recording a reference and projection or none; or a cover `R.**`. The
breaking events a debt records serve only its diagnostic, which the component
leaves out. -/
inductive Debt (T : Type) where
  | inv (a : Shape T) (i : InvId) (rc : Option (Rec T))
  | cover (r : Shape T)
  deriving DecidableEq, Repr

/-- A kill batch [ENT-5], as the component reports it to the fact core: the
resolved places it writes, the bindings whose scope it ends, and the reference
variables it rebinds or ends. -/
structure Kills (T : Type) where
  writes : List (Shape T)
  ended : List String
  rebound : List String
  deriving Repr

/-- The fact core's judgments that the sealing rules read, over its terms `T`
and its fact states `F`. -/
structure Core (T F : Type) where
  /-- The facts prove two offsets distinct [OWN-7, ENT-6]. -/
  distinct : F → T → T → Bool
  /-- The facts prove the instance of an affine type invariant over a spelled
  place, its binder data the terms that place forms [TYPE-11.S5, MSR-4]. -/
  proves : F → Place T → InvId → Bool
  /-- The kill batch writes or ends the support of the term [ENT-5]. -/
  killed : F → Kills T → T → Bool
  /-- The term reads a tracked place [ENT-2]. -/
  tracked : T → Bool

/-- What the sealing judgments of a body read: the declarations, each root's
declared type, and the fact core. -/
structure Ctx (T F : Type) where
  decls : Decls
  tys : String → Option STy
  core : Core T F

/-- The sealing state at a point of a body: its debts [TYPE-11.S2], and each
live reference variable's members [REF-1]. -/
structure St (T : Type) where
  debts : List (Debt T)
  refs : List (String × List (Shape T))
  deriving Repr

/-- The right-hand side of a commit or a `let`, as the sealing rules tell them
apart [TYPE-11.S3]. -/
inductive Rhs (T : Type) where
  /-- A value that is no place read — a literal, a call result, a
  construction — which carries no debt. -/
  | fresh
  /-- A read of a place, copied or moved. -/
  | read (p : Place T) (move : Bool)
  deriving Repr

/-- An event of a body as the sealing rules see it, each with its statement's
kill batch [ENT-5]. -/
inductive Event (T : Type) where
  /-- `set w = v;`, or an [OP-12] update, whose target w is not a complete own
  local binding: v is handed off [TYPE-11.S4], and the write and store rows
  apply [TYPE-11.S3]. -/
  | store (w : Place T) (rhs : Rhs T) (kb : Kills T)
  /-- `let y = v;`, or `set y = v;` whose target is the complete own local
  binding y: the store row at y, or the continuation row [TYPE-11.S3]. -/
  | bind (y : String) (rhs : Rhs T) (kb : Kills T)
  /-- `swap(first: &f, second: &s)` [OP-11, TYPE-11.S3]. -/
  | swap (f s : Place T) (kb : Kills T)
  /-- A call: the places it hands off — each own or copy argument's place and
  each reference argument's referent [TYPE-11.S4] — and its projected writes,
  each a reference argument's place with the projection its row declares
  below the parameter [EFF-5]. -/
  | call (handed : List (Place T)) (writes : List (Place T × List (SStep T))) (kb : Kills T)
  /-- The death of a root binding [TYPE-11.S3], with the kill batch of its
  scope exit or consume. -/
  | death (x : String) (kb : Kills T)
  /-- `let q = &p;`, or `set q = &p;` [REF-1]. -/
  | bindRef (q : String) (p : Place T) (kb : Kills T)
  /-- A kill batch alone: a scope exit or the end of a reference [TYPE-11.S3]. -/
  | kill (kb : Kills T)
  deriving Repr

variable {T F : Type}

/-- Whether a step has no unknown position. -/
def SStep.known : SStep T → Bool
  | .elem .unknown => false
  | _ => true

/-- What a resolved step selects. -/
def SStep.kind : SStep T → Kind
  | .field i => .field i
  | .inner => .inner
  | .elem _ => .elem
  | .payload v i => .payload v i

/-- A step of a binder datum's projection as a resolved step. -/
def CStep.lift : CStep → SStep T
  | .field i => .field i
  | .inner => .inner
  | .elem _ => .elem .unknown
  | .payload v i => .payload v i

/-- The captured offsets of a list of steps. -/
def offsets : List (SStep T) → List T
  | [] => []
  | .elem (.cap t) :: ss => t :: offsets ss
  | _ :: ss => offsets ss

/-! ## Reference members -/

/-- A reference description's members for a reference variable: those of its
first entry. -/
def lookupRefs : List (String × List (Shape T)) → String → List (Shape T)
  | [], _ => []
  | (q', ms) :: rest, q => if q' = q then ms else lookupRefs rest q

/-- A reference variable's members [REF-1]. -/
def St.members (s : St T) (q : String) : List (Shape T) :=
  lookupRefs s.refs q

/-- A reference description with one reference variable's members set. -/
def setRef (refs : List (String × List (Shape T))) (q : String) (ms : List (Shape T)) :
    List (String × List (Shape T)) :=
  (q, ms) :: refs.filter (·.1 != q)

/-- The union of two reference descriptions at a merge [REF-1]: each reference
variable's members are the union of its incoming members. -/
def unionRefs [DecidableEq T] (a b : List (String × List (Shape T))) :
    List (String × List (Shape T)) :=
  (a.map (·.1) ++ b.map (·.1)).eraseDups.map fun q =>
    (q, lookupRefs a q ++ (lookupRefs b q).filter fun m => !(lookupRefs a q).contains m)

/-- The resolved members of a spelled place [REF-1]: the place itself when it
starts at a binding, and each member of the reference variable followed by its
steps when it starts at `q^`. -/
def St.resolve (s : St T) (p : Place T) : List (Shape T) :=
  match p.base with
  | .var x => [Shape.mk x p.steps]
  | .deref q => (s.members q).map fun m => Shape.mk m.root (m.steps ++ p.steps)

/-- Each resolved member of a spelled place, with the reference variable it is
spelled through and the level of that variable's referent in it, when the
place resolves through it to several members [TYPE-11.S3]. -/
def St.resolveRec (s : St T) (p : Place T) : List (Shape T × Option (String × Nat)) :=
  match p.base with
  | .var x => [(Shape.mk x p.steps, none)]
  | .deref q =>
    (s.members q).map fun m =>
      (Shape.mk m.root (m.steps ++ p.steps),
        if 2 ≤ (s.members q).length then some (q, m.steps.length) else none)

/-! ## Overlap and the debt judgments -/

/-- Whether two steps at one position provably select different storages
[OWN-7]: two different fields, two offsets the facts prove distinct, or two
different fields of one variant's payload. Two payload steps of different
variants select the same storage. -/
def Ctx.sepStep (X : Ctx T F) (f : F) : SStep T → SStep T → Bool
  | .field i, .field j => i != j
  | .elem (.cap s), .elem (.cap t) => X.core.distinct f s t
  | .payload v i, .payload v' j => v == v' && i != j
  | _, _ => false

/-- Whether some step of two places' common prefix provably selects two
different storages [OWN-7]. -/
def Ctx.sepSteps (X : Ctx T F) (f : F) : List (SStep T) → List (SStep T) → Bool
  | s :: ss, t :: ts => X.sepStep f s t || Ctx.sepSteps X f ss ts
  | _, _ => false

/-- Whether two resolved places may overlap [OWN-7]: one root, and no step of
their common prefix provably selects two different storages; an undischarged
separation counts as overlap. -/
def Ctx.mayOverlap (X : Ctx T F) (f : F) (a b : Shape T) : Bool :=
  a.root == b.root && !X.sepSteps f a.steps b.steps

/-- Whether two steps are one known step, every captured offset equal
[TYPE-11.S2]. -/
def provedStep [DecidableEq T] (x y : SStep T) : Bool :=
  x.known && decide (x = y)

/-- Whether a list of steps begins with another, every captured offset
equal. -/
def provedPrefix [DecidableEq T] : List (SStep T) → List (SStep T) → Bool
  | [], _ => true
  | p :: ps, s :: ss => provedStep p s && provedPrefix ps ss
  | _ :: _, [] => false

/-- Whether a debt may be at or below a place [TYPE-11.S2]: its anchor may
have at least the place's level and may overlap it, or it is a cover whose
place may overlap it. -/
def Ctx.mayAtOrBelow (X : Ctx T F) (f : F) (p : Shape T) : Debt T → Bool
  | .inv a _ _ => decide (p.steps.length ≤ a.steps.length) && X.mayOverlap f a p
  | .cover R => X.mayOverlap f R p

/-- Whether a debt is proved at or below a place [TYPE-11.S2]: its anchor, or
its cover's place, has the place's root and begins with its steps, every
captured offset equal. -/
def Debt.provedAtOrBelow [DecidableEq T] (p : Shape T) : Debt T → Bool
  | .inv a _ _ => a.root == p.root && provedPrefix p.steps a.steps
  | .cover R => R.root == p.root && provedPrefix p.steps R.steps

/-- Whether a debt may denote an anchor and an invariant [TYPE-11.S2]: it is a
cover that may overlap the anchor, or it names the invariant at an anchor of
the same level that may overlap it. -/
def Ctx.mayDenote (X : Ctx T F) (f : F) (a : Shape T) (I : InvId) : Debt T → Bool
  | .cover R => X.mayOverlap f R a
  | .inv b J _ => J == I && b.steps.length == a.steps.length && X.mayOverlap f b a

/-! ## Debt sets -/

/-- Whether two debts have one key: one anchor and invariant, or one cover. -/
def Debt.sameKey [DecidableEq T] : Debt T → Debt T → Bool
  | .inv a I _, .inv b J _ => decide (a = b) && decide (I = J)
  | .cover R, .cover S => decide (R = S)
  | _, _ => false

/-- Two debts of one key, united [TYPE-11.S2]: the result keeps a recorded
reference only when both record the same one. -/
def Debt.unite [DecidableEq T] : Debt T → Debt T → Debt T
  | .inv a I r₁, .inv _ _ r₂ => .inv a I (if r₁ = r₂ then r₁ else none)
  | d, _ => d

/-- A debt set with one more debt, united with the set's debt of its key when
it has one. -/
def addDebt [DecidableEq T] (D : List (Debt T)) (d : Debt T) : List (Debt T) :=
  if D.any (·.sameKey d) then D.map fun e => if e.sameKey d then e.unite d else e
  else D ++ [d]

/-- A debt set with more debts, each united in. -/
def addDebts [DecidableEq T] (D : List (Debt T)) (ds : List (Debt T)) : List (Debt T) :=
  ds.foldl addDebt D

/-! ## Rebasing and exchange -/

/-- A debt that may be at or below a place p, rebased onto a place q
[TYPE-11.S2]: the anchor's steps down to the value at p's level become q's.
Every step of the component selects a value and none is a cover step, so p's
own steps, when the anchor begins with them, and the anchor's shortest prefix
at p's level are one prefix: the anchor's first steps, as many as p has. A
cover above p rebases to the cover of q, and a rebased debt records no
reference. -/
def Debt.rebase (p q : Shape T) : Debt T → Debt T
  | .inv a I _ =>
    if p.steps.length ≤ a.steps.length then
      .inv ⟨q.root, q.steps ++ a.steps.drop p.steps.length⟩ I none
    else .cover q
  | .cover R =>
    if p.steps.length ≤ R.steps.length then
      .cover ⟨q.root, q.steps ++ R.steps.drop p.steps.length⟩
    else .cover q

/-- A debt a `swap` exchanges from target o onto target t [TYPE-11.S3]: it
names its invariant at t when its anchor lies at o's level, and is the cover
of t otherwise, so an exchange appends no steps below t; it records no
reference [TYPE-11.S2]. -/
def Debt.exchange (o t : Shape T) : Debt T → Debt T
  | .inv a I _ => if a.steps.length = o.steps.length then .inv t I none else .cover t
  | .cover _ => .cover t

/-! ## Restoration and satisfaction -/

/-- The spelled place a debt's instance is formed over, with its invariant
[TYPE-11.S5]: `r^` followed by p for a debt that records r and p, and the
anchor's own place for any other. An anchor with `[?]`, and a cover, form no
instance. -/
def Debt.instancePlace? : Debt T → Option (Place T × InvId)
  | .inv a I rc =>
    if a.steps.all SStep.known then
      (match rc with
       | some ⟨q, p⟩ => some (⟨.deref q, p⟩, I)
       | none => some (⟨.var a.root, a.steps⟩, I))
    else none
  | .cover _ => none

/-- Whether a debt is restored at a point [TYPE-11.S5]: the facts there prove
the instance of its invariant. -/
def Ctx.restored (X : Ctx T F) (f : F) (d : Debt T) : Bool :=
  match d.instancePlace? with
  | some (p, I) => X.core.proves f p I
  | none => false

/-- Whether a step is a captured offset. -/
def SStep.isCap : SStep T → Bool
  | .elem (.cap _) => true
  | _ => false

/-- Whether an anchor's steps agree with a handed place's at each position of
the handed place [TYPE-11.S5]: `[?]` where the handed place has a captured
offset, and otherwise the handed place's own known step. -/
def agrees [DecidableEq T] : List (SStep T) → List (SStep T) → Bool
  | _, [] => true
  | [], _ :: _ => false
  | x :: xs, y :: ys =>
    ((x == .elem .unknown && y.isCap) || (x.known && x == y)) && agrees xs ys

/-- Whether a debt at an anchor with `[?]` is satisfied at a handed member
[TYPE-11.S5]: the member has a captured offset at each `[?]` of the anchor
within its length, the anchor's other steps there are the member's, the anchor
has no `[?]` below the member, and the facts prove the instance at the anchor
with the member's offsets at its `[?]` positions, which is the member's steps
followed by the anchor's below them. -/
def Ctx.satisfied [DecidableEq T] (X : Ctx T F) (f : F) (m : Shape T) : Debt T → Bool
  | .inv a I _ =>
    a.root == m.root && !(a.steps.all SStep.known) && agrees a.steps m.steps &&
      (a.steps.drop m.steps.length).all SStep.known &&
      X.core.proves f ⟨.var a.root, m.steps ++ a.steps.drop m.steps.length⟩ I
  | .cover _ => false

/-- Each debt of a set offered for restoration [TYPE-11.S5]; the restored ones
leave it. -/
def Ctx.restoreAll (X : Ctx T F) (f : F) (D : List (Debt T)) : List (Debt T) :=
  D.filter fun d => !X.restored f d

/-! ## Hand-offs and instances -/

/-- A hand-off of the value at a place [TYPE-11.S4]: every debt that may be at
or below a resolved member of the handed place must be restored, or satisfied
at that member, in the state before the hand-off's own events; the restored
ones leave the set. None when one is neither. -/
def Ctx.handoff [DecidableEq T] (X : Ctx T F) (f : F) (s : St T) (p : Place T) :
    Option (St T) :=
  if s.debts.all (fun d => (s.resolve p).all fun m =>
      !X.mayAtOrBelow f m d || X.restored f d || X.satisfied f m d) then
    some ⟨s.debts.filter (fun d =>
      !((s.resolve p).any (fun m => X.mayAtOrBelow f m d) && X.restored f d)), s.refs⟩
  else none

/-- Whether the instance of an affine invariant at a place is established
[TYPE-11.S7]: no debt may denote any resolved member of the place and the
invariant. The other premises — the root binding live and no atomic
statement's target, each payload step under a current refinement — are the
walk's, and make the place exist in the run. -/
def Ctx.instance? (X : Ctx T F) (f : F) (s : St T) (p : Place T) (I : InvId) : Bool :=
  (s.resolve p).all fun m => s.debts.all fun d => !X.mayDenote f m I d

/-! ## The rows of TYPE-11.S3 -/

/-- The declared type of a resolved place. -/
def Ctx.tyAt (X : Ctx T F) (a : Shape T) : Option STy :=
  match X.tys a.root with
  | some t => X.decls.walk t (a.steps.map SStep.kind)
  | none => none

/-- The affine type invariants of the struct a resolved place holds; an
anchor is a place with one [TYPE-11.S1]. -/
def Ctx.invsAt (X : Ctx T F) (a : Shape T) : List InvId :=
  match X.tyAt a with
  | some t => X.decls.invsOf t
  | none => []

/-- Whether a write at w reaches an invariant's read set at an anchor a under
[ENT-5]'s kill judgment [TYPE-11.S3]: w may overlap one of the invariant's
binder data read from a. -/
def Ctx.reaches (X : Ctx T F) (f : F) (w a : Shape T) (I : InvId) : Bool :=
  match X.decls.inv? I with
  | some inv => inv.data.any fun d => X.mayOverlap f w ⟨a.root, a.steps ++ d.map CStep.lift⟩
  | none => false

/-- A list of steps after a kill batch: each offset the batch kills, and each
later offset that reads a tracked place, becomes `[?]` [TYPE-11.S3]. -/
def Ctx.widenSteps (X : Ctx T F) (f : F) (kb : Kills T) : Bool → List (SStep T) → List (SStep T)
  | _, [] => []
  | after, .elem (.cap t) :: ss =>
    if X.core.killed f kb t || (after && X.core.tracked t) then
      .elem .unknown :: Ctx.widenSteps X f kb true ss
    else .elem (.cap t) :: Ctx.widenSteps X f kb after ss
  | after, s :: ss => s :: Ctx.widenSteps X f kb after ss

/-- Whether a kill batch leaves a debt as it is [TYPE-11.S3]: it kills no
offset of the debt's anchor or recorded projection, and rebinds or ends no
reference the debt records. -/
def Ctx.untouched (X : Ctx T F) (f : F) (kb : Kills T) : Debt T → Bool
  | .inv a _ rc =>
    (offsets a.steps).all (fun t => !X.core.killed f kb t) &&
      (match rc with
       | some ⟨q, p⟩ => !kb.rebound.contains q && (offsets p).all fun t => !X.core.killed f kb t
       | none => true)
  | .cover R => (offsets R.steps).all fun t => !X.core.killed f kb t

/-- A debt a kill batch touches, widened [TYPE-11.S3]: each killed offset, and
each later offset that reads a tracked place, becomes `[?]`, and the debt
records no reference. -/
def Ctx.widen (X : Ctx T F) (f : F) (kb : Kills T) : Debt T → Debt T
  | .inv a I _ => .inv ⟨a.root, X.widenSteps f kb false a.steps⟩ I none
  | .cover R => .cover ⟨R.root, X.widenSteps f kb false R.steps⟩

/-- A debt that a statement's own rows add, as the statement's kill batch
leaves it: widened when the batch touches it. It is not offered for
restoration: the state before the batch may prove the instance its own write
breaks. -/
def Ctx.settle (X : Ctx T F) (f : F) (kb : Kills T) (d : Debt T) : Debt T :=
  if X.untouched f kb d then d else X.widen f kb d

/-- The kill row [TYPE-11.S3]: each debt a kill batch touches is first offered
for restoration in the state before the batch [TYPE-11.S5], and one not
restored is widened. -/
def Ctx.killRow [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (D : List (Debt T)) :
    List (Debt T) :=
  addDebts [] (D.filterMap fun d =>
    if X.untouched f kb d then some d
    else if X.restored f d then none
    else some (X.widen f kb d))

/-- A reference variable's member after a kill batch: an offset the batch kills
becomes `[?]`. The reference keeps the value the offset had when it was formed
[REF-1], which the term no longer names. -/
def Ctx.widenShape (X : Ctx T F) (f : F) (kb : Kills T) (m : Shape T) : Shape T :=
  ⟨m.root, X.widenSteps f kb false m.steps⟩

/-- Each reference variable's members after a kill batch. -/
def Ctx.killRefs (X : Ctx T F) (f : F) (kb : Kills T) (refs : List (String × List (Shape T))) :
    List (String × List (Shape T)) :=
  refs.map fun e => (e.1, e.2.map (X.widenShape f kb))

/-- The reference and projection a write-row debt at level k records
[TYPE-11.S3]: r and the projection from `r^` to the anchor, when the write is
spelled through r with its referent at level b and the anchor lies at or below
that referent. -/
def writeRec (w : Shape T) (k : Nat) : Option (String × Nat) → Option (Rec T)
  | some (q, b) => if b ≤ k then some ⟨q, (w.steps.take k).drop b⟩ else none
  | none => none

/-- The write row [TYPE-11.S3] for one resolved member w of a write: (a, I) for
each anchor a strictly above w at a level below hi and each affine invariant I
of a whose read set the write reaches. When the place is spelled through a
reference r that resolves to several members, `through` gives r and the level
of its referent in w, and a debt at or below the referent records r and the
projection from `r^` to a. An ordinary write passes w's level as hi; a call's
projected write passes its argument referent's, since the callee leaves the
referent sealed. Each debt is settled by the statement's kill batch. -/
def Ctx.writeDebts [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (w : Shape T)
    (hi : Nat) (through : Option (String × Nat)) : List (Debt T) :=
  (List.range (min hi w.steps.length)).flatMap fun k =>
    (X.invsAt ⟨w.root, w.steps.take k⟩).filterMap fun I =>
      if X.reaches f w ⟨w.root, w.steps.take k⟩ I then
        some (X.settle f kb (.inv ⟨w.root, w.steps.take k⟩ I (writeRec w k through)))
      else none

/-- The write rows [TYPE-11.S3] of a write at a spelled place p followed by a
projection: those of each resolved member followed by the projection, with
the member's own level as the bound. An ordinary write has no projection. -/
def Ctx.writeRows [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (s : St T) (p : Place T)
    (proj : List (SStep T)) : List (Debt T) :=
  (s.resolveRec p).flatMap fun mt =>
    X.writeDebts f kb ⟨mt.1.root, mt.1.steps ++ proj⟩ mt.1.steps.length mt.2

/-- Whether a debt records the reference variable q and a projection p, and its
anchor is the member m followed by p. -/
def Debt.recordsAt [DecidableEq T] (q : String) (m : Shape T) : Debt T → Bool
  | .inv a _ (some ⟨r, p⟩) => r == q && decide (a = ⟨m.root, m.steps ++ p⟩)
  | _ => false

/-- The store rows [TYPE-11.S3] of a commit at w whose right-hand side is a
hand-off, a call result or a construction: at a place resolving to one member,
each debt proved at or below it leaves the set; at a place spelled through a
reference variable q that resolves to several members, at each member, each
debt proved at or below it that records q and a projection p and whose anchor
is q's referent there followed by p. -/
def storeRemove [DecidableEq T] (s : St T) (w : Place T) (D : List (Debt T)) : List (Debt T) :=
  match w.base with
  | .deref q =>
    if 2 ≤ (s.members q).length then
      D.filter fun d => !((s.members q).any fun m =>
        d.provedAtOrBelow ⟨m.root, m.steps ++ w.steps⟩ && d.recordsAt q m)
    else D.filter fun d => !((s.resolve w).any fun m => d.provedAtOrBelow m)
  | .var _ => D.filter fun d => !((s.resolve w).any fun m => d.provedAtOrBelow m)

/-- The continuation row [TYPE-11.S3] of a value from place p into the
complete own local binding y: it takes each debt that may be at or below a
member of p, rebased onto y; removes each debt proved at or below y; then adds
the taken debts. -/
def Ctx.continueRow [DecidableEq T] (X : Ctx T F) (f : F) (s : St T) (p : Place T) (y : String)
    (D : List (Debt T)) : List (Debt T) :=
  addDebts (D.filter fun d => !d.provedAtOrBelow ⟨y, []⟩)
    ((s.resolve p).flatMap fun m =>
      (D.filter fun d => X.mayAtOrBelow f m d).map (Debt.rebase m ⟨y, []⟩))

/-- Whether a debt is proved at or below a target that resolves to one member. -/
def provedOne [DecidableEq T] : List (Shape T) → Debt T → Bool
  | [m], d => d.provedAtOrBelow m
  | _, _ => false

/-- The debts that stay where they are at a swap: all of them when the two
targets may be one place, and otherwise each one not proved at or below a
target that resolves to one member. -/
def Ctx.swapStay [DecidableEq T] (X : Ctx T F) (f : F) (fs ss : List (Shape T)) (D : List (Debt T)) :
    List (Debt T) :=
  if fs.any (fun a => ss.any fun b => X.mayOverlap f a b && a.steps.length == b.steps.length) then D
  else D.filter fun d => !(provedOne fs d || provedOne ss d)

/-- The debts a swap exchanges from each member o of one target onto each
member t of the other: each debt that may be at or below o, or is proved there,
exchanged and settled by the swap's kill batch. -/
def Ctx.across [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (D : List (Debt T))
    (src dst : List (Shape T)) : List (Debt T) :=
  src.flatMap fun o =>
    (D.filter fun d => X.mayAtOrBelow f o d || d.provedAtOrBelow o).flatMap fun d =>
      dst.map fun t => X.settle f kb (d.exchange o t)

/-- The swap row [TYPE-11.S3] for `swap(first: &f, second: &s)`, given each
target's resolved members: a debt proved at or below a target that resolves
to one member moves onto the other target, exchanged; a debt that may be at
or below a member of one target without being proved there stays where it is
and is copied onto each member of the other target, exchanged; when the two
targets may be one place, each keeps its own debts as well, so each receives
the union. -/
def Ctx.swapRow [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (fs ss : List (Shape T))
    (D : List (Debt T)) : List (Debt T) :=
  addDebts (X.swapStay f fs ss D) (X.across f kb D fs ss ++ X.across f kb D ss fs)

/-- The death row [TYPE-11.S3]: each debt whose anchor, or cover's place, is
rooted at the binding leaves the set. -/
def deathRow (x : String) (D : List (Debt T)) : List (Debt T) :=
  D.filter fun d =>
    match d with
    | .inv a _ _ => a.root != x
    | .cover R => R.root != x

/-! ## Events -/

/-- The store rows of a commit at w, then the kill row, then the write row, in
the state after the right-hand side's hand-off [TYPE-11.S3]. -/
def Ctx.storeRows [DecidableEq T] (X : Ctx T F) (f : F) (kb : Kills T) (w : Place T) (s : St T) :
    St T :=
  ⟨addDebts (X.killRow f kb (storeRemove s w s.debts)) (X.writeRows f kb s w []),
    X.killRefs f kb s.refs⟩

/-- The sealing state after an event, or none when the rules reject it. Each
row is judged in the state before the event's kill batch, with the members
the event's places resolve to there; the kill row comes before the rows that
add debts, whose debts the batch settles. -/
def Ctx.step [DecidableEq T] (X : Ctx T F) (f : F) (s : St T) : Event T → Option (St T)
  | .store w rhs kb =>
    -- the right-hand side's hand-off [TYPE-11.S4]
    (match rhs with
     | .read p _ => X.handoff f s p
     | .fresh => some s).map (X.storeRows f kb w)
  | .bind y rhs kb =>
    match rhs with
    | .fresh =>
      -- the store row at y; a root has no anchor strictly above it
      some ⟨X.killRow f kb (storeRemove s ⟨.var y, []⟩ s.debts), X.killRefs f kb s.refs⟩
    | .read p _ =>
      -- the kill row, then the continuation row
      some ⟨X.continueRow f s p y (X.killRow f kb s.debts), X.killRefs f kb s.refs⟩
  | .swap fp sp kb =>
    -- [OP-11]: every pair of targets equal or disjoint, neither possibly a
    -- proper ancestor of the other
    if (s.resolve fp).all (fun a => (s.resolve sp).all fun b =>
        !X.mayOverlap f a b || a.steps.length == b.steps.length) then
      -- restoration at the swap, in the state before its events
      -- [TYPE-11.S5]; the kill row, the write rows of the two targets, then
      -- the exchange
      some ⟨X.swapRow f kb (s.resolve fp) (s.resolve sp)
          (addDebts (X.killRow f kb (s.debts.filter fun d =>
              !((s.resolve fp ++ s.resolve sp).any (fun m => X.mayAtOrBelow f m d) &&
                X.restored f d)))
            (X.writeRows f kb s fp [] ++ X.writeRows f kb s sp [])),
        X.killRefs f kb s.refs⟩
    else none
  | .call handed ws kb =>
    -- each argument's hand-off, in the state before the call's events; then
    -- the kill row, and each projected write's row for the anchors strictly
    -- above its argument's referent
    (handed.foldlM (fun t p => X.handoff f t p) s).map fun s₁ =>
      ⟨addDebts (X.killRow f kb s₁.debts) (ws.flatMap fun pw => X.writeRows f kb s₁ pw.1 pw.2),
        X.killRefs f kb s₁.refs⟩
  | .death x kb => some ⟨deathRow x (X.killRow f kb s.debts), X.killRefs f kb s.refs⟩
  | .bindRef q p kb =>
    some ⟨X.killRow f kb s.debts,
      setRef (X.killRefs f kb s.refs) q ((s.resolve p).map (X.widenShape f kb))⟩
  | .kill kb =>
    some ⟨X.killRow f kb s.debts, (X.killRefs f kb s.refs).filter fun e => !kb.rebound.contains e.1⟩

/-- A state with an incoming edge's debts after their restoration in the
edge's own state, and the edge's reference members, united in [TYPE-11.S6,
REF-1]. -/
def Ctx.absorb [DecidableEq T] (X : Ctx T F) (h : St T) (e : F × St T) : St T :=
  ⟨addDebts h.debts (X.restoreAll e.1 e.2.debts), unionRefs h.refs e.2.refs⟩

/-- The sealing state at a merge of control inputs [TYPE-11.S6]: each incoming
edge first offers each of its debts for restoration in its own state, and the
debts after the merge are the union of those that remain. The instances the
merge then establishes are those `instance?` gives on each edge. -/
def Ctx.join [DecidableEq T] (X : Ctx T F) (edges : List (F × St T)) : St T :=
  edges.foldl X.absorb ⟨[], []⟩

/-- The sealing state at a loop header [TYPE-11.S6]: the entering edge's
debts, and every backedge's and `continue`'s after their restoration. The walk
iterates the body from this state until it no longer changes. -/
def Ctx.header [DecidableEq T] (X : Ctx T F) (entry : St T) (backs : List (F × St T)) : St T :=
  backs.foldl X.absorb entry

/-! ## Meaning in a run -/

/-- A resolved step denotes a runtime step: the same field, content or
payload field, and an element whose index is the offset's value, or any
element for `[?]`. -/
inductive StepDenotes (ev : Run → T → Option Int) (r : Run) : SStep T → CStep → Prop
  | field (i : Nat) : StepDenotes ev r (.field i) (.field i)
  | inner : StepDenotes ev r .inner .inner
  | unknown (k : Nat) : StepDenotes ev r (.elem .unknown) (.elem k)
  | cap (t : T) (k : Nat) (h : ev r t = some (k : Int)) : StepDenotes ev r (.elem (.cap t)) (.elem k)
  | payload (v i : Nat) : StepDenotes ev r (.payload v i) (.payload v i)

/-- Each resolved step denotes the runtime step at its position. -/
inductive StepsDenote (ev : Run → T → Option Int) (r : Run) : List (SStep T) → List CStep → Prop
  | nil : StepsDenote ev r [] []
  | cons {s : SStep T} {c : CStep} {ss : List (SStep T)} {cs : List CStep} :
      StepDenotes ev r s c → StepsDenote ev r ss cs → StepsDenote ev r (s :: ss) (c :: cs)

/-- A resolved place denotes a runtime place. -/
def Shape.Denotes (a : Shape T) (ev : Run → T → Option Int) (r : Run) (π : CPlace) : Prop :=
  a.root = π.root ∧ StepsDenote ev r a.steps π.path

/-- A spelled place denotes a runtime place: through the path a reference
variable names, for a place spelled through it. -/
def Place.Denotes (p : Place T) (ev : Run → T → Option Int) (r : Run) (π : CPlace) : Prop :=
  match p.base with
  | .var x => π.root = x ∧ StepsDenote ev r p.steps π.path
  | .deref q => ∃ (ρ : CPlace) (rest : List CStep), r.refs q = some ρ ∧ StepsDenote ev r p.steps rest ∧
      π = ⟨ρ.root, ρ.path ++ rest⟩

/-- The declared type of a runtime place's value. -/
def Ctx.tyOf (X : Ctx T F) (π : CPlace) : Option STy :=
  match X.tys π.root with
  | some t => X.decls.walk t (π.path.map CStep.kind)
  | none => none

/-- The affine type invariants of the struct a runtime place holds. -/
def Ctx.invsOfPlace (X : Ctx T F) (π : CPlace) : List InvId :=
  match X.tyOf π with
  | some t => X.decls.invsOf t
  | none => []

/-- An invariant holds at a runtime place, when the place exists. -/
def Ctx.InvHolds (X : Ctx T F) (r : Run) (π : CPlace) (I : InvId) : Prop :=
  ∀ (v : Val) (inv : Inv), r.get π = some v → X.decls.inv? I = some inv → inv.HoldsOf v

/-- The affine type invariants at a path below a value of a type. -/
def Ctx.invsBelow (X : Ctx T F) (t : STy) (s : List CStep) : List InvId :=
  match X.decls.walk t (s.map CStep.kind) with
  | some t' => X.decls.invsOf t'
  | none => []

/-- A value is valid at a type [TYPE-11.S1]: every struct value it owns
satisfies each affine type invariant of its struct. -/
def Ctx.ValidVal (X : Ctx T F) (t : STy) (v : Val) : Prop :=
  ∀ (s : List CStep) (I : InvId) (u : Val) (inv : Inv), I ∈ X.invsBelow t s →
    Val.get v s = some u → X.decls.inv? I = some inv →
    inv.HoldsOf u

/-- The value at a runtime place is valid at its declared type. -/
def Ctx.Valid (X : Ctx T F) (r : Run) (π : CPlace) : Prop :=
  ∀ (t : STy) (v : Val), X.tyOf π = some t → r.get π = some v → X.ValidVal t v

/-- A recorded reference and projection denote a runtime place: the one `r^`
followed by p denotes. A debt that records none places no condition. -/
def Rec.Denotes : Option (Rec T) → (Run → T → Option Int) → Run → CPlace → Prop
  | some ⟨q, p⟩, ev, r, π => (Place.mk (.deref q) p).Denotes ev r π
  | none, _, _, _ => True

/-- A debt covers an invariant at a runtime place: it names the invariant at
an anchor that denotes the place, and its recorded reference and projection,
if any, denote the place too; or it is a cover above the place. -/
def Debt.Covers : Debt T → (Run → T → Option Int) → Run → CPlace → InvId → Prop
  | .inv a J rc, ev, r, π, I => J = I ∧ a.Denotes ev r π ∧ Rec.Denotes rc ev r π
  | .cover R, ev, r, _, _ => ∃ ρ : CPlace, R.Denotes ev r ρ ∧ π.Below ρ

/-- The sealing invariant [TYPE-11.S2]: each affine invariant at each runtime
place holds unless a debt covers it. -/
def Ctx.Sealed (X : Ctx T F) (ev : Run → T → Option Int) (r : Run) (D : List (Debt T)) : Prop :=
  ∀ (π : CPlace) (I : InvId), I ∈ X.invsOfPlace π → ¬ X.InvHolds r π I →
    ∃ d ∈ D, Debt.Covers d ev r π I

/-- Each reference variable that names a path in the run has a member that
denotes it [REF-1]. -/
def RefsOk (ev : Run → T → Option Int) (r : Run) (s : St T) : Prop :=
  ∀ (q : String) (ρ : CPlace), r.refs q = some ρ → ∃ m ∈ s.members q, Shape.Denotes m ev r ρ

/-- What the terms mean in a run, which fact states hold there, and the
soundness of the fact core's judgments the sealing rules read [ENT-2, ENT-6,
MSR-4]. -/
structure Sem (X : Ctx T F) where
  eval : Run → T → Option Int
  Facts : F → Run → Prop
  distinct_sound : ∀ f r s t x y, Facts f r → X.core.distinct f s t = true →
    eval r s = some x → eval r t = some y → x ≠ y
  proves_sound : ∀ f r p I π, Facts f r → X.core.proves f p I = true →
    Place.Denotes p eval r π → X.InvHolds r π I

/-- The sealing state agrees with a run: the sealing invariant holds of its
debts, and its reference members describe the run's references. -/
def Ctx.Holds (X : Ctx T F) (M : Sem X) (s : St T) (r : Run) : Prop :=
  X.Sealed M.eval r s.debts ∧ RefsOk M.eval r s

/-- A step changes only what its kill batch reports: each term the batch does
not kill keeps its value [ENT-5], and each reference variable it does not
rebind or end keeps its path. -/
def Sem.Frame {X : Ctx T F} (M : Sem X) (f : F) (kb : Kills T) (r r' : Run) : Prop :=
  (∀ t, X.core.killed f kb t = false → M.eval r' t = M.eval r t) ∧
  (∀ q, q ∉ kb.rebound → r'.refs q = r.refs q)

/-- The value a commit or `let` stores at a place of declared type t, as the
rest of the checker delivers it: a value that is no place read is valid
[TYPE-11.S4, FN-9], and a place read gives the value of a place of the same
type. -/
def Ctx.RhsValue (X : Ctx T F) (M : Sem X) (r : Run) (t : Option STy) : Rhs T → Val → Prop
  | .fresh, v => ∀ t', t = some t' → X.ValidVal t' v
  | .read p _, v => ∃ π : CPlace, p.Denotes M.eval r π ∧ r.get π = some v ∧ X.tyOf π = t

/-- What an event does in the run, with what the rest of the checker and the
callee supply: the type rules, the kill judgment's soundness, and for a call
its row's exactness [EFF-2] — each place apart from every place it writes
keeps its value, and each place below no written referent that exists after it
existed before — its exits' hand-offs, which leave each written referent
valid [TYPE-11.S4], and that it rebinds no reference variable of the caller
[REF-1]. -/
def Ctx.RunStep (X : Ctx T F) (M : Sem X) (f : F) : Event T → Run → Run → Prop
  | .store w rhs kb, r, r' =>
    ∃ (π : CPlace) (v : Val), w.Denotes M.eval r π ∧ X.RhsValue M r (X.tyOf π) rhs v ∧
      r.set π v = some r' ∧ M.Frame f kb r r'
  | .bind y rhs kb, r, r' =>
    ∃ v : Val, X.RhsValue M r (X.tys y) rhs v ∧ r'.vals = override r.vals y (some v) ∧
      r'.refs = r.refs ∧ M.Frame f kb r r'
  | .swap fp sp kb, r, r' =>
    ∃ (πf πs : CPlace) (vf vs : Val), fp.Denotes M.eval r πf ∧ sp.Denotes M.eval r πs ∧
      r.get πf = some vf ∧ r.get πs = some vs ∧ X.tyOf πf = X.tyOf πs ∧
      (if πf = πs then r' = r
       else ∃ r₁ : Run, r.set πf vs = some r₁ ∧ r₁.set πs vf = some r') ∧
      M.Frame f kb r r'
  | .call _ ws kb, r, r' =>
    (∀ π : CPlace,
      (∀ (p : Place T) (proj : List (SStep T)), (p, proj) ∈ ws →
        ∀ (πa : CPlace) (ext : List CStep), p.Denotes M.eval r πa →
          StepsDenote M.eval r proj ext → π.Apart ⟨πa.root, πa.path ++ ext⟩) →
      r'.get π = r.get π) ∧
    (∀ π : CPlace,
      (∀ (p : Place T) (proj : List (SStep T)), (p, proj) ∈ ws →
        ∀ πa : CPlace, p.Denotes M.eval r πa → ¬ π.Below πa) →
      (r'.get π).isSome → (r.get π).isSome) ∧
    (∀ (p : Place T) (proj : List (SStep T)), (p, proj) ∈ ws →
      ∀ πa : CPlace, p.Denotes M.eval r πa → X.Valid r' πa) ∧
    r'.refs = r.refs ∧ M.Frame f kb r r'
  | .death x kb, r, r' =>
    r'.vals = override r.vals x none ∧ r'.refs = r.refs ∧ M.Frame f kb r r'
  | .bindRef q p kb, r, r' =>
    ∃ π : CPlace, p.Denotes M.eval r π ∧ r'.vals = r.vals ∧
      r'.refs = override r.refs q (some π) ∧ M.Frame f kb r r'
  | .kill kb, r, r' =>
    (∀ x, x ∉ kb.ended → r'.vals x = r.vals x) ∧ (∀ x ∈ kb.ended, r'.vals x = none) ∧
      (∀ q ∈ kb.rebound, r'.refs q = none) ∧ M.Frame f kb r r'

end Whitefoot.Seal
