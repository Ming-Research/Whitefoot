-- checks: Shifts mask counts and tohex formats signed values as hex.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {bit.lshift(1,31),bit.rshift(-1,1),bit.lshift(1,32),bit.rshift(-1,32),bit.tohex(-1),bit.tohex(255,4),bit.tohex(255,-4)}
