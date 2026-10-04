-- checks: Assignment and table constructor expand only final multiple returns.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function f() return 1,2,3 end
local a,b,c=f(); local x,y=f(),9
return {{a,b,c},{x,y},{0,f()},{f(),0},{(f())}}
