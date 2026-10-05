-- checks: __index supports both table delegation and a function fallback.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a=setmetatable({own=3},{__index={missing=7}})
local b=setmetatable({},{__index=function(t,k) return "fallback:" .. k end})
return {a.own,a.missing,b.hello,rawget(a,"missing") == nil}
