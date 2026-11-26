import WhitefootProof.ObligationSound

/-!
# The values of right-hand sides and the postconditions at a return

What the checker's value of an accepted right-hand side says of its runtime
value, which the transfer proofs use, and the obligation of a return:

- `infix_sound`, `op_sound` and `call_sound`: the checker's value of an infix
  operation, a table operation, or a call of `exit_status` or `array_filled`
  describes the runtime value;
- `evalRhs_sound` and `evalRhs_val`: so does the value of any right-hand side
  that calls no function of the program, and the snapshot holds and stays
  fresh at the generator the right-hand side leaves;
- `return_ensures`: when the state a return's check reads holds and the value
  it checks describes the returned value, that value lies in its type's range
  and the function's postconditions hold of it and of the parameters' entry
  values [FN-9].
-/

namespace Whitefoot

open Fact Aff

namespace Checker

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
  ⟨fun _ _ h => (by simp at h), fun _ h => (by simp at h), hgx, ht⟩

theorem typeAtom_out {t : IntTy} {g g' : Gen} {img : Form}
    (h : (typeAtom t).run g = .ok (img, g')) :
    img = Form.atom g.next ∧
      g' = { g with next := g.next + 1, atoms := (g.next, t.minVal, t.maxVal) :: g.atoms } := by
  rw [CheckerRun.typeAtom_run] at h
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
local syntax "int_walk " ident : tactic
local macro_rules
  | `(tactic| int_walk $h:ident) => `(tactic| (
      try dsimp only at $h:ident
      obtain ⟨_, _, hget, $h:ident⟩ := CheckerRun.run_bind $h
      obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
      rw [h3, h4] at $h:ident
      try dsimp only at $h:ident
      try simp only [Bool.false_and, Bool.false_eq_true, ↓reduceIte] at $h:ident
      try split at $h:ident
      all_goals first
        | (exfalso; exact CheckerRun.run_fail $h)
        | (exfalso; obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind $h; exact CheckerRun.run_fail hf)
        | skip
      try dsimp only at $h:ident))

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
  obtain ⟨va, g2, hva, hr⟩ := CheckerRun.run_bind hr
  obtain ⟨vb, g3, hvb, hr⟩ := CheckerRun.run_bind hr
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
          cases hfa : va.image with
          | none =>
            rw [hfa] at hr
            dsimp only at hr
            obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
            obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure himg
              refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
              obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
              (rw [hm .add rfl, Form.eval_add, ← hx, ← hy]) <;> rfl
        · int_walk hr
          cases hfa : va.image with
          | none =>
            rw [hfa] at hr
            dsimp only at hr
            obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
            obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure himg
              refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
              obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
              (rw [hm .sub rfl, Form.eval_sub, ← hx, ← hy]) <;> rfl
        · int_walk hr
          cases hfa : va.image with
          | none =>
            rw [hfa] at hr
            dsimp only at hr
            obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
            obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
            exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
          | some fa =>
            cases hfb : vb.image with
            | none =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
            | some fb =>
              rw [hfa, hfb] at hr
              dsimp only at hr
              split at hr
              · rename_i hc
                obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
                obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
                obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure himg
                refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inr ⟨rfl, fun w hw => ?_⟩)
                obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
                (rw [hm .mul rfl, Form.eval_scale, ← Form.eval_isConst (G := G3) hc, ← hx,
                  ← hy]) <;> rfl
              · split at hr
                · rename_i _ hc
                  obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
                  obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
                  obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure himg
                  refine int_arm_end hg3 hext23 (hres rfl t hvt) hgx
                    (Or.inr ⟨rfl, fun w hw => ?_⟩)
                  obtain ⟨x, y, q, rfl, hx, hy, hm⟩ := himgs fa fb hfa hfb w hw rfl t hvt
                  rw [hm .mul rfl, Form.eval_scale, ← Form.eval_isConst (G := G3) hc, ← hx, ← hy]
                  show Value.int t (x * y) = Value.int t (y * x)
                  rw [Int.mul_comm]
                · obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
                  obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
                  exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
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
          obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
          obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- `defined` arithmetic
      dsimp only at hr
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
      exact ⟨G3, hg3, hext23, hbool rfl⟩
  · cases m
    · -- exact division and remainder
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        cases o
        all_goals
          int_walk hr
          obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
          obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
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
          obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
          obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
          exact int_arm_end hg3 hext23 (hres rfl t hvt) hgx (Or.inl (typeAtom_out himg))
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
        refine ⟨G3, hg3, hext23, fun w hw => ?_⟩
        obtain ⟨t, ht⟩ := hint rfl w hw
        rw [hvt] at ht
        cases ht
    · -- `defined` division
      dsimp only at hr
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
      exact ⟨G3, hg3, hext23, hbool rfl⟩
  · -- comparisons
    dsimp only at hr
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
    exact ⟨G3, hg3, hext23, hbool rfl⟩

/-! ## Table and call values -/

/-- Each checker value of a list describes the runtime value at its place. -/
inductive ValsDenote (σ : Nat → Option Value) (ν : TermVal) (G : Nat → Int) :
    List Val → List Value → Prop
  | nil : ValsDenote σ ν G [] []
  | cons {v : Val} {w : Value} {vs : List Val} {ws : List Value} :
      v.Denotes σ ν G w → ValsDenote σ ν G vs ws → ValsDenote σ ν G (v :: vs) (w :: ws)

/-- The checker values of a list of atoms describe their runtime values, at
the valuation the last atom leaves [OP-4]. -/
theorem evalAtoms_val {env : Env} {st : State} {E E0 : Whitefoot.Env} {ν : TermVal} :
    ∀ (as : List Atom) {g g1 : Gen} {vs : List Val} {G : Nat → Int},
      Good env st g E E0 G ν → (∀ a ∈ as, a.LitOk) →
      (as.mapM (Checker.evalAtom env st)).run g = .ok (vs, g1) →
      ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
        ∀ ws, as.mapM (Whitefoot.evalAtom E) = .ok ws →
          ValsDenote (valueOf env.vars E) ν G1 vs ws
  | [], g, g1, vs, G, hg, _, h => by
    rw [List.mapM_nil] at h
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h
    refine ⟨G, hg, Ext.refl _ _, fun ws hws => ?_⟩
    rw [List.mapM_nil] at hws
    cases hws
    exact .nil
  | a :: as, g, g1, vs, G, hg, hlit, h => by
    rw [List.mapM_cons] at h
    obtain ⟨v, g2, hv, h⟩ := CheckerRun.run_bind h
    obtain ⟨vs', g3, hvs, h⟩ := CheckerRun.run_bind h
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h
    obtain ⟨-, G2, hg2, hext2, hbel2, hden2⟩ :=
      evalAtom_sound a hg (hlit a List.mem_cons_self) hv
    obtain ⟨G3, hg3, hext3, hden3⟩ :=
      evalAtoms_val as hg2 (fun b hb => hlit b (List.mem_cons_of_mem _ hb)) hvs
    refine ⟨G3, hg3, Ext.trans hext2 hext3, fun ws hws => ?_⟩
    rw [List.mapM_cons] at hws
    obtain ⟨w, hw, hws⟩ := Typing.bind_ok hws
    obtain ⟨ws', hws', hws⟩ := Typing.bind_ok hws
    cases hws
    exact .cons ((hden2 w hw).congr fun f hf x hx => hext3.2 x (hbel2 f hf x hx))
      (hden3 ws' hws')

/-- A table step gives a `Bool` unless it is an integer negation or absolute
value of one operand. -/
theorem evalOp_bool {o : TableOp} {ws : List Value} {w : Value}
    (ho : ∀ u m, o = .unary u m → m = .defined ∨ ws.length ≠ 1)
    (h : evalOp o ws = .ok w) : w.Typed .bool := by
  unfold evalOp at h
  split at h
  · rename_i u m t x
    have hm : m = .defined := by
      rcases ho u m rfl with hm | hm
      · exact hm
      · exact absurd rfl hm
    subst hm
    split at h
    · simp [ill] at h
    · dsimp only at h
      cases h
      exact trivial
  all_goals first
    | (cases h; exact trivial)
    | (simp [ill] at h)

theorem unary_exactForm_val {u : UnaryOp} {t : IntTy} {x q : Int}
    (h : u.exactForm t x = some q) : q = u.math x := by
  unfold UnaryOp.exactForm at h
  by_cases hd : u.defined t x = true
  · rw [ite_eq_left hd] at h
    cases h
    rfl
  · rw [ite_eq_right hd] at h
    cases h

/-- An exact or wrapping negation or absolute value that gives a value gives
an integer of its operand's type in its range; an exact one gives the
mathematical result. -/
theorem evalOp_unary_int {u : UnaryOp} {m : Mode} {t : IntTy} {x : Int} {w : Value}
    (hm : m ≠ .defined) (hx : t.Mem x) (h : evalOp (.unary u m) [.int t x] = .ok w) :
    ∃ q, w = .int t q ∧ t.Mem q ∧ (m = .exact → q = u.math x) := by
  simp only [evalOp] at h
  split at h
  · simp [ill] at h
  · rename_i hsig
    have hs : t.signed = true := by simpa using hsig
    cases m with
    | exact =>
      dsimp only at h
      cases hq : u.exactForm t x with
      | none => simp [hq, Whitefoot.stuck] at h
      | some q =>
        simp [hq] at h
        subst h
        exact ⟨q, rfl, UnaryOp.exactForm_mem hs hx hq, fun _ => unary_exactForm_val hq⟩
    | wrap =>
      dsimp only at h
      cases h
      exact ⟨_, rfl, IntTy.wrap_mem t _, fun h => by cases h⟩
    | defined => exact absurd rfl hm

/-- A negation or absolute value that gives a value took an integer. -/
theorem evalOp_unary_arg {u : UnaryOp} {m : Mode} {x w : Value}
    (h : evalOp (.unary u m) [x] = .ok w) : ∃ t z, x = .int t z := by
  cases x
  case int t z => exact ⟨t, z, rfl⟩
  all_goals simp [evalOp, ill] at h

/-- A one-operand Goal expression gives the interpreter's result when its
operand's expression gives the operand. -/
theorem mk1_op_eval {σ : Nat → Option Value} {o : TableOp} {ga : Option GExpr}
    {wa w : Value} (ha : ∀ e, ga = some e → e.eval σ = some wa)
    (h : evalOp o [wa] = .ok w) :
    ∀ e, mk1 (GExpr.op1 o) ga = some e → e.eval σ = some w := by
  intro e he
  cases ga with
  | none => simp [mk1] at he
  | some ea =>
    simp [mk1] at he
    subst he
    simp only [GExpr.eval, ha ea rfl, h, Except.toOption]

/-- A two-operand table Goal expression gives the interpreter's result when
its operands' expressions give the operands. -/
theorem mk2_op_eval {σ : Nat → Option Value} {o : TableOp} {ga gb : Option GExpr}
    {wa wb w : Value} (ha : ∀ e, ga = some e → e.eval σ = some wa)
    (hb : ∀ e, gb = some e → e.eval σ = some wb) (h : evalOp o [wa, wb] = .ok w) :
    ∀ e, mk2 (GExpr.op2 o) ga gb = some e → e.eval σ = some w := by
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

/-- A value with only a type describes every runtime value of that type. -/
theorem ty_val_denotes {σ : Nat → Option Value} {ν : TermVal} {G : Nat → Int} {ty : Ty}
    {w : Value} (ht : w.Typed ty) : ({ ty := ty } : Val).Denotes σ ν G w :=
  ⟨fun _ _ h => (by simp at h), fun _ h => (by simp at h), fun _ h => (by simp at h), ht⟩

set_option maxHeartbeats 2000000 in
/-- The value of an accepted table right-hand side: its checker value
describes the runtime value. -/
theorem op_sound {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {o : TableOp} {as : List Atom} {r : Rhs} {sc : Static.Scope}
    {ty : Ty} (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.op o as)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.op o as) = .ok ty) :
    ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
      ∀ w, evalExpr env.prog E (.op o as) = .ok w →
        r.val.Denotes (valueOf env.vars E) ν G1 w := by
  obtain ⟨ts, hts, -⟩ := Typing.bind_ok hty
  simp only [evalRhs] at hr
  obtain ⟨vs, g2, hvs, hr⟩ := CheckerRun.run_bind hr
  obtain ⟨G2, hg2, hext2, hden⟩ := evalAtoms_val as hgood (mapM_litOk hts) hvs
  have hrun : ∀ w, evalExpr env.prog E (.op o as) = .ok w →
      ∃ ws, ValsDenote (valueOf env.vars E) ν G2 vs ws ∧ evalOp o ws = .ok w := by
    intro w hw
    simp only [evalExpr] at hw
    obtain ⟨ws, hws, hw⟩ := Typing.bind_ok hw
    exact ⟨ws, hden ws hws, hw⟩
  rcases vs with _ | ⟨va, _ | ⟨vb, _ | ⟨vc, vrest⟩⟩⟩
  · exfalso
    cases o <;> (dsimp only at hr; exact CheckerRun.run_fail hr)
  · -- one operand
    have hone : ∀ w, evalExpr env.prog E (.op o as) = .ok w →
        ∃ x, va.Denotes (valueOf env.vars E) ν G2 x ∧ evalOp o [x] = .ok w := by
      intro w hw
      obtain ⟨ws, hD, he⟩ := hrun w hw
      cases hD with
      | cons hd hrest =>
        cases hrest
        exact ⟨_, hd, he⟩
    have hgx : ∀ w, evalExpr env.prog E (.op o as) = .ok w → ∀ e,
        mk1 (GExpr.op1 o) va.gexpr = some e → e.eval (valueOf env.vars E) = some w := by
      intro w hw
      obtain ⟨x, hd, he⟩ := hone w hw
      exact mk1_op_eval hd.gexpr he
    have hbool : (∀ u m, o = .unary u m → m = .defined) → ∀ w,
        evalExpr env.prog E (.op o as) = .ok w →
        ({ gexpr := mk1 (GExpr.op1 o) va.gexpr, ty := .bool } : Val).Denotes
          (valueOf env.vars E) ν G2 w := by
      intro ho w hw
      obtain ⟨x, -, he⟩ := hone w hw
      exact bool_val_denotes (hgx w hw) (evalOp_bool (fun u m h => .inl (ho u m h)) he)
    rcases o with ⟨u, m⟩ | _ | _ | _ | _
    · dsimp only at hr
      cases hvt : va.ty
      case int t =>
        rw [hvt] at hr
        have hint : ∀ w, evalExpr env.prog E (.op (.unary u m) as) = .ok w → ∃ z,
            va.Denotes (valueOf env.vars E) ν G2 (.int t z) ∧ t.Mem z ∧
              evalOp (.unary u m) [.int t z] = .ok w := by
          intro w hw
          obtain ⟨x, hd, he⟩ := hone w hw
          obtain ⟨t', z, rfl⟩ := evalOp_unary_arg he
          have ht' := hd.typed.int_ty
          rw [hvt] at ht'
          cases ht'
          exact ⟨z, hd, hd.typed.int_mem, he⟩
        have hres : m ≠ .defined → ∀ w, evalExpr env.prog E (.op (.unary u m) as) = .ok w →
            ∃ q, w = .int t q ∧ t.Mem q := by
          intro hm w hw
          obtain ⟨z, -, hz, he⟩ := hint w hw
          obtain ⟨q, hq, hqm, -⟩ := evalOp_unary_int hm hz he
          exact ⟨q, hq, hqm⟩
        cases m
        · -- exact negation and absolute value
          dsimp only at hr
          try simp only [beq_self_eq_true, ↓reduceIte] at hr
          obtain ⟨gen, g4, hget, hr⟩ := CheckerRun.run_bind hr
          obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
          rw [h3, h4] at hr
          try dsimp only at hr
          split at hr
          · exfalso
            first
              | exact CheckerRun.run_fail hr
              | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hr; exact CheckerRun.run_fail hf)
          · cases u
            · cases hfa : va.image with
              | none =>
                rw [hfa] at hr
                dsimp only at hr
                obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
                obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
                exact int_arm_end hg2 hext2 (hres (by decide)) hgx (Or.inl (typeAtom_out himg))
              | some f =>
                rw [hfa] at hr
                dsimp only at hr
                obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
                obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
                obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure himg
                refine int_arm_end hg2 hext2 (hres (by decide)) hgx
                  (Or.inr ⟨rfl, fun w hw => ?_⟩)
                obtain ⟨z, hd, hz, he⟩ := hint w hw
                obtain ⟨q, rfl, -, hq⟩ := evalOp_unary_int (by decide) hz he
                obtain ⟨t1, h1⟩ := hd.image f hfa
                injection h1 with _ hx
                rw [hq rfl, Form.eval_scale, ← hx]
                show Value.int t (-z) = Value.int t (-1 * z)
                exact congrArg (Value.int t) (by omega)
            · dsimp only at hr
              obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
              obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
              exact int_arm_end hg2 hext2 (hres (by decide)) hgx (Or.inl (typeAtom_out himg))
        · -- wrapping negation and absolute value
          dsimp only at hr
          try simp only [show (Mode.wrap == Mode.exact) = false by decide, Bool.false_eq_true,
            ↓reduceIte] at hr
          cases u
          all_goals
            try dsimp only at hr
            obtain ⟨img, g5, himg, hr⟩ := CheckerRun.run_bind hr
            obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
            exact int_arm_end hg2 hext2 (hres (by decide)) hgx (Or.inl (typeAtom_out himg))
        · -- `defined` queries
          dsimp only at hr
          obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
          exact ⟨G2, hg2, hext2, hbool (fun _ _ h => by cases h; rfl)⟩
      all_goals
        rw [hvt] at hr
        dsimp only at hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
        refine ⟨G2, hg2, hext2, fun w hw => ?_⟩
        obtain ⟨x, hd, he⟩ := hone w hw
        obtain ⟨t', z, rfl⟩ := evalOp_unary_arg he
        have ht' := hd.typed.int_ty
        rw [hvt] at ht'
        cases ht'
    all_goals
      dsimp only at hr
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
      exact ⟨G2, hg2, hext2, hbool (fun _ _ h => by cases h)⟩
  · -- two operands
    have hden2 : ∀ w, evalExpr env.prog E (.op o as) = .ok w →
        ({ gexpr := mk2 (GExpr.op2 o) va.gexpr vb.gexpr, ty := .bool } : Val).Denotes
          (valueOf env.vars E) ν G2 w := by
      intro w hw
      obtain ⟨ws, hD, he⟩ := hrun w hw
      cases hD with
      | cons hda hrest =>
        cases hrest with
        | cons hdb hrest' =>
          cases hrest'
          exact bool_val_denotes (mk2_op_eval hda.gexpr hdb.gexpr he)
            (evalOp_bool (fun _ _ _ => .inr (by simp)) he)
    cases o <;>
      (dsimp only at hr
       obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
       exact ⟨G2, hg2, hext2, hden2⟩)
  · exfalso
    cases o <;> (dsimp only at hr; exact CheckerRun.run_fail hr)

