-- checks: tostring uses Lua number formatting and exponent notation.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {tostring(1/3), tostring(0.1 + 0.2), tostring(1e20), tostring(1e-9)}
