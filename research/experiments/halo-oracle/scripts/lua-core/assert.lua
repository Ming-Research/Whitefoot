-- checks: assert returns all successful arguments and raises a chosen message.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,b=assert(17,"kept")
local ok,e=pcall(function() assert(false,"assertion-marker") end)
return {a,b,ok,e}
