# Halo library comparison results

24 script/budget comparisons; 24 equal; 0 mismatched.

Reference-only C bootstrap installs callbacks from Redis 7.0.15 script_lua.c and links its original rand.c. Each corpus source is identical.

```json
{
  "source_revision": "7e04ffb69fea817d7e513f2cb47520e6f3ad6752",
  "source_sha256": {
    "lib/halo/vm/LICENSE.md": "d4f4b70cb6c589fa99130eaeb6c27daf5c52b634f4bf95dcd114d71a1dea44d1",
    "lib/halo/vm/builtins.wf": "98c712dace52bb7b1b07c43cc1d3d5a5079a7453586db6f277f950fa52cbf7f0",
    "lib/halo/vm/calls.wf": "d863461dbba0f1e9f39b6151bfc1cbf8db3055f01de0de1dffbe32f498a3d264",
    "lib/halo/vm/collect.wf": "2875232dbf0186415be8e521c0a2d14614638be8173ba2fcc79eb85df6915e63",
    "lib/halo/vm/continuations.wf": "9d2feb734c7caed6d6d65817181aa04c879c1dd06302b697f916bf952f04c599",
    "lib/halo/vm/dispatch.wf": "fe463f69749d93914ba4f87bcd5e0ec475ccaeb26a3adfd79fbbd75452de52c3",
    "lib/halo/vm/handlers.wf": "e12124a4157eb4776345f8fe4afec268c500ced11a52225172ac20441ab94eb8",
    "lib/halo/vm/instructions.wf": "e215c7d624b10adac7a955253f8dab2556c04f2da9489f9635fd733296ad412f",
    "lib/halo/vm/library-base.wf": "fb9fc46d5f09471de190d2a1ae48a7f8db118b43ba50a79c28d0a4974a586b81",
    "lib/halo/vm/library-callbacks.wf": "392f146cf64a5e25c8bbabc5dcdeecc3472004272b4a94b7204337871235225c",
    "lib/halo/vm/library-decimal.wf": "0c01d351582ec88f6101dcad997c4a3bbd7fcde0ce410761489962b9214cf2a0",
    "lib/halo/vm/library-dispatch.wf": "8c40561ade58b5eb999d7b8e3e38c9e193d4df9f5a456d055f02d5b8ac843c08",
    "lib/halo/vm/library-format.wf": "49a201e16ffd9bb9c34ba425bd79d421d1e033e10298bd081ba16c13b5f1ee88",
    "lib/halo/vm/library-hyperbolic.wf": "df2c7425d93ca4907a6f3f5101ced4505a448e2a684dfcfd123bab0a95550bde",
    "lib/halo/vm/library-libm.wf": "49ebf733338dd82c43d53e82f4fe5ff8ef2bcafb23fc3e1c8724cd079cfe9196",
    "lib/halo/vm/library-math.wf": "5f942f6fa8b1241f0b22a6681e8bcbb22506e1702594e2f02a7669d997c02dc5",
    "lib/halo/vm/library-sort.wf": "c68c2f874d4bb5ec55c11624a80c40b16d3ad7d8e09590277eb0a9d91dde611c",
    "lib/halo/vm/library-string.wf": "eccfb26c443a65e9bf2094efd381706491543261591a5c2f9fbc59d9c69059e7",
    "lib/halo/vm/library-table.wf": "e660685a348ad7b50b11289fa2c0df3eb378ac0d694b3b99d181d45318f62869",
    "lib/halo/vm/library-trig.wf": "ec5fdbf5b2ae83cca40bd9028dbf17d686d4ff743a698c25a4cd08ec50b31605",
    "lib/halo/vm/library.wf": "dc1f8b74420449bcd205919729c895edfa15b7d3d5bc0fc198786898bb8c249c",
    "lib/halo/vm/module.wfm": "70647320aea7100a507a5e512a64d0853a2624ce900759f8b899443b470ee088",
    "lib/halo/vm/slow.wf": "5199c3986177ba8771d650cf45ea5dc663e7a25b67bb28fd39f092df79a9a5bd",
    "lib/halo/vm/state.wf": "19c44969c8a3860526bc36b768dafed741f7762a4739e6a1652e475ad6dccae1",
    "research/experiments/halo-lib/base.lua": "c8625e9512e332ce6f5e82edc90f891f6c173106a7a977cfa79029d34cdedd37",
    "research/experiments/halo-lib/boundaries.lua": "d0fec42de44aae6b321c308263273bb066c52087af257bf6d57cba0edb7a822a",
    "research/experiments/halo-lib/errors.lua": "6b1cd9ee0c88216bc48252af7d8475deb53227d940f1968cc1b7f77452c074ca",
    "research/experiments/halo-lib/math.lua": "0c6f369e38eff2f99e70a19a89ddb4bc4133a0edfce1f047fb07cd8f6d55b363",
    "research/experiments/halo-lib/modules.wfg": "afad44e70f456785aab4700eeb487ac33881b0e4c8392bcc4630efb5f13a9723",
    "research/experiments/halo-lib/numeric.lua": "464e90d62ec5d76ba8e9a10b27f5520eb5cd40327db5459030ef56c405a28360",
    "research/experiments/halo-lib/random.lua": "5d40907213b7782442b5f3968525515183981b917c4e136c5803532e77a58096",
    "research/experiments/halo-lib/reference.c": "6cb2ae2e3bce6592098340bc83617711701888f9cd0087963bb8b4ab97afa3d7",
    "research/experiments/halo-lib/reference.lua": "95ae4a1334f2924ad3f57e14240a4ba99a994b0955cc2c8351fffa85d1f592f4",
    "research/experiments/halo-lib/run.py": "6ed96b001efa41864bc0cd859f5be3c7e4ecedf92e9aa17cbde8b8add50fc373",
    "research/experiments/halo-lib/runner/module.wfm": "946e2b91cc9a9ed595b125eea83f4a799c13bddb9a2df3a96359ca8eb8910dfa",
    "research/experiments/halo-lib/runner/run.wf": "10eb0a9f5caed4bd55cffc6cdceba351d0714eb90a8b3ef2884c4d2b5b72f4eb",
    "research/experiments/halo-lib/strings.lua": "3585efeb9831ef1ef1d45260f0d77270a1cb4112921934f6b569d93b076e43e2",
    "research/experiments/halo-lib/tables.lua": "741e4ba0fad636815c5baf147c2116f6d2c3c6005707e89a135a2b3461f1b83a",
    "Reference lua.o": "6053685a58fa1f6485b95ed3a18a1e1204933596bac75b4f6719cdd500b90e11",
    "Reference liblua.a": "40c361395e506d975b9c9b4fa17f117db801ee57723379d16605a1d706f715e0",
    "Redis deps/lua/src/linit.c": "c0c97c45862f6c978620a9682cd2201f0fc29c2afbcab824ba20d26cc1745e56",
    "Redis src/rand.c": "9b5736358366ff72ec8375fdc4633a7cc162db13c877d3cb8fc31c65b44a1354",
    "Redis src/script_lua.c": "478849e42481123796c6758f3cbbf16154098de2adc8c7548e8f099b88dd0a4c"
  },
  "adapter_sha256": "06e096157929c0bcc8c69ede0db0c87de6be8e89ee0867391a29cfeea01672b1",
  "compiler_sha256": "58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11",
  "lua_sha256": "dd2f2bb469b8c423292e23a5ab0dea2c4f3ea23658b76d55cd5f296d5d94e91e",
  "rows": [
    {
      "script": "numeric",
      "budget": 18446744073709551615,
      "lines": 5,
      "equal": true,
      "seconds": 0.005758291110396385
    },
    {
      "script": "numeric",
      "budget": 7,
      "lines": 5,
      "equal": true,
      "seconds": 0.0053870826959609985
    },
    {
      "script": "numeric",
      "budget": 1,
      "lines": 5,
      "equal": true,
      "seconds": 0.005554042290896177
    },
    {
      "script": "base",
      "budget": 18446744073709551615,
      "lines": 22,
      "equal": true,
      "seconds": 0.004188165999948978
    },
    {
      "script": "base",
      "budget": 7,
      "lines": 22,
      "equal": true,
      "seconds": 0.004366250243037939
    },
    {
      "script": "base",
      "budget": 1,
      "lines": 22,
      "equal": true,
      "seconds": 0.004373874980956316
    },
    {
      "script": "strings",
      "budget": 18446744073709551615,
      "lines": 18,
      "equal": true,
      "seconds": 0.004696500021964312
    },
    {
      "script": "strings",
      "budget": 7,
      "lines": 18,
      "equal": true,
      "seconds": 0.004136375151574612
    },
    {
      "script": "strings",
      "budget": 1,
      "lines": 18,
      "equal": true,
      "seconds": 0.004422042053192854
    },
    {
      "script": "tables",
      "budget": 18446744073709551615,
      "lines": 13,
      "equal": true,
      "seconds": 0.003912624903023243
    },
    {
      "script": "tables",
      "budget": 7,
      "lines": 13,
      "equal": true,
      "seconds": 0.0033529591746628284
    },
    {
      "script": "tables",
      "budget": 1,
      "lines": 13,
      "equal": true,
      "seconds": 0.0036782086826860905
    },
    {
      "script": "math",
      "budget": 18446744073709551615,
      "lines": 17,
      "equal": true,
      "seconds": 0.0035151252523064613
    },
    {
      "script": "math",
      "budget": 7,
      "lines": 17,
      "equal": true,
      "seconds": 0.00342083303257823
    },
    {
      "script": "math",
      "budget": 1,
      "lines": 17,
      "equal": true,
      "seconds": 0.003323583398014307
    },
    {
      "script": "random",
      "budget": 18446744073709551615,
      "lines": 17,
      "equal": true,
      "seconds": 0.003197791986167431
    },
    {
      "script": "random",
      "budget": 7,
      "lines": 17,
      "equal": true,
      "seconds": 0.003242999780923128
    },
    {
      "script": "random",
      "budget": 1,
      "lines": 17,
      "equal": true,
      "seconds": 0.0031840833835303783
    },
    {
      "script": "errors",
      "budget": 18446744073709551615,
      "lines": 13,
      "equal": true,
      "seconds": 0.003155624959617853
    },
    {
      "script": "errors",
      "budget": 7,
      "lines": 13,
      "equal": true,
      "seconds": 0.0031676669605076313
    },
    {
      "script": "errors",
      "budget": 1,
      "lines": 13,
      "equal": true,
      "seconds": 0.0031855828128755093
    },
    {
      "script": "boundaries",
      "budget": 18446744073709551615,
      "lines": 20,
      "equal": true,
      "seconds": 0.003813875373452902
    },
    {
      "script": "boundaries",
      "budget": 7,
      "lines": 20,
      "equal": true,
      "seconds": 0.003910125233232975
    },
    {
      "script": "boundaries",
      "budget": 1,
      "lines": 20,
      "equal": true,
      "seconds": 0.0039037498645484447
    }
  ],
  "mismatches": []
}
```

