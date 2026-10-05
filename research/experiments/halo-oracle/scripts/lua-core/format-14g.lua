-- checks: Explicit %.14g rounding at fourteen significant digits.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {string.format("%.14g", 1/3), string.format("%.14g", 123456789012345), string.format("%.14g", 1.234567890123456)}
