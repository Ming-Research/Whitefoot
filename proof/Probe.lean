import WhitefootProof.Points

-- Temporary (spec-proof-theorems): prints the elaborated shapes of the
-- definitions the simulation takes apart; removed with its workflow.

set_option pp.notation false

#check @Whitefoot.Static.stmts.eq_2
#check @Whitefoot.Static.stmt.eq_1
#check @Whitefoot.Static.stmt.eq_2
#check @Whitefoot.Static.stmt.eq_3
#check @Whitefoot.Static.stmt.eq_4
#check @Whitefoot.Static.stmt.eq_5
#check @Whitefoot.Static.stmt.eq_7
#check @Whitefoot.Static.stmt.eq_9
#check @Whitefoot.Static.checkFn.eq_1
#check @Whitefoot.Static.checkProgram.eq_1
#check @Whitefoot.Program.check.eq_1
#check @Whitefoot.Checker.walkStmt.eq_1
#check @Whitefoot.Checker.walkStmt.eq_2
#check @Whitefoot.Checker.walkStmt.eq_3
#check @Whitefoot.Checker.walkStmt.eq_4
#check @Whitefoot.Checker.walkStmt.eq_5
#check @Whitefoot.Checker.walkStmt.eq_7
#check @Whitefoot.Checker.walkStmt.eq_9
#print Whitefoot.Checker.freshId
#print Whitefoot.Checker.merge