set_option maxHeartbeats 1000000 in
/-- The value of an accepted call of `exit_status` or `array_filled`: its
checker value describes the runtime value. -/
theorem call_sound {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {r : Rhs} {sc : Static.Scope} {ty : Ty}
    (hgood : Good env st g E E0 G ν)
    (hr : (evalRhs env st (.call path targs as)).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc (.call path targs as) = .ok ty) (hcall : r.call = none) :
    ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
      ∀ w, evalExpr env.prog E (.call path targs as) = .ok w →
        r.val.Denotes (valueOf env.vars E) ν G1 w := by
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
      obtain ⟨args, g2, -, hr⟩ := CheckerRun.run_bind hr
      obtain ⟨gen, g3, -, hr⟩ := CheckerRun.run_bind hr
      obtain ⟨u, g4, -, hr⟩ := CheckerRun.run_bind hr
      split at hr
      all_goals
        obtain ⟨img, g5, -, hr⟩ := CheckerRun.run_bind hr
        obtain ⟨hr', -⟩ := CheckerRun.run_pure hr
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
        obtain ⟨vs, g2, hvs, hr⟩ := CheckerRun.run_bind hr
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
        rw [List.mapM_cons, List.mapM_nil] at hvs
        obtain ⟨v, g3, hv, hvs⟩ := CheckerRun.run_bind hvs
        try dsimp only at hv
        obtain ⟨bs, g4, hbs, hvs⟩ := CheckerRun.run_bind hvs
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hbs
        obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hvs
        obtain ⟨-, G3, hg3, hext3, -, -⟩ := evalAtom_sound a hgood (atomTy_litOk hat) hv
        refine ⟨G3, hg3, hext3, fun w hw => ?_⟩
        simp only [evalExpr, hc] at hw
        obtain ⟨wa, -, hw⟩ := Typing.bind_ok hw
        split at hw
        · cases hw
          exact ty_val_denotes (by first | rfl | simp [Value.Typed, Static.exitStatusTy])
        · simp [ill] at hw
    | arrayFilled =>
      rcases targs with _ | ⟨(et | _ | _ | ⟨_, _⟩ | _) | _, _ | ⟨_ | n, _ | ⟨_, _⟩⟩⟩ <;>
        try exact absurd hty Typing.reject_ne
      obtain ⟨_, hn, hty⟩ := Typing.bind_ok hty
      obtain ⟨_, ha, -⟩ := Typing.bind_ok hty
      obtain ⟨a, rfl, hat⟩ := Typing.atomTy_one hn ha
      dsimp only at hr
      obtain ⟨vs, g2, hvs, hr⟩ := CheckerRun.run_bind hr
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
      rw [List.mapM_cons, List.mapM_nil] at hvs
      obtain ⟨v, g3, hv, hvs⟩ := CheckerRun.run_bind hvs
      try dsimp only at hv
      obtain ⟨bs, g4, hbs, hvs⟩ := CheckerRun.run_bind hvs
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hbs
      obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hvs
      obtain ⟨-, G3, hg3, hext3, -, hden⟩ := evalAtom_sound a hgood (atomTy_litOk hat) hv
      refine ⟨G3, hg3, hext3, fun w hw => ?_⟩
      simp only [evalExpr, hc] at hw
      obtain ⟨wa, hwa, hw⟩ := Typing.bind_ok hw
      split at hw
      · rename_i t' x
        split at hw
        · rename_i htt
          cases hw
          have hx : t'.Mem x := (hden _ hwa).typed.int_mem
          have he : et = t' := by simpa using htt
          subst he
          refine ty_val_denotes
            (show Value.Typed (.array et (List.replicate n x)) (.array et n) from
              ⟨rfl, by simp, fun z hz => ?_⟩)
          rw [List.eq_of_mem_replicate hz]
          exact hx
        · simp [ill] at hw
      · simp [ill] at hw

/-- The value of an accepted right-hand side that calls no function of the
program: its checker value describes the runtime value [OP-2, OP-4]. -/
theorem evalRhs_sound {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {e : Expr} {r : Rhs} {sc : Static.Scope} {ty : Ty}
    (hgood : Good env st g E E0 G ν) (hr : (evalRhs env st e).run g = .ok (r, g1))
    (hty : Static.exprTy env.prog sc e = .ok ty) (hcall : r.call = none) :
    ∃ G1, Good env st g1 E E0 G1 ν ∧ Ext g g1 G G1 ∧
      ∀ w, evalExpr env.prog E e = .ok w → r.val.Denotes (valueOf env.vars E) ν G1 w := by
  rcases e with a | ⟨a, op, b⟩ | ⟨o, as⟩ | ⟨path, targs, as⟩ | bv
  · simp only [evalRhs] at hr
    obtain ⟨v, g2, hv, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
    obtain ⟨-, G2, hg2, hext2, -, hden⟩ := evalAtom_sound a hgood (atomTy_litOk hty) hv
    exact ⟨G2, hg2, hext2, fun w hw => hden w hw⟩
  · exact infix_sound hgood hr hty
  · exact op_sound hgood hr hty
  · exact call_sound hgood hr hty hcall
  · simp only [evalRhs] at hr
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hr
    refine ⟨G, hgood, Ext.refl _ _, fun w hw => ?_⟩
    simp only [evalExpr] at hw
    cases hw
    exact bool_val_denotes (fun e he => by cases he; rfl) trivial

/-- The value-level form the transfer proofs use: after an accepted
right-hand side that calls no function of the program, the snapshot holds at
the new generator, is fresh there, and the checker value describes the
runtime value. -/
theorem evalRhs_val {env : Env} {st : State} {g g1 : Gen} {E E0 : Whitefoot.Env}
    {e : Expr} {r : Rhs}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0))
    (hf : (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next)
    (hr : (evalRhs env st e).run g = .ok (r, g1))
    (ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t) (hcall : r.call = none) :
    ∃ G1 ν, (⟨env.vars, st, g1.atoms⟩ : Snapshot).HoldsWith E E0 G1 ν ∧
      (⟨env.vars, st, g1.atoms⟩ : Snapshot).Fresh g1.next ∧
      ∀ w, evalExpr env.prog E e = .ok w → r.val.Denotes (valueOf env.vars E) ν G1 w := by
  obtain ⟨G, ν, hw⟩ := hs
  obtain ⟨sc, ty, hty⟩ := ht
  obtain ⟨G1, hg1, -, hden⟩ := evalRhs_sound ⟨hw, hf⟩ hr hty hcall
  exact ⟨G1, ν, hg1.holds, hg1.fresh, hden⟩

/-! ## Postconditions at a return -/

/-- The names one side of a relation carries once normalized [FN-9]. -/
def sideNames (a : Affine) : List String :=
  match lin a with
  | some l => l.clean.2.map (·.1)
  | none => Affine.names a

theorem relNames_eq (r : Relation) : relNames r = sideNames r.lhs ++ sideNames r.rhs := by
  first
    | rfl
    | simp only [relNames, sideNames]

theorem linSum_congr {val1 val2 : String → Int} :
    ∀ (l : List (String × Int)), (∀ q ∈ l, val1 q.1 = val2 q.1) →
      linSum val1 l = linSum val2 l
  | [], _ => rfl
  | p :: rest, h => by
    simp only [linSum]
    rw [h p List.mem_cons_self, linSum_congr rest fun q hq => h q (List.mem_cons_of_mem _ hq)]

theorem affVal_congr {val1 val2 : String → Int} :
    ∀ (a : Affine), (∀ x ∈ Affine.names a, val1 x = val2 x) → affVal val1 a = affVal val2 a
  | .atom (.int _ _), _ => rfl
  | .atom (.var x), h => h x (by simp [Affine.names])
  | .atom .unit, _ => rfl
  | .atom (.index _ _), _ => rfl
  | .add a b, h => by
    simp only [affVal]
    rw [affVal_congr a fun x hx => h x (by simp [Affine.names, hx]),
      affVal_congr b fun x hx => h x (by simp [Affine.names, hx])]
  | .sub a b, h => by
    simp only [affVal]
    rw [affVal_congr a fun x hx => h x (by simp [Affine.names, hx]),
      affVal_congr b fun x hx => h x (by simp [Affine.names, hx])]
  | .mul a b, h => by
    simp only [affVal]
    rw [affVal_congr a fun x hx => h x (by simp [Affine.names, hx]),
      affVal_congr b fun x hx => h x (by simp [Affine.names, hx])]

/-- A relation side's value reads only the names the side carries. -/
theorem affVal_side {val1 val2 : String → Int} {a : Affine}
    (h : ∀ x ∈ sideNames a, val1 x = val2 x) : affVal val1 a = affVal val2 a := by
  cases hl : lin a with
  | none =>
    simp only [sideNames, hl] at h
    exact affVal_congr a h
  | some l =>
    simp only [sideNames, hl] at h
    rw [← (lin_sound val1 a l hl).2, ← (lin_sound val2 a l hl).2, ← Lin.eval_clean val1 l,
      ← Lin.eval_clean val2 l]
    unfold Lin.eval
    congr 1
    exact linSum_congr _ fun q hq => h q.1 (List.mem_map_of_mem hq)

/-- A relation holds under one valuation of its names when it holds under
another that agrees on the names it carries. -/
theorem relation_congr {val1 val2 : String → Int} {rq : Relation}
    (h : ∀ x ∈ relNames rq, val1 x = val2 x)
    (h2 : rq.op.eval (affVal val1 rq.lhs) (affVal val1 rq.rhs) = true) :
    rq.op.eval (affVal val2 rq.lhs) (affVal val2 rq.rhs) = true := by
  rw [← affVal_side (a := rq.lhs) fun x hx =>
      h x (by rw [relNames_eq]; exact List.mem_append_left _ hx),
    ← affVal_side (a := rq.rhs) fun x hx =>
      h x (by rw [relNames_eq]; exact List.mem_append_right _ hx)]
  exact h2

theorem foldl_names {s : List (String × Int) → String × Int → List (String × Int)}
    (hs : ∀ acc p, ∀ x ∈ (s acc p).map (·.1), x ∈ acc.map (·.1) ∨ x = p.1) :
    ∀ (l acc : List (String × Int)) (x : String), x ∈ (l.foldl s acc).map (·.1) →
      x ∈ acc.map (·.1) ∨ x ∈ l.map (·.1)
  | [], _, _, h => .inl h
  | p :: rest, acc, x, h => by
    rw [List.foldl_cons] at h
    rcases foldl_names hs rest (s acc p) x h with h1 | h1
    · rcases hs acc p x h1 with h2 | h2
      · exact .inl h2
      · exact .inr (by rw [List.map_cons, h2]; exact List.mem_cons_self)
    · exact .inr (List.mem_cons_of_mem _ h1)

theorem linAdd_names (a b : Lin) :
    ∀ x ∈ (linAdd a b).2.map (·.1), x ∈ a.2.map (·.1) ∨ x ∈ b.2.map (·.1) := by
  intro x hx
  unfold linAdd at hx
  refine foldl_names ?_ b.2 a.2 x hx
  rintro acc ⟨y, k⟩ z hz
  dsimp only at hz
  split at hz
  · rw [names_map ?_ acc] at hz
    · exact .inl hz
    · rintro ⟨y', j⟩
      dsimp only
      split <;> rfl
  · rw [List.map_append, List.mem_append] at hz
    rcases hz with hz | hz
    · exact .inl hz
    · right
      simpa using hz

theorem linScale_names (c : Int) (l : Lin) : (linScale c l).2.map (·.1) = l.2.map (·.1) := by
  unfold linScale
  dsimp only
  rw [names_map ?_ l.2]
  rintro ⟨x, j⟩
  rfl

/-- An affine expression's linear form carries only names it writes. -/
theorem lin_names : ∀ (a : Affine) (l : Lin), lin a = some l →
    ∀ x ∈ l.2.map (·.1), x ∈ Affine.names a
  | .atom (.int _ v), l, h, x, hx => by
    simp only [lin, Option.some.injEq] at h
    subst h
    simp at hx
  | .atom (.var y), l, h, x, hx => by
    simp only [lin, Option.some.injEq] at h
    subst h
    simpa [Affine.names] using hx
  | .atom .unit, l, h, _, _ => by simp [lin] at h
  | .atom (.index _ _), l, h, _, _ => by simp [lin] at h
  | .add a b, l, h, x, hx => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨la, ha, lb, hb, hl⟩ := h
    subst hl
    simp only [Affine.names, List.mem_append]
    rcases linAdd_names la lb x hx with h1 | h1
    · exact .inl (lin_names a la ha x h1)
    · exact .inr (lin_names b lb hb x h1)
  | .sub a b, l, h, x, hx => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq] at h
    obtain ⟨la, ha, lb, hb, hl⟩ := h
    subst hl
    simp only [Affine.names, List.mem_append]
    rcases linAdd_names la (linScale (-1) lb) x hx with h1 | h1
    · exact .inl (lin_names a la ha x h1)
    · rw [linScale_names] at h1
      exact .inr (lin_names b lb hb x h1)
  | .mul a b, l, h, x, hx => by
    simp only [lin, Option.bind_eq_bind, Option.bind_eq_some_iff] at h
    obtain ⟨la, ha, lb, hb, h⟩ := h
    simp only [Affine.names, List.mem_append]
    split at h
    · simp only [Option.some.injEq] at h
      subst h
      rw [linScale_names] at hx
      exact .inr (lin_names b lb hb x hx)
    · split at h
      · simp only [Option.some.injEq] at h
        subst h
        rw [linScale_names] at hx
        exact .inl (lin_names a la ha x hx)
      · simp at h

