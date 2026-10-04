# Halo lexer comparison results

Measured on 2026-10-04 (UTC).

Host: `macOS-26.6.2-arm64-arm-64bit-Mach-O`.

## Final observation

| Observation | Count |
| --- | ---: |
| Lua oracle corpus files | 80 |
| Generated tricky inputs | 501 |
| Total files / independent sources | 581 |
| Tokens, including EOF records | 7079 |
| Terminal lexical errors | 115 |
| Mismatches | 0 |
| Wrong-output mutation controls detected | 6 |

Every file was compared byte for byte against the C oracle. The full native
comparison took 2.017 seconds after building; the Whitefoot batch took 0.247
seconds. The smallest three-file sample compared 269 tokens without a
mismatch; its batch took 0.253 seconds. These are run-sizing observations,
not a performance comparison of the lexers.

Both native dumpers built successfully with the supplied compiler and the
reference C sources. The C source and configuration files `llex.c` and
`luaconf.h` were also compared with those beside the supplied reference Lua
executable: both `cmp` invocations returned zero. A direct `loadstring`
probe with that executable confirmed its level-zero nesting diagnostic.

## Reproduction and identity

From the worktree root:

```sh
perl .github/run-check.pl halo-lex-compare python3 research/experiments/halo-lex/run.py
/private/tmp/wf-halo/compiler/target/gate/whitefootc --graph lib/halo/modules.wfg --check-modules
make static
```

The lexer and the dump graph pass module checking. All `make static` groups
pass: repository invariants, specification archives, README translation,
specification prose, guidance, source size and design form. `git diff --check`
passes. No compiler Cargo build or full `make check` was run.

The implementation was validated on the worktree based on `14726fb50afc330b057bc7ad611c2c9f9bba6415`.
Its exact source fingerprint is independent of this results document:
concatenate each sorted `lib/halo/lex/*.wf` or `*.wfm` basename, NUL, and
file bytes, then take SHA-256. The runner verifies that these bytes stay
unchanged through the comparison.

| Artifact | SHA-256 |
| --- | --- |
| Lexer sources | `f80a01bce64c5e2c32b6939751e9fe44695c88fd6ee2a580f1af41f36d3af099` |
| Supplied compiler executable | `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11` |
| Reference llex.c | `f1ade23f957e69f164ff28e133b201dff5d81726c390b060110b60907831e79b` |

The collection guard was added after the final native comparison and
checked independently: an empty corpus and a missing corpus each raised
the intended failure, while the maintained corpus collected 80 files.
Its two failure controls now run in the maintained command. The guard does
not change the nonempty corpus selection or either native dumper.

## Mismatches found and resolved

The first full comparison covered 576 sources, 7071 token
records and 112 errors. It reported every mismatch below:

| Input label | PUC observation | Initial Halo observation |
| --- | --- | --- |
| `tricky/long-nesting` | `fixture:1: nesting of [[...]] is deprecated near '['` | Five token records, including EOF |
| `tricky/line-limit-1` | `fixture:2147483646: chunk has too many lines near ''` | `fixture:2147483647: chunk has too many lines near ''` |
| `tricky/line-limit-2` | `fixture:2147483646: chunk has too many lines near 'and'` | `fixture:2147483647: chunk has too many lines near 'and'` |
| `tricky/line-limit-3` | `fixture:2147483646: chunk has too many lines near ''` | `fixture:2147483647: chunk has too many lines near ''` |
| `tricky/line-limit-4` | `fixture:2147483646: chunk has too many lines near ''` | `fixture:2147483647: chunk has too many lines near ''` |
| `tricky/line-limit-5` | `fixture:2147483646: chunk has too many lines near 'char(1)'` | `fixture:2147483647: chunk has too many lines near 'char(1)'` |
| `tricky/line-limit-10` | `fixture:2147483646: chunk has too many lines near '['` | `fixture:2147483647: chunk has too many lines near '[\n'` |

The reference configuration enables `LUA_COMPAT_LSTR=1`; matching nested
level-zero openers therefore raise the deprecation diagnostic immediately
in both long strings and long comments. Halo now follows that branch, and
cases cover matching levels, mismatched levels and nested openers before EOF.

The reference defines `MAX_INT` as `INT_MAX - 2`, or 2147483645 on this
host. Halo now uses that bound. The seeded witnesses were corrected to
start at `MAX_INT - 1` or `MAX_INT - 2`, rather than already at or beyond
the valid line domain. They check the previous-token context, sparse comment
buffer, long opener and escaped-newline diagnostics without allocating a
multigigabyte source. No current mismatch remains.

## Coverage limits

The evidence covers the listed finite inputs on this macOS host in the
initial C locale. Other hosts, changed C locales, allocation exhaustion and
truly multigigabyte inputs were not exercised. The seeded line witnesses
check the transition and error text; they do not measure a giant-file run.
Numeric conversion belongs to the parser and was intentionally not tested
or implemented here. The corpus comparison checks lexical output, not
Lua execution, parser behavior or VM performance.

The script and both dumpers remain explicitly invoked research tooling;
no maintained compiler or conformance gate imports this experiment.
