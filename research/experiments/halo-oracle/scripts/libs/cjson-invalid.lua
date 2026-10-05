-- checks: Malformed JSON raises an error caught by pcall.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local ok,e=pcall(cjson.decode,'{"a":]')
return {ok,e}