theorem sideNames_names {a : Affine} {x : String} (hx : x ∈ sideNames a) :
    x ∈ Affine.names a := by
  cases hl : lin a with
  | none =>
    simp only [sideNames, hl] at hx
    exact hx
  | some l =>
    simp only [sideNames, hl] at hx
    obtain ⟨q, hq, rfl⟩ := List.mem_map.mp hx
    exact lin_names a l hl q.1 (List.mem_map_of_mem (List.mem_filter.mp hq).1)

theorem relNames_names {rq : Relation} {x : String} (hx : x ∈ relNames rq) :
    x ∈ Affine.names rq.lhs ∨ x ∈ Affine.names rq.rhs := by
  rw [relNames_eq, List.mem_append] at hx
  rcases hx with hx | hx
  · exact .inl (sideNames_names hx)
  · exact .inr (sideNames_names hx)

/-- An affine expression the static judgment accepts writes only names in
scope. -/
theorem affineOk_names {rule : String} {sc : Static.Scope} :
    ∀ {a : Affine} {u : Unit}, Static.affineOk rule sc a = .ok u →
      ∀ x ∈ Affine.names a, x ∈ sc.map (·.1)
  | .atom (.int _ _), _, _, x, hx => by simp [Affine.names] at hx
  | .atom (.var y), _, h, x, hx => by
    simp only [Affine.names, List.mem_singleton] at hx
    subst hx
    simp only [Static.affineOk] at h
    split at h
    · rename_i z s hf
      exact List.mem_map.mpr ⟨_, List.mem_of_find?_eq_some hf, by simpa using List.find?_some hf⟩
    · exact absurd h Typing.reject_ne
    · exact absurd h Typing.reject_ne
  | .atom .unit, _, _, x, hx => by simp [Affine.names] at hx
  | .atom (.index _ _), _, _, x, hx => by simp [Affine.names] at hx
  | .add a b, _, h, x, hx => by
    obtain ⟨_, ha, hb⟩ := Typing.bind_ok h
    simp only [Affine.names, List.mem_append] at hx
    rcases hx with hx | hx
    · exact affineOk_names ha x hx
    · exact affineOk_names hb x hx
  | .sub a b, _, h, x, hx => by
    obtain ⟨_, ha, hb⟩ := Typing.bind_ok h
    simp only [Affine.names, List.mem_append] at hx
    rcases hx with hx | hx
    · exact affineOk_names ha x hx
    · exact affineOk_names hb x hx
  | .mul a b, _, h, x, hx => by
    obtain ⟨_, ha, h⟩ := Typing.bind_ok h
    obtain ⟨_, hb, -⟩ := Typing.bind_ok h
    simp only [Affine.names, List.mem_append] at hx
    rcases hx with hx | hx
    · exact affineOk_names ha x hx
    · exact affineOk_names hb x hx

