import WhitefootProof.Syntax

/-!
# The L0 relation state

The difference-bound part of the fragment's fact state [ENT-2, ENT-4, ENT-5]:
its terms, its atomic facts, the least closure, the dispositions of a query,
and the two flow operations that remove facts, a kill and a join. A state is
kept closed after every operation, so a kill that removes the facts over some
terms leaves the closed state over the others: removing a vertex from a closed
difference-bound graph keeps every path between the remaining vertices.
-/

namespace Whitefoot

namespace Fact

-- spec ENT-2 6d7c00a90d650dc8
-- spec ENT-2 3c8500e720232cd3
-- spec ENT-2 b9d2de7c65d7a579
-- spec ENT-2 19149e7a56339ac4
-- spec ENT-2 49ee19c7462f77c7
-- spec ENT-2 16ba176d2f3cf6d5
-- spec ENT-2 cd7976ae15f2eac4
-- spec ENT-2 9a47aa6d2ff78baa
/-- A term [ENT-2] of the fragment. Each carries the fragment type that gives it
its implicit bounds, except Z and a sum, which have none. -/
inductive Term where
  /-- The zero term Z (g), which carries constant relations. -/
  | z
  /-- A tracked place (a): one integer binding, by its declaration event, so a
  later binding reusing an expired spelling is another term. -/
  | place (id : Nat) (t : IntTy)
  /-- The commit value (f) of one `set` occurrence, or the returned value (f)
  of one return's computed integer expression [FN-9]: each is one fresh
  compiler-owned identity the walk mints, which no event kills. -/
  | commit (id : Nat) (t : IntTy)
  /-- A call datum [ENT-3.S13]: one operand's pre-transfer value. -/
  | datum (id : Nat) (t : IntTy)
  /-- A sum term (k) of an FN-9 relation's side that carries its additional
  parameter. -/
  | sum (a b : Term)
  deriving DecidableEq, Repr, Inhabited

namespace Term

-- spec ENT-5 c7857991638b5382
-- spec ENT-5 4a80a2e784e658c3
-- spec ENT-5 8f3fa6bdbfdae21e
/-- The bindings whose places a term reads: its support [ENT-5]. Z, commit
values and call datums have empty support and never die. -/
def support : Term → List Nat
  | .place id _ => [id]
  | .sum a b => a.support ++ b.support
  | _ => []

/-- The fragment type whose bounds the term carries implicitly [ENT-2]. -/
def ty? : Term → Option IntTy
  | .place _ t | .commit _ t | .datum _ t => some t
  | _ => none

end Term

-- spec ENT-2 f6c1b49aff61ec2e
/-- A difference bound `a - b <= c` [ENT-2]. -/
structure Bound where
  a : Term
  b : Term
  c : Int
  deriving DecidableEq, Repr, Inhabited

-- spec ENT-2 44ece0d9a7d5d882
/-- An offset disequality `a - b != c`; it is the same fact as `b - a != -c`. -/
structure Diseq where
  a : Term
  b : Term
  c : Int
  deriving DecidableEq, Repr, Inhabited

/-- The closed L0 state at one point. A contradictory state derives every
relation [ENT-4]. -/
structure L0 where
  contra : Bool := false
  bounds : List Bound := []
  diseqs : List Diseq := []
  deriving Repr, Inhabited

/-- The contradictory, all-derivable state. -/
def L0.bottom : L0 := { contra := true }

/-- The terms the facts of a state name, with `extra` and Z, each once. -/
def termUniverse (extra : List Term) (bs : List Bound) (ds : List Diseq) : Array Term :=
  let all := Term.z :: extra ++ bs.flatMap (fun b => [b.a, b.b]) ++ ds.flatMap (fun d => [d.a, d.b])
  (all.eraseDups).toArray

/-- A matrix of the tightest bounds between terms by index. -/
abbrev Matrix := Array (Array (Option Int))

def Matrix.get (m : Matrix) (i j : Nat) : Option Int := (m[i]!)[j]!

