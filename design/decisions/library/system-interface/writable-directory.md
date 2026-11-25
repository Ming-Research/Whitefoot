# Writable directory

The read and write halves of a directory, what the write half lets a program do to files and subdirectories, and which names a root accepts.

### decision directory-has-separate-read-and-write-halves
`Inputs.cwd` is a `Directory`, an ordinary struct of a `DirectoryRead` and a `DirectoryWrite` that close separately, every write takes the write half, and closing it makes the directory read-only.
- rejected: an agent proposed one `Directory` handle with both authorities and a consuming function that weakens it; the owner rejected it, because every read would need a second form or a weakening first, and the handle would hide a two-count close behind one value.
- rejected: an agent proposed opening a write authority from a read authority; the owner rejected it, because the separation of read and write would then show only in types, not in what a function can reach.
- rejected: an agent proposed one directory type for both authorities; the owner rejected it, because a function given only the read half must not be able to write below it.

### decision write-half-appends-truncates-renames-syncs
Files can be appended, set to a length and synced, and entries renamed, removed and synced within one directory through its write half; sync promises only handoff to the host's durability.
- rejected: an agent proposed positioned writes for rewriting a log; the owner rejected it, because an interruption leaves a partly rewritten log, while an AOF rewrite needs a synced temporary file renamed over the log.
- rejected: an agent proposed one combined replacement operation; the owner rejected it, because it cannot remove obsolete parts.
- rejected: an agent proposed a specified crash-consistency model for `sync_file`; the owner rejected it, because no conformance case could test it, and it promises more than the host interfaces give.

### decision directory-write-open-creates-if-missing
`open_directory_write` opens the write half of a directory below a write half, creating it empty when no entry has the name and keeping its entries when one does.
- rejected: an agent proposed a separate directory creation operation; the owner rejected it, because it costs a second host call on Windows, where one call creates or opens.
- rejected: an agent proposed leaving the directory hierarchy flat in the program; the owner rejected it, because a writable hierarchy, such as the directory Redis 7.0.15 keeps its append-only files in, would be inexpressible.

### decision move-file-between-directories
`move_file` moves a file from below one directory's write half to below another's, with `rename_file`'s atomic replacement and open-handle rule.
- rejected: an agent proposed giving `rename_file` two roots; the owner rejected it, because a rename within one directory could not call it, since one handle passed as both written roots of a call overlaps itself.

### decision root-names-are-one-component
A name given with a root denotes an entry directly below it only when it is one nonempty path component other than `.` and `..`; every operation refuses any other with `InvalidPath`.
- rejected: an agent proposed passing `.` and `..` to the host; the owner rejected it, because the host resolves them to the directory itself and its parent, so a function given a subdirectory's write half could get the write half above it.

### decision open-read-looks-a-path-up-below-its-root
`relative_path` refuses a `..` component, and `open_read` looks a path up component by component below its root, failing at a link, on Windows a reparse point, at any component.
- rejected: an agent proposed refusing only `..` and keeping the host's following of links; the owner rejected it, because a link below the root still leaves it, so a function given a read half would reach what the process can, which the name operations already prevent.
- rejected: an agent proposed folding `..` away lexically instead of refusing it; the owner rejected it, because `a/../b` must read nothing, and the fold gives one file two spellings and a second rule to state and test.
- replaced: A lookup of the whole path in one host call for `open_read`, which the specification left unstated, was on main; the owner changed it, because a root would then bound only the name operations, while a path read from untrusted bytes takes the multi-component route.