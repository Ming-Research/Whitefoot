-- checks: Shared __eq, __lt and __le control table comparisons.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local mt={__eq=function(a,b) return a.n==b.n end,__lt=function(a,b) return a.n<b.n end,__le=function(a,b) return a.n<=b.n end}
local a,b,c=setmetatable({n=1},mt),setmetatable({n=2},mt),setmetatable({n=1},mt)
return {a==c,a==b,a<b,a<=c,b<=a,rawequal(a,c)}
