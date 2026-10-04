-- checks: sqrt, math.pow, exponentiation and math.huge.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {math.sqrt(81),math.pow(2,10),2^10,tostring(2^-3),tostring(math.sqrt(2)),math.huge == 1/0,tostring(math.huge)}
