-- checks: Lua numbers truncate toward zero; true is one and false is nil.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {3.9,-3.9,0,true,false,"bulk"}
