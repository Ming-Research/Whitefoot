-- checks: Redis refuses access to an undeclared global.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- error: true
return halo_oracle_missing
