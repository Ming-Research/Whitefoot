import WhitefootProof

/-! Temporary (spec-proof-discharge-sound): prints the elaborated forms the
draft proofs unfold; removed with `Draft.lean`. -/

open Whitefoot

#print Checker.evalAtom
#check @Checker.evalAtom.eq_def
#print Checker.evalRhs
#print Checker.checkReturn
#print Checker.relHolds
#print Checker.discharged
#print Checker.discharged.bridge
#print Checker.discharged.quotient
#print Checker.domainQuery
#print Checker.candidates
#print Checker.l0Images
#print Checker.lin
#print Checker.linAdd
#print Checker.Lin
#print Checker.relFacts
#print Checker.Query
#print Checker.Val
#print Checker.State
#print Checker.Env
#print Checker.Gen
#print Snapshot
#print Relation.holds
#print allHold
#print evalExpr
#print evalAtom
#print Static.exprTy
#print Static.invariantOk
