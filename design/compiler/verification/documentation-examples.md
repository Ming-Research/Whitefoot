Decision: Complete Whitefoot programs in Markdown under `docs/` and both root READMEs use `whitefoot` fences whose declared acceptance, execution or rejection the gate verifies, with the [guide authors' convention](../../../docs/guide/README.md#writing-examples) owning the marker meanings, because the reader-visible claim must stay attached to its program and Whitefoot-kit shares that convention, instead of untagged programs checked only from an author's remembered result or fragments completed by invented wrappers.

Decision: A Rust integration test in the existing corpus target checks acceptance and rejection through the CLI's source-checking library entry and runs through shared compilation, linking and bounded execution support, reporting all failing blocks together, because these paths preserve the distinction between source acceptance and executable behavior while the existing gate already runs the target, instead of a separate extractor, duplicated native construction or a new verification stage.

Decision: A rejection's attached `whitefoot-output` block holds author-maintained expected text compared byte for byte with the complete default diagnostic, because the displayed rejection is part of the reader's example, instead of comparing only selected fields or automatically regenerating expected output from the compiler.

Rejected:
- A Python extractor: rejected because compiler-backed checks go through `cargo test`, and a separate extractor would duplicate the Rust test's source and outcome boundary.
- Checking untagged fences: rejected because existing articles contain fragments and output that are not complete programs, so treating every fence as source would invent claims those articles do not make.
- Regenerating expected output automatically: rejected because the expected text is what the reader sees and must change only when an author changes it; rewriting it from the actual rejection would conceal a changed claim.
