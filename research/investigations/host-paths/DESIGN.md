# Host paths without widening delegated directory authority

Status: proposed direction, 2026-10-13. This is a design study for an owner
decision, not an approved language rule or an implemented API. Inspected base:
`3adedae9bb1a9028e0afd95d2776e5330db4abc1`, on `claude/host-paths`.
No compiler, specification, design-tree or conformance change belongs to this
study. The examples below have been inspected, not compiled or executed.

## Question and comparison

How can Firn accept `firn /etc/redis/redis.conf`, open the existing directory
named by `dir /var/lib/redis`, and open `include /etc/redis/common.conf` from
configuration bytes, while a helper given only a directory keeps the existing
read/write distinction and one-component naming discipline?

Compare an explicit host-namespace authority with widening existing directory
operations. Reject the explicit-authority proposal if the intended programs
cannot express their paths, if a helper can manufacture this authority from
bytes or a directory, if its implementation needs a second ownership/effect
system, or if its contracts exclude an embedded target. Native implementation
feasibility and the acceptance observations below can overturn it. This study
makes no performance prediction and schedules no timing experiment.

The P0 release need is supplied by Firn; this checkout contains no reproduced
Firn run. The observed gaps here are declarations and cases, not newly measured
compiler results. An absolute argument is rejected by the current path
conversion; a byte window has no conversion to a path; a directory name cannot
contain separators; the writable-directory open creates an absent directory.
Creating Firn's configured data directory would conceal a configuration error.
Creating AOF entries *inside an existing selected directory* is intended.

## Current authority and evidence

