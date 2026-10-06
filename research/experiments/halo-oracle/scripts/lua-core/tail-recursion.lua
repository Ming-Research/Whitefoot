-- checks: Proper tail calls allow deep recursion without growing call stack.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function sum(n,a) if n==0 then return a end; return sum(n-1,a+n) end
return sum(20000,0)
