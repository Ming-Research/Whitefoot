# Result propagation

How a recoverable failure is forwarded: an ordinary `Result` value and the `propagate` form.

### decision failures-propagate-as-values
Recoverable failure is an ordinary `Result` value forwarded by `let value = propagate expression;`, with no exception, throw, catch or unwinding.
- replaced: The spelling `try` was on main; the owner changed it, because `try` suggests entering exception handling, while the language only forwards an ordinary value, and `propagate` names that exact action.
- replaced: Implicit consumption of a bare affine `Result` in propagation was on main; the owner changed it, because the common rule that a consuming use of an owned place writes `move` marks the death of a whole owner the same way at every value boundary.
