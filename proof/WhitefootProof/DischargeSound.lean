import WhitefootProof.Holds

/-!
# The disposition proves only goals that hold

The disposition (`discharged`) proves a query's goal by any of its forms: the
signed goal itself, an L0 normalization, the affine targets with their bridges,
an alternative of affine targets, or the interval product [MSR-4]. Under a
valuation that makes a snapshot hold (`Holds.lean`):

- `discharged_sound`: what a discharged query means holds (`Query.Means`); on
  the way, the L0 images and the view an affine query reads are satisfied;
- `relQuery_means`: a relation query means its relation over the names'
  values [ENT-2, ENT-6];
- `evalAtom_sound`: an atom the checker evaluated never sticks, as each
  subscript is below its length, and its checker value describes its runtime
  value [OP-4];
- `domainQuery_means`: an exact infix operation's domain query means its
  domain [OP-2], with the operands' intervals and the interval product sound
  on the way.
-/

namespace Whitefoot

open Fact Aff

namespace Checker

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
            | .datum _ _ | .commit _ _ => (st.data.find? (·.1 == x)).map (·.2)
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
        | commit id ty =>
          obtain ⟨p, hp, rfl⟩ := Option.map_eq_some_iff.mp hfx
          have hid : p.1 = Term.commit id ty := by simpa using List.find?_some hp
          rw [← hid]
          exact h.data p (List.mem_of_find?_eq_some hp)
        | z => cases hfx
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
    simp only [Whitefoot.evalAtom, hb, ho, Typing.ok_bind, CheckerRun.error_bind]
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
    obtain ⟨hv, hg1⟩ := CheckerRun.run_pure h
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
    obtain ⟨hv, hg1⟩ := CheckerRun.run_pure h
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
      exact (CheckerRun.run_fail h).elim
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
        obtain ⟨hv', hg1⟩ := CheckerRun.run_pure h
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
        obtain ⟨hv', hg1⟩ := CheckerRun.run_pure h
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
      exact (CheckerRun.run_fail h).elim
    | some vb =>
      simp only [Checker.evalAtom, hv] at h
      obtain ⟨o, g2, ho, h⟩ := CheckerRun.run_bind h
      obtain ⟨hnost, G2, hg2, hext2, -, hden⟩ := evalAtom_sound off hg hlit ho
      cases hty : vb.ty with
      | array t n =>
        simp only [hty] at h
        obtain ⟨g3, g4, hget, h⟩ := CheckerRun.run_bind h
        obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
        rw [h3, h4] at h
        try dsimp only at h
        split at h
        · exfalso
          first
            | exact CheckerRun.run_fail h
            | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind h; exact CheckerRun.run_fail hf)
        · rename_i hnd
          have hdis := CheckerRun.not_not_true hnd
          have h' : (Checker.typeAtom t >>= fun img =>
              (pure { image := some img, gexpr := o.gexpr.map (GExpr.index vb.id ·),
                      ty := .int t } : Checker.M Val)).run g2 = .ok (v, g1) := h
          obtain ⟨img, g6, himg, h⟩ := CheckerRun.run_bind h'
          rw [CheckerRun.typeAtom_run] at himg
          simp only [Except.ok.injEq, Prod.mk.injEq] at himg
          obtain ⟨rfl, rfl⟩ := himg
          obtain ⟨hv', hg1'⟩ := CheckerRun.run_pure h
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
        exact (CheckerRun.run_fail h).elim


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

end Checker

end Whitefoot
