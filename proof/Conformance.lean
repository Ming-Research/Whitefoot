import Lean.Data.Json
import WhitefootProof

/-!
`lake exe conformance`, run from this directory, compares the model with the
conformance cases (`../tests/conformance/manifest.jsonl`). Every case is
parsed. A case the parser stops at with `outside` uses a construct outside the
fragment and is only counted. Every `accept` and `run` case written in the
fragment must parse and resolve, and every `run` case must run to `done` with
its expected exit status, `exit_status`'s code. A `reject` case is only counted
here, and each the parser already rejects is printed beside the rule the case
cites: comparing rejections needs the fragment's checker. Each disagreement is
printed, and the check fails while one remains: either the model is not
faithful or the case shows a defect of the rules.
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

def Counts.line (kind : String) (c : Counts) : String :=
  s!"{kind}: {c.total} cases, {c.parsed} in the fragment, {c.outside} outside it, " ++
    s!"{c.rejected} rejected by the parser" ++
    (if kind == "run" then s!", {c.ran} run to their expected status" else "")

def describe : Outcome → String
  | .done (.status c) => s!"done with exit status {c}"
  | .done v => s!"done with {repr v}"
  | .stuck s => s!"stuck: {repr s}"
  | .illFormed w => s!"ill formed: {w}"
  | .outOfSteps => s!"out of steps after {budget}"

def main (args : List String) : IO UInt32 := do
  let verbose := args.contains "--verbose"
  let manifest ← IO.FS.readFile "../tests/conformance/manifest.jsonl"
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
    let mut c := (counts.find? (·.1 == kind)).map (·.2) |>.getD {}
    c := { c with total := c.total + 1 }
    let path : System.FilePath := s!"../tests/conformance/cases/{id}.wf"
    let result ← if (← path.pathExists) then
        pure (parseProgram (← IO.FS.readFile path))
      else
        pure (.error (.outside "module programs"))
    match result with
    | .error (.outside what) =>
      c := { c with outside := c.outside + 1 }
      let n := (reasons.find? (·.1 == what)).map (·.2) |>.getD 0
      reasons := (what, n + 1) :: reasons.filter (·.1 != what)
    | .error (.reject rule msg) =>
      c := { c with rejected := c.rejected + 1 }
      if kind != "reject" then
        problems := problems.push s!"{id} ({kind}): the parser rejects it citing {rule}: {msg}"
      else
        let cited := (expect.getObjValAs? String "rule").toOption.getD "?"
        IO.println s!"{id}: the parser rejects it citing {rule} ({msg}); the case cites {cited}"
    | .ok p =>
      c := { c with parsed := c.parsed + 1 }
      if verbose then IO.println s!"{id} ({kind}): in the fragment"
      if kind == "run" then
        let expected := (expect.getObjValAs? Int "exit").toOption.getD 0
        match p.run budget with
        | .done (.status code) =>
          if code == expected then c := { c with ran := c.ran + 1 }
          else
            problems := problems.push s!"{id} (run): exit status {code}, expected {expected}"
        | o => problems := problems.push s!"{id} (run): {describe o}, expected exit {expected}"
    counts := counts.map fun (k, old) => if k == kind then (k, c) else (k, old)
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
