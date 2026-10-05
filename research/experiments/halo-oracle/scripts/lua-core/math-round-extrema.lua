-- checks: floor, ceil, abs, max, min and fmod for signed numbers.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {math.floor(-1.2),math.ceil(-1.2),math.abs(-7),math.max(-2,5,3),math.min(-2,5,3),tostring(math.fmod(-7.5,2))}
