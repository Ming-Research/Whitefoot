# Halo end-to-end comparison

Local parent revision: `dadf30663de6e42af9cc84f6e5b64e72dee6e23c`. Source digest: `83af4f14a7f74c8c55c292f127439cb9cf40aca5f6adfd8ceaabce2b14cf94c5`.
The digest includes every Halo module, the graph, host and runner; it identifies uncommitted source bytes too.

Executable SHA-256: `325f7f4efcf0bbc5ed2e28a454cc10f88f08f4e8262bd4be2dc8ab8a62c7fbdc`.
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
| apps/hash-cas | 1 | PASS | 0.0046 |  |
| apps/hash-cas | 7 | PASS | 0.0035 |  |
| apps/hash-cas | 1000 | PASS | 0.0033 |  |
| apps/lock-extend | 1 | PASS | 0.0036 |  |
| apps/lock-extend | 7 | PASS | 0.0033 |  |
| apps/lock-extend | 1000 | PASS | 0.0033 |  |
| apps/queue-move | 1 | PASS | 0.0038 |  |
| apps/queue-move | 7 | PASS | 0.0033 |  |
| apps/queue-move | 1000 | PASS | 0.0032 |  |
| apps/rate-limiter | 1 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/rate-limiter | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/rate-limiter | 1000 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/redlock-release | 1 | PASS | 0.0037 |  |
| apps/redlock-release | 7 | PASS | 0.0033 |  |
| apps/redlock-release | 1000 | PASS | 0.0033 |  |
| apps/sliding-window | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/sliding-window | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| apps/sliding-window | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tonumber' |
| libs/bit-logical | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-logical | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-logical | 1000 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/bit-shifts-hex | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'bit' |
| libs/cjson-arrays-nested | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-arrays-nested | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-arrays-nested | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-invalid | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-numbers | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cjson-objects | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cjson' |
| libs/cmsgpack-binary | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-binary | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-binary | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/cmsgpack-roundtrip | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'cmsgpack' |
| libs/struct-integers | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-integers | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-integers | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| libs/struct-strings-floats | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'struct' |
| lua-core/array-holes | 1 | PASS | 0.0033 |  |
| lua-core/array-holes | 7 | PASS | 0.0033 |  |
| lua-core/array-holes | 1000 | PASS | 0.0033 |  |
| lua-core/assert | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/assert | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/assert | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'assert' |
| lua-core/concat-numbers | 1 | FAIL | 0.0034 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/concat-numbers | 7 | FAIL | 0.0033 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/concat-numbers | 1000 | FAIL | 0.0032 | runtime/compile error: ERR attempt to concatenate a number value |
| lua-core/counter-closure | 1 | PASS | 0.0035 |  |
| lua-core/counter-closure | 7 | PASS | 0.0034 |  |
| lua-core/counter-closure | 1000 | PASS | 0.0032 |  |
| lua-core/embedded-zero | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/embedded-zero | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/embedded-zero | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/error-levels | 1 | PASS | 0.0036 |  |
| lua-core/error-levels | 7 | PASS | 0.0033 |  |
| lua-core/error-levels | 1000 | PASS | 0.0033 |  |
| lua-core/error-values | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/error-values | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/error-values | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/float-print | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/float-print | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/float-print | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/format-14g | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/format-14g | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/format-14g | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/integer-doubles | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/integer-doubles | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/integer-doubles | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/ipairs | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/ipairs | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/ipairs | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 7 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/loop-closures | 1000 | FAIL | 0.0039 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'ipairs' |
| lua-core/math-powers | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-powers | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-powers | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-random | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/math-round-extrema | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'math' |
| lua-core/meta-call | 1 | PASS | 0.0034 |  |
| lua-core/meta-call | 7 | PASS | 0.0033 |  |
| lua-core/meta-call | 1000 | PASS | 0.0033 |  |
| lua-core/meta-comparisons | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-comparisons | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-comparisons | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-concat-tostring | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-concat-tostring | 7 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-concat-tostring | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/meta-index | 1 | PASS | 0.0034 |  |
| lua-core/meta-index | 7 | PASS | 0.0033 |  |
| lua-core/meta-index | 1000 | PASS | 0.0033 |  |
| lua-core/meta-le-fallback | 1 | PASS | 0.0033 |  |
| lua-core/meta-le-fallback | 7 | PASS | 0.0033 |  |
| lua-core/meta-le-fallback | 1000 | PASS | 0.0033 |  |
| lua-core/meta-newindex | 1 | PASS | 0.0033 |  |
| lua-core/meta-newindex | 7 | PASS | 0.0034 |  |
| lua-core/meta-newindex | 1000 | PASS | 0.0033 |  |
| lua-core/meta-protection-raw | 1 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-protection-raw | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-protection-raw | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'rawequal' |
| lua-core/meta-table-length | 1 | PASS | 0.0033 |  |
| lua-core/meta-table-length | 7 | PASS | 0.0032 |  |
| lua-core/meta-table-length | 1000 | PASS | 0.0032 |  |
| lua-core/metamethod-error | 1 | PASS | 0.0033 |  |
| lua-core/metamethod-error | 7 | PASS | 0.0032 |  |
| lua-core/metamethod-error | 1000 | PASS | 0.0032 |  |
| lua-core/multiple-returns | 1 | PASS | 0.0034 |  |
| lua-core/multiple-returns | 7 | PASS | 0.0033 |  |
| lua-core/multiple-returns | 1000 | PASS | 0.0033 |  |
| lua-core/negative-division | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/negative-division | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/negative-division | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/next-pairs | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/next-pairs | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/next-pairs | 1000 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'next' |
| lua-core/nil-returns | 1 | PASS | 0.0034 |  |
| lua-core/nil-returns | 7 | PASS | 0.0033 |  |
| lua-core/nil-returns | 1000 | PASS | 0.0033 |  |
| lua-core/nonfinite | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/nonfinite | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/nonfinite | 1000 | FAIL | 0.0036 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'tostring' |
| lua-core/numeric-coercion | 1 | FAIL | 0.0032 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/numeric-coercion | 7 | FAIL | 0.0032 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/numeric-coercion | 1000 | FAIL | 0.0032 | runtime/compile error: ERR attempt to perform arithmetic on a string value |
| lua-core/pcall-xpcall | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/pcall-xpcall | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/pcall-xpcall | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'xpcall' |
| lua-core/shared-upvalues | 1 | PASS | 0.0034 |  |
| lua-core/shared-upvalues | 7 | PASS | 0.0033 |  |
| lua-core/shared-upvalues | 1000 | PASS | 0.0033 |  |
| lua-core/string-byte-char | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-byte-char | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-byte-char | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-escapes | 1 | PASS | 0.0032 |  |
| lua-core/string-escapes | 7 | PASS | 0.0034 |  |
| lua-core/string-escapes | 1000 | PASS | 0.0034 |  |
| lua-core/string-find | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-find | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-find | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 7 | FAIL | 0.0037 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-format | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 1 | FAIL | 0.0031 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gmatch | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 1 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-dynamic | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 1 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-gsub-string | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-length-order | 1 | PASS | 0.0033 |  |
| lua-core/string-length-order | 7 | PASS | 0.0033 |  |
| lua-core/string-length-order | 1000 | PASS | 0.0032 |  |
| lua-core/string-match | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-match | 7 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-match | 1000 | FAIL | 0.0031 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/string-transforms | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'string' |
| lua-core/table-concat | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-concat | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-concat | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 7 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-insert-remove | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-parts | 1 | PASS | 0.0032 |  |
| lua-core/table-parts | 7 | PASS | 0.0033 |  |
| lua-core/table-parts | 1000 | PASS | 0.0033 |  |
| lua-core/table-sort | 1 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-sort | 7 | FAIL | 0.0035 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/table-sort | 1000 | FAIL | 0.0032 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'table' |
| lua-core/tail-recursion | 1 | PASS | 0.9786 |  |
| lua-core/tail-recursion | 7 | PASS | 0.1450 |  |
| lua-core/tail-recursion | 1000 | PASS | 0.0062 |  |
| lua-core/unpack-select-varargs | 1 | FAIL | 0.0038 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| lua-core/unpack-select-varargs | 7 | FAIL | 0.0034 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| lua-core/unpack-select-varargs | 1000 | FAIL | 0.0033 | unavailable global/library: ERR Script attempted to access nonexistent global variable 'unpack' |
| redis-api/call-error | 1 | FAIL | 0.0033 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-error | 7 | FAIL | 0.0035 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-error | 1000 | FAIL | 0.0032 | error text/location difference: ERR Wrong number of args calling Redis command from script |
| redis-api/call-success | 1 | PASS | 0.0034 |  |
| redis-api/call-success | 7 | PASS | 0.0033 |  |
| redis-api/call-success | 1000 | PASS | 0.0033 |  |
| redis-api/error-reply | 1 | PASS | 0.0033 |  |
| redis-api/error-reply | 7 | PASS | 0.0033 |  |
| redis-api/error-reply | 1000 | PASS | 0.0032 |  |
| redis-api/global-read | 1 | FAIL | 0.0032 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-read | 7 | FAIL | 0.0032 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-read | 1000 | FAIL | 0.0032 | error text/location difference: ERR Script attempted to access nonexistent global variable 'halo_oracle_missing' |
| redis-api/global-write | 1 | FAIL | 0.0032 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/global-write | 7 | FAIL | 0.0032 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/global-write | 1000 | FAIL | 0.0032 | error text/location difference: ERR Attempt to modify a readonly table |
| redis-api/log-keys-argv | 1 | PASS | 0.0034 |  |
| redis-api/log-keys-argv | 7 | PASS | 0.0033 |  |
| redis-api/log-keys-argv | 1000 | PASS | 0.0033 |  |
| redis-api/lua-arrays | 1 | PASS | 0.0032 |  |
| redis-api/lua-arrays | 7 | PASS | 0.0032 |  |
| redis-api/lua-arrays | 1000 | PASS | 0.0032 |  |
| redis-api/lua-nil | 1 | PASS | 0.0032 |  |
| redis-api/lua-nil | 7 | PASS | 0.0033 |  |
| redis-api/lua-nil | 1000 | PASS | 0.0033 |  |
| redis-api/lua-scalars | 1 | PASS | 0.0033 |  |
| redis-api/lua-scalars | 7 | PASS | 0.0033 |  |
| redis-api/lua-scalars | 1000 | PASS | 0.0033 |  |
| redis-api/pcall-success-error | 1 | PASS | 0.0034 |  |
| redis-api/pcall-success-error | 7 | PASS | 0.0033 |  |
| redis-api/pcall-success-error | 1000 | PASS | 0.0032 |  |
| redis-api/resp-error-raised | 1 | PASS | 0.0033 |  |
| redis-api/resp-error-raised | 7 | PASS | 0.0032 |  |
| redis-api/resp-error-raised | 1000 | PASS | 0.0033 |  |
| redis-api/resp-values | 1 | PASS | 0.0037 |  |
| redis-api/resp-values | 7 | PASS | 0.0034 |  |
| redis-api/resp-values | 1000 | PASS | 0.0032 |  |
| redis-api/return-error-table | 1 | PASS | 0.0032 |  |
| redis-api/return-error-table | 7 | PASS | 0.0032 |  |
| redis-api/return-error-table | 1000 | PASS | 0.0031 |  |
| redis-api/return-status-table | 1 | PASS | 0.0032 |  |
| redis-api/return-status-table | 7 | PASS | 0.0031 |  |
| redis-api/return-status-table | 1000 | PASS | 0.0033 |  |
| redis-api/sha1hex | 1 | FAIL | 0.0036 | missing library/host member: ERR attempt to call a nil value |
| redis-api/sha1hex | 7 | FAIL | 0.0032 | missing library/host member: ERR attempt to call a nil value |
| redis-api/sha1hex | 1000 | FAIL | 0.0032 | missing library/host member: ERR attempt to call a nil value |
| redis-api/status-reply | 1 | PASS | 0.0032 |  |
| redis-api/status-reply | 7 | PASS | 0.0033 |  |
| redis-api/status-reply | 1000 | PASS | 0.0034 |  |

