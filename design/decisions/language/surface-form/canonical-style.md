# Canonical style

The bytes of the one legal rendering of a program: indentation, line ends, blank lines and the spaces between tokens.

### decision two-space-lf-canonical-rendering
The canonical rendering uses two spaces per brace depth, LF-only lines with no tabs or trailing spaces, one blank line between top-level items, and closed token-attachment sets.
- rejected: an agent proposed a wider or tab-based indent, Allman-style braces, or another attachment convention; the owner rejected it, because one legal rendering needs every byte fixed, and these are the conventional values.
