import WhitefootProof.Interpreter
import WhitefootProof.Walk

/-!
# Points of a run and the checker's facts there

The names the soundness theorems are stated in. A point is a function's name
and a statement's position in its body (`Pos`). The checker's walk records, on
entering each statement it visits, the bindings in scope, the fact state and
the atoms minted so far (`Snapshot`); `Program.factsAt` reads that record. The
interpreter's frame names the statement it is about to run through the
position its continuation carries; `Program.Reaches` says a run from `main`
gets there with given bindings. What a snapshot's facts mean of a frame is
defined in `Holds.lean`.
-/

namespace Whitefoot

/-- The run's start: `main` entered with no argument. -/
def Program.start (p : Program) : Option Config :=
  match p.fn? "main" with
  | some f =>
    match enter p f [] .exit [] with
    | .next c => some c
    | _ => none
  | none => none

/-- The program has `main`, `main` takes no parameter, and its requirements
hold. -/
def Program.EntryHolds (p : Program) : Prop :=
  ∃ f, p.fn? "main" = some f ∧ f.params = [] ∧ allHold [] f.requires = .ok true

/-- Zero or more steps of the machine. -/
inductive Steps (p : Program) : Config → Config → Prop where
  | refl (c : Config) : Steps p c c
  | tail {c c' c'' : Config} : Steps p c c' → step p c' = .next c'' → Steps p c c''

/-- The point a configuration's running frame is about to run, with that
frame's bindings and parameter entry values. -/
def Config.point? (c : Config) : Option ((String × Pos) × (Env × Env)) :=
  match c.frames with
  | f :: _ =>
    match f.kont with
    | .stmts blk i (_ :: _) :: _ => some ((f.fn.name, blk ++ [i]), (f.env, f.entry))
    | _ => none
  | [] => none

/-- A run from `main` reaches the point with the running frame's bindings and
entry values. -/
def Program.Reaches (p : Program) (pt : String × Pos) (st : Env × Env) : Prop :=
  ∃ c0 c, p.start = some c0 ∧ Steps p c0 c ∧ c.point? = some (pt, st)

/-- The snapshots the walk of one function records, by position; none when the
checker rejects the function. -/
def Program.points (p : Program) (f : FnDecl) : List (Pos × Snapshot) :=
  match (Checker.checkFn p f).run {} with
  | .ok (_, g) => g.points
  | .error _ => []

/-- Every function's snapshots, under its name. -/
def Program.pointTable (p : Program) : List (String × List (Pos × Snapshot)) :=
  p.fns.map fun f => (f.name, p.points f)

/-- The snapshot of a point in a table. A name two functions share selects the
first, as `Program.fn?` does. -/
def snapshotIn (table : List (String × List (Pos × Snapshot))) (pt : String × Pos) :
    Option Snapshot := do
  (← table.lookup pt.1).lookup pt.2

/-- The snapshot the walk recorded at a point. -/
def Program.snapshot? (p : Program) (pt : String × Pos) : Option Snapshot :=
  snapshotIn p.pointTable pt

/-- The checker's facts at a point: its snapshot, or a contradictory state
where the walk recorded none. -/
def Program.factsAt (p : Program) (pt : String × Pos) : Snapshot :=
  (p.snapshot? pt).getD { vars := [], state := { l0 := Fact.L0.bottom }, atoms := [] }

end Whitefoot
