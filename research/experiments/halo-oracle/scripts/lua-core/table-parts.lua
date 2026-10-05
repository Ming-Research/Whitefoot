-- checks: Array and hash entries coexist; hash entries do not add array length.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t = {10,20, label="tag", [0]="zero"}
return {#t,t[1],t[2],t.label,t[0],t[3] == nil}
