-- checks: Embedded NUL survives length, slicing, byte lookup and RESP bulk.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local s = "a\000b"
return {s, #s, string.byte(s,2), string.sub(s,2,3), s .. "\000"}
