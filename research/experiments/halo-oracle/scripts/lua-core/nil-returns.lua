-- checks: select observes nil return slots that RESP arrays would truncate.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function f() return 10,nil,30 end
local a,b,c=f()
return {select("#",f()),a,b == nil,c}
