import WhitefootProof.Affine

/-!
# Soundness of the affine procedures

What an affine form and an inequality mean under a valuation of the atoms, when
a view is satisfied, and that `DIRECT`, the two integer tightenings, the
residual check and `AUTO` answer "holds" only for an inequality that holds
under every valuation satisfying the view [ENT-6].

A valuation `G` gives each compiler-owned value atom an integer. A view is
satisfied when it is not contradictory, every atom lies in its interval and
every published premise and every L0 image holds. That the images a state's
view carries hold, given the state's L0 relations, is the L0 side of the proof
and is not shown here.
-/

namespace Whitefoot

namespace Aff

/-- `Σ a_i * G(x_i)`. -/
def evalCoeffs (G : Nat → Int) : Coeffs → Int
  | [] => 0
  | p :: rest => p.2 * G p.1 + evalCoeffs G rest

/-- The value of `c + Σ a_i * x_i` under `G`. -/
def Form.eval (G : Nat → Int) (f : Form) : Int := f.c + evalCoeffs G f.xs

/-- `Σ a_i * G(x_i) <= c`. -/
def Ineq.Holds (G : Nat → Int) (i : Ineq) : Prop := evalCoeffs G i.xs ≤ i.c

/-- Not contradictory, every atom in its interval, every published premise
and every L0 image holding. -/
def View.Sat (v : View) (G : Nat → Int) : Prop :=
  v.contra = false ∧
  (∀ x, (v.interval x).1 ≤ G x ∧ G x ≤ (v.interval x).2) ∧
  (∀ p ∈ v.premises, p.Holds G) ∧
  (∀ r ∈ v.images, r.Holds G)

/-! ## Coefficient lists -/

theorem eval_addTerm (G : Nat → Int) (x : Nat) (a : Int) (xs : Coeffs) :
    evalCoeffs G (addTerm xs x a) = evalCoeffs G xs + a * G x := by
  induction xs with
  | nil =>
    by_cases ha : a = 0
    · subst ha; simp [addTerm, evalCoeffs]
    · simp [addTerm, evalCoeffs, ha]
  | cons p rest ih =>
    obtain ⟨y, b⟩ := p
    by_cases hxy : x < y
    · by_cases ha : a = 0
      · subst ha; simp [addTerm, evalCoeffs, hxy]
      · simp [addTerm, evalCoeffs, hxy, ha] <;> omega
    · by_cases he : x = y
      · subst y
        have hsum : (a + b) * G x = a * G x + b * G x := Int.add_mul a b (G x)
        by_cases hab : a + b = 0
        · have h0 : (a + b) * G x = 0 := by rw [hab, Int.zero_mul]
          simp [addTerm, evalCoeffs, hab] <;> omega
        · simp [addTerm, evalCoeffs, hab] <;> omega
      · simp [addTerm, evalCoeffs, hxy, he, ih] <;> omega

theorem addCoeffs_nil (xs : Coeffs) : addCoeffs xs [] = xs := rfl

theorem addCoeffs_cons (xs : Coeffs) (x : Nat) (a : Int) (rest : Coeffs) :
    addCoeffs xs ((x, a) :: rest) = addCoeffs (addTerm xs x a) rest := rfl

theorem eval_addCoeffs (G : Nat → Int) (xs ys : Coeffs) :
    evalCoeffs G (addCoeffs xs ys) = evalCoeffs G xs + evalCoeffs G ys := by
  induction ys generalizing xs with
  | nil => simp [addCoeffs_nil, evalCoeffs]
  | cons p rest ih =>
    obtain ⟨x, a⟩ := p
    rw [addCoeffs_cons, ih, eval_addTerm]
    simp only [evalCoeffs]
    omega

