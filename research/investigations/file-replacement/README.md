# File replacement

## The question

`std::fs` writes a file by appending to it and setting its length, and
syncs it ([writable directory](../../../design/language/system-interface/writable-directory.md)).
Renaming, removing and syncing a directory were left out until a program
needed them, with Redis's `BGREWRITEAOF` named as that program
([design](../io-model/TIME-AND-FILES.md#writable-directories-and-append-only-files)).
firn now needs it.

An append-only file grows by every write, so a long-running cache or session
store fills its disk, and its replay at start grows without bound. Redis
7.0.15 bounds both by rewriting:
- it writes a new file holding the current dataset and syncs it;
- it renames the new file over the old one, or in its multi-part layout
  renames a new manifest over the old manifest;
- it syncs the directory so that the rename itself survives;
- it removes the files the new layout no longer names.

A program written in Whitefoot cannot do any of the last three steps today.

The question is which operations `std::fs` should add, so that a program can
replace a file with another whole one and remove what it no longer needs,
with a failure at any point leaving one of the two files whole.

## What a program needs

- **Replace by name.** After the replacement the name names the new file,
  and no observer finds the name absent or naming a partial file between the
  two states. POSIX `rename` gives this within one file system.
- **Make the replacement last.** A rename is durable only once the directory
  holding the name is synced. Like `sync_file`, the operation promises only
  that the host was asked; what survives a host failure stays unspecified,
  as the owner ruled for files (Q26).
- **Remove a name.** This is for the old parts and for a temporary file left
  by a rewrite that stopped.
- **Keep open files working.** A file that is open for appending or reading
  stays open, and keeps its bytes, after its name is replaced or removed. A
  writer that still holds the old log therefore cannot lose bytes it has
  already appended. It does append to a file that no longer has a name, so a
  rewrite must switch the writer to the new file in the same step.

## Hosts

- **Linux and macOS.**
  - `renameat` replaces the target within one directory atomically.
  - `unlinkat` removes a name; an open handle keeps the file.
  - `fsync` on a directory handle syncs its entries.
  - io_uring has `IORING_OP_RENAMEAT` and `IORING_OP_UNLINKAT`, and fsync on
    a directory descriptor.
- **Windows.**
  - `SetFileInformationByHandle` with `FileRenameInfoEx` and
    `FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS`
    replaces atomically.
  - `FileDispositionInfoEx` with `FILE_DISPOSITION_FLAG_DELETE |
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` removes a name while handles stay
    open, as on POSIX.
  - Both need the file's open handles to share deletion
    (`FILE_SHARE_DELETE`).
  - `wf__windows_open_delete` opens both rename and removal handles with
    `FILE_WRITE_THROUGH`; `SetFileInformationByHandle` writes its namespace
    changes through before returning. Microsoft's documented
    [write-through behavior](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew#caching-behavior)
    includes flushing NTFS metadata changes such as renames. Windows
    `sync_directory` therefore has nothing further to hand over to the host.
    A host that refuses the open or the requested namespace operation returns
    its error through the existing `IoError` mapping.

## Candidates

- **A. Three operations on the write half.**
  - `rename_file(factory, root: &DirectoryWrite, from, to)` replaces any
    entry named `to`.
  - `remove_file(factory, root: &DirectoryWrite, name)`.
  - `sync_directory(factory, root: &DirectoryWrite)`.

  Each operation names one directory, and its names follow `open_append`'s
  form, a byte range of a name below the root. A rewrite composes them, and
  so does Redis's multi-part layout, which renames a manifest and removes
  several parts.
- **B. One replacing operation.** A file opened under a temporary name is
  synced, renamed over a target and its directory synced, in one call. This
  is harder to misuse for one file, but it covers neither removal nor a
  layout of several files, so B would still need A's removal.
- **C. Writes at an offset**, so a program rewrites in place. A failure
  partway leaves neither file whole, so C does not answer the question.

Two refinements of A:
- **One directory or two.** `renameat` can move a name between directories,
  which can fail across file systems (`EXDEV`) and would take two write
  halves. A rewrite needs only one directory, as Redis's `appendonlydir`
  does.
- **Opening a file new.** A rewrite needs its temporary file empty. With
  `open_append` and `truncate_file(0)` that takes two calls; an operation
  that creates a file new or empties it would take one. This is not needed
  to answer the question.

## Proposal

A: `rename_file`, `remove_file` and `sync_directory`, each in one directory.

Validation, stated before implementing:
- one conformance case per operation through each host route;
- a program that rewrites a log through a temporary file and a rename and
  is stopped at each step, before the rename, after the rename and before
  the directory sync, and after it, then finds one of the two files whole,
  which the writable-directory TODO entry names as the test;
- a case that removes and replaces names of files still open, then shows
  the open handles keep their bytes.

The proposal would be rejected if one of the three hosts cannot give atomic
replacement with open files kept; the program would then need B's narrower
promise, or C.

## Status

Proposed; not adopted.