The [constitution](../../../docs/constitution.md#external-interaction-through-ordinary-objects)
requires ordinary objects, ownership, effects and callable contracts for
external interaction. Its development objectives include kernels and small
embedded systems. It does not require the process working directory to be a
filesystem sandbox, nor require every target to have a host filesystem.
These facts permit a new ordinary authority object; they do not uniquely
select this design.

The [active specification](../../../spec/kernel-spec.md), PRE-2, declares:

* `Inputs.cwd: Directory`, with independently closable read and write halves.
  No `chdir` or host-namespace root is supplied.
* `relative_path(HostString) -> Result<RelativePath, PathError>` and
  `open_read(factory, root: &DirectoryRead, path: &RelativePath)`.
* The name-window functions `open_file`, `open_directory`,
  `open_directory_write`, `open_append`, `rename_file`, `move_file` and
  `remove_file`. A name is one nonempty component other than `.` or `..`;
  an invalid name returns `IoError.InvalidPath`.
* `open_directory` opens an existing read half; `open_directory_write` opens
  or creates a write half. There is no existing-only writable path open.
* File append, rename and synchronization already provide the AOF operations.
  `sync_file` and `sync_directory` promise handoff to host durability, not
  survival under an invented crash model.

The premise that today's program can touch only objects beneath cwd needs
correction. A **name** and a **RelativePath** have different contracts. PRE-2
describes `open_read` as opening a file “below root”, but does not define
confinement, dot-dot resolution or a filesystem symlink policy. The
[dot-dot case](../../../tests/conformance/cases/run-syspath-dotdot-preserved.wf)
and its [manifest](../../../tests/conformance/manifest.jsonl) explicitly describe
paths as not confined. The case actually observes only successful conversion;
it does not open an outside file or prove spelling preservation. Neither its
description nor runtime behavior supplies a missing normative guarantee.

Source inspection of
[ordinary_values.c](../../../compiler/src/backend/ordinary_values.c) shows
that POSIX conversion rejects a leading slash or embedded NUL but does not
remove or reject `..`; `open_read` passes the spelling to a native relative
open without no-follow flags. The same file gives one-component opens
no-follow flags. The
[Windows adapter](../../../compiler/src/backend/windows_runtime.c) rejects
rooted and drive-prefixed RelativePath values and has separate follow and
no-follow routes. These are implementation observations, not executions on
this revision. Module-source symlink restrictions in MOD-2/MOD-11 govern
compiler inputs and cannot be imported into the filesystem API.

The firm boundaries worth preserving are possession of an ordinary object,
read versus write authority, and the single-name rule. Even no-follow opens
do not promise a global sandbox: mounts, hard links, external renames and
aliases exist. A retained directory is a selected object, not a lexical
prefix check. Existing `open_read` must not be advertised as confined.

### Which safety property each mechanism establishes

Filesystem authority below is an API property built from ordinary types and
contracts. It is not the kernel's `copy`/`drop` capability classification, and
CAP-1 expressly adds no special capability category. The following separates
what the specification says, why the design selected it, and what the native
implementation supplies. “No escape from the root” needs this scope: entry-name
operations prevent parent selection and link redirection; the same statement
is not established for `open_read`'s multi-component RelativePath.

| Part of the current model | Spec rules and design ground | Protected property and threat | Present limit |
|---|---|---|---|
| Authority is supplied by a handle, not an ambient path-only open | PRE-2 opaque host handles and callable parameters; TYPE-2 opaque construction; CAP-1 ordinary ownership/effects; EFF-1/EFF-2 formal state rows. [System interface](../../../design/language/system-interface.md) rejects ambient mutable host access and forgeable descriptors; [declaration home](../../../design/language/system-interface/declaration-home.md) makes handles ordinary opaque declarations. | A library given bytes cannot manufacture a filesystem handle or silently choose an unrelated ambient cwd. The caller can withhold a directory or provide just the intended read half. This prevents undeclared inter-function authority and forgeable integer-descriptor access. | A directory parameter is not itself proof of every spatial restriction: its functions determine its reach. The existing `open_read` route admits host-relative parent/link lookup. Effect rows describe behavior; they do not grant permission. |
| Read authority cannot manufacture write authority | PRE-2 DirectoryRead/DirectoryWrite signatures and separate closes; TYPE-2 and PROV-6 derive ordinary structure/linearity; OWN-1 consumption. [Writable directories](../../../design/language/system-interface/writable-directory.md) rejects opening write authority from a read half. | A config-reading or search helper with only DirectoryRead cannot append, rename, remove or obtain a DirectoryWrite. A helper cannot retain a closable owner by implicit copy or forget its close on a normal exit. | A caller that separately passes a write half or the proposed broad HostPaths intentionally grants more. Public Directory construction does not prove both halves name the same directory. |
| Entry names cannot escape syntactically to an ancestor or an absolute namespace | PRE-2 single nonempty component excluding `.` and `..`; FN-8 proves the byte-window bounds. [Writable-directory names](../../../design/language/system-interface/writable-directory.md) says this prevents a subdirectory's write half obtaining its parent's write half. | Rejecting separators, empty names, self and parent names prevents a caller-controlled name from replacing the supplied root with an absolute path or selecting its parent. The byte-range proof prevents an out-of-bounds read before name validation. | RelativePath is expressly distinguished from a name in that design node. This is not a lexical-prefix sandbox for all filesystem objects. Hard links, mounts and external aliases do not become memory-safety violations or forbidden host outcomes. |
| One-component opens do not follow links | PRE-2's directly-below-root name rule is the relevant contract, but it does **not explicitly state** a link refusal rule. Current POSIX O_NOFOLLOW and Windows FILE_OPEN_REPARSE_POINT plus opened-kind rejection implement it. The [writable-directory](../../../design/language/system-interface/writable-directory.md) locality decision and [enumeration](../../../design/language/system-interface/directory-enumeration.md) byte-entry design explain why the name remains an entry selected under a retained root. No existing node was found that explicitly owns the general no-follow choice. | A directory entry replaced by a symlink cannot redirect a traversal open or AOF append to an unrelated target. An atomic no-follow open checks the entry being opened, preventing the lstat-then-follow substitution race. With exactly one component, final-component no-follow is sufficient; with several components it would not protect intermediate links. | This is current implementation behavior with missing explicit normative/design ownership, to be resolved in the owner's direction. Windows currently rejects every reparse-point kind at the type check, a broader refusal than just symlinks/junctions. Renames/removes affect entries and need a separately stated policy. |
| Acquisition, completion and close preserve resource ownership | PRE-2 factory credit/close contracts; OWN-1/PROV-6 linear handles; STOR-3 empty fieldless opaque drop; WAIT-2 loan return on host outcome; HOST-1 order through shared written state. [Handle factory](../../../design/language/system-interface/handle-factory.md) and [system interface](../../../design/language/system-interface.md) own these choices. | An in-flight native operation must not outlive its path, buffer or directory loan. Failure cannot leak an acquired half or spend a handle credit permanently. AOF ordering must be expressed through shared state rather than assumed from native path identity. | The ordinary model does not prove OS permissions, name existence or crash survival. These are defined host outcomes and PRE-2/SCOPE-3 external conditions. |

The missing explicit no-follow rule is a specification/design gap rather than
a reason to weaken the existing runtime. The proposal asks the owner to make
the path/name distinction normative and to decide the legacy path scope. It
does not assert an already approved universal root-confinement rule.

The relevant historical grounds are the
[system-interface decisions](../../../design/language/system-interface.md),
[writable-directory decisions](../../../design/language/system-interface/writable-directory.md),
[directory-enumeration decisions](../../../design/language/system-interface/directory-enumeration.md),
[handle-factory decisions](../../../design/language/system-interface/handle-factory.md)
and [host-string backing decision](../../../design/language/system-interface/host-string-lease.md),
under [language](../../../design/language.md). The
[I/O study](../io-model/DESIGN.md),
[first-principles derivation](../io-model/FIRST-PRINCIPLES.md) and
[time-and-files study](../io-model/TIME-AND-FILES.md) explain why selectors,
acquisitions and explicit closes are separate. Their old permits, region
syntax and deferred file operations are historical; PRE-2's current factory
reference and file operations govern this proposal. “No capability root” in
that old study rejects a special language-level authority model, not an
ordinary opaque input that grants a particular operation.

Current discriminating cases are
[absolute rejection](../../../tests/conformance/cases/run-syspath-absolute-rejected.wf),
[byte argument type rejection](../../../tests/conformance/cases/sys14-no-path-from-bytes.wf),
[separator rejection](../../../tests/conformance/cases/sys14-open-directory-component.wf),
[empty name rejection](../../../tests/conformance/cases/sys14-open-directory-empty-name.wf)
and [dot-name rejection](../../../tests/conformance/cases/sysubdir-run-dot-names.wf).
The byte mismatch remains a valid negative test if a *different* API accepts
bytes. None should be weakened to make the proposal pass.

## Options

The platform, error and acceptance sections below apply to every option;
this table identifies the differences, including what each option alone
does not deliver.

| Option | Expressibility | Authority and compatibility | Delivery cost and risk |
|---|---|---|---|
| **A. Explicit host-path authority in Inputs (recommended)** | Absolute paths, parents, runtime byte paths, linked selected roots and existing-only directory acquisition. An explicit base handles relative configuration paths without changing process cwd. | A new ordinary object grants host namespace lookup under process credentials. Directory-only helpers receive no new operations. Existing name rules and read/write halves stay. This adds an explicit broad route; it does not retroactively make legacy `open_read` confined. | PRE-2 input/type/functions, encoding and resolution rules; ABI input construction/destructuring updates; three host adapters and new conformance fixtures. No syntax, checker category or proof rule. Additional API surface makes the boundary visible. |
| **B. Widen RelativePath and relative_path; no new authority** | Absolute argv paths and host parents. Alone it still cannot read a path from file bytes, select a directory, or require an existing writable directory; it needs byte construction and directory-path opens too. | The same directory parameter now authorizes absolute paths that ignore it. In particular an existing write half could open parents if the new operations accept it. Retaining read/write types avoids write-from-read but loses the existing locality of write delegation. `RelativePath` becomes a misleading name. | Fewer new input fields, but changes an existing contract and the absolute-rejection verdict, path representation/backing, all consumers and all host adapters. The smaller declaration diff is not a design reason. |
| **C. Byte-constructed Path value and path opens on any directory** | All requested paths when the value supports absolute paths and parents; a value can be stored and reused. Needs existing-only directory operations in addition to the value. | A path is data, not permission. A directory authorizes whatever its new path opens allow. Absolute and escaping write paths broaden delegated write authority; refusing escape preserves that authority but fails the broad-host-path part of the need. Read-only roots must never yield write halves. | New value representation, storage/lifetime and conversion contracts, plus opens, platform rules and conformance. An opaque heap-owning path with empty drop is invalid; use ordinary owned storage or an explicit linear close. A validated path can prove encoding, never future existence or safe resolution. |
| **D. Directory-by-path open with MustExist / NoCreate semantics** | Opens a named existing directory and distinguishes absence from successful creation. Alone it does not add absolute authority, file-byte construction or include-file opens. | Creation policy and lookup authority are independent. D inherits A's explicit host authority, B/C's widened directory authority, or today's narrower path source; a mode never grants authority by itself. | A fixed existing-only operation, or a typed mode on a path-directory operation, plus no-create native disposition, kind/error checks and fixtures. This is a required component of A/B/C/E, not a competing complete host-path architecture. Recommend fixed MustExist for the new selected-root open; keep the current one-name create-if-missing operation. |
| **E. New host authority plus a reusable owned Path value** | A's coverage plus reuse without repeated decoding; C's value can be supplied to authority-taking opens. | Keeps A's separation. The value does not grant authority and the authority is still mandatory for the broad operations. | A's work plus C's storage/conversion interface. Prefer this if a concrete consumer needs retained decoded paths or a bounded conversion buffer cannot support the relevant targets. No such need is established by Firn's three opens. |

For **every** complete option, explicit path lookup follows intermediate and
final links reached after the selected host dialect's normalization. Existing
single-component directory/file opens used for traversal must not follow links. A symlink passed as the user's chosen root is different
from a symlink encountered later by enumeration. Adding a follow flag to the
existing name opens would mix these roles; it is not selected. Rename/remove
operate on directory entries and must not dereference a link to modify its
target; the specification amendment must state their exact link-entry outcome
separately from the open policy.

B and C need the same Windows resolution work as A; a new name or path type
does not supply that implementation. On hosts without symlinks, ordinary
existing paths work for all five choices and no symlink emulation is required.
On targets without a host filesystem, A/E supply no host-path authority;
B/C must report unavailable filesystem operations through ordinary outcomes
or use an entry that receives no such host objects. D inherits the chosen
resolver's availability. None may reject the target merely because it lacks Unix, Windows, links, threads or a heap.

An additional alternative is to pre-open every allowed file/directory in a
launcher. It can attenuate authority well, but cannot satisfy arbitrary later
include paths read from configuration without a new resolver service. It is
not a replacement for this need. Rewriting absolute paths relative to cwd,
copying configuration files, changing process cwd, or accepting create-on-open
for `dir` would hide the gap and is not proposed.

### D is a creation-policy component, not a replacement for A

The new option D is an `open_directory_by_path` operation whose creation policy
is **MustExist**, also described as **NoCreate**. A missing directory is an
error. It can be offered as a separate existing-only function or as one value
of an ordinary `DirectoryOpenMode` with `MustExist()` and `CreateIfMissing()`.
The mode only selects creation behavior; it does not authorize absolute paths,
parents, links or write access. Those choices still come from A, B or C.
The former option D, explicit authority plus retained Path storage, is now E.

Recommend the separate existing-only path function already shown under A.
Firn's configured root requires MustExist; its AOF subdirectories continue to
use today's create-if-missing single-name operation. Adding a mode to that
existing operation would still leave multi-component paths inexpressible.
A type-valued mode is viable when the *same* path operation has a demonstrated
need for both policies, but is not necessary for these acquisitions. Checking
existence and then calling a creating open is not NoCreate: removal between
the calls would turn the configuration error into creation.

### Parent components: lexical or filesystem resolution

Do not conflate path-data validation with object lookup. On POSIX, absolute and
directory-descriptor-relative `openat` resolve `..` in component order through the
filesystem, after any preceding symlink. For example, let `base/link` point to
`/outside/child`: `link/../marker` selects `/outside/marker`, not `base/marker`.
Removing `link/..` in a constructor changes which file the user requested and
can hide a missing or unsearchable intermediate directory.

Windows needs a different, explicitly named dialect. Ordinary Win32 DOS/UNC
paths undergo textual dot-component normalization before the remaining path
is opened; extended `\\?\` paths skip that normalization. Choosing native
host semantics therefore does not mean promising POSIX-style physical parents
on Windows. [Microsoft's normalization description](https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats#path-normalization)
states the textual removal and the extended-prefix distinction. The current
Whitefoot Windows worker calls NtCreateFile directly and supplies no such
normalizer; the proposed dialect is new work, not existing behavior.

| Option | POSIX `..` | Windows `..` | Why |
|---|---|---|---|
| A | Preserve input; the OS resolves parents after links at open time, with no root-confinement promise on this broad route. | Use the selected ordinary Win32 normalization for DOS/UNC, then native lookup; preserve extended-path spelling and its host outcome. | An explicit host-path operation should mean the user's native path, while its base is explicit rather than ambient. |
| B | Same OS resolution as A. Widening the constructor must not collapse components. | Needs A's dialect conversion; merely allowing drive/UNC strings in the current validator is insufficient. | Reusing RelativePath does not make a lexical rewrite equivalent to host lookup. |
| C | The stored Path validates encoding and preserves components; `..` is resolved by the OS when opened from the chosen directory. | Store the spelling/dialect; apply its host normalization when the base is supplied for opening. | Construction has neither a root object nor knowledge of the current link targets. Path data cannot certify containment or existence. |
| D | Inherits the path resolver of A/B/C/E; MustExist changes no parent resolution. A minimal extension using today's RelativePath preserves the spelling for native lookup. | Inherits the chosen dialect; NoCreate does not add DOS/UNC translation. | Creation policy is independent of path interpretation and authority. |
| E | Same OS resolution as A; retained Path values preserve components. | Same dialect-specific normalization as A, with stored input data. | Reuse should not change which object is selected. |

A lexical-all-platform alternative would be a different logical-path API;
it is not selected for host paths because the POSIX witness distinguishes it
from native lookup. A confined-directory alternative would reject escape or
use a verified beneath-root resolver; it must also address symlink targets and
namespace races. Neither lexical cancellation nor a string-prefix check proves
confinement. Windows explicit-base lookup across renames remains a feasibility
condition: converting the base handle to a string and reopening it can lose
the retained-object guarantee. Do not quietly weaken that condition to make
the DOS translation easy.

## Recommended ordinary interface

Names in this section are provisional. Add `HostPaths`, an ordinary opaque
`nocopy` input, and append `host_paths: Option<HostPaths>` to `Inputs`.
It grants lookup in the host filesystem namespace available to this execution;
it grants no OS permission bypass, raw descriptor or process-directory mutation.
It is backed by invocation-lifetime metadata, like current argument handles,
and owns no separately closable native descriptor. Its empty drop must really
be empty. Do not allocate a hidden per-open owning path in this value.
If implementation needs independently owned resources instead, revise its
linearity and add an explicit factory-accounted close before implementation.

Use argument-backed HostString or a proved byte window directly, rather than
introducing a path value whose only consumer immediately opens it. Each
operation gets both the broad authority and a `DirectoryRead` base. The
directory chooses the relative starting point; it is not the authority for
absolute lookup or for acquiring write access. A callee given just a read
half cannot obtain a write half. A callee also given HostPaths intentionally
has additional authority, just as a callee given a separate write half does.

Proposed declarations use the existing grammar and types (shown with aliases
resolved to their owning modules):

```whitefoot
public opaque nocopy struct HostPaths {
}

public enum PathEncoding {
  NativeBytes();
  Utf8();
}

public fn open_host_read(factory: &std::io::HandleFactory, paths: &HostPaths, base: &DirectoryRead, path: &std::text::HostString) -> result: Result<ReadFile, std::io::IoError> reads(paths), reads(base), reads(path), writes(factory) waits doc "Opens an existing regular file by host path.";

public fn open_host_read_bytes(factory: &std::io::HandleFactory, paths: &HostPaths, base: &DirectoryRead, name: &[u8], start: u64, end: u64, encoding: PathEncoding) -> result: Result<ReadFile, std::io::IoError> reads(paths), reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} doc "Opens an existing regular file by the selected encoded host path.";

