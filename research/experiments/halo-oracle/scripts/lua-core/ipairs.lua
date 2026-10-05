-- checks: ipairs stops at the first nil even when later array entries exist.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t={10,20,30,40}; t[3]=nil
local out={}
for i,v in ipairs(t) do out[#out+1]={i,v} end
return out
