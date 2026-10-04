# Halo end-to-end comparison

Local parent revision: `2b6cc81c6e7f39051f3a269448facf4a30180bba`. Source digest: `d3b22ea53c3abb2fc66f66059d887cb7efa8c6ff6bdac91b01247d085a2bc6d1`.
The digest includes every Halo module, the graph, host and runner; it identifies uncommitted source bytes too.

Executable SHA-256: `673901ad99e028dc31d41fa4c9e0c0c55455440cb3b8f6be3b4f567ff3da3f62`.
Compiler SHA-256: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.

| Group | Budget | Passed | Failed |
| --- | --- | ---: | ---: |
| libs | 7 | 7 | 3 |

| Case | Budget | Result | Seconds | Failure reason |
| --- | ---: | --- | ---: | --- |
| libs/bit-logical | 7 | PASS | 0.2516 |  |
| libs/bit-shifts-hex | 7 | PASS | 0.0042 |  |
| libs/cjson-arrays-nested | 7 | PASS | 0.0045 |  |
| libs/cjson-invalid | 7 | PASS | 0.0040 |  |
| libs/cjson-numbers | 7 | PASS | 0.0048 |  |
| libs/cjson-objects | 7 | PASS | 0.0039 |  |
| libs/cmsgpack-binary | 7 | FAIL | 0.0047 | missing library/host member: ERR user_script:7: attempt to call a nil value |
| libs/cmsgpack-roundtrip | 7 | PASS | 0.0048 |  |
| libs/struct-integers | 7 | FAIL | 0.0049 | missing library/host member: ERR user_script:7: attempt to call a nil value |
| libs/struct-strings-floats | 7 | FAIL | 0.0042 | missing library/host member: ERR user_script:7: attempt to call a nil value |

No scripts or expected replies were changed. No network or Redis server was used.
The reference is the stored Redis 7.0.15 RESP2 corpus. All cases are executed, including unavailable library cases.
Budget comparisons use fresh stores. They compare replies; atomic kill/restart and host Stop store preservation require separate checks.
