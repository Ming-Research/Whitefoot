# call-grain-args

Harness of the [call-grain-args investigation](../../investigations/call-grain-args/README.md):
the minimal pair, the build of the arms, the identity checks and the paired
timing. It runs only in CI, from `.github/workflows/tmp-call-grain.yml`, and no
gate reads it. Remove this directory with the investigation.

| File | Role |
|---|---|
| `generate.py` | emits `pair.wf` with N mixing steps per round |
| `pair_reference.h`, `pair_reference.c` | independent C reference of the pair's result |
| `pair_host.ll`, `pair_oracle.c` | native entry and prepare/call/check oracle for the pair |
| `runner.c` | timing process: warm call, timed calls, hand-out count |
| `Makefile` | builds one arm (compiler, flag, runtime revision) |
| `identity.py` | LLVM comparison of the kernels and disassembly comparison of the sequential clones |
| `stats.py` | paired ratios, bootstrap interval, noise rule, states |
| `run.py` | commands `calibrate build identity verify size measure summarize` |
| `selftest.py` | checks that the instruments fail on the wrong input they exist to catch |

Commands run in CI as the workflow lists them; the interpretation of every
output line is in the investigation README.
