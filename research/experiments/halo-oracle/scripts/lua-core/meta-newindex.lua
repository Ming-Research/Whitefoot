-- checks: __newindex intercepts absent entries while existing ones write directly.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local sink={}
local t=setmetatable({existing=1},{__newindex=function(_,k,v) sink[k]=v*2 end})
t.existing=4; t.absent=5
return {t.existing,sink.absent,rawget(t,"absent") == nil}
