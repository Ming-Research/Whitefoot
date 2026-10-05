-- checks: JSON objects, empty table encoding and decoded null/boolean values.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t=cjson.decode('{"name":"halo","n":7,"yes":true,"none":null}')
return {cjson.encode({name="halo"}),cjson.encode({}),t.name,t.n,t.yes,t.none==cjson.null}
