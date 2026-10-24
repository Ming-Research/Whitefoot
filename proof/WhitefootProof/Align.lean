import WhitefootProof.Safety
import WhitefootProof.Holds

/-!
# The walk's binding table and the judgments' contexts

The checker judges each statement without facts from a static context
(`Static.lean`) and walks it with a binding table. Some lemmas the simulation
rests on read a judgment in the binding table's scope (`Checker.scopeOf`): a
call's requirements and an invariant are judged there, and a return's
postconditions need the function's parameters bound as the walk bound them.
This module proves that a context stays in line with the walk's binding table
(`Sim.CtxOK`): its scope is the table's, it keeps every parameter's name
taken, and each parameter's binding is the one the walk's parameter list names.

A function's entry starts in line (`Sim.ctxOK_entry`), and each statement's
walk and judgment keep it in line (`Sim.StmtAlign`). A `let` does because a
right-hand side's checker value has the type its judgment gives
(`Checker.evalRhs_ty`). That lemma reads the type off every value a checker
computation can return (`Checker.Yields`), without following the checks the
computation runs on its way.
-/

namespace Whitefoot

namespace Checker

/-! ## What every successful run returns -/

/-- Every successful run of `m` returns a value that satisfies `P`. -/
def Yields {α : Type} (P : α → Prop) (m : M α) : Prop :=
  ∀ g a g', m.run g = .ok (a, g') → P a