theorem fn_mem {p : Program} {name : String} {f : FnDecl} (h : p.fn? name = some f) :
    f ∈ p.fns :=
  List.mem_of_find?_eq_some h

theorem bool_or_of_not_and {a b : Bool} (h : ¬((!a && !b) = true)) : a = true ∨ b = true := by
  cases a <;> cases b <;> simp_all

theorem eq_of_not_bne {a b : String} (h : ¬((a != b) = true)) : a = b := by
  simpa using h

/-- What a return the checker accepted records: each name a postcondition
carries, other than the result, that is a parameter has a stable entry image
or the state is contradictory, and each postcondition's member queries are
discharged [FN-9]. -/
theorem checkReturn_inv {env : Env} {st : State} {ret : Val} {g g' : Gen} {u : Unit}
    (h : (checkReturn env st ret).run g = .ok (u, g')) :
    ∀ rq ∈ env.fn.ensures,
      (∀ n ∈ relNames rq, n ≠ env.fn.result → ∀ y id,
        env.params.find? (·.1 == n) = some (y, id) →
          st.stable.contains id = true ∨ st.contra = true) ∧
      (memberQueries rq (fun x => if x == env.fn.result then ret.term else lookT env x)
        (fun x => if x == env.fn.result then ret.image else lookI env st x)).all
          (discharged env g st) = true := by
  simp only [checkReturn] at h
  obtain ⟨gen, g2, hget, h⟩ := CheckerRun.run_bind h
  obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
  rw [h3, h4] at h
  try dsimp only at h
  split at h
  · exfalso
    first
      | exact CheckerRun.run_fail h
      | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind h; exact CheckerRun.run_fail hf)
  · obtain ⟨u1, g3, hfor, -⟩ := CheckerRun.run_bind h
    exact (CheckerRun.forIn_run (P := fun rq =>
        (∀ n ∈ relNames rq, n ≠ env.fn.result → ∀ y id,
          env.params.find? (·.1 == n) = some (y, id) →
            st.stable.contains id = true ∨ st.contra = true) ∧
        (memberQueries rq (fun x => if x == env.fn.result then ret.term else lookT env x)
          (fun x => if x == env.fn.result then ret.image else lookI env st x)).all
            (discharged env g st) = true)
      (fun rq g0 s g0' hb => by
        try dsimp only at hb
        obtain ⟨u2, g5, hfor2, hb⟩ := CheckerRun.run_bind hb
        obtain ⟨hg5, hn⟩ := CheckerRun.forIn_run (P := fun n => n ≠ env.fn.result → ∀ y id,
            env.params.find? (·.1 == n) = some (y, id) →
              st.stable.contains id = true ∨ st.contra = true)
          (fun n g6 s2 g6' hb2 => by
            try dsimp only at hb2
            split at hb2
            · cases hfind : List.find? (fun x => x.1 == n) env.params with
              | none =>
                rw [hfind] at hb2
                try dsimp only at hb2
                refine ⟨fun _ y id hf => ?_, CheckerRun.run_pure hb2⟩
                (try rw [hfind] at hf)
                cases hf
              | some p =>
                obtain ⟨y0, id0⟩ := p
                rw [hfind] at hb2
                try dsimp only at hb2
                split at hb2
                · exfalso
                  first
                    | exact CheckerRun.run_fail hb2
                    | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hb2
                       exact CheckerRun.run_fail hf)
                · rename_i hst
                  refine ⟨fun _ y id hf => ?_, CheckerRun.run_pure hb2⟩
                  (try rw [hfind] at hf)
                  cases hf
                  exact bool_or_of_not_and hst
            · rename_i hne
              exact ⟨fun h' => absurd (eq_of_not_bne hne) h', CheckerRun.run_pure hb2⟩)
          hfor2
        rw [hg5] at hb
        split at hb
        · exfalso
          first
            | exact CheckerRun.run_fail hb
            | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hb; exact CheckerRun.run_fail hf)
        · rename_i hnd
          exact ⟨⟨hn, CheckerRun.not_not_true hnd⟩, CheckerRun.run_pure hb⟩)
      hfor).2

