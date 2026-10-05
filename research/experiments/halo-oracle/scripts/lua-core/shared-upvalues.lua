-- checks: Sibling closures share the same mutable captured local.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local n=10
local function get() return n end
local function put(v) n=v end
put(25); local a=get(); put(31)
return {a,get()}
