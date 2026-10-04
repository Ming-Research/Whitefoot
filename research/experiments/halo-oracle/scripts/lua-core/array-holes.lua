-- checks: Lua 5.1 length selects a boundary for arrays with holes.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- note: The chosen hole boundary is implementation-dependent; the boundary predicate is portable.
local t={10,20,30,40}
t[2]=nil
local n=#t
return {n, n == 0 or (t[n] ~= nil and t[n+1] == nil), # {10,20,30}}
