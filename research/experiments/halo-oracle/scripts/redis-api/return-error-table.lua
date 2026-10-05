-- checks: A returned err field becomes a RESP error.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- error: true
return {err="oracle custom error"}
