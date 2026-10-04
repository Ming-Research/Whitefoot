# Halo slice 1 library comparison

This experiment compiles Lua source with `pkg::compile`, runs it with
`pkg::vm::start`, and compares printed text with Redis's bundled Lua 5.1.5.
It serves the slice 1 library and number wiring described in
`research/investigations/halo/VM.md`, sections 3 and 8. The adapter, corpus,
and comparison runner live here, outside the compiler gates; remove the
adapter and runner when an embedding-level oracle replaces this experiment.

Work is local to this worktree, with no Cargo invocation or network access.
The supplied compiler is `/private/tmp/wf-halo/compiler/target/gate/whitefootc`.
