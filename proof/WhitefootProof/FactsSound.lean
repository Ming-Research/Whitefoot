import WhitefootProof.Facts

/-!
# Soundness of the L0 relation state

The facts of `Facts.lean` mean something once the terms take values: Z takes 0,
a sum the sum of its sides, and a term of a fragment type a member of that
type. A state is satisfied by a valuation when it is not contradictory and
every bound and disequality it stores holds. This module proves that each
operation of the state keeps that meaning:

- `close` and `add`: when their input facts hold and every term they name lies
  in its type, the result is satisfied. Each step of the matrix closure lowers
  an entry only to a value the valuation already bounds, so every entry stays
  a true bound, and no diagonal entry can be negative.
- `tightest`, `bound`, `diseq`, `eq` and `interval`: what a satisfied state
  answers holds.
- `kill`: the facts left after a kill hold under any valuation that agrees on
  the terms whose support the kill spares.
- `join`: when the input actually taken is satisfied, so is the join, under a
  valuation that agrees with it on the terms that input and the live list
  name.
-/

namespace Whitefoot

namespace Fact

/-! ## Valuations -/

/-- A valuation of the terms: Z takes 0 and a sum the sum of its sides. -/
structure TermVal where
  val : Term → Int
  val_z : val .z = 0
  val_sum : ∀ a b, val (.sum a b) = val a + val b

/-- A term with a fragment type takes a member of that type. -/
def TermVal.Typed (ν : TermVal) (t : Term) : Prop :=
  ∀ ty, t.ty? = some ty → ty.minVal ≤ ν.val t ∧ ν.val t ≤ ty.maxVal

/-- The state is not contradictory, and every bound and disequality it stores
holds. -/
def L0.Sat (s : L0) (ν : TermVal) : Prop :=
  s.contra = false ∧
  (∀ b ∈ s.bounds, ν.val b.a - ν.val b.b ≤ b.c) ∧
  (∀ d ∈ s.diseqs, ν.val d.a - ν.val d.b ≠ d.c)

/-- The terms with a fragment type that a term is built from. -/
def Term.atoms : Term → List Term
  | .z => []
  | .sum a b => a.atoms ++ b.atoms
  | .place id t => [.place id t]
  | .commit id t => [.commit id t]
  | .datum id t => [.datum id t]

/-! ## Lists -/

theorem foldl_inv {α β : Type} (P : α → Prop) (f : α → β → α) :
    ∀ (l : List β) (init : α), P init → (∀ a b, b ∈ l → P a → P (f a b)) →
      P (l.foldl f init)
  | [], _, h, _ => h
  | b :: l, init, h, hf =>
    foldl_inv P f l (f init b) (hf init b List.mem_cons_self h)
      (fun a b' hb ha => hf a b' (List.mem_cons_of_mem _ hb) ha)

theorem findIdx?_beq : ∀ (l : List Term) (t : Term), t ∈ l →
    ∃ i, l.findIdx? (· == t) = some i ∧ l[i]! = t
  | [], _, h => by simp at h
  | x :: xs, t, h => by
    by_cases hx : x = t
    · subst hx
      exact ⟨0, by simp [List.findIdx?_cons], List.getElem!_cons_zero⟩
    · have ht : t ∈ xs := by
        rcases List.mem_cons.mp h with h | h
        · exact absurd h.symm hx
        · exact h
      obtain ⟨i, hi, hget⟩ := findIdx?_beq xs t ht
      refine ⟨i + 1, ?_, ?_⟩
      · simp [List.findIdx?_cons, hx, hi]
      · rw [List.getElem!_cons_succ]
        exact hget

/-- The index `close` gives a term of its universe names that term. -/
theorem getElem!_findIdx? (U : Array Term) (t : Term) (h : t ∈ U.toList) :
    U[(U.toList.findIdx? (· == t)).getD 0]! = t := by
  obtain ⟨i, hi, hget⟩ := findIdx?_beq U.toList t h
  rw [hi, Option.getD_some, ← Array.getElem!_toList]
  exact hget

