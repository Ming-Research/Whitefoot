Node: language/system-interface/outcome-typing

Replaces the decision beginning "IoError carries fixed-size inline error detail", because the new variant `DeadlinePassed` reports the program's own bound, which no host produces, so it has no native code or origin to carry.

Decision: Library interfaces that distinguish success from failure use ordinary Result with operation-specific payload types, because ordinary propagation then needs no conversion from equivalent success/error variants and exhaustive matching should not require outcomes the operation cannot produce, instead of redundant acquisition enums or a shared outcome union coupling unrelated reading, writing and copying callers.

Decision: Every `IoError` variant a host refusal produces carries fixed-size inline error detail, a numeric native code and an ordinary origin discriminator, and `DeadlinePassed`, which the program's own deadline produces, carries none, because a heap-backed message would add an allocation obligation to every fallible library call and a deadline has no native code, instead of an owned diagnostic string or buffer, or a native code invented for the deadline.
