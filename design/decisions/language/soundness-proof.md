# Soundness proof

The machine-checked evidence that the specification's rules are sound: the Lean model of the rules, its proofs and the check that keeps them in step with the specification.

### decision soundness-is-proved-over-a-faithful-lean-model
The specification's soundness is evidenced by Lean 4 proofs over an executable model in `proof/` that formalizes the covered rules' own sentences, for a fragment of the constructs.
- rejected: an agent proposed proving an idealized core calculus in Rocq with Iris, in the manner of RustBelt; the owner rejected it, because the proof would hold of the calculus, while the defects found so far lived in rule details the calculus leaves out.
- rejected: an agent proposed a faithful model in Isabelle/HOL; the owner rejected it, because the model would run the conformance cases only through code generated in another language, and agents write Isabelle far less well than Lean.
- rejected: an agent proposed an executable model tested with no proof, as the end state; the owner rejected it, because testing finds counterexamples but cannot show there are none.

### decision soundness-proof-is-checked-in-the-same-work
`proof.yml` gates each change to the specification or the model: every covered rule line is marked or listed with its hash, and every theorem uses only Lean's three standard axioms.
- rejected: an agent proposed bringing the model and proofs up to date before each release; the owner rejected it, because a defect admitted by a changed rule would reach downstream pins before the proof saw it, and the catch-up is a separate cadence of its own.
- rejected: an agent proposed bringing the model and proofs up to date from time to time; the owner rejected it, because every formal model in the specification-representation study's precedents that was kept on its own cadence drifted and then froze.