theorem eval_scaleCoeffs (G : Nat → Int) (k : Int) (xs : Coeffs) :
    evalCoeffs G (scaleCoeffs k xs) = k * evalCoeffs G xs := by
  by_cases hk : k = 0
  · subst hk; simp [scaleCoeffs, evalCoeffs]
  · have hk' : ¬(k == 0) = true := by simpa using hk
    rw [scaleCoeffs, ite_eq_right hk']
    induction xs with
    | nil => simp [evalCoeffs]
    | cons p rest ih =>
      obtain ⟨x, a⟩ := p
      simp only [List.map_cons, evalCoeffs]
      rw [ih]
      rw [Int.mul_add, Int.mul_assoc]

/-- Mapping each coefficient to its quotient by `k` scales the sum by `1/k`
when `k` divides every coefficient. -/
theorem eval_map_mul (G : Nat → Int) (k : Int) (g : Nat × Int → Nat × Int) (xs : Coeffs)
    (hg : ∀ p ∈ xs, (g p).1 = p.1 ∧ (g p).2 * k = p.2) :
    evalCoeffs G (xs.map g) * k = evalCoeffs G xs := by
  induction xs with
  | nil => simp [evalCoeffs]
  | cons p rest ih =>
    obtain ⟨h1, h2⟩ := hg p (by simp)
    have ih' := ih (fun q hq => hg q (by simp [hq]))
    simp only [List.map_cons, evalCoeffs]
    rw [h1, Int.add_mul, ih']
    rw [Int.mul_right_comm, h2]

/-! ## Forms and inequalities -/

theorem Form.eval_const (G : Nat → Int) (c : Int) : (Form.const c).eval G = c := by
  simp [Form.eval, Form.const, evalCoeffs]

theorem Form.eval_atom (G : Nat → Int) (x : Nat) : (Form.atom x).eval G = G x := by
  simp [Form.eval, Form.atom, evalCoeffs]

theorem Form.eval_add (G : Nat → Int) (f g : Form) :
    (f.add g).eval G = f.eval G + g.eval G := by
  simp only [Form.eval, Form.add, eval_addCoeffs]
  omega

theorem Form.eval_scale (G : Nat → Int) (k : Int) (f : Form) :
    (f.scale k).eval G = k * f.eval G := by
  simp only [Form.eval, Form.scale, eval_scaleCoeffs, Int.mul_add]

theorem Form.eval_sub (G : Nat → Int) (f g : Form) :
    (f.sub g).eval G = f.eval G - g.eval G := by
  rw [Form.sub, Form.eval_add, Form.eval_scale]
  omega

/-- `Ineq.le lhs rhs` holds exactly when `lhs <= rhs` does. -/
theorem Ineq.holds_le (G : Nat → Int) (lhs rhs : Form) :
    (Ineq.le lhs rhs).Holds G ↔ lhs.eval G ≤ rhs.eval G := by
  have h := Form.eval_sub G lhs rhs
  simp only [Form.eval] at h ⊢
  show evalCoeffs G (lhs.sub rhs).xs ≤ -(lhs.sub rhs).c ↔ _
  constructor <;> intro h' <;> omega

theorem Ineq.add_holds {G : Nat → Int} {p q : Ineq} (hp : p.Holds G) (hq : q.Holds G) :
    (p.add q).Holds G := by
  unfold Ineq.Holds at hp hq
  show evalCoeffs G (addCoeffs p.xs q.xs) ≤ p.c + q.c
  rw [eval_addCoeffs]
  omega

/-- `T - S <= c_T - c_S` and `S` give `T`. -/
theorem Ineq.holds_of_sub {G : Nat → Int} {t s : Ineq} (h : (t.sub s).Holds G)
    (hs : s.Holds G) : t.Holds G := by
  unfold Ineq.Holds at hs ⊢
  change evalCoeffs G (addCoeffs t.xs (scaleCoeffs (-1) s.xs)) ≤ t.c - s.c at h
  rw [eval_addCoeffs, eval_scaleCoeffs] at h
  omega

theorem fdiv_ge_of_mul_le {y c k : Int} (hk : 0 < k) (h : y * k ≤ c) : y ≤ c.fdiv k := by
  have hk0 : 0 ≤ k := Int.le_of_lt hk
  have e : c.fdiv k = c / k := by rw [Int.fdiv_eq_ediv]; simp [hk0]
  rw [e]
  exact (Int.le_ediv_iff_mul_le hk).2 h

theorem holds_of_map {G : Nat → Int} {k : Int} (hk : 0 < k) (g : Nat × Int → Nat × Int)
    (xs : Coeffs) (c : Int) (hg : ∀ p ∈ xs, (g p).1 = p.1 ∧ (g p).2 * k = p.2)
    (h : evalCoeffs G xs ≤ c) : evalCoeffs G (xs.map g) ≤ c.fdiv k := by
  have hm := eval_map_mul G k g xs hg
  exact fdiv_ge_of_mul_le hk (by omega)

private theorem bool_or_not {b c : Bool} (h : ¬(b || !c) = true) : b = false ∧ c = true := by
  cases b <;> cases c <;> simp_all

/-- Dividing a holding inequality by a factor of every coefficient, its bound
rounded down, gives one that holds. -/
theorem Ineq.div_holds {G : Nat → Int} {s s' : Ineq} {k : Int} (h : s.div k = some s')
    (hs : s.Holds G) : s'.Holds G := by
  unfold Ineq.div at h
  split at h
  · simp at h
  · rename_i hcond
    obtain ⟨hk1, hall⟩ := bool_or_not hcond
    have hk : 1 < k := by
      have := of_decide_eq_false hk1
      omega
    cases h
    unfold Ineq.Holds at hs ⊢
    refine holds_of_map (by omega) _ s.xs s.c ?_ hs
    intro p hp
    obtain ⟨x, a⟩ := p
    have ha : a % k = 0 := by simpa using List.all_eq_true.1 hall (x, a) hp
    exact ⟨rfl, Int.ediv_mul_cancel (Int.dvd_of_emod_eq_zero ha)⟩

/-! ## Interval substitution -/

theorem foldl_inv {α β : Type} (f : β → α → β) (P : α → Prop) (Q : β → Prop)
    (hf : ∀ acc r, P r → Q acc → Q (f acc r)) :
    ∀ (l : List α) (acc : β), (∀ r ∈ l, P r) → Q acc → Q (l.foldl f acc)
  | [], _, _, hq => hq
  | r :: rest, acc, hl, hq =>
    foldl_inv f P Q hf rest (f acc r) (fun r' hr' => hl r' (by simp [hr']))
      (hf acc r (hl r (by simp)) hq)

