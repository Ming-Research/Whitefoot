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
  that never creates. A program needing exclusive creation would need this
  split; none does yet, and with B a create-if-missing is a create whose
  `AlreadyExists` is ignored followed by an open, which on Windows is two
  host calls where A is one.
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
  so the common case could not be written.
- **E. A second operation, `move_file`,** taking the source's and the
  destination's write halves, with `rename_file`'s atomic replacement and
  its open-handle rule. A move between file systems is a host refusal. POSIX
  gives it as `renameat` with two directory descriptors; Windows as
  `FileRenameInfoEx` whose `RootDirectory` names the destination directory,
  on one volume.

Proposal: E, beside A. Its conformance cases move a file from the working
directory into a subdirectory, over an existing file, and from a missing
name, and keep a handle open across the move.
