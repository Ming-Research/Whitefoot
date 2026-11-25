# Acceptance records

How the checker decides that a function is accepted: the obligation records it forms and the answers the entailment engine gives them.

### decision every-obligation-is-a-checker-record
The checker forms each mandatory obligation of a function as one record with its rule and site, and accepts the function only when the engine discharged every record.
- replaced: acceptance that listed the engine's outcome lists for each family by hand was on main; the owner changed it, because a family the listing or the walk missed was accepted silently, and each family was mapped to its rule a second time.
- rejected: an agent proposed that the engine form the records as its walk judges them; the owner rejected it, because an obligation the walk never judged would leave no record and be accepted, while a checker record with no answer rejects.
- rejected: an agent proposed keeping the lists for each family with a test that every list is in the listing; the owner rejected it, because the test guards the listing but not the mapping to rules, and each family must still be listed by hand.