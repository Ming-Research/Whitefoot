import WhitefootProof

/-! Temporary (spec-proof-discharge-sound): prints the elaborated forms and
library lemma statements the draft proofs use; removed with `Draft.lean`. -/

open Whitefoot

#print Checker.evalAtom
#print Checker.evalRhs
#print Checker.checkReturn
#print Checker.domainQuery
#print Checker.lin
#print Checker.linImage
#print Checker.linTerm
#print Aff.l0Images
#check @List.map_congr_left
#check @List.mem_map_of_mem
#check @List.find?_eq_none
#check @List.find?_some
#check @List.mem_of_find?_eq_some
#check @List.nodup_append
#check @List.nodup_cons
#check @List.foldlM_cons
#check @List.mem_cons_of_mem
#check @List.filter_cons_of_pos
#check @List.filter_cons_of_neg
#check @List.all_cons
#check @Option.bind_eq_some_iff
#check @Option.map_eq_some_iff
#check @Int.ediv_mul_le
#check @Int.lt_of_mul_lt_mul_right
#check @decide_eq_true_eq
