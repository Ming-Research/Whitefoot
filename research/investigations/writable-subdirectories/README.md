# Writable subdirectories

## The question

A program holding a directory's write half can create, append to, cut,
rename and remove files directly below it and sync its entries [PRE-2]. It
cannot create a directory, and it cannot obtain the write half of a
directory below its own: `open_directory` takes a read half and returns one,
and every name is one path component.

firn's append-only file rewrite needs both. It keeps Redis 7.0.15's
multi-part layout ([Firn-wf aof-rewrite
investigation](https://github.com/Ming-Research/Firn-wf/blob/6bc2afbc4/research/investigations/aof-rewrite/README.md),
selected by the owner as Firn-wf ledger Q213 A): a directory named by
`appenddirname`, `appendonlydir` by default, below the working directory,
holding a base file, incremental files and a manifest. Redis creates the
directory when it is missing and keeps it when present
(`src/aof.c`, `aofOpenIfNeededOnServerStart` calling `dirCreateIfMissing`,
which ignores `EEXIST`), then appends, renames and removes files inside it
and syncs it (`fsyncFileDir`).

The question is which operation gives a program the write half of a
directory below its own, and with which create rule.

## What a program needs

- Create the directory when it is missing, and keep it and its files when
  present.
- Its write half, to open, append to, rename and remove files inside it and
  to sync its entries, through the existing operations.
- Its read half, to open its files for reading and list them. The existing
  `open_directory` on the parent's read half already gives it.
- The new directory's own entry handed to the host's durability mechanism,
  which `sync_directory` on the parent's write half already does.

## Hosts

- **POSIX.** A directory half is a descriptor opened with
  `O_RDONLY | O_DIRECTORY`. `mkdirat(parent, name, 0777)` creates a
  directory and fails with `EEXIST` when any entry has the name;
  `openat(parent, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)`
  then opens it, refusing a symbolic link or a file at the name.
- **Windows.** `NtCreateFile` with `RootDirectory` set to the parent's
  handle, `FILE_DIRECTORY_FILE` and the `FILE_OPEN_IF` disposition creates
  the directory when missing and opens it otherwise in one call; the
  existing component opens already name files relative to a directory
  handle. `FILE_OPEN_REPARSE_POINT` keeps a link at the name from being
  followed, as the existing component opens do.
- **io_uring.** The other component operations reach the host through the
  shared file adapter, and this one can too.

## Candidates

- **A. One operation that opens a subdirectory's write half, creating it
  when missing.**
  `open_directory_write(factory, root: &DirectoryWrite, name, start, end)
  -> Result<DirectoryWrite, IoError>`. Its create rule is `open_append`'s,
  for a directory. Write authority below a directory comes only from write
  authority over it. The read half comes from `open_directory` on the
  parent's read half, as `Inputs.cwd`'s halves come from two opens; the two
  halves may then name different directories if the name changes between
  the opens, which the writable-directory decision already accepts for any
  pair of halves.
- **B. Two operations.** `create_directory`, which fails with
  `AlreadyExists` when the name is present, and an `open_directory_write`
  that never creates. Exclusive creation needs this split. Without it, B
  spells create-if-missing as a create whose `AlreadyExists` is ignored
  followed by an open, two host calls on Windows where A's `FILE_OPEN_IF`
  is one, and a name replaced between the two calls is opened without
  having been created. A is reopened when a program needs exclusive
  creation, beside the other create rules `docs/todo.md` records.
- **C. No change to Whitefoot.** firn keeps its base, incremental files and
  manifest flat in its working directory. The gap stays: firn could not
  replay a directory Redis wrote without moving its files, and any other
  program needing a directory hierarchy stays unable to express it.

## Proposal

A. Validation, stated before implementing:
- **Conformance cases** for the operation's rules:
  - a missing directory is created, and a file appended through its write
    half reads back through `open_directory` and `open_file` on the
    parent's read half;
  - opening an existing directory keeps its files;
  - a name naming a file is an error;
  - `rename_file`, `remove_file` and `sync_directory` act inside the
    subdirectory through its write half.
- **Hosts:** the cases run on Linux and macOS in the gate and on Windows in
  `io-hosts.yml`.

The proposal would be rejected if one of the three hosts cannot create or
open a directory relative to a directory handle without following a link
at the name; the operation would then need B's separate creation or a rule
for links.

## Moving a file between directories

Redis 7.0.15 upgrades an old single append-only file by moving it from the
working directory into `appendonlydir` (`aofUpgradePrepare`, a `rename`
between two directories). `rename_file` acts within one directory's write
half.

- **D. `rename_file` with two roots.** One operation for both cases, but a
  rename within one directory would pass one handle as both roots, and
  `writes(from_root)` with `writes(to_root)` on one handle overlap [EFF-5],
  so the common case could not be written. Declaring the roots read instead
  would not help: host operations are ordered only where their footprints
  overlap with a write [HOST-1], so a rename that only read its directory
  could be reordered against another rename or a removal in it, which
  changes what the directory holds.
- **E. A second operation, `move_file`,** taking the source's and the
  destination's write halves, with `rename_file`'s atomic replacement and
  its open-handle rule. A move between file systems is a host refusal. POSIX
  gives it as `renameat` with two directory descriptors; Windows as
  `NtSetInformationFile` with `FileRenameInformationEx` whose
  `RootDirectory` names the destination directory, on one volume.

Proposal: E, beside A. Its conformance cases move a file from the working
directory into a subdirectory, over an existing file, and from a missing
name, keep a handle open across the move, and show that one handle passed
as both roots is refused [EFF-5]. The proposal would be rejected if a host
cannot replace the destination atomically across two directories of one
file system, as A would be by a host that cannot create or open a
directory relative to a handle.

## Effects of the operations on their root

A rename, a removal and a move write the directories they change, so the
host orders them against every other change of those directories [HOST-1].
`open_append` reads its root, so two opens that may create entries are
ordered only through other state both reach, such as one factory, and
otherwise in whatever order the host takes them, their outcomes being
inputs of the execution [WAIT-2]: an `open_append` of a name and an
`open_directory_write` of the same name, unordered, leave a file or a
directory there depending on which the host took first. A later rename or
removal of the entry, which writes the root, stays ordered after either.
`open_directory_write` asks no more of its root than `open_append` and
reads it likewise; giving both writes instead would order every creation
through one root at the cost of overlapped creations through it.

## Names

A name given with a root is meant to select an entry directly below it.
The hosts resolve `.` to the directory itself and `..` to its parent:
POSIX `mkdirat(dir, "..")` fails with `EEXIST`, which create-if-missing
accepts, and `openat(dir, "..", O_DIRECTORY | O_NOFOLLOW)` then opens the
parent, since `O_NOFOLLOW` concerns only symbolic links. A function given a
subdirectory's write half would thereby obtain the write half of the
directory above it. The component check refuses `.` and `..` for every
operation that takes a name with a root, as it refuses an empty name or one
holding a separator. The path library, `relative_path` with `open_read`,
keeps its components as given and is outside this rule, so a read half can
still reach the directory above it through a path; `docs/todo.md` records
that asymmetry.

## Windows renames resolve against the file's directory

kernel32's `SetFileInformationByHandle` with `FileRenameInfoEx`, given a
bare target name and no `RootDirectory`, resolves the name against the
process's current directory, not the renamed file's directory. In the
working directory the two coincide; inside a subdirectory the renamed file
left it (`sysubdir-run-namespace` exited 21, its check that the parent
stays unchanged, in io-hosts run 37694724459). Both renames therefore call
ntdll's `NtSetInformationFile` with `FileRenameInformationEx`: a bare name
with no root renames within the file's own directory, and `move_file`
names the destination through `RootDirectory` (passing in io-hosts run
37696388709).

## Status

Adopted: proposals A and E, with the names rule, are PRE-2 of specification v0.98.
