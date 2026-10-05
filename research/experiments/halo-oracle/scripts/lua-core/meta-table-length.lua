-- checks: Lua 5.1 ignores __len on tables.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local hits=0
local t=setmetatable({10,20},{__len=function() hits=hits+1; return 99 end})
return {#t,hits}
