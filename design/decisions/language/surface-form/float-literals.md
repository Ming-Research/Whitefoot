# Float literals

The one spelling of a finite float literal.

### decision floats-use-scientific-spelling
A finite float's one spelling is the shortest, then least, decimal that rounds to its bits, with one nonzero integer digit when it has an exponent, so 500 is `5.0e2`.
- replaced: A rule that admitted any integer part before an exponent, so that 500 was `0.5e3`, was on main; the owner changed it, because ordinary scientific notation is what writers and agents produce, and a zero digit before an exponent is not scientific notation.
- rejected: an agent proposed accepting every decimal that rounds to the value; the owner rejected it, because it gives up one spelling per construct, which every other literal keeps.
