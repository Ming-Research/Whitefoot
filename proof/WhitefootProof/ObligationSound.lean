import WhitefootProof.DischargeSound

/-!
# What the checker proves holds at run time

The obligation lemmas the soundness theorems use, each for a right-hand side
the checker accepted in a snapshot that holds (`Holds.lean`):

- `evalRhs_progress` and `target_progress`: a right-hand side that calls no
  function of the program, and a subscript target, never stick, as every
  subscript is below its length [OP-4] and every exact operation is in its
  domain [OP-2];
- `relHolds_sound`: an invariant the checker proves holds of the frame
  [INV-1];
- `evalRhs_call_some` and `evalRhs_call_none`: the checker publishes a call
  exactly when the right-hand side calls a function of the program;
- `callArgs_progress`, `callArgs_inRange` and `call_requires`: a published
  call's arguments never stick, lie in their types' ranges, and satisfy the
  callee's requirements [FN-8].
-/

namespace Whitefoot

open Fact Aff

namespace Checker

/-! ## Progress: a right-hand side the checker accepted never sticks -/

/-- A bind whose first step and every continuation never stick never sticks. -/
theorem noStuck_bind {α β : Type} {x : Eval α} {f : α → Eval β}
    (hx : ∀ k, x ≠ .error (.stuck k)) (hf : ∀ a, x = .ok a → ∀ k, f a ≠ .error (.stuck k)) :
    ∀ k, (x >>= f) ≠ .error (.stuck k) := by
  intro k hk
  cases hxe : x with
  | error e =>
    rw [hxe, CheckerRun.error_bind] at hk
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
  obtain ⟨v, g1, hv, h⟩ := CheckerRun.run_bind h
  obtain ⟨bs, g2, hbs, h⟩ := CheckerRun.run_bind h
  obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hbs
  obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h
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
    obtain ⟨-, rfl⟩ := CheckerRun.run_pure h
    refine ⟨fun k hk => ?_, G, hg, Ext.refl _ _⟩
    rw [List.mapM_nil] at hk
    cases hk
  | a :: as, g, g1, vs, G, hg, hlit, h => by
    rw [List.mapM_cons] at h
    obtain ⟨v, g2, hv, h⟩ := CheckerRun.run_bind h
    obtain ⟨vs', g3, hvs, h⟩ := CheckerRun.run_bind h
    obtain ⟨-, rfl⟩ := CheckerRun.run_pure h
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
  obtain ⟨va, g2, hva, hr⟩ := CheckerRun.run_bind hr
  obtain ⟨vb, g3, hvb, hr⟩ := CheckerRun.run_bind hr
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
    obtain ⟨gen, g4, hget, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact CheckerRun.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hr; exact CheckerRun.run_fail hf)
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
    obtain ⟨gen, g4, hget, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact CheckerRun.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hr; exact CheckerRun.run_fail hf)
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
  obtain ⟨vs, g2, hvs, hr⟩ := CheckerRun.run_bind hr
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
    obtain ⟨gen, g4, hget, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
    rw [h3, h4] at hr
    try dsimp only at hr
    split at hr
    · exfalso
      first
        | exact CheckerRun.run_fail hr
        | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hr; exact CheckerRun.run_fail hf)
    · rename_i hnd
      have hdis := CheckerRun.not_not_true hnd
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
        obtain ⟨vs, g2, hvs, -⟩ := CheckerRun.run_bind hr
        rw [List.mapM_cons, List.mapM_nil] at hvs
        obtain ⟨v, g3, hv, -⟩ := CheckerRun.run_bind hvs
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
      obtain ⟨vs, g2, hvs, -⟩ := CheckerRun.run_bind hr
      rw [List.mapM_cons, List.mapM_nil] at hvs
      obtain ⟨v, g3, hv, -⟩ := CheckerRun.run_bind hvs
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
    obtain ⟨v, g2, hv, -⟩ := CheckerRun.run_bind hr
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
        have hm := List.mem_map_of_mem (f := fun p : String × Value => p.1) he
        exact hnd.1 hm
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
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h
    refine ⟨fun p hp => by simp at hp, G, hg, Ext.refl _ _, fun args ha => ?_⟩
    cases ha
    exact .nil
  | (n, a) :: as, g, g1, vs, G, hg, hlit, h, hf => by
    rw [List.mapM_cons, hf] at h
    obtain ⟨p, g2, hp, h⟩ := CheckerRun.run_bind h
    obtain ⟨v, g3, hv, hp⟩ := CheckerRun.run_bind hp
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure hp
    obtain ⟨vs', g4, hvs, h⟩ := CheckerRun.run_bind h
    obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h
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
local syntax "walk_pure " ident : tactic
local macro_rules
  | `(tactic| walk_pure $h:ident) => `(tactic|
      repeat' first
        | exact (CheckerRun.run_fail $h).elim
        | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind $h; exact (CheckerRun.run_fail hf).elim)
        | (obtain ⟨hpe, -⟩ := CheckerRun.run_pure $h; subst hpe; rfl)
        | (obtain ⟨_, _, -, $h:ident⟩ := CheckerRun.run_bind $h)
        | (split at $h:ident)
        | (dsimp only at $h:ident))

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
      | arrayFilled =>
        -- The arm also inspects the type arguments, so the match reduces only
        -- once they are constructors; a split would keep the `fn` arm.
        rcases targs with _ | ⟨(et | _ | _ | ⟨_, _⟩ | _) | _, _ | ⟨_ | n, _ | ⟨_, _⟩⟩⟩ <;>
          dsimp only at hr <;> walk_pure hr
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
  obtain ⟨args, g2, -, hr⟩ := CheckerRun.run_bind hr
  obtain ⟨gen, g3, -, hr⟩ := CheckerRun.run_bind hr
  obtain ⟨u, g4, -, hr⟩ := CheckerRun.run_bind hr
  split at hr
  all_goals
    obtain ⟨img, g5, -, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure hr
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
    obtain ⟨args, g2, hargs, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨gen, g3, hget, hr⟩ := CheckerRun.run_bind hr
    obtain ⟨h3, h4⟩ := CheckerRun.run_get hget
    rw [h3, h4] at hr
    obtain ⟨u, g4, hfor, hr⟩ := CheckerRun.run_bind hr
    have hall := (CheckerRun.forIn_run (P := fun rq => discharged env g2 st (relQuery
        (relG (fun x => (args.find? (·.1 == x)).bind (·.2.gexpr)) rq) rq
        (fun x => (args.find? (·.1 == x)).bind (·.2.term))
        (fun x => (args.find? (·.1 == x)).bind (·.2.image))) = true)
      (fun rq g' s g'' hb => by
        try dsimp only at hb
        split at hb
        · exfalso
          first
            | exact CheckerRun.run_fail hb
            | (obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hb; exact CheckerRun.run_fail hf)
        · rename_i hnd
          refine ⟨CheckerRun.not_not_true hnd, ?_⟩
          first
            | exact CheckerRun.run_pure hb
            | (obtain ⟨_, _, h0, hb⟩ := CheckerRun.run_bind hb
               obtain ⟨rfl, rfl⟩ := CheckerRun.run_pure h0
               exact CheckerRun.run_pure hb))
      hfor).2
    have hcall : r.call = some (f', args) := by
      split at hr
      all_goals
        obtain ⟨img, g5, -, hr⟩ := CheckerRun.run_bind hr
        obtain ⟨rfl, -⟩ := CheckerRun.run_pure hr
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
    ok_get (P := fun a => EnvTyped a (f.params.map fun prm =>
      (prm.name, Static.normTy env.prog prm.ty))) (a := args)
      (Typing.args_ok (p := env.prog) hE hn hat) hargs
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

end Checker

end Whitefoot
