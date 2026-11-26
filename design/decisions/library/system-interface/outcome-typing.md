# Outcome typing

How library operations report success and failure, and what an IoError carries.

### decision library-outcomes-use-ordinary-result
Library interfaces that tell success from failure return an ordinary Result with payload types of their own operation.
- rejected: an agent proposed acquisition enums with variants equivalent to success and error; the owner rejected it, because ordinary propagation then needs no conversion from such variants.
- rejected: an agent proposed one shared outcome union for reading, writing and copying; the owner rejected it, because it couples unrelated callers, and exhaustive matching should not require outcomes the operation cannot produce.

### decision io-error-carries-inline-host-detail
Every IoError variant a host refusal produces carries a numeric native code and an origin inline, while `DeadlinePassed` and `Cancelled`, which the program's own bounds produce, carry none.
- rejected: an agent proposed an owned diagnostic string or buffer in IoError; the owner rejected it, because a message on the heap would add an allocation obligation to every fallible library call.
- replaced: Inline detail on every IoError variant was on main; the owner changed it, because `DeadlinePassed` reports the program's own bound, which no host produces, so it has no native code or origin to carry.