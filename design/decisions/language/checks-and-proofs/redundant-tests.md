# Redundant tests

Which `if` and `value_if` conditions are rejected because the checker already knows their outcome.

### decision decided-conditions-rejected
An `if` or `value_if` condition is rejected when every fact its selected branch would establish is already derivable, with a repair that deletes the test and its dead branch.
- replaced: accepting conditions the checker decides was on main; the owner changed it, because source should hold only the tests its proofs still need, as a proof block automatic derivation already proves is rejected.
- rejected: an agent proposed warning about decided tests instead; the owner rejected it, because source should hold only the tests its proofs still need.
- rejected: an agent proposed judging only the goal origins, or deciding by any one origin; the owner rejected it, because deleting a test one origin decides removes the facts its other origins set up, and a later goal that needed them fails.
- rejected: an agent proposed a language form for a program's run-time check of its own final result; the owner rejected it, because the harness that runs the program can check its output.

### decision generic-test-judged-once
A generic body's condition is judged once, at the symbolic instance of its own parameters under their written bounds.
- rejected: an agent proposed judging each concrete instance; the owner rejected it, because a test that some substitutions need must stay writable when another substitution decides it.

### decision contradictory-state-test-admitted
A condition whose entering state is contradictory is admitted.
- rejected: an agent proposed treating the contradiction as one decided truth, or a separate rejection of unreachable states; the owner rejected it, because explosion proves both truths and selects no reachable branch to keep.