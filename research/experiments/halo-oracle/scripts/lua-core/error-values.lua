-- checks: pcall preserves string and table error objects without stringifying tables.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,s=pcall(function() error("boom",0) end)
local obj={tag="table-error"}
local b,t=pcall(function() error(obj) end)
return {a,s,b,type(t),t.tag,rawequal(t,obj)}
