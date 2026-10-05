-- checks: unpack ranges and select preserve vararg count including nil.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function f(...) return {select("#",...),select(1,...) == 10,select(2,...) == nil,select(3,...)} end
return {f(10,nil,30),{unpack({4,5,6},2,3)},select(-1,7,8,9)}
