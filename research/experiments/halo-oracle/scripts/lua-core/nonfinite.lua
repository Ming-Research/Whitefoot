-- checks: Infinity and NaN arithmetic, comparisons and string conversion.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local inf, nan = 1/0, 0/0
return {tostring(inf), tostring(-inf), tostring(nan), inf == inf, nan == nan, inf > 1e300, nan < 0}