theorem mem_termUniverse {t : Term} {extra : List Term} {bs : List Bound}
    {ds : List Diseq} :
    t ∈ (termUniverse extra bs ds).toList ↔
      t = .z ∨ t ∈ extra ∨ (∃ b ∈ bs, t = b.a ∨ t = b.b) ∨
        ∃ d ∈ ds, t = d.a ∨ t = d.b := by
  simp only [termUniverse, List.toList_toArray, List.mem_eraseDups, List.mem_append,
    List.mem_cons, List.mem_flatMap, List.not_mem_nil, or_false, or_assoc, iff_self]

theorem L0.mem_terms {t : Term} {s : L0} :
    t ∈ s.terms ↔
      (∃ b ∈ s.bounds, t = b.a ∨ t = b.b) ∨ ∃ d ∈ s.diseqs, t = d.a ∨ t = d.b := by
  simp only [L0.terms, List.mem_eraseDups, List.mem_append, List.mem_flatMap,
    List.mem_cons, List.not_mem_nil, or_false, iff_self]

theorem not_and_not {x y : Bool} (h : (!x && !y) = true) : x = false ∧ y = false := by
  revert h
  cases x <;> cases y <;> decide

theorem zero_mem_type (ty : IntTy) : ty.minVal ≤ 0 ∧ 0 ≤ ty.maxVal := by
  cases ty <;> decide

/-! ## The closure matrix -/

/-- Every entry of the matrix bounds the difference of the terms its indices
name under `T`. -/
def Matrix.Sound (ν : TermVal) (T : Nat → Term) (m : Matrix) : Prop :=
  ∀ i j v, m.get i j = some v → ν.val (T i) - ν.val (T j) ≤ v