## Validation

The implementation revision is `7e04ffb69fea817d7e513f2cb47520e6f3ad6752`.
The JSON above identifies the VM, adapter sources, corpus, oracle sources,
compiler and executables by SHA-256. The final comparison runner additionally
records the adapter graph and WF sources; its changes after that revision only
extend this identity record. All recorded hashes were checked against the files
used, including the adapter executable.

- Direct `/private/tmp/wf-halo/compiler/target/gate/whitefootc --graph lib/halo/modules.wfg --check-modules`: exit 0; all six modules accepted; wall 225.45 s, user 136.86 s, system 26.05 s.
- Native adapter build with the same compiler, `--graph research/experiments/halo-lib/modules.wfg --cache /private/tmp/halo-lib-cache --entry run -o /private/tmp/halo-lib-run-deliver`: exit 0; wall 367.74 s, user 256.78 s, system 41.81 s.
- Eight compiled Lua scripts at budgets unlimited, 7 and 1: 24 comparisons equal, zero mismatches; 125 output lines per corpus pass, 375 lines compared across the budgets. The adapter compiles each unchanged Lua source through `pkg::compile`, starts its script through `pkg::vm::start`, and resumes budget stops.
- Comparator controls changed a byte, dropped a line and reversed lines, both generically and for each script's reference output. Every mutation produced a mismatch. The independently relinked Redis random oracle also matched the supplied executable for every non-random script.
- `git diff --check` and Python syntax compilation passed. No Cargo, network, push, pull request, or full repository gate was used.

