# Documentation examples

How the gate checks the Whitefoot programs shown in the user guides and the two root READMEs.

### decision guide-examples-declare-their-outcome
Complete programs in `docs/guide/` and both root READMEs sit in `whitefoot` fences whose declared outcome (accept, `run exit=N`, `reject RULE`) the gate checks.
- replaced: Leaving programs untagged, checked only by an author's memory of one compile, was on main; the owner changed it, because the claim a reader sees must stay checked together with its program, and the marks match the ones Whitefoot-kit already uses.
- rejected: an agent proposed treating every untagged fence as a program; the owner rejected it, because the articles hold fragments and output that are not complete programs.
- rejected: an agent proposed checking every Markdown file under `docs/` with the current compiler; the owner rejected it, because articles state results at the compiler revision they name, and 14 article programs and one essay program did not match the current compiler.

### decision guide-examples-run-in-the-corpus-tests
A Rust test in the existing corpus target checks every marked block through the compiler's library check entry and the shared build-and-run support, and reports every failing block.
- rejected: an agent proposed a Python script that extracts and checks the examples; the owner rejected it, because the repository checks the compiler through `cargo test`.
- rejected: an agent proposed a new verification stage for the examples; the owner rejected it, because the gate already runs the corpus target.

### decision guide-rejections-match-their-text
A `whitefoot-output` block after a rejected example holds the full default diagnostic, written by its author, and the test compares it byte for byte.
- rejected: an agent proposed comparing only selected fields of the rejection; the owner rejected it, because the shown rejection text is part of the reader's example.
- rejected: an agent proposed rewriting the expected text from the compiler's actual output; the owner rejected it, because the text must change only when its author changes it, and an automatic rewrite would hide a changed claim.