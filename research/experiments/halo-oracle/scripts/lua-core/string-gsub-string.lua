-- checks: gsub supports capture substitutions, whole match, percent and limits.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,n=string.gsub("ab12 cd34","(%a+)(%d+)","%2:%1:%%:%0",1)
return {a,n,{string.gsub("banana","a","A")}}