public fn open_host_directory(factory: &std::io::HandleFactory, paths: &HostPaths, base: &DirectoryRead, path: &std::text::HostString) -> result: Result<DirectoryRead, std::io::IoError> reads(paths), reads(base), reads(path), writes(factory) waits doc "Opens an existing directory for reading by host path.";

public fn open_host_directory_bytes(factory: &std::io::HandleFactory, paths: &HostPaths, base: &DirectoryRead, name: &[u8], start: u64, end: u64, encoding: PathEncoding) -> result: Result<Directory, std::io::IoError> reads(paths), reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} doc "Opens both halves of an existing directory by host path and never creates it.";
```

The directory pair and read-only directory functions serve different current
consumers: Firn needs the pair for data storage and subsequent relative includes;
wfgrep needs only a readable selected root. The names must make that difference
clear when the interface is finalized; a mechanically symmetric family of
every encoding and authority combination is not required for this decision.
The pair open resolves once, then obtains two independently closable handles
to that same directory. It spends two factory credits on success and retains
neither handles nor credits on failure, including failure to obtain the second
half. Public construction of `Directory` still implies no matching relation
between arbitrary halves.

All opens remain `waits`; selectors contribute reads and the acquisition
contributes `writes(factory)`, as current PRE-2 does. The directory pair does
not create an entry. No global `world` effect or root-identity tracker is added.
HOST-1, EFF-1/EFF-5 and WAIT-2 still govern order. After selecting the data
directory, use the same factory through append, file sync, rename and directory
sync so the whole AOF publication chain has an explicit shared written state,
including the transition from file operations to directory operations. Different
factories/handles are not ordered merely because host paths alias.

Retain the caller's byte loan until completion; interpret only `[start,end)`
and retain no source reference after return. Native scratch encoding and NUL
termination belong to the ordinary operation bundle. There is no borrowed
path object stored past the call and no ownership exception.

### Resolution and encoding proposal

* An unqualified relative path starts at the supplied base. Absolute paths
  use the namespace denoted by HostPaths. Parents are permitted. Lookup
  follows intermediate and final symbolic links, and the host decides a loop,
  dangling link or excessive resolution failure. Do not lexically collapse
  POSIX `link/..` before host lookup. This is a resolver, not a beneath-root
  check. A returned directory anchors subsequent name opens to that object.
* Firn chooses the base as application state. Startup configuration can use
  cwd; after applying `dir`, a subsequent relative include can use the selected
  directory's read half. This can express configuration-order semantics
  without `chdir`; this study does not claim to have audited Redis's entire
  include parser, globbing, recursive includes or directive ordering.
* NativeBytes is raw non-NUL path bytes on POSIX. On Windows propose explicit
  UTF-16LE code units, including unpaired surrogates, an even byte count and no
  zero code unit. HostString is passed losslessly without a UTF-8 round trip.
  Utf8 validates Unicode scalar encoding and converts to native encoding;
  on POSIX it uses the same UTF-8 bytes. Config parsers choose an encoding,
  not the library's locale. Empty paths and embedded native NUL return
  InvalidPath. Native byte identity is a proposed clarification, not a
  claim that PRE-2 already defines the Windows encoding.
* Windows qualified drive paths (`C:\data\redis.conf`) and UNC paths
  (`\\server\share\redis.conf`) resolve in their named volume/share.
  DOS paths accept host separators; ordinary DOS slash conversion and lexical
  dot-component normalization precede native link lookup. They do not use
  POSIX physical-parent rules. A relative
  path uses the supplied directory. A root-relative path uses that base's
  volume/share root. A drive-relative `C:part` uses the base when it is on C,
  otherwise the invocation's captured current-directory context for C,
  including its host default. That context belongs to HostPaths and is not
  reread from mutable ambient process cwd on each call.
* Extended Windows filesystem paths (`\\?\C:\...` and
  `\\?\UNC\...`) keep their host dialect's different parsing rules;
  do not silently add/remove the prefix or normalize its separators and dots.
  Device paths, alternate streams and other namespaces require explicit
  classification: this proposal exposes regular files and directories only,
  with no new device API. Host syntax errors return InvalidPath; a resolved
  unsupported object kind returns Unsupported (directories passed to a file
  open return IsDirectory). Ordinary file streams supported by the host need
  a named acceptance case, not an accidental path escape through a component
  open. Drive and UNC namespaces mean HostPaths is not a literal `/` directory.

Microsoft documents the distinct drive-relative/qualified/UNC forms and
normalization contexts in [Windows path formats](https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats).
Its [namespace reference](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)
documents the different extended-path parsing and device namespace. These
sources establish why the mapping needs an explicit contract; the base and
captured-context choices above are this proposal, not a claim about a current
Whitefoot implementation.

### Weak and embedded targets

HostPaths is absent when the invocation grants no host filesystem namespace,
whether because the target lacks one or the launcher withholds it. Matching
None is ordinary program behavior; it grants no fallback permission through
cwd. An embedded filesystem with no symlinks can supply the authority for its
actual namespace; ordinary paths work and unavailable operations return
Unsupported. Do not emulate Unix links or require a host OS to compile
unrelated programs. PROG-3/MOD-9 already allow entry signatures other than
Inputs, and STOR-8 admits no-heap entries. Bare-metal programs need not
construct today's host-oriented Inputs (whose mandatory cwd already needs a
host integration); the new optional field does not solve or worsen that
pre-existing integration question by manufacturing fake directories.

Byte-window opens impose no mandatory source heap allocation or new thread.
A bounded-memory filesystem implementation must obey the same window and
error contract. This is an obligation and an acceptance row, not verified
embedded implementation evidence. A target lacking only links is distinct
from a target lacking all filesystem authority. Unsupported must not conceal
a missing implementation on a host this change claims to support.

### Error and resource outcomes

Use existing IoError variants; introduce no new exceptions or traps. Wrong
window proofs are FN-8 source rejections, while valid windows containing bad
path data return InvalidPath. A missing component or dangling target yields
NotFound; a nondirectory intermediate or final target of a directory open yields
NotDirectory; permission/readonly refusals retain PermissionDenied/ReadOnly;
native or factory exhaustion uses ResourceExhausted. A POSIX link loop maps through the current `ELOOP` classification to
InvalidPath with its native code; Windows link failures retain their stated
native mapping. Do not invent a success or a confinement error. Unsupported covers unavailable operation/object kinds. Actual native
error classification must use a stated per-host mapping, not force unlike OS
failures into one invented errno. The decisive absent-directory fixture must
use a searchable existing parent so its NotFound outcome is unambiguous.

Preserve PRE-2's `code` then `origin` fields, native errno/Win32 codes,
NTSTATUS-to-Win32 conversion, origin 1 for opens and 4 for open status queries;
synthesized errors have zero code and origin 0. Legacy relative_path still
uses PathError.PathInvalid. This proposal validates bytes during open and
therefore returns IoError.InvalidPath. Failed new opens restore acquisition
credits. Existing close errors remain ordinary results and linear owners are
consumed exactly once on success and failure. There is no stronger durability
promise and no timeout-based source acceptance.

For the proposed existing-only directory open, the portable distinction is:

| Situation, with an otherwise valid path | Proposed kind | Current native mapping and acceptance setup |
|---|---|---|
| A required intermediate or the final directory does not exist; a followed link is dangling | NotFound | POSIX ENOENT; Windows ERROR_FILE_NOT_FOUND or ERROR_PATH_NOT_FOUND. Use a searchable existing parent and independently verify that no directory appeared. |
| An intermediate component is a regular file, or the final target exists but is not a directory | NotDirectory | POSIX ENOTDIR from an O_DIRECTORY open. Windows native ERROR_DIRECTORY maps to NotDirectory; if opened-handle kind inspection finds the wrong kind, the new API must produce this result explicitly. |
| Search/traverse/open access is denied | PermissionDenied | POSIX EACCES/EPERM; Windows ERROR_ACCESS_DENIED, ERROR_NETWORK_ACCESS_DENIED or ERROR_PRIVILEGE_NOT_HELD. A denied ancestor can mask absence; do not promise NotFound when the host could not search it. |

These mappings are inspected in
[ordinary_values.c](../../../compiler/src/backend/ordinary_values.c), `wf_error`.
The current wrong-kind finish maps `WF_FILE_OPEN_OTHER_KIND` to Unsupported;
the [Windows kind classifier](../../../compiler/src/backend/windows_runtime.c)
can return that outcome for a regular file when a directory was requested.
Consequently the new NotDirectory guarantee needs a distinct directory-kind
refusal or a directory-only native constraint; it is not already supplied by
changing the path validator. Keep disallowed links/reparse kinds distinct from
“regular file where a directory was required”. A synthesized wrong-kind result
has zero code/origin; a native rejection keeps its native code and origin.
Do not perform a path-based status precheck to manufacture these distinctions.
Inspect the object actually opened or use the native directory-only constraint.

NotFound does not trigger mkdir. NotDirectory does not trigger a file open.
PermissionDenied does not trigger a cwd-relative retry. Failure returns every
reserved credit and closes any provisional handle. None of these outcomes is
a source rejection or an exception.

## Current runtime and option mapping

The following is source inspection of the listed completion units at the
study base, not a Linux/macOS/Windows execution. It covers the call paths and
constraints that the new interfaces would actually use.

### Existing native routes

| Operation | Ordinary-value and completion route | Native behavior relevant to the proposal |
|---|---|---|
| open_read | `wf__body_open_read_start` in [ordinary_values.c](../../../compiler/src/backend/ordinary_values.c) calls the common open with flags zero and expected regular kind; [bridge.c](../../../compiler/src/backend/completion/bridge.c) submits `WF_FILE_OPEN_AT`. | POSIX openat gets O_NONBLOCK from the regular-kind helper, then fstat checks the acquired descriptor. There is no O_NOFOLLOW. Windows uses its root-relative NtCreateFile worker without FILE_OPEN_REPARSE_POINT and with FILE_OPEN. |
| open_directory / open_file | `wf_component` validates and copies one name into the operation frame, then uses the same common open request. | POSIX directory flags are O_DIRECTORY\|O_NOFOLLOW, file flags O_NOFOLLOW\|O_NONBLOCK. Windows maps the no-follow selector to FILE_OPEN_REPARSE_POINT and rejects an opened reparse kind. Both use existing-only open dispositions. |
| open_append | Common open request after single-name validation. | POSIX O_WRONLY\|O_APPEND\|O_CREAT\|O_NOFOLLOW\|O_NONBLOCK, mode 0666. Windows write-file class selects FILE_OPEN_IF and no-follow. This creates a file entry, not the configured data directory. |
| open_directory_write | Separate `WF_FILE_OPEN_DIRECTORY_WRITE` request, not merely an OPEN_AT flag. | [file_posix.c](../../../compiler/src/backend/completion/file_posix.c) performs mkdirat(0777), accepts EEXIST, then openat with O_RDONLY\|O_DIRECTORY\|O_NOFOLLOW\|O_CLOEXEC. [file_windows.c](../../../compiler/src/backend/completion/file_windows.c) invokes the creating directory worker, which uses FILE_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT and FILE_OPEN_IF. |

The Windows worker in
[windows_runtime.c](../../../compiler/src/backend/windows_runtime.c) validates
a relative UTF-16 spelling, fills `UNICODE_STRING`, supplies the retained HANDLE
as `OBJECT_ATTRIBUTES.RootDirectory`, and calls NtCreateFile. It does not
translate DOS drive paths, UNC paths or extended prefixes, and does not capture
per-drive current directories. These tasks are common work for every option
that promises that dialect. Removing `relative_path_valid` alone does not do it.
Creation disposition is independent of following: FILE_OPEN instead of
FILE_OPEN_IF prevents creation; FILE_OPEN_REPARSE_POINT controls the selected
entry, not the creation policy.

Windows then queries FileAttributeTagInfo on the opened HANDLE, closes it on
failure/wrong kind, adopts it into a CRT descriptor and registers its class.
The classifier currently treats *every* FILE_ATTRIBUTE_REPARSE_POINT as a link,
before looking at the directory bit. A specification about only symlinks and
junctions must not claim this already implements all reparse-tag distinctions.
The proposed direction preserves no-follow entry opens and asks the owner to
retain this conservative refusal until a separate nonredirecting-tag need is
established.

On POSIX, the ordinary open leaf and Linux ring both check fstat on the descriptor
actually returned, and close it on status failure or wrong kind. The regular
open's O_NONBLOCK prevents opening a FIFO from blocking before its kind is known.
That detail belongs in new read-file opens as well. A directory-only open should
use O_DIRECTORY, without O_CREAT, and a selected-root path should omit O_NOFOLLOW
so the explicit user's final link is followed.

### Completion transport, lifetime and acquisition

* [linux_io_uring.c](../../../compiler/src/backend/completion/linux_io_uring.c),
  `wf_linux_io_uring_carries` and its SQE builder, carries `WF_FILE_OPEN_AT` as
  IORING_OP_OPENAT. It supplies directory descriptor, path pointer, flags and
  optional mode; it does not use openat2 or request a beneath-root guarantee.
  The independent mkdir-plus-open writable-directory request is not carried.
  A new single existing-only open can reuse the ordinary shape; a composite
  open-plus-duplicate pair has no existing ring shape as one request.
* [windows_iocp.c](../../../compiler/src/backend/completion/windows_iocp.c),
  `wf_windows_iocp_carries`, carries positioned reads and connect/receive/send,
  not file or directory opens. Opens go through the shared typed file adapter;
  the NtCreateFile worker can wait for a pending result with WaitForSingleObject.
  Successful file opens register a descriptor class, while the first later
  eligible I/O offer associates the HANDLE with IOCP. A new open must not invent
  an OVERLAPPED-open packet or associate an unqualified directory handle.
* [bridge.c](../../../compiler/src/backend/completion/bridge.c) tries a native
  ring, then the applicable adapter route. [file_adapter.c](../../../compiler/src/backend/completion/file_adapter.c)
  supports helpers and a joining thread claiming its own queued request; some
  routes execute inline. macOS has no Linux ring here and uses the same typed
  POSIX leaf. “Waits” promises the outcome/loan boundary, not another thread or
  a native asynchronous open. After a route owns a request, an adapter failure
  cannot trigger an independent reopen through another route.
* Ring requests retain native path pointers. The bridge neither copies a full
  path nor accepts a length; argv-backed RelativePath has stable native NUL
  termination. Component names are copied into the operation frame. Its
  component bounds are 255 bytes on Linux, 1023 on macOS and 510 on Windows;
  these are not full host-path capacities. Byte-window opens need their own
  aligned/native-encoded, NUL-terminated scratch kept through native completion.
  Do not submit a nonterminated slice, truncate to that component buffer, or
  release conversion scratch when submission returns.
* The current Windows worker scans UTF-16 until NUL with an UNICODE_STRING-sized
  bound. A new length-carrying byte interface must check encoding/length without
  reading beyond its supplied window. Native capacity failure remains an
  ordinary path/resource outcome, not an unsupported-target exclusion.
* Existing opens reserve one factory credit and return it on refusal. A new
  Directory pair needs two credits, one selected native object, duplication or
  a same-object reopen, and rollback if the second handle fails. POSIX invocation
  cwd already uses F_DUPFD_CLOEXEC; Windows cwd currently uses two independent
  opens of `.`. The latter is not evidence that a new path-selected pair resolves
  once. Completion publication can immediately release the frame, so cleanup
  and all native path access must finish before that publication.
* [runtime.c](../../../compiler/src/backend/completion/runtime.c) manages wake
  and park notifications. It contains no path pool, directory-pair lifecycle
  or namespace resolver to reuse. No change to its ordinary scheduling model
  follows merely from accepting another path representation.

### Mapping the options onto these routes

| Option | Linux | macOS | Windows | Shared lifecycle work |
|---|---|---|---|---|
| A | New authority/byte wrappers submit an existing OPEN_AT shape for read or must-exist directory lookup; an absolute path is resolved by openat's host semantics. Use the adapter for pair acquisition if a composite shape is selected. | Same descriptor-relative openat and kind-check leaf, through the typed adapter. No Linux-only facility is required. | New explicit host-namespace/dialect resolver feeds the adapter's existing-only NtCreateFile acquisition. Current relative worker needs an extended interface; DOS/UNC is not a validator change. | Inputs/opaque authority wiring; byte scratch across waits; two-credit pair and rollback. No new checker effect category. |
| B | Widening the argv path validator can reuse the direct pointer OPEN_AT route; a file-byte constructor still needs owned backing or a new direct-byte operation. Add existing-only directory lookup. | Same change to ordinary wrappers and POSIX leaf. | Still needs A's path resolver; absolute spellings cannot simply be passed as current root-relative NT names. | Revisit invocation-backed RelativePath storage and empty drop before admitting file bytes. Change legacy absolute-path contract and affected consumers. |
| C | The ordinary owned Path supplies a stable native buffer during OPEN_AT; rooted/parent writes require the new broader directory contract. | Same typed leaf; storing a path is not a new native open primitive. | Path conversion/storage plus A's host-dialect resolver; use the adapter, not an IOCP open. | Owned representation, explicit release if linear or ordinary storage drop, reuse/conversion contracts, pair acquisition. |
| D | Existing-only path lookup uses OPEN_AT with O_DIRECTORY and no mkdirat/O_CREAT; it can be ring-qualified as a single open. | Same no-create leaf through the adapter. | FILE_OPEN, never FILE_OPEN_IF; directory-only constraint or explicit NotDirectory kind outcome. | Inherits path/authority/storage from the chosen complete option. Returning both halves adds duplication/rollback; a mode alone adds neither. |
| E | A's route supplied by C's retained buffer. | A's route plus retained conversion storage. | A's resolver plus C's owned encoded Path. | A's authority and pair work plus C's reusable value lifetime. |

The runtime investigation found two corrections to the earlier study text:
POSIX ELOOP is currently InvalidPath rather than Other, and a directory request
cannot assume the existing Windows wrong-kind finish already returns
NotDirectory. Both are now stated explicitly with their acceptance obligations.

## Implementation and specification cost

These are inspected work areas; the working-day estimates below are planning
judgment, not measured productivity or a performance result. All complete
host-path options require host work; B/C broaden existing root authority. D is
an orthogonal existing-only directory requirement included in their scope.

| Area | A and E | B and C |
|---|---|---|
| Specification and library | PRE-2 optional input, ordinary handle, functions, encoding, path-vs-name/link policy, exact outcomes and factory accounting. E also adds owned Path construction/release. | PRE-2 existing path contract changes plus missing directory/byte functions; B must rename or redefine RelativePath. C specifies Path storage and delegated read/write meaning. |
| Compiler integration | Standard-library declarations, generated interface data and host bindings, ordinary lowering, Inputs layout and all construction/destructuring sites. No new ownership rule. | Similar declarations/lowering except input layout; changed existing representation and path consumers. C adds value storage lowering if opaque. |
| Linux | Native path opens relative to a retained directory or absolute namespace; byte window termination; existing-only directory route and rollback; same completion transport. | Same resolution and existing-only work, with changed dispatch of legacy opens. |
| macOS | Descriptor-relative open and directory-kind checks; bounded waiting-host helper as already used; same race/resource obligations as Linux. No dependency on Linux-only resolution APIs. | Same target-specific work; lexical validation cannot replace atomic object acquisition. |
| Windows | Lossless argument/native-byte conversion, DOS/UNC/extended qualification, captured drive context, anchored relative lookup, following selected reparse targets, no-follow traversal and pair acquisition. Highest unresolved semantic/implementation cost. | All the same Windows ambiguities; accepting more strings in relative_path does not resolve them. |
| Conformance | Add acquisition and absence outcomes, native path/link fixtures, encoding, no-authority/type/proof cases and resource accounting. Keep existing legacy verdicts. | B changes absolute-path verdict under approved amendment; C adds construction cases. Both need checks exposing broadened authority and read/write constraints. |

Linux [openat2](https://man7.org/linux/man-pages/man2/openat2.2.html) explicitly
separates beneath-root and no-symlink policies; its facilities are evidence
that these are distinct choices, not a portability prerequisite. Native
Windows [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
provides existing-only opens and controls final reparse-point handling;
[NtCreateFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntcreatefile)
documents a root-directory handle and creation dispositions. These are
implementation leads, not proof that one call implements the entire proposed
Windows resolution table. A direct Windows feasibility case must settle
retained-base behavior across renames and the interaction of reparse traversal
with DOS normalization before implementation is accepted.

Do not canonicalize, inspect a prefix, then reopen that string: a namespace
change can invalidate the inspection. Resolve the selected path and keep the
opened handle; obtain both halves from that opened object. An external rename
during broad resolution is a host outcome, not permission to reacquire an
unrelated object for the second half. For one-component opens, an atomic
no-follow acquisition and kind check protects the current operation. A
pre-open lstat alone does not. Native adapter/internal failure-injection tests
belong to runtime tests, not new language rules.

### Rough implementation effort in working days

These ranges assume one experienced implementer working full time, a settled
owner direction, the inspected compiler/runtime architecture, and CI access
to Linux, macOS and Windows. They are engineering judgment from the affected
interfaces and native routes, not recorded timings or prior productivity
measurements. They include implementation, focused CI validation and ordinary
review/fix time in each area. They exclude owner-response time, CI queue time,
unrelated gate failures, performance studies and downstream Firn changes.
They also do not price a new strict sandbox design; that would require another
study. The rows compare complete coverage of the stated needs, except D,
which is explicitly the directory-policy component alone.

| Option | Spec rules and guidance | Prelude / stdlib and compiler bindings | Linux runtime | macOS runtime | Windows runtime | Conformance and integration/resource cases | Total person working days |
|---|---:|---:|---:|---:|---:|---:|---:|
| A: authority plus direct path inputs, including D | 2–3 | 2–4 | 3–5 | 2–3 | 6–10 | 4–6 | 19–31 |
| B: widened RelativePath plus missing byte/directory operations, including D | 2–3 | 2–4 | 3–5 | 2–3 | 6–10 | 4–7 | 19–32 |
| C: retained Path plus directory-path operations, including D | 2–4 | 3–5 | 3–5 | 2–3 | 6–10 | 5–8 | 21–35 |
| D: must-exist directory-by-path and pair, with path representation/resolver already supplied | 1–2 | 1–2 | 1–2 | 1–2 | 2–4 | 2–3 | 8–15 |
| E: A plus retained reusable Path, including D | 3–5 | 4–6 | 4–6 | 3–4 | 7–12 | 5–8 | 26–41 |

D's days are already included in A/B/C/E, not added to those totals. Its row
excludes adding absolute authority, file-byte conversion and Windows path
dialects from scratch. Thus its smaller total does not mean it can deliver
the whole Firn need more cheaply. Spec and stdlib columns include choosing
the exact declarations, generated standard-library data, ordinary host-binding
metadata and Inputs construction/destructuring where applicable; no PRE-1
kernel operation or new checker rule is assumed. Runtime columns include the
path storage, opened-kind and credit/handle failure cases specific to that
platform; the final cases column covers formal expectations, fixture wiring
and cross-platform integration rather than counting those local repairs twice.

The principal spread is Windows qualification/normalization with a retained
base, directory-kind outcomes and reparse classification. B does not avoid
that work, and C/E add representation lifetime work. If the retained-base
feasibility case fails, these ranges stop being applicable: revise the design
before quoting a new implementation estimate. Parallel platform work can
shorten elapsed time; the table measures person-days, not a promised schedule.
Effort is reported for planning and does not select language semantics.

## Related requests and acceptance

Merge the **design scope**, but retain both request links and their separate
acceptance observations until fulfilled. The runtime-byte multilevel lookup
request `coord-wfbl-04-19` is covered by byte-window host opens, including a
linked intermediate component. The chosen-root symlink request
`coord-wfbl-04-11` is covered by open_host_directory on an argument, followed
by existing no-follow one-name traversal. Neither needs a separate language
mechanism. A alone without byte-window and directory opens would not cover
either fully; B alone covers neither. D alone addresses only directory
acquisition; combined with A/B/C/E it also opens the selected linked directory under that choice's
lookup rules. C/E cover both subject to the authority tradeoffs above. Do not mark either board item resolved by a research record.

The board connector is not available in this session. No board merge, request,
card publication or status change is claimed; the requested card is a local
draft. The work stops at the requested research boundary, pending an owner
direction, not at a discovered need to change the compiler secretly.

After approval, extract cases into formal ownership rather than making the
gate depend on this research record. Use the specified outcome as oracle and
native host fixtures to distinguish host path dialects. Each row below must
produce the stated observation; a skipped link fixture is not a pass on a
host that claims link support.

| Observation | Acceptance criterion and meaningful wrong result |
|---|---|
| Startup absolute config | A file outside cwd containing a known byte opens by argv and returns that byte. Returning PathInvalid or a same-named cwd file fails. |
| Runtime bytes | A config-read byte window names a multilevel relative include; an intermediate link points outside the starting directory. The opened bytes match the target. Rejecting the link or substituting argv fails. |
| Existing-only data dir | Existing target preserves a sentinel; missing target returns NotFound and independent parent enumeration shows no entry was created. A regular-file target returns NotDirectory. |
| Logical base | Use two directories with different same-named files; relative opens use the supplied base even after another data-directory selection. Absolute names ignore that base. No process-wide chdir occurs. |
| Root link versus traversal link | A user-named linked root opens; a linked file and a linked child directory encountered by one-name opens do not yield their targets. Both intermediate and final selected-path links are exercised. |
| Parent resolution | Open a parent outside base; separately use a POSIX link/../file fixture whose physical result differs from lexical collapse. Observe exact target bytes. Retain separate legacy RelativePath behavior checks. |
| Windows path classes | Different sentinel files distinguish qualified drive, UNC, relative, root-relative and drive-relative forms; extended paths exercise their distinct parsing; a DOS link/parent fixture distinguishes host textual normalization from POSIX physical-parent lookup. Reparse-root selection succeeds; subsequent one-name traversal does not follow name-surrogate links/junctions. |
| Native encoding | Non-UTF-8 POSIX name, Windows non-ASCII and unpaired-surrogate native names round-trip; odd UTF-16 byte count, NUL and malformed Utf8 return InvalidPath without opening any prefix. A nonzero window offset excludes poison bytes outside it. |
| Error behavior | Missing, denied, nondirectory, directory-as-file, unsupported kind, overlong path, dangling link and link loop fixtures return their specified class/code mapping. No silent create, retry-as-cwd or alternate encoding. |
| Authority and ownership | A byte value cannot construct HostPaths; a directory-only helper cannot call the new operations; a read-only helper cannot produce write authority. Duplicate/missing consumption of returned directory halves is rejected by ordinary rules. |
| Proof and absence | Reversed/out-of-bounds windows reject under FN-8; valid empty windows yield InvalidPath. A None authority branch performs no host-path open; a custom entry without filesystem inputs runs without filesystem calls. A no-heap entry and a filesystem-without-links target remain supported without link emulation. |
| AOF integration | In the selected data directory, append a complete known buffer with short-write handling, sync the file, rename a temporary single name to the AOF name and sync the directory. Observe exact bytes and old-handle behavior; cwd remains untouched. `.`/`..`/separator names still fail. |
| Resources and races | Runtime tests inject first/second handle failures and close failures; budget returns to baseline and no partial pair escapes. A retained base still selects its opened directory after rename; replacement during selection never produces mismatched halves. |

Conformance covers normative observations and rejection rules; program tests
cover the integrated AOF sequence; runtime tests cover native cleanup and
race machinery. Run the smallest focused CI case first, then the required
Linux/macOS/Windows gates on the proposed implementation. No such validation
was run for this research-only task, and no acceptance row is marked passed.

## Windows as built

Phase 2 of the host-path change (2026-10-13) implements the Windows rows of
the interface above; PRE-2 states the resulting rules. This note records how
the implementation meets the feasibility conditions this study left open. It
was written before any Windows run; the CI results on the pull request are
its evidence.

* **Encoding.** NativeBytes is UTF-16LE code units: an odd byte count or a
  zero code unit is InvalidPath, unpaired surrogates pass through. Utf8 is
  validated and converted to UTF-16 in the ordinary library before the open
  is submitted. A HostString argument is already UTF-16 and is passed in
  place. The limit is 32767 code units, the longest `UNICODE_STRING`.
* **Dialect.** The worker in
  [windows_runtime.c](../../../compiler/src/backend/windows_runtime.c)
  classifies the path: `\\?\` becomes `\??\` plus the rest, unnormalized; a
  device path (`\\.\`, `//?/` and the like) is ERROR_INVALID_NAME; every other
  form goes through the host's own `GetFullPathNameW` after its starting
  point, so slash conversion, `.`/`..` removal clamped at the root and
  trailing-dot trimming are the host's code, not a reimplementation. A
  normalized result the host turns into a device path (a reserved name such
  as `CON`) is ERROR_INVALID_NAME rather than an open of the device.