/-- Lower the entry `(i, j)` to `v` when `v` is tighter. -/
def Matrix.lower (m : Matrix) (i j : Nat) (v : Int) : Matrix :=
  match m.get i j with
  | some w => if v < w then m.set! i ((m[i]!).set! j (some v)) else m
  | none => m.set! i ((m[i]!).set! j (some v))

/-- Rule (1) of the closure to its fixed point: from `t1 - t2 <= c1` and
`t2 - t3 <= c2`, `t1 - t3 <= c1 + c2`, with rule (3), the smaller constant of
two bounds on one ordered pair, kept by `lower`. -/
def transitive (n : Nat) (m : Matrix) : Matrix :=
  (List.range n).foldl (fun m k =>
    (List.range n).foldl (fun m i =>
      match m.get i k with
      | none => m
      | some dik =>
        (List.range n).foldl (fun m j =>
          match m.get k j with
          | none => m
          | some dkj => m.lower i j (dik + dkj)) m) m) m

/-- Rule (2) once over every disequality: a bound on its pair whose constant is
its offset is tightened by one, in each orientation. Returns whether any
bound changed. -/
def strengthen (idx : Term → Nat) (ds : List Diseq) (m : Matrix) : Matrix × Bool :=
  ds.foldl (fun (m, changed) d =>
    let i := idx d.a
    let j := idx d.b
    let (m, c1) := if m.get i j == some d.c then (m.lower i j (d.c - 1), true) else (m, false)
    let (m, c2) := if m.get j i == some (-d.c) then (m.lower j i (-d.c - 1), true) else (m, false)
    (m, changed || c1 || c2)) (m, false)

-- spec ENT-4 46be01ee1de97c7a
/-- Alternate rules (1) and (2) until nothing changes. Each disequality
tightens each orientation at most once, since a tightened bound no longer
equals its offset and bounds only decrease, so `2 * |ds| + 1` rounds reach
the fixed point. -/
def saturate (n : Nat) (idx : Term → Nat) (ds : List Diseq) : Nat → Matrix → Matrix
  | 0, m => m
  | fuel + 1, m =>
    let m := transitive n m
    let (m', changed) := strengthen idx ds m
    if changed then saturate n idx ds fuel m' else m

-- spec ENT-2 7e4144507c761eef
/-- The implicit facts of a term [ENT-2]: a term of fragment type T lies in
`min(T)..=max(T)`, as two bounds through Z. -/
def implicitBounds (t : Term) : List Bound :=
  match t.ty? with
  | some ty => [⟨t, .z, ty.maxVal⟩, ⟨.z, t, -ty.minVal⟩]
  | none => []

-- spec ENT-4 794d5e0c6c92b373
/-- The least closure of a set of established facts together with the implicit
facts of `live` and of every term they name [ENT-4]. A negative cycle, `t - t
<= -1` for some t, makes the state contradictory. -/
def close (live : List Term) (bs : List Bound) (ds : List Diseq) : L0 :=
  let terms := termUniverse live bs ds
  let n := terms.size
  let idx : Term → Nat := fun t => (terms.toList.findIdx? (· == t)).getD 0
  let all := bs ++ terms.toList.flatMap implicitBounds
  let m0 : Matrix := Array.replicate n (Array.replicate n none)
  let m0 := (List.range n).foldl (fun m i => m.lower i i 0) m0
  let m0 := all.foldl (fun m b => m.lower (idx b.a) (idx b.b) b.c) m0
  let m := saturate n idx ds (2 * ds.length + 1) m0
  if (List.range n).any (fun i => match m.get i i with | some v => v < 0 | none => false) then
    L0.bottom
  else
    let bounds : List Bound := (List.range n).flatMap fun i => (List.range n).filterMap fun j =>
      if i == j then none else (m.get i j).map fun c => ⟨terms[i]!, terms[j]!, c⟩
    { contra := false, bounds, diseqs := ds.eraseDups }

namespace L0

/-- Add facts to a closed state and close again. -/
def add (s : L0) (live : List Term) (bs : List Bound) (ds : List Diseq := []) : L0 :=
  if s.contra then s else close live (s.bounds ++ bs) (s.diseqs ++ ds)

/-- The tightest bound `a - b <= c` the closed state holds, if any. -/
def tightest (s : L0) (a b : Term) : Option Int :=
  if a == b then some 0 else
  (s.bounds.find? (fun x => x.a == a && x.b == b)).map (·.c)

/-- Whether `a - b <= c` is derivable [ENT-4]. -/
def bound (s : L0) (a b : Term) (c : Int) : Bool :=
  s.contra || match s.tightest a b with
    | some c' => decide (c' ≤ c)
    | none => false