/-- A postcondition whose member queries a return discharged holds of the
returned integer and the parameters' entry values, when every parameter it
carries still has its entry value [FN-9]. -/
theorem ensures_holds {env : Env} {st : State} {g : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {ret : Val} {t : IntTy} {z : Int} {rq : Relation}
    (hw : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hd : ret.Denotes (valueOf env.vars E) ν G (.int t z))
    (hcur : ∀ x ∈ relNames rq, x ≠ env.fn.result →
      Whitefoot.Env.get E0 x = Whitefoot.Env.get E x)
    (hdis : (memberQueries rq (fun x => if x == env.fn.result then ret.term else lookT env x)
      (fun x => if x == env.fn.result then ret.image else lookI env st x)).all
        (discharged env g st) = true) :
    rq.op.eval
      (affVal (fun x => intOf (Whitefoot.Env.get ((env.fn.result, .int t z) :: E0) x)) rq.lhs)
      (affVal (fun x => intOf (Whitefoot.Env.get ((env.fn.result, .int t z) :: E0) x)) rq.rhs)
      = true := by
  refine relation_congr (val1 := fun x => if x = env.fn.result then z
    else intOf (Whitefoot.Env.get E x)) (fun x hx => ?_) ?_
  · show (if x = env.fn.result then z else intOf (Whitefoot.Env.get E x)) =
      intOf (Whitefoot.Env.get ((env.fn.result, .int t z) :: E0) x)
    rw [Whitefoot.Env.get_cons]
    by_cases hxr : x = env.fn.result
    · rw [ite_eq_left hxr, ite_eq_left hxr.symm]
      rfl
    · rw [ite_eq_right hxr, ite_eq_right (Ne.symm hxr), hcur x hx hxr]
  · refine members_sound hw ?_ ?_ hdis
    · intro x t' k hx
      try dsimp only at hx ⊢
      by_cases hxr : x = env.fn.result
      · rw [ite_eq_left (show (x == env.fn.result) = true by simp [hxr])] at hx
        obtain ⟨t2, e2⟩ := hd.term t' k hx
        injection e2 with _ e2
        rw [ite_eq_left hxr]
        exact e2.symm
      · rw [ite_eq_right (show ¬ (x == env.fn.result) = true by simp [hxr])] at hx
        rw [ite_eq_right hxr]
        exact lookT_sound hw x t' k hx
    · intro x f hx
      try dsimp only at hx ⊢
      by_cases hxr : x = env.fn.result
      · rw [ite_eq_left (show (x == env.fn.result) = true by simp [hxr])] at hx
        obtain ⟨t2, e2⟩ := hd.image f hx
        injection e2 with _ e2
        rw [ite_eq_left hxr]
        exact e2.symm
      · rw [ite_eq_right (show ¬ (x == env.fn.result) = true by simp [hxr])] at hx
        rw [ite_eq_right hxr]
        exact lookI_sound hw x f hx

/-- A return the checker accepted gives a value in its type's range, and the
function's postconditions hold of it and of the parameters' entry values
[FN-9]: the state the return's check reads holds on the frame, and the checker
value it checks describes the returned value. -/
theorem return_ensures {env : Env} {st : State} {g g' : Gen} {E E0 : Whitefoot.Env}
    {G : Nat → Int} {ν : TermVal} {ret : Val} {w : Value}
    (hs : (⟨env.vars, st, g.atoms⟩ : Snapshot).HoldsWith E E0 G ν)
    (hd : ret.Denotes (valueOf env.vars E) ν G w)
    (hwt : w.Typed (Static.normTy env.prog env.fn.resultTy))
    (hp : env.prog.accepts = true) (hfn : env.prog.fn? env.fn.name = some env.fn)
    (hpar : ∀ prm ∈ env.fn.params, ∃ vr, env.var? prm.name = some vr ∧
      env.params.find? (·.1 == prm.name) = some (prm.name, vr.id))
    (hE0 : EnvTyped E0 (Typing.paramScope env.prog env.fn))
    (hret : (checkReturn env st ret).run g = .ok ((), g')) :
    w.InRange ∧ allHold ((env.fn.result, w) :: E0) env.fn.ensures = .ok true := by
  refine ⟨hwt.inRange, ?_⟩
  obtain ⟨u, hck⟩ := Typing.checkFn_of_mem (Typing.accepts_static hp) (fn_mem hfn)
  have hdis := checkReturn_inv hret
  rcases (Typing.checkFn_inv hck).2.1 with hnil | ⟨t, hrt, hR⟩
  · exact allHold_of fun rq hrq => by
      rw [hnil] at hrq
      simp at hrq
  · apply allHold_of
    intro rq hrq
    have hvt : w.Typed (.int t) := by
      rw [← hrt]
      exact hwt
    obtain ⟨z, rfl, -⟩ := hvt.int
    obtain ⟨⟨_, hl⟩, ⟨_, hrr⟩⟩ := hR rq hrq
    rw [holds_val (EnvTyped.cons hvt hE0) hl hrr]
    obtain ⟨hstab, hq⟩ := hdis rq hrq
    refine congrArg Except.ok (ensures_holds hs hd ?_ hq)
    intro x hx hxr
    have hxs : x ∈ ((env.fn.result, Ty.int t) :: Typing.paramScope env.prog env.fn).map (·.1) := by
      rcases relNames_names hx with h | h
      · exact affineOk_names hl x h
      · exact affineOk_names hrr x h
    simp only [List.map_cons, List.mem_cons] at hxs
    rcases hxs with hxs | hxs
    · exact absurd hxs hxr
    · rw [Typing.paramScope, List.map_map, List.mem_map] at hxs
      obtain ⟨prm, hprm, hpn⟩ := hxs
      have hpn' : prm.name = x := hpn
      obtain ⟨vr, hvr, hfind⟩ := hpar prm (List.mem_reverse.mp hprm)
      rw [hpn'] at hvr hfind
      have hnc : st.contra = false := hs.not_contra
      have hcont : st.stable.contains vr.id = true := by
        rcases hstab x hx hxr x vr.id hfind with h | h
        · exact h
        · rw [hnc] at h
          cases h
      have hvr' : env.vars.find? (·.name == x) = some vr := hvr
      have hmem : vr.id ∈ st.stable := List.contains_iff_mem.mp hcont
      have hname : vr.name = x := by simpa using List.find?_some hvr'
      obtain ⟨wx, hgw, -, hval⟩ :=
        Env.Fits.get (vars := env.vars) hs.fits hs.ids hvr'
      have hst : Checker.valueOf env.vars E vr.id = (Whitefoot.Env.get E0 vr.name).toOption :=
        hs.stable vr (List.mem_of_find?_eq_some hvr') hmem
      rw [hval, hname] at hst
      rw [toOption_some hst.symm, hgw]

end Checker

end Whitefoot
