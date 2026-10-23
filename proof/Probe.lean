import WhitefootProof.Points

-- Temporary (spec-proof-theorems): prints the elaborated shapes of the
-- definitions the simulation takes apart, and the library lemmas it uses;
-- removed with its workflow.

set_option pp.notation false

#check @Whitefoot.Static.stmt.eq_6
#check @Whitefoot.Static.stmt.eq_8
#check @Whitefoot.Static.stmt.eq_10
#check @Whitefoot.Static.stmt.eq_11
#check @Whitefoot.Static.stmt.eq_12
#check @Whitefoot.Checker.walkStmt.eq_6
#check @Whitefoot.Checker.walkStmt.eq_8
#check @Whitefoot.Checker.walkStmt.eq_10
#check @Whitefoot.Checker.merge.eq_3

set_option pp.notation true

#check @List.forIn_cons
#check @List.forIn_nil
#check @List.mem_map
#check @beq_self_eq_true
#check @List.mem_append_left
#check @List.mem_append
#check @List.prefix_append
#check @List.IsPrefix.trans
#check @List.find?_some
#check @List.mem_cons
#check @List.drop_append_of_le_length
#check @List.length_drop
#check @List.getElem_drop
#check @Nat.lt_irrefl
