-- checks: JSON arrays and nested objects round-trip without relying on object order.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t=cjson.decode('{"outer":[1,{"inner":["x",false,null]}]}')
return {cjson.encode({1,2,3}),cjson.encode({{1,2},{3,4}}),t.outer[1],t.outer[2].inner[1],t.outer[2].inner[2],t.outer[2].inner[3]==cjson.null}
