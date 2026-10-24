import WhitefootProof.Typing
import WhitefootProof.FactsSound
import WhitefootProof.AffineSound

/-!
# What the checker's facts mean of a frame

The checker's walk records a snapshot on entering each statement: the bindings
in scope, the fact state and the atoms registered so far (`Points.lean`). This
module says what a snapshot claims of an interpreter frame, its bindings `E`
and its parameters' entry values `E0`:

- a value lies in its type's range (`Value.InRange`), and a frame fits a
  binding table when it has the same names in the same order, newest first,
  each value of its binding's type (`Env.Fits`);
- every identity a snapshot names lies below a bound (`Snapshot.Fresh`), so the
  atoms and bindings the walk mints later are new to it;
- a snapshot holds on a frame when the frame fits its binding table and some
  valuation of the atoms and of the terms makes every part of its fact state
  hold of the frame: the L0 facts, the images, the call data, the premises, the
  signed goals, the links, the stable parameters and the captured quotients
  (`Snapshot.HoldsWith`, `Snapshot.Holds`); a contradictory state holds of no
  frame;
- a checker value describes a runtime value when its term, its image and its
  goal expression each give that value and the value has its type
  (`Checker.Val.Denotes`).

The lemmas that follow a successful run of the checker's state monad come
first; the later proof modules use them.
-/

namespace Whitefoot

open Fact Aff

/-! ## Runs of the checker -/

namespace CheckerRun

theorem error_bind {ε α β : Type} (e : ε) (f : α → Except ε β) :
    (Except.error e >>= f) = .error e := rfl

