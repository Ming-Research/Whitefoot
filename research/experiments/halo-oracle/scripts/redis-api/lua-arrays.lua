-- checks: Nested array conversion stops at first nil and ignores hash entries.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {{1,false,3},{1,nil,3},{[1]="a",[2]="b",tag="ignored"},{},"tail"}
