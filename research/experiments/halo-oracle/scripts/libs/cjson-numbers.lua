-- checks: cjson number formatting is independent of RESP integer conversion.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {cjson.encode({1,1.25,1/3,1e20,1e-9}),cjson.decode("1.25") == 1.25}
