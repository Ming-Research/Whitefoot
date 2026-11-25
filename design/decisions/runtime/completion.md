# Completion

How the runtime carries a host operation to its completion: the record, the rings, the helper threads, the socket options and the restricted fork child of a snapshot.

### decision completion-record-lives-in-the-call
A completion record lives in the native activation of the linked call that submits it and is found by address, with no global slot pool, token or capacity wait.
- replaced: A fixed global pool of completion slots, with generation-checked tokens and a refusal the submitter looped on, was on main; the owner changed it, because the call already owns the request until it returns, and the pool only added refusals nothing could act on.

### decision ring-failure-after-acceptance-stops-the-process
A root ring that is absent or refused by the probe before the entry falls back to the blocking-helper adapter; a target failure after an operation is accepted stops the process.
- rejected: an agent proposed sending an operation to the adapter, or answering an `IoError`, when the target fails after accepting it; the owner rejected it, because the operation then belongs to the target, so a fallback would run it twice and ignoring the error would strand it.
- rejected: an agent proposed starting fewer drivers when a later ring cannot start; the owner rejected it, because the probe fixes the driver count before the entry receives it, so the program already holds that count.

### decision completion-record-stays-160-bytes
The native completion record is 160 bytes, and a request that needs more room stays on the helper adapter.
- rejected: an agent proposed a larger shared record, or a buffer owned by the accept beside it; the owner rejected it, because growing the common record for one operation would tax every pending request.

### decision no-helpers-beside-a-ready-ring
With a ready native ring and no helper count set, the helper pool stays empty until a second context starts or an operation with a deadline or a real cancellation watch reaches the adapter.
- rejected: an agent proposed starting or growing a helper pool beside a ready ring; the owner rejected it, because on the warm Linux many-file comparison the helper handoff cost more than the adapter work it moved (data: `research/investigations/io-model/RESULTS.md`).

### decision helpers-grow-to-eight-without-a-ring
Without a ready native ring and with no helper count set, helpers start at zero and grow on measured waits and queued demand, or for a peer-bound request, up to eight.
- replaced: One helper started for a program with no helper count was on main; the owner changed it, because one helper was the worst measured setting for a program with width, and starting empty avoids handoff for work that does not wait (data: `research/investigations/io-model/RESULTS.md`).
- rejected: an agent proposed a ceiling of the CPU count; the owner rejected it, because the three-CPU macOS comparisons showed useful I/O width beyond the CPU count (data: `research/investigations/io-model/RESULTS.md`).

### decision open-kind-read-by-fstat
The kind of a descriptor an open produced is read with one `fstat` on the reaping thread.
- rejected: an agent proposed a linked `IORING_OP_STATX` after the open; the owner rejected it, because two ring round trips cost more than the open they wrap, 152 ms against 116 ms on the eight-wide many-file program (data: `research/investigations/io-model/RESULTS.md`).

### decision every-tcp-socket-sets-nodelay
Every TCP socket the runtime creates or accepts sets `TCP_NODELAY`.
- replaced: The host's default packet policy was on main; the owner changed it, because under it io_uring ran about 1,562 requests a second with a 41 ms p99 at 64 peers and 64 KiB, and the option gave 11.8 to 15 times that (data: `research/investigations/io-model/SCHEDULER-FINDINGS.md`).

### decision posix-listeners-reuse-the-address
On a POSIX host every listening socket sets `SO_REUSEADDR` before its bind; on Windows a listening socket sets no address option.
- replaced: No address option on any host was on main; the owner changed it, because a POSIX bind refused a restarted server its port while the stopped server's connections waited out TIME_WAIT, a minute on Linux (data: `research/investigations/io-model/NETWORK.md`).
- rejected: an agent proposed `SO_REUSEADDR` on Windows too; the owner rejected it, because Winsock's option lets a socket bind a port another socket holds, and Windows needs no option for a restart.
- rejected: an agent proposed `SO_REUSEADDR` only when the program or its environment asks; the owner rejected it, because every server restarted on its port needs it, and a setting no source names would decide a bind's answer.

### decision rings-run-without-cooperative-task-work
Each io_uring ring is created without `IORING_SETUP_COOP_TASKRUN`.
- replaced: `IORING_SETUP_COOP_TASKRUN` on each ring was on main; the owner changed it, because the flag waits for the submitting thread to enter the kernel, and a driver's role can pass to a spare while that thread computes for seconds (data: `research/investigations/driver-handoff/DESIGN.md`).

### decision rings-stay-out-of-fork-children
Every io_uring mapping is advised `MADV_DONTFORK` when its ring is created, and a refused advice refuses that ring.
- replaced: Leaving rings for a forked child to inherit was on main; the owner changed it, because a child holding the parent's live mappings could read or advance a queue the parent's kernel ring still owns.

### decision snapshot-child-runs-a-restricted-capsule
On Linux x86-64 with glibc a snapshot child forked inside a hold runs only a restricted capsule of raw system calls and one fixed native encoder, and leaves by `exit_group`.
- rejected: an agent proposed running ordinary runtime or Whitefoot code, stdio, atexit handlers or the allocator in the child; the owner rejected it, because the child inherits locks and allocator state held by threads that are absent from it, so a lock or an allocation there can hang or corrupt.

### decision source-export-uses-a-runtime-encoder
A source snapshot export runs in the restricted capsule with a generated type description and a runtime-owned encoder that calls no user callback.
- rejected: an agent proposed giving Whitefoot source the capsule's native encoder pointer or its raw descriptors; the owner rejected it, because the capsule gives a child that runs only fixed code, but neither a typed walk of the value nor source ownership of the output.

### decision snapshot-export-prepares-before-the-hold
A snapshot export reserves its descriptors, job control and child workspace before the hold, makes one creation attempt in the held call, and activates the parent after the hold.
- rejected: an agent proposed building the output or starting an ordinary runtime task while the dataset is held; the owner rejected it, because allocation, descriptor setup and driver access would widen what runs inside the hold, and inherited native state is no service to the child.

### decision snapshot-export-reaps-by-waitpid
A source export job reaps its child with `waitpid` on the child it alone owns, and opens no pidfd after the capture.
- rejected: an agent proposed a pidfd to watch the child, opened after the hold; the owner rejected it, because the child's identity cannot be reused before its only owner reaps it, and reading the data pipe already shows the output's progress.

### decision snapshot-export-only-on-linux-glibc
The source snapshot exporter runs on Linux x86-64 with glibc; any other target reports an explicit target capability failure.
- rejected: an agent proposed refusing such source, or silently using another snapshot algorithm, on other targets; the owner rejected it, because the capsule's audit qualifies only that platform and proves nothing elsewhere, while the source itself is valid.

### decision snapshot-signal-exclusion-at-atomic-entry
A composition using the snapshot exporter prepares signal exclusion when an atomic statement begins, and starts the queued prepared jobs after it releases every target.
- rejected: an agent proposed finding the direct capture calls, or setting up signals and descriptors inside the hold; the owner rejected it, because a forwarding wrapper may find its reserve only inside the held block, and a resumed acquisition may run on another executor.

### decision snapshot-finish-is-a-helper-request
Finishing a snapshot export is a closed helper-adapter request that carries the owned job and an abort flag.
- rejected: an agent proposed blocking the source executor, or a user callback or a generic runtime thunk in the completion record; the owner rejected it, because draining a full pipe and confirming a reap can wait, while service contexts must keep running.
