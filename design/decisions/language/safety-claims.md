# Safety claims

The page that states Whitefoot's safety claims with the specification rules and conformance cases behind each, and the check that holds the page to them.

### decision safety-claims-live-on-one-checked-page
`docs/safety-claims.md` names for each safety claim the specification rules and conformance cases it rests on, and `make static` holds every citation to the specification and the manifest.
- replaced: safety claims spread over the README, the constitution and articles, citing rules nothing reads, was on main; the owner changed it, because a reader checking a claim needs the path from claim to rule to case in one place, and a checked page turns red when a rule or case it cites changes.
- rejected: an agent proposed a `claims` field on each manifest case; the owner rejected it, because it puts claim assignment into the conformance evidence and its schema, which only the specification's verdicts may change, and spreads one claim over thousands of manifest lines.
- rejected: an agent proposed the page with no check; the owner rejected it, because the page would then be a record nothing reads, and a safety page that has drifted states a claim the specification no longer makes.

### decision safety-claims-cite-a-chosen-sample-of-cases
The cases a row of the page names are a readable sample its author chooses, each checked to cite the row's rule, and not every case that cites the rule.
- rejected: an agent proposed generating the page from the manifest; the owner rejected it, because it would list every case that cites a rule, hundreds for one rule, which a reader cannot check, and which cases best show a rule is a judgment a script should not make.

### decision safety-claims-size-the-trusted-base-as-a-snapshot
The page lists each part of the trusted base with its size, counted by a stated method, and the check requires only that the method still finds its files, not that the figures match.
- rejected: an agent proposed holding the trusted-base figures to the checkout within a tolerance; the owner rejected it, because the figures carry no claim of their own, so the check would fail changes that did not touch the trusted base.
