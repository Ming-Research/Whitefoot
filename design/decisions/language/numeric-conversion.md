# Numeric conversion

The conversions between numeric types: exact, checked, domain query, wrapping and rounding.

### decision exact-conversion-family
Numeric conversion has one exact value relation, exposed as proof-required `cvt`, total `cvt.checked` returning Result and total `cvt.defined` returning its domain, each with one result shape.
- replaced: A `cvt` whose result shape depended on the type pair was on main; the owner changed it, because a range proof should remove the failure arm from the source, and a generic result shape should follow the chosen interface rather than whether the pair is total.
- rejected: an agent proposed adding a proved-conversion alias beside the mixed-result `cvt`; the owner rejected it, because it keeps two exact-conversion interfaces and makes a generic result shape depend on the type pair.
- rejected: an agent proposed keeping the mixed-result interface and improving optimization; the owner rejected it, because removing a backend check does not remove a source error arm for a domain already proved.
- rejected: an agent proposed inferring float integrality or exact representability from a range alone; the owner rejected it, because fractional values and values between adjacent representable numbers can lie inside the same range.

### decision same-type-conversion-copies
A conversion between equal numeric types is admitted and copies the whole representation, while a float conversion across formats keeps the specified canonical NaN policy.
- rejected: an agent proposed refusing same-type instances; the owner rejected it, because a generic conversion at equal endpoint types should be an ordinary copy.
- rejected: an agent proposed normalizing NaN payloads in same-type conversions; the owner rejected it, because no representation changes at that boundary, so the copy needs no NaN test.

### decision exact-integer-conversion-keeps-equality
Exact integer conversions keep their mathematical value, and a checked integer-to-integer conversion attaches that equality to the Result it returns.
- rejected: an agent proposed discarding the equality, or rebuilding it against an input changed afterwards; the owner rejected it, because later indexing and verified returns need the input relation across naming and value delivery.

### decision integer-wrap-conversion
`cvt.wrap` is total between every pair of integer types, giving the destination's modular value, and publishes no exact equality with its input.
- rejected: an agent proposed a truncation spelling for narrowing only; the owner rejected it, because low-bit extraction and modular sign changes ask for a different result from exact conversion, and should stay one uniform integer-generic operation.

### decision nearest-rounding-conversion
`cvt.nearest::<Src, Dst>` is total over the 20 pairs with a float destination, rounding to nearest with ties to even, with no domain obligation and no fact about its input.
- rejected: an agent proposed `cvt.strict` as the rounding conversion; the owner rejected it, because beside the exact bare `cvt` the suffix reads as the stricter form while it is the lossy one, and `.strict` already labels float arithmetic's guarantees.
- rejected: an agent proposed a float-table row such as `fnarrow.strict` for f64 to f32 only; the owner rejected it, because it serves neither integer sources such as u64 counts converted to f64 nor symbolic endpoints, so a second spelling would follow.
- rejected: an agent proposed rounding from the bits in a library; the owner rejected it, because it costs a software sequence of shifts, masks and branches where one hardware conversion exists, and the first attempt was wrong yet accepted.
- rejected: an agent proposed making bare `cvt` round for float destinations; the owner rejected it, because it removes the exactness domain and silently changes the meaning of accepted programs.
- rejected: an agent proposed admitting integer destinations in the rounding conversion; the owner rejected it, because float-to-integer conversion needs its own NaN, saturation and rounding-direction choice, which no consumer has made.