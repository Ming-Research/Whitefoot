-- checks: next and pairs visit hash entries; output is sorted to avoid hash order.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t={alpha=1,beta=2,gamma=3}
local a,b={},{}
local k=next(t)
while k do a[#a+1]=k .. "=" .. t[k]; k=next(t,k) end
for k,v in pairs(t) do b[#b+1]=k .. "=" .. v end
table.sort(a); table.sort(b)
return {a,b,next({}) == nil}
