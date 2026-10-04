-- checks: table.concat uses separators, slice bounds and numeric elements.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t={"a",2,"c",4}
return {table.concat(t,"|"),table.concat(t,",",2,3),table.concat(t,",",3,2)}
