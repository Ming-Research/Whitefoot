import WhitefootProof.Sealing

/-!
# Soundness of the sealing rules

That the sealing rules of `Sealing.lean` keep their invariant: when the
sealing state agrees with a run before an event the rules accept, it agrees
with the run after it (`Ctx.step_holds`); the state at a merge or a loop
header agrees with the run arriving on any of its edges (`Ctx.join_holds`,
`Ctx.header_entry_holds`, `Ctx.header_back_holds`); a hand-off the rules
accept hands off a valid value (`Ctx.handoff_valid`); and an instance the
rules establish is true (`Ctx.instance_true`). Each rests on the premises
`Sem` and `Ctx.RunStep` state of the fact core, the callee and the rest of the
checker.
-/

namespace Whitefoot.Seal

variable {T F : Type}

/-! ## Lists -/

theorem nth_upd_same {α : Type} : ∀ (l : List α) (n : Nat) (b : α),
    (nth l n).isSome = true → nth (upd l n b) n = some b
  | [], _, _, h => by simp [nth] at h
  | _ :: _, 0, _, _ => rfl
  | _ :: l, n + 1, b, h => nth_upd_same l n b h

theorem nth_upd_other {α : Type} : ∀ (l : List α) (n m : Nat) (b : α),
    m ≠ n → nth (upd l n b) m = nth l m
  | [], _, _, _, _ => rfl
  | _ :: _, 0, 0, _, h => absurd rfl h
  | _ :: _, 0, _ + 1, _, _ => rfl
  | _ :: _, _ + 1, 0, _, _ => rfl
  | _ :: l, n + 1, m + 1, b, h => nth_upd_other l n m b (by omega)

/-- Two lists that begin one list are prefix-related. -/
theorem append_prefix_cases {α : Type} : ∀ (a b c d : List α), a ++ b = c ++ d →
    (∃ e, a ++ e = c) ∨ (∃ e, c ++ e = a)
  | [], _, c, _, _ => Or.inl ⟨c, rfl⟩
  | x :: a, _, [], _, _ => Or.inr ⟨x :: a, rfl⟩
  | x :: a, b, y :: c, d, h => by
    simp only [List.cons_append, List.cons.injEq] at h
    obtain ⟨rfl, h⟩ := h
    rcases append_prefix_cases a b c d h with ⟨e, he⟩ | ⟨e, he⟩
    · exact Or.inl ⟨e, by rw [List.cons_append, he]⟩
    · exact Or.inr ⟨e, by rw [List.cons_append, he]⟩

theorem mem_eq_of_length_lt_two {α : Type} : ∀ {L : List α} {a b : α},
    L.length < 2 → a ∈ L → b ∈ L → a = b
  | [], _, _, _, ha, _ => by cases ha
  | [_], _, _, _, ha, hb => by
    rw [List.mem_singleton] at ha hb
    rw [ha, hb]
  | _ :: _ :: l, _, _, h, _, _ => by simp only [List.length_cons] at h; omega

/-- Two runtime paths are prefix-related: one begins the other. -/
def Prefixed (c₁ c₂ : List CStep) : Prop := (∃ e, c₁ ++ e = c₂) ∨ (∃ e, c₂ ++ e = c₁)

theorem Prefixed.cons {c c' : CStep} {cs cs' : List CStep} (h : Prefixed (c :: cs) (c' :: cs')) :
    c = c' ∧ Prefixed cs cs' := by
  rcases h with ⟨e, he⟩ | ⟨e, he⟩
  · simp only [List.cons_append, List.cons.injEq] at he
    exact ⟨he.1, Or.inl ⟨e, he.2⟩⟩
  · simp only [List.cons_append, List.cons.injEq] at he
    exact ⟨he.1.symm, Or.inr ⟨e, he.2⟩⟩

/-! ## Values -/

