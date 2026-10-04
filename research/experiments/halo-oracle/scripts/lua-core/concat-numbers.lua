-- checks: Concatenation coerces numbers with Lua number formatting.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {"value=" .. 42 .. ":" .. 1.25, 1 .. 2 .. "x", "third=" .. (1/3)}
