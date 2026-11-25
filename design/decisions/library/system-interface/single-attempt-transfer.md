# Single-attempt transfer

How much one byte-I/O call transfers, and where looping until done belongs.

### decision byte-io-makes-one-attempt
The byte-I/O primitives report one attempt's progress and outcome, and looping policies such as read-exact and write-all are left to source helpers.
- rejected: an agent proposed byte-I/O primitives with a built-in loop to completion; the owner rejected it, because the caller can then keep partial progress and choose whether to retry, while a native loop hides that choice.