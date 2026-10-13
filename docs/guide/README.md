# User guide

- [Getting started](getting-started.md)
- Language tour — to come
- Standard library reference — to come
- Guide to fixing rejections — to come

## Writing examples

The gate checks every Markdown file under `docs/`, plus the repository's
root `README.md` and `README.zh-CN.md`. A `whitefoot` fence holds a complete
Whitefoot program in canonical layout. Its info string selects the check:

- `whitefoot`: require the acceptance that `whitefootc --check` reports,
  without lowering, linking or running.
- `whitefoot run exit=N`: compile and link, then run with no arguments and
  require exit status `N`, from 0 through 255.
- `whitefoot reject RULE`: require a source rejection whose first diagnostic
  cites `RULE`.

Any form may add `file=NAME`, where `NAME` is a nonempty ASCII file name
containing only letters, digits, dots, hyphens or underscores, without
directories. The names `.` and `..` are not allowed. The default is
`example.wf`. This name appears in the diagnostic's locations; the line
numbers count from the start of the program, not from the Markdown file.

A rejection block may be followed by a `whitefoot-output` fence, with only
blank lines between the two blocks. That fence holds the full expected
rejection text. The check compares it byte for byte, including its final
newline, with the text the compiler's command-line renderer prints. The
`whitefoot-output` marker takes no attributes.

Write fragments as `text` fences or inline code. Other fences, including
`sh`, `text` and fences with no info string, are not programs and are not
compiled. Those markers take no attributes: `sh foo=bar` and
`text file=example.wf` fail the check. An unknown attribute or two outcomes
on a `whitefoot` fence also fails it. Each failure names the Markdown file
and line.

Fences use at least three backticks or tildes. The closing fence uses the
same character, is at least as long as the opening fence, and has no info
string. Use spaces to indent fences inside lists or block quotes. The
check reports unsupported tab indentation in those containers.
