import WhitefootProof.Facts

/-!
# The affine part of the fact state

ENT-6's current-value images and the two affine procedures every numeric
consumer shares: `DIRECT`, a nonrecursive check, and `AUTO`, which tries
`DIRECT` after subtracting zero, one or two published premises, or one image of
a closed L0 bound, each with two integer tightenings. An image is an affine
form over compiler-owned immutable value atoms; each atom carries an interval,
its type's or, for a join's delta atom, the constants it stands for.
-/

namespace Whitefoot

namespace Aff

/-- `Σ a_i * x_i`, sorted by atom, with no zero coefficient. -/
abbrev Coeffs := List (Nat × Int)

/-- Add `a * x` to a sorted coefficient list. -/
def addTerm (xs : Coeffs) (x : Nat) (a : Int) : Coeffs :=
  match xs with
  | [] => if a == 0 then [] else [(x, a)]
  | (y, b) :: rest =>
    if x < y then (if a == 0 then xs else (x, a) :: xs)
    else if x == y then (if a + b == 0 then rest else (y, a + b) :: rest)
    else (y, b) :: addTerm rest x a

def addCoeffs (xs ys : Coeffs) : Coeffs := ys.foldl (fun acc (x, a) => addTerm acc x a) xs

def scaleCoeffs (k : Int) (xs : Coeffs) : Coeffs :=
  if k == 0 then [] else xs.map fun (x, a) => (x, k * a)

-- spec ENT-6 adba2e86b0db150c
/-- An image `c + Σ a_i * x_i` [ENT-6]. -/
structure Form where
  c : Int := 0
  xs : Coeffs := []
  deriving DecidableEq, Repr, Inhabited

namespace Form

def const (c : Int) : Form := { c }

def atom (x : Nat) : Form := { xs := [(x, 1)] }

def add (f g : Form) : Form := { c := f.c + g.c, xs := addCoeffs f.xs g.xs }

def scale (k : Int) (f : Form) : Form := { c := k * f.c, xs := scaleCoeffs k f.xs }

def sub (f g : Form) : Form := f.add (g.scale (-1))

def isConst (f : Form) : Bool := f.xs.isEmpty

end Form

/-- Whether a value is a checked `i128` [ENT-6]. -/
def fits (v : Int) : Bool := decide (-(2 ^ 127) ≤ v) && decide (v < 2 ^ 127)

/-- A normalized affine inequality `Σ a_i * x_i <= c`. -/
structure Ineq where
  xs : Coeffs
  c : Int
  deriving DecidableEq, Repr, Inhabited

namespace Ineq

/-- `lhs <= rhs`. -/
def le (lhs rhs : Form) : Ineq :=
  let d := lhs.sub rhs
  { xs := d.xs, c := -d.c }

def add (p q : Ineq) : Ineq := { xs := addCoeffs p.xs q.xs, c := p.c + q.c }

def sub (t s : Ineq) : Ineq := { xs := addCoeffs t.xs (scaleCoeffs (-1) s.xs), c := t.c - s.c }

/-- Every coefficient and the bound are checked `i128` values. -/
def ok (i : Ineq) : Bool := fits i.c && i.xs.all fun (_, a) => fits a

/-- `S / k` with its bound rounded toward negative infinity, when k divides
every coefficient. -/
def div (s : Ineq) (k : Int) : Option Ineq :=
  if k ≤ 1 || !s.xs.all (fun (_, a) => a % k == 0) then none
  else some { xs := s.xs.map (fun (x, a) => (x, a / k)), c := s.c.fdiv k }

end Ineq

/-- What a query reads besides its target: whether the state is
contradictory, each atom's interval, the published premises in order, and the
strongest canonical L0 images. -/
structure View where
  contra : Bool
  interval : Nat → Int × Int
  premises : List Ineq
  images : List Ineq

/-- An atom's endpoints for interval substitution [ENT-6]: the strongest of
its interval and the L0 images over that atom alone. -/
def View.hi (v : View) (x : Nat) : Int :=
  v.images.foldl (fun acc r => if r.xs == [(x, 1)] then min acc r.c else acc) (v.interval x).2

def View.lo (v : View) (x : Nat) : Int :=
  v.images.foldl (fun acc r => if r.xs == [(x, -1)] then max acc (-r.c) else acc) (v.interval x).1

