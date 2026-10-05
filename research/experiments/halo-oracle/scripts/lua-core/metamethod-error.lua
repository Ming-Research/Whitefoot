-- checks: Errors raised inside a metamethod propagate through pcall.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t=setmetatable({},{__index=function() error("index-failed",0) end})
local ok,e=pcall(function() return t.missing end)
return {ok,e}
