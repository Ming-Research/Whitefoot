# Halo library comparison results

21 script/budget comparisons; 21 equal; 0 mismatched.

Reference-only C bootstrap installs callbacks from Redis 7.0.15 script_lua.c and links its original rand.c. Each corpus source is identical.

```json
{
  "source_revision": "7775cd8206b63e933318fad4e25ed4354a268afd",
  "source_sha256": {
    "lib/halo/vm/LICENSE.md": "d4f4b70cb6c589fa99130eaeb6c27daf5c52b634f4bf95dcd114d71a1dea44d1",
    "lib/halo/vm/builtins.wf": "3ad248a13b2fe02d84d1e5e4c91d7884d9449336109fd073d9d3a42607eba538",
    "lib/halo/vm/calls.wf": "d863461dbba0f1e9f39b6151bfc1cbf8db3055f01de0de1dffbe32f498a3d264",
    "lib/halo/vm/collect.wf": "2875232dbf0186415be8e521c0a2d14614638be8173ba2fcc79eb85df6915e63",
    "lib/halo/vm/continuations.wf": "9d2feb734c7caed6d6d65817181aa04c879c1dd06302b697f916bf952f04c599",
    "lib/halo/vm/dispatch.wf": "fe463f69749d93914ba4f87bcd5e0ec475ccaeb26a3adfd79fbbd75452de52c3",
    "lib/halo/vm/handlers.wf": "e12124a4157eb4776345f8fe4afec268c500ced11a52225172ac20441ab94eb8",
    "lib/halo/vm/instructions.wf": "e215c7d624b10adac7a955253f8dab2556c04f2da9489f9635fd733296ad412f",
    "lib/halo/vm/library-base.wf": "d5d82f00aa5220f75f4ee15d8437539dc72c3573704e587cf3ed049b12f4c10c",
    "lib/halo/vm/library-callbacks.wf": "392f146cf64a5e25c8bbabc5dcdeecc3472004272b4a94b7204337871235225c",
    "lib/halo/vm/library-decimal.wf": "0c01d351582ec88f6101dcad997c4a3bbd7fcde0ce410761489962b9214cf2a0",
    "lib/halo/vm/library-dispatch.wf": "77bc0efd5bb2e25c95ef90b92826ffe98ffb45933cd5eac7db1c10014dff37bd",
    "lib/halo/vm/library-format.wf": "7858440f1dc23625793459b10ccb56fda64249e46c025ddcd4f388fbe70f875e",
    "lib/halo/vm/library-hyperbolic.wf": "df2c7425d93ca4907a6f3f5101ced4505a448e2a684dfcfd123bab0a95550bde",
    "lib/halo/vm/library-libm.wf": "49ebf733338dd82c43d53e82f4fe5ff8ef2bcafb23fc3e1c8724cd079cfe9196",
    "lib/halo/vm/library-math.wf": "dba77cf46e402a2e40b9e37c0992279b1f95734111c03e078ee32c6db60b9e4f",
    "lib/halo/vm/library-sort.wf": "abbb875ed2efe0e8fd6916317948439019386b19f28a8010a5b51d22e8636ad3",
    "lib/halo/vm/library-string.wf": "870c37a1ef0fc95f2996d9bf0b20dcfcb25364a355d3b0a3a418edb7274eac58",
    "lib/halo/vm/library-table.wf": "2086e8ff857fdee8dc676df4bb69954fd3b1c732e41ffb0630c82f747cb0afd1",
    "lib/halo/vm/library-trig.wf": "ec5fdbf5b2ae83cca40bd9028dbf17d686d4ff743a698c25a4cd08ec50b31605",
    "lib/halo/vm/library.wf": "a1758ddfed64a4f037a88828e4f71da0dd1faf9a94ee284a3770e9de5f152e51",
    "lib/halo/vm/module.wfm": "70647320aea7100a507a5e512a64d0853a2624ce900759f8b899443b470ee088",
    "lib/halo/vm/slow.wf": "e9b652174b8824364192b4b23af6ebcbd0eb1c1f3d6d507fdd1da7c8f21c6cca",
    "lib/halo/vm/state.wf": "44aa9161f8b847345be6d7b9939e8934f3f291340dbee7f63575afad1d27b7e2",
    "research/experiments/halo-lib/base.lua": "c8625e9512e332ce6f5e82edc90f891f6c173106a7a977cfa79029d34cdedd37",
    "research/experiments/halo-lib/errors.lua": "6b1cd9ee0c88216bc48252af7d8475deb53227d940f1968cc1b7f77452c074ca",
    "research/experiments/halo-lib/math.lua": "94fa8a2a02d1e2bdfc6a179a36c5cce0a691c610e404d7451040982b27a0e6e0",
    "research/experiments/halo-lib/numeric.lua": "464e90d62ec5d76ba8e9a10b27f5520eb5cd40327db5459030ef56c405a28360",
    "research/experiments/halo-lib/random.lua": "8bf555f9849c7a660d559a005953a11f075e09effdf83ddc917604b25b50f399",
    "research/experiments/halo-lib/reference.c": "6cb2ae2e3bce6592098340bc83617711701888f9cd0087963bb8b4ab97afa3d7",
    "research/experiments/halo-lib/reference.lua": "95ae4a1334f2924ad3f57e14240a4ba99a994b0955cc2c8351fffa85d1f592f4",
    "research/experiments/halo-lib/run.py": "5d06a811053554604f0fe93cb0ef2bd208540b9655a213ca40ef25b608804740",
    "research/experiments/halo-lib/strings.lua": "287a3124aa459d324e8c39d33d68dc1d47ba2a9fff09997fb03f33f7010028e7",
    "research/experiments/halo-lib/tables.lua": "741e4ba0fad636815c5baf147c2116f6d2c3c6005707e89a135a2b3461f1b83a",
    "Reference lua.o": "6053685a58fa1f6485b95ed3a18a1e1204933596bac75b4f6719cdd500b90e11",
    "Reference liblua.a": "40c361395e506d975b9c9b4fa17f117db801ee57723379d16605a1d706f715e0",
    "Redis deps/lua/src/linit.c": "c0c97c45862f6c978620a9682cd2201f0fc29c2afbcab824ba20d26cc1745e56",
    "Redis src/rand.c": "9b5736358366ff72ec8375fdc4633a7cc162db13c877d3cb8fc31c65b44a1354",
    "Redis src/script_lua.c": "478849e42481123796c6758f3cbbf16154098de2adc8c7548e8f099b88dd0a4c"
  },
  "adapter_sha256": "b5ee1e73d37ea2907b06f103bb05233717031820797396c6be728ce9754c84ff",
  "compiler_sha256": "58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11",
  "lua_sha256": "dd2f2bb469b8c423292e23a5ab0dea2c4f3ea23658b76d55cd5f296d5d94e91e",
  "rows": [
    {
      "script": "numeric",
      "budget": 18446744073709551615,
      "lines": 5,
      "equal": true,
      "seconds": 0.005251625087112188
    },
    {
      "script": "numeric",
      "budget": 7,
      "lines": 5,
      "equal": true,
      "seconds": 0.005322250071913004
    },
    {
      "script": "numeric",
      "budget": 1,
      "lines": 5,
      "equal": true,
      "seconds": 0.006027417257428169
    },
    {
      "script": "base",
      "budget": 18446744073709551615,
      "lines": 22,
      "equal": true,
      "seconds": 0.0045373328030109406
    },
    {
      "script": "base",
      "budget": 7,
      "lines": 22,
      "equal": true,
      "seconds": 0.004201625008136034
    },
    {
      "script": "base",
      "budget": 1,
      "lines": 22,
      "equal": true,
      "seconds": 0.00445404089987278
    },
    {
      "script": "strings",
      "budget": 18446744073709551615,
      "lines": 17,
      "equal": true,
      "seconds": 0.0038302079774439335
    },
    {
      "script": "strings",
      "budget": 7,
      "lines": 17,
      "equal": true,
      "seconds": 0.0036780829541385174
    },
    {
      "script": "strings",
      "budget": 1,
      "lines": 17,
      "equal": true,
      "seconds": 0.003535791765898466
    },
    {
      "script": "tables",
      "budget": 18446744073709551615,
      "lines": 13,
      "equal": true,
      "seconds": 0.0032432912848889828
    },
    {
      "script": "tables",
      "budget": 7,
      "lines": 13,
      "equal": true,
      "seconds": 0.0033507919870316982
    },
    {
      "script": "tables",
      "budget": 1,
      "lines": 13,
      "equal": true,
      "seconds": 0.003191041760146618
    },
    {
      "script": "math",
      "budget": 18446744073709551615,
      "lines": 11,
      "equal": true,
      "seconds": 0.0030587497167289257
    },
    {
      "script": "math",
      "budget": 7,
      "lines": 11,
      "equal": true,
      "seconds": 0.0030904579907655716
    },
    {
      "script": "math",
      "budget": 1,
      "lines": 11,
      "equal": true,
      "seconds": 0.0030025001615285873
    },
    {
      "script": "random",
      "budget": 18446744073709551615,
      "lines": 16,
      "equal": true,
      "seconds": 0.0030922922305762768
    },
    {
      "script": "random",
      "budget": 7,
      "lines": 16,
      "equal": true,
      "seconds": 0.003059291746467352
    },
    {
      "script": "random",
      "budget": 1,
      "lines": 16,
      "equal": true,
      "seconds": 0.003156291786581278
    },
    {
      "script": "errors",
      "budget": 18446744073709551615,
      "lines": 13,
      "equal": true,
      "seconds": 0.0032879579812288284
    },
    {
      "script": "errors",
      "budget": 7,
      "lines": 13,
      "equal": true,
      "seconds": 0.0031219585798680782
    },
    {
      "script": "errors",
      "budget": 1,
      "lines": 13,
      "equal": true,
      "seconds": 0.0031111659482121468
    }
  ],
  "mismatches": []
}
```

