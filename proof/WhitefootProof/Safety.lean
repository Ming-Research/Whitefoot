import WhitefootProof.Points

/-!
# Safety of a run, by steps

The first phase's two theorems say that an accepted program's run never stops
stuck and that the checker's facts hold at every point the run reaches. Both
follow from one property of a configuration, counted in steps (`Safe`): the
next `n` steps from it never stop stuck, and every configuration they pass
through satisfies a given meaning of the facts at the point it is about to
run. An ill-formed halt counts as safe here; the typing theorem excludes it on
its own.

This file holds that property, its basic lemmas, the reductions of the two
theorems to it, and the lemmas that take apart a successful run of the
checker's walk. The simulation proves an accepted program's start safe for
every number of steps by following the walk statement by statement.
-/

namespace Whitefoot

/-- What one step leaves: a next configuration with the property, the run's
end, or an ill-formed halt; never a stuck halt. -/
def StepOK (P : Config → Prop) : Step → Prop
  | .next c => P c
  | .done _ => True
  | .halt (.illFormed _) => True
  | .halt (.stuck _) => False

/-- The point a configuration is about to run, if any, satisfies `H` with the
running frame's bindings and entry values. -/
def PointOK (H : String × Pos → Env × Env → Prop) (c : Config) : Prop :=
  ∀ pt st, c.point? = some (pt, st) → H pt st

/-- The next `n` steps from `c` never stop stuck, and every configuration they
pass through satisfies `H` at the point it is about to run. -/
def Safe (p : Program) (H : String × Pos → Env × Env → Prop) (n : Nat) (c : Config) : Prop :=
  match n with
  | 0 => True
  | n + 1 => PointOK H c ∧ StepOK (Safe p H n) (step p c)
termination_by structural n

theorem StepOK.mono {P Q : Config → Prop} (h : ∀ c, P c → Q c) :
    ∀ s : Step, StepOK P s → StepOK Q s
  | .next c, hs => h c hs
  | .done _, _ => True.intro
  | .halt (.illFormed _), _ => True.intro
  | .halt (.stuck _), hs => hs

theorem Safe.mono {p : Program} {H : String × Pos → Env × Env → Prop} :
    ∀ {m n : Nat} {c : Config}, m ≤ n → Safe p H n c → Safe p H m c
  | 0, _, _, _, _ => by simp only [Safe]
  | _ + 1, 0, _, hmn, _ => by omega
  | m + 1, n + 1, c, hmn, h => by
    simp only [Safe] at h ⊢
    exact ⟨h.1, StepOK.mono (fun _ hc => Safe.mono (Nat.le_of_succ_le_succ hmn) hc) _ h.2⟩

