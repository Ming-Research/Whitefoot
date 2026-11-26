import Lake
open Lake DSL

-- The Lean model of the specification and its proofs. Lean's own library is
-- the only dependency; `lean-toolchain` pins the release.
package «whitefoot-proof» where
  leanOptions := #[⟨`autoImplicit, false⟩]

@[default_target]
lean_lib WhitefootProof

-- Building `StandardAxioms` fails when any declaration of the model depends
-- on an axiom beyond Lean's standard three.
@[default_target]
lean_lib StandardAxioms

-- `lake exe coverage` checks the model's sentence coverage of the
-- specification; `coverage.txt` describes it.
@[default_target]
lean_exe coverage where
  root := `Coverage

-- `lake exe conformance` runs the model on the conformance cases written in
-- the fragment; `Conformance.lean` describes the comparison.
@[default_target]
lean_exe conformance where
  root := `Conformance
