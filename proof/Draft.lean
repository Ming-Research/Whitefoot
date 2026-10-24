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
        exact ⟨_, rfl, (fun k' hk => by cases hk),
          by simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind]⟩
    all_goals
      right
      exact ⟨_, rfl, (fun k' hk => by cases hk),
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
        try dsimp only at h
        split at h
        · exfalso
          first
            | exact Discharge.run_fail h
            | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind h; exact Discharge.run_fail hf)
        · rename_i hnd
          have hdis := Discharge.not_not_true hnd
          have h' : (Checker.typeAtom t >>= fun img =>
              (pure { image := some img, gexpr := o.gexpr.map (GExpr.index vb.id ·),
                      ty := .int t } : Checker.M Val)).run g2 = .ok (v, g1) := h
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
            refine ⟨(fun k' hk' => by rw [hix] at hk'; cases hk'),
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


/-! ## Products and quotients of bounded integers -/

theorem mul_mono_r {a b c : Int} (hc : 0 ≤ c) (h : a ≤ b) : a * c ≤ b * c := by
  have h1 : 0 ≤ (b - a) * c := Int.mul_nonneg (by omega) hc
  rw [Int.sub_mul] at h1
  omega

theorem mul_anti_r {a b c : Int} (hc : c ≤ 0) (h : a ≤ b) : b * c ≤ a * c := by
  have h1 : 0 ≤ (b - a) * -c := Int.mul_nonneg (by omega) (by omega)
  rw [Int.mul_neg, Int.sub_mul] at h1
  omega

theorem mul_mono_l {a b c : Int} (hc : 0 ≤ c) (h : a ≤ b) : c * a ≤ c * b := by
  rw [Int.mul_comm c a, Int.mul_comm c b]
  exact mul_mono_r hc h

theorem mul_anti_l {a b c : Int} (hc : c ≤ 0) (h : a ≤ b) : c * b ≤ c * a := by
  rw [Int.mul_comm c a, Int.mul_comm c b]
  exact mul_anti_r hc h

/-- A product of two interval members is at most `M` when the four corner
products are. -/
theorem mul_le_corners {x y a0 a1 b0 b1 M : Int} (hx0 : a0 ≤ x) (hx1 : x ≤ a1)
    (hy0 : b0 ≤ y) (hy1 : y ≤ b1) (h00 : a0 * b0 ≤ M) (h01 : a0 * b1 ≤ M)
    (h10 : a1 * b0 ≤ M) (h11 : a1 * b1 ≤ M) : x * y ≤ M := by
  rcases Int.le_total 0 y with hy | hy
  · have e1 := mul_mono_r hy hx1
    rcases Int.le_total 0 a1 with ha | ha
    · have e2 := mul_mono_l ha hy1
      omega
    · have e2 := mul_anti_l ha hy0
      omega
  · have e1 := mul_anti_r hy hx0
    rcases Int.le_total 0 a0 with ha | ha
    · have e2 := mul_mono_l ha hy1
      omega
    · have e2 := mul_anti_l ha hy0
      omega

/-- A product of two interval members is at least `M` when the four corner
products are. -/
theorem corners_le_mul {x y a0 a1 b0 b1 M : Int} (hx0 : a0 ≤ x) (hx1 : x ≤ a1)
    (hy0 : b0 ≤ y) (hy1 : y ≤ b1) (h00 : M ≤ a0 * b0) (h01 : M ≤ a0 * b1)
    (h10 : M ≤ a1 * b0) (h11 : M ≤ a1 * b1) : M ≤ x * y := by
  rcases Int.le_total 0 y with hy | hy
  · have e1 := mul_mono_r hy hx0
    rcases Int.le_total 0 a0 with ha | ha
    · have e2 := mul_mono_l ha hy0
      omega
    · have e2 := mul_anti_l ha hy1
      omega
  · have e1 := mul_anti_r hy hx1
    rcases Int.le_total 0 a1 with ha | ha
    · have e2 := mul_mono_l ha hy0
      omega
    · have e2 := mul_anti_l ha hy1
      omega

/-- `q ≤ a.fdiv c` with `0 < c` gives `q * c ≤ a`. -/
theorem le_fdiv_pos {a c q : Int} (hc : 0 < c) (h : q ≤ a.fdiv c) : q * c ≤ a := by
  rw [Int.fdiv_eq_ediv_of_nonneg a (Int.le_of_lt hc)] at h
  have h1 := mul_mono_r (Int.le_of_lt hc) h
  have h2 : a / c * c ≤ a := Int.ediv_mul_le a (by omega)
  omega

/-- `q ≤ a.fdiv c` with `c < 0` gives `a ≤ q * c`. -/
theorem le_fdiv_neg {a c q : Int} (hc : c < 0) (h : q ≤ a.fdiv c) : a ≤ q * c := by
  rw [← Int.neg_fdiv_neg a c, Int.fdiv_eq_ediv_of_nonneg (-a) (show (0 : Int) ≤ -c by omega)] at h
  have h1 := mul_mono_r (c := -c) (by omega) h
  have h2 : -a / -c * -c ≤ -a := Int.ediv_mul_le (-a) (by omega)
  have e1 : q * -c = -(q * c) := Int.mul_neg q c
  omega

/-- On a signed type the greatest member is one less than the negated least. -/
theorem signed_maxVal {t : IntTy} (h : t.signed = true) : t.maxVal = -t.minVal - 1 := by
  cases t <;> simp [IntTy.signed] at h <;> decide

theorem Form.eval_isConst {G : Nat → Int} {f : Form} (h : f.isConst = true) :
    f.eval G = f.c := by
  unfold Form.isConst at h
  unfold Form.eval
  cases hx : f.xs with
  | nil => simp [evalCoeffs]
  | cons p r =>
    rw [hx] at h
    simp at h

theorem _root_.Whitefoot.Value.Typed.int_mem {t : IntTy} {x : Int} {ty : Ty}
    (h : (Value.int t x).Typed ty) : t.Mem x := by
  cases ty <;> first | exact h.2 | exact (h : False).elim

theorem _root_.Whitefoot.Value.Typed.int_ty {t : IntTy} {x : Int} {ty : Ty}
    (h : (Value.int t x).Typed ty) : ty = .int t := by
  cases ty <;> first | exact (congrArg Ty.int h.1).symm | exact (h : False).elim

/-! ## Operand intervals -/

/-- An operand's interval holds its value [ENT-3.S7]. -/
theorem operandInterval_sound {st : State} {ν : TermVal} {t : IntTy} {v : Val} {x : Int}
    (hl0 : st.l0.Sat ν) (hterm : ∀ u k, v.term = some (u, k) → ν.val u + k = x)
    (hx : t.Mem x) :
    (operandInterval st t v).1 ≤ x ∧ x ≤ (operandInterval st t v).2 := by
  cases hvt : v.term with
  | none =>
    have e : operandInterval st t v = (t.minVal, t.maxVal) := by
      simp only [operandInterval, hvt]
    rw [e]
    exact hx
  | some p =>
    obtain ⟨u, k⟩ := p
    have e := hterm u k hvt
    by_cases hu : u = .z
    · subst hu
      rw [ν.val_z] at e
      have e2 : operandInterval st t v = (k, k) := by
        simp only [operandInterval, hvt]
      rw [e2]
      show k ≤ x ∧ x ≤ k
      omega
    · have e2 : operandInterval st t v = (match st.l0.interval u with
          | some (lo, hi) => (lo + k, hi + k)
          | none => (t.minVal, t.maxVal)) := by
        unfold operandInterval
        rw [hvt]
        cases u <;> first | exact absurd rfl hu | rfl
      rw [e2]
      cases hi : st.l0.interval u with
      | none => exact hx
      | some b =>
        obtain ⟨lo, hi'⟩ := b
        have hb := L0.interval_sound hl0 hi
        show lo + k ≤ x ∧ x ≤ hi' + k
        omega

