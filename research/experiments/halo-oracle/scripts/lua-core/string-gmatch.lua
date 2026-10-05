-- checks: gmatch iterates captured words and digits.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local out={}
for k,v in string.gmatch("a=1 b=22 c=333","(%a+)=(%d+)") do out[#out+1]={k,v} end
return out
