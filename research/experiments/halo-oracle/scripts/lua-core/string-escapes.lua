-- checks: Quoted escapes, decimal byte escapes and long bracket strings.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {"a\nb\tc\r\\\"", '\097\098\099', [[literal\ntext]], "\001\255"}