theorem View.le_hi {v : View} {G : Nat → Int} (x : Nat) (hx : G x ≤ (v.interval x).2)
    (himg : ∀ r ∈ v.images, r.Holds G) : G x ≤ v.hi x := by
  unfold View.hi
  refine foldl_inv _ (fun r => r.Holds G) (fun acc => G x ≤ acc) ?_ v.images _ himg hx
  intro acc r hr hacc
  try dsimp only at hr hacc ⊢
  split
  · rename_i hxs
    have hxs' : r.xs = [(x, 1)] := by simpa using hxs
    have h1 : evalCoeffs G r.xs ≤ r.c := hr
    rw [hxs'] at h1
    simp [evalCoeffs] at h1
    omega
  · exact hacc

theorem View.lo_le {v : View} {G : Nat → Int} (x : Nat) (hx : (v.interval x).1 ≤ G x)
    (himg : ∀ r ∈ v.images, r.Holds G) : v.lo x ≤ G x := by
  unfold View.lo
  refine foldl_inv _ (fun r => r.Holds G) (fun acc => acc ≤ G x) ?_ v.images _ himg hx
  intro acc r hr hacc
  try dsimp only at hr hacc ⊢
  split
  · rename_i hxs
    have hxs' : r.xs = [(x, -1)] := by simpa using hxs
    have h1 : evalCoeffs G r.xs ≤ r.c := hr
    rw [hxs'] at h1
    simp [evalCoeffs] at h1
    omega
  · exact hacc

/-- A left fold whose step adds at least `k * G(x)` for each term `(x, k)`
ends at least the sum's value above its start. -/
theorem foldl_sum_le (G : Nat → Int) (F : Int → Nat × Int → Int)
    (hF : ∀ acc acc' p, acc ≤ acc' → acc + p.2 * G p.1 ≤ F acc' p) :
    ∀ (ys : Coeffs) (acc acc' : Int), acc ≤ acc' → acc + evalCoeffs G ys ≤ ys.foldl F acc'
  | [], acc, acc', h => by simpa [evalCoeffs] using h
  | p :: rest, acc, acc', h => by
    have := foldl_sum_le G F hF rest (acc + p.2 * G p.1) (F acc' p) (hF acc acc' p h)
    simp only [List.foldl_cons, evalCoeffs]
    omega

/-- Interval substitution bounds the sum from above: each term `k * x` is at
most `k` times the endpoint its sign selects. -/
theorem eval_le_substMax {v : View} {G : Nat → Int} (hlo : ∀ x, v.lo x ≤ G x)
    (hhi : ∀ x, G x ≤ v.hi x) (xs : Coeffs) : evalCoeffs G xs ≤ v.substMax xs := by
  unfold View.substMax
  rw [← Int.zero_add (evalCoeffs G xs)]
  refine foldl_sum_le G _ ?_ xs 0 0 (Int.le_refl 0)
  intro acc acc' p h
  obtain ⟨x, k⟩ := p
  try dsimp only
  split
  · rename_i hk
    have hd : 0 ≤ k * (v.hi x - G x) :=
      Int.mul_nonneg (Int.le_of_lt hk) (by have := hhi x; omega)
    rw [Int.mul_sub] at hd
    omega
  · rename_i hk
    have hd : 0 ≤ (-k) * (G x - v.lo x) :=
      Int.mul_nonneg (by omega) (by have := hlo x; omega)
    rw [Int.neg_mul, Int.mul_sub] at hd
    omega

/-! ## The procedures -/

/-- `DIRECT(A)` answers "holds" only for an `A` that holds [ENT-6]. -/
theorem direct_sound {v : View} {a : Ineq} {G : Nat → Int} (hv : v.Sat G)
    (h : direct v a = true) : a.Holds G := by
  obtain ⟨hc, hint, -, himg⟩ := hv
  unfold direct at h
  simp only [Bool.or_eq_true] at h
  rcases h with (h | h) | h
  · simp [hc] at h
  · obtain ⟨r, hr, hx⟩ := List.any_eq_true.1 h
    simp only [Bool.and_eq_true, decide_eq_true_eq] at hx
    obtain ⟨hxs, hle⟩ := hx
    have hxs' : r.xs = a.xs := by simpa using hxs
    have := himg r hr
    unfold Ineq.Holds at this ⊢
    rw [← hxs']
    omega
  · have hle := of_decide_eq_true h
    have hsub := eval_le_substMax (v := v) (G := G)
      (fun x => View.lo_le x (hint x).1 himg) (fun x => View.le_hi x (hint x).2 himg) a.xs
    unfold Ineq.Holds
    omega

/-- Each tightening of a holding `S` holds. -/
theorem tightenings_sound {G : Nat → Int} {t s s' : Ineq} (hs : s.Holds G)
    (h : s' ∈ tightenings t s) : s'.Holds G := by
  unfold tightenings at h
  split at h
  · simp at h
  · obtain ⟨k, -, hk⟩ := List.mem_filterMap.1 h
    cases k with
    | none => simp at hk
    | some k => exact Ineq.div_holds hk hs

/-- The residual check of `T` against a holding `S` answers "holds" only for
a `T` that holds. -/
theorem tryResidual_sound {v : View} {t s : Ineq} {G : Nat → Int} (hv : v.Sat G)
    (hs : s.Holds G) (h : tryResidual v t s = true) : t.Holds G := by
  unfold tryResidual at h
  obtain ⟨s', hmem, hx⟩ := List.any_eq_true.1 h
  simp only [Bool.and_eq_true] at hx
  have hs' : s'.Holds G := by
    rcases List.mem_cons.1 hmem with rfl | hm
    · exact hs
    · exact tightenings_sound hs hm
  exact Ineq.holds_of_sub (direct_sound hv hx.2) hs'

theorem fst_mem_zipIdx {α : Type} :
    ∀ (l : List α) (n : Nat) (p : α × Nat), p ∈ l.zipIdx n → p.1 ∈ l
  | [], _, _, h => by simp at h
  | a :: l, n, p, h => by
    rw [List.zipIdx_cons, List.mem_cons] at h
    rcases h with rfl | h
    · simp
    · simp [fst_mem_zipIdx l (n + 1) p h]

theorem mem_of_mem_drop' {α : Type} :
    ∀ (n : Nat) (l : List α) (a : α), a ∈ l.drop n → a ∈ l
  | 0, _, _, h => by simpa using h
  | _ + 1, [], _, h => by simp at h
  | n + 1, b :: l, a, h => by
    rw [List.drop_succ_cons] at h
    simp [mem_of_mem_drop' n l a h]

/-- `AUTO(T)` answers "holds" only for a `T` that holds [ENT-6]. -/
theorem auto_sound {v : View} {t : Ineq} {G : Nat → Int} (hv : v.Sat G)
    (h : auto v t = true) : t.Holds G := by
  have hprem := hv.2.2.1
  have himg := hv.2.2.2
  unfold auto at h
  simp only [Bool.or_eq_true] at h
  rcases h with ((h | h) | h) | h
  · exact direct_sound hv h
  · obtain ⟨p, hp, h⟩ := List.any_eq_true.1 h
    exact tryResidual_sound hv (hprem p hp) h
  · obtain ⟨⟨p, i⟩, hpi, h⟩ := List.any_eq_true.1 h
    obtain ⟨q, hq, h⟩ := List.any_eq_true.1 h
    have hp : p ∈ v.premises := fst_mem_zipIdx v.premises 0 (p, i) hpi
    exact tryResidual_sound hv
      (Ineq.add_holds (hprem p hp) (hprem q (mem_of_mem_drop' i v.premises q hq))) h
  · obtain ⟨r, hr, h⟩ := List.any_eq_true.1 h
    exact tryResidual_sound hv (himg r hr) h

end Aff

end Whitefoot
