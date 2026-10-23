import WhitefootProof

/-!
# Draft: what the checker's facts mean, and the disposition's soundness

Temporary (spec-proof-discharge-sound): the temporary workflow compiles this
file outside the package until it checks; it then moves into the package's
modules and this file is removed.
-/

namespace Whitefoot

open Fact Aff

/-! ## Monads -/

namespace Discharge

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

theorem fail_bind {α β : Type} (r m : String) (k : α → Checker.M β) :
    (Checker.fail r m >>= k) = Checker.fail r m := rfl

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

end Discharge

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
  exact e2

/-! ## The L0 images and the view -/

/-- The L0 images of candidates whose images give their terms' values hold
[ENT-6]. -/
theorem l0Images_sound {s : L0} {cands : List (Term × Form)} {ν : TermVal} {G : Nat → Int}
    (hs : s.Sat ν) (hc : ∀ c ∈ cands, ν.val c.1 = c.2.eval G) :
    ∀ i ∈ l0Images s cands, i.Holds G := by
  unfold l0Images
  rw [ite_eq_right (by simp [hs.1])]
  dsimp only
  refine Aff.foldl_inv _ (fun (i : Ineq) => i.Holds G)
    (fun (acc : List Ineq) => ∀ j ∈ acc, j.Holds G) ?_ _ [] ?_ ?_
  · intro acc i hi hacc
    have hi' : i.Holds G := hi
    have hacc' : ∀ j ∈ acc, j.Holds G := hacc
    show ∀ j ∈ _, Ineq.Holds G j
    split
    · split
      · intro j hj
        obtain ⟨k, hk, hkj⟩ := List.mem_map.mp hj
        subst hkj
        show (if k.xs == i.xs then i else k).Holds G
        split
        · exact hi'
        · exact hacc' k hk
      · exact hacc'
    · intro j hj
      rcases List.mem_append.mp hj with hj | hj
      · exact hacc' j hj
      · rw [List.mem_singleton.mp hj]
        exact hi'
  · intro i hmem
    show i.Holds G
    obtain ⟨⟨l, fl⟩, hl, hi⟩ := List.mem_flatMap.mp hmem
    obtain ⟨⟨r, fr⟩, hr, hi2⟩ := List.mem_filterMap.mp hi
    have hl' : ν.val l = fl.eval G := hc _ hl
    have hr' : ν.val r = fr.eval G := hc _ hr
    have hi3 : (if l == r then none else (s.tightest l r).bind fun c =>
        if (Ineq.le (fl.sub fr) (Form.const c)).ok &&
          !(Ineq.le (fl.sub fr) (Form.const c)).xs.isEmpty
        then some (Ineq.le (fl.sub fr) (Form.const c)) else none) = some i := hi2
    by_cases hlr : (l == r) = true
    · rw [ite_eq_left hlr] at hi3
      cases hi3
    · rw [ite_eq_right hlr] at hi3
      cases hcr : s.tightest l r with
      | none => simp [hcr] at hi3
      | some c =>
        rw [hcr, Option.bind_some] at hi3
        by_cases hok : ((Ineq.le (fl.sub fr) (Form.const c)).ok &&
            !(Ineq.le (fl.sub fr) (Form.const c)).xs.isEmpty) = true
        · rw [ite_eq_left hok] at hi3
          cases hi3
          rw [Ineq.holds_le, Form.eval_sub, Form.eval_const, ← hl', ← hr']
          exact L0.tightest_sound hs hcr
        · rw [ite_eq_right hok] at hi3
          cases hi3
  · intro j hj
    simp at hj

/-- Every candidate of the closed L0-to-affine index has an image that gives
its term's value. -/
theorem candidates_sound {env : Env} {st : State} {A : List (Nat × Int × Int)}
    {E E0 : Whitefoot.Env} {G : Nat → Int} {ν : TermVal}
    (h : (⟨env.vars, st, A⟩ : Snapshot).HoldsWith E E0 G ν) :
    ∀ c ∈ candidates env st, ν.val c.1 = c.2.eval G := by
  unfold candidates
  intro c hc
  simp only [List.mem_append, List.mem_cons] at hc
  rcases hc with ((rfl | hc) | hc) | hc
  · simp [ν.val_z, Form.eval_const]
  · exact h.data c hc
  · obtain ⟨u, hu, hcu⟩ := List.mem_filterMap.mp hc
    have hu' := (List.mem_filter.mp hu).1
    cases u with
    | sum a b =>
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
        Option.some.injEq] at hcu
      obtain ⟨fa, ha, fb, hb, rfl⟩ := hcu
      have himg : ∀ (x : Term) (fx : Form), x ∈ [a, b] →
          (match x with
            | .place id _ => st.image? id
            | .datum _ _ => (st.data.find? (·.1 == x)).map (·.2)
            | _ => none) = some fx → ν.val x = fx.eval G := by
        intro x fx hx hfx
        cases x with
        | place id ty =>
          have hat : Term.place id ty ∈ (Term.sum a b).atoms := by
            simp only [List.mem_cons, List.mem_singleton, List.not_mem_nil, or_false] at hx
            rcases hx with hxa | hxb
            · rw [← hxa]
              simp [Term.atoms]
            · rw [← hxb]
              simp [Term.atoms]
          obtain ⟨v, hv, rfl, hty⟩ := h.l0Places _ hu' id ty hat
          exact h.image_place hv hty hfx
        | datum id ty =>
          obtain ⟨p, hp, rfl⟩ := Option.map_eq_some_iff.mp hfx
          have hid : p.1 = Term.datum id ty := by simpa using List.find?_some hp
          rw [← hid]
          exact h.data p (List.mem_of_find?_eq_some hp)
        | z => cases hfx
        | commit _ _ => cases hfx
        | sum _ _ => cases hfx
      dsimp only
      rw [ν.val_sum, Form.eval_add, himg a fa (by simp) ha, himg b fb (by simp) hb]
    | z => cases hcu
    | place _ _ => cases hcu
    | commit _ _ => cases hcu
    | datum _ _ => cases hcu
  · obtain ⟨v, hv, hcv⟩ := List.mem_filterMap.mp hc
    have hv' := List.mem_reverse.mp hv
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at hcv
    obtain ⟨t, ht, f, hf, rfl⟩ := hcv
    unfold Env.term? at ht
    cases hty : v.ty with
    | int ty =>
      rw [hty] at ht
      cases ht
      exact h.image_place hv' hty hf
    | unit => rw [hty] at ht; cases ht
    | bool => rw [hty] at ht; cases ht
    | array _ _ => rw [hty] at ht; cases ht
    | named _ => rw [hty] at ht; cases ht

/-- The view an affine query reads is satisfied by the atoms' valuation. -/
theorem view_sat {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env} {G : Nat → Int}
    {ν : TermVal} (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν) :
    (view env g st).Sat G :=
  ⟨h.not_contra, fun x => h.atoms x, fun p hp => h.premises p hp,
    l0Images_sound h.l0 (candidates_sound h)⟩

