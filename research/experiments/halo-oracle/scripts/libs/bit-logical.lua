-- checks: bit.band, bor and bxor use signed 32-bit results.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {bit.band(255,15),bit.bor(16,3),bit.bxor(255,15),bit.band(-1,2147483648),bit.bor(1,2,4),bit.bxor(7,3,1)}
