# Match form

The conditional forms `if` and `match`, conditional values, and what a match must name.

### decision the-scrutinee-type-picks-if-or-match
A `Bool` condition takes `if`, and an enum or integer scrutinee takes `match`; the writer does not choose.
- replaced: `match` as the only conditional was on main; the owner changed it, because a `Bool` scrutinee's two arms are always `True()` and `False()` in fixed order, so the arms carried nothing the condition did not.

### decision conditional-values-are-give-initializers
A conditional value is a `let` initializer whose every arm or branch ends in exactly one `give` or diverges.
- replaced: A conditional value only through a helper function that returns from each arm was on main; the owner changed it, because conditional initialization is the pattern an AI writer needs most, and a helper per conditional value is mechanical.

### decision the-scrutinee-is-a-full-expression
A `match` scrutinee is a full expression.
- rejected: an agent proposed restricting the scrutinee to a place, so the value is bound first and the name matched; the owner rejected it, because that taxes the conditional idiom with a mechanical temporary at every use and a name a weak writer must invent.

### decision an-empty-else-is-rejected
An `if` with no `else` is the one spelling of an empty alternative: an empty `else` block is rejected, and an `else` holding only one `if` is written `else if`.
- rejected: an agent proposed admitting both spellings of an empty alternative; the owner rejected it, because one construct has one spelling.

### decision enum-matches-name-every-variant
A `match` on an enum names every declared variant and has no wildcard arm.
- rejected: an agent proposed exhaustiveness with a catch-all arm; the owner rejected it, because a wildcard arm is where a newly added variant would be absorbed in silence, while naming every variant shows each site that must change.
