-- checks: error levels select caller locations; level zero has no location.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function fail(level) error("level-check",level) end
local a,x=pcall(fail,0); local b,y=pcall(fail,1)
local c,z=pcall(function() fail(2) end)
return {a,x,b,y,c,z}
