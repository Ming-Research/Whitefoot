# Inline-policy experiment

Instruments of the [inline-policy investigation](../../investigations/inline-policy/DESIGN.md)
(status board item wf-inline-policy). Remove this directory when that
investigation closes.

- `clang-arm.sh` turns one compiler binary into an arm. whitefootc uses a
  `clang` beside its own executable when one is there
  (`design/compiler/toolchain-resolution.md`), so a directory holding a copy
  of `whitefootc`, this script copied in as `clang`, an `ld.lld` link to the
  host's lld (a sibling clang selects the packaged layout, which requires
  one), and an `arm.conf` that
  sets the arm's variables (listed in the script's header) compiles every
  program with those settings: an optimization level in place of `-O2`,
  LLVM options for compiles and LTO links, full LTO for the default build,
  and a file collecting LLVM's inline remarks. Nothing else differs between
  two such directories holding the same binary.
- `census.py reduce --name NAME FILES` reduces the collected remarks of one
  program: the decisions between Whitefoot definitions by outcome and
  reason, the thresholds met, the priced call sites by cost over threshold,
  and the sites close to it. `census.py self-test` checks the parser on
  fixed remark text and YAML.

The temporary workflow of the investigation's branch runs both; the record
cites each result with its run.
