import Lean
import WhitefootProof

/-!
Every declaration of the model may depend on no axiom beyond Lean's standard
three. An unproved step (`sorry`, the axiom `sorryAx`), an evaluation the
kernel does not check (`native_decide`, which adds an axiom of its own) or a
new `axiom` would add one, and this file would fail to build.

The check enumerates the declarations of every module under `WhitefootProof`
itself, so nothing has to be listed to be checked. `lake exe coverage`
requires `WhitefootProof.lean` to import every module file under
`WhitefootProof/`, so every module is here.
-/

open Lean Elab Command

/-- Fails unless every declaration of every module under `WhitefootProof`
depends on no axiom beyond `propext`, `Classical.choice` and `Quot.sound`. -/
elab "#require_standard_axioms" : command => do
  let env ← getEnv
  let standard := [``propext, ``Classical.choice, ``Quot.sound]
  let mut checked : Nat := 0
  for i in [0:env.header.moduleNames.size] do
    if (`WhitefootProof).isPrefixOf env.header.moduleNames[i]! then
      for name in env.header.moduleData[i]!.constNames do
        let axioms ← liftCoreM <| collectAxioms name
        let extra := axioms.toList.filter (fun a => !standard.contains a)
        unless extra.isEmpty do
          logError m!"{name} depends on {extra}"
        checked := checked + 1
  if checked == 0 then
    logError "found no declaration of the model to check"
  else
    logInfo m!"{checked} declarations of the model depend on no axiom beyond the standard three"

#require_standard_axioms