/-- An operand's interval for the interval product holds its value: each
premise's tightening keeps the value inside [MSR-4]. -/
theorem productInterval_sound {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {t : IntTy} {v : Val} {x : Int}
    (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hterm : ∀ u k, v.term = some (u, k) → ν.val u + k = x)
    (himg : ∀ f, v.image = some f → f.eval G = x) (hx : t.Mem x) :
    (productInterval (view env g st) st t v).1 ≤ x ∧
      x ≤ (productInterval (view env g st) st t v).2 := by
  have hv := view_sat h
  have hb := operandInterval_sound h.l0 hterm hx
  cases hfi : v.image with
  | none =>
    unfold productInterval
    rw [hfi]
    exact hb
  | some f =>
    have hxf := himg f hfi
    have hsub : ∀ xs, evalCoeffs G xs ≤ (view env g st).substMax xs :=
      eval_le_substMax (fun y => View.lo_le y (hv.2.1 y).1 hv.2.2.2)
        (fun y => View.le_hi y (hv.2.1 y).2 hv.2.2.2)
    unfold productInterval
    rw [hfi]
    dsimp only
    refine Aff.foldl_inv _ (fun p : Ineq => p.Holds G)
      (fun acc : Int × Int => acc.1 ≤ x ∧ x ≤ acc.2) ?_ _ _ (fun p hp => hv.2.2.1 p hp) hb
    intro acc p hp hacc
    obtain ⟨lo, hi⟩ := acc
    dsimp only at hacc ⊢
    have hpp : evalCoeffs G p.xs ≤ p.c := hp
    have hxe : x = f.c + evalCoeffs G f.xs := hxf.symm
    have hup := hsub (addCoeffs f.xs (scaleCoeffs (-1) p.xs))
    have hdown := hsub (addCoeffs (scaleCoeffs (-1) f.xs) (scaleCoeffs (-1) p.xs))
    rw [eval_addCoeffs, eval_scaleCoeffs] at hup
    rw [eval_addCoeffs, eval_scaleCoeffs, eval_scaleCoeffs] at hdown
    constructor
    · split
      · exact Int.max_le.mpr ⟨hacc.1, by omega⟩
      · exact hacc.1
    · split
      · exact Int.le_min.mpr ⟨hacc.2, by omega⟩
      · exact hacc.2

/-! ## Exact infix operations: the domain query [OP-2] -/

/-- An infix operation's domain at its operands' values: the arithmetic rows
need their mathematical results in the type, division and remainder their
domain queries; a comparison always has a result. -/
def InfixDomain (op : InfixOp) (t : IntTy) (x y : Int) : Prop :=
  match op with
  | .arith o _ => o.defined t x y = true
  | .divide o _ => o.defined t x y = true
  | .compare _ => True

theorem facts_bound {ν : TermVal} {a b : Term} {c : Int}
    (h : ∃ fs ∈ [(([⟨a, b, c⟩] : List Bound), ([] : List Diseq))], FactsHold ν fs) :
    ν.val a - ν.val b ≤ c := by
  obtain ⟨fs, hfs, hf⟩ := h
  rw [List.mem_singleton] at hfs
  subst hfs
  exact hf.1 _ List.mem_cons_self

theorem facts_ground {ν : TermVal} {ok : Bool}
    (h : ∃ fs ∈ (if ok = true then [(([] : List Bound), ([] : List Diseq))]
      else [(([⟨.z, .z, -1⟩] : List Bound), ([] : List Diseq))]), FactsHold ν fs) :
    ok = true := by
  cases ok with
  | true => rfl
  | false =>
    obtain ⟨fs, hfs, hf⟩ := h
    simp only [Bool.false_eq_true, ↓reduceIte, List.mem_singleton] at hfs
    subst hfs
    have : ν.val Term.z - ν.val Term.z ≤ -1 := hf.1 _ List.mem_cons_self
    omega

/-- A disequality component: its ground form when the term is Z, otherwise
one disequality fact, gives `u + k ≠ c`. -/
theorem facts_ne {ν : TermVal} {u : Term} {k c d : Int} {ok : Bool}
    (hok : ok = true → k ≠ c) (hd : d = c - k)
    (h : ∃ fs ∈ (if (u == Term.z) = true then
        (if ok = true then [(([] : List Bound), ([] : List Diseq))]
          else [(([⟨.z, .z, -1⟩] : List Bound), ([] : List Diseq))])
      else [(([] : List Bound), ([⟨u, .z, d⟩] : List Diseq))]), FactsHold ν fs) :
    ν.val u + k ≠ c := by
  have hz := ν.val_z
  obtain ⟨fs, hfs, hf⟩ := h
  by_cases hu : u = Term.z
  · subst hu
    simp only [beq_self_eq_true, ↓reduceIte] at hfs
    cases hok' : ok with
    | true =>
      have := hok hok'
      omega
    | false =>
      rw [hok'] at hfs
      simp only [Bool.false_eq_true, ↓reduceIte, List.mem_singleton] at hfs
      subst hfs
      have : ν.val Term.z - ν.val Term.z ≤ -1 := hf.1 _ List.mem_cons_self
      omega
  · have hu' : (u == Term.z) = false := by simpa using hu
    rw [hu'] at hfs
    simp only [Bool.false_eq_true, ↓reduceIte, List.mem_singleton] at hfs
    subst hfs
    have : ν.val u - ν.val Term.z ≠ d := hf.2 _ List.mem_cons_self
    omega

/-- The two bounds of a constant multiple give the product's range. -/
theorem scaled_sound {ν : TermVal} {t : IntTy} {u : Term} {c : Int}
    {Z cs : List (List (List Bound × List Diseq))}
    (hcs : ((if c > 0 then [[([⟨u, .z, t.maxVal.fdiv c⟩], [])], [([⟨.z, u, (-t.minVal).fdiv c⟩], [])]]
      else if (c == 0) = true then Z
      else [[([⟨u, .z, t.minVal.fdiv c⟩], [])], [([⟨.z, u, (-t.maxVal).fdiv c⟩], [])]]) :
        List (List (List Bound × List Diseq))) = cs)
    (hall : ∀ c ∈ cs, ∃ fs ∈ c, FactsHold ν fs) :
    t.minVal ≤ ν.val u * c ∧ ν.val u * c ≤ t.maxVal := by
  have hz := ν.val_z
  split at hcs
  · rename_i hpos
    subst hcs
    have h1 := facts_bound (hall _ List.mem_cons_self)
    have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
    have e1 := le_fdiv_pos hpos (show ν.val u ≤ t.maxVal.fdiv c by omega)
    have e2 := le_fdiv_pos hpos (show -ν.val u ≤ (-t.minVal).fdiv c by omega)
    have e3 : -ν.val u * c = -(ν.val u * c) := Int.neg_mul _ _
    omega
  · split at hcs
    · rename_i _ h0
      have hc : c = 0 := by simpa using h0
      subst hc
      rw [Int.mul_zero]
      exact Fact.zero_mem_type t
    · rename_i hpos h0
      have hneg : c < 0 := by
        have : c ≠ 0 := by simpa using h0
        omega
      subst hcs
      have h1 := facts_bound (hall _ List.mem_cons_self)
      have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
      have e1 := le_fdiv_neg hneg (show ν.val u ≤ t.minVal.fdiv c by omega)
      have e2 := le_fdiv_neg hneg (show -ν.val u ≤ (-t.maxVal).fdiv c by omega)
      have e3 : -ν.val u * c = -(ν.val u * c) := Int.neg_mul _ _
      omega

/-- A domain query's L0 normalization, when its facts hold, gives the
operation's domain [MSR-4]. -/
theorem domain_l0 {ν : TermVal} {op : InfixOp} {t : IntTy} {va vb : Val} {x y : Int}
    {goal : GExpr} {cs : List (List (List Bound × List Diseq))}
    (hat : ∀ u k, va.term = some (u, k) → ν.val u + k = x)
    (hbt : ∀ u k, vb.term = some (u, k) → ν.val u + k = y)
    (hcs : (domainQuery goal op t va vb).l0 = some cs)
    (hall : ∀ c ∈ cs, ∃ fs ∈ c, FactsHold ν fs) : InfixDomain op t x y := by
  have hz := ν.val_z
  delta domainQuery at hcs
  dsimp only at hcs
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · obtain ⟨⟨ta, ka⟩, hta⟩ : ∃ p, va.term = some p := by
      cases hta : va.term with
      | none => cases htb : vb.term <;> cases o <;> simp [hta, htb] at hcs
      | some p => exact ⟨p, rfl⟩
    obtain ⟨⟨tb, kb⟩, htb⟩ : ∃ p, vb.term = some p := by
      cases htb : vb.term with
      | none => cases o <;> simp [hta, htb] at hcs
      | some p => exact ⟨p, rfl⟩
    have ex := hat ta ka hta
    have ey := hbt tb kb htb
    cases o with
    | add =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hta, htb] at hcs
      split at hcs
      · rename_i hc
        simp only [Bool.and_eq_true, beq_iff_eq] at hc
        obtain ⟨rfl, rfl⟩ := hc
        simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ground (hall _ List.mem_cons_self)
        simp only [decide_eq_true_eq] at h1
        omega
      · split at hcs
        · rename_i hc
          simp only [beq_iff_eq] at hc
          subst hc
          simp only [Option.some.injEq] at hcs
          subst hcs
          have h1 := facts_bound (hall _ List.mem_cons_self)
          have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
          omega
        · split at hcs
          · rename_i hc
            simp only [beq_iff_eq] at hc
            subst hc
            simp only [Option.some.injEq] at hcs
            subst hcs
            have h1 := facts_bound (hall _ List.mem_cons_self)
            have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
            omega
          · simp at hcs
    | sub =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hta, htb] at hcs
      split at hcs
      · rename_i hc
        simp only [Bool.and_eq_true, beq_iff_eq] at hc
        obtain ⟨rfl, rfl⟩ := hc
        simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ground (hall _ List.mem_cons_self)
        simp only [decide_eq_true_eq] at h1
        omega
      · split at hcs
        · rename_i hc
          simp only [beq_iff_eq] at hc
          subst hc
          simp only [Option.some.injEq] at hcs
          subst hcs
          have h1 := facts_bound (hall _ List.mem_cons_self)
          have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
          omega
        · split at hcs
          · rename_i hc
            simp only [beq_iff_eq] at hc
            subst hc
            simp only [Option.some.injEq] at hcs
            subst hcs
            have h1 := facts_bound (hall _ List.mem_cons_self)
            have h2 := facts_bound (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
            omega
          · simp at hcs
    | mul =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hta, htb] at hcs
      split at hcs
      · rename_i hc
        simp only [Bool.and_eq_true, beq_iff_eq] at hc
        obtain ⟨rfl, rfl⟩ := hc
        simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ground (hall _ List.mem_cons_self)
        simp only [decide_eq_true_eq] at h1
        have hx : x = ka := by omega
        have hy : y = kb := by omega
        subst hx hy
        exact h1
      · split at hcs
        · rename_i hc
          simp only [Bool.and_eq_true, beq_iff_eq] at hc
          obtain ⟨rfl, rfl⟩ := hc
          simp only [Option.some.injEq] at hcs
          have hx : x = ν.val ta := by omega
          have hy : y = kb := by omega
          subst hx hy
          exact scaled_sound hcs hall
        · split at hcs
          · rename_i hc
            simp only [Bool.and_eq_true, beq_iff_eq] at hc
            obtain ⟨rfl, rfl⟩ := hc
            simp only [Option.some.injEq] at hcs
            have hx : x = ka := by omega
            have hy : y = ν.val tb := by omega
            subst hx hy
            rw [Int.mul_comm]
            exact scaled_sound hcs hall
          · simp at hcs
  · simp only [InfixDomain, DivOp.defined, decide_eq_true_eq]
    obtain ⟨⟨td, kd⟩, htb⟩ : ∃ p, vb.term = some p := by
      cases htb : vb.term with
      | none => cases hta : va.term <;> simp [hta, htb] at hcs
      | some p => exact ⟨p, rfl⟩
    have ey := hbt td kd htb
    cases hta : va.term with
    | none =>
      simp only [hta, htb] at hcs
      split at hcs
      · simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ne (k := kd) (c := 0) (fun h => by simpa using h) (by omega)
          (hall _ List.mem_cons_self)
        have h2 := facts_ne (k := kd) (c := -1) (fun h => by simpa using h) (by omega)
          (hall _ (List.mem_cons_of_mem _ List.mem_cons_self))
        exact ⟨by omega, Or.inr (Or.inr (by omega))⟩
      · rename_i hsig
        simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ne (k := kd) (c := 0) (fun h => by simpa using h) (by omega)
          (hall _ List.mem_cons_self)
        exact ⟨by omega, Or.inl (by simpa using hsig)⟩
    | some pa =>
      obtain ⟨tn, kn⟩ := pa
      have ex := hat tn kn hta
      simp only [hta, htb] at hcs
      split at hcs
      · simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ne (k := kd) (c := 0) (fun h => by simpa using h) (by omega)
          (hall _ List.mem_cons_self)
        obtain ⟨fs, hfs, hf⟩ := hall _ (List.mem_cons_of_mem _ List.mem_cons_self)
        rcases List.mem_append.mp hfs with hfs | hfs
        · have h2 := facts_ne (k := kn) (c := t.minVal) (fun h => by simpa using h)
            (by omega) ⟨fs, hfs, hf⟩
          exact ⟨by omega, Or.inr (Or.inl (by omega))⟩
        · have h2 := facts_ne (k := kd) (c := -1) (fun h => by simpa using h) (by omega)
            ⟨fs, hfs, hf⟩
          exact ⟨by omega, Or.inr (Or.inr (by omega))⟩
      · rename_i hsig
        simp only [Option.some.injEq] at hcs
        subst hcs
        have h1 := facts_ne (k := kd) (c := 0) (fun h => by simpa using h) (by omega)
          (hall _ List.mem_cons_self)
        exact ⟨by omega, Or.inl (by simpa using hsig)⟩
  · exact trivial

/-- A domain query's affine targets, when they hold, give the operation's
domain [MSR-4]. -/
theorem domain_affine {G : Nat → Int} {op : InfixOp} {t : IntTy} {va vb : Val} {x y : Int}
    {goal : GExpr} {ts : List (Ineq × Option (Form × Term))}
    (hai : ∀ f, va.image = some f → f.eval G = x)
    (hbi : ∀ f, vb.image = some f → f.eval G = y)
    (hts : (domainQuery goal op t va vb).affine = some ts)
    (hall : ∀ q ∈ ts, q.1.Holds G) : InfixDomain op t x y := by
  delta domainQuery at hts
  dsimp only at hts
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · obtain ⟨fa, hfa⟩ : ∃ f, va.image = some f := by
      cases hfa : va.image with
      | none => cases hfb : vb.image <;> cases o <;> simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    obtain ⟨fb, hfb⟩ : ∃ f, vb.image = some f := by
      cases hfb : vb.image with
      | none => cases o <;> simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    have ex := hai fa hfa
    have ey := hbi fb hfb
    cases o with
    | add =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hfa, hfb, Option.some.injEq] at hts
      subst hts
      have h1 : (Ineq.le (fa.add fb) (Form.const t.maxVal)).Holds G := hall _ List.mem_cons_self
      have h2 : (Ineq.le (Form.const t.minVal) (fa.add fb)).Holds G :=
        hall _ (List.mem_cons_of_mem _ List.mem_cons_self)
      rw [Ineq.holds_le, Form.eval_add, Form.eval_const] at h1 h2
      omega
    | sub =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hfa, hfb, Option.some.injEq] at hts
      subst hts
      have h1 : (Ineq.le (fa.sub fb) (Form.const t.maxVal)).Holds G := hall _ List.mem_cons_self
      have h2 : (Ineq.le (Form.const t.minVal) (fa.sub fb)).Holds G :=
        hall _ (List.mem_cons_of_mem _ List.mem_cons_self)
      rw [Ineq.holds_le, Form.eval_sub, Form.eval_const] at h1 h2
      omega
    | mul =>
      simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
      simp only [IntTy.Mem, ArithOp.math]
      simp only [hfa, hfb] at hts
      split at hts
      · rename_i hc
        simp only [Option.some.injEq] at hts
        subst hts
        have h1 : (Ineq.le (fb.scale fa.c) (Form.const t.maxVal)).Holds G :=
          hall _ List.mem_cons_self
        have h2 : (Ineq.le (Form.const t.minVal) (fb.scale fa.c)).Holds G :=
          hall _ (List.mem_cons_of_mem _ List.mem_cons_self)
        rw [Ineq.holds_le, Form.eval_scale, Form.eval_const] at h1 h2
        have hxc : x = fa.c := by rw [← ex, Form.eval_isConst hc]
        subst hxc
        subst ey
        omega
      · split at hts
        · rename_i hc
          simp only [Option.some.injEq] at hts
          subst hts
          have h1 : (Ineq.le (fa.scale fb.c) (Form.const t.maxVal)).Holds G :=
            hall _ List.mem_cons_self
          have h2 : (Ineq.le (Form.const t.minVal) (fa.scale fb.c)).Holds G :=
            hall _ (List.mem_cons_of_mem _ List.mem_cons_self)
          rw [Ineq.holds_le, Form.eval_scale, Form.eval_const] at h1 h2
          have hyc : y = fb.c := by rw [← ey, Form.eval_isConst hc]
          subst hyc
          subst ex
          have e : fa.eval G * fb.c = fb.c * fa.eval G := Int.mul_comm _ _
          omega
        · simp at hts
  · simp only [InfixDomain, DivOp.defined, decide_eq_true_eq]
    obtain ⟨fa, hfa⟩ : ∃ f, va.image = some f := by
      cases hfa : va.image with
      | none => cases hfb : vb.image <;> simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    obtain ⟨fb, hfb⟩ : ∃ f, vb.image = some f := by
      cases hfb : vb.image with
      | none => simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    have ey := hbi fb hfb
    simp only [hfa, hfb] at hts
    split at hts
    · rename_i hsig
      simp only [Option.some.injEq] at hts
      subst hts
      have h1 : (Ineq.le (Form.const 1) fb).Holds G := hall _ List.mem_cons_self
      rw [Ineq.holds_le, Form.eval_const] at h1
      exact ⟨by omega, Or.inl (by simpa using hsig)⟩
    · simp at hts
  · exact trivial

/-- A domain query's affine alternatives, when one holds, give the
operation's domain [MSR-4]. -/
theorem domain_affineAny {G : Nat → Int} {op : InfixOp} {t : IntTy} {va vb : Val}
    {x y : Int} {goal : GExpr} {ts : List Ineq}
    (hai : ∀ f, va.image = some f → f.eval G = x)
    (hbi : ∀ f, vb.image = some f → f.eval G = y)
    (hts : ts ∈ (domainQuery goal op t va vb).affineAny)
    (hall : ∀ q ∈ ts, q.Holds G) : InfixDomain op t x y := by
  delta domainQuery at hts
  dsimp only at hts
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases hfa : va.image <;> cases hfb : vb.image <;> cases o <;> simp [hfa, hfb] at hts
  · simp only [InfixDomain, DivOp.defined, decide_eq_true_eq]
    obtain ⟨fa, hfa⟩ : ∃ f, va.image = some f := by
      cases hfa : va.image with
      | none => cases hfb : vb.image <;> simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    obtain ⟨fd, hfb⟩ : ∃ f, vb.image = some f := by
      cases hfb : vb.image with
      | none => simp [hfa, hfb] at hts
      | some f => exact ⟨f, rfl⟩
    have ex := hai fa hfa
    have ey := hbi fd hfb
    simp only [hfa, hfb] at hts
    split at hts
    · simp at hts
    · simp only [List.mem_flatMap, List.mem_map, List.mem_cons, List.not_mem_nil,
        or_false] at hts
      obtain ⟨z, hz, o', ho, rfl⟩ := hts
      have h1 := hall z List.mem_cons_self
      have h2 := hall o' (List.mem_cons_of_mem _ List.mem_cons_self)
      rcases hz with rfl | rfl <;> rcases ho with rfl | rfl | rfl <;>
        simp only [Ineq.holds_le, Form.eval_const] at h1 h2 <;>
        exact ⟨by omega, Or.inr (by omega)⟩
  · exact trivial

