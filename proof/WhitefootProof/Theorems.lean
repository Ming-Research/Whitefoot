import WhitefootProof.Loops
import WhitefootProof.ValueSound

/-!
# The two theorems under the snapshots' meaning

The simulation proves both theorems from premises about the meaning of the
facts (`Sim.Laws`). The meaning of a snapshot on a frame (`Snapshot.Holds`)
and the soundness of the obligations are proved (`Holds.lean`,
`ObligationSound.lean`, `ValueSound.lean`), so this file gives the simulation
that meaning and proves the discharge group of its premises from those
lemmas. What is left is the transfer group (`Sim.TransferLaws`) and the step
that gives a computed return value its term and image, which the return's
postcondition lemma reads (`Sim.ReturnedSound`); the item
spec-proof-transfer-sound proves them.
-/

namespace Whitefoot

namespace Sim

/-- The meaning the facts are given: a snapshot holds on a frame, a snapshot's
identities lie below a counter, and a value lies in its type's range. -/
abbrev snapshotModel : Model where
  Holds := Snapshot.Holds
  Fresh := Snapshot.Fresh
  InRange := Value.InRange

/-- A return's computed value: from a snapshot that holds and is fresh, the
step that gives the value its term and image leaves a fresh snapshot that
holds under valuations by which the returned value describes the value the
expression evaluates to. -/
def ReturnedSound : Prop :=
  ∀ {env : Checker.Env} {st st' : Checker.State} {g g1 g2 : Checker.Gen} {e : Expr}
    {r : Checker.Rhs} {ret : Checker.Val} {E E0 : Env} {w : Value},
    (⟨env.vars, st, g.atoms⟩ : Snapshot).Holds (E, E0) →
    (⟨env.vars, st, g.atoms⟩ : Snapshot).Fresh g.next →
    (Checker.evalRhs env st e).run g = .ok (r, g1) →
    (∃ sc t, Static.exprTy env.prog sc e = .ok t) → r.call = none →
    evalExpr env.prog E e = .ok w →
    (Checker.returnedValue env st e r).run g1 = .ok ((st', ret), g2) →
    ∃ G ν, (⟨env.vars, st', g2.atoms⟩ : Snapshot).HoldsWith E E0 G ν ∧
      (⟨env.vars, st', g2.atoms⟩ : Snapshot).Fresh g2.next ∧
      ret.Denotes (Checker.valueOf env.vars E) ν G w

/-- The discharge group of the simulation's premises holds for the snapshots'
meaning. A return's postconditions are read off the value the return step
computes, so they also take that step's lemma. -/
theorem discharge_laws (hret : ReturnedSound) : DischargeLaws snapshotModel where
  fits := Snapshot.Holds.fits
  not_contra := Snapshot.Holds.not_contra
  atoms_mono := Snapshot.Holds.atoms_mono
  fresh_mono := Snapshot.Fresh.mono
  fresh_vars h hv := by
    have h' : Snapshot.Fresh _ _ := h
    show Snapshot.Fresh _ _
    exact ⟨hv, h'.terms, h'.goals, h'.images, h'.data, h'.premises, h'.stable, h'.links,
      h'.quotients, h'.atoms⟩
  fresh_ids h := by
    have h' : Snapshot.Fresh _ _ := h
    exact h'.vars
  evalRhs_progress := Checker.evalRhs_progress
  evalRhs_call_some := Checker.evalRhs_call_some
  evalRhs_call_none := Checker.evalRhs_call_none
  callArgs_progress := Checker.callArgs_progress
  callArgs_inRange := Checker.callArgs_inRange
  call_requires := Checker.call_requires
  target_progress := Checker.target_progress
  return_ensures hs hf hr ht hp hfn hcall hrv hcr hv hvt hpar hE0 := by
    obtain ⟨G, ν, hw, -, hd⟩ := hret hs hf hr ht hcall hv hrv
    exact Checker.return_ensures hw hd hvt hp hfn hpar hE0 hcr
  relHolds_sound := Checker.relHolds_sound

/-- The first theorem from the transfer lemmas: an accepted program whose entry
holds ends or runs out of steps, never stuck. -/
theorem accepted_never_stuck_of_transfer (hret : ReturnedSound)
    (T : TransferLaws snapshotModel) {p : Program} (hp : p.accepts = true)
    (hentry : p.EntryHolds) (steps : Nat) :
    (∃ v, p.run steps = .done v) ∨ p.run steps = .outOfSteps :=
  accepted_never_stuck_of_laws (F := snapshotModel)
    { toDischargeLaws := discharge_laws hret, toTransferLaws := T } hp hentry steps

/-- The second theorem from the transfer lemmas: the checker's facts hold at
every point a run of an accepted program reaches. -/
theorem facts_hold_of_transfer (hret : ReturnedSound) (T : TransferLaws snapshotModel)
    {p : Program} (hp : p.accepts = true) :
    ∀ pt st, p.Reaches pt st → (p.factsAt pt).Holds st :=
  facts_hold_of_laws (F := snapshotModel)
    { toDischargeLaws := discharge_laws hret, toTransferLaws := T } hp

end Sim

end Whitefoot
