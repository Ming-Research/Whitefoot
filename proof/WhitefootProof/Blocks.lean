import WhitefootProof.Simulation

/-!
# Statement lists and `if`

A statement list runs its statements one after another: each from a frame on
which the snapshot the walk recorded on entering it holds, the next from the
join of the normal edges leaving it. An `if` runs one of its blocks from the
state the walk entered that block with, and at the block's end leaves the
bindings the block opened. Alongside, the walk's freshness: every state a walk
leaves has its identities below the counter the walk ends with, which a later
join reads.

The triples of the statements a list holds are premises of the list's triple,
and the triples of an `if`'s blocks are premises of the `if`'s; the
simulation's induction over the syntax ties them together once every kind of
statement has its triple.
-/

namespace Whitefoot

namespace Checker

theorem run_fail_bind_ok {α β : Type} {rule msg : String} {f : α → M β} {g g' : Gen} {b : β}
    (h : ((fail rule msg : M α) >>= f).run g = .ok (b, g')) : False := by
  obtain ⟨_, _, hf, _⟩ := run_bind_ok h
  exact run_fail_ok hf

/-- A successful run of a choice ran the side it took, from the same
generator. -/
theorem run_ite_ok {α : Type} {c : Prop} [Decidable c] {A B : M α} {g g' : Gen} {a : α}
    (h : (if c then A else B).run g = .ok (a, g')) :
    A.run g = .ok (a, g') ∨ B.run g = .ok (a, g') := by
  by_cases hc : c
  · rw [ite_eq_left hc] at h
    exact Or.inl h
  · rw [ite_eq_right hc] at h
    exact Or.inr h

/-- The end of an `if`'s walk: both blocks walked, from the states their sides
of the condition establish, and their exits gathered. -/
theorem ifTail_ok {env : Env} {st1 : State} {O : Origins} {pos : Pos} {thn els : List Stmt}
    {g2 g' : Gen} {env' : Env} {o : Out}
    (h : (walkList env (branchState env st1 O true) (pos ++ [0]) 0 thn >>= fun ot =>
          walkList env (branchState env st1 O false) (pos ++ [1]) 0 els >>= fun oe =>
          (pure (env, ({ normal := (ot.normal ++ oe.normal).map (·.exit env.vars),
                         breaks := ot.breaks ++ oe.breaks, conts := ot.conts ++ oe.conts } : Out)) :
            M (Env × Out))).run g2 = .ok ((env', o), g')) :
    ∃ ot gT oe,
      (walkList env (branchState env st1 O true) (pos ++ [0]) 0 thn).run g2 = .ok (ot, gT) ∧
      (walkList env (branchState env st1 O false) (pos ++ [1]) 0 els).run gT = .ok (oe, g') ∧
      env' = env ∧
      o = { normal := (ot.normal ++ oe.normal).map (·.exit env.vars),
            breaks := ot.breaks ++ oe.breaks, conts := ot.conts ++ oe.conts } := by
  obtain ⟨ot, gT, hwt, h1⟩ := run_bind_ok h
  obtain ⟨oe, g3, hwe, h2⟩ := run_bind_ok h1
  obtain ⟨h3, rfl⟩ := run_pure_ok h2
  exact ⟨ot, gT, oe, hwt, hwe, congrArg Prod.fst h3, congrArg Prod.snd h3⟩

/-- A successful walk of an `if`: the condition's right-hand side, its
publication when it calls a function of the program, the checks that the
condition is not decided either way (which only reject), and both blocks. -/
theorem walkStmt_if_ok {env : Env} {st : State} {pos : Pos} {c : Expr} {thn els : List Stmt}
    {he : Bool} {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.if_ c thn els he)).run g = .ok ((env', o), g')) :
    ∃ r g1 st1 g2 ot gT oe, (evalRhs env st c).run g = .ok (r, g1) ∧
      Published env st r g1 st1 g2 ∧
      (walkList env (branchState env st1
          (if r.call.isSome then {} else origins env st1 c r.val.gexpr) true)
        (pos ++ [0]) 0 thn).run g2 = .ok (ot, gT) ∧
      (walkList env (branchState env st1
          (if r.call.isSome then {} else origins env st1 c r.val.gexpr) false)
        (pos ++ [1]) 0 els).run gT = .ok (oe, g') ∧
      env' = env ∧
      o = { normal := (ot.normal ++ oe.normal).map (·.exit env.vars),
            breaks := ot.breaks ++ oe.breaks, conts := ot.conts ++ oe.conts } := by
  rw [walkStmt] at h
  obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
  cases hc : r.call with
  | none =>
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    obtain ⟨hst, hg⟩ := run_pure_ok hp
    refine ⟨r, g1, st1, g2, ?_⟩
    rw [hc]
    obtain ⟨_, _, hget, h3⟩ := run_bind_ok h2
    obtain ⟨rfl, rfl⟩ := run_get_ok hget
    rcases run_ite_ok h3 with hA | hB
    · rcases run_ite_ok hA with hA1 | hA2
      · exact (run_fail_bind_ok hA1).elim
      rcases run_ite_ok hA2 with hA21 | hA22
      · exact (run_fail_bind_ok hA21).elim
      obtain ⟨ot, gT, oe, hwt, hwe, he', ho⟩ := ifTail_ok hA22
      exact ⟨ot, gT, oe, hr, Or.inl ⟨hc, hst, hg⟩, hwt, hwe, he', ho⟩
    · obtain ⟨ot, gT, oe, hwt, hwe, he', ho⟩ := ifTail_ok hB
      exact ⟨ot, gT, oe, hr, Or.inl ⟨hc, hst, hg⟩, hwt, hwe, he', ho⟩
  | some fa =>
    obtain ⟨f, args⟩ := fa
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    refine ⟨r, g1, st1, g2, ?_⟩
    rw [hc]
    obtain ⟨_, _, hget, h3⟩ := run_bind_ok h2
    obtain ⟨rfl, rfl⟩ := run_get_ok hget
    rcases run_ite_ok h3 with hA | hB
    · rcases run_ite_ok hA with hA1 | hA2
      · exact (run_fail_bind_ok hA1).elim
      rcases run_ite_ok hA2 with hA21 | hA22
      · exact (run_fail_bind_ok hA21).elim
      obtain ⟨ot, gT, oe, hwt, hwe, he', ho⟩ := ifTail_ok hA22
      exact ⟨ot, gT, oe, hr, Or.inr ⟨f, args, hc, hp⟩, hwt, hwe, he', ho⟩
    · obtain ⟨ot, gT, oe, hwt, hwe, he', ho⟩ := ifTail_ok hB
      exact ⟨ot, gT, oe, hr, Or.inr ⟨f, args, hc, hp⟩, hwt, hwe, he', ho⟩

/-- A successful run of a value wrapped in `some` returns `some`. -/
theorem run_map_some {α : Type} {x : M α} {g g' : Gen} {o : Option α}
    (h : (some <$> x).run g = .ok (o, g')) : ∃ a, o = some a := by
  rw [map_eq_pure_bind] at h
  obtain ⟨a, _, _, h2⟩ := run_bind_ok h
  exact ⟨a, (run_pure_ok h2).1⟩

/-- A join returns no state only when no edge reaches it. -/
theorem merge_none {env : Env} {ss : List State} {g g' : Gen}
    (h : (merge env ss).run g = .ok (none, g')) : ss = [] := by
  cases ss with
  | nil => rfl
  | cons s1 rest =>
    exfalso
    cases rest with
    | nil => cases h
    | cons s2 rest' =>
      change (some <$> joinStates env (s1 :: s2 :: rest')).run g = .ok (none, g') at h
      obtain ⟨_, ha⟩ := run_map_some h
      cases ha

end Checker

namespace Sim

/-! ## The reachability of a loop's head, one step at a time -/

theorem reachList_nil (k : Checker.Reach) : Checker.reachList k [] = k.normal := by
  first | rfl | rw [Checker.reachList] | simp only [Checker.reachList]

theorem reachList_cons (k : Checker.Reach) (s : Stmt) (rest : List Stmt) :
    Checker.reachList k (s :: rest) =
      Checker.reachStmt { k with normal := Checker.reachList k rest } s := by
  first | rfl | rw [Checker.reachList] | simp only [Checker.reachList]

theorem killList_cons (k : Checker.Reach) (s : Stmt) (rest : List Stmt) :
    Checker.killList k (s :: rest) =
      Checker.killStmt { k with normal := Checker.reachList k rest } s ++
        Checker.killList k rest := by
  first | rfl | rw [Checker.killList] | simp only [Checker.killList]

theorem reachStmt_if (k : Checker.Reach) (c : Expr) (thn els : List Stmt) (he : Bool) :
    Checker.reachStmt k (.if_ c thn els he) =
      (Checker.reachList k thn || Checker.reachList k els) := by
  first | rfl | rw [Checker.reachStmt] | simp only [Checker.reachStmt]

theorem killStmt_if (k : Checker.Reach) (c : Expr) (thn els : List Stmt) (he : Bool) :
    Checker.killStmt k (.if_ c thn els he) = Checker.killList k thn ++ Checker.killList k els := by
  first | rfl | rw [Checker.killStmt] | simp only [Checker.killStmt]

theorem bor_true_left {a b : Bool} (h : a = true) : (a || b) = true := by
  subst h
  rfl

theorem bor_true_right {a b : Bool} (h : b = true) : (a || b) = true := by
  subst h
  cases a <;> rfl

/-! ## Identities below a counter -/

theorem ids_append {new vars : List Checker.Var} {N : Nat} (h1 : ∀ v ∈ new, v.id < N)
    (h2 : ∀ v ∈ vars, v.id < N) : ∀ v ∈ new ++ vars, v.id < N := by
  intro v hv
  rcases List.mem_append.mp hv with h | h
  · exact h1 v h
  · exact h2 v h

theorem ids_mono {vars : List Checker.Var} {N N' : Nat} (h : ∀ v ∈ vars, v.id < N)
    (hN : N ≤ N') : ∀ v ∈ vars, v.id < N' :=
  fun v hv => Nat.lt_of_lt_of_le (h v hv) hN

/-- A state's identities and its atoms' lie below the counter `n`, whatever
bindings in scope below `n` it is read with. -/
def StFresh (F : Model) (st : Checker.State) (A : List (Nat × Int × Int)) (n : Nat) : Prop :=
  F.Fresh ⟨[], st, A⟩ n

theorem StFresh.of {F : Model} (L : Laws F) {vars : List Checker.Var} {st : Checker.State}
    {A : List (Nat × Int × Int)} {n : Nat} (h : F.Fresh ⟨vars, st, A⟩ n) : StFresh F st A n :=
  L.fresh_vars h fun _ hv => by cases hv

theorem StFresh.at {F : Model} (L : Laws F) {vars : List Checker.Var} {st : Checker.State}
    {A : List (Nat × Int × Int)} {n : Nat} (h : StFresh F st A n) (hv : ∀ v ∈ vars, v.id < n) :
    F.Fresh ⟨vars, st, A⟩ n :=
  L.fresh_vars h hv

theorem StFresh.lift {F : Model} (L : Laws F) {st : Checker.State} {g g' : Checker.Gen}
    (h : StFresh F st g.atoms g.next) (hm : g.Mints g') : StFresh F st g'.atoms g'.next := by
  obtain ⟨hn, new, ha, hb⟩ := hm
  have h' := L.fresh_mono (new := new) h hn fun a ha' => (hb a ha').2.1
  unfold StFresh
  rw [ha]
  exact h'

theorem fresh_lift {F : Model} (L : Laws F) {vars : List Checker.Var} {st : Checker.State}
    {g g' : Checker.Gen} (h : F.Fresh ⟨vars, st, g.atoms⟩ g.next) (hm : g.Mints g') :
    F.Fresh ⟨vars, st, g'.atoms⟩ g'.next :=
  StFresh.at L (StFresh.lift L (StFresh.of L h) hm) (ids_mono (L.fresh_ids h) hm.1)

/-- Every state a walk leaves, by its normal edges, `break`s and `continue`s, is
fresh. -/
structure OutFresh (F : Model) (o : Checker.Out) (A : List (Nat × Int × Int)) (n : Nat) :
    Prop where
  normal : ∀ s ∈ o.normal, StFresh F s A n
  breaks : ∀ b ∈ o.breaks, StFresh F b.2 A n
  conts : ∀ b ∈ o.conts, StFresh F b.2 A n

theorem OutFresh.lift {F : Model} (L : Laws F) {o : Checker.Out} {g g' : Checker.Gen}
    (h : OutFresh F o g.atoms g.next) (hm : g.Mints g') : OutFresh F o g'.atoms g'.next :=
  ⟨fun s hs => StFresh.lift L (h.normal s hs) hm, fun b hb => StFresh.lift L (h.breaks b hb) hm,
    fun b hb => StFresh.lift L (h.conts b hb) hm⟩

/-- A statement's walk from a fresh entering state leaves only fresh states. -/
def StmtFresh (F : Model) (s : Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (pos : Pos) (g g' : Checker.Gen)
    (env' : Checker.Env) (o : Checker.Out),
    (Checker.walkStmt env st pos s).run g = .ok ((env', o), g') →
    F.Fresh ⟨env.vars, st, g.atoms⟩ g.next → OutFresh F o g'.atoms g'.next

/-- The same for a statement list. -/
def ListFresh (F : Model) (ss : List Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (blk : Pos) (i : Nat) (g g' : Checker.Gen)
    (o : Checker.Out), (Checker.walkList env st blk i ss).run g = .ok (o, g') →
    F.Fresh ⟨env.vars, st, g.atoms⟩ g.next → OutFresh F o g'.atoms g'.next

/-- A statement's walk keeps the environment but for the bindings it opens,
whose identities lie below the counter it ends with. -/
def StmtEnv (s : Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (pos : Pos) (g g' : Checker.Gen)
    (env' : Checker.Env) (o : Checker.Out),
    (Checker.walkStmt env st pos s).run g = .ok ((env', o), g') →
    ∃ new : List Checker.Var, env' = { env with vars := new ++ env.vars } ∧
      ∀ v ∈ new, v.id < g'.next

/-- A right-hand side's publication only mints. -/
theorem published_mints {env : Checker.Env} {st st1 : Checker.State} {r : Checker.Rhs}
    {g1 g2 : Checker.Gen} (hpub : Checker.Published env st r g1 st1 g2) : g1.Mints g2 := by
  rcases hpub with ⟨_, _, rfl⟩ | ⟨f, args, _, hp⟩
  · exact Checker.Gen.Mints.refl _
  · exact (Checker.publish_grows hp).1

/-- What a statement's right-hand side leaves, with a fresh entering state, is
fresh. -/
theorem published_fresh {F : Model} (L : Laws F) {env : Checker.Env} {st st1 : Checker.State}
    {r : Checker.Rhs} {g g1 g2 : Checker.Gen} {e : Expr}
    (hF : F.Fresh ⟨env.vars, st, g.atoms⟩ g.next)
    (hr : (Checker.evalRhs env st e).run g = .ok (r, g1))
    (hpub : Checker.Published env st r g1 st1 g2) : F.Fresh ⟨env.vars, st1, g2.atoms⟩ g2.next := by
  rcases hpub with ⟨_, rfl, rfl⟩ | ⟨f, args, hc, hp⟩
  · exact fresh_lift L hF (Checker.evalRhs_grows hr).1
  · exact L.publish_fresh hF hr hc hp

theorem call_cases (r : Checker.Rhs) : r.call = none ∨ ∃ fa, r.call = some fa := by
  cases r.call with
  | none => exact Or.inl rfl
  | some fa => exact Or.inr ⟨fa, rfl⟩

theorem branchState_empty (env : Checker.Env) (st : Checker.State) (b : Bool) :
    Checker.branchState env st {} b = st := rfl

/-- A condition that calls no function of the program enters its blocks with
its origins' facts. -/
theorem branchState_none {env : Checker.Env} {st : Checker.State} {r : Checker.Rhs} {c : Expr}
    {b : Bool} (h : r.call = none) :
    Checker.branchState env st
        (if r.call.isSome then {} else Checker.origins env st c r.val.gexpr) b =
      Checker.branchState env st (Checker.origins env st c r.val.gexpr) b := by
  have hc : ¬(r.call.isSome = true) := by simp [h]
  rw [ite_eq_right hc]

/-- A condition that calls a function of the program enters its blocks with
the state the call left. -/
theorem branchState_call {env : Checker.Env} {st : Checker.State} {r : Checker.Rhs} {c : Expr}
    {b : Bool} {fa : FnDecl × List (String × Checker.Val)} (h : r.call = some fa) :
    Checker.branchState env st
      (if r.call.isSome then {} else Checker.origins env st c r.val.gexpr) b = st := by
  have hc : r.call.isSome = true := by simp [h]
  rw [ite_eq_left hc, branchState_empty]

/-- The state an `if` enters either block with is fresh. -/
theorem if_entry_fresh {F : Model} (L : Laws F) {env : Checker.Env} {st st1 : Checker.State}
    {g g1 g2 : Checker.Gen} {c : Expr} {r : Checker.Rhs}
    (hF : F.Fresh ⟨env.vars, st, g.atoms⟩ g.next)
    (hr : (Checker.evalRhs env st c).run g = .ok (r, g1))
    (hpub : Checker.Published env st r g1 st1 g2) (b : Bool) :
    F.Fresh ⟨env.vars, Checker.branchState env st1
      (if r.call.isSome then {} else Checker.origins env st1 c r.val.gexpr) b, g2.atoms⟩
      g2.next := by
  rcases call_cases r with hrc | ⟨⟨f', vs⟩, hrc⟩
  · rw [branchState_none hrc]
    obtain ⟨rfl, rfl⟩ := published_none hpub hrc
    exact L.branch_fresh hF hr hrc
  · rw [branchState_call hrc]
    exact L.publish_fresh hF hr hrc (published_call hpub hrc)

/-! ## Each statement's walk: its bindings and the freshness of its exits -/

theorem env_same {env env' : Checker.Env} {N : Nat} (h : env' = env) :
    ∃ new : List Checker.Var, env' = { env with vars := new ++ env.vars } ∧ ∀ v ∈ new, v.id < N :=
  ⟨[], by subst h; rfl, fun _ hv => by cases hv⟩

theorem env_let (x : String) (e : Expr) : StmtEnv (.let_ x e) := by
  intro env st pos g g' env' o hw
  obtain ⟨r, g1, st', _, hb, henv', _⟩ := Checker.walkStmt_let_ok hw
  refine ⟨[{ name := x, id := g1.next, ty := r.val.ty }], henv', fun v hv => ?_⟩
  cases hv with
  | head => exact Nat.lt_of_lt_of_le (Nat.lt_succ_self _) (Checker.bindLet_grows hb).1.1
  | tail _ h => cases h

theorem env_setVar (x : String) (e : Expr) : StmtEnv (.set (.var x) e) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, _, _, henv', _⟩ := Checker.walkStmt_setVar_ok hw
  exact env_same henv'

theorem env_setIndex (b : String) (off : Atom) (e : Expr) : StmtEnv (.set (.index b off) e) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, _, _, _, _, _, henv', _⟩ := Checker.walkStmt_setIndex_ok hw
  exact env_same henv'

theorem env_call (e : Expr) : StmtEnv (.call e) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, henv', _⟩ := Checker.walkStmt_call_ok hw
  exact env_same henv'

theorem env_ret (e : Expr) : StmtEnv (.ret e) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, _, _, henv', _⟩ := Checker.walkStmt_ret_ok hw
  exact env_same henv'

theorem env_invariant (nm : String) (rel : Relation) : StmtEnv (.invariant nm rel) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, henv', _, _⟩ := Checker.walkStmt_invariant_ok hw
  exact env_same henv'

theorem env_brk (label : Option String) : StmtEnv (.brk label) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, henv', _, _⟩ := Checker.walkStmt_brk_ok hw
  exact env_same henv'

theorem env_cont (label : Option String) : StmtEnv (.cont label) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, henv', _, _⟩ := Checker.walkStmt_cont_ok hw
  exact env_same henv'

theorem env_if (c : Expr) (thn els : List Stmt) (he : Bool) : StmtEnv (.if_ c thn els he) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, _, _, _, _, _, _, henv', _⟩ := Checker.walkStmt_if_ok hw
  exact env_same henv'

theorem fresh_let {F : Model} (L : Laws F) (x : String) (e : Expr) : StmtFresh F (.let_ x e) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨r, g1, st', hr, hb, _, ho⟩ := Checker.walkStmt_let_ok hw
  subst o
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L (L.let_fresh hF hr hb)
  | tail _ h => cases h

theorem fresh_setVar {F : Model} (L : Laws F) (x : String) (e : Expr) :
    StmtFresh F (.set (.var x) e) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨v, r, g1, st', hv, hr, hsv, _, ho⟩ := Checker.walkStmt_setVar_ok hw
  subst o
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L (L.setVar_fresh hF hv hr hsv)
  | tail _ h => cases h

theorem fresh_setIndex {F : Model} (L : Laws F) (b : String) (off : Atom) (e : Expr) :
    StmtFresh F (.set (.index b off) e) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨v, r, g1, st1, g2, val, _, hr, hpub, hat, _, ho⟩ := Checker.walkStmt_setIndex_ok hw
  subst o
  have h1 := fresh_lift L (published_fresh L hF hr hpub) (Checker.evalAtom_grows hat).1
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L (L.write_fresh h1)
  | tail _ h => cases h

theorem fresh_call {F : Model} (L : Laws F) (e : Expr) : StmtFresh F (.call e) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨r, g1, st1, hr, hpub, _, ho⟩ := Checker.walkStmt_call_ok hw
  subst o
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L (published_fresh L hF hr hpub)
  | tail _ h => cases h

theorem fresh_ret {F : Model} (e : Expr) : StmtFresh F (.ret e) := by
  intro env st pos g g' env' o hw _
  obtain ⟨_, _, _, _, _, _, _, _, ho⟩ := Checker.walkStmt_ret_ok hw
  subst o
  exact ⟨(fun s hs => by cases hs), (fun b hb => by cases hb), fun b hb => by cases hb⟩

theorem fresh_invariant {F : Model} (L : Laws F) (nm : String) (rel : Relation) :
    StmtFresh F (.invariant nm rel) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨_, _, ho, hg'⟩ := Checker.walkStmt_invariant_ok hw
  subst o
  subst g'
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L (L.establishRel_fresh hF)
  | tail _ h => cases h

theorem fresh_brk {F : Model} (L : Laws F) (label : Option String) : StmtFresh F (.brk label) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨l, _, _, ho, hg'⟩ := Checker.walkStmt_brk_ok hw
  subst o
  subst g'
  refine ⟨(fun s hs => by cases hs), fun b hb => ?_, fun b hb => by cases hb⟩
  cases hb with
  | head => exact StFresh.of L hF
  | tail _ h => cases h

theorem fresh_cont {F : Model} (L : Laws F) (label : Option String) :
    StmtFresh F (.cont label) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨l, _, _, ho, hg'⟩ := Checker.walkStmt_cont_ok hw
  subst o
  subst g'
  refine ⟨(fun s hs => by cases hs), (fun b hb => by cases hb), fun b hb => ?_⟩
  cases hb with
  | head => exact StFresh.of L hF
  | tail _ h => cases h

theorem fresh_if {F : Model} (L : Laws F) (c : Expr) (thn els : List Stmt) (he : Bool)
    (hTF : ListFresh F thn) (hEF : ListFresh F els) : StmtFresh F (.if_ c thn els he) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨r, g1, st1, g2, ot, gT, oe, hr, hpub, hwt, hwe, _, ho⟩ := Checker.walkStmt_if_ok hw
  subst o
  have hg2 : g.Mints g2 :=
    Checker.Gen.Mints.trans (Checker.evalRhs_grows hr).1 (published_mints hpub)
  have hmT := (Checker.walkList_grows hwt).1
  have hmE := (Checker.walkList_grows hwe).1
  have hFT := if_entry_fresh L hF hr hpub true
  have hFE := fresh_lift L (if_entry_fresh L hF hr hpub false) hmT
  have hfT := OutFresh.lift L (hTF _ _ _ _ _ _ _ hwt hFT) hmE
  have hfE := hEF _ _ _ _ _ _ _ hwe hFE
  have hvars : ∀ v ∈ env.vars, v.id < g'.next :=
    ids_mono (L.fresh_ids hF) (Nat.le_trans hg2.1 (Nat.le_trans hmT.1 hmE.1))
  refine ⟨fun s hs => ?_, fun b hb => ?_, fun b hb => ?_⟩
  · obtain ⟨s0, hs0, rfl⟩ := List.mem_map.mp hs
    have h0 : StFresh F s0 g'.atoms g'.next := by
      rcases List.mem_append.mp hs0 with h | h
      · exact hfT.normal s0 h
      · exact hfE.normal s0 h
    exact StFresh.of L (L.exit_fresh (new := []) (vars := env.vars) (StFresh.at L h0 hvars))
  · rcases List.mem_append.mp hb with h | h
    · exact hfT.breaks b h
    · exact hfE.breaks b h
  · rcases List.mem_append.mp hb with h | h
    · exact hfT.conts b h
    · exact hfE.conts b h

theorem list_fresh_nil {F : Model} (L : Laws F) : ListFresh F [] := by
  intro env st blk i g g' o hw hF
  rw [Checker.walkList] at hw
  obtain ⟨rfl, rfl⟩ := Checker.run_pure_ok hw
  refine ⟨fun s hs => ?_, (fun b hb => by cases hb), fun b hb => by cases hb⟩
  cases hs with
  | head => exact StFresh.of L hF
  | tail _ h => cases h

theorem list_fresh_cons {F : Model} (L : Laws F) {s : Stmt} {rest : List Stmt}
    (hSF : StmtFresh F s) (hV : StmtEnv s) (hLF : ListFresh F rest) : ListFresh F (s :: rest) := by
  intro env st blk i g g' o hw hF
  obtain ⟨env', os, g2, hws, hnil, hcons⟩ := Checker.walkList_cons_ok hw
  obtain ⟨new, rfl, hnew⟩ := hV _ _ _ _ _ _ _ hws
  have hfo := hSF _ _ _ _ _ _ _ hws hF
  have hvars2 : ∀ v ∈ new ++ env.vars, v.id < g2.next :=
    ids_append hnew (ids_mono (L.fresh_ids hF) (Checker.walkStmt_grows hws).1.1)
  by_cases hr : rest = []
  · obtain ⟨rfl, rfl⟩ := hnil hr
    exact hfo
  · rcases hcons hr with ⟨hmn, rfl⟩ | ⟨st', g3, orest, hmerge, hwr, rfl⟩
    · exact OutFresh.lift L hfo (Checker.merge_grows hmn).1
    · have hgm := Checker.merge_grows hmerge
      have hF3 : F.Fresh ⟨new ++ env.vars, st', g3.atoms⟩ g3.next :=
        L.merge_fresh (env := { env with vars := new ++ env.vars })
          (fun s' hs' => StFresh.at L (hfo.normal s' hs') hvars2) hmerge
      have hfr := hLF { env with vars := new ++ env.vars } st' blk (i + 1) g3 g' orest hwr hF3
      have hm2' : g2.Mints g' := Checker.Gen.Mints.trans hgm.1 (Checker.walkList_grows hwr).1
      refine ⟨fun s hs => hfr.normal s hs, fun b hb => ?_, fun b hb => ?_⟩
      · rcases List.mem_append.mp hb with h | h
        · exact StFresh.lift L (hfo.breaks b h) hm2'
        · exact hfr.breaks b h
      · rcases List.mem_append.mp hb with h | h
        · exact StFresh.lift L (hfo.conts b h) hm2'
        · exact hfr.conts b h

/-! ## What a statement list carries -/

/-- The statements passed the judgments without facts, in some context. -/
def StaticListOK (p : Program) (fn : FnDecl) (ss : List Stmt) : Prop :=
  ∃ c live fl, Static.stmts p fn c live ss = .ok fl

/-- Leaving a statement list off its end: as `NormalOK` says, under the
bindings `vars` the list began with and any bindings its `let`s opened, minted
below `N`. -/
def EndOK (F : Model) (p : Program) (n : Nat) (W : List Watch) (vars : List Checker.Var)
    (A : List (Nat × Int × Int)) (N : Nat) (normal : List Checker.State) (fn : FnDecl)
    (E0 : Env) (k : List Kont) (D : Dest) (callers : List Frame) : Prop :=
  ∀ new : List Checker.Var, (∀ v ∈ new, v.id < N) →
    NormalOK F p n W (new ++ vars) A normal fn E0 k D callers

/-- A statement list's triple: from a frame where the list's entering snapshot
holds, with every enclosing loop watched, running the list is safe for one
step more than every way of leaving it. -/
def ListOK (F : Model) (p : Program) (fn : FnDecl) (ss : List Stmt) : Prop :=
  ∀ (env : Checker.Env) (st : Checker.State) (blk : Pos) (i : Nat) (g g' : Checker.Gen)
    (o : Checker.Out) (n : Nat) (W : List Watch) (E E0 : Env) (k : List Kont) (D : Dest)
    (callers : List Frame),
    EnvOK p fn env → (∀ lc ∈ env.loops, lc.id < g.next) →
    (Checker.walkList env st blk i ss).run g = .ok (o, g') →
    StaticListOK p fn ss → PtsOK p fn g' →
    F.Holds ⟨env.vars, st, g.atoms⟩ (E, E0) → F.Fresh ⟨env.vars, st, g.atoms⟩ g.next →
    (∀ w ∈ W, w.At ss E) →
    EndOK F p n W env.vars g'.atoms g'.next o.normal fn E0 k D callers →
    BreakOK F p n W env.loops env.vars g'.atoms g'.next o.breaks fn E0 k D callers →
    ContOK F p n W env.loops env.vars g'.atoms g'.next o.conts fn E0 k D callers →
    RetOK F p n fn E0 D callers → FnSafeTo F p n →
    SafeAt F p (n + 1) (cfg fn E E0 (.stmts blk i ss :: k) D callers)

theorem SafeAt.zero {F : Model} {p : Program} {c : Config} : SafeAt F p 0 c := by
  first | exact True.intro | simp only [SafeAt, Safe]

theorem prefix_mem {α : Type} {l1 l2 : List α} {a : α} (h : l1 <+: l2) (ha : a ∈ l1) :
    a ∈ l2 := by
  obtain ⟨t, rfl⟩ := h
  exact List.mem_append.mpr (Or.inl ha)

theorem PtsOK.of_prefix {p : Program} {fn : FnDecl} {g g' : Checker.Gen} (h : PtsOK p fn g')
    (hp : g.points <+: g'.points) : PtsOK p fn g := by
  obtain ⟨h1, h2⟩ := h
  exact ⟨List.IsPrefix.trans hp h1, h2⟩

theorem EndOK.mono {F : Model} {p : Program} {m n : Nat} {W : List Watch}
    {vars : List Checker.Var} {A : List (Nat × Int × Int)} {N : Nat}
    {normal : List Checker.State} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} (h : EndOK F p n W vars A N normal fn E0 k D callers) (hmn : m ≤ n) :
    EndOK F p m W vars A N normal fn E0 k D callers :=
  fun new hnew => NormalOK.mono (h new hnew) hmn

/-- Bindings opened before the list began count as the list's own. -/
theorem EndOK.pre {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {vars pre : List Checker.Var} {A : List (Nat × Int × Int)} {N : Nat}
    {normal : List Checker.State} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} (h : EndOK F p n W vars A N normal fn E0 k D callers)
    (hpre : ∀ v ∈ pre, v.id < N) : EndOK F p n W (pre ++ vars) A N normal fn E0 k D callers := by
  intro new hnew
  have h' := h (new ++ pre) (ids_append hnew hpre)
  rw [List.append_assoc] at h'
  exact h'

theorem BreakOK.pre {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars pre : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} (h : BreakOK F p n W loops vars A N breaks fn E0 k D callers)
    (hpre : ∀ v ∈ pre, v.id < N) :
    BreakOK F p n W loops (pre ++ vars) A N breaks fn E0 k D callers :=
  fun label lc s ht hs new hnew E k0 hH hW =>
    h label lc s ht hs (new ++ pre) (ids_append hnew hpre) E k0
      (by rw [List.append_assoc]; exact hH) hW

theorem ContOK.pre {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars pre : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} (h : ContOK F p n W loops vars A N conts fn E0 k D callers)
    (hpre : ∀ v ∈ pre, v.id < N) :
    ContOK F p n W loops (pre ++ vars) A N conts fn E0 k D callers :=
  fun label lc s ht hs new hnew E k0 hH hW =>
    h label lc s ht hs (new ++ pre) (ids_append hnew hpre) E k0
      (by rw [List.append_assoc]; exact hH) hW

theorem BreakOK.sub {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {b1 b2 : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} (h : BreakOK F p n W loops vars A N b2 fn E0 k D callers)
    (hsub : ∀ b ∈ b1, b ∈ b2) : BreakOK F p n W loops vars A N b1 fn E0 k D callers :=
  fun label lc s ht hs => h label lc s ht (hsub _ hs)

theorem ContOK.sub {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {b1 b2 : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} (h : ContOK F p n W loops vars A N b2 fn E0 k D callers)
    (hsub : ∀ b ∈ b1, b ∈ b2) : ContOK F p n W loops vars A N b1 fn E0 k D callers :=
  fun label lc s ht hs => h label lc s ht (hsub _ hs)

/-- An exit from a walk that ended earlier, with fewer atoms minted: its states
are fresh then, so they still hold once more are minted. -/
theorem BreakOK.lift {F : Model} {p : Program} (L : Laws F) {n m : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {g1 g2 : Checker.Gen}
    {b1 b2 : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame}
    (h : BreakOK F p n W loops vars g2.atoms g2.next b2 fn E0 k D callers) (hmn : m ≤ n)
    (hm : g1.Mints g2) (hsub : ∀ b ∈ b1, b ∈ b2)
    (hfr : ∀ b ∈ b1, StFresh F b.2 g1.atoms g1.next) (hvars : ∀ v ∈ vars, v.id < g1.next) :
    BreakOK F p m W loops vars g1.atoms g1.next b1 fn E0 k D callers :=
  fun label lc s ht hs new hnew E k0 hH hW =>
    StepOK.safeAt_mono hmn (h label lc s ht (hsub _ hs) new
      (fun v hv => Nat.lt_of_lt_of_le (hnew v hv) hm.1) E k0
      (L.lift hH (StFresh.at L (hfr _ hs) (ids_append hnew hvars)) hm) hW)

theorem ContOK.lift {F : Model} {p : Program} (L : Laws F) {n m : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {g1 g2 : Checker.Gen}
    {b1 b2 : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame}
    (h : ContOK F p n W loops vars g2.atoms g2.next b2 fn E0 k D callers) (hmn : m ≤ n)
    (hm : g1.Mints g2) (hsub : ∀ b ∈ b1, b ∈ b2)
    (hfr : ∀ b ∈ b1, StFresh F b.2 g1.atoms g1.next) (hvars : ∀ v ∈ vars, v.id < g1.next) :
    ContOK F p m W loops vars g1.atoms g1.next b1 fn E0 k D callers :=
  fun label lc s ht hs new hnew E k0 hH hW =>
    StepOK.safeAt_mono hmn (h label lc s ht (hsub _ hs) new
      (fun v hv => Nat.lt_of_lt_of_le (hnew v hv) hm.1) E k0
      (L.lift hH (StFresh.at L (hfr _ hs) (ids_append hnew hvars)) hm) hW)

/-! ## The watches of a list's head statement -/

/-- An enclosing loop seen from the head statement of a list whose rest is
`rest`: the head's continuation runs the rest first. -/
def Watch.before (rest : List Stmt) (w : Watch) : Watch :=
  { w with reach := { w.reach with normal := Checker.reachList w.reach rest } }

theorem Watch.before_find (rest : List Stmt) (w : Watch) (label : Option String) :
    (w.before rest).reach.find label = w.reach.find label := by
  cases label <;> rfl

theorem Watch.before_at {w : Watch} {s : Stmt} {rest : List Stmt} {E : Env}
    (h : w.At (s :: rest) E) : (w.before rest).AtStmt s E := by
  obtain ⟨h1, h2, h3⟩ := h
  refine ⟨h1, fun hr => h2 ?_, fun x hx => h3 x ?_⟩
  · rw [reachList_cons]
    exact hr
  · rw [killList_cons]
    exact List.mem_append.mpr (Or.inl hx)

theorem Watch.Holds.before_normal {W : List Watch} {rest : List Stmt} {E : Env}
    (h : Watch.Holds (W.map (Watch.before rest)) (·.reach.normal) E) :
    ∀ w ∈ W, w.E1.length ≤ E.length ∧ (Checker.reachList w.reach rest = true → w.Agree E) :=
  fun w hw => h (w.before rest) (List.mem_map.mpr ⟨w, hw, rfl⟩)

theorem BreakOK.before {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} {rest : List Stmt}
    (h : BreakOK F p n W loops vars A N breaks fn E0 k D callers) :
    BreakOK F p n (W.map (Watch.before rest)) loops vars A N breaks fn E0 k D callers := by
  intro label lc s ht hs new hnew E k0 hH hW
  refine h label lc s ht hs new hnew E k0 hH fun w hw => ?_
  obtain ⟨h1, h2⟩ := hW (w.before rest) (List.mem_map.mpr ⟨w, hw, rfl⟩)
  refine ⟨h1, fun hsel => h2 ?_⟩
  show ((((w.before rest).reach.find label).map (·.1)).getD false) = true
  rw [Watch.before_find]
  exact hsel

theorem ContOK.before {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {N : Nat} {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont}
    {D : Dest} {callers : List Frame} {rest : List Stmt}
    (h : ContOK F p n W loops vars A N conts fn E0 k D callers) :
    ContOK F p n (W.map (Watch.before rest)) loops vars A N conts fn E0 k D callers := by
  intro label lc s ht hs new hnew E k0 hH hW
  refine h label lc s ht hs new hnew E k0 hH fun w hw => ?_
  obtain ⟨h1, h2⟩ := hW (w.before rest) (List.mem_map.mpr ⟨w, hw, rfl⟩)
  refine ⟨h1, fun hsel => h2 ?_⟩
  show ((((w.before rest).reach.find label).map (·.2)).getD false) = true
  rw [Watch.before_find]
  exact hsel

/-! ## The list triples -/

theorem list_nil {F : Model} {p : Program} {fn : FnDecl} : ListOK F p fn [] := by
  intro env st blk i g g' o n W E E0 k D callers _ _ hw _ _ hH _ hW hN _ _ _ _
  rw [Checker.walkList] at hw
  obtain ⟨rfl, rfl⟩ := Checker.run_pure_ok hw
  refine Safe.of_next (point_stmts_nil _ _ _ _ _ _ _ _) (step_stmts_nil _ _ _ _ _ _ _ _ _) ?_
  refine hN [] (fun _ h => by cases h) st (List.Mem.head _) E hH fun w hw' => ?_
  obtain ⟨hlen, hag, _⟩ := hW w hw'
  exact ⟨hlen, fun hn => hag (by rw [reachList_nil]; exact hn)⟩

theorem list_cons {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) {s : Stmt}
    {rest : List Stmt} (hS : StmtOK F p fn s) (hV : StmtEnv s) (hSF : StmtFresh F s)
    (hL : ListOK F p fn rest) : ListOK F p fn (s :: rest) := by
  intro env st blk i g g' o n W E E0 k D callers henv hlp hw hs hpts hH hF hW hN hB hC hR hFn
  obtain ⟨env', os, g2, hws, hnil, hcons⟩ := Checker.walkList_cons_ok hw
  obtain ⟨new, rfl, hnew⟩ := hV _ _ _ _ _ _ _ hws
  obtain ⟨cx, live, fl, hst⟩ := hs
  obtain ⟨_, cs, fs, fr, hstmt, hrest⟩ := Static.stmts_cons_ok hst
  obtain ⟨hm2, L2, hp2, _⟩ := Checker.walkStmt_grows hws
  have hfo := hSF _ _ _ _ _ _ _ hws hF
  have hvars : ∀ v ∈ env.vars, v.id < g2.next := ids_mono (L.fresh_ids hF) hm2.1
  have hvars' : ∀ v ∈ new ++ env.vars, v.id < g2.next := ids_append hnew hvars
  have henv' : EnvOK p fn { env with vars := new ++ env.vars } :=
    ⟨henv.prog, henv.fn_eq, henv.accepts, henv.fn_of, henv.headers⟩
  -- What follows the statement: its generator grows to the list's, and its
  -- `break`s and `continue`s are among the list's.
  have hafter : g2.points <+: g'.points ∧ g2.Mints g' ∧ (∀ b ∈ os.breaks, b ∈ o.breaks) ∧
      (∀ b ∈ os.conts, b ∈ o.conts) := by
    by_cases hr : rest = []
    · obtain ⟨rfl, rfl⟩ := hnil hr
      exact ⟨⟨[], List.append_nil _⟩, Checker.Gen.Mints.refl _, fun _ h => h, fun _ h => h⟩
    · rcases hcons hr with ⟨hmn, rfl⟩ | ⟨st', g3, orest, hmerge, hwr, rfl⟩
      · have hgm := Checker.merge_grows hmn
        exact ⟨⟨[], by rw [List.append_nil, hgm.2]⟩, hgm.1, fun _ h => h, fun _ h => h⟩
      · have hgm := Checker.merge_grows hmerge
        obtain ⟨hm3, L3, hp3, _⟩ := Checker.walkList_grows hwr
        exact ⟨⟨L3, by rw [hp3, hgm.2]⟩, Checker.Gen.Mints.trans hgm.1 hm3,
          fun _ h => List.mem_append.mpr (Or.inl h), fun _ h => List.mem_append.mpr (Or.inl h)⟩
  obtain ⟨hpre, hm2', hsubB, hsubC⟩ := hafter
  -- The snapshot recorded at the statement's position is the one it was
  -- entered with.
  have hpt : p.factsAt (fn.name, blk ++ [i]) = ⟨env.vars, st, g.atoms⟩ := by
    obtain ⟨hpre', hnd⟩ := hpts
    refine Program.factsAt_eq henv.fn_of (prefix_mem hpre' (prefix_mem hpre ?_)) hnd
    rw [hp2]
    exact List.mem_append.mpr (Or.inl (List.mem_append.mpr (Or.inr (List.Mem.head _))))
  have hW0 : ∀ w ∈ W.map (Watch.before rest), w.AtStmt s E := by
    intro w' hw'
    obtain ⟨w, hw, rfl⟩ := List.mem_map.mp hw'
    exact Watch.before_at (hW w hw)
  have hB' : BreakOK F p n (W.map (Watch.before rest)) env.loops env.vars g2.atoms g2.next
      os.breaks fn E0 (.stmts blk (i + 1) rest :: k) D callers :=
    BreakOK.before (BreakOK.stmts (BreakOK.lift L hB (Nat.le_refl n) hm2' hsubB hfo.breaks hvars))
  have hC' : ContOK F p n (W.map (Watch.before rest)) env.loops env.vars g2.atoms g2.next
      os.conts fn E0 (.stmts blk (i + 1) rest :: k) D callers :=
    ContOK.before (ContOK.stmts (ContOK.lift L hC (Nat.le_refl n) hm2' hsubC hfo.conts hvars))
  -- Leaving the statement normally goes on with the rest of the list.
  have hN' : NormalOK F p n (W.map (Watch.before rest)) (new ++ env.vars) g2.atoms os.normal fn
      E0 (.stmts blk (i + 1) rest :: k) D callers := by
    intro s0 hs0 E' hH' hW'
    have hWr := Watch.Holds.before_normal hW'
    by_cases hr : rest = []
    · subst hr
      obtain ⟨rfl, rfl⟩ := hnil rfl
      cases n with
      | zero => exact SafeAt.zero
      | succ m =>
        refine Safe.of_next (point_stmts_nil _ _ _ _ _ _ _ _) (step_stmts_nil _ _ _ _ _ _ _ _ _) ?_
        refine SafeAt.mono (Nat.le_succ m) (hN new hnew s0 hs0 E' hH' fun w hw => ?_)
        obtain ⟨h1, h2⟩ := hWr w hw
        exact ⟨h1, fun hn => h2 (by rw [reachList_nil]; exact hn)⟩
    · rcases hcons hr with ⟨hmn, rfl⟩ | ⟨st', g3, orest, hmerge, hwr, rfl⟩
      · rw [Checker.merge_none hmn] at hs0
        cases hs0
      · cases n with
        | zero => exact SafeAt.zero
        | succ m =>
          have hgm := Checker.merge_grows hmerge
          have hfr : ∀ s' ∈ os.normal, F.Fresh ⟨new ++ env.vars, s', g2.atoms⟩ g2.next :=
            fun s' hs' => StFresh.at L (hfo.normal s' hs') hvars'
          have hH3 := L.merge_sound (env := { env with vars := new ++ env.vars }) hs0 hH' hfr
            henv.headers hmerge
          have hF3 := L.merge_fresh (env := { env with vars := new ++ env.vars }) hfr hmerge
          have hnew' : ∀ v ∈ new, v.id < g'.next := ids_mono hnew hm2'.1
          refine hL { env with vars := new ++ env.vars } st' blk (i + 1) g3 g' orest m W E' E0 k D
            callers henv'
            (fun lc hlc => Nat.lt_of_lt_of_le (hlp lc hlc) (Nat.le_trans hm2.1 hgm.1.1))
            hwr ⟨cs, fs.normal, fr, hrest⟩ hpts hH3 hF3 ?_ ?_ ?_ ?_ (hR.mono (Nat.le_succ m))
            (hFn.mono (Nat.le_succ m))
          · intro w hw
            obtain ⟨hl, hag⟩ := hWr w hw
            refine ⟨hl, hag, fun x hx => (hW w hw).2.2 x ?_⟩
            rw [killList_cons]
            exact List.mem_append.mpr (Or.inr hx)
          · exact EndOK.pre (EndOK.mono hN (Nat.le_succ m)) hnew'
          · exact BreakOK.pre (BreakOK.sub (BreakOK.mono hB (Nat.le_succ m))
              fun _ h => List.mem_append.mpr (Or.inr h)) hnew'
          · exact ContOK.pre (ContOK.sub (ContOK.mono hC (Nat.le_succ m))
              fun _ h => List.mem_append.mpr (Or.inr h)) hnew'
  refine Safe.intro ?_ ?_
  · intro pt st' hc
    rw [point_stmts_cons] at hc
    cases hc
    show F.Holds (p.factsAt (fn.name, blk ++ [i])) (E, E0)
    rw [hpt]
    exact hH
  · rw [step_stmts_cons]
    exact hS env st (blk ++ [i])
      { g with points := g.points ++ [(blk ++ [i], ⟨env.vars, st, g.atoms⟩)] } g2 _ os n
      (W.map (Watch.before rest)) E E0 _ _ D callers henv hlp hws ⟨cx, _, hstmt⟩
      (PtsOK.of_prefix hpts hpre) hH hF hW0 hN' hB' hC' hR hFn

/-! ## `if` -/

theorem stepStmt_if (p : Program) (f : Frame) (callers : List Frame) (pos : Pos) (c : Expr)
    (thn els : List Stmt) (he : Bool) (k : List Kont) :
    stepStmt p f callers pos (.if_ c thn els he) k =
      evalRhs p f callers c (.test pos thn els) k fun v =>
        Step.ofEval (branch { f with kont := k } v pos thn els) fun g =>
          .next { frames := g :: callers } := rfl

theorem branch_noStuck (f : Frame) (v : Value) (pos : Pos) (thn els : List Stmt) :
    NoStuck (branch f v pos thn els) := by
  intro s h
  cases v <;> cases h

/-- A condition that selects a block is a `Bool`; the frame goes on into the
block it selects, with a block end that restores its bindings. -/
theorem branch_ok {f f' : Frame} {v : Value} {pos : Pos} {thn els : List Stmt}
    (h : branch f v pos thn els = .ok f') :
    ∃ b, v = .bool b ∧
      f' = { f with
        kont := .stmts (pos ++ [if b then 0 else 1]) 0 (if b then thn else els) ::
          .scope f.env.length :: f.kont } := by
  cases v with
  | bool b =>
    refine ⟨b, rfl, ?_⟩
    cases h
    rfl
  | int t x => cases h
  | unit => cases h
  | array t es => cases h
  | status code => cases h

theorem stepOK_doReturn_test {P : Config → Prop} {p : Program} {v : Value} {f c : Frame}
    {callers : List Frame} {pos : Pos} {thn els : List Stmt} (hd : f.dest = .test pos thn els)
    (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true)
    (h : v.hasTy p f.fn.resultTy = true →
      StepOK P (Step.ofEval (branch c v pos thn els) fun g => .next { frames := g :: callers })) :
    StepOK P (doReturn p v (f :: c :: callers)) := by
  rw [doReturn.eq_def]
  dsimp only
  cases hty : v.hasTy p f.fn.resultTy with
  | false => exact True.intro
  | true =>
    rw [hens, hd]
    exact h hty

/-- Entering a block of an `if` is safe when the block's walk is, and leaving
the block off its end goes on as the `if` leaves normally. -/
theorem block_entry {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) {blk : List Stmt}
    (hL : ListOK F p fn blk) {env : Checker.Env} {st0 : Checker.State} {bpos : Pos}
    {gA gB g' : Checker.Gen} {ob : Checker.Out} {n : Nat} {W : List Watch} {E E0 : Env}
    {k : List Kont} {D : Dest} {callers : List Frame} {normal : List Checker.State}
    {breaks conts : List (Nat × Checker.State)}
    (henv : EnvOK p fn env) (hlp : ∀ lc ∈ env.loops, lc.id < gA.next)
    (hwb : (Checker.walkList env st0 bpos 0 blk).run gA = .ok (ob, gB))
    (hs : StaticListOK p fn blk) (hpts : PtsOK p fn gB) (hm : gB.Mints g')
    (hH : F.Holds ⟨env.vars, st0, gA.atoms⟩ (E, E0))
    (hF : F.Fresh ⟨env.vars, st0, gA.atoms⟩ gA.next) (hfo : OutFresh F ob gB.atoms gB.next)
    (hW : ∀ w ∈ W, w.At blk E)
    (hN : NormalOK F p n W env.vars g'.atoms normal fn E0 k D callers)
    (hB : BreakOK F p n W env.loops env.vars g'.atoms g'.next breaks fn E0 k D callers)
    (hC : ContOK F p n W env.loops env.vars g'.atoms g'.next conts fn E0 k D callers)
    (hR : RetOK F p n fn E0 D callers) (hFn : FnSafeTo F p n)
    (hnorm : ∀ s ∈ ob.normal, s.exit env.vars ∈ normal)
    (hbrk : ∀ b ∈ ob.breaks, b ∈ breaks) (hcnt : ∀ b ∈ ob.conts, b ∈ conts) :
    SafeAt F p n (cfg fn E E0 (.stmts bpos 0 blk :: .scope E.length :: k) D callers) := by
  cases n with
  | zero => exact SafeAt.zero
  | succ m =>
    have hmB := (Checker.walkList_grows hwb).1
    have hvars : ∀ v ∈ env.vars, v.id < gB.next := ids_mono (L.fresh_ids hF) hmB.1
    have hlen : E.length = env.vars.length := by
      have h := congrArg List.length (L.names hH)
      simp only [List.length_map] at h
      exact h
    refine hL env st0 bpos 0 gA gB ob m W E E0 (.scope E.length :: k) D callers henv hlp hwb hs
      hpts hH hF hW ?_ ?_ ?_ (hR.mono (Nat.le_succ m)) (hFn.mono (Nat.le_succ m))
    · intro new hnew s hs E' hH' hW'
      cases m with
      | zero => exact SafeAt.zero
      | succ j =>
        refine Safe.of_next (point_scope _ _ _ _ _ _ _) (step_scope _ _ _ _ _ _ _ _) ?_
        have hx := L.exit_sound hH'
        have hxf : F.Fresh ⟨env.vars, s.exit env.vars, gB.atoms⟩ gB.next :=
          L.exit_fresh (StFresh.at L (hfo.normal s hs) (ids_append hnew hvars))
        rw [← hlen] at hx
        refine SafeAt.mono (by omega) (hN _ (hnorm s hs) _ (L.lift hx hxf hm) fun w hw => ?_)
        obtain ⟨hl1, hag⟩ := hW' w hw
        obtain ⟨hl0, _, _⟩ := hW w hw
        refine ⟨?_, fun hsel => (hag hsel).restore hl0⟩
        simp only [Env.restore, List.length_drop]
        omega
    · exact BreakOK.scope (BreakOK.lift L hB (Nat.le_succ m) hm hbrk hfo.breaks hvars)
    · exact ContOK.scope (ContOK.lift L hC (Nat.le_succ m) hm hcnt hfo.conts hvars)

theorem stmt_if {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (c : Expr)
    (thn els : List Stmt) (he : Bool) (hT : ListOK F p fn thn) (hE : ListOK F p fn els)
    (hTF : ListFresh F thn) (hEF : ListFresh F els) : StmtOK F p fn (.if_ c thn els he) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv hlp hw hs hpts hH hF hW hN hB hC hR hFn
  obtain ⟨r, g1, st1, g2, ot, gT, oe, hr, hpub, hwt, hwe, henv', ho⟩ := Checker.walkStmt_if_ok hw
  subst env'
  subst o
  obtain ⟨cx, rx, hcx⟩ := hs
  obtain ⟨⟨tc, htc⟩, ⟨flT, hflT⟩, flE, hflE⟩ := Static.stmt_if_ok hcx
  have henv0 := henv
  obtain ⟨hprog, hfneq, hacc, _, _⟩ := henv
  subst hprog
  subst hfneq
  have ht : ∃ sc t, Static.exprTy env.prog sc c = .ok t := ⟨cx.scope, tc, htc⟩
  have hm02 : g.Mints g2 :=
    Checker.Gen.Mints.trans (Checker.evalRhs_grows hr).1 (published_mints hpub)
  obtain ⟨hmT, LT, hpT, _⟩ := Checker.walkList_grows hwt
  obtain ⟨hmE, LE, hpE, _⟩ := Checker.walkList_grows hwe
  have hO := if_entry_fresh L hF hr hpub
  have hlp2 : ∀ lc ∈ env.loops, lc.id < g2.next :=
    fun lc hlc => Nat.lt_of_lt_of_le (hlp lc hlc) hm02.1
  have hlpT : ∀ lc ∈ env.loops, lc.id < gT.next :=
    fun lc hlc => Nat.lt_of_lt_of_le (hlp2 lc hlc) hmT.1
  -- Every enclosing loop is watched from each block as from the `if`.
  have hWT : ∀ w ∈ W, w.At thn E := by
    intro w hw
    obtain ⟨h1, h2, h3⟩ := hW w hw
    refine ⟨h1, fun hr' => h2 ?_, fun x hx => h3 x ?_⟩
    · rw [reachStmt_if]
      exact bor_true_left hr'
    · rw [killStmt_if]
      exact List.mem_append.mpr (Or.inl hx)
  have hWE : ∀ w ∈ W, w.At els E := by
    intro w hw
    obtain ⟨h1, h2, h3⟩ := hW w hw
    refine ⟨h1, fun hr' => h2 ?_, fun x hx => h3 x ?_⟩
    · rw [reachStmt_if]
      exact bor_true_right hr'
    · rw [killStmt_if]
      exact List.mem_append.mpr (Or.inr hx)
  -- The block a condition's value selects is safe to enter from a frame where
  -- the state the walk entered it with holds.
  have hblk : ∀ b : Bool,
      F.Holds ⟨env.vars, Checker.branchState env st1
        (if r.call.isSome then {} else Checker.origins env st1 c r.val.gexpr) b, g2.atoms⟩
        (E, E0) →
      SafeAt F env.prog n (cfg env.fn E E0 (.stmts (pos ++ [if b then 0 else 1]) 0
        (if b then thn else els) :: .scope E.length :: k) D callers) := by
    intro b hHb
    cases b with
    | true =>
      exact block_entry L hT henv0 hlp2 hwt ⟨cx, true, flT, hflT⟩
        (PtsOK.of_prefix hpts ⟨LE, hpE.symm⟩) hmE hHb (hO true) (hTF _ _ _ _ _ _ _ hwt (hO true))
        hWT hN hB hC hR hFn
        (fun s hs => List.mem_map.mpr ⟨s, List.mem_append.mpr (Or.inl hs), rfl⟩)
        (fun b hb => List.mem_append.mpr (Or.inl hb)) (fun b hb => List.mem_append.mpr (Or.inl hb))
    | false =>
      have hFb := fresh_lift L (hO false) hmT
      exact block_entry L hE henv0 hlpT hwe ⟨cx, true, flE, hflE⟩ hpts
        (Checker.Gen.Mints.refl _) (L.lift hHb (hO false) hmT) hFb (hEF _ _ _ _ _ _ _ hwe hFb) hWE
        hN hB hC hR hFn
        (fun s hs => List.mem_map.mpr ⟨s, List.mem_append.mpr (Or.inr hs), rfl⟩)
        (fun b hb => List.mem_append.mpr (Or.inr hb)) (fun b hb => List.mem_append.mpr (Or.inr hb))
  rw [stepStmt_if]
  refine stepOK_evalRhs (fun path targs as g3 hg hc => ?_) (fun hnc => ?_)
  · obtain ⟨vs, hrc, _⟩ := L.evalRhs_call_some hr hg hc
    have hp := published_call hpub hrc
    refine ⟨L.callArgs_progress hH hF hr ht hg hrc, fun args hargs => ?_⟩
    refine hFn.now g3 args (.test pos thn els) _ (callee_fn hc)
      (L.callArgs_inRange hH hF hr ht hg hrc hargs)
      (L.call_requires hH hF hr ht hacc hg hrc hargs) ?_
    intro E' k' w hw hens
    refine stepOK_doReturn_test rfl hens fun hty => ?_
    have hH1 := L.publish_sound hH hF hr ht hacc hg hrc hp hargs hw hty hens
    refine stepOK_ofEval (branch_noStuck _ _ _ _ _) fun f' hbr => ?_
    obtain ⟨b, _, rfl⟩ := branch_ok hbr
    refine hblk b ?_
    -- A condition that calls a function has no origins: the block is entered
    -- with the state the callee's postconditions were published into.
    rw [branchState_call hrc]
    exact hH1
  · have hrc : r.call = none := L.evalRhs_call_none hr hnc
    obtain ⟨rfl, rfl⟩ := published_none hpub hrc
    refine ⟨L.evalRhs_progress hH hF hr ht hrc, fun v hv => ?_⟩
    refine stepOK_ofEval (branch_noStuck _ _ _ _ _) fun f' hbr => ?_
    obtain ⟨b, hvb, rfl⟩ := branch_ok hbr
    subst hvb
    refine hblk b ?_
    rw [branchState_none hrc]
    exact L.branch_sound hH hF hr ht hrc hv

end Sim

end Whitefoot
