Node: language/system-interface/writable-directory

Decision: `Inputs.cwd` is a `Directory`, an ordinary struct of a `DirectoryRead` and a `DirectoryWrite` that close separately, every write operation takes the write half, and a program weakens the directory to read-only by closing the write half, because well-typed code may pair halves of different directories and each half's own close keeps cleanup correct for any pair, as for `TcpConnection`, while a function given only the read half cannot write below it (owner's rulings Q25 and Q30), instead of one handle with both authorities and a function that weakens it, write authority derived from read authority, or one directory type for both.

Decision: A file is written only by appending: `open_append` opens or creates it below a write half, `append_once` makes one host write at its end, and `sync_file` promises only that the bytes appended before it were handed to the host's durability mechanism, leaving what survives a host failure unspecified, because an append-only log synced on a schedule is the persistence the programs in view write and no conformance case could test a crash model (owner's ruling Q26), instead of positioned writes, truncation and renaming now, or a specified crash model ([design](../../../research/investigations/io-model/TIME-AND-FILES.md#writable-directories-and-append-only-files)).

Rejected:
- One `Directory` handle with both authorities and a consuming function that weakens it: rejected because every read operation would then need a second form or a weakening first, and the handle would hide a two-count close behind one value.
- Opening a write authority from a read authority: rejected because the read-write separation would then be visible only in types, not in what a function can reach.
- A specified crash-consistency model for `sync_file`: rejected by the owner because it cannot be tested and promises more than the host interfaces give.
