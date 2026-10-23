import WhitefootProof.Safety

/-!
# The simulation of a function's run by its walk

The interpreter runs an accepted function's statements in the order the
checker's walk visited them. A statement's triple says that from a frame where
the snapshot the walk recorded on entering it holds, its step is safe for as
many steps as what it leaves to is safe. What the facts mean, and the lemmas
about each step of the walk, come in as premises (`Sim.Laws`), each a lemma
another item of the proof proves.
-/

namespace Whitefoot

namespace Sim

/-! ## The generator only grows -/

/-- The counter does not go back, and the atoms minted are numbered between
the two counters with nonempty intervals. -/
def Mints (g g' : Checker.Gen) : Prop :=
  g.next ≤ g'.next ∧
  ∃ new : List (Nat × Int × Int), g'.atoms = new ++ g.atoms ∧
    ∀ a ∈ new, g.next ≤ a.1 ∧ a.1 < g'.next ∧ a.2.1 ≤ a.2.2

/-- Minting, with the snapshot table unchanged. -/
def Grows (g g' : Checker.Gen) : Prop :=
  Mints g g' ∧ g'.points = g.points

theorem Mints.refl (g : Checker.Gen) : Mints g g :=
  ⟨Nat.le_refl _, [], rfl, fun _ ha => by cases ha⟩

theorem Mints.trans {g1 g2 g3 : Checker.Gen} (h1 : Mints g1 g2) (h2 : Mints g2 g3) :
    Mints g1 g3 := by
  obtain ⟨hn1, new1, ha1, hb1⟩ := h1
  obtain ⟨hn2, new2, ha2, hb2⟩ := h2
  refine ⟨Nat.le_trans hn1 hn2, new2 ++ new1, ?_, ?_⟩
  · rw [ha2, ha1, List.append_assoc]
  · intro a ha
    rcases List.mem_append.mp ha with h | h
    · obtain ⟨b1, b2, b3⟩ := hb2 a h
      exact ⟨Nat.le_trans hn1 b1, b2, b3⟩
    · obtain ⟨b1, b2, b3⟩ := hb1 a h
      exact ⟨b1, Nat.lt_of_lt_of_le b2 hn2, b3⟩

/-! ## What the simulation takes as premises -/

/-- The values of a call's arguments, under their names. -/
def callArgs (E : Env) (as : List (String × Atom)) : Eval (List (String × Value)) :=
  as.mapM fun (n, a) => do return (n, ← evalAtom E a)

/-- The lemmas the simulation rests on. The first group is about the meaning
`F` gives the facts; the discharge group says a proved obligation holds at run
time; the growth group says the walk only mints; the transfer group says each
statement's new snapshot holds of the frame the step leaves. -/
structure Laws (F : Model) : Prop where
  names : ∀ {s : Snapshot} {E E0 : Env}, F.Holds s (E, E0) → E.map (·.1) = s.vars.map (·.name)
  not_contra : ∀ {s : Snapshot} {fr : Env × Env}, F.Holds s fr → s.state.contra = false
  atoms_mono : ∀ {s : Snapshot} {fr : Env × Env} {n : Nat} {new : List (Nat × Int × Int)},
    F.Holds s fr → F.Fresh s n → (∀ a ∈ new, n ≤ a.1 ∧ a.2.1 ≤ a.2.2) →
    F.Holds { s with atoms := new ++ s.atoms } fr
  fresh_mono : ∀ {s : Snapshot} {n n' : Nat} {new : List (Nat × Int × Int)},
    F.Fresh s n → n ≤ n' → (∀ a ∈ new, a.1 < n') → F.Fresh { s with atoms := new ++ s.atoms } n'
  fresh_vars : ∀ {vars vars' : List Checker.Var} {st : Checker.State}
    {A : List (Nat × Int × Int)} {n : Nat},
    F.Fresh ⟨vars, st, A⟩ n → (∀ v ∈ vars', v.id < n) → F.Fresh ⟨vars', st, A⟩ n
  fresh_ids : ∀ {vars : List Checker.Var} {st : Checker.State} {A : List (Nat × Int × Int)}
    {n : Nat}, F.Fresh ⟨vars, st, A⟩ n → ∀ v ∈ vars, v.id < n
  evalRhs_progress : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → r.call = none →
    ∀ k, evalExpr env.prog E e ≠ .error (.stuck k)
  evalRhs_call_some : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl},
    (Checker.evalRhs env st e).run g = .ok (r, g1) → e = .call path targs as →
    env.prog.callee? path = some (.fn f) → ∃ vs, r.call = some (f, vs) ∧ r.val.term = none
  evalRhs_call_none : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs},
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∀ path targs as f, e = .call path targs as → env.prog.callee? path ≠ some (.fn f)) →
    r.call = none
  callArgs_progress : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl} {vs : List (String × Checker.Val)},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → e = .call path targs as →
    r.call = some (f, vs) → ∀ k, callArgs E as ≠ .error (.stuck k)
  callArgs_inRange : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl} {vs : List (String × Checker.Val)}
    {args : List (String × Value)},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → e = .call path targs as →
    r.call = some (f, vs) → callArgs E as = .ok args → ∀ a ∈ args, F.InRange a.2
  call_requires : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl} {vs : List (String × Checker.Val)}
    {args : List (String × Value)},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → env.prog.accepts = true →
    e = .call path targs as → r.call = some (f, vs) → callArgs E as = .ok args →
    args.map (·.1) = f.params.map (·.name) → allHold args.reverse f.requires = .ok true
  target_progress : ∀ {env : Checker.Env} {st : Checker.State} {g g1 : Checker.Gen}
    {b : String} {off : Atom} {val : Checker.Val} {E E0 : Env},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) →
    (Checker.evalAtom env st (.index b off)).run g = .ok (val, g1) →
    (∃ sc t, Static.atomTy sc (.index b off) = .ok t) →
    ∀ k, evalAtom E (.index b off) ≠ .error (.stuck k)
  return_ensures : ∀ {env : Checker.Env} {st : Checker.State} {g g1 g2 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env} {v : Value},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → env.prog.accepts = true →
    env.prog.fn? env.fn.name = some env.fn → r.call = none →
    (Checker.checkReturn env st r.val).run g1 = .ok ((), g2) → evalExpr env.prog E e = .ok v →
    F.InRange v ∧ allHold ((env.fn.result, v) :: E0) env.fn.ensures = .ok true
  relHolds_sound : ∀ {env : Checker.Env} {st : Checker.State} {g : Checker.Gen}
    {rel : Relation} {E E0 : Env},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (∃ sc, Static.invariantOk sc rel = .ok ()) → Checker.relHolds env g st rel = true →
    Relation.holds E rel = .ok true
  evalRhs_grows : ∀ {env : Checker.Env} {st : Checker.State} {g g' : Checker.Gen} {e : Expr}
    {r : Checker.Rhs}, (Checker.evalRhs env st e).run g = .ok (r, g') → Grows g g'
  let_sound : ∀ {env : Checker.Env} {st st' : Checker.State} {g g1 g2 : Checker.Gen}
    {x : String} {e : Expr} {r : Checker.Rhs} {E E0 : Env} {val : Value},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → r.call = none →
    (Checker.bindLet env st { name := x, id := g1.next, ty := r.val.ty } r).run
      { g1 with next := g1.next + 1 } = .ok (st', g2) →
    evalExpr env.prog E e = .ok val →
    F.Holds ⟨{ name := x, id := g1.next, ty := r.val.ty } :: env.vars, st', g2.atoms⟩
      ((x, val) :: E, E0)
  let_call_sound : ∀ {env : Checker.Env} {st st' : Checker.State} {g g1 g2 : Checker.Gen}
    {x : String} {e : Expr} {r : Checker.Rhs} {E E0 : Env} {path : List String}
    {targs : List TArg} {as : List (String × Atom)} {f : FnDecl}
    {vs : List (String × Checker.Val)} {args : List (String × Value)} {w : Value},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → env.prog.accepts = true →
    e = .call path targs as → r.call = some (f, vs) →
    (Checker.bindLet env st { name := x, id := g1.next, ty := r.val.ty } r).run
      { g1 with next := g1.next + 1 } = .ok (st', g2) →
    callArgs E as = .ok args → F.InRange w → w.hasTy env.prog f.resultTy = true →
    allHold ((f.result, w) :: args.reverse) f.ensures = .ok true →
    F.Holds ⟨{ name := x, id := g1.next, ty := r.val.ty } :: env.vars, st', g2.atoms⟩
      ((x, w) :: E, E0)
  setVar_sound : ∀ {env : Checker.Env} {st st' : Checker.State} {g g1 g2 : Checker.Gen}
    {x : String} {v : Checker.Var} {e : Expr} {r : Checker.Rhs} {E E0 : Env} {val : Value}
    {fr fr' : Frame},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    env.var? x = some v → (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → r.call = none →
    (Checker.setVar env st v r).run g1 = .ok (st', g2) → evalExpr env.prog E e = .ok val →
    commitTo fr (.var x) val = .ok fr' → fr.env = E →
    F.Holds ⟨env.vars, st', g2.atoms⟩ (fr'.env, E0)
  setVar_call_sound : ∀ {env : Checker.Env} {st st' : Checker.State} {g g1 g2 : Checker.Gen}
    {x : String} {v : Checker.Var} {e : Expr} {r : Checker.Rhs} {E E0 : Env}
    {path : List String} {targs : List TArg} {as : List (String × Atom)} {f : FnDecl}
    {vs : List (String × Checker.Val)} {args : List (String × Value)} {w : Value}
    {fr fr' : Frame},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    env.var? x = some v → (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → env.prog.accepts = true →
    e = .call path targs as → r.call = some (f, vs) →
    (Checker.setVar env st v r).run g1 = .ok (st', g2) →
    callArgs E as = .ok args → F.InRange w → w.hasTy env.prog f.resultTy = true →
    allHold ((f.result, w) :: args.reverse) f.ensures = .ok true →
    commitTo fr (.var x) w = .ok fr' → fr.env = E →
    F.Holds ⟨env.vars, st', g2.atoms⟩ (fr'.env, E0)
  setIndex_sound : ∀ {env : Checker.Env} {st : Checker.State} {g g1 g2 : Checker.Gen}
    {b : String} {off : Atom} {v : Checker.Var} {e : Expr} {r : Checker.Rhs} {E E0 : Env}
    {val : Value} {vt : Checker.Val} {fr fr' : Frame},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    env.var? b = some v → (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) →
    (∃ sc t, Static.atomTy sc (.index b off) = .ok t) → r.call = none →
    (Checker.evalAtom env st (.index b off)).run g1 = .ok (vt, g2) →
    evalExpr env.prog E e = .ok val → commitTo fr (.index b off) val = .ok fr' → fr.env = E →
    F.Holds ⟨env.vars, st.write v.id, g2.atoms⟩ (fr'.env, E0)
  setIndex_call_sound : ∀ {env : Checker.Env} {st st1 : Checker.State}
    {g g1 g2 g3 : Checker.Gen} {b : String} {off : Atom} {v : Checker.Var} {e : Expr}
    {r : Checker.Rhs} {E E0 : Env} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl} {vs : List (String × Checker.Val)}
    {args : List (String × Value)} {w : Value} {vt : Checker.Val} {fr fr' : Frame},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    env.var? b = some v → (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) →
    (∃ sc t, Static.atomTy sc (.index b off) = .ok t) → env.prog.accepts = true →
    e = .call path targs as → r.call = some (f, vs) →
    (Checker.publish env st f vs none none id).run g1 = .ok (st1, g2) →
    (Checker.evalAtom env st1 (.index b off)).run g2 = .ok (vt, g3) →
    callArgs E as = .ok args → F.InRange w → w.hasTy env.prog f.resultTy = true →
    allHold ((f.result, w) :: args.reverse) f.ensures = .ok true →
    commitTo fr (.index b off) w = .ok fr' → fr.env = E →
    F.Holds ⟨env.vars, st1.write v.id, g3.atoms⟩ (fr'.env, E0)
  publish_sound : ∀ {env : Checker.Env} {st st1 : Checker.State} {g g1 g2 : Checker.Gen}
    {e : Expr} {r : Checker.Rhs} {E E0 : Env} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {f : FnDecl} {vs : List (String × Checker.Val)}
    {args : List (String × Value)} {w : Value},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → env.prog.accepts = true →
    e = .call path targs as → r.call = some (f, vs) →
    (Checker.publish env st f vs none none id).run g1 = .ok (st1, g2) →
    callArgs E as = .ok args → F.InRange w → w.hasTy env.prog f.resultTy = true →
    allHold ((f.result, w) :: args.reverse) f.ensures = .ok true →
    F.Holds ⟨env.vars, st1, g2.atoms⟩ (E, E0)
  establishRel_sound : ∀ {env : Checker.Env} {st : Checker.State} {g : Checker.Gen}
    {rel : Relation} {E E0 : Env},
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (∃ sc, Static.invariantOk sc rel = .ok ()) → Relation.holds E rel = .ok true →
    F.Holds ⟨env.vars, Checker.establishRel env st rel, g.atoms⟩ (E, E0)

/-- A snapshot that holds still holds once more atoms are minted. -/
theorem Laws.lift {F : Model} (L : Laws F) {vars : List Checker.Var} {st : Checker.State}
    {g g' : Checker.Gen} {fr : Env × Env} (hH : F.Holds ⟨vars, st, g.atoms⟩ fr)
    (hF : F.Fresh ⟨vars, st, g.atoms⟩ g.next) (hm : Mints g g') :
    F.Holds ⟨vars, st, g'.atoms⟩ fr := by
  obtain ⟨_, new, ha, hb⟩ := hm
  have h := L.atoms_mono (new := new) hH hF fun a ha' => ⟨(hb a ha').1, (hb a ha').2.2⟩
  rw [ha]
  exact h

/-! ## Results of the interpreter's evaluations -/

theorem eval_bind_ok {α β : Type} {x : Eval α} {f : α → Eval β} {b : β}
    (h : (x >>= f) = .ok b) : ∃ a, x = .ok a ∧ f a = .ok b := by
  cases x with
  | error e => cases h
  | ok a => exact ⟨a, rfl, h⟩

/-- An evaluation that does not stop stuck. -/
def NoStuck {α : Type} (x : Eval α) : Prop :=
  ∀ s, x ≠ .error (.stuck s)

theorem NoStuck.bind {α β : Type} {x : Eval α} {f : α → Eval β} (hx : NoStuck x)
    (hf : ∀ a, x = .ok a → NoStuck (f a)) : NoStuck (x >>= f) := by
  intro s h
  cases hx' : x with
  | ok a =>
    rw [hx'] at h
    exact hf a hx' s h
  | error e =>
    rw [hx'] at h
    cases e with
    | stuck s' => exact hx s' hx'
    | illFormed w => cases h

theorem NoStuck.ok {α : Type} (a : α) : NoStuck (.ok a : Eval α) :=
  fun _ h => by cases h

theorem NoStuck.ill {α : Type} (w : String) : NoStuck (ill w : Eval α) :=
  fun _ h => by cases h

theorem stepOK_ofEval {P : Config → Prop} {α : Type} {e : Eval α} {k : α → Step}
    (hs : NoStuck e) (hk : ∀ a, e = .ok a → StepOK P (k a)) : StepOK P (Step.ofEval e k) := by
  cases he : e with
  | ok a => exact hk a he
  | error h =>
    cases h with
    | stuck s => exact absurd he (hs s)
    | illFormed w => exact True.intro

theorem get_noStuck (E : Env) (x : String) : NoStuck (E.get x) := by
  intro s h
  unfold Env.get at h
  split at h <;> cases h

theorem put_noStuck (x : String) (v : Value) : ∀ E : Env, NoStuck (E.put x v)
  | [] => fun _ h => by cases h
  | (y, w) :: rest => by
    intro s h
    rw [Env.put] at h
    split at h
    · cases h
    · cases hr : Env.put rest x v with
      | ok r =>
        rw [hr] at h
        cases h
      | error e =>
        cases e with
        | stuck s' => exact put_noStuck x v rest s' hr
        | illFormed w' =>
          rw [hr] at h
          cases h

theorem put_length {x : String} {v : Value} :
    ∀ {E E' : Env}, E.put x v = .ok E' → E'.length = E.length
  | [], _, h => by cases h
  | (y, w) :: rest, E', h => by
    rw [Env.put] at h
    split at h
    · cases h
      rfl
    · obtain ⟨R, hR, h2⟩ := eval_bind_ok h
      cases h2
      exact congrArg (· + 1) (put_length hR)

/-- A write into a frame `top ++ R` writes into `top`, or past it into `R`. -/
theorem put_append {x : String} {v : Value} {R : Env} :
    ∀ {top E' : Env}, (top ++ R).put x v = .ok E' →
      (∃ top', top.put x v = .ok top' ∧ E' = top' ++ R) ∨
        ∃ R', R.put x v = .ok R' ∧ E' = top ++ R'
  | [], E', h => Or.inr ⟨E', h, rfl⟩
  | (y, u) :: top, E', h => by
    have h0 : Env.put ((y, u) :: (top ++ R)) x v = .ok E' := h
    rw [Env.put] at h0
    split at h0
    · rename_i hyx
      cases h0
      refine Or.inl ⟨(x, v) :: top, ?_, rfl⟩
      rw [Env.put, ite_eq_left hyx]
    · rename_i hyx
      obtain ⟨r, hr, h2⟩ := eval_bind_ok h0
      cases h2
      rcases put_append hr with ⟨top', ht, rfl⟩ | ⟨R', hR, rfl⟩
      · refine Or.inl ⟨(y, u) :: top', ?_, rfl⟩
        rw [Env.put, ite_eq_right hyx, ht]
        all_goals rfl
      · exact Or.inr ⟨R', hR, rfl⟩

theorem write_noStuck (fr : Frame) (x : String) (off : Option Nat) (v : Value) :
    NoStuck (write fr x off v) := by
  unfold write
  refine NoStuck.bind (get_noStuck _ _) fun old _ => ?_
  refine NoStuck.bind ?_ fun new _ => NoStuck.bind (put_noStuck _ _ _) fun _ _ => NoStuck.ok _
  intro s h
  repeat' split at h
  all_goals cases h

theorem write_shape {fr fr' : Frame} {x : String} {off : Option Nat} {v : Value}
    (h : write fr x off v = .ok fr') :
    fr' = { fr with env := fr'.env } ∧ ∃ new, fr.env.put x new = .ok fr'.env := by
  unfold write at h
  obtain ⟨_, _, h1⟩ := eval_bind_ok h
  obtain ⟨new, _, h2⟩ := eval_bind_ok h1
  obtain ⟨E', hput, h3⟩ := eval_bind_ok h2
  cases h3
  exact ⟨rfl, new, hput⟩

theorem commitTo_var_noStuck (fr : Frame) (x : String) (v : Value) :
    NoStuck (commitTo fr (.var x) v) := by
  rw [commitTo]
  exact NoStuck.bind (get_noStuck _ _) fun _ _ => write_noStuck _ _ _ _

theorem evalAtom_index_off {E : Env} {x : String} {off : Atom} {a : Value}
    (h : evalAtom E (.index x off) = .ok a) : ∃ i, evalAtom E off = .ok i := by
  rw [evalAtom] at h
  obtain ⟨_, _, h1⟩ := eval_bind_ok h
  obtain ⟨i, hi, _⟩ := eval_bind_ok h1
  exact ⟨i, hi⟩

theorem commitTo_index_noStuck {fr : Frame} {x : String} {off : Atom} {v : Value}
    (ht : NoStuck (evalAtom fr.env (.index x off))) :
    NoStuck (commitTo fr (.index x off) v) := by
  rw [commitTo]
  refine NoStuck.bind ht fun a ha => ?_
  obtain ⟨i, hi⟩ := evalAtom_index_off ha
  refine NoStuck.bind (fun s h => by rw [hi] at h; cases h) fun i' _ => ?_
  cases i' with
  | int t' j => exact write_noStuck _ _ _ _
  | _ => exact NoStuck.ill _

/-- A commit changes only the frame's bindings, by writing the target's
binding. -/
theorem commitTo_var_shape {fr fr' : Frame} {x : String} {v : Value}
    (h : commitTo fr (.var x) v = .ok fr') :
    fr' = { fr with env := fr'.env } ∧ ∃ new, fr.env.put x new = .ok fr'.env := by
  rw [commitTo] at h
  obtain ⟨_, _, h1⟩ := eval_bind_ok h
  exact write_shape h1

theorem commitTo_index_shape {fr fr' : Frame} {b : String} {off : Atom} {v : Value}
    (h : commitTo fr (.index b off) v = .ok fr') :
    fr' = { fr with env := fr'.env } ∧ ∃ new, fr.env.put b new = .ok fr'.env := by
  rw [commitTo] at h
  obtain ⟨_, _, h1⟩ := eval_bind_ok h
  obtain ⟨i, _, h2⟩ := eval_bind_ok h1
  cases i with
  | int t' j => exact write_shape h2
  | _ => cases h2

/-- A callee path that resolves to a function of the program names it. -/
theorem callee_fn {p : Program} {path : List String} {g : FnDecl}
    (h : p.callee? path = some (.fn g)) : p.fn? g.name = some g := by
  unfold Program.callee? at h
  split at h
  · split at h
    · rename_i f hf
      cases h
      rw [beq_iff_eq.mp (List.find?_some hf)]
      exact hf
    · split at h <;> cases h
  · cases h
  · cases h

/-! ## Returning to a caller

A callee's return checks its result type, its postconditions, and sends the
value where the call put it. -/

theorem stepOK_doReturn_bind {P : Config → Prop} {p : Program} {v : Value} {f c : Frame}
    {callers : List Frame} {x : String} (hd : f.dest = .bind x)
    (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true)
    (h : v.hasTy p f.fn.resultTy = true →
      P { frames := { c with env := (x, v) :: c.env } :: callers }) :
    StepOK P (doReturn p v (f :: c :: callers)) := by
  cases hty : v.hasTy p f.fn.resultTy with
  | false =>
    rw [doReturn, hty]
    exact True.intro
  | true =>
    rw [doReturn, hty, hens, hd]
    exact h hty

theorem stepOK_doReturn_assign {P : Config → Prop} {p : Program} {v : Value} {f c : Frame}
    {callers : List Frame} {t : Target} (hd : f.dest = .assign t)
    (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true)
    (h : v.hasTy p f.fn.resultTy = true →
      StepOK P (Step.ofEval (commitTo c t v) fun g => .next { frames := g :: callers })) :
    StepOK P (doReturn p v (f :: c :: callers)) := by
  cases hty : v.hasTy p f.fn.resultTy with
  | false =>
    rw [doReturn, hty]
    exact True.intro
  | true =>
    rw [doReturn, hty, hens, hd]
    exact h hty

theorem stepOK_doReturn_discard {P : Config → Prop} {p : Program} {v : Value} {f c : Frame}
    {callers : List Frame} (hd : f.dest = .discard)
    (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true)
    (h : v.hasTy p f.fn.resultTy = true → P { frames := c :: callers }) :
    StepOK P (doReturn p v (f :: c :: callers)) := by
  cases hty : v.hasTy p f.fn.resultTy with
  | false =>
    rw [doReturn, hty]
    exact True.intro
  | true =>
    rw [doReturn, hty, hens, hd]
    exact h hty

theorem stepOK_doReturn_ret {P : Config → Prop} {p : Program} {v : Value} {f c : Frame}
    {callers : List Frame} (hd : f.dest = .ret)
    (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true)
    (h : v.hasTy p f.fn.resultTy = true → StepOK P (doReturn p v (c :: callers))) :
    StepOK P (doReturn p v (f :: c :: callers)) := by
  cases hty : v.hasTy p f.fn.resultTy with
  | false =>
    rw [doReturn, hty]
    exact True.intro
  | true =>
    rw [doReturn, hty, hens, hd]
    exact h hty

/-! ## A statement's step, by its kind -/

theorem stepStmt_let (p : Program) (f : Frame) (callers : List Frame) (pos : Pos) (x : String)
    (e : Expr) (k : List Kont) :
    stepStmt p f callers pos (.let_ x e) k =
      evalRhs p f callers e (.bind x) k fun v =>
        .next { frames := { f with env := (x, v) :: f.env, kont := k } :: callers } := rfl

theorem stepStmt_setTo (p : Program) (f : Frame) (callers : List Frame) (pos : Pos)
    (t : Target) (e : Expr) (k : List Kont) :
    stepStmt p f callers pos (.set t e) k =
      evalRhs p f callers e (.assign t) k fun v =>
        Step.ofEval (commitTo { f with kont := k } t v) fun g => .next { frames := g :: callers } :=
  rfl

theorem stepStmt_call (p : Program) (f : Frame) (callers : List Frame) (pos : Pos) (e : Expr)
    (k : List Kont) :
    stepStmt p f callers pos (.call e) k =
      evalRhs p f callers e .discard k fun _ =>
        .next { frames := { f with kont := k } :: callers } := rfl

theorem stepStmt_ret (p : Program) (f : Frame) (callers : List Frame) (pos : Pos) (e : Expr)
    (k : List Kont) :
    stepStmt p f callers pos (.ret e) k =
      evalRhs p f callers e .ret k fun v => doReturn p v (f :: callers) := rfl

theorem stepStmt_invariant (p : Program) (f : Frame) (callers : List Frame) (pos : Pos)
    (nm : String) (rel : Relation) (k : List Kont) :
    stepStmt p f callers pos (.invariant nm rel) k =
      .next { frames := { f with kont := k } :: callers } := rfl

/-- The interpreter's right-hand side enters a function of the program, or
evaluates the expression. -/
theorem stepOK_evalRhs {P : Config → Prop} {p : Program} {f : Frame} {callers : List Frame}
    {e : Expr} {dest : Dest} {k : List Kont} {fin : Value → Step}
    (hcall : ∀ path targs as g, e = .call path targs as → p.callee? path = some (.fn g) →
      NoStuck (callArgs f.env as) ∧ ∀ args, callArgs f.env as = .ok args →
        StepOK P (enter p g args dest ({ f with kont := k } :: callers)))
    (hval : (∀ path targs as g, e = .call path targs as → p.callee? path ≠ some (.fn g)) →
      NoStuck (evalExpr p f.env e) ∧ ∀ v, evalExpr p f.env e = .ok v → StepOK P (fin v)) :
    StepOK P (evalRhs p f callers e dest k fin) := by
  have hv : (∀ path targs as g, e = .call path targs as → p.callee? path ≠ some (.fn g)) →
      evalRhs p f callers e dest k fin = Step.ofEval (evalExpr p f.env e) fin →
      StepOK P (evalRhs p f callers e dest k fin) := by
    intro hn heq
    rw [heq]
    exact stepOK_ofEval (hval hn).1 (hval hn).2
  cases e with
  | call path targs as =>
    cases hc : p.callee? path with
    | none =>
      refine hv (fun path' targs' as' g he => ?_) (by rw [evalRhs, hc] <;> rfl)
      cases he
      rw [hc]
      intro h
      cases h
    | some c =>
      cases c with
      | fn g =>
        obtain ⟨hs, hk⟩ := hcall path targs as g rfl hc
        rw [evalRhs, hc]
        exact stepOK_ofEval hs hk
      | exitStatus =>
        refine hv (fun path' targs' as' g he => ?_) (by rw [evalRhs, hc] <;> rfl)
        cases he
        rw [hc]
        intro h
        cases h
      | arrayFilled =>
        refine hv (fun path' targs' as' g he => ?_) (by rw [evalRhs, hc] <;> rfl)
        cases he
        rw [hc]
        intro h
        cases h
  | atom a => exact hv (fun _ _ _ _ he => by cases he) rfl
  | «infix» l o r => exact hv (fun _ _ _ _ he => by cases he) rfl
  | op o as => exact hv (fun _ _ _ _ he => by cases he) rfl
  | bool b => exact hv (fun _ _ _ _ he => by cases he) rfl

/-! ## The checker's side of a statement -/

theorem published_none {env : Checker.Env} {st st1 : Checker.State} {r : Checker.Rhs}
    {g g1 : Checker.Gen} (h : Checker.Published env st r g st1 g1) (hr : r.call = none) :
    st1 = st ∧ g1 = g := by
  rcases h with ⟨_, h1, h2⟩ | ⟨f, args, hs, _⟩
  · exact ⟨h1, h2⟩
  · rw [hr] at hs
    cases hs

theorem published_call {env : Checker.Env} {st st1 : Checker.State} {r : Checker.Rhs}
    {g g1 : Checker.Gen} {f : FnDecl} {args : List (String × Checker.Val)}
    (h : Checker.Published env st r g st1 g1) (hr : r.call = some (f, args)) :
    (Checker.publish env st f args none none id).run g = .ok (st1, g1) := by
  rcases h with ⟨hn, _, _⟩ | ⟨f', args', hs, hp⟩
  · rw [hr] at hn
    cases hn
  · rw [hr] at hs
    cases hs
    exact hp

/-- A return of a call's value, which no postcondition reads, is checked only
in a function without postconditions. -/
theorem checkReturn_ensures_nil {env : Checker.Env} {st : Checker.State} {ret : Checker.Val}
    {g g' : Checker.Gen} (h : (Checker.checkReturn env st ret).run g = .ok ((), g'))
    (hn : ret.term = none) : env.fn.ensures = [] := by
  rw [Checker.checkReturn] at h
  obtain ⟨_, _, _, h1⟩ := Checker.run_bind_ok h
  obtain ⟨hc, _⟩ := Checker.run_ite_fail_ok h1
  cases he : env.fn.ensures with
  | nil => rfl
  | cons r rs =>
    rw [he, hn] at hc
    exact (hc rfl).elim

/-! ## Frames that agree with a loop's entry frame -/

theorem Agrees.length {killed : List String} :
    ∀ {vars : List Checker.Var} {seen : List String} {R E1 : Env},
      Agrees killed seen vars R E1 → R.length = E1.length
  | [], _, _, _, h => by
    obtain ⟨rfl, rfl⟩ := h
    rfl
  | _ :: _, _, _, _, h => by
    obtain ⟨_, _, _, _, rfl, rfl, _, h'⟩ := h
    exact congrArg (· + 1) (Agrees.length h')

/-- The entry frame agrees with itself when it names its bindings as the walk
does. -/
theorem Agrees.self {killed : List String} :
    ∀ {vars : List Checker.Var} {seen : List String} {E1 : Env},
      E1.map (·.1) = vars.map (·.name) → Agrees killed seen vars E1 E1
  | [], _, [], _ => ⟨rfl, rfl⟩
  | [], _, _ :: _, h => by cases h
  | _ :: _, _, [], h => by cases h
  | v :: _, _, (y, b) :: E1, h => by
    have h' : y = v.name ∧ E1.map (·.1) = _ := List.cons.inj h
    obtain ⟨rfl, h2⟩ := h'
    exact ⟨b, b, E1, E1, rfl, rfl, Or.inl rfl, Agrees.self h2⟩

/-- Writing the newest binding of a killed name no binding above shadows keeps
the bindings in line. -/
theorem Agrees.put {killed : List String} {x : String} {v : Value} (hx : x ∈ killed) :
    ∀ {vars : List Checker.Var} {seen : List String} {R R' E1 : Env},
      Agrees killed seen vars R E1 → x ∉ seen → R.put x v = .ok R' →
        Agrees killed seen vars R' E1
  | [], _, _, _, _, h, _, hp => by
    obtain ⟨rfl, rfl⟩ := h
    cases hp
  | u :: _, seen, _, _, _, h, hs, hp => by
    obtain ⟨a, b, R0, E10, rfl, rfl, hab, h'⟩ := h
    rw [Env.put] at hp
    split at hp
    · rename_i hyx
      cases hp
      have hu : u.name = x := beq_iff_eq.mp hyx
      refine ⟨v, b, R0, E10, by rw [hu], rfl, Or.inr ⟨by rw [hu]; exact hx, by rw [hu]; exact hs⟩,
        h'⟩
    · rename_i hyx
      obtain ⟨R1, hR1, h2⟩ := eval_bind_ok hp
      cases h2
      have hne : x ≠ u.name := fun he => hyx (beq_iff_eq.mpr he.symm)
      have hs' : x ∉ u.name :: seen := fun hm => (List.mem_cons.mp hm).elim hne hs
      exact ⟨a, b, R1, E10, rfl, rfl, hab, Agrees.put hx h' hs' hR1⟩

theorem Watch.Agree.cons {w : Watch} {E : Env} (x : String) (v : Value) (h : w.Agree E) :
    w.Agree ((x, v) :: E) := by
  obtain ⟨top, R, hE, hR⟩ := h
  exact ⟨(x, v) :: top, R, by subst hE; rfl, hR⟩

theorem Watch.Agree.put {w : Watch} {E E' : Env} {x : String} {v : Value} (h : w.Agree E)
    (hx : x ∈ w.killed) (hp : E.put x v = .ok E') : w.Agree E' := by
  obtain ⟨top, R, rfl, hR⟩ := h
  rcases put_append hp with ⟨top', _, rfl⟩ | ⟨R', hR', rfl⟩
  · exact ⟨top', R, rfl, hR⟩
  · exact ⟨top, R', rfl, Agrees.put hx hR (fun h => by cases h) hR'⟩

theorem Watch.Agree.restore {w : Watch} {E : Env} {d : Nat} (h : w.Agree E)
    (hd : w.E1.length ≤ d) : w.Agree (E.restore d) := by
  obtain ⟨top, R, rfl, hR⟩ := h
  have hlen := hR.length
  refine ⟨top.drop ((top ++ R).length - d), R, ?_, hR⟩
  unfold Env.restore
  apply List.drop_append_of_le_length
  simp only [List.length_append]
  omega

/-- A statement whose continuation is its normal edge and which leaves the
frame as it was. -/
theorem watch_same {W : List Watch} {s : Stmt} {E : Env}
    (hr : ∀ w : Watch, Checker.reachStmt w.reach s = w.reach.normal)
    (hW : ∀ w ∈ W, w.AtStmt s E) : Watch.Holds W (·.reach.normal) E := by
  intro w hw
  obtain ⟨hlen, hag, _⟩ := hW w hw
  exact ⟨hlen, fun hn => hag (by rw [hr w]; exact hn)⟩

/-- The same for one that adds a binding. -/
theorem watch_cons {W : List Watch} {s : Stmt} {E : Env} (x : String) (v : Value)
    (hr : ∀ w : Watch, Checker.reachStmt w.reach s = w.reach.normal)
    (hW : ∀ w ∈ W, w.AtStmt s E) : Watch.Holds W (·.reach.normal) ((x, v) :: E) := by
  intro w hw
  obtain ⟨hlen, hag, _⟩ := hW w hw
  exact ⟨Nat.le_succ_of_le hlen, fun hn => (hag (by rw [hr w]; exact hn)).cons x v⟩

/-- The same for a `set` of the binding named `x`, which kills `x` wherever its
edge reaches a loop's head. -/
theorem watch_set {W : List Watch} {s : Stmt} {E E' : Env} {x : String} {v : Value}
    (hr : ∀ w : Watch, Checker.reachStmt w.reach s = w.reach.normal)
    (hk : ∀ w : Watch, Checker.killStmt w.reach s = if w.reach.normal then [x] else [])
    (hW : ∀ w ∈ W, w.AtStmt s E) (hp : E.put x v = .ok E') :
    Watch.Holds W (·.reach.normal) E' := by
  intro w hw
  obtain ⟨hlen, hag, hkl⟩ := hW w hw
  refine ⟨by rw [put_length hp]; exact hlen, fun hn => ?_⟩
  have hn' : w.reach.normal = true := hn
  have hmem : x ∈ w.killed := by
    apply hkl x
    rw [hk w, ite_eq_left hn']
    exact List.Mem.head _
  exact (hag (by rw [hr w]; exact hn')).put hmem hp

/-! ## The triples -/

/-- The walk's environment belongs to the function `fn` of the accepted
program `p`, and the loops it is in have proved header relations. -/
structure EnvOK (p : Program) (fn : FnDecl) (env : Checker.Env) : Prop where
  prog : env.prog = p
  fn_eq : env.fn = fn
  accepts : p.accepts = true
  fn_of : p.fn? fn.name = some fn
  headers : ∀ lc ∈ env.loops, ∀ rel ∈ lc.headers, ∃ sc, Static.invariantOk sc rel = .ok ()

/-- The snapshots recorded by the time the generator is `g` begin the
function's table, whose positions are distinct. -/
def PtsOK (p : Program) (fn : FnDecl) (g : Checker.Gen) : Prop :=
  g.points <+: p.points fn ∧ ((p.points fn).map (·.1)).Nodup

/-- The statement passed the judgments without facts, in some context. -/
def StaticStmtOK (p : Program) (fn : FnDecl) (s : Stmt) : Prop :=
  ∃ c r, Static.stmt p fn c s = .ok r

/-- A statement's triple: from a frame where its entering snapshot holds,
with every enclosing loop watched, its step is safe for `n` steps when what it
leaves to is. -/
def StmtOK (F : Model) (p : Program) (fn : FnDecl) (s : Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (pos : Pos) (g g' : Checker.Gen)
    (env' : Checker.Env) (o : Checker.Out) (n : Nat) (W : List Watch) (E E0 : Env)
    (k0 k : List Kont) (D : Dest) (callers : List Frame),
    EnvOK p fn env → (∀ lc ∈ env.loops, lc.id < g.next) →
    (Checker.walkStmt env st pos s).run g = .ok ((env', o), g') →
    StaticStmtOK p fn s → PtsOK p fn g' →
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (∀ w ∈ W, w.AtStmt s E) →
    NormalOK F p n W env'.vars g'.atoms o.normal fn E0 k D callers →
    BreakOK F p n W env.loops env.vars g'.atoms g'.next o.breaks fn E0 k D callers →
    ContOK F p n W env.loops env.vars g'.atoms g'.next o.conts fn E0 k D callers →
    RetOK F p n fn E0 D callers → FnSafe F p n →
    StepOK (SafeAt F p n)
      (stepStmt p { fn, env := E, entry := E0, kont := k0, dest := D } callers pos s k)

theorem stmt_let {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (x : String) (e : Expr) :
    StmtOK F p fn (.let_ x e) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv _ hw hs _ hH hF hW hN _ _ _ hFn
  obtain ⟨r, g1, st', hr, hb, henv', ho⟩ := Checker.walkStmt_let_ok hw
  subst env'
  subst o
  obtain ⟨hprog, hfneq, hacc, _, _⟩ := henv
  subst hprog
  subst hfneq
  have ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t := by
    obtain ⟨c, _, hc⟩ := hs
    obtain ⟨t, ht⟩ := Static.stmt_let_ok hc
    exact ⟨c.scope, t, ht⟩
  rw [stepStmt_let]
  refine stepOK_evalRhs (fun path targs as g2 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, _⟩ := L.evalRhs_call_some hr hg hc
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn g2 args (.bind x) _ (callee_fn hc) (L.callArgs_inRange hH hF hr ht hg hrc hargs)
      (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    exact stepOK_doReturn_bind rfl hens fun hty =>
      hN st' (List.Mem.head _) _ (L.let_call_sound hH hF hr ht hacc hg hrc hb hargs hw hty hens)
        (watch_cons x w (fun _ => rfl) hW)
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun v hv => ?_⟩
    exact hN st' (List.Mem.head _) _ (L.let_sound hH hF hr ht hrc hb hv)
      (watch_cons x v (fun _ => rfl) hW)

theorem stmt_setVar {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (x : String)
    (e : Expr) : StmtOK F p fn (.set (.var x) e) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv _ hw hs _ hH hF hW hN _ _ _ hFn
  obtain ⟨v, r, g1, st', hv, hr, hsv, henv', ho⟩ := Checker.walkStmt_setVar_ok hw
  subst env'
  subst o
  obtain ⟨hprog, hfneq, hacc, _, _⟩ := henv
  subst hprog
  subst hfneq
  have ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t := by
    obtain ⟨c, _, hc⟩ := hs
    obtain ⟨t, ht⟩ := Static.stmt_setVar_ok hc
    exact ⟨c.scope, t, ht⟩
  rw [stepStmt_setTo]
  refine stepOK_evalRhs (fun path targs as g2 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, _⟩ := L.evalRhs_call_some hr hg hc
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn g2 args (.assign (.var x)) _ (callee_fn hc)
      (L.callArgs_inRange hH hF hr ht hg hrc hargs) (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    refine stepOK_doReturn_assign rfl hens fun hty => ?_
    refine stepOK_ofEval (commitTo_var_noStuck _ _ _) fun fr' hc' => ?_
    obtain ⟨hshape, new, hput⟩ := commitTo_var_shape hc'
    rw [hshape]
    exact hN st' (List.Mem.head _) _
      (L.setVar_call_sound hH hF hv hr ht hacc hg hrc hsv hargs hw hty hens hc' rfl)
      (watch_set (fun _ => rfl) (fun _ => rfl) hW hput)
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun val hval => ?_⟩
    refine stepOK_ofEval (commitTo_var_noStuck _ _ _) fun fr' hc' => ?_
    obtain ⟨hshape, new, hput⟩ := commitTo_var_shape hc'
    rw [hshape]
    exact hN st' (List.Mem.head _) _ (L.setVar_sound hH hF hv hr ht hrc hsv hval hc' rfl)
      (watch_set (fun _ => rfl) (fun _ => rfl) hW hput)

theorem stmt_setIndex {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (b : String)
    (off : Atom) (e : Expr) : StmtOK F p fn (.set (.index b off) e) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv _ hw hs _ hH hF hW hN _ _ _ hFn
  obtain ⟨v, r, g1, st1, g2, vt, hv, hr, hpub, hat, henv', ho⟩ := Checker.walkStmt_setIndex_ok hw
  subst env'
  subst o
  obtain ⟨hprog, hfneq, hacc, _, _⟩ := henv
  subst hprog
  subst hfneq
  have hst : (∃ sc t, Static.atomTy sc (.index b off) = .ok t) ∧
      ∃ sc t, Static.exprTy env.prog sc e = .ok t := by
    obtain ⟨c, _, hc⟩ := hs
    obtain ⟨⟨t1, h1⟩, t2, h2⟩ := Static.stmt_setIndex_ok hc
    exact ⟨⟨c.scope, t1, h1⟩, c.scope, t2, h2⟩
  obtain ⟨hta, ht⟩ := hst
  rw [stepStmt_setTo]
  refine stepOK_evalRhs (fun path targs as g3 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, _⟩ := L.evalRhs_call_some hr hg hc
    have hp := published_call hpub hrc
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn g3 args (.assign (.index b off)) _ (callee_fn hc)
      (L.callArgs_inRange hH hF hr ht hg hrc hargs) (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    refine stepOK_doReturn_assign rfl hens fun hty => ?_
    have hH1 := L.publish_sound hH hF hr ht hacc hg hrc hp hargs hw hty hens
    refine stepOK_ofEval (commitTo_index_noStuck (L.target_progress hH1 hat hta)) fun fr' hc' => ?_
    obtain ⟨hshape, new, hput⟩ := commitTo_index_shape hc'
    rw [hshape]
    exact hN _ (List.Mem.head _) _
      (L.setIndex_call_sound hH hF hv hr ht hta hacc hg hrc hp hat hargs hw hty hens hc' rfl)
      (watch_set (fun _ => rfl) (fun _ => rfl) hW hput)
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    obtain ⟨hs1, hg2⟩ := published_none hpub hrc
    subst st1
    subst g2
    have hH1 := L.lift hH hF (L.evalRhs_grows hr).1
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun val hval => ?_⟩
    refine stepOK_ofEval (commitTo_index_noStuck (L.target_progress hH1 hat hta)) fun fr' hc' => ?_
    obtain ⟨hshape, new, hput⟩ := commitTo_index_shape hc'
    rw [hshape]
    exact hN _ (List.Mem.head _) _ (L.setIndex_sound hH hF hv hr ht hta hrc hat hval hc' rfl)
      (watch_set (fun _ => rfl) (fun _ => rfl) hW hput)

theorem stmt_call {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (e : Expr) :
    StmtOK F p fn (.call e) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv _ hw hs _ hH hF hW hN _ _ _ hFn
  obtain ⟨r, g1, st1, hr, hpub, henv', ho⟩ := Checker.walkStmt_call_ok hw
  subst env'
  subst o
  obtain ⟨hprog, hfneq, hacc, _, _⟩ := henv
  subst hprog
  subst hfneq
  have ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t := by
    obtain ⟨c, _, hc⟩ := hs
    obtain ⟨t, ht⟩ := Static.stmt_call_ok hc
    exact ⟨c.scope, t, ht⟩
  rw [stepStmt_call]
  refine stepOK_evalRhs (fun path targs as g2 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, _⟩ := L.evalRhs_call_some hr hg hc
    have hp := published_call hpub hrc
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn g2 args .discard _ (callee_fn hc) (L.callArgs_inRange hH hF hr ht hg hrc hargs)
      (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    exact stepOK_doReturn_discard rfl hens fun hty =>
      hN st1 (List.Mem.head _) _ (L.publish_sound hH hF hr ht hacc hg hrc hp hargs hw hty hens)
        (watch_same (fun _ => rfl) hW)
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    obtain ⟨hs1, hg1⟩ := published_none hpub hrc
    subst st1
    subst g'
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun _ _ => ?_⟩
    exact hN st (List.Mem.head _) _ (L.lift hH hF (L.evalRhs_grows hr).1)
      (watch_same (fun _ => rfl) hW)

theorem stmt_ret {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (e : Expr) :
    StmtOK F p fn (.ret e) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv _ hw hs _ hH hF _ _ _ _ hR hFn
  obtain ⟨r, g1, st1, g2, hr, hpub, hret, henv', ho⟩ := Checker.walkStmt_ret_ok hw
  subst env'
  subst o
  obtain ⟨hprog, hfneq, hacc, hfnof, _⟩ := henv
  subst hprog
  subst hfneq
  have ht : ∃ sc t, Static.exprTy env.prog sc e = .ok t := by
    obtain ⟨c, _, hc⟩ := hs
    obtain ⟨t, ht⟩ := Static.stmt_ret_ok hc
    exact ⟨c.scope, t, ht⟩
  rw [stepStmt_ret]
  refine stepOK_evalRhs (fun path targs as g3 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, hterm⟩ := L.evalRhs_call_some hr hg hc
    have hens0 : env.fn.ensures = [] := checkReturn_ensures_nil hret hterm
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn g3 args .ret _ (callee_fn hc) (L.callArgs_inRange hH hF hr ht hg hrc hargs)
      (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    refine stepOK_doReturn_ret rfl hens fun _ => ?_
    refine hR E k w hw ?_
    rw [hens0]
    all_goals rfl
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    obtain ⟨hs1, hg2⟩ := published_none hpub hrc
    subst st1
    subst g2
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun v hv => ?_⟩
    obtain ⟨hin, hens⟩ := L.return_ensures hH hF hr ht hacc hfnof hrc hret hv
    exact hR E k0 v hin hens

theorem stmt_invariant {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (nm : String)
    (rel : Relation) : StmtOK F p fn (.invariant nm rel) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers _ _ hw hs _ hH hF hW hN _ _ _ _
  obtain ⟨hrel, henv', ho, hg'⟩ := Checker.walkStmt_invariant_ok hw
  subst env'
  subst o
  subst g'
  have hinv : ∃ sc, Static.invariantOk sc rel = .ok () := by
    obtain ⟨c, _, hc⟩ := hs
    exact ⟨c.scope, Static.stmt_invariant_ok hc⟩
  rw [stepStmt_invariant]
  exact hN _ (List.Mem.head _) E
    (L.establishRel_sound hH hF hinv (L.relHolds_sound hH hF hinv hrel))
    (watch_same (fun _ => rfl) hW)

theorem stmt_brk {F : Model} {p : Program} {fn : FnDecl} (label : Option String) :
    StmtOK F p fn (.brk label) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers _ _ hw _ _ hH _ hW _ hB _ _ _
  obtain ⟨l, hl, henv', ho, hg'⟩ := Checker.walkStmt_brk_ok hw
  subst env'
  subst o
  subst g'
  rw [stepStmt_brk]
  refine hB label l st hl (List.Mem.head _) [] (fun _ hv => by cases hv) E k0 hH ?_
  intro w hw
  obtain ⟨hlen, hag, _⟩ := hW w hw
  exact ⟨hlen, hag⟩

theorem stmt_cont {F : Model} {p : Program} {fn : FnDecl} (label : Option String) :
    StmtOK F p fn (.cont label) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers _ _ hw _ _ hH _ hW _ _ hC _ _
  obtain ⟨l, hl, henv', ho, hg'⟩ := Checker.walkStmt_cont_ok hw
  subst env'
  subst o
  subst g'
  rw [stepStmt_cont]
  refine hC label l st hl (List.Mem.head _) [] (fun _ hv => by cases hv) E k0 hH ?_
  intro w hw
  obtain ⟨hlen, hag, _⟩ := hW w hw
  exact ⟨hlen, hag⟩

end Sim

end Whitefoot
