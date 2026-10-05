-- checks: Arithmetic coerces numeric strings; tonumber handles bases and failures.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {"12" + 3, "6" * "7", tostring("5" / 2), tonumber("ff",16), tonumber("101",2), tonumber("Z",36), tonumber("bad") == nil, tostring(tonumber(" 12.5 "))}
