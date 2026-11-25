# Build inputs

What the compiler's build derives from the specification, and how the specification's identity, integrity and version are kept.

### decision build-derives-spec-identity-and-grammar
The build script derives the specification's SHA-256 identity and the parser's grammar tables from `spec/kernel-spec.md` and keeps them as build outputs.
- replaced: A `Status:` line repeating the version, a committed copy of the generated parser tables and a test comparing a fresh generation with that copy was on main; the owner changed it, because each copy needs synchronization work and gives no other source of truth.
- rejected: an agent proposed removing the grammar generator and keeping only a frozen table; the owner rejected it, because grammar amendments still need a reproducible path from the normative productions to the parser's data.

### decision python-checks-spec-document-integrity
The existing Python conformance tooling checks the specification's duplicate definitions and unresolved rule references straight from its Markdown.
- replaced: A Rust specification-checker binary, built and run to repeat same-source identity comparisons, was on main; the owner changed it, because duplicate definitions and unresolved rule references need no compiler run or native artifact.

### decision spec-version-derived-from-log
The specification carries no version and an amendment adds no archive; an entry's version comes from its place in `spec/log.md`, and `.github/spec-version.sh` prints it.
- replaced: A version number taken on the branch was on main; the owner changed it, because every open amendment wrote it at the same places, 112 of 307 merges of main in two weeks were such collisions, and nothing read it (data: `research/investigations/spec-version-churn/README.md`).
- rejected: an agent proposed keeping the number on the branch and renumbering by script; the owner rejected it, because each collision would still need a merge of main and a new run of the checks, and collisions grow with the number of open amendments.
- rejected: an agent proposed assigning the number by a commit to main after each merge; the owner rejected it, because that changes the specification's bytes after the gate passed on them, needs a workflow that pushes to main, and edits the log heading where the next branch adds its entry.
- rejected: the owner proposed fixing the version by time, written into the title line or into archive names; the owner rejected it, because every branch edits the title line, so two branches collide as they do on a number.
- rejected: an agent proposed naming an amendment by the date and title of its log entry, with no number; the owner rejected it, because the name is long, 14 of the first 63 entries share one date, and a date gives no order (data: `research/investigations/spec-version-churn/README.md`).
- rejected: an agent proposed letting one amendment be open at a time; the owner rejected it, because at the median 12 hours open that allows two merges a day against the 4.4 that merged (data: `research/investigations/spec-version-churn/README.md`).
