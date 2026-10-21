/-!
# Integer value semantics

The value part of [OP-2] for add, subtract, multiply, negation, absolute value
and the six comparisons: the closed integer types and their value sets,
wrapping, and the exact, defined, wrap and checked forms. Division, remainder
and shifts are outside this slice.

A `-- spec <ID> <hash>` line above a definition marks the specification line
that definition formalizes; `lake exe coverage` checks every mark against the
specification's current text (see `coverage.txt`).

An exact form returns `none` where its domain query is false: the
specification gives that operation no result, and the model takes no step.
-/

namespace Whitefoot

-- spec OP-2 b9d1e5b8e0b6674b
/-- The closed integer-type set `i8 i16 i32 i64 u8 u16 u32 u64`. -/
inductive IntTy where
  | i8 | i16 | i32 | i64 | u8 | u16 | u32 | u64
  deriving DecidableEq, Repr

namespace IntTy

/-- The width `K` of `iK` and `uK`. -/
def bits : IntTy → Nat
  | .i8 | .u8 => 8
  | .i16 | .u16 => 16
  | .i32 | .u32 => 32
  | .i64 | .u64 => 64

/-- Whether the type is one of the signed `iK`. -/
def signed : IntTy → Bool
  | .i8 | .i16 | .i32 | .i64 => true
  | .u8 | .u16 | .u32 | .u64 => false

-- spec OP-2 e581538ac45c36b6
/-- `MIN(T)`, the least member of the value set: `-2^(K-1)` for `iK`, `0` for `uK`. -/
def minVal (t : IntTy) : Int :=
  if t.signed then -((2 : Int) ^ (t.bits - 1)) else 0

/-- The greatest member of the value set: `2^(K-1)-1` for `iK`, `2^K-1` for `uK`. -/
def maxVal (t : IntTy) : Int :=
  if t.signed then (2 : Int) ^ (t.bits - 1) - 1 else (2 : Int) ^ t.bits - 1

-- spec OP-2 fbe253f1ec4a8a2c
/-- `z` belongs to T. Values are mathematical integers, never host-language
machine words. -/
def Mem (t : IntTy) (z : Int) : Prop :=
  t.minVal ≤ z ∧ z ≤ t.maxVal

instance (t : IntTy) (z : Int) : Decidable (t.Mem z) :=
  inferInstanceAs (Decidable (t.minVal ≤ z ∧ z ≤ t.maxVal))

-- spec OP-2 03e82e4cb9af04f5
/-- `M = 2^K`. -/
def modulus (t : IntTy) : Int :=
  (2 : Int) ^ t.bits

-- spec OP-2 c9485f47c75fe36b
-- spec OP-2 f4399753a8f2fbe4
/-- `wrap_T(z)`: the integer `u` with `0 <= u < M` congruent to `z` modulo `M`,
less `M` for `iK` when `u >= 2^(K-1)`. `wrap_unique` shows `u` is unique. -/
def wrap (t : IntTy) (z : Int) : Int :=
  if t.signed ∧ z % t.modulus ≥ (2 : Int) ^ (t.bits - 1) then
    z % t.modulus - t.modulus
  else
    z % t.modulus

/-- A wrapped value belongs to T. -/
theorem wrap_mem (t : IntTy) (z : Int) : t.Mem (t.wrap z) := by
  cases t <;> simp [Mem, wrap, minVal, maxVal, modulus, bits, signed] <;> (try split) <;> omega

/-- A wrapped value is congruent to the unwrapped one modulo `M`. -/
theorem wrap_dvd (t : IntTy) (z : Int) : t.modulus ∣ t.wrap z - z := by
  cases t <;> simp [wrap, modulus, bits, signed] <;> (try split) <;> omega

