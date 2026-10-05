-- checks: Default and custom comparator sort numbers into opposite orders.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a={3,1,4,1,5}; local b={3,1,4,1,5}
table.sort(a); table.sort(b,function(x,y) return x>y end)
return {a,b}
