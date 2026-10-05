-- checks: Redis refuses assignment to an undeclared global.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- error: true
halo_oracle_global = 17
return halo_oracle_global
