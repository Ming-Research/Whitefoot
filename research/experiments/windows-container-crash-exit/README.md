# Ending a process that holds a gibibyte of stack, in a Windows container

The Windows package check builds a program that exhausts the runtime floor's
stack and runs it. In a process-isolated Windows Server Core container that run
took about twenty minutes, on one occasion from the start of the image to its
end; on a hosted Windows runner outside the container the same kind of run takes
about a second. This bundle records the comparison that located the delay.

## Question

Does ending a process by a crash (a fast fail or an unhandled exception) while
it holds the runtime floor's 1 GiB of committed stack take minutes in the
container only, or also on the runner itself, and does the delay depend on the
way the process ends?

## Comparison and rejection criterion

One program, [`exit.c`](exit.c), takes a mode on its command line. It runs its
work on a thread with a reserved stack of 1 GiB (the size of
`WF_FLOOR_STACK_BYTES` in `compiler/src/backend/wf_floor_windows.c`) or 16 MiB
(`small`), recurses with 56 KiB frames until the stack overflows, and ends by
the mode's method from a vectored exception handler that mirrors the floor's:
`abort`, `exit` (`ExitProcess`), `terminate` (`TerminateProcess`) or `fastfail`
(`__fastfail`); `nohandler` installs no handler. `abortonly` and `plainfast`
end at once without any overflow, and `raise` ends from a raised exception.

Each mode ran in the container and on the runner, with a limit of 90 s in the
container and the step limit of six minutes on the runner. The claim that the
delay belongs to the container is rejected if a crash mode at 1 GiB takes
longer than a few seconds on the runner, and the claim that it depends on the
way the process ends is rejected if `exit` or `terminate` is slow at 1 GiB in
the container.

## Result

Measured on 2026-10-21 on hosted `windows-2022` runners, container image
`mcr.microsoft.com/windows/servercore:ltsc2022` with process isolation (Windows
Server 2022, OS build 20348.5622), program compiled by the pinned llvm-mingw
clang for `x86_64-w64-windows-gnu`. A cell with `>` ran into the 90 s limit
and was stopped, so the real time is unknown.

| mode | stack | container | runner |
|---|---|---|---|
| `abortonly` | none | 2 s | |
| `plainfast` | none | 1 s | |
| `raise-abort` | none | 1 s | |
| `overflow-small-abort` | 16 MiB | 2 s | 1 s |
| `overflow-small-exit` | 16 MiB | 0 s | |
| `overflow-small-fastfail` | 16 MiB | 2 s | |
| `overflow-abort` | 1 GiB | > 90 s | 1 s |
| `overflow-fastfail` | 1 GiB | > 90 s | 1 s |
| `overflow-nohandler` | 1 GiB | > 90 s | |
| `overflow-exit` | 1 GiB | 0 s | 0 s |
| `overflow-terminate` | 1 GiB | 0 s | |

Switching Windows Error Reporting off in the container (`Disabled` and
`DontShowUI` set to 1 under `HKLM\SOFTWARE\Microsoft\Windows\Windows Error
Reporting`) left `overflow-abort` at 1 GiB over 90 s. The Windows Error
Reporting service is stopped in the container image. The package check's own
stack witness took 20 min 38 s in one run and 26 minutes for the whole job in
another.

The delay appears only when a process holding the 1 GiB of committed stack
ends by a crash inside the process-isolated container, and the same crash ends
within a second on the runner. Ending by `ExitProcess` or `TerminateProcess` is
immediate in both places.

## Limits

One image build and one runner pool were tested. What the container spends the
time on is not identified: disabling Windows Error Reporting did not change it,
and no crash dump or other cause was inspected. The program imitates the floor's
handler and is not the floor. The 16 MiB and 1 GiB sizes show that the delay
grows with the committed stack but do not fit its growth. The runner times are
one run each.

## Use

The Windows package check ([design](../../../design/compiler/downstream-releases.md))
builds the stack witness in the container and runs the image on the runner,
because of this result. To repeat the measurement, compile `exit.c` with the
pinned llvm-mingw clang (`--target=x86_64-w64-windows-gnu -O1`), start a
container with `docker run --detach --isolation process` on a `windows-2022`
runner with the directory bind-mounted, and time `docker exec` of each mode;
run the same modes directly on the runner. The temporary workflow that ran them,
`tmp-windows-pkg.yml` on the branch of the Windows package work, was removed
after the measurement.
