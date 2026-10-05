-- checks: Metatable protection and raw operations bypass ordinary metamethods.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local mt={__metatable="locked",__index=function() return 99 end}
local t=setmetatable({},mt); rawset(t,"x",7)
local ok=pcall(setmetatable,t,{})
local u={}; local m={}; setmetatable(u,m)
return {getmetatable(t),ok,rawget(t,"x"),rawget(t,"y") == nil,t.y,rawequal(getmetatable(u),m),rawequal(t,t)}