No scripts or expected replies were changed. No network or Redis server was used.
The reference is the stored Redis 7.0.15 RESP2 corpus. All cases are executed, including unavailable library cases.
Budget comparisons use fresh stores. They compare replies; atomic kill/restart and host Stop store preservation require separate checks.

## Validation and review

WF source revision: `dadf30663de6e42af9cc84f6e5b64e72dee6e23c`; runner source bytes are identified by the source digest above.
The final native build completed in 163.71 seconds, exit 0; the direct full Halo graph check accepted all seven modules in 80.44 seconds.

Commands actually run:

- `whitefootc --graph lib/halo/modules.wfg --check-modules` (direct, without the check wrapper): all modules accepted.
- `whitefootc --graph research/experiments/halo-e2e/modules.wfg --check-module pkg::test`: accepted.
- `perl .github/run-check.pl halo-e2e-native whitefootc --cache /private/tmp/halo-e2e-cache --graph research/experiments/halo-e2e/modules.wfg --entry test -o /private/tmp/halo-e2e-test`: native build exit 0. The supplied compiler executable's bytes are hashed above.
- `/private/tmp/halo-e2e-test one two three`: embedding probe exit 0, including forced GC retention and reclamation, host outcomes, budget continuation, flush/reset, stale IDs, three exact formatter byte expectations, and terminal Stop refusing resume without another host call.
- `python3 -B research/experiments/halo-e2e/run.py --compiler /private/tmp/wf-halo/compiler/target/gate/whitefootc --binary /private/tmp/halo-e2e-test --budgets 1,7,1000 --report research/experiments/halo-e2e/RESULTS.md --actual /private/tmp/halo-e2e-actual`: exit 1 because 144 of 240 comparisons differ. Every script ran; no skip or expected file was changed. All replies were identical across budgets.
- `make static`: passed. `git diff --check`: passed. Full `make check` and Cargo were not run under the task's no-Cargo constraint.