/-- A run of `n` steps from a configuration safe for `n` steps does not stop
stuck. -/
theorem Safe.runFrom_ne_stuck {p : Program} {H : String × Pos → Env × Env → Prop} :
    ∀ {n : Nat} {c : Config}, Safe p H n c → ∀ s, runFrom p n c ≠ .stuck s
  | 0, _, _, s => by simp [runFrom]
  | n + 1, c, h, s => by
    simp only [Safe] at h
    have hstep := h.2
    simp only [runFrom]
    split
    · rename_i c' heq
      rw [heq] at hstep
      exact Safe.runFrom_ne_stuck (n := n) (c := c') hstep s
    · intro h'
      cases h'
    · rename_i k heq
      rw [heq] at hstep
      simp only [StepOK] at hstep
    · intro h'
      cases h'

/-- Safety for every number of steps passes along the machine's steps. -/
theorem Safe.of_steps {p : Program} {H : String × Pos → Env × Env → Prop} {c c' : Config}
    (hs : Steps p c c') : (∀ n, Safe p H n c) → ∀ n, Safe p H n c' := by
  induction hs with
  | refl => exact fun h => h
  | tail _ hstep ih =>
    intro h n
    have h1 := ih h (n + 1)
    simp only [Safe] at h1
    have h2 := h1.2
    rw [hstep] at h2
    exact h2

/-- A configuration safe for one step or more satisfies `H` at its point. -/
theorem Safe.point {p : Program} {H : String × Pos → Env × Env → Prop} {n : Nat} {c : Config}
    {pt : String × Pos} {st : Env × Env} (h : Safe p H (n + 1) c)
    (hc : c.point? = some (pt, st)) : H pt st := by
  simp only [Safe] at h
  exact h.1 pt st hc

/-- `main` with no parameter and true requirements is entered with an empty
frame about to run its body. -/
theorem enter_main {p : Program} {f : FnDecl} (hparams : f.params = [])
    (hreq : allHold [] f.requires = .ok true) :
    enter p f [] .exit [] =
      .next { frames := [{ fn := f, env := [], entry := [], kont := [.stmts [] 0 f.body],
                           dest := .exit }] } := by
  simp [enter, hparams, hreq, Step.ofEval]

/-- A program whose entry holds starts where entering `main` leads. -/
theorem Program.EntryHolds.start {p : Program} (h : p.EntryHolds) :
    ∃ f c0, p.fn? "main" = some f ∧ enter p f [] .exit [] = .next c0 ∧ p.start = some c0 := by
  obtain ⟨f, hf, hparams, hreq⟩ := h
  refine ⟨f, _, hf, enter_main hparams hreq, ?_⟩
  simp only [Program.start, hf, enter_main hparams hreq]

theorem Program.run_eq_runFrom {p : Program} {f : FnDecl} {c0 : Config}
    (hf : p.fn? "main" = some f) (he : enter p f [] .exit [] = .next c0) (steps : Nat) :
    p.run steps = runFrom p steps c0 := by
  simp only [Program.run, hf, he]

/-- The facts hold at every point a run reaches when, under the meaning
`Holds`, the start is safe for every number of steps. -/
theorem facts_hold_of_safe {p : Program} {Holds : Snapshot → Env × Env → Prop}
    (hsafe : ∀ c0, p.start = some c0 →
      ∀ n, Safe p (fun pt st => Holds (p.factsAt pt) st) n c0) :
    ∀ pt st, p.Reaches pt st → Holds (p.factsAt pt) st := by
  intro pt st hr
  obtain ⟨c0, c, hstart, hsteps, hpt⟩ := hr
  have h := Safe.of_steps hsteps (hsafe c0 hstart)
  exact Safe.point (n := 0) (h (0 + 1)) hpt

/-- A program whose entry holds and whose start is safe for every number of
steps never stops stuck. -/
theorem never_stuck_of_safe {p : Program} {H : String × Pos → Env × Env → Prop}
    (hentry : p.EntryHolds) (hsafe : ∀ c0, p.start = some c0 → ∀ n, Safe p H n c0)
    (steps : Nat) (s : Stuck) : p.run steps ≠ .stuck s := by
  obtain ⟨f, c0, hf, he, hstart⟩ := hentry.start
  rw [Program.run_eq_runFrom hf he]
  exact Safe.runFrom_ne_stuck (hsafe c0 hstart steps) s

/-- A run that neither stops stuck nor halts ill-formed ends or runs out of
steps. -/
theorem done_or_outOfSteps {p : Program} {steps : Nat}
    (hstuck : ∀ s, p.run steps ≠ .stuck s) (hill : ∀ w, p.run steps ≠ .illFormed w) :
    (∃ v, p.run steps = .done v) ∨ p.run steps = .outOfSteps := by
  cases h : p.run steps with
  | done v => exact Or.inl ⟨v, rfl⟩
  | stuck s => exact absurd h (hstuck s)
  | illFormed w => exact absurd h (hill w)
  | outOfSteps => exact Or.inr rfl

/-- The first theorem's conclusion, given a meaning of the facts under which
the start is safe for every number of steps and the typing theorem's
exclusion of ill-formed halts. -/
theorem accepted_never_stuck_of_safe {p : Program} {Holds : Snapshot → Env × Env → Prop}
    (hentry : p.EntryHolds)
    (hsafe : ∀ c0, p.start = some c0 →
      ∀ n, Safe p (fun pt st => Holds (p.factsAt pt) st) n c0)
    (hill : ∀ steps w, p.run steps ≠ .illFormed w) (steps : Nat) :
    (∃ v, p.run steps = .done v) ∨ p.run steps = .outOfSteps :=
  done_or_outOfSteps (never_stuck_of_safe hentry hsafe steps) (hill steps)

/-! ## The machine's loop exits

A `break` or `continue` looks for its loop's marker past the block ends and
statement lists above it. -/

theorem findLoop_stmts (label : Option String) (blk : Pos) (i : Nat) (ss : List Stmt)
    (k : List Kont) : findLoop label (.stmts blk i ss :: k) = findLoop label k := rfl

theorem findLoop_scope (label : Option String) (d : Nat) (k : List Kont) :
    findLoop label (.scope d :: k) = findLoop label k := rfl

theorem stepStmt_brk (p : Program) (f : Frame) (callers : List Frame) (pos : Pos)
    (label : Option String) (k : List Kont) :
    stepStmt p f callers pos (.brk label) k = stepStmt.stepBreak f callers label k := rfl

theorem stepStmt_cont (p : Program) (f : Frame) (callers : List Frame) (pos : Pos)
    (label : Option String) (k : List Kont) :
    stepStmt p f callers pos (.cont label) k = stepStmt.stepContinue f callers label k := rfl

theorem stepBreak_stmts (f : Frame) (callers : List Frame) (label : Option String) (blk : Pos)
    (i : Nat) (ss : List Stmt) (k : List Kont) :
    stepStmt.stepBreak f callers label (.stmts blk i ss :: k) =
      stepStmt.stepBreak f callers label k := rfl

theorem stepBreak_scope (f : Frame) (callers : List Frame) (label : Option String) (d : Nat)
    (k : List Kont) :
    stepStmt.stepBreak f callers label (.scope d :: k) = stepStmt.stepBreak f callers label k :=
  rfl

theorem stepContinue_stmts (f : Frame) (callers : List Frame) (label : Option String) (blk : Pos)
    (i : Nat) (ss : List Stmt) (k : List Kont) :
    stepStmt.stepContinue f callers label (.stmts blk i ss :: k) =
      stepStmt.stepContinue f callers label k := rfl

theorem stepContinue_scope (f : Frame) (callers : List Frame) (label : Option String) (d : Nat)
    (k : List Kont) :
    stepStmt.stepContinue f callers label (.scope d :: k) =
      stepStmt.stepContinue f callers label k := rfl

/-! ## One step of the machine, by what its running frame does next

The simulation names the running frame by its parts: the function, the
bindings, the parameters' entry values, the continuation and where the result
goes. -/

/-- The configuration whose running frame runs `fn` with bindings `E`, entry
values `E0`, continuation `k` and destination `D`, above `callers`. -/
abbrev cfg (fn : FnDecl) (E E0 : Env) (k : List Kont) (D : Dest) (callers : List Frame) :
    Config :=
  { frames := { fn, env := E, entry := E0, kont := k, dest := D } :: callers }

theorem step_stmts_cons (p : Program) (fn : FnDecl) (E E0 : Env) (blk : Pos) (i : Nat)
    (s : Stmt) (ss : List Stmt) (k : List Kont) (D : Dest) (callers : List Frame) :
    step p (cfg fn E E0 (.stmts blk i (s :: ss) :: k) D callers) =
      stepStmt p { fn, env := E, entry := E0, kont := .stmts blk i (s :: ss) :: k, dest := D }
        callers (blk ++ [i]) s (.stmts blk (i + 1) ss :: k) := rfl

theorem step_stmts_nil (p : Program) (fn : FnDecl) (E E0 : Env) (blk : Pos) (i : Nat)
    (k : List Kont) (D : Dest) (callers : List Frame) :
    step p (cfg fn E E0 (.stmts blk i [] :: k) D callers) = .next (cfg fn E E0 k D callers) :=
  rfl

theorem step_scope (p : Program) (fn : FnDecl) (E E0 : Env) (d : Nat) (k : List Kont)
    (D : Dest) (callers : List Frame) :
    step p (cfg fn E E0 (.scope d :: k) D callers) =
      .next (cfg fn (E.restore d) E0 k D callers) := rfl

theorem step_loop (p : Program) (fn : FnDecl) (E E0 : Env) (l : Option String) (blk : Pos)
    (body : List Stmt) (d : Nat) (k : List Kont) (D : Dest) (callers : List Frame) :
    step p (cfg fn E E0 (.loop l blk body d :: k) D callers) =
      .next (cfg fn (E.restore d) E0 (.stmts blk 0 body :: .loop l blk body d :: k) D callers) :=
  rfl

theorem point_stmts_cons (fn : FnDecl) (E E0 : Env) (blk : Pos) (i : Nat) (s : Stmt)
    (ss : List Stmt) (k : List Kont) (D : Dest) (callers : List Frame) :
    (cfg fn E E0 (.stmts blk i (s :: ss) :: k) D callers).point? =
      some ((fn.name, blk ++ [i]), (E, E0)) := rfl

theorem point_stmts_nil (fn : FnDecl) (E E0 : Env) (blk : Pos) (i : Nat) (k : List Kont)
    (D : Dest) (callers : List Frame) :
    (cfg fn E E0 (.stmts blk i [] :: k) D callers).point? = none := rfl

theorem point_scope (fn : FnDecl) (E E0 : Env) (d : Nat) (k : List Kont) (D : Dest)
    (callers : List Frame) : (cfg fn E E0 (.scope d :: k) D callers).point? = none := rfl

theorem point_loop (fn : FnDecl) (E E0 : Env) (l : Option String) (blk : Pos)
    (body : List Stmt) (d : Nat) (k : List Kont) (D : Dest) (callers : List Frame) :
    (cfg fn E E0 (.loop l blk body d :: k) D callers).point? = none := rfl

/-- A configuration at no point satisfies any point condition. -/
theorem PointOK.of_none {H : String × Pos → Env × Env → Prop} {c : Config}
    (h : c.point? = none) : PointOK H c := by
  intro pt st hc
  rw [h] at hc
  cases hc

/-- One more step is safe when the configuration's point condition holds and
its step leads somewhere safe. -/
theorem Safe.intro {p : Program} {H : String × Pos → Env × Env → Prop} {n : Nat} {c : Config}
    (hp : PointOK H c) (hs : StepOK (Safe p H n) (step p c)) : Safe p H (n + 1) c := by
  simp only [Safe]
  exact ⟨hp, hs⟩

/-- A configuration at no point whose step leads to a configuration safe for
`n` steps is safe for `n + 1`. -/
theorem Safe.of_next {p : Program} {H : String × Pos → Env × Env → Prop} {n : Nat}
    {c c' : Config} (hp : c.point? = none) (hs : step p c = .next c') (h : Safe p H n c') :
    Safe p H (n + 1) c := by
  apply Safe.intro (PointOK.of_none hp)
  rw [hs]
  exact h

namespace Checker

/-! ## Taking apart a run of the checker

The walk runs in `M`, a state monad over the generator. A successful run of a
sequence is a successful run of its first part followed by one of the rest. -/

theorem run_bind_ok {α β : Type} {x : M α} {f : α → M β} {g g' : Gen} {b : β}
    (h : (x >>= f).run g = .ok (b, g')) :
    ∃ a g1, x.run g = .ok (a, g1) ∧ (f a).run g1 = .ok (b, g') := by
  change (x.run g >>= fun r => (f r.1).run r.2) = .ok (b, g') at h
  cases hx : x.run g with
  | error e =>
    rw [hx] at h
    cases h
  | ok r =>
    rw [hx] at h
    exact ⟨r.1, r.2, rfl, h⟩

theorem run_pure_ok {α : Type} {a b : α} {g g' : Gen} (h : (pure a : M α).run g = .ok (b, g')) :
    b = a ∧ g' = g := by
  cases h
  exact ⟨rfl, rfl⟩

theorem run_get_ok {g g0 g1 : Gen} (h : (get : M Gen).run g = .ok (g0, g1)) :
    g0 = g ∧ g1 = g := by
  cases h
  exact ⟨rfl, rfl⟩

theorem run_modify_ok {f : Gen → Gen} {g g' : Gen} {u : PUnit}
    (h : (modify f : M PUnit).run g = .ok (u, g')) : g' = f g := by
  cases h
  rfl

/-- A successful walk of a nonempty statement list records the first
statement's snapshot, walks that statement, and, when more statements follow,
walks them from the join of its normal edges, unless it has none. -/
theorem walkList_cons_ok {env : Env} {st : State} {blk : Pos} {i : Nat} {s : Stmt}
    {rest : List Stmt} {g g' : Gen} {o : Out}
    (h : (walkList env st blk i (s :: rest)).run g = .ok (o, g')) :
    ∃ env' os g2,
      (walkStmt env st (blk ++ [i]) s).run
        { g with points := g.points ++ [(blk ++ [i], { vars := env.vars, state := st, atoms := g.atoms })] } =
        .ok ((env', os), g2) ∧
      (rest = [] → o = os ∧ g' = g2) ∧
      (rest ≠ [] →
        ((merge env' os.normal).run g2 = .ok (none, g') ∧ o = os) ∨
        ∃ st' g3 orest, (merge env' os.normal).run g2 = .ok (some st', g3) ∧
          (walkList env' st' blk (i + 1) rest).run g3 = .ok (orest, g') ∧ o = os.seq orest) := by
  rw [walkList] at h
  obtain ⟨g0, g1, hget, h1⟩ := run_bind_ok h
  obtain ⟨rfl, rfl⟩ := run_get_ok hget
  obtain ⟨_, g2, hmod, h2⟩ := run_bind_ok h1
  obtain rfl := run_modify_ok hmod
  obtain ⟨⟨env', os⟩, g3, hw, h3⟩ := run_bind_ok h2
  refine ⟨env', os, g3, hw, ?_, ?_⟩
  · intro hrest
    subst hrest
    exact run_pure_ok h3
  · intro hrest
    cases rest with
    | nil => exact absurd rfl hrest
    | cons s' rest' =>
      obtain ⟨m, g4, hm, h4⟩ := run_bind_ok h3
      cases m with
      | none =>
        obtain ⟨rfl, rfl⟩ := run_pure_ok h4
        exact Or.inl ⟨hm, rfl⟩
      | some st' =>
        obtain ⟨orest, g5, hwl, h5⟩ := run_bind_ok h4
        obtain ⟨rfl, rfl⟩ := run_pure_ok h5
        exact Or.inr ⟨st', g4, orest, hm, hwl, rfl⟩

theorem run_fail_ok {α : Type} {rule msg : String} {g g' : Gen} {a : α}
    (h : (fail rule msg : M α).run g = .ok (a, g')) : False := by
  cases h

theorem run_freshId_ok {g g' : Gen} {n : Nat} (h : freshId.run g = .ok (n, g')) :
    n = g.next ∧ g' = { g with next := g.next + 1 } := by
  cases h
  exact ⟨rfl, rfl⟩

/-- A check that rejects when its condition holds, followed by the rest: a
successful run passed the check and ran the rest from the same generator. -/
theorem run_ite_fail_ok {α : Type} {c : Prop} [Decidable c] {rule msg : String}
    {K : PUnit → M α} {g g' : Gen} {a : α}
    (h : (if c then (fail rule msg >>= fun u => K u) else K PUnit.unit).run g = .ok (a, g')) :
    ¬c ∧ (K PUnit.unit).run g = .ok (a, g') := by
  by_cases hc : c
  · rw [if_pos hc] at h
    obtain ⟨_, _, hf, _⟩ := run_bind_ok h
    exact (run_fail_ok hf).elim
  · rw [if_neg hc] at h
    exact ⟨hc, h⟩

theorem bool_of_not_not {b : Bool} (h : ¬((!b) = true)) : b = true := by
  cases b
  · exact absurd rfl h
  · rfl

/-- The state a statement's right-hand side leaves for what follows: a call of
a program function publishes its callee's postconditions, any other
right-hand side leaves the state as it is. -/
def Published (env : Env) (st : State) (r : Rhs) (g : Gen) (st1 : State) (g1 : Gen) : Prop :=
  (r.call = none ∧ st1 = st ∧ g1 = g) ∨
    ∃ f args, r.call = some (f, args) ∧
      (publish env st f args none none id).run g = .ok (st1, g1)

/-! ## Taking apart one statement's walk

Each lemma reads a successful run of `walkStmt` for one kind of statement back
into the runs it is made of. A `loop` is taken apart once its head is a
function of its own. -/

theorem walkStmt_let_ok {env : Env} {st : State} {pos : Pos} {x : String} {e : Expr}
    {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.let_ x e)).run g = .ok ((env', o), g')) :
    ∃ r g1 st', (evalRhs env st e).run g = .ok (r, g1) ∧
      (bindLet env st { name := x, id := g1.next, ty := r.val.ty } r).run
        { g1 with next := g1.next + 1 } = .ok (st', g') ∧
      env' = { env with vars := { name := x, id := g1.next, ty := r.val.ty } :: env.vars } ∧
      o = { normal := [st'] } := by
  rw [walkStmt] at h
  obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
  obtain ⟨n, g2, hn, h2⟩ := run_bind_ok h1
  obtain ⟨rfl, rfl⟩ := run_freshId_ok hn
  obtain ⟨st', g3, hb, h3⟩ := run_bind_ok h2
  obtain ⟨h4, rfl⟩ := run_pure_ok h3
  exact ⟨r, g1, st', hr, hb, congrArg Prod.fst h4, congrArg Prod.snd h4⟩

theorem walkStmt_setVar_ok {env : Env} {st : State} {pos : Pos} {x : String} {e : Expr}
    {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.set (.var x) e)).run g = .ok ((env', o), g')) :
    ∃ v r g1 st', env.var? x = some v ∧ (evalRhs env st e).run g = .ok (r, g1) ∧
      (setVar env st v r).run g1 = .ok (st', g') ∧ env' = env ∧ o = { normal := [st'] } := by
  rw [walkStmt] at h
  cases hv : env.var? x with
  | none =>
    rw [hv] at h
    exact (run_fail_ok h).elim
  | some v =>
    rw [hv] at h
    obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
    obtain ⟨st', g2, hs, h2⟩ := run_bind_ok h1
    obtain ⟨h3, rfl⟩ := run_pure_ok h2
    exact ⟨v, r, g1, st', (by first | exact hv | rfl), hr, hs, congrArg Prod.fst h3,
      congrArg Prod.snd h3⟩

theorem walkStmt_setIndex_ok {env : Env} {st : State} {pos : Pos} {b : String} {off : Atom}
    {e : Expr} {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.set (.index b off) e)).run g = .ok ((env', o), g')) :
    ∃ v r g1 st1 g2 val, env.var? b = some v ∧ (evalRhs env st e).run g = .ok (r, g1) ∧
      Published env st r g1 st1 g2 ∧ (evalAtom env st1 (.index b off)).run g2 = .ok (val, g') ∧
      env' = env ∧ o = { normal := [st1.write v.id] } := by
  rw [walkStmt] at h
  cases hv : env.var? b with
  | none =>
    rw [hv] at h
    exact (run_fail_ok h).elim
  | some v =>
    rw [hv] at h
    obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
    cases hc : r.call with
    | none =>
      rw [hc] at h1
      obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
      obtain ⟨hst, hg⟩ := run_pure_ok hp
      obtain ⟨val, g3, hval, h3⟩ := run_bind_ok h2
      obtain ⟨h4, rfl⟩ := run_pure_ok h3
      exact ⟨v, r, g1, st1, g2, val, (by first | exact hv | rfl), hr, Or.inl ⟨hc, hst, hg⟩,
        hval, congrArg Prod.fst h4, congrArg Prod.snd h4⟩
    | some fa =>
      obtain ⟨f, args⟩ := fa
      rw [hc] at h1
      obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
      obtain ⟨val, g3, hval, h3⟩ := run_bind_ok h2
      obtain ⟨h4, rfl⟩ := run_pure_ok h3
      exact ⟨v, r, g1, st1, g2, val, (by first | exact hv | rfl), hr,
        Or.inr ⟨f, args, hc, hp⟩, hval, congrArg Prod.fst h4, congrArg Prod.snd h4⟩

theorem walkStmt_call_ok {env : Env} {st : State} {pos : Pos} {e : Expr} {g g' : Gen}
    {env' : Env} {o : Out} (h : (walkStmt env st pos (.call e)).run g = .ok ((env', o), g')) :
    ∃ r g1 st1, (evalRhs env st e).run g = .ok (r, g1) ∧ Published env st r g1 st1 g' ∧
      env' = env ∧ o = { normal := [st1] } := by
  rw [walkStmt] at h
  obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
  cases hc : r.call with
  | none =>
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    obtain ⟨hst, hg⟩ := run_pure_ok hp
    obtain ⟨h3, rfl⟩ := run_pure_ok h2
    exact ⟨r, g1, st1, hr, Or.inl ⟨hc, hst, hg⟩, congrArg Prod.fst h3, congrArg Prod.snd h3⟩
  | some fa =>
    obtain ⟨f, args⟩ := fa
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    obtain ⟨h3, rfl⟩ := run_pure_ok h2
    exact ⟨r, g1, st1, hr, Or.inr ⟨f, args, hc, hp⟩, congrArg Prod.fst h3,
      congrArg Prod.snd h3⟩

theorem walkStmt_ret_ok {env : Env} {st : State} {pos : Pos} {e : Expr} {g g' : Gen}
    {env' : Env} {o : Out} (h : (walkStmt env st pos (.ret e)).run g = .ok ((env', o), g')) :
    ∃ r g1 st1 g2, (evalRhs env st e).run g = .ok (r, g1) ∧ Published env st r g1 st1 g2 ∧
      (checkReturn env st1 r.val).run g2 = .ok ((), g') ∧ env' = env ∧ o = {} := by
  rw [walkStmt] at h
  obtain ⟨r, g1, hr, h1⟩ := run_bind_ok h
  cases hc : r.call with
  | none =>
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    obtain ⟨hst, hg⟩ := run_pure_ok hp
    obtain ⟨⟨⟩, g3, hcr, h3⟩ := run_bind_ok h2
    obtain ⟨h4, rfl⟩ := run_pure_ok h3
    exact ⟨r, g1, st1, g2, hr, Or.inl ⟨hc, hst, hg⟩, hcr, congrArg Prod.fst h4,
      congrArg Prod.snd h4⟩
  | some fa =>
    obtain ⟨f, args⟩ := fa
    rw [hc] at h1
    obtain ⟨st1, g2, hp, h2⟩ := run_bind_ok h1
    obtain ⟨⟨⟩, g3, hcr, h3⟩ := run_bind_ok h2
    obtain ⟨h4, rfl⟩ := run_pure_ok h3
    exact ⟨r, g1, st1, g2, hr, Or.inr ⟨f, args, hc, hp⟩, hcr, congrArg Prod.fst h4,
      congrArg Prod.snd h4⟩

theorem walkStmt_brk_ok {env : Env} {st : State} {pos : Pos} {label : Option String}
    {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.brk label)).run g = .ok ((env', o), g')) :
    ∃ l, target env.loops label = some l ∧ env' = env ∧ o = { breaks := [(l.id, st)] } ∧
      g' = g := by
  rw [walkStmt] at h
  cases ht : target env.loops label with
  | none =>
    rw [ht] at h
    exact (run_fail_ok h).elim
  | some l =>
    rw [ht] at h
    obtain ⟨h1, rfl⟩ := run_pure_ok h
    exact ⟨l, (by first | exact ht | rfl), congrArg Prod.fst h1, congrArg Prod.snd h1, rfl⟩

theorem walkStmt_cont_ok {env : Env} {st : State} {pos : Pos} {label : Option String}
    {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.cont label)).run g = .ok ((env', o), g')) :
    ∃ l, target env.loops label = some l ∧ env' = env ∧ o = { conts := [(l.id, st)] } ∧
      g' = g := by
  rw [walkStmt] at h
  cases ht : target env.loops label with
  | none =>
    rw [ht] at h
    exact (run_fail_ok h).elim
  | some l =>
    rw [ht] at h
    obtain ⟨h1, rfl⟩ := run_pure_ok h
    exact ⟨l, (by first | exact ht | rfl), congrArg Prod.fst h1, congrArg Prod.snd h1, rfl⟩

theorem walkStmt_invariant_ok {env : Env} {st : State} {pos : Pos} {n : String} {rel : Relation}
    {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.invariant n rel)).run g = .ok ((env', o), g')) :
    relHolds env g st rel = true ∧ env' = env ∧ o = { normal := [establishRel env st rel] } ∧
      g' = g := by
  rw [walkStmt] at h
  obtain ⟨g0, g1, hget, h1⟩ := run_bind_ok h
  obtain ⟨rfl, rfl⟩ := run_get_ok hget
  obtain ⟨hc, h2⟩ := run_ite_fail_ok h1
  obtain ⟨h3, rfl⟩ := run_pure_ok h2
  exact ⟨bool_of_not_not hc, congrArg Prod.fst h3, congrArg Prod.snd h3, rfl⟩

end Checker

namespace Static

/-! ## Taking apart a run of the static judgment

The statement judgment runs in `Judge`, an exception monad. The simulation
needs from it what each statement's right-hand side, target and relations
passed, and that the blocks a statement holds passed too. -/

theorem bind_ok {α β : Type} {x : Judge α} {f : α → Judge β} {b : β}
    (h : (x >>= f) = .ok b) : ∃ a, x = .ok a ∧ f a = .ok b := by
  cases x with
  | error e => cases h
  | ok a => exact ⟨a, rfl, h⟩

theorem reject_ok {α : Type} {rule msg : String} {a : α}
    (h : (rejectBy rule msg : Judge α) = .ok a) : False := by
  cases h

/-- A check that rejects when its condition holds, followed by the rest. -/
theorem ite_reject_ok {α : Type} {c : Prop} [Decidable c] {rule msg : String}
    {K : PUnit → Judge α} {a : α}
    (h : (if c then (rejectBy rule msg >>= fun u => K u) else K PUnit.unit) = .ok a) :
    ¬c ∧ K PUnit.unit = .ok a := by
  by_cases hc : c
  · rw [if_pos hc] at h
    obtain ⟨_, hr, _⟩ := bind_ok h
    exact (reject_ok hr).elim
  · rw [if_neg hc] at h
    exact ⟨hc, h⟩

theorem stmts_cons_ok {p : Program} {f : FnDecl} {c : Ctx} {live : Bool} {s : Stmt}
    {rest : List Stmt} {fl : Flow} (h : stmts p f c live (s :: rest) = .ok fl) :
    live = true ∧
      ∃ c' fs fr, stmt p f c s = .ok (c', fs) ∧ stmts p f c' fs.normal rest = .ok fr := by
  rw [stmts] at h
  obtain ⟨hl, h1⟩ := ite_reject_ok h
  obtain ⟨⟨c', fs⟩, hs, h2⟩ := bind_ok h1
  obtain ⟨fr, hr, _⟩ := bind_ok h2
  exact ⟨Checker.bool_of_not_not hl, c', fs, fr, hs, hr⟩

theorem stmt_let_ok {p : Program} {f : FnDecl} {c : Ctx} {x : String} {e : Expr}
    {r : Ctx × Flow} (h : stmt p f c (.let_ x e) = .ok r) : ∃ t, exprTy p c.scope e = .ok t := by
  rw [stmt] at h
  obtain ⟨_, _, h1⟩ := bind_ok h
  obtain ⟨t, ht, _⟩ := bind_ok h1
  exact ⟨t, ht⟩

theorem stmt_setVar_ok {p : Program} {f : FnDecl} {c : Ctx} {x : String} {e : Expr}
    {r : Ctx × Flow} (h : stmt p f c (.set (.var x) e) = .ok r) :
    ∃ t, exprTy p c.scope e = .ok t := by
  rw [stmt] at h
  obtain ⟨_, h1⟩ := ite_reject_ok h
  obtain ⟨_, _, h2⟩ := bind_ok h1
  obtain ⟨t, ht, _⟩ := bind_ok h2
  exact ⟨t, ht⟩

theorem stmt_setIndex_ok {p : Program} {f : FnDecl} {c : Ctx} {b : String} {off : Atom}
    {e : Expr} {r : Ctx × Flow} (h : stmt p f c (.set (.index b off) e) = .ok r) :
    (∃ t, atomTy c.scope (.index b off) = .ok t) ∧ ∃ t, exprTy p c.scope e = .ok t := by
  rw [stmt] at h
  obtain ⟨_, h1⟩ := ite_reject_ok h
  obtain ⟨ta, hta, h2⟩ := bind_ok h1
  obtain ⟨t, ht, _⟩ := bind_ok h2
  exact ⟨⟨ta, hta⟩, t, ht⟩

/-- An expression statement that is not a call is rejected. -/
theorem stmt_call_other {p : Program} {f : FnDecl} {c : Ctx} {e : Expr} {r : Ctx × Flow}
    (h : stmt p f c (.call e) = .ok r)
    (he : ∀ callee targs args, e = .call callee targs args → False) : False := by
  rw [stmt] at h
  all_goals first
    | exact he
    | exact reject_ok h
    | obtain ⟨_, hr, _⟩ := bind_ok h; exact reject_ok hr

theorem stmt_call_ok {p : Program} {f : FnDecl} {c : Ctx} {e : Expr} {r : Ctx × Flow}
    (h : stmt p f c (.call e) = .ok r) : ∃ t, exprTy p c.scope e = .ok t := by
  cases e with
  | call callee targs args =>
    rw [stmt] at h
    obtain ⟨_, hd, _⟩ := bind_ok h
    cases ht : exprTy p c.scope (.call callee targs args) with
    | error err =>
      rw [ht] at hd
      cases hd
    | ok t => exact ⟨t, rfl⟩
  | atom a => exact (stmt_call_other h fun _ _ _ he => by cases he).elim
  | infix l o r' => exact (stmt_call_other h fun _ _ _ he => by cases he).elim
  | op o args => exact (stmt_call_other h fun _ _ _ he => by cases he).elim
  | bool b => exact (stmt_call_other h fun _ _ _ he => by cases he).elim

theorem stmt_ret_ok {p : Program} {f : FnDecl} {c : Ctx} {e : Expr} {r : Ctx × Flow}
    (h : stmt p f c (.ret e) = .ok r) : ∃ t, exprTy p c.scope e = .ok t := by
  rw [stmt] at h
  obtain ⟨t, ht, _⟩ := bind_ok h
  exact ⟨t, ht⟩

theorem stmt_if_ok {p : Program} {f : FnDecl} {c : Ctx} {cond : Expr} {thn els : List Stmt}
    {he : Bool} {r : Ctx × Flow} (h : stmt p f c (.if_ cond thn els he) = .ok r) :
    (∃ t, exprTy p c.scope cond = .ok t) ∧ (∃ fl, stmts p f c true thn = .ok fl) ∧
      ∃ fl, stmts p f c true els = .ok fl := by
  rw [stmt] at h
  obtain ⟨tc, htc, h1⟩ := bind_ok h
  obtain ⟨_, h2⟩ := ite_reject_ok h1
  obtain ⟨a, ha, h3⟩ := bind_ok h2
  obtain ⟨b, hb, _⟩ := bind_ok h3
  exact ⟨⟨tc, htc⟩, ⟨a, ha⟩, b, hb⟩

theorem stmt_invariant_ok {p : Program} {f : FnDecl} {c : Ctx} {n : String} {rel : Relation}
    {r : Ctx × Flow} (h : stmt p f c (.invariant n rel) = .ok r) :
    invariantOk c.scope rel = .ok () := by
  rw [stmt] at h
  obtain ⟨_, h1⟩ := ite_reject_ok h
  obtain ⟨⟨⟩, hi, _⟩ := bind_ok h1
  exact hi

end Static

/-! ## The snapshot the walk recorded at a point -/

theorem lookup_of_mem_nodup {β : Type} :
    ∀ {T : List (Pos × β)} {q : Pos} {b : β}, (q, b) ∈ T → (T.map (·.1)).Nodup →
      T.lookup q = some b
  | [], _, _, hm, _ => by cases hm
  | (q', b') :: T, q, b, hm, hnd => by
    have hnd' : List.Pairwise (fun a c => a ≠ c) (q' :: T.map (·.1)) := hnd
    cases hnd' with
    | cons h1 h2 =>
      cases hm with
      | head => simp only [List.lookup, beq_self_eq_true]
      | tail _ hm' =>
        cases hq : q == q' with
        | true =>
          exact absurd (beq_iff_eq.mp hq).symm (h1 q (List.mem_map.mpr ⟨(q, b), hm', rfl⟩))
        | false =>
          simp only [List.lookup, hq]
          exact lookup_of_mem_nodup hm' h2

/-- The first entry a name selects in a list keyed by it. -/
theorem lookup_map_of_find? {α β : Type} {key : α → String} {val : α → β} {n : String} :
    ∀ {l : List α} {a : α}, l.find? (fun x => key x == n) = some a →
      (l.map fun x => (key x, val x)).lookup n = some (val a)
  | [], _, hf => by cases hf
  | x :: xs, a, hf => by
    cases hx : key x == n with
    | true =>
      have hn : (n == key x) = true := beq_iff_eq.mpr (beq_iff_eq.mp hx).symm
      have h1 : (x :: xs).find? (fun x => key x == n) = some x := by
        simp only [List.find?, hx]
      rw [h1] at hf
      cases hf
      simp only [List.map, List.lookup, hn]
    | false =>
      have h1 : (x :: xs).find? (fun x => key x == n) = xs.find? (fun x => key x == n) := by
        simp only [List.find?, hx]
      rw [h1] at hf
      have hn : (n == key x) = false := by
        cases hn' : n == key x with
        | false => rfl
        | true =>
          rw [beq_iff_eq.mpr (beq_iff_eq.mp hn').symm] at hx
          cases hx
      simp only [List.map, List.lookup, hn]
      exact lookup_map_of_find? hf

/-- In a function the program names by its name, a position recorded once in
the function's table reads back the snapshot recorded there. -/
theorem Program.factsAt_eq {p : Program} {fn : FnDecl} {pos : Pos} {snap : Snapshot}
    (hfn : p.fn? fn.name = some fn) (hm : (pos, snap) ∈ p.points fn)
    (hnd : ((p.points fn).map (·.1)).Nodup) : p.factsAt (fn.name, pos) = snap := by
  have h1 : p.pointTable.lookup fn.name = some (p.points fn) := lookup_map_of_find? hfn
  have h2 : (p.points fn).lookup pos = some snap := lookup_of_mem_nodup hm hnd
  have h3 : p.snapshot? (fn.name, pos) = some snap := by
    show (p.pointTable.lookup fn.name).bind (fun t => t.lookup pos) = some snap
    rw [h1]
    exact h2
  unfold Program.factsAt
  rw [h3]
  all_goals rfl

/-! ## What the simulation carries

The machine runs a function's body as the checker walked it. A statement list
is safe to run from a frame on which its entry snapshot holds when every way of
leaving it is safe: falling off its end, a `break` or `continue` to an
enclosing loop, and a return. Each way is a premise stated at a step count
`n`; the list itself is then safe for `n + 1` steps, since leaving it takes at
least one. Leaving to a loop's next iteration needs, besides the facts, that
the frame still agrees with the loop's entry frame on every binding the loop's
head did not kill: a `Watch` per enclosing loop carries that. -/

namespace Sim

/-- What the simulation needs from the meaning of facts, which the other parts
of the proof define: a snapshot holds on a frame's bindings and parameter entry
values, a snapshot's identities all lie below a counter, and a value lies in
its type's range. -/
structure Model where
  Holds : Snapshot → Env × Env → Prop
  Fresh : Snapshot → Nat → Prop
  InRange : Value → Prop

/-- Safety for `n` steps under the facts the checker recorded, read through a
meaning. -/
abbrev SafeAt (F : Model) (p : Program) (n : Nat) (c : Config) : Prop :=
  Safe p (fun pt st => F.Holds (p.factsAt pt) st) n c

/-- Returning, from a frame of `fn` with entry values `E0` and destination
`D`, a value in range whose postconditions hold is safe for `n` steps. -/
def RetOK (F : Model) (p : Program) (n : Nat) (fn : FnDecl) (E0 : Env) (D : Dest)
    (callers : List Frame) : Prop :=
  ∀ (E : Env) (k : List Kont) (v : Value), F.InRange v →
    allHold ((fn.result, v) :: E0) fn.ensures = .ok true →
    StepOK (SafeAt F p n) (doReturn p v ({ fn, env := E, entry := E0, kont := k, dest := D } :: callers))

/-- An enclosing loop seen from a point of its body: the frame `E1` the loop
was entered with, the names its head kills, the bindings in scope at the loop,
and how the point's continuation reaches the loop's next iteration head. -/
structure Watch where
  E1 : Env
  killed : List String
  vars : List Checker.Var
  reach : Checker.Reach

/-- A frame agrees with the loop's entry frame on the bindings that were in
scope at the loop, but for the newest binding of a killed name. -/
def Watch.Agree (w : Watch) (E : Env) : Prop :=
  (E.restore w.E1.length).map (·.1) = w.E1.map (·.1) ∧
  ∀ j (h : j < (E.restore w.E1.length).length) (h1 : j < w.E1.length),
    (E.restore w.E1.length)[j].2 = w.E1[j].2 ∨
      ∃ x ∈ w.killed, w.vars.findIdx? (·.name == x) = some j

/-- At a point whose block continues with `ss`: if `ss` can reach the loop's
head the frame agrees, and every name `ss` sets on a way to the head is killed. -/
def Watch.At (w : Watch) (ss : List Stmt) (E : Env) : Prop :=
  (Checker.reachList w.reach ss = true → w.Agree E) ∧
  ∀ x ∈ Checker.killList w.reach ss, x ∈ w.killed

/-- The same at one statement, whose continuation `w.reach` describes. -/
def Watch.AtStmt (w : Watch) (s : Stmt) (E : Env) : Prop :=
  (Checker.reachStmt w.reach s = true → w.Agree E) ∧
  ∀ x ∈ Checker.killStmt w.reach s, x ∈ w.killed

/-- Leaving off the end, with bindings `vars` in the walk's view and the
atoms `A` minted when the walk of what is left ended: from each normal state
that holds, with the frame agreeing wherever the end reaches a loop's head,
the continuation `k` is safe for `n` steps. -/
def NormalOK (F : Model) (p : Program) (n : Nat) (W : List Watch) (vars : List Checker.Var)
    (A : List (Nat × Int × Int)) (normal : List Checker.State) (fn : FnDecl) (E0 : Env)
    (k : List Kont) (D : Dest) (callers : List Frame) : Prop :=
  ∀ s ∈ normal, ∀ E : Env, F.Holds ⟨vars, s, A⟩ (E, E0) →
    (∀ w ∈ W, w.reach.normal = true → w.Agree E) →
    SafeAt F p n (cfg fn E E0 k D callers)

/-- Leaving by a `break` to the loop of `loops` its label resolves to, from
inside any number of nested blocks: the step is safe from each such state that
holds, with the frame agreeing wherever the `break` reaches a loop's head. -/
def BreakOK (F : Model) (p : Program) (n : Nat) (W : List Watch) (loops : List Checker.LoopCtx)
    (vars : List Checker.Var) (A : List (Nat × Int × Int)) (breaks : List (Nat × Checker.State))
    (fn : FnDecl) (E0 : Env) (k : List Kont) (D : Dest) (callers : List Frame) : Prop :=
  ∀ (label : Option String) (lc : Checker.LoopCtx) (s : Checker.State),
    Checker.target loops label = some lc → (lc.id, s) ∈ breaks →
    ∀ (new : List Checker.Var) (E : Env) (k0 : List Kont), F.Holds ⟨new ++ vars, s, A⟩ (E, E0) →
    (∀ w ∈ W, ((w.reach.find label).map (·.1)).getD false = true → w.Agree E) →
    StepOK (SafeAt F p n)
      (stepStmt.stepBreak { fn, env := E, entry := E0, kont := k0, dest := D } callers label k)

/-- Leaving by a `continue`, as `BreakOK` says for a `break`. -/
def ContOK (F : Model) (p : Program) (n : Nat) (W : List Watch) (loops : List Checker.LoopCtx)
    (vars : List Checker.Var) (A : List (Nat × Int × Int)) (conts : List (Nat × Checker.State))
    (fn : FnDecl) (E0 : Env) (k : List Kont) (D : Dest) (callers : List Frame) : Prop :=
  ∀ (label : Option String) (lc : Checker.LoopCtx) (s : Checker.State),
    Checker.target loops label = some lc → (lc.id, s) ∈ conts →
    ∀ (new : List Checker.Var) (E : Env) (k0 : List Kont), F.Holds ⟨new ++ vars, s, A⟩ (E, E0) →
    (∀ w ∈ W, ((w.reach.find label).map (·.2)).getD false = true → w.Agree E) →
    StepOK (SafeAt F p n)
      (stepStmt.stepContinue { fn, env := E, entry := E0, kont := k0, dest := D } callers label k)

/-- Entering a function of the program from a call, with arguments in range
whose requirements hold once their names are the parameters', from any caller
whose return is safe, is safe for `n` steps. -/
def FnSafe (F : Model) (p : Program) (n : Nat) : Prop :=
  ∀ (g : FnDecl) (args : List (String × Value)) (D : Dest) (callers : List Frame),
    p.fn? g.name = some g → (∀ a ∈ args, F.InRange a.2) →
    (args.map (·.1) = g.params.map (·.name) → allHold args.reverse g.requires = .ok true) →
    RetOK F p n g args.reverse D callers →
    StepOK (SafeAt F p n) (enter p g args D callers)

theorem SafeAt.mono {F : Model} {p : Program} {m n : Nat} {c : Config} (hmn : m ≤ n)
    (h : SafeAt F p n c) : SafeAt F p m c :=
  Safe.mono hmn h

theorem StepOK.safeAt_mono {F : Model} {p : Program} {m n : Nat} {s : Step} (hmn : m ≤ n)
    (h : StepOK (SafeAt F p n) s) : StepOK (SafeAt F p m) s :=
  StepOK.mono (fun _ hc => SafeAt.mono hmn hc) s h

theorem RetOK.mono {F : Model} {p : Program} {m n : Nat} {fn : FnDecl} {E0 : Env} {D : Dest}
    {callers : List Frame} (h : RetOK F p n fn E0 D callers) (hmn : m ≤ n) :
    RetOK F p m fn E0 D callers :=
  fun E k v hv hens => StepOK.safeAt_mono hmn (h E k v hv hens)

theorem NormalOK.mono {F : Model} {p : Program} {m n : Nat} {W : List Watch}
    {vars : List Checker.Var} {A : List (Nat × Int × Int)} {normal : List Checker.State}
    {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest} {callers : List Frame}
    (h : NormalOK F p n W vars A normal fn E0 k D callers) (hmn : m ≤ n) :
    NormalOK F p m W vars A normal fn E0 k D callers :=
  fun s hs E hH hW => SafeAt.mono hmn (h s hs E hH hW)

theorem BreakOK.mono {F : Model} {p : Program} {m n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} (h : BreakOK F p n W loops vars A breaks fn E0 k D callers)
    (hmn : m ≤ n) : BreakOK F p m W loops vars A breaks fn E0 k D callers :=
  fun label lc s ht hs new E k0 hH hW => StepOK.safeAt_mono hmn (h label lc s ht hs new E k0 hH hW)

theorem ContOK.mono {F : Model} {p : Program} {m n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} (h : ContOK F p n W loops vars A conts fn E0 k D callers)
    (hmn : m ≤ n) : ContOK F p m W loops vars A conts fn E0 k D callers :=
  fun label lc s ht hs new E k0 hH hW => StepOK.safeAt_mono hmn (h label lc s ht hs new E k0 hH hW)

/-- A `break` or `continue` passes over the end of a statement list or a
block: what it is safe to leave to is the same past either. -/
theorem BreakOK.stmts {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} {blk : Pos} {i : Nat} {ss : List Stmt}
    (h : BreakOK F p n W loops vars A breaks fn E0 k D callers) :
    BreakOK F p n W loops vars A breaks fn E0 (.stmts blk i ss :: k) D callers :=
  fun label lc s ht hs new E k0 hH hW => by
    rw [stepBreak_stmts]
    exact h label lc s ht hs new E k0 hH hW

theorem BreakOK.scope {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} {d : Nat}
    (h : BreakOK F p n W loops vars A breaks fn E0 k D callers) :
    BreakOK F p n W loops vars A breaks fn E0 (.scope d :: k) D callers :=
  fun label lc s ht hs new E k0 hH hW => by
    rw [stepBreak_scope]
    exact h label lc s ht hs new E k0 hH hW

theorem ContOK.stmts {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} {blk : Pos} {i : Nat} {ss : List Stmt}
    (h : ContOK F p n W loops vars A conts fn E0 k D callers) :
    ContOK F p n W loops vars A conts fn E0 (.stmts blk i ss :: k) D callers :=
  fun label lc s ht hs new E k0 hH hW => by
    rw [stepContinue_stmts]
    exact h label lc s ht hs new E k0 hH hW

theorem ContOK.scope {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {loops : List Checker.LoopCtx} {vars : List Checker.Var} {A : List (Nat × Int × Int)}
    {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} {d : Nat}
    (h : ContOK F p n W loops vars A conts fn E0 k D callers) :
    ContOK F p n W loops vars A conts fn E0 (.scope d :: k) D callers :=
  fun label lc s ht hs new E k0 hH hW => by
    rw [stepContinue_scope]
    exact h label lc s ht hs new E k0 hH hW

end Sim

end Whitefoot
