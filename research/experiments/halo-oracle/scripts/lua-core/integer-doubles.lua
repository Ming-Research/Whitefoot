-- checks: Integer-valued doubles, precision boundary and number type.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {type(42), tostring(42.0), tostring(2^53), tostring(2^53 + 1), 7.0, -7.0, 2^53 == 2^53 + 1, 2^53 ~= 2^53 + 2}