/-- A domain query's interval product, when its four corners lie in the type,
gives the multiplication's domain [MSR-4]. -/
theorem domain_product {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {op : InfixOp} {t : IntTy} {va vb : Val} {x y : Int}
    {goal : GExpr} {a b : Val} {t' : IntTy}
    (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hat : ∀ u k, va.term = some (u, k) → ν.val u + k = x)
    (hai : ∀ f, va.image = some f → f.eval G = x) (hax : t.Mem x)
    (hbt : ∀ u k, vb.term = some (u, k) → ν.val u + k = y)
    (hbi : ∀ f, vb.image = some f → f.eval G = y) (hbx : t.Mem y)
    (hp : (domainQuery goal op t va vb).product = some (a, b, t'))
    (hi : (intervalProduct (view env g st) st a b t').isSome = true) :
    InfixDomain op t x y := by
  delta domainQuery at hp
  dsimp only at hp
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases o with
    | add => cases hfa : va.image <;> cases hfb : vb.image <;> simp [hfa, hfb] at hp
    | sub => cases hfa : va.image <;> cases hfb : vb.image <;> simp [hfa, hfb] at hp
    | mul =>
      obtain ⟨fa, hfa⟩ : ∃ f, va.image = some f := by
        cases hfa : va.image with
        | none => cases hfb : vb.image <;> simp [hfa, hfb] at hp
        | some f => exact ⟨f, rfl⟩
      obtain ⟨fb, hfb⟩ : ∃ f, vb.image = some f := by
        cases hfb : vb.image with
        | none => simp [hfa, hfb] at hp
        | some f => exact ⟨f, rfl⟩
      simp only [hfa, hfb] at hp
      split at hp
      · simp at hp
      · simp only [Option.some.injEq, Prod.mk.injEq] at hp
        obtain ⟨rfl, rfl, rfl⟩ := hp
        have hA := productInterval_sound h hat hai hax
        have hB := productInterval_sound h hbt hbi hbx
        unfold intervalProduct at hi
        revert hi hA hB
        generalize productInterval (view env g st) st t va = pa
        generalize productInterval (view env g st) st t vb = pb
        obtain ⟨a0, a1⟩ := pa
        obtain ⟨b0, b1⟩ := pb
        intro hi hA hB
        dsimp only at hi hA hB
        split at hi
        · rename_i hall
          rw [List.all_eq_true] at hall
          have c00 := hall (a0 * b0) (by simp)
          have c01 := hall (a0 * b1) (by simp)
          have c10 := hall (a1 * b0) (by simp)
          have c11 := hall (a1 * b1) (by simp)
          simp only [Bool.and_eq_true, decide_eq_true_eq] at c00 c01 c10 c11
          simp only [InfixDomain, ArithOp.defined, decide_eq_true_eq]
          simp only [IntTy.Mem, ArithOp.math]
          exact ⟨corners_le_mul hA.1 hA.2 hB.1 hB.2 c00.2.1 c01.2.1 c10.2.1 c11.2.1,
            mul_le_corners hA.1 hA.2 hB.1 hB.2 c00.2.2 c01.2.2 c10.2.2 c11.2.2⟩
        · simp at hi
  · cases hfa : va.image <;> cases hfb : vb.image <;> simp [hfa, hfb] at hp
  · exact trivial

/-- A domain query's affine targets carry no bridge term. -/
theorem domain_bridge_none {goal : GExpr} {op : InfixOp} {t : IntTy} {va vb : Val}
    {ts : List (Ineq × Option (Form × Term))}
    (hts : (domainQuery goal op t va vb).affine = some ts) : ∀ q ∈ ts, q.2 = none := by
  delta domainQuery at hts
  dsimp only at hts
  repeat' split at hts
  all_goals first
    | (simp only [Option.some.injEq] at hts; subst hts; simp)
    | simp at hts

/-- What an infix operation's domain query means: its domain [OP-2, MSR-4]. -/
theorem domainQuery_means {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {goal : GExpr} {op : InfixOp} {t : IntTy} {va vb : Val}
    {x y : Int} (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (ha : va.Denotes (valueOf env.vars E) ν G (.int t x))
    (hb : vb.Denotes (valueOf env.vars E) ν G (.int t y))
    (hg : goal.eval (valueOf env.vars E) = some (.bool true) → InfixDomain op t x y) :
    (domainQuery goal op t va vb).Means (valueOf env.vars E) ν G
      (fun a b t => (intervalProduct (view env g st) st a b t).isSome = true)
      (InfixDomain op t x y) := by
  have hat : ∀ u k, va.term = some (u, k) → ν.val u + k = x := fun u k hk => by
    obtain ⟨t', e⟩ := ha.term u k hk
    exact ((Value.int.inj e).2).symm
  have hbt : ∀ u k, vb.term = some (u, k) → ν.val u + k = y := fun u k hk => by
    obtain ⟨t', e⟩ := hb.term u k hk
    exact ((Value.int.inj e).2).symm
  have hai : ∀ f, va.image = some f → f.eval G = x := fun f hf => by
    obtain ⟨t', e⟩ := ha.image f hf
    exact ((Value.int.inj e).2).symm
  have hbi : ∀ f, vb.image = some f → f.eval G = y := fun f hf => by
    obtain ⟨t', e⟩ := hb.image f hf
    exact ((Value.int.inj e).2).symm
  have hax := ha.typed.int_mem
  have hbx := hb.typed.int_mem
  refine ⟨fun e he hev => ?_, fun cs hcs hall => domain_l0 hat hbt hcs hall,
    fun ts hts hall => domain_affine hai hbi hts hall,
    fun ts hts q hq lhs r hr => ?_, fun ts hts hall => domain_affineAny hai hbi hts hall,
    fun a b t' hp hi => domain_product h hat hai hax hbt hbi hbx hp hi⟩
  · have he' : some goal = some e := he
    cases he'
    exact hg hev
  · rw [domain_bridge_none hts q hq] at hr
    cases hr

/-- The goal of an exact arithmetic row is its `.defined` query. -/
theorem infix_goal_arith {σ : Nat → Option Value} {o : ArithOp} {t : IntTy} {x y : Int}
    {ga gb : Option GExpr} (ha : ∀ e, ga = some e → e.eval σ = some (.int t x))
    (hb : ∀ e, gb = some e → e.eval σ = some (.int t y))
    (h : ((mk2 (GExpr.infix (.arith o .defined)) ga gb).getD .unit).eval σ =
      some (.bool true)) :
    o.defined t x y = true := by
  cases ga with
  | none => simp [mk2, GExpr.eval] at h
  | some ea =>
    cases gb with
    | none => simp [mk2, GExpr.eval] at h
    | some eb =>
      simpa [mk2, GExpr.eval, ha ea rfl, hb eb rfl, evalInfix, Except.toOption] using h

/-- The goal of an exact division or remainder is its `.defined` query. -/
theorem infix_goal_divide {σ : Nat → Option Value} {o : DivOp} {t : IntTy} {x y : Int}
    {ga gb : Option GExpr} (ha : ∀ e, ga = some e → e.eval σ = some (.int t x))
    (hb : ∀ e, gb = some e → e.eval σ = some (.int t y))
    (h : ((mk2 (GExpr.infix (.divide o .defined)) ga gb).getD .unit).eval σ =
      some (.bool true)) :
    o.defined t x y = true := by
  cases ga with
  | none => simp [mk2, GExpr.eval] at h
  | some ea =>
    cases gb with
    | none => simp [mk2, GExpr.eval] at h
    | some eb =>
      simpa [mk2, GExpr.eval, ha ea rfl, hb eb rfl, evalInfix, Except.toOption] using h

/-- The goal of an exact negation or absolute value is its `.defined` query. -/
theorem unary_goal {σ : Nat → Option Value} {u : UnaryOp} {t : IntTy} {x : Int}
    {ga : Option GExpr} (ha : ∀ e, ga = some e → e.eval σ = some (.int t x))
    (hs : t.signed = true)
    (h : ((mk1 (GExpr.op1 (.unary u .defined)) ga).getD .unit).eval σ = some (.bool true)) :
    u.defined t x = true := by
  cases ga with
  | none => simp [mk1, GExpr.eval] at h
  | some e => simpa [mk1, GExpr.eval, ha e rfl, evalOp, hs, Except.toOption] using h

/-! ## Progress: a right-hand side the checker accepted never sticks -/

/-- A bind whose first step and every continuation never stick never sticks. -/
theorem noStuck_bind {α β : Type} {x : Eval α} {f : α → Eval β}
    (hx : ∀ k, x ≠ .error (.stuck k)) (hf : ∀ a, x = .ok a → ∀ k, f a ≠ .error (.stuck k)) :
    ∀ k, (x >>= f) ≠ .error (.stuck k) := by
  intro k hk
  cases hxe : x with
  | error e =>
    rw [hxe, Discharge.error_bind] at hk
    cases hk
    exact hx k hxe
  | ok a =>
    rw [hxe, Typing.ok_bind] at hk
    exact hf a hxe k hk

/-- An interpreter infix step sticks only at an exact form outside its
domain. -/
theorem evalInfix_stuck {op : InfixOp} {wa wb : Value} {k : Stuck}
    (h : evalInfix op wa wb = .error (.stuck k)) :
    ∃ t x y, wa = .int t x ∧ wb = .int t y ∧
      ((∃ o, op = .arith o .exact ∧ o.exactForm t x y = none) ∨
       (∃ o, op = .divide o .exact ∧ o.exactForm t x y = none)) := by
  cases wa with
  | int t x =>
    cases wb with
    | int t' y =>
      by_cases htt : t = t'
      · subst htt
        refine ⟨t, x, y, rfl, rfl, ?_⟩
        simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte] at h
        rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
        · cases m
          · left
            refine ⟨o, rfl, ?_⟩
            cases hx : o.exactForm t x y with
            | none => rfl
            | some r => simp [hx] at h
          · simp at h
          · simp at h
        · cases m
          · right
            refine ⟨o, rfl, ?_⟩
            cases hx : o.exactForm t x y with
            | none => rfl
            | some r => simp [hx] at h
          · simp [ill] at h
          · simp at h
        · simp at h
      · have hb : (t != t') = true := by simpa using htt
        simp [evalInfix, hb, ill] at h
    | bool _ => simp [evalInfix, ill] at h
    | unit => simp [evalInfix, ill] at h
    | array _ _ => simp [evalInfix, ill] at h
    | status _ => simp [evalInfix, ill] at h
  | bool b =>
    cases wb with
    | bool b' =>
      rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c <;> (try cases c) <;> simp [evalInfix, ill] at h
    | int _ _ => simp [evalInfix, ill] at h
    | unit => simp [evalInfix, ill] at h
    | array _ _ => simp [evalInfix, ill] at h
    | status _ => simp [evalInfix, ill] at h
  | unit => simp [evalInfix, ill] at h
  | array _ _ => simp [evalInfix, ill] at h
  | status _ => simp [evalInfix, ill] at h

/-- An interpreter table operation sticks only at an exact negation or
absolute value of a signed type outside its domain. -/
theorem evalOp_stuck {o : TableOp} {ws : List Value} {k : Stuck}
    (h : evalOp o ws = .error (.stuck k)) :
    ∃ u t x, o = .unary u .exact ∧ ws = [.int t x] ∧ t.signed = true ∧
      u.exactForm t x = none := by
  unfold evalOp at h
  split at h
  · rename_i u m t x
    split at h
    · simp [ill] at h
    · rename_i hsig
      cases m with
      | exact =>
        refine ⟨u, t, x, rfl, rfl, by simpa using hsig, ?_⟩
        cases hx : u.exactForm t x with
        | none => rfl
        | some r => simp [hx] at h
      | wrap => simp at h
      | defined => simp at h
  all_goals simp [ill] at h

/-- The static check admits only literals in their types. -/
theorem atomTy_litOk {sc : Static.Scope} :
    ∀ {a : Atom} {t : Ty}, Static.atomTy sc a = .ok t → a.LitOk
  | .int _ _, _, h => by
    simp only [Static.atomTy] at h
    obtain ⟨_, hu, -⟩ := Typing.bind_ok h
    exact Typing.literalOk_mem hu
  | .unit, _, _ => trivial
  | .var _, _, _ => trivial
  | .index _ off, _, h => by
    simp only [Static.atomTy] at h
    obtain ⟨_, -, h⟩ := Typing.bind_ok h
    obtain ⟨_, hot, -⟩ := Typing.bind_ok h
    exact (atomTy_litOk hot : off.LitOk)

theorem mapM_litOk {sc : Static.Scope} :
    ∀ {as : List Atom} {ts : List Ty}, as.mapM (Static.atomTy sc) = .ok ts → ∀ a ∈ as, a.LitOk
  | [], _, _, a, ha => by simp at ha
  | b :: as, ts, h, a, ha => by
    rw [List.mapM_cons] at h
    obtain ⟨_, hb, h⟩ := Typing.bind_ok h
    obtain ⟨_, has, -⟩ := Typing.bind_ok h
    rcases List.mem_cons.mp ha with rfl | ha
    · exact atomTy_litOk hb
    · exact mapM_litOk has a ha

theorem mapM_length {α β ε : Type} {f : α → Except ε β} :
    ∀ {as : List α} {ws : List β}, as.mapM f = .ok ws → as.length = ws.length
  | [], ws, h => by
    simp only [List.mapM_nil] at h
    cases h
    rfl
  | a :: as, ws, h => by
    rw [List.mapM_cons] at h
    obtain ⟨_, -, h⟩ := Typing.bind_ok h
    obtain ⟨bs, hbs, h⟩ := Typing.bind_ok h
    cases h
    simp [mapM_length hbs]

theorem mapM_single_run {f : Atom → M Val} {a : Atom} {g g' : Gen} {vs : List Val}
    (h : ([a].mapM f).run g = .ok (vs, g')) : ∃ v, vs = [v] ∧ (f a).run g = .ok (v, g') := by
  rw [List.mapM_cons, List.mapM_nil] at h
  obtain ⟨v, g1, hv, h⟩ := Discharge.run_bind h
  obtain ⟨bs, g2, hbs, h⟩ := Discharge.run_bind h
  obtain ⟨rfl, rfl⟩ := Discharge.run_pure hbs
  obtain ⟨rfl, rfl⟩ := Discharge.run_pure h
  exact ⟨v, rfl, hv⟩

/-- Evaluating a list of atoms the checker evaluated never sticks, and the
facts keep holding with the atoms' registrations [OP-4]. -/
theorem evalAtoms_sound {env : Env} {st : State} {E E0 : Whitefoot.Env} {ν : TermVal} :
    ∀ (as : List Atom) {g g1 : Gen} {vs : List Val} {G : Nat → Int},
      Good env st g E E0 G ν → (∀ a ∈ as, a.LitOk) →
      (as.mapM (Checker.evalAtom env st)).run g = .ok (vs, g1) →
      (∀ k, as.mapM (Whitefoot.evalAtom E) ≠ .error (.stuck k)) ∧
      ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1
  | [], g, g1, vs, G, hg, _, h => by
    rw [List.mapM_nil] at h
    obtain ⟨-, rfl⟩ := Discharge.run_pure h
    refine ⟨fun k hk => ?_, G, hg, Ext.refl _ _⟩
    rw [List.mapM_nil] at hk
    cases hk
  | a :: as, g, g1, vs, G, hg, hlit, h => by
    rw [List.mapM_cons] at h
    obtain ⟨v, g2, hv, h⟩ := Discharge.run_bind h
    obtain ⟨vs', g3, hvs, h⟩ := Discharge.run_bind h
    obtain ⟨-, rfl⟩ := Discharge.run_pure h
    obtain ⟨hna, G2, hg2, hext2, -, -⟩ := evalAtom_sound a hg (hlit a List.mem_cons_self) hv
    obtain ⟨hnas, G3, hg3, hext3⟩ :=
      evalAtoms_sound as hg2 (fun b hb => hlit b (List.mem_cons_of_mem _ hb)) hvs
    refine ⟨?_, G3, hg3, Ext.trans hext2 hext3⟩
    rw [List.mapM_cons]
    exact noStuck_bind hna fun _ _ => noStuck_bind hnas fun _ _ k hk => by cases hk

/-- An accepted infix right-hand side never sticks [OP-2, OP-4]. -/
theorem infix_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {a b : Atom} {op : InfixOp} {r : Rhs} {sc : Static.Scope}
    {ty : Ty} (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.infix a op b)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.infix a op b) = .ok ty) :
    ∀ k, evalExpr env.prog E (.infix a op b) ≠ .error (.stuck k) := by
  obtain ⟨ta, hta, hty⟩ := Typing.bind_ok hty
  obtain ⟨tb, htb, -⟩ := Typing.bind_ok hty
  simp only [evalRhs] at hr
  obtain ⟨va, g2, hva, hr⟩ := Discharge.run_bind hr
  obtain ⟨vb, g3, hvb, hr⟩ := Discharge.run_bind hr
  obtain ⟨hna, G2, hg2, -, hbel2, hda⟩ := evalAtom_sound a hgood (atomTy_litOk hta) hva
  obtain ⟨hnb, G3, hg3, hext3, -, hdb⟩ := evalAtom_sound b hg2 (atomTy_litOk htb) hvb
  simp only [evalExpr]
  refine noStuck_bind hna fun wa hwa => noStuck_bind hnb fun wb hwb => ?_
  intro k hk
  obtain ⟨t, x, y, rfl, rfl, hex⟩ := evalInfix_stuck hk
  have hda3 : va.Denotes (valueOf env.vars E) ν G3 (.int t x) :=
    (hda _ hwa).congr fun f hf z hz => hext3.2 z (hbel2 f hf z hz)
  have hdb3 := hdb _ hwb
  have hvty : va.ty = .int t := (hda _ hwa).typed.int_ty
  rw [hvty] at hr
  rcases hex with ⟨o, rfl, hnone⟩ | ⟨o, rfl, hnone⟩
  · try dsimp only at hr
    obtain ⟨gen, g4, hget, hr⟩ := Discharge.run_bind hr
    obtain ⟨h3, h4⟩ := Discharge.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact Discharge.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind hr; exact Discharge.run_fail hf)
    · rename_i hnd
      have hdis : discharged env g3 st
          (domainQuery ((mk2 (GExpr.infix (.arith o .defined)) va.gexpr vb.gexpr).getD .unit)
            (.arith o .exact) t va vb) = true := by
        revert hnd
        simp
      have hP : InfixDomain (.arith o .exact) t x y :=
        discharged_sound hg3.holds
          (domainQuery_means (op := .arith o .exact) hg3.holds hda3 hdb3 fun hev =>
            infix_goal_arith hda3.gexpr hdb3.gexpr hev) hdis
      have hP' : o.defined t x y = true := hP
      simp [ArithOp.exactForm, hP'] at hnone
  · try dsimp only at hr
    obtain ⟨gen, g4, hget, hr⟩ := Discharge.run_bind hr
    obtain ⟨h3, h4⟩ := Discharge.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact Discharge.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind hr; exact Discharge.run_fail hf)
    · rename_i hnd
      have hdis : discharged env g3 st
          (domainQuery ((mk2 (GExpr.infix (.divide o .defined)) va.gexpr vb.gexpr).getD .unit)
            (.divide o .exact) t va vb) = true := by
        revert hnd
        simp
      have hP : InfixDomain (.divide o .exact) t x y :=
        discharged_sound hg3.holds
          (domainQuery_means (op := .divide o .exact) hg3.holds hda3 hdb3 fun hev =>
            infix_goal_divide hda3.gexpr hdb3.gexpr hev) hdis
      have hP' : o.defined t x y = true := hP
      simp [DivOp.exactForm, hP'] at hnone

/-- An accepted table operation never sticks [OP-2, OP-4]. -/
theorem op_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {o : TableOp} {as : List Atom} {r : Rhs} {sc : Static.Scope}
    {ty : Ty} (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.op o as)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.op o as) = .ok ty) :
    ∀ k, evalExpr env.prog E (.op o as) ≠ .error (.stuck k) := by
  obtain ⟨ts, hts, -⟩ := Typing.bind_ok hty
  have hlit : ∀ a ∈ as, a.LitOk := mapM_litOk hts
  simp only [evalRhs] at hr
  obtain ⟨vs, g2, hvs, hr⟩ := Discharge.run_bind hr
  obtain ⟨hnas, -⟩ := evalAtoms_sound as hgood hlit hvs
  simp only [evalExpr]
  refine noStuck_bind hnas fun ws hws => ?_
  intro k hk
  obtain ⟨u, t, x, rfl, rfl, hsig, hnone⟩ := evalOp_stuck hk
  have hlen := mapM_length hws
  rcases as with _ | ⟨a, _ | ⟨a', rest⟩⟩
  · simp at hlen
  · obtain ⟨va, rfl, hva⟩ := mapM_single_run hvs
    obtain ⟨-, G2, hg2, -, -, hda⟩ := evalAtom_sound a hgood (hlit a List.mem_cons_self) hva
    have hwa : Whitefoot.evalAtom E a = .ok (.int t x) := by
      rw [List.mapM_cons, List.mapM_nil] at hws
      obtain ⟨w, hw, h2⟩ := Typing.bind_ok hws
      obtain ⟨_, _, h3⟩ := Typing.bind_ok h2
      cases h3
      exact hw
    have hda' := hda _ hwa
    have hvty : va.ty = .int t := hda'.typed.int_ty
    dsimp only at hr
    rw [hvty] at hr
    dsimp only at hr
    try simp only [beq_self_eq_true, ↓reduceIte] at hr
    obtain ⟨gen, g4, hget, hr⟩ := Discharge.run_bind hr
    obtain ⟨h3, h4⟩ := Discharge.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact Discharge.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind hr; exact Discharge.run_fail hf)
    · rename_i hnd
      have hdis := Discharge.not_not_true hnd
      have hP : t.signed = true → u.defined t x = true := by
        refine discharged_sound hg2.holds ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ hdis
        · intro e he hev hs'
          simp only [Option.some.injEq] at he
          subst he
          exact unary_goal hda'.gexpr hs' hev
        · intro cs hcs hall _
          obtain ⟨⟨u', k'⟩, hxk, hcs⟩ := Option.map_eq_some_iff.mp hcs
          have ex : ν.val u' + k' = x := by
            obtain ⟨t', e'⟩ := hda'.term u' k' hxk
            exact ((Value.int.inj e').2).symm
          simp only [UnaryOp.defined, decide_eq_true_eq]
          dsimp only at hcs
          split at hcs
          · rename_i hz
            have hz' : u' = Term.z := by simpa using hz
            subst hz'
            subst hcs
            obtain ⟨fs, hfs, hf⟩ := hall _ List.mem_cons_self
            rw [List.mem_singleton] at hfs
            subst hfs
            rw [ν.val_z] at ex
            split at hf
            · rename_i hk
              have hk' : k' ≠ t.minVal := by simpa using hk
              omega
            · have : ν.val Term.z - ν.val Term.z ≤ -1 := hf.1 _ List.mem_cons_self
              omega
          · subst hcs
            obtain ⟨fs, hfs, hf⟩ := hall _ List.mem_cons_self
            rw [List.mem_singleton] at hfs
            subst hfs
            have h1 : ν.val u' - ν.val Term.z ≠ t.minVal - k' := hf.2 _ List.mem_cons_self
            rw [ν.val_z] at h1
            omega
        · intro ts hts hall _
          obtain ⟨f, hf, hts⟩ := Option.map_eq_some_iff.mp hts
          have ex : f.eval G2 = x := by
            obtain ⟨t', e'⟩ := hda'.image f hf
            exact ((Value.int.inj e').2).symm
          simp only [UnaryOp.defined, decide_eq_true_eq]
          cases u with
          | neg =>
            dsimp only at hts
            subst hts
            have h1 : (Ineq.le (f.scale (-1)) (Form.const t.maxVal)).Holds G2 :=
              hall _ List.mem_cons_self
            rw [Ineq.holds_le, Form.eval_scale, Form.eval_const] at h1
            have hmx := signed_maxVal hsig
            omega
          | abs =>
            dsimp only at hts
            subst hts
            have h1 : (Ineq.le (Form.const (t.minVal + 1)) f).Holds G2 :=
              hall _ List.mem_cons_self
            rw [Ineq.holds_le, Form.eval_const] at h1
            omega
        · intro ts hts q hq lhs r' hr' _
          obtain ⟨f, hf, hts⟩ := Option.map_eq_some_iff.mp hts
          have hnone : ∀ q ∈ ts, q.2 = none := by
            cases u <;> (dsimp only at hts; subst hts; simp)
          rw [hnone q hq] at hr'
          cases hr'
        · intro ts hts
          simp at hts
        · intro a' b' t' hp
          simp at hp
      have hd := hP hsig
      simp [UnaryOp.exactForm, hd] at hnone
  · simp at hlen

/-- An accepted call of a host or prelude function never sticks [OP-4]; a call
of a program function is the interpreter's call step, not an expression. -/
theorem call_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {r : Rhs} {sc : Static.Scope} {ty : Ty}
    (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.call path targs as)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.call path targs as) = .ok ty) (hcall : r.call = none) :
    ∀ k, evalExpr env.prog E (.call path targs as) ≠ .error (.stuck k) := by
  simp only [Static.exprTy] at hty
  simp only [evalRhs] at hr
  cases hc : env.prog.callee? path with
  | none =>
    rw [hc] at hty
    exact absurd hty Typing.reject_ne
  | some ce =>
    rw [hc] at hty hr
    cases ce with
    | fn f =>
      exfalso
      dsimp only at hr
      obtain ⟨args, g2, -, hr⟩ := Discharge.run_bind hr
      obtain ⟨gen, g3, -, hr⟩ := Discharge.run_bind hr
      obtain ⟨u, g4, -, hr⟩ := Discharge.run_bind hr
      split at hr
      all_goals
        obtain ⟨img, g5, -, hr⟩ := Discharge.run_bind hr
        obtain ⟨hr', -⟩ := Discharge.run_pure hr
        subst hr'
        simp at hcall
    | exitStatus =>
      cases targs with
      | cons _ _ => exact absurd hty Typing.reject_ne
      | nil =>
        obtain ⟨_, hn, hty⟩ := Typing.bind_ok hty
        obtain ⟨_, ha, -⟩ := Typing.bind_ok hty
        obtain ⟨a, rfl, hat⟩ := Typing.atomTy_one hn ha
        dsimp only at hr
        obtain ⟨vs, g2, hvs, -⟩ := Discharge.run_bind hr
        rw [List.mapM_cons, List.mapM_nil] at hvs
        obtain ⟨v, g3, hv, -⟩ := Discharge.run_bind hvs
        try dsimp only at hv
        have hna := (evalAtom_sound a hgood (atomTy_litOk hat) hv).1
        simp only [evalExpr, hc]
        refine noStuck_bind hna fun w _ => ?_
        intro k hk
        split at hk <;> simp [ill] at hk
    | arrayFilled =>
      rcases targs with _ | ⟨(et | _ | _ | ⟨_, _⟩ | _) | _, _ | ⟨_ | n, _ | ⟨_, _⟩⟩⟩ <;>
        try exact absurd hty Typing.reject_ne
      obtain ⟨_, hn, hty⟩ := Typing.bind_ok hty
      obtain ⟨_, ha, -⟩ := Typing.bind_ok hty
      obtain ⟨a, rfl, hat⟩ := Typing.atomTy_one hn ha
      dsimp only at hr
      obtain ⟨vs, g2, hvs, -⟩ := Discharge.run_bind hr
      rw [List.mapM_cons, List.mapM_nil] at hvs
      obtain ⟨v, g3, hv, -⟩ := Discharge.run_bind hvs
      try dsimp only at hv
      have hna := (evalAtom_sound a hgood (atomTy_litOk hat) hv).1
      simp only [evalExpr, hc]
      refine noStuck_bind hna fun w _ => ?_
      intro k hk
      split at hk
      · split at hk <;> simp [ill] at hk
      · simp [ill] at hk

/-- A right-hand side the checker accepted, with no call of a program
function, never sticks: its subscripts are below their lengths [OP-4] and each
exact operation is in its domain [OP-2]. -/
theorem evalRhs_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {e : Expr} {r : Rhs}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t) (hcall : r.call = none) :
    ∀ k, evalExpr env.prog E e ≠ .error (.stuck k) := by
  obtain ⟨G, ν, hw⟩ := hs
  have hgood : Good env st g E E0 G ν := ⟨hw, hf⟩
  obtain ⟨sc, ty, hty⟩ := ht
  rcases e with a | ⟨a, op, b⟩ | ⟨o, as⟩ | ⟨path, targs, as⟩ | bv
  · simp only [evalRhs] at hr
    obtain ⟨v, g2, hv, -⟩ := Discharge.run_bind hr
    simp only [evalExpr]
    exact (evalAtom_sound a hgood (atomTy_litOk hty) hv).1
  · exact infix_progress hgood hr hty
  · exact op_progress hgood hr hty
  · exact call_progress hgood hr hty hcall
  · intro k hk
    simp [evalExpr] at hk

/-- A subscript target the checker evaluated never sticks [OP-4]. -/
theorem target_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {b : String} {off : Atom} {val : Val}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (Checker.evalAtom env st (.index b off)).run g = .ok (val, g1))
    (ht : ∃ sc t, Static.atomTy sc (.index b off) = .ok t) :
    ∀ k, Whitefoot.evalAtom E (.index b off) ≠ .error (.stuck k) := by
  obtain ⟨G, ν, hw⟩ := hs
  obtain ⟨sc, ty, hty⟩ := ht
  exact (evalAtom_sound (.index b off) ⟨hw, hf⟩ (atomTy_litOk hty) hr).1

/-! ## Relations over a frame's names -/

/-- The integer an evaluation gives, and zero for any other result. -/
def intOf : Eval Value → Int
  | .ok (.int _ z) => z
  | _ => 0

theorem ok_get {α : Type} {P : α → Prop} {x : Eval α} {a : α} (h : Typing.Ok P x)
    (hx : x = .ok a) : P a := by
  subst hx
  exact h

theorem toOption_some {ε α : Type} {x : Except ε α} {a : α} (h : x.toOption = some a) :
    x = .ok a := by
  cases x with
  | error e => simp [Except.toOption] at h
  | ok b =>
    simp only [Except.toOption, Option.some.injEq] at h
    rw [h]

/-- An affine expression whose names are integer values in scope evaluates to
its value with each name read from the frame [INV-1]. -/
theorem evalAffine_val {E : Whitefoot.Env} {sc : Static.Scope} (hE : EnvTyped E sc)
    {rule : String} :
    ∀ {a : Affine} {u : Unit}, Static.affineOk rule sc a = .ok u →
      evalAffine E a = .ok (affVal (fun x => intOf (Whitefoot.Env.get E x)) a)
  | .atom (.int t v), _, _ => rfl
  | .atom (.var x), _, h => by
    simp only [Static.affineOk] at h
    split at h
    · rename_i y s hf
      obtain ⟨v, hv, hvt⟩ := hE.find x hf
      obtain ⟨z, rfl, -⟩ := hvt.int
      simp only [evalAffine, Whitefoot.evalAtom, affVal, hv, intOf] <;> rfl
    · exact absurd h Typing.reject_ne
    · exact absurd h Typing.reject_ne
  | .atom .unit, _, h => absurd h Typing.reject_ne
  | .atom (.index _ _), _, h => absurd h Typing.reject_ne
  | .add a b, _, h => by
    obtain ⟨_, ha, hb⟩ := Typing.bind_ok h
    simp only [evalAffine, affVal, evalAffine_val hE ha, evalAffine_val hE hb] <;> rfl
  | .sub a b, _, h => by
    obtain ⟨_, ha, hb⟩ := Typing.bind_ok h
    simp only [evalAffine, affVal, evalAffine_val hE ha, evalAffine_val hE hb] <;> rfl
  | .mul a b, _, h => by
    obtain ⟨_, ha, h⟩ := Typing.bind_ok h
    obtain ⟨_, hb, -⟩ := Typing.bind_ok h
    simp only [evalAffine, affVal, evalAffine_val hE ha, evalAffine_val hE hb] <;> rfl

theorem holds_val {E : Whitefoot.Env} {sc : Static.Scope} (hE : EnvTyped E sc) {rule : String}
    {r : Relation} {u v : Unit} (hl : Static.affineOk rule sc r.lhs = .ok u)
    (hr : Static.affineOk rule sc r.rhs = .ok v) :
    Relation.holds E r = .ok (r.op.eval (affVal (fun x => intOf (Whitefoot.Env.get E x)) r.lhs)
      (affVal (fun x => intOf (Whitefoot.Env.get E x)) r.rhs)) := by
  simp only [Relation.holds, evalAffine_val hE hl, evalAffine_val hE hr] <;> rfl

theorem allHold_of {E : Whitefoot.Env} :
    ∀ {rs : List Relation}, (∀ r ∈ rs, Relation.holds E r = .ok true) → allHold E rs = .ok true
  | [], _ => rfl
  | r :: rs, h => by
    simp only [allHold, h r List.mem_cons_self]
    exact allHold_of fun r' hr' => h r' (List.mem_cons_of_mem _ hr')

/-- A relation each of whose member queries is discharged holds of the names'
values [INV-1, FN-9]. -/
theorem members_sound {env : Env} {g : Gen} {st : State} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {r : Relation}
    {looks : String → Option (Term × Int)} {looki : String → Option Form} {val : String → Int}
    (h : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hs : ∀ x t k, looks x = some (t, k) → ν.val t + k = val x)
    (hi : ∀ x f, looki x = some f → f.eval G = val x)
    (hd : (memberQueries r looks looki).all (discharged env g st) = true) :
    r.op.eval (affVal val r.lhs) (affVal val r.rhs) = true := by
  have hq : ∀ (r' : Relation), discharged env g st (relQuery none r' looks looki) = true →
      r'.op.eval (affVal val r'.lhs) (affVal val r'.rhs) = true := fun r' hd' =>
    discharged_sound h (relQuery_means hs hi (fun e he => by simp at he)) hd'
  cases hop : r.op with
  | eq =>
    simp only [memberQueries, hop, List.all_cons, List.all_nil, Bool.and_true,
      Bool.and_eq_true] at hd
    have h1 := hq _ hd.1
    have h2 := hq _ hd.2
    simp only [CmpOp.eval, decide_eq_true_eq] at h1 h2 ⊢
    omega
  | _ =>
    simp only [memberQueries, hop, List.all_cons, List.all_nil, Bool.and_true] at hd
    have h1 := hq r hd
    rw [hop] at h1
    exact h1

/-- A binding's current term gives its value in the frame. -/
theorem lookT_sound {env : Env} {st : State} {A : List (Nat × Int × Int)} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} (hw : (⟨env.vars, st, A⟩ : Snapshot).HoldsWith E E0 G ν) :
    ∀ x t k, lookT env x = some (t, k) → ν.val t + k = intOf (Whitefoot.Env.get E x) := by
  intro x t k h
  cases hv : env.var? x with
  | none => simp [lookT, hv] at h
  | some v =>
    have hv' : env.vars.find? (·.name == x) = some v := hv
    cases hty : v.ty with
    | int ty =>
      have ht : Env.term? v = some (.place v.id ty) := by simp [Env.term?, hty]
      simp [lookT, hv, ht] at h
      obtain ⟨h1, h2⟩ := h
      subst h1
      subst h2
      obtain ⟨w, hg, -, hval⟩ := Env.Fits.get (vars := env.vars) hw.fits hw.ids hv'
      have hp : Checker.valueOf env.vars E v.id = some (.int ty (ν.val (.place v.id ty))) :=
        hw.places v (List.mem_of_find?_eq_some hv') ty hty
      rw [hval] at hp
      cases hp
      simp [hg, intOf]
    | _ =>
      have ht : Env.term? v = none := by simp [Env.term?, hty]
      simp [lookT, hv, ht] at h

/-- A binding's current image gives its value in the frame. -/
theorem lookI_sound {env : Env} {st : State} {A : List (Nat × Int × Int)} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} (hw : (⟨env.vars, st, A⟩ : Snapshot).HoldsWith E E0 G ν) :
    ∀ x f, lookI env st x = some f → f.eval G = intOf (Whitefoot.Env.get E x) := by
  intro x f h
  cases hv : env.var? x with
  | none => simp [lookI, hv] at h
  | some v =>
    have hv' : env.vars.find? (·.name == x) = some v := hv
    have hi : st.image? v.id = some f := by simpa [lookI, hv] using h
    obtain ⟨p, hp, hpf⟩ := Option.map_eq_some_iff.mp hi
    subst hpf
    have hid : p.1 = v.id := by simpa using List.find?_some hp
    obtain ⟨t, ht⟩ := hw.images p (List.mem_of_find?_eq_some hp)
    obtain ⟨w, hg, -, hval⟩ := Env.Fits.get (vars := env.vars) hw.fits hw.ids hv'
    have ht' : Checker.valueOf env.vars E v.id = some (.int t (p.2.eval G)) := by
      rw [← hid]
      exact ht
    rw [hval] at ht'
    cases ht'
    simp [hg, intOf]

/-- An invariant the checker proves holds of the frame [INV-1]. -/
theorem relHolds_sound {env : Env} {st : State} {g : Gen} {E E0 : Whitefoot.Env}
    {rel : Relation}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hinv : Static.invariantOk (scopeOf env.vars) rel = .ok ())
    (hd : relHolds env g st rel = true) :
    Relation.holds E rel = .ok true := by
  obtain ⟨G, ν, hw⟩ := hs
  simp only [Static.invariantOk, Typing.reject_bind] at hinv
  obtain ⟨-, h⟩ := Typing.ite_reject hinv
  obtain ⟨_, hl, hr⟩ := Typing.bind_ok h
  have hE : EnvTyped E (scopeOf env.vars) := hw.fits
  rw [holds_val hE hl hr]
  exact congrArg Except.ok (members_sound hw (lookT_sound hw) (lookI_sound hw) hd)

/-! ## Names in a frame -/

theorem nodup_reverse' {α : Type} {l : List α} (h : l.Nodup) : l.reverse.Nodup := by
  unfold List.Nodup at *
  rw [List.pairwise_reverse]
  exact h.imp fun hab hba => hab hba.symm

/-- A binding of a frame whose names are distinct is the one its name reads. -/
theorem get_of_mem : ∀ {E : Whitefoot.Env}, (E.map (·.1)).Nodup → ∀ {x : String} {w : Value},
    (x, w) ∈ E → Whitefoot.Env.get E x = .ok w
  | [], _, _, _, h => by simp at h
  | (n, v) :: E, hnd, x, w, h => by
    rw [List.map_cons, List.nodup_cons] at hnd
    rw [Whitefoot.Env.get_cons]
    rcases List.mem_cons.mp h with he | he
    · cases he
      rw [if_pos rfl]
    · have hne : n ≠ x := by
        rintro rfl
        exact hnd.1 (List.mem_map_of_mem he)
      rw [if_neg hne]
      exact get_of_mem hnd.2 he

theorem eraseDups_length_le : ∀ (n : Nat) (l : List String), l.length ≤ n →
    l.eraseDups.length ≤ l.length
  | _, [], _ => by simp
  | 0, _ :: _, h => by simp at h
  | n + 1, a :: as, h => by
    rw [List.eraseDups_cons, List.length_cons, List.length_cons]
    rw [List.length_cons] at h
    exact Nat.succ_le_succ (Nat.le_trans (eraseDups_length_le n _ (Nat.le_trans
      (List.length_filter_le _ _) (Nat.le_of_succ_le_succ h))) (List.length_filter_le _ _))

theorem nodup_of_eraseDups_length : ∀ (n : Nat) (l : List String), l.length ≤ n →
    l.eraseDups.length = l.length → l.Nodup
  | _, [], _, _ => by simp
  | 0, _ :: _, hn, _ => by simp at hn
  | n + 1, a :: as, hn, h => by
    rw [List.eraseDups_cons, List.length_cons, List.length_cons] at h
    rw [List.length_cons] at hn
    have hle1 := eraseDups_length_le n (as.filter fun b => !b == a)
      (Nat.le_trans (List.length_filter_le _ _) (Nat.le_of_succ_le_succ hn))
    have hle2 := List.length_filter_le (fun b => !b == a) as
    have hf : (as.filter fun b => !b == a).length = as.length := by omega
    have hall := List.length_filter_eq_length_iff.mp hf
    have hfe : (as.filter fun b => !b == a) = as := List.filter_eq_self.mpr hall
    rw [List.nodup_cons]
    refine ⟨fun ha => ?_, ?_⟩
    · have := hall a ha
      simp at this
    · have h' : (as.filter fun b => !b == a).eraseDups.length = as.length := by omega
      rw [hfe] at h'
      exact nodup_of_eraseDups_length n as (Nat.le_of_succ_le_succ hn) h'

/-- An accepted function's parameter names are distinct [TYPE-6]. -/
theorem checkFn_nodup {p : Program} {f : FnDecl} {u : Unit} (h : Static.checkFn p f = .ok u) :
    (f.params.map (·.name)).Nodup := by
  simp only [Static.checkFn, Typing.reject_bind] at h
  obtain ⟨hne, -⟩ := Typing.ite_reject h
  have hlen : (f.params.map (·.name)).eraseDups.length = (f.params.map (·.name)).length := by
    simpa using hne
  exact nodup_of_eraseDups_length _ _ (Nat.le_refl _) hlen

/-- Arguments whose types pass the static check have literals of their
types. -/
theorem argTypes_litOk {sc : Static.Scope} {p : Program} :
    ∀ {as : List (String × Atom)} {ts : List Ty}, as.length = ts.length →
      Static.argTypes sc p ((as.map (·.2)).zip ts) = .ok () → ∀ q ∈ as, q.2.LitOk
  | [], _, _, _, q, hq => by simp at hq
  | _ :: _, [], hl, _, _, _ => by simp at hl
  | (n, a) :: as, t :: ts, hl, h, q, hq => by
    simp only [List.map_cons, List.zip_cons_cons, Static.argTypes, Typing.reject_bind] at h
    obtain ⟨ta, hta, h⟩ := Typing.bind_ok h
    obtain ⟨-, h⟩ := Typing.ite_reject h
    rcases List.mem_cons.mp hq with rfl | hq
    · exact atomTy_litOk hta
    · exact argTypes_litOk (by simpa using hl) h q hq

/-! ## Arguments of a call -/

/-- Checker values of named arguments describe their runtime values, name by
name. -/
inductive ArgsDenote (σ : Nat → Option Value) (ν : TermVal) (G : Nat → Int) :
    List (String × Val) → List (String × Value) → Prop
  | nil : ArgsDenote σ ν G [] []
  | cons {n : String} {v : Val} {w : Value} {vs : List (String × Val)}
      {ws : List (String × Value)} :
      v.Denotes σ ν G w → ArgsDenote σ ν G vs ws →
        ArgsDenote σ ν G ((n, v) :: vs) ((n, w) :: ws)

theorem ArgsDenote.mem {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int}
    {vs : List (String × Val)} {ws : List (String × Value)} (h : ArgsDenote σ ν G vs ws) :
    ∀ {n : String} {v : Val}, (n, v) ∈ vs → ∃ w, (n, w) ∈ ws ∧ v.Denotes σ ν G w := by
  induction h with
  | nil => intro n v hm; simp at hm
  | cons hd _ ih =>
    intro n v hm
    rcases List.mem_cons.mp hm with he | he
    · cases he
      exact ⟨_, List.mem_cons_self, hd⟩
    · obtain ⟨w, hw, hden⟩ := ih he
      exact ⟨w, List.mem_cons_of_mem _ hw, hden⟩

theorem ArgsDenote.inRange {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int}
    {vs : List (String × Val)} {ws : List (String × Value)} (h : ArgsDenote σ ν G vs ws) :
    ∀ q ∈ ws, q.2.InRange := by
  induction h with
  | nil => intro q hq; simp at hq
  | cons hd _ ih =>
    intro q hq
    rcases List.mem_cons.mp hq with rfl | hq
    · exact hd.typed.inRange
    · exact ih q hq

/-- An argument the checker read by name describes the value its parameter
takes in the callee's frame. -/
theorem argVal_of_find {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int}
    {vs : List (String × Val)} {args : List (String × Value)}
    (hD : ArgsDenote σ ν G vs args) (hnd : (args.reverse.map (·.1)).Nodup) {x : String}
    {p : String × Val} (hp : vs.find? (·.1 == x) = some p) :
    ∃ w, Whitefoot.Env.get args.reverse x = .ok w ∧ p.2.Denotes σ ν G w := by
  obtain ⟨n, v⟩ := p
  have hx : n = x := by simpa using List.find?_some hp
  subst hx
  obtain ⟨w, hw, hden⟩ := hD.mem (List.mem_of_find?_eq_some hp)
  exact ⟨w, get_of_mem hnd (List.mem_reverse.mpr hw), hden⟩

/-- The interpreter evaluated named arguments one by one. -/
inductive ArgsEval (E : Whitefoot.Env) : List (String × Atom) → List (String × Value) → Prop
  | nil : ArgsEval E [] []
  | cons {n : String} {a : Atom} {w : Value} {as : List (String × Atom)}
      {ws : List (String × Value)} :
      Whitefoot.evalAtom E a = .ok w → ArgsEval E as ws →
        ArgsEval E ((n, a) :: as) ((n, w) :: ws)

theorem mapM_argsEval {E : Whitefoot.Env} {f' : String × Atom → Eval (String × Value)} :
    ∀ {as : List (String × Atom)} {args : List (String × Value)}, as.mapM f' = .ok args →
      (∀ n a, f' (n, a) = (Whitefoot.evalAtom E a >>= fun w => pure (n, w))) →
      ArgsEval E as args
  | [], args, h, _ => by
    rw [List.mapM_nil] at h
    cases h
    exact .nil
  | (n, a) :: as, args, h, hf' => by
    rw [List.mapM_cons, hf'] at h
    obtain ⟨p, hp, h⟩ := Typing.bind_ok h
    obtain ⟨w, hw, hp⟩ := Typing.bind_ok hp
    cases hp
    obtain ⟨ws, hws, h⟩ := Typing.bind_ok h
    cases h
    exact .cons hw (mapM_argsEval hws hf')

theorem mapM_noStuck {E : Whitefoot.Env} {f' : String × Atom → Eval (String × Value)}
    (hf' : ∀ n a, f' (n, a) = (Whitefoot.evalAtom E a >>= fun w => pure (n, w))) :
    ∀ {as : List (String × Atom)},
      (∀ p ∈ as, ∀ k, Whitefoot.evalAtom E p.2 ≠ .error (.stuck k)) →
      ∀ k, as.mapM f' ≠ .error (.stuck k)
  | [], _ => by
    intro k hk
    rw [List.mapM_nil] at hk
    cases hk
  | (n, a) :: as, h => by
    rw [List.mapM_cons, hf']
    exact noStuck_bind (noStuck_bind (h (n, a) List.mem_cons_self) fun _ _ k hk => by cases hk)
      fun _ _ => noStuck_bind (mapM_noStuck hf' fun p hp => h p (List.mem_cons_of_mem _ hp))
        fun _ _ k hk => by cases hk

/-- Evaluating a call's arguments the checker evaluated never sticks, and each
checker value describes its argument's value [OP-4]. -/
theorem evalArgs_sound {env : Env} {st : State} {E E0 : Whitefoot.Env} {ν : TermVal}
    {f : String × Atom → M (String × Val)} :
    ∀ (as : List (String × Atom)) {g g1 : Gen} {vs : List (String × Val)} {G : Nat → Int},
      Good env st g E E0 G ν → (∀ p ∈ as, p.2.LitOk) → (as.mapM f).run g = .ok (vs, g1) →
      (∀ n a, f (n, a) = (Checker.evalAtom env st a >>= fun v => pure (n, v))) →
      (∀ p ∈ as, ∀ k, Whitefoot.evalAtom E p.2 ≠ .error (.stuck k)) ∧
      ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
        ∀ args, ArgsEval E as args → ArgsDenote (valueOf env.vars E) ν G1 vs args
  | [], g, g1, vs, G, hg, _, h, _ => by
    rw [List.mapM_nil] at h
    obtain ⟨rfl, rfl⟩ := Discharge.run_pure h
    refine ⟨fun p hp => by simp at hp, G, hg, Ext.refl _ _, fun args ha => ?_⟩
    cases ha
    exact .nil
  | (n, a) :: as, g, g1, vs, G, hg, hlit, h, hf => by
    rw [List.mapM_cons, hf] at h
    obtain ⟨p, g2, hp, h⟩ := Discharge.run_bind h
    obtain ⟨v, g3, hv, hp⟩ := Discharge.run_bind hp
    obtain ⟨rfl, rfl⟩ := Discharge.run_pure hp
    obtain ⟨vs', g4, hvs, h⟩ := Discharge.run_bind h
    obtain ⟨rfl, rfl⟩ := Discharge.run_pure h
    obtain ⟨hna, G2, hg2, hext2, hbel2, hden2⟩ :=
      evalAtom_sound a hg (hlit (n, a) List.mem_cons_self) hv
    obtain ⟨hnas, G3, hg3, hext3, hden3⟩ :=
      evalArgs_sound as hg2 (fun q hq => hlit q (List.mem_cons_of_mem _ hq)) hvs hf
    refine ⟨fun q hq => ?_, G3, hg3, Ext.trans hext2 hext3, fun args ha => ?_⟩
    · rcases List.mem_cons.mp hq with rfl | hq
      · exact hna
      · exact hnas q hq
    · cases ha with
      | cons hw hrest =>
        refine .cons ?_ (hden3 _ hrest)
        exact (hden2 _ hw).congr fun f hf x hx => hext3.2 x (hbel2 f hf x hx)

/-! ## Calls -/

/-- Follow a checker run that succeeded to the `pure` it ends in, closing the
branches that fail, and prove the goal there by `rfl`. -/
syntax "walk_pure " ident : tactic
macro_rules
  | `(tactic| walk_pure $h:ident) => `(tactic|
      repeat' first
        | exact (Discharge.run_fail $h).elim
        | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind $h; exact (Discharge.run_fail hf).elim)
        | (obtain ⟨rfl, -⟩ := Discharge.run_pure $h; rfl)
        | (obtain ⟨_, _, -, $h⟩ := Discharge.run_bind $h)
        | (split at $h)
        | (dsimp only at $h))

set_option maxHeartbeats 1000000 in
/-- A right-hand side that calls no function of the program publishes no
call. -/
theorem evalRhs_call_none {env : Env} {st : State} {g g1 : Gen} {e : Expr} {r : Rhs}
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (hn : ∀ path targs as f, e = .call path targs as → env.prog.callee? path ≠ some (.fn f)) :
    r.call = none := by
  rcases e with a | ⟨a, op, b⟩ | ⟨o, as⟩ | ⟨path, targs, as⟩ | bv
  · simp only [evalRhs] at hr
    walk_pure hr
  · simp only [evalRhs] at hr
    walk_pure hr
  · simp only [evalRhs] at hr
    walk_pure hr
  · simp only [evalRhs] at hr
    cases hce : env.prog.callee? path with
    | none =>
      rw [hce] at hr
      walk_pure hr
    | some ce =>
      rw [hce] at hr
      cases ce with
      | fn f => exact absurd hce (hn path targs as f rfl)
      | exitStatus => walk_pure hr
      | arrayFilled => walk_pure hr
  · simp only [evalRhs] at hr
    walk_pure hr

/-- A call of a function of the program publishes the call, and its value is
no term. -/
theorem evalRhs_call_some {env : Env} {st : State} {g g1 : Gen} {e : Expr} {r : Rhs}
    {path : List String} {targs : List TArg} {as : List (String × Atom)} {f : FnDecl}
    (hr : (evalRhs env st e).run g = .ok (r, g1)) (he : e = .call path targs as)
    (hc : env.prog.callee? path = some (.fn f)) :
    ∃ vs, r.call = some (f, vs) ∧ r.val.term = none := by
  subst he
  simp only [evalRhs] at hr
  rw [hc] at hr
  dsimp only at hr
  obtain ⟨args, g2, -, hr⟩ := Discharge.run_bind hr
  obtain ⟨gen, g3, -, hr⟩ := Discharge.run_bind hr
  obtain ⟨u, g4, -, hr⟩ := Discharge.run_bind hr
  split at hr
  all_goals
    obtain ⟨img, g5, -, hr⟩ := Discharge.run_bind hr
    obtain ⟨rfl, -⟩ := Discharge.run_pure hr
    exact ⟨args, rfl, rfl⟩

/-- What a published call records: the callee, its arguments' checker values
and that each requirement was discharged after the arguments [FN-8]. -/
theorem evalRhs_call_inv {env : Env} {st : State} {g g1 : Gen} {r : Rhs}
    {path : List String} {targs : List TArg} {as : List (String × Atom)} {f : FnDecl}
    {vs : List (String × Val)}
    (hr : (evalRhs env st (.call path targs as)).run g = .ok (r, g1))
    (hc : r.call = some (f, vs)) :
    env.prog.callee? path = some (.fn f) ∧
    ∃ (g2 : Gen) (F : String × Atom → M (String × Val)), (as.mapM F).run g = .ok (vs, g2) ∧
      (∀ n a, F (n, a) = (Checker.evalAtom env st a >>= fun v => pure (n, v))) ∧
      ∀ rq ∈ f.requires, discharged env g2 st (relQuery
        (relG (fun x => (vs.find? (·.1 == x)).bind (·.2.gexpr)) rq) rq
        (fun x => (vs.find? (·.1 == x)).bind (·.2.term))
        (fun x => (vs.find? (·.1 == x)).bind (·.2.image))) = true := by
  by_cases hfn : ∃ f', env.prog.callee? path = some (.fn f')
  · obtain ⟨f', hce⟩ := hfn
    simp only [evalRhs] at hr
    rw [hce] at hr
    dsimp only at hr
    obtain ⟨args, g2, hargs, hr⟩ := Discharge.run_bind hr
    obtain ⟨gen, g3, hget, hr⟩ := Discharge.run_bind hr
    obtain ⟨h3, h4⟩ := Discharge.run_get hget
    rw [h3, h4] at hr
    obtain ⟨u, g4, hfor, hr⟩ := Discharge.run_bind hr
    have hall := (Discharge.forIn_run (P := fun rq => discharged env g2 st (relQuery
        (relG (fun x => (args.find? (·.1 == x)).bind (·.2.gexpr)) rq) rq
        (fun x => (args.find? (·.1 == x)).bind (·.2.term))
        (fun x => (args.find? (·.1 == x)).bind (·.2.image))) = true)
      (fun rq g' s g'' hb => by
        dsimp only at hb
        split at hb
        · exfalso
          first
            | exact Discharge.run_fail hb
            | (obtain ⟨_, _, hf, -⟩ := Discharge.run_bind hb; exact Discharge.run_fail hf)
        · rename_i hnd
          refine ⟨Discharge.not_not_true hnd, ?_⟩
          first
            | exact Discharge.run_pure hb
            | (obtain ⟨_, _, h0, hb⟩ := Discharge.run_bind hb
               obtain ⟨rfl, rfl⟩ := Discharge.run_pure h0
               exact Discharge.run_pure hb))
      hfor).2
    have hcall : r.call = some (f', args) := by
      split at hr
      all_goals
        obtain ⟨img, g5, -, hr⟩ := Discharge.run_bind hr
        obtain ⟨rfl, -⟩ := Discharge.run_pure hr
        rfl
    rw [hcall] at hc
    simp only [Option.some.injEq, Prod.mk.injEq] at hc
    obtain ⟨rfl, rfl⟩ := hc
    exact ⟨hce, g2, _, hargs, fun _ _ => rfl, hall⟩
  · exfalso
    have h0 := evalRhs_call_none hr fun p' t' a' f'' he hce => by
      cases he
      exact hfn ⟨f'', hce⟩
    rw [h0] at hc
    cases hc

/-- The arguments of a published call never stick [OP-4]. -/
theorem callArgs_progress {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {e : Expr} {r : Rhs} {path : List String} {targs : List TArg} {as : List (String × Atom)}
    {f : FnDecl} {vs : List (String × Val)}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t)
    (he : e = .call path targs as) (hc : r.call = some (f, vs)) :
    ∀ k, as.mapM (fun (n, a) => do return (n, ← Whitefoot.evalAtom E a)) ≠ .error (.stuck k) := by
  subst he
  obtain ⟨G, ν, hw⟩ := hs
  obtain ⟨sc, t, hty⟩ := ht
  obtain ⟨hce, g2, F, hvs, hF, -⟩ := evalRhs_call_inv hr hc
  obtain ⟨hn, hat, -⟩ := Typing.exprTy_fn hty hce
  have hlit := argTypes_litOk (by simpa using congrArg List.length hn) hat
  obtain ⟨hall, -⟩ := evalArgs_sound as ⟨hw, hf⟩ hlit hvs hF
  refine mapM_noStuck ?_ hall
  intro n a
  rfl

/-- The arguments of a published call are in their types' ranges. -/
theorem callArgs_inRange {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {e : Expr} {r : Rhs} {path : List String} {targs : List TArg} {as : List (String × Atom)}
    {f : FnDecl} {vs : List (String × Val)} {args : List (String × Value)}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t)
    (he : e = .call path targs as) (hc : r.call = some (f, vs))
    (hargs : as.mapM (fun (n, a) => do return (n, ← Whitefoot.evalAtom E a)) = .ok args) :
    ∀ a ∈ args, a.2.InRange := by
  subst he
  obtain ⟨G, ν, hw⟩ := hs
  obtain ⟨sc, t, hty⟩ := ht
  obtain ⟨hce, g2, F, hvs, hF, -⟩ := evalRhs_call_inv hr hc
  obtain ⟨hn, hat, -⟩ := Typing.exprTy_fn hty hce
  have hlit := argTypes_litOk (by simpa using congrArg List.length hn) hat
  obtain ⟨-, G2, -, -, hden⟩ := evalArgs_sound as ⟨hw, hf⟩ hlit hvs hF
  exact (hden args (mapM_argsEval hargs fun _ _ => rfl)).inRange

theorem eval_aff_int {σ : Nat → Option Value} {op : ArithOp} {ea eb : GExpr} {t : IntTy}
    {z : Int} (h : (GExpr.aff op ea eb).eval σ = some (.int t z)) :
    ∃ t1 x t2 y, ea.eval σ = some (.int t1 x) ∧ eb.eval σ = some (.int t2 y) ∧
      z = op.math x y := by
  revert h
  simp only [GExpr.eval]
  cases ha : ea.eval σ with
  | none => simp
  | some va =>
    cases hb : eb.eval σ with
    | none => cases va <;> simp
    | some vb =>
      cases va with
      | int t1 x =>
        cases vb with
        | int t2 y =>
          intro h
          simp only [Option.some.injEq, Value.int.injEq] at h
          exact ⟨t1, x, t2, y, rfl, rfl, h.2.symm⟩
        | _ => simp
      | _ => simp

theorem eval_cmp_true {σ : Nat → Option Value} {op : CmpOp} {ea eb : GExpr}
    (h : (GExpr.cmp op ea eb).eval σ = some (.bool true)) :
    ∃ t1 x t2 y, ea.eval σ = some (.int t1 x) ∧ eb.eval σ = some (.int t2 y) ∧
      op.eval x y = true := by
  revert h
  simp only [GExpr.eval]
  cases ha : ea.eval σ with
  | none => simp
  | some va =>
    cases hb : eb.eval σ with
    | none => cases va <;> simp
    | some vb =>
      cases va with
      | int t1 x =>
        cases vb with
        | int t2 y =>
          intro h
          simp only [Option.some.injEq, Value.bool.injEq] at h
          exact ⟨t1, x, t2, y, rfl, rfl, h⟩
        | _ => simp
      | _ => simp

/-- An affine expression's Goal tree evaluates to the expression's value when
each name's tree evaluates to the name's value. -/
theorem affG_eval {σ : Nat → Option Value} {look : String → Option GExpr} {val : String → Int}
    (hl : ∀ x e, look x = some e → ∀ t z, e.eval σ = some (.int t z) → z = val x) :
    ∀ (a : Affine) (ga : GExpr), affG look a = some ga →
      ∀ t z, ga.eval σ = some (.int t z) → z = affVal val a
  | .atom (.int t' v), ga, h, t, z, hz => by
    simp only [affG, Option.some.injEq] at h
    subst h
    simp only [GExpr.eval, Option.some.injEq, Value.int.injEq] at hz
    exact hz.2.symm
  | .atom (.var x), ga, h, t, z, hz => hl x ga h t z hz
  | .atom .unit, ga, h, _, _, _ => by simp [affG] at h
  | .atom (.index _ _), ga, h, _, _, _ => by simp [affG] at h
  | .add a b, ga, h, t, z, hz => by
    simp only [affG, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨ea, hea, eb, heb, rfl⟩ := h
    obtain ⟨t1, x, t2, y, hx, hy, rfl⟩ := eval_aff_int hz
    have e1 := affG_eval hl a ea hea t1 x hx
    have e2 := affG_eval hl b eb heb t2 y hy
    subst e1
    subst e2
    rfl
  | .sub a b, ga, h, t, z, hz => by
    simp only [affG, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨ea, hea, eb, heb, rfl⟩ := h
    obtain ⟨t1, x, t2, y, hx, hy, rfl⟩ := eval_aff_int hz
    have e1 := affG_eval hl a ea hea t1 x hx
    have e2 := affG_eval hl b eb heb t2 y hy
    subst e1
    subst e2
    rfl
  | .mul a b, ga, h, t, z, hz => by
    simp only [affG, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨ea, hea, eb, heb, rfl⟩ := h
    obtain ⟨t1, x, t2, y, hx, hy, rfl⟩ := eval_aff_int hz
    have e1 := affG_eval hl a ea hea t1 x hx
    have e2 := affG_eval hl b eb heb t2 y hy
    subst e1
    subst e2
    rfl

/-- A published call's requirements hold of its arguments in the callee's
frame [FN-8]. -/
theorem call_requires {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {e : Expr} {r : Rhs} {path : List String} {targs : List TArg} {as : List (String × Atom)}
    {f : FnDecl} {vs : List (String × Val)} {args : List (String × Value)}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (ht : ∃ t, Static.exprTy env.prog (scopeOf env.vars) e = .ok t)
    (hp : env.prog.accepts = true) (he : e = .call path targs as) (hc : r.call = some (f, vs))
    (hargs : as.mapM (fun (n, a) => do return (n, ← Whitefoot.evalAtom E a)) = .ok args) :
    allHold args.reverse f.requires = .ok true := by
  subst he
  obtain ⟨G, ν, hw⟩ := hs
  obtain ⟨t, hty⟩ := ht
  obtain ⟨hce, g2, F, hvs, hF, hreq⟩ := evalRhs_call_inv hr hc
  obtain ⟨hn, hat, -⟩ := Typing.exprTy_fn hty hce
  have hE : EnvTyped E (scopeOf env.vars) := hw.fits
  have hargT : EnvTyped args (f.params.map fun prm => (prm.name, Static.normTy env.prog prm.ty)) :=
    ok_get (Typing.args_ok (p := env.prog) hE hn hat) hargs
  have hlit := argTypes_litOk (by simpa using congrArg List.length hn) hat
  obtain ⟨-, G2, hg2, -, hden⟩ := evalArgs_sound as ⟨hw, hf⟩ hlit hvs hF
  have hD := hden args (mapM_argsEval hargs fun _ _ => rfl)
  obtain ⟨u, hfn⟩ := Typing.checkFn_of_mem (Typing.accepts_static hp) (Typing.callee_mem hce)
  have hnd : (args.reverse.map (·.1)).Nodup := by
    rw [List.map_reverse, hargT.names, List.map_map]
    exact nodup_reverse' (checkFn_nodup hfn)
  have hR := (Typing.checkFn_inv hfn).1
  have hE' : EnvTyped args.reverse (Typing.paramScope env.prog f) := by
    have h' := hargT.reverse
    rw [← List.map_reverse] at h'
    exact h'
  have hsT : ∀ x t k, (vs.find? (·.1 == x)).bind (·.2.term) = some (t, k) →
      ν.val t + k = intOf (Whitefoot.Env.get args.reverse x) := by
    intro x t k h
    obtain ⟨p, hp, hpt⟩ := Option.bind_eq_some_iff.mp h
    obtain ⟨w, hget, hdw⟩ := argVal_of_find hD hnd hp
    obtain ⟨ty, rfl⟩ := hdw.term t k hpt
    simp only [hget, intOf]
  have hiT : ∀ x fm, (vs.find? (·.1 == x)).bind (·.2.image) = some fm →
      fm.eval G2 = intOf (Whitefoot.Env.get args.reverse x) := by
    intro x fm h
    obtain ⟨p, hp, hpf⟩ := Option.bind_eq_some_iff.mp h
    obtain ⟨w, hget, hdw⟩ := argVal_of_find hD hnd hp
    obtain ⟨ty, rfl⟩ := hdw.image fm hpf
    simp only [hget, intOf]
  have hlG : ∀ x ge, (vs.find? (·.1 == x)).bind (·.2.gexpr) = some ge →
      ∀ t z, ge.eval (valueOf env.vars E) = some (.int t z) →
        z = intOf (Whitefoot.Env.get args.reverse x) := by
    intro x ge h t z hz
    obtain ⟨p, hp, hpg⟩ := Option.bind_eq_some_iff.mp h
    obtain ⟨w, hget, hdw⟩ := argVal_of_find hD hnd hp
    have hw' := hdw.gexpr ge hpg
    rw [hz] at hw'
    cases hw'
    simp only [hget, intOf]
  have hgT : ∀ (rq : Relation) (ge : GExpr),
      relG (fun x => (vs.find? (·.1 == x)).bind (·.2.gexpr)) rq = some ge →
      ge.eval (valueOf env.vars E) = some (.bool true) →
      rq.op.eval (affVal (fun x => intOf (Whitefoot.Env.get args.reverse x)) rq.lhs)
        (affVal (fun x => intOf (Whitefoot.Env.get args.reverse x)) rq.rhs) = true := by
    intro rq ge hge hev
    simp only [relG, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at hge
    obtain ⟨ea, hea, eb, heb, rfl⟩ := hge
    obtain ⟨t1, x, t2, y, hx, hy, hxy⟩ := eval_cmp_true hev
    have e1 := affG_eval hlG rq.lhs ea hea t1 x hx
    have e2 := affG_eval hlG rq.rhs eb heb t2 y hy
    rw [e1, e2] at hxy
    exact hxy
  apply allHold_of
  intro rq hrq
  obtain ⟨⟨_, hl⟩, ⟨_, hrr⟩⟩ := hR rq hrq
  rw [holds_val hE' hl hrr]
  exact congrArg Except.ok
    (discharged_sound hg2.holds (relQuery_means hsT hiT (hgT rq)) (hreq rq hrq))

/-! ## Infix values -/

/-- The infix operators whose result is an integer: exact and wrapping
arithmetic, division and remainder. -/
def intResult : InfixOp → Bool
  | .arith _ .exact | .arith _ .wrap | .divide _ .exact | .divide _ .wrap => true
  | _ => false

theorem exactForm_val {o : ArithOp} {t : IntTy} {x y q : Int}
    (h : o.exactForm t x y = some q) : q = o.math x y := by
  unfold ArithOp.exactForm at h
  by_cases hd : o.defined t x y = true
  · rw [ite_eq_left hd] at h
    cases h
    rfl
  · rw [ite_eq_right hd] at h
    cases h

/-- An infix step of a comparison or a `defined` query gives a `Bool`. -/
theorem evalInfix_bool {op : InfixOp} {wa wb w : Value} (hop : intResult op = false)
    (h : evalInfix op wa wb = .ok w) : w.Typed .bool := by
  cases wa with
  | int t x =>
    cases wb with
    | int t' y =>
      by_cases htt : t = t'
      · subst htt
        simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte] at h
        rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
        · cases m
          · simp [intResult] at hop
          · simp [intResult] at hop
          · simp at h
            subst h
            exact True.intro
        · cases m
          · simp [intResult] at hop
          · simp [intResult] at hop
          · simp at h
            subst h
            exact True.intro
        · simp at h
          subst h
          exact True.intro
      · have hb : (t != t') = true := by simpa using htt
        simp [evalInfix, hb, ill] at h
    | bool _ => simp [evalInfix, ill] at h
    | unit => simp [evalInfix, ill] at h
    | array _ _ => simp [evalInfix, ill] at h
    | status _ => simp [evalInfix, ill] at h
  | bool b =>
    cases wb with
    | bool b' =>
      rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
      · simp [evalInfix, ill] at h
      · simp [evalInfix, ill] at h
      · cases c <;> simp [evalInfix, ill] at h <;> (subst h; exact True.intro)
    | int _ _ => simp [evalInfix, ill] at h
    | unit => simp [evalInfix, ill] at h
    | array _ _ => simp [evalInfix, ill] at h
    | status _ => simp [evalInfix, ill] at h
  | unit => simp [evalInfix, ill] at h
  | array _ _ => simp [evalInfix, ill] at h
  | status _ => simp [evalInfix, ill] at h

/-- An integer-result infix step that gives a value took an integer on the
left. -/
theorem evalInfix_int_left {op : InfixOp} {wa wb w : Value} (hop : intResult op = true)
    (h : evalInfix op wa wb = .ok w) : ∃ t x, wa = .int t x := by
  cases wa with
  | int t x => exact ⟨t, x, rfl⟩
  | bool b =>
    rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
    · cases wb <;> simp [evalInfix, ill] at h
    · cases wb <;> simp [evalInfix, ill] at h
    · simp [intResult] at hop
  | unit => simp [evalInfix, ill] at h
  | array _ _ => simp [evalInfix, ill] at h
  | status _ => simp [evalInfix, ill] at h

/-- An integer-result infix step that gives a value took two integers of the
left operand's type and gives an integer of that type in its range; an exact
arithmetic step gives the mathematical result. -/
theorem evalInfix_int {op : InfixOp} {wa wb w : Value} {t : IntTy} {tyb : Ty}
    (hop : intResult op = true) (ha : wa.Typed (.int t)) (hb : wb.Typed tyb)
    (h : evalInfix op wa wb = .ok w) :
    ∃ x y q, wa = .int t x ∧ wb = .int t y ∧ w = .int t q ∧ t.Mem q ∧
      ∀ o, op = .arith o .exact → q = o.math x y := by
  obtain ⟨x, rfl, hx⟩ := ha.int
  cases wb with
  | int t' y =>
    by_cases htt : t = t'
    · subst htt
      have hy : t.Mem y := hb.int_mem
      simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte] at h
      rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
      · cases m
        · cases hq : o.exactForm t x y with
          | none => simp [hq, Whitefoot.stuck] at h
          | some q =>
            simp [hq] at h
            subst h
            exact ⟨x, y, q, rfl, rfl, rfl, ArithOp.exactForm_mem hq,
              fun o' ho => by cases ho; exact exactForm_val hq⟩
        · simp at h
          subst h
          exact ⟨x, y, _, rfl, rfl, rfl, IntTy.wrap_mem t (o.math x y), fun o' ho => by cases ho⟩
        · simp [intResult] at hop
      · cases m
        · cases hq : o.exactForm t x y with
          | none => simp [hq, Whitefoot.stuck] at h
          | some q =>
            simp [hq] at h
            subst h
            exact ⟨x, y, q, rfl, rfl, rfl, Typing.divExact_mem hx hy hq,
              fun o' ho => by cases ho⟩
        · simp [ill] at h
        · simp [intResult] at hop
      · simp [intResult] at hop
    · have hb' : (t != t') = true := by simpa using htt
      simp [evalInfix, hb', ill] at h
  | bool _ => simp [evalInfix, ill] at h
  | unit => simp [evalInfix, ill] at h
  | array _ _ => simp [evalInfix, ill] at h
  | status _ => simp [evalInfix, ill] at h

/-- A two-operand Goal expression gives the interpreter's result when its
operands' expressions give the operands. -/
theorem mk2_infix_eval {σ : Nat → Option Value} {op : InfixOp} {ga gb : Option GExpr}
    {wa wb w : Value} (ha : ∀ e, ga = some e → e.eval σ = some wa)
    (hb : ∀ e, gb = some e → e.eval σ = some wb) (h : evalInfix op wa wb = .ok w) :
    ∀ e, mk2 (GExpr.infix op) ga gb = some e → e.eval σ = some w := by
  intro e he
  cases ga with
  | none => simp [mk2] at he
  | some ea =>
    cases gb with
    | none => simp [mk2] at he
    | some eb =>
      simp only [mk2, Option.some.injEq] at he
      subst he
      simp only [GExpr.eval, ha ea rfl, hb eb rfl, h, Except.toOption]

/-- A `Bool` value with only a Goal expression describes a runtime `Bool` its
expression gives. -/
theorem bool_val_denotes {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int}
    {gx : Option GExpr} {w : Value} (hgx : ∀ e, gx = some e → e.eval σ = some w)
    (ht : w.Typed .bool) : ({ gexpr := gx, ty := .bool } : Val).Denotes σ ν G w :=
  ⟨fun _ _ h => by cases h, fun _ h => by cases h, hgx, ht⟩

theorem typeAtom_out {t : IntTy} {g g' : Gen} {img : Form}
    (h : (typeAtom t).run g = .ok (img, g')) :
    img = Form.atom g.next ∧
      g' = { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms } := by
  rw [Discharge.typeAtom_run] at h
  simp only [Except.ok.injEq, Prod.mk.injEq] at h
  obtain ⟨rfl, rfl⟩ := h
  exact ⟨rfl, rfl⟩

/-- The end of an integer arm: its image is a fresh atom, or a form at the same
generator that gives the runtime result. -/
theorem int_arm_end {env : Env} {st : State} {g0 g g' : Gen} {E E0 : Whitefoot.Env}
    {G0 G : Nat → Int} {ν : TermVal} {t : IntTy} {img : Form} {gx : Option GExpr}
    {ev : Eval Value} (hg : Good env st g E E0 G ν) (hext : Ext g0 g G0 G)
    (hres : ∀ w, ev = .ok w → ∃ q, w = .int t q ∧ t.Mem q)
    (hgx : ∀ w, ev = .ok w → ∀ e, gx = some e → e.eval (valueOf env.vars E) = some w)
    (himg : (img = Form.atom g.next ∧
        g' = { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms }) ∨
      (g' = g ∧ ∀ w, ev = .ok w → w = .int t (img.eval G))) :
    ∃ G', Good env st g' E E0 G' ν ∧ Ext g0 g' G0 G' ∧ ∀ w, ev = .ok w →
      ({ image := some img, gexpr := gx, ty := .int t } : Val).Denotes
        (valueOf env.vars E) ν G' w := by
  have hz : t.Mem (intOf ev) := by
    cases hev : ev with
    | error _ => exact Fact.zero_mem_type t
    | ok w =>
      obtain ⟨q, rfl, hq⟩ := hres w hev
      exact hq
  rcases himg with ⟨rfl, rfl⟩ | ⟨rfl, himg⟩
  · refine ⟨_, hg.typeAtom hz, hext.trans Good.typeAtom_ext, fun w hw => ?_⟩
    obtain ⟨q, rfl, hq⟩ := hres w hw
    refine ⟨fun x k hk => by simp at hk, fun f hf => ?_, hgx _ hw, ⟨rfl, hq⟩⟩
    simp only [Option.some.injEq] at hf
    subst hf
    refine ⟨t, ?_⟩
    rw [Form.eval_atom]
    simp [hw, intOf]
  · refine ⟨G, hg, hext, fun w hw => ?_⟩
    obtain ⟨q, rfl, hq⟩ := hres w hw
    refine ⟨fun x k hk => by simp at hk, fun f hf => ?_, hgx _ hw, ⟨rfl, hq⟩⟩
    simp only [Option.some.injEq] at hf
    subst hf
    exact ⟨t, himg _ hw⟩

/-- The runtime operands and result of an integer-result infix expression
whose left operand the checker typed as an integer. -/
theorem infix_runtime {E : Whitefoot.Env} {a b : Atom} {op : InfixOp} {va vb : Val}
    {t : IntTy} {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int} {p : Program}
    {w : Value} (hop : intResult op = true) (hvt : va.ty = .int t)
    (hda : ∀ w, Whitefoot.evalAtom E a = .ok w → va.Denotes σ ν G w)
    (hdb : ∀ w, Whitefoot.evalAtom E b = .ok w → vb.Denotes σ ν G w)
    (hw : evalExpr p E (.infix a op b) = .ok w) :
    ∃ x y q, Whitefoot.evalAtom E a = .ok (.int t x) ∧
      Whitefoot.evalAtom E b = .ok (.int t y) ∧ w = .int t q ∧ t.Mem q ∧
      ∀ o, op = .arith o .exact → q = o.math x y := by
  simp only [evalExpr] at hw
  obtain ⟨wa, hwa, hw⟩ := Typing.bind_ok hw
  obtain ⟨wb, hwb, hw⟩ := Typing.bind_ok hw
  have hta : wa.Typed (.int t) := by
    rw [← hvt]
    exact (hda wa hwa).typed
  obtain ⟨x, y, q, rfl, rfl, rfl, hq, hm⟩ := evalInfix_int hop hta (hdb wb hwb).typed hw
  exact ⟨x, y, q, hwa, hwb, rfl, hq, hm⟩

/-- Follow an integer infix arm from its generator read past its domain test,
closing the branch that fails. -/
syntax "int_walk " ident : tactic
macro_rules
  | `(tactic| int_walk $h:ident) => `(tactic| (
      try dsimp only at $h
      obtain ⟨_, _, hget, $h⟩ := Discharge.run_bind $h
      obtain ⟨h3, h4⟩ := Discharge.run_get hget
      rw [h3, h4] at $h
      try dsimp only at $h
      try split at $h
      all_goals first
        | (exfalso; exact Discharge.run_fail $h)
        | (exfalso; obtain ⟨_, _, hf, -⟩ := Discharge.run_bind $h; exact Discharge.run_fail hf)
        | skip
      try dsimp only at $h))

set_option maxHeartbeats 1000000 in
/-- The value of an accepted infix right-hand side: its checker value
describes the runtime value. -/
theorem infix_sound {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {a b : Atom} {op : InfixOp} {r : Rhs} {sc : Static.Scope}
    {ty : Ty} (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.infix a op b)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.infix a op b) = .ok ty) :
    ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
      ∀ w, evalExpr env.prog E (.infix a op b) = .ok w →
        r.val.Denotes (valueOf env.vars E) ν G1 w := by
  obtain ⟨ta, hta, hty⟩ := Typing.bind_ok hty
  obtain ⟨tb, htb, -⟩ := Typing.bind_ok hty
  simp only [evalRhs] at hr
  obtain ⟨va, g2, hva, hr⟩ := Discharge.run_bind hr
  obtain ⟨vb, g3, hvb, hr⟩ := Discharge.run_bind hr
  obtain ⟨-, G2, hg2, hext2, hbel2, hda⟩ := evalAtom_sound a hgood (atomTy_litOk hta) hva
  obtain ⟨-, G3, hg3, hext3, -, hdb⟩ := evalAtom_sound b hg2 (atomTy_litOk htb) hvb
  have hda3 : ∀ w, Whitefoot.evalAtom E a = .ok w → va.Denotes (valueOf env.vars E) ν G3 w :=
    fun w hw => (hda w hw).congr fun f hf z hz => hext3.2 z (hbel2 f hf z hz)
  have hext23 : Ext g g3 G G3 := hext2.trans hext3
  have hrun : ∀ w, evalExpr env.prog E (.infix a op b) = .ok w →
      ∃ wa wb, Whitefoot.evalAtom E a = .ok wa ∧ Whitefoot.evalAtom E b = .ok wb ∧
        evalInfix op wa wb = .ok w := by
    intro w hw
    simp only [evalExpr] at hw
    obtain ⟨wa, hwa, hw⟩ := Typing.bind_ok hw
    obtain ⟨wb, hwb, hw⟩ := Typing.bind_ok hw
    exact ⟨wa, wb, hwa, hwb, hw⟩
  have hgx : ∀ w, evalExpr env.prog E (.infix a op b) = .ok w → ∀ e,
      mk2 (GExpr.infix op) va.gexpr vb.gexpr = some e →
        e.eval (valueOf env.vars E) = some w := by
    intro w hw
    obtain ⟨wa, wb, hwa, hwb, h⟩ := hrun w hw
    exact mk2_infix_eval (hda3 wa hwa).gexpr (hdb wb hwb).gexpr h
  have hres : intResult op = true → ∀ t, va.ty = .int t → ∀ w,
      evalExpr env.prog E (.infix a op b) = .ok w → ∃ q, w = .int t q ∧ t.Mem q := by
    intro hop t hvt w hw
    obtain ⟨-, -, q, -, -, hq, hm, -⟩ := infix_runtime hop hvt hda3 hdb hw
    exact ⟨q, hq, hm⟩
  have hint : intResult op = true → ∀ w, evalExpr env.prog E (.infix a op b) = .ok w →
      ∃ t, va.ty = .int t := by
    intro hop w hw
    obtain ⟨wa, wb, hwa, -, h⟩ := hrun w hw
    obtain ⟨t, x, rfl⟩ := evalInfix_int_left hop h
    exact ⟨t, (hda3 _ hwa).typed.int_ty⟩
  have hbool : intResult op = false → ∀ w, evalExpr env.prog E (.infix a op b) = .ok w →
      ({ gexpr := mk2 (GExpr.infix op) va.gexpr vb.gexpr, ty := .bool } : Val).Denotes
        (valueOf env.vars E) ν G3 w := by
    intro hop w hw
    obtain ⟨wa, wb, -, -, h⟩ := hrun w hw
    exact bool_val_denotes (hgx w hw) (evalInfix_bool hop h)
  -- The operands' images give the runtime operands.
  have himgs : ∀ fa fb, va.image = some fa → vb.image = some fb → ∀ w,
      evalExpr env.prog E (.infix a op b) = .ok w → intResult op = true → ∀ t,
      va.ty = .int t → ∃ x y q, w = .int t q ∧ x = fa.eval G3 ∧ y = fb.eval G3 ∧
        ∀ o, op = .arith o .exact → q = o.math x y := by
    intro fa fb hfa hfb w hw hop t hvt
    obtain ⟨x, y, q, hwa, hwb, hq, -, hm⟩ := infix_runtime hop hvt hda3 hdb hw
    obtain ⟨t1, h1⟩ := (hda3 _ hwa).image fa hfa
    obtain ⟨t2, h2⟩ := (hdb _ hwb).image fb hfb
    injection h1 with _ hx
    injection h2 with _ hy
    exact ⟨x, y, q, hq, hx, hy, hm⟩
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases m
    · -- exact arithmetic
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        cases o
        · int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          cases hfa : va.image with
          | none =>
            simp only [hfa] at himg
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              simp only [hfa, hfb] at himg
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              simp only [hfa, hfb] at himg
              obtain ⟨rfl, rfl⟩ := Discharge.run_pure himg
              refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
              obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
              (rw [hm .add rfl, Form.eval_add, ← hx, ← hy]) <;> rfl
        · int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          cases hfa : va.image with
          | none =>
            simp only [hfa] at himg
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              simp only [hfa, hfb] at himg
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              simp only [hfa, hfb] at himg
              obtain ⟨rfl, rfl⟩ := Discharge.run_pure himg
              refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
              obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
              (rw [hm .sub rfl, Form.eval_sub, ← hx, ← hy]) <;> rfl
        · int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          cases hfa : va.image with
          | none =>
            simp only [hfa] at himg
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              simp only [hfa, hfb] at himg
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              simp only [hfa, hfb] at himg
              split at himg
              · rename_i hc
                obtain ⟨rfl, rfl⟩ := Discharge.run_pure himg
                refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
                obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
                (rw [hm .mul rfl, Form.eval_scale, ← Form.eval_isConst (G := G3) hc, ← hx,
                  ← hy]) <;> rfl
              · split at himg
                · rename_i _ hc
                  obtain ⟨rfl, rfl⟩ := Discharge.run_pure himg
                  refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx
                    (Or.inr ⟨rfl, fun w hw => ?_⟩)
                  obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
                  rw [hm .mul rfl, Form.eval_scale, ← Form.eval_isConst (G := G3) hc, ← hx, ← hy]
                  show Value.int t (x * y) = Value.int t (y * x)
                  rw [Int.mul_comm]
                · exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- wrapping arithmetic
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        cases o
        all_goals
          int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- `defined` arithmetic
      dsimp only at hr
      obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
      exact ⟨G3, hg3, hext23, hbool rfl⟩
  · cases m
    · -- exact division and remainder
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        cases o
        all_goals
          int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- wrapping division, which the interpreter does not take
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        cases o
        all_goals
          int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := Discharge.run_bind hr
          obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- `defined` division
      dsimp only at hr
      obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
      exact ⟨G3, hg3, hext23, hbool rfl⟩
  · -- comparisons
    dsimp only at hr
    obtain ⟨rfl, rfl⟩ := Discharge.run_pure hr
    exact ⟨G3, hg3, hext23, hbool rfl⟩

end Checker

end Whitefoot