theorem CStep.eq_of_fits (s s' : CStep) (k : VKind) (t : Nat)
    (h : s.fits k t = true) (h' : s'.fits k t = true) (hc : s.child = s'.child) : s = s' := by
  cases s <;> cases s' <;> simp_all [CStep.fits, CStep.child]

theorem Val.child_replace_same {v c c' : Val} {s : CStep} (h : Val.child v s = some c) :
    Val.child (Val.replace v s c') s = some c' := by
  cases v with
  | int x => simp [Val.child] at h
  | node k t cs =>
    cases hf : s.fits k t with
    | false => simp [Val.child, hf] at h
    | true =>
      have h1 : nth cs s.child = some c := by simpa [Val.child, hf] using h
      have h2 : Val.child (Val.replace (.node k t cs) s c') s = nth (upd cs s.child c') s.child := by
        simp [Val.replace, Val.child, hf]
      rw [h2]
      exact nth_upd_same cs s.child c' (by rw [h1]; rfl)

theorem Val.child_replace_other {v c c' : Val} {s s' : CStep} (h : Val.child v s = some c)
    (hne : s' ≠ s) : Val.child (Val.replace v s c') s' = Val.child v s' := by
  cases v with
  | int x => simp [Val.child] at h
  | node k t cs =>
    cases hf : s.fits k t with
    | false => simp [Val.child, hf] at h
    | true =>
      cases hf' : s'.fits k t with
      | false => simp [Val.replace, Val.child, hf']
      | true =>
        have hc : s'.child ≠ s.child := fun e => hne (CStep.eq_of_fits s' s k t hf' hf e)
        have e1 : Val.child (Val.replace (.node k t cs) s c') s' = nth (upd cs s.child c') s'.child := by
          simp [Val.replace, Val.child, hf']
        have e2 : Val.child (.node k t cs) s' = nth cs s'.child := by
          simp [Val.child, hf']
        rw [e1, e2]
        exact nth_upd_other cs s.child s'.child c' hc

theorem Val.get_cons_of (v c : Val) (s : CStep) (p : List CStep) (h : Val.child v s = some c) :
    Val.get v (s :: p) = Val.get c p := by
  simp [Val.get, h]

theorem Val.get_cons_none (v : Val) (s : CStep) (p : List CStep) (h : Val.child v s = none) :
    Val.get v (s :: p) = none := by
  simp [Val.get, h]

theorem Val.get_append_some (q : List CStep) : ∀ (p : List CStep) (v u : Val),
    Val.get v p = some u → Val.get v (p ++ q) = Val.get u q
  | [], v, u, h => by
    have : v = u := by simpa [Val.get] using h
    subst this; rfl
  | s :: p, v, u, h => by
    cases hc : Val.child v s with
    | none => rw [Val.get_cons_none v s p hc] at h; cases h
    | some c =>
      rw [Val.get_cons_of v c s p hc] at h
      rw [List.cons_append, Val.get_cons_of v c s (p ++ q) hc]
      exact Val.get_append_some q p c u h

theorem Val.get_append_none (q : List CStep) : ∀ (p : List CStep) (v : Val),
    Val.get v p = none → Val.get v (p ++ q) = none
  | [], v, h => by simp [Val.get] at h
  | s :: p, v, h => by
    cases hc : Val.child v s with
    | none => rw [List.cons_append, Val.get_cons_none v s (p ++ q) hc]
    | some c =>
      rw [Val.get_cons_of v c s p hc] at h
      rw [List.cons_append, Val.get_cons_of v c s (p ++ q) hc]
      exact Val.get_append_none q p c h

theorem Val.get_append_inv (q p : List CStep) (v w : Val) (h : Val.get v (p ++ q) = some w) :
    ∃ u, Val.get v p = some u ∧ Val.get u q = some w := by
  cases hp : Val.get v p with
  | none => rw [Val.get_append_none q p v hp] at h; cases h
  | some u => exact ⟨u, rfl, by rw [← Val.get_append_some q p v u hp]; exact h⟩

theorem Val.set_cons (v w : Val) (s : CStep) (p : List CStep) (v' : Val)
    (h : Val.set v (s :: p) w = some v') :
    ∃ c c', Val.child v s = some c ∧ Val.set c p w = some c' ∧ v' = Val.replace v s c' := by
  cases hc : Val.child v s with
  | none => simp [Val.set, hc] at h
  | some c =>
    cases hs : Val.set c p w with
    | none => simp [Val.set, hc, hs] at h
    | some c' =>
      simp [Val.set, hc, hs] at h
      exact ⟨c, c', rfl, rfl, by rw [h]⟩

theorem Val.set_get_same (w : Val) : ∀ (p : List CStep) (v v' : Val),
    Val.set v p w = some v' → Val.get v' p = some w
  | [], v, v', h => by
    have : w = v' := by simpa [Val.set] using h
    subst this; rfl
  | s :: p, v, v', h => by
    obtain ⟨c, c', hc, hs, rfl⟩ := Val.set_cons v w s p v' h
    rw [Val.get_cons_of _ c' s p (Val.child_replace_same (c' := c') hc)]
    exact Val.set_get_same w p c c' hs

theorem Val.set_get_isSome (w : Val) : ∀ (p : List CStep) (v v' : Val),
    Val.set v p w = some v' → (Val.get v p).isSome = true
  | [], _, _, _ => rfl
  | s :: p, v, v', h => by
    obtain ⟨c, c', hc, hs, _⟩ := Val.set_cons v w s p v' h
    rw [Val.get_cons_of v c s p hc]
    exact Val.set_get_isSome w p c c' hs

theorem Val.set_get_apart (w : Val) : ∀ (p q : List CStep) (v v' : Val),
    Val.set v p w = some v' → (∀ e, p ++ e ≠ q) → (∀ e, q ++ e ≠ p) → Val.get v' q = Val.get v q
  | [], q, _, _, _, h1, _ => (h1 q rfl).elim
  | x :: p, [], _, _, _, _, h2 => (h2 (x :: p) rfl).elim
  | s :: p, s' :: q, v, v', h, h1, h2 => by
    obtain ⟨c, c', hc, hs, rfl⟩ := Val.set_cons v w s p v' h
    by_cases he : s' = s
    · subst he
      rw [Val.get_cons_of _ c' _ q (Val.child_replace_same (c' := c') hc), Val.get_cons_of v c _ q hc]
      exact Val.set_get_apart w p q c c' hs
        (fun e he' => h1 e (by rw [List.cons_append, he']))
        (fun e he' => h2 e (by rw [List.cons_append, he']))
    · simp only [Val.get, Val.child_replace_other (c' := c') hc he]

/-! ## Runs -/

theorem Run.get_of_vals {r : Run} {π : CPlace} {v : Val} (hv : r.vals π.root = some v) :
    r.get π = Val.get v π.path := by
  simp [Run.get, hv]

theorem Run.get_of_vals_none {r : Run} {π : CPlace} (hv : r.vals π.root = none) : r.get π = none := by
  simp [Run.get, hv]

theorem Run.get_some {r : Run} {π : CPlace} {u : Val} (h : r.get π = some u) :
    ∃ v, r.vals π.root = some v ∧ Val.get v π.path = some u := by
  cases hv : r.vals π.root with
  | none => rw [Run.get_of_vals_none hv] at h; cases h
  | some v => exact ⟨v, rfl, by rw [← Run.get_of_vals hv]; exact h⟩

theorem Run.get_append_some {r : Run} {π : CPlace} {u : Val} (e : List CStep)
    (h : r.get π = some u) : r.get ⟨π.root, π.path ++ e⟩ = Val.get u e := by
  obtain ⟨v, hv, hg⟩ := Run.get_some h
  rw [Run.get_of_vals (π := ⟨π.root, π.path ++ e⟩) hv]
  exact Val.get_append_some e π.path v u hg

theorem Run.get_append_inv {r : Run} {π : CPlace} {w : Val} (e : List CStep)
    (h : r.get ⟨π.root, π.path ++ e⟩ = some w) : ∃ u, r.get π = some u ∧ Val.get u e = some w := by
  obtain ⟨v, hv, hg⟩ := Run.get_some h
  obtain ⟨u, hu, hu'⟩ := Val.get_append_inv e π.path v w hg
  exact ⟨u, by rw [Run.get_of_vals (π := π) hv]; exact hu, hu'⟩

theorem Run.get_prefix_isSome {r : Run} {π : CPlace} (e : List CStep)
    (h : (r.get ⟨π.root, π.path ++ e⟩).isSome = true) : (r.get π).isSome = true := by
  cases hg : r.get ⟨π.root, π.path ++ e⟩ with
  | none => rw [hg] at h; cases h
  | some w =>
    obtain ⟨u, hu, _⟩ := Run.get_append_inv e hg
    simp [hu]

theorem Run.set_some {r r' : Run} {π : CPlace} {w : Val} (h : r.set π w = some r') :
    ∃ v v', r.vals π.root = some v ∧ Val.set v π.path w = some v' ∧
      r' = { r with vals := override r.vals π.root (some v') } := by
  cases hv : r.vals π.root with
  | none => simp [Run.set, hv] at h
  | some v =>
    cases hs : Val.set v π.path w with
    | none => simp [Run.set, hv, hs] at h
    | some v' =>
      simp [Run.set, hv, hs] at h
      exact ⟨v, v', rfl, rfl, by rw [h]⟩

theorem Run.set_refs {r r' : Run} {π : CPlace} {w : Val} (h : r.set π w = some r') :
    r'.refs = r.refs := by
  obtain ⟨v, v', _, _, rfl⟩ := Run.set_some h
  rfl

theorem Run.set_get_below {r r' : Run} {π : CPlace} {w : Val} (h : r.set π w = some r')
    (e : List CStep) : r'.get ⟨π.root, π.path ++ e⟩ = Val.get w e := by
  obtain ⟨v, v', _, hs, rfl⟩ := Run.set_some h
  have hv' : ({ r with vals := override r.vals π.root (some v') } : Run).vals π.root = some v' := by
    simp [override]
  rw [Run.get_of_vals (π := ⟨π.root, π.path ++ e⟩) hv']
  exact Val.get_append_some e π.path v' w (Val.set_get_same w π.path v v' hs)

theorem Run.set_get_isSome {r r' : Run} {π : CPlace} {w : Val} (h : r.set π w = some r') :
    (r.get π).isSome = true := by
  obtain ⟨v, v', hv, hs, _⟩ := Run.set_some h
  rw [Run.get_of_vals hv]
  exact Val.set_get_isSome w π.path v v' hs

theorem Run.set_get_apart {r r' : Run} {π ρ : CPlace} {w : Val} (h : r.set π w = some r')
    (hap : π.Apart ρ) : r'.get ρ = r.get ρ := by
  obtain ⟨hap1, hap2⟩ := hap
  obtain ⟨v, v', hv, hs, rfl⟩ := Run.set_some h
  by_cases hroot : ρ.root = π.root
  · have hv' : ({ r with vals := override r.vals π.root (some v') } : Run).vals ρ.root = some v' := by
      simp [override, hroot]
    have hvρ : r.vals ρ.root = some v := by rw [hroot]; exact hv
    rw [Run.get_of_vals hv', Run.get_of_vals hvρ]
    apply Val.set_get_apart w π.path ρ.path v v' hs
    · intro e he
      exact hap2 ⟨hroot, e, he⟩
    · intro e he
      exact hap1 ⟨hroot.symm, e, he⟩
  · have hvx : ({ r with vals := override r.vals π.root (some v') } : Run).vals ρ.root =
        r.vals ρ.root := by
      simp [override, hroot]
    simp only [Run.get, hvx]

/-! ## Places -/

theorem CPlace.eq_mk {π : CPlace} {x : String} {p : List CStep} (h1 : π.root = x) (h2 : π.path = p) :
    π = ⟨x, p⟩ := by
  cases π; simp_all

theorem CPlace.below_refl (π : CPlace) : π.Below π := ⟨rfl, [], List.append_nil _⟩

theorem CPlace.below_append (π : CPlace) (e : List CStep) : CPlace.Below ⟨π.root, π.path ++ e⟩ π :=
  ⟨rfl, e, rfl⟩

theorem CPlace.below_trans {π ρ σ : CPlace} (h1 : π.Below ρ) (h2 : ρ.Below σ) : π.Below σ := by
  obtain ⟨r1, s1, e1⟩ := h1
  obtain ⟨r2, s2, e2⟩ := h2
  exact ⟨r1.trans r2, s2 ++ s1, by rw [← List.append_assoc, e2, e1]⟩

/-- A place below another is the other followed by a path. -/
theorem CPlace.below_eq {π ρ : CPlace} (h : π.Below ρ) : ∃ e, π = ⟨ρ.root, ρ.path ++ e⟩ := by
  obtain ⟨hr, e, he⟩ := h
  exact ⟨e, CPlace.eq_mk hr he.symm⟩

/-! ## Types -/

theorem Decls.walk_append_some (D : Decls) : ∀ (t t' : STy) (ks ks' : List Kind),
    D.walk t ks = some t' → D.walk t (ks ++ ks') = D.walk t' ks'
  | t, t', [], ks', h => by
    have : t = t' := by simpa [Decls.walk] using h
    subst this; rfl
  | t, t', k :: ks, ks', h => by
    cases hk : D.stepTy t k with
    | none => simp [Decls.walk, hk] at h
    | some t1 =>
      have h' : D.walk t1 ks = some t' := by simpa [Decls.walk, hk] using h
      have e : D.walk t (k :: ks ++ ks') = D.walk t1 (ks ++ ks') := by
        simp [Decls.walk, hk]
      rw [e]
      exact Decls.walk_append_some D t1 t' ks ks' h'

theorem Decls.walk_append_none (D : Decls) : ∀ (t : STy) (ks ks' : List Kind),
    D.walk t ks = none → D.walk t (ks ++ ks') = none
  | t, [], ks', h => by simp [Decls.walk] at h
  | t, k :: ks, ks', h => by
    cases hk : D.stepTy t k with
    | none => simp [Decls.walk, hk]
    | some t1 =>
      have h' : D.walk t1 ks = none := by simpa [Decls.walk, hk] using h
      have e : D.walk t (k :: ks ++ ks') = D.walk t1 (ks ++ ks') := by
        simp [Decls.walk, hk]
      rw [e]
      exact Decls.walk_append_none D t1 ks ks' h'

theorem Ctx.tyOf_append_some (X : Ctx T F) {π : CPlace} {t : STy} (e : List CStep)
    (h : X.tyOf π = some t) : X.tyOf ⟨π.root, π.path ++ e⟩ = X.decls.walk t (e.map CStep.kind) := by
  cases ht : X.tys π.root with
  | none => simp [Ctx.tyOf, ht] at h
  | some t0 =>
    have h' : X.decls.walk t0 (π.path.map CStep.kind) = some t := by simpa [Ctx.tyOf, ht] using h
    have e1 : X.tyOf ⟨π.root, π.path ++ e⟩ =
        X.decls.walk t0 (π.path.map CStep.kind ++ e.map CStep.kind) := by
      simp [Ctx.tyOf, ht]
    rw [e1]
    exact Decls.walk_append_some _ _ _ _ _ h'

theorem Ctx.tyOf_append_none (X : Ctx T F) {π : CPlace} (e : List CStep)
    (h : X.tyOf π = none) : X.tyOf ⟨π.root, π.path ++ e⟩ = none := by
  cases ht : X.tys π.root with
  | none => simp [Ctx.tyOf, ht]
  | some t0 =>
    have h' : X.decls.walk t0 (π.path.map CStep.kind) = none := by simpa [Ctx.tyOf, ht] using h
    have e1 : X.tyOf ⟨π.root, π.path ++ e⟩ =
        X.decls.walk t0 (π.path.map CStep.kind ++ e.map CStep.kind) := by
      simp [Ctx.tyOf, ht]
    rw [e1]
    exact Decls.walk_append_none _ _ _ _ h'

theorem Ctx.mem_invsOfPlace {X : Ctx T F} {π : CPlace} {I : InvId} (h : I ∈ X.invsOfPlace π) :
    ∃ t, X.tyOf π = some t ∧ I ∈ X.decls.invsOf t := by
  cases ht : X.tyOf π with
  | none => simp [Ctx.invsOfPlace, ht] at h
  | some t => exact ⟨t, rfl, by simpa [Ctx.invsOfPlace, ht] using h⟩

theorem Ctx.mem_invsOfPlace_of {X : Ctx T F} {π : CPlace} {I : InvId} {t : STy}
    (ht : X.tyOf π = some t) (h : I ∈ X.decls.invsOf t) : I ∈ X.invsOfPlace π := by
  simpa [Ctx.invsOfPlace, ht] using h

theorem Ctx.mem_invsBelow {X : Ctx T F} {t : STy} {e : List CStep} {I : InvId}
    (h : I ∈ X.invsBelow t e) :
    ∃ t', X.decls.walk t (e.map CStep.kind) = some t' ∧ I ∈ X.decls.invsOf t' := by
  cases hw : X.decls.walk t (e.map CStep.kind) with
  | none => simp [Ctx.invsBelow, hw] at h
  | some t' => exact ⟨t', rfl, by simpa [Ctx.invsBelow, hw] using h⟩

theorem Ctx.mem_invsBelow_of_append {X : Ctx T F} {π : CPlace} {e : List CStep} {I : InvId}
    {t : STy} (ht : X.tyOf π = some t) (h : I ∈ X.invsOfPlace ⟨π.root, π.path ++ e⟩) :
    I ∈ X.invsBelow t e := by
  obtain ⟨t', ht', hI⟩ := Ctx.mem_invsOfPlace h
  rw [Ctx.tyOf_append_some X e ht] at ht'
  simpa [Ctx.invsBelow, ht'] using hI

theorem Ctx.mem_invsOfPlace_append_of {X : Ctx T F} {π : CPlace} {e : List CStep} {I : InvId}
    {t : STy} (ht : X.tyOf π = some t) (h : I ∈ X.invsBelow t e) :
    I ∈ X.invsOfPlace ⟨π.root, π.path ++ e⟩ := by
  obtain ⟨t', hw, hI⟩ := Ctx.mem_invsBelow h
  exact Ctx.mem_invsOfPlace_of (by rw [Ctx.tyOf_append_some X e ht]; exact hw) hI

theorem Ctx.tyOf_of_mem_append {X : Ctx T F} {π : CPlace} {e : List CStep} {I : InvId}
    (h : I ∈ X.invsOfPlace ⟨π.root, π.path ++ e⟩) : ∃ t, X.tyOf π = some t := by
  obtain ⟨t', ht', _⟩ := Ctx.mem_invsOfPlace h
  cases ht : X.tyOf π with
  | none => rw [Ctx.tyOf_append_none X e ht] at ht'; cases ht'
  | some t => exact ⟨t, rfl⟩

theorem Ctx.tyOf_append_congr (X : Ctx T F) {π ρ : CPlace} (h : X.tyOf π = X.tyOf ρ)
    (e : List CStep) : X.tyOf ⟨π.root, π.path ++ e⟩ = X.tyOf ⟨ρ.root, ρ.path ++ e⟩ := by
  cases ht : X.tyOf π with
  | none =>
    rw [ht] at h
    rw [Ctx.tyOf_append_none X e ht, Ctx.tyOf_append_none X e h.symm]
  | some t =>
    rw [ht] at h
    rw [Ctx.tyOf_append_some X e ht, Ctx.tyOf_append_some X e h.symm]

theorem Ctx.tyOf_rebase (X : Ctx T F) {π : CPlace} {y : String} (h : X.tyOf π = X.tys y)
    (e : List CStep) : X.tyOf ⟨y, e⟩ = X.tyOf ⟨π.root, π.path ++ e⟩ := by
  cases ht : X.tyOf π with
  | none =>
    rw [Ctx.tyOf_append_none X e ht]
    rw [ht] at h
    simp [Ctx.tyOf, ← h]
  | some t =>
    rw [Ctx.tyOf_append_some X e ht]
    rw [ht] at h
    simp [Ctx.tyOf, ← h]

theorem Ctx.invsOfPlace_congr (X : Ctx T F) {π ρ : CPlace} (h : X.tyOf π = X.tyOf ρ) :
    X.invsOfPlace π = X.invsOfPlace ρ := by
  unfold Ctx.invsOfPlace; rw [h]

theorem Ctx.invHolds_congr (X : Ctx T F) {r r' : Run} {π ρ : CPlace} (I : InvId)
    (h : r'.get ρ = r.get π) : X.InvHolds r' ρ I ↔ X.InvHolds r π I := by
  unfold Ctx.InvHolds; rw [h]

theorem Ctx.tyAt_eq (X : Ctx T F) {a : Shape T} {π : CPlace} (h1 : a.root = π.root)
    (h2 : a.steps.map SStep.kind = π.path.map CStep.kind) : X.tyAt a = X.tyOf π := by
  cases ht : X.tys π.root with
  | none => simp [Ctx.tyAt, Ctx.tyOf, h1, ht]
  | some t => simp [Ctx.tyAt, Ctx.tyOf, h1, ht, h2]

theorem Ctx.invsAt_eq (X : Ctx T F) {a : Shape T} {π : CPlace} (h1 : a.root = π.root)
    (h2 : a.steps.map SStep.kind = π.path.map CStep.kind) : X.invsAt a = X.invsOfPlace π := by
  have e := Ctx.tyAt_eq X h1 h2
  cases ht : X.tyOf π with
  | none => simp [Ctx.invsAt, Ctx.invsOfPlace, e, ht]
  | some t => simp [Ctx.invsAt, Ctx.invsOfPlace, e, ht]

/-! ## Denotation -/

section Denote

variable {ev : Run → T → Option Int} {r : Run}

theorem StepsDenote.length {ss : List (SStep T)} {cs : List CStep} (h : StepsDenote ev r ss cs) :
    ss.length = cs.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem StepDenotes.kind_eq {s : SStep T} {c : CStep} (h : StepDenotes ev r s c) :
    s.kind = c.kind := by
  cases h <;> rfl

theorem StepsDenote.kinds {ss : List (SStep T)} {cs : List CStep} (h : StepsDenote ev r ss cs) :
    ss.map SStep.kind = cs.map CStep.kind := by
  induction h with
  | nil => rfl
  | cons hd _ ih => simp [ih, hd.kind_eq]

theorem StepsDenote.append {ss ss' : List (SStep T)} {cs cs' : List CStep}
    (h : StepsDenote ev r ss cs) (h' : StepsDenote ev r ss' cs') :
    StepsDenote ev r (ss ++ ss') (cs ++ cs') := by
  induction h with
  | nil => exact h'
  | cons hd _ ih => exact StepsDenote.cons hd ih

theorem StepsDenote.split {ss' : List (SStep T)} : ∀ {ss : List (SStep T)} {cs : List CStep},
    StepsDenote ev r (ss ++ ss') cs →
      ∃ c₁ c₂, cs = c₁ ++ c₂ ∧ StepsDenote ev r ss c₁ ∧ StepsDenote ev r ss' c₂ := by
  intro ss
  induction ss with
  | nil => intro cs h; exact ⟨[], cs, rfl, .nil, h⟩
  | cons s ss ih =>
    intro cs h
    rw [List.cons_append] at h
    cases h with
    | cons hd tl =>
      obtain ⟨c₁, c₂, rfl, h1, h2⟩ := ih tl
      exact ⟨_ :: c₁, c₂, rfl, .cons hd h1, h2⟩

theorem StepsDenote.split_path {c₂ : List CStep} : ∀ {c₁ : List CStep} {ss : List (SStep T)},
    StepsDenote ev r ss (c₁ ++ c₂) →
      ∃ s₁ s₂, ss = s₁ ++ s₂ ∧ StepsDenote ev r s₁ c₁ ∧ StepsDenote ev r s₂ c₂ := by
  intro c₁
  induction c₁ with
  | nil => intro ss h; exact ⟨[], ss, rfl, .nil, h⟩
  | cons c c₁ ih =>
    intro ss h
    rw [List.cons_append] at h
    cases h with
    | cons hd tl =>
      obtain ⟨s₁, s₂, rfl, h1, h2⟩ := ih tl
      exact ⟨_ :: s₁, s₂, rfl, .cons hd h1, h2⟩

theorem StepDenotes.unique {s : SStep T} {c c' : CStep} (hk : s.known = true)
    (h : StepDenotes ev r s c) (h' : StepDenotes ev r s c') : c = c' := by
  cases h with
  | field i => cases h'; rfl
  | inner => cases h'; rfl
  | unknown k => simp [SStep.known] at hk
  | cap t k e =>
    cases h' with
    | cap _ k' e' =>
      rw [e] at e'
      have h1 : (k : Int) = (k' : Int) := Option.some.inj e'
      have h2 : k = k' := by omega
      rw [h2]
  | payload v i => cases h'; rfl

theorem StepsDenote.unique {ss : List (SStep T)} {cs : List CStep} (h : StepsDenote ev r ss cs) :
    ∀ {cs' : List CStep}, ss.all SStep.known = true → StepsDenote ev r ss cs' → cs = cs' := by
  induction h with
  | nil => intro cs' _ h'; cases h'; rfl
  | cons hd _ ih =>
    intro cs' hk h'
    cases h' with
    | cons hd' tl' =>
      simp only [List.all_cons, Bool.and_eq_true] at hk
      rw [hd.unique hk.1 hd', ih hk.2 tl']

theorem StepsDenote.lift : ∀ (cs : List CStep), StepsDenote ev r (cs.map CStep.lift) cs
  | [] => .nil
  | c :: cs => .cons (by cases c <;> constructor) (StepsDenote.lift cs)

end Denote

theorem mem_offsets_cons {s : SStep T} {ss : List (SStep T)} {t : T} :
    t ∈ offsets (s :: ss) ↔ SStep.elem (.cap t) = s ∨ t ∈ offsets ss := by
  cases s with
  | field i => simp [offsets]
  | inner => simp [offsets]
  | elem ix =>
    cases ix with
    | cap t' => simp [offsets]
    | unknown => simp [offsets]
  | payload v i => simp [offsets]

/-! ## Kill batches -/

section Kills

variable {X : Ctx T F} {M : Sem X} {f : F} {kb : Kills T} {r r' : Run}

theorem StepDenotes.frame (hfr : M.Frame f kb r r') {s : SStep T} {c : CStep}
    (hs : ∀ t, SStep.elem (.cap t) = s → X.core.killed f kb t = false)
    (h : StepDenotes M.eval r s c) : StepDenotes M.eval r' s c := by
  cases h with
  | field i => exact .field i
  | inner => exact .inner
  | unknown k => exact .unknown k
  | cap t k e => exact .cap t k (by rw [hfr.1 t (hs t rfl)]; exact e)
  | payload v i => exact .payload v i

theorem StepsDenote.frame (hfr : M.Frame f kb r r') {ss : List (SStep T)} {cs : List CStep}
    (h : StepsDenote M.eval r ss cs) :
    (∀ t ∈ offsets ss, X.core.killed f kb t = false) → StepsDenote M.eval r' ss cs := by
  induction h with
  | nil => intro _; exact .nil
  | cons hd _ ih =>
    intro hk
    exact .cons (StepDenotes.frame hfr (fun t e => hk t (mem_offsets_cons.2 (Or.inl e))) hd)
      (ih (fun t ht => hk t (mem_offsets_cons.2 (Or.inr ht))))

theorem StepsDenote.widen (hfr : M.Frame f kb r r') {ss : List (SStep T)} {cs : List CStep}
    (h : StepsDenote M.eval r ss cs) : ∀ (after : Bool),
      StepsDenote M.eval r (X.widenSteps f kb after ss) cs ∧
        StepsDenote M.eval r' (X.widenSteps f kb after ss) cs := by
  induction h with
  | nil => intro _; exact ⟨.nil, .nil⟩
  | @cons s c ss cs hd tl ih =>
    intro after
    cases hd with
    | field i =>
      obtain ⟨h1, h2⟩ := ih after
      exact ⟨.cons (.field i) h1, .cons (.field i) h2⟩
    | inner =>
      obtain ⟨h1, h2⟩ := ih after
      exact ⟨.cons .inner h1, .cons .inner h2⟩
    | unknown k =>
      obtain ⟨h1, h2⟩ := ih after
      exact ⟨.cons (.unknown k) h1, .cons (.unknown k) h2⟩
    | payload v i =>
      obtain ⟨h1, h2⟩ := ih after
      exact ⟨.cons (.payload v i) h1, .cons (.payload v i) h2⟩
    | cap t k e =>
      cases hk : X.core.killed f kb t with
      | true =>
        have ew : X.widenSteps f kb after (.elem (.cap t) :: ss) =
            .elem .unknown :: X.widenSteps f kb true ss := by
          simp [Ctx.widenSteps, hk]
        rw [ew]
        obtain ⟨h1, h2⟩ := ih true
        exact ⟨.cons (.unknown k) h1, .cons (.unknown k) h2⟩
      | false =>
        cases hw : (after && X.core.tracked t) with
        | true =>
          have ew : X.widenSteps f kb after (.elem (.cap t) :: ss) =
              .elem .unknown :: X.widenSteps f kb true ss := by
            simp [Ctx.widenSteps, hk, hw]
          rw [ew]
          obtain ⟨h1, h2⟩ := ih true
          exact ⟨.cons (.unknown k) h1, .cons (.unknown k) h2⟩
        | false =>
          have ew : X.widenSteps f kb after (.elem (.cap t) :: ss) =
              .elem (.cap t) :: X.widenSteps f kb after ss := by
            simp [Ctx.widenSteps, hk, hw]
          rw [ew]
          obtain ⟨h1, h2⟩ := ih after
          exact ⟨.cons (.cap t k e) h1, .cons (.cap t k (by rw [hfr.1 t hk]; exact e)) h2⟩

theorem offsets_unkilled {ss : List (SStep T)}
    (h : ((offsets ss).all fun t => !X.core.killed f kb t) = true) :
    ∀ t ∈ offsets ss, X.core.killed f kb t = false := by
  intro t ht
  have h1 : (!X.core.killed f kb t) = true := List.all_eq_true.1 h t ht
  simpa using h1

end Kills

/-! ## Booleans -/

theorem band_true {a b : Bool} (h : (a && b) = true) : a = true ∧ b = true := by
  cases a <;> cases b <;> first | exact ⟨rfl, rfl⟩ | exact absurd h (by decide)

theorem bor_true {a b : Bool} (h : (a || b) = true) : a = true ∨ b = true := by
  cases a <;> cases b <;> first | exact Or.inl rfl | exact Or.inr rfl | exact absurd h (by decide)

/-! ## Debt sets -/

/-- A property of debts that dropping a recorded reference keeps. -/
def RecClosed (P : Debt T → Prop) : Prop := ∀ a I rc, P (.inv a I rc) → P (.inv a I none)

theorem covers_recClosed (ev : Run → T → Option Int) (r : Run) (π : CPlace) (I : InvId) :
    RecClosed (T := T) (fun e => e.Covers ev r π I) :=
  fun _ _ _ h => ⟨h.1, h.2.1, trivial⟩

theorem covers2_recClosed (ev : Run → T → Option Int) (r r' : Run) (π : CPlace) (I : InvId) :
    RecClosed (T := T) (fun e => e.Covers ev r π I ∧ e.Covers ev r' π I) :=
  fun _ _ _ h => ⟨⟨h.1.1, h.1.2.1, trivial⟩, ⟨h.2.1, h.2.2.1, trivial⟩⟩

theorem unite_left [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) {e x : Debt T}
    (h : P e) : P (e.unite x) := by
  cases e with
  | inv a I r₁ =>
    cases x with
    | inv b J r₂ =>
      by_cases hr : r₁ = r₂
      · simp only [Debt.unite, if_pos hr]; exact h
      · simp only [Debt.unite, if_neg hr]; exact hP a I r₁ h
    | cover R => exact h
  | cover R => exact h

theorem unite_right [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) {e x : Debt T}
    (hk : e.sameKey x = true) (h : P x) : P (e.unite x) := by
  cases e with
  | inv a I r₁ =>
    cases x with
    | inv b J r₂ =>
      have hk' : (decide (a = b) && decide (I = J)) = true := hk
      obtain ⟨hab, hIJ⟩ := band_true hk'
      have hab' : a = b := of_decide_eq_true hab
      have hIJ' : I = J := of_decide_eq_true hIJ
      subst hab' hIJ'
      by_cases hr : r₁ = r₂
      · simp only [Debt.unite, if_pos hr]; rw [hr]; exact h
      · simp only [Debt.unite, if_neg hr]; exact hP _ _ _ h
    | cover R => cases hk
  | cover R =>
    cases x with
    | inv b J r₂ => cases hk
    | cover S =>
      have hk' : decide (R = S) = true := hk
      have hRS : R = S := of_decide_eq_true hk'
      subst hRS; exact h

theorem addDebt_keep [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) {D : List (Debt T)}
    {x d : Debt T} (hd : d ∈ D) (h : P d) : ∃ e ∈ addDebt D x, P e := by
  unfold addDebt
  split
  · refine ⟨if d.sameKey x then d.unite x else d, List.mem_map.2 ⟨d, hd, rfl⟩, ?_⟩
    split
    · exact unite_left hP h
    · exact h
  · exact ⟨d, List.mem_append.2 (Or.inl hd), h⟩

theorem addDebt_new [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) {D : List (Debt T)}
    {x : Debt T} (h : P x) : ∃ e ∈ addDebt D x, P e := by
  unfold addDebt
  split
  · rename_i hany
    obtain ⟨e, he, hk⟩ := List.any_eq_true.1 hany
    have hk' : e.sameKey x = true := hk
    refine ⟨if e.sameKey x then e.unite x else e, List.mem_map.2 ⟨e, he, rfl⟩, ?_⟩
    rw [if_pos hk']
    exact unite_right hP hk' h
  · exact ⟨x, List.mem_append.2 (Or.inr (List.mem_singleton.2 rfl)), h⟩

theorem addDebts_keep [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) :
    ∀ (ds : List (Debt T)) {D : List (Debt T)}, (∃ d ∈ D, P d) → ∃ e ∈ addDebts D ds, P e
  | [], _, h => h
  | x :: ds, D, ⟨_, hd, h⟩ => addDebts_keep hP ds (D := addDebt D x) (addDebt_keep hP hd h)

theorem addDebts_new [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P) :
    ∀ (ds : List (Debt T)) {D : List (Debt T)} {x : Debt T}, x ∈ ds → P x →
      ∃ e ∈ addDebts D ds, P e
  | [], _, _, hx, _ => by cases hx
  | y :: ds, D, x, hx, h => by
    rcases List.mem_cons.1 hx with rfl | hx'
    · exact addDebts_keep hP ds (D := addDebt D x) (addDebt_new hP h)
    · exact addDebts_new hP ds (D := addDebt D y) hx' h

theorem addDebts_new' [DecidableEq T] {P : Debt T → Prop} (hP : RecClosed P)
    {D ds : List (Debt T)} (h : ∃ x ∈ ds, P x) : ∃ e ∈ addDebts D ds, P e := by
  obtain ⟨x, hx, hp⟩ := h
  exact addDebts_new hP ds hx hp

/-! ## Restoration and satisfaction -/

section Judge

variable {X : Ctx T F}

theorem Ctx.restored_none {f : F} {a : Shape T} {J : InvId} (hk : a.steps.all SStep.known = true) :
    X.restored f (.inv a J none) = X.core.proves f ⟨.var a.root, a.steps⟩ J := by
  simp only [Ctx.restored, Debt.instancePlace?, if_pos hk]

theorem Ctx.restored_some {f : F} {a : Shape T} {J : InvId} {q : String} {p : List (SStep T)}
    (hk : a.steps.all SStep.known = true) :
    X.restored f (.inv a J (some ⟨q, p⟩)) = X.core.proves f ⟨.deref q, p⟩ J := by
  simp only [Ctx.restored, Debt.instancePlace?, if_pos hk]

theorem Ctx.restored_unknown {f : F} {a : Shape T} {J : InvId} {rc : Option (Rec T)}
    (hk : ¬ a.steps.all SStep.known = true) : X.restored f (.inv a J rc) = false := by
  simp only [Ctx.restored, Debt.instancePlace?, if_neg hk]

theorem Ctx.restored_cover {f : F} {R : Shape T} : X.restored f (.cover R) = false := by
  simp only [Ctx.restored, Debt.instancePlace?]

/-- A restored debt covers only places whose invariant holds. -/
theorem Ctx.restored_sound (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r) {d : Debt T}
    {π : CPlace} {I : InvId} (hc : d.Covers M.eval r π I) (hr : X.restored f d = true) :
    X.InvHolds r π I := by
  cases d with
  | cover R => rw [Ctx.restored_cover] at hr; cases hr
  | inv a J rc =>
    obtain ⟨rfl, ⟨ha1, ha2⟩, hrc⟩ := hc
    by_cases hk : a.steps.all SStep.known = true
    · cases rc with
      | none =>
        rw [Ctx.restored_none hk] at hr
        exact M.proves_sound f r _ _ π hf hr ⟨ha1.symm, ha2⟩
      | some rr =>
        obtain ⟨q, p⟩ := rr
        rw [Ctx.restored_some hk] at hr
        exact M.proves_sound f r _ _ π hf hr hrc
    · rw [Ctx.restored_unknown hk] at hr; cases hr

theorem Ctx.satisfied_inv {f : F} {m a : Shape T} {J : InvId} {rc : Option (Rec T)} [DecidableEq T]
    (h : X.satisfied f m (.inv a J rc) = true) :
    X.core.proves f ⟨.var a.root, m.steps ++ a.steps.drop m.steps.length⟩ J = true := by
  have h' : ((a.root == m.root && !(a.steps.all SStep.known) && agrees a.steps m.steps &&
      (a.steps.drop m.steps.length).all SStep.known) &&
      X.core.proves f ⟨.var a.root, m.steps ++ a.steps.drop m.steps.length⟩ J) = true := h
  exact (band_true h').2

/-! ## Overlap -/

theorem Ctx.sepStep_false (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r) {x y : SStep T}
    {c : CStep} (hx : StepDenotes M.eval r x c) (hy : StepDenotes M.eval r y c) :
    X.sepStep f x y = false := by
  cases hx with
  | field i => cases hy; simp [Ctx.sepStep]
  | inner => cases hy; rfl
  | unknown k => cases hy <;> rfl
  | cap s k e =>
    cases hy with
    | unknown _ => rfl
    | cap t _ e' =>
      cases hd : X.core.distinct f s t with
      | false => exact hd
      | true => exact absurd rfl (M.distinct_sound f r s t k k hf hd e e')
  | payload v i => cases hy; simp [Ctx.sepStep]

theorem Ctx.sepSteps_false (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r) {as : List (SStep T)}
    {c₁ : List CStep} (h1 : StepsDenote M.eval r as c₁) :
    ∀ {bs : List (SStep T)} {c₂ : List CStep}, StepsDenote M.eval r bs c₂ → Prefixed c₁ c₂ →
      X.sepSteps f as bs = false := by
  induction h1 with
  | nil => intro bs _ _ _; rfl
  | @cons s c ss cs hd _ ih =>
    intro bs c₂ h2 hp
    cases h2 with
    | nil => rfl
    | @cons t c' ts cs' hd' tl' =>
      obtain ⟨rfl, hp'⟩ := Prefixed.cons hp
      have e1 : X.sepSteps f (s :: ss) (t :: ts) = (X.sepStep f s t || X.sepSteps f ss ts) := rfl
      rw [e1, Ctx.sepStep_false M hf hd hd', ih tl' hp', Bool.or_false]

/-- Two resolved places that denote prefix-related places of one root may
overlap. -/
theorem Ctx.mayOverlap_of (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r) {a b : Shape T}
    {π ρ : CPlace} (ha : a.Denotes M.eval r π) (hb : b.Denotes M.eval r ρ)
    (hroot : π.root = ρ.root) (hp : Prefixed π.path ρ.path) : X.mayOverlap f a b = true := by
  obtain ⟨ha1, ha2⟩ := ha
  obtain ⟨hb1, hb2⟩ := hb
  have e1 : X.mayOverlap f a b = (a.root == b.root && !X.sepSteps f a.steps b.steps) := rfl
  rw [e1, Ctx.sepSteps_false M hf ha2 hb2 hp, ha1, hb1, hroot]
  simp

/-- A debt that covers a place at or below a member's place may be at or
below the member. -/
theorem Ctx.mayAtOrBelow_of_covers (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r) {m : Shape T}
    {π : CPlace} (hm : m.Denotes M.eval r π) {d : Debt T} {I : InvId} {e : List CStep}
    (hc : d.Covers M.eval r ⟨π.root, π.path ++ e⟩ I) : X.mayAtOrBelow f m d = true := by
  cases d with
  | inv a J rc =>
    obtain ⟨_, ha, _⟩ := hc
    have h1 := hm.2.length
    have h2 : a.steps.length = π.path.length + e.length := by
      rw [ha.2.length]; simp
    have hlen : m.steps.length ≤ a.steps.length := by omega
    have hov : X.mayOverlap f a m = true :=
      Ctx.mayOverlap_of M hf ha hm rfl (Or.inr ⟨e, rfl⟩)
    have e1 : X.mayAtOrBelow f m (.inv a J rc) =
        (decide (m.steps.length ≤ a.steps.length) && X.mayOverlap f a m) := rfl
    rw [e1, decide_eq_true hlen, hov, Bool.and_self]
  | cover R =>
    obtain ⟨ρ, hR, hb1, s, hs⟩ := hc
    exact Ctx.mayOverlap_of M hf hR hm hb1.symm (append_prefix_cases ρ.path s π.path e hs)

/-- A satisfied debt covers only places whose invariant holds. -/
theorem Ctx.satisfied_sound [DecidableEq T] (M : Sem X) {f : F} {r : Run} (hf : M.Facts f r)
    {m : Shape T} {π : CPlace} (hm : m.Denotes M.eval r π) {d : Debt T} {I : InvId}
    {e : List CStep} (hc : d.Covers M.eval r ⟨π.root, π.path ++ e⟩ I)
    (hs : X.satisfied f m d = true) : X.InvHolds r ⟨π.root, π.path ++ e⟩ I := by
  cases d with
  | cover R => cases hs
  | inv a J rc =>
    obtain ⟨rfl, ⟨ha1, ha2⟩, _⟩ := hc
    have hp := Ctx.satisfied_inv hs
    apply M.proves_sound f r _ _ _ hf hp
    obtain ⟨s₁, s₂, hsplit, h1, h2⟩ := StepsDenote.split_path ha2
    have hl : s₁.length = m.steps.length := by rw [h1.length, hm.2.length]
    have hdrop : a.steps.drop m.steps.length = s₂ := by rw [hsplit]; exact List.drop_left' hl
    refine ⟨ha1.symm, ?_⟩
    show StepsDenote M.eval r (m.steps ++ a.steps.drop m.steps.length) (π.path ++ e)
    rw [hdrop]
    exact StepsDenote.append hm.2 h2

/-! ## Proved prefixes -/

theorem provedPrefix_spec [DecidableEq T] : ∀ (p s : List (SStep T)), provedPrefix p s = true →
    p.all SStep.known = true ∧ ∃ rest, p ++ rest = s
  | [], s, _ => ⟨rfl, s, rfl⟩
  | _ :: _, [], h => by cases h
  | x :: p, y :: s, h => by
    have h' : (provedStep x y && provedPrefix p s) = true := h
    obtain ⟨hxy, hps⟩ := band_true h'
    have hxy' : (x.known && decide (x = y)) = true := hxy
    obtain ⟨hk, he⟩ := band_true hxy'
    have he' : x = y := of_decide_eq_true he
    subst he'
    obtain ⟨hall, rest, hrest⟩ := provedPrefix_spec p s hps
    refine ⟨?_, rest, by rw [List.cons_append, hrest]⟩
    show (x.known && p.all SStep.known) = true
    simp [hk, hall]

/-- A debt proved at or below a member covers only places at or below the
member's place. -/
theorem below_of_provedAtOrBelow [DecidableEq T] {ev : Run → T → Option Int} {r : Run}
    {m : Shape T} {πw : CPlace} (hm : m.Denotes ev r πw) {d : Debt T} {π : CPlace} {I : InvId}
    (hp : d.provedAtOrBelow m = true) (hc : d.Covers ev r π I) : π.Below πw := by
  obtain ⟨hm1, hm2⟩ := hm
  cases d with
  | inv a J rc =>
    obtain ⟨_, ⟨ha1, ha2⟩, _⟩ := hc
    have hp' : (a.root == m.root && provedPrefix m.steps a.steps) = true := hp
    obtain ⟨hroot, hpp⟩ := band_true hp'
    have hroot' : a.root = m.root := beq_iff_eq.1 hroot
    obtain ⟨hk, rest, hrest⟩ := provedPrefix_spec _ _ hpp
    rw [← hrest] at ha2
    obtain ⟨c₁, c₂, hc12, h1, _⟩ := StepsDenote.split ha2
    have he : c₁ = πw.path := StepsDenote.unique h1 hk hm2
    refine ⟨?_, c₂, ?_⟩
    · rw [← ha1, hroot', hm1]
    · rw [← he, hc12]
  | cover R =>
    obtain ⟨ρ, ⟨hR1, hR2⟩, hb⟩ := hc
    have hp' : (R.root == m.root && provedPrefix m.steps R.steps) = true := hp
    obtain ⟨hroot, hpp⟩ := band_true hp'
    have hroot' : R.root = m.root := beq_iff_eq.1 hroot
    obtain ⟨hk, rest, hrest⟩ := provedPrefix_spec _ _ hpp
    rw [← hrest] at hR2
    obtain ⟨c₁, c₂, hc12, h1, _⟩ := StepsDenote.split hR2
    have he : c₁ = πw.path := StepsDenote.unique h1 hk hm2
    have hρ : ρ.Below πw := ⟨by rw [← hR1, hroot', hm1], c₂, by rw [← he, hc12]⟩
    exact CPlace.below_trans hb hρ

end Judge

/-! ## The kill row -/

section KillRow

variable {X : Ctx T F} {M : Sem X} {f : F} {kb : Kills T} {r r' : Run}

/-- A debt the kill batch leaves as it is covers the same places after it. -/
theorem Ctx.covers_untouched (hfr : M.Frame f kb r r') {d : Debt T} {π : CPlace} {I : InvId}
    (hu : X.untouched f kb d = true) (hc : d.Covers M.eval r π I) : d.Covers M.eval r' π I := by
  cases d with
  | inv a J rc =>
    obtain ⟨hJ, ⟨ha1, ha2⟩, hrc⟩ := hc
    cases rc with
    | none =>
      have hu' : (((offsets a.steps).all fun t => !X.core.killed f kb t) && true) = true := hu
      exact ⟨hJ, ⟨ha1, StepsDenote.frame hfr ha2 (offsets_unkilled (band_true hu').1)⟩, trivial⟩
    | some rr =>
      obtain ⟨q, p⟩ := rr
      have hu' : (((offsets a.steps).all fun t => !X.core.killed f kb t) &&
          (!kb.rebound.contains q && (offsets p).all fun t => !X.core.killed f kb t)) = true := hu
      obtain ⟨hu1, hu2⟩ := band_true hu'
      obtain ⟨hq, hp⟩ := band_true hu2
      obtain ⟨ρ, rest, hρ, hrest, hπ⟩ := hrc
      have hq' : q ∉ kb.rebound := fun hmem => by
        have h1 : kb.rebound.contains q = true := List.contains_iff_mem.2 hmem
        rw [h1] at hq
        exact absurd hq (by decide)
      exact ⟨hJ, ⟨ha1, StepsDenote.frame hfr ha2 (offsets_unkilled hu1)⟩,
        ρ, rest, by rw [hfr.2 q hq']; exact hρ, StepsDenote.frame hfr hrest (offsets_unkilled hp), hπ⟩
  | cover R =>
    obtain ⟨ρ, ⟨hR1, hR2⟩, hb⟩ := hc
    have hu' : ((offsets R.steps).all fun t => !X.core.killed f kb t) = true := hu
    exact ⟨ρ, ⟨hR1, StepsDenote.frame hfr hR2 (offsets_unkilled hu')⟩, hb⟩

/-- A widened debt covers the places the debt covered, before and after the
kill batch. -/
theorem Ctx.covers_widen (hfr : M.Frame f kb r r') {d : Debt T} {π : CPlace} {I : InvId}
    (hc : d.Covers M.eval r π I) :
    (X.widen f kb d).Covers M.eval r π I ∧ (X.widen f kb d).Covers M.eval r' π I := by
  cases d with
  | inv a J rc =>
    obtain ⟨hJ, ⟨ha1, ha2⟩, _⟩ := hc
    obtain ⟨w1, w2⟩ := StepsDenote.widen hfr ha2 false
    exact ⟨⟨hJ, ⟨ha1, w1⟩, trivial⟩, ⟨hJ, ⟨ha1, w2⟩, trivial⟩⟩
  | cover R =>
    obtain ⟨ρ, ⟨hR1, hR2⟩, hb⟩ := hc
    obtain ⟨w1, w2⟩ := StepsDenote.widen hfr hR2 false
    exact ⟨⟨ρ, ⟨hR1, w1⟩, hb⟩, ⟨ρ, ⟨hR1, w2⟩, hb⟩⟩

theorem Ctx.covers_settle (hfr : M.Frame f kb r r') {d : Debt T} {π : CPlace} {I : InvId}
    (hc : d.Covers M.eval r π I) :
    (X.settle f kb d).Covers M.eval r π I ∧ (X.settle f kb d).Covers M.eval r' π I := by
  cases hu : X.untouched f kb d with
  | true =>
    have e : X.settle f kb d = d := by simp [Ctx.settle, hu]
    rw [e]; exact ⟨hc, Ctx.covers_untouched hfr hu hc⟩
  | false =>
    have e : X.settle f kb d = X.widen f kb d := by simp [Ctx.settle, hu]
    rw [e]; exact Ctx.covers_widen hfr hc

/-- The kill row keeps a debt that covers a place whose invariant fails, as a
debt that covers it before and after the batch. -/
theorem Ctx.killRow_cover [DecidableEq T] (hf : M.Facts f r) (hfr : M.Frame f kb r r')
    {D : List (Debt T)} {d : Debt T} {π : CPlace} {I : InvId} (hd : d ∈ D)
    (hc : d.Covers M.eval r π I) (hn : ¬ X.InvHolds r π I) :
    ∃ e ∈ X.killRow f kb D, e.Covers M.eval r π I ∧ e.Covers M.eval r' π I := by
  apply addDebts_new' (covers2_recClosed M.eval r r' π I)
  cases hu : X.untouched f kb d with
  | true =>
    exact ⟨d, List.mem_filterMap.2 ⟨d, hd, by simp [hu]⟩, hc, Ctx.covers_untouched hfr hu hc⟩
  | false =>
    have hr : X.restored f d = false := by
      cases h : X.restored f d with
      | false => rfl
      | true => exact absurd (Ctx.restored_sound M hf hc h) hn
    exact ⟨X.widen f kb d, List.mem_filterMap.2 ⟨d, hd, by simp [hu, hr]⟩, Ctx.covers_widen hfr hc⟩

end KillRow

/-! ## Reference members -/

theorem lookupRefs_cons (x : String × List (Shape T)) (rest : List (String × List (Shape T)))
    (q : String) : lookupRefs (x :: rest) q = if x.1 = q then x.2 else lookupRefs rest q := by
  obtain ⟨q', ms⟩ := x; rfl

theorem lookupRefs_killRefs (X : Ctx T F) (f : F) (kb : Kills T) (q : String) :
    ∀ (refs : List (String × List (Shape T))),
      lookupRefs (X.killRefs f kb refs) q = (lookupRefs refs q).map (X.widenShape f kb)
  | [] => rfl
  | (q', ms) :: rest => by
    have ih := lookupRefs_killRefs X f kb q rest
    show lookupRefs ((q', ms.map (X.widenShape f kb)) :: X.killRefs f kb rest) q =
      (lookupRefs ((q', ms) :: rest) q).map (X.widenShape f kb)
    by_cases he : q' = q
    · simp only [lookupRefs, if_pos he]
    · simp only [lookupRefs, if_neg he]; exact ih

theorem lookupRefs_filter (p : String × List (Shape T) → Bool) (q : String)
    (hp : ∀ x : String × List (Shape T), x.1 = q → p x = true) :
    ∀ (refs : List (String × List (Shape T))), lookupRefs (refs.filter p) q = lookupRefs refs q
  | [] => rfl
  | (q', ms) :: rest => by
    have ih := lookupRefs_filter p q hp rest
    by_cases he : q' = q
    · rw [List.filter_cons_of_pos (hp (q', ms) he)]
      simp only [lookupRefs, if_pos he]
    · cases hpx : p (q', ms) with
      | true =>
        rw [List.filter_cons_of_pos hpx]
        simp only [lookupRefs, if_neg he]
        exact ih
      | false =>
        have hn : ¬ p (q', ms) = true := by rw [hpx]; decide
        rw [List.filter_cons_of_neg hn]
        simp only [lookupRefs, if_neg he]
        exact ih

theorem lookupRefs_setRef_same (refs : List (String × List (Shape T))) (q : String)
    (ms : List (Shape T)) : lookupRefs (setRef refs q ms) q = ms := by
  simp [setRef, lookupRefs]

theorem lookupRefs_setRef_other (refs : List (String × List (Shape T))) {q q' : String}
    (ms : List (Shape T)) (hne : q' ≠ q) :
    lookupRefs (setRef refs q ms) q' = lookupRefs refs q' := by
  have e1 : lookupRefs (setRef refs q ms) q' = lookupRefs (refs.filter (·.1 != q)) q' := by
    simp [setRef, lookupRefs, Ne.symm hne]
  rw [e1]
  apply lookupRefs_filter
  intro x hx
  simp [hx, hne]

theorem mem_keys_of_mem_lookupRefs {m : Shape T} {q : String} :
    ∀ (refs : List (String × List (Shape T))), m ∈ lookupRefs refs q → q ∈ refs.map (·.1)
  | [], h => by simp [lookupRefs] at h
  | (q', ms) :: rest, h => by
    by_cases he : q' = q
    · subst he; simp
    · have h' : m ∈ lookupRefs rest q := by simpa [lookupRefs, he] using h
      exact List.mem_cons_of_mem _ (mem_keys_of_mem_lookupRefs rest h')

theorem lookupRefs_map_key (G : String → String × List (Shape T)) (hG : ∀ x, (G x).1 = x)
    (q : String) : ∀ (L : List String), q ∈ L → lookupRefs (L.map G) q = (G q).2
  | [], hq => by cases hq
  | x :: L, hq => by
    rw [List.map_cons, lookupRefs_cons, hG x]
    by_cases he : x = q
    · rw [if_pos he, he]
    · rw [if_neg he]
      rcases List.mem_cons.1 hq with h | h
      · exact absurd h.symm he
      · exact lookupRefs_map_key G hG q L h

theorem lookupRefs_unionRefs [DecidableEq T] (a b : List (String × List (Shape T))) (q : String)
    (hq : q ∈ (a.map (·.1) ++ b.map (·.1)).eraseDups) :
    lookupRefs (unionRefs a b) q =
      lookupRefs a q ++ (lookupRefs b q).filter fun m => !(lookupRefs a q).contains m :=
  lookupRefs_map_key
    (fun q => (q, lookupRefs a q ++ (lookupRefs b q).filter fun m => !(lookupRefs a q).contains m))
    (fun _ => rfl) q _ hq

theorem mem_unionRefs_left [DecidableEq T] {a b : List (String × List (Shape T))} {q : String}
    {m : Shape T} (h : m ∈ lookupRefs a q) : m ∈ lookupRefs (unionRefs a b) q := by
  have hq : q ∈ (a.map (·.1) ++ b.map (·.1)).eraseDups :=
    List.mem_eraseDups.2 (List.mem_append.2 (Or.inl (mem_keys_of_mem_lookupRefs a h)))
  rw [lookupRefs_unionRefs a b q hq]
  exact List.mem_append.2 (Or.inl h)

theorem mem_unionRefs_right [DecidableEq T] {a b : List (String × List (Shape T))} {q : String}
    {m : Shape T} (h : m ∈ lookupRefs b q) : m ∈ lookupRefs (unionRefs a b) q := by
  have hq : q ∈ (a.map (·.1) ++ b.map (·.1)).eraseDups :=
    List.mem_eraseDups.2 (List.mem_append.2 (Or.inr (mem_keys_of_mem_lookupRefs b h)))
  rw [lookupRefs_unionRefs a b q hq]
  by_cases ha : m ∈ lookupRefs a q
  · exact List.mem_append.2 (Or.inl ha)
  · refine List.mem_append.2 (Or.inr (List.mem_filter.2 ⟨h, ?_⟩))
    simp [ha]

/-- After a kill batch, each reference variable that keeps its path has a
member that denotes it. -/
theorem Ctx.refsOk_kill {X : Ctx T F} {M : Sem X} {f : F} {kb : Kills T} {r r' : Run}
    (hfr : M.Frame f kb r r') {s : St T} (hok : RefsOk M.eval r s) {q : String} {ρ : CPlace}
    (hq : r.refs q = some ρ) :
    ∃ m ∈ lookupRefs (X.killRefs f kb s.refs) q, m.Denotes M.eval r' ρ := by
  obtain ⟨m, hm, h1, h2⟩ := hok q ρ hq
  refine ⟨X.widenShape f kb m, ?_, h1, (StepsDenote.widen hfr h2 false).2⟩
  rw [lookupRefs_killRefs]
  exact List.mem_map.2 ⟨m, hm, rfl⟩

/-! ## Resolution -/

/-- A spelled place that denotes a runtime place has a resolved member that
denotes it. -/
theorem St.resolve_denote {ev : Run → T → Option Int} {r : Run} {s : St T}
    (hok : RefsOk ev r s) {p : Place T} {π : CPlace} (hp : p.Denotes ev r π) :
    ∃ m ∈ s.resolve p, m.Denotes ev r π := by
  obtain ⟨base, steps⟩ := p
  cases base with
  | var x =>
    obtain ⟨h1, h2⟩ := hp
    exact ⟨⟨x, steps⟩, by simp [St.resolve], h1.symm, h2⟩
  | deref q =>
    obtain ⟨ρ, rest, hq, hst, rfl⟩ := hp
    obtain ⟨m, hm, hm1, hm2⟩ := hok q ρ hq
    refine ⟨⟨m.root, m.steps ++ steps⟩, ?_, hm1, StepsDenote.append hm2 hst⟩
    simp only [St.resolve]
    exact List.mem_map.2 ⟨m, hm, rfl⟩

/-- The same for the members with their recorded reference: a member through a
reference r records r and the level of r's referent. -/
theorem St.resolveRec_denote {ev : Run → T → Option Int} {r : Run} {s : St T}
    (hok : RefsOk ev r s) {p : Place T} {π : CPlace} (hp : p.Denotes ev r π) :
    ∃ m th, (m, th) ∈ s.resolveRec p ∧ m.Denotes ev r π ∧
      ∀ q b, th = some (q, b) →
        ∃ ρ rest, r.refs q = some ρ ∧ b = ρ.path.length ∧ π = ⟨ρ.root, ρ.path ++ rest⟩ := by
  obtain ⟨base, steps⟩ := p
  cases base with
  | var x =>
    obtain ⟨h1, h2⟩ := hp
    refine ⟨⟨x, steps⟩, none, by simp [St.resolveRec], ⟨h1.symm, h2⟩, ?_⟩
    intro q b h; cases h
  | deref q =>
    obtain ⟨ρ, rest, hq, hst, rfl⟩ := hp
    obtain ⟨m, hm, hm1, hm2⟩ := hok q ρ hq
    refine ⟨⟨m.root, m.steps ++ steps⟩,
      if 2 ≤ (s.members q).length then some (q, m.steps.length) else none, ?_,
      ⟨hm1, StepsDenote.append hm2 hst⟩, ?_⟩
    · simp only [St.resolveRec]
      exact List.mem_map.2 ⟨m, hm, rfl⟩
    · intro q' b h
      split at h
      · cases h
        exact ⟨ρ, rest, hq, hm2.length, rfl⟩
      · cases h

/-- When a spelled place resolves to fewer than two members, each of them
denotes the place it denotes. -/
theorem St.resolve_one {ev : Run → T → Option Int} {r : Run} {s : St T} (hok : RefsOk ev r s)
    {p : Place T} {π : CPlace} (hp : p.Denotes ev r π) (hlen : (s.resolve p).length < 2) :
    ∀ m ∈ s.resolve p, m.Denotes ev r π := by
  intro m hm
  obtain ⟨m₀, hm₀, hd₀⟩ := St.resolve_denote hok hp
  rw [mem_eq_of_length_lt_two hlen hm hm₀]
  exact hd₀


/-! ## Store rows -/

theorem mem_filter_not {α : Type} {p : α → Bool} {L : List α} {x : α} (hx : x ∈ L)
    (h : p x = false) : x ∈ L.filter fun y => !p y :=
  List.mem_filter.2 ⟨hx, by simp [h]⟩

section Store

variable {ev : Run → T → Option Int} {r : Run}

theorem not_any_proved_of_one [DecidableEq T] {s : St T} (hok : RefsOk ev r s) {w : Place T}
    {πw : CPlace} (hw : w.Denotes ev r πw) (hlen : (s.resolve w).length < 2) {d : Debt T}
    {π : CPlace} {I : InvId} (hc : d.Covers ev r π I) (hn : ¬ π.Below πw) :
    ((s.resolve w).any fun m => d.provedAtOrBelow m) = false := by
  cases h : ((s.resolve w).any fun m => d.provedAtOrBelow m) with
  | false => rfl
  | true =>
    obtain ⟨m, hm, hp⟩ := List.any_eq_true.1 h
    exact absurd (below_of_provedAtOrBelow (St.resolve_one hok hw hlen m hm) hp hc) hn

/-- A debt proved at or below a member of a reference variable's referent
followed by a place's steps, that records the reference variable and a
projection beginning with those steps, covers only places at or below the
place. -/
theorem below_of_recordsAt [DecidableEq T] {q : String} {steps : List (SStep T)} {πw : CPlace}
    (hw : (Place.mk (.deref q) steps).Denotes ev r πw) {m : Shape T} {d : Debt T} {π : CPlace}
    {I : InvId} (hp : d.provedAtOrBelow ⟨m.root, m.steps ++ steps⟩ = true)
    (hr : d.recordsAt q m = true) (hc : d.Covers ev r π I) : π.Below πw := by
  cases d with
  | cover R => cases hr
  | inv a J rc =>
    cases rc with
    | none => cases hr
    | some rr =>
      obtain ⟨q', p⟩ := rr
      have hr' : (q' == q && decide (a = ⟨m.root, m.steps ++ p⟩)) = true := hr
      obtain ⟨hq, ha⟩ := band_true hr'
      have hq' : q' = q := beq_iff_eq.1 hq
      have ha' : a = ⟨m.root, m.steps ++ p⟩ := of_decide_eq_true ha
      subst ha'
      rw [hq'] at hc
      have hp' : ((Shape.mk m.root (m.steps ++ p)).root == m.root &&
          provedPrefix (m.steps ++ steps) (m.steps ++ p)) = true := hp
      obtain ⟨_, hpp⟩ := band_true hp'
      obtain ⟨hk, rest, hrest⟩ := provedPrefix_spec _ _ hpp
      rw [List.append_assoc] at hrest
      have hp2 : steps ++ rest = p := (List.append_right_inj _).1 hrest
      obtain ⟨_, _, hrc⟩ := hc
      obtain ⟨ρ, rest', hρ, hst, hπ⟩ := hrc
      obtain ⟨ρ', rest'', hρ', hst', hπw⟩ := hw
      rw [hρ] at hρ'
      have hρρ : ρ = ρ' := Option.some.inj hρ'
      subst hρρ
      rw [← hp2] at hst
      obtain ⟨c₁, c₂, hc12, h1, _⟩ := StepsDenote.split hst
      have hk2 : steps.all SStep.known = true := by
        rw [List.all_append] at hk
        exact (band_true hk).2
      have he : c₁ = rest'' := StepsDenote.unique h1 hk2 hst'
      subst he hπ hπw hc12
      exact ⟨rfl, c₂, List.append_assoc ..⟩

/-- The store rows keep each debt that covers a place outside the stored
place. -/
theorem storeRemove_keep [DecidableEq T] {s : St T} (hok : RefsOk ev r s) {w : Place T}
    {πw : CPlace} (hw : w.Denotes ev r πw) {D : List (Debt T)} {d : Debt T} (hd : d ∈ D)
    {π : CPlace} {I : InvId} (hc : d.Covers ev r π I) (hn : ¬ π.Below πw) :
    d ∈ storeRemove s w D := by
  obtain ⟨base, steps⟩ := w
  cases base with
  | var x =>
    have hlen : (s.resolve ⟨.var x, steps⟩).length < 2 := by simp [St.resolve]
    have e1 : storeRemove s ⟨.var x, steps⟩ D =
        D.filter fun d => !((s.resolve ⟨.var x, steps⟩).any fun m => d.provedAtOrBelow m) := by
      simp only [storeRemove]
    rw [e1]
    exact mem_filter_not hd (not_any_proved_of_one hok hw hlen hc hn)
  | deref q =>
    by_cases h2 : 2 ≤ (s.members q).length
    · have hf : ((s.members q).any fun m =>
          d.provedAtOrBelow ⟨m.root, m.steps ++ steps⟩ && d.recordsAt q m) = false := by
        cases h : ((s.members q).any fun m =>
            d.provedAtOrBelow ⟨m.root, m.steps ++ steps⟩ && d.recordsAt q m) with
        | false => rfl
        | true =>
          obtain ⟨m, _, hm⟩ := List.any_eq_true.1 h
          obtain ⟨hp, hr⟩ := band_true hm
          exact absurd (below_of_recordsAt hw hp hr hc) hn
      have e1 : storeRemove s ⟨.deref q, steps⟩ D = D.filter fun d => !((s.members q).any fun m =>
          d.provedAtOrBelow ⟨m.root, m.steps ++ steps⟩ && d.recordsAt q m) := by
        simp only [storeRemove, if_pos h2]
      rw [e1]
      exact mem_filter_not hd hf
    · have hlen : (s.resolve ⟨.deref q, steps⟩).length < 2 := by
        simp only [St.resolve, List.length_map]
        omega
      have e1 : storeRemove s ⟨.deref q, steps⟩ D =
          D.filter fun d => !((s.resolve ⟨.deref q, steps⟩).any fun m => d.provedAtOrBelow m) := by
        simp only [storeRemove, if_neg h2]
      rw [e1]
      exact mem_filter_not hd (not_any_proved_of_one hok hw hlen hc hn)

end Store

/-! ## Invariants read only their data -/

theorem Side.eval_congr {S : Side} {v v' : Val}
    (h : ∀ dp ∈ S.data, Val.get v' dp = Val.get v dp) : S.eval v' = S.eval v := by
  cases S with
  | datum p k =>
    have hp : Val.get v' p = Val.get v p := h p (List.mem_singleton.2 rfl)
    simp only [Side.eval, hp]
  | const k => rfl

/-- An invariant that holds at a place still holds after a step that keeps the
place existing only where it existed and keeps the value of each binder datum
the invariant reads. -/
theorem Ctx.invHolds_of_data {X : Ctx T F} {r r' : Run} {π : CPlace} {I : InvId}
    (hex : (r'.get π).isSome = true → (r.get π).isSome = true)
    (hdata : ∀ inv, X.decls.inv? I = some inv → ∀ dp ∈ inv.data,
      r'.get ⟨π.root, π.path ++ dp⟩ = r.get ⟨π.root, π.path ++ dp⟩)
    (h : X.InvHolds r π I) : X.InvHolds r' π I := by
  intro v' inv hv' hinv
  have hs : (r.get π).isSome = true := hex (by simp [hv'])
  cases hv : r.get π with
  | none => rw [hv] at hs; cases hs
  | some v =>
    have hd : ∀ dp ∈ inv.data, Val.get v' dp = Val.get v dp := by
      intro dp hdp
      rw [← Run.get_append_some dp hv', ← Run.get_append_some dp hv]
      exact hdata inv hinv dp hdp
    have hl : inv.lhs.eval v' = inv.lhs.eval v :=
      Side.eval_congr fun dp hdp => hd dp (List.mem_append.2 (Or.inl hdp))
    have hr : inv.rhs.eval v' = inv.rhs.eval v :=
      Side.eval_congr fun dp hdp => hd dp (List.mem_append.2 (Or.inr hdp))
    intro a b ha hb
    rw [hl] at ha
    rw [hr] at hb
    exact h v inv hv hinv a b ha hb

/-! ## Hand-offs -/

section Handoff

variable {X : Ctx T F} [DecidableEq T]

theorem Ctx.handoff_spec {f : F} {s s₁ : St T} {p : Place T} (h : X.handoff f s p = some s₁) :
    (s.debts.all fun d => (s.resolve p).all fun m =>
        !X.mayAtOrBelow f m d || X.restored f d || X.satisfied f m d) = true ∧
      s₁ = ⟨s.debts.filter (fun d =>
        !((s.resolve p).any (fun m => X.mayAtOrBelow f m d) && X.restored f d)), s.refs⟩ := by
  unfold Ctx.handoff at h
  split at h
  · rename_i hc
    exact ⟨hc, (Option.some.inj h).symm⟩
  · cases h

/-- A hand-off keeps the sealing state's agreement with the run: the debts it
removes are restored. -/
theorem Ctx.handoff_holds {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) {s s₁ : St T}
    {p : Place T} (hh : X.Holds M s r) (h : X.handoff f s p = some s₁) : X.Holds M s₁ r := by
  obtain ⟨_, rfl⟩ := Ctx.handoff_spec h
  refine ⟨?_, hh.2⟩
  intro π I hI hn
  obtain ⟨d, hd, hc⟩ := hh.1 π I hI hn
  refine ⟨d, ?_, hc⟩
  have hr : X.restored f d = false := by
    cases h' : X.restored f d with
    | false => rfl
    | true => exact absurd (Ctx.restored_sound M hf hc h') hn
  exact List.mem_filter.2 ⟨hd, by simp [hr]⟩

/-- A hand-off the rules accept hands off a value each of whose invariants
holds. -/
theorem Ctx.handoff_invHolds {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) {s s₁ : St T}
    {p : Place T} (hh : X.Holds M s r) (h : X.handoff f s p = some s₁) {πp : CPlace}
    (hp : p.Denotes M.eval r πp) (e : List CStep) (I : InvId)
    (hI : I ∈ X.invsOfPlace ⟨πp.root, πp.path ++ e⟩) :
    X.InvHolds r ⟨πp.root, πp.path ++ e⟩ I := by
  obtain ⟨hall, _⟩ := Ctx.handoff_spec h
  apply Classical.byContradiction
  intro hn
  obtain ⟨d, hd, hc⟩ := hh.1 _ I hI hn
  obtain ⟨m, hm, hmd⟩ := St.resolve_denote hh.2 hp
  have h1 : (!X.mayAtOrBelow f m d || X.restored f d || X.satisfied f m d) = true :=
    List.all_eq_true.1 (List.all_eq_true.1 hall d hd) m hm
  have hmay : X.mayAtOrBelow f m d = true := Ctx.mayAtOrBelow_of_covers M hf hmd hc
  rw [hmay] at h1
  rcases bor_true h1 with h2 | h2
  · rcases bor_true h2 with h3 | h3
    · cases h3
    · exact hn (Ctx.restored_sound M hf hc h3)
  · exact hn (Ctx.satisfied_sound M hf hmd hc h2)

/-- The value a hand-off the rules accept hands off is valid [TYPE-11.S4]. -/
theorem Ctx.handoff_valid {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) {s s₁ : St T}
    {p : Place T} (hh : X.Holds M s r) (h : X.handoff f s p = some s₁) {πp : CPlace}
    (hp : p.Denotes M.eval r πp) : X.Valid r πp := by
  intro t v ht hv e I u inv hI hu hinv
  exact Ctx.handoff_invHolds hf hh h hp e I (Ctx.mem_invsOfPlace_append_of ht hI) u inv
    ((Run.get_append_some e hv).trans hu) hinv

/-- A call's hand-offs keep the sealing state's agreement with the run. -/
theorem Ctx.handoffs_holds {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) :
    ∀ (ps : List (Place T)) {s s₁ : St T}, X.Holds M s r →
      ps.foldlM (fun t p => X.handoff f t p) s = some s₁ → X.Holds M s₁ r
  | [], s, s₁, hh, h => by
    have e : s = s₁ := Option.some.inj h
    rw [← e]
    exact hh
  | p :: ps, s, s₁, hh, h => by
    have h' : ((X.handoff f s p).bind fun t => ps.foldlM (fun t p => X.handoff f t p) t) =
        some s₁ := h
    cases ht : X.handoff f s p with
    | none => rw [ht] at h'; cases h'
    | some t =>
      rw [ht] at h'
      exact Ctx.handoffs_holds hf ps (Ctx.handoff_holds hf hh ht) h'

/-- An instance the rules establish holds [TYPE-11.S7]. -/
theorem Ctx.instance_true {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) {s : St T}
    (hh : X.Holds M s r) {p : Place T} {I : InvId} (hi : X.instance? f s p I = true)
    {π : CPlace} (hp : p.Denotes M.eval r π) (hI : I ∈ X.invsOfPlace π) : X.InvHolds r π I := by
  apply Classical.byContradiction
  intro hn
  obtain ⟨d, hd, hc⟩ := hh.1 π I hI hn
  obtain ⟨m, hm, hmd⟩ := St.resolve_denote hh.2 hp
  have h1 : (!X.mayDenote f m I d) = true :=
    List.all_eq_true.1 (List.all_eq_true.1 hi m hm) d hd
  have h2 : X.mayDenote f m I d = true := by
    cases d with
    | cover R =>
      obtain ⟨ρ, hR, hb1, e, he⟩ := hc
      exact Ctx.mayOverlap_of M hf hR hmd hb1.symm (Or.inl ⟨e, he⟩)
    | inv a J rc =>
      obtain ⟨hJ, ha, _⟩ := hc
      have hlen : a.steps.length = m.steps.length := by rw [ha.2.length, hmd.2.length]
      have hov : X.mayOverlap f a m = true :=
        Ctx.mayOverlap_of M hf ha hmd rfl (Or.inl ⟨[], List.append_nil _⟩)
      have e1 : X.mayDenote f m I (.inv a J rc) =
          (J == I && a.steps.length == m.steps.length && X.mayOverlap f a m) := rfl
      rw [e1, hJ, hlen, hov]
      simp
  rw [h2] at h1
  cases h1

end Handoff

/-! ## Merges and loop headers -/

section Merge

variable {X : Ctx T F} [DecidableEq T]

theorem Ctx.restoreAll_keep {M : Sem X} {f : F} {r : Run} (hf : M.Facts f r) {D : List (Debt T)}
    {d : Debt T} (hd : d ∈ D) {π : CPlace} {I : InvId} (hc : d.Covers M.eval r π I)
    (hn : ¬ X.InvHolds r π I) : d ∈ X.restoreAll f D := by
  have hr : X.restored f d = false := by
    cases h' : X.restored f d with
    | false => rfl
    | true => exact absurd (Ctx.restored_sound M hf hc h') hn
  exact mem_filter_not hd hr

theorem Ctx.fold_absorb_keep {P : Debt T → Prop} (hP : RecClosed P) :
    ∀ (edges : List (F × St T)) {h : St T}, (∃ d ∈ h.debts, P d) →
      ∃ d ∈ (edges.foldl X.absorb h).debts, P d
  | [], _, hd => hd
  | e :: edges, h, hd =>
    Ctx.fold_absorb_keep hP edges (h := X.absorb h e) (addDebts_keep hP _ hd)

theorem Ctx.fold_absorb_new {P : Debt T → Prop} (hP : RecClosed P) :
    ∀ (edges : List (F × St T)) {h : St T} {e : F × St T} {d : Debt T}, e ∈ edges →
      d ∈ X.restoreAll e.1 e.2.debts → P d → ∃ d' ∈ (edges.foldl X.absorb h).debts, P d'
  | [], _, _, _, he, _, _ => by cases he
  | e' :: edges, h, e, d, he, hd, hp => by
    rcases List.mem_cons.1 he with he1 | he'
    · rw [he1] at hd
      exact Ctx.fold_absorb_keep hP edges (h := X.absorb h e') (addDebts_new hP _ hd hp)
    · exact Ctx.fold_absorb_new hP edges (h := X.absorb h e') he' hd hp

theorem Ctx.fold_absorb_refs_keep {q : String} {m : Shape T} :
    ∀ (edges : List (F × St T)) {h : St T}, m ∈ lookupRefs h.refs q →
      m ∈ lookupRefs (edges.foldl X.absorb h).refs q
  | [], _, hm => hm
  | e :: edges, h, hm =>
    Ctx.fold_absorb_refs_keep edges (h := X.absorb h e) (mem_unionRefs_left hm)

theorem Ctx.fold_absorb_refs_new {q : String} {m : Shape T} :
    ∀ (edges : List (F × St T)) {h : St T} {e : F × St T}, e ∈ edges →
      m ∈ lookupRefs e.2.refs q → m ∈ lookupRefs (edges.foldl X.absorb h).refs q
  | [], _, _, he, _ => by cases he
  | e' :: edges, h, e, he, hm => by
    rcases List.mem_cons.1 he with he1 | he'
    · rw [he1] at hm
      exact Ctx.fold_absorb_refs_keep edges (h := X.absorb h e') (mem_unionRefs_right hm)
    · exact Ctx.fold_absorb_refs_new edges (h := X.absorb h e') he' hm

/-- The state at a merge agrees with the run arriving on any of its edges
[TYPE-11.S6]. -/
theorem Ctx.join_holds {M : Sem X} {edges : List (F × St T)} {f : F} {s : St T} {r : Run}
    (he : (f, s) ∈ edges) (hf : M.Facts f r) (hh : X.Holds M s r) :
    X.Holds M (X.join edges) r := by
  refine ⟨?_, ?_⟩
  · intro π I hI hn
    obtain ⟨d, hd, hc⟩ := hh.1 π I hI hn
    exact Ctx.fold_absorb_new (covers_recClosed M.eval r π I) edges he
      (Ctx.restoreAll_keep hf hd hc hn) hc
  · intro q ρ hq
    obtain ⟨m, hm, hmd⟩ := hh.2 q ρ hq
    exact ⟨m, Ctx.fold_absorb_refs_new edges he hm, hmd⟩

/-- The state at a loop header agrees with the run entering the loop
[TYPE-11.S6]. -/
theorem Ctx.header_entry_holds {M : Sem X} {entry : St T} {backs : List (F × St T)} {r : Run}
    (hh : X.Holds M entry r) : X.Holds M (X.header entry backs) r := by
  refine ⟨?_, ?_⟩
  · intro π I hI hn
    exact Ctx.fold_absorb_keep (covers_recClosed M.eval r π I) backs (hh.1 π I hI hn)
  · intro q ρ hq
    obtain ⟨m, hm, hmd⟩ := hh.2 q ρ hq
    exact ⟨m, Ctx.fold_absorb_refs_keep backs hm, hmd⟩

/-- The state at a loop header agrees with the run arriving on a backedge or a
`continue` [TYPE-11.S6]. -/
theorem Ctx.header_back_holds {M : Sem X} {entry : St T} {backs : List (F × St T)} {f : F}
    {s : St T} {r : Run} (he : (f, s) ∈ backs) (hf : M.Facts f r) (hh : X.Holds M s r) :
    X.Holds M (X.header entry backs) r := by
  refine ⟨?_, ?_⟩
  · intro π I hI hn
    obtain ⟨d, hd, hc⟩ := hh.1 π I hI hn
    exact Ctx.fold_absorb_new (covers_recClosed M.eval r π I) backs he
      (Ctx.restoreAll_keep hf hd hc hn) hc
  · intro q ρ hq
    obtain ⟨m, hm, hmd⟩ := hh.2 q ρ hq
    exact ⟨m, Ctx.fold_absorb_refs_new backs he hm, hmd⟩

end Merge

end Whitefoot.Seal