The first native build took 144.89 seconds; a scalar sample took 0.3587 seconds
on its first invocation and 0.0036/0.0034 seconds on subsequent invocations.
That sample justified running the 80-case batch. These times size the work;
they are not a Halo-versus-Redis performance comparison. The comparison checks
stored Redis replies, without starting Redis or using the network.

Independent read-only review used gpt-6-sol, base
`e543203ba5a66a8e9fec30ad28b7d7e09c1919ef` through the worktree, groups A, D, C,
R, M and V; T was not triggered. It inspected the complete change, VM design,
relevant decision ancestors and validation without rerunning green suites.
It found a duplicated formatter prefix and unsafe use of an old checkpoint
following ordinary HostStopped. Both were repaired with distinguishing probes;
focused re-review of those hunks found no new issue. The implementing agent then
built and ran the repaired program as listed above. The later Python sensitivity
checks additionally reject changed wire bytes, integers, order, missing replies,
type changes and invalid JSON reply schemas, and check UTF-8 header transport.

No specification rule, conformance expectation, design tree or other Halo module
was changed. [GAPS.md](GAPS.md) records the remaining integration limitations,
including missing libraries, error PC/Redis EVAL wrapper parity, retained
cross-script closures and callback-time pinning. This is a first correctness
comparison, not evidence of full Redis compatibility or the VM's remaining
memory-limit/kill/restart falsifiers. The user explicitly requested local
commits and forbade publication, so there is no PR or remote revision.

