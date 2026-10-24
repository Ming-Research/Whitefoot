import WhitefootProof.Points

-- Temporary (spec-proof-theorems): prints the elaborated shapes of the
-- definitions the simulation takes apart next (a loop's walk and step, a
-- function's entry); removed with its workflow.

set_option pp.notation false

#check @Whitefoot.Checker.walkStmt.eq_7
#print Whitefoot.Checker.loopHead
#print Whitefoot.Checker.entryState
#print Whitefoot.Checker.checkFn

set_option pp.notation true

#check @Whitefoot.Checker.walkStmt.eq_7
#check @Whitefoot.stepStmt.stepLoop.eq_def
#check @Whitefoot.stepStmt.stepBreak.eq_def
#check @Whitefoot.stepStmt.stepContinue.eq_def
#check @Whitefoot.step.eq_def
#print Whitefoot.enter
#check @List.filter_cons
#check @List.mem_filter
#check @List.eraseDups
