-- checks: byte ranges and char preserve boundary bytes including NUL.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {{string.byte("ABC",1,3)},string.char(0,65,255),string.byte("abc",-1),select("#",string.byte("abc",4,5))}
