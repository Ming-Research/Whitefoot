import Lean.Data.Json
import WhitefootProof

/-!
`lake exe conformance`, run from this directory, compares the model with the
conformance cases (`../tests/conformance/manifest.jsonl`). Every case is
parsed. A case the parser stops at with `outside` uses a construct outside the
fragment and is only counted. Every `accept` and `run` case written in the
fragment must parse and resolve, and every `run` case must run to `done` with
its expected exit status, `exit_status`'s code; the checker must accept every
`accept` and `run` case. A `reject` case whose expected rule `coverage.txt`
covers must be rejected, by the parser or by the checker; one whose rule is not
covered is only counted. Each rejection is printed beside the rule the case
cites when the two differ.

`fragment-cases.txt` lists every case the parser reads to a program or to a
rejection, so a case the parser wrongly puts outside the fragment, or a
listed case missing from the manifest, is a disagreement too; a case newly in
the fragment is added to the list. A few programs the cases do not run are
run here with their outcomes, and a few programs no case exercises are
checked here with their expected judgments, one of them a tree the parser
could not produce. Each disagreement is printed, and the check fails while one
remains: either the model is not faithful or the case shows a defect of the
rules.

Every case the checker accepts is also run from `main`, and at each point the
run reaches the checker's walk must have recorded a snapshot whose bindings
have the running frame's names, in its order: the interpreter and the walk
then agree on every statement's position and scope.
-/

open Whitefoot

/-- The step budget of one run. -/
def budget : Nat := 10000000

structure Counts where
  total : Nat := 0
  outside : Nat := 0
  rejected : Nat := 0
  parsed : Nat := 0
  ran : Nat := 0
  accepted : Nat := 0
  compared : Nat := 0
  checked : Nat := 0

def Counts.line (kind : String) (c : Counts) : String :=
  s!"{kind}: {c.total} cases, {c.parsed} in the fragment, {c.outside} outside it, " ++
    s!"{c.rejected} rejected by the parser" ++
    (if kind == "run" then s!", {c.ran} run to their expected status" else "") ++
    (if kind == "reject" then
      s!"; {c.compared} cite a covered rule, and {c.checked} of those are rejected"
    else s!", {c.accepted} accepted by the checker")

/-- Programs the run cases do not exercise, each with a test of its outcome. -/
def examples : List (String × String × (Outcome → Bool)) := [
  ("a call in an if condition", "fn ready(x: u64) -> result: Bool pure {
  return x < 8_u64;
}
fn main() -> status: std::process::ExitStatus pure {
  if ready(x: 9_u64) {
    return std::process::exit_status(code: 1_u8);
  }
  if ready(x: 1_u64) {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 2_u8);
}
", (· == .done (.status 0))),
  ("an unresolved name", "fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: missing);
}
", fun | .illFormed _ => true | _ => false)
]

/-- Programs no conformance case exercises, each with the rule the checker
must reject it citing, or `none` when it must accept it. -/
def checkerExamples : List (String × String × Option String) := [
  -- The direct goal `band(a, a)` reaches its comparison through two links;
  -- the expanded goal proves it too, so the condition is always true [OP-5].
  ("a condition true through two definition links", "fn f(x: u8) -> result: u8 pure {
  if x < 3_u8 {
    let p = x < 5_u8;
    let a = band(p, p);
    if band(a, a) {
      return 1_u8;
    }
    return 0_u8;
  }
  return 2_u8;
}
fn main() -> status: std::process::ExitStatus pure {
  let r = f(x: 1_u8);
  return std::process::exit_status(code: r);
}
", some "OP-5"),
  -- The postcondition names the result only in `r - r`, which cancels: its
  -- normalized relation terms carry no result datum, so it is rejected
  -- [FN-9]. Accepted, the published disequality of the call datum with
  -- itself would hold on every input, survive the join, and give `y0` the
  -- bound 254 on the else path too, where `y0 + 1_u8` overflows.
  ("a postcondition whose result cancels", "fn g(v: u8) -> r: u8 pure contract {
  ensures r - r + v != v + 1_u8;
} {
  return v;
}
fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 1>(value: 255_u8);
  let y = a[0_u64];
  let y0 = y;
  if y < 255_u8 {
    set y = y + 1_u8;
    let w = g(v: y);
    set y = w;
  }
  let z = y0 + 1_u8;
  return std::process::exit_status(code: z);
}
", some "FN-9")
]

