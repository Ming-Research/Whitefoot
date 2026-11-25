# Exclusive file creation

## The question

A program holding a directory's write half creates a file only through
`open_append`, which creates the file when no entry has the name and opens
the one there otherwise [PRE-2]. The program cannot tell which happened.

Firn's append-only file rewrite opens and cuts a temporary file named after
its log. When another dataset shares the log directory and happens to have a
file of that name, the rewrite empties that dataset's file and still reports
success. A test for the name followed by `open_append` does not help: another
process can take the name between the two calls. Only an operation in which
the host tests for the name and creates the file at once can fail on a taken
name, so the standard library has to supply it.

The question is how that operation is spelled, what it does at an existing
entry, and what its row says about its root.

## Hosts

- **POSIX.** `openat(root, name, O_WRONLY | O_APPEND | O_CREAT | O_EXCL |
  O_NOFOLLOW | O_NONBLOCK, 0666)`. With `O_CREAT | O_EXCL`, `open` fails with
  `EEXIST` when the name exists, and the check for existence and the creation
  are atomic with respect to other such opens of the same name in the same
  directory. A symbolic link at the name, dangling or not, is an existing name
  that is never followed. This is the append open's own flag set with
  `O_EXCL` added.
- **Windows.** `NtCreateFile` with `RootDirectory` set to the directory's
  handle, the `FILE_CREATE` disposition, `FILE_NON_DIRECTORY_FILE` and
  `FILE_OPEN_REPARSE_POINT`. An existing entry, a directory or a reparse point
  included, is refused with `STATUS_OBJECT_NAME_COLLISION`, which converts to
  `ERROR_ALREADY_EXISTS` (183). `FILE_OPEN_REPARSE_POINT` keeps a link at the
  name from being followed, as every other name's open does. Without it, a
  dangling link would be followed and its target created.
- **io_uring.** `IORING_OP_OPENAT` takes the same flags. The runtime passes
  an open's flags to the ring, the POSIX helper and the Windows file adapter
  unchanged.

Both hosts' refusal codes are already the `AlreadyExists` class (PRE-2's
class table names `EEXIST` and `ERROR_ALREADY_EXISTS`, and the runtime also
classes `ERROR_FILE_EXISTS`).

## Candidates

- **A. A function of its own,** `create_new_file(factory, root:
  &DirectoryWrite, name, start, end) -> Result<WriteFile, IoError>`. It
  creates an empty file open for appending, and with any entry at the name it
  returns `AlreadyExists` and changes nothing. This is how the library already
  separates create rules: `open_directory_write` creates when missing, and
  the host-path opens never create.
- **B. A create-rule parameter on `open_append`,** an enum of open-or-create
  and create-only. It saves one function, but adds a type, and every existing
  call of `open_append` has to spell the common rule.
- **C1. The row reads root,** as `open_append` and `open_directory_write`
  read it.
- **C2. The row writes root,** as `rename_file`, `move_file` and
  `remove_file` write it.

### The root effect

[HOST-1] orders two host operations of one context when their footprints
overlap with a write. Every `std::fs` operation that takes a factory writes
it, so within one context, operations through one factory are ordered
whatever the row says about the root. The rows differ only for calls given
distinct factories (`factory_share`) in one context. Contexts never share a
borrow of one root.

An exclusive create's result is the order itself:

| First | Then | Results |
|---|---|---|
| `create_new_file(n)` | `open_append(n)` | `Ok`, `Ok` (the append opens the created file) |
| `open_append(n)` | `create_new_file(n)` | `Ok`, `AlreadyExists` |
| `create_new_file(n)` | `create_new_file(n)` | `Ok`, `AlreadyExists` |

Under C1, two such calls through distinct factories in one context have no
order, so which one gets `AlreadyExists` is decided by the host. Under C2, the
create is ordered against every other operation through that root, as
written. That is the reason rename and removal write the directory: so that
the host orders them against its other changes. The cost is that two creates
through one root with distinct factories in one context cannot overlap.

## The name

Rust's `OpenOptions::create_new` has the same meaning: atomic, failing when
any entry has the name, a dangling symbolic link included. Two conventions
would mislead if borrowed:

- POSIX `creat()` is `open` with `O_CREAT | O_TRUNC`, which cuts an existing
  file, exactly the failure above.
- Win32 `CreateFileW` opens an existing file under most dispositions.

`create_new_file` says that the file it gives is new. `create_file` would be
read as either convention.

## Comparison with the single-name operations

| Rule | `open_append` | `create_new_file` | `rename_file` | `remove_file` |
|---|---|---|---|---|
| Root | write half | write half | write half | write half |
| Row on root | reads | writes | writes | writes |
| Name | one component, not `.` or `..`, else `InvalidPath` | same | same | same |
| No entry at the name | creates an empty file | creates an empty file | refused | refused |
| A file at the name | opens it, bytes kept | `AlreadyExists`, unchanged | replaces it at the destination | removes it |
| A symbolic link at the name | `Err`, its target not opened | `AlreadyExists`, its target not created | unchanged rule | unchanged rule |
| Handle credit | spent on success, returned on failure | same | none | none |
| Result | `WriteFile` | `WriteFile` of an empty file | unit | unit |
| Handle across a later rename or removal | keeps its bytes | same rule | — | — |

## Proposal

A with C2 and the name `create_new_file`, as plan and question
approve-exclusive-create of item #180 recommend. Validation, stated before
implementing:

- **Conformance cases:**
  - `syscreate-run-new-file`: a missing name is created empty; its appended
    byte reads back; a second create is `AlreadyExists`.
  - `syscreate-run-existing-file-kept`: an existing file is `AlreadyExists`
    and keeps exactly its bytes.
  - `syscreate-run-existing-directory`: a directory at the name is
    `AlreadyExists`; it and its file stay.
  - `syscreate-run-names`: `.`, `..`, the empty name and `a/b` are
    `InvalidPath`; `a/b` creates no `b` in `a`.
  - `syscreate-run-links` (POSIX fixtures): a dangling link and a link to a
    file are `AlreadyExists`; no target is created and the file keeps its
    byte.
  - `syscreate-neg-through-read-half`: a read half is a type mismatch.
  - `syscreate-neg-root-read-row`: a wrapper that only reads the root is
    refused under C2.
- **Runtime probe** (`compiler/src/backend/ordinary_values_probe.c`, run on
  Linux, macOS and Windows): real host links, dangling and to a file, are
  `AlreadyExists`. The dangling link's target is not created, the file's bytes
  are kept, the credit is returned, and no Windows handle is left. The Windows
  conformance adapter makes no link fixtures, so this is the Windows evidence
  for links.
- **Hosts:** the cases run on Linux and macOS in the gate, the self-arranging
  ones on Windows in `io-hosts.yml`.

The proposal would be rejected if one of the three hosts could not create
exclusively relative to a directory handle without following a link at the
name.

## Status

Implemented on the work branch of item #180; the root effect follows the
owner's answer to approve-exclusive-create.
