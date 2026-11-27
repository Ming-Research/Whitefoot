# Sanitized builds

How the compiler builds a program under a host sanitizer, and how CI runs accepted programs under the sanitizers.

### decision sanitize-option-instruments-every-build
`whitefootc --sanitize address|thread` adds the sanitizer's attribute to every emitted definition and its clang arguments to every runtime compile, module compile, link and native-cache key.
- rejected: an agent proposed instrumenting only the test harness's own link; the owner rejected it, because outsiders could not then build their own programs sanitized, and the harness link is not the path the compiler ships.

### decision sanitizers-run-in-their-own-workflow
A Linux workflow of its own runs the corpus programs and every running conformance case, also built with `--par`, under both sanitizers, and any report fails it.
- rejected: an agent proposed running the sanitizers as a group of the gate; the owner rejected it, because the gate would run every case twice more, and the platform-specific checks stay apart from the gate.