The initial small sample sized execution before the full corpus. Builds and
module checking were measured separately from script execution. These timings
are observations on this host, not a performance comparison.

## Coverage and review

The corpus invokes all 60 installed library functions, the private `ipairs`
iterator, and the two numeric constants. It covers arithmetic and numeric-loop
string coercion, concat formatting and numeric power; protected callbacks and
metatables; format conversions, flags, widths, precision, binary strings and
errors; table operations and sort comparator traces; Redis random sequences and
interval errors; and selected floating boundaries including huge arguments,
subnormals, signed zero, infinities and NaN. The boundary script separates
accepted multi-result sizes from the PUC stack limit and exercises C-int
narrowing rather than only small integers.

A separate read-only `gpt-6.1-sol` reviewer inspected the complete diff from
`e543203ba5a66a8e9fec30ad28b7d7e09c1919ef`, the relevant current guidance and
VM design, and groups A, D, C, M and V of the repository review checklist.
Specification/conformance and gate-specific groups were outside the changed
scope. The review included design correspondence; no tree or specification
changed under this task's explicit file boundary. Follow-up review inspected
the repaired logic through the implementation revision above and the final
runtime evidence. Green suites were not rerun by the reviewer.

Findings fixed and checked against the reference included NUL `%c` padding,
alternate octal precision, PUC C-int narrowing, callback error locations across
budget resumes, multi-result capacity and byte error text, and concat's choice
of erroneous operand. Additional probes fixed iterator argument validation,
Redis interval-width overflow, native callback activation handling, and NaN
format flags. The repaired paths are represented in the corpus. No source
review finding remains open within this scope.

