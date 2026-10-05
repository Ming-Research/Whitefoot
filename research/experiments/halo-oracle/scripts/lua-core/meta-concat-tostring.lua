-- checks: __concat handles table operands; tostring honors __tostring.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local mt={__tostring=function(t) return "T" .. t.n end,__concat=function(a,b) return tostring(a) .. ":" .. tostring(b) end}
local t=setmetatable({n=7},mt)
return {tostring(t),t .. "x","x" .. t}
