-- checks: String byte length and lexicographic comparisons.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {#"hello", #"", #"é", "a" < "b", "aa" < "b", "10" < "2", "abc" == "abc"}
