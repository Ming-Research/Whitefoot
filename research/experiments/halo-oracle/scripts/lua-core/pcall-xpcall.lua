-- checks: Protected calls preserve success returns and handler-transformed errors.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,b,c=pcall(function() return 7,"ok" end)
local d,e=xpcall(function() error("boom",0) end,function(x) return "handled:" .. x end)
local f,g=xpcall(function() return 9 end,function() return "unused" end)
return {a,b,c,d,e,f,g}
