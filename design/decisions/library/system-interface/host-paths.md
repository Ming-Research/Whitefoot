# Host paths

How a program opens files and directories by a path the host interprets: the authority it needs, the opens it has, how lookup works on POSIX and Windows, and how an open's inputs live across its wait.

### decision host-path-opens-need-an-authority
A program opens by host path only through `HostPaths`, a fieldless value the host gives as an optional input that no program can build; a function given only a directory half cannot open by host path.
- rejected: an agent proposed widening the relative path type to accept absolute paths and parents; the owner rejected it, because the same directory parameter would then authorize paths that ignore it, so a write half could reach its parents.
- rejected: an agent proposed a path value built from bytes with path opens on any directory; the owner rejected it, because a path is data, not permission, so either every directory half gains authority over escaping paths, or the broad need is not met.

### decision host-path-opens-never-create
`HostPaths` offers six opens, a read file, a directory's read half and both halves of an existing directory, each from a host string or a byte window with an encoding, and none creates an entry.
- rejected: an agent proposed the existing create-if-missing directory open for host paths; the owner rejected it, because it hides a misconfigured path by creating an empty directory, while a data directory that must exist is refused when missing.

### decision host-path-lookup-is-the-hosts-own
Host-path lookup is the host's own lookup from the given directory, following every link and resolving parent components as the host does, while single-name opens never follow a link at the name.
- rejected: an agent proposed lexical normalization of POSIX paths; the owner rejected it, because it opens a different file than the host when a component is a link, and a path a user names should mean what the host's tools take it to mean.

### decision host-paths-only-where-a-filesystem-exists
POSIX and Windows hosts give `HostPaths`, and a host without a host filesystem gives none, so no program reaches a host-path open there.
- rejected: an agent proposed rejecting targets without a host filesystem; the owner rejected it, because such a target must still run programs that do not use one.

### decision windows-host-paths-follow-win32
A Windows host path is UTF-16 code units looked up in the Win32 dialect, and a relative path whose normalized form begins with base's reported spelling opens from base's handle by its remainder.
- rejected: an agent proposed resolving every relative path from base's reported spelling; the owner rejected it, because a concurrent rename of base could redirect it.
- rejected: an agent proposed resolving parent components physically on Windows; the owner rejected it, because the Win32 dialect does not, and the host's own tools mean by `..` what the dialect means.

### decision existing-directory-halves-share-one-lookup
Both halves of an existing directory come from one host lookup, the second duplicating the first's descriptor, and a byte-window path is copied into a bounded runtime block.
- rejected: an agent proposed two lookups for the two halves; the owner rejected it, because the lookups could select two different directories if an entry is replaced between them.
- rejected: an agent proposed borrowing the caller's buffer across the open's wait; the owner rejected it, because the copy must outlive the caller's buffer while the open waits.