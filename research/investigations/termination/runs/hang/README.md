# Hang reproduction

**Drivers.** Both drivers were built from a Snowghost checkout at the named
revision, in `renderer/`, with Snowghost's pinned compiler (Whitefoot
290b575b):

    whitefoot/compiler/target/gate/whitefootc --graph modules.wfg --entry html_tree_oracle -o <driver>

| Driver | Revision | sha256 |
|---|---|---|
| oracle_09d33ba | 09d33ba | 06d585057bc23828fe5b940ff8040460e1a8955721a8a0e4f6501e35168134b2 |
| oracle_1120edf | 1120edf | 0b33e3cb7f63df6c147c635ba73d601c103b27a205baf72581dc3df0156d9a23 |

**Runs.** Each case in `inputs.dat` was written to its own file. It was run
from the Snowghost root with 1120edf's `tests/html/tree_oracle.py`, an empty
tests directory and a 20-second limit:

    python3 -B tree_oracle.py <empty dir> <driver> <case.dat>

`results.tsv` records each result. `control.dat` is a passing input that
shows the pre-fix driver runs at all.