/-- The greatest value interval substitution gives `Σ a_i * x_i`: each atom at
its upper endpoint for a positive coefficient and its lower one for a negative
coefficient. -/
def View.substMax (v : View) (xs : Coeffs) : Int :=
  xs.foldl (fun acc (x, k) => acc + (if k > 0 then k * v.hi x else k * v.lo x)) 0

-- spec ENT-6 3ec3128a964e0e54
-- spec ENT-6 f26a113dfd860040
/-- `DIRECT(A)` [ENT-6]: a contradictory state; a strongest canonical L0 image
with A's coefficient vector and a bound no greater than A's; or the bound
interval substitution gives every remaining atom, its upper endpoint for a
positive coefficient and its lower endpoint for a negative one. An atom's
endpoints are the strongest of its interval and the L0 images over that atom
alone. -/
def direct (v : View) (a : Ineq) : Bool :=
  v.contra || v.images.any (fun r => r.xs == a.xs && decide (r.c ≤ a.c)) ||
    decide (v.substMax a.xs ≤ a.c)

-- spec ENT-6 18858a03439c809f
-- spec ENT-6 031b53f565043d06
/-- The two integer tightenings of an accumulated sum [ENT-6]: by the factor
that makes S's coefficients exactly k times T's, then by the greatest common
divisor of S's coefficients. -/
def tightenings (t s : Ineq) : List Ineq :=
  if s.xs.isEmpty then [] else
  let k1 : Option Int := match t.xs, s.xs with
    | (x, a) :: _, (y, b) :: _ =>
      if x == y && a != 0 && b % a == 0 && scaleCoeffs (b / a) t.xs == s.xs then some (b / a)
      else none
    | _, _ => none
  let g : Int := s.xs.foldl (fun acc (_, a) => Int.ofNat (Nat.gcd acc.natAbs a.natAbs)) 0
  [k1, some g].filterMap fun k => k.bind s.div

/-- Check `DIRECT(T - S)` and then each tightening of S. An unrepresentable
candidate is skipped. -/
def tryResidual (v : View) (t s : Ineq) : Bool :=
  (s :: tightenings t s).any fun s' =>
    let r := t.sub s'
    s'.ok && r.ok && direct v r

-- spec ENT-6 b26dbcb5643237e4
-- spec ENT-6 89f49c9f4b52c902
-- spec ENT-6 7ec766f67f6d20a6
-- spec ENT-6 0a8d774c98036f74
-- spec ENT-6 cf7079bdd5db4ecb
-- spec ENT-6 17cbbdf64db77ff9
-- spec ENT-6 1d4ee31336b451f4
-- spec MSR-4 f529e0bc60a1d404
/-- `AUTO(T)` [ENT-6]: `DIRECT(T)`, then T less every published premise, every
unordered pair of them (a premise with itself included), and every strongest
canonical L0 image, each with its two tightenings. -/
def auto (v : View) (t : Ineq) : Bool :=
  direct v t ||
  v.premises.any (fun p => tryResidual v t p) ||
  (v.premises.zipIdx.any fun (p, i) =>
    (v.premises.drop i).any fun q => tryResidual v t (p.add q)) ||
  v.images.any (fun r => tryResidual v t r)

-- spec ENT-6 8148f1589e1fcec1
-- spec ENT-6 69a4b53a7d66bed9
-- spec ENT-6 e5c3e20fbba67cc6
/-- The strongest canonical L0 images [ENT-6]: for each ordered pair of
candidate terms with a closed bound `l - r <= c`, the inequality
`image(l) - image(r) <= c`, keeping the smallest bound per coefficient vector.
-/
def l0Images (s : Fact.L0) (cands : List (Fact.Term × Form)) : List Ineq :=
  if s.contra then [] else
  let all := cands.flatMap fun (l, fl) => cands.filterMap fun (r, fr) =>
    if l == r then none else
    (s.tightest l r).bind fun c =>
      let i := Ineq.le (fl.sub fr) (Form.const c)
      if i.ok && !i.xs.isEmpty then some i else none
  all.foldl (fun acc i =>
    match acc.find? (·.xs == i.xs) with
    | some j => if i.c < j.c then acc.map (fun k => if k.xs == i.xs then i else k) else acc
    | none => acc ++ [i]) []

end Aff

end Whitefoot