## Remaining limits and findings along the way

- These observations do not establish correctly rounded results for all
  binary64 inputs or identical last bits on every host libm. The musl-derived
  kernels, the changed large-angle reduction, and license notices are documented
  in `lib/halo/vm/LICENSE.md`. Reopen numerical accuracy with an independent
  binary64 oracle when a real workload needs a stronger bound; compare errors
  in ulps, including reduction boundaries and subnormal inputs.
- PUC default object strings contain addresses; Halo uses handles. The host
  print adapter covers scalar text, not an embedding's full print builtin.
  `tostring` metamethod results are tested explicitly. Different host NaN
  spellings and platform-dependent C conversions need their own oracle.
- The existing Script lacks original source-name and local debug-name metadata.
  Locations use `user_script` and registered builtin names. Alias and method
  diagnostic names are not fully PUC-equivalent. Reopen in the value/compiler
  owners when debug fidelity is required, adding independent alias, method and
  source-name comparisons before changing metadata.
- Exact-decimal and log/exp helpers duplicate package-internal number helpers
  because changes to that package were excluded. Consolidate through shared
  exports when that interface is in scope; preserve this corpus and test the
  shared numeric kernels against independent reference values.
- Module checking and adapter building are much slower than executing the
  corpus. A compiler process sample showed checker entailment/flow-join work,
  but did not isolate a cause. Reopen with a reduced module witness if this
  blocks the next VM experiment; measure checker and native build phases
  separately before attributing the cost.

These cross-owner follow-ups are recorded here because `docs/todo.md` and the
other package owners are outside the permitted files. No specification rule,
conformance expectation, released archive, or standing project rule changed.

## Later change

After these results the lead reverted `7e04ffb69` ("Match the supplied Lua
printf NaN flag behavior"), which had made `string.format` drop the sign and
the `+` and space flags for a NaN as macOS printf does. Halo follows glibc on
Linux, firn's platform reference (VM.md, H4), which prints `-nan`, `+nan` and
` nan`; the macOS oracle disagrees there, so the `formatnan` line left the
corpus and the Linux rerun is recorded in docs/todo.md.