/-- Trees the parser cannot produce, each with the rule the checker must
reject it citing. -/
def builtExamples : List (String × Program × String) := [
  -- `300_u8` lies outside `u8` [FORM-7]; the parser stops at its spelling.
  ("a literal outside its type", {
    aliases := [],
    fns := [{
      name := "main", params := [], result := "status",
      resultTy := .named ["std", "process", "ExitStatus"],
      contract := false, requires := [], ensures := [],
      body := [.ret (.call ["std", "process", "exit_status"] [] [("code", .int .u8 300)])] }] },
    "FORM-7")
]

/-- Run a program from `main` for at most `steps` steps and check every point
it reaches: the walk recorded a snapshot there, and the snapshot's bindings
are the frame's, by name and in order. Returns the first point that fails. -/
def checkPoints (p : Program) (steps : Nat) : Option String := Id.run do
  let table := p.pointTable
  let some c0 := p.start | return none
  let mut c := c0
  for _ in [0:steps] do
    if let some ((fn, pos), (env, _)) := c.point? then
      match snapshotIn table (fn, pos) with
      | none => return some s!"no snapshot at {fn} {pos}"
      | some snap =>
        let walked := snap.vars.map (·.name)
        let running := env.map (·.1)
        if walked != running then
          return some s!"at {fn} {pos} the walk has {walked} in scope and the frame {running}"
    match step p c with
    | .next c' => c := c'
    | _ => return none
  return none

def describe : Outcome → String
  | .done (.status c) => s!"done with exit status {c}"
  | .done v => s!"done with {repr v}"
  | .stuck s => s!"stuck: {repr s}"
  | .illFormed w => s!"ill formed: {w}"
  | .outOfSteps => s!"out of steps after {budget}"

