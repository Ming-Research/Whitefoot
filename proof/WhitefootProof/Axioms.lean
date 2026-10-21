import Lean
import WhitefootProof.Integers

/-!
The model's theorems may depend on no axiom beyond Lean's standard three. An
unproved step (`sorry`, the axiom `sorryAx`), an evaluation the kernel does
not check (`Lean.ofReduceBool`) or a new `axiom` would add one, and this file
would fail to build. Each theorem the model states is named below.
-/

open Lean Elab Command

/-- Fails unless every named declaration depends on no axiom beyond `propext`,
`Classical.choice` and `Quot.sound`. -/
elab "#require_standard_axioms " ids:ident+ : command => do
  let standard := [``propext, ``Classical.choice, ``Quot.sound]
  for id in ids do
    let name ← liftCoreM <| realizeGlobalConstNoOverloadWithInfo id
    let axioms ← liftCoreM <| collectAxioms name
    let extra := axioms.toList.filter (fun a => !standard.contains a)
    unless extra.isEmpty do
      throwErrorAt id m!"{name} depends on {extra}"

#require_standard_axioms
  Whitefoot.IntTy.wrap_mem
  Whitefoot.IntTy.wrap_dvd
  Whitefoot.IntTy.wrap_unique
  Whitefoot.IntTy.wrap_of_mem
  Whitefoot.ArithOp.exactForm_mem
  Whitefoot.ArithOp.exactForm_eq_wrapForm
  Whitefoot.ArithOp.checkedForm_ok_iff
  Whitefoot.UnaryOp.exactForm_mem
  Whitefoot.UnaryOp.exactForm_eq_wrapForm
  Whitefoot.UnaryOp.abs_wrapForm_min