## After merging the library (2026-10-04)

With the slice-1 library merged and installed by `new_engine` (the lead wired
`pkg::vm::install_libraries`, which the parallel embedding had not yet
called), 183 of 240 runs pass: per budget, apps 6/6, lua-core 43/48,
redis-api 12/16, libs 0/10, equal across budgets 1, 7 and 1000. The
remaining failures: Lua patterns (string.find, match, gmatch, gsub; five
scripts), Redis's error text with script SHA and location (three), and
`redis.sha1hex` (one); the libs group awaits cjson, cmsgpack, bit and struct.

## Redis error replies and SHA-1 (2026-10-04)

Source revision: `4f647756e36d8e1bb0e0cb8f588ce31d19a11590`. Source digest: `27ab1daa5e9a31e9474a715c6252fca67e2be2f289e14be715727077ec245e3c`.
Executable SHA-256: `414c1caa20edf9f1394e720329cc866a9949f2e40219a8c7a7e3cae496b982f1`.
Compiler SHA-256: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`. The digest identifies the complete
Halo graph, host and runner used by this run; later documentation-only changes
do not change these tested source bytes.

The Whitefoot embedding supplies SHA-1 and Redis 7 EVAL error composition.
The host raises unhandled command errors as tables and preserves Redis's
string error result under `pcall`. Global reads acquire their Lua prefix
when raised; the terminal host adds it to raw readonly refusals. Script
identity hashes the unchanged body, including comments and final newline.
The reference formatting is Redis 7.0.15's `src/eval.c` error handler,
`src/script_lua.c` EVAL reply composition, global protection and SHA-1
member; the stored oracle replies were not regenerated or changed.

| Group | Budget | Passed | Failed |
| --- | --- | ---: | ---: |
| apps | 1 | 6 | 0 |
| apps | 7 | 6 | 0 |
| apps | 1000 | 6 | 0 |
| libs | 1 | 0 | 10 |
| libs | 7 | 0 | 10 |
| libs | 1000 | 0 | 10 |
| lua-core | 1 | 43 | 5 |
| lua-core | 7 | 43 | 5 |
| lua-core | 1000 | 43 | 5 |
| redis-api | 1 | 16 | 0 |
| redis-api | 7 | 16 | 0 |
| redis-api | 1000 | 16 | 0 |

195/240 comparisons pass, with identical replies across budgets 1, 7 and
1000. Every redis-api case now passes, including call-error, global-read,
global-write and sha1hex. The remaining 45 comparisons are the same missing
libraries: five Lua pattern scripts per budget (find, gmatch, both gsub
forms and match), plus ten codec/bit/struct scripts per budget.

Validation actually run (the final runner uses the source revision above):

- `python3 -B research/experiments/halo-e2e/run.py --compiler /private/tmp/wf-halo/compiler/target/gate/whitefootc --budgets 1,7,1000 --verify-sha1 --verify-errors --report /private/tmp/halo-errtext-full-results.md --actual /private/tmp/halo-errtext-replies`: native build exit 0 in 405.023 seconds; runner exit 1 for the 45 library mismatches above. No script was skipped.
- `SHA-1: 3 fixed + 1000 seeded random binary vectors match hashlib; total 3.777s, range 0.0031..0.3899s`
- `Redis errors: 10 source/location/value probes at each budget + 2 protected-error probes pass` These expectations come from the local Redis sources and `hashlib`, with nil/boolean/number/level-zero strings also checked using Redis's bundled Lua 5.1.5 executable. The nil probe failed on the preceding binary with `(error object is not a string)`, then passed after the tostring conversion repair.
- Injected all-zero digests and incorrect error text/location were rejected by the added probe harness; the existing schema and reply sensitivity checks also ran.
- Direct `whitefootc --graph lib/halo/modules.wfg --check-module pkg::embed` and `whitefootc --graph research/experiments/halo-e2e/modules.wfg --check-module pkg::test`: accepted.
- The embedding lifecycle probe on the preceding native build (`test one two three`): exit 0. Lifecycle code did not change afterward; the final runner exercises the changed formatter through the value/location probes.
- `make static` and `git diff --check`: passed. No Cargo, full `make check`, network, push or PR operation was run. No specification, conformance expectation or other Halo module changed.

The first useful SHA-1 sample passed under all three budgets; subsequent
scalar invocations took roughly 3–5 ms, supporting the 1,000-vector and
80-script batches. Native build samples took 336.30 and 410.15 seconds.
These timings size correctness runs, not a Halo-versus-Redis performance
comparison; concurrently active local compiler work was observed.

Found along the way: repaired nonstring scalar error conversion and avoided
using the budget checkpoint as a failure PC. [GAPS.md](GAPS.md#error-locations-and-stop)
records observed remaining location boundaries: a saved redis.call member
reports its lookup line instead of its later call line; protected readonly
errors lack a Lua prefix; an explicit level-zero error equal to the readonly
message receives an unwanted prefix. Those require VM failure kind/PC
information outside this task's file boundary. Corpus parity is established;
general Redis EVAL location parity remains incomplete.

Independent read-only review used gpt-6-sol, base
`7d6e73ea5ab7ae2996792d695b28d7df8f96dbc0` through
`4f647756e36d8e1bb0e0cb8f588ce31d19a11590` plus the documentation diff,
checking groups A, D, C, M and V. T and R did not trigger: no specification,
formal-test, gate or material design choice changed. It read the complete
diff, affected callers, design context, Redis formatting sources, validation
and failure witnesses without rerunning green suites. Findings: none within
scope. The last static check's first attempt was blocked by another worktree's
active shared lock; no lock was removed or other worktree changed.
Publication is intentionally absent under the user's local-only instruction.

## All scripts (2026-10-04, 3697deca8)

With the pattern functions and the cjson, cmsgpack, bit and struct bindings
merged, and the compiler rebuilt from main with the per-arm match-dispatch
lowering, all 80 oracle scripts reply as Redis 7.0.15 does at budgets 1, 7 and
1000: 240 of 240 runs (apps 6/6, libs 10/10, lua-core 48/48, redis-api 16/16).
The macOS-only qualification of VM.md H4 still applies.
