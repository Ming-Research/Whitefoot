-- checks: sub negative indices, upper/lower, rep and reverse.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {string.sub("abcdef",2,4),string.sub("abcdef",-3,-1),string.sub("abc",4),string.upper("aBc"),string.lower("AbC"),string.rep("ab",3),string.reverse("abc")}