/-! ## The disposition -/

/-- An L0 normalization's facts hold. -/
def FactsHold (ν : TermVal) (fs : List Bound × List Diseq) : Prop :=
  (∀ b ∈ fs.1, ν.val b.a - ν.val b.b ≤ b.c) ∧ (∀ d ∈ fs.2, ν.val d.a - ν.val d.b ≠ d.c)

theorem derivable_sound {s : L0} {ν : TermVal} {fs : List Bound × List Diseq} (hs : s.Sat ν)
    (h : derivable s fs = true) : FactsHold ν fs := by
  unfold derivable at h
  rw [Bool.and_eq_true, List.all_eq_true, List.all_eq_true] at h
  exact ⟨fun b hb => L0.bound_sound hs (h.1 b hb), fun d hd => L0.diseq_sound hs (h.2 d hd)⟩

/-- What a query's discharge gives: `P` follows from its signed goal's truth,
from any L0 normalization whose facts hold, from all its affine targets
holding (each affine target with a bridge term holds once its left side is at
most that term), from any alternative of affine targets, and from the
interval product when `prod` says it holds. -/
structure Query.Means (q : Query) (σ : Nat → Option Value) (ν : TermVal) (G : Nat → Int)
    (prod : Val → Val → IntTy → Prop) (P : Prop) : Prop where
  goal : ∀ e, q.goal = some e → e.eval σ = some (.bool true) → P
  l0 : ∀ cs, q.l0 = some cs → (∀ c ∈ cs, ∃ fs ∈ c, FactsHold ν fs) → P
  affine : ∀ ts, q.affine = some ts → (∀ t ∈ ts, t.1.Holds G) → P
  bridge : ∀ ts, q.affine = some ts → ∀ t ∈ ts, ∀ lhs r, t.2 = some (lhs, r) →
    lhs.eval G ≤ ν.val r → t.1.Holds G
  affineAny : ∀ ts ∈ q.affineAny, (∀ t ∈ ts, t.Holds G) → P
  product : ∀ a b t, q.product = some (a, b, t) → prod a b t → P
/-- A discharged goal holds [MSR-4]: each step of the disposition answers
"discharged" only when what the query means holds of the frame. -/
theorem discharged_sound {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {q : Query} {P : Prop}
    (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hq : q.Means (valueOf env.vars E) ν G
      (fun a b t => (intervalProduct (view env g st) st a b t).isSome = true) P)
    (hd : discharged env g st q = true) : P := by
  have hv := view_sat h
  have hnc := h.not_contra
  unfold discharged at hd
  simp only [Bool.or_eq_true] at hd
  rcases hd with ((((hc | hg) | hl) | ha) | hany) | hp
  · simp [hnc] at hc
  · cases hqg : q.goal with
    | none => simp [hqg] at hg
    | some e =>
      simp only [hqg, Option.any_some] at hg
      exact hq.goal e hqg (present_sound h.goals h.links hg)
  · cases hql : q.l0 with
    | none => simp [hql] at hl
    | some cs =>
      simp only [hql, Option.any_some, List.all_eq_true, List.any_eq_true] at hl
      exact hq.l0 cs hql fun c hc => by
        obtain ⟨fs, hfs, hd⟩ := hl c hc
        exact ⟨fs, hfs, derivable_sound h.l0 hd⟩
  · cases hqa : q.affine with
    | none => simp [hqa] at ha
    | some ts =>
      simp only [hqa, Option.any_some, List.all_eq_true] at ha
      refine hq.affine ts hqa fun t ht => ?_
      have hx := ha t ht
      obtain ⟨tt, br⟩ := t
      dsimp only at hx ⊢
      simp only [Bool.or_eq_true] at hx
      rcases hx with (hx | hx) | hx
      · exact Aff.auto_sound hv hx
      · -- the affine-left / L0-right bridge
        unfold discharged.bridge at hx
        split at hx
        · cases hx
        · rename_i lhs r
          obtain ⟨⟨m, fm⟩, hm, hx⟩ := List.any_eq_true.mp hx
          simp only [Bool.and_eq_true] at hx
          obtain ⟨-, hx⟩ := hx
          split at hx
          · rename_i c hcm
            have hres := Aff.auto_sound hv hx
            rw [Ineq.holds_le, Form.eval_sub, Form.eval_const] at hres
            have hl0 := L0.tightest_sound h.l0 hcm
            -- the candidate's image gives its place's value
            obtain ⟨v, hv', hmv⟩ := List.mem_filterMap.mp hm
            simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
              Option.some.injEq] at hmv
            obtain ⟨tm, htm, fm', hfm, hpair⟩ := hmv
            cases hpair
            have hvv := List.mem_reverse.mp hv'
            unfold Env.term? at htm
            have hmval : ν.val m = fm.eval G := by
              cases hty : v.ty with
              | int ty =>
                rw [hty] at htm
                cases htm
                exact h.image_place hvv hty hfm
              | unit => rw [hty] at htm; cases htm
              | bool => rw [hty] at htm; cases htm
              | array _ _ => rw [hty] at htm; cases htm
              | named _ => rw [hty] at htm; cases htm
            exact hq.bridge ts hqa (tt, some (lhs, r)) ht lhs r rfl (by omega)
          · cases hx
      · -- the quotient bound
        unfold discharged.quotient at hx
        split at hx
        · rename_i qa hxs
          obtain ⟨d, hdm, hd2⟩ := List.any_eq_true.mp hx
          simp only [Bool.and_eq_true, beq_iff_eq] at hd2
          obtain ⟨hq2, hauto⟩ := hd2
          have hres := Aff.auto_sound hv hauto
          rw [Ineq.holds_le, Form.eval_sub, Form.eval_scale, Form.eval_const] at hres
          obtain ⟨hn, hdpos, hqv⟩ := h.quotients d hdm
          show evalCoeffs G tt.xs ≤ tt.c
          rw [hxs]
          simp only [evalCoeffs]
          rw [← hq2]
          have hq1 : G d.q * d.divisor.eval G ≤ d.dividend.eval G := by
            rw [hqv]
            exact Int.ediv_mul_le _ (by omega)
          have hlt : G d.q * d.divisor.eval G < (tt.c + 1) * d.divisor.eval G := by
            omega
          have := Int.lt_of_mul_lt_mul_right hlt (by omega)
          omega
        · cases hx
  · obtain ⟨ts, hts, hall⟩ := List.any_eq_true.mp hany
    rw [List.all_eq_true] at hall
    exact hq.affineAny ts hts fun t ht => Aff.auto_sound hv (hall t ht)
  · cases hqp : q.product with
    | none => simp [hqp] at hp
    | some abt =>
      obtain ⟨a, b, t⟩ := abt
      simp only [hqp, Option.any_some] at hp
      exact hq.product a b t hqp hp

