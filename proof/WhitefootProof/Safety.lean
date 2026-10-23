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
  obtain ⟨g0, g1, hget, h⟩ := run_bind_ok h
  obtain ⟨rfl, rfl⟩ := run_get_ok hget
  obtain ⟨_, g2, hmod, h⟩ := run_bind_ok h
  obtain rfl := run_modify_ok hmod
  obtain ⟨⟨env', os⟩, g3, hw, h⟩ := run_bind_ok h
  refine ⟨env', os, g3, hw, ?_, ?_⟩
  · intro hrest
    subst hrest
    exact run_pure_ok h
  · intro hrest
    cases rest with
    | nil => exact absurd rfl hrest
    | cons s' rest' =>
      obtain ⟨m, g4, hm, h⟩ := run_bind_ok h
      cases m with
      | none =>
        obtain ⟨rfl, rfl⟩ := run_pure_ok h
        exact Or.inl ⟨hm, rfl⟩
      | some st' =>
        obtain ⟨orest, g5, hwl, h⟩ := run_bind_ok h
        obtain ⟨rfl, rfl⟩ := run_pure_ok h
        exact Or.inr ⟨st', g4, orest, hm, hwl, rfl⟩

end Checker

end Whitefoot
