# Handle-count arms

Inputs of [the handle-count investigation](../../investigations/handle-count/README.md),
which registers the question, the arms and the decision rule.

- `value.wf`, `reference.wf`, `keys.wf`, `module.wfm`, `modules.wfg`: the two
  arms (entries `handle_value`, `handle_reference`) and the map seeding and
  verification they share.
- `harness.wf`: the gate, deadline, totals and report line, copied unchanged
  from `research/experiments/shared-counters/harness.wf` on branch
  `claude/shared-counter-arms` (itself ported from the owner's Firn-wf
  runtime-contention harness). Its read-count fields stay zero here.
- `build.sh --pr C --base C --out DIR`: builds the binaries and checks the
  handle-count calls in the emitted LLVM.
- `run.sh --out DIR --bins DIR`: runs the arms interleaved and writes `raw.tsv`.
- `summarize.py [--smoke | --pilot] RAW.tsv`: the registered reading.

`.github/workflows/tmp-handle-count.yml` runs them; it is removed after the
run.
