# Dispatch

How a program selects code to run: no function values, and `match` over enum variants and integer constants.

### decision no-dynamic-dispatch
The kernel has no function values and no dynamic call dispatch; `match` selects an arm by a variant or an integer constant, and every call in an arm has a statically known callee.
- rejected: an agent proposed trait objects, vtables or closures with captured environments; the owner rejected it, because selecting a data value keeps the callee's exact effect row, ownership discipline and per-call proof obligations in view.

### decision integer-constant-match
`match` dispatches on integer constants for all eight integer types, concrete instances of an Int-bound parameter included, while Bool keeps `if`.
- rejected: an agent proposed converting the integer code to an enum first; the owner rejected it, because Firn's command dispatcher needs the selection without a chain of about 130 equality tests, and such a conversion is itself that chain.
- rejected: an agent proposed a new `switch` spelling; the owner rejected it, because it names a lowering mechanism rather than the checked selection.
- rejected: an agent proposed integer arms for Bool or a second enum match spelling; the owner rejected it, because Bool already has its complete conditional form in `if`.

### decision integer-arm-labels
An integer arm label is a typed integer literal, a character literal, generic zero or one, or a name of a const item of the scrutinee's exact type, and labels are distinct by value.
- rejected: an agent proposed resolving duplicate labels by source order; the owner rejected it, because two names for one value must not select the same value twice.
- rejected: an agent proposed a bare variant label written like a name; the owner rejected it, because the variant form with parentheses tells variant labels from constant names lexically and keeps payload binding explicit.

### decision integer-match-else-rule
Every integer match has a value arm; a partial listing ends in exactly one `else`, and a listing of the whole type has none.
- rejected: an agent proposed admitting `else` after a listing of the whole type; the owner rejected it, because that `else` can select no value and would give complete coverage two source forms.
- rejected: an agent proposed a wildcard arm; the owner rejected it, because `else` already names the remaining alternative.
- rejected: an agent proposed judging coverage against the entering proof interval; the owner rejected it, because coverage would then depend on unrelated proof edits rather than on the scrutinee's declared type.

### decision one-value-per-integer-arm
An integer match arm carries one constant value; there are no range labels and no grouped labels.
- rejected: an agent proposed range labels or a `|` union of labels; the owner rejected it, because the command dispatcher needs one value per arm, and one value leaves no overlap, precedence or multiple-entry meaning to settle.

### decision statement-and-value-match-share-arms
Statement match and value-initializer match share the integer arms and the coverage judgment, and value delivery follows the existing `give` rule.
- rejected: an agent proposed integer match for statements only; the owner rejected it, because that forces dispatch that produces a value into mutable temporaries.

### decision integer-match-selects-by-equality
An integer scrutinee is evaluated once and its arm is selected by equality, with no guards evaluated in order, and the language prescribes no backend instruction.
- rejected: an agent proposed defining integer match as an if-chain; the owner rejected it, because selection by equality gives an LLVM switch a direct implementation without making a backend strategy observable.