/-- Writing one entry leaves every other entry as it was. -/
theorem Matrix.get_setEntry (m : Matrix) (i j : Nat) (x : Option Int) (i' j' : Nat) :
    Matrix.get (m.set! i ((m[i]!).set! j x)) i' j' = m.get i' j' ∨
      (i' = i ∧ j' = j ∧ Matrix.get (m.set! i ((m[i]!).set! j x)) i' j' = x) := by
  unfold Matrix.get
  by_cases hi : i = i'
  · subst hi
    by_cases hm : i < m.size
    · rw [Array.getElem!_set!_self m i _ hm]
      by_cases hj : j = j'
      · subst hj
        by_cases hr : j < (m[i]!).size
        · exact Or.inr ⟨rfl, rfl, Array.getElem!_set!_self _ j x hr⟩
        · left
          have hrow : (m[i]!).set! j x = m[i]! :=
            Array.setIfInBounds_eq_of_size_le (by omega)
          rw [hrow]
      · left
        exact Array.getElem!_set!_ne _ j j' x hj
    · left
      have hrows : m.set! i ((m[i]!).set! j x) = m :=
        Array.setIfInBounds_eq_of_size_le (by omega)
      rw [hrows]
  · left
    rw [Array.getElem!_set!_ne m i i' _ hi]

theorem Matrix.get_replicate (n i j : Nat) :
    Matrix.get (Array.replicate n (Array.replicate n (none : Option Int))) i j = none := by
  unfold Matrix.get
  by_cases hi : i < n
  · rw [getElem!_pos _ i (by simpa using hi), Array.getElem_replicate]
    by_cases hj : j < n
    · rw [getElem!_pos _ j (by simpa using hj), Array.getElem_replicate]
    · rw [getElem!_neg _ j (by simpa using hj)]
      rfl
  · rw [getElem!_neg _ i (by simpa using hi)]
    show (#[] : Array (Option Int))[j]! = none
    rw [getElem!_neg _ j (by simp)]
    rfl

theorem Matrix.Sound.replicate {ν : TermVal} {T : Nat → Term} (n : Nat) :
    Matrix.Sound ν T (Array.replicate n (Array.replicate n (none : Option Int))) := by
  intro i j v h
  rw [Matrix.get_replicate] at h
  cases h

/-- Lowering an entry to a value the valuation bounds keeps the matrix sound. -/
theorem Matrix.Sound.lower {ν : TermVal} {T : Nat → Term} {m : Matrix}
    (hm : Matrix.Sound ν T m) {i j : Nat} {v : Int}
    (hv : ν.val (T i) - ν.val (T j) ≤ v) : Matrix.Sound ν T (m.lower i j v) := by
  have key : Matrix.Sound ν T (m.set! i ((m[i]!).set! j (some v))) := by
    intro i' j' w hw
    rcases Matrix.get_setEntry m i j (some v) i' j' with h | ⟨rfl, rfl, h⟩
    · exact hm i' j' w (h.symm.trans hw)
    · rw [h] at hw
      cases hw
      exact hv
  unfold Matrix.lower
  split
  · split
    · exact key
    · exact hm
  · exact key

/-- Rule (1): a sum of two entries through a middle term bounds the outer
pair. -/
theorem transitive_sound {ν : TermVal} {T : Nat → Term} (n : Nat) {m : Matrix}
    (hm : Matrix.Sound ν T m) : Matrix.Sound ν T (transitive n m) := by
  unfold transitive
  refine foldl_inv (Matrix.Sound ν T) _ _ _ hm (fun m k _ hm => ?_)
  refine foldl_inv (Matrix.Sound ν T) _ _ _ hm (fun m i _ hm => ?_)
  split
  · exact hm
  · next dik hik =>
    have hik' := hm _ _ _ hik
    refine foldl_inv (Matrix.Sound ν T) _ _ _ hm (fun m j _ hm => ?_)
    split
    · exact hm
    · next dkj hkj =>
      apply hm.lower
      have := hm _ _ _ hkj
      omega

/-- Rule (2): a bound whose constant is the offset of a disequality on the same
pair tightens by one. -/
theorem strengthen_sound {ν : TermVal} {T : Nat → Term} {idx : Term → Nat}
    {ds : List Diseq} {m : Matrix}
    (hidx : ∀ d ∈ ds, T (idx d.a) = d.a ∧ T (idx d.b) = d.b)
    (hd : ∀ d ∈ ds, ν.val d.a - ν.val d.b ≠ d.c) (hm : Matrix.Sound ν T m) :
    Matrix.Sound ν T (strengthen idx ds m).1 := by
  unfold strengthen
  refine foldl_inv (fun p : Matrix × Bool => Matrix.Sound ν T p.1) _ ds (m, false) hm ?_
  rintro ⟨m, changed⟩ d hdm hm
  obtain ⟨ha, hb⟩ := hidx d hdm
  have hne := hd d hdm
  have step1 : ∀ m : Matrix, Matrix.Sound ν T m →
      m.get (idx d.a) (idx d.b) = some d.c →
      Matrix.Sound ν T (m.lower (idx d.a) (idx d.b) (d.c - 1)) := by
    intro m hm h
    apply hm.lower
    have := hm _ _ _ h
    rw [ha, hb] at this ⊢
    omega
  have step2 : ∀ m : Matrix, Matrix.Sound ν T m →
      m.get (idx d.b) (idx d.a) = some (-d.c) →
      Matrix.Sound ν T (m.lower (idx d.b) (idx d.a) (-d.c - 1)) := by
    intro m hm h
    apply hm.lower
    have := hm _ _ _ h
    rw [ha, hb] at this ⊢
    omega
  have hm0 : Matrix.Sound ν T m := hm
  dsimp only
  by_cases h1 : (m.get (idx d.a) (idx d.b) == some d.c) = true
  · rw [ite_eq_left h1]
    dsimp only
    have hm1 := step1 m hm0 (beq_iff_eq.mp h1)
    by_cases h2 : ((m.lower (idx d.a) (idx d.b) (d.c - 1)).get (idx d.b) (idx d.a) ==
        some (-d.c)) = true
    · rw [ite_eq_left h2]
      exact step2 _ hm1 (beq_iff_eq.mp h2)
    · rw [ite_eq_right h2]
      exact hm1
  · rw [ite_eq_right h1]
    dsimp only
    by_cases h2 : (m.get (idx d.b) (idx d.a) == some (-d.c)) = true
    · rw [ite_eq_left h2]
      exact step2 _ hm0 (beq_iff_eq.mp h2)
    · rw [ite_eq_right h2]
      exact hm0

theorem saturate_sound {ν : TermVal} {T : Nat → Term} {n : Nat} {idx : Term → Nat}
    {ds : List Diseq}
    (hidx : ∀ d ∈ ds, T (idx d.a) = d.a ∧ T (idx d.b) = d.b)
    (hd : ∀ d ∈ ds, ν.val d.a - ν.val d.b ≠ d.c) :
    ∀ (fuel : Nat) (m : Matrix), Matrix.Sound ν T m →
      Matrix.Sound ν T (saturate n idx ds fuel m)
  | 0, m, hm => by
    simp only [saturate]
    exact hm
  | fuel + 1, m, hm => by
    have h1 := transitive_sound n hm
    have h2 := strengthen_sound hidx hd h1
    simp only [saturate]
    split
    · exact saturate_sound hidx hd fuel _ h2
    · exact h1

/-- The last step of `close`: no diagonal entry of a sound matrix is negative,
so the state is not contradictory, and every bound read off the matrix holds. -/
theorem sat_of_matrix (U : Array Term) (m : Matrix) (ds : List Diseq) (ν : TermVal)
    (hm : Matrix.Sound ν (fun i => U[i]!) m)
    (hd : ∀ d ∈ ds, ν.val d.a - ν.val d.b ≠ d.c) :
    L0.Sat (if (List.range U.size).any
          (fun i => match m.get i i with | some v => v < 0 | none => false) then
        L0.bottom
      else
        { contra := false,
          bounds := (List.range U.size).flatMap fun i => (List.range U.size).filterMap
            fun j => if i == j then none else (m.get i j).map fun c => ⟨U[i]!, U[j]!, c⟩,
          diseqs := ds.eraseDups }) ν := by
  split
  · next hany =>
    exfalso
    obtain ⟨i, -, hi⟩ := List.any_eq_true.mp hany
    split at hi
    · next v hv =>
      have := hm i i v hv
      have hlt := of_decide_eq_true hi
      omega
    · cases hi
  · refine ⟨rfl, ?_, ?_⟩
    · intro b hb
      obtain ⟨i, -, hb⟩ := List.mem_flatMap.mp hb
      obtain ⟨j, -, hj⟩ := List.mem_filterMap.mp hb
      by_cases hij : (i == j) = true
      · rw [ite_eq_left hij] at hj
        cases hj
      · rw [ite_eq_right hij] at hj
        obtain ⟨c, hc, rfl⟩ := Option.map_eq_some_iff.mp hj
        exact hm i j c hc
    · intro d hdm
      exact hd d (List.mem_eraseDups.mp hdm)

/-! ## Closure and addition -/

/-- When the established facts hold and every term of the universe lies in its
type, the closure is satisfied. -/
theorem close_sat {live : List Term} {bs : List Bound} {ds : List Diseq} {ν : TermVal}
    (hb : ∀ b ∈ bs, ν.val b.a - ν.val b.b ≤ b.c)
    (hd : ∀ d ∈ ds, ν.val d.a - ν.val d.b ≠ d.c)
    (ht : ∀ t ∈ (termUniverse live bs ds).toList, ν.Typed t) :
    (close live bs ds).Sat ν := by
  let U := termUniverse live bs ds
  let idx : Term → Nat := fun t => (U.toList.findIdx? (· == t)).getD 0
  let T : Nat → Term := fun i => U[i]!
  have hidx : ∀ t ∈ U.toList, T (idx t) = t := fun t h => getElem!_findIdx? U t h
  have hz : Term.z ∈ U.toList := mem_termUniverse.mpr (Or.inl rfl)
  have hbs : ∀ b ∈ bs, b.a ∈ U.toList ∧ b.b ∈ U.toList := fun b h =>
    ⟨mem_termUniverse.mpr (Or.inr (Or.inr (Or.inl ⟨b, h, Or.inl rfl⟩))),
      mem_termUniverse.mpr (Or.inr (Or.inr (Or.inl ⟨b, h, Or.inr rfl⟩)))⟩
  have hds : ∀ d ∈ ds, d.a ∈ U.toList ∧ d.b ∈ U.toList := fun d h =>
    ⟨mem_termUniverse.mpr (Or.inr (Or.inr (Or.inr ⟨d, h, Or.inl rfl⟩))),
      mem_termUniverse.mpr (Or.inr (Or.inr (Or.inr ⟨d, h, Or.inr rfl⟩)))⟩
  -- every established and implicit bound holds and names terms of the universe
  have hall : ∀ b ∈ bs ++ U.toList.flatMap implicitBounds,
      ν.val b.a - ν.val b.b ≤ b.c ∧ b.a ∈ U.toList ∧ b.b ∈ U.toList := by
    intro b h
    rcases List.mem_append.mp h with h | h
    · exact ⟨hb b h, hbs b h⟩
    · obtain ⟨t, htU, hbt⟩ := List.mem_flatMap.mp h
      unfold implicitBounds at hbt
      split at hbt
      · next ty hty =>
        obtain ⟨hlo, hhi⟩ := ht t htU ty hty
        have h0 := ν.val_z
        simp only [List.mem_cons, List.not_mem_nil, or_false] at hbt
        rcases hbt with rfl | rfl
        · refine ⟨?_, htU, hz⟩
          show ν.val t - ν.val .z ≤ ty.maxVal
          omega
        · refine ⟨?_, hz, htU⟩
          show ν.val .z - ν.val t ≤ -ty.minVal
          omega
      · simp at hbt
  have h1 := foldl_inv (Matrix.Sound ν T) (fun m i => m.lower i i 0) (List.range U.size) _
    (Matrix.Sound.replicate U.size) (fun m i _ hm => hm.lower (by omega))
  have h2 := foldl_inv (Matrix.Sound ν T) (fun m b => m.lower (idx b.a) (idx b.b) b.c)
    (bs ++ U.toList.flatMap implicitBounds) _ h1 (fun m b hbm hm => hm.lower (by
      obtain ⟨h, ha, hb⟩ := hall b hbm
      rw [hidx _ ha, hidx _ hb]
      exact h))
  have h3 := saturate_sound (T := T) (n := U.size) (idx := idx)
    (fun d hdm => ⟨hidx _ (hds d hdm).1, hidx _ (hds d hdm).2⟩) hd (2 * ds.length + 1) _ h2
  exact sat_of_matrix U _ ds ν h3 hd

/-- Adding facts that hold to a satisfied state keeps it satisfied, when every
term of the new universe lies in its type. -/
theorem L0.add_sat {s : L0} {live : List Term} {bs : List Bound} {ds : List Diseq}
    {ν : TermVal} (hs : s.Sat ν) (hb : ∀ b ∈ bs, ν.val b.a - ν.val b.b ≤ b.c)
    (hd : ∀ d ∈ ds, ν.val d.a - ν.val d.b ≠ d.c)
    (ht : ∀ t ∈ (termUniverse live (s.bounds ++ bs) (s.diseqs ++ ds)).toList, ν.Typed t) :
    (s.add live bs ds).Sat ν := by
  obtain ⟨hc, hsb, hsd⟩ := hs
  unfold L0.add
  rw [ite_eq_right (show ¬(s.contra = true) by simp [hc])]
  refine close_sat (fun b h => ?_) (fun d h => ?_) ht
  · rcases List.mem_append.mp h with h | h
    · exact hsb b h
    · exact hb b h
  · rcases List.mem_append.mp h with h | h
    · exact hsd d h
    · exact hd d h

/-! ## Queries -/

theorem L0.tightest_sound {s : L0} {a b : Term} {c : Int} {ν : TermVal} (hs : s.Sat ν)
    (h : s.tightest a b = some c) : ν.val a - ν.val b ≤ c := by
  unfold L0.tightest at h
  split at h
  · next hab =>
    cases h
    rw [beq_iff_eq.mp hab]
    omega
  · obtain ⟨x, hx, hc⟩ := Option.map_eq_some_iff.mp h
    have hp := List.find?_some hx
    simp only [Bool.and_eq_true, beq_iff_eq] at hp
    obtain ⟨rfl, rfl⟩ := hp
    subst hc
    exact hs.2.1 x (List.mem_of_find?_eq_some hx)

theorem L0.bound_sound {s : L0} {a b : Term} {c : Int} {ν : TermVal} (hs : s.Sat ν)
    (h : s.bound a b c = true) : ν.val a - ν.val b ≤ c := by
  unfold L0.bound at h
  rw [hs.1, Bool.false_or] at h
  split at h
  · next c' hc' =>
    have := L0.tightest_sound hs hc'
    have hle := of_decide_eq_true h
    omega
  · cases h

theorem L0.diseq_sound {s : L0} {a b : Term} {c : Int} {ν : TermVal} (hs : s.Sat ν)
    (h : s.diseq a b c = true) : ν.val a - ν.val b ≠ c := by
  unfold L0.diseq at h
  rw [hs.1, Bool.false_or, Bool.or_eq_true, Bool.or_eq_true] at h
  rcases h with (h | h) | h
  · obtain ⟨d, hd, hp⟩ := List.any_eq_true.mp h
    simp only [Bool.or_eq_true, Bool.and_eq_true, beq_iff_eq] at hp
    have hne := hs.2.2 d hd
    rcases hp with ⟨⟨rfl, rfl⟩, rfl⟩ | ⟨⟨rfl, rfl⟩, h3⟩
    · exact hne
    · omega
  · have := L0.bound_sound hs h
    omega
  · have := L0.bound_sound hs h
    omega

theorem L0.eq_sound {s : L0} {a b : Term} {c : Int} {ν : TermVal} (hs : s.Sat ν)
    (h : s.eq a b c = true) : ν.val a - ν.val b = c := by
  unfold L0.eq at h
  rw [Bool.and_eq_true] at h
  have h1 := L0.bound_sound hs h.1
  have h2 := L0.bound_sound hs h.2
  omega

theorem L0.interval_sound {s : L0} {t : Term} {lo hi : Int} {ν : TermVal} (hs : s.Sat ν)
    (h : s.interval t = some (lo, hi)) : lo ≤ ν.val t ∧ ν.val t ≤ hi := by
  unfold L0.interval at h
  have hz := ν.val_z
  cases h1 : s.tightest t .z with
  | none =>
    rw [h1] at h
    simp at h
  | some hi' =>
    cases h2 : s.tightest .z t with
    | none =>
      rw [h1, h2] at h
      simp at h
    | some lo' =>
      rw [h1, h2] at h
      simp only [Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl⟩ := h
      have := L0.tightest_sound hs h1
      have := L0.tightest_sound hs h2
      omega

/-! ## Kill -/

/-- The facts a kill leaves hold under any valuation that agrees on every term
whose support the kill spares. -/
theorem L0.kill_sat {s : L0} {dead : Nat → Bool} {ν ν' : TermVal} (hs : s.Sat ν)
    (hν : ∀ t, t.support.any dead = false → ν'.val t = ν.val t) :
    (s.kill dead).Sat ν' := by
  obtain ⟨hc, hb, hd⟩ := hs
  unfold L0.kill
  rw [ite_eq_right (show ¬(s.contra = true) by simp [hc])]
  dsimp only
  refine ⟨hc, fun b hm => ?_, fun d hm => ?_⟩
  · obtain ⟨hm, hk⟩ := List.mem_filter.mp hm
    obtain ⟨ha, hb'⟩ := not_and_not hk
    rw [hν _ ha, hν _ hb']
    exact hb b hm
  · obtain ⟨hm, hk⟩ := List.mem_filter.mp hm
    obtain ⟨ha, hb'⟩ := not_and_not hk
    rw [hν _ ha, hν _ hb']
    exact hd d hm

/-! ## Join -/

/-- A satisfied state stays satisfied under a valuation that agrees on the terms
it names. -/
theorem L0.sat_congr {s : L0} {ν ν' : TermVal} (hs : s.Sat ν)
    (h : ∀ t ∈ s.terms, ν'.val t = ν.val t) : s.Sat ν' := by
  obtain ⟨hc, hb, hd⟩ := hs
  refine ⟨hc, fun b hm => ?_, fun d hm => ?_⟩
  · rw [h b.a (L0.mem_terms.mpr (Or.inl ⟨b, hm, Or.inl rfl⟩)),
      h b.b (L0.mem_terms.mpr (Or.inl ⟨b, hm, Or.inr rfl⟩))]
    exact hb b hm
  · rw [h d.a (L0.mem_terms.mpr (Or.inr ⟨d, hm, Or.inl rfl⟩)),
      h d.b (L0.mem_terms.mpr (Or.inr ⟨d, hm, Or.inr rfl⟩))]
    exact hd d hm

/-- The constant the join keeps for a pair is at least the first input's and at
least every other input's tightest bound on it. -/
theorem foldl_max {a b : Term} : ∀ (rest : List L0) (init : Option Int) (c : Int),
    rest.foldl (fun acc s => acc.bind fun c => (s.tightest a b).map fun c' => max c c')
      init = some c →
    (∃ c0, init = some c0 ∧ c0 ≤ c) ∧ ∀ s ∈ rest, ∃ c', s.tightest a b = some c' ∧ c' ≤ c
  | [], init, c, h => ⟨⟨c, h, Int.le_refl c⟩, by simp⟩
  | s :: rest, init, c, h => by
    rw [List.foldl_cons] at h
    obtain ⟨⟨c1, h1, hc1⟩, hrest⟩ := foldl_max rest _ c h
    cases init with
    | none => simp at h1
    | some c0 =>
      cases ht : s.tightest a b with
      | none =>
        rw [ht] at h1
        simp at h1
      | some c' =>
        rw [ht] at h1
        simp at h1
        have := Int.le_max_left c0 c'
        have := Int.le_max_right c0 c'
        refine ⟨⟨c0, rfl, by omega⟩, ?_⟩
        intro s' hs'
        rcases List.mem_cons.mp hs' with rfl | hs'
        · exact ⟨c', ht, by omega⟩
        · exact hrest s' hs'

/-- When one input of a join is satisfied and every term takes a member of its
type, the join is satisfied under the same valuation. -/
theorem join_sat_of_typed {live : List Term} {inputs : List L0} {s : L0} {ν : TermVal}
    (hin : s ∈ inputs) (hs : s.Sat ν) (hty : ∀ t, ν.Typed t) :
    (join live inputs).Sat ν := by
  have hsf : s ∈ inputs.filter (!·.contra) := List.mem_filter.mpr ⟨hin, by simp [hs.1]⟩
  unfold join
  split
  · next h =>
    rw [h] at hsf
    simp at hsf
  · next first rest h =>
    rw [h] at hsf
    refine close_sat ?_ ?_ (fun t _ => hty t)
    · intro b hb
      obtain ⟨b0, hb0, hmap⟩ := List.mem_filterMap.mp hb
      obtain ⟨c, hc, rfl⟩ := Option.map_eq_some_iff.mp hmap
      obtain ⟨⟨c0, hc0, hle⟩, hrest⟩ := foldl_max rest (some b0.c) c hc
      cases hc0
      show ν.val b0.a - ν.val b0.b ≤ c
      rcases List.mem_cons.mp hsf with rfl | hsr
      · have := hs.2.1 b0 hb0
        omega
      · obtain ⟨c', hc', hle'⟩ := hrest s hsr
        have := L0.tightest_sound hs hc'
        omega
    · intro d hd
      have hall := (List.mem_filter.mp hd).2
      exact L0.diseq_sound hs (List.all_eq_true.mp hall s hsf)

/-- The value of a term when the atoms in `keep` take their values under `ν` and
every other atom takes 0. -/
def TermVal.restrictVal (ν : TermVal) (keep : List Term) : Term → Int
  | .z => 0
  | .sum a b => restrictVal ν keep a + restrictVal ν keep b
  | .place id t => if Term.place id t ∈ keep then ν.val (.place id t) else 0
  | .commit id t => if Term.commit id t ∈ keep then ν.val (.commit id t) else 0
  | .datum id t => if Term.datum id t ∈ keep then ν.val (.datum id t) else 0

/-- `ν` on the atoms in `keep`, 0 on every other atom. -/
def TermVal.restrict (ν : TermVal) (keep : List Term) : TermVal where
  val := ν.restrictVal keep
  val_z := rfl
  val_sum _ _ := rfl

theorem TermVal.restrict_atom (ν : TermVal) (keep : List Term) (t : Term)
    (hv : (ν.restrict keep).val t = if t ∈ keep then ν.val t else 0) :
    (t ∈ keep → (ν.restrict keep).val t = ν.val t) ∧
      ((∀ u ∈ keep, ν.Typed u) → (ν.restrict keep).Typed t) := by
  refine ⟨fun hk => by rw [hv, ite_eq_left hk], fun h ty hty => ?_⟩
  rw [hv]
  by_cases hk : t ∈ keep
  · rw [ite_eq_left hk]
    exact h t hk ty hty
  · rw [ite_eq_right hk]
    exact zero_mem_type ty

theorem TermVal.restrict_val (ν : TermVal) (keep : List Term) :
    ∀ t : Term, (∀ u ∈ t.atoms, u ∈ keep) → (ν.restrict keep).val t = ν.val t
  | .z, _ => ν.val_z.symm
  | .sum a b, h => by
    have ha : ν.restrictVal keep a = ν.val a :=
      restrict_val ν keep a (fun u hu => h u (List.mem_append_left _ hu))
    have hb : ν.restrictVal keep b = ν.val b :=
      restrict_val ν keep b (fun u hu => h u (List.mem_append_right _ hu))
    show ν.restrictVal keep a + ν.restrictVal keep b = ν.val (.sum a b)
    rw [ν.val_sum, ha, hb]
  | .place id t, h =>
    (restrict_atom ν keep (.place id t) rfl).1 (h _ (List.mem_singleton_self _))
  | .commit id t, h =>
    (restrict_atom ν keep (.commit id t) rfl).1 (h _ (List.mem_singleton_self _))
  | .datum id t, h =>
    (restrict_atom ν keep (.datum id t) rfl).1 (h _ (List.mem_singleton_self _))

theorem TermVal.restrict_typed (ν : TermVal) (keep : List Term)
    (h : ∀ u ∈ keep, ν.Typed u) : ∀ t, (ν.restrict keep).Typed t
  | .z => by
    intro ty hty
    cases hty
  | .sum _ _ => by
    intro ty hty
    cases hty
  | .place id t => (restrict_atom ν keep (.place id t) rfl).2 h
  | .commit id t => (restrict_atom ν keep (.commit id t) rfl).2 h
  | .datum id t => (restrict_atom ν keep (.datum id t) rfl).2 h

/-- When the input actually taken is satisfied and the atoms of the terms it and
the live list name lie in their types, some valuation that agrees with it on
those terms, and gives every term a member of its type, satisfies the join.
Terms that only other inputs name may have no value on this path; the join
keeps no fact on them but a reflexive disequality, which any value satisfies. -/
theorem join_sat {live : List Term} {inputs : List L0} {s : L0} {ν : TermVal}
    (hin : s ∈ inputs) (hs : s.Sat ν)
    (hty : ∀ t ∈ live ++ s.terms, ∀ u ∈ t.atoms, ν.Typed u) :
    ∃ ν' : TermVal, (∀ t ∈ live ++ s.terms, ν'.val t = ν.val t) ∧
      (∀ t, ν'.Typed t) ∧ (join live inputs).Sat ν' := by
  let keep := (live ++ s.terms).flatMap Term.atoms
  have hagree : ∀ t ∈ live ++ s.terms, (ν.restrict keep).val t = ν.val t := fun t ht =>
    TermVal.restrict_val ν keep t (fun u hu => List.mem_flatMap.mpr ⟨t, ht, hu⟩)
  have htyped : ∀ t, (ν.restrict keep).Typed t :=
    TermVal.restrict_typed ν keep (fun u hu => by
      obtain ⟨t, ht, hut⟩ := List.mem_flatMap.mp hu
      exact hty t ht u hut)
  have hs' : s.Sat (ν.restrict keep) :=
    L0.sat_congr hs (fun t ht => hagree t (List.mem_append_right _ ht))
  exact ⟨ν.restrict keep, hagree, htyped, join_sat_of_typed hin hs' htyped⟩

end Fact

end Whitefoot