Additional validation:

- Direct `whitefootc --graph lib/halo/modules.wfg --check-modules`: exit 0,
  no diagnostics, on the VM source committed in `07e35a5ff`; later commits
  change experiment code and license prose, not WF module bodies.
- Native adapter compilation through `modules.wfg`, entry `run`: exit 0.
  The adapter compiles Lua sources with `pkg::compile::compile` and executes
  them through `pkg::vm::start`, resuming every budget exit.
- Comparator controls detected changed bytes, removed lines and reordered
  lines. The adapter's initial EOF handling failed with exit 2 and was fixed
  to distinguish `ReadEnd` from `ReadFailed`.
- The initial Lua-only Redis bootstrap lost error locations through tail
  calls, giving three mismatched comparisons. The native reference shim
  fixes that oracle defect; the VM random implementation was unchanged.
- `git diff --check` and Python syntax compilation passed. No Cargo, full
  repository gate, network operation, push or pull request was run.

Coverage is seven scripts, 97 unique output lines and 291 compared lines.
The corpus calls all 60 installed functions, the private ipairs iterator,
and both math constants. Sort includes a full comparator side-effect trace,
a reversed comparator and the invalid-order error; callbacks also run with
budgets 7 and 1. Format covers every requested conversion, flags, widths,
precisions, binary strings and scanner errors. The numeric script covers
arithmetic and numeric-for string coercion, concatenation and exponentiation.