/-! ## Linear forms over names -/

/-- A coefficient list's value when each name `x` has the value `val x`. -/
def linSum (val : String → Int) : List (String × Int) → Int
  | [] => 0
  | p :: rest => p.2 * val p.1 + linSum val rest

/-- A linear form's value. -/
def Lin.eval (val : String → Int) (l : Lin) : Int := l.1 + linSum val l.2

/-- An affine expression's value when each name `x` has the value `val x`; an
atom that is neither a literal nor a name counts zero, as it has no linear
form. -/
def affVal (val : String → Int) : Affine → Int
  | .atom (.int _ v) => v
  | .atom (.var x) => val x
  | .atom _ => 0
  | .add a b => affVal val a + affVal val b
  | .sub a b => affVal val a - affVal val b
  | .mul a b => affVal val a * affVal val b

theorem linSum_append (val : String → Int) :
    ∀ l l' : List (String × Int), linSum val (l ++ l') = linSum val l + linSum val l'
  | [], l' => by simp [linSum]
  | p :: rest, l' => by
    rw [List.cons_append]
    simp only [linSum]
    rw [linSum_append val rest l']
    omega

theorem map_eq_self {α : Type} {f : α → α} : ∀ {l : List α}, (∀ q ∈ l, f q = q) → l.map f = l
  | [], _ => rfl
  | p :: rest, h => by
    rw [List.map_cons, h p List.mem_cons_self,
      map_eq_self (fun q hq => h q (List.mem_cons_of_mem _ hq))]

theorem names_map {f : String × Int → String × Int} (hf : ∀ p, (f p).1 = p.1)
    (l : List (String × Int)) : (l.map f).map (·.1) = l.map (·.1) := by
  rw [List.map_map]
  exact List.map_congr_left (fun q _ => hf q)

theorem foldl_linSum (val : String → Int)
    (s : List (String × Int) → String × Int → List (String × Int))
    (hs : ∀ acc p, (acc.map (·.1)).Nodup →
      ((s acc p).map (·.1)).Nodup ∧ linSum val (s acc p) = linSum val acc + p.2 * val p.1) :
    ∀ (l acc : List (String × Int)), (acc.map (·.1)).Nodup →
      ((l.foldl s acc).map (·.1)).Nodup ∧
        linSum val (l.foldl s acc) = linSum val acc + linSum val l
  | [], acc, h => ⟨h, by simp [linSum]⟩
  | p :: rest, acc, h => by
    rw [List.foldl_cons]
    obtain ⟨h1, h2⟩ := hs acc p h
    obtain ⟨h3, h4⟩ := foldl_linSum val s hs rest (s acc p) h1
    refine ⟨h3, ?_⟩
    rw [h4, h2]
    simp only [linSum]
    omega

theorem foldl_lin (val : String → Int)
    (s : List (String × Int) → String × Int → List (String × Int))
    (hs : ∀ acc p, (acc.map (·.1)).Nodup →
      ((s acc p).map (·.1)).Nodup ∧ linSum val (s acc p) = linSum val acc + p.2 * val p.1)
    (a b : Lin) (ha : (a.2.map (·.1)).Nodup) :
    (((a.1 + b.1, b.2.foldl s a.2) : Lin).2.map (·.1)).Nodup ∧
      Lin.eval val (a.1 + b.1, b.2.foldl s a.2) = Lin.eval val a + Lin.eval val b := by
  obtain ⟨h1, h2⟩ := foldl_linSum val s hs b.2 a.2 ha
  refine ⟨h1, ?_⟩
  unfold Lin.eval
  dsimp only
  rw [h2]
  omega

/-- Adding `k` to the coefficient of a name the list carries once adds
`k * val x` to its value. -/
theorem linSum_bump (val : String → Int) (x : String) (k : Int)
    (f : String × Int → String × Int) (hf : ∀ p, f p = if p.1 = x then (p.1, p.2 + k) else p) :
    ∀ l : List (String × Int), (l.map (·.1)).Nodup → x ∈ l.map (·.1) →
      ((l.map f).map (·.1)).Nodup ∧ linSum val (l.map f) = linSum val l + k * val x
  | [], _, hx => by simp at hx
  | p :: rest, hnd, hx => by
    have hfst : ∀ q, (f q).1 = q.1 := by
      intro q
      rw [hf q]
      split <;> rfl
    rw [List.map_cons, List.nodup_cons] at hnd
    refine ⟨by rw [names_map hfst]; exact List.nodup_cons.mpr hnd, ?_⟩
    rw [List.map_cons, hf p]
    by_cases hp : p.1 = x
    · have hrest : rest.map f = rest := by
        apply map_eq_self
        intro q hq
        have hqx : ¬ q.1 = x := by
          intro hqx
          exact hnd.1 (by rw [hp, ← hqx]; exact List.mem_map_of_mem hq)
        rw [hf q, ite_eq_right hqx]
      rw [ite_eq_left hp, hrest]
      simp only [linSum]
      show (p.2 + k) * val p.1 + linSum val rest = p.2 * val p.1 + linSum val rest + k * val x
      rw [hp, Int.add_mul]
      omega
    · have hx' : x ∈ rest.map (·.1) := by
        rw [List.map_cons, List.mem_cons] at hx
        rcases hx with hx | hx
        · exact absurd hx.symm hp
        · exact hx
      rw [ite_eq_right hp]
      simp only [linSum]
      rw [(linSum_bump val x k f hf rest hnd.2 hx').2]
      omega

/-- `linAdd` keeps names distinct and adds values [INV-1]. -/
theorem linAdd_sound (val : String → Int) (a b : Lin) (ha : (a.2.map (·.1)).Nodup) :
    ((linAdd a b).2.map (·.1)).Nodup ∧
      Lin.eval val (linAdd a b) = Lin.eval val a + Lin.eval val b := by
  unfold linAdd
  refine foldl_lin val _ ?_ a b ha
  rintro acc ⟨x, k⟩ hnd
  dsimp only
  split
  · rename_i q hq
    have hx : x ∈ acc.map (·.1) := by
      have h1 : q.1 = x := by simpa using List.find?_some hq
      rw [← h1]
      exact List.mem_map_of_mem (List.mem_of_find?_eq_some hq)
    refine linSum_bump val x k _ ?_ acc hnd hx
    rintro ⟨y, j⟩
    by_cases hy : y = x
    · subst hy
      simp
    · simp [hy]
  · rename_i hq
    have hx : x ∉ acc.map (·.1) := by
      intro hm
      obtain ⟨q, hq1, rfl⟩ := List.mem_map.mp hm
      have := List.find?_eq_none.mp hq q hq1
      simp at this
    refine ⟨?_, ?_⟩
    · rw [List.map_append, List.nodup_append]
      refine ⟨hnd, by simp, ?_⟩
      intro a1 ha1 b1 hb1 he
      simp only [List.map_cons, List.map_nil, List.mem_singleton] at hb1
      rw [hb1] at he
      exact hx (he ▸ ha1)
    · rw [linSum_append]
      simp only [linSum]
      omega

theorem linSum_scale (val : String → Int) (c : Int) (f : String × Int → String × Int)
    (hf : ∀ p, f p = (p.1, c * p.2)) :
    ∀ l : List (String × Int), linSum val (l.map f) = c * linSum val l
  | [] => by simp [linSum]
  | p :: rest => by
    rw [List.map_cons, hf p]
    simp only [linSum]
    show c * p.2 * val p.1 + linSum val (rest.map f) = c * (p.2 * val p.1 + linSum val rest)
    rw [linSum_scale val c f hf rest, Int.mul_add, Int.mul_assoc]

theorem linScale_eval (val : String → Int) (c : Int) (l : Lin) :
    Lin.eval val (linScale c l) = c * Lin.eval val l := by
  unfold linScale Lin.eval
  dsimp only
  rw [linSum_scale val c _ ?_ l.2, Int.mul_add]
  intro p
  obtain ⟨x, j⟩ := p
  rfl

theorem linScale_nodup (c : Int) (l : Lin) (hl : (l.2.map (·.1)).Nodup) :
    ((linScale c l).2.map (·.1)).Nodup := by
  unfold linScale
  dsimp only
  rw [names_map ?_ l.2]
  · exact hl
  · intro p
    obtain ⟨x, j⟩ := p
    rfl

theorem linSum_zero (val : String → Int) :
    ∀ l : List (String × Int), l.all (·.2 == 0) = true → linSum val l = 0
  | [], _ => rfl
  | p :: rest, h => by
    rw [List.all_cons, Bool.and_eq_true] at h
    have hp : p.2 = 0 := by simpa using h.1
    simp only [linSum, hp, Int.zero_mul, Int.zero_add]
    exact linSum_zero val rest h.2

theorem linSum_clean (val : String → Int) :
    ∀ l : List (String × Int), linSum val (l.filter (·.2 != 0)) = linSum val l
  | [] => rfl
  | p :: rest => by
    by_cases hp : p.2 = 0
    · rw [List.filter_cons_of_neg (by simp [hp])]
      simp only [linSum, hp, Int.zero_mul, Int.zero_add]
      exact linSum_clean val rest
    · rw [List.filter_cons_of_pos (by simp [hp])]
      simp only [linSum, linSum_clean val rest]

theorem Lin.eval_clean (val : String → Int) (l : Lin) :
    Lin.eval val l.clean = Lin.eval val l := by
  unfold Lin.clean Lin.eval
  dsimp only
  rw [linSum_clean]

/-- An affine expression's linear form carries each name once and has the
expression's value [INV-1]. -/
theorem lin_sound (val : String → Int) :
    ∀ (a : Affine) (l : Lin), lin a = some l →
      (l.2.map (·.1)).Nodup ∧ Lin.eval val l = affVal val a
  | .atom (.int _ v), l, h => by
    simp only [lin, Option.some.injEq] at h
    subst h
    simp [Lin.eval, linSum, affVal]
  | .atom (.var x), l, h => by
    simp only [lin, Option.some.injEq] at h
    subst h
    simp [Lin.eval, linSum, affVal]
  | .atom .unit, l, h => by simp [lin] at h
  | .atom (.index _ _), l, h => by simp [lin] at h
  | .add a b, l, h => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨la, ha, lb, hb, hl⟩ := h
    subst hl
    obtain ⟨ha1, ha2⟩ := lin_sound val a la ha
    obtain ⟨-, hb2⟩ := lin_sound val b lb hb
    obtain ⟨h1, h2⟩ := linAdd_sound val la lb ha1
    refine ⟨h1, ?_⟩
    rw [h2, ha2, hb2]
    rfl
  | .sub a b, l, h => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨la, ha, lb, hb, hl⟩ := h
    subst hl
    obtain ⟨ha1, ha2⟩ := lin_sound val a la ha
    obtain ⟨-, hb2⟩ := lin_sound val b lb hb
    obtain ⟨h1, h2⟩ := linAdd_sound val la (linScale (-1) lb) ha1
    refine ⟨h1, ?_⟩
    rw [h2, linScale_eval, ha2, hb2]
    show affVal val a + -1 * affVal val b = affVal val a - affVal val b
    omega
  | .mul a b, l, h => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨la, ha, lb, hb, h⟩ := h
    obtain ⟨ha1, ha2⟩ := lin_sound val a la ha
    obtain ⟨hb1, hb2⟩ := lin_sound val b lb hb
    split at h
    · rename_i hc
      simp only [Option.some.injEq] at h
      subst h
      refine ⟨linScale_nodup la.1 lb hb1, ?_⟩
      have hz : Lin.eval val la = la.1 := by
        unfold Lin.eval
        rw [linSum_zero val la.2 hc, Int.add_zero]
      rw [linScale_eval, hb2]
      show la.1 * affVal val b = affVal val a * affVal val b
      rw [← ha2, hz]
    · split at h
      · rename_i _ hc
        simp only [Option.some.injEq] at h
        subst h
        refine ⟨linScale_nodup lb.1 la ha1, ?_⟩
        have hz : Lin.eval val lb = lb.1 := by
          unfold Lin.eval
          rw [linSum_zero val lb.2 hc, Int.add_zero]
        rw [linScale_eval, ha2]
        show lb.1 * affVal val a = affVal val a * affVal val b
        rw [← hb2, hz, Int.mul_comm]
      · simp at h

/-! ## Relation queries -/

/-- A comparison depends only on the difference of its sides. -/
theorem cmp_shift {op : CmpOp} {a b c d : Int} (h : a - b = c - d)
    (h1 : op.eval c d = true) : op.eval a b = true := by
  cases op <;> simp only [CmpOp.eval, decide_eq_true_eq] at h1 ⊢ <;> omega

/-- A relation's L0 facts give the relation [ENT-2]. -/
theorem relFacts_sound {ν : TermVal} {op : CmpOp} {ta tb : Term} {ka kb : Int}
    (h : FactsHold ν (relFacts op (ta, ka) (tb, kb))) :
    op.eval (ν.val ta + ka) (ν.val tb + kb) = true := by
  obtain ⟨hb, hd⟩ := h
  cases op with
  | eq =>
    have h1 : ν.val ta - ν.val tb ≤ kb - ka := hb _ List.mem_cons_self
    have h2 : ν.val tb - ν.val ta ≤ ka - kb := hb _ (List.mem_cons_of_mem _ List.mem_cons_self)
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | ne =>
    have h1 : ν.val ta - ν.val tb ≠ kb - ka := hd _ List.mem_cons_self
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | lt =>
    have h1 : ν.val ta - ν.val tb ≤ kb - ka - 1 := hb _ List.mem_cons_self
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | le =>
    have h1 : ν.val ta - ν.val tb ≤ kb - ka := hb _ List.mem_cons_self
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | gt =>
    have h1 : ν.val tb - ν.val ta ≤ ka - kb - 1 := hb _ List.mem_cons_self
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | ge =>
    have h1 : ν.val tb - ν.val ta ≤ ka - kb := hb _ List.mem_cons_self
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega

/-- A difference bound's L0 facts give the relation between the two sides'
values [ENT-3.S16]. -/
theorem diffFacts_sound {ν : TermVal} {val : String → Int} {op : CmpOp} {la lb : Lin}
    {looks : String → Option (Term × Int)} {fs : List Bound × List Diseq}
    (hla : (la.2.map (·.1)).Nodup)
    (hs : ∀ x t k, looks x = some (t, k) → ν.val t + k = val x)
    (h : diffFacts op la lb looks = some fs) (hf : FactsHold ν fs) :
    op.eval (Lin.eval val la) (Lin.eval val lb) = true := by
  have hD : Lin.eval val (linAdd la (linScale (-1) lb)).clean =
      Lin.eval val la - Lin.eval val lb := by
    rw [Lin.eval_clean, (linAdd_sound val la (linScale (-1) lb) hla).2, linScale_eval]
    omega
  unfold diffFacts at h
  dsimp only at h
  revert h hD
  generalize (linAdd la (linScale (-1) lb)).clean = d
  intro h hD
  obtain ⟨c, l⟩ := d
  rcases l with _ | ⟨⟨x, a⟩, _ | ⟨⟨y, b⟩, _ | ⟨z, rest⟩⟩⟩
  · simp only [Option.some.injEq] at h
    subst h
    have e : Lin.eval val (c, []) = c := by simp [Lin.eval, linSum]
    rw [e] at hD
    have h1 := relFacts_sound hf
    rw [ν.val_z] at h1
    exact cmp_shift (by omega) h1
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨⟨t, k⟩, hl, h⟩ := h
    have hv := hs x t k hl
    have e : Lin.eval val (c, [(x, a)]) = c + a * val x := by simp [Lin.eval, linSum]
    rw [e] at hD
    dsimp only at h
    split at h
    · rename_i ha
      simp only [Option.pure_def, Option.some.injEq] at h
      subst h
      have ha1 : a = 1 := by simpa using ha
      subst ha1
      have h1 := relFacts_sound hf
      rw [ν.val_z] at h1
      exact cmp_shift (by omega) h1
    · split at h
      · rename_i ha
        simp only [Option.pure_def, Option.some.injEq] at h
        subst h
        have ha1 : a = -1 := by simpa using ha
        subst ha1
        have h1 := relFacts_sound hf
        rw [ν.val_z] at h1
        exact cmp_shift (by omega) h1
      · simp at h
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨⟨t, k⟩, hl, h⟩ := h
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨⟨u, j⟩, hl2, h⟩ := h
    have hv := hs x t k hl
    have hw := hs y u j hl2
    have e : Lin.eval val (c, [(x, a), (y, b)]) = c + a * val x + b * val y := by
      unfold Lin.eval
      simp only [linSum]
      omega
    rw [e] at hD
    dsimp only at h
    split at h
    · rename_i hab
      simp only [Option.pure_def, Option.some.injEq] at h
      subst h
      simp only [Bool.and_eq_true, beq_iff_eq] at hab
      obtain ⟨rfl, rfl⟩ := hab
      have h1 := relFacts_sound hf
      exact cmp_shift (by omega) h1
    · split at h
      · rename_i hab
        simp only [Option.pure_def, Option.some.injEq] at h
        subst h
        simp only [Bool.and_eq_true, beq_iff_eq] at hab
        obtain ⟨rfl, rfl⟩ := hab
        have h1 := relFacts_sound hf
        exact cmp_shift (by omega) h1
      · simp at h
  · simp at h

/-- A side's term operand gives the side's value. -/
theorem linTerm_sound {ν : TermVal} {val : String → Int} {looks : String → Option (Term × Int)}
    (hs : ∀ x t k, looks x = some (t, k) → ν.val t + k = val x) {l : Lin} {t : Term} {k : Int}
    (h : linTerm looks l = some (t, k)) : ν.val t + k = Lin.eval val l := by
  rw [← Lin.eval_clean]
  unfold linTerm at h
  split at h
  · rename_i hc
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    unfold Lin.eval
    rw [hc]
    simp [linSum, Lin.clean, ν.val_z]
  · rename_i x hc
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨⟨t', k'⟩, hl, h⟩ := h
    have hv := hs x _ _ hl
    simp only [Option.pure_def, Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    unfold Lin.eval
    rw [hc]
    simp only [linSum, Lin.clean]
    omega
  · simp at h

theorem foldlM_image {G : Nat → Int} {val : String → Int}
    (s : Form → String × Int → Option Form)
    (hs : ∀ acc p F, s acc p = some F → F.eval G = acc.eval G + p.2 * val p.1) :
    ∀ (l : List (String × Int)) (acc F : Form), l.foldlM s acc = some F →
      F.eval G = acc.eval G + linSum val l
  | [], acc, F, h => by
    rw [List.foldlM_nil] at h
    simp only [Option.pure_def, Option.some.injEq] at h
    subst h
    simp [linSum]
  | p :: rest, acc, F, h => by
    rw [List.foldlM_cons] at h
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨acc', h1, h2⟩ := h
    rw [foldlM_image s hs rest acc' F h2, hs acc p acc' h1]
    simp only [linSum]
    omega

/-- A linear form's image gives the form's value when each operand's image
gives the operand's value. -/
theorem linImage_sound {G : Nat → Int} {val : String → Int} {looki : String → Option Form}
    (hi : ∀ x f, looki x = some f → f.eval G = val x) {l : Lin} {F : Form}
    (h : linImage looki l = some F) : F.eval G = Lin.eval val l := by
  unfold linImage at h
  have key := foldlM_image (G := G) (val := val) _ ?_ l.2 (Form.const l.1) F h
  · unfold Lin.eval
    rw [key, Form.eval_const]
  · rintro acc ⟨x, k⟩ F' hp
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at hp
    obtain ⟨f, hf, rfl⟩ := hp
    rw [Form.eval_add, Form.eval_scale, hi x f hf]

/-- The bridge of a `<=` component holds the component once its left side is
at most its term. -/
theorem le_bridge {ν : TermVal} {G : Nat → Int} {L R : Form} {tR : Option (Term × Int)}
    (hR : ∀ t k, tR = some (t, k) → ν.val t + k = R.eval G) :
    ∀ c ∈ (bridges .le L R none tR).getD [], ∀ lhs r, c.2 = some (lhs, r) →
      lhs.eval G ≤ ν.val r → c.1.Holds G := by
  intro c hc lhs r h2 hle
  simp only [bridges, Option.getD_some, List.mem_singleton] at hc
  subst hc
  rcases tR with _ | ⟨t, k⟩
  · simp at h2
  · dsimp only at h2
    split at h2
    · cases h2
    · obtain ⟨rfl, rfl⟩ := Prod.mk.inj (Option.some.inj h2)
      show (Ineq.le L R).Holds G
      rw [Ineq.holds_le]
      rw [Form.eval_sub, Form.eval_const] at hle
      have := hR _ _ rfl
      omega

/-- A relation's affine components give the relation, and each bridge holds
its component [ENT-6]. -/
theorem bridges_spec {ν : TermVal} {G : Nat → Int} {op : CmpOp} {a b : Form}
    {ta tb : Option (Term × Int)} {ts : List (Ineq × Option (Form × Term))}
    (hta : ∀ t k, ta = some (t, k) → ν.val t + k = a.eval G)
    (htb : ∀ t k, tb = some (t, k) → ν.val t + k = b.eval G)
    (h : bridges op a b ta tb = some ts) :
    ((∀ t ∈ ts, t.1.Holds G) → op.eval (a.eval G) (b.eval G) = true) ∧
    ∀ t ∈ ts, ∀ lhs r, t.2 = some (lhs, r) → lhs.eval G ≤ ν.val r → t.1.Holds G := by
  cases op with
  | le =>
    obtain rfl := Option.some.inj h
    refine ⟨fun hall => ?_, fun t ht => le_bridge (L := a) htb t ht⟩
    have h1 := (Ineq.holds_le G a b).mp (hall _ List.mem_cons_self)
    simp only [CmpOp.eval, decide_eq_true_eq]
    exact h1
  | lt =>
    obtain rfl := Option.some.inj h
    refine ⟨fun hall => ?_, fun t ht => le_bridge (L := a.add (Form.const 1)) htb t ht⟩
    have h1 := (Ineq.holds_le G _ b).mp (hall _ List.mem_cons_self)
    rw [Form.eval_add, Form.eval_const] at h1
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | ge =>
    obtain rfl := Option.some.inj h
    refine ⟨fun hall => ?_, fun t ht => le_bridge (L := b) hta t ht⟩
    have h1 := (Ineq.holds_le G b a).mp (hall _ List.mem_cons_self)
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | gt =>
    obtain rfl := Option.some.inj h
    refine ⟨fun hall => ?_, fun t ht => le_bridge (L := b.add (Form.const 1)) hta t ht⟩
    have h1 := (Ineq.holds_le G _ a).mp (hall _ List.mem_cons_self)
    rw [Form.eval_add, Form.eval_const] at h1
    simp only [CmpOp.eval, decide_eq_true_eq]
    omega
  | eq =>
    obtain rfl := Option.some.inj h
    refine ⟨fun hall => ?_, fun t ht => ?_⟩
    · have h1 := (Ineq.holds_le G a b).mp (hall _ List.mem_cons_self)
      have h2 := (Ineq.holds_le G b a).mp
        (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
      simp only [CmpOp.eval, decide_eq_true_eq]
      omega
    · rcases List.mem_cons.mp ht with ht | ht
      · exact le_bridge (L := a) htb t (by rw [ht]; exact List.mem_cons_self)
      · exact le_bridge (L := b) hta t ht
  | ne => simp [bridges] at h

/-- What a relation query means: its relation over the names' values, given
operand terms and images that give those values and a goal whose truth
gives the relation [MSR-4]. -/
theorem relQuery_means {goal : Option GExpr} {r : Relation}
    {looks : String → Option (Term × Int)} {looki : String → Option Form}
    {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int} {prod : Val → Val → IntTy → Prop}
    {val : String → Int}
    (hs : ∀ x t k, looks x = some (t, k) → ν.val t + k = val x)
    (hi : ∀ x f, looki x = some f → f.eval G = val x)
    (hg : ∀ e, goal = some e → e.eval σ = some (.bool true) →
      r.op.eval (affVal val r.lhs) (affVal val r.rhs) = true) :
    (relQuery goal r looks looki).Means σ ν G prod
      (r.op.eval (affVal val r.lhs) (affVal val r.rhs) = true) := by
  unfold relQuery
  split
  · rename_i la lb hla hlb
    obtain ⟨hna, hva⟩ := lin_sound val r.lhs la hla
    obtain ⟨-, hvb⟩ := lin_sound val r.rhs lb hlb
    rw [← hva, ← hvb] at hg ⊢
    have hta : ∀ fa, linImage looki la = some fa →
        ∀ t k, linTerm looks la = some (t, k) → ν.val t + k = fa.eval G :=
      fun fa hfa t k h => (linTerm_sound hs h).trans (linImage_sound hi hfa).symm
    have htb : ∀ fb, linImage looki lb = some fb →
        ∀ t k, linTerm looks lb = some (t, k) → ν.val t + k = fb.eval G :=
      fun fb hfb t k h => (linTerm_sound hs h).trans (linImage_sound hi hfb).symm
    refine ⟨hg, ?_, ?_, ?_, ?_, ?_⟩
    · intro cs hcs hall
      obtain ⟨fs, hfs, hcs⟩ := Option.map_eq_some_iff.mp hcs
      subst hcs
      obtain ⟨fs', hfs', hfact⟩ := hall _ List.mem_cons_self
      simp only [List.mem_singleton] at hfs'
      subst hfs'
      exact diffFacts_sound hna hs hfs hfact
    · intro ts hts hall
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at hts
      obtain ⟨fa, hfa, fb, hfb, hb⟩ := hts
      have h1 := (bridges_spec (hta fa hfa) (htb fb hfb) hb).1 hall
      rw [linImage_sound hi hfa, linImage_sound hi hfb] at h1
      exact h1
    · intro ts hts t ht lhs r' h2 hle
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at hts
      obtain ⟨fa, hfa, fb, hfb, hb⟩ := hts
      exact (bridges_spec (hta fa hfa) (htb fb hfb) hb).2 t ht lhs r' h2 hle
    · intro ts hts
      simp at hts
    · intro a b t hp
      simp at hp
  · refine ⟨hg, ?_, ?_, ?_, ?_, ?_⟩
    · intro cs hcs
      simp at hcs
    · intro ts hts
      simp at hts
    · intro ts hts
      simp at hts
    · intro ts hts
      simp at hts
    · intro a b t hp
      simp at hp

/-! ## Atoms: subscripts are proved below their lengths [OP-4] -/

/-- How the interpreter reads `b[off]` from an array binding: the offset's
failure, a `u64` offset compared with the length, or an ill-formed offset. -/
theorem evalAtom_index_cases {E : Whitefoot.Env} {b : String} {off : Atom} {t : IntTy}
    {es : List Int} (hb : E.get b = .ok (.array t es)) :
    (∃ e, Whitefoot.evalAtom E off = .error e ∧
      Whitefoot.evalAtom E (.index b off) = .error e) ∨
    (∃ k, Whitefoot.evalAtom E off = .ok (.int .u64 k) ∧
      Whitefoot.evalAtom E (.index b off) =
        if k < es.length then .ok (.int t (es.getD k.toNat 0)) else stuck .bounds) ∨
    (∃ w, Whitefoot.evalAtom E off = .ok w ∧ (∀ k, w ≠ .int .u64 k) ∧
      Whitefoot.evalAtom E (.index b off) = ill "a subscript offset is not a u64") := by
  cases ho : Whitefoot.evalAtom E off with
  | error e =>
    left
    refine ⟨e, rfl, ?_⟩
    simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind, Discharge.error_bind]
  | ok w =>
    right
    rcases w with ⟨t', k⟩ | bb | _ | ⟨t', es'⟩ | c
    · cases t' with
      | u64 =>
        left
        exact ⟨k, rfl, by simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind]⟩
      | _ =>
        right
        exact ⟨_, rfl, fun k' hk => by cases hk,
          by simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind]⟩
    all_goals
      right
      exact ⟨_, rfl, fun k' hk => by cases hk,
        by simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind]⟩

/-- Evaluating an atom in a state whose facts hold: the interpreter's reading
of the atom never sticks (a subscript is below its length), the facts keep
holding with the atom's new registrations, and the checker's value describes
the runtime value [OP-4, MSR-4]. -/
theorem evalAtom_sound {env : Env} {st : State} {E E0 : Whitefoot.Env} {ν : TermVal} :
    ∀ (a : Atom) {g g1 : Gen} {v : Val} {G : Nat → Int}, Good env st g E E0 G ν → a.LitOk →
      (Checker.evalAtom env st a).run g = .ok (v, g1) →
      (∀ k, Whitefoot.evalAtom E a ≠ .error (.stuck k)) ∧
      ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧ v.Below g1.next ∧
        ∀ w, Whitefoot.evalAtom E a = .ok w → v.Denotes (valueOf env.vars E) ν G1 w
  | .int t z, g, g1, v, G, hg, hlit, h => by
    simp only [Checker.evalAtom] at h
    obtain ⟨hv, hg1⟩ := Discharge.run_pure h
    subst hv
    rw [hg1]
    refine ⟨fun k hk => by simp [Whitefoot.evalAtom] at hk, G, hg, Ext.refl g G, ?_, ?_⟩
    · intro f hf x hx
      simp only [Option.some.injEq] at hf
      subst hf
      simp [Form.atomIds, Form.const] at hx
    · intro w hw
      simp only [Whitefoot.evalAtom, Except.ok.injEq] at hw
      subst hw
      refine ⟨fun x k hk => ?_, fun f hf => ?_, fun e he => ?_, ⟨rfl, hlit⟩⟩
      · simp only [Option.some.injEq, Prod.mk.injEq] at hk
        obtain ⟨rfl, rfl⟩ := hk
        exact ⟨t, by rw [ν.val_z, Int.zero_add]⟩
      · simp only [Option.some.injEq] at hf
        subst hf
        exact ⟨t, by rw [Form.eval_const]⟩
      · simp only [Option.some.injEq] at he
        subst he
        rfl
  | .unit, g, g1, v, G, hg, _, h => by
    simp only [Checker.evalAtom] at h
    obtain ⟨hv, hg1⟩ := Discharge.run_pure h
    subst hv
    rw [hg1]
    refine ⟨fun k hk => by simp [Whitefoot.evalAtom] at hk, G, hg, Ext.refl g G,
      fun f hf => by simp at hf, ?_⟩
    intro w hw
    simp only [Whitefoot.evalAtom, Except.ok.injEq] at hw
    subst hw
    refine ⟨fun x k hk => by simp at hk, fun f hf => by simp at hf, fun e he => ?_, trivial⟩
    simp only [Option.some.injEq] at he
    subst he
    rfl
  | .var x, g, g1, v, G, hg, _, h => by
    cases hv : env.var? x with
    | none =>
      simp only [Checker.evalAtom, hv] at h
      exact (Discharge.run_fail h).elim
    | some vr =>
      obtain ⟨w, hw, hwt, hval⟩ := Env.Fits.get hg.holds.fits hg.holds.ids hv
      have hval' : valueOf env.vars E vr.id = some w := hval
      have hvr : vr ∈ env.vars := List.mem_of_find?_eq_some hv
      have hrun : Whitefoot.evalAtom E (.var x) = .ok w := hw
      have hnost : ∀ k, Whitefoot.evalAtom E (.var x) ≠ .error (.stuck k) := by
        intro k hk
        rw [hrun] at hk
        cases hk
      cases hty : vr.ty with
      | int t =>
        simp only [Checker.evalAtom, hv, hty] at h
        obtain ⟨hv', hg1⟩ := Discharge.run_pure h
        subst hv'
        rw [hg1]
        refine ⟨hnost, G, hg, Ext.refl g G, ?_, ?_⟩
        · intro f hf y hy
          obtain ⟨p, hp, hpf⟩ := Option.map_eq_some_iff.mp hf
          subst hpf
          exact (hg.fresh.images p (List.mem_of_find?_eq_some hp)).2 y hy
        · intro w' hw'
          rw [hrun] at hw'
          cases hw'
          have hpl : valueOf env.vars E vr.id = some (.int t (ν.val (.place vr.id t))) :=
            hg.holds.places vr hvr t hty
          rw [hval'] at hpl
          refine ⟨?_, ?_, ?_, ?_⟩
          · intro y k hk
            simp only [Option.some.injEq, Prod.mk.injEq] at hk
            obtain ⟨rfl, rfl⟩ := hk
            exact ⟨t, by rw [Option.some.inj hpl, Int.add_zero]⟩
          · intro f hf
            obtain ⟨p, hp, hpf⟩ := Option.map_eq_some_iff.mp hf
            subst hpf
            have hid : p.1 = vr.id := by simpa using List.find?_some hp
            obtain ⟨t', ht'⟩ := hg.holds.images p (List.mem_of_find?_eq_some hp)
            have ht'' : valueOf env.vars E p.1 = some (.int t' (p.2.eval G)) := ht'
            rw [hid, hval'] at ht''
            exact ⟨t', Option.some.inj ht''⟩
          · intro e he
            simp only [Option.some.injEq] at he
            subst he
            exact hval'
          · rw [hty] at hwt
            exact hwt
      | _ =>
        simp only [Checker.evalAtom, hv, hty] at h
        obtain ⟨hv', hg1⟩ := Discharge.run_pure h
        subst hv'
        rw [hg1]
        refine ⟨hnost, G, hg, Ext.refl g G, fun f hf => by simp at hf, ?_⟩
        intro w' hw'
        rw [hrun] at hw'
        cases hw'
        refine ⟨fun y k hk => by simp at hk, fun f hf => by simp at hf, ?_, ?_⟩
        · intro e he
          simp only [Option.some.injEq] at he
          subst he
          exact hval'
        · rw [hty] at hwt
          exact hwt
  | .index b off, g, g1, v, G, hg, hlit, h => by
    cases hv : env.var? b with
    | none =>
      simp only [Checker.evalAtom, hv] at h
      exact (Discharge.run_fail h).elim
    | some vb =>
      simp only [Checker.evalAtom, hv] at h
      obtain ⟨o, g2, ho, h⟩ := Discharge.run_bind h
      obtain ⟨hnost, G2, hg2, hext2, -, hden⟩ := evalAtom_sound off hg hlit ho
      cases hty : vb.ty with
      | array t n =>
        simp only [hty] at h
        obtain ⟨g3, g4, hget, h⟩ := Discharge.run_bind h
        obtain ⟨h3, h4⟩ := Discharge.run_get hget
        rw [h3, h4] at h
        split at h
        · exfalso
          first
            | exact Discharge.run_fail h
            | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind h; exact Discharge.run_fail hf)
        · rename_i hnd
          have hdis := Discharge.not_not_true hnd
          have h' : (Checker.typeAtom t >>= fun img =>
              (pure ({ image := some img, gexpr := o.gexpr.map (GExpr.index vb.id ·),
                ty := .int t } : Val) : Checker.M Val)).run g2 = .ok (v, g1) := h
          obtain ⟨img, g6, himg, h⟩ := Discharge.run_bind h'
          rw [Discharge.typeAtom_run] at himg
          simp only [Except.ok.injEq, Prod.mk.injEq] at himg
          obtain ⟨rfl, rfl⟩ := himg
          obtain ⟨hv', hg1'⟩ := Discharge.run_pure h
          subst hv'
          subst hg1'
          -- the subscript is below the length whenever the offset evaluates
          have hP : ∀ w, Whitefoot.evalAtom E off = .ok w → ∀ t' z, w = .int t' z →
              z ≤ (n : Int) - 1 := by
            refine discharged_sound hg2.holds ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ hdis
            · intro e he
              simp at he
            · intro cs hcs hall w hw t' z hwz
              obtain ⟨⟨x, k⟩, hxk, hcs⟩ := Option.map_eq_some_iff.mp hcs
              subst hcs
              obtain ⟨fs, hfs, hfh⟩ := hall _ List.mem_cons_self
              simp only [List.mem_singleton] at hfs
              subst hfs
              have hb1 : ν.val x - ν.val .z ≤ (n : Int) - 1 - k := hfh.1 _ List.mem_cons_self
              obtain ⟨t2, ht2⟩ := (hden w hw).term x k hxk
              rw [hwz] at ht2
              injection ht2 with _ ht2
              rw [ν.val_z] at hb1
              omega
            · intro ts hts hall w hw t' z hwz
              obtain ⟨f, hf, hts⟩ := Option.map_eq_some_iff.mp hts
              subst hts
              have h1 : (Ineq.le f (Form.const ((n : Int) - 1))).Holds G2 :=
                hall _ List.mem_cons_self
              rw [Ineq.holds_le, Form.eval_const] at h1
              obtain ⟨t2, ht2⟩ := (hden w hw).image f hf
              rw [hwz] at ht2
              injection ht2 with _ ht2
              omega
            · intro ts hts t2 ht lhs r h2 _
              obtain ⟨f, hf, hts⟩ := Option.map_eq_some_iff.mp hts
              subst hts
              simp only [List.mem_singleton] at ht
              subst ht
              simp at h2
            · intro ts hts
              simp at hts
            · intro a b' t2 hp
              simp at hp
          obtain ⟨wb, hwb, hwbt, hvalb⟩ := Env.Fits.get hg.holds.fits hg.holds.ids hv
          rw [hty] at hwbt
          obtain ⟨es, hes_eq, hlen, hes⟩ := hwbt.array
          subst hes_eq
          have hvalb' : valueOf env.vars E vb.id = some (.array t es) := hvalb
          have hbelow : ∀ f : Form, some (Form.atom g2.next) = some f →
              ∀ x ∈ f.atomIds, x < g2.next + 1 := by
            intro f hf x hx
            simp only [Option.some.injEq] at hf
            subst hf
            simp only [Form.atomIds, Form.atom, List.map_cons, List.map_nil,
              List.mem_singleton] at hx
            omega
          rcases evalAtom_index_cases (off := off) hwb with
            ⟨e, hoff, hix⟩ | ⟨k, hoff, hix⟩ | ⟨w, hoff, hnu, hix⟩
          · -- the offset's evaluation fails, and the read with it
            refine ⟨fun k hk => hnost k (by rw [hoff]; rw [hix] at hk; exact hk),
              fun y => if y = g2.next then 0 else G2 y,
              hg2.typeAtom (Fact.zero_mem_type t), Ext.trans hext2 Good.typeAtom_ext,
              hbelow, ?_⟩
            intro w hw
            rw [hix] at hw
            cases hw
          · -- a `u64` offset, proved below the length
            have hk := hP _ hoff .u64 k rfl
            have hden_o := hden _ hoff
            have hk0 : IntTy.Mem .u64 k := Value.Typed.inRange hden_o.typed
            have h0 : (0 : Int) ≤ k := by
              have := hk0.1
              simpa [IntTy.minVal, IntTy.signed] using this
            have hlt : k < (es.length : Int) := by
              rw [hlen]
              omega
            rw [ite_eq_left hlt] at hix
            have hmem : t.Mem (es.getD k.toNat 0) := hes _ (Typing.getD_mem hlt h0)
            refine ⟨fun k' hk' => by rw [hix] at hk'; cases hk',
              fun y => if y = g2.next then es.getD k.toNat 0 else G2 y,
              hg2.typeAtom hmem, Ext.trans hext2 Good.typeAtom_ext, hbelow, ?_⟩
            intro w hw
            rw [hix] at hw
            cases hw
            refine ⟨fun x k hk => by simp at hk, fun f hf => ?_, fun e he => ?_, ⟨rfl, hmem⟩⟩
            · simp only [Option.some.injEq] at hf
              subst hf
              exact ⟨t, by rw [Form.eval_atom]; simp⟩
            · obtain ⟨eo, heo, hee⟩ := Option.map_eq_some_iff.mp he
              subst hee
              have h1 : eo.eval (valueOf env.vars E) = some (.int .u64 k) := hden_o.gexpr eo heo
              simp only [GExpr.eval, hvalb', h1, hlt, ↓reduceIte]
          · -- an ill-formed offset
            refine ⟨fun k hk => by rw [hix] at hk; simp [ill] at hk,
              fun y => if y = g2.next then 0 else G2 y,
              hg2.typeAtom (Fact.zero_mem_type t), Ext.trans hext2 Good.typeAtom_ext,
              hbelow, ?_⟩
            intro w' hw'
            rw [hix] at hw'
            simp [ill] at hw'
      | _ =>
        simp only [hty] at h
        exact (Discharge.run_fail h).elim

end Checker

end Whitefoot
