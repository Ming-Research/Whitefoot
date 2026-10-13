Decision: The conditional form is selected by the scrutinee's type and never by the writer, a Bool condition takes `if` and an enum or integer scrutinee takes `match`, because a Bool scrutinee's two arm labels are always exactly `True()` and `False()` in fixed order, so the arm ceremony carried no information the condition did not already carry, instead of match as the sole conditional.

Decision: A conditional value is a `let` initializer whose every arm or branch ends in exactly one `give` or diverges, because conditional initialization is the most common pattern an AI writer needs and delivering the value removes a mechanical helper function per conditional value, instead of extracting a helper function that returns from each arm.

Decision: The scrutinee is a full expression, because restricting it to a place taxes the sole conditional idiom with a mechanical temporary at every use and adds naming burden for a weak writer, instead of binding the value first and matching on the name.

Decision: An else-free `if` statement is the one spelling of its empty alternative, its empty `else` block is rejected, and its `else` holding exactly one `if` flattens to `else if`, because one construct has one spelling, instead of admitting both spellings of an empty alternative.

Decision: An enum match names every declared variant and has no wildcard arm, because a wildcard arm is exactly where a newly added variant would be absorbed in silence, whereas enumeration surfaces every site that must change, instead of exhaustiveness with a catch-all arm.