/-- `wrap_T(z)` is the only member of T congruent to `z` modulo `M`. -/
theorem wrap_unique (t : IntTy) (z w : Int) (hw : t.Mem w) (hd : t.modulus ∣ w - z) :
    w = t.wrap z := by
  cases t <;> simp [Mem, wrap, minVal, maxVal, modulus, bits, signed] at hw hd ⊢ <;>
    (try split) <;> omega

/-- Wrapping leaves a member of T unchanged. -/
theorem wrap_of_mem {t : IntTy} {z : Int} (h : t.Mem z) : t.wrap z = z :=
  (wrap_unique t z z h (by simp)).symm

end IntTy

-- spec OP-2 12ccb1e9b931d855
/-- The result of a checked form: `Ok(value: z)` or `Err(error: Overflow())`. -/
inductive Checked where
  | ok (value : Int)
  | overflow
  deriving DecidableEq, Repr

/-- Add, subtract and multiply. -/
inductive ArithOp where
  | add | sub | mul
  deriving DecidableEq, Repr

namespace ArithOp

/-- The mathematical result. -/
def math : ArithOp → Int → Int → Int
  | .add, a, b => a + b
  | .sub, a, b => a - b
  | .mul, a, b => a * b

-- spec OP-2 b21f9e5bdd9eed6a
-- spec OP-2 47dca85475f38189
-- spec OP-2 8e92c842001881d5
-- spec OP-2 b1083d4ca19eb63b
-- spec OP-2 ce096ef35f4647b5
/-- The domain queries `+defined`, `-defined` and `*defined` at T: a total `Bool`
that executes no exact operation. -/
def defined (op : ArithOp) (t : IntTy) (a b : Int) : Bool :=
  decide (t.Mem (op.math a b))

-- spec OP-2 5503711c8aacc135
-- spec OP-2 cde0d6cacd62e4e1
/-- The exact form: the mathematical result where the domain query holds, and
no result elsewhere. -/
def exactForm (op : ArithOp) (t : IntTy) (a b : Int) : Option Int :=
  if op.defined t a b then some (op.math a b) else none

-- spec OP-2 b2f9211f8c7d02cf
/-- The wrap form: `wrap_T` of the mathematical result. -/
def wrapForm (op : ArithOp) (t : IntTy) (a b : Int) : Int :=
  t.wrap (op.math a b)

/-- The checked form. -/
def checkedForm (op : ArithOp) (t : IntTy) (a b : Int) : Checked :=
  if t.Mem (op.math a b) then .ok (op.math a b) else .overflow

/-- An exact form that returns a result returns a member of T: no silent
overflow. -/
theorem exactForm_mem {op : ArithOp} {t : IntTy} {a b r : Int}
    (h : op.exactForm t a b = some r) : t.Mem r := by
  unfold exactForm at h
  by_cases hd : op.defined t a b = true
  · rw [if_pos hd] at h
    cases h
    exact of_decide_eq_true hd
  · rw [if_neg hd] at h
    cases h

/-- Where its domain query holds, the exact form returns the wrap form's
result, so a discharged operation needs no runtime test. -/
theorem exactForm_eq_wrapForm {op : ArithOp} {t : IntTy} {a b : Int}
    (hd : op.defined t a b = true) : op.exactForm t a b = some (op.wrapForm t a b) := by
  have hm : t.Mem (op.math a b) := of_decide_eq_true hd
  unfold exactForm wrapForm
  rw [if_pos hd, IntTy.wrap_of_mem hm]

/-- The checked form returns `Ok` exactly where the exact form returns a result,
and with the same value. -/
theorem checkedForm_ok_iff {op : ArithOp} {t : IntTy} {a b r : Int} :
    op.checkedForm t a b = .ok r ↔ op.exactForm t a b = some r := by
  unfold checkedForm exactForm defined
  by_cases hm : t.Mem (op.math a b)
  · simp [hm]
  · simp [hm]

end ArithOp

/-- Negation and absolute value, defined for the signed types. -/
inductive UnaryOp where
  | neg | abs
  deriving DecidableEq, Repr

