# Diagnostic rendering

How the driver prints a compiler stop: the record of labeled fields, its lean text and JSON forms, and what a recorded rejection keeps.

### decision stops-render-as-labeled-records
Each rejection prints by default as a `file:line:column: error[RULE]: Kind` line, the marked source line and one `label: value` line per field; `--diagnostic-format json` prints the whole record.
- replaced: Printing the stage payload's `Debug` text was on main; the owner changed it, because `Debug` is not a stable format and the hand-written implementations already diverge from the derived shape.
- rejected: an agent proposed JSON as the default rendering; the owner rejected it, because it escapes the source and prose the agent compares with its program, drops the marker and costs more to read per record (data: `research/investigations/readable-diagnostics/DESIGN.md`).
- rejected: an agent proposed repeating the envelope as labeled lines under the summary; the owner rejected it, because the rule, kind and position are in the summary already, and category, stage and byte interval give a writer nothing to act on.
- rejected: an agent proposed hand-written prose per rejection kind; the owner rejected it, because about a hundred maintained sentences would restate fields and mechanical fixes the payload already carries exactly.

### decision payload-fields-listed-by-destructuring
The diagnostic module lists each payload's fields and each classification's variants by exhaustive destructuring, and a node a payload names carries its coordinate from the checker.
- rejected: an agent proposed parsing each payload's derived `Debug` output into a generic tree; the owner rejected it, because `Debug` is not a stable format and a parsed node path still cannot be resolved without the tree.
- rejected: an agent proposed a serialization derive from an external crate; the owner rejected it, because the exhaustive listings already keep the closed schema in step with its types, and a proc-macro dependency set shows no benefit over them.
- rejected: an agent proposed returning the resolved unit with every semantic failure so the driver can resolve node paths; the owner rejected it, because that changes every failure variant and its tests, while the checker holds the tree where a payload names another node.

### decision text-lists-quote-string-items
A text-format list writes each string item double-quoted with the renderer's escapes, and keeps the typed display of numbers and structured items.
- replaced: Leaving string items unquoted beside quoted exact tokens was on main; the owner changed it, because a string can contain the list's comma-and-space separator, so a list of two items could not be told from one.

### decision module-rejections-keep-structured-diagnostics
A recorded module rejection keeps every reported rule, source coordinate and expected-terminal set as structured data beside its rendered text.
- rejected: an agent proposed rebuilding the diagnostic fields from the displayed text; the owner rejected it, because a fresh and a cached check must give the same ordered syntax diagnostics and the lean text is not lossless.

### decision allocation-layout-failures-name-the-call
A target layout failure caused by an allocation names the payload type, its size and alignment, the target limit with its value, and the nearest call the writer wrote that needs it.
- rejected: an agent proposed reporting only an unlocated allocation category; the owner rejected it, because a writer can repair a source-valid program only from the type, the limit and the call.
- rejected: an agent proposed guessing the call from source text or symbol names; the owner rejected it, because the call origins lowering already records find the call the writer wrote.
- rejected: an agent proposed moving target representability into source acceptance; the owner rejected it, because [STOR-6] places it after checking.
