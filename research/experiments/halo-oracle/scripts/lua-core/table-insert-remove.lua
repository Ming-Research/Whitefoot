-- checks: Array insertion shifts entries; removal returns and shifts values.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t={"a","c"}
table.insert(t,2,"b"); table.insert(t,"d")
local x=table.remove(t,2); local y=table.remove(t)
return {x,y,t,#t}