namespace UnaryOp

-- spec OP-2 b5dc1d3d54f9afb7
/-- The mathematical result: `-x`, and the nonnegative absolute value. -/
def math : UnaryOp → Int → Int
  | .neg, x => -x
  | .abs, x => if x < 0 then -x else x

-- spec OP-2 b407705213d09be6
-- spec OP-2 630562baf88d2844
/-- The domain queries `ineg.defined` and `iabs.defined`. -/
def defined (_ : UnaryOp) (t : IntTy) (x : Int) : Bool :=
  decide (x ≠ t.minVal)

/-- The exact form. -/
def exactForm (op : UnaryOp) (t : IntTy) (x : Int) : Option Int :=
  if op.defined t x then some (op.math x) else none

/-- The wrap form. -/
def wrapForm (op : UnaryOp) (t : IntTy) (x : Int) : Int :=
  t.wrap (op.math x)

/-- The checked form. -/
def checkedForm (op : UnaryOp) (t : IntTy) (x : Int) : Checked :=
  if t.Mem (op.math x) then .ok (op.math x) else .overflow

/-- On a signed type, an exact form that returns a result returns a member of T. -/
theorem exactForm_mem {op : UnaryOp} {t : IntTy} {x r : Int} (hs : t.signed = true)
    (hx : t.Mem x) (h : op.exactForm t x = some r) : t.Mem r := by
  unfold exactForm at h
  by_cases hd : op.defined t x = true
  · rw [if_pos hd] at h
    cases h
    have hne : x ≠ t.minVal := of_decide_eq_true hd
    cases op <;> cases t <;>
      simp [math, IntTy.Mem, IntTy.minVal, IntTy.maxVal, IntTy.bits, IntTy.signed]
        at hs hx hne ⊢ <;>
      (try split) <;> omega
  · rw [if_neg hd] at h
    cases h

/-- On a signed type, where its domain query holds, the exact form returns the
wrap form's result. -/
theorem exactForm_eq_wrapForm {op : UnaryOp} {t : IntTy} {x : Int} (hs : t.signed = true)
    (hx : t.Mem x) (hd : op.defined t x = true) :
    op.exactForm t x = some (op.wrapForm t x) := by
  have hr : op.exactForm t x = some (op.math x) := by
    unfold exactForm
    rw [if_pos hd]
  have hm : t.Mem (op.math x) := exactForm_mem hs hx hr
  rw [hr]
  unfold wrapForm
  rw [IntTy.wrap_of_mem hm]

/-- `iabs.wrap(MIN(T))` returns `MIN(T)`. -/
theorem abs_wrapForm_min {t : IntTy} (hs : t.signed = true) :
    UnaryOp.abs.wrapForm t t.minVal = t.minVal := by
  cases t <;> simp [IntTy.signed] at hs <;>
    first
    | decide
    | simp [wrapForm, math, IntTy.wrap, IntTy.minVal, IntTy.modulus, IntTy.bits, IntTy.signed]

end UnaryOp

-- spec OP-2 822c3ccaf78ce417
-- spec OP-2 68823f08ba6f4e02
-- spec OP-2 0a955796e48be770
/-- The six integer comparisons, total and `Bool`-valued, over the operands'
mathematical values in T. The values of an unsigned T are nonnegative, so
mathematical order is unsigned order there. -/
inductive CmpOp where
  | eq | ne | lt | le | gt | ge
  deriving DecidableEq, Repr

/-- The comparison's result. -/
def CmpOp.eval : CmpOp → Int → Int → Bool
  | .eq, a, b => decide (a = b)
  | .ne, a, b => decide (a ≠ b)
  | .lt, a, b => decide (a < b)
  | .le, a, b => decide (a ≤ b)
  | .gt, a, b => decide (a > b)
  | .ge, a, b => decide (a ≥ b)

end Whitefoot