* **Base and renames.** The existing representation keeps a directory as a
  HANDLE that NtCreateFile accepts as `RootDirectory`, and NTFS resolves no
  `..` below such a handle. A relative path is therefore normalized after the
  spelling `GetFinalPathNameByHandleW` reports for base at the open; when the
  result is base or below it, the remainder is opened from base's handle, so
  a rename of base or its parents, before or during the open, cannot redirect
  it. Only a path that climbs above base (or a root-relative path) is opened
  by its full spelling, which a concurrent rename of base between the
  spelling query and the open could redirect. This is the Win32 meaning of
  `..` (textual, after base's current spelling); no physical-parent lookup
  exists to preserve. A base with no DOS spelling (a volume mounted without a
  drive letter or folder) makes relative opens fail with the host's error.
* **Drive context.** A drive-relative path on another drive than base's uses
  `GetFullPathNameW`'s per-drive current directory, read at the open. Whitefoot
  has no operation that changes the process working directory or the
  per-drive environment entries, so this equals the context at startup; the
  value is not copied into `HostPaths`, which stays empty.
* **Links.** Host-path opens omit FILE_OPEN_REPARSE_POINT, so NT follows
  symbolic links and junctions; a reparse attribute left on the opened object
  is a tag resolved in place (deduplication, cloud placeholders) and its kind
  is the object's own. Single-name opens keep FILE_OPEN_REPARSE_POINT and the
  refusal of every reparse kind, as the owner chose.
* **Kinds and pair.** A directory open passes FILE_DIRECTORY_FILE, so a
  non-directory is the host's ERROR_DIRECTORY (NotDirectory with its code);
  an intermediate regular file is ERROR_PATH_NOT_FOUND (NotFound), unlike
  POSIX ENOTDIR. The write half is `_dup` of the read half, one directory
  object, registered as a directory root; a refused duplicate closes the read
  half and returns both credits.
* **Scratch.** The worker's spelling and normalization buffers come from the
  process heap, as the descriptor registry's do, and are freed before the
  worker returns; the window copy is the ordinary library's runtime-pool
  block, as on POSIX.

Coverage: the conformance adapter is POSIX-only, so the `syshostpath` cases do
not run on Windows. The program test `programs::host_paths` runs on the
Windows io-hosts job: drive-absolute, relative, `..`, root-relative,
same-drive drive-relative and extended argument paths, a missing directory
not created, a file as the directory, and UTF-8 and UTF-16 byte windows with
their invalid forms. Symbolic links and junctions are not exercised on
Windows: creating a symbolic link needs a privilege or developer mode.

## Recommendation, uncertainties and rule delta

Choose A with byte-window opens and an explicit base. It makes host namespace
authority visible without changing the authority of every existing directory
write half, permits config-read paths without an owning opaque-path lifetime,
and separates user selection from recursive traversal. E becomes preferable
if a concrete retained-path consumer or conversion/resource experiment shows
why storing a decoded value is needed; a smaller signature count alone would
not justify B or C.

Confidence: 3/5. The need and the missing declarations are clear; native
Windows base/drive semantics, name-surrogate classification and bounded-target
backing have not been implemented or run. A counterexample showing that
ordinary state/effects cannot express the required behavior would reopen the
interface. A Windows result showing the explicit-base table cannot be honored
would reopen that table before any normative amendment. If the owner instead
requires a strict sandbox, this proposal is insufficient: legacy open_read,
external aliases and the new broad authority need a separate threat model,
not a claim that the existing one-component restriction already proves it.

| Rule or decision | Current form and proposed disposition |
|---|---|
| PRE-2 Inputs and host declarations | Add optional authority and the four proposed opens, encoding enum, outcome/accounting contracts. Existing declarations remain. No approval has selected these additions yet. |
| PRE-2 name restriction | Keep single nonempty component other than `.`/`..`. Explicitly distinguish new `name` byte windows used as host paths from legacy entry-name arguments; use clearer final parameter names to avoid applying the blanket name rule to the wrong family. |
| PRE-2 open_read wording | “Below root” is ambiguous beside the non-confined conformance description and native route. Clarify legacy relative lookup in the amendment; do not silently claim it enforces confinement or change its verdict from implementation convenience. |
| PRE-2 link and encoding semantics | Add explicit path-follow versus name-open no-follow rules and native-byte definitions; these are missing normative details, not already proved guarantees. State namespace-operation link-entry outcomes separately. |
| PRE-2 open_directory_write | Keep create-if-missing behavior for AOF subdirectory creation. The new pair open is existing-only; no precheck followed by this creating operation. |
| HOST-1, WAIT-1/WAIT-2, EFF-1/EFF-5, FN-8, TYPE-2, PROV-6, STOR-3/STOR-8 | Unchanged: ordinary effects, waits, range proofs, opaque authority, linear returned resources, empty opaque drop and no-heap obligations. A hidden destructor or ambient path-only open would conflict. |
| Writable-directory and host-string design grounds | Preserve read/write separation and existing invocation-backed values. Add grounds for broad lookup and byte windows only in a later approved design-tree change. |

Found along the way: the legacy path confinement wording/evidence mismatch
and absent normative filesystem symlink/native encoding table are recorded
above as part of the required amendment, not repaired in this study. The
dot-dot case's limited observation needs an opening fixture, not only a new
description. Historical I/O snippets are not current runnable examples and
were not copied as current grammar. No unrelated implementation change or
approval-log entry is made.

## Decisions required from the owner

Each line is a distinct choice within the proposed direction. Choosing A in
the local draft card selects the recommended bundle below, not an approved
specification revision, implementation or merge. A different selection must
be reflected in the corresponding contracts and acceptance rows before coding.

- **Lookup authority:** choose A's separately supplied HostPaths rather than expanding every directory's path authority as in B/C; D is included as a component and E's retained value is deferred.
- **Legacy root scope:** clarify existing RelativePath/open_read as host-relative, potentially escaping lookup, and retain single-name root locality; if strict confinement is wanted, commission a separate resolver/threat-model change rather than infer it from “below root”.
- **Parent resolution:** choose native POSIX component resolution after symlinks and Windows dialect-specific DOS normalization / extended-path behavior, rather than universal lexical `..` cancellation.
- **Link policy:** follow intermediate/final links in explicit user-path selection, keep single-name opens no-follow, and preserve current conservative Windows reparse refusal pending a concrete need to distinguish nonredirecting tags.
- **Creation policy and API shape:** include D's MustExist / NoCreate contract as a separate selected-directory path open; retain existing one-name create-if-missing operations rather than add a mode solely for symmetry.
- **Relative base:** require an explicit directory base, preserving it as an opened object; do not mutate or reread ambient process cwd, and define captured Windows drive context as part of HostPaths.
- **Path data and encoding:** accept lossless invocation HostString or caller byte windows with explicit NativeBytes/Utf8 encoding; choose POSIX raw bytes and Windows UTF-16LE native bytes, without a new retained Path value for now.
- **Weak targets and authority grant:** supply Option<HostPaths>, with None for absent/withheld filesystem authority; retain custom/no-heap entries and ordinary Unsupported outcomes for unavailable facilities, without excluding targets lacking links or a desktop host.
- **Returned directory authority and lifetime:** let the configured-root operation return two separately closable halves obtained from one selected directory, charge two factory credits and roll back both acquisitions on refusal; retain a separate read-only selected-root open for wfgrep.
- **Existing-directory errors:** specify NotFound for searchable missing paths, NotDirectory for existing non-directory targets, PermissionDenied for access refusal, and preserve native codes; add the missing Windows directory-kind outcome rather than accept generic Unsupported for an ordinary file target.
- **Work scope and validation:** merge the two backlog needs into this design, keep their distinct acceptance observations, and accept implementation only after the stated Linux/macOS/Windows and weak-target evidence; use 19–31 person-days as A's provisional planning range, not a semantic selection ground.

## Complete source witnesses

The following section contains complete callable examples for the three
boundaries, not a Redis parser or a complete server. The before forms use
current declarations; the after forms use only current grammar with the
provisional library declarations above. Their lack of a current implementation
is intentional, and they are not presented as compiler-validated programs.

### Invocation argument naming the configuration file

The current conversion consumes the argument's `HostString`. It accepts only
the relative-path form the supplied implementation recognizes; the arranged
absolute-path conformance example returns `PathInvalid`. The helper translates
argument/conversion errors into a synthesized `InvalidInput`; a production
caller may retain distinct diagnostic outcomes instead.

Before:

```whitefoot
alias Args = std::text::Args;
alias DirectoryRead = std::fs::DirectoryRead;
alias HandleFactory = std::io::HandleFactory;
alias IoError = std::io::IoError;
alias ReadFile = std::fs::ReadFile;

fn open_config_arg(args: &Args, factory: &HandleFactory, base: &DirectoryRead, position: u64) -> result: Result<ReadFile, IoError> reads(args), reads(base), writes(factory) waits {
  match std::text::arg_get(args: args, position: position) {
    Ok(value: text) => {
      match std::fs::relative_path(value: move text) {
        Ok(value: path) => {
          return std::fs::open_read(factory: factory, root: base, path: &path);
        }
        Err(error: problem) => {
          let error = IoError::InvalidInput(code: 0_u32, origin: 0_u8);
          return Err<ReadFile, IoError>(error: error);
        }
      }
    }
    Err(error: problem) => {
      let error = IoError::InvalidInput(code: 0_u32, origin: 0_u8);
      return Err<ReadFile, IoError>(error: error);
    }
  }
}
```

After, the native invocation argument reaches the authorized open directly:

```whitefoot
alias Args = std::text::Args;
alias DirectoryRead = std::fs::DirectoryRead;
alias HandleFactory = std::io::HandleFactory;
alias HostPaths = std::fs::HostPaths;
alias IoError = std::io::IoError;
alias ReadFile = std::fs::ReadFile;

fn open_config_arg(args: &Args, factory: &HandleFactory, paths: &HostPaths, base: &DirectoryRead, position: u64) -> result: Result<ReadFile, IoError> reads(args), reads(paths), reads(base), writes(factory) waits {
  match std::text::arg_get(args: args, position: position) {
    Ok(value: text) => {
      return std::fs::open_host_read(factory: factory, paths: paths, base: base, path: &text);
    }
    Err(error: problem) => {
      let error = IoError::InvalidInput(code: 0_u32, origin: 0_u8);
      return Err<ReadFile, IoError>(error: error);
    }
  }
}
```

Both helpers return the acquired linear file owner to their caller, which must
eventually consume it through `close_read`, including on parse failures. Neither
helper acquires a file and then returns an error without closing it.

### Parsed configuration bytes naming the existing data directory

The current interfaces accept one component, not an arbitrary byte path. The
following current helper creates a missing simple directory when opening its
write half, then opens its read half. An absolute or multi-component byte name
returns `InvalidPath`. This behavior does not express an existing-only host path
such as `/var/lib/redis` or `../data`.

Before:

```whitefoot
alias Directory = std::fs::Directory;
alias HandleFactory = std::io::HandleFactory;
alias IoError = std::io::IoError;

fn open_data_directory(factory: &HandleFactory, base: &Directory, name: &[u8], start: u64, end: u64) -> result: Result<Directory, IoError> reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  match std::fs::open_directory_write(factory: factory, root: &base^.write, name: name, start: start, end: end) {
    Ok(value: write) => {
      match std::fs::open_directory(factory: factory, root: &base^.read, name: name, start: start, end: end) {
        Ok(value: read) => {
          let directory = Directory(read: move read, write: move write);
          return Ok<Directory, IoError>(value: move directory);
        }
        Err(error: error) => {
          std::fs::close_directory_write(factory: factory, directory: move write);
          return Err<Directory, IoError>(error: error);
        }
      }
    }
    Err(error: error) => {
      return Err<Directory, IoError>(error: error);
    }
  }
}
```

This before helper does not guarantee that its two opens selected the same
directory under namespace changes. The ordinary public `Directory` constructor
does not establish such a relation; it merely packages independently owned
halves. A failed second open closes the already acquired write half, but a newly
created directory remains present. A close failure is deliberately secondary
to the original open failure in this illustrative helper.

After, the authorized operation opens an existing directory, acquires two
credits, and returns both halves of that same opened directory. A second-credit
failure rolls back the first acquisition; it never creates a directory:

```whitefoot
alias Directory = std::fs::Directory;
alias DirectoryRead = std::fs::DirectoryRead;
alias HandleFactory = std::io::HandleFactory;
alias HostPaths = std::fs::HostPaths;
alias IoError = std::io::IoError;
alias PathEncoding = std::fs::PathEncoding;

fn open_data_directory(factory: &HandleFactory, paths: &HostPaths, base: &DirectoryRead, name: &[u8], start: u64, end: u64, encoding: PathEncoding) -> result: Result<Directory, IoError> reads(paths), reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  return std::fs::open_host_directory_bytes(factory: factory, paths: paths, base: base, name: name, start: start, end: end, encoding: encoding);
}
```

The returned owned `Directory` is transferred to the existing AOF caller.
That caller keeps its existing create/append/sync/rename/directory-sync sequence
using the returned write half and closes both halves on its leaving paths.
This example does not introduce a replacement AOF implementation or assume
away short writes. It changes how that caller acquires its directory.

### Parsed configuration bytes naming an included configuration file

The current byte API can open a direct file component. It cannot express a
multi-component byte include such as `/etc/redis/common.conf`, `conf/extra.conf`, or a
parent path as one operation. It returns `InvalidPath` for such names rather
than converting them to `RelativePath`.

Before:

```whitefoot
alias DirectoryRead = std::fs::DirectoryRead;
alias HandleFactory = std::io::HandleFactory;
alias IoError = std::io::IoError;
alias ReadFile = std::fs::ReadFile;

fn open_include(factory: &HandleFactory, base: &DirectoryRead, name: &[u8], start: u64, end: u64) -> result: Result<ReadFile, IoError> reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  return std::fs::open_file(factory: factory, root: base, name: name, start: start, end: end);
}
```

After, the caller passes authority, an explicit relative-path base, and the
encoding of the parsed bytes. No process-wide `chdir` is required:

```whitefoot
alias DirectoryRead = std::fs::DirectoryRead;
alias HandleFactory = std::io::HandleFactory;
alias HostPaths = std::fs::HostPaths;
alias IoError = std::io::IoError;
alias PathEncoding = std::fs::PathEncoding;
alias ReadFile = std::fs::ReadFile;

fn open_include(factory: &HandleFactory, paths: &HostPaths, base: &DirectoryRead, name: &[u8], start: u64, end: u64, encoding: PathEncoding) -> result: Result<ReadFile, IoError> reads(paths), reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  return std::fs::open_host_read_bytes(factory: factory, paths: paths, base: base, name: name, start: start, end: end, encoding: encoding);
}

fn open_native_include(factory: &HandleFactory, paths: &HostPaths, base: &DirectoryRead, name: &[u8], start: u64, end: u64) -> result: Result<ReadFile, IoError> reads(paths), reads(base), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  let encoding = PathEncoding::NativeBytes();
  return open_include(factory: factory, paths: paths, base: base, name: name, start: start, end: end, encoding: encoding);
}
```

An equivalent UTF-8 caller binds `PathEncoding::Utf8()` before calling
`open_include`. The choice must describe the parser's actual byte encoding;
valid UTF-8 is not a premise of the source bounds contract. Invalid conversion
is an ordinary open outcome.

The read half of the directory returned by the second example can be used
directly as the base of an include. This separate complete helper demonstrates
the ordinary field projection and its enclosing effect path:

```whitefoot
alias Directory = std::fs::Directory;
alias HandleFactory = std::io::HandleFactory;
alias HostPaths = std::fs::HostPaths;
alias IoError = std::io::IoError;
alias PathEncoding = std::fs::PathEncoding;
alias ReadFile = std::fs::ReadFile;

fn open_include_in_directory(factory: &HandleFactory, paths: &HostPaths, directory: &Directory, name: &[u8], start: u64, end: u64) -> result: Result<ReadFile, IoError> reads(paths), reads(directory.read), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} {
  let encoding = PathEncoding::NativeBytes();
  return std::fs::open_host_read_bytes(factory: factory, paths: paths, base: &directory^.read, name: name, start: start, end: end, encoding: encoding);
}
```

### Separate static witness: bytes cannot become a current RelativePath

This is a complete function with valid grammar that should be rejected by the
current type judgment. Its byte run is not `HostString`; it does not use a
nonexistent byte-conversion function:

```whitefoot
fn rejected_byte_path() -> result: unit pure {
  let name = slots_new::<u8, 4>();
  std::fs::relative_path(value: move name);
  return unit;
}
```

The existing `sys14-no-path-from-bytes` and
`type5-neg-vector-as-relative-path` cases select a `TYPE-5` rejection for this
kind of mismatch. The illustrative empty run isolates the nominal mismatch;
the populated existing case shows that byte content does not supply the missing
nominal type.


The new form is right because the caller supplies namespace authority separately
from path bytes and the relative base; the returned directory lets AOF code
continue using its existing limited write half and single-name operations.

### Complete entry witnesses for an argv configuration path

Both blocks are standalone source-bundle programs. They were checked against
the v0.149 grammar by reading only; neither was compiled or executed. The first
uses current declarations. The second uses the proposed `HostPaths`,
`open_host_read`, and the proposed last `Inputs` field
`host_paths: Option<HostPaths>`; those declarations are not supplied today.

Both programs open invocation argument position one. On successful acquisition,
they consume the file through `close_read` before returning from the helper. The
entry wrapper then consumes both original working-directory halves on every
route, including missing argument, rejected path, failed open, failed file
close, and (in the proposal) absent host-path authority. Directory-close results
are deliberately ignored in these minimal witnesses; consumption is explicit,
but the exit code does not certify that the host reported successful closes.

The status codes are zero for a successful file open and close, one for missing
argument, two for rejected current `relative_path`, three for failed file open,
four for failed file close, and five for absent proposed authority.

#### Before: the existing relative-path conversion

An arranged absolute argv name such as `/etc/redis/redis.conf` reaches the
current conversion and returns status two. A relative argv path reaches the
ordinary open under the supplied directory read half.

```whitefoot
alias Args = std::text::Args;
alias DirectoryRead = std::fs::DirectoryRead;
alias ExitStatus = std::process::ExitStatus;
alias HandleFactory = std::io::HandleFactory;
alias Inputs = std::process::Inputs;

fn open_config_arg(args: &Args, factory: &HandleFactory, base: &DirectoryRead) -> code: u8 reads(args), reads(base), writes(factory) waits {
  match std::text::arg_get(args: args, position: 1_u64) {
    Ok(value: text) => {
      match std::fs::relative_path(value: move text) {
        Ok(value: path) => {
          match std::fs::open_read(factory: factory, root: base, path: &path) {
            Ok(value: file) => {
              match std::fs::close_read(factory: factory, file: move file) {
                Ok(value: closed) => {
                  return 0_u8;
                }
                Err(error: problem) => {
                  return 4_u8;
                }
              }
            }
            Err(error: problem) => {
              return 3_u8;
            }
          }
        }
        Err(error: problem) => {
          return 2_u8;
        }
      }
    }
    Err(error: problem) => {
      return 1_u8;
    }
  }
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  let Inputs(args: arguments, drivers: unused_driver_count, cwd: directory, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter) = move inputs;
  let std::fs::Directory(read: root, write: root_write) = move directory;
  let code = open_config_arg(args: &arguments, factory: &factory, base: &root);
  std::fs::close_directory_write(factory: &factory, directory: move root_write);
  std::fs::close_directory(factory: &factory, directory: move root);
  return std::process::exit_status(code: code);
}
```

#### After: optional explicit host-path authority

With `Some` authority, the argument reaches the proposed open as its native
`HostString`; an absolute path ignores the supplied relative-path base. With
`None`, the entry returns status five after closing the original directory
halves. The authority's invocation-backed opaque value has empty drop and no
host descriptor or finalizer to close.

```whitefoot
alias Args = std::text::Args;
alias DirectoryRead = std::fs::DirectoryRead;
alias ExitStatus = std::process::ExitStatus;
alias HandleFactory = std::io::HandleFactory;
alias HostPaths = std::fs::HostPaths;
alias Inputs = std::process::Inputs;

fn open_config_arg(args: &Args, factory: &HandleFactory, paths: &HostPaths, base: &DirectoryRead) -> code: u8 reads(args), reads(paths), reads(base), writes(factory) waits {
  match std::text::arg_get(args: args, position: 1_u64) {
    Ok(value: text) => {
      match std::fs::open_host_read(factory: factory, paths: paths, base: base, path: &text) {
        Ok(value: file) => {
          match std::fs::close_read(factory: factory, file: move file) {
            Ok(value: closed) => {
              return 0_u8;
            }
            Err(error: problem) => {
              return 4_u8;
            }
          }
        }
        Err(error: problem) => {
          return 3_u8;
        }
      }
    }
    Err(error: problem) => {
      return 1_u8;
    }
  }
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  let Inputs(args: arguments, drivers: unused_driver_count, cwd: directory, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: host_paths) = move inputs;
  let std::fs::Directory(read: root, write: root_write) = move directory;
  let code = match move host_paths {
    Some(value: paths) => {
      give open_config_arg(args: &arguments, factory: &factory, paths: &paths, base: &root);
    }
    None() => {
      give 5_u8;
    }
  }
  std::fs::close_directory_write(factory: &factory, directory: move root_write);
  std::fs::close_directory(factory: &factory, directory: move root);
  return std::process::exit_status(code: code);
}
```

#### Reading checks

The full `Inputs` destructuring preserves the current declaration order,
including `drivers` after `args`, and appends only the proposed `host_paths`
field. Each function call argument is an atom. The `give` operand may itself be
a call under GRAM-4/GRAM-5; it is not nested inside another call's argument.
Matches use the scrutinee enum's labels, so their `Ok`, `Err`, `Some`, and `None`
labels do not need constructor generic arguments. No result constructor is
written in these programs. `HostPaths` and `Option<HostPaths>` are affine under
the proposal, so the latter is matched with explicit `move`.

Both entries are `pure waits`: their input is received by value and subsequent
state is local, so the enclosing formal-rooted row is empty under EFF-2. The
ordinary host footprints remain; `pure` does not state that these programs do
no I/O. The helpers exhibit the reference-rooted rows written on their
signatures. All acquired linear owners are consumed before their scopes leave.