theorem Yields.run {α : Type} {P : α → Prop} {m : M α} {g g' : Gen} {a : α}
    (hm : Yields P m) (h : m.run g = .ok (a, g')) : P a :=
  hm g a g' h

theorem Yields.of_pure {α : Type} {P : α → Prop} {a : α} (h : P a) : Yields P (pure a) :=
  fun _ _ _ hr => by
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure hr
    exact h

theorem Yields.of_bind {α β : Type} {P : β → Prop} {m : M α} {f : α → M β}
    (hf : ∀ a, Yields P (f a)) : Yields P (m >>= f) :=
  fun g b g' hr => by
    obtain ⟨a, g1, -, h2⟩ := CheckerRun.run_bind hr
    exact hf a g1 b g' h2

theorem Yields.of_fail {α : Type} {P : α → Prop} (rule msg : String) :
    Yields P (fail rule msg : M α) :=
  fun _ _ _ hr => (CheckerRun.run_fail hr).elim

theorem Yields.of_fail_bind {α β : Type} {P : β → Prop} {rule msg : String} {f : α → M β} :
    Yields P (fail rule msg >>= f) :=
  fun _ _ _ hr => by
    obtain ⟨_, _, hf, -⟩ := CheckerRun.run_bind hr
    exact (CheckerRun.run_fail hf).elim

/-- Follows a checker computation through its binds, matches and conditionals
to each `pure` it can end in, and proves the predicate there by `rfl`. A bind's
first computation is passed over, and each step matches the computation's head
at reducible transparency, so the checks a computation runs on its way, such
as a discharge query, are never unfolded. -/
syntax "yields" : tactic

macro_rules
  | `(tactic| yields) => `(tactic| repeat' first
      | rfl
      | with_reducible exact Yields.of_fail _ _
      | with_reducible exact Yields.of_fail_bind
      | (with_reducible refine Yields.of_bind fun _ => ?_)
      | (with_reducible refine Yields.of_pure ?_)
      | dsimp only
      | split)

/-! ## A right-hand side's value has its judged type -/

theorem find?_map_key {α β : Type} (key : α → String) (val : α → β) (x : String) :
    ∀ l : List α, (l.map fun a => (key a, val a)).find? (·.1 == x) =
      (l.find? fun a => key a == x).map fun a => (key a, val a)
  | [] => rfl
  | a :: l => by
    cases h : key a == x with
    | true => simp only [List.map, List.find?, h, Option.map]
    | false =>
      simp only [List.map, List.find?, h]
      exact find?_map_key key val x l

/-- A binding of the table has its type in the table's scope. -/
theorem lookup_scopeOf {env : Env} {x : String} {v : Var} (h : env.var? x = some v) :
    Static.lookup (scopeOf env.vars) x = .ok v.ty := by
  have h' : env.vars.find? (·.name == x) = some v := h
  unfold Static.lookup scopeOf
  rw [find?_map_key Var.name Var.ty x env.vars, h']
  all_goals rfl

theorem evalAtom_ty {env : Env} {st : State} {a : Atom} {g g' : Gen} {v : Val} {t : Ty}
    (h : (evalAtom env st a).run g = .ok (v, g'))
    (ht : Static.atomTy (scopeOf env.vars) a = .ok t) : v.ty = t := by
  cases a with
  | int t0 z =>
    simp only [evalAtom] at h
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
    cases ht
    rfl
  | unit =>
    simp only [evalAtom] at h
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    cases ht
    rfl
  | var x =>
    cases hv : env.var? x with
    | none =>
      simp only [evalAtom, hv] at h
      exact (CheckerRun.run_fail h).elim
    | some w =>
      have hwt : w.ty = t := by
        have h1 : Static.lookup (scopeOf env.vars) x = .ok t := ht
        rw [lookup_scopeOf hv] at h1
        exact Except.ok.inj h1
      subst hwt
      cases hty : w.ty <;> simp only [evalAtom, hv, hty] at h <;>
        (obtain ⟨rfl, -⟩ := CheckerRun.run_pure h) <;> rfl
  | index b off =>
    obtain ⟨bt, hbt, ht⟩ := Typing.bind_ok ht
    obtain ⟨ot, -, ht⟩ := Typing.bind_ok ht
    cases hv : env.var? b with
    | none =>
      simp only [evalAtom, hv] at h
      exact (CheckerRun.run_fail h).elim
    | some w =>
      rw [lookup_scopeOf hv] at hbt
      have hwt : w.ty = bt := Except.ok.inj hbt
      simp only [evalAtom, hv] at h
      obtain ⟨o, g1, -, h⟩ := CheckerRun.run_bind h
      rw [hwt] at h
      cases bt with
      | array et n =>
        dsimp only at ht
        split at ht
        · cases ht
          refine Yields.run (P := fun v : Val => v.ty = _) ?_ h
          yields
        · exact absurd ht Typing.reject_ne
      | int _ => exact absurd ht Typing.reject_ne
      | unit => exact absurd ht Typing.reject_ne
      | bool => exact absurd ht Typing.reject_ne
      | named _ => exact absurd ht Typing.reject_ne

/-- The type the checker gives an infix operation's value, by its operator
and its left operand's type. -/
def infixRes : InfixOp → Ty → Ty
  | .compare _, _ => .bool
  | .arith _ .defined, _ => .bool
  | .divide _ .defined, _ => .bool
  | _, .int t => .int t
  | _, _ => .bool

theorem infixTy_res {op : InfixOp} {ta tb t : Ty} (h : Static.infixTy op ta tb = .ok t) :
    t = infixRes op ta := by
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases ta <;> cases tb <;> try exact absurd h Typing.reject_ne
    rename_i s s'
    by_cases hss : s = s'
    · subst hss
      cases m <;> simp [Static.infixTy] at h <;> subst h <;> rfl
    · simp [Static.infixTy, hss, rejectBy] at h
  · cases m
    · cases ta <;> cases tb <;> try exact absurd h Typing.reject_ne
      rename_i s s'
      by_cases hss : s = s'
      · subst hss
        simp [Static.infixTy] at h
        subst h
        rfl
      · simp [Static.infixTy, hss, rejectBy] at h
    · exact absurd h Typing.reject_ne
    · cases ta <;> cases tb <;> try exact absurd h Typing.reject_ne
      rename_i s s'
      by_cases hss : s = s'
      · subst hss
        simp [Static.infixTy] at h
        subst h
        rfl
      · simp [Static.infixTy, hss, rejectBy] at h
  · cases c <;> cases ta <;> cases tb <;> try exact absurd h Typing.reject_ne
    all_goals first
      | (rename_i s s'
         by_cases hss : s = s'
         · subst hss
           simp [Static.infixTy] at h
           subst h
           rfl
         · simp [Static.infixTy, hss, rejectBy] at h)
      | (simp [Static.infixTy] at h
         subst h
         rfl)

/-- The type the checker gives a table operation's value, by its operator and
its operands' types. -/
def opRes : TableOp → List Ty → Ty
  | .unary _ m, [ty] =>
    match ty, m with
    | .int _, .defined => .bool
    | .int t, _ => .int t
    | ty, _ => ty
  | _, _ => .bool

theorem opTy_res {o : TableOp} {ts : List Ty} {t : Ty} (h : Static.opTy o ts = .ok t) :
    t = opRes o ts := by
  rcases ts with _ | ⟨t1, _ | ⟨t2, _ | ⟨t3, ts⟩⟩⟩
  · cases o <;> exact absurd h Typing.reject_ne
  · cases o with
    | unary u m =>
      cases t1 <;> try exact absurd h Typing.reject_ne
      rename_i s
      cases hs : s.signed
      · simp [Static.opTy, hs, rejectBy] at h
      · cases m <;> simp [Static.opTy, hs] at h <;> subst h <;> rfl
    | bnot =>
      cases t1 <;> try exact absurd h Typing.reject_ne
      simp [Static.opTy] at h
      subst h
      rfl
    | band => cases t1 <;> exact absurd h Typing.reject_ne
    | bor => cases t1 <;> exact absurd h Typing.reject_ne
    | bxor => cases t1 <;> exact absurd h Typing.reject_ne
  · cases o <;> cases t1 <;> cases t2 <;> try exact absurd h Typing.reject_ne
    all_goals
      simp [Static.opTy] at h
      subst h
      rfl
  · cases o <;> cases t1 <;> cases t2 <;>
      first
        | exact absurd h Typing.reject_ne
        | (cases t3 <;> exact absurd h Typing.reject_ne)

theorem mapM_evalAtom_ty {env : Env} {st : State} :
    ∀ {as : List Atom} {g g' : Gen} {vs : List Val} {ts : List Ty},
      (as.mapM (evalAtom env st)).run g = .ok (vs, g') →
      as.mapM (Static.atomTy (scopeOf env.vars)) = .ok ts → ts = vs.map (·.ty)
  | [], g, g', vs, ts, h, ht => by
    rw [List.mapM_nil] at h ht
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    cases ht
    rfl
  | a :: as, g, g', vs, ts, h, ht => by
    rw [List.mapM_cons] at h ht
    obtain ⟨v, g1, hv, h⟩ := CheckerRun.run_bind h
    obtain ⟨vs', g2, hvs, h⟩ := CheckerRun.run_bind h
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    obtain ⟨t1, ht1, ht⟩ := Typing.bind_ok ht
    obtain ⟨ts', hts, ht⟩ := Typing.bind_ok ht
    cases ht
    show t1 :: ts' = v.ty :: vs'.map (·.ty)
    rw [evalAtom_ty hv ht1, mapM_evalAtom_ty hvs hts]

theorem infix_ty {env : Env} {st : State} {a b : Atom} {op : InfixOp} {g g' : Gen} {r : Rhs}
    {t : Ty} (h : (evalRhs env st (.infix a op b)).run g = .ok (r, g'))
    (ht : Static.exprTy env.prog (scopeOf env.vars) (.infix a op b) = .ok t) :
    r.val.ty = t := by
  simp only [evalRhs] at h
  obtain ⟨va, g1, hva, h⟩ := CheckerRun.run_bind h
  obtain ⟨vb, g2, -, h⟩ := CheckerRun.run_bind h
  obtain ⟨ta, hta, ht⟩ := Typing.bind_ok ht
  obtain ⟨tb, -, ht⟩ := Typing.bind_ok ht
  rw [infixTy_res ht, ← evalAtom_ty hva hta]
  refine Yields.run (P := fun r : Rhs => r.val.ty = infixRes op va.ty) ?_ h
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases m <;> cases va.ty <;> yields
  · cases m <;> cases va.ty <;> yields
  · yields

theorem op_ty {env : Env} {st : State} {o : TableOp} {as : List Atom} {g g' : Gen} {r : Rhs}
    {t : Ty} (h : (evalRhs env st (.op o as)).run g = .ok (r, g'))
    (ht : Static.exprTy env.prog (scopeOf env.vars) (.op o as) = .ok t) :
    r.val.ty = t := by
  simp only [evalRhs] at h
  obtain ⟨vs, g1, hvs, h⟩ := CheckerRun.run_bind h
  obtain ⟨ts, hts, ht⟩ := Typing.bind_ok ht
  rw [opTy_res ht, mapM_evalAtom_ty hvs hts]
  refine Yields.run (P := fun r : Rhs => r.val.ty = opRes o (vs.map (·.ty))) ?_ h
  rcases vs with _ | ⟨va, _ | ⟨vb, _ | ⟨vc, rest⟩⟩⟩
  · cases o <;> yields
  · simp only [List.map_cons, List.map_nil]
    cases o with
    | unary u m =>
      -- Select the operator's arm first, so the cases on the operand's type
      -- reach the match on it.
      dsimp only
      cases m <;> cases va.ty <;> yields
    | band => yields
    | bor => yields
    | bxor => yields
    | bnot => yields
  · cases o <;> yields
  · cases o <;> yields

theorem call_ty {env : Env} {st : State} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {g g' : Gen} {r : Rhs} {t : Ty}
    (h : (evalRhs env st (.call path targs as)).run g = .ok (r, g'))
    (ht : Static.exprTy env.prog (scopeOf env.vars) (.call path targs as) = .ok t) :
    r.val.ty = t := by
  simp only [evalRhs] at h
  simp only [Static.exprTy] at ht
  cases hc : env.prog.callee? path with
  | none =>
    rw [hc] at ht
    exact absurd ht Typing.reject_ne
  | some ce =>
    rw [hc] at h ht
    cases ce with
    | fn f =>
      cases targs with
      | cons _ _ => exact absurd ht Typing.reject_ne
      | nil =>
        obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
        obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
        cases ht
        refine Yields.run (P := fun r : Rhs => r.val.ty = _) ?_ h
        yields
    | exitStatus =>
      cases targs with
      | cons _ _ => exact absurd ht Typing.reject_ne
      | nil =>
        obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
        obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
        cases ht
        refine Yields.run (P := fun r : Rhs => r.val.ty = _) ?_ h
        yields
    | arrayFilled =>
      rcases targs with _ | ⟨(et | _ | _ | ⟨_, _⟩ | _) | _, _ | ⟨_ | n, _ | ⟨_, _⟩⟩⟩ <;>
        try exact absurd ht Typing.reject_ne
      obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
      obtain ⟨_, -, ht⟩ := Typing.bind_ok ht
      cases ht
      refine Yields.run (P := fun r : Rhs => r.val.ty = _) ?_ h
      yields

/-- A right-hand side's checker value has the type the judgment without facts
gives it in the binding table's scope. -/
theorem evalRhs_ty {env : Env} {st : State} {e : Expr} {g g' : Gen} {r : Rhs} {t : Ty}
    (h : (evalRhs env st e).run g = .ok (r, g'))
    (ht : Static.exprTy env.prog (scopeOf env.vars) e = .ok t) : r.val.ty = t := by
  rcases e with a | ⟨a, op, b⟩ | ⟨o, as⟩ | ⟨path, targs, as⟩ | bv
  · simp only [evalRhs] at h
    obtain ⟨v, g1, hv, h⟩ := CheckerRun.run_bind h
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    exact evalAtom_ty hv ht
  · exact infix_ty h ht
  · exact op_ty h ht
  · exact call_ty h ht
  · simp only [evalRhs] at h
    obtain ⟨rfl, -⟩ := CheckerRun.run_pure h
    cases ht
    rfl

/-- A function's entry lists each parameter's binding as its binding table
does. -/
theorem entryState_params {p : Program} {f : FnDecl} {g g' : Gen} {es : Env × State}
    (h : (entryState p f).run g = .ok (es, g')) :
    es.1.params = es.1.vars.map fun v => (v.name, v.id) := by
  refine Yields.run
    (P := fun es : Env × State => es.1.params = es.1.vars.map fun v => (v.name, v.id)) ?_ h
  unfold entryState
  yields

end Checker

namespace Sim

/-! ## A function's entry -/

/-- A value of a static type that the interpreter's test gives a declared type
has the declared type's resolved form. -/
theorem typed_normTy {p : Program} {v : Value} {t T : Ty} (h : v.Typed t)
    (hT : v.hasTy p T = true) : t = Static.normTy p T := by
  cases v <;> cases t <;> try exact (h : False).elim
  all_goals
    cases T <;> simp_all [Value.Typed, Static.normTy, Value.hasTy, Program.isExitStatus]

/-- Arguments named as the parameters and of their declared types, typed as a
scope says: the scope is the parameters'. -/
theorem scope_of_args {p : Program} :
    ∀ {args : List (String × Value)} {ps : List Param} {sc : Static.Scope},
      EnvTyped args sc → args.map (·.1) = ps.map (·.name) →
      (∀ x ∈ args.zip ps, x.1.2.hasTy p x.2.ty = true) →
      sc = ps.map fun prm => (prm.name, Static.normTy p prm.ty)
  | [], [], _, h, _, _ => by
    cases h
    rfl
  | [], _ :: _, _, _, hn, _ => by simp at hn
  | _ :: _, [], _, _, hn, _ => by simp at hn
  | (n, v) :: as, prm :: ps, _, h, hn, ht => by
    cases h with
    | cons hv h' =>
      simp only [List.map_cons, List.cons.injEq] at hn
      obtain ⟨rfl, hn⟩ := hn
      have h1 : v.hasTy p prm.ty = true := ht ((prm.name, v), prm) (by simp)
      have h2 : ∀ x ∈ as.zip ps, x.1.2.hasTy p x.2.ty = true :=
        fun x hx => ht x (by simp [hx])
      rw [scope_of_args h' hn h2, typed_normTy hv h1]
      all_goals rfl

/-- The binding table a function's entry holds on gives the parameters' scope
when the entry's arguments are the parameters'. -/
theorem scopeOf_entry {p : Program} {g : FnDecl} {vars : List Checker.Var}
    {args : List (String × Value)} (hE : EnvTyped args.reverse (Checker.scopeOf vars))
    (hn : args.map (·.1) = g.params.map (·.name))
    (ht : ∀ x ∈ args.zip g.params, x.1.2.hasTy p x.2.ty = true) :
    Checker.scopeOf vars = Typing.paramScope p g := by
  have h1 := hE.reverse
  rw [List.reverse_reverse] at h1
  have h2 := congrArg List.reverse (scope_of_args h1 hn ht)
  rw [List.reverse_reverse] at h2
  rw [h2]
  simp [Typing.paramScope]

/-! ## Contexts in line with the walk -/

/-- A static context in line with the walk's environment of the function `fn`:
its scope is the binding table's, it keeps the parameters' names taken, and
each parameter's binding is the one the parameter list names. -/
structure CtxOK (fn : FnDecl) (env : Checker.Env) (c : Static.Ctx) : Prop where
  scope : c.scope = Checker.scopeOf env.vars
  taken : ∀ prm ∈ fn.params, prm.name ∈ c.taken
  params : ∀ prm ∈ fn.params, ∃ vr, env.var? prm.name = some vr ∧
    env.params.find? (·.1 == prm.name) = some (prm.name, vr.id)

theorem params_of_vars {env : Checker.Env}
    (hp : env.params = env.vars.map fun v => (v.name, v.id)) {x : String}
    (hx : ∃ v ∈ env.vars, v.name = x) :
    ∃ vr, env.var? x = some vr ∧ env.params.find? (·.1 == x) = some (x, vr.id) := by
  obtain ⟨v, hv, hvx⟩ := hx
  cases hf : env.vars.find? (·.name == x) with
  | none =>
    have := List.find?_eq_none.mp hf v hv
    simp [hvx] at this
  | some vr =>
    refine ⟨vr, hf, ?_⟩
    have hname : vr.name = x := by simpa using List.find?_some hf
    rw [hp, Checker.find?_map_key Checker.Var.name Checker.Var.id x env.vars, hf]
    simp [hname]

/-- A function's body is judged from a context in line with its entry. -/
theorem ctxOK_entry {p : Program} {g : FnDecl} {env : Checker.Env}
    (hsc : Checker.scopeOf env.vars = Typing.paramScope p g)
    (hparams : env.params = env.vars.map fun v => (v.name, v.id)) :
    CtxOK g env (Typing.initCtx p g) := by
  refine ⟨hsc.symm, fun prm hprm => ?_, fun prm hprm => ?_⟩
  · show prm.name ∈ g.params.map (·.name) ++ Static.moduleAliases p
    exact List.mem_append.mpr (Or.inl (List.mem_map.mpr ⟨prm, hprm, rfl⟩))
  · have hmem : (prm.name, Static.normTy p prm.ty) ∈ Checker.scopeOf env.vars := by
      rw [hsc]
      exact List.mem_map.mpr ⟨prm, List.mem_reverse.mpr hprm, rfl⟩
    obtain ⟨v, hv, hve⟩ := List.mem_map.mp hmem
    exact params_of_vars hparams ⟨v, hv, congrArg Prod.fst hve⟩

/-- A statement's walk and its judgment keep a context in line. -/
def StmtAlign (p : Program) (fn : FnDecl) (s : Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (pos : Pos) (g g' : Checker.Gen)
    (env' : Checker.Env) (o : Checker.Out) (c c' : Static.Ctx) (fl : Static.Flow),
    env.prog = p → (Checker.walkStmt env st pos s).run g = .ok ((env', o), g') →
    Static.stmt p fn c s = .ok (c', fl) → CtxOK fn env c → CtxOK fn env' c'

/-- A `let` opens a binding of the type its judgment adds to the scope, under
a name no parameter has. -/
theorem align_let {p : Program} {fn : FnDecl} (x : String) (e : Expr) :
    StmtAlign p fn (.let_ x e) := by
  intro env st pos g g' env' o c c' fl hprog hw hc hcx
  obtain ⟨r, g1, st', hr, -, henv', -⟩ := Checker.walkStmt_let_ok hw
  subst henv'
  have hfr : ¬(c.taken.contains x = true) := by
    simp only [Static.stmt] at hc
    obtain ⟨_, hf, -⟩ := Typing.bind_ok hc
    unfold Static.fresh at hf
    exact (Typing.ite_reject hf).1
  obtain ⟨t, ht, hr'⟩ := Typing.stmt_let hc
  cases hr'
  have hty : r.val.ty = t := by
    subst hprog
    rw [hcx.scope] at ht
    exact Checker.evalRhs_ty hr ht
  refine ⟨?_, fun prm hprm => List.mem_cons_of_mem _ (hcx.taken prm hprm), fun prm hprm => ?_⟩
  · show (x, t) :: c.scope =
      Checker.scopeOf ({ name := x, id := g1.next, ty := r.val.ty } :: env.vars)
    rw [hcx.scope, hty]
    all_goals rfl
  · obtain ⟨vr, hv, hp⟩ := hcx.params prm hprm
    refine ⟨vr, ?_, hp⟩
    have hne : (x == prm.name) = false := by
      cases hb : x == prm.name with
      | false => rfl
      | true =>
        refine absurd (List.contains_iff_mem.mpr ?_) hfr
        rw [eq_of_beq hb]
        exact hcx.taken prm hprm
    show List.find? (fun v => v.name == prm.name)
      ({ name := x, id := g1.next, ty := r.val.ty } :: env.vars) = some vr
    simp only [List.find?, hne]
    exact hv

end Sim

end Whitefoot