Remaining uncertainty and findings:

- The numerical ports have selected normal, small and very large argument
  observations. This is not an exhaustive error bound or correctly rounded
  claim for all binary64 inputs. The reduction adaptation and port lineage
  are documented in `lib/halo/vm/LICENSE.md`.
- PUC object address text necessarily differs from Halo handle text. The
  corpus compares scalar and explicit `__tostring` results. Host print is
  only a scalar oracle adapter.
- `Script` lacks a chunk source-name field, so VM error locations retain the
  existing `user_script` naming convention. General chunk-name propagation
  requires changes to the value/compiler owners outside this task's scope.
- C conversions outside their representable range and enormous intervals
  that overflow PUC's signed C arithmetic remain platform-dependent and are
  not covered by these scripts.
- Decimal and log/exp internal helpers duplicate the existing number package
  because changing that package's exports was outside the allowed files.
  Share those helpers when that interface is next in scope.
- Native generic-template checking grew substantially for this adapter. A
  process sample showed entailment flow walking and flow joins; no isolated
  cause or compiler change is claimed. Reopen with a reduced checker witness
  if this build cost blocks the next Halo experiment.
- These deferred cross-owner findings are recorded here because the task
  permits edits only under the VM and this experiment; `docs/todo.md` and
  design-tree updates were outside the authorized file boundary.

No specification rule or conformance evidence changed.
