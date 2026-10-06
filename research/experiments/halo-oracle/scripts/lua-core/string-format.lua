-- checks: format exercises %d %s %q %x %g and width/precision.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {string.format("%d|%s|%x|%g|%5.2f",-17,"word",255,1.25,1.5),string.format("%q","a\nb\"\\\000"),string.format("%d",3.9)}