theorem run_bind {α β : Type} {x : Checker.M α} {f : α → Checker.M β} {g g' : Checker.Gen}
    {b : β} (h : (x >>= f).run g = .ok (b, g')) :
    ∃ a g1, x.run g = .ok (a, g1) ∧ (f a).run g1 = .ok (b, g') := by
  rw [StateT.run_bind] at h
  obtain ⟨⟨a, g1⟩, h1, h2⟩ := Typing.bind_ok h
  exact ⟨a, g1, h1, h2⟩

theorem run_pure {α : Type} {a b : α} {g g' : Checker.Gen}
    (h : (pure a : Checker.M α).run g = .ok (b, g')) : b = a ∧ g' = g := by
  have e : (pure a : Checker.M α).run g = .ok (a, g) := rfl
  rw [e] at h
  cases h
  exact ⟨rfl, rfl⟩

theorem run_get {g a g' : Checker.Gen}
    (h : (get : Checker.M Checker.Gen).run g = .ok (a, g')) : a = g ∧ g' = g := by
  have e : (get : Checker.M Checker.Gen).run g = .ok (g, g) := rfl
  rw [e] at h
  cases h
  exact ⟨rfl, rfl⟩

theorem fail_run {α : Type} (r m : String) (g : Checker.Gen) :
    (Checker.fail r m : Checker.M α).run g = .error ⟨r, m⟩ := rfl

theorem run_fail {α : Type} {r m : String} {g : Checker.Gen} {p : α × Checker.Gen}
    (h : (Checker.fail r m : Checker.M α).run g = .ok p) : False := by
  rw [fail_run] at h
  cases h

theorem typeAtom_run (t : IntTy) (g : Checker.Gen) :
    (Checker.typeAtom t).run g = .ok (Form.atom g.next,
      { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms }) := rfl

/-- Every element of a list passes the body of a checker `for` loop that
succeeds, when the body neither changes the generator nor stops early. -/
theorem forIn_run {α : Type} {body : α → PUnit → Checker.M (ForInStep PUnit)} {P : α → Prop}
    (hb : ∀ a g s g', (body a ⟨⟩).run g = .ok (s, g') → P a ∧ s = .yield ⟨⟩ ∧ g' = g) :
    ∀ {l : List α} {g g' : Checker.Gen} {u : PUnit},
      (forIn l ⟨⟩ body).run g = .ok (u, g') → g' = g ∧ ∀ a ∈ l, P a
  | [], g, g', u, h => by
    rw [List.forIn_nil] at h
    obtain ⟨-, hg⟩ := run_pure h
    exact ⟨hg, by simp⟩
  | x :: xs, g, g', u, h => by
    rw [List.forIn_cons] at h
    obtain ⟨s, g1, hs, hrest⟩ := run_bind h
    obtain ⟨hx, hs1, hg1⟩ := hb x g s g1 hs
    rw [hs1, hg1] at hrest
    have hrest' : (forIn xs ⟨⟩ body).run g = .ok (u, g') := hrest
    obtain ⟨hg, hall⟩ := forIn_run hb hrest'
    refine ⟨hg, fun a ha => ?_⟩
    rcases List.mem_cons.mp ha with hax | ha'
    · rw [hax]
      exact hx
    · exact hall a ha'

theorem not_not_true {b : Bool} (h : ¬((!b) = true)) : b = true := by
  cases b <;> simp_all

end CheckerRun

/-! ## Values, frames and the binding table -/

/-- A value lies in its type's range: an integer is a member of its type, and
every element of an array is. -/
def Value.InRange : Value → Prop
  | .int t v => t.Mem v
  | .array t es => ∀ z ∈ es, t.Mem z
  | _ => True

theorem Value.Typed.inRange {w : Value} {ty : Ty} (h : w.Typed ty) : w.InRange := by
  cases ty with
  | int t =>
    obtain ⟨z, rfl, hz⟩ := h.int
    exact hz
  | array t n =>
    obtain ⟨es, rfl, -, hes⟩ := h.array
    exact hes
  | _ => cases w <;> simp_all [Value.Typed, Value.InRange]

/-- An atom's integer literals are members of their types, as the static
check requires. -/
def Atom.LitOk : Atom → Prop
  | .int t v => t.Mem v
  | .index _ off => off.LitOk
  | _ => True

namespace Checker

/-- The scope a binding table gives the judgments without facts. -/
def scopeOf (vars : List Var) : Static.Scope := vars.map fun v => (v.name, v.ty)

end Checker

/-- A frame's bindings match a binding table: the same names in the same
order, newest first, and each value has its binding's type. -/
def Env.Fits (E : Env) (vars : List Checker.Var) : Prop := EnvTyped E (Checker.scopeOf vars)

theorem EnvTyped.cons_inv {n : String} {w : Value} {E : Env} {y : String} {t : Ty}
    {sc : Static.Scope} (h : EnvTyped ((n, w) :: E) ((y, t) :: sc)) :
    n = y ∧ w.Typed t ∧ EnvTyped E sc := by
  cases h with
  | cons hw hE => exact ⟨rfl, hw, hE⟩

theorem Env.get_cons (n : String) (w : Value) (E : Env) (x : String) :
    Env.get ((n, w) :: E) x = if n = x then .ok w else Env.get E x := by
  by_cases h : n = x
  · subst h
    simp [Env.get]
  · simp [Env.get, h]

namespace Checker

/-- The value binding `id` has in a frame, read through a binding table. -/
def valueOf (vars : List Var) (E : Whitefoot.Env) (id : Nat) : Option Value :=
  ((vars.zip E).find? (·.1.id == id)).map (·.2.2)

theorem valueOf_cons (v : Var) (vs : List Var) (b : String × Value) (E : Whitefoot.Env)
    (id : Nat) :
    valueOf (v :: vs) (b :: E) id = if v.id = id then some b.2 else valueOf vs E id := by
  unfold valueOf
  by_cases h : v.id = id
  · simp [h]
  · simp [h]

end Checker

theorem Env.Fits.get : ∀ {vars : List Checker.Var} {E : Env}, E.Fits vars →
    (vars.map (·.id)).Nodup → ∀ {x : String} {v : Checker.Var},
    vars.find? (·.name == x) = some v →
    ∃ w, E.get x = .ok w ∧ w.Typed v.ty ∧ Checker.valueOf vars E v.id = some w
  | [], _, _, _, x, v, hv => by simp at hv
  | v' :: vs, [], h, _, x, v, hv => by
    have h' : EnvTyped [] ((v'.name, v'.ty) :: Checker.scopeOf vs) := h
    cases h'
  | v' :: vs, (n, w) :: E, h, hid, x, v, hv => by
    have h' : EnvTyped ((n, w) :: E) ((v'.name, v'.ty) :: Checker.scopeOf vs) := h
    obtain ⟨hn, hw, hrest⟩ := EnvTyped.cons_inv h'
    rw [List.map_cons, List.nodup_cons] at hid
    by_cases hx : v'.name = x
    · rw [List.find?_cons_of_pos (by simp [hx])] at hv
      cases hv
      refine ⟨w, ?_, hw, ?_⟩
      · rw [Env.get_cons, ite_eq_left (hn.trans hx)]
      · rw [Checker.valueOf_cons, ite_eq_left rfl]
    · rw [List.find?_cons_of_neg (by simp [hx])] at hv
      obtain ⟨w', hg, hfit, hval⟩ := Env.Fits.get hrest hid.2 hv
      refine ⟨w', ?_, hfit, ?_⟩
      · rw [Env.get_cons, ite_eq_right (by rw [hn]; exact hx)]
        exact hg
      · have hmem : v ∈ vs := List.mem_of_find?_eq_some hv
        have hne : v'.id ≠ v.id := fun he => hid.1 (he ▸ List.mem_map_of_mem hmem)
        rw [Checker.valueOf_cons, ite_eq_right hne]
        exact hval

theorem Env.Fits.names {E : Env} {vars : List Checker.Var} (h : E.Fits vars) :
    E.map (·.1) = vars.map (·.name) := by
  have h' : EnvTyped E (Checker.scopeOf vars) := h
  rw [h'.names]
  simp [Checker.scopeOf]

/-! ## Identities a state names -/

namespace Fact

/-- The identities a term names. -/
def Term.ids : Term → List Nat
  | .z => []
  | .place id _ => [id]
  | .commit id _ => [id]
  | .datum id _ => [id]
  | .sum a b => a.ids ++ b.ids

end Fact

namespace Aff

def Form.atomIds (f : Form) : List Nat := f.xs.map (·.1)

def Ineq.atomIds (i : Ineq) : List Nat := i.xs.map (·.1)

theorem evalCoeffs_congr {G G' : Nat → Int} :
    ∀ {xs : Coeffs}, (∀ x ∈ xs.map (·.1), G' x = G x) → evalCoeffs G' xs = evalCoeffs G xs
  | [], _ => rfl
  | p :: rest, h => by
    have h1 : G' p.1 = G p.1 := h p.1 (by simp)
    have h2 := evalCoeffs_congr (xs := rest) (fun x hx => h x (by simp [hx]))
    simp only [evalCoeffs, h1, h2]

theorem Form.eval_congr {G G' : Nat → Int} {f : Form} (h : ∀ x ∈ f.atomIds, G' x = G x) :
    f.eval G' = f.eval G := by
  unfold Form.eval
  rw [evalCoeffs_congr h]

theorem Ineq.holds_congr {G G' : Nat → Int} {i : Ineq} (h : ∀ x ∈ i.atomIds, G' x = G x) :
    i.Holds G' ↔ i.Holds G := by
  unfold Ineq.Holds
  rw [evalCoeffs_congr h]

end Aff

/-! ## Goal expressions over a frame -/

namespace Checker

namespace GExpr

/-- The binding identities an expression reads. -/
def ids : GExpr → List Nat
  | .var id => [id]
  | .index b off => b :: off.ids
  | .infix _ a b => a.ids ++ b.ids
  | .op1 _ a => a.ids
  | .op2 _ a b => a.ids ++ b.ids
  | .aff _ a b => a.ids ++ b.ids
  | .cmp _ a b => a.ids ++ b.ids
  | _ => []

/-- An expression's value when each binding identity `id` has the value
`σ id`: the interpreter's operations, with a contract clause's `+`, `-` and
`*` over the mathematical integers. -/
def eval (σ : Nat → Option Value) : GExpr → Option Value
  | .int t v => some (.int t v)
  | .bool b => some (.bool b)
  | .unit => some .unit
  | .var id => σ id
  | .index b off =>
    match σ b, eval σ off with
    | some (.array t es), some (.int .u64 k) =>
      if k < es.length then some (.int t (es.getD k.toNat 0)) else none
    | _, _ => none
  | .infix op a b =>
    match eval σ a, eval σ b with
    | some x, some y => (evalInfix op x y).toOption
    | _, _ => none
  | .op1 o a =>
    match eval σ a with
    | some x => (evalOp o [x]).toOption
    | none => none
  | .op2 o a b =>
    match eval σ a, eval σ b with
    | some x, some y => (evalOp o [x, y]).toOption
    | _, _ => none
  | .aff op a b =>
    match eval σ a, eval σ b with
    | some (.int t x), some (.int _ y) => some (.int t (op.math x y))
    | _, _ => none
  | .cmp op a b =>
    match eval σ a, eval σ b with
    | some (.int _ x), some (.int _ y) => some (.bool (op.eval x y))
    | _, _ => none

theorem eval_subst {σ : Nat → Option Value} {f : Nat → Option GExpr}
    (hf : ∀ id e, f id = some e → e.eval σ = σ id) : ∀ g : GExpr, (g.subst f).eval σ = g.eval σ
  | .int _ _ => rfl
  | .bool _ => rfl
  | .unit => rfl
  | .var id => by
    simp only [GExpr.subst]
    cases h : f id with
    | none => rfl
    | some e => exact hf id e h
  | .index b off => by simp only [GExpr.subst, GExpr.eval, eval_subst hf off]
  | .infix op a b => by simp only [GExpr.subst, GExpr.eval, eval_subst hf a, eval_subst hf b]
  | .op1 o a => by simp only [GExpr.subst, GExpr.eval, eval_subst hf a]
  | .op2 o a b => by simp only [GExpr.subst, GExpr.eval, eval_subst hf a, eval_subst hf b]
  | .aff o a b => by simp only [GExpr.subst, GExpr.eval, eval_subst hf a, eval_subst hf b]
  | .cmp o a b => by simp only [GExpr.subst, GExpr.eval, eval_subst hf a, eval_subst hf b]

end GExpr

/-- An origin expansion keeps an expression's value when every live link
holds. -/
theorem eval_expand {σ : Nat → Option Value} {st : State}
    (hl : ∀ l ∈ st.links, σ l.1 = l.2.eval σ) (g : GExpr) :
    (st.expand g).eval σ = g.eval σ := by
  unfold State.expand
  have hf : ∀ id e, (st.links.find? (·.1 == id)).map (·.2) = some e → e.eval σ = σ id := by
    intro id e h
    obtain ⟨l, hl', rfl⟩ := Option.map_eq_some_iff.mp h
    have hid : l.1 = id := by simpa using List.find?_some hl'
    rw [← hid]
    exact (hl l (List.mem_of_find?_eq_some hl')).symm
  generalize List.range (st.links.length + 1) = ns
  induction ns generalizing g with
  | nil => rfl
  | cons n ns ih =>
    simp only [List.foldl_cons]
    rw [ih, GExpr.eval_subst hf]

/-- A signed goal present in the state has its sign's value. -/
theorem present_sound {σ : Nat → Option Value} {st : State} {s : Bool} {g : GExpr}
    (hg : ∀ p ∈ st.goals, p.2.eval σ = some (.bool p.1))
    (hl : ∀ l ∈ st.links, σ l.1 = l.2.eval σ)
    (h : st.present s g = true) : g.eval σ = some (.bool s) := by
  unfold State.present at h
  dsimp only at h
  obtain ⟨⟨s', g'⟩, hm, hx⟩ := List.any_eq_true.mp h
  simp only [Bool.and_eq_true, Bool.or_eq_true, beq_iff_eq] at hx
  obtain ⟨rfl, hx⟩ := hx
  have h1 := hg _ hm
  rcases hx with rfl | hx
  · exact h1
  · rw [← eval_expand hl g, ← hx, eval_expand hl g']
    exact h1

/-- An atom's registered interval, or the checked `i128` range when it has
none, as a view reads it [ENT-6]. -/
def atomInterval (atoms : List (Nat × Int × Int)) (x : Nat) : Int × Int :=
  ((atoms.find? (·.1 == x)).map (·.2)).getD (-(2 ^ 127), 2 ^ 127 - 1)

theorem atomInterval_cons (x : Nat) (lo hi : Int) (atoms : List (Nat × Int × Int)) (y : Nat) :
    atomInterval ((x, lo, hi) :: atoms) y = if x = y then (lo, hi) else atomInterval atoms y := by
  unfold atomInterval
  by_cases h : x = y
  · subst h
    simp
  · simp [h]

end Checker

/-! ## Fresh identities -/

/-- Every identity a snapshot names lies below `n`: its bindings, the terms
its L0 facts name, its goals, images, call data, premises, stable parameters,
links and captured divisions, and its registered atoms. -/
structure Snapshot.Fresh (s : Snapshot) (n : Nat) : Prop where
  vars : ∀ v ∈ s.vars, v.id < n
  terms : ∀ t ∈ s.state.l0.terms, ∀ x ∈ t.ids, x < n
  goals : ∀ p ∈ s.state.goals, ∀ x ∈ p.2.ids, x < n
  images : ∀ p ∈ s.state.images, p.1 < n ∧ ∀ x ∈ p.2.atomIds, x < n
  data : ∀ p ∈ s.state.data, (∀ x ∈ p.1.ids, x < n) ∧ ∀ x ∈ p.2.atomIds, x < n
  premises : ∀ i ∈ s.state.premises, ∀ x ∈ i.atomIds, x < n
  stable : ∀ x ∈ s.state.stable, x < n
  links : ∀ l ∈ s.state.links, l.1 < n ∧ ∀ x ∈ l.2.ids, x < n
  quotients : ∀ d ∈ s.state.quotients,
    d.q < n ∧ (∀ x ∈ d.dividend.atomIds, x < n) ∧ ∀ x ∈ d.divisor.atomIds, x < n
  atoms : ∀ a ∈ s.atoms, a.1 < n

theorem Snapshot.Fresh.mono {s : Snapshot} {n n' : Nat} {new : List (Nat × Int × Int)}
    (h : s.Fresh n) (hn : n ≤ n') (hnew : ∀ a ∈ new, a.1 < n') :
    ({ s with atoms := new ++ s.atoms } : Snapshot).Fresh n' where
  vars v hv := by have := h.vars v hv; omega
  terms t ht x hx := by have := h.terms t ht x hx; omega
  goals p hp x hx := by have := h.goals p hp x hx; omega
  images p hp := by
    have := h.images p hp
    exact ⟨by omega, fun x hx => by have := this.2 x hx; omega⟩
  data p hp := by
    have := h.data p hp
    exact ⟨fun x hx => by have := this.1 x hx; omega, fun x hx => by have := this.2 x hx; omega⟩
  premises i hi x hx := by have := h.premises i hi x hx; omega
  stable x hx := by have := h.stable x hx; omega
  links l hl := by
    have := h.links l hl
    exact ⟨by omega, fun x hx => by have := this.2 x hx; omega⟩
  quotients d hd := by
    have := h.quotients d hd
    exact ⟨by omega, fun x hx => by have := this.2.1 x hx; omega,
      fun x hx => by have := this.2.2 x hx; omega⟩
  atoms a ha := by
    rcases List.mem_append.mp ha with ha | ha
    · exact hnew a ha
    · have := h.atoms a ha
      omega

/-! ## What a snapshot's facts mean of a frame -/

/-- The facts of a snapshot hold on a frame with bindings `E` and parameter
entry values `E0`, under a valuation `G` of the atoms and `ν` of the terms:
the frame fits the binding table, whose identities are distinct; every term
lies in its type; every atom lies in the interval a view reads for it; a
tracked place takes its binding's value; the L0 facts hold and name the
places of bindings in the table with their types; every image is its
binding's value; every call datum's image is its term's value; every premise
holds; every goal has its sign's value; every live link's binding has its
right-hand side's value; every stable parameter still has its entry value;
and every captured quotient is its division's quotient. -/
structure Snapshot.HoldsWith (s : Snapshot) (E E0 : Env) (G : Nat → Int) (ν : TermVal) :
    Prop where
  fits : E.Fits s.vars
  ids : (s.vars.map (·.id)).Nodup
  typed : ∀ t, ν.Typed t
  atoms : ∀ x, (Checker.atomInterval s.atoms x).1 ≤ G x ∧ G x ≤ (Checker.atomInterval s.atoms x).2
  places : ∀ v ∈ s.vars, ∀ t, v.ty = .int t →
    Checker.valueOf s.vars E v.id = some (.int t (ν.val (.place v.id t)))
  l0 : s.state.l0.Sat ν
  l0Places : ∀ u ∈ s.state.l0.terms, ∀ id ty, Term.place id ty ∈ u.atoms →
    ∃ v ∈ s.vars, v.id = id ∧ v.ty = .int ty
  images : ∀ p ∈ s.state.images, ∃ t, Checker.valueOf s.vars E p.1 = some (.int t (p.2.eval G))
  data : ∀ p ∈ s.state.data, ν.val p.1 = p.2.eval G
  premises : ∀ i ∈ s.state.premises, i.Holds G
  goals : ∀ p ∈ s.state.goals, p.2.eval (Checker.valueOf s.vars E) = some (.bool p.1)
  links : ∀ l ∈ s.state.links,
    Checker.valueOf s.vars E l.1 = l.2.eval (Checker.valueOf s.vars E)
  stable : ∀ v ∈ s.vars, v.id ∈ s.state.stable →
    Checker.valueOf s.vars E v.id = (E0.get v.name).toOption
  quotients : ∀ d ∈ s.state.quotients, 0 ≤ d.dividend.eval G ∧ 0 < d.divisor.eval G ∧
    G d.q = d.dividend.eval G / d.divisor.eval G

/-- A snapshot holds on a frame (its bindings and its parameters' entry
values) when some valuation of the atoms and terms makes its facts hold. -/
def Snapshot.Holds (s : Snapshot) (fr : Env × Env) : Prop :=
  ∃ G ν, s.HoldsWith fr.1 fr.2 G ν

theorem Snapshot.Holds.fits {s : Snapshot} {E E0 : Env} (h : s.Holds (E, E0)) :
    E.Fits s.vars := by
  obtain ⟨G, ν, h⟩ := h
  exact h.fits

theorem Snapshot.HoldsWith.not_contra {s : Snapshot} {E E0 : Env} {G : Nat → Int} {ν : TermVal}
    (h : s.HoldsWith E E0 G ν) : s.state.contra = false := by
  unfold Checker.State.contra
  rw [h.l0.1, Bool.false_or, List.any_eq_false]
  intro p hm hc
  obtain ⟨b, e⟩ := p
  have hm2 : ((!b), e) ∈ s.state.goals := List.contains_iff_mem.mp hc
  have h1 := h.goals _ hm
  have h2 := h.goals _ hm2
  rw [h1] at h2
  cases b <;> simp at h2

theorem Snapshot.Holds.not_contra {s : Snapshot} {fr : Env × Env} (h : s.Holds fr) :
    s.state.contra = false := by
  obtain ⟨G, ν, h⟩ := h
  exact h.not_contra

/-- The facts keep holding when the atoms take new values below `n` nowhere
and the new registration's intervals hold. -/
theorem Snapshot.HoldsWith.extend {s : Snapshot} {E E0 : Env} {G : Nat → Int} {ν : TermVal}
    {n : Nat} (h : s.HoldsWith E E0 G ν) (hf : s.Fresh n) {atoms' : List (Nat × Int × Int)}
    {G' : Nat → Int}
    (hatoms : ∀ x, (Checker.atomInterval atoms' x).1 ≤ G' x ∧
      G' x ≤ (Checker.atomInterval atoms' x).2)
    (hagree : ∀ x, x < n → G' x = G x) :
    ({ s with atoms := atoms' } : Snapshot).HoldsWith E E0 G' ν where
  fits := h.fits
  ids := h.ids
  typed := h.typed
  atoms := hatoms
  places := h.places
  l0 := h.l0
  l0Places := h.l0Places
  images p hp := by
    obtain ⟨t, ht⟩ := h.images p hp
    refine ⟨t, ?_⟩
    rw [Form.eval_congr (fun x hx => hagree x ((hf.images p hp).2 x hx))]
    exact ht
  data p hp := by
    rw [Form.eval_congr (fun x hx => hagree x ((hf.data p hp).2 x hx))]
    exact h.data p hp
  premises i hi :=
    (Ineq.holds_congr (fun x hx => hagree x (hf.premises i hi x hx))).mpr (h.premises i hi)
  goals := h.goals
  links := h.links
  stable := h.stable
  quotients d hd := by
    obtain ⟨h1, h2, h3⟩ := h.quotients d hd
    obtain ⟨hq, hn, hd'⟩ := hf.quotients d hd
    rw [Form.eval_congr (fun x hx => hagree x (hn x hx)),
      Form.eval_congr (fun x hx => hagree x (hd' x hx)), hagree _ hq]
    exact ⟨h1, h2, h3⟩

/-- A newly registered atom at or above `n` may take any value in its
interval. -/
theorem Snapshot.HoldsWith.mint {s : Snapshot} {E E0 : Env} {G : Nat → Int} {ν : TermVal}
    {n : Nat} (h : s.HoldsWith E E0 G ν) (hf : s.Fresh n) {x : Nat} (hx : n ≤ x)
    {lo hi z : Int} (hz : lo ≤ z ∧ z ≤ hi) :
    ({ s with atoms := (x, lo, hi) :: s.atoms } : Snapshot).HoldsWith E E0
      (fun y => if y = x then z else G y) ν := by
  refine h.extend hf ?_ ?_
  · intro y
    rw [Checker.atomInterval_cons]
    by_cases hy : x = y
    · subst hy
      simp [hz.1, hz.2]
    · rw [if_neg hy]
      have hne : ¬ y = x := fun e => hy e.symm
      simp only [if_neg hne]
      exact h.atoms y
  · intro y hy
    have hne : ¬ y = x := by omega
    simp only [if_neg hne]

theorem Snapshot.Holds.atoms_mono {s : Snapshot} {fr : Env × Env} {n : Nat}
    {new : List (Nat × Int × Int)} (h : s.Holds fr) (hf : s.Fresh n)
    (hnew : ∀ a ∈ new, n ≤ a.1 ∧ a.2.1 ≤ a.2.2) :
    ({ s with atoms := new ++ s.atoms } : Snapshot).Holds fr := by
  obtain ⟨G, ν, h⟩ := h
  refine ⟨fun y => match new.find? (·.1 == y) with
    | some a => a.2.1
    | none => G y, ν, h.extend hf ?_ ?_⟩
  · intro y
    unfold Checker.atomInterval
    rw [List.find?_append]
    cases hy : new.find? (·.1 == y) with
    | none =>
      simp only [Option.none_or]
      exact h.atoms y
    | some a =>
      have ha := hnew a (List.mem_of_find?_eq_some hy)
      simp only [Option.some_or, Option.map_some, Option.getD_some]
      exact ⟨Int.le_refl _, ha.2⟩
  · intro y hy
    have hnone : new.find? (·.1 == y) = none := by
      rw [List.find?_eq_none]
      intro a ha hc
      have h1 := (hnew a ha).1
      have h2 : a.1 = y := by simpa using hc
      omega
    simp only [hnone]

/-! ## What a checker value says of a runtime value -/

namespace Checker

/-- A checker value describes a runtime value: its term with its offset, its
image and its Goal expression each give that value, and the value has its
type. -/
structure Val.Denotes (v : Val) (σ : Nat → Option Value) (ν : TermVal) (G : Nat → Int)
    (w : Value) : Prop where
  term : ∀ x k, v.term = some (x, k) → ∃ t, w = .int t (ν.val x + k)
  image : ∀ f, v.image = some f → ∃ t, w = .int t (f.eval G)
  gexpr : ∀ e, v.gexpr = some e → e.eval σ = some w
  typed : w.Typed v.ty

/-- Every atom a value's image names lies below `n`. -/
def Val.Below (v : Val) (n : Nat) : Prop := ∀ f, v.image = some f → ∀ x ∈ f.atomIds, x < n

theorem Val.Denotes.congr {v : Val} {σ : Nat → Option Value} {ν : TermVal} {G G' : Nat → Int}
    {w : Value} (h : v.Denotes σ ν G w)
    (hg : ∀ f, v.image = some f → ∀ x ∈ f.atomIds, G' x = G x) : v.Denotes σ ν G' w where
  term := h.term
  image f hf := by
    obtain ⟨t, ht⟩ := h.image f hf
    exact ⟨t, by rw [Form.eval_congr (hg f hf)]; exact ht⟩
  gexpr := h.gexpr
  typed := h.typed

/-- The walk's state at a generator: its snapshot holds and is fresh. -/
structure Good (env : Env) (st : State) (g : Gen) (E E0 : Whitefoot.Env) (G : Nat → Int)
    (ν : TermVal) : Prop where
  holds : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν
  fresh : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next

/-- Evaluating from `g` to `g1` changed the valuation of no atom below
`g.next`. -/
def Ext (g g1 : Gen) (G G1 : Nat → Int) : Prop :=
  g.next ≤ g1.next ∧ ∀ x, x < g.next → G1 x = G x

theorem Ext.refl (g : Gen) (G : Nat → Int) : Ext g g G G := ⟨Nat.le_refl _, fun _ _ => rfl⟩

theorem Ext.trans {g g1 g2 : Gen} {G G1 G2 : Nat → Int} (h1 : Ext g g1 G G1)
    (h2 : Ext g1 g2 G1 G2) : Ext g g2 G G2 :=
  ⟨Nat.le_trans h1.1 h2.1, fun x hx => by rw [h2.2 x (by have := h1.1; omega), h1.2 x hx]⟩

theorem Good.typeAtom {env : Env} {st : State} {g : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} (h : Good env st g E E0 G ν) {t : IntTy} {z : Int}
    (hz : t.Mem z) :
    Good env st { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms }
      E E0 (fun y => if y = g.next then z else G y) ν where
  holds := h.holds.mint h.fresh (Nat.le_refl _) hz
  fresh := h.fresh.mono (new := [(g.next, t.minVal, t.maxVal)]) (Nat.le_succ _) (by simp)

theorem Good.typeAtom_ext {g : Gen} {G : Nat → Int} {t : IntTy} {z : Int} :
    Ext g { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms } G
      (fun y => if y = g.next then z else G y) :=
  ⟨Nat.le_succ _, fun x hx => by
    have hne : ¬ x = g.next := by omega
    simp only [if_neg hne]⟩

/-- A tracked binding's place takes the value its image gives. -/
theorem _root_.Whitefoot.Snapshot.HoldsWith.image_place {vars : List Var} {st : State}
    {A : List (Nat × Int × Int)} {E E0 : Whitefoot.Env} {G : Nat → Int} {ν : TermVal}
    (h : (⟨vars, st, A⟩ : Snapshot).HoldsWith E E0 G ν) {v : Var} (hv : v ∈ vars)
    {t : IntTy} (ht : v.ty = .int t) {f : Form} (hf : st.image? v.id = some f) :
    ν.val (.place v.id t) = f.eval G := by
  obtain ⟨p, hp, hpf⟩ := Option.map_eq_some_iff.mp hf
  subst hpf
  have hid : p.1 = v.id := by simpa using List.find?_some hp
  obtain ⟨t', ht'⟩ := h.images p (List.mem_of_find?_eq_some hp)
  have hpl := h.places v hv t ht
  rw [hid] at ht'
  have e := Option.some.inj (hpl.symm.trans ht')
  injection e with _ e2

end Checker

end Whitefoot
