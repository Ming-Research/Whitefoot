-- checks: Without __le, less-or-equal falls back to reversed __lt.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local mt={__lt=function(a,b) return a.n<b.n end}
local a,b=setmetatable({n=1},mt),setmetatable({n=2},mt)
return {a<=b,b<=a,a<=a}
