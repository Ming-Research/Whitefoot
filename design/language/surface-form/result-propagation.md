Decision: Recoverable failure is an ordinary `Result` value forwarded by `let value = propagate expression;`, with no exception, throw, catch, or unwinding, because `try` commonly suggests entering exception-handling control flow while the language only forwards an ordinary value, and `propagate` names that exact action, instead of a `try` spelling.

Rejected:
- Implicit consumption of a bare affine Result in propagation: rejected because the common explicit consuming-place rule in `language/ownership` marks whole-owner death consistently across value boundaries.