-- spec ENT-4 013ed975964ad10e
/-- Whether `a - b != c` is derivable [ENT-4]: present under either
orientation, or implied by a strict bound on either side. -/
def diseq (s : L0) (a b : Term) (c : Int) : Bool :=
  s.contra || s.diseqs.any (fun d => (d.a == a && d.b == b && d.c == c) ||
      (d.a == b && d.b == a && d.c == -c)) ||
    s.bound a b (c - 1) || s.bound b a (-c - 1)

/-- Whether `a - b = c` is derivable. -/
def eq (s : L0) (a b : Term) (c : Int) : Bool := s.bound a b c && s.bound b a (-c)

/-- The interval `lo..=hi` the closed bounds through Z give a term, if both
ends are bounded. -/
def interval (s : L0) (t : Term) : Option (Int × Int) :=
  match s.tightest t .z, s.tightest .z t with
  | some hi, some lo => some (-lo, hi)
  | _, _ => none

-- spec ENT-5 097623f069055649
/-- Remove every fact one of whose terms has a support member `dead` holds
[ENT-5]. The state is closed before the kill, so every consequence that names
only surviving terms survives. -/
def kill (s : L0) (dead : Nat → Bool) : L0 :=
  if s.contra then s else
  let hit (t : Term) := t.support.any dead
  { s with bounds := s.bounds.filter (fun b => !hit b.a && !hit b.b),
           diseqs := s.diseqs.filter (fun d => !hit d.a && !hit d.b) }

/-- The terms a state names. -/
def terms (s : L0) : List Term :=
  (s.bounds.flatMap (fun b => [b.a, b.b]) ++ s.diseqs.flatMap (fun d => [d.a, d.b])).eraseDups

end L0

-- spec ENT-5 4ccc008042be463f
-- spec ENT-5 9b007c6d4b989dc4
-- spec ENT-5 7c3cb36f9eb0dcbd
-- spec ENT-5 4a4b3beb5eb21882
/-- The L0 join of the states reaching one merge [ENT-5]: contradictory inputs
impose no constraint; over the others, each ordered pair keeps the weakest bound
all of them hold, and a disequality whose offset is zero or that some input
establishes is kept when every input derives it. With no non-contradictory
input the join is contradictory. -/
def join (live : List Term) (inputs : List L0) : L0 :=
  match inputs.filter (!·.contra) with
  | [] => L0.bottom
  | first :: rest =>
    let bounds : List Bound := first.bounds.filterMap fun b =>
      rest.foldl (fun acc s => acc.bind fun c =>
        (s.tightest b.a b.b).map fun c' => max c c') (some b.c) |>.map fun c => ⟨b.a, b.b, c⟩
    let terms := (Term.z :: (first :: rest).flatMap L0.terms).eraseDups
    let zero := terms.flatMap fun a => terms.filterMap fun b =>
      if a == b then none else some (⟨a, b, 0⟩ : Diseq)
    let candidates := (zero ++ (first :: rest).flatMap (·.diseqs)).eraseDups
    let diseqs := candidates.filter fun d => (first :: rest).all fun s => s.diseq d.a d.b d.c
    close live bounds diseqs

end Fact

end Whitefoot