def main (args : List String) : IO UInt32 := do
  let verbose := args.contains "--verbose"
  let manifest ← IO.FS.readFile "../tests/conformance/manifest.jsonl"
  let listed := (← IO.FS.readFile "fragment-cases.txt").splitOn "\n"
    |>.map (fun l => Lexer.ofChars (l.toList.filter (!·.isWhitespace)))
    |>.filter fun l => !l.isEmpty && !l.startsWith "#"
  let covered := (← IO.FS.readFile "coverage.txt").splitOn "\n"
    |>.filterMap fun l => match l.splitOn " " with
      | ["rule", id] => some id
      | _ => none
  let mut seen : List String := []
  let mut ids : List String := []
  let mut counts : List (String × Counts) := [("accept", {}), ("run", {}), ("reject", {})]
  let mut reasons : List (String × Nat) := []
  let mut problems : Array String := #[]
  for line in manifest.splitOn "\n" do
    if line.toList.all Char.isWhitespace || line.startsWith "#" then continue
    let json ← match Lean.Json.parse line with
      | .ok j => pure j
      | .error e => throw (IO.userError s!"a manifest line is not JSON: {e}")
    let some id := (json.getObjValAs? String "id").toOption | continue
    let some expect := (json.getObjVal? "expect").toOption | continue
    let some kind := (expect.getObjValAs? String "kind").toOption | continue
    ids := id :: ids
    let mut c := (counts.find? (·.1 == kind)).map (·.2) |>.getD {}
    c := { c with total := c.total + 1 }
    let path : System.FilePath := s!"../tests/conformance/cases/{id}.wf"
    let result ← if (← path.pathExists) then
        pure (parseProgram (← IO.FS.readFile path))
      else
        pure (.error (.outside "module programs"))
    match result with
    | .error (.outside what) =>
      if listed.contains id then
        let msg := s!"{id} ({kind}): listed in fragment-cases.txt but outside the fragment: {what}"
        problems := problems.push msg
      c := { c with outside := c.outside + 1 }
      let n := (reasons.find? (·.1 == what)).map (·.2) |>.getD 0
      reasons := (what, n + 1) :: reasons.filter (·.1 != what)
    | .error (.reject rule msg) =>
      seen := id :: seen
      c := { c with rejected := c.rejected + 1 }
      if kind == "reject" && covered.contains ((expect.getObjValAs? String "rule").toOption.getD "?") then
        c := { c with compared := c.compared + 1, checked := c.checked + 1 }
      if kind != "reject" then
        problems := problems.push s!"{id} ({kind}): the parser rejects it citing {rule}: {msg}"
      else
        let cited := (expect.getObjValAs? String "rule").toOption.getD "?"
        IO.println s!"{id}: the parser rejects it citing {rule} ({msg}); the case cites {cited}"
    | .ok p =>
      seen := id :: seen
      c := { c with parsed := c.parsed + 1 }
      if verbose then IO.println s!"{id} ({kind}): in the fragment"
      let cited := (expect.getObjValAs? String "rule").toOption.getD "?"
      match kind, p.check with
      | "reject", .ok _ =>
        if covered.contains cited then
          c := { c with compared := c.compared + 1 }
          problems := problems.push s!"{id} (reject, {cited}): the checker accepts it"
      | "reject", .error rj =>
        if covered.contains cited then
          c := { c with compared := c.compared + 1, checked := c.checked + 1 }
        if rj.rule != cited then
          IO.println s!"{id}: the checker rejects it citing {rj.rule} ({rj.msg}); the case cites {cited}"
      | _, .ok _ =>
        c := { c with accepted := c.accepted + 1 }
        if let some msg := checkPoints p budget then
          problems := problems.push s!"{id} ({kind}): {msg}"
      | _, .error rj =>
        problems := problems.push s!"{id} ({kind}): the checker rejects it citing {rj.rule}: {rj.msg}"
      if kind == "run" then
        let expected := (expect.getObjValAs? Int "exit").toOption.getD 0
        match p.run budget with
        | .done (.status code) =>
          if code == expected then c := { c with ran := c.ran + 1 }
          else
            problems := problems.push s!"{id} (run): exit status {code}, expected {expected}"
        | o => problems := problems.push s!"{id} (run): {describe o}, expected exit {expected}"
    counts := counts.map fun (k, old) => if k == kind then (k, c) else (k, old)
  for id in seen.reverse do
    if !listed.contains id then
      problems := problems.push s!"{id}: in the fragment but not listed in fragment-cases.txt"
  for id in listed do
    if !ids.contains id then
      problems := problems.push s!"{id}: listed in fragment-cases.txt but not in the manifest"
  for (name, src, ok) in examples do
    match parseProgram src with
    | .ok p =>
      let o := p.run budget
      if !ok o then problems := problems.push s!"example {name}: {describe o}"
    | .error e => problems := problems.push s!"example {name}: the parser stops: {repr e}"
  for (name, src, rule) in checkerExamples do
    match parseProgram src with
    | .error e => problems := problems.push s!"example {name}: the parser stops: {repr e}"
    | .ok p =>
      let got : Option String := match p.check with
        | .ok _ => none
        | .error rj => some rj.rule
      if got != rule then
        let label (r : Option String) : String :=
          match r with
          | some rule => s!"a rejection citing {rule}"
          | none => "acceptance"
        problems := problems.push s!"example {name}: the checker gives {label got}, expected {label rule}"
  for (name, p, rule) in builtExamples do
    match p.check with
    | .ok _ => problems := problems.push s!"example {name}: the checker accepts it, expected a rejection citing {rule}"
    | .error rj =>
      if rj.rule != rule then
        problems := problems.push s!"example {name}: the checker rejects it citing {rj.rule}, expected {rule}"
  for (kind, c) in counts do
    IO.println (c.line kind)
  IO.println "outside the fragment, by the first construct met:"
  for (what, n) in reasons.toArray.qsort (fun a b => a.2 > b.2) do
    IO.println s!"  {n} {what}"
  if problems.isEmpty then
    IO.println "conformance: the model agrees with every case it compares"
    return 0
  else
    for p in problems do
      IO.eprintln p
    IO.eprintln s!"conformance: {problems.size} disagreements"
    return 1
