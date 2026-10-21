/-!
`lake exe coverage`, run from this directory, checks the model's sentence
coverage of the specification: every line of every covered rule is marked as
formalized by a `-- spec <ID> <hash>` comment in a source file under
`WhitefootProof/`, or listed in `coverage.txt` as outside the model, and every
mark and listing names a line the specification still has. `coverage.txt`
gives the list's format. `lake exe coverage --show <ID>` prints each line of a
rule with its hash and disposition.
-/

/-- The 64-bit FNV-1a hash of a line's UTF-8 bytes. It detects a changed line;
nothing relies on it resisting a deliberate collision. -/
def lineHash (line : String) : UInt64 :=
  line.toUTF8.foldl (fun h b => (h ^^^ b.toUInt64) * 1099511628211) 14695981039346656037

def ofChars (cs : List Char) : String :=
  cs.foldl String.push ""

/-- The hash as 16 lowercase hexadecimal digits. -/
def hex16 (h : UInt64) : String :=
  let digits := Nat.toDigits 16 h.toNat
  digits.foldl String.push ("".pushn '0' (16 - digits.length))

/-- Whether `cs` is a rule ID: capitals and digits, a hyphen, then a digit
followed by letters, digits and dots, as in `OP-2` or `ENT-4.OT`. -/
def validId (cs : List Char) : Bool :=
  match cs.span (· != '-') with
  | (c :: pre, '-' :: d :: post) =>
    c.isUpper && pre.all (fun x => x.isUpper || x.isDigit) &&
      d.isDigit && post.all (fun x => x.isAlphanum || x == '.')
  | _ => false

/-- The rule ID a line opens, `OP-2` for a line starting `[OP-2]`. -/
def ruleIdOf (line : String) : Option String :=
  match line.toList with
  | '[' :: rest =>
    let body := rest.takeWhile (· != ']')
    if (rest.drop body.length).head? == some ']' && validId body then
      some (ofChars body)
    else
      none
  | _ => none

/-- A Markdown heading: one to six `#` and a space. -/
def isHeading (line : String) : Bool :=
  let cs := line.toList
  let marks := cs.takeWhile (· == '#')
  0 < marks.length && marks.length ≤ 6 && (cs.drop marks.length).head? == some ' '

def isBlank (line : String) : Bool :=
  line.toList.all Char.isWhitespace

def words (line : String) : List String :=
  (line.splitOn " ").filter (· != "")

/-- One rule's text: its ID and its lines other than blank and code-fence lines. -/
structure RuleText where
  id : String
  lines : Array String

