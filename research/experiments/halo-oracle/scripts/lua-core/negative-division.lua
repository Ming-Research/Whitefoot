-- checks: Lua 5.1 division is floating; modulo follows floor division.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {tostring(-7/3), math.floor(-7/3), -7%3, 7%-3, -7%-3, math.fmod(-7,3)}
