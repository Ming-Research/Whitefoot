# Halo end-to-end comparison

Local parent revision: `cfabeeeacdfd6c6524932e0e228c68184c1ab82a`. Source digest: `2b14d8b1c2ca269cbacd8f05108f6d41f7bf14b057b93ed4ac7d610e02f5854b`.
The digest includes every Halo module, the graph, host and runner; it identifies uncommitted source bytes too.

Executable SHA-256: `a066ab2f264638679b3e5751b09074af607935ac7bff2b7533f5babb3630d267`.
Compiler SHA-256: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.

| Group | Budget | Passed | Failed |
| --- | --- | ---: | ---: |
| apps | 1 | 4 | 2 |
| apps | 7 | 4 | 2 |
| apps | 1000 | 4 | 2 |
| libs | 1 | 0 | 10 |
| libs | 7 | 0 | 10 |
| libs | 1000 | 0 | 10 |
| lua-core | 1 | 16 | 32 |
| lua-core | 7 | 16 | 32 |
| lua-core | 1000 | 16 | 32 |
| redis-api | 1 | 12 | 4 |
| redis-api | 7 | 12 | 4 |
| redis-api | 1000 | 12 | 4 |

| Case | Budget | Result | Seconds | Failure reason |
| --- | ---: | --- | ---: | --- |
| apps/hash-cas | 1 | PASS | 0.0048 |  |
| apps/hash-cas | 7 | PASS | 0.0035 |  |
| apps/hash-cas | 1000 | PASS | 0.0034 |  |
| apps/lock-extend | 1 | PASS | 0.0036 |  |
| apps/lock-extend | 7 | PASS | 0.0039 |  |
| apps/lock-extend | 1000 | PASS | 0.0034 |  |
| apps/queue-move | 1 | PASS | 0.0045 |  |
| apps/queue-move | 7 | PASS | 0.0034 |  |
| apps/queue-move | 1000 | PASS | 0.0035 |  |
| apps/rate-limiter | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/rate-limiter | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/rate-limiter | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/redlock-release | 1 | PASS | 0.0038 |  |
| apps/redlock-release | 7 | PASS | 0.0034 |  |
| apps/redlock-release | 1000 | PASS | 0.0033 |  |
| apps/sliding-window | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/sliding-window | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/sliding-window | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| libs/bit-logical | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-logical | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-logical | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/cjson-arrays-nested | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-arrays-nested | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-arrays-nested | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 1000 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cmsgpack-binary | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-binary | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-binary | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/struct-integers | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-integers | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-integers | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| lua-core/array-holes | 1 | PASS | 0.0036 |  |
| lua-core/array-holes | 7 | PASS | 0.0035 |  |
| lua-core/array-holes | 1000 | PASS | 0.0035 |  |
| lua-core/assert | 1 | FAIL | 0.0037 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/assert | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/assert | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/concat-numbers | 1 | FAIL | 0.0032 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/concat-numbers | 7 | FAIL | 0.0032 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/concat-numbers | 1000 | FAIL | 0.0033 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/counter-closure | 1 | PASS | 0.0036 |  |
| lua-core/counter-closure | 7 | PASS | 0.0033 |  |
| lua-core/counter-closure | 1000 | PASS | 0.0033 |  |
| lua-core/embedded-zero | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/embedded-zero | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/embedded-zero | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/error-levels | 1 | PASS | 0.0036 |  |
| lua-core/error-levels | 7 | PASS | 0.0033 |  |
| lua-core/error-levels | 1000 | PASS | 0.0031 |  |
| lua-core/error-values | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/error-values | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/error-values | 1000 | FAIL | 0.0040 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/float-print | 1 | FAIL | 0.0037 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/float-print | 7 | FAIL | 0.0038 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/float-print | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/format-14g | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/format-14g | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/format-14g | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/integer-doubles | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/integer-doubles | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/integer-doubles | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/ipairs | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/ipairs | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/ipairs | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/math-powers | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-powers | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-powers | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/meta-call | 1 | PASS | 0.0033 |  |
| lua-core/meta-call | 7 | PASS | 0.0033 |  |
| lua-core/meta-call | 1000 | PASS | 0.0033 |  |
| lua-core/meta-comparisons | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-comparisons | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-comparisons | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-concat-tostring | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-concat-tostring | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-concat-tostring | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-index | 1 | PASS | 0.0033 |  |
| lua-core/meta-index | 7 | PASS | 0.0032 |  |
| lua-core/meta-index | 1000 | PASS | 0.0032 |  |
| lua-core/meta-le-fallback | 1 | PASS | 0.0032 |  |
| lua-core/meta-le-fallback | 7 | PASS | 0.0032 |  |
| lua-core/meta-le-fallback | 1000 | PASS | 0.0032 |  |
| lua-core/meta-newindex | 1 | PASS | 0.0032 |  |
| lua-core/meta-newindex | 7 | PASS | 0.0032 |  |
| lua-core/meta-newindex | 1000 | PASS | 0.0033 |  |
| lua-core/meta-protection-raw | 1 | FAIL | 0.0042 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-protection-raw | 7 | FAIL | 0.0039 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-protection-raw | 1000 | FAIL | 0.0037 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-table-length | 1 | PASS | 0.0033 |  |
| lua-core/meta-table-length | 7 | PASS | 0.0032 |  |
| lua-core/meta-table-length | 1000 | PASS | 0.0032 |  |
| lua-core/metamethod-error | 1 | PASS | 0.0034 |  |
| lua-core/metamethod-error | 7 | PASS | 0.0032 |  |
| lua-core/metamethod-error | 1000 | PASS | 0.0032 |  |
| lua-core/multiple-returns | 1 | PASS | 0.0035 |  |
| lua-core/multiple-returns | 7 | PASS | 0.0033 |  |
| lua-core/multiple-returns | 1000 | PASS | 0.0033 |  |
| lua-core/negative-division | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/negative-division | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/negative-division | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/next-pairs | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/next-pairs | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/next-pairs | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/nil-returns | 1 | PASS | 0.0034 |  |
| lua-core/nil-returns | 7 | PASS | 0.0032 |  |
| lua-core/nil-returns | 1000 | PASS | 0.0033 |  |
| lua-core/nonfinite | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/nonfinite | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/nonfinite | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/numeric-coercion | 1 | FAIL | 0.0032 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/numeric-coercion | 7 | FAIL | 0.0034 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/numeric-coercion | 1000 | FAIL | 0.0033 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/pcall-xpcall | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/pcall-xpcall | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/pcall-xpcall | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/shared-upvalues | 1 | PASS | 0.0034 |  |
| lua-core/shared-upvalues | 7 | PASS | 0.0032 |  |
| lua-core/shared-upvalues | 1000 | PASS | 0.0033 |  |
| lua-core/string-byte-char | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-byte-char | 7 | FAIL | 0.0031 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-byte-char | 1000 | FAIL | 0.0031 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-escapes | 1 | PASS | 0.0031 |  |
| lua-core/string-escapes | 7 | PASS | 0.0031 |  |
| lua-core/string-escapes | 1000 | PASS | 0.0032 |  |
| lua-core/string-find | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-find | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-find | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-length-order | 1 | PASS | 0.0033 |  |
| lua-core/string-length-order | 7 | PASS | 0.0033 |  |
| lua-core/string-length-order | 1000 | PASS | 0.0032 |  |
| lua-core/string-match | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-match | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-match | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/table-concat | 1 | FAIL | 0.0031 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-concat | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-concat | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-parts | 1 | PASS | 0.0032 |  |
| lua-core/table-parts | 7 | PASS | 0.0033 |  |
| lua-core/table-parts | 1000 | PASS | 0.0032 |  |
| lua-core/table-sort | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-sort | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-sort | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/tail-recursion | 1 | PASS | 0.9796 |  |
| lua-core/tail-recursion | 7 | PASS | 0.1424 |  |
| lua-core/tail-recursion | 1000 | PASS | 0.0061 |  |
| lua-core/unpack-select-varargs | 1 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| lua-core/unpack-select-varargs | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| lua-core/unpack-select-varargs | 1000 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| redis-api/call-error | 1 | FAIL | 0.0033 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-error | 7 | FAIL | 0.0033 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-error | 1000 | FAIL | 0.0033 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-success | 1 | PASS | 0.0034 |  |
| redis-api/call-success | 7 | PASS | 0.0032 |  |
| redis-api/call-success | 1000 | PASS | 0.0032 |  |
| redis-api/error-reply | 1 | PASS | 0.0032 |  |
| redis-api/error-reply | 7 | PASS | 0.0031 |  |
| redis-api/error-reply | 1000 | PASS | 0.0033 |  |
| redis-api/global-read | 1 | FAIL | 0.0032 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-read | 7 | FAIL | 0.0033 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-read | 1000 | FAIL | 0.0033 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-write | 1 | FAIL | 0.0034 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/global-write | 7 | FAIL | 0.0033 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/global-write | 1000 | FAIL | 0.0033 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/log-keys-argv | 1 | PASS | 0.0033 |  |
| redis-api/log-keys-argv | 7 | PASS | 0.0035 |  |
| redis-api/log-keys-argv | 1000 | PASS | 0.0033 |  |
| redis-api/lua-arrays | 1 | PASS | 0.0032 |  |
| redis-api/lua-arrays | 7 | PASS | 0.0033 |  |
| redis-api/lua-arrays | 1000 | PASS | 0.0032 |  |
| redis-api/lua-nil | 1 | PASS | 0.0033 |  |
| redis-api/lua-nil | 7 | PASS | 0.0033 |  |
| redis-api/lua-nil | 1000 | PASS | 0.0033 |  |
| redis-api/lua-scalars | 1 | PASS | 0.0032 |  |
| redis-api/lua-scalars | 7 | PASS | 0.0031 |  |
| redis-api/lua-scalars | 1000 | PASS | 0.0032 |  |
| redis-api/pcall-success-error | 1 | PASS | 0.0033 |  |
| redis-api/pcall-success-error | 7 | PASS | 0.0032 |  |
| redis-api/pcall-success-error | 1000 | PASS | 0.0033 |  |
| redis-api/resp-error-raised | 1 | PASS | 0.0033 |  |
| redis-api/resp-error-raised | 7 | PASS | 0.0032 |  |
| redis-api/resp-error-raised | 1000 | PASS | 0.0033 |  |
| redis-api/resp-values | 1 | PASS | 0.0038 |  |
| redis-api/resp-values | 7 | PASS | 0.0034 |  |
| redis-api/resp-values | 1000 | PASS | 0.0033 |  |
| redis-api/return-error-table | 1 | PASS | 0.0033 |  |
| redis-api/return-error-table | 7 | PASS | 0.0032 |  |
| redis-api/return-error-table | 1000 | PASS | 0.0033 |  |
| redis-api/return-status-table | 1 | PASS | 0.0033 |  |
| redis-api/return-status-table | 7 | PASS | 0.0033 |  |
| redis-api/return-status-table | 1000 | PASS | 0.0033 |  |
| redis-api/sha1hex | 1 | FAIL | 0.0032 | missing library/host member: ERR attempt to call a nil value |
| redis-api/sha1hex | 7 | FAIL | 0.0033 | missing library/host member: ERR attempt to call a nil value |
| redis-api/sha1hex | 1000 | FAIL | 0.0033 | missing library/host member: ERR attempt to call a nil value |
| redis-api/status-reply | 1 | PASS | 0.0033 |  |
| redis-api/status-reply | 7 | PASS | 0.0032 |  |
| redis-api/status-reply | 1000 | PASS | 0.0033 |  |

No scripts or expected replies were changed. No network or Redis server was used.
The reference is the stored Redis 7.0.15 RESP2 corpus. All cases are executed, including unavailable library cases.
Budget comparisons use fresh stores. They compare replies; atomic kill/restart and host Stop store preservation require separate checks.