/-- The rules of the specification in order. A rule's text runs from the line
that opens it to the next rule or heading outside a code fence. -/
def parseSpec (text : String) : Array RuleText := Id.run do
  let mut out : Array RuleText := #[]
  let mut cur : Option RuleText := none
  let mut inFence := false
  for line in text.splitOn "\n" do
    if line.startsWith "```" then
      inFence := !inFence
    else if !inFence && (ruleIdOf line).isSome then
      if let some r := cur then
        out := out.push r
      cur := some { id := (ruleIdOf line).getD "", lines := #[line] }
    else if !inFence && isHeading line then
      if let some r := cur then
        out := out.push r
      cur := none
    else if !isBlank line then
      if let some r := cur then
        cur := some { r with lines := r.lines.push line }
  if let some r := cur then
    out := out.push r
  return out

/-- An entry of `coverage.txt` or a mark in a model source file. -/
inductive Entry where
  /-- `family P`: every rule `P-…` is named by a `rule` or `skip` entry. -/
  | family (prefix_ : String)
  /-- `rule ID`: every line of the rule is marked or listed outside. -/
  | rule (id : String)
  /-- `skip ID reason`: a rule of a covered family left outside as a whole. -/
  | skip (id : String)
  /-- `outside ID HASH reason`: one line of a covered rule left outside. -/
  | outside (id hash : String)
  /-- `-- spec ID HASH` in a model source file: the line is formalized there. -/
  | mark (id hash : String)

/-- The entries of `coverage.txt`, with a problem for each line that is none. -/
def parseList (text : String) : Array (String × Entry) × Array String := Id.run do
  let mut entries := #[]
  let mut problems := #[]
  let mut number := 0
  for line in text.splitOn "\n" do
    number := number + 1
    let place := s!"coverage.txt:{number}"
    match words line with
    | [] => pure ()
    | ["family", p] => entries := entries.push (place, .family p)
    | ["rule", id] => entries := entries.push (place, .rule id)
    | "skip" :: id :: _ :: _ => entries := entries.push (place, .skip id)
    | "outside" :: id :: hash :: _ :: _ => entries := entries.push (place, .outside id hash)
    | w :: _ =>
      if !w.startsWith "#" then
        problems := problems.push s!"{place}: not an entry: {line}"
  return (entries, problems)

/-- The `-- spec ID HASH` marks of one model source file. -/
def parseMarks (file : String) (text : String) : Array (String × Entry) := Id.run do
  let mut marks := #[]
  let mut number := 0
  for line in text.splitOn "\n" do
    number := number + 1
    match words line with
    | ["--", "spec", id, hash] => marks := marks.push (s!"{file}:{number}", .mark id hash)
    | _ => pure ()
  return marks

def shorten (line : String) : String :=
  let cs := line.toList
  if cs.length ≤ 100 then line else ofChars (cs.take 100) ++ "…"

/-- Every coverage problem, each a line of explanation. -/
def check (rules : Array RuleText) (entries : Array (String × Entry)) : Array String := Id.run do
  let mut problems := #[]
  let known := rules.map (·.id)
  let covered := entries.filterMap fun
    | (_, .rule id) => some id
    | _ => none
  let named := entries.filterMap fun
    | (_, .rule id) => some id
    | (_, .skip id) => some id
    | _ => none
  for (place, entry) in entries do
    match entry with
    | .family p =>
      for id in known do
        if id.startsWith (p ++ "-") && !named.contains id then
          problems := problems.push
            s!"{place}: {id} is in family {p} but has no `rule` or `skip` entry"
    | .rule id | .skip id =>
      if !known.contains id then
        problems := problems.push s!"{place}: the specification has no rule {id}"
    | .outside id hash | .mark id hash =>
      match rules.find? (·.id == id) with
      | none => problems := problems.push s!"{place}: the specification has no rule {id}"
      | some r =>
        if !covered.contains id then
          problems := problems.push s!"{place}: {id} has no `rule {id}` entry in coverage.txt"
        else if !r.lines.any (fun l => hex16 (lineHash l) == hash) then
          problems := problems.push
            (s!"{place}: {id} has no line with hash {hash}; the line changed or was " ++
              "removed, so revisit what this entry claims and give the current line's hash")
  for r in rules do
    if covered.contains r.id then
      for l in r.lines do
        let hash := hex16 (lineHash l)
        let claimed := entries.any fun
          | (_, .outside id h) => id == r.id && h == hash
          | (_, .mark id h) => id == r.id && h == hash
          | _ => false
        if !claimed then
          problems := problems.push
            (s!"{r.id} {hash} is neither marked nor listed outside: {shorten l}\n" ++
              s!"  mark the definition that formalizes it with `-- spec {r.id} {hash}`, " ++
              s!"or list it in coverage.txt as `outside {r.id} {hash} <reason>`")
  return problems

/-- Each line of one rule with its hash and the entries that claim it. -/
def showRule (rules : Array RuleText) (entries : Array (String × Entry)) (id : String) :
    Array String := Id.run do
  match rules.find? (·.id == id) with
  | none => return #[s!"the specification has no rule {id}"]
  | some r =>
    let mut out := #[]
    for l in r.lines do
      let hash := hex16 (lineHash l)
      let claims := entries.filterMap fun
        | (place, .outside i h) => if i == id && h == hash then some s!"outside at {place}" else none
        | (place, .mark i h) => if i == id && h == hash then some s!"modeled at {place}" else none
        | _ => none
      let disposition := if claims.isEmpty then "unclaimed" else ", ".intercalate claims.toList
      out := out.push s!"{hash}  {disposition}\n  {shorten l}"
    return out

def main (args : List String) : IO UInt32 := do
  let spec ← IO.FS.readFile "../spec/kernel-spec.md"
  let rules := parseSpec spec
  let (listed, listProblems) := parseList (← IO.FS.readFile "coverage.txt")
  let mut entries := listed
  let files ← System.FilePath.walkDir "WhitefootProof"
  for file in files do
    if file.extension == some "lean" then
      entries := entries ++ parseMarks file.toString (← IO.FS.readFile file)
  match args with
  | ["--show", id] =>
    for line in showRule rules entries id do
      IO.println line
    return 0
  | [] =>
    let problems := listProblems ++ check rules entries
    if problems.isEmpty then
      let covered := entries.filter fun
        | (_, .rule _) => true
        | _ => false
      let marks := entries.filter fun
        | (_, .mark _ _) => true
        | _ => false
      let outside := entries.filter fun
        | (_, .outside _ _) => true
        | _ => false
      IO.println s!"coverage: {covered.size} rules covered, {marks.size} lines marked as modeled, {outside.size} listed outside"
      return 0
    else
      for p in problems do
        IO.eprintln p
      IO.eprintln s!"coverage: {problems.size} problems"
      return 1
  | _ =>
    IO.eprintln "usage: lake exe coverage [--show ID]"
    return 2
