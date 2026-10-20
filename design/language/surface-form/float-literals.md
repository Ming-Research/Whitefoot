Decision: A finite float's one spelling is chosen among the decimals that round to its bit pattern and whose integer component is one nonzero digit when an exponent is present, by fewest bytes and then least bytes, because ordinary scientific notation is what writers and agents produce, so 500 is `5.0e2` and 12345000000 is `1.2345e10`, instead of admitting any integer component, under which the zero integer wins every tie (500 as `0.5e3`) and a longer integer component can be shorter (`12.345e9`).

Rejected:
- Keeping the rule and generating `0.5e3`-style candidates in the compiler: rejected because a zero integer digit before an exponent is not scientific notation, so the one spelling of common values such as 500 would be one no writer produces.
- Accepting every decimal that rounds to the value: rejected because it gives up one spelling per construct, the surface-form principle every other literal keeps.
